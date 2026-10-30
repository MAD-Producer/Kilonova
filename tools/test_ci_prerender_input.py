"""Parser controls, not substitute native performance evidence."""
import copy
import unittest

from analyze_prerender_input import assess


def fixture():
    runs = []
    for scenario in ("property_drag", "timeline_scrub"):
        for episode, enabled in enumerate((False, True, True, False)):
            samples = []
            for number in range(60):
                samples.append(dict(sample=number, verified=True, foregroundUs=80 if enabled else 100,
                                    pressUs=10, changeUs=30, commitUs=50,
                                    presentUs=30 if enabled else 50, presentPolls=1, actions=[1, 1, 1],
                                    workerTiming=[dict(request=number + 1, wallUs=20, queueWaitUs=5,
                                                       workUs=15, operation="present")],
                                    gpu=dict(name="synthetic parser fixture", backend="fixture"),
                                    prerender=dict(enabled=enabled, fault=None),
                                    warm=dict(enabled=enabled, fault=None, hold="idle", cache=dict(resident_frames=3)),
                                    before=dict(stopped=False, lastFailure=None), revision=number,
                                    result=dict(frame=0 if scenario == "property_drag" else number + 1,
                                                position=[250 if number % 2 == 0 else 240, 180, 0],
                                                project={}, pixels=dict(bytes=640 * 360 * 4, sha256="a" * 64, gestureActive=False))))
            runs.append(dict(scenario=scenario, episode=episode, enabled=enabled, samples=samples))
    return dict(schemaVersion=1, samplesPerEpisode=60, order=[False, True, True, False],
                window=[1280, 800], composition=[640, 360], profile="High authored single-time",
                partialSamples=[], runs=runs, limits="synthetic parser fixture")


class EvidenceTests(unittest.TestCase):
    def test_complete_abba_data_retains_every_pair(self):
        result = assess(fixture())
        self.assertTrue(result["noRegression"])
        self.assertEqual(len(result["scenarios"]), 2)
        self.assertEqual(result["scenarios"][0]["pairs"][1]["ratio"], .8)

    def test_one_pair_regression_is_not_hidden_by_pooled_improvement(self):
        data = fixture()
        for sample in data["runs"][2]["samples"]:
            sample.update(foregroundUs=101, presentUs=51)
        result = assess(data)
        self.assertFalse(result["noRegression"])
        self.assertEqual(result["scenarios"][0]["pairs"][1]["ratio"], 1.01)

    def test_incomplete_native_and_invalid_cost_evidence_are_rejected(self):
        source = fixture()
        mutations = [
            lambda d: d["runs"].pop(),
            lambda d: d.update(partialSamples=[{}]),
            lambda d: d["runs"][0]["samples"].pop(),
            lambda d: d["runs"][1]["samples"][0]["warm"]["cache"].update(resident_frames=0),
            lambda d: d["runs"][1]["samples"][0]["warm"].update(hold="unsupported_profile"),
            lambda d: d["runs"][1]["samples"][0].update(before=None),
            lambda d: d["runs"][1]["samples"][0].update(workerTiming=[]),
            lambda d: d["runs"][1]["samples"][0].update(gpu=None),
            lambda d: d["runs"][1]["samples"][0].update(verified=False),
            lambda d: d["runs"][1]["samples"][0].update(foregroundUs=True),
            lambda d: d["runs"][1]["samples"][0].update(commitUs=90),
            lambda d: d["runs"][1]["samples"][0].update(presentUs=0),
            lambda d: d["runs"][1]["samples"][0]["result"]["pixels"].update(sha256="b" * 64),
            lambda d: d["runs"][1]["samples"][0]["result"].update(position=[0, 0, 0]),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index):
                data = copy.deepcopy(source)
                mutate(data)
                with self.assertRaises(ValueError):
                    assess(data)


if __name__ == "__main__":
    unittest.main()
