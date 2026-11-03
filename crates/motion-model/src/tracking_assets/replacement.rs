//! One authoring transaction for a new plate, its original solves and explicit
//! binding changes. This validates metadata only: the host must verify encoded
//! bytes, decoded dimensions/PTS and managed originals before publication.
use crate::{
    ensure,
    media_assets::{self, AssetKind, AssetRef},
    tracking::{camera::CameraBinding, plane::PlaneBinding, point::PointBinding},
    AudioAsset, Project, Result, TrackingAsset, VideoAsset,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_BINDING_UPDATES: usize = 1024;

fn nullable_binding<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// Null explicitly detaches a binding; omitting it is a malformed request.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingUpdate {
    Point {
        composition: String,
        object: u64,
        #[serde(deserialize_with = "nullable_binding")]
        binding: Option<PointBinding>,
    },
    Plane {
        composition: String,
        object: u64,
        #[serde(deserialize_with = "nullable_binding")]
        binding: Option<PlaneBinding>,
    },
    Camera {
        composition: String,
        object: u64,
        #[serde(deserialize_with = "nullable_binding")]
        binding: Option<CameraBinding>,
    },
}
impl BindingUpdate {
    fn address(&self) -> (&str, u64, u8) {
        match self {
            Self::Point {
                composition,
                object,
                ..
            } => (composition, *object, 0),
            Self::Plane {
                composition,
                object,
                ..
            } => (composition, *object, 1),
            Self::Camera {
                composition,
                object,
                ..
            } => (composition, *object, 2),
        }
    }
    fn apply(&self, project: &mut Project, replaced: &HashSet<u64>) -> Result<()> {
        match self {
            Self::Point {
                object, binding, ..
            } => {
                let layer = project.layer_mut(*object)?;
                ensure(!layer.locked, "tracked replacement point layer is locked")?;
                let asset = binding
                    .as_ref()
                    .map(|b| b.asset)
                    .or_else(|| layer.tracking_point.as_ref().map(|b| b.asset));
                ensure(
                    asset.is_some_and(|id| replaced.contains(&id)),
                    "point binding must address a replaced solve",
                )?;
                layer.tracking_point = binding.clone();
            }
            Self::Plane {
                object, binding, ..
            } => {
                let layer = project.layer_mut(*object)?;
                ensure(!layer.locked, "tracked replacement plane layer is locked")?;
                let asset = binding
                    .as_ref()
                    .map(|b| b.asset)
                    .or_else(|| layer.tracking_plane.as_ref().map(|b| b.asset));
                ensure(
                    asset.is_some_and(|id| replaced.contains(&id)),
                    "plane binding must address a replaced solve",
                )?;
                layer.tracking_plane = binding.clone();
            }
            Self::Camera {
                object, binding, ..
            } => {
                let camera = project.camera_for_mut(*object)?;
                let asset = binding
                    .as_ref()
                    .map(|b| b.asset)
                    .or_else(|| camera.tracking.as_ref().map(|b| b.asset));
                ensure(
                    asset.is_some_and(|id| replaced.contains(&id)),
                    "camera binding must address a replaced solve",
                )?;
                camera.tracking = binding.clone();
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replacement {
    pub video: VideoAsset,
    #[serde(deserialize_with = "nullable_binding")]
    pub audio: Option<AudioAsset>,
    pub tracking: Vec<TrackingAsset>,
    pub bindings: Vec<BindingUpdate>,
}
impl Replacement {
    /// Preserve source/attachment IDs and all unmentioned authored corrections.
    /// Return a complete valid candidate without mutating the supplied project.
    pub fn propose(&self, project: &Project) -> Result<Project> {
        project.validate()?;
        ensure(
            !self.tracking.is_empty() && self.tracking.len() <= super::MAX_TRACKING_ASSETS,
            "tracked replacement requires bounded original solves",
        )?;
        ensure(
            self.bindings.len() <= MAX_BINDING_UPDATES,
            "too many tracked replacement bindings",
        )?;
        let source = AssetRef {
            kind: AssetKind::Video,
            id: self.video.id,
        };
        let old = project
            .video_assets
            .iter()
            .find(|a| a.id == self.video.id)
            .ok_or_else(|| crate::Error::Invalid("tracked replacement video is missing".into()))?;
        ensure(
            self.video.path != old.path,
            "tracked replacement must retain the old video file for undo",
        )?;
        let expected: HashSet<_> = project
            .tracking_assets
            .iter()
            .filter(|a| a.source == source)
            .map(|a| a.id)
            .collect();
        let incoming: HashSet<_> = self.tracking.iter().map(|a| a.id).collect();
        ensure(
            incoming.len() == self.tracking.len(),
            "duplicate tracked replacement solve",
        )?;
        ensure(
            !expected.is_empty() && expected == incoming,
            "tracked replacement must replace every solve attached to the video",
        )?;
        for asset in &self.tracking {
            ensure(
                asset.source == source,
                "replacement solve selects a different source",
            )?;
            let previous = project
                .tracking_assets
                .iter()
                .find(|a| a.id == asset.id)
                .ok_or_else(|| crate::Error::Invalid("replacement solve is missing".into()))?;
            ensure(
                previous.path != asset.path || previous == asset,
                "tracked replacement must retain the old original solve for undo",
            )?;
        }
        let mut seen = HashSet::new();
        for update in &self.bindings {
            let address = update.address();
            ensure(
                !address.0.is_empty() && address.0.len() <= 128,
                "invalid tracked replacement composition",
            )?;
            ensure(
                seen.insert(address),
                "duplicate tracked replacement binding",
            )?;
        }
        let mut candidate = project.clone();
        media_assets::apply(
            &mut candidate,
            media_assets::AssetAction::ReplaceVideo {
                video: self.video.clone(),
                audio: self.audio.clone(),
            },
        )?;
        for asset in &self.tracking {
            let destination = candidate
                .tracking_assets
                .iter_mut()
                .find(|a| a.id == asset.id)
                .ok_or_else(|| crate::Error::Invalid("replacement solve is missing".into()))?;
            *destination = asset.clone();
        }
        let active = candidate.composition_id.clone();
        for update in &self.bindings {
            candidate.activate_composition(update.address().0)?;
            update.apply(&mut candidate, &incoming)?;
        }
        candidate.activate_composition(&active)?;
        candidate.validate()?;
        Ok(candidate)
    }
}
