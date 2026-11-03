//! A ready-task identity for host verification outside session/task locks.
//! This token is not a filesystem lease and does not itself prove source bytes.
use super::*;

#[derive(Clone)]
pub struct VideoReplacementCandidate {
    task: Arc<Mutex<Task>>,
    stage: PathBuf,
    relative_source: PathBuf,
    asset: VideoAsset,
}
impl VideoReplacementCandidate {
    pub fn source_path(&self) -> PathBuf {
        self.stage.join("source.mp4")
    }
    pub fn asset(&self) -> &VideoAsset {
        &self.asset
    }
    /// Relative to the owning jobs root, avoiding platform path-spelling guesses.
    pub fn relative_source_path(&self) -> &Path {
        &self.relative_source
    }
    pub(super) fn matches(&self, task: &Arc<Mutex<Task>>, state: &Task) -> bool {
        Arc::ptr_eq(task, &self.task)
            && state.status.state == "ready"
            && state
                .prepared
                .as_ref()
                .is_some_and(|p| p.stage.0 == self.stage && p.probe.asset == self.asset)
    }
}
impl VideoJobs {
    /// Capture metadata only. The caller performs all source IO after unlocking.
    pub fn replacement_candidate(&self, id: &str) -> Result<VideoReplacementCandidate> {
        let task = self.task(id)?;
        let state = task.lock().map_err(|_| "video task poisoned")?;
        if state.status.state != "ready" {
            return Err("video replacement is not ready".into());
        }
        let prepared = state.prepared.as_ref().ok_or("prepared video missing")?;
        let candidate = VideoReplacementCandidate {
            task: task.clone(),
            stage: prepared.stage.0.clone(),
            relative_source: prepared
                .stage
                .0
                .strip_prefix(&self.root)
                .map_err(|_| "prepared video outside job root")?
                .join("source.mp4"),
            asset: prepared.probe.asset.clone(),
        };
        Ok(candidate)
    }
    /// Check the exact task/staging identity while holding the publication guard.
    /// A release/recreation of the same request string cannot reuse an old proof.
    pub fn replace_candidate(
        &self,
        id: &str,
        engine: &mut impl EditSink,
        asset: u64,
        candidate: &VideoReplacementCandidate,
    ) -> Result<VideoTaskStatus> {
        self.commit_target(id, engine, ImportTarget::Replace(asset), Some(candidate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(jobs: &VideoJobs, stage: &Path) {
        fs::create_dir(stage).unwrap();
        fs::write(stage.join("source.mp4"), [17; 64]).unwrap();
        let task = super::super::completion_tests::task();
        {
            let mut state = task.lock().unwrap();
            state.status.request_id = "replace".into();
            state.status.state = "ready".into();
            state.prepared = Some(super::super::completion_tests::prepared(
                stage.canonicalize().unwrap(),
            ));
        }
        jobs.tasks.lock().unwrap().insert("replace".into(), task);
    }

    #[test]
    fn recreated_request_cannot_publish_with_an_old_candidate() {
        let root = tempfile::tempdir().unwrap();
        let jobs = VideoJobs::new(root.path().into()).unwrap();
        ready(&jobs, &root.path().join("first"));
        let old = jobs.replacement_candidate("replace").unwrap();
        jobs.cancel("replace").unwrap();
        jobs.release("replace").unwrap();
        ready(&jobs, &root.path().join("second"));
        let mut project = motion_core::Project::new(8, 8, 1, 1).unwrap();
        project.video_assets.push(old.asset().clone());
        let mut engine = motion_core::Engine::new(project).unwrap();
        let before = engine.snapshot();
        let cause = jobs
            .replace_candidate("replace", &mut engine, 1, &old)
            .err()
            .expect("a recreated request must reject its predecessor's candidate");
        assert!(
            cause.to_string().contains("candidate was superseded"),
            "{cause}"
        );
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.revision(), 0);
        assert_eq!(jobs.status("replace").unwrap().state, "ready");
        assert!(root.path().join("second/source.mp4").is_file());
        let current = jobs.replacement_candidate("replace").unwrap();
        let task = jobs.task("replace").unwrap();
        assert!(current.matches(&task, &task.lock().unwrap()));
    }

    #[test]
    fn changed_stream_metadata_invalidates_a_ready_candidate() {
        let root = tempfile::tempdir().unwrap();
        let jobs = VideoJobs::new(root.path().into()).unwrap();
        ready(&jobs, &root.path().join("staged"));
        let candidate = jobs.replacement_candidate("replace").unwrap();
        let task = jobs.task("replace").unwrap();
        let mut state = task.lock().unwrap();
        state.prepared.as_mut().unwrap().probe.asset.track = 1;
        assert!(!candidate.matches(&task, &state));
        state.prepared.as_mut().unwrap().probe.asset.track = 0;
        assert!(candidate.matches(&task, &state));
        state.status.state = "cancelled".into();
        assert!(!candidate.matches(&task, &state));
    }
}
