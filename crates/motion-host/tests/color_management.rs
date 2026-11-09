use motion_core::{color_management::ProjectColorError, Command, Project};
use motion_host::{Platform, Session, SurfaceTarget, VideoDecoder, VideoQuery};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
#[path = "../../motion-core/tests/support/color_project.rs"]
mod fixture;

#[test]
fn managed_session_and_edits_refuse_unprepared_runtime_without_partial_commit() {
    let root = tempfile::tempdir().unwrap();
    let error = Session::new(fixture::project(), root.path().into(), Arc::new(NoIo))
        .err()
        .unwrap();
    assert!(matches!(
        error,
        motion_host::Error::Model(motion_core::Error::ProjectColor(
            ProjectColorError::RuntimeNotPrepared {
                phase: "scene_create"
            }
        ))
    ));
    let initial = Project::new(64, 64, 30, 120).unwrap();
    let mut session = Session::new(initial.clone(), root.path().into(), Arc::new(NoIo)).unwrap();
    let history = session.engine.history_diagnostics();
    let error = session
        .apply_command_batch(vec![
            Command::SetColor {
                object: 0,
                value: [1.; 4],
            },
            Command::SetColorManagement {
                settings: fixture::managed(fixture::settings()),
            },
        ])
        .unwrap_err();
    assert!(matches!(
        error,
        motion_host::Error::Model(motion_core::Error::ProjectColor(
            ProjectColorError::RuntimeNotPrepared {
                phase: "edit_prepare"
            }
        ))
    ));
    assert_eq!(session.engine.snapshot(), initial);
    assert_eq!(session.engine.history_diagnostics(), history);
    assert_eq!(session.engine.revision(), 0);
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot["project"], serde_json::to_value(&initial).unwrap());
    session
        .apply_command_batch(vec![Command::SetColor {
            object: 0,
            value: [1.; 4],
        }])
        .unwrap();
    assert_eq!(session.engine.project().background, [1.; 4]);
}

#[test]
fn existing_scene_refuses_managed_resampling_before_mutating_frame_or_layers() {
    let initial = Project::new(64, 64, 30, 120).unwrap();
    let mut scene = motion_render::Scene::new(&initial).unwrap();
    let old_frame = scene.frame;
    let error = scene
        .sample(
            &fixture::project(),
            3.,
            None,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        motion_core::Error::ProjectColor(ProjectColorError::RuntimeNotPrepared {
            phase: "scene_sample"
        })
    ));
    assert_eq!(scene.frame, old_frame);
    assert!(scene.layers.is_empty());
    scene
        .sample(&initial, 3., None, &motion_core::ExpressionEvaluator)
        .unwrap();
}

struct NoIo;
impl Platform for NoIo {
    fn name(&self) -> &'static str {
        "color-contract-no-io"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("color preflight must not probe media")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        panic!("color preflight must not decode")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        panic!("color preflight must not decode audio")
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> motion_media::Result<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _: motion_render::AllocationBudget,
    ) -> motion_host::Result<SurfaceTarget> {
        panic!("color preflight must not create GPU surfaces")
    }
}
