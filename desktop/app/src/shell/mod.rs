//! Shell orchestrators. Panels emit actions; only the shell owns windows and workers.
mod acceptance;
pub(crate) mod actions;
mod lifecycle;
mod media;
mod nodes;
mod presentation;

pub(crate) mod styles;

mod workspaces;

mod design_acceptance;
