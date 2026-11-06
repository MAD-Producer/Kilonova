"""Export admission tests; these do not qualify learned inference or quality."""
from pathlib import Path
import contextlib
import io
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import export_ai_depth as exporter


class DepthExportAdmission(unittest.TestCase):
    def test_resolution_is_part_of_processing_identity(self):
        identities = {exporter.processing_identity(n) for n in (266, 392, 518)}
        self.assertEqual(len(identities), 3)
        self.assertIn("dav2-small-rgb8-letterbox518-align-corners-relative-v1", identities)
        for n in (266, 392, 518):
            with self.subTest(side=n):
                self.assertIn(f"letterbox{n}-", exporter.processing_identity(n))

    def test_invalid_profile_is_refused_before_filesystem_or_model_imports(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            output = root / "not-created" / "export"
            with mock.patch.object(exporter.subprocess, "check_output") as git:
                for side in (True, False, 518.0, "518", 0, 1, 265, 267, 391, 393, 517, 519, 4096):
                    with self.subTest(side=side), self.assertRaisesRegex(ValueError, "export side"):
                        exporter.export(root / "missing-source", root / "missing-weights", output, side)
                    self.assertFalse(output.parent.exists())
                git.assert_not_called()

    def test_existing_destination_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            marker = root / "model.json"
            marker.write_bytes(b"original model")
            with mock.patch.object(exporter.subprocess, "check_output") as git:
                with self.assertRaises(FileExistsError):
                    exporter.export(root, root / "weights", root, 266)
                git.assert_not_called()
            self.assertEqual(marker.read_bytes(), b"original model")

    def test_command_line_requires_explicit_supported_side_without_coercion(self):
        command = ["--reference", "source", "--weights", "weights", "--output", "output"]
        self.assertEqual(exporter.arguments(command).side, 518)
        for n in (266, 392, 518):
            self.assertEqual(exporter.arguments(command + ["--side", str(n)]).side, n)
        for value in ("517", "518.0", "True", "-1"):
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
                exporter.arguments(command + ["--side", value])
            self.assertEqual(error.exception.code, 2)

    def test_optimized_real_cli_refuses_invalid_side_before_output_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            output = root / "new-output"
            for optimized in (False, True):
                command = [sys.executable, "-B"] + (["-O"] if optimized else [])
                child = subprocess.run(command + [exporter.__file__, "--reference", str(root),
                    "--weights", str(root / "missing"), "--output", str(output), "--side", "517"],
                    capture_output=True, check=False)
                self.assertEqual(child.returncode, 2)
                self.assertEqual(child.stdout, b"")
                self.assertIn(b"invalid choice", child.stderr)
                self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
