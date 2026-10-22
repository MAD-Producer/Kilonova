use motion_host::{preview_inputs::*, Platform, VideoDecoder, VideoQuery};
use motion_render::image_resources::Error as ImageError;
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

struct NoMedia;
impl Platform for NoMedia {
    fn name(&self) -> &'static str {
        "no-media"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        Err("unexpected video probe".into())
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        Err("unexpected video decode".into())
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        Err("unexpected audio decode".into())
    }
    fn media_capabilities(
        &self,
        _: Option<&VideoQuery>,
    ) -> motion_media::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> motion_host::Result<motion_host::SurfaceTarget> {
        Err("unexpected GPU surface".into())
    }
}

#[test]
fn preview_repeated_failure_preserves_same_native_cause_then_new_request_recovers() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let path = root.path().join("assets/image.png");
    image::save_buffer(&path, &[20, 40, 60, 255], 1, 1, image::ColorType::Rgba8).unwrap();
    let healthy = std::fs::read(&path).unwrap();
    let mut corrupt = healthy.clone();
    let idat = corrupt.windows(4).position(|b| b == b"IDAT").unwrap();
    let len = u32::from_be_bytes(corrupt[idat - 4..idat].try_into().unwrap()) as usize;
    corrupt[idat + 4 + len] ^= 1;
    std::fs::write(&path, corrupt).unwrap();
    let mut project = motion_core::Project::new(32, 32, 30, 90).unwrap();
    project.assets.push(motion_core::Asset {
        id: 17,
        path: "assets/image.png".into(),
        width: 1,
        height: 1,
    });
    let before = serde_json::to_value(&project).unwrap();
    let mut reader = PreviewInputReader::new(Arc::new(NoMedia));
    let request = |sequence| InputRequest {
        sequence,
        source: InputSource::ImageAsset { asset: 17 },
        frame: 0.,
        max_edge: 8,
    };
    reader
        .request(&project, root.path(), 5, request(1))
        .unwrap();
    let began = Instant::now();
    let first = loop {
        match reader.poll(1, 5) {
            Err(motion_host::Error::ImageResource(ImageError::Shared(cause))) => break cause,
            Err(error) => panic!("wrong native cause: {error:?}"),
            Ok(InputPoll::Pending(_)) => {}
            Ok(InputPoll::Ready(_)) => panic!("corrupt PNG accepted"),
        }
        assert!(began.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    };
    assert!(matches!(first.as_ref(), ImageError::Png(_)));
    for _ in 0..3 {
        match reader.poll(1, 5) {
            Err(motion_host::Error::ImageResource(ImageError::Shared(cause))) => {
                assert!(Arc::ptr_eq(&first, &cause))
            }
            _ => panic!("repeated preview lost original failure"),
        }
    }
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    std::fs::write(path, healthy).unwrap();
    reader
        .request(&project, root.path(), 5, request(2))
        .unwrap();
    let began = Instant::now();
    loop {
        match reader.poll(2, 5).unwrap() {
            InputPoll::Ready(prepared) => {
                assert_eq!(prepared.info.sequence, 2);
                match prepared.pixels {
                    InputPixels::Image(pixels) => assert_eq!(pixels.rgba, [20, 40, 60, 255]),
                    _ => panic!("expected original image"),
                }
                break;
            }
            InputPoll::Pending(_) => {}
        }
        assert!(began.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
}
