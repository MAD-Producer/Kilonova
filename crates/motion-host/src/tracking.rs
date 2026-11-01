//! External tracking primitives and original-document project attachments.
//! No solver, UI import routing or evaluated pose-track capability is implied.
//! Camera coordinates are +X right, +Y down and +Z forward. Image coordinates
//! are continuous pixels from the upper-left image edge (first pixel center .5).
pub mod asset;
pub mod decode;
pub mod decoded;
pub mod encode;
pub mod plate;
pub mod pose;
pub mod time;
pub mod source;
pub mod project;
pub mod import;
