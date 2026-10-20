use super::*;
use std::{fs, path::Path, process::Command, thread, time::Duration};

fn isolated(name: &str, debug: bool, body: impl FnOnce(&Path)) {
    let variable = format!("MOTION_TEST_ANALYSIS_{name}");
    if let Some(root) = std::env::var_os(&variable) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        body(&root);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            &format!("shell::audio_analysis::worker::tests::{name}"),
        ])
        .env(&variable, root.path())
        .env_remove("MOTION_LOG");
    if debug {
        command.env("MOTION_LOG", "debug");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("audio analysis "));
    let stderr = String::from_utf8_lossy(&output.stderr);
    if debug {
        assert!(!stderr.contains("ERROR"), "{stderr}");
    } else {
        assert!(
            stderr.contains("audio analysis failed request=analysis-observation-error"),
            "{stderr}"
        );
        assert!(
            stderr.contains(
                "audio analysis worker exited unexpectedly request=analysis-observation-panic"
            ),
            "{stderr}"
        );
    }
}

fn wait_for(root: &Path, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        crate::logging::flush();
        let text = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        if text.contains(needle) {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "missing analysis observation: {needle}\n{text}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn default_start_and_unobserved_analysis_failures_keep_request_context() {
    isolated(
        "default_start_and_unobserved_analysis_failures_keep_request_context",
        false,
        |root| {
            let asset = motion_core::AudioAsset {
                id: 77,
                path: "assets/test.wav".into(),
                mime: "audio/wav".into(),
                bytes: 4,
                track: 0,
                sample_rate: 48_000,
                channels: 1,
                sample_frames: 48_000,
                duration_us: 1_000_000,
            };
            let wave = motion_media::cache_path(root, &asset)
                .unwrap()
                .with_extension("wave");
            fs::create_dir_all(wave.parent().unwrap()).unwrap();
            fs::write(&wave, b"bad!").unwrap();
            let expected =
                motion_media::audio_references(root, &asset, 0., 1., &mut |_| Ok(())).unwrap_err().to_string();
            assert_eq!(expected, "waveform cache invalid; call prepare_audio");
            let (entered, entry) = mpsc::channel();
            let (release, resume) = mpsc::channel();
            let release = Release(release);
            let task_root = root.to_owned();
            let receive = spawn(
                "analysis-observation-error".into(),
                88,
                asset.id,
                move || {
                    entered.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(15)).unwrap();
                    motion_media::audio_references(&task_root, &asset, 0., 1., &mut |_| Ok(()))
                        .map(|_| vec![]).map_err(Into::into)
                },
            )
            .unwrap();
            entry.recv_timeout(Duration::from_secs(5)).unwrap();
            // Read directly while work is blocked: the worker must flush its start.
            let before = fs::read_to_string(root.join("motion-studio.log")).unwrap();
            assert!(before.contains("audio analysis start request=analysis-observation-error operation=audio_reference_analysis object=88 source=77"), "{before}");
            assert!(!before.contains("audio analysis failed"));
            drop(receive);
            drop(release);
            let failed = wait_for(
                root,
                "audio analysis failed request=analysis-observation-error",
            );
            let line = failed
                .lines()
                .find(|line| line.contains("audio analysis failed"))
                .unwrap();
            assert!(
                line.contains("ERROR")
                    && line.contains("operation=audio_reference_analysis object=88 source=77"),
                "{line}"
            );
            assert!(
                line.contains("ms reason=") && line.contains(&expected),
                "{line}"
            );
            assert_eq!(fs::read(&wave).unwrap(), b"bad!");

            let (entered, entry) = mpsc::channel();
            let (release, resume) = mpsc::channel();
            let release = Release(release);
            let receive = spawn("analysis-observation-panic".into(), 89, 78, move || {
                entered.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(15)).unwrap();
                panic!("controlled analysis native panic");
            })
            .unwrap();
            entry.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(release);
            // Preserve the original unwinding/disconnected contract rather than
            // converting the panic into a different business result.
            assert_eq!(
                receive.recv_timeout(Duration::from_secs(10)),
                Err(mpsc::RecvTimeoutError::Disconnected)
            );
            let text = wait_for(
                root,
                "audio analysis worker exited unexpectedly request=analysis-observation-panic",
            );
            let line = text
                .lines()
                .find(|line| line.contains("worker exited unexpectedly"))
                .unwrap();
            assert!(
                line.contains("operation=audio_reference_analysis object=89 source=78"),
                "{line}"
            );
            assert!(line.contains("ms reason=unwind"), "{line}");
            assert!(text.contains("controlled analysis native panic"));
            assert!(text
                .contains("default_start_and_unobserved_analysis_failures_keep_request_context"));
            assert!(!text.contains("audio analysis done"));
        },
    );
}

#[test]
fn normal_cancellation_and_success_only_emit_debug_completion() {
    isolated(
        "normal_cancellation_and_success_only_emit_debug_completion",
        true,
        |root| {
            let cancelled = spawn("analysis-normal-cancel".into(), 90, 79, || {
                Err("cancelled".into())
            })
            .unwrap();
            assert_eq!(
                cancelled.recv_timeout(Duration::from_secs(5)).unwrap(),
                Err("cancelled".into())
            );
            let completed =
                spawn("analysis-normal-complete".into(), 91, 80, || Ok(vec![4, 8])).unwrap();
            assert_eq!(
                completed.recv_timeout(Duration::from_secs(5)).unwrap(),
                Ok(vec![4, 8])
            );
            wait_for(root, "phase=cancelled");
            let text = wait_for(root, "phase=complete markers=Some(2)");
            assert_eq!(
                text.lines()
                    .filter(|line| line.contains("audio analysis start"))
                    .count(),
                2
            );
            let completions: Vec<_> = text
                .lines()
                .filter(|line| line.contains("audio analysis done"))
                .collect();
            assert_eq!(completions.len(), 2);
            assert!(
                completions.iter().all(|line| line.contains("DEBUG")),
                "{text}"
            );
            assert!(!text.contains("ERROR"), "{text}");
        },
    );
}
