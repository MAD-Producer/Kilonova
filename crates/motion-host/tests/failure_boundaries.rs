use motion_core::{composition::*, Camera, Content, Layer, LayerTimeline, Project};
use motion_host::{Platform, Session, SurfaceTarget, VideoDecoder, VideoQuery};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

// Registration checks managed files and metadata without probing or decoding
// them again. Any unexpected platform media/surface work fails these tests.
struct Stub;

#[test]
fn precompose_capacity_error_reaches_structured_reply_without_session_changes() {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.layers.push(Layer::solid(
        1,
        "Selected",
        [32.; 2],
        [32., 32., 0.],
        [1.; 4],
    ));
    project.compositions = (1..MAX_COMPOSITIONS)
        .map(|id| Composition {
            id: format!("comp-{id}"),
            name: format!("Library {id}"),
            width: 64,
            height: 64,
            fps: 30,
            frames: 120,
            background: [0.; 4],
            camera: Camera::new(64, 64),
            layers: vec![],
            expressions: vec![],
        })
        .collect();
    let before = project.clone();
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::new(project, directory.path().into(), Arc::new(Stub)).unwrap();
    let history = session.engine.history_diagnostics();
    let reply = motion_host::ops::dispatch(|| {
        session.apply_commands(
            r#"[{"op":"rename","object":1,"name":"Must roll back"},
        {"op":"composition","composition":"comp-main",
         "action":{"kind":"precompose","objects":[1],"name":"Nested"}}]"#,
        )
    });
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["error_detail"]["code"], "resource_limit", "{reply}");
    assert_eq!(reply["error_detail"]["composition"], MAIN_COMPOSITION);
    assert!(reply["error"]
        .as_str()
        .unwrap()
        .contains("Composition limit"));
    assert!(
        session.engine.project() == &before,
        "failed request changed the session"
    );
    assert_eq!(session.engine.revision(), 0);
    assert_eq!(session.engine.history_diagnostics(), history);
}

#[test]
fn malformed_request_retains_json_location_and_native_identity() {
    let error = motion_host::ops::parse_request("{", 1024, "test request").unwrap_err();
    let motion_host::Error::Json(cause) = &error else {
        panic!("JSON failure lost its type: {error:?}");
    };
    assert!(cause.is_eof());
    assert_eq!(cause.line(), 1);
    assert_eq!(cause.column(), 1);
    let reply = motion_host::ops::typed_error_envelope(&error);
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"], cause.to_string());
    assert!(reply["error_detail"].is_null());
}

#[test]
fn project_creation_retains_native_io_cause_through_context() {
    let directory = tempfile::tempdir().unwrap();
    let barrier = directory.path().join("file");
    std::fs::write(&barrier, b"existing file").unwrap();
    let error = motion_host::ops::project::create(
        barrier.join("project"), "", Arc::new(Stub),
    ).unwrap_err();
    assert!(matches!(&error, motion_host::Error::Io(cause)
        if cause.raw_os_error().is_some()));
    let raw_os_error = match &error {
        motion_host::Error::Io(cause) => cause.raw_os_error(),
        _ => unreachable!(),
    };
    let original = error.to_string();
    let contextual = motion_host::Error::with_context("create project", error);
    let source = std::error::Error::source(&contextual).unwrap();
    let boxed = source.downcast_ref::<Box<motion_host::Error>>()
        .expect("context source must retain its declared Box<Error>");
    assert!(matches!(boxed.as_ref(), motion_host::Error::Io(cause)
        if cause.raw_os_error() == raw_os_error));
    assert_eq!(contextual.to_string(), format!("create project: {original}"));
    assert_eq!(std::fs::read(barrier).unwrap(), b"existing file");
}

