//! Byte-for-byte protocol and allocation checks against the pre-change writer.
#[path = "support/composition_bundle_legacy.rs"]
mod legacy;

use motion_core::{
    Camera, Composition, CompositionClip, Content, EffectInstance, Layer, Project, VideoAsset,
    VideoClip,
};
use motion_render::{
    composition_plan::{self, CompositionPlanError},
    effect_plan::PlanBuilder,
    RenderError, ResourceBudgetScope, ResourceRejection, Scene,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    error::Error,
};

struct ObservedAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
fn observe() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
// SAFETY: unchanged pointer/layout/size are delegated to System. The observation
// is a const-initialized thread-local Cell and does not allocate or acquire locks.
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        observe();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        observe();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        observe();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;
fn allocations(body: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|count| count.set(None));
        }
    }
    let reset = Reset;
    ALLOCATIONS.with(|count| assert_eq!(count.replace(Some(0)), None));
    body();
    let count = ALLOCATIONS.with(|count| count.get().unwrap());
    drop(reset);
    count
}

fn reference(id: u64, target: &str, start: i32) -> Layer {
    let mut layer = Layer::solid(id, "Reference", [64.; 2], [32., 32., 0.], [1.; 4]);
    let mut clip = CompositionClip::new(target.into());
    clip.source_start_frame = start;
    layer.content = Content::Composition { clip };
    layer
}
fn fixture() -> Project {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.layers = vec![
        reference(11, "comp-wrapper", 0),
        reference(22, "comp-wrapper", 5),
    ];
    project.video_assets.push(VideoAsset {
        id: 1,
        path: "assets/video.mp4".into(),
        bytes: 1,
        mime: "video/avc".into(),
        track: 0,
        width: 16,
        height: 32,
        rotation: 90,
        display_width: 32,
        display_height: 16,
        video_start_us: 0,
        video_end_us: 10_000_000,
        duration_us: 10_000_000,
        frame_count: 600,
        variable_frame_rate: false,
        nominal_frame_rate: 60.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    let videos = (1..=2)
        .map(|id| {
            let mut layer = Layer::solid(id, "Video", [32., 16.], [32., 32., 0.], [1.; 4]);
            layer.content = Content::Video {
                video: VideoClip::new(1),
            };
            layer
        })
        .collect();
    for (id, fps, layers) in [
        (
            "comp-wrapper",
            30,
            vec![reference(1, "comp-leaf", 0), reference(2, "comp-leaf", 3)],
        ),
        ("comp-leaf", 60, videos),
    ] {
        project.compositions.push(Composition {
            id: id.into(),
            name: id.into(),
            width: 64,
            height: 64,
            fps,
            frames: 600,
            work_area: motion_core::WorkArea::full(600),
            guides: Vec::new(),
            background: [0.; 4],
            camera: Camera::new(64, 64),
            layers,
            expressions: vec![],
        });
    }
    project.validate().unwrap();
    project
}
fn sample(project: &Project, frame: f64) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn builder() -> PlanBuilder {
    PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap()
}
fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn long(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

#[test]
fn mixed_rate_shared_subtrees_keep_every_byte_parent_slot_and_video_binding() {
    let project = fixture();
    let (mut old, mut new) = (builder(), builder());
    let (mut expected, mut actual) = (vec![], vec![]);
    for frame in [0., 1.25, 17., 119., 3.5, 0.] {
        let scene = sample(&project, frame);
        legacy::build(&mut old, &scene, &project, &[0], &mut expected).unwrap();
        composition_plan::build(&mut new, &scene, &project, &[0], &mut actual).unwrap();
        assert_eq!(actual, expected, "frame={frame}");
        assert_eq!((word(&actual, 0), word(&actual, 4)), (0x4243534d, 1));
        assert_eq!(word(&actual, 12) as usize, actual.len());
        assert_eq!((word(&actual, 8), word(&actual, 16)), (7, 6));
        assert_eq!(
            (word(&actual, 20), word(&actual, 24), word(&actual, 28)),
            (32, 80, 40)
        );
        // Two leaves under each wrapper, followed by their wrapper and root.
        for (index, parent) in [2, 2, 6, 5, 5, 6, u32::MAX].into_iter().enumerate() {
            assert_eq!(word(&actual, 32 + index * 80 + 8), parent);
        }
        fn verify(scene: &Scene, bytes: &[u8], index: &mut usize, reference: u64, slot: i32) {
            for child in &scene.nested {
                let layer = scene.layers.iter().find(|l| l.id == child.layer).unwrap();
                verify(
                    &child.scene,
                    bytes,
                    index,
                    child.layer,
                    -(layer.order as i32 + 1),
                );
            }
            let at = 32 + *index * 80;
            assert_eq!(long(bytes, at), reference);
            assert_eq!(word(bytes, at + 12) as i32, slot);
            assert_eq!(long(bytes, at + 56), scene.frame.to_bits());
            assert_eq!(word(bytes, at + 64), scene.fps);
            let videos = scene
                .layers
                .iter()
                .filter_map(|l| l.video.as_ref().map(|s| (l, s)));
            assert_eq!(word(bytes, at + 52) as usize, videos.clone().count());
            for (j, (layer, video)) in videos.enumerate() {
                let video_at = word(bytes, at + 48) as usize + j * 40;
                assert_eq!(long(bytes, video_at), layer.id);
                assert_eq!(long(bytes, video_at + 8), 1);
                assert_eq!(long(bytes, video_at + 16), video.source_time_us);
                assert_eq!(word(bytes, video_at + 24) as i32, -(layer.order as i32 + 1));
                assert_eq!(
                    (word(bytes, video_at + 28), word(bytes, video_at + 32)),
                    (32, 16)
                );
                assert_eq!(word(bytes, video_at + 36), 0);
            }
            *index += 1;
        }
        let mut index = 0;
        verify(&scene, &actual, &mut index, 0, 0);
        assert_eq!(index, 7);
    }
}

#[test]
fn warm_bundle_allocations_drop_without_changing_output() {
    let project = fixture();
    let scene = sample(&project, 10.5);
    let (mut old, mut new) = (builder(), builder());
    let (mut expected, mut actual) = (vec![], vec![]);
    legacy::build(&mut old, &scene, &project, &[0], &mut expected).unwrap();
    composition_plan::build(&mut new, &scene, &project, &[0], &mut actual).unwrap();
    let old_count = allocations(|| {
        legacy::build(&mut old, &scene, &project, &[0], &mut expected).unwrap();
    });
    let new_count = allocations(|| {
        composition_plan::build(&mut new, &scene, &project, &[0], &mut actual).unwrap();
    });
    assert_eq!(actual, expected);
    // Three non-leaf nodes and four video-containing leaves no longer create
    // temporary vectors. Other allocations in the plan builder remain in scope.
    assert!(
        old_count >= new_count + 7,
        "old={old_count} new={new_count}"
    );
    eprintln!("composition bundle allocations old={old_count} new={new_count} nodes=7 videos=8");
}

#[test]
fn missing_reference_and_asset_return_typed_errors_and_allow_recovery() {
    let project = fixture();
    let mut scene = sample(&project, 0.);
    let mut builder = builder();
    let mut bytes = vec![7; 32];
    let reference = scene.nested[0].layer;
    scene.nested[0].layer = u64::MAX;
    let error =
        composition_plan::build(&mut builder, &scene, &project, &[0], &mut bytes).unwrap_err();
    assert!(
        matches!(error, RenderError::CompositionPlan(CompositionPlanError::MissingReference {
        ref composition, layer: u64::MAX,
    }) if composition == "comp-main")
    );
    assert!(bytes.is_empty());
    scene.nested[0].layer = reference;
    let mut missing = project.clone();
    missing.video_assets.clear();
    let expected = legacy::build(&mut builder, &scene, &missing, &[0], &mut bytes).unwrap_err();
    let error =
        composition_plan::build(&mut builder, &scene, &missing, &[0], &mut bytes).unwrap_err();
    assert_eq!(error.to_string(), expected);
    assert!(
        matches!(error, RenderError::CompositionPlan(CompositionPlanError::MissingVideoAsset {
        ref composition, layer, asset: 1,
    }) if composition == "comp-leaf" && layer == scene.nested[0].scene.nested[0].scene.layers[0].id)
    );
    assert_eq!(error.resource_rejection(), None);
    composition_plan::build(&mut builder, &scene, &project, &[0], &mut bytes).unwrap();
    assert_eq!(word(&bytes, 0), composition_plan::MAGIC);
}

#[test]
fn nested_budget_refusal_keeps_original_source_identity_text_and_recovery() {
    let mut project = fixture();
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "gaussian_blur")
        .unwrap();
    let video = &mut project.compositions[1].layers[0];
    *video = Layer::solid(1, "Blur", [3840., 2160.], [32., 32., 0.], [1.; 4]);
    video.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        video.size,
    ));
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let scene = sample(&project, 0.);
    let (mut old, mut builder) = (builder(), builder());
    let mut bytes = vec![];
    let expected = legacy::build(&mut old, &scene, &project, &[0], &mut bytes).unwrap_err();
    let error =
        composition_plan::build(&mut builder, &scene, &project, &[0], &mut bytes).unwrap_err();
    assert_eq!(error.to_string(), expected);
    assert!(expected.starts_with("composition comp-leaf:"));
    assert!(
        matches!(error.resource_rejection(), Some(ResourceRejection::Budget {
        scope: ResourceBudgetScope::EffectScratch, required_bytes, budget_bytes,
    }) if required_bytes > budget_bytes)
    );
    let source = error
        .source()
        .unwrap()
        .downcast_ref::<Box<RenderError>>()
        .unwrap()
        .as_ref();
    let RenderError::Diagnostic { cause, .. } = source else { panic!("{source:?}") };
    assert!(matches!(cause.as_ref(), RenderError::Resource { .. }));
    assert!(std::ptr::eq(
        error.resource_rejection().unwrap(),
        source.resource_rejection().unwrap()
    ));
    let healthy = fixture();
    composition_plan::build(
        &mut builder,
        &sample(&healthy, 0.),
        &healthy,
        &[0],
        &mut bytes,
    )
    .unwrap();
    assert_eq!(builder.last_resource_rejection, None);
}

