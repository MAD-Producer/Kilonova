use super::*;

#[test]
fn cell_observer_history_is_bounded_cancellable_and_cannot_edit_its_neighbours() {
    use motion_host::ops::preview::{CameraGesture as Op, CameraMode};
    let source = crate::layer_clipboard::KeyframeSource::of(
        &serde_json::json!({"root":"view-history","composition":"main","document_generation":1,"revision":7}),
    );
    let mut settings = Settings::default();
    settings.set_layout(Layout::Four);
    let before = settings.clone();
    let mut history = CameraHistory::default();
    let view = settings.view_mut(3).unwrap();
    for id in 1..=70 {
        history
            .apply(view, source.clone(), 0, Op::Begin { id }, [640, 360])
            .unwrap();
        history
            .apply(
                view,
                source.clone(),
                0,
                Op::Update {
                    id,
                    mode: CameraMode::Orbit,
                    delta: [10., -5.],
                    viewport: [640, 360],
                },
                [640, 360],
            )
            .unwrap();
        history
            .apply(
                view,
                source.clone(),
                0,
                Op::End { id, commit: true },
                [640, 360],
            )
            .unwrap();
    }
    assert_eq!(history.undo.len(), 64);
    assert!(history.redo.is_empty());
    let changed = view.clone();
    history
        .apply(view, source.clone(), 0, Op::Undo, [640, 360])
        .unwrap();
    assert_ne!(*view, changed);
    history
        .apply(view, source.clone(), 0, Op::Redo, [640, 360])
        .unwrap();
    assert_eq!(*view, changed);
    history
        .apply(view, source.clone(), 0, Op::Begin { id: 90 }, [640, 360])
        .unwrap();
    history
        .apply(
            view,
            source.clone(),
            0,
            Op::Update {
                id: 90,
                mode: CameraMode::Pan,
                delta: [15., 20.],
                viewport: [640, 360],
            },
            [640, 360],
        )
        .unwrap();
    history
        .apply(
            view,
            source.clone(),
            0,
            Op::End {
                id: 90,
                commit: false,
            },
            [640, 360],
        )
        .unwrap();
    assert_eq!(*view, changed);
    history
        .apply(view, source.clone(), 0, Op::Begin { id: 91 }, [640, 360])
        .unwrap();
    history
        .apply(
            view,
            source.clone(),
            0,
            Op::Update {
                id: 91,
                mode: CameraMode::Dolly,
                delta: [0., -45.],
                viewport: [640, 360],
            },
            [640, 360],
        )
        .unwrap();
    assert_ne!(*view, changed);
    history
        .apply(
            view,
            source,
            1,
            Op::End {
                id: 91,
                commit: true,
            },
            [640, 360],
        )
        .unwrap();
    assert_eq!(*view, changed);
    assert!(history.undo.is_empty());
    for i in 0..3 {
        assert_eq!(settings.view(i), before.view(i));
    }
}
use crate::{i18n::Locale, workspace::Workspace};
use motion_ui::dock::Panel;
use serde_json::{json, Value};

fn context(window: u64) -> Context {
    Context {
        window,
        document: 3,
        composition: 7,
        revision: 11,
        frame: 12.5f64.to_bits(),
    }
}
fn viewport() -> Rect {
    Rect::new(20., 60., 1200., 800.)
}
fn point(tile: Rect) -> [f32; 2] {
    [
        (tile.min[0] + tile.max[0]) * 0.5,
        (tile.min[1] + tile.max[1]) * 0.5,
    ]
}

