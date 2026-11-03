//! Animated screen insert using the existing projective corner-pin effect.
use motion_core::{Asset, Content, EffectInstance, FrameRate, Layer, LayerTimeline, Project};
use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};
use motion_host::Session;
use motion_render::{image_resources::Resolution, OutputTransform, Renderer};
use serde_json::{json, Value};
use std::{fs, path::Path};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;
const FRAMES: [f64; 6] = [5., 7.5, 15., 0., 26., 10.];
// Account for the existing Unorm8 image/effect/color-space boundaries. This is
// declared before execution, applies to every RGBA channel and has no edge mask.
const MAX_CHANNEL_ERROR: u8 = 3;
const START: [[f64; 2]; 4] = [[26., 8.], [120., 20.], [110., 88.], [16., 80.]];
const END: [[f64; 2]; 4] = [[34., 8.], [120., 28.], [102., 88.], [16., 72.]];

fn source_pixel(x: u32, y: u32) -> [u8; 4] {
    [
        128 + (x / 2) as u8,
        128 + y as u8,
        224 - ((x + y) / 4) as u8,
        255,
    ]
}

fn quad(frame: f64) -> [[f64; 2]; 4] {
    let t = ((frame - 5.) / 10.).clamp(0., 1.);
    std::array::from_fn(|i| std::array::from_fn(|j| START[i][j] + (END[i][j] - START[i][j]) * t))
}

// Solve eight correspondence equations in f64 by pivoted Gaussian elimination.
// This does not invoke or reproduce the renderer's analytic quad inverse/WGSL.
fn inverse_homography(corners: [[f64; 2]; 4]) -> [f64; 8] {
    let mut rows = [[0.; 9]; 8];
    for (i, ([x, y], [u, v])) in corners
        .into_iter()
        .zip([[0., 0.], [1., 0.], [1., 1.], [0., 1.]])
        .enumerate()
    {
        rows[2 * i] = [x, y, 1., 0., 0., 0., -u * x, -u * y, u];
        rows[2 * i + 1] = [0., 0., 0., x, y, 1., -v * x, -v * y, v];
    }
    for column in 0..8 {
        let pivot = (column..8)
            .max_by(|&a, &b| rows[a][column].abs().total_cmp(&rows[b][column].abs()))
            .unwrap();
        rows.swap(column, pivot);
        let scale = rows[column][column];
        assert!(scale.abs() > 1e-12, "singular authored quad");
        for x in &mut rows[column][column..] {
            *x /= scale;
        }
        for row in 0..8 {
            if row == column {
                continue;
            }
            let factor = rows[row][column];
            for j in column..9 {
                rows[row][j] -= factor * rows[column][j];
            }
        }
    }
    std::array::from_fn(|i| rows[i][8])
}

fn bilinear(u: f64, v: f64) -> [u8; 4] {
    let sx = (u * WIDTH as f64 - 0.5).clamp(0., (WIDTH - 1) as f64);
    let sy = (v * HEIGHT as f64 - 0.5).clamp(0., (HEIGHT - 1) as f64);
    let (x, y) = (sx.floor() as u32, sy.floor() as u32);
    let (tx, ty) = (sx - x as f64, sy - y as f64);
    let samples = [
        source_pixel(x, y),
        source_pixel((x + 1).min(WIDTH - 1), y),
        source_pixel(x, (y + 1).min(HEIGHT - 1)),
        source_pixel((x + 1).min(WIDTH - 1), (y + 1).min(HEIGHT - 1)),
    ];
    std::array::from_fn(|c| {
        let a = samples[0][c] as f64 * (1. - tx) + samples[1][c] as f64 * tx;
        let b = samples[2][c] as f64 * (1. - tx) + samples[3][c] as f64 * tx;
        (a * (1. - ty) + b * ty).round() as u8
    })
}

fn raster(frame: f64, corners: [[f64; 2]; 4]) -> Vec<u8> {
    let h = inverse_homography(corners);
    let mut pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let denominator = h[6] * px + h[7] * py + 1.;
            let u = (h[0] * px + h[1] * py + h[2]) / denominator;
            let v = (h[3] * px + h[4] * py + h[5]) / denominator;
            let sample = if (5. ..25.).contains(&frame)
                && (0. ..=1.).contains(&u)
                && (0. ..=1.).contains(&v)
            {
                bilinear(u, v)
            } else {
                [0, 0, 255, 255]
            };
            let i = ((y * WIDTH + x) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&sample);
        }
    }
    pixels
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
        [WIDTH as f32, HEIGHT as f32],
    );
    let mut layer = Layer::solid(
        7,
        "Animated projective screen insert",
        [WIDTH as f32, HEIGHT as f32],
        [WIDTH as f32 / 2., HEIGHT as f32 / 2., 0.],
        [1.; 4],
    );
    layer.content = Content::Image { asset: 1 };
    layer.timeline = Some(LayerTimeline {
        in_frame: 5,
        out_frame: 25,
        offset_frame: 5,
    });
    let mut commands = vec![
        json!({"op":"add","layer":layer}),
        json!({"op":"effect","object":7,"action":{"kind":"insert","instance":effect}}),
    ];
    for (i, param) in ["tl", "tr", "br", "bl"].into_iter().enumerate() {
        commands.extend([
            json!({"op":"effect","object":7,"action":{"kind":"set","effect":1,"param":param,"frame":5,"value":[START[i][0],START[i][1],0,0]}}),
            json!({"op":"effect","object":7,"action":{"kind":"animate","effect":1,"param":param,"frame":5,"enabled":true}}),
            json!({"op":"effect","object":7,"action":{"kind":"set","effect":1,"param":param,"frame":15,"value":[END[i][0],END[i][1],0,0]}}),
        ]);
    }
    commands
}

