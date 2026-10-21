import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import ci_dependency_audit as audit


class DependencyAuditTest(unittest.TestCase):
    def invoke(self, report, status):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lock = root / "Cargo.lock"
            lock.write_text("version = 4\n", encoding="utf-8")
            output = root / "report"
            version = subprocess.CompletedProcess([], 0, f"cargo-audit {audit.VERSION}\n".encode(), b"")
            result = subprocess.CompletedProcess([], status, report, b"original audit stderr\n")
            with patch.object(audit.subprocess, "run", side_effect=[version, result]), patch.object(
                audit.subprocess, "check_output", return_value="a" * 40
            ), contextlib.redirect_stderr(io.StringIO()):
                code = audit.run(root / "cargo-audit", lock, root / "db", output)
            self.assertEqual((output / "audit.json").read_bytes(), report)
            self.assertEqual((output / "audit.stderr.log").read_bytes(), b"original audit stderr\n")
            return code, json.loads((output / "receipt.json").read_text())

    def test_success_does_not_override_native_failure_status(self):
        report = json.dumps({"vulnerabilities": {"found": False, "count": 0, "list": []}}).encode()
        code, receipt = self.invoke(report, 7)
        self.assertEqual(code, 7)
        self.assertEqual(receipt["exitCode"], 7)

    def test_vulnerabilities_cannot_pass_with_a_zero_native_status(self):
        report = json.dumps({"vulnerabilities": {"found": True, "count": 1, "list": [{"id": "control"}]}}).encode()
        code, receipt = self.invoke(report, 0)
        self.assertEqual(code, 1)
        self.assertEqual(receipt["vulnerabilities"], 1)

    def test_empty_malformed_or_inconsistent_reports_fail(self):
        for report in (b"", b"not JSON", b"{}", b'{"vulnerabilities":{"found":false,"count":1,"list":[]}}'):
            with self.subTest(report=report), self.assertRaises((ValueError, RuntimeError)):
                self.invoke(report, 1)

    def test_unverified_archive_is_never_extracted_or_written(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(audit.platform, "system", return_value="Windows"), patch.object(
            audit.platform, "machine", return_value="AMD64"
        ), patch.object(audit.urllib.request, "urlopen", return_value=io.BytesIO(b"invalid archive")), contextlib.redirect_stderr(io.StringIO()):
            output = Path(directory) / "bin"
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                audit.install(output)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