#[test]
fn closed_frozen_session_retains_host_identity_in_source_chain() {
    let error = motion_host::ops::media::freeze_audio(-1).unwrap_err();
    let motion_host::Error::Frozen(
        motion_host::ops::media::frozen::Error::Session(cause)
    ) = &error else {
        panic!("frozen reader lost its session cause: {error:?}");
    };
    assert!(matches!(cause.as_ref(), motion_host::Error::Message(message)
        if message == "native session is closed"));
    assert_eq!(error.to_string(), "native session is closed");
}

#[test]
fn structured_composition_replies_require_native_identity() {
    let directory = tempfile::tempdir().unwrap();
    let session = Session::new(Project::new(64, 64, 30, 60).unwrap(),
        directory.path().into(), Arc::new(Stub)).unwrap();
    let error = session.composition_error("context_busy", "Finish gesture", json!({"object":7}));
    let detail = error.composition_detail().unwrap().clone();
    let text = error.to_string();
    assert_eq!(motion_host::ops::dispatch(|| Err(error))["error_detail"], json!(detail));
    for reply in [
        motion_host::ops::dispatch(|| Err(text.clone().into())),
        motion_host::ops::dispatch(|| std::panic::panic_any(text.clone())),
    ] {
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"], text);
        assert!(reply["error_detail"].is_null(), "text forged native detail: {reply}");
    }
    // Explicit old text clients keep their original protocol adapter.
    assert_eq!(motion_host::ops::error_envelope(&text)["error_detail"], json!(detail));
}

#[test]
fn composition_details_survive_media_and_frozen_reader_wrappers() {
    let project = Project::new(64, 64, 30, 60).unwrap();
    let model_error = || project.composition("missing-composition").unwrap_err();
    let wrapped_media = || motion_media::Error::from(model_error())
        .context("resolve audio composition".into());
    let frozen = motion_host::ops::media::frozen::Error::Session(
        Box::new(model_error().into()));
    let errors = [
        motion_host::Error::from(wrapped_media()),
        motion_host::Error::from(frozen),
        motion_host::Error::from(motion_host::ops::media::frozen::Error::Media(wrapped_media())),
        motion_host::Error::from(motion_host::ops::media::frozen::Error::Model(model_error())),
    ];
    for error in errors {
        let reply = motion_host::ops::typed_error_envelope(&error);
        assert_eq!(reply["error_detail"]["code"], "composition_missing");
        assert_eq!(reply["error_detail"]["composition"], "missing-composition");
        assert_eq!(reply["error"], error.to_string());
    }
}

#[test]
fn rejected_command_retains_missing_object_without_committing_history() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::new(Project::new(64, 64, 30, 60).unwrap(),
        directory.path().into(), Arc::new(Stub)).unwrap();
    let before = session.engine.snapshot();
    let error = session.apply_commands(r#"{"op":"rename","object":7,"name":"missing"}"#).unwrap_err();
    assert!(matches!(&error, motion_host::Error::Model(motion_core::Error::Missing(7))));
    assert_eq!(error.to_string(), "object 7 does not exist");
    assert_eq!(session.engine.snapshot(), before);
    assert_eq!(session.engine.revision(), 0);
    assert!(!session.engine.can_undo());
}

impl Platform for Stub {
    fn name(&self) -> &'static str {
        "asset-registration-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("registration must not probe media")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        panic!("registration must not decode video")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(f64) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        panic!("registration must not decode audio")
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> motion_media::Result<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> motion_host::Result<SurfaceTarget> {
        panic!("registration must not create a surface")
    }
}

fn overflowing_audio_timebase() -> Project {
    let rates = [
        239, 233, 229, 227, 223, 211, 199, 197, 193, 191, 181, 179, 173, 167, 163, 157,
    ];
    let mut project = Project::new(64, 64, rates[0], 240).unwrap();
    for index in 1..rates.len() {
        project.compositions.push(Composition {
            id: format!("comp-{index}"),
            name: format!("Nested {index}"),
            width: 64,
            height: 64,
            fps: rates[index],
            frames: 240,
            background: [0.; 4],
            camera: Camera::new(64, 64),
            layers: Vec::new(),
            expressions: Vec::new(),
        });
    }
    for index in 0..rates.len() {
        let mut layer = Layer::solid(1, "Timed layer", [64.; 2], [32., 32., 0.], [1.; 4]);
        layer.timeline = Some(LayerTimeline {
            in_frame: 0,
            out_frame: 240,
            offset_frame: 1,
        });
        if index + 1 < rates.len() {
            layer.content = Content::Composition {
                clip: CompositionClip::new(format!("comp-{}", index + 1)),
            };
        }
        if index == 0 {
            project.layers.push(layer);
        } else {
            project.compositions[index - 1].layers.push(layer);
        }
    }
    project.validate().unwrap();
    project
}

