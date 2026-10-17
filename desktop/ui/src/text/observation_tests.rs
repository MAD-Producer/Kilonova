use super::*;
use std::{
    fs::{self, File},
    io::Write,
    sync::Mutex,
};

struct Capture(Mutex<File>);
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) && record.target().ends_with("::observability") {
            let mut file = self.0.lock().unwrap();
            writeln!(file, "{} {}", record.level(), record.args()).unwrap();
            file.flush().unwrap();
        }
    }
    fn flush(&self) {}
}

#[test]
fn real_style_and_glyph_fallbacks_leave_bounded_default_level_evidence() {
    const CHILD: &str = "MOTION_UI_FALLBACK_TEST_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        log::set_logger(Box::leak(Box::new(Capture(Mutex::new(
            File::create(&path).unwrap(),
        )))))
        .unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let sheet =
            crate::style::StyleSheet::from_json(include_bytes!("../../styles/dark.json")).unwrap();
        assert!(sheet.metric("row.height", 0.) > 0.);
        let _ = sheet.color("panel");
        let mut atlas = TextAtlas::from_system_language(14., false).unwrap();
        assert!(atlas.glyph('W').uv[2] > 0.);
        assert!(atlas.pixel_data().iter().any(|byte| *byte != 0));
        for _ in 0..120 {
            let _ = atlas.glyph(' ');
        }
        assert!(fs::read_to_string(&path).unwrap().is_empty());
        let cloned = sheet.clone();
        for i in 0..120 {
            let source = if i % 2 == 0 { &sheet } else { &cloned };
            assert_eq!(source.metric("test-missing-metric", 17.), 17.);
            assert_eq!(source.color("test-missing-color").0, [0.; 4]);
        }
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 4, "{text}");
        assert_eq!(
            text.lines()
                .filter(|line| line.starts_with("ERROR"))
                .count(),
            2
        );
        assert!(text.contains("site=style.metric key=\"test-missing-metric\" count=60"));
        assert!(text.contains("fallback=17") && text.contains("fallback=transparent"));
        let replacement =
            crate::style::StyleSheet::from_json(include_bytes!("../../styles/dark.json")).unwrap();
        assert_eq!(replacement.metric("test-missing-metric", 19.), 19.);
        let replaced = fs::read_to_string(&path).unwrap();
        assert_eq!(replaced.lines().count(), 5); // Replacement owns a fresh episode.
        let missing = ['\u{10ffff}', '\u{10fffe}', '\u{0378}']
            .into_iter()
            .find(|c| !atlas.supports(*c))
            .unwrap();
        for _ in 0..120 {
            let entry = atlas.glyph(missing);
            assert_eq!(entry.uv, [0.; 4]);
            assert!(entry.advance > 0.);
        }
        let missing_log = fs::read_to_string(&path).unwrap();
        let glyph_events: Vec<_> = missing_log
            .lines()
            .filter(|line| line.contains("site=text.missing_glyph"))
            .collect();
        assert_eq!(glyph_events.len(), 2, "{missing_log}");
        assert!(glyph_events[1].starts_with("ERROR") && glyph_events[1].contains("count=60"));
        assert!(glyph_events[1].contains(&format!("U+{:04X}", missing as u32)));
        // A real, small atlas fills repeatedly while rasterizing installed-font
        // glyphs. Keep the existing clear/repack policy and expose its pressure.
        let w = atlas.glyph('W');
        let width = ((w.uv[2] - w.uv[0]) * atlas.config.width as f32).round() as u32;
        let height = ((w.uv[3] - w.uv[1]) * atlas.config.height as f32).round() as u32;
        assert!(width > 0 && height > 0);
        atlas.config.width = (width + 2) * 2;
        atlas.config.height = height + 2;
        atlas.pixels = vec![0; (atlas.config.width * atlas.config.height * 4) as usize];
        atlas.clear();
        for i in 1..=240 {
            let _ = atlas.glyph_at('W', 14. + i as f32 / 1_000_000.);
        }
        let pressure = fs::read_to_string(&path).unwrap();
        let pressure_events: Vec<_> = pressure
            .lines()
            .filter(|line| line.contains("site=text.atlas_full"))
            .collect();
        assert_eq!(pressure_events.len(), 2, "{pressure}");
        assert!(pressure_events[1].starts_with("ERROR") && pressure_events[1].contains("count=60"));
        // Untrusted lookup names cannot create an unbounded observation map or
        // one error record per distinct name after the observation capacity.
        let limited = crate::observability::Failures::default();
        for i in 0..1000 {
            limited.record(
                "test.capacity",
                &format!("key-{i}"),
                format_args!("fallback=unchanged"),
            );
        }
        let all = fs::read_to_string(&path).unwrap();
        assert_eq!(
            all.lines()
                .filter(|line| line.contains("site=test.capacity"))
                .count(),
            130
        );
        assert!(all.contains("overflow=true"));
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "motion-ui-observability-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let path = root.join("events.log");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","text::observation_tests::real_style_and_glyph_fallbacks_leave_bounded_default_level_evidence"])
        .env(CHILD,&path).output().unwrap();
    fs::remove_file(&path).unwrap();
    fs::remove_dir(&root).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("UI degradation"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("UI degradation"));
}
