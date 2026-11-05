//! Static photographed screen replacement, separate from tracking/edge quality.
use motion_core::{Asset, Content, EffectInstance, FrameRate, Layer, LayerTimeline, Project};
use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};
use motion_host::Session;
use motion_render::{image_resources::Resolution, OutputTransform, Renderer};
use serde_json::{json, Value};
use std::{fs, path::Path};

const W: u32 = 960;
const H: u32 = 654;
const CORNERS: [[f64; 2]; 4] = [[168., 60.], [219., 74.], [205., 116.], [154., 106.]];
const FRAMES: [f64; 5] = [5., 7.5, 15., 0., 26.];
// Declared in source.json before native execution; no observed-error fitting.
const MAX_ERROR: u8 = 3;
const EDGE_EXCLUSION: f64 = 2.;
const MIN_INTERIOR: usize = 1500;
const PHOTO: &[u8] = include_bytes!("fixtures/professional-screen/destiny-screen.png");

fn source_pixel(x: u32, y: u32) -> [u8; 4] {
    [
        128 + (x * 96 / (W - 1)) as u8,
        128 + (y * 96 / (H - 1)) as u8,
        224 - (x * 32 / (W - 1)) as u8,
        255,
    ]
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
    (255.
        * if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        })
    .round() as u8
}

// Independent eight-equation f64 correspondence solve, not renderer WGSL.
fn homography() -> [f64; 8] {
    let mut rows = [[0.; 9]; 8];
    for (i, ([x, y], [u, v])) in CORNERS
        .into_iter()
        .zip([[0., 0.], [1., 0.], [1., 1.], [0., 1.]])
        .enumerate()
    {
        rows[2 * i] = [x, y, 1., 0., 0., 0., -u * x, -u * y, u];
        rows[2 * i + 1] = [0., 0., 0., x, y, 1., -v * x, -v * y, v];
    }
    for c in 0..8 {
        let pivot = (c..8)
            .max_by(|&a, &b| rows[a][c].abs().total_cmp(&rows[b][c].abs()))
            .unwrap();
        rows.swap(c, pivot);
        let divisor = rows[c][c];
        assert!(divisor.abs() > 1e-12);
        for x in &mut rows[c][c..] {
            *x /= divisor;
        }
        for r in 0..8 {
            if r == c {
                continue;
            }
            let factor = rows[r][c];
            for j in c..9 {
                rows[r][j] -= factor * rows[c][j];
            }
        }
    }
    std::array::from_fn(|i| rows[i][8])
}

fn sample(u: f64, v: f64) -> [f64; 3] {
    let x = (u * f64::from(W) - 0.5).clamp(0., f64::from(W - 1));
    let y = (v * f64::from(H) - 0.5).clamp(0., f64::from(H - 1));
    let (ix, iy) = (x.floor() as u32, y.floor() as u32);
    let (tx, ty) = (x - f64::from(ix), y - f64::from(iy));
    let p = [
        source_pixel(ix, iy),
        source_pixel((ix + 1).min(W - 1), iy),
        source_pixel(ix, (iy + 1).min(H - 1)),
        source_pixel((ix + 1).min(W - 1), (iy + 1).min(H - 1)),
    ];
    std::array::from_fn(|c| {
        let top = linear(p[0][c]) * (1. - tx) + linear(p[1][c]) * tx;
        let bottom = linear(p[2][c]) * (1. - tx) + linear(p[3][c]) * tx;
        top * (1. - ty) + bottom * ty
    })
}

fn reference(photo: &[u8], frame: f64) -> (Vec<u8>, Vec<bool>, usize) {
    let h = homography();
    let mut result = photo.to_vec();
    let mut checked = vec![true; (W * H) as usize];
    let mut interior = 0;
    if !(5. ..25.).contains(&frame) {
        return (result, checked, interior);
    }
    let opacity = 0.35 + 0.55 * ((frame - 5.) / 10.).clamp(0., 1.);
    for y in 56..121 {
        for x in 150..224 {
            let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let denominator = h[6] * px + h[7] * py + 1.;
            let u = (h[0] * px + h[1] * py + h[2]) / denominator;
            let v = (h[3] * px + h[4] * py + h[5]) / denominator;
            let i = (y * W + x) as usize;
            let distance = (0..4)
                .map(|j| {
                    let [ax, ay] = CORNERS[j];
                    let [bx, by] = CORNERS[(j + 1) % 4];
                    ((bx - ax) * (py - ay) - (by - ay) * (px - ax)).abs() / (bx - ax).hypot(by - ay)
                })
                .fold(f64::INFINITY, f64::min);
            if distance < EDGE_EXCLUSION {
                checked[i] = false;
            }
            if (0. ..=1.).contains(&u) && (0. ..=1.).contains(&v) {
                if checked[i] {
                    interior += 1;
                }
                let source = sample(u, v);
                for c in 0..3 {
                    result[4 * i + c] =
                        encoded(source[c] * opacity + linear(photo[4 * i + c]) * (1. - opacity));
                }
            }
        }
    }
    (result, checked, interior)
}

