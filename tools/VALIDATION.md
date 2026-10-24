# Tool and Android validation inventory

`validation_inventory.json` records the execution policy for every repository
Python tool and every Android class containing JUnit `@Test` methods. A tool
that is not invoked directly by CI still receives the workflow's syntax checks;
it is not automatically unused code.

- `ci`: the workflow or its discovered unit tests execute the tool/class.
- `manual`: an explicit maintainer or instrumented acceptance command is needed.
- `physical_device`: the result is physical-device evidence, such as presentation
  deadlines or controlled benchmark measurements, rather than emulator evidence.

The inventory includes the invocation, prerequisites and reason for manual tools,
and the source file and command for Android classes. Update it when adding,
moving, renaming or deleting tools/tests. Installed `node_modules` and Python
bytecode are external/generated dependencies and are excluded from tool discovery.
New unclassified repository tools or Android test classes fail the guard.

Run the offline guards from the repository root:

```console
python tools/validation_inventory.py
python tools/ci_android_ui.py --list-tests
python -m unittest discover -s tools -p 'test_ci_*.py' -v
python -O -m unittest discover -s tools -p 'test_ci_tools.py' -v
```

The existing workflow's `test_ci_*.py` discovery picks up `test_ci_tools.py`.
The tests run the actual validator CLI/main functions in optimized subprocesses,
with synthetic reports and controlled I/O in temporary directories. These are
regressions for validation logic; they do not claim codec, GPU or physical-device
acceptance. Bare Python `assert` is forbidden in repository tools because `-O`
removes it; the unit tests retain `unittest.TestCase.assert*` methods.

The Android CI selectors are explicit and initially retain the existing 38
selectors and 103 tests. Full-class selectors count current source methods;
adding another method increases the expected count. A selected class or method
cannot silently disappear. Partial-class selections document why the other
methods remain manual. Source discovery ignores comments and string fixtures.

## Effect generator validation

After regeneration, use the Rust loader to validate all declared effect libraries:

```console
python tools/validation_inventory.py --effect-validator target/debug/effect_tool
```

On Windows use the `.exe` executable. This runs `plan`, `pack` into a temporary
archive, and `check` for each library in the inventory. The shared Rust validation
covers manifest limits, parameter IDs/ranges/enums, resource resolution and WGSL
loops; Python does not implement another shader/schema validator. It does not
rewrite committed or published packages.

CI already builds `effect_tool` in the SDK packer validation step. Add this line
to that step, after the executable exists, to enforce library validation in CI:

```sh
python tools/validation_inventory.py --effect-validator "$GITHUB_WORKSPACE/target/debug/effect_tool"
```

Alternatively, set `MOTION_EFFECT_VALIDATOR` when running `test_ci_tools.py` to
enable its real-loader integration test. The offline-only test run explicitly
skips that one integration test if an executable is not provided.

## Performance reports

`validate_performance.py` retains its device capture mode and can also validate
already captured evidence without installing APKs or contacting adb:

```console
python tools/validate_performance.py --report preview-performance.json --output artifacts/performance-recheck
```

Both modes retain the report and `summary.json` before raising on any failed
measured budget. CPU p95 must be at most 4 ms, reliably sampled GPU p95 at most
10 ms, PSS at most 400 MiB, asset textures at most 128 MiB, and reference fill at
most four screens. Invalid numeric measurements and debuggable reports fail.
GPU coverage below 99% remains `null`, with a missing gate, rather than becoming
a passed GPU check. Exit zero means the available measurements pass; it does
not establish complete physical-device acceptance. The summary retains
`fullA12AcceptanceComplete: false` and its other missing gates. Supply
`--seconds` when rechecking a capture shorter than the default ten minutes.

## Desktop native GPU latency archive

`collect_desktop_performance.py` is an explicit manual study, separate from the
PR test matrix. It builds the Release `performance_baseline` example and keeps
six fixed projects, raw cold/warm/steady frames, actual adapter/driver metadata,
original failures and source/binary/artifact hashes. Run `--validate-only` to
check an existing archive without running a GPU. Its archive/failure regressions
are discovered by `test_ci_desktop_performance.py`.

New schema-3 records add original per-encoder timestamp words, the native queue
timestamp period, exact epoch/encoder IDs, and complete bounded trace windows.
Offline request IDs are explicitly case-local sample ordinals, never Engine
worker requests or joins by authored frame. Static vector preparation is inside
the main encoder before the root pass; nested composition and YUV conversion
commands retain separate identities. These intervals can overlap and must not
be summed into exclusive GPU busy time. Final PNG readback is after trace stop.
`test_ci_desktop_performance_commands.py` checks malformed protocol rejection;
its synthetic records do not establish actual native availability or timings.

Schema-2 records include bounded CPU package/program text ownership,
the actual single reusable YUV fixture capacities, and separately timed metadata
enumeration. Unknown process/driver storage remains null. Historical schema-1
archives require the collector at their recorded source; no missing CPU records
are synthesized. Schema-2 archives remain attributed to their historical source
and have no command evidence. New native collection requires schema 3. Protocol
tests do not replace a new source-bound native run.

See [scope and pending native acceptance](../engineering/validation/desktop-performance-baseline.md).
This headless, synchronized latency study does not measure window playback,
physical display deadlines, file decoding or Android device performance.
