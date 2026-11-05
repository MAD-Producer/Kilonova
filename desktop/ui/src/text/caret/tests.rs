use crate::text::{format_controls_tests::Capture, TextAtlas};
use crate::{
    paint::PaintList,
    widgets::{TextEditor, TextViewport},
    Input, Rect,
};
use std::{fs, fs::File, sync::Mutex};

#[test]
fn editor_failures_keep_default_level_evidence_and_healthy_source_text() {
    const CHILD: &str = "MOTION_UI_CARET_CHILD";
    const NAME: &str =
        "text::caret::tests::editor_failures_keep_default_level_evidence_and_healthy_source_text";
    if let Some(path) = std::env::var_os(CHILD) {
        log::set_logger(Box::leak(Box::new(Capture(Mutex::new(
            File::create(&path).unwrap(),
        )))))
        .unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let layout = atlas.layout_line("e\u{301}").unwrap();
        assert_eq!(layout.caret_x(1), None);
        let oversized = "PRIVATE-EDITOR-TEXT".repeat(super::super::shaping::MAX_RUN_BYTES / 18 + 1);
        let mut editor = TextEditor::default();
        editor.begin_multiline("healthy", 1024);
        let area = Rect::new(0., 0., 300., 80.);
        let render = |atlas: &mut TextAtlas, editor: &mut TextEditor, bad: bool| {
            let mut paint = PaintList::default();
            editor.multiline_view_colored(
                &mut TextViewport::default(),
                &mut paint,
                atlas,
                area,
                &Input::default(),
                &|_, s| {
                    vec![(
                        if bad {
                            "PRIVATE-SYNTAX-REPLACEMENT".into()
                        } else {
                            s.into()
                        },
                        crate::Color::rgb(200, 20, 20),
                    )]
                },
            );
            paint
        };
        for i in 0..120 {
            assert!(atlas.editor_layout(&oversized).is_none());
            assert_eq!(atlas.editor_caret(&layout, 1), 0.);
            let bad = render(&mut atlas, &mut editor, true);
            let good = render(&mut atlas, &mut editor, false);
            let geometry = |p: PaintList| {
                p.glyphs
                    .into_iter()
                    .map(|g| (g.position, g.size, g.uv))
                    .collect::<Vec<_>>()
            };
            let actual = geometry(bad);
            assert!(!actual.is_empty());
            assert_eq!(
                actual,
                geometry(good),
                "faulty syntax callback must not replace source glyphs"
            );
            assert!(atlas.editor_layout("healthy").is_some());
            if i == 29 {
                atlas.clear();
            }
        }
        let events = fs::read_to_string(&path).unwrap();
        for key in ["editor", "caret_boundary", "editor_syntax"] {
            let marker = format!("site=text.layout key={key:?} ");
            let lines = events
                .lines()
                .filter(|s| s.contains(&marker))
                .collect::<Vec<_>>();
            assert_eq!(lines.len(), 2, "{events}");
            assert!(lines[0].starts_with("WARN ") && lines[0].contains("count=1 "));
            assert!(lines[1].starts_with("ERROR ") && lines[1].contains("count=60 "));
        }
        assert_eq!(events.lines().count(), 6, "{events}");
        assert!(events.contains("byte=1 bytes=3") && events.contains("limit=16384"));
        assert!(
            !events.contains("PRIVATE-EDITOR-TEXT")
                && !events.contains("PRIVATE-SYNTAX-REPLACEMENT")
        );
        let mut replacement = TextAtlas::from_system(12.).unwrap();
        for _ in 0..60 {
            assert!(replacement.editor_layout(&oversized).is_none());
        }
        assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 8);
        return;
    }
    let explicit = std::env::var_os("MOTION_UI_CARET_ARTIFACTS");
    let root = explicit
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "motion-caret-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
        });
    fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG")
        .env(CHILD, root.join("events.log"))
        .output()
        .unwrap();
    fs::write(root.join("child-stdout.log"), &output.stdout).unwrap();
    fs::write(root.join("child-stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains("UI degradation") && !stdout.contains("PRIVATE-"));
    assert!(!stderr.contains("WARN ") && !stderr.contains("PRIVATE-"));
    assert_eq!(
        stderr
            .lines()
            .filter(|s| s.starts_with("ERROR ") && s.contains("site=text.layout"))
            .count(),
        4,
        "{stderr}"
    );
    if explicit.is_none() {
        for name in ["events.log", "child-stdout.log", "child-stderr.log"] {
            fs::remove_file(root.join(name)).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }
}
