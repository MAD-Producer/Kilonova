"""Optional point/planar prototype: original video -> strict tracking JSON.

Use explicit local FFmpeg/ffprobe and the credited external OpenCV/NumPy versions.
All observations use original, unrotated source pixels and exact signed PTS.
Lost tracks remain lost until an explicit correction; this is not a 3D solver.
"""
import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import importlib.metadata
import json
import logging
import os
from pathlib import Path
import subprocess
import sys
import threading
import time

from plate_contract import (ConsecutiveFailure, MAX_FRAMES, MAX_SOURCE_BYTES, check_coordinates,
                            metadata, read_request, require, timestamp)


def digest(path, maximum=MAX_SOURCE_BYTES):
    result = hashlib.sha256()
    with Path(path).open('rb') as source:
        info = os.fstat(source.fileno())
        require(0 < info.st_size <= maximum, 'source file exceeds byte budget')
        count = 0
        while block := source.read(1 << 20):
            count += len(block)
            require(count <= maximum, 'source grew beyond byte budget')
            result.update(block)
        after = os.fstat(source.fileno())
    require(count == info.st_size and (info.st_size, info.st_mtime_ns) == (after.st_size, after.st_mtime_ns), 'source changed during hashing')
    return result.hexdigest()


def atomic_json(path, document):
    encoded = json.dumps(document, indent=2, allow_nan=False).encode('utf-8')+b'\n'
    require(len(encoded) <= 16 << 20, 'prototype JSON exceeds 16 MiB')
    temporary = path.with_suffix(path.suffix+'.pending')
    temporary.write_bytes(encoded)
    temporary.replace(path)


def drain_stderr(pipe, destination, stats):
    """Keep draining a child even after its bounded diagnostic file is full."""
    try:
        while block := pipe.read(65536):
            stats['bytes'] += len(block)
            allowed = max(0, (256 << 10)-stats['retained'])
            if allowed and stats['error'] is None:
                try:
                    destination.write(block[:allowed])
                    stats['retained'] += min(allowed,len(block))
                except OSError as cause:
                    stats['error'] = str(cause)[:2048]
    finally:
        pipe.close()


class Run:
    def __init__(self, output):
        output.mkdir(parents=True, exist_ok=False)
        self.output = output
        self.began = time.monotonic()
        self.log = logging.getLogger('motion.prototype.'+str(os.getpid()))
        self.log.propagate = False
        requested = os.environ.get('MOTION_LOG', 'info').upper()
        level = {'OFF':100, 'ERROR':logging.ERROR, 'WARN':logging.WARNING,
                 'WARNING':logging.WARNING, 'INFO':logging.INFO,
                 'DEBUG':logging.DEBUG, 'TRACE':logging.DEBUG}.get(requested, logging.INFO)
        self.log.setLevel(level)
        file = logging.FileHandler(output/'tracking.log', encoding='utf-8')
        file.setFormatter(logging.Formatter('%(asctime)s %(levelname)s %(message)s'))
        console = logging.StreamHandler(sys.stderr)
        console.setLevel(logging.ERROR)
        console.setFormatter(logging.Formatter('%(levelname)s %(message)s'))
        self.handlers = [file, console]
        for handler in self.handlers:
            self.log.addHandler(handler)
        self.report = dict(status='running', phase='admission', pid=os.getpid(),
            startedUtc=datetime.now(timezone.utc).isoformat(), children=[],
            scope='External 2D/planar prototype; native import and render validation separate')
        self.save()

    def save(self):
        self.report['elapsedMs'] = int((time.monotonic()-self.began)*1000)
        atomic_json(self.output/'report.json', self.report)

    @contextmanager
    def child(self, phase, command):
        self.report['phase'] = phase
        self.log.info('start phase=%s source=%s', phase, self.report.get('sourceSha256', 'pending'))
        row = dict(phase=phase, command=list(map(str,command)), startedUtc=datetime.now(timezone.utc).isoformat())
        self.report['children'].append(row)
        self.save()
        with (self.output/(phase+'.stderr.log')).open('wb') as stderr:
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            stats = dict(bytes=0, retained=0, error=None)
            pump = threading.Thread(target=drain_stderr,args=(process.stderr,stderr,stats),
                                    name='tracking-child-stderr',daemon=True)
            pump.start()
            row['pid'] = process.pid
            self.save()
            try:
                yield process.stdout
                row['exitCode'] = process.wait()
                require(row['exitCode'] == 0, 'media child failed phase='+phase+'; see bounded diagnostic tail')
            finally:
                primary = sys.exc_info()[1]
                try:
                    if process.poll() is None:
                        process.terminate()
                        try:
                            process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()
                    process.stdout.close()
                    pump.join(timeout=10)
                    require(not pump.is_alive(), 'media diagnostic reader did not stop')
                    row.update(stderrBytes=stats['bytes'], stderrRetainedBytes=stats['retained'])
                    require(stats['error'] is None, 'media diagnostic write failed: '+str(stats['error']))
                    row['exitCode'] = process.returncode
                    stderr.flush()
                    with (self.output/(phase+'.stderr.log')).open('rb') as reader:
                        reader.seek(max(0, reader.seek(0, 2)-8192))
                        row['diagnosticTail'] = reader.read(8192).decode('utf-8', errors='replace')
                    self.save()
                except Exception as cleanup:
                    if primary is None:
                        raise
                    self.log.error('media cleanup failed phase=%s primary=%s cleanup=%s',phase,str(primary)[:2048],str(cleanup)[:2048])

    def close(self):
        for handler in self.handlers:
            self.log.removeHandler(handler)
            handler.close()


