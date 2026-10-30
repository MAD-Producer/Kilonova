"""Evidence parser controls; synthetic costs are never product performance evidence."""
import unittest
from analyze_prerender_paced import assess


def fixture():
    runs = []
    for episode, enabled in enumerate([False, True, True, False]):
        frames = [dict(frame=frame, foreground_us=500 if enabled else 1000,
                       cachedPresented=frame if enabled else 0,
                       presentPolls=1, missedNextDeadline=False) for frame in range(1, 61)]
        runs.append(dict(episode=episode, enabled=enabled, frames=frames,
                         cachedPresented=60 if enabled else 0, p95_us=500 if enabled else 1000))
    return dict(fps=30, framesPerEpisode=60, order=['disabled', 'enabled', 'enabled', 'disabled'],
                partialFrames=[], runs=runs)


class PairedPlaybackEvidenceTests(unittest.TestCase):
    def test_pairs_and_pooled_costs_require_no_regression(self):
        report = fixture()
        self.assertTrue(assess(report)['noRegression'])
        for frame in report['runs'][2]['frames']:
            frame['foreground_us'] = 1001
        report['runs'][2]['p95_us'] = 1001
        result = assess(report)
        self.assertFalse(result['noRegression'])
        self.assertEqual(result['pairs'][1]['ratio'], 1.001)

    def test_p95_uses_raw_nearest_rank_and_preserves_slow_tail(self):
        report = fixture()
        run = report['runs'][1]
        for frame in run['frames'][-4:]:
            frame['foreground_us'] = 2000
        run['p95_us'] = 2000  # rank 57/60 enters the four-sample slow tail
        result = assess(report)
        self.assertEqual(result['runs'][1]['p95Us'], 2000)
        self.assertFalse(result['noRegression'])

    def test_incomplete_or_inconsistent_reports_are_not_accepted(self):
        mutations = [
            lambda r: r['runs'].pop(),
            lambda r: r['partialFrames'].append({'frame': 1}),
            lambda r: r['runs'][1]['frames'].pop(),
            lambda r: r['runs'][1]['frames'][0].update(frame=2),
            lambda r: r['runs'][1].update(p95_us=1),
            lambda r: r['runs'][1]['frames'][0].update(foreground_us=float('nan')),
            lambda r: r['runs'][1]['frames'][0].update(foreground_us=True),
            lambda r: r['runs'][1]['frames'][0].update(cachedPresented=2),
            lambda r: r['runs'][1]['frames'][0].update(presentPolls=0),
            lambda r: r['runs'][1]['frames'][0].update(missedNextDeadline=None),
        ]
        for mutate in mutations:
            report = fixture()
            mutate(report)
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                assess(report)

    def test_warmup_or_invented_hits_cannot_replace_presentations(self):
        report = fixture()
        for frame in report['runs'][1]['frames']:
            frame['cachedPresented'] = min(frame['frame'], 29)
        report['runs'][1]['cachedPresented'] = 29
        with self.assertRaisesRegex(ValueError, 'insufficient actual hits'):
            assess(report)
        report = fixture()
        report['runs'][0]['frames'][-1]['cachedPresented'] = 1
        report['runs'][0]['cachedPresented'] = 1
        with self.assertRaisesRegex(ValueError, 'disabled cache produced hits'):
            assess(report)


if __name__ == '__main__':
    unittest.main()
