use super::*;
const NAME:&str="panels::path_controls::mapped_authoring_tests::faults::mapped_path_time_faults_use_default_bounded_error_channels";
const SITES: [&str; 4] = [
    "mask.numeric_time_begin",
    "mask.numeric_time_continue",
    "vector.numeric_time_begin",
    "vector.numeric_time_continue",
];
#[test]
fn mapped_path_time_faults_use_default_bounded_error_channels() {
    const CHILD: &str = "MOTION_MAPPED_PATH_FAULT_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _log = crate::logging::init(root.clone()).unwrap();
        for host in [Panel::Masks, Panel::Vector] {
            let mut h = Harness::new(host, false, Locale::En, false);
            let healthy = h.state.clone();
            let before = healthy["project"].clone();
            let pt = center(h.rect().1);
            for _ in 0..120 {
                h.state = healthy.clone();
                h.state["document_generation"] = Value::Null;
                let out = h.draw(pointer(pt, true, false));
                assert!(out.is_empty(), "{host:?}: {out:?}");
                h.state = healthy.clone();
                h.input(pointer(pt, true, false));
                let before_field = h.state.clone();
                h.state["document_generation"] = Value::Null;
                assert!(h.draw(Input::default()).is_empty());
                h.state = before_field;
                assert_eq!(h.engine.state().unwrap()["project"], before);
            }
        }
        crate::logging::flush();
        let log = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in SITES {
            let lines: Vec<_> = log
                .lines()
                .filter(|l| l.contains(&format!("site={site} ")))
                .collect();
            assert_eq!(lines.len(), 2, "{site}: {log}");
            assert!(lines[0].contains("WARN") && lines[0].contains("count=1 "));
            assert!(lines[1].contains("ERROR") && lines[1].contains("count=60 "));
            assert!(lines
                .iter()
                .all(|l| l.contains("object=7") && l.contains("reason=")));
        }
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_MAPPED_PATH_FAULT_ARTIFACTS")
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
