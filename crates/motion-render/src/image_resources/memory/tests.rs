use super::*;
use crate::image_resources::{self as images, Error, Resolution, Source};
use std::{sync::mpsc, thread, time::Duration};

pub(super) enum Action {
    Continue,
    Panic,
}
pub(super) struct Control {
    phase: &'static str,
    entered: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<Action>>,
}
impl Control {
    pub(super) fn checkpoint(&self, phase: &'static str) {
        if self.phase == phase {
            let _ = self.entered.send(());
            let action = self.release.lock().unwrap().recv();
            if matches!(action, Ok(Action::Panic)) {
                panic!("authored decode allocation panic");
            }
        }
    }
}
struct Gate {
    entered: mpsc::Receiver<()>,
    release: mpsc::Sender<Action>,
}
impl Gate {
    fn wait(&self) {
        self.entered.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    fn release(&self) {
        let _ = self.release.send(Action::Continue);
    }
}
fn paused(
    source: Source,
    mode: Resolution,
    phase: &'static str,
) -> (DecodeTask, Gate, Arc<Allocations>) {
    let (entered, received) = mpsc::sync_channel(1);
    let (release, resume) = mpsc::channel();
    let allocations = Arc::new(Allocations {
        control: Some(Control {
            phase,
            entered,
            release: Mutex::new(resume),
        }),
        ..Default::default()
    });
    let memory = TaskMemory {
        allocations: Arc::clone(&allocations),
        began: Instant::now(),
    };
    let mut task = DecodeTask::default();
    task.start_with_memory(source, mode, false, images::MAX_DECODED_BYTES, memory)
        .unwrap();
    (
        task,
        Gate {
            entered: received,
            release,
        },
        allocations,
    )
}
fn source(root: &std::path::Path, name: &str, width: u32, height: u32) -> Source {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    let path = root.join("assets").join(name);
    if name.ends_with("jpg") {
        image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x * 17) as u8, (y * 23) as u8, 91])
        })
        .save(&path)
        .unwrap();
    } else {
        image::RgbaImage::from_pixel(width, height, image::Rgba([37, 73, 109, 255]))
            .save(&path)
            .unwrap();
    }
    Source::new(
        root,
        &motion_model::Asset {
            id: 17,
            path: format!("assets/{name}"),
            width,
            height,
        },
    )
    .unwrap()
}
fn result(task: &mut DecodeTask) -> Result<Pixels, Error> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some((_, _, _, result)) = task.poll() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "actual decode worker did not finish"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
fn actual_owned(a: &Allocations) -> u64 {
    a.state
        .lock()
        .unwrap()
        .slots
        .iter()
        .filter_map(|s| s.capacity)
        .sum()
}

#[test]
fn real_output_is_published_before_decode_and_transfers_without_pixel_changes_or_retention() {
    let root = tempfile::tempdir().unwrap();
    let input = source(root.path(), "full.png", 3, 2);
    let oracle = images::decode(&input, Resolution::Full).unwrap();
    let (mut task, gate, allocations) = paused(input, Resolution::Full, "output");
    gate.wait();
    let observed = task.memory_info();
    assert_eq!(observed.known_payload_bytes, 24);
    assert!(observed.known_capacity_bytes >= 24);
    assert_eq!(observed.details["pending"], true);
    assert_eq!(observed.details["source"], 17);
    assert_eq!(observed.details["phase"], "decoding");
    assert_eq!(observed.details["buffers"].as_array().unwrap().len(), 1);
    assert!(task.poll().is_none(), "observation must precede completion");
    gate.release();
    let pixels = result(&mut task).unwrap();
    assert_eq!(pixels.rgba, oracle.rgba);
    assert_eq!(observed.known_capacity_bytes, pixels.rgba.capacity() as u64);
    assert_eq!(task.memory_info().known_capacity_bytes, 0);
    assert_eq!(actual_owned(&allocations), 0);
    assert_eq!(
        pixels.rgba, oracle.rgba,
        "escaped result remains valid outside task accounting"
    );
}

