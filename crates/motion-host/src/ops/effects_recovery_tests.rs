use super::*;
use crate::{Platform, SurfaceTarget, VideoDecoder, VideoQuery};
use motion_media::Result as MediaResult;
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
struct Stub;
impl Platform for Stub {
    fn name(&self) -> &'static str {
        "memory-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> MediaResult<()>,
    ) -> MediaResult<motion_media::VideoProbe> {
        Err("unused".into())
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> Result<Box<dyn VideoDecoder>> {
        Err("unused".into())
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(f64) -> MediaResult<()>,
    ) -> MediaResult<motion_core::AudioAsset> {
        Err("unused".into())
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::WindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> Result<SurfaceTarget> {
        Err("unused".into())
    }
}

fn session() -> (tempfile::TempDir, Session, String) {
    let directory = tempfile::tempdir().unwrap();
    let package = motion_effects::builtin::particle_package().unwrap();
    let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
    let mut layer = motion_core::Layer::solid(1, "editor", [64.; 2], [32., 32., 0.], [0.; 4]);
    layer.effects.push(motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [64.; 2],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut session =
        Session::new(project, directory.path().to_path_buf(), Arc::new(Stub)).unwrap();
    session
        .engine
        .apply(Command::SetColor {
            object: 1,
            value: [0.1, 0.2, 0.3, 1.],
        })
        .unwrap();
    let opened = session
        .plugin_request("{\"op\":\"editor_open\",\"object\":1,\"instance\":1}")
        .unwrap();
    let token = opened["token"].as_str().unwrap().to_owned();
    (directory, session, token)
}
fn begin_and_edit(session: &mut Session, token: &str) {
    let revision = session.engine.revision();
    session.plugin_request(&json!({"op":"editor_message","token":token,"message":{"op":"begin","revision":revision}}).to_string()).unwrap();
    let revision = session.engine.revision();
    session.plugin_request(&json!({"op":"editor_message","token":token,"message":{"op":"set","revision":revision,"param":"rate","value":[120.,0.,0.,0.]}}).to_string()).unwrap();
    assert!(session.engine.gesture_active());
}
#[test]
fn expired_or_disconnected_pages_cancel_and_restore_history_and_saved_edits() {
    for reason in ["lease", "disconnect", "cancel", "surface_then_disconnect"] {
        let (directory, mut session, token) = session();
        let original = session.engine.snapshot();
        begin_and_edit(&mut session, &token);
        assert!(!session.engine.can_undo());
        assert!(session
            .engine
            .apply_batch_saved(vec![], directory.path())
            .is_err());
        match reason {
            "lease" => {
                session.editor_activity = Some(Instant::now() - Duration::from_secs(121));
                assert!(session
                    .plugin_request(
                        &json!({"op":"editor_message","token":token,"message":{"op":"state"}})
                            .to_string()
                    )
                    .is_err());
            }
            "disconnect" => {
                session
                    .plugin_request(&json!({"op":"editor_disconnected","token":token}).to_string())
                    .unwrap();
            }
            "cancel" => {
                session
                    .plugin_request("{\"op\":\"cancel_plugin_gesture\"}")
                    .unwrap();
            }
            "surface_then_disconnect" => {
                session.detach();
                // Compose can replace the preview Surface while the same
                // native editor page and transaction remain alive.
                assert!(session.editor.as_ref().unwrap().gesture);
                assert!(session.engine.gesture_active());
                session
                    .plugin_request(&json!({"op":"editor_disconnected","token":token}).to_string())
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(session.engine.snapshot(), original);
        assert!(!session.engine.gesture_active());
        assert!(session.editor.is_none());
        assert!(session.editor_token.is_empty());
        assert!(session.engine.can_undo());
        session.engine.undo().unwrap();
        session.engine.redo().unwrap();
        assert_eq!(session.engine.snapshot(), original);
        session
            .engine
            .apply_batch_saved(
                vec![Command::SetColor {
                    object: 1,
                    value: [0.4, 0.5, 0.6, 1.],
                }],
                directory.path(),
            )
            .unwrap();
        assert_eq!(
            motion_core::storage::load(directory.path()).unwrap(),
            session.engine.snapshot()
        );
    }
}
#[test]
fn valid_heartbeat_renews_the_lease_but_stale_tokens_cannot_keep_it_alive() {
    let (_, mut session, token) = session();
    begin_and_edit(&mut session, &token);
    session.editor_activity = Some(Instant::now() - Duration::from_secs(110));
    assert!(session
        .plugin_request("{\"op\":\"editor_heartbeat\",\"token\":\"stale\"}")
        .is_err());
    session
        .plugin_request(&json!({"op":"editor_heartbeat","token":token}).to_string())
        .unwrap();
    session
        .recover_plugin_editor(Instant::now() + Duration::from_secs(20))
        .unwrap();
    assert!(session.editor.as_ref().unwrap().gesture);
    session
        .plugin_request(&json!({"op":"editor_close","token":token}).to_string())
        .unwrap();
    assert!(!session.engine.gesture_active());
}

#[test]
fn host_history_commit_and_cancel_finish_the_plugin_page_consistently() {
    use crate::{ops::editing, session};
    for commit in [false, true] {
        let (directory, initial, _) = self::session();
        let original = initial.engine.snapshot();
        let id = session::open(
            directory.path().to_path_buf(),
            original.clone(),
            Arc::new(Stub),
        )
        .unwrap();
        let edited = session::with_session(id, |session| {
            let opened =
                session.plugin_request("{\"op\":\"editor_open\",\"object\":1,\"instance\":1}")?;
            begin_and_edit(session, opened["token"].as_str().unwrap());
            Ok(session.engine.snapshot())
        })
        .unwrap();
        editing::history(
            id,
            if commit {
                editing::HISTORY_COMMIT
            } else {
                editing::HISTORY_CANCEL
            },
        )
        .unwrap();
        session::with_session(id, |session| {
            assert!(session.editor.is_none());
            assert!(!session.engine.gesture_active());
            assert_eq!(
                session.engine.snapshot(),
                if commit {
                    edited.clone()
                } else {
                    original.clone()
                }
            );
            if commit {
                assert!(session.engine.undo().unwrap());
                assert_eq!(session.engine.snapshot(), original);
                assert!(session.engine.redo().unwrap());
                assert_eq!(session.engine.snapshot(), edited);
            }
            Ok(())
        })
        .unwrap();
        session::close(id);
    }
}

#[test]
fn unrelated_host_entry_recovers_an_expired_page_before_editing() {
    let (directory, initial, _) = session();
    let original = initial.engine.snapshot();
    let id = crate::session::open(
        directory.path().to_path_buf(),
        original.clone(),
        Arc::new(Stub),
    )
    .unwrap();
    crate::session::with_session(id, |session| {
        let opened =
            session.plugin_request("{\"op\":\"editor_open\",\"object\":1,\"instance\":1}")?;
        begin_and_edit(session, opened["token"].as_str().unwrap());
        session.editor_activity = Some(Instant::now() - Duration::from_secs(121));
        Ok(())
    })
    .unwrap();
    crate::session::with_session(id, |session| {
        assert_eq!(session.engine.snapshot(), original);
        assert!(!session.engine.gesture_active());
        assert_eq!(
            session.preview_info()["pluginEditorRecovery"]["last"]["reason"],
            "lease_expired"
        );
        session
            .engine
            .apply(Command::SetColor {
                object: 1,
                value: [0.4, 0.5, 0.6, 1.],
            })
            .unwrap();
        Ok(())
    })
    .unwrap();
    crate::session::close(id);
}

#[test]
fn trusted_plugin_cancel_does_not_cancel_an_unrelated_host_gesture() {
    let (_, mut session, _) = session();
    session
        .plugin_request("{\"op\":\"editor_close\",\"token\":\"stale\"}")
        .unwrap_err();
    session
        .plugin_request("{\"op\":\"cancel_plugin_gesture\"}")
        .unwrap();
    session.engine.begin_gesture().unwrap();
    session
        .plugin_request("{\"op\":\"cancel_plugin_gesture\"}")
        .unwrap();
    assert!(session.engine.gesture_active());
    session.engine.end_gesture(false).unwrap();
}
