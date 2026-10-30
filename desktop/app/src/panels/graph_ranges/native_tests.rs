//! Hidden native windows, actual worker snapshots, GPU pixels and saved bytes.
use super::*;
use crate::engine::{AtlasUpload, Command as WorkerCommand, Engine as DesktopEngine, UiFrame};
use motion_core::{Curve, CurveShape, CurveSpace, Ease, Easing, EffectInstance, Layer, Project};
use motion_ui::input::Modifiers;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

#[derive(Clone, Copy, Debug)]
enum Kind {
    Property,
    Effect,
    Comparison,
    Mini,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Property => "property",
            Self::Effect => "effect",
            Self::Comparison => "comparison",
            Self::Mini => "mini",
        }
    }
    fn panel(self) -> Panel {
        if matches!(self, Self::Mini) {
            Panel::Curves
        } else {
            Panel::Timeline
        }
    }
}
#[derive(Clone, Copy)]
struct Case {
    window: u64,
    size: [f32; 2],
    ratio: f32,
    locale: Locale,
    kind: Kind,
    speed: bool,
}
impl Case {
    fn name(self) -> String {
        format!(
            "{}-{}-{}-{}-{}",
            if self.locale == Locale::Zh {
                "zh"
            } else {
                "en"
            },
            if self.window == 0 { "main" } else { "float" },
            self.kind.name(),
            if self.speed { "speed" } else { "value" },
            self.ratio
        )
    }
    fn target(self) -> KeyframeTarget {
        if matches!(self.kind, Kind::Effect) {
            KeyframeTarget::Effect {
                object: 7,
                effect: 1,
                param: "speed".into(),
            }
        } else {
            KeyframeTarget::Property {
                object: 7,
                property: if matches!(self.kind, Kind::Mini) && self.speed {
                    Property::Position
                } else {
                    Property::Opacity
                },
                axis: None,
            }
        }
    }
    fn selected_frame(self) -> u32 {
        if matches!(self.kind, Kind::Effect) {
            80
        } else {
            70
        }
    }
}
fn key_input(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}
fn project_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, folder: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(folder).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            // Engine::ProjectLock owns this zero-byte OS mutex for the entire
            // session. It is not persisted authored data and Windows forbids
            // even a second read handle; all other files must remain readable.
            if path == root.join(".motion-editor.lock") {
                assert_eq!(entry.metadata().unwrap().len(), 0);
                continue;
            }
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, out);
            } else {
                assert!(entry.file_type().unwrap().is_file());
                out.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    std::fs::read(&path).unwrap_or_else(|cause| {
                        panic!("reading saved project file {}: {cause}", path.display())
                    }),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}
fn fixture(root: &Path) -> (Project, String) {
    let graph = motion_graph::builtin_examples()
        .iter()
        .find(|e| e.id == "soft_sphere_motion")
        .unwrap()
        .load()
        .unwrap();
    let compiled = motion_graph::compile(&graph, &Default::default()).unwrap();
    let bytes = motion_graph::compiled_package_bytes(&compiled.manifest, &compiled.source).unwrap();
    let package = motion_effects::EffectPackage::from_bytes(bytes.clone()).unwrap();
    let source = root.join("fixture.msfx");
    std::fs::write(&source, bytes).unwrap();
    let plugins = root.join("plugins");
    motion_effects::Registry::load(&plugins)
        .unwrap()
        .install(&plugins, &source)
        .unwrap();
    let mut project = Project::new(640, 360, 30, 180).unwrap();
    let mut layer = Layer::solid(
        7,
        "Range and units · 范围验收",
        [100., 50.],
        [100., 100., 0.],
        [0.25, 0.5, 0.8, 1.],
    );
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 10,
        out_frame: 180,
        offset_frame: 10,
    });
    for (frame, opacity, x) in [(0, 0.2, 100.), (60, 0.5, 300.), (120, 0.8, 150.)] {
        layer
            .transform
            .opacity
            .upsert(frame, opacity, Ease::InOut)
            .unwrap();
        layer
            .transform
            .position
            .upsert(frame, [x, 100., 0.], Ease::Linear)
            .unwrap();
    }
    layer.transform.opacity.keys[1].ease = Ease::Linear;
    layer.transform.opacity.keys[1].curve = Some(Curve {
        space: CurveSpace::Progress,
        shape: CurveShape::Quadratic {
            control: [0.3, 1.2],
            start: 0.,
            end: 1.,
        },
    });
    layer.transform.position.keys[1].curve = Some(Curve {
        space: CurveSpace::Velocity,
        shape: CurveShape::Quadratic {
            control: [0.3, 2.],
            start: 0.,
            end: 1.,
        },
    });
    let mut effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [100., 50.],
    );
    for (frame, value) in [(10, 0.25), (70, 1.), (130, 0.5)] {
        effect
            .params
            .get_mut("speed")
            .unwrap()
            .track
            .upsert(frame, [value, 0., 0., 0.], Ease::InOut)
            .unwrap();
    }
    layer.effects.push(effect);
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    (project, package.hash)
}
fn retain_files(root: &Path, files: &BTreeMap<String, Vec<u8>>) {
    for (path, bytes) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}
