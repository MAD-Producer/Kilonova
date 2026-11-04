# Complete model/core runtime regression

Status: all 141 Cargo targets of the selected model/core packages,
including documentation tests, passed on frozen parent `2ca4e0a6eb2b3099e368843e98f476c5df9f8d32`.
The actual command completed **890 passed, zero failed,
1 original ignored, zero filtered** in 477.266 seconds.
Actual invocations and summaries were matched against Cargo metadata.

The command was `cargo test -p motion-model -p motion-core --no-fail-fast
--locked --offline -- --test-threads=1`. No assertions, expression deadlines,
ignore declarations or name filters changed. This includes the new expression
interruption observations; one local pass does not identify the original CI
timeout cause or establish its repair. This gate covers neither subsequent
main `e8ea491e`, whole-workspace regression nor desktop default compiler faults.
Android is excluded.

The first attempt stopped before tests with its exit unknown. The second and
third attempts failed before tests with linker LNK1285 errors naming corrupt
test PDB files. Original outputs and receipts are retained without rewriting.
The named PDB and then the matching core test PDB family were moved reversibly
within E, with every before/after SHA-256 checked. No cache files were deleted.
The raw PDBs remain in private artifacts; the archive includes their exact
preservation manifests rather than the multi-gigabyte payloads. The fourth
attempt completed after that specific build-artifact repair, without a source
or test-policy change.

The [archive](data/frame-rate85-model-core-complete-v4.zip) retains all four
attempts, source closure, raw outputs, controllers, environment and target
audit. All members were read back and hash-verified; its
[receipt](data/frame-rate85-model-core-complete-v4.json) records scope and limits.
This is not completion of the 3D route or issues 85/86/88.
