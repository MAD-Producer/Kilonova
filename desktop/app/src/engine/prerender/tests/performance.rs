//! Explicit native model playback study; never part of ordinary test gates.
use super::*;

fn ui() -> UiFrame {
    UiFrame {
        paint: Default::default(),
        atlas: None,
        logical_size: [640., 360.],
        composition: Some([0., 0., 640., 360.]),
        composition_image: None,
        window: 0,
        node_preview: None,
    }
}
fn show(events: &mut EventLoop<()>, engine: &Engine) -> u32 {
    let began = Instant::now();
    let mut polls = 0;
    loop {
        polls += 1;
        if receive(events, &engine.present_async(ui()).unwrap()) {
            return polls;
        }
        assert!(began.elapsed() < Duration::from_secs(30));
    }
}
fn p95(records: &[Value]) -> f64 {
    let mut costs: Vec<_> = records
        .iter()
        .map(|r| r["foreground_us"].as_f64().unwrap())
        .collect();
    costs.sort_by(f64::total_cmp);
    costs[(costs.len() * 95).div_ceil(100) - 1]
}
struct Evidence {
    root: PathBuf,
    runs: Vec<Value>,
    current: Vec<Value>,
}
impl Evidence {
    fn flush(&self) {
        let report = serde_json::json!({"runs":self.runs,"partialFrames":self.current,
            "composition":[640,360],"window":[640,360],"fps":30,"framesPerEpisode":60,
            "order":["disabled","enabled","enabled","disabled"],
            "profile":"High authored single-time; owned animated triangle; explicit frame cadence",
            "limits":"Native Engine-only; no UI build, whole-app input or universal hardware guarantee"});
        if let Ok(bytes) = serde_json::to_vec_pretty(&report) {
            if let Err(error) = std::fs::write(self.root.join("paced-costs.json"), bytes) {
                eprintln!("paced study evidence write failed: {error}");
            }
        }
    }
}
impl Drop for Evidence {
    fn drop(&mut self) {
        self.flush();
    }
}

