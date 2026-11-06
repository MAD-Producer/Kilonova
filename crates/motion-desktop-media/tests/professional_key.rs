//! Synthetic numerical controls for the SDK sample, separate from footage quality.
use motion_core::{Asset, Content, EffectInstance, FrameRate, Layer, LayerTimeline, Project};
use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};
use motion_effects::{package_directory, EffectPackage};
use motion_host::Session;
use motion_render::{image_resources::Resolution, OutputTransform, Renderer};
use serde_json::json;
use std::{fs, path::Path};

const W: u32 = 32;
const H: u32 = 16;
const FRAMES: [f64; 5] = [5., 7.5, 15., 0., 10.];
// Declared before execution, for the existing Unorm8 image/effect/composite path.
const MAX_CHANNEL_ERROR: u8 = 3;

#[path = "professional_key/footage.rs"]
mod footage;

fn source(x: u32) -> [u8; 4] {
    match x / 8 {
        0 => [0, 255, 0, 255],
        1 => [220, 32, 64, 128],
        // This chroma lies inside the soft transition, unlike the opaque red
        // and keyed green controls. It must exercise partial key coverage.
        2 => [64, 160, 64, 255],
        _ => [0, 0, 0, 200],
    }
}
fn linear(v: u8) -> f64 {
    let v = f64::from(v) / 255.;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn encoded(v: f64) -> u8 {
    let v = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    };
    (v.clamp(0., 1.) * 255.).round() as u8
}
fn golden(frame: f64) -> Vec<u8> {
    let mut result = vec![0; (W * H * 4) as usize];
    if !(5. ..25.).contains(&frame) {
        return result;
    }
    let spill = ((frame - 5.) / 10.).clamp(0., 1.);
    for y in 0..H {
        for x in 0..W {
            let input = source(x);
            let mut rgb = [linear(input[0]), linear(input[1]), linear(input[2])];
            let sum: f64 = rgb.iter().sum();
            let coverage = if sum == 0. {
                1.
            } else {
                let chroma = [rgb[0] / sum, rgb[1] / sum - 1., rgb[2] / sum];
                let distance = chroma.iter().map(|v| v * v).sum::<f64>().sqrt();
                let t = ((distance - 0.1) / 0.3).clamp(0., 1.);
                t * t * (3. - 2. * t)
            };
            let alpha = (f64::from(input[3]) * coverage).round() as u8;
            if alpha == 0 {
                continue;
            }
            rgb[1] -= spill * (rgb[1] - rgb[0].max(rgb[2])).max(0.);
            let at = ((y * W + x) * 4) as usize;
            result[at..at + 4].copy_from_slice(&[
                encoded(rgb[0]),
                encoded(rgb[1]),
                encoded(rgb[2]),
                alpha,
            ]);
        }
    }
    result
}
fn render(session: &mut Session, renderer: &mut Renderer, frame: f64) -> Vec<u8> {
    session.frame = frame;
    session.sample().unwrap();
    renderer.set_effect_registry(session.effects.registry.clone());
    renderer
        .configure_assets_with_scene(session.engine.project(), &session.root, &session.scene)
        .unwrap();
    assert!(renderer
        .prepare_scene_assets(&session.scene, Resolution::Full, false)
        .unwrap());
    let target = renderer
        .sdr_capture_target(
            session.engine.project().width,
            session.engine.project().height,
            OutputTransform::SDR_CLIP,
        )
        .unwrap();
    renderer
        .capture_sdr(&session.scene, &target)
        .unwrap()
        .0
        .pixels
}
fn compare(actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len());
    let error = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        error <= MAX_CHANNEL_ERROR,
        "chroma-key Unorm8 numerical error={error}"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn chroma_sample_public_api_preserves_alpha_local_animation_history_reopen_and_frozen_output() {
    let soft_alpha = golden(5.)[(16 * 4) + 3];
    assert!((1..255).contains(&soft_alpha), "soft-transition fixture");
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    fs::create_dir_all(root.join("assets")).unwrap();
    let package = EffectPackage::from_bytes(
        package_directory(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/examples/chroma-key"),
        )
        .unwrap(),
    )
    .unwrap();
    fs::create_dir(directory.path().join("plugins")).unwrap();
    let plugin_path = directory.path().join("plugins/key.msfx");
    fs::write(&plugin_path, &package.bytes).unwrap();
    let source_path = root.join("assets/source.png");
    image::RgbaImage::from_fn(W, H, |x, _| image::Rgba(source(x)))
        .save(&source_path)
        .unwrap();
    let mut project =
        Project::new_with_frame_rate(W, H, FrameRate::new(30000, 1001).unwrap(), 30).unwrap();
    // Inspect the keyed layer's coverage, without compositing it over the
    // constructor's opaque default composition background.
    project.background = [0.; 4];
    project.assets.push(Asset {
        id: 1,
        path: "assets/source.png".into(),
        width: W,
        height: H,
    });
    let initial = project.clone();
    let mut session =
        Session::new(project, root.clone(), motion_desktop_media::platform()).unwrap();
    let mut layer = Layer::solid(
        7,
        "Key sample",
        [W as f32, H as f32],
        [W as f32 / 2., H as f32 / 2., 0.],
        [1.; 4],
    );
    layer.content = Content::Image { asset: 1 };
    layer.timeline = Some(LayerTimeline {
        in_frame: 5,
        out_frame: 25,
        offset_frame: 5,
    });
    let effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        layer.size,
    );
    let commands = json!([
        {"op":"add","layer":layer},
        {"op":"effect","object":7,"action":{"kind":"insert","instance":effect}},
        {"op":"effect","object":7,"action":{"kind":"set","effect":1,"param":"tolerance","frame":5,"value":[0.1,0,0,0]}},
        {"op":"effect","object":7,"action":{"kind":"set","effect":1,"param":"spill","frame":5,"value":[0,0,0,0]}},
        {"op":"effect","object":7,"action":{"kind":"animate","effect":1,"param":"spill","frame":5,"enabled":true}},
        {"op":"effect","object":7,"action":{"kind":"set","effect":1,"param":"spill","frame":15,"value":[1,0,0,0]}}
    ]);
    session.apply_commands(&commands.to_string()).unwrap();
    let edited = session.engine.snapshot();
    assert_eq!(
        edited.layers[0].effects[0].params["spill"]
            .track
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [0, 10]
    );
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut previews = Vec::new();
    for frame in FRAMES {
        let actual = render(&mut session, &mut renderer, frame);
        compare(&actual, &golden(frame));
        previews.push(actual);
    }
    assert_ne!(
        previews[0], previews[2],
        "spill animation must change surviving RGB"
    );
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &initial);
    assert!(render(&mut session, &mut renderer, 10.)
        .iter()
        .all(|v| *v == 0));
    session.engine.redo().unwrap();
    assert_eq!(session.engine.project(), &edited);
    assert_eq!(render(&mut session, &mut renderer, 7.5), previews[1]);
    motion_core::storage::save(&root, &edited).unwrap();
    let mut reopened = Session::new(
        motion_core::storage::load(&root).unwrap(),
        root.clone(),
        motion_desktop_media::platform(),
    )
    .unwrap();
    for (i, frame) in FRAMES.into_iter().enumerate() {
        assert_eq!(render(&mut reopened, &mut renderer, frame), previews[i]);
    }
    if let Some(output) = std::env::var_os("MOTION_PROFESSIONAL_KEY_ARTIFACTS") {
        let output = std::path::PathBuf::from(output);
        fs::create_dir_all(output.join("assets")).unwrap();
        fs::copy(&source_path, output.join("assets/source.png")).unwrap();
        fs::write(output.join("key.msfx"), &package.bytes).unwrap();
        for (name, value) in [
            ("initial-project.json", json!(initial)),
            ("project.json", json!(edited)),
            ("commands.json", commands),
        ] {
            fs::write(
                output.join(name),
                serde_json::to_vec_pretty(&value).unwrap(),
            )
            .unwrap();
        }
        for (i, frame) in FRAMES.into_iter().enumerate() {
            image::save_buffer(
                output.join(format!("native-{i}.png")),
                &previews[i],
                W,
                H,
                image::ColorType::Rgba8,
            )
            .unwrap();
            image::save_buffer(
                output.join(format!("golden-{i}.png")),
                &golden(frame),
                W,
                H,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    session.frame = 7.5;
    let output = directory.path().join("frozen.png");
    let frozen =
        FrameFreezePlan::capture(&session, Some(output.clone()), &ExportControl::default())
            .unwrap();
    session
        .apply_commands(&json!({"op":"delete","object":7}).to_string())
        .unwrap();
    drop(session);
    drop(reopened);
    fs::remove_file(plugin_path).unwrap();
    fs::remove_file(source_path).unwrap();
    let receipt = frozen.export(&ExportControl::default()).unwrap();
    assert_eq!(receipt["frame"], 7.5);
    assert_eq!(
        image::open(&output).unwrap().into_rgba8().into_raw(),
        previews[1]
    );
    if let Some(artifacts) = std::env::var_os("MOTION_PROFESSIONAL_KEY_ARTIFACTS") {
        let artifacts = std::path::PathBuf::from(artifacts);
        fs::copy(output, artifacts.join("frozen.png")).unwrap();
        fs::write(
            artifacts.join("frozen-receipt.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }
}
