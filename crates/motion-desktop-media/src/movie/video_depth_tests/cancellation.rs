//! Cancel inside an actual decoder call, after task capture/source verification.
use super::*;
use motion_host::{
    platform::{SurfaceTarget, VideoQuery},
    video_frame::{DecodedFrame, RgbEncoding},
    Platform,
};
use std::sync::atomic::{AtomicBool, Ordering};

struct CancelOnFrame {
    inner: Arc<dyn Platform>,
    armed: Arc<AtomicBool>,
    entered: Arc<AtomicBool>,
    control: ExportControl,
}
struct Decoder {
    inner: Box<dyn VideoDecoder>,
    armed: Arc<AtomicBool>,
    entered: Arc<AtomicBool>,
    control: ExportControl,
}
impl VideoDecoder for Decoder {
    fn frame(
        &mut self,
        target: u64,
        check: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<DecodedFrame> {
        if self.armed.swap(false, Ordering::SeqCst) {
            self.entered.store(true, Ordering::SeqCst);
            assert!(self.control.cancel());
        }
        self.inner.frame(target, check)
    }
    fn rgb_encoding(&self) -> motion_media::Result<RgbEncoding> {
        self.inner.rgb_encoding()
    }
    fn seeks(&self) -> u64 {
        self.inner.seeks()
    }
}
impl Platform for CancelOnFrame {
    fn name(&self) -> &'static str {
        "owned-real-codec-cancel"
    }
    fn probe_video(
        &self,
        path: &Path,
        selected: Option<u32>,
        audio: Option<u32>,
        check: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        self.inner.probe_video(path, selected, audio, check)
    }
    fn open_decoder(
        &self,
        path: &Path,
        asset: motion_core::VideoAsset,
        pts: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        Ok(Box::new(Decoder {
            inner: self.inner.open_decoder(path, asset, pts)?,
            armed: self.armed.clone(),
            entered: self.entered.clone(),
            control: self.control.clone(),
        }))
    }
    fn decode_audio(
        &self,
        path: &Path,
        pcm: &Path,
        selected: Option<u32>,
        bytes: u64,
        check: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        self.inner.decode_audio(path, pcm, selected, bytes, check)
    }
    fn media_capabilities(
        &self,
        query: Option<&VideoQuery>,
    ) -> motion_media::Result<serde_json::Value> {
        self.inner.media_capabilities(query)
    }
    fn attach_surface(
        &self,
        window: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        width: u32,
        height: u32,
        budget: motion_render::AllocationBudget,
    ) -> motion_host::Result<SurfaceTarget> {
        self.inner.attach_surface(window, width, height, budget)
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cancellation_inside_real_frozen_video_decoder_preserves_protocol_reason_and_complete_output() {
    let original = tempfile::tempdir().unwrap();
    let outputs = tempfile::tempdir().unwrap();
    let (project, pts, decoded) = source(original.path());
    let control = ExportControl::default();
    let armed = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(AtomicBool::new(false));
    let platform = Arc::new(CancelOnFrame {
        inner: crate::platform(),
        armed: armed.clone(),
        entered: entered.clone(),
        control: control.clone(),
    });
    let id = motion_host::session::open(original.path().into(), project, platform).unwrap();
    motion_host::session::with_session(id, |s| {
        s.configure_depth(RedDepth)?;
        author(s, 0, pts[0], &decoded[0]);
        Ok(())
    })
    .unwrap();
    assert!(motion_host::ops::export::capture(id)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    let plan = motion_host::session::with_session(id, |s| {
        wait(s);
        Ok(FrozenMovie::capture_plan(s, &control).unwrap())
    })
    .unwrap();
    let frozen = plan.freeze(&control).unwrap();
    motion_host::session::close(id);
    original.close().unwrap();
    let output = outputs.path().join("complete.mp4");
    fs::write(&output, b"existing complete output").unwrap();
    armed.store(true, Ordering::SeqCst);
    let error = frozen
        .export(&output, &control, |_| {})
        .err()
        .expect("decoder must observe cancellation");
    assert!(
        entered.load(Ordering::SeqCst),
        "cancellation must occur inside the actual frame call"
    );
    assert_eq!(
        error, "movie export cancelled",
        "the engine recognizes this exact protocol reason"
    );
    assert_eq!(fs::read(&output).unwrap(), b"existing complete output");
    assert_eq!(
        fs::read_dir(outputs.path()).unwrap().count(),
        1,
        "no incomplete movie may remain"
    );
}
