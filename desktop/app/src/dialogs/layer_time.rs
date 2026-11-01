//! Draft layer clocks and source remaps; one guarded batch owns Apply.
use super::*;
use motion_core::{
    layer_time::{self, LayerTimeAction, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch},
    Command, Content, Easing, Layer,
};
use motion_ui::widgets::{Dropdown, ScrollArea, TextEditor};

#[cfg(test)]
mod tests;
mod view;

pub(crate) fn eligible(state: &Value, objects: &[u64]) -> bool {
    !objects.is_empty()
        && state["observing"] != true
        && state["document_generation"].is_u64()
        && state["revision"].is_u64()
        && state["root"].as_str().is_some_and(|root| !root.is_empty())
        && state["composition"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
        && objects.iter().all(|id| {
            state["project"]["layers"].as_array().is_some_and(|layers| {
                layers
                    .iter()
                    .any(|l| l["id"] == *id && l["locked"] == false)
            })
        })
}

struct Field {
    text: String,
    original: String,
    editor: TextEditor,
    replace: bool,
}
impl Field {
    fn new(text: String) -> Self {
        Self {
            original: text.clone(),
            text,
            editor: Default::default(),
            replace: true,
        }
    }
    fn number(&self) -> Result<f64, String> {
        self.text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| "请输入有限数值 / Enter a finite number".into())
    }
}
struct KeyEdit {
    index: usize,
    frame: Field,
    source: Field,
}
pub(crate) struct Editor {
    pub(crate) owner: u64,
    source: crate::layer_clipboard::KeyframeSource,
    layers: Vec<Layer>,
    frames: u32,
    at: f64,
    stretch: Field,
    direction: usize,
    anchor: usize,
    resize: bool,
    remap: usize,
    keys: Vec<TimeRemapKey>,
    key_edit: Option<KeyEdit>,
    menus: [Dropdown; 4],
    scroll: ScrollArea,
    focus: Option<usize>,
    error: Option<String>,
}

