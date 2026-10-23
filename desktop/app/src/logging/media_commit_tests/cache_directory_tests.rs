use super::*;

struct RejectSave {
    project: Project,
    obstruct: bool,
    cache: Option<PathBuf>,
    displaced: Option<PathBuf>,
}

impl motion_model::EditSink for RejectSave {
    fn project(&self) -> &Project {
        &self.project
    }

    fn apply_batch_saved(
        &mut self,
        commands: Vec<motion_core::Command>,
        root: &Path,
    ) -> motion_core::Result<Vec<motion_core::EditResult>> {
        let member = commands
            .iter()
            .find_map(|command| match command {
                motion_core::Command::RegisterAudioAsset { asset } => {
                    Some(motion_media::cache_path(root, asset).unwrap())
                }
                motion_core::Command::RegisterVideoAsset { asset } => {
                    Some(motion_media::video_cache_path(root, asset).unwrap())
                }
                _ => None,
            })
            .expect("import must publish an asset before saving");
        let cache = member.parent().unwrap().to_path_buf();
        assert_eq!(fs::read_dir(&cache).unwrap().count(), 2);
        assert!(fs::metadata(&member).unwrap().len() > 0);
        if self.obstruct {
            let displaced = cache.with_extension("external");
            fs::rename(&cache, &displaced).unwrap();
            fs::write(&cache, b"foreign cache destination").unwrap();
            self.displaced = Some(displaced);
        }
        self.cache = Some(cache);
        Err(motion_core::Error::Invalid(
            "controlled cache model save failure".into(),
        ))
    }
}

#[test]
fn directory_rollback_faults_keep_default_errors_and_explicit_import_recovery() {
    const CHILD: &str = "MOTION_CACHE_DIRECTORY_FILE_TEST_CHILD";
    const TEST: &str = "logging::media_commit_tests::cache_directory_tests::directory_rollback_faults_keep_default_errors_and_explicit_import_recovery";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        for kind in ["audio", "video"] {
            for obstruct in [false, true] {
                let project_root = root.join(format!("{kind}-{obstruct}"));
                fs::create_dir(&project_root).unwrap();
                let mut sink = RejectSave {
                    project: Project::new(64, 64, 30, 90).unwrap(),
                    obstruct,
                    cache: None,
                    displaced: None,
                };
                let before = sink.project.clone();
                let id = format!("commit-observation-{kind}");
                let error = if kind == "audio" {
                    audio(&project_root)
                        .commit(&id, &mut sink)
                        .unwrap()
                        .error
                        .unwrap()
                } else {
                    video(&project_root)
                        .commit(&id, &mut sink)
                        .unwrap()
                        .error
                        .unwrap()
                };
                assert!(
                    error.starts_with("controlled cache model save failure"),
                    "{error}"
                );
                assert_eq!(sink.project, before);
                let cache = sink.cache.unwrap();
                if obstruct {
                    assert!(
                        error.contains("cache rollback failed; recovery directory="),
                        "{error}"
                    );
                    assert_eq!(fs::read(&cache).unwrap(), b"foreign cache destination");
                    assert_eq!(fs::read_dir(sink.displaced.unwrap()).unwrap().count(), 2);
                    let recovery: Vec<_> = fs::read_dir(cache.parent().unwrap())
                        .unwrap()
                        .map(|entry| entry.unwrap().path())
                        .filter(|path| {
                            path.file_name()
                                .unwrap()
                                .to_string_lossy()
                                .starts_with(".cache-publish-")
                        })
                        .collect();
                    assert_eq!(recovery.len(), 1);
                    assert!(recovery[0].join("old-0").is_dir());
                    fs::remove_file(&cache).unwrap();
                } else {
                    assert!(cache.is_dir());
                    assert_eq!(fs::read_dir(&cache).unwrap().count(), 0);
                    assert!(!error.contains("rollback failed"));
                }
                assert_eq!(
                    fs::read_dir(project_root.join("assets")).unwrap().count(),
                    0
                );
                let mut engine = Engine::new(before).unwrap();
                let state = if kind == "audio" {
                    audio(&project_root).commit(&id, &mut engine).unwrap().state
                } else {
                    video(&project_root).commit(&id, &mut engine).unwrap().state
                };
                assert_eq!(state, "succeeded");
                assert_eq!(engine.snapshot().layers.len(), 1);
            }
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    for text in [&file, &stderr] {
        assert_eq!(
            text.lines().filter(|line| line.contains("ERROR")).count(),
            6,
            "{text}"
        );
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("media error request=")
                    && line.contains("controlled cache model save failure"))
                .count(),
            4
        );
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("cache rollback failed;")
                    && !line.contains("media error request="))
                .count(),
            2
        );
        assert!(text.contains("remove new cache") && text.contains("changed file type"));
    }
    assert!(!stderr.contains("media start "));
    assert!(!stdout.contains("media ") && !stdout.contains("cache rollback failed"));
}
