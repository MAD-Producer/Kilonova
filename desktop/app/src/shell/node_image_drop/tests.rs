use super::*;
#[test]
fn native_drop_coordinate_failure_preserves_graph_and_default_error_sinks() {
    const CHILD: &str = "MOTION_NODE_DROP_POINT_CHILD";
    const NAME: &str = "shell::node_image_drop::tests::native_drop_coordinate_failure_preserves_graph_and_default_error_sinks";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = logging::init(root.join("logs")).unwrap();
        let mut shell = Shell::new(root.join("project"), None, Some(Locale::En)).unwrap();
        #[repr(C)]
        struct Point {
            x: i32,
            y: i32,
        }
        #[link(name = "user32")]
        unsafe extern "system" {
            fn ScreenToClient(window: *mut std::ffi::c_void, point: *mut Point) -> i32;
        }
        let mut point = Point { x: 0, y: 0 };
        assert_eq!(
            unsafe { ScreenToClient(std::ptr::null_mut(), &mut point) },
            0
        );
        let error = std::io::Error::last_os_error();
        let cause = error.to_string();
        let before = shell.panels.node_editor.snapshot();
        assert!(shell.drop_node_image(root.join("image.png"), Err(error), 42));
        assert_eq!(
            shell.panels.node_editor.snapshot().document,
            before.document
        );
        assert_eq!(
            shell.panels.node_editor.snapshot().revision,
            before.revision
        );
        assert!(shell.actions.is_empty() && !shell.node_tasks.busy());
        std::fs::write(root.join("cause.txt"), cause).unwrap();
        logging::flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_NODE_DROP_POINT_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let cause = std::fs::read_to_string(root.join("cause.txt")).unwrap();
    let marker = "node image drop failed window=42 phase=client_position";
    for sink in [&file, &stderr] {
        assert_eq!(sink.lines().filter(|line| line.contains(marker)).count(), 1);
        assert!(
            sink.lines()
                .any(|line| line.contains(marker) && line.contains(&cause))
        );
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(!stdout.contains(marker) && !stdout.contains(&cause));
}
