"""Archive validation/failure regressions; synthetic records are not GPU evidence."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import collect_desktop_performance as study


SOURCE = "0123456789abcdef" * 2 + "01234567"


def fixture(name="animated", gpu=True, samples=3):
    frames = []
    for index in range(8 + samples):
        row = {key: index + 1 for key in study.METRICS}
        row.update({key: 0 for key in study.COUNTERS})
        row.update(index=index, phase="cold" if index == 0 else "warmup" if index < 8 else "measured",
                   compositionFrame=0 if name == "static" else index, mainSubmissions=1,
                   pixelReadbackBytes=0, timestampReadbackBytes=32 if gpu else 0,
                   textureLedger={"knownBytes": 16, "unknownCapacityCount": 1, "resources": [
                       {"id": "shared-texture", "logicalBytes": 16, "references": [{"owner": "one"}, {"owner": "two"}]},
                       {"id": "unknown-texture", "logicalBytes": None, "references": []}]},
                   logicalAdmission={"budgetBytes": 1024, "admittedBytes": 32, "peakAdmittedBytes": 64,
                                     "liveAllocations": 3, "encoderScopes": 0})
        if not gpu:
            row.update({key: None for key in study.METRICS if key.startswith("gpu")})
        frames.append(row)
    return {"schemaVersion": 1, "case": name, "sourceRevision": SOURCE, "debugAssertions": False,
            "dimensions": [1280, 720], "fps": 30, "renderMode": "fixed-full-resolution", "warmupFrames": 8, "measuredFrames": samples,
            "adapter": {"name": "Protocol fixture only", "driver": "", "driverInfo": "", "backend": "Test", "deviceType": "Test"},
            "gpuTimestampSupported": gpu, "frames": frames,
            "nativeMemoryPressure": {"reading": {"status": "unavailable"}}, "coverageGaps": ["physical hardware not measured"],
            "distributions": {key: study.distribution(row[key] for row in frames[8:] if row[key] is not None) for key in study.METRICS}}


class DesktopPerformanceTests(unittest.TestCase):
    def test_percentiles_recompute_from_steady_raw_samples_and_keep_shared_unknown_capacity(self):
        report = fixture()
        result = study.validate_case(report, "animated", SOURCE, 3)
        self.assertEqual(result["distributions"]["cpuPrepareUs"], {"count": 3, "p50": 10, "p95": 11, "p99": 11, "max": 11})
        self.assertEqual(result["allFrameCounterTotals"]["mainSubmissions"], 11)
        self.assertEqual(study.distribution(iter(range(1, 101)))["p99"], 99)

    def test_absent_gpu_support_is_unknown_and_never_a_zero_cost_sample(self):
        report = fixture(gpu=False)
        result = study.validate_case(report, "animated", SOURCE, 3)
        self.assertIsNone(result["distributions"]["gpuTotalUs"])
        self.assertEqual(result["gpuSampleCoverage"], 0)
        report["frames"][0]["gpuTotalUs"] = 0
        with self.assertRaisesRegex(ValueError, "must stay unknown"):
            study.validate_case(report, "animated", SOURCE, 3)

    def test_incomplete_or_misleading_native_records_are_rejected(self):
        mutations = [
            (lambda r: r.update(sourceRevision="other"), "source"),
            (lambda r: r.update(debugAssertions=True), "Debug"),
            (lambda r: r.update(dimensions=[640, 360]), "dimensions"),
            (lambda r: r.update(renderMode="auto"), "mode"),
            (lambda r: r["frames"].pop(), "missing"),
            (lambda r: r["frames"][0].update(phase="measured"), "warmup"),
            (lambda r: r["frames"][2].update(index=1), "order"),
            (lambda r: r["frames"][1].update(index=True), "measurement"),
            (lambda r: r["frames"][0].update(pixelReadbackBytes=4), "readback"),
            (lambda r: r["frames"][0].update(cpuPrepareUs=float("nan")), "measurement"),
            (lambda r: r["frames"][0].update(videoUploadBytes=-1), "measurement"),
            (lambda r: r["frames"][0].update(gpuTotalUs=None), "measurement"),
            (lambda r: r["frames"][0]["textureLedger"].update(unknownCapacityCount=0), "Unknown capacity"),
            (lambda r: r["frames"][0]["textureLedger"].update(knownBytes=15), "subtotal"),
            (lambda r: r["frames"][0]["textureLedger"]["resources"].append(copy.deepcopy(r["frames"][0]["textureLedger"]["resources"][0])), "twice"),
            (lambda r: r["distributions"]["cpuPrepareUs"].update(p95=1), "percentiles"),
        ]
        for mutate, reason in mutations:
            with self.subTest(reason=reason):
                report = fixture()
                mutate(report)
                with self.assertRaisesRegex(ValueError, reason):
                    study.validate_case(report, "animated", SOURCE, 3)

    def test_actual_failed_child_retains_start_exit_and_original_channels(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            with self.assertRaisesRegex(ValueError, "exit 9"):
                study.run([sys.executable, "-c", "import sys;sys.stderr.write('original native failure\\n');sys.exit(9)"],
                          output, "native", None)
            self.assertEqual((output / "native.stdout.log").read_bytes(), b"")
            self.assertEqual((output / "native.stderr.log").read_text().strip(), "original native failure")
            self.assertEqual(json.loads((output / "native.exit.json").read_text())["exitCode"], 9)
            self.assertEqual(json.loads((output / "native.started.json").read_text())["phase"], "native")

    def test_all_cases_and_exact_fixture_hashes_are_required_before_archive_acceptance(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            native = output / "native"
            native.mkdir()
            cases = []
            for name in study.CASES:
                folder = native / name
                folder.mkdir()
                project = b'{"purpose":"protocol fixture, not GPU acceptance"}'
                (folder / "project.json").write_bytes(project)
                report = fixture(name)
                report["projectSha256"] = hashlib.sha256(project).hexdigest()
                (folder / "report.json").write_text(json.dumps(report))
                (folder / "final.png").write_bytes(b"protocol-fixture-not-native-pixels")
                cases.append({"case": name, "report": f"{name}/report.json", "projectSha256": report["projectSha256"]})
            (native / "report.json").write_text(json.dumps({"schemaVersion": 1, "cases": cases}))
            receipt = {"sourceRevision": SOURCE, "samples": 3}
            result = study.validate_output(output, receipt)
            self.assertFalse(result["physicalDeviceAcceptanceComplete"])
            self.assertEqual(len(result["cases"]), 6)
            (native / "static/project.json").write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "fixture changed"):
                study.validate_output(output, receipt)
            (native / "report.json").write_text(json.dumps({"schemaVersion": 1, "cases": cases[:-1]}))
            with self.assertRaisesRegex(ValueError, "six-case"):
                study.validate_output(output, receipt)

    def test_optimized_python_keeps_validation_and_does_not_replace_a_failure_with_an_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            (output / "provenance.json").write_text(json.dumps({"files": {"missing-native.json": "0" * 64}}))
            result = subprocess.run([sys.executable, "-O", str(Path(study.__file__)),
                                     "--output", str(output), "--validate-only"], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((output / "summary.json").exists())
            self.assertEqual(result.stdout, b"")


if __name__ == "__main__":
    unittest.main()
