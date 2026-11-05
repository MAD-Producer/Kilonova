use super::*;
use motion_core::LayerTimeline;
pub(super) fn entering(p: &mut Project, id: u64, begin: i32) {
    let mut layer = p.layers[0].clone();
    layer.id = id;
    layer.timeline = Some(LayerTimeline {
        in_frame: begin,
        out_frame: 10,
        offset_frame: begin,
    });
    p.layers.push(layer);
}

#[test]
fn entering_layer_is_decoded_before_visibility_and_first_frame_reuses_the_same_arc() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    p.layers[0].timeline = Some(LayerTimeline {
        in_frame: 3,
        out_frame: 10,
        offset_frame: 3,
    });
    let control = Arc::new(Control::default());
    let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
    let current = scene(&p, 0.);
    let result = reader
        .prepare_scene(&p, root.path(), &current, 0.)
        .unwrap()
        .unwrap();
    assert!(
        result.is_empty(),
        "future pixels must not be returned as current"
    );
    assert!(
        reader.streams.contains_key(&7),
        "upcoming layer needs a decoder before entering its clip"
    );
    wait(|| {
        reader.streams[&7]
            .shared
            .0
            .lock()
            .unwrap()
            .cache
            .find(0)
            .is_some()
    });
    let cached = reader.streams[&7]
        .shared
        .0
        .lock()
        .unwrap()
        .cache
        .find(0)
        .unwrap();
    for _ in 0..10 {
        assert!(reader
            .prepare_scene(&p, root.path(), &current, 0.)
            .unwrap()
            .unwrap()
            .is_empty());
    }
    assert_eq!(
        *control.calls.lock().unwrap(),
        [0],
        "idle polling cannot reopen/redecode the warm picture"
    );
    let first = scene(&p, 3.);
    let output = reader
        .prepare_scene(&p, root.path(), &first, 3.)
        .unwrap()
        .expect("first visible frame already prepared");
    assert_eq!(output.len(), 1);
    assert!(Arc::ptr_eq(&cached, &output[0].1.left));
    assert_eq!(output[0].1.left.pts, 0);
    assert_eq!(reader.metrics()["maxDecoderWorkers"], 4);
}

#[test]
fn exact_sampling_never_starts_upcoming_work_and_seeking_away_retires_it() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    p.layers[0].timeline = Some(LayerTimeline {
        in_frame: 5,
        out_frame: 10,
        offset_frame: 5,
    });
    let control = Arc::new(Control::default());
    let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
    let near = scene(&p, 1.);
    assert!(reader
        .prepare_scene_exact(&p, root.path(), &near)
        .unwrap()
        .unwrap()
        .is_empty());
    assert!(reader.streams.is_empty());
    assert!(reader
        .prepare_scene_sampling_exact(&p, root.path(), &near)
        .unwrap()
        .unwrap()
        .is_empty());
    assert!(reader.streams.is_empty());
    reader.prepare_scene(&p, root.path(), &near, 1.).unwrap();
    assert!(reader.streams.contains_key(&7));
    let away = scene(&p, 0.);
    assert!(reader
        .prepare_scene(&p, root.path(), &away, 0.)
        .unwrap()
        .unwrap()
        .is_empty());
    assert!(reader.streams.is_empty());
    reader.finish().unwrap();
    assert!(reader.retiring.is_empty());
}

#[test]
fn four_required_streams_win_over_future_layers_and_release_a_slot_at_the_cut() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    for id in 8..=10 {
        let mut layer = p.layers[0].clone();
        layer.id = id;
        p.layers.push(layer);
    }
    p.layers[0].timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 3,
        offset_frame: 0,
    });
    entering(&mut p, 11, 3);
    let control = Arc::new(Control::default());
    let mut reader = VideoFrames::new(Arc::new(Backend(control)));
    let before = scene(&p, 1.);
    ready(&mut reader, &p, root.path(), &before);
    assert_eq!(reader.streams.len(), 4);
    assert!(
        !reader.streams.contains_key(&11),
        "warmup cannot take a mandatory worker slot"
    );
    let cut = scene(&p, 3.);
    wait(|| {
        let result = reader.prepare_scene(&p, root.path(), &cut, 3.).unwrap();
        assert!(
            reader.streams.len() + reader.retiring.len() <= 4,
            "retiring decoders retain their slots"
        );
        result.is_some()
    });
    assert!(!reader.streams.contains_key(&7));
    assert!(reader.streams.contains_key(&11));
}

#[test]
fn blocked_future_decoder_does_not_delay_an_already_ready_current_frame() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    p.layers[0].time_mapping = Some(remap(&[(0., 2.)]));
    let control = Arc::new(Control::default());
    let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
    let current = scene(&p, 2.);
    ready(&mut reader, &p, root.path(), &current);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    struct Release(Option<mpsc::Sender<()>>);
    impl Drop for Release {
        fn drop(&mut self) {
            if let Some(tx) = self.0.take() {
                let _ = tx.send(());
            }
        }
    }
    let release = Release(Some(release_tx));
    *control.gate.lock().unwrap() = Some((
        0,
        Arc::new(Gate {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        }),
    ));
    entering(&mut p, 8, 4);
    p.layers[1].time_mapping = None;
    let now = scene(&p, 2.);
    let output = reader
        .prepare_scene(&p, root.path(), &now, 2.)
        .unwrap()
        .unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].0, 7);
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let output = reader
        .prepare_scene(&p, root.path(), &now, 2.)
        .unwrap()
        .expect("blocked future decode must not stall current frame");
    assert_eq!(output.len(), 1);
    let state = reader.streams[&8].shared.0.lock().unwrap();
    assert!(
        state.in_flight.is_some(),
        "verify the decoder is actually blocked"
    );
    drop(state);
    drop(release);
    reader.finish().unwrap();
}

