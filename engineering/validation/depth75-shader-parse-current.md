# Native preparation shader parse diagnostic

Status: diagnostic implemented and verified; original parser failure unresolved.

The complete host-library command in `depth75-adapter-memory-controls-v3` failed
in the original multiview exposure test. Naga reported end of input at byte 5683,
line 109, while expecting `)`. The error did not retain the generated module's
digest. No specific driver, external module, cache, compiler or parser defect is
inferred from that result.

Parse errors now include the exact submitted module's SHA-256, byte count and
bounded compilation category. These are computed only on error. Both image-effect
and host-program compilation preserve the original typed parser cause, spans and
diagnostic. The additional metadata does not copy complete user shader text,
change normalization or add retries. Existing caller error routing is unchanged.

The production utility control constructs the real PlanBuilder and recompiles
each actual generated module through the host compiler. All three succeeded:

| Utility | UTF-8 bytes | SHA-256 |
| --- | ---: | --- |
| sdk-raster | 5708 | df9cc1fd9baaa72086d0d06fdf9530f3783f685e82baa393a575e21165367f78 |
| sdk-convert | 5683 | 6d617dd0965f06a832b0201d5326d773cfa33d43fad69ff65136efde90ec6cd5 |
| sdk-mask-source | 5763 | 11a2b8085e49b4a1075b0ba8b1e486be7f1f19deeb8174afb8bf1135f5732dd7 |

The conversion module's length and final line coincide with the earlier error;
this is a source-identification clue, not proof that the failed module had these
exact bytes. This CPU control does not reproduce a deterministic syntax defect.

One source-changed, instrumented host-library command was then run to capture
module identity if the original failure recurred. It completed with 294 passed,
zero failed, one original ignore and no filtering. The earlier error did not
recur, so no failed-module digest was obtained. This result is not described as
fixing the original failure, and no unchanged-source retry was performed.

The complete `shader_error_identity` target passed all ten tests, including two
new malformed-module controls. They verify exact host-module fingerprints and
expanded image-module identity while retaining Naga's original typed error.
The real utility control passed one test with 352 other library tests filtered;
it is not a complete renderer regression. All existing assertions remain intact.

Raw commands, source diff and logs are in private
`artifacts/pr-queue/depth75-shader-parse-diagnostic-v1/`. Original session 59161
terminated with zero and was consumed at chunk 2c3a85. Source remained base
`fe3a13ab4ed9e06400eb2eb4e8fa19666cd172bf` plus diff SHA-256
`b6a2186132376d1d6726a93d028082c5d2e5e9f0b0e7156bf05e2cd4e180b3df`.
No backend, compiler, stack or environment workaround, new acquisition, panel
change, assertion relaxation or native process intervention was used.
