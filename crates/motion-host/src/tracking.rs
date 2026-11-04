//! Original tracking IO and transaction admission. Pure contracts live in the model.
pub use motion_core::tracking::{asset, colmap, decode, document, encode, plate, pose, time};
pub mod decoded;
pub mod source;
pub mod project;
pub mod import;
