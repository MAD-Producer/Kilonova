use std::{
    fmt::{self, Write},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

// Fixed process-lifetime counter across all shards: healthy IO and clear_poison do not reset it.
static RECOVERIES: AtomicUsize = AtomicUsize::new(0);

/// Declare before the cache shard guard so normal and unwinding drops log unlocked.
#[derive(Default)]
pub(super) struct Events {
    recovery: Option<(usize, usize)>,
    cleanup: Option<String>,
}
impl Events {
    pub(super) fn recovered(&mut self, shard: usize) {
        let previous = RECOVERIES
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            })
            .unwrap_or_else(|n| n);
        if previous == 0 || previous == 59 {
            self.recovery = Some((previous.saturating_add(1), shard));
        }
    }
    pub(super) fn cleanup_failed(&mut self, path: &Path, error: &std::io::Error) {
        self.cleanup = Some(context(path, error));
    }
}
impl Drop for Events {
    fn drop(&mut self) {
        if let Some((count, shard)) = self.recovery {
            let context = format_args!("image proxy cache lock recovered site=image_proxy.lock lifetime_count={count} shard={shard} reason=poisoned_mutex recovery=into_inner clear_poison=true cached_bytes=validated");
            if count == 1 {
                log::warn!("{context}");
            } else {
                log::error!("{context}");
            }
        }
        if let Some(context) = &self.cleanup {
            log::warn!("image proxy temporary cleanup failed: {context}");
        }
    }
}

/// Separate bounds preserve a useful error even when the path is very long.
pub(super) fn context(path: &Path, error: &std::io::Error) -> String {
    format!(
        "path={} error={}",
        bounded(format_args!("{}", path.display())),
        bounded(format_args!("{error}"))
    )
}
fn bounded(args: fmt::Arguments<'_>) -> String {
    struct Bounded(String, usize);
    impl Write for Bounded {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            for ch in text.chars().take(self.1) {
                self.0.push(ch);
                self.1 -= 1;
            }
            Ok(())
        }
    }
    let mut bounded = Bounded(String::new(), 256);
    let _ = bounded.write_fmt(args);
    bounded.0
}
