//! Real photographed SDR controls, independent of the synthetic kernel oracle.
use super::render;
use motion_core::{
    masks::LayerMask,
    vector::{PathNode, VectorPath},
    Asset, Content, EffectInstance, FrameRate, Layer, LayerTimeline, Project, Track,
};
use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};
use motion_effects::{package_directory, EffectPackage};
use motion_host::Session;
use motion_render::Renderer;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const WIDTH: u32 = 512;
const HEIGHT: u32 = 270;
// Fixed by manual annotation before any native key execution. These core-region
// controls do not represent physical ground-truth alpha at soft/blurred edges.
const BACKGROUND_MAX_ALPHA: u8 = 5;
const FOREGROUND_MIN_ALPHA: u8 = 250;

fn same_pixels(actual: &[u8], expected: &[u8], phase: &str) {
    assert_eq!(actual.len(), expected.len(), "{phase}");
    let first = actual.iter().zip(expected).position(|(a, b)| a != b);
    assert!(
        first.is_none(),
        "{phase}: first differing channel={first:?}"
    );
}

fn check_regions(pixels: &[u8], manifest: &Value, number: u64) -> Vec<Value> {
    assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
    manifest["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|region| {
            let b: Vec<u32> = region["box"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect();
            assert_eq!(b.len(), 4);
            assert!(b[0] < b[2] && b[1] < b[3] && b[2] <= WIDTH && b[3] <= HEIGHT);
            let mut minimum = u8::MAX;
            let mut maximum = 0;
            for y in b[1]..b[3] {
                for x in b[0]..b[2] {
                    let alpha = pixels[((y * WIDTH + x) * 4 + 3) as usize];
                    minimum = minimum.min(alpha);
                    maximum = maximum.max(alpha);
                }
            }
            let id = region["id"].as_str().unwrap();
            match region["label"].as_str().unwrap() {
                "background" => assert!(
                    maximum <= BACKGROUND_MAX_ALPHA,
                    "frame={number} region={id} background max alpha={maximum}"
                ),
                "foreground" => assert!(
                    minimum >= FOREGROUND_MIN_ALPHA,
                    "frame={number} region={id} foreground min alpha={minimum}"
                ),
                other => panic!("unknown manually authored region label={other}"),
            }
            json!({"id":id,"minimumAlpha":minimum,"maximumAlpha":maximum})
        })
        .collect()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn photographed_key_with_manual_holdout_history_reopen_and_frozen_png_keep_fixed_controls() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chroma-key");
    let manifest: Value =
        serde_json::from_slice(&fs::read(fixtures.join("source.json")).unwrap()).unwrap();
    assert_eq!(manifest["extent"], json!([WIDTH, HEIGHT]));
    assert_eq!(
        manifest["budgets"]["backgroundMaximumAlphaByte"],
        BACKGROUND_MAX_ALPHA
    );
    assert_eq!(
        manifest["budgets"]["foregroundMinimumAlphaByte"],
        FOREGROUND_MIN_ALPHA
    );
    assert!(manifest["sourceCaptureRate"].is_null() && manifest["sourcePts"].is_null());
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
    let plugin = directory.path().join("plugins/key.msfx");
    fs::write(&plugin, &package.bytes).unwrap();
    // Explicitly authored demonstration clock, unrelated to unknown capture PTS.
    let mut project =
        Project::new_with_frame_rate(WIDTH, HEIGHT, FrameRate::integer(24).unwrap(), 3).unwrap();
    // Core alpha belongs to the keyed footage, not an opaque composition matte.
    project.background = [0.; 4];
    let mut commands = vec![];
    let mut spill_commands = vec![];
    let mut holdout_commands = vec![];
    let frames = manifest["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 3);
    for (index, frame) in frames.iter().enumerate() {
        let number = frame["number"].as_u64().unwrap();
        assert_eq!(number, 500 + index as u64);
        let file = frame["file"].as_str().unwrap();
        let original = fs::read(fixtures.join(file)).unwrap();
        let digest = format!("{:x}", Sha256::digest(&original));
        assert_eq!(digest, frame["sha256"].as_str().unwrap());
        assert_eq!(
            image::load_from_memory(&original)
                .unwrap()
                .to_rgb8()
                .dimensions(),
            (WIDTH, HEIGHT)
        );
        fs::write(root.join("assets").join(file), original).unwrap();
        let asset = index as u64 + 1;
        let object = index as u64 + 7;
        project.assets.push(Asset {
            id: asset,
            path: format!("assets/{file}"),
            width: WIDTH,
            height: HEIGHT,
        });
        let mut layer = Layer::solid(
            object,
            &format!("Photographed source {number}"),
            [WIDTH as f32, HEIGHT as f32],
            [WIDTH as f32 / 2., HEIGHT as f32 / 2., 0.],
            [1.; 4],
        );
        layer.content = Content::Image { asset };
        layer.timeline = Some(LayerTimeline {
            in_frame: index as i32,
            out_frame: index as i32 + 1,
            offset_frame: index as i32,
        });
        // Authored on the visible interior of the front person's dark clothing
        // in frame 500, after the raw classifier failure was discovered. This
        // is manual compositing assistance, not an automatic-key improvement
        // or an unseen-frame qualification. No core ROI or alpha budget changes.
        let mut holdout = layer.clone();
        holdout.id = 20 + index as u64;
        holdout.name = format!("Manual inner-clothing holdout {number}");
        let points = manifest["manualHoldout"]["polygon"].as_array().unwrap();
        let mut mask = LayerMask::new(
            1,
            VectorPath {
                id: 1,
                closed: true,
                tangents: Default::default(),
                nodes: points
                    .iter()
                    .enumerate()
                    .map(|(i, p)| PathNode {
                        id: i as u64 + 1,
                        geometry: Track::constant([
                            p[0].as_f64().unwrap() as f32,
                            p[1].as_f64().unwrap() as f32,
                            0.,
                            0.,
                            0.,
                            0.,
                        ]),
                    })
                    .collect(),
            },
        );
        mask.name = "Authored interior protection".into();
        mask.feather = Track::constant([1., 1.]);
        holdout_commands.push(json!({"op":"add","layer":holdout}));
        holdout_commands.push(json!({"op":"mask","object":20 + index as u64,
            "action":{"kind":"add","mask":mask}}));
        let effect = EffectInstance::new(
            asset,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            &package.manifest.effects[0],
            layer.size,
        );
        commands.push(json!({"op":"add","layer":layer}));
        commands.push(
            json!({"op":"effect","object":object,"action":{"kind":"insert","instance":effect}}),
        );
        for param in ["key_color", "tolerance", "softness", "spill"] {
            let value = if param == "key_color" {
                manifest["parameters"][param].clone()
            } else {
                json!([
                    if param == "spill" {
                        0.
                    } else {
                        manifest["parameters"][param].as_f64().unwrap()
                    },
                    0,
                    0,
                    0
                ])
            };
            commands.push(json!({"op":"effect","object":object,"action":{"kind":"set","effect":asset,"param":param,"frame":index,"value":value}}));
        }
        spill_commands.push(json!({"op":"effect","object":object,"action":{"kind":"set","effect":asset,"param":"spill","frame":index,"value":[manifest["parameters"]["spill"],0,0,0]}}));
    }
    commands.extend(holdout_commands);
    let initial = project.clone();
    let mut session =
        Session::new(project, root.clone(), motion_desktop_media::platform()).unwrap();
    session
        .apply_commands(&json!(commands).to_string())
        .unwrap();
    let without_spill = session.engine.snapshot();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut before = vec![];
    for frame in 0..3 {
        before.push(render(&mut session, &mut renderer, frame as f64));
    }
    session
        .apply_commands(&json!(spill_commands).to_string())
        .unwrap();
    let edited = session.engine.snapshot();
    // Preserve the unassisted key output as separate evidence. Removing this
    // manually authored protection must remain a real, reversible public edit.
    let remove_holdouts = json!([{"op":"delete","object":20},
        {"op":"delete","object":21},{"op":"delete","object":22}]);
    session
        .apply_commands(&remove_holdouts.to_string())
        .unwrap();
    for (index, frame) in frames.iter().enumerate() {
        let raw = render(&mut session, &mut renderer, index as f64);
        if let Some(output) = std::env::var_os("MOTION_PROFESSIONAL_KEY_ARTIFACTS") {
            let output = std::path::PathBuf::from(output).join("photographed/raw-key");
            fs::create_dir_all(&output).unwrap();
            image::save_buffer(
                output.join(format!("{}.png", frame["number"])),
                &raw,
                WIDTH,
                HEIGHT,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &edited);
    let mut previews = vec![];
    let mut scores = vec![];
    for (index, frame) in frames.iter().enumerate() {
        let number = frame["number"].as_u64().unwrap();
        let actual = render(&mut session, &mut renderer, index as f64);
        // Retain the native result before quality assertions: a failed matte
        // must remain inspectable rather than losing its pixels on TempDir drop.
        if let Some(output) = std::env::var_os("MOTION_PROFESSIONAL_KEY_ARTIFACTS") {
            let output = std::path::PathBuf::from(output).join("photographed/probes");
            fs::create_dir_all(&output).unwrap();
            for (label, pixels) in [("native", &actual), ("before-spill", &before[index])] {
                image::save_buffer(
                    output.join(format!("{number}-{label}.png")),
                    pixels,
                    WIDTH,
                    HEIGHT,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            image::GrayImage::from_fn(WIDTH, HEIGHT, |x, y| {
                image::Luma([actual[((y * WIDTH + x) * 4 + 3) as usize]])
            })
            .save(output.join(format!("{number}-matte.png")))
            .unwrap();
        }
        scores.push(json!({"number":number,"regions":check_regions(&actual,&manifest,number)}));
        let mut changed = 0;
        for (position, (a, b)) in actual
            .chunks_exact(4)
            .zip(before[index].chunks_exact(4))
            .enumerate()
        {
            assert_eq!(
                a[3], b[3],
                "spill changed coverage frame={number} pixel={position}"
            );
            assert!(
                a[0].abs_diff(b[0]) <= 1 && a[2].abs_diff(b[2]) <= 1,
                "spill changed non-key channel frame={number} pixel={position}"
            );
            assert!(
                a[1] <= b[1].saturating_add(1),
                "spill increased green frame={number} pixel={position}"
            );
            changed += usize::from(a[1].abs_diff(b[1]) > 1);
        }
        assert!(
            changed > 0,
            "spill must change surviving green frame={number}"
        );
        previews.push(actual);
    }
    same_pixels(
        &render(&mut session, &mut renderer, 1.5),
        &previews[1],
        "fractional still-image hold",
    );
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &without_spill);
    for (index, pixels) in before.iter().enumerate() {
        same_pixels(
            &render(&mut session, &mut renderer, index as f64),
            pixels,
            "spill undo",
        );
    }
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &initial);
    session.engine.redo().unwrap();
    assert_eq!(session.engine.project(), &without_spill);
    session.engine.redo().unwrap();
    assert_eq!(session.engine.project(), &edited);
    motion_core::storage::save(&root, &edited).unwrap();
    let mut reopened = Session::new(
        motion_core::storage::load(&root).unwrap(),
        root.clone(),
        motion_desktop_media::platform(),
    )
    .unwrap();
    for (index, pixels) in previews.iter().enumerate() {
        same_pixels(
            &render(&mut reopened, &mut renderer, index as f64),
            pixels,
            "fresh Session reopen",
        );
    }
    if let Some(output) = std::env::var_os("MOTION_PROFESSIONAL_KEY_ARTIFACTS") {
        let output = std::path::PathBuf::from(output).join("photographed");
        fs::create_dir_all(output.join("assets")).unwrap();
        fs::copy(fixtures.join("source.json"), output.join("source.json")).unwrap();
        fs::copy(fixtures.join("LICENSE.md"), output.join("LICENSE.md")).unwrap();
        fs::write(output.join("key.msfx"), &package.bytes).unwrap();
        for (name, value) in [
            ("initial-project.json", json!(initial)),
            ("project.json", json!(edited)),
            ("commands.json", json!(commands)),
            ("spill-commands.json", json!(spill_commands)),
            ("core-regions.json", json!(scores)),
        ] {
            fs::write(
                output.join(name),
                serde_json::to_vec_pretty(&value).unwrap(),
            )
            .unwrap();
        }
        for (index, frame) in frames.iter().enumerate() {
            let number = frame["number"].as_u64().unwrap();
            let file = frame["file"].as_str().unwrap();
            fs::copy(fixtures.join(file), output.join("assets").join(file)).unwrap();
            for (label, pixels) in [
                ("before-spill", &before[index]),
                ("native", &previews[index]),
            ] {
                image::save_buffer(
                    output.join(format!("{number}-{label}.png")),
                    pixels,
                    WIDTH,
                    HEIGHT,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            let rgba = image::RgbaImage::from_raw(WIDTH, HEIGHT, previews[index].clone()).unwrap();
            let alpha = image::GrayImage::from_fn(WIDTH, HEIGHT, |x, y| {
                image::Luma([rgba.get_pixel(x, y)[3]])
            });
            alpha
                .save(output.join(format!("{number}-matte.png")))
                .unwrap();
            for crop in manifest["crops"].as_array().unwrap() {
                let b = crop["box"].as_array().unwrap();
                let b: [u32; 4] = std::array::from_fn(|i| b[i].as_u64().unwrap() as u32);
                image::imageops::crop_imm(&rgba, b[0], b[1], b[2] - b[0], b[3] - b[1])
                    .to_image()
                    .save(output.join(format!("{number}-{}.png", crop["id"].as_str().unwrap())))
                    .unwrap();
            }
        }
    }
    session.frame = 1.5;
    let destination = directory.path().join("frozen.png");
    let frozen = FrameFreezePlan::capture(
        &session,
        Some(destination.clone()),
        &ExportControl::default(),
    )
    .unwrap();
    session.apply_commands(&json!([{"op":"delete","object":7},{"op":"delete","object":8},{"op":"delete","object":9},{"op":"delete","object":20},{"op":"delete","object":21},{"op":"delete","object":22}]).to_string()).unwrap();
    assert!(session.engine.project().layers.is_empty());
    drop(session);
    drop(reopened);
    fs::remove_file(plugin).unwrap();
    for frame in frames {
        fs::remove_file(root.join("assets").join(frame["file"].as_str().unwrap())).unwrap();
    }
    let receipt = frozen.export(&ExportControl::default()).unwrap();
    assert_eq!(receipt["frame"], 1.5);
    same_pixels(
        &image::open(&destination).unwrap().into_rgba8().into_raw(),
        &previews[1],
        "frozen PNG after source/package deletion and Session close",
    );
    if let Some(output) = std::env::var_os("MOTION_PROFESSIONAL_KEY_ARTIFACTS") {
        let output = std::path::PathBuf::from(output).join("photographed");
        fs::copy(destination, output.join("frozen.png")).unwrap();
        fs::write(
            output.join("frozen-receipt.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }
}