#[test]
fn tile_hit_regions_are_disjoint_bounded_and_exclude_gaps_in_small_and_fractional_windows() {
    for size in [
        [0., 0.],
        [1., 1.],
        [2., 2.],
        [3., 50.],
        [211.5, 407.25],
        [1600., 900.],
    ] {
        for layout in [Layout::Single, Layout::Two, Layout::Four] {
            for gap in [0., 1., 1.5, 8.] {
                let viewport = Rect::new(20., 60., size[0], size[1]);
                let tiles = layout.tiles(viewport, gap).unwrap();
                assert!(tiles.is_empty() || tiles.len() == layout.count());
                for (i, tile) in tiles.iter().enumerate() {
                    assert!(viewport.contains_rect(*tile));
                    assert!(tile.width() > 0. && tile.height() > 0.);
                    assert_eq!(layout.hit(viewport, gap, point(*tile)), Ok(Some(i)));
                    for (j, other) in tiles.iter().enumerate().filter(|(j, _)| *j != i) {
                        assert!(
                            !(tile.min[0] < other.max[0]
                                && tile.max[0] > other.min[0]
                                && tile.min[1] < other.max[1]
                                && tile.max[1] > other.min[1]),
                            "overlap {i}/{j}"
                        );
                    }
                }
                assert_eq!(layout.hit(viewport, gap, viewport.max), Ok(None));
                if gap > 0. && tiles.len() > 1 {
                    assert_eq!(
                        layout.hit(
                            viewport,
                            gap,
                            [tiles[0].max[0] + gap * 0.5, point(tiles[0])[1]]
                        ),
                        Ok(None)
                    );
                }
            }
        }
    }
    for gap in [-1., f32::NAN, f32::INFINITY] {
        assert!(Layout::Four.tiles(viewport(), gap).is_err());
    }
    assert!(Layout::Four
        .tiles(Rect::new(f32::NAN, 0., 100., 100.), 1.)
        .is_err());
    assert_eq!(Layout::Four.hit(viewport(), 1., [f32::NAN, 0.]), Ok(None));
    assert!(Layout::Four
        .tiles(
            Rect {
                min: [-f32::MAX, 0.],
                max: [f32::MAX, 100.]
            },
            1.
        )
        .is_err());
    assert!(Layout::Two
        .tiles(
            Rect {
                min: [16_777_216., 0.],
                max: [16_777_218., 100.]
            },
            0.
        )
        .unwrap()
        .is_empty());
}

#[test]
fn navigation_gestures_only_change_the_captured_tile_and_foreign_owners_cannot_steal_them() {
    for owner in [0, 19] {
        let mut views = Views::default();
        views.set_layout(Layout::Four);
        let original = views.settings.clone();
        let tiles = Layout::Four.tiles(viewport(), 2.).unwrap();
        for (index, tile) in tiles.iter().enumerate() {
            assert!(views
                .begin(context(owner), viewport(), 2., point(*tile))
                .unwrap());
            assert_eq!(views.settings.active(), index);
            let captured = views.captured(context(owner), viewport(), 2.).unwrap();
            assert!(captured
                .navigation
                .zoom_at(*tile, [1920., 1080.], point(*tile), 2.));
            assert!(captured.navigation.pan(*tile, [1920., 1080.], [17., -31.]));
            for other in 0..4 {
                assert_eq!(
                    views.settings.views[other] == original.views[other],
                    other != index
                );
            }
            assert!(views.captured(context(owner + 1), viewport(), 2.).is_none());
            assert!(!views
                .begin(
                    context(owner + 1),
                    viewport(),
                    2.,
                    point(tiles[(index + 1) % 4])
                )
                .unwrap());
            assert!(!views.release(context(owner + 1), viewport(), 2.));
            assert_eq!(views.settings.active(), index);
            assert!(views.cancel());
            assert_eq!(views.settings.views, original.views);
            assert!(!views.cancel());
        }
        assert!(views
            .begin(context(owner), viewport(), 2., point(tiles[2]))
            .unwrap());
        views
            .captured(context(owner), viewport(), 2.)
            .unwrap()
            .navigation
            .pan(tiles[2], [1920., 1080.], [40., 10.]);
        let committed = views.settings.clone();
        assert!(views.release(context(owner), viewport(), 2.));
        assert!(!views.cancel());
        assert_eq!(views.settings, committed);
    }
}

#[test]
fn stale_document_frame_revision_layout_and_geometry_roll_back_before_further_input() {
    let mut views = Views::default();
    views.set_layout(Layout::Four);
    let tile = Layout::Four.tiles(viewport(), 2.).unwrap()[3];
    let variants = [
        Context {
            document: 4,
            ..context(0)
        },
        Context {
            composition: 8,
            ..context(0)
        },
        Context {
            revision: 12,
            ..context(0)
        },
        Context {
            frame: 13f64.to_bits(),
            ..context(0)
        },
    ];
    for changed in variants {
        let before = views.settings.clone();
        assert!(views
            .begin(context(0), viewport(), 2., point(tile))
            .unwrap());
        assert!(views
            .captured(context(0), viewport(), 2.)
            .unwrap()
            .navigation
            .pan(tile, [1920., 1080.], [20., -10.]));
        assert!(views.captured(changed, viewport(), 2.).is_none());
        assert_eq!(views.settings.views, before.views);
        assert!(!views.cancel());
    }
    for (changed, gap) in [(Rect::new(20., 60., 1201., 800.), 2.), (viewport(), 3.)] {
        let before = views.settings.clone();
        assert!(views
            .begin(context(0), viewport(), 2., point(tile))
            .unwrap());
        views
            .captured(context(0), viewport(), 2.)
            .unwrap()
            .navigation
            .pan(tile, [1920., 1080.], [20., -10.]);
        assert!(views.captured(context(0), changed, gap).is_none());
        assert_eq!(views.settings.views, before.views);
    }
    assert!(views
        .begin(context(0), viewport(), 2., point(tile))
        .unwrap());
    views
        .captured(context(0), viewport(), 2.)
        .unwrap()
        .navigation
        .pan(tile, [1920., 1080.], [20., -10.]);
    views.set_layout(Layout::Single);
    assert_eq!(views.settings.layout(), Layout::Single);
    assert_eq!(views.settings.active(), 0);
    assert_eq!(views.settings.views, Settings::default().views);
    views.set_layout(Layout::Four);
    assert_eq!(views.settings.view(3).unwrap().mode, Mode::Free);
    assert!(views
        .begin(context(0), viewport(), 2., point(tile))
        .unwrap());
    views
        .captured(context(0), viewport(), 2.)
        .unwrap()
        .navigation
        .pan(tile, [1920., 1080.], [20., -10.]);
    assert!(views.set_mode(3, Mode::Top));
    assert_eq!(views.settings.view(3).unwrap().mode, Mode::Top);
    assert_eq!(
        views.settings.view(3).unwrap().navigation,
        Navigation::default()
    );
    assert!(!views.cancel());
    let before = views.settings.clone();
    assert!(!views.set_mode(4, Mode::Free));
    assert_eq!(views.settings, before);
}