fn panels(state: &Value, catalogue: &Value, case: Case) -> Panels {
    let mut p = Panels {
        locale: case.locale,
        focused: Some(case.kind.panel()),
        ..Default::default()
    };
    p.select_only(Some(7));
    p.timeline.set_graph_mode(true);
    p.timeline.speed_graph = case.speed;
    p.effect_controls.catalogue = catalogue.clone().into();
    p.timeline.expanded.insert(7);
    match case.kind {
        Kind::Effect => {
            p.timeline.collapsed_transforms.insert(7);
            p.timeline.expanded_effects.insert(7);
            super::super::effect_timeline::project_key(
                &mut p,
                state,
                7,
                1,
                "speed",
                case.selected_frame(),
            );
        }
        Kind::Comparison => {
            super::super::seed_curve_comparison(&mut p, state);
        }
        _ => {
            let KeyframeTarget::Property { property, axis, .. } = case.target() else {
                unreachable!()
            };
            p.animation_controls.select_key_in(
                state,
                7,
                property,
                axis,
                i64::from(case.selected_frame()),
            );
        }
    }
    p.selected_keys
        .apply(
            state,
            [super::super::key_selection::Key {
                target: case.target(),
                frame: case.selected_frame(),
            }],
            super::super::key_selection::Mode::Replace,
        )
        .unwrap();
    p
}
fn draw(
    engine: &DesktopEngine,
    p: &mut Panels,
    atlas: &mut TextAtlas,
    case: Case,
    input: Input,
) -> (PaintList, Vec<Action>) {
    let state = engine.state().unwrap();
    let mut paint = PaintList::default();
    let actions = if case.window == 0 {
        super::super::chrome(
            &mut paint,
            atlas,
            case.size,
            &dock::Node::dock(case.kind.panel().dock()),
            p,
            &state,
            &input,
        )
    } else {
        super::super::floating_chrome(
            &mut paint,
            atlas,
            case.kind.panel(),
            case.size,
            p,
            &state,
            &input,
        )
    };
    (paint, actions)
}
fn step(
    engine: &DesktopEngine,
    p: &mut Panels,
    atlas: &mut TextAtlas,
    case: Case,
    input: Input,
) -> PaintList {
    let (paint, actions) = draw(engine, p, atlas, case, input);
    assert!(
        actions.is_empty(),
        "case={} view input emitted authored/transport actions: {actions:?}",
        case.name()
    );
    paint
}
fn probe(p: &Panels, case: Case) -> Probe {
    p.timeline
        .graph_ranges
        .probes
        .iter()
        .find(|probe| match case.kind {
            Kind::Comparison => {
                probe.id.surface == Surface::Comparison && probe.id.unit.contains("Percent")
            }
            Kind::Mini => probe.id.surface == Surface::Mini(Panel::Curves),
            _ => {
                probe.id.surface == Surface::Timeline
                    && probe.id.target.as_ref() == Some(&case.target())
            }
        })
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "case={} did not render the expected actual graph",
                case.name()
            )
        })
}
fn open(engine: &DesktopEngine, p: &mut Panels, atlas: &mut TextAtlas, case: Case) {
    step(engine, p, atlas, case, Input::default());
    let probe = probe(p, case);
    let styles = PaintList::default().styles();
    let slot = styles.metric("graph.control.height", 24.);
    let pad = styles.metric("padding.panel", 8.);
    let point = [
        probe.header.max[0] - pad - slot * 0.5,
        (probe.header.min[1] + probe.header.max[1]) * 0.5,
    ];
    step(
        engine,
        p,
        atlas,
        case,
        Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
    );
    assert!(
        p.timeline.graph_ranges.editing(),
        "case={} More launcher did not open",
        case.name()
    );
    step(engine, p, atlas, case, Input::default());
    assert!(
        p.timeline.graph_ranges.menu_card.is_some(),
        "case={} range menu disappeared after its real launcher press",
        case.name()
    );
}
fn choose(
    engine: &DesktopEngine,
    p: &mut Panels,
    atlas: &mut TextAtlas,
    case: Case,
    item: usize,
    pointer: bool,
) {
    if pointer {
        let card = p.timeline.graph_ranges.menu_card.unwrap();
        let styles = PaintList::default().styles();
        let point = [
            card.min[0] + card.width() * 0.5,
            card.min[1]
                + styles.metric("padding.panel", 8.)
                + styles.metric("row.height", 22.) * (item as f32 + 0.5),
        ];
        step(
            engine,
            p,
            atlas,
            case,
            Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            },
        );
    } else {
        for _ in 0..10 {
            if p.timeline.graph_ranges.menu.as_ref().unwrap().focus == item {
                break;
            }
            step(engine, p, atlas, case, key_input(Key::Down));
        }
        assert_eq!(p.timeline.graph_ranges.menu.as_ref().unwrap().focus, item);
        step(engine, p, atlas, case, key_input(Key::Space));
    }
}
fn capture(
    events: &mut EventLoop<()>,
    engine: &DesktopEngine,
    atlas: &TextAtlas,
    case: Case,
    paint: PaintList,
    path: &Path,
) -> image::RgbaImage {
    let config = atlas.config();
    crate::native_test_support::capture_ui(
        events,
        engine,
        UiFrame {
            paint,
            atlas: Some(AtlasUpload {
                pixels: atlas.pixel_data().to_vec(),
                width: config.width,
                height: config.height,
            }),
            logical_size: case.size,
            composition: None,
            inspection: Default::default(),
            composition_image: None,
            window: case.window,
            node_preview: None,
        },
        path.into(),
    )
    .unwrap();
    let image = image::open(path).unwrap().to_rgba8();
    assert_eq!(
        image.dimensions(),
        (
            (case.size[0] * case.ratio).round() as u32,
            (case.size[1] * case.ratio).round() as u32
        )
    );
    image
}
fn pixel_measurements(
    image: &image::RgbaImage,
    probe: &Probe,
    state: &Value,
    case: Case,
) -> Vec<Value> {
    let data =
        super::super::curve_comparison::data::load(state, &case.target(), case.locale).unwrap();
    let easing = if matches!(case.kind, Kind::Mini) {
        let name = if case.speed { "position" } else { "opacity" };
        let key = state["timeline_layers"][0]["properties"][name]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|key| key["frame"] == 70)
            .unwrap();
        Easing {
            ease: serde_json::from_value(key["ease"].clone()).unwrap(),
            curve: serde_json::from_value(key["curve"].clone()).unwrap(),
        }
    } else {
        Easing::default()
    };
    let mut measurements = vec![];
    for fraction in [0.2, 0.3, 0.4, 0.6, 0.7, 0.8] {
        let frame = probe.bounds.x.at(fraction);
        let value = if matches!(case.kind, Kind::Mini) {
            let s = easing.sample(frame);
            if case.speed {
                s.velocity / easing.curve.unwrap().definition_scale()
            } else {
                s.progress
            }
        } else {
            f64::from(data.sample(frame, 0, 30., case.speed).unwrap())
        };
        let logical = [
            probe.plot.min[0] + probe.plot.width() * fraction as f32,
            probe.plot.max[1] - probe.bounds.y.fraction(value) as f32 * probe.plot.height(),
        ];
        if !probe.plot.shrink(8.).contains(logical) {
            continue;
        }
        let expected = logical.map(|v| v * case.ratio);
        let radius = (case.ratio * 3.).ceil() as i32;
        let mut distance = f64::INFINITY;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let x = expected[0].floor() as i32 + dx;
                let y = expected[1].floor() as i32 + dy;
                if x < 0 || y < 0 || x >= image.width() as i32 || y >= image.height() as i32 {
                    continue;
                }
                let rgba = image.get_pixel(x as u32, y as u32).0;
                let channels = [rgba[0], rgba[1], rgba[2]];
                if channels.iter().max().unwrap() - channels.iter().min().unwrap() > 30
                    && *channels.iter().max().unwrap() > 100
                {
                    distance = distance.min(
                        (f64::from(x) + 0.5 - f64::from(expected[0]))
                            .hypot(f64::from(y) + 0.5 - f64::from(expected[1])),
                    );
                }
            }
        }
        assert!(
            distance <= f64::from(case.ratio) * 1.5 + 0.75,
            "case={} model={frame},{value} expected={expected:?} nearest={distance}",
            case.name()
        );
        measurements.push(serde_json::json!({"frame_or_normalized_t":frame,"value":value,"logical":logical,"physical":expected,"nearest_colored_curve_pixel_distance":distance}));
    }
    assert!(
        measurements.len() >= 2,
        "case={} insufficient in-view model probes",
        case.name()
    );
    measurements
}
fn save(events: &mut EventLoop<()>, engine: &DesktopEngine) {
    let reply = engine.request(WorkerCommand::Save).unwrap();
    crate::native_test_support::receive(events, &reply, Duration::from_secs(30))
        .unwrap()
        .unwrap();
}

