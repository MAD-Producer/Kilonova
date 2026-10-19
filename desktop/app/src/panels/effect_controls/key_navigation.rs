use super::*;
use crate::panels::key_navigation::{self as navigation, Choice};
#[cfg(test)]
mod selection_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_navigation_parameters_reach_default_file_and_error_stderr_without_stdout() {
        const CHILD: &str = "MOTION_EFFECT_KEY_NAV_OBSERVATION_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logging = crate::logging::init(root.clone()).unwrap();
            let channel = Channel {
                object: 7,
                effect: 1,
                parameter: "amount".into(),
                context: ("fixture".into(), "main".into()),
            };
            let healthy = EffectParam {
                kind: motion_effects::ParamKind::Float,
                animatable: true,
                min: 0.,
                max: 100.,
                implemented: true,
                default: [0.; 4],
                track: motion_core::Track::constant([0.; 4]),
                curve: None,
            };
            let mut state = serde_json::json!({"root":"fixture", "composition":"main", "frame":12, "project":{"frames":120, "layers":[{"id":7, "effects":[{"id":1, "params":{"amount":healthy}}]}]}});
            let input = Input {
                keys: vec![(Key::K, Default::default())],
                ..Default::default()
            };
            let mut controls = Controls::default();
            let mut out = Vec::new();
            controls.key_channel = Some(channel.clone());
            assert!(controls.navigate_keys(&state, Some(7), &input, &mut out));
            assert!(out.is_empty());
            assert!(std::fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .is_empty());
            let good = state["project"]["layers"][0]["effects"][0]["params"]["amount"].clone();
            for _ in 0..120 {
                state["project"]["layers"][0]["effects"][0]["params"]["amount"] = 17.into();
                assert!(!controls.navigate_keys(&state, Some(7), &input, &mut out));
                state["project"]["layers"][0]["effects"][0]["params"]["amount"] = good.clone();
                assert!(controls.navigate_keys(&state, Some(7), &input, &mut out));
            }
            assert!(out.is_empty());
            // This existing panel observer counts per call site for the process
            // lifetime; healthy neighbours do not reset malformed-row evidence.
            let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            let records: Vec<_> = text
                .lines()
                .filter(|line| line.contains("site=effects.navigation_parameter "))
                .collect();
            assert_eq!(records.len(), 2, "{text}");
            assert!(records[0].contains("WARN") && records[0].contains("count=1 "));
            assert!(records[1].contains("ERROR") && records[1].contains("count=60 "));
            assert!(records
                .iter()
                .all(|line| line.contains("object=7 effect=1 param=amount")
                    && line.contains("reason=")));
            state["project"]["layers"] = serde_json::json!([]);
            assert!(!controls.navigate_keys(&state, Some(7), &input, &mut out));
            assert!(controls.key_channel.is_none());
            assert_eq!(
                std::fs::read_to_string(root.join("motion-studio.log")).unwrap(),
                text
            );
            return;
        }
        let folder = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "panels::effect_controls::key_navigation::tests::malformed_navigation_parameters_reach_default_file_and_error_stderr_without_stdout"])
            .env_remove("MOTION_LOG").env(CHILD,folder.path()).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("persistent panel degradation site=effects.navigation_parameter"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Channel {
    object: u64,
    effect: u64,
    parameter: String,
    context: (String, String),
}

fn times(parameter: &EffectParam, offset: f64) -> Vec<f64> {
    if let Some(curve) = &parameter.curve {
        curve
            .keys
            .iter()
            .map(|key| f64::from(key.frame) + offset)
            .collect()
    } else {
        parameter
            .track
            .keys
            .iter()
            .map(|key| f64::from(key.frame) + offset)
            .collect()
    }
}

impl Controls {
    pub(super) fn choose_parameter(&mut self, state: &Value, object: u64, effect: u64, parameter: &str) {
        self.mirrored_channel = None;
        self.key_channel = Some(Channel { object, effect, parameter: parameter.into(),
            context: (state["root"].as_str().unwrap_or("").into(),
                state["composition"].as_str().unwrap_or("").into()) });
        self.selection_intent = true;
    }

    pub(in crate::panels) fn parameter_selected(&self, state: &Value, object: u64, effect: u64, parameter: &str) -> bool {
        self.key_channel.as_ref().or(self.mirrored_channel.as_ref()).is_some_and(|channel|
            channel.object == object && channel.effect == effect && channel.parameter == parameter
                && channel.context.0 == state["root"].as_str().unwrap_or("")
                && channel.context.1 == state["composition"].as_str().unwrap_or(""))
    }

    pub(in crate::panels) fn take_selection_intent(&mut self) -> bool {
        std::mem::take(&mut self.selection_intent)
    }

    pub(in crate::panels) fn has_key_channel(&self) -> bool {
        self.key_channel.is_some()
    }

    pub(in crate::panels) fn clear_key_channel(&mut self) {
        self.key_channel = None;
        self.mirrored_channel = None;
        self.selection_intent = false;
    }

    pub(in crate::panels) fn mirror_selection_from(&mut self, source: &Self) {
        if self.key_channel.is_none() && self.mirrored_channel == source.key_channel {
            return;
        }
        self.clear_key_channel();
        self.mirrored_channel = source.key_channel.clone();
    }

    pub(in crate::panels) fn navigate_keys(
        &mut self,
        state: &Value,
        selected: Option<u64>,
        input: &Input,
        out: &mut Vec<Action>,
    ) -> bool {
        let Some(channel) = self.key_channel.clone() else {
            return false;
        };
        let context = (
            state["root"].as_str().unwrap_or("").to_owned(),
            state["composition"].as_str().unwrap_or("").to_owned(),
        );
        let source = state["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|layer| layer["id"].as_u64() == Some(channel.object));
        let source = source.and_then(|layer| {
            layer["effects"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|effect| effect["id"].as_u64() == Some(channel.effect))
                .and_then(|effect| effect["params"].get(&channel.parameter))
                .map(|parameter| (layer, parameter))
        });
        let Some((layer, parameter)) =
            source.filter(|_| selected == Some(channel.object) && context == channel.context)
        else {
            self.clear_key_channel();
            return false;
        };
        let Some(parameter) = observe!(
            "effects.navigation_parameter",
            serde_json::from_value::<EffectParam>(parameter.clone()),
            "object={} effect={} param={}",
            channel.object,
            channel.effect,
            channel.parameter
        )
        .ok() else {
            return false;
        };
        let times = times(
            &parameter,
            layer["timeline"]["offset_frame"].as_f64().unwrap_or(0.),
        );
        let mut at = state["frame"].as_f64().unwrap_or(0.);
        let last = crate::panels::last_frame(state);
        let mut used = false;
        for (key, modifiers, _) in input.key_presses().filter(|(key, modifiers, _)| {
            matches!(key, Key::J | Key::K)
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.shift
        }) {
            let _ = modifiers;
            used = true;
            let adjacent = navigation::neighbours(times.iter().copied(), at, last);
            if let Some(frame) = if key == Key::J {
                adjacent.previous
            } else {
                adjacent.next
            } {
                at = frame;
                out.push(Action::Seek(frame));
            }
        }
        used
    }
}

pub(super) fn parameter_navigation(
    key_channel: &mut Option<Channel>,
    list: &mut PaintList,
    row: Rect,
    state: &Value,
    input: &Input,
    object: u64,
    effect: u64,
    param: &str,
    parameter: &EffectParam,
    local: f64,
    locked: bool,
    out: &mut Vec<Action>,
) -> bool {
    let at = state["frame"].as_f64().unwrap_or(0.);
    let last = crate::panels::last_frame(state);
    let adjacent = navigation::neighbours(times(parameter, at - local), at, last);
    let frame = navigation::editable_frame(at, last);
    let editable = !locked
        && state["observing"] != true
        && parameter.animatable
        && parameter.implemented
        && frame.is_some();
    let rect = navigation::rail(&list.styles(), row);
    let choice = navigation::draw(list, rect, input, adjacent, editable);
    let picked = input.pressed.is_some_and(|(button, point)|
        button == MouseButton::Left && row.contains(point));
    if choice.is_none() && !picked { return false; }
    *key_channel = Some(Channel {
        object,
        effect,
        parameter: param.into(),
        context: (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
        ),
    });
    let Some(choice) = choice else { return true; };
    match choice {
        Choice::Seek(frame) => out.push(Action::Seek(frame)),
        Choice::Toggle => {
            let Some(frame) = frame.filter(|_| editable) else {
                return true;
            };
            if adjacent.current {
                out.push(Action::Edit(Command::Effect {
                    object,
                    action: EffectAction::DeleteKey {
                        effect,
                        param: param.into(),
                        frame,
                    },
                }));
            } else {
                let value = if let Some(curve) = &parameter.curve {
                    let mut value = curve::value_at(curve, local);
                    value.sampled_lut = Some(curve.sample(local).to_vec());
                    EffectAction::SetCurveObject {
                        effect,
                        param: param.into(),
                        frame,
                        value,
                    }
                } else {
                    EffectAction::Set {
                        effect,
                        param: param.into(),
                        frame,
                        value: parameter.sample(local),
                    }
                };
                out.push(Action::EditBatch(vec![
                    Command::Effect {
                        object,
                        action: EffectAction::Animate {
                            effect,
                            param: param.into(),
                            frame,
                            enabled: true,
                        },
                    },
                    Command::Effect {
                        object,
                        action: value,
                    },
                ]));
            }
        }
    }
    true
}
