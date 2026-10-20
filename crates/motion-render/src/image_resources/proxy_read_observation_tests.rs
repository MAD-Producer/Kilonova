use super::*;
use std::{io::Write, sync::Mutex};

struct Capture(Mutex<File>);
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata())
            && record.args().to_string().contains("site=image_proxy.read ")
        {
            assert!(
                CACHE_LOCKS.iter().all(|cache| cache.try_lock().is_ok()),
                "logging held a cache shard lock"
            );
            let mut file = self.0.lock().unwrap();
            writeln!(file, "{} {}", record.level(), record.args()).unwrap();
            file.flush().unwrap();
        }
    }
    fn flush(&self) {}
}

#[test]
fn corrupt_proxy_reads_have_bounded_evidence_and_preserve_real_decode_fallback() {
    const CHILD: &str = "MOTION_PROXY_READ_TEST_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let log = root.join("events.log");
        log::set_logger(Box::leak(Box::new(Capture(Mutex::new(
            File::create(&log).unwrap(),
        )))))
        .unwrap();
        log::set_max_level(if std::env::var_os("MOTION_PROXY_READ_DEBUG").is_some() {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        });
        let missing = root.join("missing.rgba");
        for _ in 0..120 {
            assert!(read_proxy(&missing, 2, 1).is_none());
        }
        assert!(fs::read_to_string(&log).unwrap().is_empty());
        let path = root.join(format!("{:064x}.rgba", 1));
        let pixels = Pixels {
            width: 2,
            height: 1,
            rgba: vec![80; 8],
            cached: false,
        };
        write_proxy(&path, &pixels);
        let valid = fs::read(&path).unwrap();
        let mut cases = vec![];
        cases.push(("proxy length mismatch", valid[..valid.len() - 1].to_vec()));
        let mut header = valid.clone();
        header[0] ^= 1;
        cases.push(("proxy header or dimensions mismatch", header));
        let mut dimensions = valid.clone();
        dimensions[4] = 3;
        cases.push(("proxy header or dimensions mismatch", dimensions));
        let mut checksum = valid.clone();
        checksum[12] ^= 1;
        cases.push(("proxy checksum mismatch", checksum.clone()));
        for (index, (cause, corrupt)) in cases.iter().enumerate() {
            fs::write(&path, corrupt).unwrap();
            for _ in 0..120 {
                assert!(read_proxy(&path, 2, 1).is_none());
            }
            let text = fs::read_to_string(&log).unwrap();
            let errors: Vec<_> = text
                .lines()
                .filter(|line| line.starts_with("ERROR"))
                .collect();
            assert_eq!(errors.len(), index + 1, "{text}");
            assert!(errors.last().unwrap().contains(cause), "{text}");
            assert!(errors.last().unwrap().contains("width=2 height=1"));
            assert!(errors
                .last()
                .unwrap()
                .contains(&format!("path={}", path.display())));
            fs::write(&path, &valid).unwrap();
            assert_eq!(read_proxy(&path, 2, 1).unwrap(), pixels.rgba);
        }
        // Directory reads cause a real IO or invalid-length failure on the host OS.
        let directory = root.join("directory.rgba");
        fs::create_dir(&directory).unwrap();
        for _ in 0..120 {
            assert!(read_proxy(&directory, 2, 1).is_none());
        }
        assert_eq!(
            fs::read_to_string(&log)
                .unwrap()
                .lines()
                .filter(|line| line.starts_with("ERROR"))
                .count(),
            5
        );
        assert_eq!(read_proxy(&path, 2, 1).unwrap(), pixels.rgba);
        fs::write(&path, &checksum).unwrap();
        for _ in 0..59 {
            assert!(read_proxy(&path, 2, 1).is_none());
        }
        assert!(read_proxy(&missing, 2, 1).is_none()); // A normal miss breaks the failure streak.
        for _ in 0..59 {
            assert!(read_proxy(&path, 2, 1).is_none());
        }
        assert_eq!(
            fs::read_to_string(&log)
                .unwrap()
                .lines()
                .filter(|line| line.starts_with("ERROR"))
                .count(),
            5
        );
        assert!(read_proxy(&path, 2, 1).is_none());
        let text = fs::read_to_string(&log).unwrap();
        assert_eq!(
            text.lines()
                .filter(|line| line.starts_with("ERROR"))
                .count(),
            6,
            "{text}"
        );
        assert!(text
            .lines()
            .filter(|line| line.starts_with("ERROR"))
            .all(|line| line.contains("count=60")));
        assert_eq!(
            text.contains("DEBUG degradation"),
            log::max_level() >= log::LevelFilter::Debug
        );

        // Exercise the public decoder, not just the cache reader: corruption
        // still falls back to source pixels and repairs the same proxy file.
        fs::create_dir(root.join("assets")).unwrap();
        let source_path = root.join("assets/source.png");
        image::RgbaImage::from_pixel(4, 2, image::Rgba([80, 120, 160, 255]))
            .save(&source_path)
            .unwrap();
        let asset = Asset {
            id: 9,
            path: "assets/source.png".into(),
            width: 4,
            height: 2,
        };
        let source = Source::new(&root, &asset).unwrap();
        let first = decode(&source, Resolution::Preview(2)).unwrap();
        assert!(!first.cached);
        let proxy = proxy_path(&source, 2, 1, None).unwrap();
        let mut damaged = fs::read(&proxy).unwrap();
        damaged[12] ^= 1;
        fs::write(&proxy, damaged).unwrap();
        let fallback = decode(&source, Resolution::Preview(2)).unwrap();
        assert!(!fallback.cached);
        assert_eq!(fallback.rgba, first.rgba);
        let repaired = decode(&source, Resolution::Preview(2)).unwrap();
        assert!(repaired.cached);
        assert_eq!(repaired.rgba, first.rgba);
        return;
    }
    for debug in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child.args(["--exact", "image_resources::proxy_read_observation_tests::corrupt_proxy_reads_have_bounded_evidence_and_preserve_real_decode_fallback"])
            .env(CHILD, root.path()).env_remove("MOTION_PROXY_READ_DEBUG");
        if debug {
            child.env("MOTION_PROXY_READ_DEBUG", "1");
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("site=image_proxy.read"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("site=image_proxy.read"));
    }
}
