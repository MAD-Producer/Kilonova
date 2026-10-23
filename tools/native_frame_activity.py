"""Derive completed-frame interval activity from an accepted native study.

Counter intervals include decoder polling/chrome presentations between observed
completions. They are not exclusive command costs or physical GPU traffic.
"""
import argparse
import json
import math
from pathlib import Path

import collect_native_video_window as native


COUNTERS = {
    'instrumentedSubmissions': ('resourceMemory', 'textures', 'submissionTracking', 'preview', 'submitted'),
    'videoUploads': ('preview', 'video', 'uploads'),
    'videoGpuConversions': ('preview', 'video', 'gpuConversions'),
    'videoEncodedConversions': ('preview', 'video', 'encodedConversions'),
    'standaloneVideoConversionSubmissions': ('preview', 'video', 'standaloneConversionSubmissions'),
    'videoBlendSubmissions': ('preview', 'video', 'frameBlendSubmissions'),
    'videoUploadBytes': ('preview', 'video', 'uploadBytes'),
    'imageUploadBytes': ('preview', 'imageUploadBytes'),
    'effectPipelineBuilds': ('preview', 'effectPipelineBuilds'),
    'compositingPipelineSetBuilds': ('preview', 'compositingPipelineSetBuilds'),
    'vectorEncodedUploadBytes': ('resources', 'vectorUpload', 'encodedUploadBytesTotal'),
    'vectorCompletedUploadBytes': ('resources', 'vectorUpload', 'completedUploadBytesTotal'),
}


def lookup(record, path):
    for key in path:
        if not isinstance(record, dict) or key not in record:
            return None
        record = record[key]
    return record


def counters(row, frame):
    # Resource samples are owned by the same sequence-verified native frame.
    record = dict(row, resources=frame.get('resources'))
    values = {}
    for name, path in COUNTERS.items():
        value = lookup(record, path)
        if name == 'instrumentedSubmissions' and lookup(
            record, ('resourceMemory', 'textures', 'submissionTracking', 'preview', 'enabled')
        ) is not True:
            value = None
        if value is not None and (type(value) is not int or value < 0):
            raise ValueError(f'Invalid cumulative counter: {name}')
        values[name] = value
    return values


def interval(previous, current):
    values, status = {}, {}
    for name, value in current.items():
        before = None if previous is None else previous.get(name)
        if value is None:
            reason = 'current_unknown'
        elif before is None:
            reason = 'baseline_unknown'
        elif value < before:
            reason = 'counter_decreased'
        else:
            values[name] = value - before
            status[name] = 'observed'
            continue
        values[name] = None
        status[name] = reason
    return values, status


def distribution(values):
    values = sorted(value for value in values if value is not None)
    if not values:
        return None
    return dict(count=len(values), max=values[-1], **{
        name: values[max(0, math.ceil(len(values)*p)-1)]
        for name, p in [('p50', .5), ('p95', .95), ('p99', .99)]
    })


def summarize(rows):
    return {name: dict(
        distribution=distribution(row['interval'][name] for row in rows),
        unknownCount=sum(row['interval'][name] is None for row in rows),
        decreasedCount=sum(row['counterStatus'][name] == 'counter_decreased' for row in rows),
    ) for name in COUNTERS}


def activity_rows(raw, frames):
    rows = [('cold', raw['cold'])]
    rows.extend(('warmup', row) for row in raw['warmup'])
    rows.extend(('measured', row) for row in raw['measured'])
    if len(rows) != len(frames):
        raise ValueError('Activity rows do not match native frame coverage')
    result, previous, previous_sequence = [], None, None
    for (phase, row), frame in zip(rows, frames):
        sequence = frame['sequence']
        if (type(sequence) is not int or sequence < 0
                or previous_sequence is not None and sequence <= previous_sequence
                or row['sourceFrame'] != frame['frame']):
            raise ValueError('Activity identity differs from native frame order')
        current = counters(row, frame)
        delta, status = interval(previous, current)
        result.append(dict(
            sequence=sequence, previousSequence=previous_sequence,
            sourceFrame=frame['frame'], phase=phase,
            cumulative=current, interval=delta, counterStatus=status,
            cpuPrepareUs=frame['cpuPrepareUs'],
            gpuRootTotalUs=frame['gpuTotalUs'],
            associatedTimingPayloadBytes=32 if frame['gpuTotalUs'] is not None else None,
            imageReadbackBytes=None,
            presentPolls=row['presentPolls'],
            stateObservationUs=row['stateObservationUs'],
            resourceObservationUs=row['resourceObservationUs'],
        ))
        # A decrease/missing observation establishes a new baseline only for
        # later valid observations; it never turns an unknown interval into 0.
        previous, previous_sequence = current, sequence
    return result


def analyze(output):
    # Failure receipts, pixels, source identity, actual decoder streams, native
    # execution and timestamp coverage must all pass before deriving a report.
    acceptance = native.validate(output)
    runs, inputs = [], {}
    for count in (1, 4):
        root = output / 'native' / f'sources-{count}'
        raw = native.read(root / 'completion-records.json')
        profile = native.read_original_profile(root)
        rows = activity_rows(raw, profile['frames'])
        runs.append(dict(sourceCount=count, rows=rows,
                         measuredIntervalSummary=summarize([r for r in rows if r['phase'] == 'measured'])))
        for name in ('completion-records.json', 'project/exports/preview-performance.json', 'native-profile.json'):
            path = root / name
            inputs[path.relative_to(output).as_posix()] = native.sha(path)
    return dict(
        schemaVersion=1, sourceRevision=acceptance['sourceRevision'],
        acceptedInputScope=acceptance['acceptedScope'], inputSha256=inputs,
        scope='Observed counter differences between successful native composition completions',
        limitations=[
            'Intervals include pending decoder polls, chrome and other instrumented work between completions',
            'The fixed source-bound driver keeps one attached renderer per workload; counters are not cross-renderer identities',
            'The first completion has no earlier counter baseline; its interval stays null',
            'Missing/decreased counters remain null and resume only from a subsequent valid baseline',
            'Submission counters cover instrumented calls, not every raw queue submission or hardware submission',
            'Conversion counters have overlapping scopes and must not be summed',
            'Upload counters are application payload observations, not measured bus/staging traffic',
            'Pipeline counters cover effect pipelines and compositing sets, not all startup/driver compilation',
            'Associated timing payload is returned timestamp data, not total submitted/mapped/readback traffic',
            'Image readback bytes are not instrumented here and remain null',
            'GPU root time excludes earlier conversion, nested, vector and MSAA stages',
            'This is neither physical display FPS nor an optimization before/after comparison',
        ],
        completeGpuStageCoverage=False, completeReadbackCoverage=False,
        runs=runs,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--study', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise ValueError('Activity output must be new; original evidence is never overwritten')
    result = analyze(args.study.resolve())
    native.write_json(args.output, result)
    print(json.dumps(dict(sourceRevision=result['sourceRevision'], workloads=len(result['runs']),
                          output=str(args.output))))


if __name__ == '__main__':
    main()
