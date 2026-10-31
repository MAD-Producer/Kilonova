"""Launcher IO controls only; these never mount or qualify native ENOSPC."""
import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

import validate_prerender_full_volume as launcher


class NativeFailureEvidence(unittest.TestCase):
    def run_child(self, code):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        out = Path(temporary.name)
        report = {}
        checkpoints = []

        def save():
            checkpoints.append(json.loads(json.dumps(report)))

        errors = io.StringIO()
        with contextlib.redirect_stderr(errors):
            with self.assertRaisesRegex(ValueError, 'native test failed'):
                launcher.run_native([sys.executable, '-c', code], os.environ.copy(), out, report, save)
        return out, report, checkpoints, errors.getvalue()

    def test_real_child_failure_exposes_both_causes_before_cleanup(self):
        out, report, checkpoints, errors = self.run_child(
            "import sys;print('native stdout cause');print('native stderr cause',file=sys.stderr);sys.exit(101)"
        )
        self.assertEqual(report['exitCode'], 101)
        self.assertEqual((out / 'stdout.log').read_text().strip(), 'native stdout cause')
        self.assertEqual((out / 'stderr.log').read_text().strip(), 'native stderr cause')
        self.assertTrue(any(row.get('exitCode') == 101 for row in checkpoints))
        self.assertIn('native stdout cause', errors)
        self.assertIn('native stderr cause', errors)

    def test_summary_words_are_not_an_executed_test_summary(self):
        out, report, checkpoints, errors = self.run_child(
            "print('a diagnostic says 1 passed; 0 failed; 0 ignored')"
        )
        self.assertEqual(report['exitCode'], 0)
        self.assertEqual(report['counts'], [])
        self.assertIn('a diagnostic says', errors)

    def test_failure_excerpt_is_bounded_and_original_files_are_complete(self):
        out, report, checkpoints, errors = self.run_child(
            "import sys;print('HEAD_NATIVE_CAUSE'+'x'*100000+'TAIL_NATIVE_CAUSE');sys.exit(101)"
        )
        self.assertGreater((out / 'stdout.log').stat().st_size, 100000)
        self.assertIn('HEAD_NATIVE_CAUSE', errors)
        self.assertIn('TAIL_NATIVE_CAUSE', errors)
        self.assertLess(len(errors.encode('utf-8')), 20000)


if __name__ == '__main__':
    unittest.main()
