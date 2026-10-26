//! Real multi-group compiler invalidation and allocation-window measurements.
use motion_core::{Asset, Content, EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, WorkingSpace,
};
use motion_render::{
    AllocationBudget, AllocationFrameOutcome, FrameOperation, ImageCaptureTarget, Renderer, Scene,
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Instant};

#[allow(dead_code)]
#[path = "precision_golden.rs"]
mod reference;

pub const MODES: [ChannelPrecision; 4] = [
    ChannelPrecision::Unorm8,
    ChannelPrecision::Unorm16,
    ChannelPrecision::Float16,
    ChannelPrecision::Float32,
];

fn fixture(mode: ChannelPrecision, width: u32, height: u32) -> (Project, Vec<u8>) {
    let size = [width as f32, height as f32];
    let package = motion_effects::builtin::package().unwrap();
    let mut p = Project::new(width, height, 30, 60).unwrap();
    p.working_precision = mode;
    p.background = [0.; 4];
    p.assets.push(Asset {
        id: 1,
        path: "assets/generated-dag-grid.png".into(),
        width,
        height,
    });
    for id in 1..=2 {
        let mut layer = Layer::solid(
            id,
            "independent group",
            size,
            [size[0] / 2., size[1] / 2., 0.],
            [1.; 4],
        );
        layer.content = Content::Image { asset: 1 };
        layer.transform.opacity.value = if id == 1 { 0.6 } else { 1. };
        // This order has a proved transfer path in all four precisions.
        for (offset, name) in ["invert", "tint"].into_iter().enumerate() {
            let def = package
                .manifest
                .effects
                .iter()
                .find(|d| d.id == name)
                .unwrap();
            let mut e = EffectInstance::new(
                id * 10 + offset as u64,
                &package.manifest.id,
                &package.manifest.version,
                &package.hash,
                def,
                size,
            );
            if name == "invert" {
                e.params.get_mut("blend").unwrap().track.value[0] = 23.75;
            }
            if name == "tint" {
                e.params.get_mut("p0003").unwrap().track.value[0] = 68.25;
            }
            layer.effects.push(e);
        }
        p.layers.push(layer);
    }
    p.rebuild_plugin_dependencies();
    let pixels = (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                [
                    (x * 255 / (width - 1)) as u8,
                    (y * 255 / (height - 1)) as u8,
                    ((x * 43 + y * 71) % 256) as u8,
                    [0, 1, 63, 128, 255][(x % 5) as usize],
                ]
            })
        })
        .collect();
    (p, pixels)
}

struct Run {
    renderer: Renderer,
    target: ImageCaptureTarget,
    budget: AllocationBudget,
}
impl Run {
    fn new(mode: ChannelPrecision, width: u32, height: u32, fused: bool, pixels: &[u8]) -> Self {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let format = motion_render::image_formats::native_format(
            motion_render::image_formats::color_storage(mode),
        );
        let budget = AllocationBudget::new(512 << 20).unwrap();
        let mut renderer = pollster::block_on(Renderer::new_profiled_with_precision_changes(
            &instance,
            None,
            format,
            true,
            budget.clone(),
        ))
        .unwrap();
        renderer.set_pointwise_fusion(fused);
        renderer.upload_image(1, width, height, pixels).unwrap();
        let target = renderer
            .image_capture_target(
                width,
                height,
                ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Premultiplied,
                    },
                    mode,
                ),
            )
            .unwrap();
        Self {
            renderer,
            target,
            budget,
        }
    }
    fn capture(&mut self, scene: &Scene, request: u64) -> (Vec<[f64; 4]>, Value) {
        self.renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let before = self.budget.info().unwrap();
        let frame = self
            .budget
            .begin_frame(request, FrameOperation::Render)
            .unwrap()
            .unwrap();
        let began = Instant::now();
        let stats = self
            .renderer
            .draw(scene, self.target.view(), scene.width, scene.height)
            .unwrap();
        let submit_wall_ns = began.elapsed().as_nanos();
        let image = self.renderer.read_image_target(&self.target).unwrap();
        let submit_readback_wall_ns = began.elapsed().as_nanos();
        frame.finish(AllocationFrameOutcome::Rendered);
        self.renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let peaks = self.budget.frame_peaks().unwrap();
        assert!(peaks.active.is_empty() && !peaks.tracking_fault);
        assert_eq!(peaks.omitted_frames, 0);
        assert_eq!(peaks.unconfirmed_submissions, 0);
        assert_eq!(peaks.scope, "session_enrolled_native_capacity");
        assert!(!peaks.complete_coverage && !peaks.is_physical_vram);
        let frame = peaks.recent.iter().find(|f| f.request == request).unwrap();
        assert_eq!(frame.pending_submissions, 0);
        assert_eq!(frame.unconfirmed_submissions, 0);
        assert!(frame.end_admitted_bytes.is_some());
        assert!(frame.peak_admitted_bytes >= before.admitted_bytes);
        let mut identities = BTreeSet::new();
        let shaders: Vec<_> = self
            .renderer
            .compiled_shader_owners()
            .filter_map(|(_, shader)| {
                let code = motion_effects::shader::compiled_code_identity(shader);
                identities.insert(code.digest).then(|| {
                    json!({"identity":code.digest,
                "wgsl_bytes":shader.wgsl.len(),
                "glsl_bytes":shader.glsl.vertex.len()+shader.glsl.fragment.len()})
                })
            })
            .collect();
        let evidence = json!({"draw_submit_wall_ns":submit_wall_ns,"draw_readback_wall_ns":submit_readback_wall_ns,
            "cpu_prepare_us":stats.cpu_prepare_us,"native_draw_calls":stats.draw_calls,"native_pipeline_builds":self.renderer.effect_pipeline_builds(),
            "native_pipeline_build_ns":self.renderer.effect_pipeline_build_nanos(),
            "fusion":self.renderer.pointwise_fusion_report(),"retained_shader_sources":shaders,
            "allocation_before":before,"allocation_window":frame,"allocation_after":self.budget.info().unwrap(),
            "allocation_scope":{"name":peaks.scope,"complete_coverage":peaks.complete_coverage,
                "is_physical_vram":peaks.is_physical_vram,"omitted_frames":peaks.omitted_frames},
            "counter_scope":"Per-renderer cumulative successful pipeline counters/timers; retained code includes original and generated programs, deduplicated by actual code identity"});
        (reference::linear(&image), evidence)
    }
}

