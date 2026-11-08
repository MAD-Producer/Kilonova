"""WGSL transform identity admission; fixtures do not qualify GPU inference."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

import depth_tensor as tensor
import ai_depth_onnx as shared


class TransformIdentity(unittest.TestCase):
    def modules(self, root):
        modules = []
        for name in ("numpy", "cv2"):
            package = root / name
            package.mkdir(parents=True)
            source = ("from types import SimpleNamespace\n"
                      "_core = SimpleNamespace(_multiarray_umath=SimpleNamespace(__cpu_features__={'owned': True}))\n"
                      if name == "numpy" else "def getBuildInformation(): return 'owned cv build'\n")
            (package / "__init__.py").write_text(source, encoding="utf-8")
            modules.append(SimpleNamespace(__file__=str(package / "__init__.py")))
        modules[0]._core = SimpleNamespace(_multiarray_umath=SimpleNamespace(__cpu_features__={"owned": True}))
        modules[1].getBuildInformation = lambda: "owned cv build"
        return modules

    def run_child(self, packages, args):
        # Even a machine with ORT installed must prove this child does not
        # import it. The synthetic packages isolate protocol/admission only.
        loader = """import importlib.abc, runpy, sys
class NoOrt(importlib.abc.MetaPathFinder):
    def find_spec(self, name, path=None, target=None):
        if name.split('.')[0] == 'onnxruntime':
            raise RuntimeError('unexpected ONNX Runtime dependency')
