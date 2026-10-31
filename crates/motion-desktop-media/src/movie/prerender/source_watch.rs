//! Native, bounded source invalidation. Only background preparation arms watches;
//! validity checks never touch the filesystem. An observation belongs to exactly
//! one frozen source closure, and cannot cancel a replacement closure's work.
use super::ExportControl;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

type Result<T> = std::result::Result<T, String>;
const MAX_SOURCES: usize = 4096;
const MAX_DIRECTORIES: usize = 128;
const MAX_PATH_BYTES: usize = 1 << 20;
const MAX_CAUSE_CHARS: usize = 512;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub request: u64,
    pub valid: bool,
    pub retired: bool,
    pub fault: bool,
    pub active_ticket: Option<u64>,
    /// First change or watcher fault only; bounded for the closure's lifetime.
    pub cause: Option<String>,
    /// First terminal watcher fault, separately bounded from a normal change.
    pub fault_cause: Option<String>,
}
struct State {
    retired: bool,
    fault: bool,
    next_ticket: u64,
    active: Option<(u64, ExportControl)>,
    cause: Option<String>,
    fault_cause: Option<String>,
}
struct Shared {
    request: u64,
    valid: AtomicBool,
    state: Mutex<State>,
}

#[derive(Clone)]
pub struct Observation(Arc<Shared>);
impl Observation {
    fn new(request: u64) -> Self {
        Self(Arc::new(Shared {
            request,
            valid: AtomicBool::new(true),
            state: Mutex::new(State {
                retired: false,
                fault: false,
                next_ticket: 0,
                active: None,
                cause: None,
                fault_cause: None,
            }),
        }))
    }
    /// The native event callback publishes invalidity before cancelling work.
    pub fn valid(&self) -> bool {
        self.0.valid.load(Ordering::Acquire)
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        let state = self
            .0
            .state
            .lock()
            .map_err(|_| "source watch state poisoned")?;
        Ok(Snapshot {
            request: self.0.request,
            valid: self.valid(),
            retired: state.retired,
            fault: state.fault,
            active_ticket: state.active.as_ref().map(|a| a.0),
            cause: state.cause.clone(),
            fault_cause: state.fault_cause.clone(),
        })
    }
    /// Attach before native work. Invalid/retired closures are ordinary stale
    /// inputs; they cannot accept another job. Busy is a protocol violation.
    pub fn attach(&self, control: ExportControl) -> Result<Option<Lease>> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| "source watch state poisoned")?;
        if !self.valid() || state.retired {
            return Ok(None);
        }
        if state.active.is_some() {
            return Err("source watch already owns an unacknowledged job".into());
        }
        let ticket = state
            .next_ticket
            .checked_add(1)
            .ok_or("source watch ticket exhausted")?;
        state.next_ticket = ticket;
        state.active = Some((ticket, control));
        Ok(Some(Lease {
            observation: self.clone(),
            ticket,
        }))
    }
    fn finish(&self, ticket: u64) -> Result<()> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| "source watch state poisoned")?;
        if state.active.as_ref().map(|a| a.0) != Some(ticket) {
            return Err("source watch acknowledgment ticket mismatch".into());
        }
        state.active = None;
        Ok(())
    }
    fn invalidate(&self, cause: String, fault: bool) {
        let (cause, control, actual_fault) = {
            // Poison is itself terminal: recover only to cancel, never to reuse.
            let (mut state, poisoned) = match self.0.state.lock() {
                Ok(state) => (state, false),
                Err(error) => (error.into_inner(), true),
            };
            if state.retired {
                return;
            }
            let first = self.0.valid.swap(false, Ordering::AcqRel);
            let actual_fault = fault || poisoned;
            // A normal change closes lookup, but is not the explicit recovery
            // boundary for a later backend fault. Keep one cause of each kind;
            // repeated events and callbacks from retired closures stay quiet.
            if !first && (!actual_fault || state.fault) {
                return;
            }
            let cause = if poisoned {
                "source watch state poisoned".into()
            } else {
                cause
            };
            let cause: String = cause.chars().take(MAX_CAUSE_CHARS).collect();
            if first {
                state.cause = Some(cause.clone());
            }
            if actual_fault {
                state.fault = true;
                state.fault_cause = Some(cause.clone());
            }
            (cause, state.active.as_ref().map(|a| a.1.clone()), actual_fault)
        };
        // Never call cancellation or logging with the observation mutex held.
        if let Some(control) = control {
            control.cancel();
        }
        if actual_fault {
            log::error!(
                "prerender source watch request={} phase=event cause={cause}",
                self.0.request
            );
        } else {
            log::info!(
                "prerender source changed request={} cause={cause}",
                self.0.request
            );
        }
    }
    fn retire(&self) {
        let control = {
            let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
            self.0.valid.store(false, Ordering::Release);
            state.retired = true;
            state.active.as_ref().map(|a| a.1.clone())
        };
        if let Some(control) = control {
            control.cancel();
        }
    }
}
/// Keep this lease until the real operation acknowledges completion, including
/// cancellation. Dropping it releases only its matching job slot.
pub struct Lease {
    observation: Observation,
    ticket: u64,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Err(error) = self.observation.finish(self.ticket) {
            self.observation.invalidate(error, true);
        }
    }
}