/// Counts are assertions; durations and capacity peaks are observations, with no
/// speedup/FPS threshold and no assumption that frame work is GPU parallel.
pub fn run_case(mode: ChannelPrecision, width: u32, height: u32) -> Value {
    let (mut project, pixels) = fixture(mode, width, height);
    let mut original = Run::new(mode, width, height, false, &pixels);
    let mut optimized = Run::new(mode, width, height, true, &pixels);
    let mut stable = None;
    let mut unaffected_program = None;
    let mut rows: Vec<Value> = Vec::new();
    for (index, phase) in [
        "cold",
        "numeric_first_group",
        "numeric_second_group",
        "structure_first_group",
        "restored_structure",
    ]
    .into_iter()
    .enumerate()
    {
        match index {
            1 | 2 => {
                project.layers[index - 1]
                    .effects
                    .iter_mut()
                    .find(|e| e.effect == "tint")
                    .unwrap()
                    .params
                    .get_mut("p0003")
                    .unwrap()
                    .track
                    .value[0] = 43.25
            }
            3 | 4 => project.layers[0].effects.reverse(),
            _ => {}
        }
        project.validate().unwrap();
        let mut scene = Scene::new(&project).unwrap();
        scene
            .sample(&project, 0., None, &ExpressionEvaluator)
            .unwrap();
        let request = index as u64 + 1;
        let (expected, baseline) = original.capture(&scene, request);
        let (actual, grouped) = optimized.capture(&scene, request);
        let worst = expected
            .iter()
            .zip(&actual)
            .flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs()))
            .fold(0_f64, f64::max);
        let tolerance = match mode {
            ChannelPrecision::Unorm8 => 0.018,
            ChannelPrecision::Unorm16 => 12. / 65535.,
            ChannelPrecision::Float16 => 0.008,
            ChannelPrecision::Float32 => 0.00004,
        };
        assert_eq!(expected.len(), (width * height) as usize);
        assert!(actual.iter().flatten().all(|v| v.is_finite()));
        assert!(
            worst <= tolerance,
            "{mode:?} {phase}: {worst} > {tolerance}"
        );
        let report = optimized.renderer.pointwise_fusion_report();
        let counters = (
            report.portable_compiles,
            optimized.renderer.effect_pipeline_builds(),
        );
        let second = report.groups.iter().find(|g| g.object == 2).unwrap();
        if index == 0 {
            assert_eq!(report.groups.len(), 2);
            assert_eq!((report.original_passes, report.final_passes), (14, 6));
            stable = Some(counters);
            unaffected_program = Some(second.program);
        } else {
            assert_eq!(
                Some(second.program),
                unaffected_program,
                "unaffected group was rebuilt"
            );
        }
        if index == 1 || index == 2 {
            assert_eq!(Some(counters), stable, "numeric edit compiled code");
        }
        if index == 3 {
            let added = u64::from(mode != ChannelPrecision::Unorm16);
            assert_eq!(
                counters.0,
                stable.unwrap().0 + added,
                "structural edit recompiled unrelated groups"
            );
            assert_eq!(report.groups.len(), if added == 0 { 1 } else { 2 });
            // The UNORM16 retained boundary first needs its two original
            // effect pipelines and working-space conversion. Other modes need
            // only the new fused pipeline; the second group keeps its pipeline.
            let native_added = if mode == ChannelPrecision::Unorm16 {
                3
            } else {
                1
            };
            assert_eq!(
                counters.1,
                stable.unwrap().1 + native_added,
                "structural edit rebuilt unrelated native pipelines"
            );
        }
        if index == 4 {
            assert_eq!(report.groups.len(), 2);
            assert_eq!(
                counters.0,
                rows.last().unwrap()["grouped"]["fusion"]["portable_compiles"]
                    .as_u64()
                    .unwrap(),
                "restoring accepted structure should reuse its compile"
            );
            assert_eq!(
                counters.1,
                rows.last().unwrap()["grouped"]["native_pipeline_builds"]
                    .as_u64()
                    .unwrap(),
                "restoring accepted structure should reuse native pipelines"
            );
        }
        rows.push(
            json!({"phase":phase,"worst_linear_error":worst,"tolerance":tolerance,
            "baseline":baseline,"grouped":grouped}),
        );
    }
    json!({"precision":mode,"width":width,"height":height,
        "adapter":format!("{:?}",optimized.renderer.adapter_info),"rows":rows,
        "memory_scope":"Actual application admitted texture/buffer capacities over each completed frame, including retained resources and command reservations; excludes process RSS and driver VRAM",
        "timing_scope":"CPU draw/submit and draw/readback wall intervals include synchronization; native pipeline build includes validation; portable compile is compiler CPU work; no GPU duration inferred"})
}
