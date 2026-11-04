"""Exercise real CLI rejection before optional CV imports, including python -O."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / 'tools/tracking/calibrate_photographs.py'


class CalibrationSourceAdmissionTest(unittest.TestCase):
    def test_changed_original_bytes_fail_and_retain_report_under_optimization(self):
        for optimized in (False, True):
            with self.subTest(optimized=optimized), tempfile.TemporaryDirectory() as directory:
                source = Path(directory) / 'source'
                source.mkdir()
                # First manifest entry is the upstream license. A file of the
                # expected length with different bytes must fail before CV loads.
                manifest = json.loads((ROOT / 'engineering/validation/data/tracking88-photographs-sources.json').read_text(encoding='utf-8'))
                first = manifest['files'][0]
                (source / first['name']).write_bytes(b'x' * first['bytes'])
                output = Path(directory) / 'evidence'
                args = [sys.executable] + (['-O'] if optimized else [])
                result = subprocess.run(args + [str(TOOL), '--source', str(source), '--output', str(output)], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                report = json.loads((output / 'calibration.json').read_text(encoding='utf-8'))
                self.assertEqual(report['status'], 'failed')
                self.assertIn('Source identity differs: '+first['name'], report['failure'])
                self.assertEqual(report['views'], [])
                self.assertNotIn('ModuleNotFoundError', result.stderr)

    def test_missing_original_file_fails_without_downloading_or_creating_acceptance(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'source'
            output = Path(directory) / 'evidence'
            result = subprocess.run([sys.executable, '-O', str(TOOL), '--source', str(source), '--output', str(output)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            report = json.loads((output / 'calibration.json').read_text(encoding='utf-8'))
            self.assertEqual(report['status'], 'failed')
            self.assertIn('FileNotFoundError', report['failure'])
            self.assertEqual(list(source.iterdir()), [])


if __name__ == '__main__':
    unittest.main()
