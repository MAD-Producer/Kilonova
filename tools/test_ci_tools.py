"""Offline regressions for optimized validators and explicit execution coverage."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest

import validation_inventory as inventory

ROOT = Path(__file__).resolve().parents[1]


def child(arguments, optimized=False, extra_env=None):
    env = os.environ.copy()
    env["PYTHONPATH"] = str(ROOT / "tools") + os.pathsep + env.get("PYTHONPATH", "")
    if extra_env:
        env.update(extra_env)
    return subprocess.run(
        [sys.executable, *(["-O"] if optimized else []), *map(str, arguments)],
        env=env, capture_output=True, text=True, encoding="utf-8", timeout=30,
    )


class InventoryTest(unittest.TestCase):
    def setUp(self):
        self.plan = inventory.read_inventory()
        self.directory = tempfile.TemporaryDirectory(prefix="motion-tools-test-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def android_copy(self):
        source = self.root / "android"
        shutil.copytree(ROOT / inventory.ANDROID_SOURCE, source)
        return source

    def tool_copy(self):
        for name in self.plan["tools"]:
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, target)
        target = self.root / ".github/workflows/ci.yml"
        target.parent.mkdir(parents=True)
        shutil.copy2(ROOT / ".github/workflows/ci.yml", target)
        return self.root

    def test_repository_tools_are_classified_and_keep_runtime_checks(self):
        self.assertEqual(inventory.check_tools(ROOT, self.plan), len(self.plan["tools"]))
        selectors, expected = inventory.android_suite(ROOT / inventory.ANDROID_SOURCE, self.plan)
        self.assertGreaterEqual(len(selectors), 38)
        self.assertGreaterEqual(expected, 103)
        result = child([ROOT / "tools/ci_android_ui.py", "--list-tests"], optimized=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {"classes": selectors, "tests": expected})

    def test_new_unclassified_tool_or_missing_tool_is_rejected(self):
        root = self.tool_copy()
        new = root / "tools/new_validator.py"
        new.write_text("raise SystemExit(0)\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "unclassified=.*new_validator"):
            inventory.check_tools(root, self.plan)
        new.unlink()
        (root / "tools/validate_audio.py").unlink()
        with self.assertRaisesRegex(ValueError, "missing=.*validate_audio"):
            inventory.check_tools(root, self.plan)

    def test_production_assert_or_removed_ci_wiring_is_rejected(self):
        root = self.tool_copy()
        source = root / "tools/validate_audio.py"
        original = source.read_text(encoding="utf-8")
        source.write_text(original + "\nassert False\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "optimization-sensitive assert"):
            inventory.check_tools(root, self.plan)
        source.write_text(original, encoding="utf-8")
        workflow = root / ".github/workflows/ci.yml"
        workflow.write_text(workflow.read_text(encoding="utf-8").replace("tools/ci_android_ui.py", "tools/unwired.py"), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "CI no longer invokes"):
            inventory.check_tools(root, self.plan)

    def test_android_ci_rejects_a_missing_selected_class_before_adb(self):
        source = self.android_copy()
        (source / "EffectPreviewBackendTest.kt").unlink()
        result = child(["-c", textwrap.dedent("""
            import sys
            from pathlib import Path
            import ci_android_ui as ci
            ci.SOURCE = Path(sys.argv[1])
            sys.argv = ['ci_android_ui.py', '--list-tests']
            ci.main()
        """), source], optimized=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing=['EffectPreviewBackendTest']", result.stderr)

    def test_android_new_unclassified_class_and_missing_method_are_rejected(self):
        source = self.android_copy()
        new = source / "NewAcceptanceTest.kt"
        new.write_text("class NewAcceptanceTest { @Test fun newAcceptance() {} }", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "unclassified=.*NewAcceptanceTest"):
            inventory.android_suite(source, self.plan)
        new.unlink()
        frontend = source / "FrontendLayerControlsTest.kt"
        frontend.write_text(frontend.read_text(encoding="utf-8").replace(
            "separatedKeyDragUsesCompositionTimeAndOnlyCapturedAxis", "renamedMethod"), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Selected Android test method is missing"):
            inventory.android_suite(source, self.plan)

    def test_new_method_in_a_selected_full_class_increases_expected_count(self):
        source = self.android_copy()
        selectors, before = inventory.android_suite(source, self.plan)
        path = source / "EffectPreviewBackendTest.kt"
        text = path.read_text(encoding="utf-8")
        offset = text.rfind("}")
        path.write_text(text[:offset] + "\n@Test fun additionalAcceptance() {}\n" + text[offset:], encoding="utf-8")
        after_selectors, after = inventory.android_suite(source, self.plan)
        self.assertEqual(after_selectors, selectors)
        self.assertEqual(after, before + 1)

    def test_kotlin_fixture_text_does_not_create_fake_classes_or_methods(self):
        source = '''
            // class CommentTest { @Test fun hidden() {} }
            class RealTest {
                val shader = """class StringTest { @Test fun hidden() {} }"""
                @Test fun actual() { val brace = "}" }
            }
        '''
        self.assertEqual(inventory.kotlin_test_classes(source), {"RealTest": ["actual"]})

    def test_effect_libraries_use_the_real_loader_when_available(self):
        executable = os.environ.get("MOTION_EFFECT_VALIDATOR")
        if not executable:
            self.skipTest("set MOTION_EFFECT_VALIDATOR to run Rust schema/WGSL validation")
        inventory.check_effect_libraries(ROOT, self.plan, Path(executable).resolve())


class OptimizedValidatorTest(unittest.TestCase):
    def assert_failure(self, result, message):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(message, result.stderr)

    def test_cli_input_checks_survive_optimization(self):
        for optimized in (False, True):
            for name in ("validate_media_formats.py", "validate_video_inputs.py"):
                with self.subTest(script=name, optimized=optimized):
                    result = child([ROOT / "tools" / name, "--adb", "unused", "--serial", "unused",
                                    "--adb-port", "0", "--output", "unused"], optimized)
                    self.assert_failure(result, "ADB port must be 1..65535")
            result = child([ROOT / "tools/compare_export_frames.py", "unused", "unused", "--frame", "-1"], optimized)
            self.assert_failure(result, "Frame index must be nonnegative")
        result = child([ROOT / "tools/compare_export_frames.py", "unused", "unused", "--frame", "-1"],
                       extra_env={"PYTHONOPTIMIZE": "1"})
        self.assert_failure(result, "Frame index must be nonnegative")

    def test_compare_rejects_a_nonopaque_reference_in_optimized_mode(self):
        result = child(["-c", textwrap.dedent("""
            import json, sys, types
            import compare_export_frames as validator
            class Image:
                def convert(self, _): return self
                def getchannel(self, _): return self
                def getextrema(self): return (0, 255)
            pil = types.ModuleType('PIL')
            pil.Image = types.SimpleNamespace(open=lambda _: Image())
            sys.modules['PIL'] = pil
            validator.subprocess.check_output = lambda _: json.dumps({'streams':[{'width':1,'height':1}]}).encode()
            validator.compare('unused.mp4', 'reference.png', 0)
        """)], optimized=True)
        self.assert_failure(result, "opaque background")

    def test_actual_video_main_rejects_bad_rgb_parity_under_optimization(self):
        code = textwrap.dedent(r"""
            import json, sys, tempfile
            from pathlib import Path
            import validate_video as validator
            with tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                validator.__file__ = str(root/'tools/validate_video.py')
                fixtures = root/'crates/motion-media/tests/fixtures/video'
                fixtures.mkdir(parents=True)
                (fixtures/'manifest.json').write_text(json.dumps({'variable.mp4':{'frames':[{'best_effort_timestamp_time':'0'}]}}))
                report = {'metadata':{'video':{'variable_frame_rate':True,'rotation':0}}, 'samples':[
                    {'pts_us':0,'end_us':1,'width':1,'height':1,'source_transfer':'rgba','decoder':'fixture','decode_us':0}]}
                def run(*args):
                    if args[0]=='ffmpeg': return bytes([0,0,0,255])
                    if 'find' in args: return b'files/acceptance/case/video-report.json\n'
                    if args[-1].endswith('video-report.json'): return json.dumps(report).encode()
                    return bytes([255,255,255,255])
                validator.run = run
                sys.argv = ['validate_video.py','--adb','unused','--serial','unused','--output',str(root/'evidence')]
                validator.main()
        """)
        result = child(["-c", code], optimized=True)
        self.assert_failure(result, "mean_absolute_rgb_error")

    def test_actual_audio_main_rejects_bad_pcm_parity_under_optimization(self):
        code = textwrap.dedent("""
            import array, json, sys, tempfile, types
            from pathlib import Path
            import validate_audio as validator
            with tempfile.TemporaryDirectory() as folder:
                validator.ROOT = Path(folder)
                validator.shutil.which = lambda _: 'fixture-ffmpeg'
                def run(args, **kwargs):
                    if args[0]=='fixture-ffmpeg':
                        kwargs['stdout'].write(array.array('f',[0.0,0.0]).tobytes())
                        return types.SimpleNamespace(returncode=0)
                    project=Path(args[-1]); cache=project/'cache/audio-v1'
                    cache.mkdir(parents=True)
                    (cache/'source.pcm').write_bytes(array.array('f',[1.0,1.0]).tobytes())
                    return types.SimpleNamespace(stdout=json.dumps({'metadata':{'path':'assets/source.wav','channels':1,'duration_us':2000000},'elapsed_ms':0}))
                validator.subprocess.run = run
                sys.argv=['validate_audio.py','--probe','unused','--output',str(validator.ROOT/'evidence')]
                validator.main()
        """)
        result = child(["-c", code], optimized=True)
        self.assert_failure(result, "rms_error")

    def test_actual_media_format_main_rejects_failed_parity_under_optimization(self):
        code = textwrap.dedent(r"""
            import array, hashlib, json, sys, tempfile
            from pathlib import Path
            import validate_media_formats as validator
            with tempfile.TemporaryDirectory() as folder:
                root=Path(folder);validator.__file__=str(root/'tools/validate_media_formats.py')
                fixtures=root/'crates/motion-media/tests/fixtures/formats';fixtures.mkdir(parents=True)
                (fixtures/'manifest.json').write_text(json.dumps({'pcm.wav':{'sha256':hashlib.sha256(b'owned').hexdigest(),'streams':[{'codec_name':'pcm_s16le','sample_rate':48000}]}}))
                report={'fixture':'pcm.wav','asset':{'path':'assets/pcm.wav','sample_rate':48000,'channels':1}}
                def run(*args):
                    if args[0]=='ffmpeg':return array.array('f',[0.0]).tobytes()
                    if 'find' in args:return b'files/acceptance/media-formats-case/audio-report.json\n'
                    if 'stat' in args:return b'1'
                    if args[-1].endswith('report.json'):return json.dumps(report).encode()
                    if args[-1].endswith('.pcm'):return array.array('f',[1.0]).tobytes()
                    return b'owned'
                validator.run=run
                sys.argv=['validate_media_formats.py','--adb','unused','--serial','unused','--output',str(root/'evidence')]
                validator.main()
        """)
        result = child(["-c", code], optimized=True)
        self.assert_failure(result, "rms_error")

    def test_actual_video_input_main_rejects_failed_parity_under_optimization(self):
        code = textwrap.dedent(r"""
            import hashlib, json, sys, tempfile
            from pathlib import Path
            import validate_video_inputs as validator
            names=['uhd-60.mp4','dci-60.mp4','portrait-60.mp4','hfr-240.mp4','fractional-59.94.mp4','ultrawide-60.mp4','square-60.mp4','tall-60.mp4']
            with tempfile.TemporaryDirectory() as folder:
                root=Path(folder);validator.__file__=str(root/'tools/validate_video_inputs.py')
                fixtures=root/'crates/motion-media/tests/fixtures/inputs';fixtures.mkdir(parents=True)
                manifest={name:{'sha256':hashlib.sha256(b'owned').hexdigest(),'frames':[{'best_effort_timestamp_time':'0'}]} for name in names}
                (fixtures/'manifest.json').write_text(json.dumps(manifest))
                def run(*args):
                    if args[0]=='ffmpeg':return bytes([0,0,0,255])
                    if 'find' in args:return ''.join('files/acceptance/'+name+'/input-report.json\n' for name in names).encode()
                    if 'stat' in args:return b'1'
                    if args[-1].endswith('report.json'):
                        name=Path(args[-1]).parent.name
                        return json.dumps({'fixture':name,'asset':{'path':'assets/source.mp4'},'samples':[{'pts_us':0,'width':1,'height':1}]}).encode()
                    if args[-1].endswith('.rgba'):return bytes([255,255,255,255])
                    return b'owned'
                validator.run=run
                sys.argv=['validate_video_inputs.py','--adb','unused','--serial','unused','--output',str(root/'evidence')]
                validator.main()
        """)
        result = child(["-c", code], optimized=True)
        self.assert_failure(result, "mae")

    def test_builtin_generator_rejects_duplicate_effects_under_optimization(self):
        with tempfile.TemporaryDirectory(prefix="motion-generator-test-") as folder:
            root = Path(folder)
            script = root / "tools/effect-library/generate_builtin.py"
            script.parent.mkdir(parents=True)
            shutil.copy2(ROOT / "tools/effect-library/generate_builtin.py", script)
            for name in ("library", "motion-library", "builtin-library"):
                (root / "crates/motion-effects" / name).mkdir(parents=True)
            for name in ("library", "motion-library"):
                source = root / "crates/motion-effects" / name
                (source / "manifest.json").write_text(json.dumps({"effects": [{"id": "duplicate", "passes": []}]}), encoding="utf-8")
            result = child([script], optimized=True)
            self.assert_failure(result, "duplicate")


class PerformanceBudgetTest(unittest.TestCase):
    def report(self):
        return {
            "frames": [], "frameCount": 60, "gpuSampleCoverage": 1.0,
            "cpuPrepareUs": {"p95": 4000}, "gpuTotalUs": {"p95": 10000},
            "metadata": {"graphics": {"assetTextureBytes": 128 * 1024 * 1024}},
            "android": {"debuggable": False, "displayRefreshHz": 60,
                        "memoryAndThermal": [{"pssKiB": 400 * 1024}]},
            "workload": {"maximumClippedBoundingBoxFillScreens": 4},
        }

    def run_report(self, root, report, optimized):
        source = root / "report.json"
        source.write_text(json.dumps(report), encoding="utf-8")
        return child([ROOT / "tools/validate_performance.py", "--report", source,
                      "--output", root / "evidence"], optimized)

    def test_each_exceeded_budget_fails_real_cli_and_retains_summary(self):
        cases = [
            (lambda r: r["cpuPrepareUs"].update(p95=40000), "cpuPrepareP95AtMost4ms"),
            (lambda r: r["gpuTotalUs"].update(p95=10001), "gpuTotalP95AtMost10ms"),
            (lambda r: r["android"]["memoryAndThermal"][0].update(pssKiB=400 * 1024 + 1), "pssAtMost400MiB"),
            (lambda r: r["metadata"]["graphics"].update(assetTextureBytes=128 * 1024 * 1024 + 1), "assetTexturesAtMost128MiB"),
            (lambda r: r["workload"].update(maximumClippedBoundingBoxFillScreens=4.01), "referenceFillAtMostFourScreens"),
        ]
        for optimized in (False, True):
            for change, check in cases:
                with self.subTest(check=check, optimized=optimized), tempfile.TemporaryDirectory() as folder:
                    root = Path(folder)
                    report = self.report()
                    change(report)
                    result = self.run_report(root, report, optimized)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("Performance budget exceeded", result.stderr)
                    summary = json.loads((root / "evidence/summary.json").read_text(encoding="utf-8"))
                    self.assertIs(summary["measuredBudgetChecks"][check], False)

    def test_boundary_values_pass_without_claiming_complete_device_acceptance(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            result = self.run_report(root, self.report(), optimized=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads((root / "evidence/summary.json").read_text(encoding="utf-8"))
            self.assertTrue(all(summary["measuredBudgetChecks"].values()))
            self.assertIs(summary["fullA12AcceptanceComplete"], False)

    def test_unreliable_gpu_coverage_remains_unknown_and_debuggable_reports_fail(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            report = self.report()
            report["gpuSampleCoverage"] = 0.5
            report["gpuTotalUs"]["p95"] = 40000
            result = self.run_report(root, report, optimized=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads((root / "evidence/summary.json").read_text(encoding="utf-8"))
            self.assertIsNone(summary["measuredBudgetChecks"]["gpuTotalP95AtMost10ms"])
            self.assertIn("Reliable GPU timestamp coverage", summary["missingGates"])
            report["android"]["debuggable"] = True
            result = self.run_report(root, report, optimized=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Debuggable runs", result.stderr)

    def test_invalid_numeric_measurements_cannot_pass_as_valid_budgets(self):
        for value in (-1, True, float("nan"), "4000"):
            with self.subTest(value=value), tempfile.TemporaryDirectory() as folder:
                report = self.report()
                report["cpuPrepareUs"]["p95"] = value
                result = self.run_report(Path(folder), report, optimized=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Invalid performance measurement", result.stderr)


if __name__ == "__main__":
    unittest.main()
