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
