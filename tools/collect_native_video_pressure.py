"""Explicit native four-file application-budget degradation/recovery study.

This changes a real application allowance, not physical VRAM or driver pressure.
Stepped source frames and production cooldowns do not measure playback FPS.
"""
import argparse
import json
from pathlib import Path

import collect_native_video_window as native
import native_frame_activity as activity

TEST = 'engine::native_video_study::pressure::real_file_video_pressure_cost_study'
SOURCES = [*native.SOURCES, 'tools/collect_native_video_pressure.py', 'tools/native_frame_activity.py',
           'desktop/app/src/engine/native_video_study/pressure.rs']
HIGH = 256 << 20
LOW = 32 << 20
METRICS = ('cycleWithSeekUs', 'renderCycleUs', 'presentReplyUs',
           'stateObservationUs', 'resourceObservationUs')


def quality(row, budget):
    preview = row['preview']
    adaptation = preview['memoryAdaptation']
    level = adaptation['actual']['level']
    if type(level) is not int or not 0 <= level <= 3:
        raise ValueError('Invalid actual preview quality level')
    dimensions = [(dimension + (1 << level)-1) // (1 << level) for dimension in (1280, 720)]
    if (preview['mode'] != 'high' or preview['video']['streams'] != 4
            or preview['sourceTextureBudgetBytes'] != budget
            or row['renderError'] is not None
            or adaptation['hasLastValidFrame'] is not True
            or adaptation['held'] is not False
            or adaptation['status'] != ('normal' if level == 0 else 'degraded')
            or [adaptation['actualWidth'], adaptation['actualHeight']] != dimensions
            or preview['videoResolution'] != ('decoded_source' if level == 0 else f'proxy_max_{2048 >> level}')):
        raise ValueError('Pressure state lost source, budget, valid front or explicit quality evidence')
    return level


def verify_phase(report, rows, profile, initial_level, final_level):
    name = report['name']
    budget = LOW if name == 'limited' else HIGH
    if not rows or report['completedFrames'] != len(rows) or report['sourceBudgetBytes'] != budget:
        raise ValueError('Missing actual phase completions or application allowance')
    gpu = native.verify_gpu_profile(profile, rows)
    frames = [row['sourceFrame'] for row in rows]
    if (any(type(frame) not in (int, float) or frame != int(frame) or not 0 <= frame < 600 for frame in frames)
            or frames != list(range(int(frames[0]), int(frames[0])+len(frames)))
            or any(row['lastPresentedFrame'] != row['sourceFrame'] for row in rows)):
        raise ValueError('Stepped source frames were lost, repeated or mismatched')
    levels = [quality(row, budget) for row in rows]
    if levels[-1] != final_level or report['lastPreview'] != rows[-1]['preview']:
        raise ValueError('Phase did not end at its reported actual quality')
    if name in ('normal', 'limited'):
        if len(rows) != 3 or any(level != final_level for level in levels):
            raise ValueError('Normal/limited phase did not retain its actual quality')
        if name == 'limited' and any(row['preview']['memoryAdaptation']['refusalCount'] < 1 for row in rows):
            raise ValueError('Limited phase has no actual source-budget refusal')
    else:
        if (report['successfulWarmFrames'] != 120 or type(report['probeAfterWait']) is not bool
                or len(rows) != 120 + int(report['probeAfterWait'])
                or final_level != initial_level-1
                or levels != [initial_level]*(len(rows)-1) + [final_level]):
            raise ValueError('Recovery did not use bounded successful frames and one production quality step')
        cooldown = report['cooldownBeforeWaitMs']
        elapsed = report['cooldownWaitUs']
        if (type(cooldown) is not int or not 0 <= cooldown <= 30_000
                or type(elapsed) is not int or elapsed < 0
                or report['probeAfterWait'] and elapsed < max(0, cooldown-1)*1000):
            raise ValueError('Recovery cooldown evidence is missing or inconsistent')
    for metric in METRICS:
        if any(type(row[metric]) is not int or row[metric] < 0 for row in rows):
            raise ValueError('Actual phase timing is missing or invalid')
        native.check_distribution(report[metric], rows, metric)
    # Keep counter intervals within one profiler phase. A first sample has no
    # earlier baseline; absent/decreased counters remain unknown, never zero.
    intervals = activity.activity_rows(dict(cold=rows[0], warmup=[], measured=rows[1:]), profile['frames'])
    return dict(name=name, completedFrames=len(rows), actualFinalLevel=final_level,
                rootGpuTimestampCoverage=gpu, timing={metric: report[metric] for metric in METRICS},
                intervalSummary=activity.summarize(intervals), intervals=intervals)


def verify_preserved_front(proof, level=0):
    dimensions = [(dimension + (1 << level)-1) // (1 << level) for dimension in (1280, 720)]
    if (proof.get('allBytesEqual') is not True
            or type(proof.get('actualReadbackBytes')) is not int
            or proof['actualReadbackBytes'] != dimensions[0]*dimensions[1]*4):
        raise ValueError('Missing exact retained-front readback before the next render')


def validate(output):
    metadata = native.validate_process_receipts(output, TEST)
    root = output / 'native'
    study = native.read(root / 'study.json')
    if (study['compiledSourceRevision'] != metadata['sourceRevision']
            or study['debugAssertions'] is not False
            or study['physicalPressure'] is not False or study['playbackFpsClaimed'] is not False
            or study['composition'] != [1280, 720] or study['window'] != [1280, 800]):
        raise ValueError('Wrong compiled source, Release profile or claimed pressure scope')
    manifest = native.verify_fixtures(Path(metadata['fixtures']))
    if manifest != native.read(root / 'source-fixtures.json'):
        raise ValueError('Native fixture identity changed')
    report = study['report']
    limited = report['limitedQualityLevel']
    if (type(limited) is not int or not 1 <= limited <= 3
            or report['sourceCount'] != 4 or report['highSourceBudgetBytes'] != HIGH
            or report['lowSourceBudgetBytes'] != LOW or report['projectUnchanged'] is not True):
        raise ValueError('Missing actual limited quality, budget transition or unchanged project')
    names = ['normal', 'limited', *(f'recovery-{i}' for i in range(1, limited+1))]
    if [phase['name'] for phase in report['phases']] != names:
        raise ValueError('Missing complete normal/limited/recovery phases')
    verify_preserved_front(report['phases'][1]['retainedFront'])
    verify_preserved_front(report['restoredFront'], limited)
    source_root = root / 'sources-4'
    if list(source_root.rglob('failure.json')):
        raise ValueError('Native study contains an original phase failure')
    probes = native.read(source_root / 'native-probes.json')
    if len(probes) != 4:
        raise ValueError('Missing actual four-stream native probes')
    for probe, fixture in zip(probes, manifest['sources']):
        if (native.sha(source_root / 'project' / probe['asset']['path']) != fixture['sha256']
                or len(probe['timestampsUs']) != 600):
            raise ValueError('Actual native source or index differs from fixture')
    sources = native.read(source_root / 'actual-native-source-frames.json')
    if (sources != report['actualSources'] or len(sources) != 4
            or any((source['width'], source['height']) != (3840, 2160) for source in sources)):
        raise ValueError('Final recovered decoders did not return four full-resolution sources')
    phases, inputs, next_frame, level = [], {}, 0, 0
    for phase in report['phases']:
        folder = source_root / phase['name']
        rows = native.read(folder / 'completion-records.json')['records']
        original = [json.loads(line) for line in (folder / 'frames.jsonl').read_text(encoding='utf-8').splitlines()]
        if rows != original or rows[0]['sourceFrame'] != next_frame:
            raise ValueError('Phase completion records differ from original frame evidence')
        origin = native.read(folder / 'profile-origin.json')
        profile_file = folder / 'native-profile.json'
        if origin['retainedWithoutReserialization'] is not True or origin['originalBytes'] != profile_file.stat().st_size:
            raise ValueError('Missing preserved primary profiler bytes')
        final_level = 0 if phase['name'] == 'normal' else limited if phase['name'] == 'limited' else level-1
        phases.append(verify_phase(phase, rows, native.read(profile_file), level, final_level))
        pixels = native.read(folder / 'pixel-check.json')
        if (pixels != phase['pixels'] or pixels['matches'] is not True or len(pixels['samples']) != 4
                or any(sample['matches'] is not True or sample['rgbTolerance'] != 8
                       or sample['actualRgba'][3] != 255
                       or any(abs(a-b) > 8 for a, b in zip(sample['actualRgba'][:3], sample['referenceRgba'][:3]))
                       for sample in pixels['samples'])):
            raise ValueError('Native pressure pixels differ from decoded reference centers')
        for filename in ('frames.jsonl', 'completion-records.json', 'native-profile.json', 'window.png', 'pixel-check.json'):
            path = folder / filename
            inputs[path.relative_to(output).as_posix()] = native.sha(path)
        next_frame += len(rows)
        level = final_level
    if level != 0 or report['sourceFramesConsumed'] != next_frame:
        raise ValueError('Recovery did not close at the original full-quality level')
    return dict(schemaVersion=1, sourceRevision=metadata['sourceRevision'],
                acceptedScope='Explicit desktop application source-budget degradation/recovery only',
                phases=phases, inputSha256=inputs, completedFrames=next_frame,
                physicalPressureAccepted=False, playbackFpsAccepted=False,
                completeGpuStageCoverage=False, completeReadbackCoverage=False,
                controlledOptimizationComparison=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path)
    parser.add_argument('--target-dir', type=Path)
    parser.add_argument('--environment-note', default='Uncontrolled desktop load, power and thermals; application-budget pressure only')
    parser.add_argument('--validate-only', action='store_true')
    args = parser.parse_args()
    args.output = args.output.resolve()
    if args.validate_only:
        print(json.dumps(validate(args.output)))
    else:
        if args.fixtures is None or args.target_dir is None:
            parser.error('--fixtures and --target-dir are required for collection')
        args.fixtures, args.target_dir = args.fixtures.resolve(), args.target_dir.resolve()
        native.collect(args, test_name=TEST, verifier=validate, source_files=SOURCES)


if __name__ == '__main__':
    main()
