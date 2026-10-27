use super::model::*;
use motion_desktop_media::movie::durable::PreparedMovie;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;
const MAX_DOCUMENT: u64 = 2 * 1024 * 1024;

/// Owned by the queue worker, independent of the live editor document/undo.
pub(crate) struct Store {
    directory: PathBuf,
    pub document: Document,
}
impl Store {
    pub fn open(project_root: &Path) -> Result<Self> {
        let project_root = project_root
            .canonicalize()
            .map_err(|e| format!("resolve render queue project: {e}"))?;
        let directory = project_root.join(".motion-render-queue");
        reject_link(&directory)?;
        fs::create_dir_all(&directory).map_err(|e| format!("create render queue: {e}"))?;
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        if directory.parent() != Some(project_root.as_path()) {
            return Err("render queue storage resolves outside project".into());
        }
        reject_link(&directory.join("snapshots"))?;
        fs::create_dir_all(directory.join("snapshots"))
            .map_err(|e| format!("create render snapshots: {e}"))?;
        let path = directory.join("queue.json");
        let mut document = match fs::File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_DOCUMENT + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| format!("read render queue: {e}"))?;
                if bytes.len() as u64 > MAX_DOCUMENT {
                    return Err("render queue document exceeds 2 MiB".into());
                }
                serde_json::from_slice::<Document>(&bytes)
                    .map_err(|e| format!("parse render queue: {e}"))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Document::default(),
            Err(e) => return Err(format!("open render queue: {e}")),
        };
        document.validate()?;
        let mut interrupted = 0;
        for job in &mut document.jobs {
            if job.state.active() {
                job.state = State::Interrupted;
                interrupted += 1;
            }
        }
        let store = Self {
            directory,
            document,
        };
        if interrupted > 0 {
            // An interrupted task is never silently resumed or retried.
            store.write(&store.document)?;
            log::info!(
                "render queue recovered interrupted tasks count={interrupted} path={}",
                crate::fallback_observation::short(path.display())
            );
        }
        Ok(store)
    }
    pub fn snapshots(&self) -> PathBuf {
        self.directory.join("snapshots")
    }
    fn checked_storage(&self) -> Result<PathBuf> {
        reject_link(&self.directory)?;
        if self.directory.canonicalize().map_err(|e| e.to_string())? != self.directory {
            return Err("render queue storage was replaced or relocated".into());
        }
        let snapshots = self.snapshots();
        reject_link(&snapshots)?;
        if snapshots.canonicalize().map_err(|e| e.to_string())? != snapshots {
            return Err("render snapshot storage was replaced or relocated".into());
        }
        Ok(snapshots)
    }
    pub fn snapshot_path(&self, job: &Job) -> Result<PathBuf> {
        let root = self.checked_storage()?;
        let expected = root.join(&job.snapshot);
        reject_link(&expected)?;
        let path = expected
            .canonicalize()
            .map_err(|e| format!("resolve queued input job={}: {e}", job.id))?;
        if path != expected || !path.is_dir() {
            return Err("queued input is outside its snapshot directory".into());
        }
        Ok(path)
    }
    fn write(&self, document: &Document) -> Result<()> {
        self.checked_storage()?;
        document.validate()?;
        let bytes = serde_json::to_vec_pretty(document).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_DOCUMENT {
            return Err("render queue document exceeds 2 MiB".into());
        }
        crate::workspace::write_atomic(&self.directory.join("queue.json"), &bytes)
    }
    fn change<T>(&mut self, operation: impl FnOnce(&mut Document) -> Result<T>) -> Result<T> {
        let mut next = self.document.clone();
        let value = operation(&mut next)?;
        self.write(&next)?;
        self.document = next;
        Ok(value)
    }
    pub fn save_template(&mut self, mut template: Template) -> Result<u64> {
        self.change(|document| {
            if template.id == 0 {
                template.id = document.next_template;
                document.next_template = document
                    .next_template
                    .checked_add(1)
                    .ok_or("render template ID exhausted")?;
                let id = template.id;
                document.templates.push(template);
                Ok(id)
            } else {
                let id = template.id;
                let existing = document
                    .templates
                    .iter_mut()
                    .find(|t| t.id == id)
                    .ok_or("render template does not exist")?;
                *existing = template;
                Ok(id)
            }
        })
    }
    pub fn remove_template(&mut self, id: u64) -> Result<()> {
        self.change(|document| {
            let index = document
                .templates
                .iter()
                .position(|t| t.id == id)
                .ok_or("render template does not exist")?;
            document.templates.remove(index);
            Ok(())
        })
    }
    pub fn add(
        &mut self,
        prepared: PreparedMovie,
        template: Template,
        output: PathBuf,
    ) -> Result<u64> {
        // Settings are captured by the enqueue intent, independently of later
        // edits/deletion in the template library while source freezing runs.
        template.validate()?;
        if template.id >= self.document.next_template {
            return Err("render template identity was never allocated".into());
        }
        let output = self.new_output(&output)?;
        let parent = self.checked_storage()?;
        let input = prepared.path().canonicalize().map_err(|e| e.to_string())?;
        if input.parent() != Some(parent.as_path()) {
            return Err("prepared movie belongs to another queue".into());
        }
        let snapshot = input
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid snapshot directory name")?
            .to_owned();
        let source = prepared.info().clone();
        let id = self.document.next_id;
        // Publish inputs first. A crash may leave an unreferenced private input,
        // but must never publish a task that references a deleted preparation.
        let owned = prepared.publish()?;
        let result = self.change(|document| {
            document.next_id = document
                .next_id
                .checked_add(1)
                .ok_or("render task ID exhausted")?;
            document.jobs.push(Job {
                id,
                snapshot,
                source,
                template,
                output,
                attempt: 0,
                state: State::Queued,
            });
            Ok(id)
        });
        if result.is_err() {
            // The exclusive preparation above is owned by this failed request.
            let cleanup = (|| {
                let root = self.checked_storage()?;
                reject_link(&owned)?;
                let target = owned.canonicalize().map_err(|e| e.to_string())?;
                if target != owned || target.parent() != Some(root.as_path()) {
                    return Err("unpublished task cleanup resolves outside its owned input".into());
                }
                fs::remove_dir_all(&target).map_err(|e| e.to_string())
            })();
            if let Err(error) = cleanup {
                log::error!(
                    "render queue unpublished input cleanup failed job={id} path={} cause={error}",
                    crate::fallback_observation::short(owned.display())
                );
            }
        }
        result
    }
    fn new_output(&self, output: &Path) -> Result<PathBuf> {
        validate_output(output)?;
        let parent = output
            .parent()
            .ok_or("render output directory is missing")?
            .canonicalize()
            .map_err(|e| format!("resolve render output directory: {e}"))?;
        let output = parent.join(
            output
                .file_name()
                .ok_or("render output filename is missing")?,
        );
        self.checked_storage()?;
        if output.starts_with(&self.directory) {
            return Err("render output cannot be inside private queue storage".into());
        }
        match output.try_exists() {
            Ok(false) => Ok(output),
            Ok(true) => Err(format!(
                "render output already exists: {}",
                output.display()
            )),
            Err(e) => Err(format!("inspect render output: {e}")),
        }
    }
    pub fn begin_next(&mut self) -> Result<Option<Job>> {
        if self.document.jobs.iter().any(|j| j.state.active()) {
            return Err("render queue already has an active task".into());
        }
        let Some(id) = self
            .document
            .jobs
            .iter()
            .find(|j| j.state == State::Queued)
            .map(|j| j.id)
        else {
            return Ok(None);
        };
        self.change(|document| {
            let job = job_mut(document, id)?;
            job.attempt = job
                .attempt
                .checked_add(1)
                .ok_or("render attempt count exhausted")?;
            job.state = State::Running;
            Ok(Some(job.clone()))
        })
    }
    pub fn cancel(&mut self, id: u64) -> Result<bool> {
        let state = &self
            .document
            .jobs
            .iter()
            .find(|j| j.id == id)
            .ok_or("render task does not exist")?
            .state;
        if !matches!(state, State::Queued | State::Running) {
            return Ok(false);
        }
        self.change(|document| {
            let job = job_mut(document, id)?;
            job.state = if job.state == State::Queued {
                State::Cancelled
            } else {
                State::Cancelling
            };
            Ok(true)
        })
    }
    pub fn finish(&mut self, id: u64, result: State) -> Result<()> {
        if !matches!(
            result,
            State::Succeeded { .. } | State::Failed { .. } | State::Cancelled
        ) {
            return Err("invalid terminal render result".into());
        }
        self.change(|document| {
            let job = job_mut(document, id)?;
            if !job.state.active() {
                return Err("render task is not active".into());
            }
            job.state = result;
            Ok(())
        })
    }
    pub fn retry(&mut self, id: u64, output: Option<PathBuf>) -> Result<()> {
        let job = self
            .document
            .jobs
            .iter()
            .find(|j| j.id == id)
            .ok_or("render task does not exist")?;
        if !job.state.retryable() {
            return Err("render task cannot be retried in this state".into());
        }
        let output = self.new_output(output.as_deref().unwrap_or(&job.output))?;
        self.change(|document| {
            let job = job_mut(document, id)?;
            job.output = output;
            job.state = State::Queued;
            Ok(())
        })
    }
    pub fn remove(&mut self, id: u64) -> Result<()> {
        let job = self
            .document
            .jobs
            .iter()
            .find(|j| j.id == id)
            .ok_or("render task does not exist")?;
        if job.state.active() {
            return Err("cancel the active render task before removing it".into());
        }
        let input = self.snapshots().join(&job.snapshot);
        self.change(|document| {
            job_mut(document, id)?.state = State::Removing;
            Ok(())
        })?;
        // Guard the final resolved target before any recursive removal. Never
        // follow a replaced directory outside this queue's snapshot root.
        let root = self.checked_storage()?;
        reject_link(&input)?;
        match input.canonicalize() {
            Ok(path)
                if path
                    == root.join(
                        input
                            .file_name()
                            .ok_or("task input has no directory name")?,
                    ) =>
            {
                fs::remove_dir_all(path).map_err(|e| format!("remove task input job={id}: {e}"))?
            }
            Ok(_) => return Err("task input removal resolves outside queue storage".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("resolve task input removal job={id}: {e}")),
        }
        self.change(|document| {
            document.jobs.retain(|j| j.id != id);
            Ok(())
        })
    }
}
fn reject_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err("render queue storage cannot be a symbolic link".into())
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("inspect render queue storage: {e}")),
    }
}
fn job_mut(document: &mut Document, id: u64) -> Result<&mut Job> {
    document
        .jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| "render task does not exist".into())
}

#[cfg(test)]
mod tests;
