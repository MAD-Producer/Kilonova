use super::*;
use std::{fs, io::Write, sync::Mutex};

#[test]
fn scrub_presentation_matches_owned_reference_for_magnitudes_and_special_values() {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    let color = crate::style::default_stylesheet().color("text");
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        atlas.set_pixel_ratio(ratio);
        for value in [
            0.,
            -0.,
            0.00001,
            -0.00001,
            0.5,
            -0.5,
            1.125,
            -12.25,
            999.99,
            1000.,
            1080.,
            -1000.,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ] {
            for width in [32., 180.] {
                let area = Rect::new(1.25, 4.5, width, 24.);
                let mut actual = PaintList::default();
                let mut reference = PaintList::default();
                scrub_text(&mut actual, &mut atlas, area, value, color, Align::Right);
                let size = atlas.font_size();
                crate::ui::text_clipped(
                    &mut reference,
                    &mut atlas,
                    area,
                    &crate::ui::format_value(value),
                    color,
                    Align::Right,
                    size,
                );
                let glyphs = |list: &PaintList| {
                    list.glyphs
                        .iter()
                        .map(|g| (g.position, g.size, g.uv, g.color.0))
                        .collect::<Vec<_>>()
                };
                assert!(!reference.glyphs.is_empty());
                assert_eq!(
                    glyphs(&actual),
                    glyphs(&reference),
                    "value={value} ratio={ratio} width={width}"
                );
            }
        }
    }
}

struct Failed;
impl fmt::Display for Failed {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("private formatting probe")?;
        Err(fmt::Error)
    }
}
struct Sink(Mutex<fs::File>);
impl log::Log for Sink {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) && record.target().ends_with("::observability") {
            let text = format!("{} {}", record.level(), record.args());
            let mut file = self.0.lock().unwrap();
            writeln!(file, "{text}").unwrap();
            file.flush().unwrap();
            if record.level() == log::Level::Error {
                writeln!(std::io::stderr().lock(), "{text}").unwrap();
            }
        }
    }
    fn flush(&self) {}
}

#[test]
fn formatting_failures_keep_default_level_evidence_and_no_partial_draws() {
    const TEST: &str = "ui::numeric_readout::tests::formatting_failures_keep_default_level_evidence_and_no_partial_draws";
    const CHILD: &str = "MOTION_NUMERIC_FORMAT_LOG";
    let Ok(path) = std::env::var(CHILD) else {
        let root = std::env::temp_dir().join(format!(
            "numeric-readout-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("info.log");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, &path)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let file = fs::read_to_string(&path).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(file.matches("operation=numeric_readout").count(), 4);
        assert_eq!(file.matches("WARN ").count(), 2);
        assert_eq!(file.matches("ERROR ").count(), 2);
        assert_eq!(stderr.matches("operation=numeric_readout").count(), 2);
        assert!(!stderr.contains("WARN "));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("operation=numeric_readout"));
        assert!(!file.contains("private formatting probe"));
        assert!(file.contains("phase=format cause="));
        assert!(file.contains("count=60 "));
        if let Some(directory) = std::env::var_os("MOTION_UI_NUMERIC_READOUT_ARTIFACTS") {
            let directory = std::path::Path::new(&directory);
            fs::create_dir_all(directory).unwrap();
            fs::write(directory.join("numeric-format-info.log"), &file).unwrap();
            fs::write(directory.join("numeric-format.stdout"), &output.stdout).unwrap();
            fs::write(directory.join("numeric-format.stderr"), &output.stderr).unwrap();
        }
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&root).unwrap();
        return;
    };
    log::set_logger(Box::leak(Box::new(Sink(Mutex::new(
        fs::File::create(&path).unwrap(),
    )))))
    .unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    let color = crate::style::default_stylesheet().color("text");
    for generation in 0..2 {
        if generation == 1 {
            atlas = TextAtlas::from_system_language(12., false).unwrap();
        }
        for _ in 0..59 {
            draw_formatted(&mut atlas, format_args!("{Failed}"), |_, _| {
                panic!("partial draw")
            });
        }
        let before = fs::read_to_string(&path).unwrap();
        assert_eq!(before.matches("ERROR ").count(), generation);
        draw_formatted(&mut atlas, format_args!("{Failed}"), |_, _| {
            panic!("partial draw")
        });
        let mut list = PaintList::default();
        draw_formatted(&mut atlas, format_args!("12.50"), |atlas, text| {
            assert_eq!(text, "12.50");
            crate::ui::text(
                &mut list,
                atlas,
                Rect::new(0., 0., 100., 24.),
                text,
                color,
                Align::Left,
            );
        });
        assert!(!list.glyphs.is_empty(), "healthy drawing must recover");
        for _ in 0..120 {
            draw_formatted(&mut atlas, format_args!("{Failed}"), |_, _| {
                panic!("partial draw")
            });
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap().matches("ERROR ").count(),
            generation + 1,
            "atlas-lifetime suppression must survive healthy draws"
        );
    }
}
