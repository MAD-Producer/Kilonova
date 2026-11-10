# Prerender grouped-view admission and input timer parking (#82)

These components were implemented and measured on base `2b6e4860` with the exact
source hashes retained below. They remain component evidence on that source;
current-main integration, strict foreground P95 and full regression still need
their final gates. The existing failed acceptance is not cleared.

Grouped views own outputs for independent cameras and placements. The optional
single authored-view producer now refuses that profile, retires warm frames and
drains its in-flight/retained ownership. Returning to a single authored view
rearms warming. An actual native negative control first failed because grouped
views kept warming; the corrected native control, late-input regression, viewer
regression and three host grouped-view controls passed. An earlier host command
received a cached35-test binary rather than the intended coverage and failed its
count guard; that result is not credited as the intended source's acceptance.
The native fixture also needed the actual retained texture reader in place of
capture metadata before its original pixel assertions passed. Original records
remain, including the unchanged pixel assertion and added explicit byte count.

Optional timer polling now parks while input is pending and no completion lease
needs acknowledgment. Outstanding acknowledgments continue polling so that
cancellation cannot strand ownership. A successful current Present rearms work;
a stale Present cannot acknowledge newer input. Faults park after draining and
require explicit configuration to recover. Status reports the current input
hold. CPU and native negative controls originally failed their intended timer
parking assertions. Four CPU controls and six subsequent native boundary/
recovery controls passed, preserving existing thresholds and assertions.

The complete original controller records are in the
[grouped-view archive](data/native-prerender-multiview-admission-controls.zip)
and [manifest](data/native-prerender-multiview-admission-controls.json), and the
[parking archive](data/native-prerender-input-poll-parking-controls.zip)
and [manifest](data/native-prerender-input-poll-parking-controls.json).
Retained originals report terminal component results, and their exact controller
and child PIDs are now absent. No command was restarted to obtain a pass.
Archive hashes, CRC and copied bytes were checked before incorporation.
No binary or image/media payload is included.

The parking record's candidate hashes exactly match the current viewer, runtime
and referenced test-module sources before integration. Its complete source map
also identifies the earlier grouped-view-only source separately; the later
parking increment is not mislabeled as that earlier measured source. The local
unreferenced phase-diagnostic fixture is not enabled by these changes.

No panels, UI layout/style or render precision policy changes. This correction
does not qualify the failed strict input P95, physical constrained-device power,
the original native abnormal exits, actual final-head MCP/UI, complete workspace
regression or cross-platform CI. Issue #82 and draft PR #575 remain incomplete.
