use motion_core::{Asset, Content, Layer, Project};
use motion_render::{
    image_resources::{scene_assets, Resolution},
    Renderer, Scene,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! { static COUNT: Cell<Option<usize>> = const { Cell::new(None) }; }
struct Allocator;
fn note() {
    let _ = COUNT.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value.saturating_add(1)));
        }
    });
}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            note();
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            note();
        }
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() {
            note();
        }
        next
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
fn allocations(work: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            let _ = COUNT.try_with(|count| count.set(None));
        }
    }
    COUNT.with(|count| {
        assert!(count.get().is_none());
        count.set(Some(0));
    });
    let reset = Reset;
    work();
    let count = COUNT.with(|count| count.get().unwrap());
    drop(reset);
    count
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn ready_image_preparation_does_not_clone_an_unchanged_allowed_set() {
    assert!(
        allocations(|| {
            black_box(Box::new(7u64));
        }) > 0
    );
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let mut project = Project::new(16, 16, 30, 30).unwrap();
    for id in 1..=64 {
        let path = format!("assets/image-{id}.png");
        image::save_buffer(
            root.path().join(&path),
            &[id as u8, 50, 160, 255].repeat(16 * 16),
            16,
            16,
            image::ColorType::Rgba8,
        )
        .unwrap();
        project.assets.push(Asset {
            id,
            path,
            width: 16,
            height: 16,
        });
        let mut layer = Layer::solid(id, "image", [16.; 2], [8., 8., 0.], [1.; 4]);
        layer.content = Content::Image { asset: id };
        project.layers.push(layer);
    }
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    assert!(renderer
        .prepare_scene_assets(&scene, Resolution::Full, false)
        .unwrap());
    let target = renderer.capture_target(16, 16).unwrap();
    let expected_pixels = renderer.capture(&scene, &target).unwrap().0;
    let before = (
        renderer.image_decodes,
        renderer.image_upload_bytes,
        renderer.texture_bytes(),
    );
    assert_eq!(scene_assets(&scene).len(), 64);
    // Preserve the earlier allowed-set budget, then independently bound the
    // unchanged active-set path by demand collection alone.
    let reference = allocations(|| {
        let wanted = scene_assets(&scene);
        black_box(wanted.clone());
        black_box(wanted);
    });
    let demand_only = allocations(|| {
        black_box(scene_assets(&scene));
    });
    assert!(
        demand_only > 0,
        "the allocation probe must observe demand collection"
    );
    let mut active_counts = Vec::new();
    for (resolution, asynchronous) in [(Resolution::Full, false), (Resolution::Preview(32), true)] {
        assert!(renderer
            .prepare_scene_assets(&scene, resolution, asynchronous)
            .unwrap());
        let count = allocations(|| {
            for _ in 0..20 {
                assert!(black_box(&mut renderer)
                    .prepare_scene_assets(&scene, resolution, asynchronous)
                    .unwrap());
            }
        });
        println!("64 ready images / 20 preparations {resolution:?} async={asynchronous}: allocations={count} reference_per_preparation={reference}");
        assert!(
            count <= reference * 20 + 20,
            "unchanged allowed-set allocation: {count}, demand/active reference={reference}"
        );
        active_counts.push((resolution, count));
        assert_eq!(
            (
                renderer.image_decodes,
                renderer.image_upload_bytes,
                renderer.texture_bytes()
            ),
            before
        );
        assert_eq!(renderer.image_idle_bytes(), 0);
        for id in 1..=64 {
            assert_eq!(renderer.image_dimensions(id), Some((16, 16)));
        }
        assert_eq!(
            renderer.capture(&scene, &target).unwrap().0,
            expected_pixels
        );
        renderer.check_health().unwrap();
    }
    for (resolution, count) in active_counts {
        assert!(
            count <= demand_only * 20,
            "unchanged active-set allocation {resolution:?}: {count}, demand-only reference={demand_only}"
        );
    }
}
