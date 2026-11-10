"""Bounded, deterministic sensor timestamp association; no image/model reads."""
from bisect import bisect_left, bisect_right
from dataclasses import dataclass
import re

LIMIT_NS = 20_000_000
_STAMP = re.compile(r'([0-9]{1,12})(?:\.([0-9]{1,9}))?\Z')


def nanoseconds(stamp):
    match = _STAMP.fullmatch(stamp)
    if match is None:
        raise ValueError('timestamp must be a nonnegative decimal with at most nine fractional digits')
    whole, fraction = match.groups()
    return int(whole) * 1_000_000_000 + int((fraction or '').ljust(9, '0'))


@dataclass(frozen=True)
class Frame:
    timestamp_ns: int
    relative: str


@dataclass(frozen=True)
class Pair:
    rgb: Frame
    depth: Frame

    @property
    def signed_difference_ns(self):
        return self.depth.timestamp_ns - self.rgb.timestamp_ns


def parse(text, kind):
    if kind not in ('rgb', 'depth') or len(text) > 1 << 20:
        raise ValueError('invalid timestamp kind or input budget')
    frames = []
    names = set()
    for line in text.splitlines():
        if len(line) > 256:
            raise ValueError('timestamp line exceeds budget')
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        fields = line.split()
        if len(fields) != 2:
            raise ValueError('timestamp row requires exactly two fields')
        stamp, relative = fields
        if not relative.startswith(kind + '/') or not relative.endswith('.png'):
            raise ValueError('frame path differs from timestamp kind')
        stem = relative[len(kind)+1:-4]
        timestamp = nanoseconds(stamp)
        if nanoseconds(stem) != timestamp:
            raise ValueError('filename and row timestamp differ')
        if relative in names or (frames and timestamp <= frames[-1].timestamp_ns):
            raise ValueError('duplicate or unordered timestamp row')
        names.add(relative)
        frames.append(Frame(timestamp, relative))
        if len(frames) > 4096:
            raise ValueError('timestamp frame count exceeds budget')
    if not frames:
        raise ValueError('empty timestamp sequence')
    return frames


def associate(rgb, depth, count=32):
    """Globally nearest unique matching, with exact deterministic tie order."""
    if not 0 < len(rgb) <= 4096 or not 0 < len(depth) <= 4096 or not 0 < count <= 4096:
        raise ValueError('invalid association count')
    for frames in (rgb, depth):
        if any(a.timestamp_ns >= b.timestamp_ns for a, b in zip(frames, frames[1:])):
            raise ValueError('association requires strictly increasing timestamps')
    times = [frame.timestamp_ns for frame in depth]
    candidates = []
    for ri, frame in enumerate(rgb):
        lo = bisect_left(times, frame.timestamp_ns - LIMIT_NS)
        hi = bisect_right(times, frame.timestamp_ns + LIMIT_NS)
        for di in range(lo, hi):
            candidates.append((abs(times[di]-frame.timestamp_ns), frame.timestamp_ns,
                               times[di], ri, di))
            if len(candidates) > 65536:
                raise ValueError('association candidate count exceeds budget')
    used_rgb, used_depth, pairs = set(), set(), []
    for _, _, _, ri, di in sorted(candidates):
        if ri in used_rgb or di in used_depth:
            continue
        used_rgb.add(ri)
        used_depth.add(di)
        pairs.append(Pair(rgb[ri], depth[di]))
    pairs.sort(key=lambda pair: pair.rgb.timestamp_ns)
    if len(pairs) < count:
        raise ValueError('insufficient uniquely associated frames within fixed20ms')
    return pairs[:count]
