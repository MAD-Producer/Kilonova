//! Public compositing API with an independently rasterized geometric golden.
use motion_core::{
    masks::LayerMask,
    vector::{PathNode, VectorPath},
    FrameRate, Layer, LayerTimeline, Project, Track,
};
use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};
use motion_host::Session;
use motion_render::{image_resources::Resolution, OutputTransform, Renderer};
use serde_json::{json, Value};
use std::{fs, path::Path};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;
const FRAMES: [f64; 6] = [5., 7.5, 15., 0., 26., 10.];
// Unorm8 mask, matte and child-composition intermediates each quantize.
// This budget is fixed independently of any measured output.
const MAX_CHANNEL_ERROR: u8 = 3;

fn rectangle(id: u64, bounds: [f32; 4]) -> LayerMask {
    let [left, top, right, bottom] = bounds;
    LayerMask::new(
        id,
        VectorPath {
            id,
            closed: true,
            tangents: Default::default(),
            nodes: [[left, top], [right, top], [right, bottom], [left, bottom]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    )
}

fn commands(mode: &str) -> Vec<Value> {
    let layer = |id, name, color| {
        let mut value = Layer::solid(
            id,
            name,
            [WIDTH as f32, HEIGHT as f32],
            [WIDTH as f32 / 2., HEIGHT as f32 / 2., 0.],
            color,
        );
        value.timeline = Some(LayerTimeline {
            in_frame: 5,
            out_frame: 25,
            offset_frame: 5,
        });
        value
    };
    let source = rectangle(1, [8., 8., 40., 40.]);
    let mut hole = rectangle(2, [20., 16., 28., 28.]);
    hole.mode = motion_core::masks::MaskMode::Subtract;
    let mut result = vec![
        json!({"op":"add","layer":layer(1,"Hidden green matte",[0.,1.,0.,1.])}),
        json!({"op":"add","layer":layer(2,"Red target with a source-space hole",[1.,0.,0.,1.])}),
        json!({"op":"flags","object":1,"visible":false,"locked":false}),
        json!({"op":"mask","object":1,"action":{"kind":"add","mask":source}}),
        json!({"op":"mask","object":2,"action":{"kind":"add","mask":rectangle(1,[4.,4.,60.,44.])}}),
        json!({"op":"mask","object":2,"action":{"kind":"add","mask":hole}}),
        json!({"op":"mask","object":1,"action":{"kind":"animate","mask":1,"property":"opacity","frame":5,"enabled":true}}),
        json!({"op":"mask","object":1,"action":{"kind":"set","mask":1,"property":"opacity","frame":15,"value":[50]}}),
        json!({"op":"set_track_matte","object":2,"matte":{"source":1,"mode":mode,"hide_source":true}}),
    ];
    for node in &source.path.nodes {
        let mut end = node.geometry.value;
        end[0] += 8.;
        result.push(
            json!({"op":"mask","object":1,"action":{"kind":"animate","mask":1,
            "property":{"node":node.id},"frame":5,"enabled":true}}),
        );
        result.push(
            json!({"op":"mask","object":1,"action":{"kind":"set","mask":1,
            "property":{"node":node.id},"frame":15,"value":end}}),
        );
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
        .sdr_capture_target(WIDTH, HEIGHT, OutputTransform::SDR_CLIP)
        .unwrap();
    renderer
        .capture_sdr(&session.scene, &target)
        .unwrap()
        .0
        .pixels
}

// Pixel-center rectangle membership and documented coverage arithmetic. This
// does not sample a Scene, invoke a mask tessellator or reuse a renderer shader.
fn golden(mode: &str, frame: f64) -> Vec<u8> {
    let mut pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
    if !(5. ..25.).contains(&frame) {
        return pixels;
    }
    let t = ((frame - 5.) / 10.).clamp(0., 1.) as f32;
    let shift = 8. * t;
    let opacity = 1. - 0.5 * t;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let target = (4. ..60.).contains(&px)
                && (4. ..44.).contains(&py)
                && !((20. ..28.).contains(&px) && (16. ..28.).contains(&py));
            let source = (8. + shift..40. + shift).contains(&px) && (8. ..40.).contains(&py);
            let mut coverage = if source { opacity } else { 0. };
            if mode.starts_with("luma") {
                coverage *= 0.7152;
            }
            if mode.ends_with("inverted") {
                coverage = 1. - coverage;
            }
            if target {
                let alpha = (coverage * 255.).round() as u8;
                let i = ((y * WIDTH + x) * 4) as usize;
                pixels[i..i + 4].copy_from_slice(&[if alpha > 0 { 255 } else { 0 }, 0, 0, alpha]);
            }
        }
    }
    pixels
}

fn compare(actual: &[u8], expected: &[u8], context: &str) -> u8 {
    assert_eq!(actual.len(), expected.len());
    let worst = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    if worst > MAX_CHANNEL_ERROR {
        let first = actual
            .chunks_exact(4)
            .zip(expected.chunks_exact(4))
            .enumerate()
            .find(|(_, (a, b))| {
                a.iter()
                    .zip(*b)
                    .any(|(a, b)| a.abs_diff(*b) > MAX_CHANNEL_ERROR)
            });
        if let Some((index, (a, b))) = first {
            eprintln!(
                "{context}: first mismatch at ({},{}), actual {a:?}, expected {b:?}",
                index % WIDTH as usize,
                index / WIDTH as usize
            );
        }
        if let Some(output) = std::env::var_os("MOTION_PROFESSIONAL_MATTE_ARTIFACTS") {
            let output = std::path::PathBuf::from(output);
            fs::create_dir_all(&output).unwrap();
            png(&output.join("failed-actual.png"), actual);
            png(&output.join("failed-golden.png"), expected);
        }
    }
    assert!(
        worst <= MAX_CHANNEL_ERROR,
        "{context}: maximum channel error {worst} exceeds {MAX_CHANNEL_ERROR}"
    );
    worst
}

fn png(path: &Path, pixels: &[u8]) {
    image::save_buffer(path, pixels, WIDTH, HEIGHT, image::ColorType::Rgba8).unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn animated_source_masks_and_four_mattes_preserve_goldens_through_precompose_and_freeze() {
    let directory = tempfile::tempdir().unwrap();
    let output =
        std::env::var_os("MOTION_PROFESSIONAL_MATTE_ARTIFACTS").map(std::path::PathBuf::from);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut cases = Vec::new();
    for mode in ["alpha", "alpha_inverted", "luma", "luma_inverted"] {
        let root = directory.path().join(mode);
        fs::create_dir(&root).unwrap();
        let mut original =
            Project::new_with_frame_rate(WIDTH, HEIGHT, FrameRate::new(30000, 1001).unwrap(), 30)
                .unwrap();
        original.background = [0.; 4];
        let mut session = Session::new(
            original.clone(),
            root.clone(),
            motion_desktop_media::platform(),
        )
        .unwrap();
        let commands = commands(mode);
        session
            .apply_commands(&serde_json::to_string(&commands).unwrap())
            .unwrap();
        let flat = session.engine.snapshot();
        let path = &flat.layers[0].masks[0].path;
        assert!(path.nodes.iter().all(|node| node
            .geometry
            .keys
            .iter()
            .map(|key| key.frame)
            .collect::<Vec<_>>()
            == vec![0, 10]));
        assert!(!flat.layers[0].visible);
        let history = session.engine.history_diagnostics();
        let invalid = json!({"op":"composition","action":{"kind":"precompose","objects":[2],"name":"Broken association"}});
        let error = session.apply_commands(&invalid.to_string()).unwrap_err();
        assert!(
            error.to_string().contains("cross_selection_matte"),
            "{error}"
        );
        assert_eq!(session.engine.project(), &flat);
        assert_eq!(session.engine.history_diagnostics(), history);
        let mut previews = Vec::new();
        let mut results = Vec::new();
        for frame in FRAMES {
            let pixels = render(&mut session, &mut renderer, frame);
            let expected = golden(mode, frame);
            if pixels.iter().zip(&expected).any(|(a,b)| a.abs_diff(*b)>MAX_CHANNEL_ERROR) {
                for layer in &session.scene.layers {
                    eprintln!("sample frame={frame} layer={} visible={} opacity={} masks={:?}",layer.id,layer.composite_visible,layer.opacity,layer.masks);
                }
                let mut fresh = pollster::block_on(Renderer::headless()).unwrap();
                let fresh_pixels = render(&mut session,&mut fresh,frame);
                eprintln!("fresh renderer differs from retained renderer: {}",fresh_pixels!=pixels);
                if let Some(output) = &output {
                    fs::create_dir_all(output).unwrap();
                    png(&output.join("failed-fresh-renderer.png"),&fresh_pixels);
                }
            }
            let worst = compare(&pixels, &expected, &format!("{mode} flat at {frame}"));
            results.push(json!({"frame":frame,"flatGoldenMax":worst}));
            previews.push(pixels);
        }
        assert_ne!(
            previews[0], previews[1],
            "moving mask/opacity must visibly change coverage"
        );
        let precompose = json!({"op":"composition","action":{"kind":"precompose","objects":[1,2],"name":format!("{mode} complete matte group"),"range":"composition"}});
        session.apply_commands(&precompose.to_string()).unwrap();
        let nested = session.engine.snapshot();
        assert_eq!(nested.layers.len(), 1);
        assert_eq!(nested.compositions.len(), 1);
        let child = nested.composition(&nested.compositions[0].id).unwrap();
        assert_eq!(child.layers, flat.layers);
        assert_eq!(child.background, [0.; 4]);
        for (index, frame) in FRAMES.into_iter().enumerate() {
            let pixels = render(&mut session, &mut renderer, frame);
            results[index]["nestedGoldenMax"] = json!(compare(
                &pixels,
                &golden(mode, frame),
                &format!("{mode} nested at {frame}")
            ));
            results[index]["nestedFlatMax"] = json!(compare(
                &pixels,
                &previews[index],
                &format!("{mode} nesting difference at {frame}")
            ));
        }
        session.engine.undo().unwrap();
        assert_eq!(session.engine.project(), &flat);
        session.engine.undo().unwrap();
        assert_eq!(session.engine.project(), &original);
        assert!(render(&mut session, &mut renderer, 10.)
            .iter()
            .all(|byte| *byte == 0));
        session.engine.redo().unwrap();
        session.engine.redo().unwrap();
        assert_eq!(session.engine.project(), &nested);
        motion_core::storage::save(&root, &nested).unwrap();
        let mut reopened = Session::new(
            motion_core::storage::load(&root).unwrap(),
            root.clone(),
            motion_desktop_media::platform(),
        )
        .unwrap();
        assert_eq!(reopened.engine.project(), &nested);
        for (index, frame) in FRAMES.into_iter().enumerate() {
            let pixels = render(&mut reopened, &mut renderer, frame);
            assert_eq!(
                pixels,
                render(&mut session, &mut renderer, frame),
                "{mode} reopen {frame}"
            );
            compare(&pixels, &previews[index], "reopened versus flat");
        }
        let frozen_pixels = render(&mut session, &mut renderer, 7.5);
        let revision = session.engine.revision();
        let plan = FrameFreezePlan::capture(
            &session,
            Some(directory.path().join(format!("{mode}.png"))),
            &ExportControl::default(),
        )
        .unwrap();
        session
            .apply_commands(&json!({"op":"delete","object":nested.layers[0].id}).to_string())
            .unwrap();
        assert_ne!(render(&mut session, &mut renderer, 7.5), frozen_pixels);
        drop(reopened);
        drop(session);
        let result = plan.export(&ExportControl::default()).unwrap();
        assert_eq!(result["revision"], revision);
        let frozen = image::open(result["path"].as_str().unwrap())
            .unwrap()
            .into_rgba8();
        assert_eq!(frozen.dimensions(), (WIDTH, HEIGHT));
        assert_eq!(frozen.as_raw(), &frozen_pixels);
        compare(
            frozen.as_raw(),
            &golden(mode, 7.5),
            "frozen independent golden",
        );
        if let Some(output) = &output {
            let output = output.join(mode);
            fs::create_dir_all(&output).unwrap();
            fs::copy(root.join("project.json"), output.join("project.json")).unwrap();
            for (name, value) in [
                ("initial-project.json", json!(original)),
                ("flat-project.json", json!(flat)),
                ("commands.json", json!(commands)),
                ("precompose.json", precompose),
            ] {
                fs::write(
                    output.join(name),
                    serde_json::to_vec_pretty(&value).unwrap(),
                )
                .unwrap();
            }
            frozen.save(output.join("frozen-7.5.png")).unwrap();
            for (index, frame) in FRAMES.into_iter().enumerate() {
                png(&output.join(format!("flat-{frame}.png")), &previews[index]);
                png(
                    &output.join(format!("golden-{frame}.png")),
                    &golden(mode, frame),
                );
            }
        }
        cases.push(
            json!({"mode":mode,"comparisons":results,"frozenExact":true,"revision":revision}),
        );
    }
    if let Some(output) = &output {
        fs::write(output.join("result.json"),serde_json::to_vec_pretty(&json!({
            "scope":"Actual public host source masks, four track matte modes, nested flat precompose, independent CPU geometric goldens, history/reopen/frozen PNGs after live deletion and session closure",
            "limits":["Default 8-bit headless only", "Procedural geometric shot, not photographed roto/greenscreen footage", "No feather, effects or external media in this fixture"],
            "author":"Motion Studio repository contributors","license":"MIT","externalAssets":false,
            "width":WIDTH,"height":HEIGHT,"maximumChannelErrorBudget":MAX_CHANNEL_ERROR,"cases":cases
        })).unwrap()).unwrap();
    }
}
