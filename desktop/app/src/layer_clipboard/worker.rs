use super::{storage, Frozen, Prepared, Read};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

pub(crate) enum Work {
    Capture { source: Read, cache: PathBuf },
    Prepare { source: Arc<Frozen>, target: Read },
}
pub(crate) enum Output {
    Captured(Arc<Frozen>),
    Prepared(Prepared),
}
struct Progress {
    request: u64,
    phase: AtomicU8,
    began: Instant,
}
pub(crate) struct Job {
    receive: mpsc::Receiver<Result<Output, String>>,
    pub cancel: Arc<AtomicBool>,
    progress: Arc<Progress>,
    finished: bool,
}
impl Drop for Job {
    fn drop(&mut self) {
        if !self.finished {
            self.cancel.store(true, Ordering::Release);
        }
    }
}
fn bounded(message: &str) -> String {
    message.chars().take(1024).collect()
}
impl Job {
    pub(crate) fn start(work: Work) -> Result<Self, String> {
        Self::start_with(work, || {})
    }
    pub(super) fn start_with(
        work: Work,
        before: impl FnOnce() + Send + 'static,
    ) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let request = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| "clipboard worker request space exhausted")?;
        let (operation, stamp, count) = match &work {
            Work::Capture { source, .. } => (
                "capture",
                &source.stamp,
                source.clipboard.as_ref().map_or(0, |c| c.objects.len()),
            ),
            Work::Prepare { source, target } => ("prepare", &target.stamp, source.objects.len()),
        };
        let progress = Arc::new(Progress {
            request,
            phase: AtomicU8::new(0),
            began: Instant::now(),
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let (send, receive) = mpsc::channel();
        let job = Self {
            receive,
            cancel: cancel.clone(),
            progress: progress.clone(),
            finished: false,
        };
        // Publish the job and Info start before entering user-controlled IO.
        log::info!("layer clipboard start request={request} operation={operation} phase=queued generation={} revision={} composition={} objects={count} root={}",
            stamp.generation, stamp.revision, stamp.composition, bounded(&stamp.root.to_string_lossy()));
        std::thread::Builder::new().name("motion-layer-clipboard".into()).spawn(move || {
            progress.phase.store(1, Ordering::Release);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                before();
                match work {
                    Work::Capture { source, cache } => storage::capture(source, &cache, &cancel)
                        .map(|frozen| Output::Captured(Arc::new(frozen))),
                    Work::Prepare { source, target } => {
                        let project = target.project.ok_or("clipboard target metadata missing")?;
                        storage::prepare(&source, target.stamp, &project, &cancel).map(Output::Prepared)
                    }
                }
            })).unwrap_or_else(|payload| {
                let cause = payload.downcast_ref::<String>().map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied()).unwrap_or("non-text panic");
                Err(format!("clipboard worker panicked: {}", bounded(cause)))
            });
            progress.phase.store(2, Ordering::Release);
            let elapsed = progress.began.elapsed().as_millis();
            match &result {
                Err(error) if error == "clipboard operation cancelled" => log::debug!("layer clipboard cancelled request={request} operation={operation} elapsed_ms={elapsed}"),
                Err(error) => log::error!("layer clipboard failed request={request} operation={operation} phase=io elapsed_ms={elapsed} reason={}", bounded(error)),
                Ok(_) => log::debug!("layer clipboard done request={request} operation={operation} elapsed_ms={elapsed}"),
            }
            // The UI may have closed. Dropping an unsent result releases only
            // this request's owned files; no committed transaction is involved.
            let _ = send.send(result);
        }).map_err(|error| {
            log::error!("layer clipboard worker spawn failed request={request} operation={operation} reason={error}");
            error.to_string()
        })?;
        Ok(job)
    }
    pub(crate) fn status(&self) -> (u64, &'static str, Duration) {
        (
            self.progress.request,
            match self.progress.phase.load(Ordering::Acquire) {
                0 => "queued",
                1 => "io",
                _ => "complete",
            },
            self.progress.began.elapsed(),
        )
    }
    pub(crate) fn poll(&mut self) -> Option<Result<Output, String>> {
        match self.receive.try_recv() {
            Ok(result) => {
                self.finished = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.finished = true;
                let (request, phase, elapsed) = self.status();
                log::error!("layer clipboard worker disconnected request={request} phase={phase} elapsed_ms={}", elapsed.as_millis());
                Some(Err("clipboard worker stopped before replying".into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controlled_block_publishes_in_flight_state_and_cancellation_before_io() {
        let root = tempfile::tempdir().unwrap();
        let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            1, "Layer", [1.; 2], [0.; 3], [1.; 4],
        ));
        let source = Read {
            stamp: super::super::Stamp {
                generation: 1,
                root: root.path().into(),
                composition: project.composition_id.clone(),
                revision: 0,
            },
            frame: 0,
            clipboard: Some(motion_core::LayerClipboard::capture(&project, vec![1]).unwrap()),
            project: None,
        };
        let (entered, ready) = mpsc::channel();
        let (release, proceed) = mpsc::channel();
        let mut job = Job::start_with(
            Work::Capture {
                source,
                cache: root.path().join("cache"),
            },
            move || {
                entered.send(()).unwrap();
                proceed.recv().unwrap();
            },
        )
        .unwrap();
        ready.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(job.status().1, "io");
        assert!(job.status().0 > 0 && job.poll().is_none());
        assert!(!root.path().join("cache").exists());
        job.cancel.store(true, Ordering::Release);
        release.send(()).unwrap();
        let began = Instant::now();
        loop {
            if let Some(result) = job.poll() {
                assert!(matches!(result, Err(error) if error == "clipboard operation cancelled"));
                break;
            }
            assert!(began.elapsed() < Duration::from_secs(10));
            std::thread::yield_now();
        }
        assert_eq!(job.status().1, "complete");
        assert!(!root.path().join("cache").exists());
    }
}
