use std::{
    thread::JoinHandle,
    time::{Duration, Instant},
};

// This is a functional teardown deadline, not a decode retry or watchdog.
// All workers owned by one reader share it. OS scheduling can overrun it.
const DROP_WAIT: Duration = Duration::from_millis(50);

struct Worker {
    handle: JoinHandle<()>,
    object: Option<u64>,
    source: Option<u64>,
}

pub(super) struct Shutdown {
    owner: &'static str,
    began: Instant,
    workers: Vec<Worker>,
}
impl Shutdown {
    pub(super) fn new(owner: &'static str) -> Self {
        Self {
            owner,
            began: Instant::now(),
            workers: Vec::new(),
        }
    }
    pub(super) fn push(
        &mut self,
        handle: JoinHandle<()>,
        object: Option<u64>,
        source: Option<u64>,
    ) {
        self.workers.push(Worker {
            handle,
            object,
            source,
        });
    }
    pub(super) fn finish(mut self) {
        if self.workers.is_empty() {
            return;
        }
        let owner = self.owner;
        log::info!(
            "video shutdown start owner={owner} workers={} budget_ms={}",
            self.workers.len(),
            DROP_WAIT.as_millis()
        );
        log::logger().flush();
        while !self.workers.is_empty() {
            let mut index = 0;
            while index < self.workers.len() {
                if self.workers[index].handle.is_finished() {
                    let worker = self.workers.swap_remove(index);
                    let thread = worker.handle.thread().id();
                    if let Err(payload) = worker.handle.join() {
                        log::error!("video shutdown join failed owner={owner} object={:?} source={:?} thread={thread:?} phase=join {}ms disposition=joined reason={}", worker.object, worker.source, self.began.elapsed().as_millis(), motion_media::panic_reason(payload));
                    }
                } else {
                    index += 1;
                }
            }
            if self.workers.is_empty() {
                break;
            }
            let Some(remaining) = DROP_WAIT.checked_sub(self.began.elapsed()) else {
                break;
            };
            if remaining.is_zero() {
                break;
            }
            std::thread::sleep(remaining.min(Duration::from_millis(1)));
        }
        for worker in self.workers.drain(..) {
            log::error!("video shutdown timeout owner={owner} object={:?} source={:?} thread={:?} phase=join {}ms budget_ms={} disposition=detached; worker retains its native resources until it returns", worker.object, worker.source, worker.handle.thread().id(), self.began.elapsed().as_millis(), DROP_WAIT.as_millis());
            // Rust cannot safely terminate a native decoder. Stop was already
            // published; this explicitly releases only our join ownership.
            drop(worker.handle);
        }
        log::debug!(
            "video shutdown done owner={owner} {}ms",
            self.began.elapsed().as_millis()
        );
    }
}
