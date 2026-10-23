# Session resource budget and bounded preview: desktop acceptance

Tested product/test source: `03f629938635bfb7566520deacd5acf303a0880c`, integrated with main `8a72209725528bdbfe3fcf70f17ab554152a2174`.
All fifteen complete-suite, native, build and unchanged GUI/MCP gates passed on
that clean frozen source. The accompanying JSON preserves exact commands,
sources, exits, required actually passing tests and immutable raw-log SHA-256.
The fifteen local gates below are Windows executions, including commands
matching default CI. Linux and macOS Intel full desktop suites are independent
remote executions on the PR merge checkout, recorded separately below. Local
Windows GUI/readback results do not imply cross-platform window interaction or
macOS ARM native execution.

## Current cross-platform CI

Product/test source: `03f629938635bfb7566520deacd5acf303a0880c`. Workflow-only validation head:
`d3f4f776c2f8d9b791dadc23f25d93abcbfdb420`. Its only change is `.github/workflows/ci.yml`; the actual
merge checkout is `c3f057e37b72bd8dea0a95a3b979c5db9bcd54a2`, whose captured parents include
that validation head. Run `39886680559`,
attempt `1`. Every required job passed. Each desktop job's
raw log identifies its real DXGI/Metal/Vulkan pressure query and bounded native
snapshot, all four unit-test binaries, the complete unfiltered desktop
command, original default-output assertions, native UI pixel/admission tests and
configured package install/read tests. The manifest retains those checks and
raw log hashes. A successful log download is not treated as a successful job.
Linux uses software Vulkan; macOS execution is Intel. Formal output/FFmpeg,
native Windows window lifecycle and GUI/MCP have their own local gates below.

