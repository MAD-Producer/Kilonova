use super::*;
const NAME: &str="panels::path_controls::mapped_authoring_tests::navigation::faults::path_navigation_faults_use_default_bounded_file_and_stderr_channels";
const SITES: [&str; 3] = [
    "mask.navigation_time",
    "vector.navigation_time",
    "path.navigation_selection",
];
#[test]
fn path_navigation_faults_use_default_bounded_file_and_stderr_channels() {
    const CHILD: &str = "MOTION_PATH_NAVIGATION_FAULT_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _log = crate::logging::init(root.clone()).unwrap();
        for host in [Panel::Masks, Panel::Vector] {
            let mut h = Harness::new(host, false, Locale::En, false);
            let healthy = h.state.clone();
            for _ in 0..120 {
                h.state = healthy.clone();
                h.state["frame"] = Value::Null;
                assert!(h.draw(Input::default()).is_empty());
                h.state = healthy.clone();
                assert!(h.draw(Input::default()).is_empty());
            }
            assert_eq!(h.engine.state().unwrap()["project"], healthy["project"]);
        }
        let mut h = Harness::new(Panel::Vector, false, Locale::En, false);
        let healthy = h.state.clone();
        let pt = super::rail(&h, 0, 2);
        for _ in 0..120 {
            h.state = healthy.clone();
            h.state["document_generation"] = Value::Null;
            let out = h.draw(pointer(pt, true, false));
            assert!(matches!(out.as_slice(), [Action::Seek(11.)]));
            h.state = healthy.clone();
            assert!(h.draw(pointer(pt, false, true)).is_empty());
        }
        assert_eq!(h.engine.state().unwrap()["project"], healthy["project"]);
        crate::logging::flush();
        let log = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in SITES {
            let records: Vec<_> = log
                .lines()
                .filter(|l| l.contains(&format!("site={site} ")))
                .collect();
            assert_eq!(records.len(), 2, "{site}: {log}");
            assert!(records[0].contains("WARN") && records[0].contains("count=1 "));
            assert!(records[1].contains("ERROR") && records[1].contains("count=60 "));
            assert!(records
                .iter()
                .all(|l| l.contains("object=") && l.contains("reason=")));
        }
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PATH_NAVIGATION_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    for site in SITES {
        let lines: Vec<_> = stderr
            .lines()
            .filter(|l| l.contains(&format!("site={site} ")))
            .collect();
        assert_eq!(lines.len(), 1, "{site}: {stderr}");
        assert!(lines[0].contains("ERROR") && lines[0].contains("count=60 "));
        assert!(!stdout.contains(site));
    }
    assert!(!stdout.contains(" INFO [") && !stdout.contains(" ERROR "));
}
