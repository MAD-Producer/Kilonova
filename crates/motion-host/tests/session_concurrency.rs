use motion_core::Project;
use motion_host::{session, Platform, Result, SurfaceTarget, VideoDecoder, VideoQuery};
use motion_media::Result as MediaResult;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

const DEADLINE: Duration = Duration::from_secs(5);
const SETUP_DEADLINE: Duration = Duration::from_secs(30);
const WRONG_THREAD: &str = "native session must run on its owning worker thread";

struct Stub {
    _teardown: Option<Teardown>,
}

struct Teardown {
    entered: mpsc::SyncSender<thread::ThreadId>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl Drop for Teardown {
    fn drop(&mut self) {
        self.entered.send(thread::current().id()).unwrap();
        self.release
            .get_mut()
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap();
    }
}

fn open(root: &Path, teardown: Option<Teardown>) -> i64 {
    session::open(
        root.into(),
        Project::new(64, 64, 30, 120).unwrap(),
        Arc::new(Stub {
            _teardown: teardown,
        }),
    )
    .unwrap()
}

#[test]
fn blocked_handler_does_not_block_other_sessions_or_wrong_thread_rejection() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, resume) = mpsc::sync_channel(1);
    let owner = thread::spawn(move || {
        let id = open(first.path(), None);
        session::with_session(id, |session| {
            session.frame = 17.;
            entered.send(id).unwrap();
            resume.recv_timeout(Duration::from_secs(30)).unwrap();
            Ok(())
        })
        .unwrap();
        // A foreign close while the handler was blocked must remain a no-op.
        let frame = session::with_session(id, |session| Ok(session.frame));
        session::close(id);
        frame
    });
    let id = waiting.recv_timeout(SETUP_DEADLINE).unwrap();
    let (done, completed) = mpsc::sync_channel(1);
    let other = thread::spawn(move || {
        let wrong = session::with_session(id, |_| Ok(())).unwrap_err();
        session::close(id);
        let other = open(second.path(), None);
        let operation = session::with_session(other, |session| {
            session.frame = 23.;
            Ok(session.frame)
        });
        session::close(other);
        done.send((wrong, operation)).unwrap();
    });
    let while_blocked = completed.recv_timeout(DEADLINE);
    // Always release and join before asserting, including against the old lock.
    release.send(()).unwrap();
    let frame = owner.join().unwrap();
    other.join().unwrap();
    let (wrong, operation) = while_blocked.expect("another session waited for the blocked handler");
    assert_eq!(wrong.to_string(), WRONG_THREAD);
    assert_eq!(operation.unwrap(), 23.);
    assert_eq!(frame.unwrap(), 17.);
    assert_eq!(
        session::with_session(id, |_| Ok(())).unwrap_err().to_string(),
        "native session is closed"
    );
}

#[test]
fn blocked_owner_teardown_does_not_hold_the_registry() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, resume) = mpsc::sync_channel(1);
    let owner = thread::spawn(move || {
        let id = open(
            first.path(),
            Some(Teardown {
                entered,
                release: Mutex::new(resume),
            }),
        );
        session::close(id);
        thread::current().id()
    });
    // Session owns the final platform references; this is its real destructor.
    let dropping_thread = waiting.recv_timeout(SETUP_DEADLINE).unwrap();
    let (done, completed) = mpsc::sync_channel(1);
    let other = thread::spawn(move || {
        let id = open(second.path(), None);
        let operation = session::with_session(id, |session| Ok(session.frame));
        session::close(id);
        done.send(operation).unwrap();
    });
    let while_dropping = completed.recv_timeout(DEADLINE);
    release.send(()).unwrap();
    let owning_thread = owner.join().unwrap();
    other.join().unwrap();
    assert_eq!(dropping_thread, owning_thread);
    assert_eq!(
        while_dropping
            .expect("session cleanup held the global registry")
            .unwrap(),
        0.
    );
}

#[test]
fn operation_error_and_panic_preserve_session_access_and_resource_scope() {
    let root = tempfile::tempdir().unwrap();
    let inside = root.path().join("inside");
    std::fs::create_dir(&inside).unwrap();
    let id = open(&inside, None);
    let error = session::with_session(id, |session| -> Result<()> {
        session.frame = 31.;
        Err("original handler failure".into())
    });
    assert_eq!(error.unwrap_err().to_string(), "original handler failure");
    let panic = std::panic::catch_unwind(|| {
        session::with_session(id, |_| -> Result<()> { panic!("controlled handler panic") })
    });
    assert!(panic.is_err());
    assert_eq!(
        panic.unwrap_err().downcast_ref::<&str>(),
        Some(&"controlled handler panic")
    );
    assert_eq!(
        session::with_session(id, |session| Ok(session.frame)).unwrap(),
        31.
    );
    assert_eq!(session::resource_info(root.path()).unwrap()["sessions"], 1);
    assert_eq!(session::resource_info(&inside).unwrap()["graphics"], 0);
    session::close(id);
    session::close(id);
    assert_eq!(session::resource_info(root.path()).unwrap()["sessions"], 0);
}

impl Platform for Stub {
    fn name(&self) -> &'static str {
        "session-concurrency-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> MediaResult<()>,
    ) -> MediaResult<motion_media::VideoProbe> {
        panic!("empty project must not probe media")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> MediaResult<Box<dyn VideoDecoder>> {
        panic!("empty project must not decode video")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(f64) -> MediaResult<()>,
    ) -> MediaResult<motion_core::AudioAsset> {
        panic!("empty project must not decode audio")
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> MediaResult<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> Result<SurfaceTarget> {
        panic!("empty project must not create a surface")
    }
}
