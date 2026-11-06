import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import depth_tensor


class TensorContract(unittest.TestCase):
    def test_manifest_rejects_invalid_requests_before_optional_imports(self):
        valid = dict(schema=1, stage="prepare", processing=depth_tensor.PROCESSING,
                     width=2, height=3, rgb_sha256="a"*64, depth_sha256=None)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "transform.json").write_text(json.dumps(valid))
            self.assertEqual(depth_tensor.manifest(root), valid)
            changes = [("schema", True), ("schema", 2), ("stage", "infer"),
                       ("width", True), ("height", 4097), ("rgb_sha256", "x"*64),
                       ("depth_sha256", "b"*64), ("processing", "different")]
            for field, value in changes:
                bad = copy.deepcopy(valid)
                bad[field] = value
                (root / "transform.json").write_text(json.dumps(bad))
                with self.assertRaises(ValueError, msg=field):
                    depth_tensor.manifest(root)

    def test_owned_bytes_check_the_same_read_and_refuse_size_or_digest_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input.rgb"
            path.write_bytes(b"rgb")
            digest = hashlib.sha256(b"rgb").hexdigest()
            self.assertEqual(depth_tensor.owned_bytes(path, 3, digest), b"rgb")
            with self.assertRaises(ValueError):
                depth_tensor.owned_bytes(path, 2, digest)
            with self.assertRaises(ValueError):
                depth_tensor.owned_bytes(path, 3, "a"*64)

    def test_actual_invalid_child_retains_a_bounded_protocol_cause(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "transform.json").write_text('{"schema":99}')
            for optimized in (False, True):
                command = [sys.executable, "-B"] + (["-O"] if optimized else [])
                child = subprocess.run(command + [str(Path(depth_tensor.__file__)), directory],
                                       capture_output=True, check=False)
                self.assertEqual(child.returncode, 1)
                response = json.loads(child.stdout)
                self.assertIn("unexpected or missing transform fields", response["error"])
                self.assertLess(len(child.stdout), 2048)


if __name__ == "__main__":
    unittest.main()
