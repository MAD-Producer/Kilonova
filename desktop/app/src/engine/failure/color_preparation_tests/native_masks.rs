//! Actual managed PNG worker: coverage admission, original publication and recovery.
use super::*;
use crate::engine::failure::color_tests::runtime_platform::RuntimePlatform;
use motion_core::{
    masks::LayerMask,
    vector::{PathNode, VectorPath},
    Track,
};
use motion_desktop_media::movie::ExportControl;

#[test]
#[ignore = "requires the pinned native OCIO runtime and an actual GPU"]
fn managed_mask_budget_reaches_default_channels_and_recovers_frozen_png() {
    const CHILD: &str = "MOTION_OCIO_MASK_LOG_CHILD";
    const TEST:&str="engine::failure::color_preparation_tests::native_authored::masks::managed_mask_budget_reaches_default_channels_and_recovers_frozen_png";
    const CAUSE: &str =
        "authored sources and numeric mask coverage exceed configured source texture budget";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let active = root.join("active");
        let mut p = project(&active);
        p.background = [0.; 4];
        p.layers[0].three_d = false;
        let mut mask = LayerMask::new(
            1,
            VectorPath {
                id: 1,
                tangents: Default::default(),
                closed: true,
                nodes: [[0., 0.], [32., 0.], [32., 64.], [0., 64.]]
                    .into_iter()
                    .enumerate()
                    .map(|(i, p)| PathNode {
                        id: i as u64 + 1,
                        geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                    })
                    .collect(),
            },
        );
        mask.opacity.value = 50.;
        p.layers[0].masks.push(mask);
        let motion_core::color_management::ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut p.color_management)
        else {
            unreachable!()
        };
        settings.output.transfer = motion_core::color_management::ColorTransfer::Srgb;
        motion_core::storage::save(&active, &p).unwrap();
        let platform = Arc::new(RuntimePlatform {
            manifest: PathBuf::from(std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap()),
            before_manifest: None,
            desktop: motion_desktop_media::platform(),
        });
        let engine = Engine::start(active, platform, 8 << 30).unwrap();
        let output = root.join("output.png");
        fs::write(&output, b"prior complete publication").unwrap();
        let policy = motion_render::resource_policy::MemoryPolicy::default();
        let low = motion_render::resource_policy::MemoryPolicy {
            source_texture_budget_bytes: 4 + 64 * 64 * 4,
            ..policy
        };
        engine
            .plugin(serde_json::json!({"op":"memory_policy","policy":low}).to_string())
            .unwrap();
        let before = engine.state().unwrap();
        let error = engine
            .capture_async(output.clone(), ExportControl::default())
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("PNG colour phase=compose") && error.contains(CAUSE),
            "{error}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"prior complete publication");
        let after = engine.state().unwrap();
        for key in ["project", "revision", "history", "canUndo", "canRedo"] {
            assert!(
                before.get(key).is_some(),
                "missing complete-state key {key}"
            );
            assert_eq!(after[key], before[key], "failed mask capture changed {key}");
        }
        engine
            .plugin(serde_json::json!({"op":"memory_policy","policy":policy}).to_string())
            .unwrap();
        engine
            .capture_async(output.clone(), ExportControl::default())
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let image = image::open(&output).unwrap().into_rgba8();
        assert_eq!(image.dimensions(), (64, 64));
        assert!(image.get_pixel(16, 32).0[3].abs_diff(128) <= 1);
        assert_eq!(image.get_pixel(48, 32).0, [0; 4]);
        drop(engine);
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout_path = root.path().join("stdout.txt");
    let stderr_path = root.path().join("stderr.txt");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .stdout(fs::File::create(&stdout_path).unwrap())
        .stderr(fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(120) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("mask log child exceeded 120s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("managed-mask-worker");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("default.log"), &file).unwrap();
        fs::write(directory.join("stderr.txt"), &stderr).unwrap();
        fs::write(directory.join("stdout.txt"), &stdout).unwrap();
        if root.path().join("output.png").exists() {
            fs::copy(root.path().join("output.png"), directory.join("output.png")).unwrap();
        }
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let valid = |file: &str, stderr: &str, stdout: &str| {
        file.contains("logging initialized level=INFO")
            && file.contains("async start #")
            && file.lines().any(|line| {
                line.contains("ERROR")
                    && line.contains(" capture ")
                    && line.contains("PNG colour phase=compose")
                    && line.contains(CAUSE)
                    && line.contains("ms:")
            })
            && stderr.contains(CAUSE)
            && !stderr.contains("async start #")
            && !stdout.contains(CAUSE)
            && !stdout.contains("async start #")
    };
    assert!(valid(&file, &stderr, &stdout), "{file}\n{stderr}\n{stdout}");
    assert!(!valid(&file.replace("ERROR", "DEBUG"), &stderr, &stdout));
    assert!(!valid(&file, "", &stdout));
    assert!(!valid(&file.replace(CAUSE, "lost cause"), &stderr, &stdout));
    assert!(!valid(&file, &stderr, &format!("{stdout}{stderr}")));
}
