use super::*;
use std::{sync::mpsc, time::Duration};

fn scene(count: u64) -> Scene {
    let mut project = motion_model::Project::new(64, 64, 30, 60).unwrap();
    for id in 1..=count {
        let mut layer = motion_model::Layer::solid(id, "vector", [20.; 2], [32., 32., 0.], [1.; 4]);
        layer.content = motion_model::Content::Vector {
            vector: motion_model::vector::VectorContent::shape(
                motion_model::vector::ShapeKind::Rectangle,
            ),
        };
        project.layers.push(layer);
    }
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
fn bounded_batch_overlaps_work_and_returns_input_order() {
    let (entered, incoming) = mpsc::channel();
    let mut release = Vec::new();
    let jobs: Vec<_> = (0..MAX_WORKERS)
        .map(|i| {
            let (tx, rx) = mpsc::channel();
            release.push(tx);
            (i, rx)
        })
        .collect();
    let task = std::thread::spawn(move || {
        ordered_batch(jobs, |(i, rx)| {
            entered.send(i).unwrap();
            (i, rx.recv_timeout(Duration::from_secs(5)).is_ok())
        })
        .unwrap()
    });
    let entered: Vec<_> = (0..MAX_WORKERS)
        .map(|_| incoming.recv_timeout(Duration::from_secs(5)))
        .collect();
    for tx in release {
        let _ = tx.send(());
    }
    let output = task.join().unwrap();
    assert!(
        entered.iter().all(Result::is_ok),
        "all jobs must enter before any is released: {entered:?}"
    );
    assert_eq!(
        output,
        (0..MAX_WORKERS).map(|i| (i, true)).collect::<Vec<_>>()
    );
    assert!(ordered_batch(vec![0; MAX_WORKERS + 1], |i| i).is_err());
    assert!(ordered_batch(Vec::<usize>::new(), |i| i)
        .unwrap()
        .is_empty());
}

#[test]
fn worker_panics_are_joined_and_the_original_payload_is_resumed() {
    let completed = std::sync::atomic::AtomicUsize::new(0);
    let error = catch_unwind(AssertUnwindSafe(|| {
        ordered_batch(vec![0, 1, 2], |i| {
            if i == 1 {
                std::panic::panic_any(731usize);
            }
            completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            i
        })
    }))
    .unwrap_err();
    assert_eq!(error.downcast_ref::<usize>(), Some(&731));
    assert_eq!(completed.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn parallel_meshes_match_serial_geometry_and_warm_cache_does_no_work() {
    let mut scene = scene(7);
    for (i, layer) in scene.layers.iter_mut().enumerate() {
        layer.source_rect[0] += i as f32;
        layer.source_rect[1] -= i as f32;
    }
    let mut serial = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    let mut parallel = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    serial.cpu_preparation.worker_limit = 1;
    parallel.cpu_preparation.worker_limit = MAX_WORKERS;
    serial.build(&scene, &[0], 64, 64, true).unwrap();
    parallel.build(&scene, &[0], 64, 64, true).unwrap();
    assert_eq!(parallel.cpu_preparation_info().vector_jobs, 7);
    assert_eq!(parallel.cpu_preparation_info().largest_batch, MAX_WORKERS);
    for (a, b) in serial.frame.vectors.iter().zip(&parallel.frame.vectors) {
        assert_eq!(
            (
                a.layer,
                a.width,
                a.height,
                a.fingerprint,
                a.origin,
                a.root_opacity
            ),
            (
                b.layer,
                b.width,
                b.height,
                b.fingerprint,
                b.origin,
                b.root_opacity
            )
        );
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.vertices),
            bytemuck::cast_slice::<_, u8>(&b.vertices)
        );
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.commands),
            bytemuck::cast_slice::<_, u8>(&b.commands)
        );
    }
    let old: Vec<_> = parallel
        .frame
        .vectors
        .iter()
        .map(|mesh| mesh.vertices.clone())
        .collect();
    parallel.build(&scene, &[0], 64, 64, true).unwrap();
    let info = parallel.cpu_preparation_info();
    assert_eq!(
        (
            info.vector_jobs,
            info.vector_cache_hits,
            info.largest_batch,
            info.peak_pending_vertex_bytes
        ),
        (0, 7, 0, 0)
    );
    for (old, mesh) in old.iter().zip(&parallel.frame.vectors) {
        assert!(Arc::ptr_eq(old, &mesh.vertices));
    }
}

#[test]
fn admission_keeps_scene_order_and_does_not_cache_rejected_results() {
    for workers in [1, MAX_WORKERS] {
        let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
        builder.cpu_preparation.worker_limit = workers;
        builder.build(&scene(2), &[0], 64, 64, true).unwrap();
        for id in [1, 2] {
            let mesh = &mut builder.vector_cache.get_mut(&id).unwrap().3;
            mesh.vertices = Arc::new(mesh.vertices.repeat(21845));
        }
        assert!(builder.build(&scene(5), &[0], 64, 64, true).is_err());
        assert_eq!(builder.frame.vectors.len(), 2);
        assert_eq!(builder.vector_cache.len(), 2);
        assert!(
            builder.cpu_preparation_info().peak_pending_vertex_bytes
                <= MAX_WORKERS
                    * MAX_TRIANGLE_VERTICES
                    * std::mem::size_of::<crate::vector_mesh::VectorVertex>()
        );
        builder.vector_cache.clear();
        builder.build(&scene(5), &[0], 64, 64, true).unwrap();
        assert_eq!(builder.frame.vectors.len(), 5);
    }
}

#[test]
fn later_setup_and_geometry_failures_preserve_earlier_cache_admission_and_recover() {
    for setup in [false, true] {
        let mut scene = scene(4);
        if setup {
            scene.layers[1].size = [100_000.; 2];
        } else {
            Arc::make_mut(scene.layers[1].vector.as_mut().unwrap()).paths[0].nodes[0][0] = f32::NAN;
        }
        let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
        builder.cpu_preparation.worker_limit = MAX_WORKERS;
        let error = builder.build(&scene, &[0], 64, 64, true).unwrap_err();
        assert!(error.to_string().contains("layer 2"), "{error:?}");
        assert_eq!(builder.frame.vectors.len(), 1);
        assert_eq!(builder.vector_cache.len(), 1);
        assert!(builder.vector_cache.contains_key(&1));
        builder.build(&self::scene(4), &[0], 64, 64, true).unwrap();
        assert_eq!(builder.cpu_preparation_info().vector_cache_hits, 1);
        assert_eq!(builder.frame.vectors.len(), 4);
    }
}
