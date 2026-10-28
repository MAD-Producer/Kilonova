use super::*;
use crate::FloatWindow;
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, MouseButton as NativeButton, WindowEvent},
};

#[test]
#[ignore = "manual native project asset drag screenshots and worker transactions"]
fn project_asset_native_drag_maps_zoomed_tracks_in_both_languages_and_window_owners() {
    const CHILD: &str = "MOTION_PROJECT_DRAG_NATIVE_CHILD";
    const NAME: &str = "panels::project::tests::native::drag::project_asset_native_drag_maps_zoomed_tracks_in_both_languages_and_window_owners";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_PROJECT_DRAG_NATIVE_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
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
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let project_root = root.join("project");
    std::fs::create_dir_all(project_root.join("assets")).unwrap();
    for id in 1..=3 {
        image::RgbaImage::from_pixel(4, 4, image::Rgba([80, 120, 180, 255]))
            .save(project_root.join(format!("assets/source-{id}.png")))
            .unwrap();
    }
    let project: Project = serde_json::from_value(fixture()["project"].clone()).unwrap();
    motion_core::storage::save(&project_root, &project).unwrap();
    let mut shell = Shell::new(project_root.clone(), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_position(PhysicalPosition::new(0, 0))
                    .with_inner_size(PhysicalSize::new(1500, 900)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let floating = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_position(PhysicalPosition::new(1800, 0))
                    .with_inner_size(PhysicalSize::new(1200, 840)),
            )
            .unwrap(),
    );
    let attached = shell.engine.attach_async(main.clone(), 1500, 900).unwrap();
    shell.state = native_test_support::receive(&mut events, &attached, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    shell
        .engine
        .float_window(1, floating.clone(), 1200, 840)
        .unwrap();
    shell.window = Some(main.clone());
    shell.attached = true;
    shell.scale = 1.5;
    shell.floating.insert(
        floating.id(),
        FloatWindow {
            window: floating.clone(),
            key: 1,
            panel: Panel::Project,
            input: Default::default(),
            paint_input: Default::default(),
            pointer_position: Default::default(),
            scale: 2.,
        },
    );
    let mut cases = Vec::new();
    for (locale, language) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        for (source_owner, target_owner, case) in [
            (None, None, "main"),
            (None, Some(Panel::Timeline), "main-to-floating"),
            (Some(Panel::Project), None, "floating-to-main"),
        ] {
            shell.panels.locale = locale;
            shell.workspace.locale = locale;
            shell.panels.project.browser = Controls::default();
            shell.panels.project.query.clear();
            shell.panels.timeline.graph_mode = false;
            shell.panels.timeline.zoom = 3.;
            shell.panels.timeline.offset = 20.;
            shell.workspace.root = if source_owner.is_none() && target_owner.is_none() {
                Node::Split {
                    edge: dock::Edge::Left,
                    split: 0.3,
                    first: Box::new(Node::dock(Panel::Project.dock())),
                    second: Box::new(Node::dock(Panel::Timeline.dock())),
                }
            } else {
                Node::dock(
                    if source_owner.is_some() {
                        Panel::Timeline
                    } else {
                        Panel::Project
                    }
                    .dock(),
                )
            };
            shell.floating.get_mut(&floating.id()).unwrap().panel = if target_owner.is_some() {
                Panel::Timeline
            } else {
                Panel::Project
            };
            let source_size = if source_owner.is_some() {
                [600., 420.]
            } else {
                [1000., 600.]
            };
            let target_size = if target_owner.is_some() {
                [600., 420.]
            } else {
                [1000., 600.]
            };
            frame(&mut shell, source_owner, source_size, Input::default());
            let row = rect(&shell.panels, &RowKey::Item(ProjectItem::Image { id: 1 }));
            let (_, actions) = frame(
                &mut shell,
                source_owner,
                source_size,
                click(row, MouseButton::Left),
            );
            run(&mut shell, &mut events, actions);
            assert!(shell.panels.project_drag.pending());
            frame(&mut shell, target_owner, target_size, Input::default());
            let (track, offset, ppf, frames) = shell
                .panels
                .project_drag
                .track_geometry()
                .expect("timeline drop track");
            let target = [
                track.min[0] + track.width() * 0.35,
                track.min[1] + track.height() * 0.5,
            ];
            let expected = (offset + (target[0] - track.min[0]) / ppf)
                .round()
                .clamp(0., frames.saturating_sub(1) as f32) as u32;
            let source_window = if source_owner.is_some() {
                &floating
            } else {
                &main
            };
            let target_window = if target_owner.is_some() {
                &floating
            } else {
                &main
            };
            let scale = if target_owner.is_some() { 2. } else { 1.5 };
            let from = source_window.inner_position().unwrap();
            let to = target_window.inner_position().unwrap();
            let position = PhysicalPosition::new(
                to.x as f64 + target[0] as f64 * scale - from.x as f64,
                to.y as f64 + target[1] as f64 * scale - from.y as f64,
            );
            let source_id = source_window.id();
            assert!(shell.route_project_drag_event(
                source_id,
                &WindowEvent::CursorMoved {
                    device_id: DeviceId::dummy(),
                    position
                }
            ));
            let (paint, actions) = frame(&mut shell, target_owner, target_size, Input::default());
            assert!(actions.is_empty());
            let old_scale = shell.scale;
            shell.scale = scale as f32;
            capture(
                &mut shell,
                &mut events,
                paint,
                target_size,
                u64::from(target_owner.is_some()),
                root.join(format!("{language}-{case}-drag.png")),
            );
            shell.scale = old_scale;
            assert!(shell.route_project_drag_event(
                source_id,
                &WindowEvent::MouseInput {
                    device_id: DeviceId::dummy(),
                    state: ElementState::Released,
                    button: NativeButton::Left
                }
            ));
            let (_, actions) = frame(&mut shell, target_owner, target_size, Input::default());
            assert!(
                matches!(actions.as_slice(), [Action::ProjectPlace { at_frame, .. }] if *at_frame==expected),
                "wrong drag action language={language} case={case} expected={expected} actions={actions:?}"
            );
            run(&mut shell, &mut events, actions);
            assert!(!shell.panels.project_drag.pending());
            let changed: Project = serde_json::from_value(shell.state["project"].clone()).unwrap();
            assert_eq!(changed.assets, project.assets);
            assert_eq!(changed.layers.len(), 1);
            assert_eq!(
                changed.layers[0].timeline.as_ref().unwrap().in_frame,
                i32::try_from(expected).unwrap()
            );
            shell.engine.save().unwrap();
            assert_eq!(motion_core::storage::load(&project_root).unwrap(), changed);
            let (paint, actions) = frame(&mut shell, target_owner, target_size, Input::default());
            assert!(actions.is_empty(), "release placed the asset twice");
            shell.scale = scale as f32;
            capture(
                &mut shell,
                &mut events,
                paint,
                target_size,
                u64::from(target_owner.is_some()),
                root.join(format!("{language}-{case}-placed.png")),
            );
            shell.scale = old_scale;
            let restored = shell.engine.history(0).unwrap();
            shell.accept_edit_state(restored);
            assert_eq!(
                serde_json::from_value::<Project>(shell.state["project"].clone()).unwrap(),
                project
            );
            cases.push(serde_json::json!({"language":language,"route":case,"frame":expected,"offset":offset,"pixels_per_frame":ppf,"source_scale":if source_owner.is_some(){2.}else{1.5},"target_scale":scale,"undo":true,"save_reopen":true}));
        }
    }
    std::fs::write(
        root.join("cases.json"),
        serde_json::to_vec_pretty(&cases).unwrap(),
    )
    .unwrap();
    shell.engine.close_window(1).unwrap();
    shell.floating.clear();
}
