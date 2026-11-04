use super::*;
use crate::{
    paint::PaintList,
    ui::{self, Align},
    Rect,
};
use std::{
    fs::{self, File},
    io::Write,
    sync::Mutex,
};

fn unsupported_selector(atlas: &TextAtlas) -> char {
    (0xe0100..=0xe01ef)
        .map(|v| char::from_u32(v).unwrap())
        .find(|c| !atlas.supports(*c))
        .expect(
            "installed faces must leave an unsupported supplementary selector for this regression",
        )
}

#[test]
fn unsupported_variation_sequences_do_not_add_fallback_spacing() {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    let selector = unsupported_selector(&atlas);
    let text = format!("A{selector}B");
    assert_eq!(
        atlas.measure(&text),
        atlas.measure("AB"),
        "unsupported selector U+{:04X} changed normal-base width",
        selector as u32
    );
    let entry = atlas.glyph(selector);
    assert_eq!(entry.uv, [0.; 4]);
    assert_eq!(entry.advance, 0.);
    assert_eq!(atlas.measure("A\u{200c}\u{200d}B"), atlas.measure("AB"));
}

#[test]
fn ordinary_base_glyphs_keep_their_positions_around_an_unsupported_selector() {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    let selector = unsupported_selector(&atlas);
    let text = format!("A{selector}B");
    for ratio in [1., 1.5, 2.] {
        atlas.set_pixel_ratio(ratio);
        for size in [12., 18., 24.] {
            for align in [Align::Left, Align::Center, Align::Right] {
                let mut expected = PaintList::default();
                let mut actual = PaintList::default();
                let area = Rect::new(0., 0., 300., 50.);
                let color = expected.styles().color("text");
                ui::text_sized(&mut expected, &mut atlas, area, "AB", color, align, size);
                ui::text_sized(&mut actual, &mut atlas, area, &text, color, align, size);
                assert_eq!(
                    expected.glyphs.len(),
                    2,
                    "the fixture must emit actual base glyphs"
                );
                let geometry = |paint: &PaintList| {
                    paint
                        .glyphs
                        .iter()
                        .map(|g| (g.position, g.size, g.uv, g.color.0))
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    geometry(&actual),
                    geometry(&expected),
                    "selector U+{:04X}, ratio={ratio}, size={size}",
                    selector as u32
                );
            }
        }
    }
}

struct Capture(Mutex<File>);
impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata()) && r.target().ends_with("::observability") {
            let mut file = self.0.lock().unwrap();
            writeln!(file, "{} {}", r.level(), r.args()).unwrap();
            file.flush().unwrap();
            if r.level() == log::Level::Error {
                eprintln!("ERROR {}", r.args());
            }
        }
    }
    fn flush(&self) {}
}

