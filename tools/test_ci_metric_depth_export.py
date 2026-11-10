"""Metric export admission; actual learned numerical evidence is separate."""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import export_ai_depth as exporter


class MetricExportAdmission(unittest.TestCase):
    def test_native_aspect_profile_is_explicit_and_cannot_relabel_existing_exports(self):
        identity=exporter.processing_identity(518,metric=True,native_aspect=True)
        self.assertEqual(identity,exporter.METRIC_NATIVE_PROCESSING)
        self.assertNotEqual(identity,exporter.processing_identity(518,metric=True))
        with tempfile.TemporaryDirectory() as name:
            root=Path(name).resolve()
            for kwargs in (dict(native_aspect=True),dict(metric=True,native_aspect=1),
                           dict(metric=True,native_aspect="native")):
                with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                    exporter.export(root/'reference',root/'weights',root/'absent'/'output',**kwargs)
            self.assertFalse((root/'absent').exists())

    def test_real_native_aspect_cli_requires_metric_before_io_under_optimization(self):
        with tempfile.TemporaryDirectory() as name:
            root=Path(name).resolve()
            for optimized in (False,True):
                output=root/str(optimized)/'output'
                command=[sys.executable,'-B']+(['-O'] if optimized else [])
                child=subprocess.run(command+[exporter.__file__,'--reference',str(root),
                    '--weights',str(root/'missing'),'--output',str(output),'--metric-native-aspect'],
                    capture_output=True)
                self.assertNotEqual(child.returncode,0)
                self.assertEqual(child.stdout,b'')
                self.assertIn(b'native-aspect export requires the explicit metric profile',child.stderr)
                self.assertFalse(output.parent.exists())

    def test_metric_identity_is_separate_and_only_the_declared_profile_is_admitted(self):
        self.assertNotEqual(exporter.processing_identity(518), exporter.processing_identity(518, metric=True))
        self.assertEqual(exporter.processing_identity(518, metric=True), exporter.METRIC_PROCESSING)
        self.assertNotEqual(exporter.WEIGHTS_SHA256, exporter.METRIC_WEIGHTS_SHA256)
        for side in (True, 518., "518", 266, 392, 517, 519):
            with self.subTest(side=side), self.assertRaises(ValueError):
                exporter.processing_identity(side, metric=True)
        for flag in (1, None, "metric"):
            with self.subTest(flag=flag), self.assertRaises(ValueError):
                exporter.processing_identity(518, metric=flag)

    def test_unsupported_metric_resolution_refuses_before_io_or_reference_import(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name).resolve()
            output = root / "absent" / "export"
            with mock.patch.object(exporter.subprocess, "check_output") as git:
                for side in (266, 392, True, 518.):
                    with self.subTest(side=side), self.assertRaises(ValueError):
                        exporter.export(root / "reference", root / "weights", output, side, metric=True)
                git.assert_not_called()
            self.assertFalse(output.parent.exists())

    def test_real_cli_preserves_prior_output_and_rejects_unsupported_metric_profiles(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name).resolve()
            for optimized in (False, True):
                output = root / str(optimized)
                command = [sys.executable, "-B"] + (["-O"] if optimized else [])
                child = subprocess.run(command + [exporter.__file__, "--reference", str(root),
                    "--weights", str(root / "missing"), "--output", str(output),
                    "--side", "266", "--metric-hypersim"], capture_output=True)
                self.assertNotEqual(child.returncode, 0)
                self.assertEqual(child.stdout, b"")
                self.assertIn(b"metric export side must be 518", child.stderr)
                self.assertFalse(output.exists())
            marker = root / "model.onnx"
            marker.write_bytes(b"previous output")
            with self.assertRaises(FileExistsError):
                exporter.export(root, root / "missing", root, metric=True)
            self.assertEqual(marker.read_bytes(), b"previous output")

    def test_default_cli_records_start_and_failure_without_stdout_or_published_model(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name).resolve()
            output = root / "export"
            child = subprocess.run([sys.executable, "-B", exporter.__file__,
                "--reference", str(root / "missing-reference"), "--weights", str(root / "missing"),
                "--output", str(output), "--metric-hypersim"], capture_output=True)
            self.assertNotEqual(child.returncode, 0)
            self.assertEqual(child.stdout, b"")
            self.assertIn(b"operation=metric_export failed", child.stderr)
            evidence = (root / "export.log").read_text(encoding="utf-8")
            self.assertIn("INFO start operation=metric_export phase=verify_reference_and_weights", evidence)
            self.assertIn("ERROR operation=metric_export failed", evidence)
            self.assertFalse(output.exists())

    def test_secondary_log_persistence_failure_preserves_original_export_exception(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name).resolve()
            output = root / "export"
            control = """
import os, runpy, sys
from unittest.mock import patch
original = os.fsync
calls = 0
def persist(fd):
    global calls
    calls += 1
    if calls == 2:
        raise OSError('controlled secondary persistence failure')
    original(fd)
script = sys.argv.pop(1)
with patch.object(os, 'fsync', persist):
    runpy.run_path(script, run_name='__main__')
"""
            child = subprocess.run([sys.executable, "-B", "-c", control, exporter.__file__,
                "--reference", str(root / "missing-reference"), "--weights", str(root / "missing"),
                "--output", str(output), "--metric-hypersim"], capture_output=True)
            self.assertNotEqual(child.returncode, 0)
            self.assertEqual(child.stdout, b"")
            self.assertIn(b"metric export log persistence failed", child.stderr)
            self.assertIn(b"subprocess.CalledProcessError:", child.stderr.splitlines()[-1])
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
