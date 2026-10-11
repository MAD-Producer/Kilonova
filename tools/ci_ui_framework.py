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
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for locale in ("zh", "en"):
        report = output / f"framework-{locale}.json"
        result = subprocess.run(
            [str(args.binary.resolve()), "--smoke", str(report), "--locale", locale],
            capture_output=True, text=True, encoding="utf-8", timeout=45,
        )
        (output / f"framework-{locale}.stderr.log").write_text(result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(f"{locale} framework failed: {result.stderr}")
        data = json.loads(report.read_text(encoding="utf-8"))
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
