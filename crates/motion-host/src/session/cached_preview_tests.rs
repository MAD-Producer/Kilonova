//! Real-window consumer controls. A green cached image differs from the red
//! live scene so a lookup or counter alone cannot satisfy presentation proof.
use super::*;
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, WorkingSpace,
};
use motion_render::{
    CachedCompositionFrame, CapturedImage, CompositionFrameCache, CompositionFrameContext,
    CompositionFrameKey,
};
use std::cell::Cell;
use winit::platform::windows::EventLoopBuilderExtWindows;

fn cached(
    renderer: &Renderer,
    context: CompositionFrameContext,
    precision: ChannelPrecision,
) -> Arc<CachedCompositionFrame> {
    let pixel: Vec<u8> = match precision {
        ChannelPrecision::Unorm8 => vec![0, 255, 0, 255],
        ChannelPrecision::Unorm16 => [0u16, 65535, 0, 65535]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect(),
        ChannelPrecision::Float16 => [0u16, 0x3c00, 0, 0x3c00]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect(),
        ChannelPrecision::Float32 => [0f32, 1., 0., 1.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect(),
    };
    let image = CapturedImage {
        contract: ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: AlphaMode::Premultiplied,
            },
            precision,
        ),
        storage: motion_render::image_formats::color_storage(precision),
        width: 16,
        height: 16,
        pixels: pixel.repeat(256),
    };
    let key = CompositionFrameKey::new(
        context,
        1.,
        16,
        16,
        motion_render::image_formats::native_format(image.storage),
    )
    .unwrap();
    let mut cache = CompositionFrameCache::new(context, 16 * 16 * 16, 1).unwrap();
    let target = cache.prepare_native(renderer, key).unwrap().unwrap();
    assert!(cache.install_native(renderer, target, &image).unwrap());
    cache.get(renderer, key).unwrap().unwrap()
}
fn assert_color(session: &Session, color: [u8; 4]) {
    assert!(precision_preview_tests::pixels(session)
        .chunks_exact(4)
        .all(|pixel| pixel == color));
}
fn present(
    session: &mut Session,
    frame: f64,
    cached: &CachedCompositionFrame,
    context: CompositionFrameContext,
) {
    let overlays = Cell::new(0);
    assert!(session
        .render_cached_with_view(frame, None, None, cached, context, |_, _, _| overlays
            .set(overlays.get() + 1))
        .unwrap());
    assert_eq!(overlays.get(), 1);
}

#[test]
#[ignore = "explicit real-window native cache consumption checkpoint for all four working precisions"]
fn actual_surface_uses_cached_pixels_and_rejects_stale_context_frame_profile_and_owner() {
    const NAME: &str = "session::cached_preview_tests::actual_surface_uses_cached_pixels_and_rejects_stale_context_frame_profile_and_owner";
    const CHILD: &str = "MOTION_CACHED_PREVIEW_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                NAME,
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let root = tempfile::tempdir().unwrap();
        let mut project = Project::new(16, 16, 30, 12).unwrap();
        project.working_precision = precision;
        project.background = [1., 0., 0., 1.];
        let mut session = Session::new(
            project,
            root.path().into(),
            Arc::new(last_error_observation_tests::Stub),
        )
        .unwrap();
        session.preview.set_mode(PreviewMode::High);
        session
            .attach(precision_preview_tests::surface(&events, &session))
            .unwrap();
        let context = CompositionFrameContext {
            document_generation: session.document_generation,
            device_generation: session.surface_epoch,
            content: [1; 32],
            output: [2; 32],
        };
        let cached = cached(
            &session.graphics.as_ref().unwrap().renderer,
            context,
            precision,
        );
        assert!(session.render(1.).unwrap());
        assert_eq!(session.cached_presented, 0);
        assert_color(&session, [255, 0, 0, 255]);
        present(&mut session, 1., &cached, context);
        assert_color(&session, [0, 255, 0, 255]);
        assert_eq!(session.cached_presented, 1);
        assert_eq!(session.last_presented_frame, Some(1.));
        for field in 0..4 {
            let mut stale = context;
            match field {
                0 => stale.document_generation += 1,
                1 => stale.device_generation += 1,
                2 => stale.content[0] += 1,
                _ => stale.output[0] += 1,
            }
            present(&mut session, 1., &cached, stale);
            assert_color(&session, [255, 0, 0, 255]);
            assert_eq!(session.cached_presented, 1);
        }
        present(&mut session, 2., &cached, context);
        assert_color(&session, [255, 0, 0, 255]);
        assert_eq!(session.cached_presented, 1);
        session.preview.set_mode(PreviewMode::Balanced);
        present(&mut session, 1., &cached, context);
        assert_color(&session, [255, 0, 0, 255]);
        assert_eq!(session.cached_presented, 1);
        session.preview.set_mode(PreviewMode::High);
        session.observing = true;
        present(&mut session, 1., &cached, context);
        assert_eq!(session.cached_presented, 1);
        session.observing = false;
        present(&mut session, 1., &cached, context);
        assert_color(&session, [0, 255, 0, 255]);
        assert_eq!(session.cached_presented, 2);
        let foreign = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            Default::default(),
        ))
        .unwrap();
        let foreign_cached = self::cached(&foreign, context, precision);
        let overlays = Cell::new(0);
        let error = session
            .render_cached_with_view(1., None, None, &foreign_cached, context, |_, _, _| {
                overlays.set(overlays.get() + 1)
            })
            .unwrap_err();
        assert!(error.to_string().contains("another renderer"), "{error}");
        assert_eq!(overlays.get(), 0);
        assert_eq!(session.cached_presented, 2);
        assert_color(&session, [0, 255, 0, 255]);
        session.detach();
    }
}
