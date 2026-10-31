//! Real main/floating GPU surfaces, production chrome and actual worker edits.
//! Input is supplied at the normalized boundary, not injected into the OS.
use super::*;
use crate::{
    Shell,
    engine::{AtlasUpload, UiFrame},
    native_test_support,
};
use motion_ui::dock::{Edge, Node, Panel};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

mod owner;
fn project() -> motion_core::Project {
    let mut project = fixture().project().clone();
    project.layers[0].name = "Boundary study".into();
    project.layers[0].content = motion_core::Content::Solid {
        color: [0.18, 0.48, 0.78, 1.],
    };
    project.layers[0].masks[0].expansion = Track::constant(6.);
    let mut parent = Layer::solid(9, "Parent", [1.; 2], [160., 90., 0.], [1.; 4]);
    parent.content = motion_core::Content::Null;
    project.layers.push(parent);
    let mut engine = motion_core::Engine::new(project).unwrap();
    engine
        .apply(Command::ParentWithMode {
            object: 7,
            parent: Some(9),
            frame: 33,
            mode: motion_core::ParentMode::PreserveWorld,
        })
        .unwrap();
    engine
        .apply(Command::SetVector {
            object: 9,
            property: motion_core::Property::Rotation,
            frame: 33,
            value: [0., 0., 12.],
        })
        .unwrap();
    engine
        .apply(Command::SetVector {
            object: 9,
            property: motion_core::Property::Scale,
            frame: 33,
            value: [90., 90., 100.],
        })
        .unwrap();
    engine.project().clone()
}
fn draw(shell: &mut Shell, size: [f32; 2], window: u64, input: &Input) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    paint.stylesheet = shell.panels.styles.clone();
    let actions = if window == 0 {
        crate::panels::chrome(
            &mut paint,
            &mut shell.atlas,
            size,
            shell.workspace.display_root(),
            &mut shell.panels,
            &shell.state,
            input,
        )
    } else {
        crate::panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Composition,
            size,
            &mut shell.panels,
            &shell.state,
            input,
        )
    };
    (paint, actions)
}
fn view(shell: &Shell, size: [f32; 2], window: u64, paint: &PaintList) -> Rect {
    if window == 0 {
        crate::panels::composition_view_styled(
            shell.workspace.display_root(),
            size,
            &paint.styles(),
        )
    } else {
        crate::panels::floating_view_styled(Panel::Composition, size, &paint.styles()).unwrap()
    }
}
fn capture(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    size: [f32; 2],
    window: u64,
    paint: PaintList,
    path: PathBuf,
) {
    let rect = view(shell, size, window, &paint);
    let scale = shell.scale;
    let config = shell.atlas.config();
    let ui = UiFrame { views: None, inspection: Default::default(),
        paint,
        atlas: Some(AtlasUpload {
            pixels: shell.atlas.pixel_data().to_vec(),
            width: config.width,
            height: config.height,
        }),
        logical_size: size,
        composition: Some([
            rect.min[0] * scale,
            rect.min[1] * scale,
            rect.width() * scale,
            rect.height() * scale,
        ]),
        composition_image: shell.viewer_image(rect, scale),
        window,
        node_preview: None,
    };
    assert!(ui.composition_image.is_some());
    // CaptureUi reads the composed scratch texture. Render the scene first:
    // overlay-only changes cannot prove mask rendering.
    let front = shell
        .engine
        .present_async(UiFrame { views: None, inspection: Default::default(),
            paint: PaintList::default(),
            atlas: None,
            logical_size: size,
            composition: ui.composition,
            composition_image: ui.composition_image,
            window,
            node_preview: None,
        })
        .unwrap();
    assert!(
        native_test_support::receive(events, &front, Duration::from_secs(60))
            .unwrap()
            .unwrap()
    );
    native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        ((size[0] * scale) as u32, (size[1] * scale) as u32)
    );
}
fn dispatch(shell: &mut Shell, events: &mut EventLoop<()>, actions: Vec<Action>) {
    for action in actions {
        shell.action(action, None).unwrap();
        if let Some((_, reply)) = shell.editing.take() {
            let state = native_test_support::receive(events, &reply, Duration::from_secs(30))
                .unwrap()
                .unwrap();
            shell.accept_edit_state(state);
        }
    }
}
fn current_mask(shell: &Shell) -> LayerMask {
    serde_json::from_value::<motion_core::Project>(shell.state["project"].clone())
        .unwrap()
        .layers[0]
        .masks[0]
        .clone()
}
fn screenshot_diff(a: &Path, b: &Path) -> usize {
    let a = image::open(a).unwrap().to_rgba8();
    let b = image::open(b).unwrap().to_rgba8();
    assert_eq!(a.dimensions(), b.dimensions());
    a.pixels().zip(b.pixels()).filter(|(a, b)| a != b).count()
}
fn blue_pixels(path: &Path) -> usize {
    image::open(path)
        .unwrap()
        .to_rgba8()
        .pixels()
        .filter(|p| {
            u16::from(p[2]) > u16::from(p[0]) + 50 && u16::from(p[1]) > u16::from(p[0]) + 20
        })
        .count()
}

