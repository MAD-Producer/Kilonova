"""Author four real AVC files for the explicit native-window study, not FPS evidence.

Colors, rectangles and time-dependent rotation are independently authored here.
The existing FFmpeg executable is an encoder tool, not a product dependency.
Every run uses a new directory and retains original commands and failures.
"""
import argparse
import hashlib
import json
import platform
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
SIZE = [3840, 2160]
FPS = 30
COLORS = [('000000', 'c08020', '20c080'), ('602040', '20c080', '2080c0'),
          ('406020', '2080c0', 'c02080'), ('604020', '8020c0', '20c080')]


def sha(path):
    result = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1 << 20), b''):
            result.update(block)
    return result.hexdigest()


def write_json(path, value):
    with path.open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def phase(output, name, command, *, stdout=None):
    """Keep the start before an encoder/probe can hang, and the original exit."""
    row = dict(phase=name, command=list(map(str, command)), startedUnix=time.time())
    write_json(output / (name + '.started.json'), row)
    out = output / (name + '.stdout.log') if stdout is None else stdout
    error = output / (name + '.stderr.log')
    with out.open('xb') as destination, error.open('xb') as stderr:
        process = subprocess.run(command, stdout=destination, stderr=stderr)
    row.update(exitCode=process.returncode, elapsedSeconds=time.time()-row['startedUnix'],
               stdoutFile=out.name, stdoutSha256=sha(out), stderrSha256=sha(error))
    write_json(output / (name + '.exit.json'), row)
    if process.returncode:
        raise RuntimeError(f'{name} exited {process.returncode}; original cause is in {error.name}')
    return out


def encode_command(ffmpeg, target, index, frames):
    background, first, second = COLORS[index]
    filters = (f'drawbox=x=256:y=256:w=1024:h=512:color=0x{first}:t=fill,'
               f'drawbox=x=2304:y=1024:w=512:h=768:color=0x{second}:t=fill,'
               f'rotate={index}*0.11+0.06*t:fillcolor=black')
    return [str(ffmpeg), '-hide_banner', '-nostdin', '-loglevel', 'warning', '-n', '-filter_threads', '2',
            '-f', 'lavfi', '-i', f'color=c=0x{background}:s=3840x2160:r=30',
            '-vf', filters, '-an', '-c:v', 'libopenh264', '-threads', '2',
            '-b:v', '6000000', '-rc_mode', 'bitrate', '-allow_skip_frames', '0',
            '-g', '30', '-pix_fmt', 'yuv420p', '-frames:v', str(frames),
            '-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709',
            '-color_range', 'tv', '-video_track_timescale', '1000000',
            # OpenH264 can omit primaries/transfer despite codec context flags.
            # Write the authored SDR declaration into the actual H.264 VUI.
            '-bsf:v', 'h264_metadata=colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1:video_full_range_flag=0',
            '-movflags', '+faststart', str(target)]


def verify_metadata(probe, frames):
    streams = probe['streams']
    if len(streams) != 1:
        raise ValueError('A fixture must contain exactly one video stream')
    stream = streams[0]
    for name, expected in dict(codec_name='h264', width=3840, height=2160,
                               pix_fmt='yuv420p', r_frame_rate='30/1', avg_frame_rate='30/1',
                               nb_frames=str(frames), color_space='bt709', color_range='tv',
                               color_primaries='bt709', color_transfer='bt709').items():
        if stream.get(name) != expected:
            raise ValueError(f'Actual encoded {name}={stream.get(name)!r}, expected {expected!r}')
    if abs(float(stream['duration']) - frames / FPS) > 1 / FPS:
        raise ValueError('Actual source duration differs from the declared frame interval')


