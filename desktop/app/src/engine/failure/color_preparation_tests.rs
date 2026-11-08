//! Use the real default desktop logging sink, not a test logger substitute.
use super::*;
use motion_host::color_preparation::{self as prep, Observation};
use std::fs;
#[path = "../../../../../crates/motion-core/tests/support/color_project.rs"]
mod fixture;

#[test]
fn native_color_runtime_load_failure_reaches_the_default_error_channels() {
    const CHILD: &str = "MOTION_NATIVE_COLOR_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::native_color_runtime_load_failure_reaches_the_default_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let project = fixture::project();
        fixture::write_resources(&root, &project);
        let before = serde_json::to_value(&project).unwrap();
        let observation = Observation::new(8101);
        let error = prep::prepare_native_project_colors(
            &project,
            &root,
            &root.join("unavailable-native-ocio.json"),
            &observation,
        )
        .err()
        .unwrap();
        let prep::Error::Provider { source, .. } = error else {
            panic!("native provider context must be retained")
        };
        let motion_host::Error::NativeColor(motion_effects::native_color::Error::Io {
            phase: "manifest",
            source,
            ..
        }) = *source
        else {
            panic!("original typed runtime IO error must be retained")
        };
        assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        assert!(!observation.running());
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
        let unmanaged = motion_core::Project::new(64, 64, 30, 120).unwrap();
        assert!(prep::prepare_native_project_colors(
            &unmanaged,
            &root,
            &root.join("unavailable-native-ocio.json"),
            &Observation::new(8102)
        )
        .unwrap()
        .is_none());
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout_path = root.path().join("child.stdout");
    let stderr_path = root.path().join("child.stderr");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .stdout(fs::File::create(&stdout_path).unwrap())
        .stderr(fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(20) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("native color log child exceeded 20s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("ERROR") && line.contains("color preparation request="))
        .collect();
    assert_eq!(errors.len(), 1, "{file}");
    let error = errors[0];
    for text in [
        "request=8101",
        "target=DefaultInput",
        "provider failed",
        "native OCIO phase=manifest",
        "unavailable-native-ocio.json",
        "elapsed_ms=",
    ] {
        assert!(error.contains(text), "{file}");
        assert!(stderr.contains(text), "{stderr}");
        assert!(!stdout.contains(text), "{stdout}");
    }
    let start = "color preparation request=8101 operation=prepare_project_colors phase=start";
    assert!(file.find(start).unwrap() < file.find(error).unwrap());
    assert!(!stderr.contains(start));
    assert!(!errors.iter().any(|line| line.contains("request=8102")));
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("motion-studio.log"), file).unwrap();
        fs::write(directory.join("child.stdout"), stdout).unwrap();
        fs::write(directory.join("child.stderr"), stderr).unwrap();
    }
}

