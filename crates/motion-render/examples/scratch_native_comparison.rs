//! Source-identical native fixture driver for the pre-change and candidate
//! renderers. This records allocations and pixels, not performance gains.
#[path = "../../motion-effects/tests/support/scene_packages.rs"]
mod scene_fixtures;
use motion_core::{
    compositing::{MatteMode, TrackMatte},
    *,
};
use motion_render::{effect_plan::PlanBuilder, Renderer, Scene};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{error::Error, fs, path::Path};

fn effect(
    package: &motion_effects::EffectPackage,
    name: &str,
    id: u64,
    size: [f32; 2],
) -> EffectInstance {
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == name)
        .unwrap();
    EffectInstance::new(
        id,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        size,
    )
}
fn project(sizes: &[[f32; 2]], effects: &[&str]) -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let mut p = Project::new(512, 512, 30, 120).unwrap();
    p.background = [0.; 4];
    for (i, size) in sizes.iter().enumerate() {
        let id = i as u64 + 1;
        let mut layer = Layer::solid(
            id,
            "authored scratch fixture",
            *size,
            [150. + i as f32 * 100., 150. + i as f32 * 100., 0.],
            [
                [1., 0.2, 0.5, 0.75],
                [0.1, 0.7, 1., 0.6],
                [0.7, 1., 0.2, 0.8],
            ][i % 3],
        );
        for (j, name) in effects.iter().enumerate() {
            layer
                .effects
                .push(effect(&package, name, id * 100 + j as u64 + 1, *size));
        }
        p.layers.push(layer);
    }
    p.rebuild_plugin_dependencies();
    p
}
fn record(
    name: &str,
    p: &Project,
    renderer: &mut Renderer,
    out: &Path,
    rows: &mut Vec<Value>,
) -> std::result::Result<(), Box<dyn Error>> {
    let frame = 17.;
    let mut scene = Scene::new(p)?;
    scene.sample(p, frame, None, &ExpressionEvaluator)?;
    let mut builder = PlanBuilder::new(scene_fixtures::registry()?)?;
    let plan = builder.build(&scene, &[0], 512, 512, true)?;
    let mut packed = vec![0; plan.buffer_bytes(&scene)];
    plan.write(&scene, &mut packed)?;
    let word = |offset| u32::from_ne_bytes(packed[offset..offset + 4].try_into().unwrap());
    let version = word(4);
    let (planned, descriptors) = if version == 9 {
        (
            motion_render::effect_plan::scratch_capacity_bytes(&plan.scratch_sizes),
            json!({"semanticSizes": plan.scratch_sizes}),
        )
    } else {
        assert_eq!(version, motion_render::effect_plan::PLAN_VERSION);
        let offset = word(140) as usize;
        let count = word(144) as usize;
        assert_eq!(word(148), 32);
        let entries: Vec<_> = (0..count)
            .map(|i| {
                let p = offset + i * 32;
                [
                    word(p),
                    word(p + 4),
                    word(p + 8),
                    word(p + 12),
                    word(p + 16),
                    word(p + 20),
                    word(p + 24),
                    word(p + 28),
                ]
            })
            .collect();
        (
            entries
                .iter()
                .map(|d| u64::from(d[0]) * u64::from(d[1]) * u64::from(d[6]))
                .sum(),
            json!(entries),
        )
    };
    renderer.retain_video_instances(&scene);
    let target = renderer.capture_target(512, 512)?;
    let (pixels, _) = renderer.capture(&scene, &target)?;
    renderer.check_health()?;
    assert_eq!(pixels.len(), 512 * 512 * 4);
    assert!(
        pixels
            .chunks_exact(4)
            .any(|p| p[3] > 0 && p[..3].iter().any(|c| *c > 0)),
        "empty fixture {name}"
    );
    let actual = renderer.texture_memory_breakdown().effect_scratch;
    if !plan.passes.is_empty() {
        assert_eq!(actual, planned, "{name}: planned/native scratch mismatch");
    }
    fs::write(
        out.join(format!("{name}.project.json")),
        serde_json::to_vec_pretty(p)?,
    )?;
    fs::write(out.join(format!("{name}.plan")), &packed)?;
    fs::write(out.join(format!("{name}.rgba")), &pixels)?;
    image::save_buffer(
        out.join(format!("{name}.png")),
        &pixels,
        512,
        512,
        image::ColorType::Rgba8,
    )?;
    rows.push(json!({"name":name,"frame":frame,"width":512,"height":512,
        "rgbaSha256":format!("{:x}",Sha256::digest(&pixels)),
        "projectSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(p)?)),
        "packedPlanVersion":version,"plannedTopScratchBytes":planned,"actualTopScratchBytes":actual,
        "physicalOrSemanticDescriptors":descriptors,"passes":plan.passes.len(),
        "textureLedger":renderer.texture_resource_ledger()}));
    Ok(())
}
fn main() -> std::result::Result<(), Box<dyn Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("output directory required")?;
    let out = Path::new(&path);
    fs::create_dir(out)?;
    let mut renderer = pollster::block_on(Renderer::headless())?;
    renderer.set_effect_registry(scene_fixtures::registry()?);
    let mut rows = Vec::new();
    let mixed = project(
        &[[256., 16.], [16., 256.], [256., 16.]],
        &["polar_coordinates", "gaussian_blur", "tint"],
    );
    record("mixed-aspects", &mixed, &mut renderer, out, &mut rows)?;
    let large = project(&[[384., 256.]], &["gaussian_blur", "tint"]);
    let small = project(&[[16., 16.]], &["gaussian_blur", "tint"]);
    record("large-before", &large, &mut renderer, out, &mut rows)?;
    record("small", &small, &mut renderer, out, &mut rows)?;
    record("large-after", &large, &mut renderer, out, &mut rows)?;
    let mut padded = project(
        &[[180., 90.]],
        &[
            "glow_edges",
            "gaussian_blur",
            "gaussian_blur",
            "motion_tile",
        ],
    );
    padded.layers[0].effects[0]
        .params
        .get_mut("radius")
        .unwrap()
        .track
        .value[0] = 12.;
    padded.layers[0].effects[3]
        .params
        .get_mut("output_width")
        .unwrap()
        .track
        .value[0] = 150.;
    padded.layers[0].effects[3]
        .params
        .get_mut("output_height")
        .unwrap()
        .track
        .value[0] = 125.;
    padded.rebuild_plugin_dependencies();
    record("padding-long-blur", &padded, &mut renderer, out, &mut rows)?;
    let mut matte = project(&[[190., 110.], [280., 160.]], &["gaussian_blur", "tint"]);
    matte.layers[0].visible = false;
    matte.layers[1].track_matte = Some(TrackMatte {
        source: 1,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    record("effected-matte", &matte, &mut renderer, out, &mut rows)?;
    let mut engine = Engine::new(matte)?;
    engine.apply_batch(parse_commands(
        &json!({"op":"composition","composition":MAIN_COMPOSITION,
        "action":{"kind":"precompose","objects":[1,2],"name":"authored nested fixture"}})
        .to_string(),
    )?)?;
    let mut nested = engine.snapshot();
    let package = motion_effects::builtin::package()?;
    let nested_id = nested.layers[0].id;
    nested.layers[0]
        .effects
        .push(effect(&package, "tint", nested_id * 1000, [512.; 2]));
    let generator_package = scene_fixtures::scene_package()?;
    nested.layers.push(Layer::solid(
        100,
        "archived SDK generator fixture",
        [512.; 2],
        [256., 256., 0.],
        [0.; 4],
    ));
    let mut engine = Engine::new(nested)?;
    let mut generator = effect(&generator_package, "starfield", 10001, [512.; 2]);
    generator.params.get_mut("extent").unwrap().track.value = [1024., 1024., 0., 0.];
    generator.params.get_mut("fade").unwrap().track.value = [0.; 4];
    engine.apply(Command::Effect {
        object: 100,
        action: EffectAction::Insert {
            instance: generator,
        },
    })?;
    let nested = engine.snapshot();
    record(
        "generator-matte-nested",
        &nested,
        &mut renderer,
        out,
        &mut rows,
    )?;
    // Package presence is insufficient: require visible generator contribution
    // while the nested, effected matte remains in the very same render.
    let mut without_generator = nested.clone();
    without_generator
        .layers
        .iter_mut()
        .find(|l| l.id == 100)
        .unwrap()
        .visible = false;
    let mut control = Scene::new(&without_generator)?;
    control.sample(&without_generator, 17., None, &ExpressionEvaluator)?;
    let control_target = renderer.capture_target(512, 512)?;
    let control_pixels = renderer.capture(&control, &control_target)?.0;
    let combined_pixels = fs::read(out.join("generator-matte-nested.rgba"))?;
    assert_ne!(
        combined_pixels, control_pixels,
        "generator must visibly contribute alongside nested matte"
    );
    fs::write(
        out.join("generator-disabled.project.json"),
        serde_json::to_vec_pretty(&without_generator)?,
    )?;
    fs::write(out.join("generator-disabled.rgba"), &control_pixels)?;
    image::save_buffer(
        out.join("generator-disabled.png"),
        &control_pixels,
        512,
        512,
        image::ColorType::Rgba8,
    )?;
    rows.last_mut().unwrap()["generatorContributionVerified"] = json!(true);
    record(
        "mixed-aspects-revisited",
        &mixed,
        &mut renderer,
        out,
        &mut rows,
    )?;
    assert_eq!(
        rows[1]["rgbaSha256"], rows[3]["rgbaSha256"],
        "large-small-large must recover exact pixels"
    );
    assert_eq!(
        rows[0]["rgbaSha256"], rows[7]["rgbaSha256"],
        "revisited mixed draw must recover exact pixels"
    );
    fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&json!({
        "scope":"Debug native allocation and pixel correctness; no latency or physical VRAM gain claim",
        "sourceLabel":std::env::var("MOTION_SCRATCH_NATIVE_REVISION").unwrap_or_else(|_|"unspecified".into()),
        "adapter":{"name":renderer.adapter_info.name,"backend":format!("{:?}",renderer.adapter_info.backend),
            "driver":renderer.adapter_info.driver,"driverInfo":renderer.adapter_info.driver_info},
        "rows":rows}))?,
    )?;
    Ok(())
}
