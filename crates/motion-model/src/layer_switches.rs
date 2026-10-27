//! Authored composition-local output switches. List filtering remains workspace state.
use crate::{Content, Error, Layer, Project, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerSwitch {
    Solo,
    Effects,
    Video,
    Audio,
}

impl Project {
    pub fn solo_active(&self) -> bool {
        self.layers.iter().any(|layer| layer.solo)
    }

    pub fn supports_layer_switch(&self, layer: &Layer, switch: LayerSwitch) -> bool {
        match switch {
            LayerSwitch::Solo => !matches!(layer.content, Content::Camera { .. } | Content::Null),
            LayerSwitch::Effects => effects_supported(layer),
            // A camera's existing eye controls camera activation; a null's eye
            // controls its editor handles. Neither becomes a raster output.
            LayerSwitch::Video => !matches!(layer.content, Content::Audio { .. }),
            LayerSwitch::Audio => {
                matches!(layer.content, Content::Composition { .. })
                    || self.layer_audio(layer).is_some()
            }
        }
    }

    pub fn set_layer_switch(
        &mut self,
        object: u64,
        switch: LayerSwitch,
        enabled: bool,
    ) -> Result<()> {
        let layer = self
            .layers
            .iter()
            .find(|layer| layer.id == object)
            .ok_or(Error::Missing(object))?;
        if layer.locked {
            return Err(Error::Locked(object));
        }
        crate::ensure(
            self.supports_layer_switch(layer, switch),
            &format!("layer switch {switch:?} unavailable for object {object}"),
        )?;
        let layer = self.layer_mut(object)?;
        match switch {
            LayerSwitch::Solo => layer.solo = enabled,
            LayerSwitch::Effects => layer.effects_enabled = enabled,
            LayerSwitch::Video => layer.visible = enabled,
            LayerSwitch::Audio => match &mut layer.content {
                Content::Audio { audio } => audio.muted = !enabled,
                Content::Video { video } => video.muted = !enabled,
                Content::Composition { clip } => clip.muted = !enabled,
                _ => {
                    return Err(Error::Invalid(format!(
                        "object {object} lost its audio source"
                    )))
                }
            },
        }
        Ok(())
    }
}

fn effects_supported(layer: &Layer) -> bool {
    !matches!(
        layer.content,
        Content::Camera { .. } | Content::Audio { .. } | Content::Null
    )
}

pub(crate) fn validate(layer: &Layer) -> Result<()> {
    crate::ensure(
        !layer.solo || !matches!(layer.content, Content::Camera { .. } | Content::Null),
        &format!("object {} cannot be soloed", layer.id),
    )?;
    crate::ensure(
        layer.effects_enabled || effects_supported(layer),
        &format!("object {} has no effect bypass switch", layer.id),
    )
}