#[test]
fn queued_real_pixels_survive_cancel_until_poll_and_query_never_reads_source_or_drains_result() {
    let root = tempfile::tempdir().unwrap();
    let input = source(root.path(), "queued.png", 4, 4);
    let old = input.path.clone();
    let (mut task, gate, allocations) = paused(input, Resolution::Full, "after_send");
    gate.wait();
    std::fs::rename(old, root.path().join("assets/moved.png")).unwrap();
    let before = task.memory_info();
    assert_eq!(before.known_payload_bytes, 64);
    assert_eq!(before.details["phase"], "completed_output_retained");
    task.cancel();
    let cancelled = task.memory_info();
    assert_eq!(cancelled.known_capacity_bytes, before.known_capacity_bytes);
    assert_eq!(cancelled.details["cancelRequested"], true);
    assert!(task.busy());
    assert!(matches!(result(&mut task), Err(Error::Cancelled)));
    assert_eq!(actual_owned(&allocations), 0);
    assert_eq!(task.memory_info().details["pending"], false);
    gate.release();
}

#[test]
fn real_png_proxy_and_jpeg_raster_keep_separate_actual_scratch_until_decode_finishes() {
    for (name, jpeg) in [("proxy.png", false), ("full.jpg", true)] {
        let root = tempfile::tempdir().unwrap();
        let input = source(root.path(), name, 8, 8);
        // Full-resolution oracle avoids populating the proxy cache before the
        // worker exercises the real downsampling allocation path.
        let oracle = if jpeg {
            Some(images::decode(&input, Resolution::Preview(2)).unwrap())
        } else {
            None
        };
        if jpeg {
            // Remove only this test's disposable proxy so the JPEG codec path,
            // rather than a cache hit, runs during the controlled worker.
            if let Some(path) = images::proxy_path(&input, 2, 2, None) {
                let _ = std::fs::remove_file(path);
            }
        }
        let (mut task, gate, a) = paused(input, Resolution::Preview(2), "accumulator");
        gate.wait();
        let info = task.memory_info();
        let buffers = info.details["buffers"].as_array().unwrap();
        assert_eq!(buffers.len(), if jpeg { 3 } else { 2 });
        let output = buffers.iter().find(|b| b["kind"] == "output").unwrap();
        let accumulator = buffers.iter().find(|b| b["kind"] == "accumulator").unwrap();
        assert_eq!(output["payloadBytes"], 16);
        assert_eq!(
            accumulator["payloadBytes"],
            2 * std::mem::size_of::<[f32; 4]>()
        );
        assert_eq!(
            info.known_payload_bytes,
            16 + 32 + if jpeg { 8 * 8 * 3 } else { 0 }
        );
        assert!(info.known_capacity_bytes >= info.known_payload_bytes);
        assert!(info.details["codecInternalBytes"].is_null());
        gate.release();
        let pixels = result(&mut task).unwrap();
        if let Some(oracle) = oracle {
            assert_eq!(pixels.rgba, oracle.rgba);
        } else {
            assert_eq!(pixels.rgba, [37, 73, 109, 255].repeat(4));
        }
        assert_eq!(actual_owned(&a), 0);
    }
}

#[test]
fn real_cached_proxy_output_keeps_its_lease_through_completion_without_duplicate_storage() {
    let root = tempfile::tempdir().unwrap();
    let input = source(root.path(), "cached.png", 8, 8);
    let oracle = images::decode(&input, Resolution::Preview(2)).unwrap();
    let (mut task, gate, a) = paused(input, Resolution::Preview(2), "output");
    gate.wait();
    let info = task.memory_info();
    assert_eq!(info.known_payload_bytes, 16);
    assert_eq!(info.details["buffers"].as_array().unwrap().len(), 1);
    gate.release();
    let pixels = result(&mut task).unwrap();
    assert!(pixels.cached);
    assert_eq!(pixels.rgba, oracle.rgba);
    assert_eq!(info.known_capacity_bytes, pixels.rgba.capacity() as u64);
    assert_eq!(actual_owned(&a), 0);
}