#[test]
#[ignore = "manual native paired cold/warm P95 and cache-hit study"]
fn paced_model_playback_records_paired_foreground_costs_and_real_cache_hits() {
    const NAME: &str = "engine::prerender::tests::performance::paced_model_playback_records_paired_foreground_costs_and_real_cache_hits";
    const CHILD: &str = "MOTION_PRERENDER_PACED_CHILD";
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
        if let Some(target) = std::env::var_os("MOTION_PRERENDER_PACED_ARTIFACTS") {
            let target = PathBuf::from(target);
            std::fs::create_dir_all(&target).unwrap();
            for name in ["paced-costs.json", "motion-studio.log"] {
                let path = root.path().join(name);
                if path.exists() {
                    std::fs::copy(path, target.join(name)).unwrap();
                }
            }
            std::fs::write(target.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(target.join("stderr.log"), &output.stderr).unwrap();
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
        root: root.clone(),
        runs: Vec::new(),
        current: Vec::new(),
    };
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(640, 360)),
            )
            .unwrap(),
    );
    let original = crate::engine::model_import_tests::triangle(&root.join("original"), "obj");
    let mut final_pixels = None;
    for (episode, enabled) in [false, true, true, false].into_iter().enumerate() {
        let directory = root.join(format!("episode-{episode}"));
        let project = motion_core::Project::new(640, 360, 30, 240).unwrap();
        motion_core::storage::save(&directory, &project).unwrap();
        let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
        let revision = engine.state().unwrap()["revision"].as_u64().unwrap();
        let imported = engine.import_model(original.clone(), revision).unwrap();
        let object = imported["modelImport"]["object"].as_u64().unwrap();
        let commands = [
            motion_core::Command::SetVector {
                object,
                property: motion_core::Property::Position,
                frame: 0,
                value: [240., 180., 0.],
            },
            motion_core::Command::Animate {
                object,
                property: motion_core::Property::Position,
                axis: None,
                frame: 0,
                enabled: true,
            },
            motion_core::Command::SetVector {
                object,
                property: motion_core::Property::Position,
                frame: 120,
                value: [400., 180., 0.],
            },
        ];
        engine
            .edit(serde_json::to_string(&commands).unwrap())
            .unwrap();
        let authored = engine.state().unwrap()["project"].clone();
        receive(
            &mut events,
            &engine.attach_async(window.clone(), 640, 360).unwrap(),
        );
        receive(
            &mut events,
            &engine
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
        show(&mut events, &engine);
        let control = crate::mcp::ToolRouter {
            engine: &engine,
            access: crate::mcp::Access::Edit,
        };
        control
            .call(
                "configure_prerender",
                serde_json::json!({"enabled":enabled,"window":3}),
            )
            .unwrap();
        engine.set_prerender_playing(true).unwrap();
        if enabled {
            let began = Instant::now();
            loop {
                let state = engine.state().unwrap();
                assert!(state["desktop"]["prerender"]["fault"].is_null(), "{state}");
                if state["desktop"]["prerender"]["cache"]["resident_frames"].as_u64() == Some(3) {
                    break;
                }
                assert!(began.elapsed() < Duration::from_secs(30), "{state}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let began = Instant::now();
        for number in 1..=60u32 {
            let deadline = began + Duration::from_secs_f64(f64::from(number) / 30.);
            while Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            let start = Instant::now();
            receive(&mut events, &engine.seek_async(f64::from(number)).unwrap());
            let seek_elapsed = start.elapsed();
            let polls = show(&mut events, &engine);
            let elapsed = start.elapsed();
            let state = engine.state().unwrap();
            evidence.current.push(
                serde_json::json!({"frame":number,"foreground_us":elapsed.as_micros(),
                "seek_us":seek_elapsed.as_micros(),
                "present_us":elapsed.saturating_sub(seek_elapsed).as_micros(),
                "presentPolls":polls,"completion_late_us":deadline.elapsed().as_micros(),
                "missedNextDeadline":Instant::now() > deadline + Duration::from_secs_f64(1./30.),
                "cachedPresented":state["desktop"]["prerender"]["cachedPresented"],
                "prerender":state["desktop"]["prerender"]}),
            );
            assert_eq!(state["lastPresentedFrame"], f64::from(number));
            assert!(state["desktop"]["prerender"]["fault"].is_null(), "{state}");
        }
        let front = read_front(&mut events, &engine);
        if let Some(pixels) = &final_pixels {
            assert_eq!(&front["pixels"], pixels);
        } else {
            final_pixels = Some(front["pixels"].clone());
        }
        assert!(
            front["pixels"]
                .as_array()
                .unwrap()
                .chunks_exact(4)
                .any(|p| p[0].as_u64().unwrap() > p[1].as_u64().unwrap() + 20),
            "model must be visible"
        );
        assert_eq!(engine.state().unwrap()["project"], authored);
        let hits = front["cachedPresented"].as_u64().unwrap();
        let costs = p95(&evidence.current);
        evidence.runs.push(
            serde_json::json!({"episode":episode,"enabled":enabled,"p95_us":costs,
            "cachedPresented":hits,"frames":std::mem::take(&mut evidence.current)}),
        );
        evidence.flush();
        if enabled {
            assert!(hits >= 30, "sustained playback must cache at least half of the 60 presented frames; got {hits}");
            assert!(
                hits > evidence.runs.last().unwrap()["frames"][49]["cachedPresented"].as_u64().unwrap(),
                "early cache hits must not hide a stalled final ten-frame window"
            );
        } else {
            assert_eq!(hits, 0);
        }
        engine.set_prerender_playing(false).unwrap();
        control
            .call("configure_prerender", serde_json::json!({"enabled":false}))
            .unwrap();
        drop(engine);
    }
    // Actual cost ratios are evaluated from the persisted paired raw samples;
    // successful test execution alone is never P95 non-regression acceptance.
}
