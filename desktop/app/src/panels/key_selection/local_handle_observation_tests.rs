//! Faults use the actual draw entry and default file/stderr sinks.
use super::*;
#[test]
fn malformed_projected_handle_keys_are_bounded_visible_and_recovery_is_quiet() {
    const CHILD: &str = "MOTION_GRAPH_HANDLE_FAULT_CHILD";
    const NAME:&str="panels::key_selection::tests::ui::local_timing::handles::observation::malformed_projected_handle_keys_are_bounded_visible_and_recovery_is_quiet";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _log = crate::logging::init(root.clone()).unwrap();
        let e = fixture(false);
        let s = view(&e);
        let mut p = ready(&s, Locale::En, false);
        let source =
            crate::panels::animation_data::channel(&s, 7, Property::Position, None).unwrap();
        let mut bad = source.clone();
        bad["keys"][0]["local_frame"] = Value::Null;
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let plot = crate::panels::graph_handles::Plot {
            area: motion_ui::Rect::new(0., 0., 800., 400.),
            offset: 0.,
            ppf: 10.,
            low: -10.,
            high: 30.,
            speed: false,
            fps: 30.,
            scale: 1.,
        };
        let mut render = |source: &Value| {
            let mut paint = PaintList::default();
            let mut actions = vec![];
            crate::panels::graph_handles::draw(
                &mut paint,
                &mut atlas,
                &mut p,
                source,
                &s,
                7,
                Property::Position,
                None,
                0,
                plot,
                &Input::default(),
                false,
                &mut actions,
            );
            assert!(actions.is_empty());
            paint
        };
        assert_eq!(points(&render(source)).len(), 4);
        for _ in 0..120 {
            assert!(render(&bad).shapes.is_empty());
            assert_eq!(points(&render(source)).len(), 4);
        }
        let log = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert_eq!(
            log.lines()
                .filter(|l| l.contains("site=graph_handles.local_keys count=1 "))
                .count(),
            1
        );
        assert_eq!(
            log.lines()
                .filter(|l| l.contains("site=graph_handles.local_keys count=60 "))
                .count(),
            1
        );
        assert!(log.contains("object=7 property=Position axis=None phase=handle_keys"));
        assert!(log.contains("local identity missing"));
        return;
    }
    let root = std::env::var_os("MOTION_GRAPH_HANDLE_FAULT_ARTIFACT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("motion-handle-fault-{}", std::process::id()))
        });
    std::fs::create_dir_all(&root).unwrap();
    for episode in [1, 2] {
        let folder = root.join(format!("process-{episode}"));
        std::fs::create_dir_all(&folder).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, &folder)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        std::fs::write(folder.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(folder.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("persistent panel degradation site=graph_handles.local_keys count=60"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
    }
}
