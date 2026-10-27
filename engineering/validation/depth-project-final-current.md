# Project shared model depth and calibrated-depth final checkpoint

This checkpoint combines the explicitly calibrated native depth contract,
cold retained model receiver and exposure repair, and the real all-model project
shared renderer on main `f6e7dbb21b809894062c3fcf7b9be9fd2845362d`.
The final product source is `76afd45c94e0b0c54dc9cf4997a6c9deb0f5aea3`;
the following publication commit adds only this evidence and documentation.

## Executed acceptance

- The project consumer has **334 distinct accepted native tests**, zero
  unresolved failures and zero ignores across three targets. This is the
  documented 319-test unchanged-production run plus the final complete 15-test
  model target, not one all-green 334-test invocation. The new Float16 reference
  history, failed ideal reference and independent bit-exact isolated control
  remain in [the project checkpoint](project-model-depth-current.md).
- The earlier main-aligned calibration/receiver checkpoint has 496 passes,
  zero failures and zero ignores. It overlaps the 334 and is not added to it.
  Its exact scope and later source changes remain documented in
  [the earlier checkpoint](depth-model-main-current.md).
- Build-v8 rebuilds the unchanged actual Session/native FFmpeg/CPU ONNX harness
  against the final project consumer. Actual-v5 checks **6,144** native color
  and hardware-depth pixels with maximum absolute depth error
  `1.7886120418086904e-08` against the original
  `< 1e-6` bound. Raw RGB and inference output remain bit-exact to the prior
  independent official-reference gate. Default Info starts, actual stale-PTS
  Error in file and stderr, and empty stdout pass. Illustrative supplied
  anchors are not a claim of metric model prediction accuracy.
- A prematurely scheduled build-v7 stopped before compilation or GPU execution
  because its dependency receipt was not yet written. Its original failure is
  retained. Build-v8 ran only after that receipt existed.

## Scope and remaining work

The project routes homogeneous all-model scenes with common camera and sample
count to one shared hardware depth buffer and global primitive transparency
order. Independent model assets, saved/reopened projects, reverse and abandoned
submissions, four precisions at 1x/4x, exact budget refusal/recovery and deletion
of the last model's resources are exercised. Global primitive-center sorting
does not claim exact intersecting transparency or order-independent blending.

Calibrated inference is verified by an external owning-worker harness. This
does not persist calibration in projects, route current frames automatically,
or provide arbitrary image-plane reprojection. Mixed model/plane/matte/effect/
nested depth composition and imported-camera selection remain open. No UI,
panels, styles, dependencies or third-party assets changed. Android is retired
and excluded; the delivered movie was not rerendered. Assertions, tolerances,
budgets and timeouts were not relaxed. Earlier full-workspace acceptance belongs
to the earlier morph/skin source; this checkpoint makes no new full-workspace
claim. Cross-platform checks run on this follow-up PR's current merge ref.

## Reproducible evidence

[Final source-equivalence receipt, rebuilt harness, actual outputs and retained
pre-build failure](data/depth-project-final-v1.zip): SHA-256
`f7a025be0a7c75f10298f7daf10cbe9990fc1a5f46a346259f1b4802705d42b9`, 1,260,698 bytes,
41 entries. Every entry was read back and hash-verified.
Executables, shared libraries, model weights and Python bytecode are excluded.
The existing model and calibration archives retain the remaining raw native
outputs and failure history.
