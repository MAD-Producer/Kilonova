"""Actual CLI controls: authored absence stays quiet; real failures reset/escalate."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

from plate_contract import require
from track_plate import atomic_json, digest
from validate_prototype import encode


def evaluate(ffmpeg, ffprobe, output):
    output.mkdir(parents=True, exist_ok=False)
    report = dict(status='running', scope='Actual optional tracker CLI, default Info, file/Error stderr/stdout isolation', cases=[])
    try:
        import numpy as np
        source = output/'blank.webm'
        encode(ffmpeg, source, [np.zeros((192, 256), np.uint8) for _ in range(340)])
        fingerprint = digest(source)
        for name, resume in [('authored-absence', False), ('failure-reset', True)]:
            seeds = [dict(id=state, kind='point', position=[80., 70.]) for state in ['lost', 'occluded']]
            edits = []
            for seed in seeds:
                for index in ([0, 20, 60, 120, 200, 220] if resume else [0]):
                    visible = index in [20, 120, 220]
                    edits.append(dict(track=seed['id'], source_index=index,
                        visibility='visible' if visible else seed['id'],
                        observation=dict(kind='point', pixels=[80., 70.]) if visible else None))
            request = dict(version=1, source_sha256=fingerprint, stream_index=0, tracks=seeds, corrections=edits)
            config = output/(name+'.request.json')
            atomic_json(config, request)
            env = os.environ.copy()
            env.pop('MOTION_LOG', None)
            command = [sys.executable, str(Path(__file__).with_name('track_plate.py')),
                '--source', str(source), '--request', str(config), '--output', str(output/name),
                '--ffmpeg', str(ffmpeg), '--ffprobe', str(ffprobe)]
            with (output/(name+'.stdout.log')).open('wb') as stdout, (output/(name+'.stderr.log')).open('wb') as stderr:
                child = subprocess.run(command, env=env, stdout=stdout, stderr=stderr)
            row = dict(name=name, exitCode=child.returncode)
            report['cases'].append(row)
            require(child.returncode == 0, 'actual CLI failed: '+name)
            require((output/(name+'.stdout.log')).read_bytes() == b'', 'tracker polluted stdout')
            asset = json.loads((output/name/'tracks.mstrack').read_text(encoding='utf-8'))
            evidence = json.loads((output/name/'report.json').read_text(encoding='utf-8'))
            text = (output/name/'tracking.log').read_text(encoding='utf-8')
            stderr = (output/(name+'.stderr.log')).read_text(encoding='utf-8')
            require('INFO start operation=track_plate' in text, 'default Info start missing')
            errors = [line[line.index('ERROR '):] for line in text.splitlines() if 'ERROR ' in line]
            require(stderr.splitlines() == errors, 'Error file/stderr output differs')
            row['errorRecords'] = errors
            for target in asset['tracks']:
                require(len(target['samples']) == 340, 'source coverage differs')
                manual = {edit['source_index']: edit for edit in edits if edit['track'] == target['id']}
                for index, sample in enumerate(target['samples']):
                    require(sample['source_index'] == index, 'original source order differs')
                    edit = manual.get(index)
                    require(sample['manual_correction'] == (edit is not None), 'manual identity differs')
                    require(sample['visibility'] == (edit['visibility'] if edit else 'lost'), 'missing output behavior differs')
                    require(sample['observation'] == (edit['observation'] if edit else None), 'invented geometry')
            if not resume:
                require(errors == [], 'authored absence incorrectly escalated as an automatic failure')
                require(evidence['retainedCauses'] == {}, 'authored absence became a retained fault')
            else:
                require(len(errors) == 4, 'failure reset or suppression differs')
                for target in seeds:
                    for index in [180, 280]:
                        require(sum(f"track={target['id']} " in line and f'source_index={index} ' in line
                            and 'occurrences=60' in line for line in errors) == 1,
                            'automatic failure threshold/reset frame differs')
                    require(evidence['retainedCauses'][target['id']]['firstFrame'] == 21,
                            'manual recovery cleared the primary automatic failure')
        report.update(status='accepted_actual_manual_absence_and_failure_reset_controls', sourceSha256=fingerprint)
    except BaseException as cause:
        report.update(status='failed', failure=repr(cause))
        raise
    finally:
        atomic_json(output/'evaluation.json', report)
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['ffmpeg', 'ffprobe', 'output']:
        parser.add_argument('--'+name, type=Path, required=True)
    args = parser.parse_args()
    evaluate(args.ffmpeg, args.ffprobe, args.output)
