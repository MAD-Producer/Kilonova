use motion_core::{storage, Ease, Layer, Project};
use motion_host::{Platform, Result, Session, SurfaceTarget, VideoDecoder, VideoQuery};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

struct NoMedia;
impl Platform for NoMedia {
    fn name(&self) -> &'static str {
        "merge-dimensions-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> Result<()>,
    ) -> Result<motion_media::VideoProbe> {
        panic!("unexpected media work")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> Result<Box<dyn VideoDecoder>> {
        panic!("unexpected decoder")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(f64) -> Result<()>,
    ) -> Result<motion_core::AudioAsset> {
        panic!("unexpected audio work")
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
        panic!("unexpected window")
    }
}

#[test]
fn shared_host_reports_policy_applies_command_and_preserves_state_when_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let mut p = Project::new(64, 64, 30, 120).unwrap();
    let mut layer = Layer::solid(1, "Card", [20.; 2], [20., 32., 0.], [1.; 4]);
    let track = &mut layer.transform.position;
    track.upsert(0, [20., 32., 0.], Ease::InOut).unwrap();
    track.upsert(90, [44., 32., 0.], Ease::Linear).unwrap();
    track.separate().unwrap();
    p.layers.push(layer);
    let mut session = Session::new(p, directory.path().into(), Arc::new(NoMedia)).unwrap();
    session.frame = 30.25;
    session.sample().unwrap();
    let before = session.snapshot();
    let capability = &before["capabilities"]["separate_dimensions"]["merge"];
    assert_eq!(capability["supported"], true);
    assert_eq!(capability["command"], "merge_dimensions");
    assert_eq!(capability["policy"], "aligned_keys_shared_interpolation");
    assert_eq!(capability["resampling"], false);
    let state = session
        .apply_commands(r#"{"op":"merge_dimensions","object":1,"property":"position"}"#)
        .unwrap();
    assert_eq!(state["sampledLayers"], before["sampledLayers"]);
    assert_eq!(
        state["timeline_layers"][0]["properties"]["position"]["separated"],
        false
    );
    assert_eq!(
        state["timeline_layers"][0]["properties"]["position"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(session.engine.project().layers[0]
        .transform
        .position
        .axes
        .is_none());
    storage::save(directory.path(), session.engine.project()).unwrap();
    assert_eq!(
        storage::load(directory.path()).unwrap(),
        session.engine.snapshot()
    );
    session.engine.undo().unwrap();
    session.sample().unwrap();
    assert_eq!(
        session.snapshot()["timeline_layers"],
        before["timeline_layers"]
    );
    session
        .apply_commands(
            r#"{"op":"move_key","object":1,"property":"position","axis":"y","from":90,"to":91}"#,
        )
        .unwrap();
    storage::save(directory.path(), session.engine.project()).unwrap();
    let saved = std::fs::read(directory.path().join("project.json")).unwrap();
    let before = session.snapshot();
    assert!(session
        .apply_commands(r#"{"op":"merge_dimensions","object":1,"property":"position"}"#)
        .unwrap_err()
        .contains("matching key times"));
    assert_eq!(session.snapshot(), before);
    assert_eq!(
        std::fs::read(directory.path().join("project.json")).unwrap(),
        saved
    );
}