#[test]
fn project_color_preparation_records_starts_original_causes_and_unwind_at_default_level() {
    const CHILD: &str = "MOTION_COLOR_PREPARATION_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::project_color_preparation_records_starts_original_causes_and_unwind_at_default_level";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let project = fixture::project();
        let observation = Observation::new(7001);
        assert!(matches!(
            prep::prepare_project_colors(
                &project,
                &root,
                |_| panic!("missing config cannot enter provider"),
                &observation
            ),
            Err(prep::Error::Resources(_))
        ));
        fixture::write_resources(&root, &project);
        let observation = Observation::new(7002);
        let error = prep::prepare_project_colors(
            &project,
            &root,
            |_| {
                assert!(observation.running());
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "original pinned color provider unavailable",
                )
                .into())
            },
            &observation,
        )
        .err()
        .unwrap();
        assert!(matches!(error, prep::Error::Provider { .. }));
        let observation = Observation::new(7003);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = prep::prepare_project_colors(
                &project,
                &root,
                |_| panic!("controlled color provider panic"),
                &observation,
            );
        }));
        assert!(result.is_err());
        assert!(!observation.running());
        let observation = Observation::new(7004);
        assert!(prep::prepare_project_colors(
            &motion_core::Project::new(64, 64, 30, 120).unwrap(),
            &root,
            |_| panic!("unmanaged is a normal branch"),
            &observation
        )
        .unwrap()
        .is_none());
        assert!(!observation.running());
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout_path = root.path().join("child.stdout");
    let stderr_path = root.path().join("child.stderr");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .env("RUST_BACKTRACE", "0")
        .stdout(fs::File::create(&stdout_path).unwrap())
        .stderr(fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(20) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("color preparation child exceeded 20s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("ERROR") && line.contains("color preparation request="))
        .collect();
    assert_eq!(errors.len(), 3, "{file}");
    for request in [7001, 7002, 7003] {
        let prefix = format!("color preparation request={request}");
        let start = file
            .find(&format!(
                "{prefix} operation=prepare_project_colors phase=start"
            ))
            .unwrap();
        let error = errors.iter().find(|line| line.contains(&prefix)).unwrap();
        assert!(start < file.find(*error).unwrap());
        assert!(error.contains("elapsed_ms="));
        assert!(stderr.contains(&prefix));
        assert!(!stdout.contains(&prefix));
        let start_line = file
            .lines()
            .find(|line| {
                line.contains(&format!(
                    "{prefix} operation=prepare_project_colors phase=start"
                ))
            })
            .unwrap();
        assert!(!stderr.contains(start_line));
    }
    for cause in [
        "original pinned color provider unavailable",
        "phase=resolve",
        "phase=abandoned",
        "controlled color provider panic",
    ] {
        assert!(file.contains(cause), "{file}");
        assert!(stderr.contains(cause), "{stderr}");
        assert!(!stdout.contains(cause));
    }
    let has_frame = |text: &str| {
        text.lines().any(|line| {
            line.trim_start()
                .split_once(':')
                .is_some_and(|(index, frame)| {
                    index.parse::<u32>().is_ok() && !frame.trim().is_empty()
                })
        })
    };
    assert!(
        has_frame(&file),
        "forced file backtrace is required even with RUST_BACKTRACE=0"
    );
    assert!(
        has_frame(&stderr),
        "forced stderr backtrace is required even with RUST_BACKTRACE=0"
    );
    // Negative control uses the actual captured failure, retaining its cause
    // and context but removing its frame records. A cause alone is not a trace.
    for original in [&file, &stderr] {
        let without_frames = original
            .lines()
            .filter(|line| !has_frame(line))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(without_frames.contains("controlled color provider panic"));
        assert!(!has_frame(&without_frames));
    }
    assert!(!errors.iter().any(|line| line.contains("request=7004")));
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("motion-studio.log"), file).unwrap();
        fs::write(directory.join("child.stdout"), stdout).unwrap();
        fs::write(directory.join("child.stderr"), stderr).unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn original_source_color_failures_use_default_file_and_stderr_without_faulting_data_or_cancel() {
    use motion_effects::{color_processor::*, image_contract::ImageMeaning};
    use prep::SourceImageRequest;
    use std::sync::atomic::AtomicBool;
    const CHILD: &str = "MOTION_COLOR_SOURCE_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::original_source_color_failures_use_default_file_and_stderr_without_faulting_data_or_cancel";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut p = fixture::project();
        p.assets = (1..=2)
            .map(|id| motion_core::Asset {
                id,
                path: format!("assets/{id}.png"),
                width: 3,
                height: 4,
            })
            .collect();
        let motion_core::color_management::ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut p.color_management)
        else {
            panic!("managed fixture")
        };
        settings
            .sources
            .push(motion_core::color_management::SourceColorAssignment {
                asset: 2,
                interpretation: motion_core::color_management::SourceInterpretation::Data {
                    meaning: ImageMeaning::LinearDepth {},
                },
            });
        fixture::write_resources(&root, &p);
        let prepared = prep::prepare_project_colors(
            &p,
            &root,
            |r| {
                Ok(ColorProcessorSource {
                    version: COLOR_PROCESSOR_VERSION,
                    ocio_version: r.config.manifest().ocio_version,
                    config_key: r.config.key(),
                    selection_key: r.config.resolve(r.selection).unwrap().key,
                    phase: r.phase,
                    input_alpha: r.input_alpha,
                    output_alpha: r.output_alpha,
                    shader: "vec4 motion_ocio_transform(vec4 color) { return color; }".into(),
                    tables: Vec::new(),
                })
            },
            &Observation::new(8300),
        )
        .unwrap()
        .unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let no = AtomicBool::new(false);
        let yes = AtomicBool::new(true);
        let req = |asset, cancel| SourceImageRequest {
            project: &p,
            root: &root,
            asset,
            cancel,
        };
        let observation = Observation::new(8301);
        assert!(
            matches!(prepared.prepare_source_image(&mut renderer,req(1,&no),&observation),Err(prep::Error::SourceImage{source,..}) if matches!(&*source,motion_host::Error::ImageResource(motion_render::image_resources::Error::Io(e)) if e.kind()==std::io::ErrorKind::NotFound))
        );
        assert!(!observation.running());
        fs::create_dir_all(root.join("assets")).unwrap();
        let path = root.join("assets/1.png");
        image::save_buffer(
            &path,
            &[128, 64, 32, 127].repeat(12),
            3,
            4,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let good = fs::read(&path).unwrap();
        let mut broken = good.clone();
        *broken.last_mut().unwrap() ^= 1;
        fs::write(&path, broken).unwrap();
        assert!(
            matches!(prepared.prepare_source_image(&mut renderer,req(1,&no),&Observation::new(8302)),Err(prep::Error::SourceImage{source,..}) if matches!(*source,motion_host::Error::ImageResource(motion_render::image_resources::Error::Png(_))))
        );
        fs::write(&path, good).unwrap();
        let mut policy = renderer.memory_policy();
        policy.decoded_image_budget_bytes = 191;
        renderer.set_memory_policy(policy).unwrap();
        assert!(prepared
            .prepare_source_image(&mut renderer, req(1, &no), &Observation::new(8303))
            .is_err());
        assert!(prepared
            .prepare_source_image(&mut renderer, req(2, &no), &Observation::new(8304))
            .unwrap()
            .is_none());
        assert!(
            matches!(prepared.prepare_source_image(&mut renderer,req(1,&yes),&Observation::new(8305)),Err(prep::Error::SourceImage{source,..}) if matches!(*source,motion_host::Error::ImageResource(motion_render::image_resources::Error::Cancelled)))
        );
        policy.decoded_image_budget_bytes = 1024 * 1024;
        renderer.set_memory_policy(policy).unwrap();
        let image = prepared
            .prepare_source_image(&mut renderer, req(1, &no), &Observation::new(8306))
            .unwrap()
            .unwrap();
        assert_eq!(image.asset(), 1);
        assert_eq!(renderer.image_readback_info().requested_calls, 0);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout_path = root.path().join("child.stdout");
    let stderr_path = root.path().join("child.stderr");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .stdout(fs::File::create(&stdout_path).unwrap())
        .stderr(fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(30) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("color source child exceeded 30s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|l| l.contains("ERROR") && l.contains("operation=prepare_source_color_image"))
        .collect();
    assert_eq!(errors.len(), 3, "{file}");
    for cause in [
        "CRC error:",
        "type: IEND",
        "required_bytes=192 budget_bytes=191",
    ] {
        assert!(file.contains(cause), "{file}");
        assert!(stderr.contains(cause), "{stderr}");
        assert!(!stdout.contains(cause));
    }
    for (request, phase) in [
        (8301, "resolve_source"),
        (8302, "decode_original"),
        (8303, "decode_original"),
    ] {
        let prefix = format!(
            "color preparation request={request} operation=prepare_source_color_image asset=1"
        );
        let start = file.find(&format!("{prefix} phase=start")).unwrap();
        let error = errors.iter().find(|l| l.contains(&prefix)).unwrap();
        assert!(start < file.find(*error).unwrap());
        assert!(error.contains(&format!("phase={phase}")));
        assert!(error.contains("elapsed_ms="));
        assert!(error.contains("cause="));
        assert!(stderr.contains(&prefix));
        assert!(!stdout.contains(&prefix));
        assert!(!stderr
            .lines()
            .any(|l| l.contains(&prefix) && l.contains("phase=start")));
    }
    for request in [8304, 8305, 8306] {
        let prefix = format!("color preparation request={request}");
        assert!(file.contains(&format!("{prefix} operation=prepare_source_color_image")));
        assert!(!errors.iter().any(|l| l.contains(&prefix)));
        assert!(!stderr.contains(&prefix));
        assert!(!stdout.contains(&prefix));
    }
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("source");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("motion-studio.log"), file).unwrap();
        fs::write(directory.join("child.stdout"), stdout).unwrap();
        fs::write(directory.join("child.stderr"), stderr).unwrap();
    }
}
