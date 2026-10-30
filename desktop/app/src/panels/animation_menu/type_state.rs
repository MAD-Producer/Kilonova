//! Inspect only selected authored tracks. A revision-scoped cache avoids
//! decoding long tracks again for each menu row or while the menu is idle.
use super::*;
use motion_core::{KeyframeSelection, KeyframeTarget, Track, Tween};
use serde::de::DeserializeOwned;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Linear,
    Hold,
    In,
    Out,
    InOut,
    Curve,
    Automatic,
    Mixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Summary {
    count: usize,
    automatic: usize,
    kind: Option<Kind>,
    enable: bool,
    freeze: bool,
}

impl Summary {
    pub(super) fn available(self, enabled: bool) -> bool {
        if enabled {
            self.enable
        } else {
            self.freeze
        }
    }
    pub(super) fn mixed_automatic(self) -> bool {
        self.automatic > 0 && self.automatic < self.count
    }
    pub(super) fn checked(self, operation: Operation) -> bool {
        matches!(
            (operation, self.kind),
            (Operation::Automatic(true), Some(Kind::Automatic))
                | (Operation::Interpolation(Ease::Linear), Some(Kind::Linear))
                | (Operation::Interpolation(Ease::Hold), Some(Kind::Hold))
        )
    }
    pub(super) fn title(self, locale: Locale) -> String {
        let zh = locale == Locale::Zh;
        let name = match (self.kind, zh) {
            (None, true) => "无有效关键帧",
            (None, false) => "No valid keys",
            (Some(Kind::Linear), true) => "线性",
            (Some(Kind::Linear), false) => "Linear",
            (Some(Kind::Hold), true) => "保持",
            (Some(Kind::Hold), false) => "Hold",
            (Some(Kind::In), true) => "缓入",
            (Some(Kind::In), false) => "Ease in",
            (Some(Kind::Out), true) => "缓出",
            (Some(Kind::Out), false) => "Ease out",
            (Some(Kind::InOut), true) => "双侧缓动",
            (Some(Kind::InOut), false) => "Ease in and out",
            (Some(Kind::Curve), true) => "手动曲线",
            (Some(Kind::Curve), false) => "Manual curve",
            (Some(Kind::Automatic), true) => "自动贝塞尔",
            (Some(Kind::Automatic), false) => "Automatic Bezier",
            (Some(Kind::Mixed), true) => "混合",
            (Some(Kind::Mixed), false) => "Mixed",
        };
        // Manual timing is stored on the outgoing interval. Automatic policy
        // belongs to the key; do not call a whole interval a per-key type.
        if self.kind == Some(Kind::Automatic) {
            if zh {
                format!("时间约束：{name}")
            } else {
                format!("Temporal policy: {name}")
            }
        } else if zh {
            format!("出侧插值：{name}")
        } else {
            format!("Outgoing interpolation: {name}")
        }
    }
}

#[derive(Default)]
pub(crate) struct Cache {
    entry: Option<(MenuScope, Summary)>,
    #[cfg(test)]
    pub(super) solves: usize,
}

pub(super) fn summary(panels: &Panels, state: &Value) -> Summary {
    let scope = MenuScope::of(panels, state);
    let mut cache = panels.animation_type_cache.borrow_mut();
    // A missing revision cannot establish immutable authored data. Capability
    // does not include read-only/focus gates: enabled() rechecks those live.
    let versioned = state["revision"].is_u64() && state["document_generation"].is_u64();
    if versioned {
        if let Some((old, result)) = &cache.entry {
            if old == &scope {
                return *result;
            }
        }
    }
    let inspected = inspect(state, &scope.keys);
    let result = inspected.unwrap_or_default();
    #[cfg(test)]
    {
        cache.solves += 1;
    }
    // Never memoize malformed data as a healthy disabled capability: repeated
    // observations must still reach the existing bounded degradation threshold.
    cache.entry = (versioned && inspected.is_some()).then_some((scope, result));
    result
}

