"""Pressure evidence protocol guards; no fixture here is device acceptance."""
import copy
import unittest

import collect_native_video_pressure as pressure
from test_ci_native_video_window import profile_fixture


def phase_fixture(name='normal', count=3, level=0, initial_level=None):
    profile = profile_fixture(False)
    base = profile['frames'][0]
    profile['frames'] = [dict(base, frame=i, sequence=100+i*2) for i in range(count)]
    profile.update(frameCount=count, gpuSampleCoverage=0)
    rows = []
    budget = pressure.LOW if name == 'limited' else pressure.HIGH
    for i in range(count):
        actual_level = initial_level if initial_level is not None and i < count-1 else level
        preview = dict(mode='high', sourceTextureBudgetBytes=budget, video=dict(streams=4),
                       videoResolution='decoded_source' if actual_level == 0 else f'proxy_max_{2048 >> actual_level}',
                       memoryAdaptation=dict(actual=dict(level=actual_level), held=False,
                           status='normal' if actual_level == 0 else 'degraded', refusalCount=int(actual_level > 0),
                           actualWidth=1280 >> actual_level, actualHeight=720 >> actual_level, hasLastValidFrame=True))
        rows.append(dict(sourceFrame=i, lastPresentedFrame=i, renderError=None, preview=preview,
                         presentPolls=1, **{metric:10+i for metric in pressure.METRICS}))
        profile['frames'][i].update(cpuPrepareUs=10)
    report = dict(name=name, sourceBudgetBytes=budget, completedFrames=count, lastPreview=rows[-1]['preview'],
                  **{metric:dict(count=count, p50=10+(count-1)//2, p95=10+count-1,
                                 p99=10+count-1, max=10+count-1) for metric in pressure.METRICS})
    if name.startswith('recovery'):
        report.update(successfulWarmFrames=120, probeAfterWait=count == 121,
                      cooldownBeforeWaitMs=0, cooldownWaitUs=0)
        for metric in pressure.METRICS:
            values = [row[metric] for row in rows]
            report[metric] = pressure.activity.distribution(values)
    return report, rows, profile


class NativeVideoPressureTests(unittest.TestCase):
    def test_actual_quality_is_explicit_and_unknown_gpu_and_counters_stay_unknown(self):
        for name, level in [('normal', 0), ('limited', 1)]:
            with self.subTest(name=name):
                report, rows, profile = phase_fixture(name, level=level)
                result = pressure.verify_phase(report, rows, profile, 0, level)
                self.assertFalse(result['rootGpuTimestampCoverage']['supported'])
                self.assertTrue(all(value is None for value in result['intervals'][0]['interval'].values()))

    def test_wrong_budget_front_quality_streams_and_source_identity_are_rejected(self):
        mutations = [
            lambda r: r['preview'].update(sourceTextureBudgetBytes=pressure.HIGH),
            lambda r: r['preview']['memoryAdaptation'].update(held=True),
            lambda r: r['preview']['memoryAdaptation'].update(hasLastValidFrame=False),
            lambda r: r['preview']['memoryAdaptation'].update(actualWidth=1280),
            lambda r: r['preview']['memoryAdaptation'].update(status='normal'),
            lambda r: r['preview']['memoryAdaptation'].update(refusalCount=0),
            lambda r: r['preview']['video'].update(streams=3),
            lambda r: r.update(renderError='original render failure'),
            lambda r: r.update(lastPresentedFrame=99),
            lambda r: r.update(sourceFrame=99),
        ]
        for mutation in mutations:
            report, rows, profile = phase_fixture('limited', level=1)
            mutation(rows[0])
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                pressure.verify_phase(report, rows, profile, 0, 1)

    def test_recovery_requires_real_success_records_cooldown_and_one_quality_step(self):
        report, rows, profile = phase_fixture('recovery-1', count=121, initial_level=1)
        report.update(cooldownBeforeWaitMs=1234, cooldownWaitUs=1234000)
        result = pressure.verify_phase(report, rows, profile, 1, 0)
        self.assertEqual(result['completedFrames'], 121)
        mutations = [
            lambda p: p.update(successfulWarmFrames=0),
            lambda p: p.update(probeAfterWait=False),
            lambda p: p.update(cooldownWaitUs=0),
            lambda p: p.update(completedFrames=120),
        ]
        for mutation in mutations:
            modified = copy.deepcopy(report)
            mutation(modified)
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                pressure.verify_phase(modified, rows, profile, 1, 0)
        with self.assertRaisesRegex(ValueError, 'one production quality step'):
            pressure.verify_phase(report, rows, profile, 2, 0)
        report, rows, profile = phase_fixture('recovery-1', count=121)
        with self.assertRaisesRegex(ValueError, 'one production quality step'):
            pressure.verify_phase(report, rows, profile, 1, 0)

    def test_corrupt_timings_missing_gpu_callbacks_and_front_mismatch_are_not_acceptance(self):
        report, rows, profile = phase_fixture()
        rows[0]['cycleWithSeekUs'] = None
        with self.assertRaises(ValueError):
            pressure.verify_phase(report, rows, profile, 0, 0)
        report, rows, profile = phase_fixture()
        profile['metadata']['graphics'].update(timestampReadbackBytesPerSample=32, timingErrors=0)
        with self.assertRaisesRegex(ValueError, 'no associated samples'):
            pressure.verify_phase(report, rows, profile, 0, 0)
        for proof in [{}, dict(allBytesEqual=False, actualReadbackBytes=100),
                      dict(allBytesEqual=True, actualReadbackBytes=0)]:
            with self.assertRaises(ValueError):
                pressure.verify_preserved_front(proof)


if __name__ == '__main__':
    unittest.main()
