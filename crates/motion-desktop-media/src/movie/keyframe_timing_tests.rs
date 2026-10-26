//! Frozen output must sample the authored timing, not the later live editor.
use super::*;
use motion_core::{
    timing::{TimingInterpolation, TimingSettings, TimingSide, VelocityUnits},
    Command, Ease, KeyframeSelection, KeyframeTarget, Layer, Property,
};

fn decode(path: &Path) -> Vec<Vec<u8>> {
    let probe = crate::av::probe(path, None, None, &|| Ok(())).unwrap();
    assert_eq!(probe.asset.frame_count, 6);
    assert_eq!(probe.asset.video_end_us, 200_000);
    let timestamps = probe.timestamps.clone();
    let mut decoder = crate::av::Decoder::open(path, probe.asset, probe.timestamps).unwrap();
    timestamps
        .into_iter()
        .map(|pts| {
            decoder
                .frame(pts, &|| Ok(()))
                .unwrap()
                .rgba()
                .unwrap()
                .into_owned()
        })
        .collect()
}

#[test]
#[ignore = "manual GPU and FFmpeg output proof; six frames with positive and negative controls"]
fn frozen_keyframe_timing_matches_reopened_pixels_and_differs_from_linear_and_removed_layer() {
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_KEY_TIMING_MOVIE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    fs::create_dir_all(&root).unwrap();
    let mut project = Project::new(64, 32, 30, 6).unwrap();
    project.background = [0., 0., 0., 1.];
    let mut layer = Layer::solid(
        7,
        "Timing output",
        [8.; 2],
        [8., 16., 0.],
        [1., 0.2, 0.1, 1.],
    );
    for (frame, x) in [(0, 8.), (2, 24.), (5, 48.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 16., 0.], Ease::Linear)
            .unwrap();
    }
    project.layers.push(layer);
    let source = root.join("live");
    fs::create_dir_all(&source).unwrap();
    let mut session = Session::new(project.clone(), source, crate::platform()).unwrap();
    let linear = FrozenMovie::from_session(&session).unwrap();
    let side = Some(TimingSide {
        speed: Some(60.),
        influence: Some(25.),
    });
    session
        .engine
        .apply(Command::KeyframeTiming {
            selections: vec![KeyframeSelection {
                target: KeyframeTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: None,
                },
                frames: vec![2],
            }],
            settings: TimingSettings {
                interpolation: TimingInterpolation::Bezier,
                units: VelocityUnits::PropertyPerSecond,
                incoming: side,
                outgoing: side,
                match_speed: true,
            },
        })
        .unwrap();
    let edited = session.engine.snapshot();
    assert_ne!(edited, project);
    let frozen = FrozenMovie::from_session(&session).unwrap();
    assert_eq!(frozen.project, edited);
    let saved = root.join("saved");
    motion_core::storage::save(&saved, &edited).unwrap();
    let reopened = motion_core::storage::load(&saved).unwrap();
    assert_eq!(reopened, edited);
    let control_session = Session::new(reopened, saved, crate::platform()).unwrap();
    let control = FrozenMovie::from_session(&control_session).unwrap();
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &project);
    session
        .engine
        .apply(Command::Remove {
            object: 7,
            frame: 0,
        })
        .unwrap();
    let empty = FrozenMovie::from_session(&session).unwrap();
    assert!(empty.project.layers.is_empty());
    assert_eq!(frozen.project, edited);
    drop(session);
    drop(control_session);
    let mut movies = Vec::new();
    for (name, movie) in [
        ("frozen", frozen),
        ("reopened", control),
        ("linear", linear),
        ("removed", empty),
    ] {
        let output = root.join(format!("{name}.mp4"));
        let result = movie
            .export(&output, &ExportControl::default(), |_| {})
            .unwrap();
        assert_eq!(result.frames, 6);
        let pixels = decode(&output);
        assert_eq!(pixels.len(), 6);
        for (index, frame) in pixels.iter().enumerate() {
            assert_eq!(frame.len(), 64 * 32 * 4);
            image::save_buffer(
                root.join(format!("{name}-{index}.png")),
                frame,
                64,
                32,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        movies.push(pixels);
    }
    assert_eq!(
        movies[0], movies[1],
        "frozen timing differed after save/reopen or followed live edits"
    );
    let difference = |a: usize, b: usize| {
        movies[a]
            .iter()
            .zip(&movies[b])
            .map(|(a, b)| a.iter().zip(b).filter(|(a, b)| a != b).count())
            .collect::<Vec<_>>()
    };
    let linear_difference = difference(0, 2);
    let removed_difference = difference(0, 3);
    assert!(
        linear_difference[1..5].iter().any(|n| *n > 64),
        "timing never reached output: {linear_difference:?}"
    );
    assert!(
        removed_difference.iter().all(|n| *n > 64),
        "removed-layer negative control: {removed_difference:?}"
    );
    fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "frames":6,"frozenEqualsSavedReopenedPixels":true,
            "frozenIndependentOfLiveUndoRemovalAndSessionClose":true,
            "differentBytesFromLinearPerFrame":linear_difference,
            "differentBytesFromRemovedPerFrame":removed_difference,
            "sourceProjectVersion":edited.version,
        }))
        .unwrap(),
    )
    .unwrap();
}
