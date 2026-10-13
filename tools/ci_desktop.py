#!/usr/bin/env python3
"""Exercise real desktop windows, both UI languages and the shared MCP session."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import tempfile
import threading


def verify_style_consumers() -> None:
    panels = Path(__file__).resolve().parents[1] / "desktop/app/src/panels"
    for source in panels.glob("*.rs"):
        if re.search(r"\bpalette::|\bdefault_stylesheet\s*\(", source.read_text(encoding="utf-8")):
            raise RuntimeError(f"{source.name} bypasses its window's style context")


def verify_design_screens(snapshot: dict, locale: str) -> None:
    screens = snapshot.get("designScreens", {})
    for name in ("home", "composition-settings", "workspace-manager", "effect-library", "timeline-parent-menu", "timeline-blend-menu", "timeline-graph", "value-graph", "speed-graph"):
        if name not in screens:
            raise RuntimeError(f"{locale} did not capture {name}")
        data = Path(screens[name]).read_bytes()
        if len(data) < 1024 or not data.startswith(b"\x89PNG\r\n\x1a\n"):
            raise RuntimeError(f"{locale} {name} is not a rendered PNG")


def verify_style_overrides(binary: Path, output: Path) -> None:
    override = output / "acceptance.override.json"
    override.write_text(json.dumps({
        "tokens": {
            "colors": {"panel": "#212428", "focus": "#60C5A0", "header": "#161819"},
            "sizes": {"row.height": 32, "motion.drag_threshold": 12},
            "motion": {"easing": "linear"},
        },
        "styles": {"button": {"radius": 6}},
    }), encoding="utf-8")
    for locale in ("en", "zh"):
        report = output / f"styled-panels-{locale}.json"
        result = subprocess.run(
            [str(binary), "--panels-smoke", str(report), "--locale", locale, "--style", str(override)],
            capture_output=True, text=True, encoding="utf-8", timeout=90,
        )
        (output / f"styled-panels-{locale}.stderr.log").write_text(result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(f"{locale} styled window acceptance failed: {result.stderr}")
        snapshot = json.loads(report.read_text(encoding="utf-8"))
        style = snapshot["style"]
        if not style["overrideActive"] or style["rowHeight"] != 32 or style["dragThreshold"] != 12 or style["easing"] != "linear":
            raise RuntimeError(f"{locale} ignored style/density/motion overrides")
        if len(snapshot["panels"]) < 21:
            raise RuntimeError(f"{locale} did not render every floating panel with overrides")
        baseline = json.loads((output / f"smoke-{locale}.json").read_text(encoding="utf-8"))
        if Path(snapshot["screenshot"]).read_bytes() == Path(baseline["screenshot"]).read_bytes():
            raise RuntimeError(f"{locale} style override did not change rendered pixels")


def verify_mcp(binary: Path, output: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="motion-mcp-") as folder:
        root = Path(folder) / "default"
        report = output / "mcp-ui-smoke.json"
        with (output / "mcp-ui.stderr.log").open("w", encoding="utf-8") as errors:
            options = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}
            process = subprocess.Popen(
                [str(binary), "--mcp-ui", "--smoke", str(report), "--project", str(root), "--locale", "en"],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors,
                text=True, encoding="utf-8", **options,
            )
            received: queue.Queue[str | None] = queue.Queue()

            def read() -> None:
                assert process.stdout
                for line in process.stdout:
                    received.put(line)
                received.put(None)

            threading.Thread(target=read, daemon=True).start()
            sequence = 0

            def send(method: str, params: dict | None = None, notification: bool = False) -> dict:
                nonlocal sequence
                sequence += 1
                message: dict = {"jsonrpc": "2.0", "method": method}
                if not notification:
                    message["id"] = sequence
                if params is not None:
                    message["params"] = params
                assert process.stdin
                process.stdin.write(json.dumps(message, ensure_ascii=False) + "\n")
                process.stdin.flush()
                if notification:
                    return {}
                line = received.get(timeout=20)
                if line is None:
                    raise RuntimeError("desktop stopped before its MCP reply")
                response = json.loads(line)
                if response.get("id") != sequence or "error" in response:
                    raise RuntimeError(f"invalid MCP response: {response}")
                return response["result"]

            try:
                send("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "motion-ci", "version": "1"}})
                send("notifications/initialized", notification=True)
                tools = send("tools/list")["tools"]
                if {tool["name"] for tool in tools} != {"motion_state", "motion_edit", "motion_seek", "motion_history", "motion_save", "command_list", "command_run", "command_batch", "render_preview"}:
                    raise RuntimeError("MCP catalogue is incomplete")
                state = send("tools/call", {"name": "motion_state", "arguments": {}})["structuredContent"]
                result = send("tools/call", {"name": "motion_edit", "arguments": {
                    "expectedRevision": state["revision"],
                    "commands": [{"op": "add_shape", "id": 2, "name": "MCP 矩形", "shape": "rectangle", "size": [120, 120], "position": [960, 540, 0]}],
                }})
                if result.get("isError"):
                    raise RuntimeError(f"shared edit failed: {result}")
                result = send("tools/call", {"name": "motion_seek", "arguments": {"frame": 21.5}})
                if result.get("isError") or result["structuredContent"]["frame"] != 21.5:
                    raise RuntimeError("shared fractional seek failed")
                result = send("tools/call", {"name": "motion_save", "arguments": {}})
                if result.get("isError"):
                    raise RuntimeError("shared save failed")
                assert process.stdin
                process.stdin.close()
                if process.wait(timeout=40) != 0:
                    raise RuntimeError("shared-window acceptance failed")
                saved = json.loads((root / "project.json").read_text(encoding="utf-8"))
                if not any(layer["id"] == 2 and layer["name"] == "MCP 矩形" for layer in saved["layers"]):
                    raise RuntimeError("MCP changes were not saved by the shared project")
                snapshot = json.loads(report.read_text(encoding="utf-8"))
                if snapshot["frame"] != 21.5 or not snapshot["desktop"]["sharedGpuDevice"]:
                    raise RuntimeError("GUI and MCP did not keep the same project clock/device")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=10)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("artifacts/desktop"))
    args = parser.parse_args()
    verify_style_consumers()
    binary = args.binary.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for locale in ("en", "zh"):
        report = output / f"smoke-{locale}.json"
        result = subprocess.run([str(binary), "--smoke", str(report), "--locale", locale], capture_output=True, text=True, encoding="utf-8", timeout=40)
        (output / f"smoke-{locale}.stderr.log").write_text(result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(f"{locale} desktop acceptance failed: {result.stderr}")
        snapshot = json.loads(report.read_text(encoding="utf-8"))
        verify_design_screens(snapshot, locale)
        if snapshot["locale"] != locale or snapshot["floatingFrames"].get("3", 0) < 2:
            raise RuntimeError(f"{locale} did not validate the floating composition")
        node_report = output / f"node-smoke-{locale}.json"
        node_result = subprocess.run(
            [str(binary), "--node-smoke", str(node_report), "--locale", locale],
            capture_output=True, text=True, encoding="utf-8", timeout=80,
        )
        (output / f"node-smoke-{locale}.stderr.log").write_text(node_result.stderr, encoding="utf-8")
        if node_result.returncode:
            raise RuntimeError(f"{locale} node acceptance failed: {node_result.stderr}")
        node_snapshot = json.loads(node_report.read_text(encoding="utf-8"))
        preview = node_snapshot.get("nodePreview", {})
        for check in ("clockPixelsChanged", "parameterPixelsChanged", "groupPixelsPreserved", "ungroupPixelsPreserved", "groupSourceReopened", "invalidGraphKeptValidPixels", "projectUntouched", "exampleOpenedAndRendered"):
            if preview.get(check) is not True:
                raise RuntimeError(f"{locale} node preview did not verify {check}")
        if node_snapshot["locale"] != locale or node_snapshot["floatingFrames"].get("10", 0) < 3:
            raise RuntimeError(f"{locale} did not validate the floating node workspace")
    verify_mcp(binary, output)
    verify_style_overrides(binary, output)
    print("Desktop languages, styled floating windows, live node rendering and shared MCP passed.")


if __name__ == "__main__":
    main()
