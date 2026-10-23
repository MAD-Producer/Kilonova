"""Native-study protocol/failure guards; these fixtures are not device results."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

import collect_native_video_window as study


def profile_fixture(supported=True):
    frames = [dict(sequence=sequence, frame=source, gpuCompositionUs=None,
                   gpuPresentationUs=None, gpuTotalUs=None)
              for sequence, source in [(103, 0.), (107, 1.5), (108, 2.)]]
    samples = []
    if supported:
        for frame, total in zip((frames[0], frames[2]), (10., 30.)):
            frame.update(gpuCompositionUs=total*.5, gpuPresentationUs=total*.25, gpuTotalUs=total)
            samples.append(frame)
    def distribution(field):
        values = sorted(row[field] for row in samples)
        return dict(count=2, p50=values[0], p95=values[1], p99=values[1], max=values[1]) if samples else None
    return dict(frameCount=3, overflowFrames=0, frames=frames,
                gpuSampleCount=len(samples), gpuSampleCoverage=len(samples)/3,
                metadata=dict(graphics=dict(timestampReadbackBytesPerSample=32 if supported else 0,
                                            timingErrors=0 if supported else None)),
                **{key: distribution(key) for key in ('gpuCompositionUs', 'gpuPresentationUs', 'gpuTotalUs')})


ROWS = [dict(sourceFrame=source) for source in (0., 1.5, 2.)]


class NativeVideoWindowTests(unittest.TestCase):
    def test_primary_host_profile_keeps_exact_fractional_identity_over_a_rounded_copy(self):
        with tempfile.TemporaryDirectory(prefix='motion-native-primary-profile-') as directory:
            root = Path(directory)
            original = root/'project/exports/preview-performance.json'
            original.parent.mkdir(parents=True)
            profile = profile_fixture()
            frame = 14.451362999999999
            profile['frames'][1]['frame'] = frame
            original.write_text(json.dumps(profile), encoding='utf-8')
            rounded = copy.deepcopy(profile)
            rounded['frames'][1]['frame'] = 14.451363
            (root/'native-profile.json').write_text(json.dumps(rounded), encoding='utf-8')
            rows = copy.deepcopy(ROWS)
            rows[1]['sourceFrame'] = frame
            actual = study.read_original_profile(root)
            self.assertEqual(actual['frames'][1]['frame'], frame)
            self.assertEqual(study.verify_gpu_profile(actual, rows)['sampleCount'], 2)
            with self.assertRaisesRegex(ValueError, 'frame sequences differ'):
                study.verify_gpu_profile(rounded, rows)
            # A matching secondary copy can never mask a wrong primary frame.
            original.write_text(json.dumps(rounded), encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'frame sequences differ'):
                study.verify_gpu_profile(study.read_original_profile(root), rows)

    def test_gapped_sequences_keep_partial_supported_samples_and_unknown_rows(self):
        profile = profile_fixture()
        original = copy.deepcopy(profile)
        self.assertEqual(study.verify_gpu_profile(profile, ROWS),
                         dict(supported=True, sampleCount=2, coverage=2/3))
        self.assertEqual(profile, original)
        self.assertIsNone(profile['frames'][1]['gpuTotalUs'])

    def test_supported_but_unassociated_callbacks_cannot_pass_as_no_gpu(self):
        profile = profile_fixture(False)
        profile['metadata']['graphics'].update(timestampReadbackBytesPerSample=32, timingErrors=0)
        with self.assertRaisesRegex(ValueError, 'no associated samples'):
            study.verify_gpu_profile(profile, ROWS)

    def test_unsupported_gpu_stays_unknown_and_rejects_invented_samples(self):
        profile = profile_fixture(False)
        self.assertEqual(study.verify_gpu_profile(profile, ROWS),
                         dict(supported=False, sampleCount=0, coverage=0))
        profile = profile_fixture()
        profile['metadata']['graphics'].update(timestampReadbackBytesPerSample=0, timingErrors=None)
        with self.assertRaisesRegex(ValueError, 'not invented samples'):
            study.verify_gpu_profile(profile, ROWS)

    def test_wrong_summaries_counts_sequences_and_partial_callbacks_are_rejected(self):
        mutations = [
            lambda p: p.update(gpuSampleCount=3),
            lambda p: p.update(gpuSampleCoverage=1),
            lambda p: p.update(overflowFrames=1),
            lambda p: p['frames'][1].update(sequence=103),
            lambda p: p['frames'][0].update(frame=999),
            lambda p: p['frames'][1].update(gpuTotalUs=0),
            lambda p: p['frames'][0].update(gpuTotalUs=float('nan')),
            lambda p: p['gpuTotalUs'].update(p95=0),
            lambda p: p['metadata']['graphics'].update(timingErrors=1),
        ]
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                profile = profile_fixture()
                mutate(profile)
                with self.assertRaises(ValueError):
                    study.verify_gpu_profile(profile, ROWS)

    def test_real_failed_child_keeps_original_streams_and_exit_without_acceptance(self):
        with tempfile.TemporaryDirectory(prefix='motion-native-failure-') as directory:
            root = Path(directory)
            command = [sys.executable, '-c',
                       'import sys; print("original-native-stdout"); print("original-native-cause",file=sys.stderr); sys.exit(17)']
            with self.assertRaisesRegex(RuntimeError, 'failed; original stdout/stderr/receipts retained'):
                study.run_phase(root, 'native', command, os.environ.copy())
            receipt = study.read(root/'native.exit.json')
            self.assertEqual(receipt['exitCode'], 17)
            self.assertFalse(receipt['timedOut'])
            self.assertEqual(receipt['stdoutSha256'], study.sha(root/'native.stdout.log'))
            self.assertEqual(receipt['stderrSha256'], study.sha(root/'native.stderr.log'))
            self.assertIn('original-native-stdout', (root/'native.stdout.log').read_text())
            self.assertIn('original-native-cause', (root/'native.stderr.log').read_text())
            self.assertFalse((root/'accepted.json').exists())

    def test_real_nonreturning_child_is_bounded_and_receipted_without_acceptance(self):
        with tempfile.TemporaryDirectory(prefix='motion-native-timeout-') as directory:
            root = Path(directory)
            command = [sys.executable, '-c',
                       'import time; print("entered-nonreturning-child",flush=True); time.sleep(30)']
            with self.assertRaises(RuntimeError):
                study.run_phase(root, 'native', command, os.environ.copy(), timeout=2)
            receipt = json.loads((root/'native.exit.json').read_text())
            self.assertTrue(receipt['timedOut'])
            self.assertNotEqual(receipt['exitCode'], 0)
            self.assertEqual(receipt['stdoutSha256'], study.sha(root/'native.stdout.log'))
            self.assertEqual(receipt['stderrSha256'], study.sha(root/'native.stderr.log'))
            self.assertIn('entered-nonreturning-child', (root/'native.stdout.log').read_text())
            self.assertFalse((root/'accepted.json').exists())


if __name__ == '__main__':
    unittest.main()
