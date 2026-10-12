//! Render the actual exported packages, not a separate illustration of them.
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_effects::{EffectPackage, Registry};
use motion_graph::{
    builtin_example_catalogue, builtin_examples, export_msfx, CompileOptions, ExampleCollection,
};
use motion_render::{Renderer, Scene};
use std::{fs, path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| "target/node-learning".into());
    let example_id = args.next().map(|id| id.to_string_lossy().into_owned());
    let frame_count = args
        .next()
        .map(|n| n.to_string_lossy().parse::<u32>())
        .transpose()?;
    let edge = args
        .next()
        .map(|n| n.to_string_lossy().parse::<u32>())
        .transpose()?
        .unwrap_or(64);
    if !(16..=4096).contains(&edge) {
        return Err("capture edge must be 16..=4096".into());
    }
    if frame_count.is_some_and(|n| n == 0 || n > 600) {
        return Err("frame_count must be 1..=600".into());
    }
    if example_id
        .as_ref()
        .is_some_and(|id| !builtin_examples().iter().any(|e| e.id == id))
    {
        return Err("unknown built-in example ID".into());
    }
    fs::create_dir_all(&output)?;
    fs::write(
        output.join("catalogue.json"),
        serde_json::to_vec_pretty(&builtin_example_catalogue())?,
    )?;
    let mut registry = Registry::default();
    let mut packages = Vec::new();
    let mut reports = Vec::new();
    for example in builtin_examples().iter().filter(|e| {
        example_id
            .as_ref()
            .map_or(e.collection != ExampleCollection::Fundamentals, |id| {
                e.id == id
            })
    }) {
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
    let target = renderer.capture_target(edge, edge)?;
    for (id, package) in packages {
        let mut project = Project::new(edge, edge, 30, frame_count.unwrap_or(60).max(60))?;
        project.background = [0.0; 4];
        let mut layer = Layer::solid(
            1,
            id,
            [edge as f32; 2],
            [edge as f32 / 2.0, edge as f32 / 2.0, 0.0],
            [1.0; 4],
        );
        layer.effects.push(EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            &package.manifest.effects[0],
            [edge as f32; 2],
        ));
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        project.validate()?;
        let frames: Vec<_> = if let Some(count) = frame_count {
            (0..count)
                .map(|frame| (format!("frame-{frame:04}"), f64::from(frame)))
                .collect()
        } else {
            vec![("frame-000".into(), 0.0), ("frame-015-5".into(), 15.5)]
        };
        for (suffix, frame) in frames {
            let mut scene = Scene::new(&project);
            scene.sample(&project, frame, None, &ExpressionEvaluator)?;
            let (pixels, _) = renderer.capture(&scene, &target)?;
            image::save_buffer(
                output.join(format!("{id}-{suffix}.png")),
                &pixels,
                edge,
                edge,
                image::ColorType::Rgba8,
            )?;
        }
        println!(
            "Rendered {id}: {}",
            frame_count.map_or("frames 0 and 15.5".into(), |n| format!(
                "{n} frames at 30 fps"
            ))
        );
    }
    fs::write(
        output.join("compile-reports.json"),
        serde_json::to_vec_pretty(&reports)?,
    )?;
    println!("Output: {}", output.display());
    Ok(())
}
