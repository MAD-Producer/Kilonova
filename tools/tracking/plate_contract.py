"""Bounded external prototype request/clock admission; no optional CV imports."""
import json
import math
import re
from fractions import Fraction
from pathlib import Path

MAX_FRAMES = 4096
MAX_TRACKS = 16
MAX_SOURCE_BYTES = 2 << 30


class ConsecutiveFailure:
    """One bounded site per track/run; normal/manual states reset the streak."""
    def __init__(self):
        self.count = 0

    def record(self, failed):
        if not failed:
            self.count = 0
            return None
        if self.count >= 60:
            return None
        self.count += 1
        return self.count if self.count in (1, 60) else None


class TrackFailures:
    """Per track/run: authored absence lasts until an explicit visible edit.

    Subsequent missing samples retain the existing output contract, but do not
    represent automatic measurement failures while the author has stopped the
    track. A visible edit resumes failure observation even if reseeding fails.
    """
    def __init__(self):
        self.manual_absence = False
        self.streak = ConsecutiveFailure()

    def record(self, visibility, manual):
        if manual:
            self.manual_absence = visibility != 'visible'
        failed = visibility == 'lost' and not manual and not self.manual_absence
        return failed, self.streak.record(failed)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def integer(value, low, high):
    return type(value) is int and low <= value <= high


def vector(value, size):
    return (isinstance(value, list) and len(value) == size
            and all(type(v) in (int, float) and math.isfinite(v) for v in value))


def unique_object(items):
    result = {}
    for key, value in items:
        require(key not in result, 'duplicate JSON field: '+key[:128])
        result[key] = value
    return result


def read_request(path):
    with Path(path).open('rb') as stream:
        data = stream.read(65537)
    require(len(data) <= 65536, 'prototype request exceeds 64 KiB')
    request = json.loads(data, object_pairs_hook=unique_object)
    require(isinstance(request, dict) and set(request) == {
        'version', 'source_sha256', 'stream_index', 'tracks', 'corrections'},
        'prototype request fields differ')
    require(type(request['version']) is int and request['version'] == 1, 'prototype request version differs')
    require(isinstance(request['source_sha256'], str) and re.fullmatch('[0-9a-f]{64}', request['source_sha256']), 'invalid source SHA-256')
    require(integer(request['stream_index'], 0, 127), 'invalid video stream index')
    tracks = request['tracks']
    require(isinstance(tracks, list) and 1 <= len(tracks) <= MAX_TRACKS, 'prototype track count exceeds budget')
    known = {}
    for track in tracks:
        require(isinstance(track, dict), 'track must be an object')
        kind = track.get('kind')
        require(kind in ('point', 'plane'), 'prototype supports point/plane tracks')
        field, count = ('position', 2) if kind == 'point' else ('roi', 4)
        require(set(track) == {'id', 'kind', field}, 'track fields differ')
        name = track['id']
        require(isinstance(name, str) and 0 < len(name.encode('utf-8')) <= 128
                and all(ord(c) >= 32 for c in name), 'invalid track identifier')
        require(name not in known, 'duplicate track identifier')
        require(vector(track[field], count), 'invalid track coordinates')
        known[name] = kind
    corrections = request['corrections']
    require(isinstance(corrections, list) and len(corrections) <= MAX_FRAMES * MAX_TRACKS, 'correction count exceeds budget')
    seen = set()
    for edit in corrections:
        require(isinstance(edit, dict) and set(edit) == {'track', 'source_index', 'visibility', 'observation'}, 'correction fields differ')
        require(edit['track'] in known and integer(edit['source_index'], 0, MAX_FRAMES-1), 'invalid correction target')
        key = (edit['track'], edit['source_index'])
        require(key not in seen, 'duplicate correction')
        seen.add(key)
        require(edit['visibility'] in ('visible', 'occluded', 'lost'), 'invalid correction visibility')
        observation = edit['observation']
        if edit['visibility'] == 'visible':
            kind = known[edit['track']]
            field = 'pixels' if kind == 'point' else 'matrix'
            require(isinstance(observation, dict) and set(observation) == {'kind', field}
                    and observation['kind'] == kind, 'correction observation kind differs')
            value = observation[field]
            require(vector(value, 2) if kind == 'point' else isinstance(value, list)
                    and len(value) == 3 and all(vector(row, 3) for row in value), 'invalid correction coordinates')
        else:
            require(observation is None, 'missing correction must not contain an observation')
    return request


def metadata(document, stream_index):
    streams = document.get('streams', [])
    require(len(streams) == 1, 'expected exactly one selected video stream')
    stream = streams[0]
    require(stream.get('index') == stream_index and stream.get('codec_type') == 'video', 'selected stream is not video')
    width, height = stream.get('width'), stream.get('height')
    require(integer(width, 1, 4096) and integer(height, 1, 4096) and width*height <= 4096*2160, 'prototype source extent exceeds budget')
    require(stream.get('sample_aspect_ratio', '1:1') in ('1:1', 'N/A'), 'tracking requires square source pixels')
    base = Fraction(stream['time_base'])
    rate = Fraction(stream['avg_frame_rate'])
    require(0 < base.numerator <= 1_000_000_000 and 0 < base.denominator <= 1_000_000_000, 'source timebase exceeds budget')
    require(0 < rate.numerator <= 1_000_000 and 0 < rate.denominator <= 1_000_000, 'nominal source rate exceeds budget')
    return [width, height], base, [rate.numerator, rate.denominator]


def timestamp(line, extent, base, previous):
    fields = dict(part.split('=', 1) for part in line.split('|') if '=' in part)
    require('pts' in fields and fields['pts'] != 'N/A', 'decoded frame has no original PTS')
    require([int(fields['width']), int(fields['height'])] == extent, 'decoded source extent changed')
    value = int(fields['pts']) * base.numerator
    require(-(1 << 63) <= value < (1 << 63), 'original PTS exceeds signed clock')
    require(previous is None or value > previous, 'original decoded PTS is not strictly increasing')
    return value


def check_coordinates(request, extent, count):
    width, height = extent
    for track in request['tracks']:
        if track['kind'] == 'point':
            x, y = track['position']
            require(0 <= x < width and 0 <= y < height, 'point seed is outside original source')
        else:
            x, y, w, h = track['roi']
            require(0 <= x and 0 <= y and w >= 16 and h >= 16 and x+w <= width and y+h <= height, 'plane ROI is outside original source or too small')
    for edit in request['corrections']:
        require(edit['source_index'] < count, 'correction exceeds original frame count')
        if edit['visibility'] == 'visible' and edit['observation']['kind'] == 'point':
            x, y = edit['observation']['pixels']
            require(0 <= x < width and 0 <= y < height, 'point correction is outside original source')
