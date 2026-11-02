# #88 native video under a nonlinear camera

This increment fixes an actual missing render path, rather than suppressing an
unsupported-input error. The preceding real-Surface test qualified a pinhole
camera only. Extending that control to nonzero radial/tangential coefficients
failed at `shared_geometry` for an ordinary 3D native-video plane. The complete
failure, original source identities, child exit and default-level logs remain in
the [receipt](data/tracking88-native-video-lens-v1.json) and
[source/log/front archive](data/tracking88-native-video-lens-v1.zip).

## Functional change

- Ordinary decoded video can enter the existing nonlinear overscan geometry
  pass when it satisfies the existing simple-plane restrictions. The ordinary
  pinhole route stays unchanged. Effects, temporal sampling, masks/mattes,
  non-default blending, nested spatial inputs and aperture combinations retain
  their explicit boundaries; this is not blanket support for every video graph
- RGBA uploads retain their measured alpha classification; YUV420 conversion
  produces an opaque revision. Once a geometry encode references a revision,
  the next different upload allocates a replacement instead of mutating that
  pending revision. An unchanged source/PTS remains reusable
- An admitted texture view now retains its actual format as well as its
  allocation lease. Shared geometry creates and checks an sRGB sampling view,
  including for the existing UNORM video conversion allocation with an sRGB
  reinterpretation. The allocation's physical format is not its view format.
  Creation still uses wgpu's declared-view-format validation; ownership and
  geometry restrictions are preserved. The distinction follows the public
  [wgpu TextureView contract](https://docs.rs/wgpu/29.0.0/wgpu/struct.TextureView.html)

This is separately explained product functionality. No panel, layout, control,
interaction, dependency, external asset or Android implementation was changed.
Existing synthetic source media and builtin effects are reused; all marker,
solve and test code is independently authored.

## Actual Surface and history

The new Windows control keeps encoded video bytes, original PTS and frame 24
fixed while replacing the solve and its binding. It tests a point, a corner-pin
plane and a nonlinear camera, rather than relying on changed video content to
make a stale preview look different. Each uses the same attached window/device
through replacement, undo and redo, then attaches a new session on reopening.
`CaptureUi` reads the committed front without rerendering the composition.

Independent marker measurements are exact in the accepted run:

| Binding | Before center / area | After center / area | Independent expectation |
| --- | --- | --- | --- |
| Point | (319.5, 239.5) / 3072 | (359.5, 219.5) / 3072 | Source displacement (16, -6) becomes composition displacement (40, -20) |
| Plane | (319.5, 239.5) / 9216 | (327.5, 236.5) / 2304 | Half scale and translation in layer-local coordinates |

Every case checks a real composition redraw, changed pixels, one authoring
revision, unchanged authored layer data apart from explicit bindings, exact
undo/redo/reopen pixels, successful Present counters and subsequent normal
reuse without another composition render. Both radial signs use nonzero
tangential coefficients. This Surface case establishes lens cache/history
consistency; its nonempty-pixel assertion alone is not a lens accuracy oracle.

The complete Surface test group passed **2 tests, 0 failed, 0 ignored** in
18.40 seconds, including the preceding camera/video and rejected-source control.
The source-bound invocation including compilation completed in 95.532 seconds.
The new three-case native child exited 0 (`0x00000000`), PID 35924, 11251 ms.
Default Info starts and quiet healthy Present behavior are checked. The existing
camera control continues checking actual error file/stderr and stdout isolation.

## Independent queued-input GPU acceptance

The new renderer control uploads and encodes two different video revisions
before submitting either geometry command buffer. It covers both RGBA and YUV,
two crop rotations, both radial signs, transparent geometry and a flat separator.
Each output is checked against the preceding independently authored Newton-ray,
depth and alpha oracle. Its existing two-pixel edge exclusion and two-byte
threshold remain unchanged. The eight outputs each check 288586 to 290077
stable pixels; the measured maximum channel error is **1** for every output.
This checks color interpretation and immutable pending video inputs together.

Six complete renderer integration targets passed on the final source:

| Target | Passed | Failed / ignored |
| --- | ---: | --- |
| model_layers | 53 | 0 / 0 |
| tracking_lens | 17 | 0 / 0 |
| video_allocation_budget | 2 | 0 / 0 |
| video_instances | 5 | 0 / 0 |
| video_lifetime | 8 | 0 / 0 |
| video_yuv | 7 | 0 / 0 |

The original controller exited 0 after 102.219 seconds: **92 passed**, with source
hashes unchanged. All archive members are read back and checked before publication.
The new renderer test/hook were added after the native invocation; the receipt
verifies that every production and desktop-test source used there is unchanged.

## Retained failures and remaining scope

Two initial fixture failures are retained: model binding imports used the host
byte-contract namespace, and the plane fixture used a fake package hash suitable
only for a CPU model fixture. Both were corrected without changing assertions.
The subsequent real video/lens refusal is the functional defect fixed here.
The first renderer invocation passed the five other targets and existing lens
controls, but its new YUV fixture declared full range for limited-range black/
white samples. Only that range field was corrected; the exact CPU color
assertion and pixel thresholds were retained. The complete six-target rerun
then passed. None of these failures is omitted from the archive.

These are selected, source-bound native checks, not another full desktop package
or maintained-workspace regression. The earlier full package result predates
this functional change. Final-head CI remains separate, and the stacked branch
still requires integration with subsequent main changes. Existing native crash
root causes and the optional #582 photographed tracker quality remain unresolved.
The hidden Surface does not prove physical scanout, all codecs/lenses/bindings,
or a built-in 3D camera solver. Issue #88 and the overall 3D route remain open.
