//! Shell orchestrators. Panels emit actions; only the shell owns windows and workers.
mod acceptance;
mod node_naming_acceptance;
mod node_view_acceptance;
mod node_search_acceptance;
pub(crate) mod actions;
mod lifecycle;
mod media;
mod frame_export;
pub(crate) mod audio_analysis;
mod nodes;
mod presentation;
pub(crate) mod presentation_observation;

pub(crate) mod styles;
mod preferences;
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
