//! Manual, shared baseline/current native cost study. No performance thresholds.
use super::*;
use crate::{engine::Engine as DesktopEngine, native_test_support, workspace, Shell};
use motion_core::{Curve, CurveShape, CurveSpace, Ease, EffectInstance, Layer, Project};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

const SIZE: [f32; 2] = [1280., 800.];
const NAME: &str =
    "panels::graph_native_performance_tests::actual_native_graph_costs_match_info_counters";
const CHILD: &str = "MOTION_GRAPH_PERFORMANCE_CHILD";

// Independently authored ABI bindings to documented Windows own-thread clocks.
// GetThreadTimes reports100ns units; raw cycle counts are never converted to time.
// https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-getthreadtimes
// https://learn.microsoft.com/windows/win32/api/realtimeapiset/nf-realtimeapiset-querythreadcycletime
#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}
impl FileTime {
    fn ticks(&self) -> u64 {
        (u64::from(self.high) << 32) | u64::from(self.low)
    }
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> *mut std::ffi::c_void;
    fn GetThreadTimes(
        thread: *mut std::ffi::c_void,
        created: *mut FileTime,
        exited: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
}
#[link(name = "mincore")]
unsafe extern "system" {
    fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
}
struct CpuSample {
    ticks: u64,
    cycles: u64,
}
impl CpuSample {
    fn read() -> Self {
        let (mut created, mut exited, mut kernel, mut user) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        let mut cycles = 0;
        // SAFETY: the pseudo-handle refers to this live calling thread, every
        // output points to a correctly aligned initialized local of the ABI
        // type, and no pointer is retained by either synchronous OS query.
        unsafe {
            let thread = GetCurrentThread();
            assert_ne!(
                GetThreadTimes(thread, &mut created, &mut exited, &mut kernel, &mut user),
                0,
                "GetThreadTimes failed: {}",
                std::io::Error::last_os_error()
            );
            assert_ne!(
                QueryThreadCycleTime(thread, &mut cycles),
                0,
                "QueryThreadCycleTime failed: {}",
                std::io::Error::last_os_error()
            );
        }
        Self {
            ticks: kernel.ticks().checked_add(user.ticks()).unwrap(),
            cycles,
        }
    }
}

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

fn fixture(root: &Path) -> Project {
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
    let mut project = Project::new(640, 360, 30, 360).unwrap();
    let mut layer = Layer::solid(
        7,
        "Graph performance fixture",
        [100., 50.],
        [160., 100., 0.],
        [0.25, 0.5, 0.8, 1.],
    );
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 10,
        out_frame: 360,
        offset_frame: 10,
    });
    for (frame, x, opacity) in [(0, 100., 0.2), (30, 300., 0.5), (60, 150., 0.8)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 100., 0.], Ease::InOut)
            .unwrap();
        layer
            .transform
            .opacity
            .upsert(frame, opacity, Ease::InOut)
            .unwrap();
    }
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
    for (frame, value) in [(0, 0.25), (30, 1.), (60, 0.5)] {
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
    project
}

fn configure(shell: &mut Shell, catalogue: &Value, kind: Kind, speed: bool, zoom: f32) {
    shell.workspace = workspace::Workspace::default();
    shell.workspace.root = Node::Split {
        edge: dock::Edge::Bottom,
        split: 0.45,
        first: Box::new(Node::dock(kind.panel().dock())),
        second: Box::new(Node::Split {
            edge: dock::Edge::Right,
            split: 0.25,
            first: Box::new(Node::dock(Panel::Info.dock())),
            second: Box::new(Node::dock(Panel::Composition.dock())),
        }),
    };
    let mut p = Panels {
        locale: Locale::En,
        focused: Some(kind.panel()),
        ..Default::default()
    };
    p.select_only(Some(7));
    p.timeline.set_graph_mode(true);
    p.timeline.speed_graph = speed;
    p.timeline.zoom = zoom;
    p.timeline.offset = 20.;
    p.timeline.expanded.insert(7);
    p.effect_controls.catalogue = catalogue.clone().into();
    match kind {
        Kind::Comparison => seed_curve_comparison(&mut p, &shell.state),
        Kind::Effect => {
            p.timeline.collapsed_transforms.insert(7);
            p.timeline.expanded_effects.insert(7);
            effect_timeline::project_key(&mut p, &shell.state, 7, 1, "speed", 40);
        }
        _ => p.animation_controls.select_key_in(
            &shell.state,
            7,
            if matches!(kind, Kind::Mini) && speed {
                Property::Position
            } else {
                Property::Opacity
            },
            None,
            40,
        ),
    }
    shell.panels = p;
}

