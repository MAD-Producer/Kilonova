//! Use the real default desktop logging sink, not a test logger substitute.
use super::*;
use motion_host::color_preparation::{self as prep, Observation};
use std::fs;
#[path = "../../../../../crates/motion-core/tests/support/color_project.rs"]
mod fixture;

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
        "Backtrace::force_capture",
    ] {
        assert!(file.contains(cause), "{file}");
        assert!(stderr.contains(cause), "{stderr}");
        assert!(!stdout.contains(cause));
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
