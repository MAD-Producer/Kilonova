use super::*;
use crate::{
    paint::PaintList,
    widgets::{TextEditor, TextViewport},
    Input, MouseButton, Rect,
};
use std::{fs, io::Write, sync::Mutex};

#[test]
fn tabs_map_every_source_grapheme_without_creating_inside_cluster_stops() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for source in ["", "AV e\u{301}", "\tA\te\u{301}", "א\tב", "\t\u{301}Z"] {
        let source_stops = source
            .grapheme_indices(true)
            .map(|(b, _)| b)
            .chain([source.len()])
            .collect::<Vec<_>>();
        for columns in 1..=16 {
            let expanded = ExpandedLine::new(source, columns);
            assert_eq!(
                matches!(expanded.text, Cow::Borrowed(_)),
                !source.contains('\t')
            );
            let layout = atlas.layout_line(&expanded.text).unwrap();
            for byte in 0..=source.len() + 1 {
                assert_eq!(
                    expanded.mapped_byte(&layout, byte).is_some(),
                    source_stops.contains(&byte),
                    "source={source:?} columns={columns} byte={byte}"
                );
                if let Some(mapped) = expanded.mapped_byte(&layout, byte) {
                    assert!(layout.caret_x(mapped).unwrap().is_finite());
                }
            }
            // Every pointer result is a complete source grapheme even when a
            // replacement tab space combines with a following mark.
            for step in -2..=100 {
                let x = layout.advance * step as f32 / 100.;
                assert!(source_stops.contains(&expanded.hit_test(&atlas, &layout, x).unwrap()));
            }
            assert_eq!(expanded.hit_test(&atlas, &layout, f32::NAN), None);
        }
    }
}

struct Capture(Mutex<fs::File>);
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info && metadata.target().ends_with("::observability")
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            writeln!(
                self.0.lock().unwrap(),
                "{} {}",
                record.level(),
                record.args()
            )
            .unwrap();
            if record.level() == log::Level::Error {
                eprintln!("ERROR {}", record.args());
            }
        }
    }
    fn flush(&self) {}
}

#[test]
fn invalid_expanded_boundary_and_drag_pointer_keep_default_fault_evidence() {
    const CHILD: &str = "MOTION_UI_TEXT_LINE_FAULT_CHILD";
    const NAME: &str = "widgets::text_line::tests::invalid_expanded_boundary_and_drag_pointer_keep_default_fault_evidence";
    if let Some(path) = std::env::var_os(CHILD) {
        let sink = Capture(Mutex::new(fs::File::create(&path).unwrap()));
        log::set_logger(Box::leak(Box::new(sink))).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let source = "\te\u{301}";
        let expanded = ExpandedLine::new(source, 4);
        let layout = atlas.layout_line(&expanded.text).unwrap();
        assert!(expanded.mapped_byte(&layout, 2).is_none());
        let sheet = crate::style::default_stylesheet();
        let style = sheet.resolve("field", Default::default()).unwrap();
        let point = [
            sheet.metric("text.editor.gutter.width", 44.) + 4.,
            style.padding + 2.,
        ];
        let mut editor = TextEditor::default();
        editor.begin_multiline(source, 1024);
        let mut viewport = TextViewport::default();
        let area = Rect::new(0., 0., 300., 80.);
        editor.multiline_view(
            &mut viewport,
            &mut PaintList::default(),
            &mut atlas,
            area,
            &Input {
                pressed: Some((MouseButton::Left, point)),
                held: Some(MouseButton::Left),
                mouse: point,
                ..Default::default()
            },
        );
        for _ in 0..120 {
            assert_eq!(expanded.caret(&atlas, &layout, 2), 0.);
            editor.multiline_view(
                &mut viewport,
                &mut PaintList::default(),
                &mut atlas,
                area,
                &Input {
                    held: Some(MouseButton::Left),
                    mouse: [f32::NAN, point[1]],
                    ..Default::default()
                },
            );
            assert_eq!(editor.text, source);
            assert!(expanded.caret(&atlas, &layout, source.len()).is_finite());
        }
        // A normal pointer after the fault must work without restarting either
        // the atlas or the editor and without replacing its source text.
        editor.multiline_view(
            &mut viewport,
            &mut PaintList::default(),
            &mut atlas,
            area,
            &Input {
                held: Some(MouseButton::Left),
                mouse: [point[0] + 80., point[1]],
                ..Default::default()
            },
        );
        assert_eq!(editor.caret_byte_index(), source.len());
        assert_eq!(editor.text, source);
        let events = fs::read_to_string(path).unwrap();
        assert_eq!(events.lines().count(), 4, "{events}");
        for key in ["expanded_boundary", "editor_pointer"] {
            let marker = format!("site=text.layout key={key:?} ");
            let lines = events
                .lines()
                .filter(|line| line.contains(&marker))
                .collect::<Vec<_>>();
            assert_eq!(lines.len(), 2, "{events}");
            assert!(lines[0].starts_with("WARN ") && lines[0].contains("count=1 "));
            assert!(lines[1].starts_with("ERROR ") && lines[1].contains("count=60 "));
        }
        assert!(events.contains("byte=2 display_bytes=7") && events.contains("x=NaN"));
        return;
    }
    let explicit = std::env::var_os("MOTION_UI_TEXT_LINE_ARTIFACTS");
    let root = explicit
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "motion-editor-mapping-{}-{}",
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
    assert!(!stdout.contains("UI degradation") && !stdout.contains(source_marker()));
    assert!(!stderr.contains("WARN ") && !stderr.contains(source_marker()));
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.starts_with("ERROR ") && line.contains("site=text.layout"))
            .count(),
        2,
        "{stderr}"
    );
    if explicit.is_none() {
        for name in ["events.log", "child-stdout.log", "child-stderr.log"] {
            fs::remove_file(root.join(name)).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }
}

fn source_marker() -> &'static str {
    "\te\u{301}"
}
