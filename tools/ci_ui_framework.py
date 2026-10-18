#!/usr/bin/env python3
"""Verify the standalone UI framework in real native windows, without a project."""
import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("artifacts/desktop/ui-framework"))
    parser.add_argument("--style", type=Path)
    parser.add_argument("--size", help="Native logical size, e.g. 1600x1000")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for locale in ("zh", "en"):
        report = output / f"framework-{locale}.json"
        command=[str(args.binary.resolve()), "--smoke", str(report), "--locale", locale]
        if args.style: command+= ["--style",str(args.style.resolve())]
        if args.size: command+= ["--size",args.size]
        result = subprocess.run(
            command,
            capture_output=True, text=True, encoding="utf-8", timeout=45,
        )
        (output / f"framework-{locale}.stderr.log").write_text(result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(f"{locale} framework failed: {result.stderr}")
        data = json.loads(report.read_text(encoding="utf-8"))
        dock_data = json.loads(report.with_suffix(".dock.json").read_text(encoding="utf-8"))
        for key in ("pixelMinimum", "rejectedSplitUnchanged", "savedLayoutReopened", "originalLayoutRestored", "modelAndHistoryUnchanged"):
            if dock_data.get(key) is not True:
                raise RuntimeError(f"{locale} did not verify dock {key}")
        for suffix in (".dock-before.png", ".dock-minimum.png", ".dock-layout.json"):
            if not report.with_suffix(suffix).exists():
                raise RuntimeError(f"{locale} did not capture dock acceptance {suffix}")
        if args.style and data.get("styles")!={"reloadPreservedModel":True,"invalidReloadKeptStyle":True}:
            raise RuntimeError(f"{locale} did not verify stylesheet reload")
        for key in ("ok", "independentUiOnly", "numericDragOneUndo", "undoRestored", "floatingPresented", "layoutRoundtrip"):
            if data.get(key) is not True:
                raise RuntimeError(f"{locale} did not verify {key}")
        if data.get("language") != locale or not report.with_suffix(".png").exists():
            raise RuntimeError(f"{locale} did not produce its localized screenshot")
        if not report.with_suffix(".floating.png").exists():
            raise RuntimeError(f"{locale} did not capture its native floating panel")
    print("Independent UI framework, languages, numeric undo and native floating windows passed.")


if __name__ == "__main__":
    main()
