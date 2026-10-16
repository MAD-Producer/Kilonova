use motion_core::{composition::*, Camera, Content, Layer, LayerTimeline, Project};
use motion_host::{Platform, Result, SurfaceTarget, VideoDecoder, VideoQuery};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

// Registration checks managed files and metadata without probing or decoding
// them again. Any unexpected platform media/surface work fails these tests.
struct Stub;
impl Platform for Stub {
    fn name(&self) -> &'static str {
        "asset-registration-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> Result<()>,
    ) -> Result<motion_media::VideoProbe> {
        panic!("registration must not probe media")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> Result<Box<dyn VideoDecoder>> {
        panic!("registration must not decode video")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(f64) -> Result<()>,
    ) -> Result<motion_core::AudioAsset> {
        panic!("registration must not decode audio")
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> Result<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::WindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> Result<SurfaceTarget> {
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
    assert!(error.contains("export audio plan"), "{error}");
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
