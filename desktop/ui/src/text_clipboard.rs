//! Application-owned plain text, independent of document and node clipboards.
//! No OS access, global singleton, or blocking lock is hidden in widget input.
use std::{
    fmt,
    sync::{Arc, Mutex, TryLockError},
    time::Instant,
};

pub const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Busy,
    Poisoned,
    TooLarge,
    Unavailable,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Busy => "text clipboard is busy",
            Self::Poisoned => "text clipboard state is poisoned",
            Self::TooLarge => "text clipboard or edit exceeds the 16 MiB limit",
            Self::Unavailable => "text clipboard is unavailable",
        })
    }
}
impl std::error::Error for Error {}

#[derive(Default)]
pub struct TextClipboard {
    value: Mutex<Option<Arc<str>>>,
}
impl fmt::Debug for TextClipboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TextClipboard { contents: <private> }")
    }
}
impl TextClipboard {
    pub fn store(&self, text: &str) -> Result<(), Error> {
        self.observe("copy", text.len(), || {
            if text.len() > MAX_BYTES {
                return Err(Error::TooLarge);
            }
            let text: Arc<str> = Arc::from(text);
            let previous = {
                let mut value = self.value.try_lock().map_err(lock_error)?;
                value.replace(text)
            };
            drop(previous);
            Ok(())
        })
    }
    pub fn read(&self) -> Result<Option<Arc<str>>, Error> {
        self.observe("paste", 0, || {
            Ok(self.value.try_lock().map_err(lock_error)?.clone())
        })
    }
    fn observe<T>(
        &self,
        operation: &'static str,
        bytes: usize,
        f: impl FnOnce() -> Result<T, Error>,
    ) -> Result<T, Error> {
        let began = Instant::now();
        log::info!("text clipboard start operation={operation} bytes={bytes}");
        let result = f();
        match &result {
            Ok(_) => log::debug!("text clipboard done operation={operation} elapsed_ms={}", began.elapsed().as_millis()),
            Err(error) => log::error!("text clipboard failed operation={operation} bytes={bytes} elapsed_ms={} reason={error}", began.elapsed().as_millis()),
        }
        result
    }
}
fn lock_error<T>(error: TryLockError<T>) -> Error {
    match error {
        TryLockError::WouldBlock => Error::Busy,
        TryLockError::Poisoned(_) => Error::Poisoned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contention_and_rejected_copy_preserve_the_last_owned_value() {
        let clipboard = TextClipboard::default();
        assert!(clipboard.read().unwrap().is_none());
        let mut source = String::from("独立 👩🏽‍🚀");
        clipboard.store(&source).unwrap();
        source.clear();
        let retained = clipboard.read().unwrap().unwrap();
        let lock = clipboard.value.lock().unwrap();
        assert_eq!(clipboard.store("other"), Err(Error::Busy));
        assert_eq!(clipboard.read(), Err(Error::Busy));
        drop(lock);
        assert_eq!(
            clipboard.store(&"x".repeat(MAX_BYTES + 1)),
            Err(Error::TooLarge)
        );
        assert_eq!(&*clipboard.read().unwrap().unwrap(), "独立 👩🏽‍🚀");
        clipboard.store("new").unwrap();
        assert_eq!(&*retained, "独立 👩🏽‍🚀");
    }
    #[test]
    fn poisoned_cache_reports_an_error_instead_of_recovering_or_deleting() {
        let clipboard = TextClipboard::default();
        clipboard.store("kept").unwrap();
        let _ = std::panic::catch_unwind(|| {
            let _lock = clipboard.value.lock().unwrap();
            panic!("injected clipboard poison");
        });
        assert_eq!(clipboard.store("replacement"), Err(Error::Poisoned));
        assert_eq!(clipboard.read(), Err(Error::Poisoned));
        assert_eq!(
            &**clipboard
                .value
                .lock()
                .unwrap_err()
                .get_ref()
                .as_ref()
                .unwrap(),
            "kept"
        );
    }
}