fn performance(engine: &DesktopEngine) -> Value {
    let state = engine.state().unwrap();
    let p = state["desktop"]["uiPerformance"].clone();
    for field in ["compositionRenders", "compositionReuses", "lastPresentUs"] {
        assert!(
            p[field].as_u64().is_some(),
            "missing actual Info metric {field}: {p}"
        );
    }
    p
}

fn frame(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    root: &Path,
    value: f64,
    seek: bool,
) -> Value {
    let mut seek_us = 0;
    if seek {
        let begin = Instant::now();
        let reply = shell.engine.seek_async(value).unwrap();
        let state = native_test_support::receive(events, &reply, Duration::from_secs(10))
            .unwrap()
            .unwrap();
        shell.accept_edit_state(state);
        seek_us = begin.elapsed().as_micros();
    }
    // Seek replies contain the Core snapshot, while State includes the actual
    // desktop Info counters. Read that snapshot outside the timed UI/Present
    // section so the Info panel paints real performance data in both phases.
    let begin = Instant::now();
    shell.state = shell.engine.state().unwrap();
    let snapshot_us = begin.elapsed().as_micros();
    assert_eq!(shell.state["frame"].as_f64(), Some(value));
    assert!(shell.state["desktop"]["uiPerformance"].is_object());
    shell.frame = value;
    let mut build_us = 0;
    let mut cpu_ticks = 0;
    let mut cpu_cycles = 0;
    let mut reply_us = 0;
    let mut polls = 0;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let cpu_before = CpuSample::read();
        let start = Instant::now();
        let (paint, actions) = shell.draw_commands(SIZE);
        assert!(actions.is_empty());
        let ui = shell.ui_frame(paint, SIZE, false);
        assert!(ui.composition.is_some());
        build_us += start.elapsed().as_micros();
        let cpu_after = CpuSample::read();
        cpu_ticks += cpu_after
            .ticks
            .checked_sub(cpu_before.ticks)
            .expect("thread CPU time went backwards");
        cpu_cycles += cpu_after
            .cycles
            .checked_sub(cpu_before.cycles)
            .expect("thread cycles went backwards");
        let start = Instant::now();
        let result = native_test_support::receive(
            events,
            &shell.engine.present_async(ui).unwrap(),
            Duration::from_secs(10),
        );
        reply_us += start.elapsed().as_micros();
        polls += 1;
        let result=result.unwrap_or_else(|cause| {
            std::fs::write(root.join("failure.json"),serde_json::to_vec_pretty(&serde_json::json!({"phase":"present_reply","frame":value,"cause":cause.to_string(),"native":shell.engine.native_wait_snapshot()})).unwrap()).unwrap();
            panic!("native graph performance present failed: {cause}");
        }).unwrap();
        shell.pending = !result;
        if result {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native graph composition readiness stalled"
        );
    }
    let p = performance(&shell.engine);
    serde_json::json!({"frame":value,"sought":seek,"seek_reply_us":seek_us,"state_snapshot_us":snapshot_us,"ui_build_and_atlas_us":build_us,
        "ui_thread_cpu_100ns":cpu_ticks,"ui_thread_cycles":cpu_cycles,"document_generation":shell.state["document_generation"],"revision":shell.state["revision"],
        "present_reply_us":reply_us,"present_polls":polls,
        "effective_timeline_zoom":shell.panels.timeline.zoom,"effective_timeline_offset":shell.panels.timeline.offset,
        "timeline_graph_mode":shell.panels.timeline.graph_mode,"info":p})
}

