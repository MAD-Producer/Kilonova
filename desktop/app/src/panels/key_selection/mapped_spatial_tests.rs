use super::*;
use motion_core::layer_time::*;
#[cfg(target_os = "windows")]
#[path = "mapped_spatial_native.rs"]
mod native;

const MODES: [&str; 5] = ["summary", "property", "value", "speed", "comparison"];
fn mapped(reverse: bool) -> Engine {
    let mut p = fixture().snapshot();
    let l = &mut p.layers[0];
    l.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 120,
        offset_frame: 2,
    });
    l.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if reverse { 0. } else { -12. },
        )
        .unwrap(),
        remap: None,
    });
    l.transform.position = motion_core::Track::constant([0., 20., 0.]);
    for (f, x) in [(-12, 0.), (-9, 1.), (-6, 4.), (0, 8.)] {
        l.transform
            .position
            .upsert(f, [x, 20., 0.], Ease::Linear)
            .unwrap();
    }
    l.transform.position.enable_roving(&[-9]).unwrap();
    l.transform
        .position
        .spatial_animation
        .as_mut()
        .unwrap()
        .set_fixed_time(2, -9.25)
        .unwrap();
    Engine::new(p).unwrap()
}
fn setup(locale: Locale, mode: &str, state: &Value) -> Panels {
    let mut p = Panels {
        locale,
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    p.select_only(Some(7));
    // Existing zoom only; no change to authored time or hit tolerances.
    p.timeline.zoom = 2.;
    if mode == "property" {
        p.timeline.expanded.insert(7);
    }
    if matches!(mode, "value" | "speed" | "comparison") {
        p.timeline.set_graph_mode(true);
        p.timeline.speed_graph = mode == "speed";
        p.animation_controls
            .choose_track(7, Property::Position, None);
        if mode == "comparison" {
            crate::panels::curve_comparison::tests::collect_targets(
                &mut p,
                state,
                &[key(7, None, 0).target],
            );
        }
    }
    p
}
fn selected_time(state: &Value) -> f64 {
    state["timeline_layers"][0]["properties"]["position"]["spatial_keys"][1]["frame"]
        .as_f64()
        .unwrap()
}
fn shifted(before: &Project, reverse: bool, delta: f64) -> Project {
    let mut p = before.clone();
    p.layers[0]
        .transform
        .position
        .spatial_animation
        .as_mut()
        .unwrap()
        .move_keys(&[2], delta * if reverse { -2. / 3. } else { 2. / 3. })
        .unwrap();
    p
}
fn delta_input() -> Modifiers {
    Modifiers {
        alt: true,
        ..Default::default()
    }
}

#[test]
fn normal_drag_snaps_from_the_exact_spatial_origin_to_the_playhead() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for floating in [false, true] {
            for mode in MODES {
                let mut e = mapped(reverse);
                let before = e.snapshot();
                let mut state = view(&e);
                let time = selected_time(&state);
                state["frame"] = json!(time + 3.);
                let mut p = setup(Locale::En, mode, &state);
                let (paint, _) = draw(&mut p, &state, Input::default(), floating, &mut atlas);
                let at = at_time(&paint, &p, time);
                let ppf = p.timeline.zoom_track.unwrap().width() / 120. * p.timeline.zoom;
                // Raw movement rounds to two frames; the playhead is three frames
                // away, within the unchanged snap tolerance. No Alt bypass is used.
                let to = [at[0] + 2.1 * ppf, at[1]];
                let (_, out) = draw(
                    &mut p,
                    &state,
                    pointer(at, true, false, Modifiers::default()),
                    floating,
                    &mut atlas,
                );
                selection_only(&out);
                let (_, out) = draw(
                    &mut p,
                    &state,
                    pointer(to, false, false, Modifiers::default()),
                    floating,
                    &mut atlas,
                );
                selection_only(&out);
                assert_eq!(
                    p.timeline.snap_guide,
                    Some(time + 3.),
                    "{reverse} {floating} {mode}"
                );
                assert_eq!(e.project(), &before);
                let (_, out) = draw(
                    &mut p,
                    &state,
                    pointer(to, false, true, Modifiers::default()),
                    floating,
                    &mut atlas,
                );
                e.apply_batch(atomic(out)).unwrap();
                assert_eq!(e.project(), &shifted(&before, reverse, 3.));
                e.undo().unwrap();
                assert_eq!(e.project(), &before);
            }
        }
    }
}

