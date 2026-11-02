pub(in crate::panels) fn verify(name: &str, label: &str, sites: &[&str], body: impl FnOnce()) {
    const CHILD: &str = "MOTION_STABLE_SPATIAL_FAULT_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        let root =
            std::path::PathBuf::from(std::env::var_os("MOTION_STABLE_SPATIAL_FAULT_ROOT").unwrap());
        let _guard = crate::logging::init(root.clone()).unwrap();
        body();
        crate::logging::flush();
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in sites {
            let lines: Vec<_> = file
                .lines()
                .filter(|l| l.contains(&format!("site={site} ")))
                .collect();
            assert_eq!(lines.len(), 2, "{site}: {file}");
            assert!(lines[0].contains("WARN") && lines[0].contains("count=1 "));
            assert!(lines[1].contains("ERROR") && lines[1].contains("count=60 "));
            assert!(lines
                .iter()
                .all(|l| l.contains("object=7") && l.contains("reason=")));
        }
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_STABLE_SPATIAL_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned())
        .join(label);
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(CHILD, name)
        .env("MOTION_STABLE_SPATIAL_FAULT_ROOT", &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    for site in sites {
        let lines: Vec<_> = stderr
            .lines()
            .filter(|l| l.contains(&format!("site={site} ")))
            .collect();
        assert_eq!(lines.len(), 1, "{stderr}");
        assert!(lines[0].contains("ERROR") && lines[0].contains("count=60 "));
        assert!(!stdout.contains(site));
    }
    assert!(!stdout.contains(" INFO [") && !stdout.contains(" ERROR "));
}
