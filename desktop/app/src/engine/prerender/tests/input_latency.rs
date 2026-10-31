//! Explicit application-input study. Native IDs and production hit testing;
//! injected events do not measure OS device delivery or scheduler latency.
use super::*;
use crate::{Locale, Shell, dock, panels, shell};
use sha2::{Digest, Sha256};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    platform::pump_events::EventLoopExtPumpEvents,
    window::WindowId,
};
mod driver;
use driver::*;

const SIZE: [f32; 2] = [1280., 800.];
const SAMPLES: usize = 60;

struct Evidence {
    path: PathBuf,
    runs: Vec<Value>,
    partial: Vec<Value>,
}
impl Evidence {
    fn save(&self) {
        std::fs::write(
            &self.path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion":1,"samplesPerEpisode":SAMPLES,
                "order":[false,true,true,false],"window":SIZE,"composition":[640,360],
                "profile":"High authored single-time",
                "limits":"Injected native event translation and actual UI/rendering; not OS device delivery or a universal hardware SLA",
                "runs":self.runs,"partialSamples":self.partial
            })).unwrap(),
        ).unwrap();
    }
}
impl Drop for Evidence {
    fn drop(&mut self) {
        self.save();
    }
}

#[test]
#[ignore = "explicit native Shell ABBA property-drag and timeline-scrub P95 study"]
fn native_controls_record_edit_and_scrub_latency_with_real_optional_work() {
    const NAME: &str = "engine::prerender::tests::input_latency::native_controls_record_edit_and_scrub_latency_with_real_optional_work";
    const CHILD: &str = "MOTION_PRERENDER_INPUT_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                NAME,
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        if let Some(destination) = std::env::var_os("MOTION_PRERENDER_INPUT_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in ["input-costs.json", "motion-studio.log"] {
                let path = root.path().join(name);
                if path.exists() {
                    std::fs::copy(path, destination.join(name)).unwrap();
                }
            }
            std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
        }
        assert!(
            output.status.success(),
            "status={}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let mut evidence = Evidence {
        path: root.join("input-costs.json"),
        runs: vec![],
        partial: vec![],
    };
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1280, 800)),
            )
            .unwrap(),
    );
    for scenario in ["property_drag", "timeline_scrub"] {
        let mut reference = Vec::new();
        for (episode, enabled) in [false, true, true, false].into_iter().enumerate() {
            let directory = root.join(format!("{scenario}-{episode}"));
            let mut project = motion_core::Project::new(640, 360, 30, 300).unwrap();
            let mut layer = motion_core::Layer::solid(
                7,
                "Input control",
                [100.; 2],
                [240., 180., 0.],
                [1., 0.2, 0.05, 1.],
            );
            if scenario == "timeline_scrub" {
                layer.transform.position.set_animated(0, true).unwrap();
                layer
                    .transform
                    .position
                    .set_at(0, [240., 180., 0.])
                    .unwrap();
                layer
                    .transform
                    .position
                    .set_at(120, [400., 180., 0.])
                    .unwrap();
            }
            project.layers.push(layer);
            motion_core::storage::save(&directory, &project).unwrap();
            let mut shell = Shell::new(directory, None, Some(Locale::En)).unwrap();
            shell.home = None;
            shell.window = Some(window.clone());
            shell.scale = 1.;
            shell.attached = true;
            shell.workspace.root = layout();
            shell.panels.select_only(Some(7));
            shell.panels.timeline.zoom = 1.;
            shell.panels.timeline.offset = 0.;
            receive(
                &mut events,
                &shell
                    .engine
                    .attach_async(window.clone(), 1280, 800)
                    .unwrap(),
            );
            receive(
                &mut events,
                &shell
                    .engine
                    .request(|reply| Command::TestStudy {
                        operation: |id| {
                            motion_host::session::with_session(id, |s| {
                                s.preview.set_mode(motion_render::PreviewMode::High);
                                Ok(serde_json::json!({}))
                            })
                        },
                        reply,
                    })
                    .unwrap(),
            );
            show(&mut events, &mut shell);
            crate::mcp::ToolRouter {
                engine: &shell.engine,
                access: crate::mcp::Access::Edit,
            }
            .call(
                "configure_prerender",
                serde_json::json!({"enabled":enabled,"window":3,"idleDelayMs":0}),
            )
            .unwrap();
            for number in 0..SAMPLES {
                // History verification may author another revision after the
                // measured Present. Both modes show that revision before the
                // next warmup, outside timing, as the application's redraw does.
                show(&mut events, &mut shell);
                // Reset the optional generation outside timing. A nonempty
                // cache from the previous edit is not before-input evidence.
                crate::mcp::ToolRouter {
                    engine: &shell.engine,
                    access: crate::mcp::Access::Edit,
                }
                .call(
                    "configure_prerender",
                    serde_json::json!({"enabled":enabled,"window":3,"idleDelayMs":0}),
                )
                .unwrap();
                let before = shell.engine.state().unwrap();
                let warm = warm(&mut events, &shell, enabled);
                let observation = observation(&shell);
                let point = control_point(&mut shell, scenario, number);
                let request_boundary = shell.engine.diagnostics().report()["recent"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|r| r["request"].as_u64())
                    .max()
                    .unwrap_or(0);
                shell.panels.focused = Some(if scenario == "property_drag" {
                    dock::Panel::Properties
                } else {
                    dock::Panel::Timeline
                });
                begin_phases();
                let began = Instant::now();
                feed(
                    &mut events,
                    &mut shell,
                    vec![cursor(point), button(ElementState::Pressed)],
                );
                let first = draw_dispatch(&mut events, &mut shell);
                let pressed = began.elapsed();
                let delta = if number % 2 == 0 { 20. } else { -20. };
                let moved = if scenario == "property_drag" {
                    [point[0] + delta, point[1]]
                } else {
                    [point[0] + 5., point[1]]
                };
                feed(&mut events, &mut shell, vec![cursor(moved)]);
                let movement = draw_dispatch(&mut events, &mut shell);
                let changed = began.elapsed();
                feed(
                    &mut events,
                    &mut shell,
                    vec![button(ElementState::Released)],
                );
                let released = draw_dispatch(&mut events, &mut shell);
                let committed = began.elapsed();
                let polls = show(&mut events, &mut shell);
                let elapsed = began.elapsed();
                let phases = finish_phases();
                evidence.partial.push(serde_json::json!({"sample":number,"foregroundUs":elapsed.as_micros(),
                    "pressUs":pressed.as_micros(),"changeUs":changed.as_micros(),"commitUs":committed.as_micros(),
                    "presentUs":elapsed.saturating_sub(committed).as_micros(),"presentPolls":polls,
                    "actions":[first,movement,released],"before":observation,"warm":warm,"phases":phases,
                    "verified":false}));
                evidence.save();
                // All state reads, pixel readback, history checks and disk IO
                // are outside the measured interval.
                let state = shell.engine.state().unwrap();
                let timing = shell.engine.diagnostics().report();
                let worker_timing: Vec<_> = timing["recent"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|r| {
                        r["category"] == "worker"
                            && r["request"]
                                .as_u64()
                                .is_some_and(|id| id > request_boundary)
                    })
                    .cloned()
                    .collect();
                let sample = evidence.partial.last_mut().unwrap();
                sample["workerTiming"] = serde_json::json!(worker_timing);
                sample["gpu"] = timing["gpu"].clone();
                evidence.save();
                let pixels = receive(
                    &mut events,
                    &shell
                        .engine
                        .request(|reply| Command::TestStudy {
                            operation: full_front,
                            reply,
                        })
                        .unwrap(),
                );
                assert!(state["desktop"]["prerender"]["fault"].is_null(), "{state}");
                assert_eq!(state["lastPresentedFrame"], state["frame"]);
                assert_eq!(pixels["gestureActive"], false);
                let expected_x = 240. + if number % 2 == 0 { 10. } else { 0. };
                if scenario == "property_drag" {
                    assert!(movement > 0 && released > 0, "drag must change and commit");
                    assert_eq!(state["sampledLayers"][0]["position"][0], expected_x);
                    assert_ne!(state["project"], before["project"]);
                    let undo = shell.engine.history(0).unwrap();
                    assert_eq!(
                        undo["project"], before["project"],
                        "one gesture must be one history entry"
                    );
                    shell.accept_edit_state(shell.engine.history(1).unwrap());
                    assert_eq!(shell.state["project"], state["project"]);
                } else {
                    assert!(
                        first > 0 && movement > 0,
                        "scrub must reach production seek handling"
                    );
                    assert_eq!(state["project"], before["project"]);
                    assert!(state["frame"].as_f64().unwrap() > before["frame"].as_f64().unwrap());
                }
                let result = serde_json::json!({"frame":state["frame"],
                    "position":state["sampledLayers"][0]["position"],"pixels":pixels,
                    "project":state["project"]});
                if episode == 0 {
                    reference.push(result.clone());
                } else {
                    assert_eq!(
                        result, reference[number],
                        "scenario={scenario} episode={episode} sample={number}"
                    );
                }
                let sample = evidence.partial.last_mut().unwrap();
                sample["result"] = result;
                sample["revision"] = state["revision"].clone();
                sample["prerender"] = state["desktop"]["prerender"].clone();
                sample["verified"] = Value::Bool(true);
                evidence.save();
            }
            evidence.runs.push(
                serde_json::json!({"scenario":scenario,"episode":episode,"enabled":enabled,
                "samples":std::mem::take(&mut evidence.partial)}),
            );
            evidence.save();
            shell
                .engine
                .configure_prerender(Default::default())
                .unwrap();
            drop(shell);
        }
    }
    // A versioned independent parser evaluates P95 from every raw sample;
    // successful execution alone is not performance acceptance.
}

fn full_front(id: i64) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |s| {
        let g = s.graphics.as_ref().ok_or("surface missing")?;
        assert!(g.scratch.valid());
        let target = g.renderer.image_capture_target(
            640,
            360,
            motion_effects::image_contract::ImageContract::new(
                motion_effects::image_contract::ImageMeaning::Color {
                    space: motion_effects::WorkingSpace::Linear,
                    alpha: motion_effects::AlphaMode::Premultiplied,
                },
                motion_effects::image_contract::ChannelPrecision::Unorm8,
            ),
        )?;
        motion_render::Presenter::new_admitted(
            &g.renderer,
            &g.scratch.view,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )?
        .draw(&g.renderer, target.view())?;
        let bytes = g.renderer.read_image_target(&target)?.pixels;
        assert_eq!(bytes.len(), 640 * 360 * 4);
        assert!(
            bytes
                .chunks_exact(4)
                .any(|p| p[0] > p[1].saturating_add(20)),
            "visible control missing"
        );
        Ok(
            serde_json::json!({"sha256":format!("{:x}",Sha256::digest(&bytes)),"bytes":bytes.len(),
            "gestureActive":s.engine.gesture_active()}),
        )
    })
}