fn render(s: &mut Session, renderer: &mut Renderer, frame: f64) -> Vec<u8> {
    eprintln!("screen phase=sample frame={frame}");
    s.frame = frame;
    s.sample().unwrap();
    renderer.set_effect_registry(s.effects.registry.clone());
    renderer
        .configure_assets_with_scene(s.engine.project(), &s.root, &s.scene)
        .unwrap();
    assert!(renderer
        .prepare_scene_assets(&s.scene, Resolution::Full, false)
        .unwrap());
    let target = renderer
        .sdr_capture_target(WIDTH, HEIGHT, OutputTransform::SDR_CLIP)
        .unwrap();
    renderer.capture_sdr(&s.scene, &target).unwrap().0.pixels
}

fn png(path: &Path, pixels: &[u8]) {
    image::save_buffer(path, pixels, WIDTH, HEIGHT, image::ColorType::Rgba8).unwrap();
}

fn compare(actual: &[u8], expected: &[u8], phase: &str) -> u8 {
    assert_eq!(actual.len(), expected.len());
    let worst = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    if worst > MAX_CHANNEL_ERROR {
        if let Some((index, (a, b))) = actual
            .chunks_exact(4)
            .zip(expected.chunks_exact(4))
            .enumerate()
            .find(|(_, (a, b))| {
                a.iter()
                    .zip(*b)
                    .any(|(a, b)| a.abs_diff(*b) > MAX_CHANNEL_ERROR)
            })
        {
            eprintln!(
                "{phase}: first mismatch ({},{}) actual={a:?} expected={b:?}",
                index % WIDTH as usize,
                index / WIDTH as usize
            );
        }
        if let Some(output) = std::env::var_os("MOTION_PROFESSIONAL_SCREEN_ARTIFACTS") {
            let output = std::path::PathBuf::from(output);
            fs::create_dir_all(&output).unwrap();
            png(&output.join("failed-native.png"), actual);
            png(&output.join("failed-golden.png"), expected);
        }
    }
    assert!(
        worst <= MAX_CHANNEL_ERROR,
        "{phase}: maximum channel error {worst} exceeds {MAX_CHANNEL_ERROR}"
    );
    worst
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn animated_screen_insert_matches_independent_projective_goldens_and_frozen_outputs() {
    eprintln!("screen phase=create_fixture");
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    fs::create_dir_all(root.join("assets")).unwrap();
    let source_path = root.join("assets/screen.png");
    image::RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| image::Rgba(source_pixel(x, y)))
        .save(&source_path)
        .unwrap();
    let encoded_source = fs::read(&source_path).unwrap();
    let mut initial =
        Project::new_with_frame_rate(WIDTH, HEIGHT, FrameRate::new(30000, 1001).unwrap(), 30)
            .unwrap();
    initial.background = [0., 0., 1., 1.];
    initial.assets.push(Asset {
        id: 1,
        path: "assets/screen.png".into(),
        width: WIDTH,
        height: HEIGHT,
    });
    eprintln!("screen phase=open_session");
    let mut session = Session::new(
        initial.clone(),
        root.clone(),
        motion_desktop_media::platform(),
    )
    .unwrap();
    let commands = commands();
    eprintln!("screen phase=apply_commands");
    session
        .apply_commands(&serde_json::to_string(&commands).unwrap())
        .unwrap();
    let edited = session.engine.snapshot();
    for param in ["tl", "tr", "br", "bl"] {
        assert_eq!(
            edited.layers[0].effects[0].params[param]
                .track
                .keys
                .iter()
                .map(|k| k.frame)
                .collect::<Vec<_>>(),
            vec![0, 10]
        );
    }
    eprintln!("screen phase=create_renderer");
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    eprintln!("screen adapter={:?}", renderer.adapter_info);
    let mut previews = Vec::new();
    let mut comparisons = Vec::new();
    for frame in FRAMES {
        let native = render(&mut session, &mut renderer, frame);
        let golden = raster(frame, quad(frame));
        comparisons.push(json!({"frame":frame,"maxChannelError":compare(&native,&golden,"native projective screen")}));
        previews.push(native);
    }
    let changed = previews[0]
        .chunks_exact(4)
        .zip(previews[2].chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > 128,
        "animated corners must visibly move/warp the insert"
    );
    let mut affine = START;
    affine[2] = [
        START[1][0] + START[3][0] - START[0][0],
        START[1][1] + START[3][1] - START[0][1],
    ];
    let affine_control = raster(5., affine);
    let perspective = raster(5., START);
    let distinguishing_pixels = affine_control
        .chunks_exact(4)
        .zip(perspective.chunks_exact(4))
        .filter(|(a, b)| {
            a.iter()
                .zip(*b)
                .any(|(a, b)| a.abs_diff(*b) > MAX_CHANNEL_ERROR)
        })
        .count();
    assert!(
        distinguishing_pixels > 128,
        "fixture must reject an affine substitute"
    );

    eprintln!("screen phase=undo");
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &initial);
    assert_eq!(
        render(&mut session, &mut renderer, 7.5),
        [0, 0, 255, 255].repeat((WIDTH * HEIGHT) as usize)
    );
    eprintln!("screen phase=redo");
    session.engine.redo().unwrap();
    assert_eq!(session.engine.project(), &edited);
    assert_eq!(render(&mut session, &mut renderer, 7.5), previews[1]);
    eprintln!("screen phase=save_reopen");
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
    for (i, frame) in FRAMES
        .into_iter()
        .enumerate()
        .filter(|(_, f)| (5. ..25.).contains(f))
    {
        eprintln!("screen phase=capture_frozen frame={frame}");
        session.frame = frame;
        plans.push((
            i,
            FrameFreezePlan::capture(
                &session,
                Some(directory.path().join(format!("screen-{frame}.png"))),
                &ExportControl::default(),
            )
            .unwrap(),
        ));
    }
    eprintln!("screen phase=delete_live_layer");
    session
        .apply_commands(&json!({"op":"delete","object":7}).to_string())
        .unwrap();
    assert_eq!(
        render(&mut session, &mut renderer, 7.5),
        [0, 0, 255, 255].repeat((WIDTH * HEIGHT) as usize)
    );
    eprintln!("screen phase=remove_source_close_sessions");
    fs::remove_file(&source_path).unwrap();
    drop(session);
    drop(reopened);
    let output =
        std::env::var_os("MOTION_PROFESSIONAL_SCREEN_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        fs::create_dir_all(output.join("assets")).unwrap();
        fs::write(output.join("assets/screen.png"), encoded_source).unwrap();
        for (name, value) in [
            ("initial-project.json", json!(initial)),
            ("project.json", json!(edited)),
            ("commands.json", json!(commands)),
        ] {
            fs::write(
                output.join(name),
                serde_json::to_vec_pretty(&value).unwrap(),
            )
            .unwrap();
        }
        png(&output.join("affine-control.png"), &affine_control);
    }
    let mut frozen = Vec::new();
    for (i, plan) in plans {
        eprintln!("screen phase=export_frozen frame={}", FRAMES[i]);
        let receipt = plan.export(&ExportControl::default()).unwrap();
        assert_eq!(receipt["revision"], revision);
        let actual = image::open(receipt["path"].as_str().unwrap())
            .unwrap()
            .into_rgba8();
        assert_eq!(actual.dimensions(), (WIDTH, HEIGHT));
        assert_eq!(actual.as_raw(), &previews[i]);
        compare(
            actual.as_raw(),
            &raster(FRAMES[i], quad(FRAMES[i])),
            "independent frozen screen",
        );
        if let Some(output) = &output {
            actual
                .save(output.join(format!("frozen-{}.png", FRAMES[i])))
                .unwrap();
        }
        frozen.push(json!({"frame":FRAMES[i],"revision":revision,"exact":true}));
        eprintln!("screen phase=export_verified frame={}", FRAMES[i]);
    }
    if let Some(output) = &output {
        for (i, frame) in FRAMES.into_iter().enumerate() {
            png(&output.join(format!("native-{frame}.png")), &previews[i]);
            png(
                &output.join(format!("golden-{frame}.png")),
                &raster(frame, quad(frame)),
            );
        }
        fs::write(output.join("result.json"),serde_json::to_vec_pretty(&json!({
            "scope":"Public animated four-corner PNG screen insertion, independent f64 homography/full-frame golden, history/reopen and frozen source handles",
            "limits":["Repository-authored procedural insert, not photographed footage or automatic tracking","Default Unorm8 headless, not window capture or other working precisions","Fixed integer output extent; fractional bounding-box/resizing quality not accepted by this fixture"],
            "author":"Motion Studio repository contributors","license":"MIT","externalAssets":false,
            "maximumChannelErrorBudget":MAX_CHANNEL_ERROR,"comparisons":comparisons,"frozen":frozen,
            "adapter":{"name":renderer.adapter_info.name,"backend":format!("{:?}",renderer.adapter_info.backend),"driver":renderer.adapter_info.driver,"driverInfo":renderer.adapter_info.driver_info},
            "animatedChangedPixels":changed,"affineControlDistinguishingPixels":distinguishing_pixels
        })).unwrap()).unwrap();
    }
    eprintln!("screen phase=complete");
}