/// Parse decimal percentages exactly, then reduce before the model's bounded
/// rational constructor. Never persist an approximate reciprocal from f64.
fn percentage(text: &str) -> Result<TimeStretch, String> {
    let text = text.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || !fraction.bytes().all(|c| c.is_ascii_digit())
        || fraction.len() > 6
    {
        return Err(
            "时长须为正数，最多六位小数 / Use a positive percentage with up to six decimals".into(),
        );
    }
    let scale = 10u64.pow(fraction.len() as u32);
    let n = whole
        .parse::<u64>()
        .ok()
        .and_then(|v| v.checked_mul(scale))
        .and_then(|v| {
            v.checked_add(if fraction.is_empty() {
                Some(0)
            } else {
                fraction.parse().ok()
            }?)
        })
        .ok_or_else(|| "时长数值过大 / Duration percentage is too large".to_string())?;
    let d = scale * 100;
    let (mut a, mut b) = (n, d);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    TimeStretch::new(
        u32::try_from(n / a).map_err(|e| e.to_string())?,
        u32::try_from(d / a).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
impl Editor {
    fn remappable(&self) -> bool {
        self.layers.len() == 1
            && matches!(
                self.layers[0].content,
                Content::Video { .. }
                    | Content::Audio { .. }
                    | Content::Composition { .. }
                    | Content::Model { .. }
            )
    }
    fn commit_key(&mut self) -> Result<(), String> {
        if let Some(edit) = &self.key_edit {
            let frame = edit.frame.number()?;
            let source = edit.source.number()?;
            // Construction checks the same signed f64 time bounds as playback.
            TimeRemap::new(vec![TimeRemapKey {
                frame,
                source_frame: source,
                easing: self.keys[edit.index].easing,
            }])
            .map_err(|e| e.to_string())?;
            self.keys[edit.index].frame = frame;
            self.keys[edit.index].source_frame = source;
        }
        Ok(())
    }
    fn select_key(&mut self, index: usize) -> Result<(), String> {
        if self.key_edit.as_ref().is_some_and(|e| e.index == index) {
            return Ok(());
        }
        self.commit_key()?;
        let key = &self.keys[index];
        self.key_edit = Some(KeyEdit {
            index,
            frame: Field::new(key.frame.to_string()),
            source: Field::new(key.source_frame.to_string()),
        });
        self.menus[3].open = false;
        Ok(())
    }
    fn desired_stretch(&self) -> Result<Option<TimeStretch>, String> {
        if self.stretch.text.trim().is_empty() || self.stretch.text == self.stretch.original {
            Ok(None)
        } else {
            percentage(&self.stretch.text).map(Some)
        }
    }
    fn clock_actions(&self, layer: &Layer) -> Result<Vec<LayerTimeAction>, String> {
        let mut actions = vec![];
        if let Some(stretch) = self.desired_stretch()? {
            let range = layer.interval(self.frames);
            let anchor_frame = match self.anchor {
                0 => range.in_frame,
                1 => self.at,
                _ => range.out_frame,
            };
            actions.push(LayerTimeAction::Stretch {
                stretch,
                anchor_frame,
                resize_clip: self.resize,
            });
        }
        let current = layer
            .time_mapping
            .as_ref()
            .map_or(TimeDirection::Forward, |m| m.clock.direction());
        let desired = match self.direction {
            1 => TimeDirection::Forward,
            2 => TimeDirection::Reverse,
            _ => current,
        };
        if desired != current {
            actions.push(LayerTimeAction::Reverse {});
        }
        Ok(actions)
    }
    fn commands(&mut self) -> Result<Vec<Command>, String> {
        if self.remap == 1 {
            self.commit_key()?;
        }
        let remap = if self.remap == 1 {
            if !self.remappable() {
                return Err("请选择一个视频、音频、合成或模型图层 / Select one media, composition or model layer".into());
            }
            let mut keys = self.keys.clone();
            keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
            Some(TimeRemap::new(keys).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let mut commands = vec![];
        for layer in &self.layers {
            let mut candidate = layer.clone();
            let mut actions = self.clock_actions(layer)?;
            if self.remap != 0 {
                actions.push(LayerTimeAction::Remap {
                    remap: remap.clone(),
                });
            }
            let mut changes = vec![];
            for action in actions {
                let before = candidate.clone();
                layer_time::apply(&mut candidate, self.frames, action.clone())
                    .map_err(|e| e.to_string())?;
                if candidate != before {
                    changes.push(Command::LayerTime {
                        object: layer.id,
                        action,
                    });
                }
            }
            if candidate != *layer {
                commands.extend(changes);
            }
        }
        Ok(commands)
    }
    fn add_key(&mut self) -> Result<(), String> {
        self.commit_key()?;
        if self.keys.len() >= layer_time::MAX_TIME_REMAP_KEYS {
            return Err("时间重映射关键帧已达上限 / Remap key limit reached".into());
        }
        let mut layer = self.layers[0].clone();
        for action in self.clock_actions(&layer)? {
            layer_time::apply(&mut layer, self.frames, action).map_err(|e| e.to_string())?;
        }
        let frame = layer
            .checked_local_frame(self.at)
            .map_err(|e| e.to_string())?;
        if self.keys.iter().any(|k| k.frame == frame) {
            return Err("当前图层时间已有关键帧 / A key already occupies this layer time".into());
        }
        let mut sorted = self.keys.clone();
        sorted.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        let value = TimeRemap::new(sorted)
            .and_then(|r| r.sample(frame))
            .map_err(|e| e.to_string())?
            .frame;
        self.keys.push(TimeRemapKey {
            frame,
            source_frame: value,
            easing: Easing::default(),
        });
        self.select_key(self.keys.len() - 1)
    }
}
impl Dialog {
    pub(crate) fn layer_time(
        state: &Value,
        source: crate::layer_clipboard::KeyframeSource,
        objects: Vec<u64>,
        owner: u64,
    ) -> Result<Self, String> {
        if !eligible(state, &objects) || source != crate::layer_clipboard::KeyframeSource::of(state)
        {
            return Err("layer time source is stale, read-only, locked or unavailable".into());
        }
        let frames = state["project"]["frames"]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or("composition duration missing")?;
        let at = state["frame"]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or("playhead time missing")?;
        let mut layers = vec![];
        for object in objects {
            if layers.iter().any(|l: &Layer| l.id == object) {
                return Err("duplicate layer time target".into());
            }
            let value = state["project"]["layers"]
                .as_array()
                .and_then(|ls| ls.iter().find(|l| l["id"] == object))
                .ok_or("layer missing")?;
            let layer: Layer = serde_json::from_value(value.clone())
                .map_err(|e| format!("layer {object}: {e}"))?;
            layer
                .validate_time_mapping(frames)
                .map_err(|e| e.to_string())?;
            layers.push(layer);
        }
        let first = layers[0].time_mapping.clone().unwrap_or_default();
        let stretch = if layers.iter().all(|l| {
            l.time_mapping
                .as_ref()
                .map_or(TimeStretch::default(), |m| m.clock.stretch())
                == first.clock.stretch()
        }) {
            let text = format!("{:.6}", first.clock.stretch().duration_scale() * 100.);
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            String::new()
        };
        let direction = if layers.iter().all(|l| {
            l.time_mapping
                .as_ref()
                .map_or(TimeDirection::Forward, |m| m.clock.direction())
                == first.clock.direction()
        }) {
            if first.clock.direction() == TimeDirection::Forward {
                1
            } else {
                2
            }
        } else {
            0
        };
        let mut keys = first
            .remap
            .as_ref()
            .map_or_else(Vec::new, |r| r.keys().to_vec());
        if keys.is_empty() {
            let range = layers[0].interval(frames);
            for time in [range.in_frame, range.out_frame] {
                let frame = layers[0]
                    .checked_local_frame(time)
                    .map_err(|e| e.to_string())?;
                keys.push(TimeRemapKey {
                    frame,
                    source_frame: frame,
                    easing: Easing::default(),
                });
            }
            keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        }
        let mut dialog = Self::settings(state);
        dialog.layer_time = Some(Editor {
            owner,
            source,
            layers,
            frames,
            at,
            stretch: Field::new(stretch),
            direction,
            anchor: 0,
            resize: true,
            remap: 0,
            keys,
            key_edit: None,
            menus: Default::default(),
            scroll: Default::default(),
            focus: None,
            error: None,
        });
        Ok(dialog)
    }
    pub(crate) fn modal_owner(&self) -> Option<u64> {
        self.timing_owner()
            .or_else(|| self.layer_time.as_ref().map(|e| e.owner))
    }
}
