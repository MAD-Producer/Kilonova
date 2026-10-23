# Native video application-budget study

This opt-in Windows Release study observes four independently encoded 4K files on
one real native window, session, renderer and device. It does not exhaust physical
VRAM or inject driver pressure. Android validation is outside the user-confirmed
desktop scope of issue #60.

Use the existing authored fixture generator, matching FFmpeg runtime and a clean
committed source tree. Run explicitly:

```powershell
python tools/collect_native_video_pressure.py --fixtures <absolute-fixtures> --output <new-evidence-directory> --target-dir <absolute-build-cache>
```

The driver compiles the repository Release profile with no optimization overrides
and binds the test binary to the exact source revision. Ordinary unit runs ignore
this expensive manual study. The collector preserves original process streams,
exit status, timeouts, executable hashes and any failure, using a fresh output
directory for every invocation.

The normal allowance is 256 MiB of application-owned source textures. The study
lowers it to 32 MiB, observes real refusals and bounded preview degradation, then
restores it to 256 MiB. Before rendering after each policy change, a test-only
worker operation reads the complete retained front through the real renderer and
compares every byte with the previous readback. Those readbacks and retained CPU
oracle copies are outside profiling and are released before timing resumes.

Each phase records completed source frames, actual quality and dimensions,
refusals, timing, resource ledgers and allocation-frame observations. Phase
boundaries retain the original host profiler bytes before the next phase can
overwrite its export. Screenshots after profiling compare all four video centers
with independently decoded file pixels. The final decoder observation must return
four actual full-resolution frames, and project/history state must stay unchanged.

Recovery uses real successful source frames and the production cooldown. Each
round renders 120 warm frames, pumps native messages while the reported cooldown
expires if needed, and permits one additional eligible probe. Already completed
production recovery needs no extra probe. No clock, policy, retry loop, restart,
existing assertion or backend fallback is changed for the study.

Timings describe explicitly stepped source frames, not real-time playback FPS.
Root GPU timestamps exclude conversion, nested composition, vector and MSAA work
outside that timer. Counter intervals include intervening decoder polling and
chrome work. First, missing and decreased counter intervals remain unknown.
Submission and upload counters are application observations, not hardware/bus
traffic. The study does not establish complete readback coverage, physical
residency, operating-system exhaustion or optimization gains.

Protocol tests exercise rejection and unknown handling only. They are not GPU
results. A successful collector receipt and original native evidence are required
before any run is reported as accepted. No native pressure run is claimed here.
