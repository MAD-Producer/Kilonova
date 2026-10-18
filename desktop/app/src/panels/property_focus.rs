//! Transient selection intents arbitrate existing typed controllers; targets,
//! keys and gesture transactions remain owned by their original controllers.
use super::types::Panels;
use motion_ui::Rect;

pub(super) fn navigation_scope(panel: Option<motion_ui::dock::Panel>) -> bool {
    use motion_ui::dock::Panel;
    matches!(panel, Some(Panel::Timeline | Panel::Curves | Panel::Properties | Panel::EffectControls
        | Panel::Masks | Panel::Vector | Panel::Audio | Panel::Composition))
}

/// Native acceptance opts into a bounded, read-only painted caption probe.
/// Normal editor frames do not retain property geometry here.
#[derive(Clone, Default)]
pub(crate) struct HitProbe {
    enabled: bool,
    rect: Option<Rect>,
}
impl HitProbe {
    pub(crate) fn start(&mut self) {
        self.enabled = true;
        self.rect = None;
    }
    pub(crate) fn rect(&self) -> Option<Rect> {
        self.rect
    }
    pub(in crate::panels) fn record(&mut self, rect: Rect, viewport: Rect) {
        if self.enabled
            && rect.width() > 0.
            && rect.height() > 0.
            && rect.min[0] >= viewport.min[0]
            && rect.min[1] >= viewport.min[1]
            && rect.max[0] <= viewport.max[0]
            && rect.max[1] <= viewport.max[1]
        {
            self.rect = Some(rect);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Domain {
    Transform,
    Mask,
    Shape,
    Audio,
    Effects,
    InspectorEffects,
}

/// Called before keyboard dispatch and after each panel consumes input. Only
/// actual selection commands publish an intent; passive rendering publishes none.
pub(super) fn settle(panels: &mut Panels) {
    let intents = [
        (
            Domain::Transform,
            panels.animation_controls.take_selection_intent(),
        ),
        (
            Domain::Mask,
            panels.path_controls.take_mask_selection_intent(),
        ),
        (
            Domain::Shape,
            panels.path_controls.take_shape_selection_intent(),
        ),
        (Domain::Audio, panels.audio_controls.take_selection_intent()),
        (
            Domain::Effects,
            panels.effect_controls.take_selection_intent(),
        ),
        (
            Domain::InspectorEffects,
            panels.properties.effects.take_selection_intent(),
        ),
    ];
    let Some((owner, _)) = intents.into_iter().rev().find(|(_, chosen)| *chosen) else {
        return;
    };
    if owner != Domain::Transform {
        panels.animation_controls.yield_track();
    }
    if owner != Domain::Mask {
        panels.path_controls.clear_mask_channel();
    }
    if owner != Domain::Shape {
        panels.path_controls.clear_shape_channel();
    }
    if owner != Domain::Audio {
        panels.audio_controls.clear_selection();
    }
    if owner != Domain::Effects {
        panels.effect_controls.clear_key_channel();
    }
    if owner != Domain::InspectorEffects {
        panels.properties.effects.clear_key_channel();
    }
}
