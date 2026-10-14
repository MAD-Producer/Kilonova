use super::*;
use crate::engine::Engine;
use motion_core::{AudioAsset, AudioClip, Content, Layer, LayerTimeline, Project};
use motion_ui::input::Modifiers;

struct Harness {
    _folder: tempfile::TempDir,
    engine: Engine,
    panels: Panels,
    atlas: TextAtlas,
    floating: bool,
    state: Value,
}
impl Harness {
    fn new(floating: bool, locale: Locale) -> Self {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        // Independently generated, constant stereo PCM; no device is needed.
        let count = 48000u32 * 4;
        let data_len = count * 4;
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((36 + data_len).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(48000u32.to_le_bytes());
        wav.extend(192000u32.to_le_bytes());
        wav.extend(4u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(data_len.to_le_bytes());
        for _ in 0..count {
            wav.extend(4096i16.to_le_bytes());
            wav.extend(8192i16.to_le_bytes());
        }
        std::fs::write(root.join("assets/tone.wav"), &wav).unwrap();
        let mut project = Project::new(64, 64, 30, 120).unwrap();
        project.audio_assets.push(AudioAsset {
            id: 1,
            path: "assets/tone.wav".into(),
            mime: "audio/wav".into(),
            bytes: wav.len() as u64,
            track: 0,
            sample_rate: 48000,
            channels: 2,
            sample_frames: u64::from(count),
            duration_us: 4_000_000,
        });
        let mut layer = Layer::solid(7, "Stereo", [0.; 2], [0.; 3], [1.; 4]);
        layer.content = Content::Audio {
            audio: AudioClip::new(1),
        };
        layer.timeline = Some(LayerTimeline {
            in_frame: 10,
            out_frame: 120,
            offset_frame: 10,
        });
        project.layers.push(layer);
        motion_core::storage::save(&root, &project).unwrap();
        let engine = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
        let state = engine.seek(10.).unwrap();
        let mut panels = Panels {
            locale,
            ..Default::default()
        };
        panels.select_only(Some(7));
        Self {
            _folder: folder,
            engine,
            panels,
            atlas: TextAtlas::from_system(12.).unwrap(),
            floating,
            state,
        }
    }
    fn area(&self) -> Rect {
        if self.floating {
            Rect::new(0., 30., 400., 648.)
        } else {
            let layout = Node::dock(Panel::Audio.dock());
            frames(&layout, [400., 700.])[0].content
        }
    }
    fn field(&self, control: Control) -> Rect {
        let top = self.area().min[1] + 8.;
        let composition = self.state["project"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|layer| {
                layer["id"].as_u64() == self.panels.selected
                    && layer["content"]["kind"] == "composition"
            });
        row(
            self.area(),
            top + match control {
                Control::Gain => 52.,
                Control::Left => {
                    if composition {
                        52.
                    } else {
                        138.
                    }
                }
                Control::Right => {
                    if composition {
                        114.
                    } else {
                        200.
                    }
                }
            },
        )
        .number
    }
    fn draw(&mut self, input: Input) -> Vec<Action> {
        self.state = self.engine.state().unwrap();
        if self.floating {
            floating_chrome(
                &mut PaintList::default(),
                &mut self.atlas,
                Panel::Audio,
                [400., 700.],
                &mut self.panels,
                &self.state,
                &input,
            )
        } else {
            chrome(
                &mut PaintList::default(),
                &mut self.atlas,
                [400., 700.],
                &Node::dock(Panel::Audio.dock()),
                &mut self.panels,
                &self.state,
                &input,
            )
        }
    }
    fn run(&mut self, actions: Vec<Action>) {
        for action in actions {
            self.state = match action {
                Action::Edit(command) => self
                    .engine
                    .edit(serde_json::to_string(&command).unwrap())
                    .unwrap(),
                Action::History(op) => self.engine.history(op).unwrap(),
                Action::Seek(frame) => self.engine.seek(frame).unwrap(),
                _ => panic!("unexpected audio action"),
            };
        }
    }
    fn click(&mut self, rect: Rect) {
        let point = [rect.min[0] + 5., rect.min[1] + 10.];
        let actions = self.draw(pointer(point, true, false));
        self.run(actions);
        let actions = self.draw(pointer(point, false, true));
        self.run(actions);
    }
    fn type_number(&mut self, control: Control, text: &str) {
        self.click(self.field(control));
        let actions = self.draw(Input {
            text: text.into(),
            ..Default::default()
        });
        self.run(actions);
        let actions = self.draw(Input {
            keys: vec![(Key::Enter, Modifiers::default())],
            ..Default::default()
        });
        self.run(actions);
    }
}
fn pointer(point: [f32; 2], pressed: bool, released: bool) -> Input {
    Input {
        mouse: point,
        held: (!released).then_some(MouseButton::Left),
        pressed: pressed.then_some((MouseButton::Left, point)),
        released: released.then_some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn volume(state: &Value) -> f64 {
    state["project"]["layers"][0]["content"]["audio"]["volume"]
        .as_f64()
        .unwrap()
}

#[test]
fn audio_db_typing_silence_limits_and_mute_preserve_real_gain_and_history() {
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            let mut h = Harness::new(floating, locale);
            let before = h.state["project"].clone();
            h.type_number(Control::Gain, "-6.0206");
            assert!((volume(&h.state) - 0.5).abs() < 0.000001);
            assert!(!h.panels.audio_controls.editing());
            assert_eq!(h.engine.history(0).unwrap()["project"], before);
            for value in ["-inf", "−∞"] {
                h.type_number(Control::Gain, value);
                assert_eq!(volume(&h.state), 0.);
                assert_eq!(h.engine.history(0).unwrap()["project"], before);
            }
            h.type_number(Control::Gain, "6.020599913279624");
            assert_eq!(volume(&h.state), 2.);
            let changed = h.state["project"].clone();
            // Repeated untouched text commits must not round-trip the gain through decimals.
            for _ in 0..8 {
                h.type_number(Control::Gain, "");
            }
            assert_eq!(h.state["project"], changed);
            let mute = Rect::new(h.area().min[0] + 10., h.area().min[1] + 8. + 80., 100., 24.);
            h.click(mute);
            assert_eq!(volume(&h.state), 2.);
            assert_eq!(
                h.state["project"]["layers"][0]["content"]["audio"]["muted"],
                true
            );
            h.click(mute);
            assert_eq!(volume(&h.state), 2.);
            assert_eq!(
                h.state["project"]["layers"][0]["content"]["audio"]["muted"],
                false
            );
        }
    }
}

#[test]
fn audio_scrub_release_escape_focus_and_locked_state_share_the_owner_route() {
    for floating in [false, true] {
        let mut h = Harness::new(floating, Locale::En);
        let before = h.state["project"].clone();
        let r = h.field(Control::Gain);
        let p = [r.min[0] + 15., r.min[1] + 10.];
        let actions = h.draw(pointer(p, true, false));
        h.run(actions);
        let moved = [p[0] - 30., p[1]];
        let actions = h.draw(pointer(moved, false, false));
        assert!(matches!(
            actions.as_slice(),
            [Action::History(2), Action::Edit(_)]
        ));
        h.run(actions);
        let released = [p[0] - 60., p[1]];
        let actions = h.draw(pointer(released, false, true));
        h.run(actions);
        assert!((volume(&h.state) - db_gain(-6.) as f64).abs() < 0.000001);
        h.state = h.engine.history(0).unwrap();
        assert_eq!(h.state["project"], before);
        for input in [
            Input {
                keys: vec![(Key::Escape, Modifiers::default())],
                ..Default::default()
            },
            Input {
                focus_lost: true,
                ..Default::default()
            },
        ] {
            let actions = h.draw(pointer(p, true, false));
            h.run(actions);
            let actions = h.draw(pointer(moved, false, false));
            h.run(actions);
            let actions = h.draw(input);
            h.run(actions);
            assert_eq!(h.state["project"], before);
            assert!(!h.panels.audio_controls.editing());
        }
        h.engine
            .edit(
                serde_json::to_string(&Command::Flags {
                    object: 7,
                    visible: true,
                    locked: true,
                })
                .unwrap(),
            )
            .unwrap();
        assert!(h.draw(pointer(p, true, false)).is_empty());
        assert!(!h.panels.audio_controls.editing());
    }
}

#[test]
fn audio_stereo_keys_use_local_time_preserve_the_other_channel_and_reopen() {
    for floating in [false, true] {
        let mut h = Harness::new(floating, Locale::Zh);
        h.type_number(Control::Right, "-12");
        let right = h.state["project"]["layers"][0]["audio_levels"]["right"].clone();
        let top = h.area().min[1] + 8.;
        let left = row(h.area(), top + 138.);
        h.click(left.buttons[1]); // creates local frame 0 at composition frame 10
        h.state = h.engine.seek(40.).unwrap();
        h.type_number(Control::Left, "-6");
        let levels = &h.state["project"]["layers"][0]["audio_levels"];
        assert_eq!(levels["left"]["keys"][0]["frame"], 0);
        assert_eq!(levels["left"]["keys"][1]["frame"], 30);
        assert_eq!(levels["right"], right);
        h.state = h.engine.seek(25.).unwrap();
        let actions = h.draw(pointer(
            [left.buttons[0].min[0] + 5., left.buttons[0].min[1] + 10.],
            true,
            false,
        ));
        assert!(matches!(actions.as_slice(), [Action::Seek(10.)]));
        h.run(actions);
        h.click(left.buttons[2]);
        assert_eq!(h.state["frame"], 40.);
        let before = h.state["project"].clone();
        h.click(left.buttons[1]);
        assert_eq!(
            h.state["project"]["layers"][0]["audio_levels"]["left"]["keys"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
        h.state = h.engine.save().unwrap();
        let root = std::path::PathBuf::from(h.state["root"].as_str().unwrap());
        let reopened = motion_core::storage::load(&root).unwrap();
        assert_eq!(serde_json::to_value(&reopened).unwrap(), before);
        // The persisted controls feed the real frozen PCM mixer.
        let asset = &reopened.audio_assets[0];
        let pcm = motion_media::cache_path(&root, asset).unwrap();
        std::fs::create_dir_all(pcm.parent().unwrap()).unwrap();
        motion_media::decode_audio(&root.join(&asset.path), &pcm, Some(0), 2 << 20, &mut |_| {
            Ok(())
        })
        .unwrap();
        let mut mixer = motion_media::AudioMixer::new(reopened.clone(), &root).unwrap();
        let mut samples = [0.; 2];
        mixer.mix(25 * 1600, &mut samples).unwrap();
        assert!((samples[0] - 0.125 * motion_core::audio_levels::db_gain(-3.)).abs() < 0.00001);
        assert!((samples[1] - 0.25 * motion_core::audio_levels::db_gain(-12.)).abs() < 0.00001);
        let frozen = samples;
        h.type_number(Control::Left, "-24");
        mixer.mix(25 * 1600, &mut samples).unwrap();
        assert_eq!(samples, frozen);
        assert!(reopened.layers[0].audio_levels.as_ref().unwrap().animated());
    }
}

#[test]
fn audio_invalid_input_shortcuts_and_unsupported_capabilities_do_not_edit() {
    let mut h = Harness::new(false, Locale::En);
    let before = h.state["project"].clone();
    h.type_number(Control::Gain, "9");
    assert_eq!(h.state["project"], before);
    assert!(h.panels.audio_controls.editing());
    let actions = h.draw(Input {
        keys: vec![
            (Key::Delete, Modifiers::default()),
            (Key::Space, Modifiers::default()),
        ],
        composing: true,
        ..Default::default()
    });
    assert!(actions.is_empty());
    let actions = h.draw(Input {
        keys: vec![(Key::Escape, Modifiers::default())],
        ..Default::default()
    });
    h.run(actions);
    assert_eq!(h.state["project"], before);
    let mut unsupported = h.state.clone();
    unsupported["capabilities"]["audio"]["levels"]["supported"] = Value::Bool(false);
    let rect = h.field(Control::Left);
    let area = h.area();
    let mut out = vec![];
    render(
        &mut PaintList::default(),
        &mut h.atlas,
        area,
        &mut h.panels,
        &unsupported,
        &pointer([rect.min[0] + 5., rect.min[1] + 10.], true, false),
        &mut out,
    );
    assert!(out.is_empty());
    assert!(!h.panels.audio_controls.editing());
}

#[test]
fn audio_composition_reference_uses_its_own_envelopes_without_static_gain_commands() {
    let mut h = Harness::new(false, Locale::En);
    h.state = h
        .engine
        .edit(
            serde_json::to_string(&Command::Composition {
                action: motion_core::CompositionAction::Create {
                    settings: motion_core::CompositionSettings {
                        name: "Nested".into(),
                        width: 64,
                        height: 64,
                        fps: 30,
                        frames: 120,
                        timing: "preserve_seconds".into(),
                        shorten: "reject".into(),
                    },
                },
            })
            .unwrap(),
        )
        .unwrap();
    let target = h.state["project"]["compositions"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    h.state = h
        .engine
        .edit(
            serde_json::to_string(&Command::Composition {
                action: motion_core::CompositionAction::Reference {
                    target,
                    at_frame: 0,
                },
            })
            .unwrap(),
        )
        .unwrap();
    let object = h.state["project"]["layers"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    h.panels.select_only(Some(object));
    let before = h.state["project"].clone();
    h.type_number(Control::Left, "-15");
    let layer = h.state["project"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"].as_u64() == Some(object))
        .unwrap();
    assert_eq!(layer["audio_levels"]["left"]["value"], -15.);
    assert_eq!(layer["audio_levels"]["right"]["value"], 0.);
    assert_eq!(h.engine.history(0).unwrap()["project"], before);
}

#[test]
fn audio_narrow_main_and_floating_rows_keep_numeric_input_and_release_reachable() {
    for floating in [false, true] {
        let mut h = Harness::new(floating, Locale::Zh);
        let size = [180., 700.];
        let layout = Node::dock(Panel::Audio.dock());
        let area = if floating {
            Rect::new(0., 30., size[0], size[1] - 52.)
        } else {
            frames(&layout, size)[0].content
        };
        let field = row(area, area.min[1] + 8. + 52.).number;
        let original = h.state["project"].clone();
        let start = [field.min[0] + 40., field.min[1] + 10.];
        for input in [
            pointer(start, true, false),
            pointer([start[0] - 10., start[1]], false, false),
            pointer([start[0] - 20., start[1]], false, true),
        ] {
            let state = h.engine.state().unwrap();
            let actions = if floating {
                floating_chrome(
                    &mut PaintList::default(),
                    &mut h.atlas,
                    Panel::Audio,
                    size,
                    &mut h.panels,
                    &state,
                    &input,
                )
            } else {
                chrome(
                    &mut PaintList::default(),
                    &mut h.atlas,
                    size,
                    &layout,
                    &mut h.panels,
                    &state,
                    &input,
                )
            };
            h.run(actions);
        }
        assert!((volume(&h.state) - f64::from(db_gain(-2.))).abs() < 0.000001);
        assert!(!h.panels.audio_controls.editing());
        assert_eq!(h.engine.history(0).unwrap()["project"], original);
    }
}