#[test]
fn audio_plan_failure_is_distinct_from_silence_and_export_rejects_it() {
    let project = overflowing_audio_timebase();
    let error = project.audio_voices().unwrap_err().to_string();
    assert!(error.contains("rational arithmetic budget"), "{error}");
    let directory = tempfile::tempdir().unwrap();
    let id = motion_host::session::open(directory.path().into(), project, Arc::new(Stub)).unwrap();
    let snapshot = motion_host::ops::project::state(id).unwrap();
    assert!(snapshot["has_audio"].is_null());
    assert!(snapshot["audio_error"]
        .as_str()
        .unwrap()
        .contains("rational arithmetic budget"));
    let error = motion_host::ops::export::render_plan_info(id).unwrap_err();
    assert!(error.to_string().contains("export audio plan"), "{error}");
    motion_host::session::with_session(id, |session| {
        session.engine = motion_core::Engine::new(Project::new(64, 64, 30, 60).unwrap()).unwrap();
        session.scene = motion_render::Scene::new(session.engine.project());
        let snapshot = session.snapshot();
        assert_eq!(snapshot["has_audio"], false);
        assert!(snapshot["audio_error"].is_null());
        Ok(())
    })
    .unwrap();
    motion_host::session::close(id);
}

#[test]
fn snapshot_exposes_history_pressure_without_persisting_it_into_the_project() {
    let directory = tempfile::tempdir().unwrap();
    let mut session =
        Session::new(Project::demo(), directory.path().into(), Arc::new(Stub)).unwrap();
    assert_eq!(session.snapshot()["history"]["evictions"], 0);
    for index in 0..130 {
        session
            .engine
            .apply(motion_core::Command::Rename {
                object: 1,
                name: format!("entry-{index}"),
            })
            .unwrap();
    }
    let snapshot = session.snapshot();
    assert_eq!(snapshot["history"]["evictions"], 2);
    assert_eq!(snapshot["history"]["undoEntries"], 128);
    assert!(snapshot["project"].get("history").is_none());
}

#[test]
fn keyframe_collision_reaches_command_reply_without_changing_session() {
    for separated in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut project = Project::demo();
        let track = &mut project.layers[1].transform.position;
        track.upsert(0, [0.0; 3], motion_core::Ease::Linear).unwrap();
        track.upsert(60, [1.0; 3], motion_core::Ease::Out).unwrap();
        if separated {
            track.separate().unwrap();
        }
        let before = project.clone();
        let id =
            motion_host::session::open(directory.path().into(), project, Arc::new(Stub)).unwrap();
        let axes = if separated { &["", ",\"axis\":\"x\""][..] } else { &[""][..] };
        for axis in axes {
            for op in ["move_key", "copy_key"] {
                let error = motion_host::ops::editing::command(
                    id,
                    &format!(
                        r#"{{"op":"{op}","object":2,"property":"position","from":0,"to":60{axis}}}"#
                    ),
                )
                .unwrap_err();
                assert!(error.to_string().contains("occupied"), "{error}");
                motion_host::session::with_session(id, |session| {
                    assert_eq!(session.engine.project(), &before);
                    assert_eq!(session.engine.revision(), 0);
                    assert!(!session.engine.can_undo());
                    Ok(())
                })
                .unwrap();
            }
        }
        motion_host::session::close(id);
    }
}
