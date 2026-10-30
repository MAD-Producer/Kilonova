"""Evaluate native injected-input evidence; test success is not P95 acceptance."""
import argparse
import json
import math
from pathlib import Path
import re


def require(condition, cause):
    if not condition:
        raise ValueError(cause)


def integer(value, minimum=0):
    return type(value) is int and value >= minimum


def p95(samples):
    costs = sorted(s["foregroundUs"] for s in samples)
    return costs[math.ceil(len(costs) * .95) - 1]


def assess(data):
    require(data.get("schemaVersion") == 1, "unsupported evidence schema")
    require(data.get("samplesPerEpisode") == 60, "requires 60 samples per episode")
    require(data.get("order") == [False, True, True, False], "requires fixed ABBA order")
    require(data.get("window") == [1280, 800] and data.get("composition") == [640, 360],
            "unexpected native window or composition")
    require(data.get("profile") == "High authored single-time", "unexpected output profile")
    require(data.get("partialSamples") == [], "incomplete episode retained")
    runs = data.get("runs")
    require(isinstance(runs, list) and len(runs) == 8, "requires both four-episode scenarios")
    results = []
    for scenario_index, scenario in enumerate(("property_drag", "timeline_scrub")):
        episodes = runs[scenario_index * 4:scenario_index * 4 + 4]
        reference = None
        for episode, (run, enabled) in enumerate(zip(episodes, (False, True, True, False))):
            require(run.get("scenario") == scenario and run.get("episode") == episode
                    and run.get("enabled") is enabled, "scenario/episode/order mismatch")
            samples = run.get("samples")
            require(isinstance(samples, list) and len(samples) == 60, "missing input samples")
            outcomes = []
            previous_frame = -1
            for number, sample in enumerate(samples):
                require(sample.get("sample") == number, "sample index gap")
                require(sample.get("verified") is True, "input/result/history verification incomplete")
                phases = [sample.get(key) for key in ("pressUs", "changeUs", "commitUs", "foregroundUs")]
                require(all(integer(v) for v in phases) and 0 <= phases[0] <= phases[1] <= phases[2] < phases[3],
                        "invalid or unordered phase costs")
                require(integer(sample.get("presentUs"), 1)
                        and abs(sample["presentUs"] - (phases[3] - phases[2])) <= 1,
                        "native presentation cost missing or inconsistent")
                require(integer(sample.get("presentPolls"), 1), "native Present not recorded")
                timing = sample.get("workerTiming")
                require(isinstance(timing, list) and timing
                        and all(isinstance(t, dict) and integer(t.get("request"), 1)
                                and integer(t.get("wallUs")) and integer(t.get("queueWaitUs"))
                                and integer(t.get("workUs")) for t in timing)
                        and any(t.get("operation") == "present" for t in timing),
                        "worker request/queue/work/present evidence absent")
                gpu = sample.get("gpu")
                require(isinstance(gpu, dict) and isinstance(gpu.get("backend"), str)
                        and isinstance(gpu.get("name"), str) and gpu["backend"] and gpu["name"],
                        "native adapter identity absent")
                actions = sample.get("actions")
                require(isinstance(actions, list) and len(actions) == 3
                        and all(integer(v) for v in actions), "invalid action counts")
                require(actions[1] > 0 and actions[2 if scenario == "property_drag" else 0] > 0,
                        "control input did not generate required actions")
                status = sample.get("prerender", {})
                require(status.get("fault") is None and status.get("enabled") is enabled,
                        "prerender fault or policy mismatch")
                warm = sample.get("warm", {})
                require(warm.get("enabled") is enabled and warm.get("fault") is None,
                        "invalid before-input policy")
                if enabled:
                    require(integer(warm.get("cache", {}).get("resident_frames"), 1),
                            "enabled sample never had genuine warm native work")
                    require(warm.get("hold") not in ("disabled", "no_surface", "unsupported_profile", "fault"),
                            "enabled sample held outside supported profile")
                    observation = sample.get("before")
                    require(isinstance(observation, dict) and observation.get("stopped") is False
                            and observation.get("lastFailure") is None,
                            "native owner observation absent or failed")
                outcome = sample.get("result", {})
                pixels = outcome.get("pixels", {})
                require(pixels.get("bytes") == 640 * 360 * 4 and pixels.get("gestureActive") is False
                        and re.fullmatch(r"[0-9a-f]{64}", str(pixels.get("sha256"))) is not None,
                        "full native pixel receipt missing")
                require(isinstance(outcome.get("project"), dict) and integer(sample.get("revision")),
                        "authored project/revision missing")
                position = outcome.get("position")
                require(isinstance(position, list) and len(position) == 3, "position not recorded")
                frame = outcome.get("frame")
                require(type(frame) in (int, float) and math.isfinite(frame), "invalid sampled frame")
                if scenario == "property_drag":
                    require(position == [250 if number % 2 == 0 else 240, 180, 0] and frame == 0,
                            "wrong property drag result")
                else:
                    require(frame > previous_frame, "timeline did not advance through controls")
                    previous_frame = frame
                outcomes.append(outcome)
            if reference is None:
                reference = outcomes
            else:
                require(outcomes == reference, "native pixels/properties/project differ across ABBA episodes")
        pairs = [{"enabled": e, "disabled": d,
                  "ratio": p95(episodes[e]["samples"]) / p95(episodes[d]["samples"])}
                 for e, d in ((1, 0), (2, 3))]
        enabled = [s for e in (1, 2) for s in episodes[e]["samples"]]
        disabled = [s for e in (0, 3) for s in episodes[e]["samples"]]
        pooled = p95(enabled) / p95(disabled)
        results.append(dict(scenario=scenario, pairs=pairs, pooledRatio=pooled,
                            noRegression=pooled <= 1 and all(p["ratio"] <= 1 for p in pairs)))
    return dict(scenarios=results, noRegression=all(r["noRegression"] for r in results),
                limits=data.get("limits"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        report = assess(json.loads(args.evidence.read_text(encoding="utf-8")))
    except (ValueError, TypeError, KeyError, OSError) as error:
        report = dict(noRegression=False, invalidEvidence=str(error))
    text = json.dumps(report, indent=2)
    print(text)
    if args.output:
        args.output.write_text(text + "\n", encoding="utf-8")
    return 0 if report["noRegression"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