#[test]
fn shaped_draw_failures_keep_bounded_default_level_causes_and_owner_scope() {
    const CHILD: &str = "MOTION_UI_SHAPED_DRAW_CHILD";
    const NAME: &str = "text::format_controls_tests::shaped_draw_failures_keep_bounded_default_level_causes_and_owner_scope";
    if let Some(path) = std::env::var_os(CHILD) {
        log::set_logger(Box::leak(Box::new(Capture(Mutex::new(
            File::create(&path).unwrap(),
        )))))
        .unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        let area = Rect::new(0., 0., 500., 40.);
        let draw = |atlas: &mut TextAtlas, label: &str, size: f32| {
            let mut paint = PaintList::default();
            let color = paint.styles().color("text");
            ui::text_sized(&mut paint, atlas, area, label, color, Align::Left, size);
            paint
        };
        for _ in 0..120 {
            assert!(!draw(&mut atlas, "cafe\u{301} \u{200d}", 12.)
                .glyphs
                .is_empty());
            assert!(draw(&mut atlas, " ", 12.).glyphs.is_empty());
            assert!(draw(&mut atlas, "", 12.).glyphs.is_empty());
        }
        assert!(fs::read_to_string(&path).unwrap().is_empty());
        let oversized = "PRIVATE-LABEL-CONTENT".repeat(shaping::MAX_RUN_BYTES / 20 + 1);
        assert!(oversized.len() > shaping::MAX_RUN_BYTES);
        let missing = '\u{10ffff}';
        assert!(!atlas.supports(missing));
        let actual_glyph = atlas.layout_line("A").unwrap().glyphs[0].clone();
        for i in 0..120 {
            assert_eq!(atlas.measure(&oversized), 0.);
            assert_eq!(atlas.ellipsize(&oversized, 500.), "");
            assert!(draw(&mut atlas, &oversized, 12.).glyphs.is_empty());
            assert!(draw(&mut atlas, "\u{10ffff}", 12.).glyphs.is_empty());
            assert!(draw(&mut atlas, "A", f32::NAN).glyphs.is_empty());
            assert!(atlas.layout_bitmap(&actual_glyph, f32::NAN).is_none());
            if i == 29 {
                // Counters belong to the atlas lifetime, not consecutive
                // frames. Healthy labels and cache clears must not reset them.
                assert!(!draw(&mut atlas, "healthy", 12.).glyphs.is_empty());
                atlas.clear();
            }
        }
        let events = fs::read_to_string(&path).unwrap();
        let keys = [
            ("text.layout", "shape"),
            ("text.layout", "fit"),
            ("text.layout", "draw"),
            ("text.layout", "invalid_size"),
            ("text.shaped_missing", "U+10FFFF"),
            ("text.shaped_raster", "InvalidSize"),
        ];
        for (site, key) in keys {
            let marker = format!("site={site} key={key:?} ");
            let lines: Vec<_> = events.lines().filter(|s| s.contains(&marker)).collect();
            assert_eq!(lines.len(), 2, "{marker}: {events}");
            assert!(lines[0].starts_with("WARN ") && lines[0].contains("count=1 "));
            assert!(lines[1].starts_with("ERROR ") && lines[1].contains("count=60 "));
        }
        assert_eq!(events.lines().count(), 12, "{events}");
        assert!(!events.contains("PRIVATE-LABEL-CONTENT"));
        assert!(events.contains("bytes=") && events.contains("limit=16384"));
        assert!(events.contains("face=Primary") && events.contains("cluster_source=U+10FFFF"));
        assert!(events.contains("size=NaN") && events.contains("pixel_ratio=1"));
        assert!(!draw(&mut atlas, "healthy", 12.).glyphs.is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), events);
        let mut replacement = TextAtlas::from_system_language(12., false).unwrap();
        for _ in 0..60 {
            assert!(draw(&mut replacement, "\u{10ffff}", 12.).glyphs.is_empty());
        }
        let next = fs::read_to_string(&path).unwrap();
        assert_eq!(next.lines().count(), 14, "{next}");
        let owner = |line: &str| {
            line.split_whitespace()
                .find(|s| s.starts_with("owner="))
                .unwrap()
                .to_owned()
        };
        assert_ne!(
            owner(events.lines().next().unwrap()),
            owner(next.lines().last().unwrap())
        );
        return;
    }
    let explicit = std::env::var_os("MOTION_UI_SHAPED_DRAW_ARTIFACTS");
    let root = explicit
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "motion-ui-shaped-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
        });
    fs::create_dir_all(&root).unwrap();
    let path = root.join("events.log");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG")
        .env(CHILD, &path)
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
    assert!(!String::from_utf8_lossy(&output.stdout).contains("UI degradation"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr
            .lines()
            .filter(|s| s.starts_with("ERROR ") && s.contains("UI degradation"))
            .count(),
        7,
        "{stderr}"
    );
    assert!(!stderr.contains("WARN ") && !stderr.contains("PRIVATE-LABEL-CONTENT"));
    if explicit.is_none() {
        for file in ["events.log", "child-stdout.log", "child-stderr.log"] {
            fs::remove_file(root.join(file)).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }
}

#[test]
fn sequence_controls_are_normal_while_real_missing_glyphs_keep_default_level_evidence() {
    const CHILD: &str = "MOTION_UI_SEQUENCE_CONTROL_CHILD";
    const NAME:&str="text::format_controls_tests::sequence_controls_are_normal_while_real_missing_glyphs_keep_default_level_evidence";
    if let Some(path) = std::env::var_os(CHILD) {
        let logger = Capture(Mutex::new(File::create(&path).unwrap()));
        log::set_logger(Box::leak(Box::new(logger))).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        let selector = unsupported_selector(&atlas);
        for _ in 0..120 {
            let _ = atlas.glyph(selector);
            let _ = atlas.glyph('\u{200d}');
        }
        let quiet = fs::read_to_string(&path).unwrap();
        assert!(
            quiet.is_empty(),
            "normal sequence control produced fault evidence: {quiet}"
        );
        let missing = ['\u{10ffff}', '\u{10fffe}', '\u{0378}']
            .into_iter()
            .find(|c| !atlas.supports(*c))
            .unwrap();
        for _ in 0..30 {
            let entry = atlas.glyph(missing);
            assert_eq!(entry.uv, [0.; 4]);
            assert!(entry.advance > 0.);
        }
        for _ in 0..120 {
            let _ = atlas.glyph(selector);
        }
        for _ in 30..120 {
            let _ = atlas.glyph(missing);
        }
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].starts_with("WARN") && lines[0].contains("count=1 "));
        assert!(lines[1].starts_with("ERROR") && lines[1].contains("count=60 "));
        assert!(lines.iter().all(|s| s.contains("site=text.missing_glyph ")
            && s.contains(&format!("U+{:04X}", missing as u32))));
        return;
    }
    let explicit = std::env::var_os("MOTION_UI_SEQUENCE_CONTROL_ARTIFACTS");
    let root = explicit
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "motion-ui-sequence-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
        });
    fs::create_dir_all(&root).unwrap();
    let path = root.join("events.log");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG")
        .env(CHILD, &path)
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
    assert!(!stdout.contains("UI degradation"));
    assert_eq!(
        stderr
            .lines()
            .filter(|s| s.starts_with("ERROR ") && s.contains("site=text.missing_glyph "))
            .count(),
        1,
        "{stderr}"
    );
    if explicit.is_none() {
        for file in ["events.log", "child-stdout.log", "child-stderr.log"] {
            fs::remove_file(root.join(file)).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }
}
