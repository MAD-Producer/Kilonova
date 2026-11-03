//! Original tracking IO and transaction admission. Pure contracts live in the model.
pub use motion_core::tracking::{asset, decode, encode, plate, pose, time};
pub mod decoded;
pub mod source;
pub mod project;
pub mod import;
