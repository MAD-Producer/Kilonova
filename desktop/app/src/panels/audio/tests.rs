use super::*;
use crate::engine::Engine;
use motion_core::{
    AudioAsset, AudioClip, Content, Layer, LayerTimeline, Project, VideoAsset, VideoClip,
};
use motion_ui::input::Modifiers;
mod property_focus_tests;

#[test]
fn modified_properties_audio_filter_keeps_layout_and_visible_controls_in_sync() {
    use crate::panels::{timeline, timeline_layout::LayerRows, types::PropertyFilter};
    for locale in [Locale::Zh, Locale::En] { for floating in [false,true] {
        let mut h=Harness::new(floating,locale);
        h.panels.focused=Some(Panel::Timeline);
        for command in [
            Command::SetAudio {object:7,volume:Some(0.5),muted:None},
            Command::AudioLevels {object:7,action:AudioLevelsAction::Set {channel:AudioChannel::Left,frame:20,value:-6.,animated:Some(false)}},
            Command::AudioLevels {object:7,action:AudioLevelsAction::Set {channel:AudioChannel::Right,frame:20,value:0.,animated:Some(true)}},
        ] { h.state=h.engine.edit(serde_json::to_string(&command).unwrap()).unwrap(); }
        let before=h.state["project"].clone();
        assert!(h.draw(Input::default()).is_empty());
        let input=Input {keys:vec![(Key::U,Default::default())],..Default::default()};
        assert!(h.draw(input.clone()).is_empty());
        assert_eq!(h.panels.timeline.filters[&7],PropertyFilter::Animated);
        let row=timeline::rows(&h.state).into_iter().find(|r|r.id==7).unwrap();
        assert_eq!(LayerRows::new(&row,&h.panels.timeline,20.,locale).audio.len(),2);
        assert_eq!(filtered_controls(&h.state["project"]["layers"][0],PropertyFilter::Animated),[Control::Right]);
        assert!(h.draw(input).is_empty());
        assert_eq!(h.panels.timeline.filters[&7],PropertyFilter::Modified);
        assert_eq!(LayerRows::new(&row,&h.panels.timeline,20.,locale).audio.len(),4);
        assert_eq!(filtered_controls(&h.state["project"]["layers"][0],PropertyFilter::Modified),[Control::Gain,Control::Left,Control::Right]);
        assert_eq!(h.state["project"],before);
        assert_eq!(h.engine.state().unwrap()["project"],before);
    }}
}

