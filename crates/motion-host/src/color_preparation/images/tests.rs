use super::*;
#[path = "../../../../motion-core/tests/support/color_project.rs"]
mod fixture;
use std::{sync::mpsc, time::Duration};

#[test]
fn source_error_paths_are_bounded_escaped_and_preserve_the_filename() {
    assert_eq!(source_path("assets/missing.png"), "assets/missing.png");
    assert_eq!(
        source_path("assets/line\nname.png"),
        "assets/line\\nname.png"
    );
    for text in ["中", "\u{10ffff}", "\n", "界\u{202e}"] {
        let path = format!("assets/{}/original-source.png", text.repeat(512));
        let bounded = source_path(&path);
        assert!(bounded.len() <= 640, "{}", bounded.len());
        assert!(bounded.starts_with("assets/"));
        assert!(bounded.ends_with("/original-source.png"));
        assert!(bounded.contains('…'));
        assert!(!bounded.contains('\n') && !bounded.contains('\u{202e}'));
    }
}

fn setup(root: &Path) -> (Project, PreparedProjectColors) {
    let mut p = fixture::project();
    p.assets.push(motion_core::Asset {
        id: 1,
        path: "assets/1.png".into(),
        width: 3,
        height: 4,
    });
    fixture::write_resources(root, &p);
    std::fs::create_dir_all(root.join("assets")).unwrap();
    image::save_buffer(
        root.join("assets/1.png"),
        &[128, 64, 32, 127].repeat(12),
        3,
        4,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let prepared = prepare_project_colors(
        &p,
        root,
        |r| {
            Ok(ColorProcessorSource {
                version: motion_effects::color_processor::COLOR_PROCESSOR_VERSION,
                ocio_version: r.config.manifest().ocio_version,
                config_key: r.config.key(),
                selection_key: r.config.resolve(r.selection).unwrap().key,
                phase: r.phase,
                input_alpha: r.input_alpha,
                output_alpha: r.output_alpha,
                shader: "vec4 motion_ocio_transform(vec4 color) { return color; }".into(),
                tables: Vec::new(),
            })
        },
        &Observation::new(8200),
    )
    .unwrap()
    .unwrap();
    (p, prepared)
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn source_state_is_published_before_a_blocked_real_decode_and_clears_after_unwind() {
    let root = tempfile::tempdir().unwrap();
    let (p, prepared) = setup(root.path());
    let observation = Observation::new(8201);
    let cancel = AtomicBool::new(false);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let saved = p.clone();
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let renderer = &mut renderer;
        let prepared = &prepared;
        let p = &p;
        let observation = &observation;
        let cancel = &cancel;
        let root = root.path();
        let worker = scope.spawn(move || {
            prepared.prepare_image_with(
                renderer,
                SourceImageRequest {
                    project: p,
                    root,
                    asset: 1,
                    cancel,
                },
                observation,
                |s, b, c| {
                    assert!(observation.running());
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    image_resources::decode_color_f32(s, b, c)
                },
            )
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(observation.running());
        cancel.store(true, Ordering::Relaxed);
        release_tx.send(()).unwrap();
        assert!(
            matches!(worker.join().unwrap(),Err(Error::SourceImage{source,..}) if matches!(*source,crate::Error::ImageResource(image_resources::Error::Cancelled)))
        );
    });
    assert!(!observation.running());
    assert_eq!(p, saved);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    cancel.store(false, Ordering::Relaxed);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = prepared.prepare_image_with(
            &mut renderer,
            SourceImageRequest {
                project: &p,
                root: root.path(),
                asset: 1,
                cancel: &cancel,
            },
            &observation,
            |_, _, _| {
                assert!(observation.running());
                panic!("original color decoder panic");
            },
        );
    }))
    .unwrap_err();
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"original color decoder panic")
    );
    assert!(!observation.running());
    assert_eq!(renderer.image_readback_info().requested_calls, 0);
    let image = prepared
        .prepare_source_image(
            &mut renderer,
            SourceImageRequest {
                project: &p,
                root: root.path(),
                asset: 1,
                cancel: &cancel,
            },
            &observation,
        )
        .unwrap()
        .unwrap();
    assert_eq!(image.source_precision(), ChannelPrecision::Unorm8);
    assert!(!observation.running());
}
