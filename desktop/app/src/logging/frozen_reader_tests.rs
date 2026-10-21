use super::*;
use motion_core::{AudioAsset, AudioClip, Content, Layer, Project};
use motion_host::ops::media::frozen;
use std::{
    process::{Command, Stdio},
    sync::Arc,
    time::Instant,
};

#[test]
fn missing_frozen_pcm_keeps_native_cause_and_reaches_default_log_sinks() {
    const CHILD: &str = "MOTION_FROZEN_READER_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("assets")).unwrap();
        fs::write(project_root.join("assets/source.wav"), b"registered source").unwrap();
        let mut project = Project::new(32, 32, 30, 30).unwrap();
        project.version = 21;
        project.audio_assets.push(AudioAsset {
            id: 7,
            path: "assets/source.wav".into(),
            bytes: 17,
            mime: "audio/wav".into(),
            track: 0,
            sample_rate: 48000,
            channels: 2,
            sample_frames: 48000,
            duration_us: 1_000_000,
        });
        let mut layer = Layer::solid(4, "sound", [0.; 2], [0.; 3], [1.; 4]);
        layer.content = Content::Audio {
            audio: AudioClip::new(7),
        };
        project.layers.push(layer);
        let session = motion_host::session::open(
            project_root.clone(),
            project.clone(),
            Arc::new(motion_desktop_media::DesktopPlatform::new()),
        )
        .unwrap();
        let failure = frozen::freeze_audio(session).unwrap_err();
        let frozen::Error::Media(cause) = failure else {
            panic!("media error required")
        };
        let motion_media::Error::Io(native) = cause.primary() else {
            panic!("I/O identity lost")
        };
        assert_eq!(native.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(frozen::audio_info().unwrap().occupied, 0);
        let cache = motion_media::cache_path(&project_root, &project.audio_assets[0]).unwrap();
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(cache, vec![0; 48000 * 8]).unwrap();
        let good = frozen::freeze_audio(session).unwrap();
        frozen::release_frozen_audio(good["handle"].as_i64().unwrap()).unwrap();
        motion_host::session::close(session);
        flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout = root.path().join("stdout.txt");
    let stderr = root.path().join("stderr.txt");
    let mut child=Command::new(std::env::current_exe().unwrap())
        .args(["--exact","logging::frozen_reader_tests::missing_frozen_pcm_keeps_native_cause_and_reaches_default_log_sinks","--nocapture"])
        .env(CHILD,root.path()).env_remove("MOTION_LOG")
        .stdout(Stdio::from(File::create(&stdout).unwrap())).stderr(Stdio::from(File::create(&stderr).unwrap())).spawn().unwrap();
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(45) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("frozen-reader logging child did not finish");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = fs::read_to_string(stdout).unwrap();
    let console = fs::read_to_string(stderr).unwrap();
    assert!(status.success(), "{output}\n{console}");
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    for (text, count) in [(&file, 1), (&console, 1), (&output, 0)] {
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("frozen reader failed site=host.frozen.create"))
                .count(),
            count,
            "{text}"
        );
    }
    assert_eq!(
        file.lines()
            .filter(|line| line.contains("frozen reader start site=host.frozen.create"))
            .count(),
        2
    );
    assert!(!console.contains("frozen reader start"));
    assert!(!output.contains("frozen reader start"));
    assert!(
        file.contains("asset=7")
            && file.contains("audio cache unavailable")
            && file.contains("kind=NotFound")
    );
    assert!(!file.contains("frozen reader done"));
}
