//! Shell orchestrators. Panels emit actions; only the shell owns windows and workers.
mod acceptance;
pub(crate) mod actions;
mod lifecycle;
mod media;
pub(crate) mod audio_analysis;
mod nodes;
mod presentation;
pub(crate) mod presentation_observation;

pub(crate) mod styles;

mod workspaces;
mod workspace_acceptance;

mod design_acceptance;