#[test]
fn prewarmed_linear_pair_and_reverse_last_picture_survive_first_visibility() {
    for reversed in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut p = fixture(root.path());
        p.layers[0].timeline = Some(LayerTimeline {
            in_frame: 3,
            out_frame: 10,
            offset_frame: 3,
        });
        p.layers[0].time_mapping = Some(if reversed {
            reverse(10.)
        } else {
            remap(&[(0., 0.5)])
        });
        p.layers[0].sampling.frame_blend = motion_core::render_sampling::FrameBlend::Linear;
        let control = Arc::new(Control::default());
        let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
        let before = scene(&p, 0.);
        reader.prepare_scene(&p, root.path(), &before, 0.).unwrap();
        let left = if reversed { 800_000 } else { 0 };
        wait(|| {
            let state = reader.streams[&7].shared.0.lock().unwrap();
            state.cache.find(left).is_some() && (reversed || state.cache.find(70_000).is_some())
        });
        let cached = reader.streams[&7]
            .shared
            .0
            .lock()
            .unwrap()
            .cache
            .find(left)
            .unwrap();
        let first = scene(&p, 3.);
        let output = reader
            .prepare_scene(&p, root.path(), &first, 3.)
            .unwrap()
            .expect("both required neighbors are warm");
        assert!(Arc::ptr_eq(&cached, &output[0].1.left));
        if reversed {
            assert!(output[0].1.next.is_none());
        } else {
            let (right, weight) = output[0].1.next.as_ref().unwrap();
            assert_eq!(right.pts, 70_000);
            assert!((*weight - 5. / 7.).abs() < 1e-6);
        }
        assert!(reader.streams[&7].shared.0.lock().unwrap().cache.len() <= 5);
    }
}

#[test]
fn inactive_nested_instances_transfer_prepared_frames_without_identity_collisions() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    let leaf = p.layers.remove(0);
    p.compositions.push(Composition {
        id: "comp-warming-child".into(),
        name: "child".into(),
        width: 16,
        height: 16,
        fps: p.fps,
        frames: 10,
        work_area: WorkArea::full(10),
        guides: vec![],
        background: p.background,
        camera: p.camera.clone(),
        layers: vec![leaf],
        expressions: vec![],
    });
    for (id, start) in [(17, 3), (18, 4)] {
        let mut layer = Layer::solid(id, "reference", [16.; 2], [8., 8., 0.], [1.; 4]);
        layer.content = Content::Composition {
            clip: CompositionClip::new("comp-warming-child".into()),
        };
        layer.timeline = Some(LayerTimeline {
            in_frame: start,
            out_frame: 10,
            offset_frame: start,
        });
        p.layers.push(layer);
    }
    let control = Arc::new(Control::default());
    let mut reader = VideoFrames::new(Arc::new(Backend(control)));
    let blank = scene(&p, 0.);
    reader.prepare_scene(&p, root.path(), &blank, 0.).unwrap();
    assert_eq!(reader.streams.len(), 2);
    wait(|| {
        reader
            .streams
            .values()
            .all(|s| s.shared.0.lock().unwrap().cache.find(0).is_some())
    });
    let cached: HashMap<_, _> = reader
        .streams
        .iter()
        .map(|(id, s)| (*id, s.shared.0.lock().unwrap().cache.find(0).unwrap()))
        .collect();
    let first = scene(&p, 3.);
    let output = reader
        .prepare_scene(&p, root.path(), &first, 3.)
        .unwrap()
        .expect("first nested reference was warmed");
    assert_eq!(output.len(), 1);
    let first_id = output[0].0;
    assert!(Arc::ptr_eq(&cached[&first_id], &output[0].1.left));
    let second_id = *cached.keys().find(|id| **id != first_id).unwrap();
    wait(|| {
        reader.streams[&first_id]
            .shared
            .0
            .lock()
            .unwrap()
            .cache
            .find(70_000)
            .is_some()
    });
    let second = scene(&p, 4.);
    let output = reader
        .prepare_scene(&p, root.path(), &second, 4.)
        .unwrap()
        .expect("second nested reference was warmed");
    assert_eq!(output.len(), 2);
    let frame = &output
        .iter()
        .find(|(id, _)| *id == second_id)
        .unwrap()
        .1
        .left;
    assert!(Arc::ptr_eq(&cached[&second_id], frame));
}

#[test]
fn failed_future_native_decode_does_not_retry_or_poison_current_pixels() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    p.layers[0].time_mapping = Some(remap(&[(0., 2.)]));
    let control = Arc::new(Control::default());
    let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
    let current = scene(&p, 2.);
    ready(&mut reader, &p, root.path(), &current);
    control.fail.store(70_000, Ordering::Release);
    entering(&mut p, 8, 4);
    p.layers[1].time_mapping = Some(remap(&[(0., 1.)]));
    reader.prepare_scene(&p, root.path(), &current, 2.).unwrap();
    wait(|| reader.streams[&8].shared.0.lock().unwrap().error.is_some());
    for _ in 0..120 {
        assert!(reader
            .prepare_scene(&p, root.path(), &current, 2.)
            .unwrap()
            .is_some());
    }
    assert_eq!(
        control
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|p| **p == 70_000)
            .count(),
        1
    );
    let next = scene(&p, 4.);
    let error = reader
        .prepare_scene(&p, root.path(), &next, 4.)
        .err()
        .unwrap();
    assert!(error
        .to_string()
        .contains("controlled mapped speculative failure"));
    reader.finish().unwrap();
}