#[test]
#[ignore = "manual native baseline/current graph cost study; raw artifacts required"]
fn actual_native_graph_costs_match_info_counters() {
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_GRAPH_PERFORMANCE_ARTIFACTS")
            .map(std::path::PathBuf::from)
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
    let root = std::path::PathBuf::from(root);
    std::fs::create_dir_all(&root).unwrap();
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let project = fixture(&root);
    let project_root = root.join("project");
    motion_core::storage::save(&project_root, &project).unwrap();
    let mut shell = Shell::new(project_root, None, Some(Locale::En)).unwrap();
    shell.home = None;
    let catalogue = shell
        .engine
        .plugin(serde_json::json!({"op":"catalogue"}).to_string())
        .unwrap();
    assert!(catalogue["packages"].as_array().unwrap().iter().any(|entry|
        entry["hash"].as_str()==Some(project.layers[0].effects[0].hash.as_str())),
        "the measured authored effect must be in the real worker catalogue");
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(SIZE[0] as u32, SIZE[1] as u32)),
            )
            .unwrap(),
    );
    shell.state = native_test_support::receive(
        &mut events,
        &shell
            .engine
            .attach_async(window.clone(), SIZE[0] as u32, SIZE[1] as u32)
            .unwrap(),
        Duration::from_secs(30),
    )
    .unwrap()
    .unwrap();
    shell.window = Some(window);
    shell.attached = true;
    let before = shell.state["project"].clone();
    let revision = shell.state["revision"].clone();
    let mut cases = vec![];
    for kind in [Kind::Property, Kind::Effect, Kind::Comparison, Kind::Mini] {
        for speed in [false, true] {
            for zoom in if matches!(kind, Kind::Mini) {
                vec![2.]
            } else {
                vec![2., 128.]
            } {
                configure(&mut shell, &catalogue, kind, speed, zoom);
                let name = format!(
                    "{}-{}-x{zoom}",
                    kind.name(),
                    if speed { "speed" } else { "value" }
                );
                for advancing in [false, true] {
                    let phase = if advancing { "seek" } else { "static" };
                    let name = format!("{name}-{phase}");
                    let mut warmup = vec![];
                    for i in 0..5 {
                        warmup.push(frame(
                            &mut shell,
                            &mut events,
                            &root,
                            if advancing {
                                30. + f64::from(i) * 0.25
                            } else {
                                45.
                            },
                            advancing || i == 0,
                        ));
                    }
                    let start = performance(&shell.engine);
                    let mut records = vec![];
                    for i in 0..60 {
                        records.push(frame(
                            &mut shell,
                            &mut events,
                            &root,
                            if advancing {
                                31.25 + f64::from(i) * 0.25
                            } else {
                                45.
                            },
                            advancing,
                        ));
                    }
                    let end = performance(&shell.engine);
                    let renders = end["compositionRenders"].as_u64().unwrap()
                        - start["compositionRenders"].as_u64().unwrap();
                    let reuses = end["compositionReuses"].as_u64().unwrap()
                        - start["compositionReuses"].as_u64().unwrap();
                    assert!(
                        renders + reuses >= 60,
                        "real composition counters did not advance: {name}"
                    );
                    assert_eq!((renders, reuses), if advancing {(60, 0)} else {(0, 60)},
                        "static redraws must reuse the composition; distinct fixture frames must render: {name}");
                    let final_state = shell.engine.state().unwrap();
                    assert_eq!(final_state["project"], before);
                    assert_eq!(final_state["revision"], revision);
                    assert!(!shell.engine.failed());
                    shell.state = final_state;
                    let result = serde_json::json!({"case":name,"kind":kind.name(),"speed":speed,"timeline_zoom":zoom,"phase":phase,
                        "warmup":warmup,"frames":records,"info_before":start,"info_after":end,"renders":renders,"reuses":reuses,
                        "native":shell.engine.native_wait_snapshot()});
                    std::fs::write(
                        root.join(format!("{name}.json")),
                        serde_json::to_vec_pretty(&result).unwrap(),
                    )
                    .unwrap();
                    let (paint, actions) = shell.draw_commands(SIZE);
                    assert!(actions.is_empty());
                    let ui = shell.ui_frame(paint, SIZE, true);
                    native_test_support::capture_ui(
                        &mut events,
                        &shell.engine,
                        ui,
                        root.join(format!("{name}.png")),
                    )
                    .unwrap();
                    cases.push(result);
                }
            }
        }
    }
    assert_eq!(cases.len(), 28);
    std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "schema":3,"debug_assertions":cfg!(debug_assertions),"window":SIZE,"ratio":1.,"warmup_per_case":5,"measured_per_case":60,
        "cases":cases,"projectUnchanged":true,"revisionUnchanged":true,
        "scope":"Production Shell UI building and native surface Present, real Composition+Info+graph layout and owned procedural fixture. Static phase seeks once in warmup then only redraws UI; advancing phase directly seeks each frame, not application playback/input dispatch. State observation and final PNG readback outside timed UI/Present. Own-thread CPU100ns and raw cycles bracket UI/atlas work; probe overhead and OS counter granularity remain, cycles are never converted to elapsed time. Info reused ratio means composition texture reuse, not all allocator reuse. Default-scale Mini only; timeline zoom fields are2 and128 and effective values are recorded. Not a cold-start, physical-input, release-FPS, or cross-device throughput claim."
    })).unwrap()).unwrap();
}
