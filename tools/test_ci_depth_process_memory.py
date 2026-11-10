"""OS lifetime peaks in an actual owned child; no model or optional imports."""
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
from unittest import mock

import ai_depth_onnx as worker


class ProcessMemory(unittest.TestCase):
    def test_platform_units_and_integer_bounds_are_explicit(self):
        self.assertEqual(worker._resident_peak_bytes(12345, 'linux'), 12345 * 1024)
        self.assertEqual(worker._resident_peak_bytes(12345, 'darwin'), 12345)
        for value, platform in [(0, 'linux'), (-1, 'darwin'), (True, 'linux'),
                                (1.5, 'darwin'), (1 << 64, 'darwin'),
                                (1 << 54, 'linux'), (123, 'unknown')]:
            with self.subTest(value=value, platform=platform):
                with self.assertRaises(ValueError):
                    worker._resident_peak_bytes(value, platform)

    def test_query_failure_is_bounded_unknown_without_fabricated_zero(self):
        with mock.patch.object(worker, '_process_peak', side_effect=OSError('original\n' + 'x' * 1000)):
            result = worker.process_peak()
        self.assertEqual(set(result), {'status', 'pid', 'cause'})
        self.assertEqual(result['status'], 'unavailable')
        self.assertEqual(result['pid'], os.getpid())
        self.assertTrue(result['cause'].startswith('OSError: original '))
        self.assertEqual(len(result['cause']), 512)
        self.assertTrue(all(c.isprintable() for c in result['cause']))

    def test_real_child_peaks_survive_releasing_touched_memory(self):
        code = '''import gc, json, os, sys
sys.path.insert(0, sys.argv[1])
import ai_depth_onnx as worker
before = worker.process_peak()
block = bytearray(64 << 20)
for i in range(0, len(block), 4096): block[i] = 71
during = worker.process_peak()
del block
gc.collect()
after = worker.process_peak()
print(json.dumps(dict(pid=os.getpid(), before=before, during=during, after=after)))
'''
        methods = dict(win32='windows_process_memory_counters', linux='getrusage_linux',
                       darwin='getrusage_darwin')
        for flags in ([], ['-O']):
            with self.subTest(flags=flags):
                child = subprocess.run([sys.executable, '-B', *flags, '-c', code,
                                        str(Path(worker.__file__).parent)],
                                       capture_output=True, timeout=20, check=False)
                self.assertEqual(child.returncode, 0, child.stderr)
                self.assertEqual(child.stderr, b'')
                result = json.loads(child.stdout)
                self.assertNotEqual(result['pid'], os.getpid())
                rows = [result[key] for key in ('before', 'during', 'after')]
                for row in rows:
                    self.assertEqual(row['pid'], result['pid'])
                    if sys.platform in methods:
                        self.assertEqual(row['status'], 'observed', row)
                        self.assertEqual(row['method'], methods[sys.platform])
                    else:
                        self.assertEqual(row['status'], 'unavailable')
                if sys.platform in methods:
                    peaks = [row['peak_resident_bytes'] for row in rows]
                    self.assertGreater(peaks[0], 0)
                    self.assertGreaterEqual(peaks[1], 64 << 20)
                    self.assertGreaterEqual(peaks[1], peaks[0])
                    self.assertGreaterEqual(peaks[2], peaks[1])
                    if sys.platform == 'win32':
                        commits = [row['peak_private_commit_bytes'] for row in rows]
                        self.assertGreaterEqual(commits[1], 64 << 20)
                        self.assertGreaterEqual(commits[2], commits[1])
                    else:
                        self.assertTrue(all(row['peak_private_commit_bytes'] is None for row in rows))


if __name__ == '__main__':
    unittest.main()