#[test]
#[ignore = "manual Windows GPU mask viewer matrix; explicit normalized input"]
fn native_boundary_viewer_main_and_independent_float_render_and_transactions() {
    let temporary = tempfile::tempdir().unwrap();
    let output = std::env::var_os("MOTION_MASK_VIEWER_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("evidence"));
    std::fs::create_dir_all(&output).unwrap();
    let _logging = crate::logging::init(output.join("logs")).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let size = [1100., 700.];
    let mut manifest = vec![];
    for (locale, language) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        for scale in [1., 1.5, 2.] {
            let directory = temporary.path().join(format!("{language}-{scale}"));
            let mut project = project();
            project.layers[0].masks[0].inverted = scale == 1.5;
            motion_core::storage::save(&directory, &project).unwrap();
            let mut shell = Shell::new(directory.clone(), None, Some(locale)).unwrap();
            shell.home = None;
            shell.scale = scale;
            shell.panels.select_only(Some(7));
            shell.panels.focused = Some(Panel::Composition);
            shell.panels.properties.transform_collapsed = true;
            shell.panels.path_controls.tool = Tool::Mask;
            shell.panels.path_controls.mask = Some(1);
            shell.panels.path_controls.masks_open = true;
            shell.workspace.root = Node::Split {
                edge: Edge::Right,
                split: 0.28,
                first: Box::new(Node::dock(Panel::Composition.dock())),
                second: Box::new(Node::dock(Panel::Properties.dock())),
            };
            let pixels = PhysicalSize::new((size[0] * scale) as u32, (size[1] * scale) as u32);
            #[allow(deprecated)]
            let main = Arc::new(
                events
                    .create_window(
                        Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(pixels),
                    )
                    .unwrap(),
            );
            #[allow(deprecated)]
            let float = Arc::new(
                events
                    .create_window(
                        Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(pixels),
                    )
                    .unwrap(),
            );
            let reply = shell
                .engine
                .attach_async(main.clone(), pixels.width, pixels.height)
                .unwrap();
            shell.state =
                native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
                    .unwrap()
                    .unwrap();
            shell.window = Some(main);
            shell.attached = true;
            shell
                .engine
                .float_window(1, float.clone(), pixels.width, pixels.height)
                .unwrap();
            shell.atlas.set_pixel_ratio(scale);
            shell.state = shell.engine.seek(33.).unwrap();
            for window in [0, 1] {
                assert!(!shell.panels.path_controls.cancel());
                shell.panels.composition.navigation.fit();
                let (paint, actions) = draw(&mut shell, size, window, &Input::default());
                assert!(actions.is_empty(), "initial draw: {actions:?}");
                let viewport = view(&shell, size, window, &paint);
                let center = [
                    (viewport.min[0] + viewport.max[0]) * 0.5,
                    (viewport.min[1] + viewport.max[1]) * 0.5,
                ];
                let zoom = if window == 0 { 1.25 } else { 1.75 };
                assert!(shell.panels.composition.navigation.zoom_at(
                    viewport,
                    [320., 180.],
                    center,
                    zoom
                ));
                let (_paint, actions) = draw(&mut shell, size, window, &Input::default());
                assert!(actions.is_empty());
                let before = shell.state["project"].clone();
                let before_mask = current_mask(&shell);
                let mut paths = vec![];
                for feather in [true, false] {
                    let (paint, actions) = draw(&mut shell, size, window, &Input::default());
                    assert!(actions.is_empty());
                    let grip = shell
                        .panels
                        .path_controls
                        .boundaries
                        .grips
                        .iter()
                        .find(|g| {
                            matches!(g.property, MaskProperty::Feather) == feather
                                && g.axis == 0
                                && g.sign > 0.
                        })
                        .unwrap()
                        .clone();
                    let mapping = shell
                        .panels
                        .composition
                        .navigation
                        .mapping(viewport, [320., 180.])
                        .unwrap();
                    let projection = layer_projection(&shell.state, 7, mapping.image).unwrap();
                    let prefix = format!(
                        "{language}-{}-{scale}x-{}",
                        if window == 0 { "main" } else { "float" },
                        if feather { "feather" } else { "expansion" }
                    );
                    let initial = output.join(format!("{prefix}-before.png"));
                    capture(
                        &mut shell,
                        &mut events,
                        size,
                        window,
                        paint,
                        initial.clone(),
                    );
                    assert!(
                        blue_pixels(&initial) > 5000,
                        "actual mask content must be rendered before capture"
                    );
                    let (paint, actions) = draw(
                        &mut shell,
                        size,
                        window,
                        &Input {
                            mouse: grip.screen,
                            ..Default::default()
                        },
                    );
                    assert!(actions.is_empty());
                    let hover = output.join(format!("{prefix}-hover.png"));
                    capture(&mut shell, &mut events, size, window, paint, hover.clone());
                    assert!(
                        screenshot_diff(&initial, &hover) > 5,
                        "hover feedback absent"
                    );
                    let (_, actions) = draw(
                        &mut shell,
                        size,
                        window,
                        &Input {
                            mouse: grip.screen,
                            pressed: Some((MouseButton::Left, grip.screen)),
                            held: Some(MouseButton::Left),
                            ..Default::default()
                        },
                    );
                    assert!(matches!(actions.as_slice(),[Action::Seek(f)] if *f==33.));
                    dispatch(&mut shell, &mut events, actions);
                    for delta in [9., 15.] {
                        let point = moved(&projection, &grip, delta);
                        let input = if delta == 9. {
                            Input {
                                mouse: point,
                                held: Some(MouseButton::Left),
                                ..Default::default()
                            }
                        } else {
                            Input {
                                mouse: [10000.; 2],
                                released: Some((MouseButton::Left, point)),
                                ..Default::default()
                            }
                        };
                        let (_, actions) = draw(&mut shell, size, window, &input);
                        assert!(!actions.is_empty());
                        dispatch(&mut shell, &mut events, actions);
                    }
                    let mask = current_mask(&shell);
                    if feather {
                        let values = mask.feather.sample(13.).unwrap();
                        close(values[0], 28.);
                        close(values[1], 45.5);
                        assert_eq!(mask.expansion, before_mask.expansion);
                        assert_eq!(mask.feather.keys.first(), before_mask.feather.keys.first());
                        assert_eq!(mask.feather.keys.last(), before_mask.feather.keys.last());
                    } else {
                        close(mask.expansion.sample(13.).unwrap(), 21.);
                        assert_eq!(mask.feather, before_mask.feather);
                    }
                    assert_eq!(mask.path, before_mask.path);
                    assert!(shell.panels.path_controls.boundaries.capture.is_none());
                    let changed = shell.state["project"].clone();
                    let (paint, actions) = draw(&mut shell, size, window, &Input::default());
                    assert!(actions.is_empty());
                    let after = output.join(format!("{prefix}-after.png"));
                    capture(&mut shell, &mut events, size, window, paint, after.clone());
                    assert!(
                        blue_pixels(&initial).abs_diff(blue_pixels(&after)) > 20,
                        "rendered mask coverage must change, not only the overlay"
                    );
                    assert!(
                        screenshot_diff(&initial, &after) > 100,
                        "rendered mask must change"
                    );
                    shell.engine.save().unwrap();
                    assert_eq!(
                        serde_json::to_value(motion_core::storage::load(&directory).unwrap())
                            .unwrap(),
                        changed
                    );
                    shell.state = shell.engine.history(0).unwrap();
                    assert_eq!(
                        shell.state["project"], before,
                        "one undo restores complete tracks"
                    );
                    shell.state = shell.engine.history(1).unwrap();
                    assert_eq!(shell.state["project"], changed);
                    shell.state = shell.engine.history(0).unwrap();
                    paths.push(serde_json::json!({"property":if feather{"feather"}else{"expansion"},"before":initial,"hover":hover,"after":after}));
                }
                manifest.push(serde_json::json!({"language":language,"window":window,"scale":scale,"zoom":zoom,
                    "inverted":project.layers[0].masks[0].inverted,"parent_rotation":12.,"screenshots":paths}));
            }
            drop(float);
        }
    }
    std::fs::write(
        output.join("matrix.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}
