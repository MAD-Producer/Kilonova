//! Shared media identities and reference preflight. Files are retained for undo.
use crate::{
    ensure, Asset, AudioAsset, Content, EffectImageInput, Layer, Project, Result, VideoAsset,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Image,
    Audio,
    Video,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetRef {
    pub kind: AssetKind,
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "asset",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MediaAsset {
    Image(Asset),
    Audio(AudioAsset),
    Video(VideoAsset),
}

impl MediaAsset {
    pub fn target(&self) -> AssetRef {
        match self {
            Self::Image(a) => AssetRef {
                kind: AssetKind::Image,
                id: a.id,
            },
            Self::Audio(a) => AssetRef {
                kind: AssetKind::Audio,
                id: a.id,
            },
            Self::Video(a) => AssetRef {
                kind: AssetKind::Video,
                id: a.id,
            },
        }
    }
    pub fn path(&self) -> &str {
        match self {
            Self::Image(a) => &a.path,
            Self::Audio(a) => &a.path,
            Self::Video(a) => &a.path,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssetAction {
    Replace {
        source: MediaAsset,
    },
    ReplaceVideo {
        video: VideoAsset,
        audio: Option<AudioAsset>,
    },
    Remove {
        target: AssetRef,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetUsage {
    pub composition: String,
    pub object: u64,
    pub name: String,
    pub role: String,
}

pub fn lookup(project: &Project, target: AssetRef) -> Option<MediaAsset> {
    match target.kind {
        AssetKind::Image => project
            .assets
            .iter()
            .find(|a| a.id == target.id)
            .cloned()
            .map(MediaAsset::Image),
        AssetKind::Audio => project
            .audio_assets
            .iter()
            .find(|a| a.id == target.id)
            .cloned()
            .map(MediaAsset::Audio),
        AssetKind::Video => project
            .video_assets
            .iter()
            .find(|a| a.id == target.id)
            .cloned()
            .map(MediaAsset::Video),
    }
}

/// Includes disabled/off-range layers, text rasters, effect inputs and video audio.
pub fn usages(project: &Project, target: AssetRef) -> Vec<AssetUsage> {
    fn visit(
        project: &Project,
        target: AssetRef,
        comp: &str,
        layers: &[Layer],
        out: &mut Vec<AssetUsage>,
    ) {
        for layer in layers {
            let role = match (&layer.content, target.kind) {
                (Content::Image { asset }, AssetKind::Image) if *asset == target.id => {
                    Some("image")
                }
                (Content::Text { raster_asset, .. }, AssetKind::Image)
                    if *raster_asset == target.id =>
                {
                    Some("text_raster")
                }
                (Content::Audio { audio }, AssetKind::Audio) if audio.asset == target.id => {
                    Some("audio")
                }
                (Content::Video { video }, AssetKind::Video) if video.asset == target.id => {
                    Some("video")
                }
                (Content::Video { video }, AssetKind::Audio)
                    if project
                        .video_assets
                        .iter()
                        .any(|a| a.id == video.asset && a.audio_asset == Some(target.id)) =>
                {
                    Some("video_audio")
                }
                _ => None,
            };
            if let Some(role) = role {
                out.push(AssetUsage {
                    composition: comp.into(),
                    object: layer.id,
                    name: layer.name.clone(),
                    role: role.into(),
                });
            }
            for effect in &layer.effects {
                if target.kind == AssetKind::Image
                    && effect.image_input == Some(EffectImageInput::Asset { asset: target.id })
                {
                    out.push(AssetUsage {
                        composition: comp.into(),
                        object: layer.id,
                        name: layer.name.clone(),
                        role: format!("effect:{}", effect.id),
                    });
                }
            }
        }
    }
    let mut out = Vec::new();
    visit(
        project,
        target,
        &project.composition_id,
        &project.layers,
        &mut out,
    );
    for comp in &project.compositions {
        visit(project, target, &comp.id, &comp.layers, &mut out);
    }
    out
}

pub fn apply(project: &mut Project, action: AssetAction) -> Result<()> {
    match action {
        AssetAction::ReplaceVideo { video, audio } => {
            let video_id = video.id;
            let old_audio = project
                .video_assets
                .iter()
                .find(|a| a.id == video_id)
                .ok_or_else(|| crate::Error::Invalid("replacement video asset missing".into()))?
                .audio_asset;
            ensure(
                video.audio_asset == audio.as_ref().map(|a| a.id),
                "replacement video/audio identities do not match",
            )?;
            if let Some(audio) = audio {
                if let Some(old) = old_audio {
                    ensure(
                        audio.id == old,
                        "replacement must retain the linked audio identity",
                    )?;
                    *project
                        .audio_assets
                        .iter_mut()
                        .find(|a| a.id == old)
                        .ok_or_else(|| {
                            crate::Error::Invalid("linked audio asset missing".into())
                        })? = audio;
                } else {
                    project.audio_assets.push(audio);
                }
            }
            let new_audio = video.audio_asset;
            *project
                .video_assets
                .iter_mut()
                .find(|a| a.id == video_id)
                .ok_or_else(|| crate::Error::Invalid("replacement video asset missing".into()))? =
                video;
            if new_audio.is_none() {
                if let Some(id) = old_audio {
                    apply(
                        project,
                        AssetAction::Remove {
                            target: AssetRef {
                                kind: AssetKind::Audio,
                                id,
                            },
                        },
                    )?;
                }
            }
        }
        AssetAction::Replace { source } => {
            let target = source.target();
            ensure(
                lookup(project, target).is_some(),
                "replacement asset no longer exists",
            )?;
            ensure(
                target.kind != AssetKind::Video,
                "video replacement requires its linked audio preflight",
            )?;
            ensure(
                target.kind != AssetKind::Audio
                    || !project
                        .video_assets
                        .iter()
                        .any(|v| v.audio_asset == Some(target.id)),
                "replace the owning video asset instead of its linked audio",
            )?;
            match source {
                MediaAsset::Image(a) => {
                    *project
                        .assets
                        .iter_mut()
                        .find(|old| old.id == target.id)
                        .ok_or_else(|| crate::Error::Invalid("image asset missing".into()))? = a
                }
                MediaAsset::Audio(a) => {
                    *project
                        .audio_assets
                        .iter_mut()
                        .find(|old| old.id == target.id)
                        .ok_or_else(|| crate::Error::Invalid("audio asset missing".into()))? = a
                }
                MediaAsset::Video(a) => {
                    *project
                        .video_assets
                        .iter_mut()
                        .find(|old| old.id == target.id)
                        .ok_or_else(|| crate::Error::Invalid("video asset missing".into()))? = a
                }
            }
        }
        AssetAction::Remove { target } => {
            ensure(lookup(project, target).is_some(), "asset no longer exists")?;
            let uses = usages(project, target);
            ensure(
                uses.is_empty(),
                &format!("asset is referenced by {} layer/effect uses", uses.len()),
            )?;
            ensure(
                target.kind != AssetKind::Audio
                    || !project
                        .video_assets
                        .iter()
                        .any(|v| v.audio_asset == Some(target.id)),
                "audio asset is owned by a video asset",
            )?;
            match target.kind {
                AssetKind::Image => project.assets.retain(|a| a.id != target.id),
                AssetKind::Audio => project.audio_assets.retain(|a| a.id != target.id),
                AssetKind::Video => project.video_assets.retain(|a| a.id != target.id),
            }
            project.library.remove_item(&match target.kind {
                AssetKind::Image => crate::ProjectItem::Image { id: target.id },
                AssetKind::Audio => crate::ProjectItem::Audio { id: target.id },
                AssetKind::Video => crate::ProjectItem::Video { id: target.id },
            });
        }
    }
    Ok(())
}