| Required job | Passed / failed / ignored | Raw log SHA-256 |
| --- | --- | --- |
| [Dependency audit](https://github.com/MAD-Producer/Kilonova/actions/runs/39886680559/job/120196363446) | Pass (non-test gate) | `f2b72725359b6854fd1c138a3c2d5f1fa6d1e9971afe5c605e85dc51326d23c4` |
| [Rust and GPU](https://github.com/MAD-Producer/Kilonova/actions/runs/39886680559/job/120196363338) | 1100 / 0 / 4 | `b57d3a1e4eba32d23359898939d7ea827e09176f87ba66dbb6dc3555b604ecca` |
| [Desktop build (ubuntu-24.04, linux-x86_64, motion-studio, x86_64-unknown-linux-gnu)](https://github.com/MAD-Producer/Kilonova/actions/runs/39886680559/job/120196363356) | 1060 / 0 / 1 | `82dbb231395a8f562d550f2ef253f86f361e54b08e675b03100a4674a101042d` |
| [Desktop build (windows-2022, windows-x86_64, motion-studio.exe, x86_64-pc-windows-msvc)](https://github.com/MAD-Producer/Kilonova/actions/runs/39886680559/job/120196363462) | 1089 / 0 / 1 | `b2b458fcf31c71ee9024d815c73a53f598ef89f7af57f02ebb31991857bbf2d6` |
| [Desktop build (macos-15-intel, macos-x86_64, motion-studio, x86_64-apple-darwin)](https://github.com/MAD-Producer/Kilonova/actions/runs/39886680559/job/120196363072) | 1060 / 0 / 1 | `68f920d707cce24a0ae56db08b486f54deb8d6b3924633b0d3643a884a741724` |

## Requirement coverage

The six issue61 implementation requirements, three acceptance criteria and the
observability contract are mapped below to actual tests on this same source.
Earlier-source passing totals cannot validate this source. Focused tests overlap
full suites and must not be summed as a distinct-test count.

| Requirement | Complete gate | Required actual test | Result |
| --- | --- | --- | --- |
| `R1_configurable_category_frame_policy_and_safe_unknown` | `render` | `memory_policy_is_bounded_and_guarded_or_unknown_devices_keep_the_floor` | Pass |
| `R1_configurable_category_frame_policy_and_safe_unknown` | `render` | `actual_vulkan_adapter_query_is_available_or_explicitly_unsupported` | Pass |
| `R1_configurable_category_frame_policy_and_safe_unknown` | `render` | `active_and_recent_metadata_are_bounded_and_saturation_recovers` | Pass |
| `R1_configurable_category_frame_policy_and_safe_unknown` | `ci-default` | `shared_json_policy_is_atomic_runtime_state_and_inherited_by_editor_rendering` | Pass |
| `R2_capacity_distinct_from_model_shader_codec_and_gpu` | `render` | `guarded_preview_refuses_resource_and_shader_errors_while_legacy_diagnostics_remain_available` | Pass |
| `R2_capacity_distinct_from_model_shader_codec_and_gpu` | `render` | `scoped_shader_rejection_is_reported_and_does_not_poison_later_rendering` | Pass |
| `R2_capacity_distinct_from_model_shader_codec_and_gpu` | `render` | `real_validation_callbacks_leave_default_level_evidence_before_health_polling` | Pass |
| `R2_capacity_distinct_from_model_shader_codec_and_gpu` | `render` | `diagnostics_success_cache_and_unrelated_errors_cannot_reuse_a_budget_refusal` | Pass |
| `R2_capacity_distinct_from_model_shader_codec_and_gpu` | `render` | `malformed_inputs_and_invalid_output_buffers_are_not_capacity_refusals` | Pass |
| `R2_capacity_distinct_from_model_shader_codec_and_gpu` | `ci-default` | `rgba_preview_is_bounded_and_bad_pixels_are_not_a_memory_refusal` | Pass |
| `R3_idle_first_bounded_proxy_target_cooldown_recovery` | `render` | `eviction_precedes_strictly_bounded_quality_steps_and_sixty_is_reported_once` | Pass |
| `R3_idle_first_bounded_proxy_target_cooldown_recovery` | `render` | `recovery_requires_low_samples_completed_frames_and_separate_thirty_second_cooldowns` | Pass |
| `R3_idle_first_bounded_proxy_target_cooldown_recovery` | `render` | `unknown_and_intermediate_readings_hold_quality_and_reset_consecutive_episodes` | Pass |
| `R3_idle_first_bounded_proxy_target_cooldown_recovery` | `render` | `typed_refusals_start_above_the_native_floor_and_keep_existing_bounded_recovery` | Pass |
| `R3_idle_first_bounded_proxy_target_cooldown_recovery` | `render` | `exhaustion_survives_repeated_requests_and_cooldown_probes_only_the_floor` | Pass |
| `R3_idle_first_bounded_proxy_target_cooldown_recovery` | `render` | `recovery_needs_time_and_healthy_frames_and_failed_probe_restores_working_quality` | Pass |
| `R3_idle_first_bounded_proxy_target_cooldown_recovery` | `ci-default` | `actual_session_evicts_idle_before_downsampling_and_preserves_front_project_and_full_capture` | Pass |
| `R3_idle_first_bounded_proxy_target_cooldown_recovery` | `ci-default` | `yuv_proxies_keep_rotation_matrix_range_and_absorb_all_native_chroma_layouts` | Pass |
| `R4_last_front_actual_quality_original_authored_state` | `render` | `failed_candidate_preserves_front_pixels_then_lower_resolution_commits_and_reuses_two_targets` | Pass |
| `R4_last_front_actual_quality_original_authored_state` | `desktop-ffmpeg` | `native_failed_reattach_keeps_original_factory_front_and_default_error_channels` | Pass |
| `R4_last_front_actual_quality_original_authored_state` | `desktop-ffmpeg` | `actual_ui_atlas_refusal_recovers_in_worker_without_a_second_upload` | Pass |
| `R5_frozen_full_quality_exports_actionable_refusal` | `media` | `frozen_png_shares_live_session_admission_and_recovers_at_full_quality` | Pass |
| `R5_frozen_full_quality_exports_actionable_refusal` | `media` | `png_uses_frozen_fractional_frame_and_all_samples_without_preview_mutation` | Pass |
| `R5_frozen_full_quality_exports_actionable_refusal` | `media` | `frozen_movie_shares_live_capacity_and_keeps_admission_after_session_close` | Pass |
| `R5_frozen_full_quality_exports_actionable_refusal` | `media` | `frozen_movie_keeps_cpu_decode_allowance_across_live_changes_and_session_close` | Pass |
| `R5_frozen_full_quality_exports_actionable_refusal` | `desktop-ffmpeg` | `session_budget_refusal_reaches_real_export_worker_reply_file_and_stderr` | Pass |
| `R5_frozen_full_quality_exports_actionable_refusal` | `desktop-ffmpeg` | `actual_movie_request_survives_freezing_and_engine_exit_in_all_frame_windows` | Pass |
| `R6_unified_source_mask_snapshot_and_explicit_cpu_limits` | `render` | `invalid_policy_is_atomic_and_mask_planning_uses_the_configured_budget` | Pass |
| `R6_unified_source_mask_snapshot_and_explicit_cpu_limits` | `render` | `configured_full_png_can_cross_the_previous_128_mib_result_limit` | Pass |
| `R6_unified_source_mask_snapshot_and_explicit_cpu_limits` | `render` | `actual_cpu_refusal_preserves_target_and_recovers_proxy_then_original_pixels` | Pass |
| `R6_unified_source_mask_snapshot_and_explicit_cpu_limits` | `ci-default` | `cpu_asset_entry_points_follow_session_policy_without_changing_legacy_pixels_or_history` | Pass |
| `A1_injected_low_capacity_finite_attempts_original_faults` | `render` | `native_presenter_refuses_second_uniform_atomically_then_admits_exact_capacity` | Pass |
| `A1_injected_low_capacity_finite_attempts_original_faults` | `render` | `exhaustion_survives_repeated_requests_and_cooldown_probes_only_the_floor` | Pass |
| `A1_injected_low_capacity_finite_attempts_original_faults` | `render` | `guarded_preview_refuses_resource_and_shader_errors_while_legacy_diagnostics_remain_available` | Pass |
| `A2_actual_resize_seek_project_replacement_retirement` | `desktop-ffmpeg` | `repeated_native_resize_seek_and_project_switch_retire_capacity_without_pixel_or_history_drift` | Pass |
| `A3_no_authored_history_drift_or_formal_pixel_change` | `desktop-ffmpeg` | `repeated_native_resize_seek_and_project_switch_retire_capacity_without_pixel_or_history_drift` | Pass |
| `A3_no_authored_history_drift_or_formal_pixel_change` | `ci-default` | `admitted_text_matches_legacy_pixels_through_real_atlas_growth_and_multiple_dpis` | Pass |
| `A3_no_authored_history_drift_or_formal_pixel_change` | `media` | `frozen_png_keeps_cpu_decode_allowance_and_preserves_publication_on_refusal` | Pass |
| `O1_start_before_work_bounded_output_and_nonqueued_readout` | `render` | `info_start_precedes_real_decoder_access_and_does_not_require_debug` | Pass |
| `O1_start_before_work_bounded_output_and_nonqueued_readout` | `render` | `blocked_work_is_published_and_readable_without_entering_the_handler` | Pass |
| `O1_start_before_work_bounded_output_and_nonqueued_readout` | `desktop-ffmpeg` | `readout_does_not_queue_behind_a_blocked_worker` | Pass |
| `O1_start_before_work_bounded_output_and_nonqueued_readout` | `desktop-ffmpeg` | `native_ui_refusals_preserve_cause_and_reach_default_file_stderr_without_stdout` | Pass |
| `O1_start_before_work_bounded_output_and_nonqueued_readout` | `desktop-ffmpeg` | `export_window_omissions_escalate_and_recover_in_default_file_and_stderr` | Pass |
| `R6_package_configuration_and_runtime_reads` | `effects` | `editor_and_effect_pngs_share_configured_capacity_and_unique_paths_count_once` | Pass |
| `R6_package_configuration_and_runtime_reads` | `effects` | `configured_package_png_can_cross_the_previous_128_mib_cap` | Pass |
| `R6_package_configuration_and_runtime_reads` | `effects` | `package_refusal_is_atomic_and_registry_reload_uses_the_same_limit` | Pass |
| `R6_package_configuration_and_runtime_reads` | `effects` | `invalid_limits_and_corrupt_pngs_are_not_typed_capacity_refusals` | Pass |
| `R6_package_configuration_and_runtime_reads` | `effects` | `pack_and_check_cli_take_explicit_limits_and_refusal_preserves_existing_output` | Pass |
| `R6_package_configuration_and_runtime_reads` | `render` | `in_memory_package_png_checks_current_capacity_and_keeps_native_codec_errors` | Pass |
| `R6_package_configuration_and_runtime_reads` | `render` | `package_png_runtime_decode_obeys_current_cpu_policy_and_recovers_native_pixels` | Pass |
| `R6_package_configuration_and_runtime_reads` | `ci-default` | `package_install_and_pixel_reads_follow_current_session_cpu_policy_without_authoring_changes` | Pass |
| `R6_package_configuration_and_runtime_reads` | `ci-default` | `actual_package_install_refusal_reaches_default_worker_reply_file_and_stderr_then_recovers` | Pass |
| `R6_package_configuration_and_runtime_reads` | `png-default` | `frozen_package_png_policy_preserves_publication_and_full_pixels_after_session_close` | Pass |
| `R6_package_configuration_and_runtime_reads` | `media` | `frozen_package_png_policy_preserves_publication_and_full_pixels_after_session_close` | Pass |
| `R6_package_configuration_and_runtime_reads` | `media` | `frozen_package_movie_uses_captured_cpu_policy_and_preserves_previous_output_on_refusal` | Pass |
| `B1_host_injected_UI_and_original_dependency_boundary` | `model` | `production_dependencies_keep_data_render_and_media_outside_the_editing_runtime` | Pass |
| `B1_host_injected_UI_and_original_dependency_boundary` | `ui-admission` | `host_injected_validation_keeps_native_cause_without_capacity_retry` | Pass |
| `B1_host_injected_UI_and_original_dependency_boundary` | `ui-admission` | `startup_refusal_is_atomic_and_actual_ui_allocations_have_disjoint_categories` | Pass |
| `B1_host_injected_UI_and_original_dependency_boundary` | `ui-admission` | `refused_atlas_and_two_buffer_growth_preserve_old_pixels_then_recover` | Pass |
| `B1_host_injected_UI_and_original_dependency_boundary` | `ui-admission` | `dropped_painter_remains_in_frame_peak_until_real_gpu_work_retires` | Pass |
| `B1_host_injected_UI_and_original_dependency_boundary` | `ui-admission` | `admitted_text_matches_legacy_pixels_through_real_atlas_growth_and_multiple_dpis` | Pass |
| `B2_original_UI_operation_in_default_error_channels` | `desktop-ui-errors` | `native_ui_refusals_preserve_cause_and_reach_default_file_stderr_without_stdout` | Pass |
| `B2_original_UI_operation_in_default_error_channels` | `desktop-ui-errors` | `real_paint_limit_failure_reaches_reply_file_and_stderr_at_default_level` | Pass |

## Final gate results

Counts are passed / failed / ignored. Existing external-image and physical-audio
probes keep their original ignore conditions. No new ignore or relaxed assertion
was introduced to pass these gates. Raw logs/receipts and native screenshots
remain under artifacts/pr-queue on E:.

| Gate | Result | Raw log SHA-256 |
| --- | --- | --- |
| `model` | 63 / 0 / 0 | `dc36a130022b48907836637e367e84053b6649facbb509fc259d9a9b3a6cd17a` |
| `ui-admission` | 5 / 0 / 0 | `a74c973af45bdd31f5b4695d1aacc4a33244e1a4fa1a2b2115c07b33fd0902ac` |
| `desktop-ui-errors` | 2 / 0 / 0 | `644999146c5dd69d82d4302090a7f590642d0a1f255109c64d133b42d283045a` |
| `effects` | 59 / 0 / 0 | `d20b82d595eb351f62c01c914125f5e25ab9a70bdf5461d4cf77c099985fc1b6` |
| `render` | 477 / 0 / 1 | `29a72de5fc07fed7a926d97694686ecdd356b1ac8438d8402fe5c27d50ff8ba5` |
| `png-default` | 1 / 0 / 0 | `5a9e5d07e52ca42849e06cc7fae90fbf3194351906e55216645cbbff71c1bff8` |
| `media` | 46 / 0 / 0 | `403de33af56daf38e5e8f917bcd3b064b56ffc4489899fcf43a1025553a54df8` |
| `ci-default` | 1065 / 0 / 1 | `eb51b97ba8d8e41cccbf5d6ad205c788c46429f72da418797f99a9d769ad1620` |
| `desktop-ffmpeg` | 780 / 0 / 1 | `5ea44ba3e485e000023d6eb18cacd5cbaa5ee5a646da9c9859dc5cee39d57fbf` |
| `workspace` | Pass (non-test gate) | `1872d7976017d0a9cc2f794320df3c6463443af37201ad4e763d8e7143dbce6e` |
| `release-check` | Pass (non-test gate) | `d5675d9d1267649398a8a82fb79d90490d1f860333416331dbe1c73a3b62a31c` |
| `build-app` | Pass (non-test gate) | `e910f03bb0d56ff146498a68911bdaae322fae9dad6b9678f94d8e52d036c0ec` |
| `gui-contract` | Pass (non-test gate) | `27cf3adf9791a9acb4510c989ac6c855bb9bafe7afcc75e594657b8eb580d8be` |
| `provenance` | Pass (non-test gate) | `6890a2a3839b3e1c6c7fcde4c48766e6f56a09b0e9323685b639864e8e19d150` |
| `styles` | Pass (non-test gate) | `6b56dba2356e479a3c79aaf059629f315d792e69c95710e9e9b093e291d3d6e5` |

## Behavior and actual native evidence

One Session allowance admits enrolled allocations across preview, plugin editors
and frozen output. Fixed categories expose current, lifetime and bounded frame
interval peaks; alias, upload, encoding and real GPU submission lifetimes retain
the same reservation. Native pressure is advisory per pool and uses safe unknown
defaults. Preview evicts idle resources before stepping down image/video proxies
and targets, with bounded attempts, cooldowns, hysteresis and explicit recovery.
Original shader, codec, model and device errors do not become capacity refusals.

The toolkit has no production dependency on the renderer. A host-injected
allocation contract retains the original native owner, upload and submission
ledger, and downcastable primary error. The unchanged model dependency-boundary
assertion and native bridge tests pass on this source.

Presenter uniforms/source and UI buffers/atlas are admitted before replacement.
A refusal keeps the previous accepted graphics/front and primary error. The
actual worker surface and atlas tests run in both default and FFmpeg complete
desktop suites; report hashes cover exact before/after PNG equality, unchanged
authoring and default log channels. Atlas recovery needs no producer resend.
The repeated native lifecycle test covers24 cases across four cycles, two project
roots, three surface extents and three authored frames. Center pixels match
independently frozen PNG references; this is not a claim of full-image equality
for all24 lifecycle cases. Category endpoints and retained capacity reconcile.

CPU image limits are explicit Session runtime policy. File images, masks,
snapshots, package imports and current-policy package PNG reads use their
corresponding configured allowances. Editor/effect package PNGs share a checked
unique-path aggregate; runtime reads apply the allowance per image. The actual
generated8192x4097 package exceeds128MiB, rejects at the conservative default and
packs/loads with configured capacity while preserving every generated pixel.
Real package installation/registry reload/CLI rejection is atomic, and native
render/host reads recover original pixels after allowance restoration.

Frozen PNG/movie jobs capture CPU/category policy while sharing live native
capacity. They retain policy after live changes and Session closure, preserve
previous publication on refusal and keep complete output dimensions, sampling
and authored state. Actual FFmpeg output is decoded for validation. Real worker
commands verify original errors, default Info starts before work, Error in file
and stderr, and stdout isolation. Healthy Present keeps its existing quiet path.

The final GUI gate uses the actual Release FFmpeg binary, the unchanged smoke
workload, existing deadlines/assertions and real default-level application logs.
Binary, screenshots, reports and file logs have source-bound hashes in the
manifest. Compilation alone is not acceptance.

## Functional changes and provenance

Shared admission, atomic replacement, pressure-driven bounded preview and
configurable package/runtime PNG limits are functional changes. Fixed metadata,
default-level causes and in-flight/frame readouts are observations. No panel,
layout, style, interaction, responder, worker timeout/restart or hidden effect
removal is introduced by this branch relative to integrated main. Main's own UI
updates are retained, and their tests run on the final source.
Resolved dependency identities are unchanged. Generated fixture images/HTML and
manifests are independently authored; normal attributed builtins are loaded via
public APIs. DESIGN remediation rows are retained because their panel locations
are not edited by this change.

## Retained attempts and limits

The earlier PR425 Rust/GPU job119960730170 failed the unchanged dependency
boundary assertion on merge checkout67d35521 (head202b73e0 into main8a722097).
Its log download exited0; that is not the job result. The raw failed conclusion,
checkout and log hash are retained in the manifest. Host injection fixes the
production dependency, and the unchanged assertion actually passes on this
accepted source. The earlier full-render process exited without a terminal
receipt; its partial log and interruption record do not establish a full pass
or a known test exit/cause.

The next Ubuntu desktop job120161445106 failed the unchanged default-channel
assertion: file missing UI glyph vertices (763/1/1). The toolkit now restores
operation names at the injected boundary without replacing the native source.
Its original output assertion and stronger startup/growth context assertions pass
on the accepted source. The first log download failed with a TLS timeout; the
second download succeeded and its retained hash supports the actual job failure.
A network observation failure is not a product test result. The matching Windows
job failed the same assertion (785/1/1); its original log is retained. The earlier
source Rust/GPU software-Vulkan job passed, and remains explicitly scoped to
that earlier merge checkout rather than accepting the final desktop source.

The earlier Debug GUI attempt failed the unchanged40-second screenshot deadline
while command logs still progressed. Its failed receipt and partial captures
remain failures. Release acceptance is an independent result, not a relaxed
deadline or relabeled failure. Earlier d92 full desktop759/2/1 remains failed;
isolated passes do not accept it. Earlier missing asset-path, exact category
history and cross-Instance identity fixture failures remain historical failures;
valid Scene constructors were explicitly unwrapped only in generated fixtures.
Original pixel, capacity, state and strict-equality assertions remain.

Capacity is enrolled logical ownership, not physical VRAM, total process RAM,
driver/codec workspace or implicit staging/swapchain residency. Independent
category maxima cannot be summed as one simultaneous peak. Frame metadata is
bounded64 active/64 recent. Native-pressure tests inject advisory values over
real resources; they do not reproduce physical driver exhaustion.

CPU limits apply per image/full source where required; package validation adds
a per-package unique-image total. Policy is not persisted authoring data.
Standalone/startup default loaders remain conservative and explicit-limit APIs
are available; no new automatic registry reload command is added. Legacy raw
Renderer/Presenter entry points are outside Session admission.

The user confirmed delivery for maintained Windows/Linux/macOS desktop on
2026-10-23. Android is unmaintained, outside this delivery, and has no build or
device acceptance. Linux software-Vulkan and macOS Intel desktop suites are
recorded above; macOS ARM execution and physical driver exhaustion are not
accepted by this record. Cross-platform window interaction and the main/tag
release chain remain separate from PR checks. Issue closure is separate.
No entire-issue closure is inferred merely from this local acceptance document.

The following earlier-source attempts retain their original exit disposition.

| Receipt | Source | Original result | Raw log SHA-256 |
| --- | --- | --- | --- |
| `ui-allocation-integrated-v1-categories.exit.json` | `5a2dfaa4` | Pass | `8ab2dc2d3979d95302f010b3fd9f4f53e13bd834609d7979991c6e1eb888f952` |
| `ui-allocation-integrated-v1-decode.exit.json` | `5a2dfaa4` | Pass | `13d48cd471ce4ea07311109d65d8811c29ff8e8a554123853aa90281d67e2cf5` |
| `ui-allocation-integrated-v1-default-check.exit.json` | `5a2dfaa4` | Pass | `851628aaa69c888080140d4f7b033d68ca6b536c801a58e8e175d4b229fab81b` |
| `ui-allocation-integrated-v1-desktop.exit.json` | `5a2dfaa4` | Pass | `b971d8034a81f1743fb13494eb7e7b71f0ef0e705d9dc035b64848538d467162` |
| `ui-allocation-integrated-v1-frame-native.exit.json` | `5a2dfaa4` | Pass | `f6751f0ecf46e6b301b38a852530cd3469814d3385e344dd2cada480ac302027` |
| `ui-allocation-integrated-v1-host-policy.exit.json` | `5a2dfaa4` | Pass | `35bcd1146702163de07d0ffd8ca147f69d270c65fee26af2b7a91e11d34407dc` |
| `ui-allocation-integrated-v1-host-pressure.exit.json` | `5a2dfaa4` | Pass | `f1cdf6a7b788398b34f9a87020b885606af8b0cc971a257f28777af5484a30bd` |
| `ui-allocation-integrated-v1-host-surface.exit.json` | `5a2dfaa4` | Pass | `1cf16e17319957178771b48979a46d391e2575ae6dc85835621e30ebcf0570fc` |
| `ui-allocation-integrated-v1-host.exit.json` | `5a2dfaa4` | Pass | `6d62f0d9e0ec60325390e27cad5777db854aafc1e2c3526ca19df505428b791a` |
| `ui-allocation-integrated-v1-lifecycle.exit.json` | `5a2dfaa4` | Pass | `98fcc9117ba8c49d0d75b8314fe80ddccb9dcfe736160466a988357250691e32` |
| `ui-allocation-integrated-v1-mask-history.exit.json` | `5a2dfaa4` | Pass | `64b73c9d8034728b4b379eb56fff90efe962a3f19a473e00e2eba7eaa08bedca` |
| `ui-allocation-integrated-v1-media.exit.json` | `5a2dfaa4` | Pass | `2b06cae1bb27475028e16dc103c54a9a9421a205f8c214af142fcd7fc37906a0` |
| `ui-allocation-integrated-v1-movie.exit.json` | `5a2dfaa4` | Pass | `558692c81e3a9cd66bbd7ae8f491ba5542fcf5708bbca867f3b03964c3978577` |
| `ui-allocation-integrated-v1-native.exit.json` | `5a2dfaa4` | Pass | `f55c63ee8370ef90979784e760c25d3bcb028057a9c7b3cb4621c5a278eb25d2` |
| `ui-allocation-integrated-v1-png-default.exit.json` | `5a2dfaa4` | Pass | `df19507eb85d8252624046939ed68326b02df96c32e013850c9d3bbee2a8df25` |
| `ui-allocation-integrated-v1-presenter-check.exit.json` | `5a2dfaa4` | Pass | `1f85597ce158c387b27cee3067f70a43a37f0999bed90539318348b0a1ee18bb` |
| `ui-allocation-integrated-v1-presenter-native.exit.json` | `5a2dfaa4` | Pass | `b1ee65690715af48503f31af86c61126230a20420c417a2a5df8334a46e964af` |
| `ui-allocation-integrated-v1-presenter-source.exit.json` | `5a2dfaa4` | Pass | `034f59bda57859e5cca4932cd44cf28395b6f3c38f71b8468255367db9fc63bf` |
| `ui-allocation-integrated-v1-render.exit.json` | `5a2dfaa4` | Pass | `bc02a6a81ae7f69bf13bc1d9796c81d41272343d46a73db1502caf97f94e6507` |
| `ui-allocation-integrated-v1-start.exit.json` | `5a2dfaa4` | Pass | `b8240a790c6e6671aaa08df9c00fcd887817fab3e907deee2f53a42971fcdbff` |
| `ui-allocation-integrated-v1-styles.exit.json` | `5a2dfaa4` | Pass | `6b56dba2356e479a3c79aaf059629f315d792e69c95710e9e9b093e291d3d6e5` |
| `ui-allocation-integrated-v1-ui-check.exit.json` | `5a2dfaa4` | Pass | `1acf690d3f09b22c0f92ae5822c08e37e1dd92b5f1e59547480dd8586c580415` |
| `ui-allocation-integrated-v1-ui-native.exit.json` | `5a2dfaa4` | Pass | `31b21195044ba33bac86f6dd0878d94225109fa8ad54572d16e2f678e6d4214c` |
| `ui-allocation-integrated-v1-ui-output.exit.json` | `5a2dfaa4` | Pass | `0810b16fb7efe4afb792bcedcc4e1c7013d90bab10665d89a7eeee855ed63283` |
| `ui-allocation-integrated-v1-ui.exit.json` | `5a2dfaa4` | Pass | `098f60e71681c1fd50b1ba51f7f09530fa2a99b75fd3a28d69d14a27e9de8d35` |
| `ui-allocation-integrated-v1-worker-atlas.exit.json` | `5a2dfaa4` | Pass | `ebc0f6d0a1765d45118aba3751dd5b409c4eb1e58c823e9ebdf0fd0233b485b8` |
| `ui-allocation-integrated-v1-worker-surface.exit.json` | `5a2dfaa4` | Pass | `a1fee4950f740d86f37124667b04e5aa7613cd2717a588ade98bc85adae77b67` |
| `ui-allocation-integrated-v1-workspace.exit.json` | `5a2dfaa4` | Pass | `b80144446ee5a6a1b67edd1b3f5f9d4a01dfeefe27b7485428d0fe017271fa24` |
| `issue61-final-build-app.exit.json` | `5a2dfaa4` | Pass | `c7d6c776aea289b4abe54aa63aba66a0933a94a873b46a04fa64d932ecb81970` |
| `issue61-final-ci-default.exit.json` | `5a2dfaa4` | Pass | `3b530f72be63100fdaeaf7f85d3a4b098e92fae9c543284924eceb4426ad246d` |
| `issue61-final-gui-contract.exit.json` | `5a2dfaa4` | Failed | `6dcd22427d2d5e34299bd082ae8aa58a319b9a603ea717d970de496f4f4e0fe6` |
| `issue61-final-release-check.exit.json` | `5a2dfaa4` | Pass | `0a5e988802816d3687d7cc21806c1d8f2ee424d762733c5117045554bd62813f` |
| `issue61-release-gui-v2-build-app.exit.json` | `5a2dfaa4` | Pass | `bc1855843d4a58b4b50ecac90154075436e40ba5ee1e8843d2189d54506361e9` |
| `issue61-release-gui-v2-gui-contract.exit.json` | `5a2dfaa4` | Pass | `27cf3adf9791a9acb4510c989ac6c855bb9bafe7afcc75e594657b8eb580d8be` |
| `issue61-release-gui-v2-provenance.exit.json` | `5a2dfaa4` | Pass | `58a0f028b6cd16d62d9b83553611cb77751146bb43c77bebecac7a1585abf347` |
| `issue61-delivery-v1-effects.exit.json` | `202b73e0` | Pass | `b94a4eae7db37c435f560c4e81e767c0f9473d62a44ffeb57dbcdb5c60c9274a` |
| `issue61-delivery-v2-effects.exit.json` | `df0b6604` | Pass | `d65afd8fe0942c4dd8ea56f70fe464493cd7783b585b3b967a40faa5fb49f90d` |
| `issue61-delivery-v2-model.exit.json` | `df0b6604` | Pass | `6a7b49a7dcc84196e034fe648e8d60941ce8322addca9e2b2547b13d16faac5f` |
| `issue61-delivery-v2-render.exit.json` | `df0b6604` | Pass | `780724825c6f52b06d3cb562b8af7a4bae57badfe2267b5e3fa5162b5afd40ed` |
| `issue61-delivery-v2-ui-admission.exit.json` | `df0b6604` | Pass | `c8f23b6a6edc28571c3fd7590892618ff098207cd3d6ed3758b82967553daaee` |
