use super::*;
use std::io::Write;

struct Capture(Mutex<std::fs::File>);
impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let text = format!("{} {}", record.level(), record.args());
        let mut file = self.0.lock().unwrap();
        writeln!(file, "{text}").unwrap();
        file.flush().unwrap();
        drop(file);
        if record.level() == log::Level::Error {
            eprintln!("{text}");
        }
    }
    fn flush(&self) {
        self.0.lock().unwrap().flush().unwrap();
    }
}

#[test]
fn warmup_faults_keep_current_pixels_default_logs_typed_causes_and_recovery() {
    const CHILD: &str = "MOTION_VIDEO_WARMUP_FAULT_CHILD";
    if let Some(directory) = std::env::var_os(CHILD) {
        let directory = PathBuf::from(directory);
        let sink = Box::leak(Box::new(Capture(Mutex::new(
            std::fs::File::create(directory.join("events.log")).unwrap(),
        ))));
        log::set_logger(sink).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let root = directory.join("project");
        let mut p = fixture(&root);
        p.layers[0].time_mapping = Some(remap(&[(0., 2.)]));
        let control = Arc::new(Control::default());
        let mut reader = VideoFrames::new(Arc::new(Backend(control)));
        let current = scene(&p, 2.);
        ready(&mut reader, &p, &root, &current);
        super::upcoming::entering(&mut p, 8, 4);
        p.layers[1].name = "PRIVATE USER LAYER NAME MUST NOT BE LOGGED".into();
        p.layers[1].time_mapping = None;
        if let Content::Video { video } = &mut p.layers[1].content {
            video.asset = 999;
        }
        let snapshot = serde_json::to_value(&p).unwrap();
        for _ in 0..120 {
            let result = reader
                .prepare_scene(&p, &root, &current, 2.)
                .unwrap()
                .unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].1.left.pts, 170_000);
        }
        assert_eq!(serde_json::to_value(&p).unwrap(), snapshot);
        if let Content::Video { video } = &mut p.layers[1].content {
            video.asset = 1;
        }
        reader.prepare_scene(&p, &root, &current, 2.).unwrap();
        if let Content::Video { video } = &mut p.layers[1].content {
            video.asset = 999;
        }
        for _ in 0..60 {
            assert!(reader
                .prepare_scene(&p, &root, &current, 2.)
                .unwrap()
                .is_some());
        }
        reader.finish().unwrap();

        // Metadata failure occurs before a decoder exists: retain its typed
        // original cause and avoid retrying even after the index reappears.
        if let Content::Video { video } = &mut p.layers[1].content {
            video.asset = 1;
        }
        p.layers.truncate(1);
        let control = Arc::new(Control::default());
        let mut reader = VideoFrames::new(Arc::new(Backend(control)));
        let current = scene(&p, 2.);
        ready(&mut reader, &p, &root, &current);
        let cache = motion_media::video_cache_path(&root, &p.video_assets[0]).unwrap();
        let cache_bytes = std::fs::read(&cache).unwrap();
        std::fs::remove_file(&cache).unwrap();
        super::upcoming::entering(&mut p, 8, 4);
        p.layers[1].time_mapping = None;
        reader.prepare_scene(&p, &root, &current, 2.).unwrap();
        assert_eq!(reader.warmup_errors.len(), 1);
        let original = reader.warmup_errors[&8].error.clone();
        assert!(matches!(original.primary(), MediaError::Message(_)));
        assert!(original
            .to_string()
            .contains("video cache missing; call prepare_video"));
        std::fs::write(&cache, cache_bytes).unwrap();
        for _ in 0..120 {
            assert!(reader
                .prepare_scene(&p, &root, &current, 2.)
                .unwrap()
                .is_some());
        }
        assert!(
            !reader.streams.contains_key(&8),
            "failed speculative preparation must not reopen automatically"
        );
        let visible = scene(&p, 4.);
        let error = reader
            .prepare_scene(&p, &root, &visible, 4.)
            .err()
            .expect("mandatory entry must expose original failure");
        assert_eq!(error.to_string(), original.to_string());
        assert!(matches!(error.primary(), MediaError::Message(_)));
        reader.clear();
        reader.finish().unwrap();
        assert!(reader.warmup_errors.is_empty());
        ready(&mut reader, &p, &root, &visible);
        reader.finish().unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_VIDEO_WARMUP_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","video_frames::mapped_prefetch::tests::upcoming_fault::warmup_faults_keep_current_pixels_default_logs_typed_causes_and_recovery","--nocapture"])
        .env_remove("MOTION_LOG").env(CHILD,&root).output().unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("events.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let plan: Vec<_> = file
        .lines()
        .filter(|l| l.contains("site=video_warmup_plan"))
        .collect();
    assert_eq!(plan.len(), 2);
    assert!(plan.iter().all(|l| l.starts_with("ERROR ")
        && l.contains("count=60")
        && l.contains("999")
        && l.contains("object=8")));
    assert_eq!(
        file.lines()
            .filter(|l| l.contains("ERROR video warmup failed"))
            .count(),
        1
    );
    assert_eq!(
        stderr.lines().filter(|l| l.starts_with("ERROR ")).count(),
        3
    );
    assert!(!stderr.contains("INFO "));
    assert!(!stderr.contains("DEBUG "));
    assert!(!stdout.contains("video warmup"));
    assert!(!stdout.contains("video_warmup_plan"));
    assert!(!file.contains("PRIVATE USER LAYER NAME"));
    assert!(!stderr.contains("PRIVATE USER LAYER NAME"));
    let failed = file.find("ERROR video warmup failed").unwrap();
    let prepare = file[..failed]
        .rfind("INFO video warmup start object=8 source=1")
        .unwrap();
    assert!(prepare < failed);
}
