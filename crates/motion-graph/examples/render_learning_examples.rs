//! Render the actual exported packages, not a separate illustration of them.
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_effects::{EffectPackage, Registry};
use motion_graph::{
    builtin_example_catalogue, builtin_examples, export_msfx, CompileOptions, ExampleCollection,
};
use motion_render::{Renderer, Scene};
use std::{fs, path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "target/node-learning".into());
    fs::create_dir_all(&output)?;
    fs::write(
        output.join("catalogue.json"),
        serde_json::to_vec_pretty(&builtin_example_catalogue())?,
    )?;
    let mut registry = Registry::default();
    let mut packages = Vec::new();
    let mut reports = Vec::new();
    for example in builtin_examples()
        .iter()
        .filter(|e| e.collection != ExampleCollection::Fundamentals)
    {
        let graph = example.load()?;
        let report = export_msfx(
            &graph,
            &CompileOptions {
                plugin_id: format!("local.motionstudio.learning.{}", example.id),
                ..Default::default()
            },
            &output.join(format!("{}.msfx", example.id)),
        )?;
        let package = Arc::new(EffectPackage::open(&report.path)?);
        registry.insert(package.clone())?;
        packages.push((example.id, package));
        reports.push(report);
    }
    let mut renderer = pollster::block_on(Renderer::headless())?;
    renderer.set_effect_registry(registry);
    let target = renderer.capture_target(64, 64)?;
    for (id, package) in packages {
        let mut project = Project::new(64, 64, 30, 60)?;
        project.background = [0.0; 4];
        let mut layer = Layer::solid(1, id, [64.0; 2], [32.0, 32.0, 0.0], [1.0; 4]);
        layer.effects.push(EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            &package.manifest.effects[0],
            [64.0; 2],
        ));
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        project.validate()?;
        for (suffix, frame) in [("frame-000", 0.0), ("frame-015-5", 15.5)] {
            let mut scene = Scene::new(&project);
            scene.sample(&project, frame, None, &ExpressionEvaluator)?;
            let (pixels, _) = renderer.capture(&scene, &target)?;
            image::save_buffer(
                output.join(format!("{id}-{suffix}.png")),
                &pixels,
                64,
                64,
                image::ColorType::Rgba8,
            )?;
        }
        println!("Rendered {id}: 0 and 15.5 frames");
    }
    fs::write(
        output.join("compile-reports.json"),
        serde_json::to_vec_pretty(&reports)?,
    )?;
    println!("Output: {}", output.display());
    Ok(())
}
