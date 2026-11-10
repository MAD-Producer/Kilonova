"""Run one native cache ENOSPC control in an isolated, bounded Linux tmpfs."""
from pathlib import Path
from datetime import datetime, timezone
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile

TEST = ('movie::prerender::worker::tests::disk_cache::full_volume::'
        'actual_full_volume_keeps_native_frames_project_bytes_and_recovery_intact')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def run_native(command, env, out, report, save):
    report['nativeStartedUtc'] = datetime.now(timezone.utc).isoformat()
    save()
    with (out / 'stdout.log').open('wb') as stdout, (out / 'stderr.log').open('wb') as stderr:
        child = subprocess.Popen(command, env=env, stdout=stdout, stderr=stderr)
        report['nativePid'] = child.pid
        save()
        code = child.wait()
    report['exitCode'] = code
    report['nativeCompletedUtc'] = datetime.now(timezone.utc).isoformat()
    # Persist the real exit before parsing or ordinary mount cleanup.
    save()
    # Native failures can contain non-UTF8 bytes. Decode only the summary view;
    # keep the original files and actual exit code as the authoritative evidence.
    output = (out / 'stdout.log').read_text(encoding='utf-8', errors='replace')
    report['counts'] = re.findall(
        r'(?m)^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored', output)
    save()
    if code != 0 or report['counts'] != [('1', '0', '0')]:
        # PRs intentionally do not upload these artifacts. Surface bounded
        # original causes in the job log as well as retaining complete files.
        print(f'native full-volume test failed: exit={code} counts={report["counts"]}', file=sys.stderr)
        for name in ('stdout.log', 'stderr.log'):
            path = out / name
            length = path.stat().st_size
            with path.open('rb') as stream:
                head = stream.read(4096)
                if length > 8192:
                    stream.seek(-4096, os.SEEK_END)
                    excerpt = head + b'\n[truncated; complete original file retained]\n' + stream.read(4096)
                else:
                    excerpt = head + stream.read(4096)
            print(f'--- {name} ({length} bytes) ---', file=sys.stderr)
            print(excerpt.decode('utf-8', errors='replace'), file=sys.stderr)
        raise ValueError('native test failed; original logs retained')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', type=Path, required=True)
    args = parser.parse_args()
    require(sys.platform == 'linux', 'bounded-volume launcher is Linux-only')
    require('MOTION_PRERENDER_FULL_VOLUME' not in os.environ, 'do not reuse an existing volume')
    out = args.artifacts.resolve()
    out.mkdir(parents=True, exist_ok=False)
    volume = Path(tempfile.mkdtemp(prefix='motion-prerender-full-'))
    require(not any(char.isspace() for char in str(volume)), 'mountpoint needs an unescaped path')
    require(not out.is_relative_to(volume), 'evidence must stay outside the bounded volume')
    mounted = False
    report = dict(status='preparing', startedUtc=datetime.now(timezone.utc).isoformat(),
                  volume=str(volume), artifacts=str(out), quotaBytes=8 << 20,
                  limits='Linux tmpfs only. No physical host-volume exhaustion, dependency changes, retries, performance acceptance or issue completion.')

    def save():
        (out / 'receipt.json').write_text(json.dumps(report, indent=2), encoding='utf-8')

    save()
    try:
        mount = ['sudo', '-n', 'mount', '-t', 'tmpfs', '-o',
                 f'size=8m,nr_inodes=1024,nodev,nosuid,noexec,mode=0700,uid={os.getuid()},gid={os.getgid()}',
                 'tmpfs', str(volume)]
        report['mountCommand'] = mount
        subprocess.run(mount, check=True)
        mounted = True
        info = os.statvfs(volume)
        total = info.f_blocks * info.f_frsize
        require(0 < total <= 8 << 20, f'unexpected filesystem capacity: {total}')
        require(os.stat(volume).st_dev != os.stat(volume.parent).st_dev, 'mount is not a separate device')
        line = next(line for line in Path('/proc/self/mountinfo').read_text().splitlines()
                    if line.split()[4] == str(volume))
        require(line.split(' - ', 1)[1].split()[0] == 'tmpfs', 'filesystem is not tmpfs')
        report.update(status='running', actualVolumeBytes=total, mountInfo=line,
                      separateDevice=True)
        command = ['cargo', 'test', '-p', 'motion-desktop-media', '--locked', '--lib',
                   '--features', 'ai-depth,motion-host/diagnostics,motion-host/gpu-tests',
                   TEST, '--', '--ignored', '--exact', '--test-threads=1']
        report['command'] = command
        save()
        env = os.environ.copy()
        env.pop('MOTION_LOG', None)
        env.update(MOTION_PRERENDER_FULL_VOLUME=str(volume),
                   MOTION_PRERENDER_FULL_VOLUME_ARTIFACTS=str(out / 'native'))
        run_native(command, env, out, report, save)
        evidence = out / 'native/native-full-volume.json'
        native = json.loads(evidence.read_text(encoding='utf-8'))
        require(native['actualOsError'] == 28 and native['actualErrorKind'] == 'StorageFull', 'missing actual ENOSPC')
        require(0 < native['filledBytes'] <= 8 << 20, 'invalid exhaustion evidence')
        for key in ['nativeFramePreserved', 'existingCacheHitOnFullVolume', 'projectBytesUnchanged',
                    'projectReloaded', 'cacheRecovered']:
            require(native[key] is True, key)
        require(native['partialEntryPublished'] is False, 'partial entry was published')
        report.update(status='passed_linux_full_volume_component',
                      nativeEvidenceSha256=hashlib.sha256(evidence.read_bytes()).hexdigest())
    except BaseException as error:
        report.update(status='failed', failure=repr(error))
        raise
    finally:
        # Never recursively remove a still-mounted filesystem. Preserve the
        # original failure if ordinary unmount/empty mountpoint cleanup fails.
        primary = sys.exc_info()[0] is not None
        cleanup = None
        if mounted:
            result = subprocess.run(['sudo', '-n', 'umount', str(volume)], capture_output=True)
            report['unmountExitCode'] = result.returncode
            if result.returncode:
                cleanup = result.stderr.decode('utf-8', errors='replace')[:2048]
            else:
                mounted = False
        if not mounted:
            try:
                volume.rmdir()
            except OSError as error:
                cleanup = repr(error)
        if cleanup:
            report['cleanupFailure'] = cleanup
            report['status'] = 'failed'
        report['completedUtc'] = datetime.now(timezone.utc).isoformat()
        save()
        if cleanup and not primary:
            raise RuntimeError(f'owned volume cleanup failed: {cleanup}')
    print(json.dumps({key: report.get(key) for key in ['status', 'exitCode', 'counts', 'actualVolumeBytes']}))


if __name__ == '__main__':
    main()
