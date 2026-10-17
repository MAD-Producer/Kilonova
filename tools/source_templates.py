"""Fail closed when a pinned source template changes unexpectedly."""


def replace_required(source: str, old: str, new: str, expected: int = 1) -> str:
    count = source.count(old)
    if not old or count != expected:
        raise ValueError(f"template replacement expected {expected} occurrence(s), found {count}: {old!r}")
    return source.replace(old, new)
