"""Reject untracked-license-risk files in the versioned release tree.

The check intentionally scans git-tracked paths. Local research directories are
not release inputs and remain outside this guard.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_BINARY_SUFFIXES = {".ffx", ".aep", ".aepx", ".cur", ".ani"}
PROPRIETARY_FONT_PREFIXES = (
    "msyh",
    "simsun",
    "simhei",
    "simkai",
    "simfang",
)
PUBLIC_TEXT_ROOTS = ("README.md", "packaging", "website")
PUBLIC_MARKERS = ("Adobe", "After Effects")


def tracked_paths() -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    )
    return [Path(item) for item in result.stdout.decode("utf-8").split("\0") if item]


def is_under(path: Path, root: str) -> bool:
    return path.as_posix() == root or path.as_posix().startswith(root.rstrip("/") + "/")


def main() -> int:
    paths = tracked_paths()
    errors: list[str] = []

    for path in paths:
        suffix = path.suffix.lower()
        name = path.name.lower()
        if suffix in FORBIDDEN_BINARY_SUFFIXES:
            errors.append(f"forbidden Adobe/project binary: {path}")
        if suffix in {".ttf", ".ttc", ".otf"} and name.startswith(PROPRIETARY_FONT_PREFIXES):
            errors.append(f"unapproved proprietary font filename: {path}")

    for path in paths:
        if not any(is_under(path, root) for root in PUBLIC_TEXT_ROOTS):
            continue
        if path.name == "NOTICE":
            continue
        try:
            text = (ROOT / path).read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for marker in PUBLIC_MARKERS:
            if marker in text:
                errors.append(f"Adobe trademark in release-facing text {path}: {marker}")

    if errors:
        print("provenance guard failed:", file=sys.stderr)
        print("\n".join(f"- {error}" for error in errors), file=sys.stderr)
        return 1
    print(f"provenance guard passed ({len(paths)} tracked paths)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