sys.meta_path.insert(0, NoOrt())
sys.path[:0] = [sys.argv[1], sys.argv[2]]
script = sys.argv[2] + '/depth_tensor.py'
sys.argv = [script] + sys.argv[3:]
runpy.run_path(script, run_name='__main__')
"""
        return subprocess.run([sys.executable, "-I", "-B", "-c", loader,
                               str(packages), str(Path(tensor.__file__).parent), *args],
                              capture_output=True, timeout=30, check=False)

    def test_runtime_tracks_only_actual_transform_packages_and_native_libraries(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            modules = self.modules(root)
            first = tensor.execution_environment(*modules)
            ort = root / "onnxruntime"
            ort.mkdir()
            (ort / "unused.pyd").write_bytes(b"not used by WGSL transforms")
            ort_libs = root / "onnxruntime.libs"
            ort_libs.mkdir()
            (ort_libs / "unused.so").write_bytes(b"also unused")
            self.assertEqual(first, tensor.execution_environment(*modules))
            libs = root / "numpy.libs"
            libs.mkdir()
            native = libs / "blas.so.1"
            native.write_bytes(b"owned numerical dependency")
            second = tensor.execution_environment(*modules)
            self.assertNotEqual(first["runtime_sha256"], second["runtime_sha256"])
            native.write_bytes(b"changed numerical dependency")
            self.assertNotEqual(second["runtime_sha256"], tensor.execution_environment(*modules)["runtime_sha256"])
            self.assertEqual(first["plan_sha256"], second["plan_sha256"])

    def test_runtime_covers_separately_installed_package_sites(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            np = self.modules(root / "first")[0]
            cv = self.modules(root / "second")[1]
            before = tensor.execution_environment(np, cv)
            libs = root / "second" / "opencv.libs"
            libs.mkdir()
            (libs / "image.so").write_bytes(b"owned separated CV dependency")
            self.assertNotEqual(before["runtime_sha256"], tensor.execution_environment(np, cv)["runtime_sha256"])

    def test_library_enumeration_order_does_not_change_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            modules = self.modules(root)
            for name in ("numpy.libs", "opencv.libs"):
                libs = root / name
                libs.mkdir()
                (libs / "control.dll").write_bytes(name.encode())
            entries = list(root.iterdir())
            before = tensor.execution_environment(*modules)
            with mock.patch.object(Path, "iterdir", return_value=iter(reversed(entries))):
                self.assertEqual(before, tensor.execution_environment(*modules))

    def test_cpu_features_and_numerical_environment_change_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            modules = self.modules(Path(directory))
            first = tensor.execution_environment(*modules)
            modules[0]._core._multiarray_umath.__cpu_features__["owned"] = False
            second = tensor.execution_environment(*modules)
            self.assertNotEqual(first["runtime_sha256"], second["runtime_sha256"])
            with mock.patch.dict(os.environ, {"OMP_NUM_THREADS": "4321"}):
                third = tensor.execution_environment(*modules)
            self.assertNotEqual(second["runtime_sha256"], third["runtime_sha256"])
            with mock.patch.dict(os.environ, {"OMP_NUM_THREADS": "x" * 1025}):
                with self.assertRaisesRegex(ValueError, "unbounded numerical environment"):
                    tensor.execution_environment(*modules)

    def test_both_embedded_sources_participate_in_transform_plan(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            modules = self.modules(root)
            common = root / "shared.py"
            transform = root / "transform.py"
            common.write_bytes(b"owned shared procedure")
            transform.write_bytes(b"owned transform procedure")
            with mock.patch.object(shared, "__file__", str(common)), mock.patch.object(tensor, "__file__", str(transform)):
                first = tensor.execution_environment(*modules)
                common.write_bytes(b"changed shared procedure")
                second = tensor.execution_environment(*modules)
                transform.write_bytes(b"changed transform procedure")
                third = tensor.execution_environment(*modules)
            self.assertEqual(first["runtime_sha256"], third["runtime_sha256"])
            self.assertNotEqual(first["plan_sha256"], second["plan_sha256"])
            self.assertNotEqual(second["plan_sha256"], third["plan_sha256"])

    def test_expected_identity_is_strict_and_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            modules = self.modules(root)
            valid = tensor.execution_environment(*modules)
            path = root / "transform-execution.json"
            path.write_text(json.dumps(valid), encoding="utf-8")
            self.assertEqual(valid, tensor.expected_execution(root))
            for field, value in (("runtime_sha256", [True] * 32), ("plan_sha256", [0] * 31),
                                 ("backend", "onnx"), ("precision", "float16"), ("unknown", 1)):
                bad = dict(valid)
                bad[field] = value
                path.write_text(json.dumps(bad), encoding="utf-8")
                with self.subTest(field=field), self.assertRaises(ValueError):
                    tensor.expected_execution(root)
            path.write_bytes(b" " * 65537)
            with self.assertRaisesRegex(ValueError, "64 KiB"):
                tensor.expected_execution(root)

    def test_numerical_build_context_is_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            modules = self.modules(Path(directory))
            modules[1].getBuildInformation = lambda: "x" * 65537
            with self.assertRaisesRegex(ValueError, "runtime context exceeds 64 KiB"):
                tensor.execution_environment(*modules)

    def test_real_child_resolves_without_input_model_or_ort_and_isolates_stdout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.modules(root)
            result = self.run_child(root, ["--resolve"])
            self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
            self.assertEqual(len(result.stdout.splitlines()), 1)
            report = json.loads(result.stdout)
            self.assertEqual(report["stage"], "resolve")
            self.assertEqual(report["encoded_operations"], 0)
            self.assertEqual(report["execution"]["backend"], "wgsl-transform")
            self.assertEqual(len(report["execution"]["runtime_sha256"]), 32)
            self.assertFalse((root / "tensor.f32").exists())
            self.assertFalse((root / "output.f32").exists())

    def test_changed_runtime_is_refused_before_reading_rgb_or_writing_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.modules(root)
            resolved = self.run_child(root, ["--resolve"])
            self.assertEqual(resolved.returncode, 0, resolved.stderr.decode(errors="replace"))
            execution = json.loads(resolved.stdout)["execution"]
            (root / "transform-execution.json").write_text(json.dumps(execution), encoding="utf-8")
            (root / "transform.json").write_text(json.dumps(dict(schema=1, stage="prepare",
                processing=tensor.PROCESSING, width=2, height=3,
                rgb_sha256=hashlib.sha256(b"owned rgb").hexdigest(), depth_sha256=None)), encoding="utf-8")
            (root / "numpy" / "changed.dll").write_bytes(b"changed after resolution")
            result = self.run_child(root, ["--attest", str(root)])
            self.assertEqual(result.returncode, 1)
            self.assertEqual(len(result.stdout.splitlines()), 1)
            self.assertIn("execution changed since cache resolution", json.loads(result.stdout)["error"])
            self.assertFalse((root / "input.rgb").exists())
            self.assertFalse((root / "tensor.f32").exists())


if __name__ == "__main__":
    unittest.main()
