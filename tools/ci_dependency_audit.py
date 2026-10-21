"""Audit the complete lockfile with a verified, pinned RustSec executable."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import platform
import subprocess
import sys
import tarfile
import urllib.request
import zipfile

VERSION = "0.22.2"
RELEASE = f"https://github.com/rustsec/rustsec/releases/download/cargo-audit/v{VERSION}"
ARCHIVES = {
    "Windows": ("x86_64-pc-windows-msvc", "zip", "0a7316540862c13d954f648917ceacca593747baed6eec180fafa590be2710ab"),
    "Linux": ("x86_64-unknown-linux-gnu", "tgz", "ab28a1bdb54db4d5d8ad5981cf1f959410370b3d28250dbd35f6a44248620e39"),
}


def install(directory):
    system = platform.system()
    if system not in ARCHIVES or platform.machine().lower() not in ("amd64", "x86_64"):
        raise ValueError("the pinned audit executable supports Linux/Windows x86_64")
    target, extension, expected = ARCHIVES[system]
    url = f"{RELEASE}/cargo-audit-{target}-v{VERSION}.{extension}"
    print(f"dependency audit: downloading pinned cargo-audit {VERSION} for {target}", file=sys.stderr)
    with urllib.request.urlopen(url, timeout=60) as response:
        data = response.read(64 * 1024 * 1024 + 1)
    if len(data) > 64 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != expected:
        raise ValueError(f"audit executable archive checksum mismatch: {url}")
    name = "cargo-audit.exe" if system == "Windows" else "cargo-audit"
    if extension == "zip":
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            entries = [entry for entry in archive.infolist() if Path(entry.filename).name == name and not entry.is_dir()]
            if len(entries) != 1:
                raise ValueError("audit archive must contain exactly one executable")
            executable = archive.read(entries[0])
    else:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            entries = [entry for entry in archive.getmembers() if Path(entry.name).name == name and entry.isfile()]
            if len(entries) != 1:
                raise ValueError("audit archive must contain exactly one executable")
            source = archive.extractfile(entries[0])
            if source is None:
                raise ValueError("audit executable is unreadable")
            with source:
                executable = source.read()
    directory.mkdir(parents=True, exist_ok=True)
    binary = directory / name
    binary.write_bytes(executable)
    binary.chmod(0o755)
    return binary.resolve()


def run(binary, lockfile, database, output):
    output.mkdir(parents=True, exist_ok=True)
    print(f"dependency audit: starting lockfile={lockfile} database={database}", file=sys.stderr)
    version = subprocess.run([str(binary), "--version"], capture_output=True, check=True, timeout=30)
    if version.stdout.decode("utf-8").strip() != f"cargo-audit {VERSION}":
        raise ValueError("audit executable version differs from the pinned release")
    revision = subprocess.check_output(["git", "-C", str(database), "rev-parse", "HEAD"], text=True, timeout=30).strip()
    command = [str(binary), "audit", "--json", "--no-yanked", "--no-fetch", "--file", str(lockfile), "--db", str(database)]
    try:
        result = subprocess.run(command, capture_output=True, timeout=180)
    except subprocess.TimeoutExpired as error:
        (output / "audit.json").write_bytes(error.stdout or b"")
        (output / "audit.stderr.log").write_bytes(error.stderr or b"")
        raise
    (output / "audit.json").write_bytes(result.stdout)
    (output / "audit.stderr.log").write_bytes(result.stderr)
    # A download/database/parse failure remains a failure, even without a report.
    if not result.stdout:
        raise RuntimeError(f"cargo-audit exited {result.returncode} without a JSON report; see {output / 'audit.stderr.log'}")
    report = json.loads(result.stdout)
    if not isinstance(report, dict) or not isinstance(report.get("vulnerabilities"), dict):
        raise ValueError("audit JSON report has no vulnerability result")
    vulnerabilities = report["vulnerabilities"]
    count = vulnerabilities.get("count")
    entries = vulnerabilities.get("list")
    if type(count) is not int or count < 0 or not isinstance(entries, list):
        raise ValueError("audit JSON report has invalid vulnerability fields")
    if type(vulnerabilities.get("found")) is not bool or vulnerabilities["found"] != (count > 0) or count != len(entries):
        raise ValueError("inconsistent audit vulnerability report")
    receipt = {
        "toolVersion": VERSION, "advisoryRevision": revision,
        "lockfileSha256": hashlib.sha256(lockfile.read_bytes()).hexdigest(),
        "exitCode": result.returncode, "vulnerabilities": count,
        "warnings": report.get("warnings", {}),
    }
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
    print(f"dependency audit: {count} vulnerabilities; exit={result.returncode}; database={revision}; report={output / 'audit.json'}", file=sys.stderr)
    return result.returncode if result.returncode else int(count > 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lockfile", type=Path, default=Path("Cargo.lock"))
    parser.add_argument("--database", type=Path, required=True, help="fresh RustSec advisory-db Git checkout; fetch failures must stop CI")
    parser.add_argument("--output", type=Path, default=Path("artifacts/dependency-audit"))
    parser.add_argument("--binary", type=Path, help="existing cargo-audit executable of the pinned version")
    args = parser.parse_args()
    try:
        binary = args.binary.resolve() if args.binary else install(args.output / "bin")
        return run(binary, args.lockfile.resolve(), args.database.resolve(), args.output.resolve())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, tarfile.TarError, zipfile.BadZipFile) as error:
        print(f"dependency audit failed: {type(error).__name__}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
