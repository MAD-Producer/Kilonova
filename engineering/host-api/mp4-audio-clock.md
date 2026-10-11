# MP4 audio presentation clock and PCM cache v2

The portable MP4/MOV audio decoder places decoded samples at packet timestamps
in the declared track timebase. It keeps gaps silent and trims the beginning of
later packets when they overlap audio already written. This preserves source
timing without stretching or resampling the encoded audio. Each packet's decoded
sample count can differ from its presentation duration.

Leading empty edits and encoder priming removal still follow the selected track's
edit list. After the complete declared sample table has been read, an edit span
may be padded by at most one movie timebase tick to account for duration rounding.
Missing packets, backwards timestamps, invalid timebases, larger missing
tails and resource-limit violations return errors. The cancellation callback is
checked while writing silent gaps as well as while decoding packets.

PCM and waveform caches now use `cache/audio-v2`, derived from
`motion_media::PCM_VERSION`. Existing source files and project data are unchanged.
Cache preparation rebuilds v2 caches from owned sources; v1 payloads cannot be
reused because their samples may have been concatenated without timestamps.
Callers continue to use media preparation, PCM read and waveform APIs, rather
than reading private cache files directly. Unchanged source metadata is validated
on rebuild. A source whose metadata no longer matches the project reports an
error instead of silently replacing the stored asset.

The same decoder is used for Android imports and cache preparation, including
audio linked to video. Android retains its existing policy of rejecting damaged
portable sources rather than accepting partial platform fallback output.
