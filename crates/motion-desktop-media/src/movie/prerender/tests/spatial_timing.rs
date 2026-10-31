//! Qualify the prerender boundary after main added stable-ID spatial timing.
use super::*;

fn project() -> Project {
    let mut project = Project::new(32, 24, 30, 6).unwrap();
    project.background = [0., 0., 0., 1.];
    let mut layer = Layer::solid(
        1,
        "Roving cache control",
        [4.; 2],
        [6., 12., 0.],
        [1., 0., 0., 1.],
    );
    for (frame, x) in [(0, 6.), (2, 8.), (5, 26.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 12., 0.], Ease::Linear)
            .unwrap();
    }
    layer.transform.position.enable_roving(&[2]).unwrap();
    assert!(layer.transform.position.keys.is_empty());
    // The middle authored key becomes time 0.5, not its old integer time 2.
    assert_eq!(layer.transform.position.sample(0.5).unwrap(), [8., 12., 0.]);
    let sampled = layer.transform.position.sample(2.25).unwrap();
    assert!((sampled[0] - 15.).abs() < 1e-4);
    assert_eq!([sampled[1], sampled[2]], [12., 0.]);
    project.layers.push(layer);
    project.validate().unwrap();
    project
}

#[test]
fn content_keys_distinguish_roving_time_constraints_with_unchanged_values_and_integer_keys() {
    let root = tempfile::tempdir().unwrap();
    let mut project = project();
    let original = frozen(project.clone(), root.path(), 1);
    let before = key::context(&original, 7, 9, &ExportControl::default()).unwrap();
    let track = &mut project.layers[0].transform.position;
    track
        .spatial_animation
        .as_mut()
        .unwrap()
        .set_fixed_time(2, 2.)
        .unwrap();
    assert!(track.keys.is_empty());
    assert_eq!(track.sample(2.).unwrap(), [8., 12., 0.]);
    project.validate().unwrap();
    let changed = frozen(project, root.path(), 1);
    let after = key::context(&changed, 7, 9, &ExportControl::default()).unwrap();
    assert_ne!(
        before.content, after.content,
        "time constraints are part of native cache identity"
    );
    assert_eq!(before.output, after.output);
    // Editing the new owner cannot retroactively mutate an older frozen plan.
    assert_eq!(
        key::context(&original, 7, 9, &ExportControl::default()).unwrap(),
        before
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_worker_preserves_fractional_roving_positions_after_owner_close_and_rewind() {
    let root = tempfile::tempdir().unwrap();
    let session = Session::new(project(), root.path().into(), crate::platform()).unwrap();
    let control = ExportControl::default();
    let input = PrerenderPlan::capture(&session, 7, 9, 82, 1 << 20, &control)
        .unwrap()
        .prepare(&control, |_| {})
        .unwrap();
    drop(session);
    let frames = std::thread::spawn(move || {
        let mut runtime = NativePrerender::open(input, &ExportControl::default()).unwrap();
        [4.5, 0.5, 2.25, 0.5].map(|frame| {
            runtime
                .render(frame, &ExportControl::default(), |_| {})
                .unwrap()
        })
    })
    .join()
    .unwrap();
    for (frame, center) in frames.iter().zip([24., 8., 15., 8.]) {
        assert_eq!(frame.image().storage, ImageStorage::Rgba8UnormSrgb);
        assert_eq!(frame.image().pixels.len(), 32 * 24 * 4);
        for (index, pixel) in frame.image().pixels.chunks_exact(4).enumerate() {
            let x = (index % 32) as f32 + 0.5;
            let y = (index / 32) as f32 + 0.5;
            let red = (x - center).abs() < 2. && (y - 12.).abs() < 2.;
            assert_eq!(
                pixel,
                if red {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 0, 0, 255]
                },
                "center={center} pixel={index}"
            );
        }
    }
    assert_ne!(frames[0].key(), frames[1].key());
    assert_ne!(frames[1].key(), frames[2].key());
    assert_eq!(frames[1].key(), frames[3].key());
    assert_eq!(frames[1].image().pixels, frames[3].image().pixels);
}