#[test]
fn workspace_round_trip_keeps_layout_each_view_and_native_panel_ownership_in_both_locales() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("workspace.json");
    for locale in [Locale::Zh, Locale::En] {
        let mut workspace = Workspace::default();
        workspace.locale = locale;
        workspace.viewer_views.set_layout(Layout::Four);
        workspace.viewer_views.active = 2;
        let tiles = Layout::Four.tiles(viewport(), 2.).unwrap();
        for (i, tile) in tiles.iter().enumerate() {
            let view = workspace.viewer_views.view_mut(i).unwrap();
            assert!(view.navigation.zoom_at(
                *tile,
                [1920., 1080.],
                point(*tile),
                0.25 * (i + 1) as f64
            ));
            assert!(view
                .navigation
                .pan(*tile, [1920., 1080.], [i as f32 * 15., -10.]));
        }
        workspace.viewer_views.view_mut(0).unwrap().mode = Mode::Camera(73);
        let mut observer = Observer::new(1920, 1080);
        observer.orbit(18., -14.).unwrap();
        observer.zoom(1.5).unwrap();
        workspace.viewer_views.view_mut(3).unwrap().orbit = Some(Orbit::capture(&observer));
        assert!(workspace.float(Panel::Composition.dock()));
        workspace.save(&path).unwrap();
        let opened = Workspace::load(&path);
        assert_eq!(opened.viewer_views, workspace.viewer_views);
        assert_eq!(opened.locale, locale);
        assert!(opened.floating.contains(&Panel::Composition.dock()));
        assert!(opened.valid());
        let raw: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(raw.get("project").is_none());
        let mut missing = raw;
        missing.as_object_mut().unwrap().remove("viewer_views");
        assert_eq!(
            serde_json::from_value::<Workspace>(missing)
                .unwrap()
                .viewer_views,
            Settings::default()
        );
    }
}

#[test]
fn invalid_saved_settings_reject_partial_navigation_unknown_modes_and_invalid_observer_values() {
    let base = serde_json::to_value(Settings::default()).unwrap();
    for (path, value) in [
        ("/layout", json!("three")),
        ("/active", json!(1)),
        ("/views", json!([])),
        ("/views/0/mode", json!({"kind":"missing"})),
        ("/views/0/mode", json!({"kind":"camera","object":-1})),
        ("/views/0/navigation", json!({"zoom":0.,"center":[0.,0.]})),
        ("/views/0/navigation", json!({"zoom":65.,"center":[0.,0.]})),
        ("/views/0/navigation", json!({"zoom":1.,"center":null})),
        ("/views/0/navigation", json!({"zoom":null,"center":[0.,0.]})),
    ] {
        let mut bad = base.clone();
        *bad.pointer_mut(path).unwrap() = value;
        assert!(
            serde_json::from_value::<Settings>(bad).is_err(),
            "accepted {path}"
        );
    }
    let orbit = serde_json::to_value(Orbit::capture(&Observer::new(1920, 1080))).unwrap();
    for (key, value) in [
        ("radius", json!(0.)),
        ("elevation", json!(90.)),
        ("fov", json!(121.)),
        ("target", json!([10000001., 0., 0.])),
    ] {
        let mut bad = base.clone();
        let mut orbit = orbit.clone();
        orbit[key] = value;
        bad["views"][0]["orbit"] = orbit;
        assert!(
            serde_json::from_value::<Settings>(bad).is_err(),
            "accepted orbit {key}"
        );
    }
    let mut bad = base.clone();
    bad["unexpected"] = true.into();
    assert!(serde_json::from_value::<Settings>(bad).is_err());
    let mut settings = Settings::default();
    settings.set_layout(Layout::Four);
    settings.view_mut(0).unwrap().mode = Mode::Camera(0);
    assert_eq!(
        serde_json::from_value::<Settings>(serde_json::to_value(&settings).unwrap()).unwrap(),
        settings
    );
}

