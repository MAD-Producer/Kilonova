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


def cpu_fixture(video):
    def buffer(payload, capacity):
        return {"payloadBytes": payload, "capacityBytes": capacity}

    package = {"id": 1, "plugin": "protocol-fixture", "hash": "same-contents",
               "registryReferences": 2, "programReferences": 3, "fileCount": 1, "shaderCount": 1,
               "archive": buffer(10, 16), "expandedFiles": buffer(20, 32), "compiledShaderText": buffer(30, 64)}
    packages = {"version": 1, "completeCoverage": False, "unknownBytes": None,
                "unobservedReferences": 0, "allPackageOwnersObserved": True,
                "knownPayloadBytes": 60, "knownCapacityBytes": 112, "resources": [package]}
    shader = {"id": 1, "programReferences": 2, "programs": ["program-one", "program-two"],
              "omittedProgramContexts": 0, "text": buffer(40, 64)}
    shaders = {"version": 1, "completeCoverage": False, "unknownBytes": None,
               "unobservedReferences": 0, "allProgramOwnersObserved": True,
               "knownPayloadBytes": 40, "knownCapacityBytes": 64, "resources": [shader]}
    size = 3840 * 2160
    generated = {"owner": "one_reusable_generated_frame", "y": buffer(size, size + 16),
                 "uv": buffer(size // 2, size // 2 + 32),
                 "knownPayloadBytes": size * 3 // 2, "knownCapacityBytes": size * 3 // 2 + 48} if video else None
    return {"scope": "benchmark_observed_owned_buffers_not_process_ram", "completeCoverage": False,
            "unknownBytes": None, "effectPackages": packages, "effectProgramShaders": shaders,
            "generatedYuv": generated, "knownPayloadBytes": 100 + (generated["knownPayloadBytes"] if video else 0),
            "knownCapacityBytes": 176 + (generated["knownCapacityBytes"] if video else 0)}


def fixture(name="animated", gpu=True, samples=3):
    frames = []
    for index in range(8 + samples):
        row = {key: index + 1 for key in study.METRICS}
        row.update({key: 0 for key in study.COUNTERS})
        row.update(index=index, phase="cold" if index == 0 else "warmup" if index < 8 else "measured",
                   compositionFrame=0 if name == "static" else index, mainSubmissions=1,
                   drawCalls=4, videoConversions=4 if name == "four-4k-yuv" else 0,
                   videoConversionSubmissions=4 if name == "four-4k-yuv" else 0,
                   videoUploadBytes=4 * 3840 * 2160 * 3 // 2 if name == "four-4k-yuv" else 0,
                   pixelReadbackBytes=0, timestampReadbackBytes=32 if gpu else 0,
                   cpuMemory=cpu_fixture(name == "four-4k-yuv"),
                   textureLedger={"knownBytes": 16, "unknownCapacityCount": 1, "resources": [
                       {"id": "shared-texture", "logicalBytes": 16, "references": [{"owner": "one"}, {"owner": "two"}]},
                       {"id": "unknown-texture", "logicalBytes": None, "references": []}]},
                   logicalAdmission={"budgetBytes": 1024, "admittedBytes": 32, "peakAdmittedBytes": 64,
                                     "liveAllocations": 3, "encoderScopes": 0})
        if not gpu:
            row.update({key: None for key in study.METRICS if key.startswith("gpu")})
        frames.append(row)
    return {"schemaVersion": 2, "case": name, "sourceRevision": SOURCE, "debugAssertions": False,
            "dimensions": [1280, 720], "fps": 30, "renderMode": "fixed-full-resolution", "warmupFrames": 8, "measuredFrames": samples,
            "memoryPolicy": {"sourceTextureBudgetBytes": 256 << 20},
            "syntheticYuv": {"dimensions": [3840, 2160], "sources": 4,
                             "oneReusableCpuFixtureBytes": 3840 * 2160 * 3 // 2,
                             "decoderMeasured": False} if name == "four-4k-yuv" else None,
            "adapter": {"name": "Protocol fixture only", "driver": "", "driverInfo": "", "backend": "Test", "deviceType": "Test"},
            "gpuTimestampSupported": gpu, "frames": frames,
            "nativeMemoryPressure": {"reading": {"status": "unavailable"}}, "coverageGaps": ["physical hardware not measured"],
            "distributions": {key: study.distribution(row[key] for row in frames[8:] if row[key] is not None) for key in study.METRICS}}


class DesktopPerformanceTests(unittest.TestCase):
    def test_cpu_capacity_keeps_shared_references_and_independent_equal_packages_separate(self):
        report = fixture()
        # Five owners share the original allocation. Equal content in a second
        # independently allocated package must still contribute its capacity.
        for row in report["frames"]:
            cpu = row["cpuMemory"]
            package = copy.deepcopy(cpu["effectPackages"]["resources"][0])
            package.update(id=2, registryReferences=1, programReferences=0)
            cpu["effectPackages"]["resources"].append(package)
            cpu["effectPackages"]["knownPayloadBytes"] += 60
            cpu["effectPackages"]["knownCapacityBytes"] += 112
            cpu["knownPayloadBytes"] += 60
            cpu["knownCapacityBytes"] += 112
        result = study.validate_case(report, "animated", SOURCE, 3)
        self.assertEqual(result["cpuMemory"]["knownCapacityBytes"]["p95"], 288)
        self.assertIsNone(result["cpuMemory"]["unknownBytes"])
        self.assertFalse(result["cpuMemory"]["completeCoverage"])

    def test_cpu_double_counts_missing_planes_and_false_complete_coverage_are_rejected(self):
        mutations = [
            (lambda c: c.update(knownCapacityBytes=c["knownCapacityBytes"] * 2), "aggregate subtotal"),
            (lambda c: c.update(completeCoverage=True), "Partial CPU"),
            (lambda c: c.update(unknownBytes=0), "Partial CPU"),
            (lambda c: c["effectPackages"].update(knownCapacityBytes=0), "domain subtotal"),
            (lambda c: c["effectPackages"]["resources"].append(copy.deepcopy(c["effectPackages"]["resources"][0])), "twice"),
            (lambda c: c["effectProgramShaders"].update(unknownBytes=0), "Partial CPU"),
            (lambda c: c["effectProgramShaders"].update(unobservedReferences=1), "owner coverage"),
            (lambda c: c["effectProgramShaders"]["resources"][0].update(programs=["x"] * 9), "Unbounded CPU"),
            (lambda c: c["effectProgramShaders"]["resources"][0]["text"].update(capacityBytes=1), "payload exceeds"),
            (lambda c: c["generatedYuv"]["uv"].update(payloadBytes=0), "one CPU YUV"),
            (lambda c: c["generatedYuv"]["y"].update(payloadBytes=4 * 3840 * 2160, capacityBytes=4 * 3840 * 2160), "one CPU YUV"),
        ]
        for mutate, reason in mutations:
            with self.subTest(reason=reason):
                report = fixture("four-4k-yuv")
                mutate(report["frames"][0]["cpuMemory"])
                with self.assertRaisesRegex(ValueError, reason):
                    study.validate_case(report, "four-4k-yuv", SOURCE, 3)

    def test_old_schema_and_absent_cpu_domain_do_not_masquerade_as_current_acceptance(self):
        report = fixture()
        report["schemaVersion"] = 1
        with self.assertRaisesRegex(ValueError, "schema"):
            study.validate_case(report, "animated", SOURCE, 3)
        report = fixture()
        del report["frames"][0]["cpuMemory"]["effectProgramShaders"]
        with self.assertRaises(KeyError):
            study.validate_case(report, "animated", SOURCE, 3)
        report = fixture("four-4k-yuv")
        report["syntheticYuv"]["decoderMeasured"] = True
        with self.assertRaisesRegex(ValueError, "not file decoding"):
            study.validate_case(report, "four-4k-yuv", SOURCE, 3)

    def test_dependency_reuse_rejects_changes_to_any_native_source_or_build_manifest(self):
        study.verify_dependency_cache_changes([])
        study.verify_dependency_cache_changes(list(study.BENCHMARK_INPUTS))
        for changed in ("crates/motion-model/src/lib.rs", "crates/motion-core/src/expressions/mod.rs",
                        "crates/motion-render/Cargo.toml", "Cargo.lock", "Cargo.toml", ".cargo/config.toml",
                        "crates/motion-effects/motion-library/manifest.json", "unknown-input"):
            with self.subTest(input=changed):
                with self.assertRaisesRegex(ValueError, "dependency inputs differ"):
                    study.verify_dependency_cache_changes([changed])

    def test_build_outputs_are_isolated_by_complete_source_revision(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            first = study.target_for_revision(base, SOURCE)
            second = study.target_for_revision(base, "f" * 40)
            self.assertNotEqual(first, second)
            self.assertEqual(first, base.resolve() / "desktop-performance" / SOURCE)
            self.assertTrue(first.is_relative_to(base.resolve()))
            for invalid in ("main", SOURCE[:8], "../" + SOURCE, "G" * 40):
                with self.assertRaisesRegex(ValueError, "complete source"):
                    study.target_for_revision(base, invalid)

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
            (lambda r: r["memoryPolicy"].update(sourceTextureBudgetBytes=128 << 20), "budget"),
            (lambda r: r["frames"].pop(), "missing"),
            (lambda r: r["frames"][0].update(phase="measured"), "warmup"),
            (lambda r: r["frames"][2].update(index=1), "order"),
            (lambda r: r["frames"][1].update(index=True), "measurement"),
            (lambda r: r["frames"][0].update(pixelReadbackBytes=4), "readback"),
            (lambda r: r["frames"][0].update(cpuPrepareUs=float("nan")), "measurement"),
            (lambda r: r["frames"][0].update(videoUploadBytes=-1), "measurement"),
            (lambda r: r["frames"][0].update(drawCalls=0), "actual draws"),
            (lambda r: r["frames"][0].update(videoConversions=1), "workload"),
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

    def test_four_native_4k_conversions_cannot_be_replaced_with_empty_or_cached_work(self):
        report = fixture("four-4k-yuv")
        study.validate_case(report, "four-4k-yuv", SOURCE, 3)
        for key in ("videoConversions", "videoConversionSubmissions", "videoUploadBytes"):
            with self.subTest(counter=key):
                changed = copy.deepcopy(report)
                changed["frames"][1][key] = 0
                with self.assertRaisesRegex(ValueError, "workload"):
                    study.validate_case(changed, "four-4k-yuv", SOURCE, 3)

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
            (native / "report.json").write_text(json.dumps({"schemaVersion": 2, "cases": cases}))
            receipt = {"sourceRevision": SOURCE, "samples": 3}
            result = study.validate_output(output, receipt)
            self.assertFalse(result["physicalDeviceAcceptanceComplete"])
            self.assertEqual(len(result["cases"]), 6)
            (native / "static/project.json").write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "fixture changed"):
                study.validate_output(output, receipt)
            (native / "report.json").write_text(json.dumps({"schemaVersion": 2, "cases": cases[:-1]}))
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
