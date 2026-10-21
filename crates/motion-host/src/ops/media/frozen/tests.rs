use super::*;
use crate::{Platform, SurfaceTarget, VideoDecoder, VideoQuery};
use motion_core::{AudioAsset, AudioClip, Content, Layer, Project};
use std::{fs, path::Path};

struct Stub;
impl Platform for Stub {
    fn name(&self) -> &'static str {
        "frozen-contract-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("must not probe")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        panic!("fixture has no video")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<AudioAsset> {
        panic!("must use prepared PCM")
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> motion_media::Result<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> crate::Result<SurfaceTarget> {
        panic!("must not create GPU")
    }
}
fn fixture(root: &Path) -> Project {
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join("assets/source.wav"), b"registered source").unwrap();
    let mut project = Project::new(32, 32, 30, 30).unwrap();
    project.version = motion_core::CURRENT_PROJECT_VERSION;
    project.audio_assets.push(AudioAsset {
        id: 7,
        path: "assets/source.wav".into(),
        bytes: 17,
        mime: "audio/wav".into(),
        track: 0,
        sample_rate: 48000,
        channels: 2,
        sample_frames: 48000,
        duration_us: 1_000_000,
    });
    let mut layer = Layer::solid(4, "sound", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(7),
    };
    project.layers.push(layer);
    project.validate().unwrap();
    project
}
#[test]
fn real_cache_failure_preserves_native_source_and_recovers_without_a_reserved_slot_leak() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let project = fixture(&root);
    let session = crate::session::open(root.clone(), project.clone(), Arc::new(Stub)).unwrap();
    let baseline = audio_info().unwrap().occupied;
    let failure = freeze_audio(session).unwrap_err();
    let Error::Media(error) = failure else {
        panic!("native media failure required")
    };
    let motion_media::Error::Io(cause) = error.primary() else {
        panic!("I/O source lost: {error:?}")
    };
    assert_eq!(cause.kind(), std::io::ErrorKind::NotFound);
    assert!(error.to_string().contains("asset=7"));
    assert_eq!(audio_info().unwrap().occupied, baseline);
    let cache = motion_media::cache_path(&root, &project.audio_assets[0]).unwrap();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    let mut pcm = Vec::new();
    for _ in 0..48000 {
        pcm.extend_from_slice(&0.25f32.to_le_bytes());
        pcm.extend_from_slice(&(-0.5f32).to_le_bytes());
    }
    fs::write(cache, pcm).unwrap();
    let report = freeze_audio(session).unwrap();
    let handle = report["handle"].as_i64().unwrap();
    let mut reference = AudioMixer::new(project, &root).unwrap();
    let mut floats = vec![0.; 128];
    let count = reference.mix(40, &mut floats).unwrap();
    crate::session::close(session); // The frozen reader owns its immutable snapshot.
    let mut bytes = vec![0; 512];
    let mixed = super::super::read_frozen_pcm_into(handle, 40, 64, &mut bytes).unwrap();
    assert_eq!(mixed["frames"], count);
    assert_eq!(mixed["pts_us"], 40 * 1_000_000 / 48000);
    let expected: Vec<u8> = floats.into_iter().flat_map(f32::to_le_bytes).collect();
    assert_eq!(bytes, expected);
    assert!(matches!(
        read_frozen_pcm_into(handle, u64::MAX, 64, &mut bytes),
        Err(Error::Media(_))
    ));
    assert_eq!(release_frozen_audio(handle).unwrap()["released"], true);
    assert_eq!(release_frozen_audio(handle).unwrap()["released"], false);
    assert!(matches!(
        read_frozen_pcm_into(handle, 0, 64, &mut bytes),
        Err(Error::Closed { .. })
    ));
}

#[test]
fn video_snapshot_sequences_and_legacy_wire_shapes_remain_compatible_after_session_close() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let mut project = Project::new(32, 32, 30, 60).unwrap();
    project.version = motion_core::CURRENT_PROJECT_VERSION;
    let session = crate::session::open(root, project.clone(), Arc::new(Stub)).unwrap();
    let report = super::super::freeze_video(session).unwrap();
    let handle = report["handle"].as_i64().unwrap();
    assert_eq!(report["project"], serde_json::to_value(project).unwrap());
    crate::session::close(session);
    for sequence in [1, 2] {
        assert_eq!(
            super::super::request_frozen_composition_frame(handle, 0., sequence).unwrap(),
            json!({"state":"ready","sequence":sequence,"instances":[]})
        );
    }
    assert!(matches!(
        request_frozen_composition_frame(handle, 0., 1),
        Err(Error::Superseded(_))
    ));
    assert!(matches!(
        request_frozen_composition_frame(handle, f64::NAN, 3),
        Err(Error::Model(_))
    ));
    assert_eq!(
        request_frozen_composition_frame(handle, 0., 2).unwrap()["state"],
        "ready"
    );
    assert!(video_info()
        .unwrap()
        .readers
        .iter()
        .all(|r| r.operation.is_none() && r.waiting == 0));
    assert_eq!(
        super::super::release_frozen_video(handle).unwrap(),
        json!({"released":true})
    );
    assert!(matches!(
        request_frozen_composition_frame(handle, 0., 3),
        Err(Error::Closed { .. })
    ));
}
