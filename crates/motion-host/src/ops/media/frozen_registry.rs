//! The map protects handles, never a reader operation or resource destructor.
use super::frozen::{Error, Result};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Audio,
    Video,
}
impl Kind {
    pub(super) fn closed(self) -> &'static str {
        match self {
            Self::Audio => "frozen audio stream is closed",
            Self::Video => "frozen video reader closed",
        }
    }
    pub(super) fn full(self) -> &'static str {
        match self {
            Self::Audio => "release frozen audio streams before creating more",
            Self::Video => "at most four frozen video readers; release finished handles",
        }
    }
}
impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Audio => "audio stream",
            Self::Video => "frozen video",
        })
    }
}

#[derive(Clone, Copy)]
pub(super) struct Context {
    pub operation: &'static str,
    pub object: u64,
    pub sequence: u64,
}
#[derive(Default)]
struct Observation {
    waiting: u64,
    active: Option<(Context, Instant)>,
}

// Field order is deliberate: the reader's destructor finishes before the final
// permit is returned, including when release races an operation. Video workers
// retain their separate existing decoder/retirement permits until they exit.
struct Entry<T> {
    value: Mutex<T>,
    observation: Mutex<Observation>,
    _permit: Permit,
}
struct Permit(Arc<AtomicUsize>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Map<T> {
    next: i64,
    entries: HashMap<i64, Arc<Entry<T>>>,
}
pub(super) struct Registry<T> {
    kind: Kind,
    limit: usize,
    occupied: Arc<AtomicUsize>,
    map: Mutex<Map<T>>,
}

#[derive(Debug, serde::Serialize)]
pub struct ReaderInfo {
    pub handle: i64,
    pub waiting: u64,
    pub operation: Option<&'static str>,
    pub object: Option<u64>,
    pub sequence: Option<u64>,
    pub elapsed_ms: Option<u64>,
}
#[derive(Debug, serde::Serialize)]
pub struct RegistryInfo {
    pub kind: Kind,
    pub limit: usize,
    /// Includes constructors and released entries still owned by in-flight calls.
    pub occupied: usize,
    pub readers: Vec<ReaderInfo>,
}

impl<T> Registry<T> {
    pub(super) fn new(kind: Kind, limit: usize) -> Self {
        Self {
            kind,
            limit,
            occupied: Arc::new(AtomicUsize::new(0)),
            map: Mutex::new(Map {
                next: 1,
                entries: HashMap::new(),
            }),
        }
    }
    pub(super) fn create(&self, make: impl FnOnce() -> Result<T>) -> Result<i64> {
        let (handle, permit) = {
            let mut map = self
                .map
                .lock()
                .map_err(|_| Error::RegistryPoisoned(self.kind))?;
            if self.occupied.load(Ordering::Acquire) >= self.limit {
                return Err(Error::Limit(self.kind));
            }
            let handle = map.next;
            map.next = handle
                .checked_add(1)
                .ok_or(Error::HandleExhausted(self.kind))?;
            self.occupied.fetch_add(1, Ordering::AcqRel);
            (handle, Permit(self.occupied.clone()))
        };
        let entry = Arc::new(Entry {
            value: Mutex::new(make()?),
            observation: Mutex::new(Observation::default()),
            _permit: permit,
        });
        let mut map = self
            .map
            .lock()
            .map_err(|_| Error::RegistryPoisoned(self.kind))?;
        map.entries.insert(handle, entry);
        Ok(handle)
    }
    pub(super) fn release(&self, handle: i64) -> Result<bool> {
        let entry = {
            self.map
                .lock()
                .map_err(|_| Error::RegistryPoisoned(self.kind))?
                .entries
                .remove(&handle)
        };
        let removed = entry.is_some();
        drop(entry); // A mixer/decoder destructor must not hold the registry lock.
        Ok(removed)
    }
    pub(super) fn with<R>(
        &self,
        handle: i64,
        context: Context,
        call: impl FnOnce(&mut T) -> Result<R>,
    ) -> Result<R> {
        let entry = {
            self.map
                .lock()
                .map_err(|_| Error::RegistryPoisoned(self.kind))?
                .entries
                .get(&handle)
                .cloned()
                .ok_or(Error::Closed {
                    kind: self.kind,
                    handle,
                })?
        };
        let mut flight = Flight::queued(&entry, self.kind)?;
        {
            let mut value = entry.value.lock().map_err(|_| Error::ReaderPoisoned {
                kind: self.kind,
                handle,
            })?;
            flight.activate(context)?;
            let result = call(&mut *value);
            drop(flight); // Clear this operation before another waiter takes value.
            result
        }
    }
    pub(super) fn info(&self) -> Result<RegistryInfo> {
        let entries: Vec<_> = {
            self.map
                .lock()
                .map_err(|_| Error::RegistryPoisoned(self.kind))?
                .entries
                .iter()
                .map(|(id, reader)| (*id, reader.clone()))
                .collect()
        };
        let mut readers = Vec::with_capacity(entries.len());
        for (handle, entry) in entries {
            let state = entry
                .observation
                .lock()
                .map_err(|_| Error::ObservationPoisoned(self.kind))?;
            readers.push(ReaderInfo {
                handle,
                waiting: state.waiting,
                operation: state.active.map(|x| x.0.operation),
                object: state.active.map(|x| x.0.object),
                sequence: state.active.map(|x| x.0.sequence),
                elapsed_ms: state
                    .active
                    .map(|x| x.1.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
            });
        }
        readers.sort_unstable_by_key(|r| r.handle);
        Ok(RegistryInfo {
            kind: self.kind,
            limit: self.limit,
            occupied: self.occupied.load(Ordering::Acquire),
            readers,
        })
    }
}

struct Flight<'a> {
    observation: &'a Mutex<Observation>,
    kind: Kind,
    active: bool,
}
impl<'a> Flight<'a> {
    fn queued<T>(entry: &'a Entry<T>, kind: Kind) -> Result<Self> {
        let mut state = entry
            .observation
            .lock()
            .map_err(|_| Error::ObservationPoisoned(kind))?;
        state.waiting = state.waiting.saturating_add(1);
        Ok(Self {
            observation: &entry.observation,
            kind,
            active: false,
        })
    }
    fn activate(&mut self, context: Context) -> Result<()> {
        let mut state = self
            .observation
            .lock()
            .map_err(|_| Error::ObservationPoisoned(self.kind))?;
        state.waiting = state.waiting.saturating_sub(1);
        state.active = Some((context, Instant::now()));
        self.active = true;
        Ok(())
    }
}
impl Drop for Flight<'_> {
    fn drop(&mut self) {
        // Cleanup records no success and never clears poison; the next operation
        // still returns the typed fault. No logger runs under observation locks.
        let mut state = self
            .observation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.active {
            state.active = None;
        } else {
            state.waiting = state.waiting.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests;