fn commands() -> Vec<Value> {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "corner_pin")
        .unwrap();
    let effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [W as f32, H as f32],
    );
    let mut layer = Layer::solid(
        7,
        "Authored replacement screen",
        [W as f32, H as f32],
        [W as f32 / 2., H as f32 / 2., 0.],
        [1.; 4],
    );
    layer.content = Content::Image { asset: 2 };
    layer.timeline = Some(LayerTimeline {
        in_frame: 5,
        out_frame: 25,
        offset_frame: 5,
    });
    let mut commands = vec![
        json!({"op":"add","layer":layer}),
        json!({"op":"effect","object":7,"action":{"kind":"insert","instance":effect}}),
    ];
    for (name, [x, y]) in ["tl", "tr", "br", "bl"].into_iter().zip(CORNERS) {
        commands.push(json!({"op":"effect","object":7,"action":{"kind":"set","effect":1,"param":name,"frame":5,"value":[x,y,0,0]}}));
    }
    commands.extend([
        json!({"op":"set_scalar","object":7,"property":"opacity","frame":5,"value":0.35}),
        json!({"op":"animate","object":7,"property":"opacity","axis":null,"frame":5,"enabled":true}),
        json!({"op":"set_scalar","object":7,"property":"opacity","frame":15,"value":0.9})
    ]);
    commands
}

fn render(s: &mut Session, r: &mut Renderer, frame: f64) -> Vec<u8> {
    s.frame = frame;
    s.sample().unwrap();
    r.set_effect_registry(s.effects.registry.clone());
    r.configure_assets_with_scene(s.engine.project(), &s.root, &s.scene)
        .unwrap();
    assert!(r
        .prepare_scene_assets(&s.scene, Resolution::Full, false)
        .unwrap());
    let target = r
        .sdr_capture_target(W, H, OutputTransform::SDR_CLIP)
        .unwrap();
    r.capture_sdr(&s.scene, &target).unwrap().0.pixels
}

