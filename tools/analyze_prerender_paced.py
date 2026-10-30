"""Assess raw ABBA native playback measurements, without treating a green test as P95 evidence."""
import argparse
import json
import math
from pathlib import Path


def require(condition, message):
    if not condition:
        raise ValueError(message)


def p95(costs):
    return sorted(costs)[math.ceil(len(costs) * .95) - 1]


def assess(data):
    require(data.get('fps') == 30 and data.get('framesPerEpisode') == 60,
            'expected 60 frames at 30 fps per episode')
    require(data.get('order') == ['disabled', 'enabled', 'enabled', 'disabled'],
            'expected declared ABBA order')
    require(data.get('partialFrames') == [], 'partial episode is not complete evidence')
    runs = data.get('runs', [])
    require(len(runs) == 4, 'expected four complete episodes')
    summaries = []
    for episode, (run, enabled) in enumerate(zip(runs, [False, True, True, False])):
        require(run.get('episode') == episode and run.get('enabled') is enabled,
                f'episode {episode}: wrong index or policy')
        frames = run.get('frames', [])
        require(len(frames) == 60 and [f.get('frame') for f in frames] == list(range(1, 61)),
                f'episode {episode}: missing, reordered or duplicated frames')
        previous = 0
        for record in frames:
            cost = record.get('foreground_us')
            require(type(cost) in (int, float) and math.isfinite(cost) and cost > 0,
                    f'episode {episode}: invalid foreground cost')
            hits = record.get('cachedPresented')
            require(type(hits) is int and previous <= hits <= record['frame'] and hits - previous <= 1,
                    f'episode {episode}: invalid real presentation count')
            require(type(record.get('presentPolls')) is int and record['presentPolls'] >= 1,
                    f'episode {episode}: missing successful Present polling')
            require(type(record.get('missedNextDeadline')) is bool,
                    f'episode {episode}: missing deadline outcome')
            previous = hits
        require(run.get('cachedPresented') == previous,
                f'episode {episode}: final count disagrees with samples')
        require(previous >= 30 if enabled else previous == 0,
                f'episode {episode}: insufficient actual hits or disabled cache produced hits')
        measured = p95([f['foreground_us'] for f in frames])
        require(run.get('p95_us') == measured, f'episode {episode}: reported P95 disagrees with raw costs')
        summaries.append(dict(episode=episode, enabled=enabled, p95Us=measured, hits=previous,
                              missedNextDeadline=sum(f['missedNextDeadline'] for f in frames)))
    pairs = [dict(enabled=e, disabled=d, ratio=summaries[e]['p95Us'] / summaries[d]['p95Us'])
             for e, d in [(1, 0), (2, 3)]]
    pooled = {str(enabled): p95([f['foreground_us'] for run in runs if run['enabled'] is enabled
                                for f in run['frames']]) for enabled in [False, True]}
    return dict(pairs=pairs, pooledP95Us=pooled, pooledRatio=pooled['True'] / pooled['False'],
                noRegression=all(pair['ratio'] <= 1 for pair in pairs) and pooled['True'] <= pooled['False'],
                runs=summaries,
                limits='Local native Engine-only measurement; not whole-app input, physical low-memory qualification or a universal hardware guarantee')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    try:
        result = assess(json.loads(args.report.read_text(encoding='utf-8')))
    except (OSError, ValueError, TypeError, KeyError) as error:
        parser.exit(2, f'Invalid paired playback evidence: {error}\n')
    encoded = json.dumps(result, indent=2)
    if args.output:
        args.output.write_text(encoded + '\n', encoding='utf-8')
    print(encoded)
    return 0 if result['noRegression'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