def generate(output, ffmpeg, ffprobe, frames):
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT).strip():
        raise ValueError('Commit the exact generator source before creating acceptance files')
    output.mkdir(parents=True, exist_ok=False)
    manifest = dict(schemaVersion=1, sourceRevision=revision, generatorSha256=sha(Path(__file__)),
                    dimensions=SIZE, fps=FPS, frames=frames, platform=platform.platform(),
                    author='Motion Studio contributors', license='MIT',
                    content='Independently authored backgrounds, two rectangles and time-dependent rotation',
                    scope='Four actual encoded source files; fixture preparation only, not Motion Studio decoding or playback acceptance',
                    tools={}, sources=[])
    try:
        for name, tool in [('ffmpeg', ffmpeg), ('ffprobe', ffprobe)]:
            version = phase(output, name + '-version', [str(tool), '-version'])
            manifest['tools'][name] = dict(path=str(tool), sha256=sha(tool),
                                          version=version.read_text(encoding='utf-8').splitlines()[0])
        for index in range(4):
            name = f'source-{index + 1}'
            target = output / (name + '.mp4')
            command = encode_command(ffmpeg, target, index, frames)
            phase(output, name + '-encode', command)
            probe_path = phase(output, name + '-probe', [str(ffprobe), '-v', 'error',
                '-select_streams', 'v:0', '-show_streams', '-show_format', '-of', 'json', str(target)])
            probe = json.loads(probe_path.read_text(encoding='utf-8'))
            verify_metadata(probe, frames)
            observations = []
            for sample in sorted({0, frames // 2, frames - 1}):
                # Keep actual native decoded pixels with their original receipts.
                # Select by decoded frame index, avoiding rounded MP4 seek times
                # accidentally skipping the last frame at the source boundary.
                raw = output / f'{name}-frame-{sample}.rgba'
                phase(output, f'{name}-decode-{sample}', [str(ffmpeg), '-hide_banner', '-nostdin',
                    '-loglevel', 'error', '-threads', '2', '-filter_threads', '2', '-i', str(target),
                    '-vf', f'select=eq(n\\,{sample})', '-frames:v', '1', '-fps_mode', 'passthrough',
                    '-an', '-f', 'rawvideo', '-pix_fmt', 'rgba', '-threads', '2', 'pipe:1'], stdout=raw)
                if raw.stat().st_size != SIZE[0] * SIZE[1] * 4:
                    raise ValueError(f'{name} frame {sample} did not produce a full native RGBA frame')
                with raw.open('rb') as pixels:
                    pixels.seek(((SIZE[1]//2)*SIZE[0]+SIZE[0]//2)*4)
                    center = list(pixels.read(4))
                observations.append(dict(frame=sample, path=raw.name, bytes=raw.stat().st_size,
                                         sha256=sha(raw), centerRgba=center))
            if frames > 1 and len({o['sha256'] for o in observations}) < 2:
                raise ValueError('Authored video did not produce changing decoded pixels')
            if len({tuple(o['centerRgba']) for o in observations}) != 1:
                raise ValueError('Actual independently decoded reference centers are not identical; native pixel checks cannot use different source times')
            manifest['sources'].append(dict(id=index+1, path=target.name, bytes=target.stat().st_size,
                sha256=sha(target), command=command, probe=probe, independentDecode=observations))
            write_json(output / (name + '.json'), manifest['sources'][-1])
        if len({source['sha256'] for source in manifest['sources']}) != 4:
            raise ValueError('The four source files must have distinct actual byte identities')
        write_json(output / 'fixtures.json', manifest)
        return manifest
    except BaseException as primary:
        try:
            write_json(output / 'failure.json', dict(cause=str(primary), type=type(primary).__name__,
                partialManifest=manifest, performanceAcceptance=False))
        except Exception as secondary:
            print(f'Primary fixture failure: {primary}; failure evidence also failed: {secondary}', file=sys.stderr)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--ffmpeg', type=Path, required=True)
    parser.add_argument('--ffprobe', type=Path, required=True)
    parser.add_argument('--frames', type=int, default=600)
    args = parser.parse_args()
    if not 2 <= args.frames <= 1800:
        parser.error('--frames must be 2..1800 (native study default: 600)')
    for tool in (args.ffmpeg, args.ffprobe):
        if not tool.is_file():
            parser.error(f'Missing explicit encoder/probe tool: {tool}')
    manifest = generate(args.output.resolve(), args.ffmpeg.resolve(), args.ffprobe.resolve(), args.frames)
    print(f'Created {len(manifest["sources"])} actual encoded sources at {args.output}; playback remains unmeasured', file=sys.stderr)


if __name__ == '__main__':
    main()
