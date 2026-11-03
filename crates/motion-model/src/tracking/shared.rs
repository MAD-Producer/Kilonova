//! One immutable decoded document shared by project, history and frozen frames.
//! The managed attachment separately retains the byte-exact original JSON.
use super::{asset::Asset, encode};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, ops::Deref, sync::Arc};

#[derive(Clone, PartialEq)]
pub struct SharedData(Arc<Asset>);
impl SharedData {
    pub fn new(asset: Asset) -> Result<Self, encode::Error> {
        // Admission bounds the typed value and its canonical transport once.
        encode::to_vec(&asset)?;
        Ok(Self(Arc::new(asset)))
    }
    pub fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Deref for SharedData {
    type Target = Asset;
    fn deref(&self) -> &Asset {
        &self.0
    }
}
impl fmt::Debug for SharedData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedTrackingData")
            .field("frames", &self.source_pts.len())
            .field("tracks", &self.tracks.len())
            .finish()
    }
}
impl Serialize for SharedData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_ref().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for SharedData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(Asset::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
