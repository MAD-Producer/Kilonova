//! Shell orchestrators. Panels emit actions; only the shell owns windows and workers.
mod acceptance;
mod node_naming_acceptance;
mod node_view_acceptance;
mod node_search_acceptance;
pub(crate) mod actions;
pub(crate) mod text_style;
mod render_queue;
mod layer_clipboard;
mod animation_presets;
mod effect_application;
mod effect_drag;
mod lifecycle;
mod media;
#[cfg(all(test, target_os = "windows"))]
mod model_import_tests;
pub(crate) mod project_import;
mod project_drag;
pub(crate) mod media_assets;
mod frame_export;
pub(crate) mod audio_analysis;
mod nodes;
mod node_image_drop;
mod presentation;
#[cfg(test)]
mod audio_preview_tests;
pub(crate) mod pointer_position;
#[cfg(all(test, target_os = "windows"))]
mod native_input_tests;
#[cfg(all(test, target_os = "windows"))]
mod project_precision_tests;
#[cfg(all(test, target_os = "windows", feature = "gpu-tests"))]
mod model_layer_tests;
pub(crate) mod presentation_observation;

pub(crate) mod styles;
mod preferences;
mod keyframe_timing;
pub(crate) mod expressions;
#[cfg(all(test,target_os="windows"))]
mod curve_comparison_tests;
mod preferences_acceptance;
mod shortcut_acceptance;

mod workspaces;
mod panel_maximize;
mod panel_maximize_acceptance;
mod workspace_acceptance;

mod design_acceptance;
mod alignment_acceptance;
mod interaction_acceptance;
mod property_acceptance;
mod shape_resize_acceptance;
mod effect_selection_acceptance;
mod effect_timeline_acceptance;
mod timeline_key_drag_acceptance;
#[cfg(all(test, target_os = "windows"))]
mod path_selection_acceptance;
#[cfg(all(test, target_os = "windows"))]
mod retired_effect_tests;

#[cfg(all(test, target_os = "windows"))]
mod node_image_drop_tests;