def capture(run, phase, command, limit):
    with run.child(phase, command) as stream:
        data = stream.read(limit+1)
        require(len(data) <= limit, 'media metadata exceeds byte budget')
    return data


def probe(run, executable, source, selected):
    common = [str(executable), '-v', 'error', '-select_streams', str(selected)]
    document = json.loads(capture(run, 'probe_stream', common+[
        '-show_entries', 'stream=index,codec_type,width,height,time_base,avg_frame_rate,sample_aspect_ratio:stream_tags=rotate:stream_side_data=rotation',
        '-of', 'json', str(source)], 128 << 10))
    extent, base, rate = metadata(document, selected)
    pts = []
    with run.child('probe_original_pts', common+['-show_frames', '-show_entries',
            'frame=pts,width,height', '-of', 'compact=p=0:nk=0', str(source)]) as stream:
        while line := stream.readline(8193):
            require(len(line) <= 8192, 'frame metadata line exceeds budget')
            text = line.decode('utf-8').strip()
            if not text:
                continue
            require(len(pts) < MAX_FRAMES, 'prototype frame count exceeds 4096; source is not truncated')
            pts.append(timestamp(text, extent, base, pts[-1] if pts else None))
    require(pts, 'video has no original decoded frames')
    return extent, base.denominator, rate, pts, document


def frames(run, executable, source, selected, extent, count):
    command = [str(executable), '-v', 'error', '-nostdin', '-threads', '1',
        '-noautorotate', '-copyts', '-i', str(source), '-map', '0:'+str(selected),
        '-an', '-sn', '-dn', '-fps_mode', 'passthrough', '-threads', '1',
        '-f', 'rawvideo', '-pix_fmt', 'gray', 'pipe:1']
    size = extent[0]*extent[1]
    with run.child('decode_original_frames', command) as stream:
        for _ in range(count):
            data = bytearray()
            while len(data) < size:
                block = stream.read(size-len(data))
                require(block, 'original decode ended before PTS sequence')
                data.extend(block)
            yield data
        require(not stream.read(1), 'original decode contains frames beyond PTS sequence')


