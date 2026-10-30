//! Actual encoded output, with an independent numeric reference and controls.
use super::*;
use motion_core::{Command, Ease, KeyframeSelection, KeyframeTarget, Layer, Property};

fn selection() -> Vec<KeyframeSelection> {
    vec![KeyframeSelection {
        target: KeyframeTarget::Property {
            object: 7,
            property: Property::Position,
            axis: None,
        },
        frames: vec![2],
    }]
}

// Independent Hermite form: the selected middle endpoint has a weighted
// harmonic physical rate of 4.8, between secants 16 and 8/3 units per frame.
fn reference(frame: f64) -> f64 {
    let (start, span, a, b, incoming, outgoing) = if frame <= 2. {
        (0., 2., 8., 40., 16., 4.8)
    } else {
        (2., 3., 40., 48., 4.8, 8. / 3.)
    };
    let t = (frame - start) / span;
    let t2 = t * t;
    let t3 = t2 * t;
    (2. * t3 - 3. * t2 + 1.) * a
        + (t3 - 2. * t2 + t) * span * incoming
        + (-2. * t3 + 3. * t2) * b
        + (t3 - t2) * span * outgoing
}

fn decode(path: &Path, root: &Path, name: &str) -> Vec<Vec<u8>> {
    let probe = crate::av::probe(path, None, None, &|| Ok(())).unwrap();
    assert_eq!(probe.asset.frame_count, 6);
    assert_eq!(probe.asset.video_end_us, 200_000);
    assert_eq!(
        probe.timestamps,
        [0, 33_333, 66_667, 100_000, 133_333, 166_667]
    );
    let timestamps = probe.timestamps.clone();
    let mut decoder = crate::av::Decoder::open(path, probe.asset, probe.timestamps).unwrap();
    timestamps
        .into_iter()
        .enumerate()
        .map(|(index, pts)| {
            let frame = decoder.frame(pts, &|| Ok(())).unwrap();
            let pixels = frame.rgba().unwrap().into_owned();
            assert_eq!(pixels.len(), 64 * 32 * 4);
            image::save_buffer(
                root.join(format!("{name}-{index}.png")),
                &pixels,
                64,
                32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            pixels
        })
        .collect()
}

fn center(pixels: &[u8]) -> Option<f64> {
    let mut count = 0;
    let mut x = 0.;
    for (index, color) in pixels.chunks_exact(4).enumerate() {
        if color[0] > 160 && color[1] < 180 && color[2] < 180 {
            x += (index % 64) as f64 + 0.5;
            count += 1;
        }
    }
    (count > 0).then(|| x / f64::from(count))
}

#[test]
#[ignore = "manual GPU and FFmpeg proof: seven six-frame movies and decoded pixel controls"]
fn automatic_timing_survives_reopen_freeze_and_live_edits_in_actual_encoded_output() {
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_AUTOMATIC_MOVIE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    fs::create_dir_all(&root).unwrap();
    let mut original = Project::new(64, 32, 30, 6).unwrap();
    original.background = [0., 0., 0., 1.];
    let mut layer = Layer::solid(
        7,
        "Automatic output",
        [8.; 2],
        [8., 16., 0.],
        [1., 0.2, 0.1, 1.],
    );
    for (frame, x) in [(0, 8.), (2, 40.), (5, 48.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 16., 0.], Ease::Linear)
            .unwrap();
    }
    original.layers.push(layer);
    let source = root.join("live");
    fs::create_dir_all(&source).unwrap();
    let mut session = Session::new(original.clone(), source, crate::platform()).unwrap();
    let linear = FrozenMovie::from_session(&session).unwrap();
    session
        .engine
        .apply(Command::KeyframeAutomatic {
            selections: selection(),
            enabled: true,
        })
        .unwrap();
    let automatic = session.engine.snapshot();
    assert!(automatic.layers[0].transform.position.keys[1].auto_bezier);
    for time in [0., 0.125, 0.5, 1., 1.75, 2., 2.25, 3., 4.5, 5.] {
        let value = automatic.layers[0].transform.position.sample(time).unwrap();
        assert!(
            (f64::from(value[0]) - reference(time)).abs() < 0.00001,
            "time={time}: {value:?}"
        );
        assert_eq!(&value[1..], &[16., 0.]);
    }
    let frozen = FrozenMovie::from_session(&session).unwrap();
    let saved = root.join("saved");
    motion_core::storage::save(&saved, &automatic).unwrap();
    let reopened = motion_core::storage::load(&saved).unwrap();
    assert_eq!(reopened, automatic);
    let reopened_session = Session::new(reopened, saved, crate::platform()).unwrap();
    let saved_movie = FrozenMovie::from_session(&reopened_session).unwrap();
    session
        .engine
        .apply(Command::KeyframeAutomatic {
            selections: selection(),
            enabled: false,
        })
        .unwrap();
    let manual = session.engine.snapshot();
    assert!(!manual.layers[0].transform.position.keys[1].auto_bezier);
    assert_ne!(manual, automatic);
    for index in 0..=500 {
        let time = f64::from(index) / 100.;
        assert_eq!(
            automatic.layers[0].transform.position.sample(time).unwrap(),
            manual.layers[0].transform.position.sample(time).unwrap()
        );
    }
    let manual_movie = FrozenMovie::from_session(&session).unwrap();
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &automatic);
    session
        .engine
        .apply(Command::KeyframeInterpolation {
            selections: selection(),
            ease: Ease::Hold,
        })
        .unwrap();
    let held = FrozenMovie::from_session(&session).unwrap();
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &automatic);
    session
        .engine
        .apply(Command::SetVector {
            object: 7,
            property: Property::Position,
            frame: 0,
            value: [16., 16., 0.],
        })
        .unwrap();
    let neighbor = FrozenMovie::from_session(&session).unwrap();
    assert!(neighbor.project.layers[0].transform.position.keys[1].auto_bezier);
    assert_eq!(
        neighbor.project.layers[0].transform.position.keys[1].value,
        [40., 16., 0.]
    );
    assert_ne!(
        neighbor.project.layers[0]
            .transform
            .position
            .sample(1.)
            .unwrap(),
        automatic.layers[0].transform.position.sample(1.).unwrap()
    );
    session.engine.undo().unwrap();
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &original);
    assert!(!session.engine.can_undo());
    session
        .engine
        .apply(Command::Remove {
            object: 7,
            frame: 0,
        })
        .unwrap();
    let removed = FrozenMovie::from_session(&session).unwrap();
    assert!(removed.project.layers.is_empty());
    assert_eq!(frozen.project, automatic);
    fs::write(
        root.join("automatic-project.json"),
        serde_json::to_vec_pretty(&automatic).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("frozen-manual-project.json"),
        serde_json::to_vec_pretty(&manual).unwrap(),
    )
    .unwrap();
    drop(session);
    drop(reopened_session);
    let mut movies = Vec::new();
    for (name, movie) in [
        ("automatic", frozen),
        ("reopened", saved_movie),
        ("manual", manual_movie),
        ("linear", linear),
        ("hold", held),
        ("edited-neighbor", neighbor),
        ("removed", removed),
    ] {
        let output = root.join(format!("{name}.mp4"));
        let result = movie
            .export(&output, &ExportControl::default(), |_| {})
            .unwrap();
        assert_eq!(result.frames, 6);
        movies.push(decode(&output, &root, name));
    }
    assert_eq!(
        movies[0], movies[1],
        "save/reopen or later live edits changed frozen output"
    );
    assert_eq!(
        movies[0], movies[2],
        "freezing automatic endpoints changed encoded pixels"
    );
    let centers: Vec<_> = movies[0]
        .iter()
        .map(|pixels| center(pixels).unwrap())
        .collect();
    for (frame, actual) in centers.iter().enumerate() {
        assert!(
            (actual - reference(frame as f64)).abs() <= 1.,
            "frame={frame} expected={} center={actual}",
            reference(frame as f64)
        );
    }
    assert!(
        movies[6].iter().all(|pixels| center(pixels).is_none()),
        "removed layer still rendered"
    );
    let mut differences = vec![];
    for index in 3..7 {
        let difference: Vec<_> = movies[0]
            .iter()
            .zip(&movies[index])
            .map(|(a, b)| a.iter().zip(b).filter(|(a, b)| a != b).count())
            .collect();
        assert!(
            difference[1..5].iter().any(|bytes| *bytes > 64),
            "control={index}: {difference:?}"
        );
        differences.push(difference);
    }
    fs::write(root.join("result.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "framesPerMovie":6,"movies":7,"savedReopenedPixelsEqual":true,"manualFreezePixelsEqual":true,
        "independentHermiteReferenceCenters":(0..6).map(|i|reference(i as f64)).collect::<Vec<_>>(),
        "decodedCenters":centers,"differentBytesFromLinearHoldNeighborRemoval":differences,
        "sourceProjectVersion":automatic.version,"closedLiveSessionBeforeExports":true
    })).unwrap()).unwrap();
}
