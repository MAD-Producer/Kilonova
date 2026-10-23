"""Build and archive an explicit native, headless desktop GPU latency study.

Raw samples and failed attempts are retained. This is not window playback,
physical-device certification, a memory-pressure test, or a CI speed threshold.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
CASES = ("static", "animated", "effect-chain", "crossing-3d", "nested-matte", "four-4k-yuv")
METRICS = ("sceneSampleUs", "cpuPrepareUs", "mediaUploadWallUs", "encodeWallUs",
           "submitWallUs", "waitUs", "waitedWallUs", "gpuCompositionUs",
           "gpuPresentationUs", "gpuTotalUs")
COUNTERS = ("mainSubmissions", "videoConversionSubmissions", "videoConversions",
            "videoUploadBytes", "imageUploadBytes", "parameterUploadBytes", "parameterResourceUploadBytes",
            "instanceUploadBytes", "effectPipelineBuilds", "drawCalls")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def number(value, name, integer=False):
    require(not isinstance(value, bool) and isinstance(value, (int, float))
            and math.isfinite(value) and value >= 0, "Invalid measurement: " + name)
    if integer:
        require(isinstance(value, int), "Expected integer counter: " + name)
    return value


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def target_for_revision(base, source):
    # Cargo fingerprints can treat relative workspace paths from another
    # worktree as fresh. Never bind a new revision to those older artifacts.
    require(len(source) == 40 and all(c in "0123456789abcdef" for c in source),
            "Expected a complete source revision")
    return base.resolve() / "desktop-performance" / source


def distribution(values):
    values = sorted(values)
    if not values:
        return None
    return {"count": len(values), **{name: values[math.ceil(len(values) * p) - 1]
            for name, p in (("p50", .5), ("p95", .95), ("p99", .99))}, "max": values[-1]}


def validate_case(report, expected_case, expected_source, samples):
    require(report["schemaVersion"] == 1 and report["case"] == expected_case, "Unexpected case/schema")
    require(report["sourceRevision"] == expected_source, "Benchmark binary source does not match build receipt")
    require(report["debugAssertions"] is False, "Debug builds are not Release latency evidence")
    require(report["dimensions"] == [1280, 720] and report["fps"] == 30,
            "Fixed dimensions/fps changed")
    require(report["renderMode"] == "fixed-full-resolution", "Render mode changed")
    require(report["warmupFrames"] == 8 and report["measuredFrames"] == samples,
            "Warmup/sample contract changed")
    for key in ("name", "driver", "driverInfo", "backend", "deviceType"):
        require(isinstance(report["adapter"][key], str), "Missing actual adapter/driver metadata")
    require(bool(report["adapter"]["name"]) and bool(report["adapter"]["backend"]), "Missing native adapter")
    frames = report["frames"]
    require(len(frames) == samples + 8, "Raw samples are missing or duplicated")
    require(type(report["gpuTimestampSupported"]) is bool, "Timestamp support must be explicit")
    totals = {key: 0 for key in COUNTERS}
    for index, frame in enumerate(frames):
        number(frame["index"], "index", integer=True)
        number(frame["compositionFrame"], "compositionFrame", integer=True)
        require(frame["index"] == index, "Raw sample order changed")
        require(frame["phase"] == ("cold" if index == 0 else "warmup" if index < 8 else "measured"),
                "Cold/warmup frames cannot enter the steady distribution")
        require(frame["compositionFrame"] == (0 if expected_case == "static" else index),
                "Composition interval changed")
        require(frame["pixelReadbackBytes"] == 0, "Image readback contaminated timed samples")
        require(frame["timestampReadbackBytes"] == (32 if report["gpuTimestampSupported"] else 0),
                "Timestamp readback accounting mismatch")
        for key in METRICS:
            if key.startswith("gpu") and not report["gpuTimestampSupported"]:
                require(frame[key] is None, "Unsupported GPU timing must stay unknown")
            else:
                number(frame[key], key)
        require(frame["mainSubmissions"] == 1, "Unexpected main submission count")
        for key in COUNTERS:
            totals[key] += number(frame[key], key, integer=True)
        require(frame["drawCalls"] > 0, "Workload produced no actual draws")
        video = expected_case == "four-4k-yuv"
        require(frame["videoConversions"] == (4 if video else 0)
                and frame["videoConversionSubmissions"] == (4 if video else 0)
                and frame["videoUploadBytes"] == (4 * 3840 * 2160 * 3 // 2 if video else 0),
                "Declared native video workload was not exercised")
        ledger = frame["textureLedger"]
        number(ledger["knownBytes"], "knownBytes", integer=True)
        number(ledger["unknownCapacityCount"], "unknownCapacityCount", integer=True)
        resources = ledger["resources"]
        identities = [resource["id"] for resource in resources]
        require(len(identities) == len(set(identities)), "Shared texture counted twice")
        known = sum(number(resource["logicalBytes"], "logicalBytes", integer=True)
                    for resource in resources if resource["logicalBytes"] is not None)
        require(ledger["knownBytes"] == known, "Texture ledger subtotal differs from actual resources")
        require(ledger["unknownCapacityCount"] == sum(resource["logicalBytes"] is None for resource in resources),
                "Unknown capacity was silently counted as zero")
        # Admission is a different instrument; never equate this with texture bytes or physical residency.
        for key in ("budgetBytes", "admittedBytes", "peakAdmittedBytes", "liveAllocations", "encoderScopes"):
            number(frame["logicalAdmission"][key], key, integer=True)
    calculated = {key: distribution(frame[key] for frame in frames[8:] if frame[key] is not None)
                  for key in METRICS}
    require(report["distributions"] == calculated, "Published percentiles differ from raw samples")
    return {"case": expected_case, "adapter": report["adapter"], "distributions": calculated,
            "allFrameCounterTotals": totals, "gpuSampleCoverage": 1 if report["gpuTimestampSupported"] else 0,
            "nativeMemoryPressure": report["nativeMemoryPressure"], "coverageGaps": report["coverageGaps"]}


def validate_output(output, receipt):
    root = output / "native"
    index = json.loads((root / "report.json").read_text(encoding="utf-8"))
    require(index["schemaVersion"] == 1 and [entry["case"] for entry in index["cases"]] == list(CASES),
            "The complete six-case study is required")
    cases = []
    for name, entry in zip(CASES, index["cases"]):
        require(entry["report"] == f"{name}/report.json", "Unexpected report path")
        folder = root / name
        report = json.loads((folder / "report.json").read_text(encoding="utf-8"))
        project_hash = digest(folder / "project.json")
        require(project_hash == report["projectSha256"] == entry["projectSha256"], "Project fixture changed")
        # The PNG is retained and hashed as a file. Its decoded RGBA digest in the native
        # report is separate; the Python validator never substitutes compressed bytes.
        require((folder / "final.png").is_file(), "Missing final native image")
        cases.append(validate_case(report, name, receipt["sourceRevision"], receipt["samples"]))
    return {"schemaVersion": 1, "sourceRevision": receipt["sourceRevision"], "cases": cases,
            "scope": "Offline serialized headless GPU latency on the recorded adapter; not paced GUI playback or display FPS",
            "physicalDeviceAcceptanceComplete": False,
            "notMeasured": ["Native window/UI/display deadlines", "File video decoding", "Physical driver exhaustion",
                            "Total process RAM or physical VRAM", "Android Adreno/Mali"]}


def git(*arguments):
    return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True).strip()


def run(command, output, phase, environment):
    # Open the receipt before starting potentially blocking native work. A hung
    # process still leaves its operation, command and prior case events on disk.
    event = {"phase": phase, "command": command, "startedUnix": time.time()}
    (output / f"{phase}.started.json").write_text(json.dumps(event, indent=2), encoding="utf-8")
    with (output / f"{phase}.stdout.log").open("wb") as stdout, (output / f"{phase}.stderr.log").open("wb") as stderr:
        result = subprocess.run(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr)
    event.update(exitCode=result.returncode, elapsedSeconds=time.time() - event["startedUnix"])
    (output / f"{phase}.exit.json").write_text(json.dumps(event, indent=2), encoding="utf-8")
    require(result.returncode == 0, f"{phase} failed with exit {result.returncode}; original stdout/stderr retained")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="New artifact directory; never overwrite a prior attempt")
    parser.add_argument("--samples", type=int, default=60)
    parser.add_argument("--target-dir", type=Path, default=Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")),
                        help="Cache root; build outputs are isolated in a child directory for the exact commit")
    parser.add_argument("--validate-only", action="store_true", help="Verify existing hashes and recompute distributions without running a device")
    parser.add_argument("--environment-note", default="Uncontrolled desktop environment; no quiet-machine or thermal certification")
    args = parser.parse_args()
    output = args.output.resolve()
    require(1 <= args.samples <= 600, "Samples must be 1..600")
    if args.validate_only:
        receipt = json.loads((output / "provenance.json").read_text(encoding="utf-8"))
        for name, expected in receipt["files"].items():
            path = (output / name).resolve()
            require(path.is_relative_to(output) and digest(path) == expected, "Artifact hash mismatch: " + name)
        validate_output(output, receipt)
        print("Verified archived raw samples and hashes", file=sys.stderr)
        return
    require(not git("status", "--porcelain"), "Commit the study source before building a source-bound baseline")
    source = git("rev-parse", "HEAD")
    output.mkdir(parents=True, exist_ok=False)
    receipt = {"sourceRevision": source, "samples": args.samples, "sourceDirty": False,
               "environment": {"platform": platform.platform(), "machine": platform.machine(),
                               "cpuCount": os.cpu_count(), "note": args.environment_note[:1024]},
               "physicalDeviceAcceptanceComplete": False}
    try:
        environment = dict(os.environ, MOTION_BENCHMARK_REVISION=source)
        target = target_for_revision(args.target_dir, source)
        run(["cargo", "build", "--release", "--locked", "-p", "motion-render", "--example", "performance_baseline",
             "--target-dir", str(target)], output, "build", environment)
        binary = target / "release/examples" / ("performance_baseline.exe" if os.name == "nt" else "performance_baseline")
        receipt["binary"] = {"path": str(binary), "sha256": digest(binary)}
        (output / "provenance.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
        run([str(binary), str(output / "native"), str(args.samples)], output, "native", environment)
        require(git("rev-parse", "HEAD") == source and not git("status", "--porcelain"), "Source changed during measurement")
        require(digest(binary) == receipt["binary"]["sha256"], "Benchmark binary changed during measurement")
        summary = validate_output(output, receipt)
        (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
        receipt["files"] = {path.relative_to(output).as_posix(): digest(path)
                            for path in sorted(output.rglob("*")) if path.is_file() and path.name != "provenance.json"}
        (output / "provenance.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
        print("Archived six native studies: " + str(output), file=sys.stderr)
    except Exception as error:
        # Preserve the primary cause and every partial artifact, including native
        # case starts. No retry, assertion relaxation or output directory reuse.
        failure = {"sourceRevision": source, "cause": str(error)[:4096], "physicalDeviceAcceptanceComplete": False}
        try:
            (output / "failure.json").write_text(json.dumps(failure, indent=2), encoding="utf-8")
        except OSError as archive_error:
            print(f"Primary study failure: {error}; failure archive also failed: {archive_error}", file=sys.stderr)
        raise


if __name__ == "__main__":
    main()