#[test]
fn mapped_spatial_drag_commits_once_in_all_views_hosts_languages_and_directions() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for floating in [false, true] {
                for mode in MODES {
                    let mut e = mapped(reverse);
                    let before = e.snapshot();
                    let state = view(&e);
                    let time = selected_time(&state);
                    assert_ne!(time.fract(), 0.);
                    let mut p = setup(locale, mode, &state);
                    let (paint, out) = draw(&mut p, &state, Input::default(), floating, &mut atlas);
                    selection_only(&out);
                    let at = at_time(&paint, &p, time);
                    let ppf = p.timeline.zoom_track.unwrap().width() / 120. * p.timeline.zoom;
                    let to = [at[0] + 3. * ppf, at[1]];
                    let (_, out) = draw(
                        &mut p,
                        &state,
                        pointer(at, true, false, delta_input()),
                        floating,
                        &mut atlas,
                    );
                    selection_only(&out);
                    assert!(
                        p.key_gestures.dragging(),
                        "{reverse} {locale:?} {floating} {mode}: {:?}",
                        p.message
                    );
                    let plan = transform::Plan::capture(&p.selected_keys, &state).unwrap();
                    assert_eq!(
                        plan.times().collect::<Vec<_>>(),
                        [time],
                        "snap origins include continuous keys"
                    );
                    let (paint, out) = draw(
                        &mut p,
                        &state,
                        pointer(to, false, false, delta_input()),
                        floating,
                        &mut atlas,
                    );
                    selection_only(&out);
                    at_time(&paint, &p, time + 3.);
                    assert_eq!(e.project(), &before);
                    assert!(!e.can_undo());
                    let (_, out) = draw(
                        &mut p,
                        &state,
                        pointer(to, false, true, delta_input()),
                        floating,
                        &mut atlas,
                    );
                    let commands = atomic(out);
                    assert!(matches!(commands.as_slice(), [Command::SpatialKeyframes {
                edit: SpatialKeyEdit::Move { keys, delta }, .. }] if keys == &[2] && *delta == 3.));
                    e.apply_batch(commands).unwrap();
                    let expected = shifted(&before, reverse, 3.);
                    assert_eq!(e.project(), &expected);
                    assert_eq!(identities(&p), [2]);
                    assert_eq!(p.selected_keys.status(&view(&e)), Status::Current);
                    e.undo().unwrap();
                    assert_eq!(e.project(), &before);
                    e.redo().unwrap();
                    assert_eq!(e.project(), &expected);
                    let temp = tempfile::tempdir().unwrap();
                    motion_core::storage::save(temp.path(), &expected).unwrap();
                    assert_eq!(motion_core::storage::load(temp.path()).unwrap(), expected);
                }
            }
        }
    }
}

#[test]
fn mapped_order_bounds_and_mixed_clock_grid_are_checked_before_dispatch() {
    for reverse in [false, true] {
        let mut e = mapped(reverse);
        let before = e.snapshot();
        let state = view(&e);
        let mut selected = Set::default();
        selected
            .apply_addressed(&state, [identity(2)], Mode::Replace)
            .unwrap();
        let plan = transform::Plan::capture(&selected, &state).unwrap();
        assert!(plan.allows(3, false));
        assert!(!plan.allows(20, false));
        assert!(!plan.allows(-20, false));
        assert!(
            !plan.allows(0, true),
            "no partial copy of continuous metadata"
        );
        assert!(plan.commands(20, None, false).is_err());
        assert_eq!(e.project(), &before);
        let mut project = before.clone();
        project.layers[1].time_mapping = Some(LayerTimeMapping {
            clock: LayerClock::new(
                TimeStretch::new(5, 2).unwrap(),
                TimeDirection::Reverse,
                0.,
                20.,
            )
            .unwrap(),
            remap: None,
        });
        e = Engine::new(project.clone()).unwrap();
        let state = view(&e);
        selected
            .apply_addressed(
                &state,
                (1..=4).map(identity).chain([3, 13].map(|frame| {
                    LocalKey {
                        target: key(9, Some(Axis::Y), 0).target,
                        frame,
                    }
                    .into()
                })),
                Mode::Replace,
            )
            .unwrap();
        let plan = transform::Plan::capture(&selected, &state).unwrap();
        assert_eq!(plan.exact_steps(1.), None);
        assert_eq!(plan.exact_steps(5.), Some(1));
        assert!(plan.allows(1, false));
        let commands = plan.commands(1, None, false).unwrap();
        assert_eq!(commands.len(), 2);
        e.apply_batch(commands).unwrap();
        let mut expected = project.clone();
        expected.layers[0]
            .transform
            .position
            .spatial_animation
            .as_mut()
            .unwrap()
            .move_keys(&[1, 2, 3, 4], 5. * if reverse { -2. / 3. } else { 2. / 3. })
            .unwrap();
        for key in &mut expected.layers[1]
            .transform
            .position
            .axes
            .as_mut()
            .unwrap()
            .y
            .keys
        {
            key.frame -= 2;
        }
        assert_eq!(e.project(), &expected);
        e.undo().unwrap();
        assert_eq!(e.project(), &project);
        e.redo().unwrap();
        assert_eq!(e.project(), &expected);
    }
}