// Watch source parents, but filter to known entries. Atomic replacement and
// deletion remain visible without trusting a handle, mtime or length, while
// sibling logs/snapshots cannot continually invalidate the closure. Ancestors
// observe the next path entry, including project-root replacement.
#[derive(Default)]
struct Scope {
    entries: Vec<PathBuf>,
}
struct Scopes(BTreeMap<PathBuf, Scope>);
impl Scopes {
    fn new(root: &Path, sources: &[PathBuf]) -> Result<Self> {
        if sources.len() > MAX_SOURCES {
            return Err(format!(
                "source watch source limit exceeded: {} > {MAX_SOURCES}",
                sources.len()
            ));
        }
        let root = root
            .canonicalize()
            .map_err(|e| format!("source watch project root: {e}"))?;
        let mut scopes = Self(BTreeMap::new());
        for relative in sources {
            if relative.as_os_str().len() > MAX_PATH_BYTES {
                return Err("source watch path budget exceeded".into());
            }
            if relative.as_os_str().is_empty()
                || relative
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
            {
                return Err("source watch requires nonempty project-relative paths".into());
            }
            let alias = root.join(relative);
            let source = alias.canonicalize().map_err(|e| {
                format!(
                    "source watch source {}: {e}",
                    relative
                        .to_string_lossy()
                        .chars()
                        .take(192)
                        .collect::<String>()
                )
            })?;
            if !source.starts_with(&root) {
                return Err("source watch source resolves outside project".into());
            }
            // Cover both the source target and any in-project symlink route.
            for path in [&alias, &source] {
                let parent = path.parent().ok_or("source watch source has no parent")?;
                let parent = parent
                    .canonicalize()
                    .map_err(|e| format!("source watch parent: {e}"))?;
                if !parent.starts_with(&root) {
                    return Err("source watch parent resolves outside project".into());
                }
                let mut child = path.to_path_buf();
                while let Some(parent) = child.parent() {
                    if child == root {
                        scopes.add_entry(parent, &child)?;
                        break;
                    }
                    if !parent.starts_with(&root) {
                        break;
                    }
                    scopes.add_entry(parent, &child)?;
                    child = parent.to_path_buf();
                }
            }
            scopes.check_budget()?;
        }
        Ok(scopes)
    }
    fn add_entry(&mut self, parent: &Path, child: &Path) -> Result<()> {
        let parent = parent
            .canonicalize()
            .map_err(|e| format!("source watch ancestor: {e}"))?;
        let name = child
            .file_name()
            .ok_or("source watch ancestor has no name")?;
        let entry = parent.join(name);
        let scope = self.0.entry(parent).or_default();
        if !scope.entries.contains(&entry) {
            scope.entries.push(entry);
        }
        self.check_budget()
    }
    fn check_budget(&self) -> Result<()> {
        let bytes = self
            .0
            .iter()
            .try_fold(0usize, |n, (p, s)| {
                s.entries
                    .iter()
                    .chain(std::iter::once(p))
                    .try_fold(n, |n, p| n.checked_add(p.as_os_str().len()))
            })
            .ok_or("source watch path budget overflow")?;
        if self.0.len() > MAX_DIRECTORIES || bytes > MAX_PATH_BYTES {
            return Err("source watch directory/path budget exceeded".into());
        }
        Ok(())
    }
    fn relevant(&self, event: &Event) -> bool {
        // Backend overflow/rescan loses path information and must fail closed,
        // even when accompanied by Access or an unrelated path.
        if event.need_rescan() {
            return true;
        }
        if matches!(event.kind, EventKind::Access(_))
            && !matches!(
                event.kind,
                EventKind::Access(
                    notify::event::AccessKind::Open(notify::event::AccessMode::Write)
                        | notify::event::AccessKind::Close(notify::event::AccessMode::Write)
                )
            )
        {
            return false;
        }
        if event.paths.is_empty() || event.paths.len() > 16 {
            return true;
        }
        event.paths.iter().any(|path| {
            self.0.iter().any(|(parent, scope)| {
                path_matches(path, parent)
                    || scope.entries.iter().any(|entry| path_matches(path, entry))
            })
        })
    }
}
fn path_matches(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn CompareStringOrdinal(
                a: *const u16,
                a_len: i32,
                b: *const u16,
                b_len: i32,
                ignore_case: i32,
            ) -> i32;
        }
        let normalize = |p: &Path| {
            let mut units: Vec<_> = p
                .as_os_str()
                .encode_wide()
                .map(|u| if u == 47 { 92 } else { u })
                .collect();
            if units.starts_with(&[92, 92, 63, 92]) {
                units.drain(..4);
                if units.starts_with(&[85, 78, 67, 92]) {
                    units.splice(..4, [92, 92]);
                }
            }
            units
        };
        let a = normalize(a);
        let b = normalize(b);
        let (Ok(a_len), Ok(b_len)) = (i32::try_from(a.len()), i32::try_from(b.len())) else {
            return true;
        };
        // SAFETY: both UTF-16 buffers remain alive, lengths fit i32 and delimit
        // their allocated slices. The API uses the OS ordinal uppercase table,
        // including non-ASCII Windows paths, with no filesystem access.
        let comparison = unsafe { CompareStringOrdinal(a.as_ptr(), a_len, b.as_ptr(), b_len, 1) };
        comparison == 0 || comparison == 2 // API error fails closed; CSTR_EQUAL=2.
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