#[test]
fn active_instance_limit_is_typed_and_clears_previous_output() {
    let project = fixture();
    let mut scene = sample(&project, 0.);
    scene.nested =
        vec![scene.nested[0].clone(); motion_core::composition::MAX_RENDER_COMPOSITION_INSTANCES];
    let mut bytes = vec![7; 256];
    let error =
        composition_plan::build(&mut builder(), &scene, &project, &[0], &mut bytes).unwrap_err();
    assert!(matches!(
        error,
        RenderError::CompositionPlan(CompositionPlanError::InstanceLimit { .. })
    ));
    assert_eq!(
        error.to_string(),
        "composition frame exceeds depth or active instance limit"
    );
    assert!(bytes.is_empty());
}

#[test]
fn depth_and_byte_limits_keep_the_existing_text_and_typed_measurements() {
    let project = Project::new(64, 64, 30, 120).unwrap();
    let mut scene = sample(&project, 0.);
    scene.curve_luts = vec![[[0.; 4]; 256]; composition_plan::MAX_BYTES / 1024];
    let mut bytes = vec![];
    let mut builder = builder();
    let expected = legacy::build(&mut builder, &scene, &project, &[0], &mut bytes).unwrap_err();
    let error =
        composition_plan::build(&mut builder, &scene, &project, &[0], &mut bytes).unwrap_err();
    assert_eq!(error.to_string(), expected);
    assert!(
        matches!(error, RenderError::CompositionPlan(CompositionPlanError::BundleSize {
        required_bytes, budget_bytes: composition_plan::MAX_BYTES,
    }) if required_bytes > composition_plan::MAX_BYTES)
    );
    assert!(bytes.len() < composition_plan::MAX_BYTES);

    let project = fixture();
    let mut base = sample(&project, 0.);
    base.nested.clear();
    let mut scene = base.clone();
    for _ in 0..motion_core::composition::MAX_COMPOSITION_DEPTH {
        let mut parent = base.clone();
        parent.nested.push(motion_render::NestedScene {
            layer: parent.layers[0].id,
            composition: scene.composition_id.clone(),
            scene: Box::new(scene),
        });
        scene = parent;
    }
    let error =
        composition_plan::build(&mut builder, &scene, &project, &[0], &mut bytes).unwrap_err();
    assert!(matches!(
        error,
        RenderError::CompositionPlan(CompositionPlanError::InstanceLimit {
            depth: motion_core::composition::MAX_COMPOSITION_DEPTH,
            ..
        })
    ));
    assert!(bytes.is_empty());
}