def track(source, request_path, output, ffmpeg, ffprobe):
    run = Run(output)
    try:
        run.log.info('start operation=track_plate phase=admission source=%s', str(source)[:256])
        request = read_request(request_path)
        fingerprint = digest(source)
        require(fingerprint == request['source_sha256'], 'source SHA-256 differs from request')
        run.report['sourceSha256'] = fingerprint
        for name, executable in [('ffmpeg',ffmpeg), ('ffprobe',ffprobe)]:
            identity = digest(executable, 512 << 20)
            version = capture(run, name+'_version', [str(executable), '-version'], 128 << 10)
            run.report[name] = dict(sha256=identity, version=version.decode('utf-8', errors='replace')[:8192])
        extent, ticks, rate, pts, native_metadata = probe(run, ffprobe, source, request['stream_index'])
        check_coordinates(request, extent, len(pts))
        run.report.update(sourceExtent=extent, ticksPerSecond=ticks, sourcePts=pts,
                          streamMetadata=native_metadata, frameCount=len(pts))
        run.save()
        versions = {name:importlib.metadata.version(name) for name in ('opencv-python-headless','numpy')}
        require(versions == {'opencv-python-headless':'5.0.0.93','numpy':'2.4.6'}, 'optional CV tool versions differ')
        import plate_flow as flow
        flow.cv2.setNumThreads(1)
        flow.cv2.setRNGSeed(88)
        run.report.update(implementation=versions, limits=flow.LIMITS)
        edits = {(e['track'],e['source_index']):e for e in request['corrections']}
        tracks = [dict(id=seed['id'],kind=seed['kind'],reference_index=0 if seed['kind']=='plane' else None,samples=[]) for seed in request['tracks']]
        trackers, counters = [], [ConsecutiveFailure() for _ in tracks]
        retained_causes = {}
        previous = None
        decoder = frames(run, ffmpeg, source, request['stream_index'], extent, len(pts))
        try:
            for index, data in enumerate(decoder):
                gray = flow.prepare(flow.np.frombuffer(data,flow.np.uint8).reshape(extent[1],extent[0]))
                if index == 0:
                    trackers = [flow.Tracker(seed,gray) for seed in request['tracks']]
                for slot, (target, tracker) in enumerate(zip(tracks,trackers)):
                    edit = edits.get((target['id'],index))
                    if edit:
                        observation, confidence = tracker.correct(gray,edit)
                        visibility = edit['visibility']
                    else:
                        observation, confidence = (tracker.initial,1. if tracker.initial is not None else 0.) if index==0 else tracker.observe(previous,gray)
                        visibility = 'visible' if observation is not None else 'lost'
                    signal = counters[slot].record(visibility=='lost' and edit is None)
                    if visibility == 'lost' and edit is None:
                        retained_causes.setdefault(target['id'],dict(firstFrame=index,
                            cause=(tracker.reason or 'track remains lost')[:256]))
                    if signal is not None:
                        log = run.log.debug if signal==1 else run.log.error
                        log('tracking degradation source=%s track=%s phase=track occurrences=%d cause=%s',
                            fingerprint,target['id'],signal,(tracker.reason or 'track remains lost')[:256])
                    target['samples'].append(dict(source_index=index,visibility=visibility,
                        confidence=confidence,observation=observation,manual_correction=edit is not None))
                previous = gray.copy()
                if index % 60 == 0:
                    run.report['processedFrames'] = index+1
                    run.save()
        finally:
            decoder.close()
        require(digest(source) == fingerprint, 'original source changed during tracking')
        asset = dict(version=1,source_id='Motion Studio external point/planar prototype',
            source_sha256=fingerprint,source_extent=extent,coordinates='right_down_forward',
            ticks_per_second=ticks,nominal_rate=rate,source_pts=pts,meters_per_world_unit=None,tracks=tracks)
        atomic_json(output/'tracks.mstrack',asset)
        run.report.update(status='completed_external_prototype_native_admission_pending', phase='done', processedFrames=len(pts),
            retainedCauses=retained_causes,
            missingSamples={t['id']:sum(s['visibility']!='visible' for s in t['samples']) for t in tracks})
        run.log.info('done operation=track_plate frames=%d tracks=%d missing_samples=%d elapsed_ms=%d',len(pts),len(tracks),sum(run.report['missingSamples'].values()),int((time.monotonic()-run.began)*1000))
        return run.report
    except BaseException as cause:
        run.report.update(status='cancelled' if isinstance(cause,KeyboardInterrupt) else 'failed', failure=str(cause)[:8192])
        if isinstance(cause,KeyboardInterrupt):
            run.log.info('cancelled operation=track_plate phase=%s',run.report['phase'])
        else:
            run.log.error('failed operation=track_plate phase=%s elapsed_ms=%d cause=%s',run.report['phase'],int((time.monotonic()-run.began)*1000),str(cause)[:8192])
        raise
    finally:
        primary = sys.exc_info()[1]
        try:
            run.save()
        except Exception as cleanup:
            if primary is None:
                raise
            run.log.error('report cleanup failed primary=%s cleanup=%s',str(primary)[:2048],str(cleanup)[:2048])
        finally:
            run.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ('source','request','output','ffmpeg','ffprobe'):
        parser.add_argument('--'+option,type=Path,required=True)
    args = parser.parse_args()
    try:
        track(args.source,args.request,args.output,args.ffmpeg,args.ffprobe)
    except KeyboardInterrupt:
        sys.exit(130)
    except Exception:
        sys.exit(1)