fn png(path: &Path, pixels: &[u8]) {
    image::save_buffer(path, pixels, W, H, image::ColorType::Rgba8).unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn photographed_screen_opacity_history_reopen_and_frozen_output_match_independent_reference() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join("assets/photo.png"), PHOTO).unwrap();
    let photo = image::load_from_memory(PHOTO).unwrap().into_rgba8();
    assert_eq!(photo.dimensions(), (W, H));
    image::RgbaImage::from_fn(W, H, |x, y| image::Rgba(source_pixel(x, y)))
        .save(root.join("assets/insert.png"))
        .unwrap();
    let source_bytes = fs::read(root.join("assets/insert.png")).unwrap();
    let mut initial =
        Project::new_with_frame_rate(W, H, FrameRate::new(30000, 1001).unwrap(), 30).unwrap();
    initial.assets.extend([
        Asset {
            id: 1,
            path: "assets/photo.png".into(),
            width: W,
            height: H,
        },
        Asset {
            id: 2,
            path: "assets/insert.png".into(),
            width: W,
            height: H,
        },
    ]);
    let mut background = Layer::solid(
        1,
        "NASA / ISS crew photograph",
        [W as f32, H as f32],
        [W as f32 / 2., H as f32 / 2., 0.],
        [1.; 4],
    );
    background.content = Content::Image { asset: 1 };
    initial.layers.push(background);
    let out =
        std::env::var_os("MOTION_PHOTOGRAPHED_SCREEN_ARTIFACTS").map(std::path::PathBuf::from);
    let commands = commands();
    if let Some(out) = &out {
        fs::create_dir_all(out.join("assets")).unwrap();
        fs::write(out.join("assets/photo.png"), PHOTO).unwrap();
        fs::write(out.join("assets/insert.png"), source_bytes).unwrap();
        fs::write(
            out.join("initial-project.json"),
            serde_json::to_vec_pretty(&initial).unwrap(),
        )
        .unwrap();
        fs::write(
            out.join("commands.json"),
            serde_json::to_vec_pretty(&commands).unwrap(),
        )
        .unwrap();
    }
    let mut session = Session::new(
        initial.clone(),
        root.clone(),
        motion_desktop_media::platform(),
    )
    .unwrap();
    session
        .apply_commands(&serde_json::to_string(&commands).unwrap())
        .unwrap();
    let edited = session.engine.snapshot();
    assert_eq!(
        edited.layers[1]
            .transform
            .opacity
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        vec![0, 10]
    );
    if let Some(out) = &out {
        fs::write(
            out.join("project.json"),
            serde_json::to_vec_pretty(&edited).unwrap(),
        )
        .unwrap();
    }
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut previews = Vec::new();
    let mut comparisons = Vec::new();
    for frame in FRAMES {
        let native = render(&mut session, &mut renderer, frame);
        let (golden, checked, interior) = reference(photo.as_raw(), frame);
        if let Some(out) = &out {
            png(&out.join(format!("native-{frame}.png")), &native);
            png(&out.join(format!("golden-{frame}.png")), &golden);
        }
        let worst = native
            .chunks_exact(4)
            .zip(golden.chunks_exact(4))
            .zip(&checked)
            .filter(|(_, checked)| **checked)
            .flat_map(|((a, b), _)| a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)))
            .max()
            .unwrap();
        comparisons.push(json!({"frame":frame,"maximumChannelError":worst,"checkedPixels":checked.iter().filter(|x|**x).count(),"checkedInsertPixels":interior}));
        eprintln!("photo_screen frame={frame} error={worst} interior={interior}");
        assert!(
            worst <= MAX_ERROR,
            "frame={frame} error={worst} budget={MAX_ERROR}"
        );
        if (5. ..25.).contains(&frame) {
            assert!(interior >= MIN_INTERIOR);
        }
        previews.push(native);
    }
    assert!(
        previews[0]
            .chunks_exact(4)
            .zip(previews[2].chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count()
            >= MIN_INTERIOR
    );
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &initial);
    let undone = render(&mut session, &mut renderer, 7.5);
    assert!(undone
        .iter()
        .zip(photo.as_raw())
        .all(|(a, b)| a.abs_diff(*b) <= MAX_ERROR));
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
    assert_eq!(reopened.engine.project(), &edited);
    for (i, frame) in FRAMES.into_iter().enumerate() {
        assert_eq!(render(&mut reopened, &mut renderer, frame), previews[i]);
    }
    let revision = session.engine.revision();
    let mut plans = Vec::new();
    for (i, frame) in FRAMES.into_iter().enumerate().take(3) {
        session.frame = frame;
        plans.push((
            i,
            FrameFreezePlan::capture(
                &session,
                Some(temp.path().join(format!("frozen-{frame}.png"))),
                &ExportControl::default(),
            )
            .unwrap(),
        ));
    }
    session
        .apply_commands(&json!({"op":"delete","object":7}).to_string())
        .unwrap();
    drop(session);
    drop(reopened);
    drop(renderer);
    fs::remove_file(root.join("assets/photo.png")).unwrap();
    fs::remove_file(root.join("assets/insert.png")).unwrap();
    for (i, plan) in plans {
        let receipt = plan.export(&ExportControl::default()).unwrap();
        assert_eq!(receipt["revision"], revision);
        let pixels = image::open(receipt["path"].as_str().unwrap())
            .unwrap()
            .into_rgba8();
        assert_eq!(pixels.as_raw(), &previews[i]);
        if let Some(out) = &out {
            pixels
                .save(out.join(format!("frozen-{}.png", FRAMES[i])))
                .unwrap();
        }
    }
    if let Some(out) = &out {
        fs::write(out.join("result.json"),serde_json::to_vec_pretty(&json!({
            "actualNativeGpu":true,"publicApi":true,"historyAndReopen":true,"frozenAfterOwnersAndSourcesRemoved":true,
            "comparisons":comparisons,"maximumChannelErrorBudget":MAX_ERROR,"edgeExclusionPixels":EDGE_EXCLUSION,
            "minimumCheckedInsertPixels":MIN_INTERIOR,"author":"NASA / ISS crew photograph; independently authored insert and reference",
            "limits":["Static authored screen alignment, not automatic tracking or real-footage temporal acceptance","Stable interior and background only; two-pixel edge exclusion","Default Unorm8 SDR output, not HDR, other working precisions or photometric matching"]
        })).unwrap()).unwrap();
    }
}