fn capability<T: Tween + DeserializeOwned>(
    source: &Value,
    frames: &[i32],
    object: u64,
) -> Option<[bool; 2]> {
    let track: Track<T> = observe!(
        "animation.types.track",
        serde_json::from_value(source.clone()),
        "object={object} phase=type_capability value_type={}",
        std::any::type_name::<T>()
    )
    .ok()?;
    // Unsupported type/curve combinations are normal disabled capabilities,
    // not faults. Malformed authored track data is observed separately.
    observe!(
        "animation.types.validity",
        track.validate_local(),
        "object={object} phase=type_capability"
    )
    .ok()?;
    Some([
        track.validate_auto_bezier_edit(frames, true).is_ok(),
        track.validate_auto_bezier_edit(frames, false).is_ok(),
    ])
}

fn inspect(state: &Value, selections: &[KeyframeSelection]) -> Option<Summary> {
    let mut result = Summary {
        enable: true,
        freeze: true,
        ..Default::default()
    };
    for selected in selections {
        let (source, offset) =
            super::super::key_selection::authored_track(state, &selected.target)?;
        let keys = observe!(
            "animation.types.keys",
            source["keys"]
                .as_array()
                .ok_or("authored key array missing"),
            "object={} phase=type_capability",
            selected.target.object()
        )
        .ok()?;
        observe!(
            "animation.types.frames",
            if keys.iter().all(|key| key["frame"]
                .as_i64()
                .is_some_and(|frame| i32::try_from(frame).is_ok()))
                && keys.windows(2).all(|pair| pair[0]["frame"].as_i64() < pair[1]["frame"].as_i64())
            {
                Ok(())
            } else {
                Err("authored key frames must be strictly ordered signed 32-bit integers")
            },
            "object={} phase=type_capability",
            selected.target.object()
        )
        .ok()?;
        let mut frames = Vec::with_capacity(selected.frames.len());
        for frame in &selected.frames {
            let local = i32::try_from(i64::from(*frame).checked_sub(offset)?).ok()?;
            // Missing keys are normal stale selection, with no fault record.
            let index = keys
                .binary_search_by(|key| {
                    key["frame"]
                        .as_i64()
                        .unwrap_or(i64::MIN)
                        .cmp(&i64::from(local))
                })
                .ok()?;
            let key = &keys[index];
            let automatic = key["auto_bezier"] == true;
            let kind = if automatic {
                Kind::Automatic
            } else if key["curve"].is_object() {
                Kind::Curve
            } else {
                match observe!(
                    "animation.types.ease",
                    serde_json::from_value::<Ease>(key["ease"].clone()),
                    "object={} frame={frame} phase=type_capability",
                    selected.target.object()
                )
                .ok()?
                {
                    Ease::Linear => Kind::Linear,
                    Ease::Hold => Kind::Hold,
                    Ease::In => Kind::In,
                    Ease::Out => Kind::Out,
                    Ease::InOut => Kind::InOut,
                }
            };
            result.kind = Some(match result.kind {
                None => kind,
                Some(prior) if prior == kind => kind,
                _ => Kind::Mixed,
            });
            result.count += 1;
            result.automatic += usize::from(automatic);
            frames.push(local);
        }
        let mut discrete = false;
        if let KeyframeTarget::Effect {
            object,
            effect,
            param,
        } = &selected.target
        {
            let layer = super::super::keyframe_clipboard::layer(state, *object)?;
            let parameter = layer["effects"]
                .as_array()?
                .iter()
                .find(|e| e["id"] == *effect)?["params"]
                .get(param)?;
            discrete = matches!(parameter["kind"].as_str(), Some("bool" | "enum"));
        }
        let object = selected.target.object();
        let possible = if discrete {
            [false; 2]
        } else {
            match &source["value"] {
                Value::Number(_) => capability::<f32>(source, &frames, object)?,
                Value::Array(values) => match values.len() {
                    2 => capability::<[f32; 2]>(source, &frames, object)?,
                    3 => capability::<[f32; 3]>(source, &frames, object)?,
                    4 => capability::<[f32; 4]>(source, &frames, object)?,
                    _ => [false; 2],
                },
                _ => [false; 2],
            }
        };
        result.enable &= possible[0];
        result.freeze &= possible[1];
    }
    result.enable &= result.count > result.automatic;
    result.freeze &= result.automatic > 0;
    Some(result)
}

#[cfg(test)]
mod tests;

#[cfg(all(test, windows, feature = "gpu-tests"))]
mod native_tests;