pub struct SourceWatch {
    observation: Observation,
    // Dropped after retire, outside every observation lock.
    _watcher: Option<RecommendedWatcher>,
}
type EventCallback = Box<dyn FnMut(notify::Result<Event>) + Send>;
impl SourceWatch {
    /// Background-only IO. Arm before opening/copying source bytes; retain the
    /// watcher for the entire frozen owner/cache lifetime. No recursive scans.
    pub fn arm(request: u64, root: &Path, sources: &[PathBuf]) -> Result<Self> {
        Self::arm_with_backend(request, root, sources, notify::recommended_watcher)
    }
    fn arm_with_backend(
        request: u64,
        root: &Path,
        sources: &[PathBuf],
        create: impl FnOnce(EventCallback) -> notify::Result<RecommendedWatcher>,
    ) -> Result<Self> {
        let began = Instant::now();
        log::info!(
            "start #{request} prerender_source_watch sources={}",
            sources.len()
        );
        log::logger().flush();
        let result: Result<Self> = (|| {
            let scopes = Scopes::new(root, sources)?;
            let identities: Vec<_> = sources
                .iter()
                .map(|p| {
                    let path = root.join(p);
                    path.canonicalize()
                        .map(|target| (path, target))
                        .map_err(|e| format!("source watch identity: {e}"))
                })
                .collect::<Result<_>>()?;
            let observation = Observation::new(request);
            let callback = observation.clone();
            let directories: Vec<_> = scopes.0.keys().cloned().collect();
            // Procedural-only closures still own validity and cancellation leases,
            // but have no filesystem entries to observe. Avoid an idle OS backend
            // and its event thread on every captured generation.
            let watcher = if directories.is_empty() {
                None
            } else {
                let mut watcher =
                    create(Box::new(move |event: notify::Result<Event>| match event {
                        Ok(event) if scopes.relevant(&event) => {
                            let path = event
                                .paths
                                .first()
                                .map(|p| p.to_string_lossy().chars().take(192).collect::<String>());
                            callback.invalidate(
                                format!(
                                    "kind={:?} rescan={} path={path:?}",
                                    event.kind,
                                    event.need_rescan()
                                ),
                                false,
                            );
                        }
                        Ok(_) => {}
                        Err(error) => callback.invalidate(error.to_string(), true),
                    }))
                    .map_err(|e| format!("create native source watcher: {e}"))?;
                for directory in directories {
                    watcher
                        .watch(&directory, RecursiveMode::NonRecursive)
                        .map_err(|e| format!("arm native source watcher: {e}"))?;
                }
                Some(watcher)
            };
            for (path, target) in identities {
                if path
                    .canonicalize()
                    .map_err(|e| format!("source watch revalidate: {e}"))?
                    != target
                {
                    return Err("source watch target changed while arming".into());
                }
            }
            Ok(Self {
                observation,
                _watcher: watcher,
            })
        })();
        if let Err(error) = &result {
            log::error!(
                "prerender source watch request={request} phase=arm elapsed_ms={} cause={}",
                began.elapsed().as_millis(),
                error.chars().take(MAX_CAUSE_CHARS).collect::<String>()
            );
        }
        result
    }
    pub fn observation(&self) -> Observation {
        self.observation.clone()
    }
}
impl Drop for SourceWatch {
    fn drop(&mut self) {
        self.observation.retire();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) fn fail_test_observation(observation: &Observation, cause: &str) {
    observation.invalidate(cause.into(), true);
}

#[cfg(test)]
pub(super) fn change_test_observation(observation: &Observation, cause: &str) {
    observation.invalidate(cause.into(), false);
}

// The Windows MoveFileEx operation used by NamedTempFile::persist cannot
// replace this fixture's still-open destination. Use the documented native
// replacement operation, retaining the old reader throughout the assertion.
#[cfg(test)]
pub(super) fn replace_test_source(replacement: tempfile::NamedTempFile, destination: &Path) {
    #[cfg(not(windows))]
    replacement.persist(destination).unwrap();
    #[cfg(windows)]
    {
        use std::{ffi::c_void, os::windows::ffi::OsStrExt};
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn ReplaceFileW(
                replaced: *const u16,
                replacement: *const u16,
                backup: *const u16,
                flags: u32,
                exclude: *mut c_void,
                reserved: *mut c_void,
            ) -> i32;
        }
        let (file, path) = replacement.into_parts();
        drop(file); // ReplaceFileW opens the replacement exclusively.
        let wide = |p: &Path| {
            let mut units: Vec<_> = p.as_os_str().encode_wide().collect();
            assert!(!units.contains(&0));
            units.push(0);
            units
        };
        let source = wide(&path);
        let destination = wide(destination);
        // SAFETY: terminated UTF-16 buffers live through the call. Optional
        // backup and reserved pointers are null; only owned fixtures are used.
        // https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew
        let replaced = unsafe {
            ReplaceFileW(
                destination.as_ptr(),
                source.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_ne!(
            replaced,
            0,
            "native replacement: {}",
            std::io::Error::last_os_error()
        );
    }
}
