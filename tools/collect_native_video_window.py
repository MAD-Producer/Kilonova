"""Explicit Windows Release study of real file video and native window replies.

The GPU profile covers the existing root composition/presentation timer. Native
reply deadlines are not compositor/monitor scanout measurements. Ordinary cargo
test does not run this study. Every invocation retains its original native logs.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
TEST = 'engine::native_video_study::real_file_video_window_cost_study'
SOURCES = ['tools/collect_native_video_window.py', 'tools/generate_window_video_fixtures.py',
           'desktop/app/src/engine.rs', 'desktop/app/src/engine/diagnostic_names.rs',
           'desktop/app/src/engine/native_video_study.rs',
           'desktop/app/src/engine/native_video_study/fixture.rs',
           'desktop/app/src/engine/native_video_study/playback.rs',
           'crates/motion-render/src/measurement.rs']


def sha(path):
    result = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1 << 20), b''):
            result.update(block)
    return result.hexdigest()


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def write_json(path, value):
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def run_phase(output, name, command, env, timeout=None):
    row = dict(phase=name, command=list(map(str, command)), startedUnix=time.time(),
               timeoutSeconds=timeout)
    write_json(output / (name + '.started.json'), row)
    stdout = output / (name + '.stdout.log')
    stderr = output / (name + '.stderr.log')
    print(f'START {name}', file=sys.stderr, flush=True)
    with stdout.open('xb') as out, stderr.open('xb') as err:
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=out, stderr=err)
        write_json(output / (name + '.process.json'), dict(pid=process.pid, startedUnix=time.time()))
        try:
            exit_code = process.wait(timeout=timeout)
            timed_out = False
        except subprocess.TimeoutExpired:
            # Only this explicitly launched test process; never a foreign worker.
            process.kill()
            exit_code = process.wait()
            timed_out = True
    row.update(exitCode=exit_code, timedOut=timed_out, elapsedSeconds=time.time()-row['startedUnix'],
               stdoutSha256=sha(stdout), stderrSha256=sha(stderr))
    write_json(output / (name + '.exit.json'), row)
    print(f'DONE {name}: exit={exit_code} timeout={timed_out}', file=sys.stderr, flush=True)
    if exit_code or timed_out:
        raise RuntimeError(f'{name} failed; original stdout/stderr/receipts retained at {output}')
    return stdout


def verify_fixtures(fixtures):
    manifest = read(fixtures / 'fixtures.json')
    if manifest['dimensions'] != [3840, 2160] or manifest['frames'] != 600 or manifest['fps'] != 30:
        raise ValueError('This study requires the actual four20second4K30fps fixtures')
    if len(manifest['sources']) != 4 or len({s['sha256'] for s in manifest['sources']}) != 4:
        raise ValueError('Four independently encoded files required')
    for index, source in enumerate(manifest['sources'], 1):
        if source['id'] != index or source['path'] != f'source-{index}.mp4':
            raise ValueError('Unexpected fixture identity/path')
        path = fixtures / source['path']
        if path.stat().st_size != source['bytes'] or sha(path) != source['sha256']:
            raise ValueError('Actual source bytes differ from the verified fixture')
        if [p['frame'] for p in source['independentDecode']] != [0, 300, 599]:
            raise ValueError('Missing real decoded first/middle/final reference frames')
        for pixel in source['independentDecode']:
            path = (fixtures / pixel['path']).resolve()
            if path.parent != fixtures.resolve() or path.stat().st_size != 3840*2160*4 or sha(path) != pixel['sha256']:
                raise ValueError('Actual independently decoded pixel bytes changed')
    return manifest


def check_distribution(actual, rows, field):
    values = sorted(row[field] for row in rows)
    expected = dict(count=len(values), max=values[-1], **{
        name:values[max(0, math.ceil(len(values)*p)-1)]
        for name, p in [('p50', .5), ('p95', .95), ('p99', .99)]})
    if actual != expected:
        raise ValueError(f'{field} distribution does not match raw completed records')


def read_original_profile(root):
    # stop_profiling writes this primary record before the test driver copies
    # it. Parsing and reserializing that copy can move an f64 by one ULP when
    # serde_json's optional float_roundtrip parser feature is disabled. Always
    # validate the host's original bytes; never loosen frame/sample equality.
    return read(root / 'project' / 'exports' / 'preview-performance.json')


def verify_gpu_profile(profile, rows):
    """Validate callback coverage; protocol fixtures alone are not GPU evidence."""
    frames = profile['frames']
    if profile['frameCount'] != len(rows) or len(frames) != len(rows) or profile['overflowFrames']:
        raise ValueError('GPU/native profile coverage does not match successful renders')
    if [r['frame'] for r in frames] != [r['sourceFrame'] for r in rows]:
        raise ValueError('GPU/native frame sequences differ from requested/completed frames')
    sequences = [r['sequence'] for r in frames]
    if any(type(s) is not int or s < 0 for s in sequences) or any(b <= a for a, b in zip(sequences, sequences[1:])):
        raise ValueError('Actual native recorded sequences must advance, including gaps')
    metrics = ('gpuCompositionUs', 'gpuPresentationUs', 'gpuTotalUs')
    samples = []
    for frame in frames:
        values = [frame[name] for name in metrics]
        if all(v is None for v in values):
            continue
        if any(type(v) not in (int, float) or not math.isfinite(v) or v < 0 for v in values):
            raise ValueError('Missing, partial or invalid GPU timing values must not become zero samples')
        samples.append(frame)
    expected_coverage = len(samples) / len(frames) if frames else 0
    if profile['gpuSampleCount'] != len(samples) or profile['gpuSampleCoverage'] != expected_coverage:
        raise ValueError('GPU sample count/coverage differs from raw callback records')
    graphics = profile['metadata']['graphics']
    readback_bytes = graphics['timestampReadbackBytesPerSample']
    if readback_bytes not in (0, 32):
        raise ValueError('Unknown native timestamp support state')
    supported = readback_bytes == 32
    if supported and not samples:
        raise ValueError('Supported native GPU timestamps produced no associated samples')
    if not supported and samples:
        raise ValueError('Unsupported timestamps must remain unknown, not invented samples')
    if graphics['timingErrors'] != (0 if supported else None):
        raise ValueError('Native GPU timing readback failed or support metadata is inconsistent')
    for field in metrics:
        if samples:
            check_distribution(profile[field], samples, field)
        elif profile[field] is not None:
            raise ValueError('Unsupported/missing GPU samples must keep null distributions')
    return dict(supported=supported, sampleCount=len(samples), coverage=expected_coverage)


def validate(output):
    metadata = read(output / 'provenance.json')
    for phase in ('build', 'native'):
        receipt = read(output / (phase + '.exit.json'))
        if receipt['exitCode'] or receipt['timedOut']:
            raise ValueError(f'Actual {phase} did not complete successfully')
        for stream in ('stdout', 'stderr'):
            if sha(output / f'{phase}.{stream}.log') != receipt[stream+'Sha256']:
                raise ValueError(f'Original {phase}/{stream} bytes changed')
    executable = read(output / 'test-binary.json')
    if sha(Path(executable['path'])) != executable['sha256']:
        raise ValueError('Original compiled test binary is no longer available unchanged')
    native_stdout = (output / 'native.stdout.log').read_text(encoding='utf-8')
    if TEST not in native_stdout or not re.search(r'test result: ok\. 1 passed; 0 failed; 0 ignored;', native_stdout):
        raise ValueError('The exact native study did not actually execute and pass')
    native = output / 'native'
    study = read(native / 'study.json')
    if study['compiledSourceRevision'] != metadata['sourceRevision']:
        raise ValueError('Native test binary was compiled for a different study source')
    if study['debugAssertions'] or [r['sourceCount'] for r in study['runs']] != [1,4]:
        raise ValueError('Missing actual Release one/four-source studies')
    manifest = read(native / 'source-fixtures.json')
    verify_fixtures(Path(metadata['fixtures']))
    gpu_coverage = []
    for count, report in zip((1,4), study['runs']):
        root = native / f'sources-{count}'
        if (root / 'failure.json').exists() or report != read(root / 'report.json'):
            raise ValueError('Native failure or changed report')
        raw = read(root / 'completion-records.json')
        measured = raw['measured']
        rows = [raw['cold'], *raw['warmup'], *measured]
        jsonl = [json.loads(line) for line in (root / 'frames.jsonl').read_text(encoding='utf-8').splitlines()]
        if rows != jsonl or not measured or not raw['warmup']:
            raise ValueError('Completed samples differ from original per-frame evidence')
        profile = read_original_profile(root)
        gpu_coverage.append(dict(sourceCount=count, **verify_gpu_profile(profile, rows)))
        for field in ('cycleWithSeekUs','completionIntervalUs','stateObservationUs','resourceObservationUs'):
            check_distribution(report[field], measured, field)
        if report['measuredCount'] != len(measured) or report['warmupCount'] != len(raw['warmup']):
            raise ValueError('Summary sample counts differ from raw completions')
        if report['requestedSourceDeadlineMisses'] != sum(r['requestedSourceDeadlineMissed'] for r in measured):
            raise ValueError('Deadline miss count differs from raw records')
        if report['skippedSourceBuckets'] != sum(r['skippedSourceBuckets'] for r in measured):
            raise ValueError('Skipped source buckets differ from raw records')
        previous = None
        for row in [*raw['warmup'],*measured]:
            frame = row['sourceFrame']
            if frame != row['lastPresentedFrame'] or not 0 <= frame < 120:
                raise ValueError('Wrong completed frame or source interval')
            bucket = math.floor(frame)
            if previous is not None and frame < previous:
                raise ValueError('Actual production clock regressed')
            if row['requestedSourceDeadlineMissed'] != (row['completionFromPlaybackUs'] > (bucket+1)*1_000_000//30):
                raise ValueError('Deadline comparison was inferred incorrectly')
            previous = frame
            preview = row['preview']
            if preview['mode'] != 'high' or preview['videoResolution'] != 'decoded_source' or preview['memoryAdaptation']['status'] != 'normal':
                raise ValueError('Unreported preview degradation')
            if preview['video']['streams'] != count:
                raise ValueError('Missing real independent decoder streams')
        probes = read(root / 'native-probes.json')
        if len(probes) != count:
            raise ValueError('Missing actual application probe/index evidence')
        for index, probe in enumerate(probes):
            source = root / 'project' / probe['asset']['path']
            if sha(source) != manifest['sources'][index]['sha256'] or len(probe['timestampsUs']) != 600:
                raise ValueError('Actual application source/index differs from fixture')
        actual_sources = read(root / 'actual-native-source-frames.json')
        if len(actual_sources) != count or any((s['width'],s['height']) != (3840,2160) for s in actual_sources):
            raise ValueError('Native decoder did not actually return full-resolution source frames')
        pixels = read(root / 'pixel-check.json')
        if not pixels['matches'] or len(pixels['samples']) != count or any(not p['matches'] for p in pixels['samples']):
            raise ValueError('Actual native output did not match independent decoded source pixels')
    return dict(schemaVersion=1, sourceRevision=metadata['sourceRevision'],
                acceptedScope='Explicit Release native file/window reply study only',
                nativeWorkloads=2, independentSourceCounts=[1,4], physicalMobileAccepted=False,
                rootGpuTimestampCoverage=gpu_coverage,
                actualDisplayScanoutAccepted=False, completeGpuStageCoverage=False,
                controlledOptimizationComparison=False)


def collect(args):
    if platform.system() != 'Windows':
        raise ValueError('The current native window driver is Windows-only')
    revision = subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip()
    if subprocess.check_output(['git','status','--porcelain'], cwd=ROOT).strip():
        raise ValueError('Commit and freeze the exact study source before compiling')
    verify_fixtures(args.fixtures)
    args.output.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env.update(CARGO_BUILD_JOBS='1', CARGO_INCREMENTAL='0', CARGO_NET_OFFLINE='true')
    # option_env! in the Rust study tracks this at compile time, so a cached
    # executable for an earlier worktree cannot masquerade as this source.
    env['MOTION_NATIVE_VIDEO_STUDY_REVISION'] = revision
    write_json(args.output / 'provenance.json', dict(sourceRevision=revision,
        sourceBlobs={p:sha(ROOT/p) for p in SOURCES}, fixtures=str(args.fixtures),
        fixtureManifestSha256=sha(args.fixtures/'fixtures.json'), targetDirectory=str(args.target_dir),
        environmentNote=args.environment_note, platform=platform.platform(),
        buildProfile='Repository Release profile, no optimization overrides',
        environment={key:env.get(key) for key in ('TEMP','TMP','FFMPEG_DIR','LIBCLANG_PATH','CARGO_BUILD_JOBS','CARGO_INCREMENTAL')}))
    try:
        build = run_phase(args.output, 'build', ['cargo','test','--release','--locked','--offline',
            '-p','motion-desktop','--features','ffmpeg','--bin','motion-studio','--no-run','-j1',
            '--target-dir',str(args.target_dir),'--message-format=json'], env)
        binaries = []
        for line in build.read_text(encoding='utf-8').splitlines():
            record = json.loads(line)
            if record.get('reason') == 'compiler-artifact' and record.get('executable') and record['profile']['test'] and record['target']['name'] == 'motion-studio':
                binaries.append(Path(record['executable']))
        if len(binaries) != 1:
            raise ValueError('Cargo did not identify exactly one actual motion-studio test binary')
        executable = binaries[0]
        runtime = Path(env['FFMPEG_DIR']) / 'bin'
        write_json(args.output / 'test-binary.json', dict(path=str(executable), sha256=sha(executable),
            runtimeLibraries={p.name:sha(p) for p in sorted(runtime.glob('*.dll'))}))
        env['MOTION_NATIVE_VIDEO_STUDY_OUTPUT'] = str(args.output/'native')
        env['MOTION_NATIVE_VIDEO_STUDY_FIXTURES'] = str(args.fixtures)
        for key in ('MOTION_LOG','MOTION_TRACE_UI','MOTION_DIAGNOSTICS_PATH','MOTION_PROJECT'):
            env.pop(key,None)
        run_phase(args.output, 'native', [str(executable),'--exact',TEST,'--ignored','--nocapture','--test-threads=1'], env, timeout=600)
        if subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT,text=True).strip() != revision or subprocess.check_output(['git','status','--porcelain'],cwd=ROOT).strip():
            raise ValueError('The measured study source changed during execution')
        result = validate(args.output)
        write_json(args.output/'accepted.json',result)
        print(json.dumps(result))
    except BaseException as primary:
        try:
            write_json(args.output/'failure.json',dict(cause=str(primary),type=type(primary).__name__,
                sourceRevision=revision,performanceAccepted=False))
        except Exception as secondary:
            print(f'Primary study failure: {primary}; evidence failure: {secondary}',file=sys.stderr)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--fixtures',type=Path)
    parser.add_argument('--target-dir',type=Path)
    parser.add_argument('--environment-note',default='Uncontrolled development desktop background load, power and thermals; no optimization-gain claim')
    parser.add_argument('--validate-only',action='store_true')
    args = parser.parse_args()
    args.output = args.output.resolve()
    if args.validate_only:
        print(json.dumps(validate(args.output)))
    else:
        if args.fixtures is None or args.target_dir is None:
            parser.error('--fixtures and --target-dir are required for collection')
        args.fixtures = args.fixtures.resolve()
        args.target_dir = args.target_dir.resolve()
        collect(args)


if __name__ == '__main__':
    main()