#[test]
#[ignore = "manual native graph-range windows, GPU captures and byte invariance; explicitly retained artifacts"]
fn graph_ranges_native_all_surfaces_locales_dpi_locks_cancellation_and_saved_files() {
    const CHILD: &str = "MOTION_GRAPH_RANGES_NATIVE_CHILD";
    const NAME:&str="panels::graph_ranges::native_tests::graph_ranges_native_all_surfaces_locales_dpi_locks_cancellation_and_saved_files";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_GRAPH_RANGES_NATIVE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_remove("MOTION_LOG")
            .env(CHILD, &root)
            .output()
            .unwrap();
        std::fs::write(root.join("stdout.log"), &result.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &result.stderr).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let (project, package_hash) = fixture(&root);
    let project_root = root.join("project");
    motion_core::storage::save(&project_root, &project).unwrap();
    let engine = DesktopEngine::start(
        project_root.clone(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let catalogue = engine
        .plugin(serde_json::json!({"op":"catalogue"}).to_string())
        .unwrap();
    assert!(catalogue["packages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["hash"] == package_hash));
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(900, 600)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(720, 640)),
            )
            .unwrap(),
    );
    let attached = engine.attach_async(main.clone(), 900, 600).unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    engine.float_window(1, float.clone(), 720, 640).unwrap();
    save(&mut events, &engine);
    let before = engine.state().unwrap();
    let files_before = project_files(&project_root);
    assert!(files_before.contains_key("project.json"));
    retain_files(&root.join("saved-before"), &files_before);
    let mut atlas = TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        12.,
        true,
    )
    .unwrap();
    let mut evidence = vec![];
    for (window, size, ratio, locale) in [
        (0, [900., 600.], 1., Locale::En),
        (0, [1600., 1000.], 1.5, Locale::Zh),
        (1, [720., 640.], 2., Locale::En),
        (1, [900., 760.], 1., Locale::Zh),
    ] {
        let physical = [(size[0] * ratio) as u32, (size[1] * ratio) as u32];
        let _ = if window == 0 {
            main.request_inner_size(PhysicalSize::new(physical[0], physical[1]))
        } else {
            float.request_inner_size(PhysicalSize::new(physical[0], physical[1]))
        };
        let resized = engine
            .resize_async(window, physical[0], physical[1])
            .unwrap();
        crate::native_test_support::receive(&mut events, &resized, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        atlas.set_pixel_ratio(ratio);
        for kind in [Kind::Property, Kind::Effect, Kind::Comparison, Kind::Mini] {
            for speed in [false, true] {
                let case = Case {
                    window,
                    size,
                    ratio,
                    locale,
                    kind,
                    speed,
                };
                let name = case.name();
                let seek = engine.seek_async(70.).unwrap();
                crate::native_test_support::receive(&mut events, &seek, Duration::from_secs(30))
                    .unwrap()
                    .unwrap();
                let state = engine.state().unwrap();
                let baseline = state.clone();
                let mut p = panels(&state, &catalogue, case);
                let paint = step(&engine, &mut p, &mut atlas, case, Input::default());
                let initial = probe(&p, case);
                let pixels = capture(
                    &mut events,
                    &engine,
                    &atlas,
                    case,
                    paint,
                    &root.join(format!("{name}-curve.png")),
                );
                let measurements = pixel_measurements(&pixels, &initial, &state, case);
                open(&engine, &mut p, &mut atlas, case);
                let options = p.timeline.graph_ranges.menu.as_ref().unwrap().options;
                assert!(options.all.is_some() && options.selection.is_some());
                let paint = step(&engine, &mut p, &mut atlas, case, Input::default());
                capture(
                    &mut events,
                    &engine,
                    &atlas,
                    case,
                    paint,
                    &root.join(format!("{name}-menu.png")),
                );
                let mut ime = key_input(Key::Space);
                ime.composing = true;
                step(&engine, &mut p, &mut atlas, case, ime);
                assert!(p.timeline.graph_ranges.editing());
                assert_eq!(probe(&p, case).bounds, initial.bounds);
                let mut repeated = key_input(Key::Space);
                repeated.key_repeats = vec![true];
                step(&engine, &mut p, &mut atlas, case, repeated);
                assert!(p.timeline.graph_ranges.editing());
                choose(&engine, &mut p, &mut atlas, case, 1, speed);
                assert!(!p.timeline.graph_ranges.editing());
                let mut paint = step(&engine, &mut p, &mut atlas, case, Input::default());
                let mut fitted = probe(&p, case);
                assert!(fitted.bounds.x.span() < initial.bounds.x.span());
                if matches!(kind, Kind::Mini) {
                    let mouse = [
                        (fitted.plot.min[0] + fitted.plot.max[0]) * 0.5,
                        (fitted.plot.min[1] + fitted.plot.max[1]) * 0.5,
                    ];
                    for modifiers in [
                        Modifiers {
                            control: true,
                            ..Default::default()
                        },
                        Modifiers {
                            control: true,
                            alt: true,
                            ..Default::default()
                        },
                    ] {
                        step(
                            &engine,
                            &mut p,
                            &mut atlas,
                            case,
                            Input {
                                mouse,
                                wheel: [0., 32.],
                                modifiers,
                                ..Default::default()
                            },
                        );
                    }
                    fitted = probe(&p, case);
                    assert!(fitted.bounds.x.span() < 0.002);
                    paint = step(&engine, &mut p, &mut atlas, case, Input::default());
                }
                capture(
                    &mut events,
                    &engine,
                    &atlas,
                    case,
                    paint,
                    &root.join(format!("{name}-fit-selection.png")),
                );
                open(&engine, &mut p, &mut atlas, case);
                choose(&engine, &mut p, &mut atlas, case, 3, speed);
                choose(&engine, &mut p, &mut atlas, case, 4, speed);
                let locked_id = p.timeline.graph_ranges.menu.as_ref().unwrap().id.clone();
                let entry = p
                    .timeline
                    .graph_ranges
                    .entries
                    .iter()
                    .find(|e| e.id == locked_id)
                    .unwrap();
                assert!(entry.y.locked());
                assert!(if matches!(kind, Kind::Mini) {
                    entry.x.locked()
                } else {
                    p.timeline.graph_ranges.locked_x()
                });
                let paint = step(&engine, &mut p, &mut atlas, case, Input::default());
                capture(
                    &mut events,
                    &engine,
                    &atlas,
                    case,
                    paint,
                    &root.join(format!("{name}-locked-menu.png")),
                );
                step(&engine, &mut p, &mut atlas, case, key_input(Key::Escape));
                assert!(!p.timeline.graph_ranges.editing());
                let seek = engine.seek_async(85.25).unwrap();
                crate::native_test_support::receive(&mut events, &seek, Duration::from_secs(30))
                    .unwrap()
                    .unwrap();
                let mouse = [
                    (fitted.plot.min[0] + fitted.plot.max[0]) * 0.5,
                    (fitted.plot.min[1] + fitted.plot.max[1]) * 0.5,
                ];
                for modifiers in [
                    Modifiers {
                        control: true,
                        ..Default::default()
                    },
                    Modifiers {
                        control: true,
                        alt: true,
                        ..Default::default()
                    },
                    Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                ] {
                    step(
                        &engine,
                        &mut p,
                        &mut atlas,
                        case,
                        Input {
                            mouse,
                            wheel: [0., 4.],
                            modifiers,
                            ..Default::default()
                        },
                    );
                    assert_eq!(
                        probe(&p, case).bounds,
                        fitted.bounds,
                        "case={name} lock moved"
                    );
                }
                open(&engine, &mut p, &mut atlas, case);
                choose(&engine, &mut p, &mut atlas, case, 3, speed);
                choose(&engine, &mut p, &mut atlas, case, 4, speed);
                choose(&engine, &mut p, &mut atlas, case, 2, speed);
                step(&engine, &mut p, &mut atlas, case, Input::default());
                assert_eq!(probe(&p, case).bounds, initial.bounds);
                let mouse = [
                    initial.plot.min[0] + initial.plot.width() * 0.6,
                    initial.plot.min[1] + initial.plot.height() * 0.55,
                ];
                let paint = step(
                    &engine,
                    &mut p,
                    &mut atlas,
                    case,
                    Input {
                        mouse,
                        ..Default::default()
                    },
                );
                capture(
                    &mut events,
                    &engine,
                    &atlas,
                    case,
                    paint,
                    &root.join(format!("{name}-readout.png")),
                );
                if matches!(kind, Kind::Mini) {
                    let (_, lines) = p.animation_controls.mini_test_readout().unwrap();
                    assert!(lines
                        .iter()
                        .any(|s| s.ends_with(if speed { "px/s" } else { "%" })));
                    if speed {
                        assert!(
                            lines[1].contains('-'),
                            "negative outgoing physical speed must retain its sign: {lines:?}"
                        );
                    }
                }
                open(&engine, &mut p, &mut atlas, case);
                step(
                    &engine,
                    &mut p,
                    &mut atlas,
                    case,
                    Input {
                        focus_lost: true,
                        ..Default::default()
                    },
                );
                assert!(!p.timeline.graph_ranges.editing());
                open(&engine, &mut p, &mut atlas, case);
                let foreign = Case {
                    window: 1 - window,
                    ..case
                };
                step(&engine, &mut p, &mut atlas, foreign, key_input(Key::Delete));
                assert!(!p.timeline.graph_ranges.editing());
                let after = engine.state().unwrap();
                for field in ["project", "revision", "history"] {
                    assert_eq!(after[field], baseline[field], "case={name} changed {field}");
                }
                evidence.push(serde_json::json!({"case":name,"window":window,"logical_size":size,"pixel_ratio":ratio,"speed":speed,
                "pixel_measurements":measurements,"fit_selection":true,"high_zoom_x_span":fitted.bounds.x.span(),"high_zoom_y_span":fitted.bounds.y.span(),"lock_x_y_playhead_wheel":true,"pointer_and_keyboard":true,
                "ime_repeat_gated":true,"focus_and_foreign_cancel":true,"history_revision_project_unchanged":true}));
            }
        }
    }
    save(&mut events, &engine);
    assert_eq!(
        project_files(&project_root),
        files_before,
        "view operations changed actual saved files"
    );
    let after_view = engine.state().unwrap();
    for field in ["project", "revision", "history"] {
        assert_eq!(after_view[field], before[field]);
    }
    // A real external authored edit invalidates an already-open menu. Its own
    // history/revision change is separated from the view-only invariant above.
    let case = Case {
        window: 0,
        size: [900., 600.],
        ratio: 1.,
        locale: Locale::En,
        kind: Kind::Property,
        speed: false,
    };
    let state = engine.state().unwrap();
    let mut p = panels(&state, &catalogue, case);
    open(&engine, &mut p, &mut atlas, case);
    let edit = engine
        .edit_async(
            serde_json::to_string(&Command::SetScalar {
                object: 7,
                property: Property::Opacity,
                frame: 70,
                value: 0.53,
            })
            .unwrap(),
        )
        .unwrap();
    crate::native_test_support::receive(&mut events, &edit, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let external = engine.state().unwrap();
    step(&engine, &mut p, &mut atlas, case, key_input(Key::Delete));
    assert!(!p.timeline.graph_ranges.editing());
    let after = engine.state().unwrap();
    for field in ["project", "revision", "history"] {
        assert_eq!(after[field], external[field]);
    }
    let undo = engine.history_async(0).unwrap();
    crate::native_test_support::receive(&mut events, &undo, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    save(&mut events, &engine);
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    assert_eq!(project_files(&project_root), files_before);
    retain_files(&root.join("saved-after"), &project_files(&project_root));
    let reopened = motion_core::storage::load(&project_root).unwrap();
    assert_eq!(serde_json::to_value(reopened).unwrap(), before["project"]);
    std::fs::write(
        root.join("saved-files-before.json"),
        serde_json::to_vec_pretty(&files_before.keys().collect::<Vec<_>>()).unwrap(),
    )
    .unwrap();
    std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({"cases":evidence,"saved_files_byte_equal":true,"reopened_project_equal":true,
        "actual_worker_revision_cancel":true,"external_edit_undo_separate_from_view_invariant":true,"package_source":"independently authored builtin graph soft_sphere_motion","package_hash":package_hash,
        "physical_os_input":false,"system_dpi_setting_changed":false,"excluded_runtime_file":"root/.motion-editor.lock: zero-byte process mutex, not authored saved data","input":"production chrome Input injection on hidden native windows; controlled scale ratios"})).unwrap()).unwrap();
    engine.request_shutdown().unwrap();
}
