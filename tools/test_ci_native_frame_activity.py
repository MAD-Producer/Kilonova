"""Counter protocol guards, not simulated GPU or performance acceptance."""
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import native_frame_activity as activity


def observation(sequence, value):
    row = dict(sourceFrame=sequence / 2, presentPolls=2,
               stateObservationUs=11, resourceObservationUs=13,
               resourceMemory=dict(textures=dict(submissionTracking=dict(
                   preview=dict(submitted=value, enabled=True)))),
               preview=dict(video=dict(gpuConversions=value, uploads=value,
                                       uploadBytes=value * 100),
                            effectPipelineBuilds=0, imageUploadBytes=0))
    frame = dict(sequence=sequence, frame=sequence / 2, cpuPrepareUs=17,
                 gpuTotalUs=None, resources=dict(vectorUpload=dict(
                     encodedUploadBytesTotal=value * 10, completedUploadBytesTotal=value * 8)))
    return row, frame


class NativeFrameActivityTests(unittest.TestCase):
    def test_gaps_keep_exact_deltas_and_first_unknown_without_mutating_evidence(self):
        pairs = [observation(sequence, value) for sequence, value in [(3, 8), (11, 13), (15, 13)]]
        raw = dict(cold=pairs[0][0], warmup=[pairs[1][0]], measured=[pairs[2][0]])
        frames = [pair[1] for pair in pairs]
        original = copy.deepcopy((raw, frames))
        rows = activity.activity_rows(raw, frames)
        self.assertEqual((raw, frames), original)
        self.assertIsNone(rows[0]['interval']['instrumentedSubmissions'])
        self.assertEqual(rows[1]['interval']['instrumentedSubmissions'], 5)
        self.assertEqual(rows[1]['interval']['videoUploadBytes'], 500)
        self.assertEqual(rows[1]['interval']['vectorEncodedUploadBytes'], 50)
        self.assertEqual(rows[2]['interval']['videoGpuConversions'], 0)
        self.assertEqual(rows[2]['interval']['effectPipelineBuilds'], 0)
        self.assertEqual(rows[1]['previousSequence'], 3)
        self.assertIsNone(rows[1]['interval']['videoBlendSubmissions'])
        self.assertIsNone(rows[1]['imageReadbackBytes'])
        self.assertIsNone(rows[1]['associatedTimingPayloadBytes'])

    def test_missing_and_decreased_counters_never_become_zero_then_recover(self):
        previous = dict(videoUploads=10)
        delta, status = activity.interval(previous, dict(videoUploads=2))
        self.assertIsNone(delta['videoUploads'])
        self.assertEqual(status['videoUploads'], 'counter_decreased')
        delta, _ = activity.interval(dict(videoUploads=2), dict(videoUploads=5))
        self.assertEqual(delta['videoUploads'], 3)
        delta, status = activity.interval(dict(videoUploads=5), dict(videoUploads=None))
        self.assertIsNone(delta['videoUploads'])
        self.assertEqual(status['videoUploads'], 'current_unknown')
        delta, status = activity.interval(dict(videoUploads=None), dict(videoUploads=12))
        self.assertIsNone(delta['videoUploads'])
        self.assertEqual(status['videoUploads'], 'baseline_unknown')

    def test_disabled_tracking_is_unknown_even_when_a_stale_count_is_present(self):
        row, frame = observation(3, 8)
        row['resourceMemory']['textures']['submissionTracking']['preview']['enabled'] = False
        self.assertIsNone(activity.counters(row, frame)['instrumentedSubmissions'])
        self.assertEqual(activity.counters(row, frame)['videoUploads'], 8)

    def test_bad_counter_types_and_wrong_frame_identity_are_rejected(self):
        for value in (True, -1, 1.5, '2'):
            with self.subTest(value=value):
                row, frame = observation(3, 8)
                row['preview']['video']['uploads'] = value
                with self.assertRaisesRegex(ValueError, 'Invalid cumulative counter'):
                    activity.counters(row, frame)
        pairs = [observation(3, 8), observation(11, 13)]
        raw = dict(cold=pairs[0][0], warmup=[], measured=[pairs[1][0]])
        frames = [pair[1] for pair in pairs]
        for mutation in ('frame', 'sequence'):
            changed = copy.deepcopy(frames)
            changed[1][mutation] = changed[0][mutation]
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, 'identity'):
                activity.activity_rows(raw, changed)
        with self.assertRaisesRegex(ValueError, 'coverage'):
            activity.activity_rows(raw, frames[:1])

    def test_summary_preserves_unknown_and_reset_counts_and_valid_zero(self):
        pairs = [observation(sequence, value) for sequence, value in [(3, 8), (11, 2), (15, 2)]]
        raw = dict(cold=pairs[0][0], warmup=[], measured=[pair[0] for pair in pairs[1:]])
        rows = activity.activity_rows(raw, [pair[1] for pair in pairs])
        summary = activity.summarize(rows[1:])
        self.assertEqual(summary['videoUploads'], dict(
            distribution=dict(count=1, p50=0, p95=0, p99=0, max=0),
            unknownCount=1, decreasedCount=1))
        self.assertIsNone(summary['videoBlendSubmissions']['distribution'])
        self.assertEqual(summary['videoBlendSubmissions']['unknownCount'], 2)

    def test_real_failed_child_receipt_blocks_analysis_and_creates_no_output(self):
        with tempfile.TemporaryDirectory(prefix='motion-activity-failure-') as directory:
            root = Path(directory)
            (root/'provenance.json').write_text(json.dumps(dict(sourceRevision='protocol-failure-fixture')))
            with self.assertRaises(RuntimeError):
                activity.native.run_phase(root, 'build',
                    [sys.executable, '-c', 'import sys; print("real-child-failure",flush=True); sys.exit(17)'],
                    os.environ.copy())
            output = root/'activity.json'
            child = subprocess.run([sys.executable, str(Path(activity.__file__)),
                '--study', str(root), '--output', str(output)], capture_output=True, text=True)
            self.assertNotEqual(child.returncode, 0)
            self.assertIn('Actual build did not complete successfully', child.stderr)
            self.assertFalse(output.exists())
            self.assertIn('real-child-failure', (root/'build.stdout.log').read_text())
            self.assertEqual(activity.native.read(root/'build.exit.json')['exitCode'], 17)


if __name__ == '__main__':
    unittest.main()