#[test]
fn cancelled_mapped_spatial_drags_never_commit_on_late_release() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for floating in [false, true] {
            for reason in [
                "escape",
                "focus",
                "revision",
                "generation",
                "clock",
                "lock",
                "copy",
            ] {
                let e = mapped(reverse);
                let before = e.snapshot();
                let state = view(&e);
                let mut p = setup(Locale::Zh, "property", &state);
                let (paint, _) = draw(&mut p, &state, Input::default(), floating, &mut atlas);
                let at = at_time(&paint, &p, selected_time(&state));
                let ppf = p.timeline.zoom_track.unwrap().width() / 120. * p.timeline.zoom;
                let to = [at[0] + 3. * ppf, at[1]];
                let modifiers = Modifiers {
                    control: reason == "copy",
                    ..delta_input()
                };
                let (_, out) = draw(
                    &mut p,
                    &state,
                    pointer(at, true, false, modifiers),
                    floating,
                    &mut atlas,
                );
                selection_only(&out);
                let (_, out) = draw(
                    &mut p,
                    &state,
                    pointer(to, false, false, modifiers),
                    floating,
                    &mut atlas,
                );
                selection_only(&out);
                let mut changed = state.clone();
                let mut input = pointer(to, false, false, modifiers);
                match reason {
                    "escape" => input
                        .keys
                        .push((motion_ui::input::Key::Escape, Modifiers::default())),
                    "focus" => input.focus_lost = true,
                    "revision" => changed["revision"] = json!(999),
                    "generation" => changed["document_generation"] = json!(999),
                    "clock" => {
                        changed["revision"] = json!(999);
                        changed["project"]["layers"][0]["time_mapping"]["clock"]["local_pivot"] =
                            json!(1.);
                    }
                    "lock" => changed["project"]["layers"][0]["locked"] = json!(true),
                    "copy" => {}
                    _ => unreachable!(),
                }
                let (_, out) = draw(&mut p, &changed, input, floating, &mut atlas);
                selection_only(&out);
                assert!(!p.key_gestures.dragging(), "{reason}");
                let (_, out) = draw(
                    &mut p,
                    &changed,
                    pointer(to, false, true, modifiers),
                    floating,
                    &mut atlas,
                );
                selection_only(&out);
                let (_, out) = draw(
                    &mut p,
                    &changed,
                    Input {
                        mouse: [to[0] + 100., to[1]],
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                );
                selection_only(&out);
                assert_eq!(e.project(), &before);
                assert!(!e.can_undo());
            }
        }
    }
}

#[test]
fn malformed_spatial_projection_is_rejected_and_uses_default_failure_outputs() {
    const NAME: &str = "panels::key_selection::tests::ui::spatial::mapped::malformed_spatial_projection_is_rejected_and_uses_default_failure_outputs";
    const CHILD: &str = "MOTION_MAPPED_SPATIAL_PLAN_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _log = crate::logging::init(root.clone()).unwrap();
        for reverse in [false, true] {
            let e = mapped(reverse);
            let mut state = view(&e);
            let before = e.snapshot();
            let time = selected_time(&state);
            state["timeline_layers"][0]["properties"]["position"]["spatial_keys"][1]["frame"] =
                json!(time + 0.125);
            let mut p = setup(Locale::Zh, "summary", &state);
            p.selected_keys
                .apply_addressed(&state, [identity(2)], Mode::Replace)
                .unwrap();
            let mut out = vec![];
            assert!(shortcut(
                &mut p,
                &state,
                crate::shortcuts::Command::NudgeKeyLater,
                &mut out
            ));
            assert!(out.is_empty());
            assert!(p
                .message
                .as_ref()
                .unwrap()
                .contains("identities or timing differ"));
            assert_eq!(e.project(), &before);
        }
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_MAPPED_SPATIAL_PLAN_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| tmp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &out.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &out.stderr).unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    for text in [&*file, &*stderr] {
        assert_eq!(
            text.lines()
                .filter(
                    |line| line.contains("key transform failed operation=prepare")
                        && line.contains("identities or timing differ")
                )
                .count(),
            2
        );
    }
    assert!(!stdout.contains("key transform failed") && !stdout.contains("motion_studio::"));
}
