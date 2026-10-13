#!/usr/bin/env python3
"""Pack declared effects using the Rust loader's plan and full validation."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def find_validator(requested: Path | None) -> Path:
    if requested is not None:
        executable = requested.resolve(strict=True)
        require(executable.is_file(), "validator must be an effect_tool executable")
        return executable
    configured = os.environ.get("MOTION_EFFECT_VALIDATOR")
    if configured:
        return find_validator(Path(configured))
    found = shutil.which("effect_tool")
    if found:
        return find_validator(Path(found))
    repository = Path(__file__).resolve().parents[3]
    filename = "effect_tool.exe" if os.name == "nt" else "effect_tool"
    for profile in ("release", "debug"):
        candidate = repository / "target" / profile / filename
        if candidate.is_file():
            return candidate.resolve(strict=True)
    raise FileNotFoundError("effect_tool is required: set --validator or MOTION_EFFECT_VALIDATOR, or build cargo build --locked -p motion-effects --bin effect_tool")


def validate(executable: Path, *arguments: str) -> str:
    result = subprocess.run([str(executable), *arguments], capture_output=True, text=True, encoding="utf-8")
    require(result.returncode == 0, f"effect_tool {arguments[0]} failed: {(result.stderr or result.stdout).strip()}")
    return result.stdout


def pack(source: Path, output: Path, validator: Path | None = None) -> dict:
    executable = find_validator(validator)
    base = source.resolve(strict=True)
    require(base.is_dir(), "source must be a directory")
    manifest_path = (base / "manifest.json").resolve(strict=True)
    require(manifest_path.is_relative_to(base) and manifest_path.is_file(), "manifest resolves outside source or is not a file")
    plan = json.loads(validate(executable, "plan", str(manifest_path)))
    require(plan.get("protocol") == 1, "unsupported effect_tool source-plan protocol; update the validator")
    manifest_size = manifest_path.stat().st_size
    require(manifest_size <= plan["manifestByteLimit"], "manifest exceeds SDK size limit")
    with manifest_path.open("rb") as stream:
        raw_manifest = stream.read(manifest_size + 1)
    require(len(raw_manifest) == manifest_size and hashlib.sha256(raw_manifest).hexdigest() == plan["manifestSha256"], "manifest changed during packaging")
    files = []
    total = manifest_size
    for entry in plan["files"]:
        name = entry["name"]
        path = (base / name).resolve(strict=True)
        require(path.is_relative_to(base) and path.is_file(), f"resource resolves outside source or is not a file: {name}")
        size = path.stat().st_size
        require(size <= entry["byteLimit"], f"file exceeds SDK budget: {name}")
        if name == "manifest.json":
            data = raw_manifest
        else:
            require(size <= plan["packageByteLimit"] - total, f"expanded package exceeds SDK budget: {name}")
            with path.open("rb") as stream:
                data = stream.read(size + 1)
            total += size
        require(len(data) == size, f"resource changed during packaging: {name}")
        files.append((name, data))
    destination = output.resolve()
    require(destination.suffix.lower() == ".msfx", "output must use .msfx")
    destination.parent.mkdir(parents=True, exist_ok=True)
    handle = tempfile.NamedTemporaryFile(prefix=".msfx-", suffix=".tmp", dir=destination.parent, delete=False)
    temporary = Path(handle.name)
    handle.close()
    try:
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
            for name, data in files:
                info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.create_system = 3
                info.external_attr = 0o100644 << 16
                archive.writestr(info, data)
        require(temporary.stat().st_size <= plan["packageByteLimit"], "compressed package exceeds SDK budget")
        validate(executable, "check", str(temporary))
        digest = hashlib.sha256(temporary.read_bytes()).hexdigest()
        os.replace(temporary, destination)
        return {"path": str(destination), "plugin": plan["plugin"], "version": plan["version"], "sha256": digest, "files": len(files), "expanded_bytes": total, "validation": "sdk"}
    finally:
        temporary.unlink(missing_ok=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--validator", type=Path, help="effect_tool executable; defaults to environment, PATH or repository target")
    args = parser.parse_args()
    try:
        print(json.dumps(pack(args.source, args.output, args.validator), ensure_ascii=False, indent=2))
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"pack_msfx: {error}\n")


if __name__ == "__main__":
    main()
