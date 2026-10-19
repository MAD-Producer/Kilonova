#!/usr/bin/env python3
"""Exercise real desktop windows, both UI languages and the shared MCP session."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import queue
import re
import struct
import subprocess
import tempfile
import threading


def verify_style_consumers(panel_root: Path | None = None) -> None:
    panels = panel_root if panel_root is not None else Path(__file__).resolve().parents[1] / "desktop/app/src/panels"
    for source in panels.rglob("*.rs"):
        if re.search(r"\bpalette::|\bdefault_stylesheet\s*\(", source.read_text(encoding="utf-8")):
            raise RuntimeError(f"{source.name} bypasses its window's style context")


def verify_design_screens(snapshot: dict, locale: str, *, property_followups: bool = False) -> None:
    screens = snapshot.get("designScreens", {})
    auxiliary = {
        f"{panel}{suffix}": size
        for panel in ("info-panel", "preview-panel", "character-panel")
        for suffix, size in (("", (360, 520)), ("-narrow", (220, 360)))
    }
    auxiliary.update({
        f"expression-inline-{mode}{suffix}": size
        for mode in ("enabled", "disabled", "locked")
        for suffix, size in (("", (900, 520)), ("-narrow", (600, 360)))
    })
    auxiliary.update({
        "effect-controls": (360, 560), "effect-controls-narrow": (220, 560),
        "effect-controls-bottom": (360, 360), "effect-curve-inline": (360, 560),
        "layer-properties-tree": (360, 560), "layer-properties-effects": (360, 560),
        "timeline-properties-tree": (720, 520),
        "camera-properties": (360, 560), "timeline-cameras": (720, 520),
        "path-hover-incoming": (900, 520), "path-hover-outgoing": (900, 520),
        "path-hover-locked": (900, 520), "timeline-translated": (900, 520),
        "timeline-navigation": (900, 520), "timeline-navigation-end": (900, 520),
        "timeline-navigation-graph": (900, 520), "timeline-navigation-hover": (900, 520),
        "timeline-navigation-drag": (900, 520),
        "layer-anchor-animation": (360, 560), "timeline-anchor-animation": (720, 520),
        "timeline-anchor-graph": (720, 520),
    })
    if property_followups:
        auxiliary = {
            "property-key-navigation": (360, 560),
            "property-key-navigation-main": (360, 560),
            "property-key-navigation-narrow": (220, 560),
            "timeline-key-navigation": (900, 520),
            "timeline-key-navigation-graph": (900, 520),
            "selection-property-main": (360, 560),
            "selection-property-float": (360, 560),
            "selection-property-narrow": (220, 560),
            "selection-property-number": (360, 560),
            "selection-anchor-main": (360, 560),
            "selection-timeline-main": (900, 520),
            "selection-timeline-float": (900, 520),
            "selection-timeline-graph": (900, 520),
            "typed-selection-mask-property": (360, 560),
            "typed-selection-mask-timeline": (900, 520),
            "typed-selection-shape-property": (360, 560),
            "typed-selection-shape-timeline": (900, 520),
            "typed-selection-audio-timeline": (900, 520),
            "timeline-3d-observation-off": (900, 520),
            "timeline-3d-observation-on": (900, 520),
            "property-merge-menu": (360, 560), "property-merge-menu-narrow": (220, 560),
            "property-axis-expression": (360, 560), "layer-lock-menu": (900, 520),
            "property-mask-group": (360, 560), "timeline-mask-group": (900, 520),
            "timeline-mask-group-narrow": (720, 520),
            "timeline-layer-columns": (720, 520), "timeline-layer-columns-minimum": (360, 520),
            "timeline-mask-add-menu": (900, 520), "timeline-mask-add-menu-narrow": (720, 520),
            "layer-unlock-menu": (900, 520), "layer-lock-main-menu": (900, 520),
            "layer-matte-menu": (900, 520), "layer-matte-menu-narrow": (720, 520),
            "timeline-columns-main-menu": (900, 520), "diagnostics-main-menu": (900, 520),
            "mask-mode-menu": (360, 560), "mask-mode-menu-narrow": (220, 560),
            "graph-raw-readonly": (900, 520),
            "effect-expression-unavailable-menu": (360, 560),
            "mask-handles-selected": (900, 520),
            "rectangle-resize-before": (900, 520), "rectangle-resize-after": (900, 520),
            "rectangle-resize-properties": (360, 560),
            **{f"timeline-key-drag-{row}-{phase}": (900, 520)
               for row in ("property", "summary") for phase in ("before", "after")},
            **{f"effect-timeline-{surface}-{mode}{menu}": (width, 760)
               for surface, width in (("main", 1200), ("floating", 900))
               for mode in ("parameters", "graph") for menu in ("", "-menu")},
            **{f"effect-selection-from-{owner}-{host}": (width, 560)
               for owner in ("controls", "inspector")
               for host, width in (("controls", 360), ("inspector", 360), ("narrow", 220))},
        }
    names = tuple(auxiliary) if property_followups else ("home", "project-open-error", "composition-settings", "workspace-manager", "effect-library", "timeline-parent-menu", "timeline-blend-menu", "timeline-graph", "value-graph", "speed-graph", "viewer-fit", "viewer-zoom", "viewer-pan", *auxiliary)
    for name in names:
        if name not in screens:
            raise RuntimeError(f"{locale} did not capture {name}")
        data = Path(screens[name]).read_bytes()
        if len(data) < 1024 or not data.startswith(b"\x89PNG\r\n\x1a\n"):
            raise RuntimeError(f"{locale} {name} is not a rendered PNG")
        if name in auxiliary:
            width, height = struct.unpack(">II", data[16:24])
            expected_width, expected_height = auxiliary[name]
            if not height or abs(width / height - expected_width / expected_height) > 0.01:
                raise RuntimeError(f"{locale} {name} has stretched panel dimensions: {width}x{height}")
    if snapshot.get("designCaptureProjectUntouched") is not True:
        raise RuntimeError(f"{locale} design screenshots did not restore the project")


def verify_property_followups(binary: Path, output: Path, *, style: Path | None = None) -> None:
    for locale in ("en", "zh"):
        prefix = "property-styled" if style else "property-panels"
        report = output / f"{prefix}-{locale}.json"
        result = subprocess.run(
            [str(binary), "--property-smoke", str(report), "--locale", locale, *(["--style", str(style)] if style else [])],
            capture_output=True, text=True, encoding="utf-8", timeout=90,
        )
        (output / f"{prefix}-{locale}.stderr.log").write_text(result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(f"{locale} property panel acceptance failed: {result.stderr}")
        for codepoint in ("U+2194", "U+2713"):
            if any("site=text.missing_glyph" in line and codepoint in line for line in result.stderr.splitlines()):
                raise RuntimeError(f"{locale} property captures regressed UI symbol rendering: {result.stderr}")
        snapshot = json.loads(report.read_text(encoding="utf-8"))
        verify_design_screens(snapshot, locale, property_followups=True)
        if style:
            actual = snapshot["style"]
            if not actual["overrideActive"] or actual["rowHeight"] != 32 or actual["dragThreshold"] != 12 or actual["easing"] != "linear":
                raise RuntimeError(f"{locale} property captures ignored style overrides")
            baseline = json.loads((output / f"property-panels-{locale}.json").read_text(encoding="utf-8"))
            for name in ("property-mask-group", "timeline-mask-group"):
                if Path(snapshot["designScreens"][name]).read_bytes() == Path(baseline["designScreens"][name]).read_bytes():
                    raise RuntimeError(f"{locale} {name} style override did not change rendered pixels")


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
        verify_design_screens(snapshot, locale)
        style = snapshot["style"]
        if not style["overrideActive"] or style["rowHeight"] != 32 or style["dragThreshold"] != 12 or style["easing"] != "linear":
            raise RuntimeError(f"{locale} ignored style/density/motion overrides")
        # Masks and expressions are property-owned; legacy IDs 6 and 9 stay readable.
        if {panel["id"] for panel in snapshot["panels"]} != set(range(1, 22)) - {6, 9}:
            raise RuntimeError(f"{locale} did not render every floating panel with overrides")
        viewer = next(panel for panel in snapshot["panels"] if panel["id"] == 3)
        navigation = viewer.get("viewerNavigationScreenshots", {})
        if set(navigation) != {"zoom", "pan"} or not all(Path(p).is_file() for p in navigation.values()):
            raise RuntimeError(f"{locale} did not render floating viewer zoom and pan")
        properties = next(panel for panel in snapshot["panels"] if panel["id"] == 11)
        for panel_id in (4, 11):
            record = next(panel for panel in snapshot["panels"] if panel["id"] == panel_id)
            anchor = record.get("anchorAnimationScreenshots", {})
            if not Path(anchor.get("keys", "")).is_file() or (panel_id == 4 and not Path(anchor.get("graph", "")).is_file()):
                raise RuntimeError(f"{locale} did not capture real floating anchor animation")
        mask_timeline = properties.get("maskTimelineScreenshot")
        if not mask_timeline or not Path(mask_timeline).is_file():
            raise RuntimeError(f"{locale} did not render masks in layer properties and the shared timeline")
        shapes = next(panel for panel in snapshot["panels"] if panel["id"] == 7)
        if not Path(shapes.get("shapeTimelineScreenshot", "")).is_file():
            raise RuntimeError(f"{locale} did not render shape content keys in the shared timeline")
        operators = shapes.get("shapeOperatorScreenshots", {})
        if set(operators) != {"fill", "stroke", "trim"} or not all(Path(p).is_file() for p in operators.values()):
            raise RuntimeError(f"{locale} did not render editable fill, stroke and trim properties")
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
                if not (process.stdout): raise RuntimeError('Desktop MCP stdout pipe is unavailable')
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
                if not (process.stdin): raise RuntimeError('Desktop MCP stdin pipe is unavailable')
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
                if not (process.stdin): raise RuntimeError('Desktop MCP stdin pipe is unavailable')
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
        workspace_probe = snapshot["workspacePresentationProbe"]
        maximize = snapshot["panelMaximizeProbe"]
        if maximize["restored"] is not True or maximize["layoutAndProjectUntouched"] is not True:
            raise RuntimeError(f"{locale} temporary panel maximize changed persistent state")
        if len(maximize["screenshots"]) != 4:
            raise RuntimeError(f"{locale} missing panel maximize and restore captures")
        for screenshot in maximize["screenshots"]:
            data = Path(screenshot).read_bytes()
            if len(data) < 1024 or not data.startswith(b"\x89PNG\r\n\x1a\n"):
                raise RuntimeError(f"{locale} invalid panel maximize capture: {screenshot}")
        floating_maximize = report.with_suffix(".panel-3-backquote.png")
        data = floating_maximize.read_bytes()
        if len(data) < 1024 or not data.startswith(b"\x89PNG\r\n\x1a\n"):
            raise RuntimeError(f"{locale} missing native floating Backquote evidence")
        presentations = workspace_probe["presentations"]
        if (workspace_probe["projectUntouched"] is not True or len(presentations) != 6
                or {record["preset"] for record in presentations}
                != {"default", "nodes", "drawing", "all", "animation"}
                or not any(record["nodeOverlay"] for record in presentations)):
            raise RuntimeError(f"{locale} did not present workspace changes with a live node overlay")
        if snapshot["locale"] != locale or snapshot["floatingFrames"].get("3", 0) < 2:
            raise RuntimeError(f"{locale} did not validate the floating composition")
        layer_menu = Path(snapshot["layerContextMenuScreenshot"])
        data = layer_menu.read_bytes()
        if len(data) < 1024 or not data.startswith(b"\x89PNG\r\n\x1a\n"):
            raise RuntimeError(f"{locale} did not render the timeline layer context menu")
        node_report = output / f"node-smoke-{locale}.json"
        node_result = subprocess.run(
            [str(binary), "--node-smoke", str(node_report), "--locale", locale],
            capture_output=True, text=True, encoding="utf-8", timeout=125,
        )
        (output / f"node-smoke-{locale}.stderr.log").write_text(node_result.stderr, encoding="utf-8")
        if node_result.returncode:
            raise RuntimeError(f"{locale} node acceptance failed: {node_result.stderr}")
        node_snapshot = json.loads(node_report.read_text(encoding="utf-8"))
        preview = node_snapshot.get("nodePreview", {})
        for window in ("viewMain", "viewFloat"):
            view = preview.get(window, {})
            for check in ("projectAndCompileUnchanged", "workspaceReload", "menuCancelled", "menuToggled"):
                if view.get(check) is not True:
                    raise RuntimeError(f"{locale} {window} did not verify {check}")
            for screen in ("overview", "noGrid", "contextMenu"):
                data = Path(view[screen]).read_bytes()
                if len(data) < 1024 or not data.startswith(b"\x89PNG\r\n\x1a\n"):
                    raise RuntimeError(f"{locale} {window} did not render {screen}")
        for check in ("clockPixelsChanged", "parameterPixelsChanged", "groupPixelsPreserved", "ungroupPixelsPreserved", "groupSourceReopened", "invalidGraphKeptValidPixels", "projectUntouched", "exampleOpenedAndRendered", "layerIdentityPreservedPixels", "layerGraphChangedPixels", "layerPublicParameterChangedPixels", "layerProjectUndoRedoRestoresSourceAndPixels", "layerPortPreviewIsObservational", "layerInvalidGraphKeptValidPixels", "layerSourceAndRuntimePackedAndReopened", "layerOfflinePackageRestoresPixels"):
            if preview.get(check) is not True:
                raise RuntimeError(f"{locale} node preview did not verify {check}")
        if node_snapshot["locale"] != locale or node_snapshot["floatingFrames"].get("10", 0) < 3:
            raise RuntimeError(f"{locale} did not validate the floating node workspace")
        for check in ("unusedDraftKeptLivePreview", "unusedDraftUndoRestoredPixels"):
            if preview.get(check) is not True:
                raise RuntimeError(f"{locale} node draft did not verify {check}")
        for check in ("inputDefaultsWirePrecedence", "inputDefaultsDisconnectRestoresValue", "inputDefaultsNativeEditChangedPixels", "inputDefaultsNativeVectorEdit", "inputDefaultsSourceReopened", "inputDefaultsUndoRestoredPixels"):
            if preview.get(check) is not True:
                raise RuntimeError(f"{locale} node input defaults did not verify {check}")
        for check in ("wireUniqueBodyChangedPixels", "wireChoiceDeferredAndCancelled", "wireChoiceRestoredPixels", "wireRewireCancelPreservedEdges", "wireInvalidBodyPreservedEdges", "wireSourceReopened", "wireNearestSocketChangedPixels", "wireUndoRestoredPixels"):
            if preview.get(check) is not True:
                raise RuntimeError(f"{locale} node wire targeting did not verify {check}")
        for check in ("diagnosticDrawerOpened", "diagnosticLocatedWithoutMutation", "diagnosticDrawerKeptValidPixels", "diagnosticDrawerClosed"):
            if preview.get(check) is not True:
                raise RuntimeError(f"{locale} node diagnostic drawer did not verify {check}")
        for check in ("onboardingDismissPreservedSource", "onboardingEmptyHasStartingPoint", "onboardingOpenedInTwoClicks", "onboardingEditChangedPixels", "onboardingSourceReopened", "onboardingUndoRestoredPixels"):
            if preview.get(check) is not True:
                raise RuntimeError(f"{locale} node onboarding did not verify {check}")
        for check in ("gradientThreeNodes", "gradientModesChangedPixels", "gradientNativeFalloffChangedPixels", "gradientSourceReopened", "gradientUndoRestoredPixels", "gradientProjectUntouched", "noiseThreeNodes", "noiseModesChangedPixels", "noiseOctavesChangedPixels", "noiseNativeSeedChangedPixels", "noiseSourceReopened", "noiseUndoRestoredPixels", "noiseProjectUntouched", "blendFiveNodes", "blendModesChangedPixels", "blendNativeStrengthChangedPixels", "blendSourceReopened", "blendUndoRestoredPixels", "blendRedoRestoredPixels", "blendProjectUntouched", "compactExamplesProjectUntouched"):
            if preview.get(check) is not True:
                raise RuntimeError(f"{locale} node primitive/example did not verify {check}")
        for example in ("soft_sphere", "soft_sphere_motion", "floating_lights"):
            if preview.get(example, {}).get("groupEditUndoAndReopen") is not True:
                raise RuntimeError(f"{locale} compact example did not verify {example}")
            for suffix in ("RootScreenshot", "InternalScreenshot", "EditedScreenshot"):
                data = Path(preview[example + suffix]).read_bytes()
                if len(data) < 1024 or not data.startswith(b"\x89PNG\r\n\x1a\n"):
                    raise RuntimeError(f"{locale} compact example did not render {example} {suffix}")
    verify_mcp(binary, output)
    verify_property_followups(binary, output)
    verify_style_overrides(binary, output)
    verify_property_followups(binary, output, style=output / "acceptance.override.json")
    print("Desktop languages, styled floating windows, live node rendering and shared MCP passed.")


if __name__ == "__main__":
    main()
