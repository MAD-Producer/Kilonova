"""Export admission tests; these do not qualify learned inference or quality."""
from pathlib import Path
import contextlib
import io
import json
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import export_ai_depth as exporter
import ai_depth_onnx as worker


class DepthExportAdmission(unittest.TestCase):
    def test_letterbox_geometry_preserves_crop_orientation_and_odd_padding(self):
        cases = [
            (400, 200, 266, (0, 66, 266, 133)),
            (200, 400, 266, (66, 0, 133, 266)),
            (400, 200, 392, (0, 98, 392, 196)),
            (400, 200, 518, (0, 129, 518, 259)),
            (3, 2, 392, (0, 65, 392, 261)),
            (4096, 1, 266, (0, 132, 266, 1)),
            (1, 4096, 518, (258, 0, 1, 518)),
            (1, 1, 392, (0, 0, 392, 392)),
        ]
        for width, height, side, expected in cases:
            with self.subTest(width=width, height=height, side=side):
                self.assertEqual(worker.letterbox_rectangle(width, height, side), expected)
        for invalid in (True, 0, -1, 4097, 12.0):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                worker.letterbox_rectangle(invalid, 12, 266)

    def test_host_worker_and_exporter_agree_on_all_processing_profiles(self):
        for side in (266, 392, 518):
            self.assertEqual(worker.processing_side(exporter.processing_identity(side)), side)
        for identity in (None, True, 266, "", worker.PROCESSING + " ",
                         worker.PROCESSING.replace("518", "517"),
                         worker.PROCESSING.replace("small", "base")):
            with self.subTest(identity=identity), self.assertRaises(ValueError):
                worker.processing_side(identity)

    def test_preprocessing_rejects_unsupported_shape_before_array_or_cv_calls(self):
        for side in (True, 518.0, 517, 0, 4096):
            with self.subTest(side=side), self.assertRaisesRegex(ValueError, "resolution"):
                worker.preprocess(None, 12, 8, None, None, side)

    def test_real_worker_rejects_unknown_processing_before_loading_a_model(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            request = dict(schema=1, model=str(root / "missing.onnx"), sha256="a" * 64,
                           processing=worker.PROCESSING.replace("518", "517"),
                           width=12, height=8, provider="CPUExecutionProvider")
            for optimized in (False, True):
                (root / "request.json").write_text(json.dumps(request), encoding="utf-8")
                command = [sys.executable, "-B"] + (["-O"] if optimized else [])
                child = subprocess.run(command + [worker.__file__, str(root)],
                                       capture_output=True, check=False)
                self.assertEqual(child.returncode, 1)
                error = json.loads(child.stdout)["error"]
                self.assertIn("phase=manifest", error)
                self.assertIn("unsupported depth inference procedure", error)
                self.assertFalse((root / "output.f32").exists())

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
