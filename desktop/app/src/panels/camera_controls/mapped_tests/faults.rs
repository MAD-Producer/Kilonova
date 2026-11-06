use super::*;
const NAME:&str="panels::camera_controls::mapped_tests::faults::camera_time_failures_are_bounded_and_visible_on_default_channels";
#[test]
fn camera_time_failures_are_bounded_and_visible_on_default_channels() {
    const CHILD: &str = "MOTION_MAPPED_CAMERA_FAULT_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut h = Harness::new(false, Locale::En, false);
        let healthy = h.state.clone();
        for _ in 0..120 {
            h.state = healthy.clone();
            h.state["document_generation"] = Value::Null;
            assert!(h.draw(Input::default()).is_empty());
            h.state = healthy.clone();
            assert!(h.draw(Input::default()).is_empty());
        }
        assert_eq!(h.engine.state().unwrap()["project"], healthy["project"]);
        // An actual worker failure after one valid candidate must roll back
        // both film width and FOV, retaining the primary cause on stderr.
        let failure=h.engine.edit(json!([
            {"op":"camera_lens","object":7,"frame":6.5,"film_width_mm":24.,"focal_length_mm":35.},
            {"op":"camera_lens","object":7,"frame":7.,"film_width_mm":36.,"focal_length_mm":50.}
        ]).to_string()).unwrap_err();
        assert!(failure.to_string().contains("continuous key addressing"), "{failure}");
        assert_eq!(h.engine.state().unwrap()["project"], healthy["project"]);
        crate::logging::flush();
        let log = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let lines: Vec<_> = log
            .lines()
            .filter(|l| l.contains("site=camera.numeric_time "))
            .collect();
        assert_eq!(lines.len(), 2, "{log}");
        assert!(lines[0].contains("WARN") && lines[0].contains("count=1 "));
        assert!(lines[1].contains("ERROR") && lines[1].contains("count=60 "));
        assert!(lines
            .iter()
            .all(|l| l.contains("object=7") && l.contains("generation")));
        assert!(log
            .lines()
            .any(|l| l.contains("INFO") && l.contains("start #") && l.contains(" edit")));
        assert!(log
            .lines()
            .any(|l| l.contains("ERROR") && l.contains("continuous key addressing")));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_MAPPED_CAMERA_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &out.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &out.stderr).unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert_eq!(
        stderr
            .lines()
            .filter(|l| l.contains("site=camera.numeric_time "))
            .count(),
        1
    );
    assert!(stderr.contains("continuous key addressing"));
    assert!(
        !stdout.contains("camera.numeric_time")
            && !stdout.contains(" INFO [")
            && !stdout.contains(" ERROR ")
    );
}