#[test]
fn observer_views_sample_independent_actual_scene_matrices_without_mutating_the_project() {
    let folder = tempfile::tempdir().unwrap();
    let engine = crate::engine::Engine::start(
        folder.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let mut layer =
        motion_core::Layer::solid(7, "Spatial", [200., 100.], [960., 540., 80.], [1.; 4]);
    layer.three_d = true;
    let state = engine
        .edit(serde_json::to_string(&motion_core::Command::Add { layer }).unwrap())
        .unwrap();
    let project: motion_core::Project = serde_json::from_value(state["project"].clone()).unwrap();
    let before = serde_json::to_value(&project).unwrap();
    let mut settings = Settings::default();
    settings.set_layout(Layout::Four);
    let mut matrices = vec![];
    for i in 0..4 {
        let observer = settings
            .view(i)
            .unwrap()
            .observer([project.width, project.height])
            .unwrap();
        let mut scene = motion_render::Scene::new(&project).unwrap();
        scene
            .sample(
                &project,
                0.,
                observer.as_ref(),
                &motion_core::ExpressionEvaluator,
            )
            .unwrap();
        matrices.push(
            scene
                .layers
                .iter()
                .find(|layer| layer.id == 7)
                .unwrap()
                .view_projection,
        );
    }
    assert_ne!(matrices[0], matrices[1]);
    assert_ne!(matrices[1], matrices[2]);
    let mut observer = settings
        .view(3)
        .unwrap()
        .observer([1920, 1080])
        .unwrap()
        .unwrap();
    observer.orbit(27., -15.).unwrap();
    settings.view_mut(3).unwrap().orbit = Some(Orbit::capture(&observer));
    for i in 0..4 {
        let observer = settings
            .view(i)
            .unwrap()
            .observer([project.width, project.height])
            .unwrap();
        let mut scene = motion_render::Scene::new(&project).unwrap();
        scene
            .sample(
                &project,
                0.,
                observer.as_ref(),
                &motion_core::ExpressionEvaluator,
            )
            .unwrap();
        let actual = scene
            .layers
            .iter()
            .find(|layer| layer.id == 7)
            .unwrap()
            .view_projection;
        assert_eq!(actual == matrices[i], i != 3);
    }
    assert_eq!(serde_json::to_value(project).unwrap(), before);
    assert_eq!(engine.state().unwrap()["project"], state["project"]);
}

#[test]
fn invalid_workspace_view_state_preserves_input_and_reports_default_channels_with_recovery_reset() {
    use std::{
        path::PathBuf,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    const CHILD: &str = "MOTION_VIEWER_STATE_CHILD";
    const NAME:&str="viewer_views::tests::invalid_workspace_view_state_preserves_input_and_reports_default_channels_with_recovery_reset";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let path = root.join("workspace.json");
        let mut bad = serde_json::to_value(Workspace::default()).unwrap();
        bad["viewer_views"]["active"] = 99.into();
        let bytes = serde_json::to_vec_pretty(&bad).unwrap();
        std::fs::write(root.join("invalid-workspace.json"), &bytes).unwrap();
        for _ in 0..2 {
            std::fs::write(&path, &bytes).unwrap();
            for _ in 0..61 {
                assert_eq!(Workspace::load(&path).viewer_views, Settings::default());
            }
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            let mut recovered = Workspace::default();
            recovered.viewer_views.set_layout(Layout::Four);
            recovered.save(&path).unwrap();
            assert_eq!(Workspace::load(&path).viewer_views.layout(), Layout::Four);
        }
        crate::logging::flush();
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_VIEWER_STATE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| folder.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG")
        .env(CHILD, &root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            std::fs::write(root.join("stdout.txt"), output.stdout).unwrap();
            std::fs::write(root.join("stderr.txt"), output.stderr).unwrap();
            panic!("view-state channel acceptance exceeded 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    std::fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for text in [file.as_str(), stderr.as_ref()] {
        assert_eq!(
            text.matches("site=workspace.load count=60").count(),
            2,
            "{text}"
        );
        assert!(text.contains("invalid multi-view settings"));
        assert!(text.contains("phase=json"));
    }
    assert_eq!(file.matches("site=workspace.load count=1 ").count(), 2);
    assert!(!stderr.contains("site=workspace.load count=1 "));
    assert!(!stdout.contains("site=workspace.load"));
}
