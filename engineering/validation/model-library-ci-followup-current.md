# Model library CI catalogue follow-up

The main release run 40899785267 at de565977 failed the Linux native/MCP
smoke because tools/ci_desktop.py required the old nine-tool catalogue. Model
import intentionally adds import_model as the tenth tool. Keep exact equality,
include the new tool and report missing/unexpected names on future failures.
No product or protocol behavior changes.

Five existing Python silent-failure checks passed. The corrected verify_mcp
completed an actual native-window/shared-MCP smoke with the retained executable
from model-import-stdio-v4 (SHA-256
6ef8ff7721baf859f5f9be324628f81d6e9c3a8b1529f246e8e3bb7f420a3d24).
This checks the current Python validator against a recorded prior executable;
it is not a current-main rebuild or cross-platform acceptance. Evidence is in
artifacts/pr-queue/model-library-mcp-smoke-followup-v1/accepted.json.

The same main run also failed an Intel frozen-worker observation test, and
Windows was cancelled. Those are not resolved by this catalogue correction.
Do not mark the four-platform main release gate accepted on this evidence.

PR run 40978352894 passed Rust/GPU, dependency audit, Linux and Windows.
Intel failed persisted_expression_preparation_keeps_first_frame_authored_data_and_default_error_sinks.
The parent assertion printed only child stderr, which was empty; the child's
harness stdout and exit status were lost. Report both streams and status for
this test and the previously failing asynchronous movie-freeze test. All
success predicates, timeouts and semantic/output assertions remain unchanged.
This is a diagnostic improvement, not evidence that either intermittent failure
is fixed. The diagnostic-only Rust edit has not been locally rerun; new CI is
required before merge. The initial failed run remains recorded at
artifacts/pr-queue/model-library-ci-followup-failure-v1/failed.log.