#[test]
fn cancellation_panic_and_disconnected_receiver_release_real_worker_buffers_at_actual_drop() {
    for phase in ["cancel", "panic", "disconnect"] {
        let root = tempfile::tempdir().unwrap();
        let input = source(root.path(), "unwind.png", 4, 4);
        let checkpoint = if phase == "disconnect" {
            "before_send"
        } else {
            "output"
        };
        let (mut task, gate, a) = paused(input, Resolution::Full, checkpoint);
        gate.wait();
        assert_eq!(task.memory_info().known_payload_bytes, 64);
        match phase {
            "cancel" => {
                task.cancel();
                assert!(
                    task.memory_info().known_capacity_bytes >= 64,
                    "cancel is not release"
                );
                gate.release();
                assert!(matches!(result(&mut task), Err(Error::Cancelled)));
            }
            "panic" => {
                let _ = gate.release.send(Action::Panic);
                assert!(
                    matches!(result(&mut task),Err(Error::WorkerPanic(reason)) if reason=="authored decode allocation panic")
                );
            }
            _ => {
                drop(task);
                gate.release();
            }
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while actual_owned(&a) != 0 {
            assert!(
                Instant::now() < deadline,
                "actual worker storage remained retained"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
}

#[test]
fn independent_same_source_tasks_keep_distinct_allocations_and_escaped_results() {
    let root = tempfile::tempdir().unwrap();
    let input = source(root.path(), "independent.png", 4, 4);
    let (mut one, g1, a1) = paused(input.clone(), Resolution::Full, "after_send");
    let (mut two, g2, a2) = paused(input, Resolution::Full, "after_send");
    g1.wait();
    g2.wait();
    let capacities =
        one.memory_info().known_capacity_bytes + two.memory_info().known_capacity_bytes;
    let p1 = result(&mut one).unwrap();
    let p2 = result(&mut two).unwrap();
    assert_eq!(capacities, (p1.rgba.capacity() + p2.rgba.capacity()) as u64);
    assert_eq!(p1.rgba, p2.rgba);
    assert_ne!(p1.rgba.as_ptr(), p2.rgba.as_ptr());
    assert_eq!(actual_owned(&a1) + actual_owned(&a2), 0);
    g1.release();
    g2.release();
}

#[test]
fn contended_observation_is_unknown_nonblocking_and_recovers_without_consuming_pixels() {
    let root = tempfile::tempdir().unwrap();
    let input = source(root.path(), "busy.png", 4, 4);
    let (task, gate, a) = paused(input, Resolution::Full, "after_send");
    gate.wait();
    let (entered, held) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let owner = Arc::clone(&a);
    let holder = thread::spawn(move || {
        let _guard = owner.state.lock().unwrap();
        let _ = entered.send(());
        let _ = resume.recv();
    });
    held.recv_timeout(Duration::from_secs(5)).unwrap();
    let (send, receive) = mpsc::channel();
    let query = thread::spawn(move || {
        let info = task.memory_info();
        let _ = send.send((task, info));
    });
    let first = receive.recv_timeout(Duration::from_secs(5));
    let nonblocking = first.is_ok();
    let _ = release.send(());
    holder.join().unwrap();
    let (mut task, info) =
        first.unwrap_or_else(|_| receive.recv_timeout(Duration::from_secs(5)).unwrap());
    query.join().unwrap();
    assert!(nonblocking, "query waited for the observation lock");
    assert_eq!(info.details["unavailable"], "busy");
    assert_eq!(info.details["allInstrumentedBuffersObserved"], false);
    assert!(info.details["unknownBytes"].is_null());
    assert_eq!(task.memory_info().known_payload_bytes, 64);
    assert_eq!(
        result(&mut task).unwrap().rgba,
        [37, 73, 109, 255].repeat(16)
    );
    gate.release();
}

#[test]
fn real_spare_capacity_and_allocating_unknown_follow_unwind_and_channel_ownership() {
    let a = Arc::new(Allocations::default());
    let tracked = allocate(Some(&a), Kind::Output, || {
        let allocating = a.state.lock().unwrap().slots[Kind::Output as usize];
        assert!(allocating.active && allocating.allocating && allocating.capacity.is_none());
        let mut bytes = Vec::<u8>::with_capacity(97);
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        Ok::<_, Error>(bytes)
    })
    .unwrap();
    assert_eq!(actual_owned(&a), tracked.capacity() as u64);
    assert!(actual_owned(&a) > tracked.len() as u64);
    let (send, receive) = mpsc::sync_channel(1);
    send.send(tracked).ok().unwrap();
    assert!(actual_owned(&a) >= 97);
    drop(receive);
    assert_eq!(
        actual_owned(&a),
        0,
        "dropping queued real Vec must drop its lease"
    );
    let unwind = std::panic::catch_unwind(|| {
        let _storage = allocate(Some(&a), Kind::Accumulator, || {
            Ok::<_, Error>(vec![[1f32; 4]; 3])
        })
        .unwrap();
        assert_eq!(actual_owned(&a), 48);
        panic!("authored scratch unwind");
    });
    assert!(unwind.is_err());
    assert_eq!(actual_owned(&a), 0);
}

#[test]
fn actual_adam7_full_and_proxy_accumulators_keep_pixels_identical_to_the_existing_decoder() {
    for mode in [Resolution::Full, Resolution::Preview(3)] {
        let root = tempfile::tempdir().unwrap();
        let input = source(root.path(), "adam7.png", 7, 5);
        std::fs::write(
            &input.path,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/image-resources-adam7.png"
            )),
        )
        .unwrap();
        let oracle = images::decode(&input, mode).unwrap();
        if mode != Resolution::Full {
            if let Some(path) = images::proxy_path(&input, oracle.width, oracle.height, None) {
                std::fs::remove_file(path).unwrap();
            }
        }
        let (mut task, gate, a) = paused(input, mode, "accumulator");
        gate.wait();
        let info = task.memory_info();
        let expected_accumulator = if mode == Resolution::Full {
            0
        } else {
            (oracle.width * oracle.height) as u64 * 16
        };
        assert_eq!(
            info.known_payload_bytes,
            oracle.rgba.len() as u64 + expected_accumulator
        );
        assert_eq!(
            info.details["buffers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["kind"] == "accumulator")
                .unwrap()["payloadBytes"],
            expected_accumulator
        );
        gate.release();
        assert_eq!(result(&mut task).unwrap().rgba, oracle.rgba);
        assert_eq!(actual_owned(&a), 0);
    }
}

#[test]
fn actual_decode_failure_keeps_its_native_cause_and_releases_output_without_poll_side_effects() {
    let root = tempfile::tempdir().unwrap();
    let input = source(root.path(), "broken.png", 4, 4);
    let (mut task, gate, a) = paused(input.clone(), Resolution::Full, "output");
    gate.wait();
    std::fs::write(&input.path, b"\x89PNG\r\n\x1a\n").unwrap();
    let expected = images::decode(&input, Resolution::Full).err().unwrap();
    assert!(actual_owned(&a) >= 64);
    gate.release();
    let actual = result(&mut task).err().unwrap();
    assert_eq!(
        std::mem::discriminant(&actual),
        std::mem::discriminant(&expected)
    );
    assert_eq!(actual.to_string(), expected.to_string());
    assert_eq!(actual_owned(&a), 0);
}

#[test]
fn poisoned_observation_stays_unknown_but_does_not_change_real_pixels_or_new_task_recovery() {
    let root = tempfile::tempdir().unwrap();
    let input = source(root.path(), "poison.png", 4, 4);
    let (mut task, gate, a) = paused(input.clone(), Resolution::Full, "after_send");
    gate.wait();
    let owner = Arc::clone(&a);
    assert!(thread::spawn(move || {
        let _guard = owner.state.lock().unwrap();
        panic!("authored observation poison");
    })
    .join()
    .is_err());
    let info = task.memory_info();
    assert_eq!(info.details["unavailable"], "poisoned_observation");
    assert_eq!(info.details["allInstrumentedBuffersObserved"], false);
    assert!(info.details["unknownBytes"].is_null());
    assert_eq!(
        result(&mut task).unwrap().rgba,
        [37, 73, 109, 255].repeat(16)
    );
    gate.release();
    let (mut next, g2, _) = paused(input, Resolution::Full, "after_send");
    g2.wait();
    assert_eq!(next.memory_info().known_payload_bytes, 64);
    assert_eq!(
        next.memory_info().details["allInstrumentedBuffersObserved"],
        true
    );
    assert_eq!(
        result(&mut next).unwrap().rgba,
        [37, 73, 109, 255].repeat(16)
    );
    g2.release();
}