#[test]
fn audio_navigation_buttons_preserve_the_other_channel_in_both_native_hosts() {
    for locale in [Locale::Zh, Locale::En] { for floating in [false, true] {
        let mut h = Harness::new(floating, locale);
        for frame in [10, 40] {
            h.state = h.engine.edit(serde_json::to_string(&Command::AudioLevels {
                object:7, action:AudioLevelsAction::Set { channel:AudioChannel::Left, frame, value:-6., animated:Some(true) }
            }).unwrap()).unwrap();
        }
        h.state = h.engine.seek(22.).unwrap();
        let before = h.state["project"].clone();
        let fields = h.fields(Control::Left);
        let styles = PaintList::default().styles();
        let point = |slot:usize| [h.area().min[0] + styles.metric("padding.panel",8.) + (slot as f32 + 0.5) * styles.metric("icon.slot",20.), fields.watch.min[1] + fields.watch.height() * 0.5];
        let points = [point(0), point(1), point(2)];
        let actions = h.draw(pointer(points[1],true,false));
        assert!(matches!(actions.as_slice(), [Action::Edit(Command::AudioLevels { object:7, action:AudioLevelsAction::Set { channel:AudioChannel::Left, frame:22, animated:Some(true), .. } })]), "{actions:?}");
        h.run(actions);
        let added = h.state["project"].clone();
        assert_eq!(added["layers"][0]["audio_levels"]["right"], before["layers"][0]["audio_levels"]["right"]);
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
        assert_eq!(h.engine.history(1).unwrap()["project"], added);
        h.engine.save().unwrap();
        let reopened = motion_core::storage::load(&h._folder.path().join("project")).unwrap();
        assert_eq!(serde_json::to_value(reopened).unwrap(), added);
        for (slot, expected) in [(0,10.),(2,40.)] {
            let actions = h.draw(pointer(points[slot],true,false));
            assert!(matches!(actions.as_slice(), [Action::Seek(frame)] if *frame == expected), "{actions:?}");
        }
        let actions = h.draw(pointer(points[1],true,false));
        assert!(matches!(actions.as_slice(), [Action::Edit(Command::AudioLevels { action:AudioLevelsAction::DeleteKey { channel:AudioChannel::Left, frame:22 }, .. })]), "{actions:?}");
        h.run(actions);
        assert_eq!(h.state["project"], before);
        h.state = h.engine.seek(10.).unwrap();
        assert!(h.draw(pointer(points[0],true,false)).is_empty());
        for cancel in [false, true] {
            let mut input = pointer(points[1],true,false);
            if cancel { input.keys.push((Key::Escape, Modifiers::default())); }
            else { input.focus_lost = true; }
            assert!(h.draw(input).is_empty());
        }
        h.state = h.engine.edit(serde_json::to_string(&Command::Flags { object:7, visible:true, locked:true }).unwrap()).unwrap();
        assert!(h.draw(pointer(points[1],true,false)).is_empty());
    }}
}

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
        Self::with_video(floating, locale, false)
    }
    fn with_video(floating: bool, locale: Locale, video: bool) -> Self {
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
        if video {
            // Metadata exercises editor transport without decoding a video.
            project.video_assets.push(VideoAsset {
                id: 2,
                path: "assets/tone.wav".into(),
                bytes: wav.len() as u64,
                mime: "video/avc".into(),
                track: 1,
                width: 64,
                height: 64,
                rotation: 0,
                display_width: 64,
                display_height: 64,
                video_start_us: 0,
                video_end_us: 4_000_000,
                duration_us: 4_000_000,
                frame_count: 120,
                variable_frame_rate: false,
                nominal_frame_rate: 30.,
                color_standard: 1,
                color_range: 1,
                audio_asset: Some(1),
            });
            project.layers[0].content = Content::Video {
                video: VideoClip::new(2),
            };
            project.layers[0].size = [64.; 2];
        }
        motion_core::storage::save(&root, &project).unwrap();
        let engine = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
        let state = engine.seek(10.).unwrap();
        let mut panels = Panels {
            locale,
            ..Default::default()
        };
        panels.select_only(Some(7));
        panels.timeline.expanded.insert(7);
        if video {
            panels.timeline.collapsed_transforms.insert(7);
        }
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
            Rect::new(0., 30., 1000., 648.)
        } else {
            let layout = Node::dock(Panel::Timeline.dock());
            frames(&layout, [1000., 700.])[0].content
        }
    }
    fn field(&self, control: Control) -> Rect {
        self.fields(control).number
    }
    fn fields(&self, control: Control) -> Row {
        let area = self.area();
        row(
            PaintList::default().styles().as_ref(),
            Rect::new(
                area.min[0],
                area.min[1],
                GUTTER.min(area.width() * 0.52),
                area.height(),
            ),
            area.min[1]
                + crate::panels::timeline::header_height(&PaintList::default().styles())
                + match control {
                    Control::Gain => 44.,
                    Control::Left => 66.,
                    Control::Right => 88.,
                },
            22.,
        )
    }
    fn draw(&mut self, input: Input) -> Vec<Action> {
        self.state = self.engine.state().unwrap();
        if self.floating {
            floating_chrome(
                &mut PaintList::default(),
                &mut self.atlas,
                Panel::Timeline,
                [1000., 700.],
                &mut self.panels,
                &self.state,
                &input,
            )
        } else {
            chrome(
                &mut PaintList::default(),
                &mut self.atlas,
                [1000., 700.],
                &Node::dock(Panel::Timeline.dock()),
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
                Action::EditBatch(commands) => self
                    .engine
                    .edit(serde_json::to_string(&commands).unwrap())
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
fn child_composition_audio_edits_never_change_the_main_layer_with_the_same_id() {
    for floating in [false, true] {
        let mut h = Harness::new(floating, Locale::Zh);
        let main_layer = h.state["project"]["layers"][0].clone();
        h.state = h
            .engine
            .edit(
                serde_json::to_string(&Command::Composition {
                    action: motion_core::CompositionAction::Create {
                        settings: motion_core::CompositionSettings {
                            name: "Child audio".into(),
                            width: 64,
                            height: 64,
                            fps: 30,
                            frames: 120,
                            timing: "preserve_frames".into(),
                            shorten: "reject".into(),
                        },
                    },
                })
                .unwrap(),
            )
            .unwrap();
        let child = h.state["project"]["compositions"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        h.engine
            .edit(
                serde_json::to_string(&Command::InComposition {
                    composition: child.clone(),
                    command: Box::new(Command::Add {
                        layer: serde_json::from_value(main_layer.clone()).unwrap(),
                    }),
                })
                .unwrap(),
            )
            .unwrap();
        h.state = h
            .engine
            .composition(
                serde_json::json!({
                    "version":1,"composition":child,"op":"open",
                })
                .to_string(),
            )
            .unwrap();
        h.state = h.engine.seek(10.).unwrap();
        h.draw(Input::default()); // establish the new selection context
        h.panels.select_only(Some(7));
        h.panels.timeline.expanded.insert(7);
        h.type_number(Control::Gain, "-6");
        assert!((volume(&h.state) - f64::from(db_gain(-6.))).abs() < 0.000001);
        h.type_number(Control::Left, "-15");
        assert_eq!(
            h.state["project"]["layers"][0]["audio_levels"]["left"]["value"],
            -15.
        );
        h.click(h.fields(Control::Right).watch);
        assert_eq!(
            h.state["project"]["layers"][0]["audio_levels"]["right"]["keys"][0]["frame"],
            0
        );
        h.engine.save().unwrap();
        let root = std::path::PathBuf::from(h.state["root"].as_str().unwrap());
        let reopened = motion_core::storage::load(&root).unwrap();
        assert_eq!(
            serde_json::to_value(&reopened.composition_view("comp-main").unwrap().layers[0])
                .unwrap(),
            main_layer
        );
        assert_eq!(
            reopened.composition_view(&child).unwrap().layers[0]
                .audio_levels
                .as_ref()
                .unwrap()
                .left
                .value,
            -15.
        );
        let undone = h.engine.history(0).unwrap();
        assert_eq!(
            undone["project"]["layers"][0]["audio_levels"]["right"]["keys"],
            serde_json::json!([])
        );
        let main = h
            .engine
            .composition(
                serde_json::json!({
                    "version":1,"composition":"comp-main","op":"open",
                })
                .to_string(),
            )
            .unwrap();
        assert_eq!(main["project"]["layers"][0], main_layer);
    }
}

#[test]
fn timeline_key_and_reference_drags_commit_once_and_delete_only_the_selected_item() {
    for floating in [false, true] {
        let mut h = Harness::new(floating, Locale::En);
        h.click(h.fields(Control::Left).watch);
        h.state = h.engine.seek(40.).unwrap();
        h.type_number(Control::Left, "-6");
        let before = h.state["project"].clone();
        let area = h.area();
        let gutter = GUTTER.min(area.width() * 0.52);
        let ppf = (area.width() - gutter - 8.) / 120.;
        let point = [
            area.min[0] + gutter + 40. * ppf,
            h.field(Control::Left).min[1] + 11.,
        ];
        for input in [
            pointer(point, true, false),
            pointer([point[0] + ppf * 10., point[1]], false, true),
        ] {
            let actions = h.draw(input);
            h.run(actions);
        }
        assert_eq!(
            h.state["project"]["layers"][0]["audio_levels"]["left"]["keys"][1]["frame"],
            40
        );
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
        // Undo restores the key at 40; clicking it gives Delete to the key, not the layer.
        h.click(Rect::new(point[0] - 5., point[1] - 10., 10., 22.));
        let actions = h.draw(Input {
            keys: vec![(Key::Delete, Default::default())],
            ..Default::default()
        });
        h.run(actions);
        assert_eq!(h.state["project"]["layers"].as_array().unwrap().len(), 1);
        assert_eq!(
            h.state["project"]["layers"][0]["audio_levels"]["left"]["keys"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        h.engine.history(0).unwrap();
        h.state = h
            .engine
            .edit(
                serde_json::to_string(&Command::AudioMarkers {
                    object: 7,
                    action: motion_core::AudioMarkersAction::ReplaceGenerated { frames: vec![60] },
                })
                .unwrap(),
            )
            .unwrap();
        let original = h.state["project"].clone();
        let marker = [area.min[0] + gutter + 60. * ppf, area.min[1] + 57.];
        for input in [
            pointer(marker, true, false),
            pointer([marker[0] + ppf * 10., marker[1]], false, false),
            Input {
                keys: vec![(Key::Escape, Default::default())],
                ..Default::default()
            },
        ] {
            let actions = h.draw(input);
            h.run(actions);
        }
        assert_eq!(h.state["project"], original);
        for input in [
            pointer(marker, true, false),
            pointer([marker[0] + ppf * 10., marker[1]], false, true),
        ] {
            let actions = h.draw(input);
            h.run(actions);
        }
        assert_eq!(
            h.state["project"]["layers"][0]["audio_markers"][0]["frame"],
            60
        );
        assert_eq!(
            h.state["project"]["layers"][0]["audio_markers"][0]["generated"],
            true
        );
        assert_eq!(
            h.state["project"]["layers"][0]["audio_markers"][0]["corrected"],
            true
        );
        let corrected = [marker[0] + ppf * 10., marker[1]];
        let actions = h.draw(Input {
            mouse: corrected,
            pressed: Some((MouseButton::Right, corrected)),
            ..Default::default()
        });
        h.run(actions);
        let actions = h.draw(Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        });
        h.run(actions);
        assert_eq!(h.panels.audio_controls.batch.as_ref().unwrap().1.len(), 1);
        assert_eq!(h.engine.history(0).unwrap()["project"], original);
    }
}

#[test]
fn ruler_markers_follow_real_audio_and_video_clip_drag_and_undo_in_main_and_floating_windows() {
    for (floating, video) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut h = Harness::with_video(floating, Locale::En, video);
        h.engine
            .edit(
                serde_json::to_string(&Command::AudioMarkers {
                    object: 7,
                    action: motion_core::AudioMarkersAction::ReplaceGenerated { frames: vec![60] },
                })
                .unwrap(),
            )
            .unwrap();
        let before = h.engine.state().unwrap()["project"].clone();
        let area = h.area();
        let gutter = GUTTER.min(area.width() * 0.52);
        let ppf = (area.width() - gutter - 8.) / 120.;
        let clip = [area.min[0] + gutter + 30. * ppf, area.min[1] + timeline::header_height(&PaintList::default().styles()) + 11.];
        for mut input in [
            pointer(clip, true, false),
            pointer([clip[0] + 20. * ppf, clip[1]], false, true),
        ] {
            input.modifiers.alt = true;
            let actions = h.draw(input);
            h.run(actions);
        }
        assert_eq!(
            h.state["project"]["layers"][0]["timeline"]["offset_frame"],
            30
        );
        assert_eq!(
            h.state["project"]["layers"][0]["audio_markers"][0]["frame"],
            50
        );
        let marker = [area.min[0] + gutter + 80. * ppf, area.min[1] + 57.];
        let actions = h.draw(pointer(marker, true, false));
        h.run(actions);
        let actions = h.draw(pointer(marker, false, true));
        h.run(actions);
        assert_eq!(h.state["frame"], 80.);
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
    }
}

#[test]
fn audio_property_menu_only_offers_analysis_selection_after_markers_exist() {
    for floating in [false, true] {
        let mut h = Harness::new(floating, Locale::Zh);
        let field = h.field(Control::Gain);
        let p = [field.min[0] + 5., field.min[1] + 10.];
        let actions = h.draw(Input {
            mouse: p,
            pressed: Some((MouseButton::Right, p)),
            ..Default::default()
        });
        h.run(actions);
        let actions = h.draw(Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        });
        h.run(actions);
        // No analysis entries: the first audio action separates stereo channels.
        assert_eq!(h.state["project"]["layers"].as_array().unwrap().len(), 2);
        h.engine.history(0).unwrap();
        h.engine
            .edit(
                serde_json::to_string(&Command::AudioMarkers {
                    object: 7,
                    action: motion_core::AudioMarkersAction::ReplaceGenerated {
                        frames: vec![40, 60],
                    },
                })
                .unwrap(),
            )
            .unwrap();
        let actions = h.draw(Input {
            mouse: p,
            pressed: Some((MouseButton::Right, p)),
            ..Default::default()
        });
        h.run(actions);
        let actions = h.draw(Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        });
        h.run(actions);
        assert_eq!(h.panels.audio_controls.batch.as_ref().unwrap().1.len(), 2);
        let before = h.state["project"].clone();
        let actions = h.draw(Input {
            keys: vec![(Key::Delete, Default::default())],
            ..Default::default()
        });
        h.run(actions);
        assert_eq!(h.state["project"]["layers"].as_array().unwrap().len(), 1);
        assert!(h.state["project"]["layers"][0]["audio_markers"].is_null());
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
    }
}

#[test]
fn video_audio_property_menu_extracts_through_the_desktop_host_and_undoes_once() {
    for floating in [false, true] {
        let mut h = Harness::with_video(floating, Locale::Zh, true);
        let before = h.state["project"].clone();
        let field = h.field(Control::Gain);
        let p = [field.min[0] + 5., field.min[1] + 32.]; // collapsed transform group adds one row
        let actions = h.draw(Input {
            mouse: p,
            pressed: Some((MouseButton::Right, p)),
            ..Default::default()
        });
        h.run(actions);
        let actions = h.draw(Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        });
        h.run(actions);
        assert_eq!(h.state["project"]["layers"].as_array().unwrap().len(), 2);
        assert_eq!(
            h.state["project"]["layers"][0]["content"]["video"]["muted"],
            true
        );
        assert_eq!(h.state["project"]["layers"][1]["content"]["kind"], "audio");
        h.engine.save().unwrap();
        let root = std::path::PathBuf::from(h.state["root"].as_str().unwrap());
        assert_eq!(
            serde_json::to_value(motion_core::storage::load(&root).unwrap()).unwrap(),
            h.state["project"]
        );
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
    }
}

#[test]
fn ruler_markers_seek_add_and_batch_delete_only_generated_references_with_one_undo() {
    for floating in [false, true] {
        let mut h = Harness::new(floating, Locale::Zh);
        let area = h.area();
        let gutter = GUTTER.min(area.width() * 0.52);
        let ppf = (area.width() - gutter - 8.) / 120.;
        // Clicking the numbered body must select the marker's anchor, not the
        // frame under the text. The whole tab also opens its context menu.
        let marker = [area.min[0] + gutter + 60. * ppf + 12., area.min[1] + 57.];
        h.engine
            .edit(
                serde_json::to_string(&vec![
                    Command::AudioMarkers {
                        object: 7,
                        action: motion_core::AudioMarkersAction::Add {
                            frame: 20,
                            label: "Manual".into(),
                        },
                    },
                    Command::AudioMarkers {
                        object: 7,
                        action: motion_core::AudioMarkersAction::ReplaceGenerated {
                            frames: vec![40, 60, 80],
                        },
                    },
                ])
                .unwrap(),
            )
            .unwrap();
        let actions = h.draw(pointer(marker, true, false));
        h.run(actions);
        let actions = h.draw(pointer(marker, false, true));
        h.run(actions);
        assert_eq!(h.state["frame"], 60.);
        assert!(!h.panels.timeline.scrubbing);
        let before = h.state["project"].clone();
        let actions = h.draw(Input {
            mouse: marker,
            pressed: Some((MouseButton::Right, marker)),
            ..Default::default()
        });
        h.run(actions);
        let actions = h.draw(Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        });
        h.run(actions);
        assert_eq!(h.panels.audio_controls.batch.as_ref().unwrap().1.len(), 3);
        let actions = h.draw(Input {
            keys: vec![(Key::Delete, Default::default())],
            ..Default::default()
        });
        h.run(actions);
        assert_eq!(h.state["project"]["layers"].as_array().unwrap().len(), 1);
        assert_eq!(
            h.state["project"]["layers"][0]["audio_markers"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            h.state["project"]["layers"][0]["audio_markers"][0]["label"],
            "Manual"
        );
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
        h.state = h.engine.seek(30.).unwrap();
        h.click(Rect::new(
            area.min[0] + gutter - 28.,
            area.min[1] + 39.,
            24.,
            22.,
        ));
        assert!(h.state["project"]["layers"][0]["audio_markers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["frame"] == 20 && m["generated"] == false));
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
    }
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
            let mute = h.fields(Control::Gain).watch;
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
        let left = h.fields(Control::Left);
        h.click(left.watch); // creates local frame 0 at composition frame 10
        h.state = h.engine.seek(40.).unwrap();
        h.type_number(Control::Left, "-6");
        let levels = &h.state["project"]["layers"][0]["audio_levels"];
        assert_eq!(levels["left"]["keys"][0]["frame"], 0);
        assert_eq!(levels["left"]["keys"][1]["frame"], 30);
        assert_eq!(levels["right"], right);
        h.state = h.engine.seek(25.).unwrap();
        let actions = h.draw(Input {
            keys: vec![(Key::J, Modifiers::default())],
            ..Default::default()
        });
        assert!(matches!(actions.as_slice(), [Action::Seek(10.)]));
        h.run(actions);
        let actions = h.draw(Input {
            keys: vec![(Key::K, Modifiers::default())],
            ..Default::default()
        });
        h.run(actions);
        assert_eq!(h.state["frame"], 40.);
        let before = h.state["project"].clone();
        let area = h.area();
        let gutter = GUTTER.min(area.width() * 0.52);
        let ppf = (area.width() - gutter - 8.) / 120.;
        h.click(Rect::new(
            area.min[0] + gutter + 40. * ppf - 5.,
            left.number.min[1],
            10.,
            22.,
        ));
        let actions = h.draw(Input {
            keys: vec![(Key::Delete, Modifiers::default())],
            ..Default::default()
        });
        h.run(actions);
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
    timeline::render(
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
    h.panels.timeline.expanded.insert(object);
    h.panels.timeline.collapsed_transforms.insert(object);
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
        let size = [400., 700.];
        let layout = Node::dock(Panel::Timeline.dock());
        let area = if floating {
            Rect::new(0., 30., size[0], size[1] - 52.)
        } else {
            frames(&layout, size)[0].content
        };
        let field = row(
            PaintList::default().styles().as_ref(),
            Rect::new(
                area.min[0],
                area.min[1],
                GUTTER.min(area.width() * 0.52),
                area.height(),
            ),
            area.min[1] + crate::panels::timeline::header_height(&PaintList::default().styles()) + 44.,
            22.,
        )
        .number;
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
                    Panel::Timeline,
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

#[test]
fn malformed_audio_level_tracks_report_and_preserve_zero_fallback() {
    const CHILD: &str = "MOTION_AUDIO_TRACK_FALLBACK_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let path = root.join("motion-studio.log");
        let normal = serde_json::json!({"id":7});
        for _ in 0..120 { assert_eq!(track(&normal,Control::Left).sample(0.).unwrap(),0.); }
        assert!(std::fs::read_to_string(&path).unwrap().is_empty());
        let bad = serde_json::json!({"id":7,"audio_levels":{"left":17}});
        for _ in 0..120 { assert_eq!(track(&bad,Control::Left).sample(0.).unwrap(),0.); track(&normal,Control::Left); }
        let text = std::fs::read_to_string(path).unwrap();
        let records = text.lines().filter(|l| l.contains("site=audio.level_track ")).collect::<Vec<_>>();
        assert_eq!(records.len(),2);
        assert!(records[1].contains("ERROR") && records[1].contains("object=7 control=left"));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","panels::audio::tests::malformed_audio_level_tracks_report_and_preserve_zero_fallback"]).env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
    assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("site=audio.level_track"));
}
