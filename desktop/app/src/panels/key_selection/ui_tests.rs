//! Pointer input goes through the same main/floating chrome used by the app.
use super::*;
use crate::i18n::Locale;
use crate::panels::{self, Action, Panel, Panels};
use motion_ui::{
    dock,
    input::{Input, Modifiers, MouseButton},
    paint::{PaintList, Pattern, Shape},
    text::TextAtlas,
};
#[path = "drag_tests.rs"]
mod dragging;

fn view(engine: &Engine) -> Value {
    let mut value = state(engine);
    value["timeline_layers"] = json!(engine.project().timeline_layers(0.).unwrap());
    value["frame"] = json!(0);
    value
}
fn draw(
    panels: &mut Panels,
    state: &Value,
    input: Input,
    floating: bool,
    atlas: &mut TextAtlas,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let actions = if floating {
        panels::floating_chrome(
            &mut paint,
            atlas,
            Panel::Timeline,
            [1000., 800.],
            panels,
            state,
            &input,
        )
    } else {
        panels::chrome(
            &mut paint,
            atlas,
            [1000., 800.],
            &dock::Node::dock(Panel::Timeline.dock()),
            panels,
            state,
            &input,
        )
    };
    (paint, actions)
}
fn point(paint: &PaintList, panels: &Panels, frame: u32) -> [f32; 2] {
    let track = panels.timeline.zoom_track.unwrap();
    let x = track.min[0] + frame as f32 * track.width() / 120.;
    let y = paint
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Procedural {
                area,
                kind: Pattern::Diamond,
                ..
            } if ((area.min[0] + area.max[0]) * 0.5 - x).abs() < 0.01 => {
                Some((area.min[1] + area.max[1]) * 0.5)
            }
            _ => None,
        })
        .min_by(f32::total_cmp)
        .expect("actual authored key diamond");
    [x, y]
}
fn selected(panels: &Panels) -> usize {
    panels
        .selected_keys
        .selections()
        .iter()
        .map(|s| s.frames.len())
        .sum()
}
fn selection_only(actions: &[Action]) {
    assert!(
        actions.iter().all(|a| matches!(a, Action::Seek(_))),
        "selection must not edit authored data: {actions:?}"
    );
}
fn click(
    panels: &mut Panels,
    state: &Value,
    at: [f32; 2],
    modifiers: Modifiers,
    floating: bool,
    atlas: &mut TextAtlas,
) {
    let (_, actions) = draw(
        panels,
        state,
        Input {
            mouse: at,
            pressed: Some((MouseButton::Left, at)),
            held: Some(MouseButton::Left),
            modifiers,
            ..Default::default()
        },
        floating,
        atlas,
    );
    selection_only(&actions);
    let (_, actions) = draw(
        panels,
        state,
        Input {
            mouse: at,
            released: Some((MouseButton::Left, at)),
            modifiers,
            ..Default::default()
        },
        floating,
        atlas,
    );
    selection_only(&actions);
}
#[test]
fn actual_summary_pointer_replace_add_toggle_and_select_all_use_channel_identities() {
    let engine = fixture();
    let state = view(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let mut panels = Panels {
                locale,
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            let (paint, actions) =
                draw(&mut panels, &state, Input::default(), floating, &mut atlas);
            assert!(actions.is_empty());
            let a = point(&paint, &panels, 10);
            let b = point(&paint, &panels, 43);
            click(
                &mut panels,
                &state,
                a,
                Modifiers::default(),
                floating,
                &mut atlas,
            );
            assert_eq!(
                panels.selected_keys.status(&state),
                Status::Current,
                "{locale:?} float={floating}"
            );
            assert_eq!(selected(&panels), 1);
            assert!(panels.selected_keys.contains(&key(7, None, 10)));
            click(
                &mut panels,
                &state,
                b,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert_eq!(
                selected(&panels),
                4,
                "summary at separated position owns its three axes"
            );
            assert!(panels.selected_keys.contains(&key(7, None, 10)));
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                assert!(panels.selected_keys.contains(&key(9, Some(axis), 43)));
            }
            click(
                &mut panels,
                &state,
                a,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert_eq!(selected(&panels), 3);
            assert!(!panels.selected_keys.contains(&key(7, None, 10)));
            assert!(super::super::shortcut(
                &mut panels,
                &state,
                crate::shortcuts::Command::SelectAll,
                &mut vec![]
            ));
            let (_, actions) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
            selection_only(&actions);
            assert_eq!(selected(&panels), 8);
            assert_eq!(panels.selected_keys.status(&state), Status::Current);
        }
    }
}
#[test]
fn actual_summary_marquee_works_in_both_directions_and_empty_selection_owns_delete() {
    let engine = fixture();
    let state = view(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for reverse in [false, true] {
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
            let a = point(&paint, &panels, 10);
            let b = point(&paint, &panels, 43);
            let track = panels.timeline.zoom_track.unwrap();
            let ppf = track.width() / 120.;
            let ends = [
                [track.min[0] + 5. * ppf, a[1].min(b[1]) - 8.],
                [track.min[0] + 60. * ppf, a[1].max(b[1]) + 16.],
            ];
            let (start, end) = if reverse {
                (ends[1], ends[0])
            } else {
                (ends[0], ends[1])
            };
            let (_, actions) = draw(
                &mut panels,
                &state,
                Input {
                    mouse: start,
                    pressed: Some((MouseButton::Left, start)),
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            assert!(
                panels.key_gestures.active(),
                "reverse={reverse} floating={floating}"
            );
            let (_, actions) = draw(
                &mut panels,
                &state,
                Input {
                    mouse: end,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            let (_, actions) = draw(
                &mut panels,
                &state,
                Input {
                    mouse: end,
                    released: Some((MouseButton::Left, end)),
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            assert!(!panels.key_gestures.active());
            assert_eq!(
                selected(&panels),
                8,
                "reverse={reverse} floating={floating}"
            );
            assert_eq!(panels.selected_keys.status(&state), Status::Current);
            let blank = [start[0], a[1].max(b[1]) + 80.];
            click(
                &mut panels,
                &state,
                blank,
                Modifiers::default(),
                floating,
                &mut atlas,
            );
            assert_eq!(panels.selected_keys.status(&state), Status::Empty);
            let mut actions = vec![];
            assert!(super::super::shortcut(
                &mut panels,
                &state,
                crate::shortcuts::Command::Delete,
                &mut actions
            ));
            assert!(
                actions.is_empty(),
                "empty key selection cannot delete a layer"
            );
        }
    }
}

#[test]
fn actual_marquee_cancels_on_escape_focus_or_changed_document_and_view() {
    let mut project = fixture().snapshot();
    let layer = project.layers[0].clone();
    for id in 100..140 {
        let mut copy = layer.clone();
        copy.id = id;
        project.layers.push(copy);
    }
    let engine = Engine::new(project).unwrap();
    let original = view(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for reason in [
            "escape",
            "focus",
            "revision",
            "generation",
            "composition",
            "zoom",
            "scroll",
        ] {
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            let (paint, _) = draw(
                &mut panels,
                &original,
                Input::default(),
                floating,
                &mut atlas,
            );
            let a = point(&paint, &panels, 10);
            click(
                &mut panels,
                &original,
                a,
                Modifiers::default(),
                floating,
                &mut atlas,
            );
            let before = panels.selected_keys.selections();
            let track = panels.timeline.zoom_track.unwrap();
            let blank = [track.min[0] + 5. * track.width() / 120., a[1] + 100.];
            draw(
                &mut panels,
                &original,
                Input {
                    mouse: blank,
                    pressed: Some((MouseButton::Left, blank)),
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert!(panels.key_gestures.active());
            let mut changed = original.clone();
            let mut input = Input {
                mouse: [blank[0] + 35., blank[1] + 30.],
                held: Some(MouseButton::Left),
                ..Default::default()
            };
            match reason {
                "escape" => input
                    .keys
                    .push((motion_ui::input::Key::Escape, Modifiers::default())),
                "focus" => input.focus_lost = true,
                "revision" => changed["revision"] = json!(engine.revision() + 1),
                "generation" => changed["document_generation"] = json!(8),
                "composition" => changed["composition"] = json!("other"),
                "zoom" => panels.timeline.zoom = 2.,
                "scroll" => panels.timeline.scroll = 10.,
                _ => unreachable!(),
            }
            let (_, actions) = draw(&mut panels, &changed, input, floating, &mut atlas);
            selection_only(&actions);
            assert!(
                !panels.key_gestures.active(),
                "{reason} floating={floating}"
            );
            assert_eq!(
                panels.selected_keys.selections(),
                before,
                "{reason} floating={floating}"
            );
            draw(
                &mut panels,
                &changed,
                Input {
                    mouse: blank,
                    released: Some((MouseButton::Left, blank)),
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert_eq!(
                panels.selected_keys.selections(),
                before,
                "late release after {reason}"
            );
        }
    }
}

fn run(engine: &mut Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => engine.apply(command).unwrap(),
            Action::EditBatch(commands) => {
                engine.apply_batch(commands).unwrap();
            }
            Action::EditorAction { source, commands } => {
                assert_eq!(
                    source,
                    crate::layer_clipboard::KeyframeSource::of(&view(engine))
                );
                engine.apply_batch(commands).unwrap();
            }
            Action::History(2) => engine.begin_gesture().unwrap(),
            Action::History(3) => engine.end_gesture(true).unwrap(),
            Action::History(4) => engine.end_gesture(false).unwrap(),
            Action::Seek(_) => {}
            other => panic!("unexpected authored-key action: {other:?}"),
        }
    }
}

#[test]
fn actual_expanded_keys_batch_f9_delete_save_reopen_and_one_undo() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let mut engine = fixture();
            let before = engine.snapshot();
            let state = view(&engine);
            let mut panels = Panels {
                locale,
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            panels.timeline.expanded.insert(7);
            panels
                .animation_controls
                .begin_row_probe(7, Property::Position, None);
            let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
            let track = panels.timeline.zoom_track.unwrap();
            let ppf = track.width() / 120.;
            let row = panels.animation_controls.probed_row(None).unwrap();
            let a = [track.min[0] + 10. * ppf, (row.min[1] + row.max[1]) * 0.5];
            let b = [track.min[0] + 20. * ppf, a[1]];
            click(
                &mut panels,
                &state,
                a,
                Modifiers::default(),
                floating,
                &mut atlas,
            );
            click(
                &mut panels,
                &state,
                b,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            let c = point(&paint, &panels, 43);
            click(
                &mut panels,
                &state,
                c,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert_eq!(selected(&panels), 5);
            let (_, actions) = draw(
                &mut panels,
                &state,
                Input {
                    keys: vec![(motion_ui::input::Key::F9, Modifiers::default())],
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert!(
                matches!(actions.as_slice(),[Action::EditorAction {commands,..}] if matches!(commands.as_slice(),[Command::EaseKeyframes {..}]))
            );
            run(&mut engine, actions);
            assert_ne!(engine.project(), &before);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            let state = view(&engine);
            let (_, actions) = draw(
                &mut panels,
                &state,
                Input {
                    keys: vec![(motion_ui::input::Key::Delete, Modifiers::default())],
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert!(
                matches!(actions.as_slice(),[Action::EditorAction {commands,..}] if matches!(commands.as_slice(),[Command::DeleteKeyframes {..}]))
            );
            run(&mut engine, actions);
            assert_eq!(engine.project().layers.len(), 2);
            assert!(engine.project().layers[0]
                .transform
                .position
                .keys
                .is_empty());
            assert_eq!(
                engine.project().layers[1]
                    .transform
                    .position
                    .axes
                    .as_ref()
                    .unwrap()
                    .x
                    .keys
                    .len(),
                1
            );
            let after = engine.snapshot();
            let folder = tempfile::tempdir().unwrap();
            motion_core::storage::save(folder.path(), &after).unwrap();
            assert_eq!(
                Engine::new(motion_core::storage::load(folder.path()).unwrap())
                    .unwrap()
                    .project(),
                &after
            );
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            engine.redo().unwrap();
            assert_eq!(engine.project(), &after);
        }
    }
}

#[test]
fn actual_single_key_drag_tracks_selection_and_cancel_restores_identity() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for cancel in [false, true] {
            let mut engine = fixture();
            let before = engine.snapshot();
            let state = view(&engine);
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
            let a = point(&paint, &panels, 10);
            let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
            let (_, actions) = draw(
                &mut panels,
                &state,
                Input {
                    mouse: a,
                    pressed: Some((MouseButton::Left, a)),
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            run(&mut engine, actions);
            let next = [a[0] + 5. * ppf, a[1]];
            let (_, actions) = draw(
                &mut panels,
                &view(&engine),
                Input {
                    mouse: next,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert!(actions.iter().any(|a| matches!(a, Action::EditBatch(_))));
            run(&mut engine, actions);
            assert!(panels.selected_keys.contains(&key(7, None, 15)));
            assert_eq!(panels.selected_keys.status(&view(&engine)), Status::Current);
            let input = if cancel {
                Input {
                    mouse: next,
                    keys: vec![(motion_ui::input::Key::Escape, Modifiers::default())],
                    ..Default::default()
                }
            } else {
                Input {
                    mouse: next,
                    released: Some((MouseButton::Left, next)),
                    ..Default::default()
                }
            };
            let (_, actions) = draw(&mut panels, &view(&engine), input, floating, &mut atlas);
            run(&mut engine, actions);
            assert_eq!(panels.selected_keys.status(&view(&engine)), Status::Current);
            assert!(panels
                .selected_keys
                .contains(&key(7, None, if cancel { 10 } else { 15 })));
            if cancel {
                assert_eq!(engine.project(), &before);
            } else {
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
            }
        }
    }
}

#[test]
fn actual_summary_selects_mask_vector_effect_and_audio_keys_at_their_own_times() {
    use motion_core::{
        masks::{LayerMask, MaskProperty},
        vector::{PathNode, ShapeKind, VectorContent, VectorPath},
        AudioAsset, AudioChannel, AudioClip, AudioLevels, Content, EffectInstance, Track,
    };
    let mut project = fixture().snapshot();
    let mut vector = VectorContent::shape(ShapeKind::Rectangle);
    vector
        .fill
        .as_mut()
        .unwrap()
        .set_animated(31, true)
        .unwrap();
    project.layers[0].content = Content::Vector { vector };
    let path = VectorPath {
        tangents: Default::default(), id: 1,
        closed: true,
        nodes: [[0., 0.], [40., 0.], [0., 40.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| PathNode {
                id: i as u64 + 1,
                geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
            })
            .collect(),
    };
    let mut mask = LayerMask::new(1, path);
    mask.opacity.set_animated(21, true).unwrap();
    project.layers[0].masks.push(mask);
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "posterize")
        .unwrap();
    let mut effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64.; 2],
    );
    let (name, param) = effect
        .params
        .iter_mut()
        .find(|(_, p)| p.animatable && p.implemented)
        .unwrap();
    param.track.set_animated(23, true).unwrap();
    let parameter = name.clone();
    project.layers[0].effects.push(effect);
    project.rebuild_plugin_dependencies();
    project.audio_assets.push(AudioAsset {
        id: 1,
        path: "assets/tone.wav".into(),
        bytes: 64,
        mime: "audio/wav".into(),
        track: 0,
        sample_rate: 48000,
        channels: 2,
        sample_frames: 192000,
        duration_us: 4000000,
    });
    let mut audio = Layer::solid(11, "Audio", [0.; 2], [0.; 3], [1.; 4]);
    audio.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    audio.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 110,
        offset_frame: 10,
    });
    let mut levels = AudioLevels::default();
    levels.left.set_animated(8, true).unwrap();
    audio.audio_levels = Some(levels);
    project.layers.push(audio);
    let engine = Engine::new(project).unwrap();
    let state = view(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
        for (frame, target) in [
            (
                28,
                KeyframeTarget::Mask {
                    object: 7,
                    mask: 1,
                    property: MaskProperty::Opacity,
                },
            ),
            (
                38,
                KeyframeTarget::Vector {
                    object: 7,
                    item: None,
                    parameter: "fill".into(),
                },
            ),
            (
                30,
                KeyframeTarget::Effect {
                    object: 7,
                    effect: 1,
                    param: parameter.clone(),
                },
            ),
            (
                18,
                KeyframeTarget::Audio {
                    object: 11,
                    channel: AudioChannel::Left,
                },
            ),
        ] {
            let at = point(&paint, &panels, frame);
            click(
                &mut panels,
                &state,
                at,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert!(
                panels.selected_keys.contains(&Key { target, frame }),
                "frame={frame} float={floating}"
            );
            assert_eq!(panels.selected_keys.status(&state), Status::Current);
        }
        assert_eq!(selected(&panels), 4);
        KeyframeClipboard::capture(engine.project(), &panels.selected_keys.selections()).unwrap();
    }
}

#[test]
fn actual_ui_capture_failures_preserve_selection_and_default_file_stderr_channels() {
    const NAME:&str="panels::key_selection::tests::ui::actual_ui_capture_failures_preserve_selection_and_default_file_stderr_channels";
    const CHILD: &str = "MOTION_KEY_SELECTION_UI_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(root.into()).unwrap();
        let engine = fixture();
        let state = view(&engine);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut panels = Panels {
            locale: Locale::Zh,
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        let (paint, _) = draw(&mut panels, &state, Input::default(), false, &mut atlas);
        let at = point(&paint, &panels, 10);
        click(
            &mut panels,
            &state,
            at,
            Modifiers::default(),
            false,
            &mut atlas,
        );
        let before = panels.selected_keys.selections();
        let mut malformed = state.clone();
        malformed["revision"] = json!(engine.revision() + 1);
        malformed["project"]["layers"][0]["audio_levels"] =
            json!({"left":null,"right":{"value":0.,"keys":[]}});
        assert!(super::super::shortcut(
            &mut panels,
            &malformed,
            crate::shortcuts::Command::SelectAll,
            &mut vec![]
        ));
        let (_, actions) = draw(&mut panels, &malformed, Input::default(), false, &mut atlas);
        assert!(actions.is_empty());
        assert_eq!(panels.selected_keys.selections(), before);
        assert!(panels.message.as_deref().is_some_and(
            |s| s.contains("关键帧选择失败") && s.contains("authored key array missing")
        ));
        let mut missing = state.clone();
        missing["document_generation"] = Value::Null;
        for _ in 0..120 {
            let (_, actions) = draw(
                &mut panels,
                &missing,
                Input {
                    mouse: at,
                    pressed: Some((MouseButton::Left, at)),
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                false,
                &mut atlas,
            );
            assert!(
                actions.is_empty(),
                "unidentified gesture must not enter the old key/layer controllers"
            );
            assert_eq!(panels.selected_keys.status(&missing), Status::Stale);
            assert_eq!(panels.selected_keys.selections(), before);
        }
        click(
            &mut panels,
            &state,
            at,
            Modifiers::default(),
            false,
            &mut atlas,
        );
        assert_eq!(panels.selected_keys.status(&state), Status::Current);
        assert_eq!(panels.selected_keys.selections(), before);
        crate::logging::flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_KEY_SELECTION_UI_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for channel in [&file, stderr.as_ref()] {
        assert!(
            channel.contains("key selection failed operation=select phase=capture")
                && channel.contains("authored key array missing"),
            "{channel}"
        );
        assert!(
            channel.contains("site=selection.gesture_scope")
                && channel.contains("count=60 ")
                && channel.contains("key selection document identity missing"),
            "{channel}"
        );
    }
    let scope: Vec<_> = file
        .lines()
        .filter(|l| l.contains("site=selection.gesture_scope"))
        .collect();
    assert_eq!(scope.len(), 2);
    assert!(scope[0].contains("WARN") && scope[0].contains("count=1 "));
    assert!(scope[1].contains("ERROR"));
    assert!(!stderr.contains("count=1 "));
    assert!(!stdout.contains("key selection failed") && !stdout.contains("panel degradation"));
}

#[test]
fn select_all_includes_offscreen_keys_and_readonly_pointer_selection_cannot_edit() {
    let engine = fixture();
    let original = view(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for graph in [false, true] {
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        panels.select_only(Some(7));
        panels.timeline.graph_mode = graph;
        panels.timeline.zoom = 4.;
        panels.timeline.offset = 70.;
        panels
            .selected_keys
            .apply(&original, [key(7, None, 10)], Mode::Replace)
            .unwrap();
        panels
            .animation_controls
            .choose_track_in(&original, 7, Property::Position, None);
        assert!(super::super::shortcut(
            &mut panels,
            &original,
            crate::shortcuts::Command::SelectAll,
            &mut vec![]
        ));
        let (_, actions) = draw(&mut panels, &original, Input::default(), false, &mut atlas);
        selection_only(&actions);
        assert_eq!(selected(&panels), if graph { 2 } else { 8 });
        assert_eq!(panels.selected_keys.status(&original), Status::Current);
    }
    for mode in ["locked", "observing"] {
        let mut state = original.clone();
        if mode == "locked" {
            state["project"]["layers"][0]["locked"] = json!(true);
        } else {
            state["observing"] = json!(true);
        }
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        let (paint, _) = draw(&mut panels, &state, Input::default(), false, &mut atlas);
        let a = point(&paint, &panels, 10);
        click(
            &mut panels,
            &state,
            a,
            Modifiers::default(),
            false,
            &mut atlas,
        );
        assert_eq!(selected(&panels), 1);
        for key in [motion_ui::input::Key::Delete, motion_ui::input::Key::F9] {
            let (_, actions) = draw(
                &mut panels,
                &state,
                Input {
                    keys: vec![(key, Modifiers::default())],
                    ..Default::default()
                },
                false,
                &mut atlas,
            );
            assert!(actions.is_empty(), "{mode}: {actions:?}");
            assert!(panels.message.is_some());
        }
    }
}

#[test]
fn actual_graph_pointer_and_marquee_select_authored_keys_without_component_duplicates() {
    let engine = fixture();
    let state = view(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            for reverse in [false, true] {
                let mut panels = Panels {
                    locale,
                    focused: Some(Panel::Timeline),
                    ..Default::default()
                };
                panels.select_only(Some(7));
                panels.timeline.graph_mode = true;
                panels
                    .animation_controls
                    .choose_track_in(&state, 7, Property::Position, None);
                let (paint, actions) =
                    draw(&mut panels, &state, Input::default(), floating, &mut atlas);
                assert!(actions.is_empty());
                let a = point(&paint, &panels, 10);
                let b = point(&paint, &panels, 20);
                click(
                    &mut panels,
                    &state,
                    a,
                    Modifiers::default(),
                    floating,
                    &mut atlas,
                );
                assert_eq!(selected(&panels), 1);
                click(
                    &mut panels,
                    &state,
                    b,
                    Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                );
                assert_eq!(
                    selected(&panels),
                    2,
                    "XYZ projections share one authored channel"
                );
                let ends = [
                    [a[0] - 20., a[1].min(b[1]) - 20.],
                    [b[0] + 20., a[1].max(b[1]) + 20.],
                ];
                let (start, end) = if reverse {
                    (ends[1], ends[0])
                } else {
                    (ends[0], ends[1])
                };
                let (_, actions) = draw(
                    &mut panels,
                    &state,
                    Input {
                        mouse: start,
                        pressed: Some((MouseButton::Left, start)),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                );
                selection_only(&actions);
                assert!(
                    panels.key_gestures.active(),
                    "curve marquee reverse={reverse} float={floating}"
                );
                let (_, actions) = draw(
                    &mut panels,
                    &state,
                    Input {
                        mouse: end,
                        released: Some((MouseButton::Left, end)),
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                );
                selection_only(&actions);
                assert_eq!(selected(&panels), 2);
                assert_eq!(panels.selected_keys.status(&state), Status::Current);
                assert!(
                    panels.selected_keys.contains(&key(7, None, 10))
                        && panels.selected_keys.contains(&key(7, None, 20))
                );
            }
        }
    }
}
