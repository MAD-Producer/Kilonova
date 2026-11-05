//! Real PNG source relink through the public host protocol and native renderer.
use motion_core::{Asset, Content, FrameRate, Layer, LayerTimeline, Project};
use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};
use motion_host::{ops, session};
use motion_render::{image_resources::Resolution, OutputTransform, Renderer};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;
// Source colors are binary sRGB and alpha; allow one Unorm8 rounding unit.
const MAX_CHANNEL_ERROR: u8 = 1;

struct Registered(i64);
impl Drop for Registered {
    fn drop(&mut self) {
        session::close(self.0);
    }
}

fn request(id: i64, value: Value) -> Value {
    ops::media::request(id, value, None).unwrap()
}

fn state(id: i64) -> Project {
    session::with_session(id, |s| Ok(s.engine.snapshot())).unwrap()
}

fn render(id: i64, renderer: &mut Renderer, frame: f64) -> Vec<u8> {
    session::with_session(id, |s| {
        s.frame = frame;
        s.sample()?;
        renderer.set_effect_registry(s.effects.registry.clone());
        renderer.configure_assets_with_scene(s.engine.project(), &s.root, &s.scene)?;
        assert!(renderer.prepare_scene_assets(&s.scene, Resolution::Full, false)?);
        let target = renderer.sdr_capture_target(WIDTH, HEIGHT, OutputTransform::SDR_CLIP)?;
        Ok(renderer.capture_sdr(&s.scene, &target)?.0.pixels)
    })
    .unwrap()
}

fn source_pixel(source: usize, version: usize, x: u32, y: u32) -> [u8; 4] {
    if !(2..46).contains(&y) || !(2..62).contains(&x) || (x >= 32) != (source == 1) {
        return [0; 4];
    }
    let checker = (x / 4 + y / 4) % 2 == 0;
    let colors = [
        [
            [[255, 0, 0, 255], [255, 255, 255, 255]],
            [[0, 255, 255, 255], [0, 0, 0, 255]],
        ],
        [
            [[0, 255, 0, 255], [255, 255, 255, 255]],
            [[0, 0, 255, 255], [255, 255, 0, 255]],
        ],
    ];
    colors[source][version][usize::from(checker)]
}

fn source(path: &Path, index: usize, version: usize) {
    image::RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
        image::Rgba(source_pixel(index, version, x, y))
    })
    .save(path)
    .unwrap();
}

// Independent frame arrangement: two disjoint opaque plates and clear border.
// The actual project uses two full-size images with complementary alpha halves.
fn golden(version: usize, frame: f64) -> Vec<u8> {
    let mut out = vec![0; (WIDTH * HEIGHT * 4) as usize];
    if !(5. ..25.).contains(&frame) {
        return out;
    }
    for y in 2..46 {
        for x in 2..62 {
            let checker = (x / 4 + y / 4) % 2 == 0;
            let rgb = match (version, x < 32, checker) {
                (0, _, true) => [255, 255, 255],
                (0, true, false) => [255, 0, 0],
                (0, false, false) => [0, 255, 0],
                (1, true, true) => [0, 0, 0],
                (1, true, false) => [0, 255, 255],
                (1, false, true) => [255, 255, 0],
                (1, false, false) => [0, 0, 255],
                _ => unreachable!(),
            };
            let offset = ((y * WIDTH + x) * 4) as usize;
            out[offset..offset + 3].copy_from_slice(&rgb);
            out[offset + 3] = 255;
        }
    }
    out
}

fn compare(actual: &[u8], expected: &[u8], phase: &str) -> u8 {
    assert_eq!(actual.len(), expected.len(), "{phase}");
    let max = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        max <= MAX_CHANNEL_ERROR,
        "{phase}: maximum channel error {max} exceeds {MAX_CHANNEL_ERROR}"
    );
    max
}

fn ready(id: i64, name: &str) -> Value {
    let began = Instant::now();
    loop {
        let status = request(id, json!({"op":"media_status","request_id":name}));
        if status["state"] != "running" {
            return status;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "{name}: {status}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn begin_request(name: &str, paths: &[PathBuf], inventory: &Value) -> Value {
    json!({"op":"start_asset_relink","request_id":name,
            "revision":inventory["revision"],"document_generation":inventory["document_generation"],
            "candidates":paths.iter().enumerate().map(|(i,path)|
                json!({"target":{"kind":"image","id":7+i as u64},"path":path})).collect::<Vec<_>>()
    })
}

fn begin(id: i64, name: &str, paths: &[PathBuf]) -> Value {
    let inventory = request(id, json!({"op":"missing_media_assets"}));
    request(id, begin_request(name, paths, &inventory))
}

fn finish(id: i64, name: &str) -> Value {
    let result = request(id, json!({"op":"finish_asset_relink","request_id":name}));
    assert_eq!(result["task"]["state"], "succeeded");
    assert_eq!(
        request(id, json!({"op":"release_asset_relink","request_id":name}))["released"],
        true
    );
    result
}

fn freeze(id: i64, path: PathBuf) -> (FrameFreezePlan, u64) {
    session::with_session(id, |s| {
        s.frame = 7.5;
        Ok((
            FrameFreezePlan::capture(s, Some(path), &ExportControl::default()).unwrap(),
            s.engine.revision(),
        ))
    })
    .unwrap()
}

fn png(path: &Path, pixels: &[u8]) {
    image::save_buffer(path, pixels, WIDTH, HEIGHT, image::ColorType::Rgba8).unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn public_relink_refreshes_pixels_and_preserves_history_missing_recovery_and_frozen_sources() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    fs::create_dir_all(root.join("assets")).unwrap();
    let mut initial =
        Project::new_with_frame_rate(WIDTH, HEIGHT, FrameRate::new(30000, 1001).unwrap(), 30)
            .unwrap();
    initial.background = [0.; 4];
    for i in 0..2 {
        let asset = Asset {
            id: 7 + i,
            path: format!("assets/plate-{i}.png"),
            width: WIDTH,
            height: HEIGHT,
        };
        source(&root.join(&asset.path), i as usize, 0);
        let mut layer = Layer::solid(
            11 + i,
            &format!("Relink plate {i}"),
            [WIDTH as f32, HEIGHT as f32],
            [WIDTH as f32 / 2., HEIGHT as f32 / 2., 0.],
            [1.; 4],
        );
        layer.content = Content::Image { asset: asset.id };
        layer.timeline = Some(LayerTimeline {
            in_frame: 5,
            out_frame: 25,
            offset_frame: 5,
        });
        initial.assets.push(asset);
        initial.layers.push(layer);
    }
    motion_core::storage::save(&root, &initial).unwrap();
    let host = ops::Host::new(motion_desktop_media::platform());
    let live = Registered(host.create(root.clone(), "").unwrap());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let original = render(live.0, &mut renderer, 7.5);
    let original_error = compare(&original, &golden(0, 7.5), "initial PNG decode");
    let (old_frozen, old_revision) = freeze(live.0, directory.path().join("old-frozen.png"));
    let paths: Vec<_> = (0..2)
        .map(|i| {
            let path = directory.path().join(format!("replacement-{i}.png"));
            source(&path, i, 1);
            path
        })
        .collect();
    let before = ops::project::state(live.0).unwrap();
    let document = fs::read(root.join("project.json")).unwrap();
    let staged = begin(live.0, "two-plates", &paths);
    assert_eq!(staged["state"], "running");
    let prepared = ready(live.0, "two-plates");
    assert_eq!(prepared["state"], "ready", "{prepared}");
    assert_eq!(state(live.0), initial, "staging must not edit the project");
    assert_eq!(
        ops::project::state(live.0).unwrap()["revision"],
        before["revision"]
    );
    assert_eq!(fs::read(root.join("project.json")).unwrap(), document);
    assert_eq!(
        render(live.0, &mut renderer, 7.5),
        original,
        "staging must not change pixels"
    );
    let committed = finish(live.0, "two-plates");
    assert_eq!(
        committed["state"]["revision"].as_u64().unwrap(),
        before["revision"].as_u64().unwrap() + 1
    );
    let replaced = state(live.0);
    assert_eq!(replaced.layers, initial.layers);
    for (previous, next) in initial.assets.iter().zip(&replaced.assets) {
        assert_eq!(previous.id, next.id);
        assert_eq!((next.width, next.height), (WIDTH, HEIGHT));
        assert_ne!(previous.path, next.path);
        assert_eq!(
            Path::new(&previous.path).file_name(),
            Path::new(&next.path).file_name()
        );
    }
    let mut comparisons = Vec::new();
    for frame in [5., 7.5, 15., 0., 26., 10.] {
        comparisons.push(json!({"frame":frame,"maxChannelError":compare(&render(live.0,&mut renderer,frame),&golden(1,frame),"relinked retained renderer")}));
    }
    let replacement = render(live.0, &mut renderer, 7.5);
    assert_ne!(
        replacement, original,
        "two real source replacements must change output"
    );
    let (new_frozen, new_revision) = freeze(live.0, directory.path().join("new-frozen.png"));
    ops::editing::history(live.0, ops::editing::HISTORY_UNDO).unwrap();
    assert_eq!(state(live.0), initial, "one undo restores both sources");
    assert_eq!(render(live.0, &mut renderer, 7.5), original);
    ops::editing::history(live.0, ops::editing::HISTORY_REDO).unwrap();
    assert_eq!(state(live.0), replaced);
    assert_eq!(render(live.0, &mut renderer, 7.5), replacement);

    let broken = directory.path().join("broken.png");
    fs::write(&broken, b"not an image").unwrap();
    let bad_before = ops::project::state(live.0).unwrap();
    let bad_document = fs::read(root.join("project.json")).unwrap();
    begin(live.0, "bad-source", &[broken, paths[1].clone()]);
    let rejected = ready(live.0, "bad-source");
    assert_eq!(rejected["state"], "failed", "{rejected}");
    assert!(rejected["error"].as_str().is_some_and(|e| !e.is_empty()));
    assert!(ops::media::request(
        live.0,
        json!({"op":"finish_asset_relink","request_id":"bad-source"}),
        None
    )
    .is_err());
    assert_eq!(state(live.0), replaced);
    assert_eq!(
        ops::project::state(live.0).unwrap()["revision"],
        bad_before["revision"]
    );
    assert_eq!(fs::read(root.join("project.json")).unwrap(), bad_document);
    request(
        live.0,
        json!({"op":"release_asset_relink","request_id":"bad-source"}),
    );
    assert_eq!(render(live.0, &mut renderer, 7.5), replacement);

    // Remove only authored fixture files. A real two-source missing-media batch
    // must restore pixels in one edit without touching layer identities/tracks.
    for asset in &replaced.assets {
        fs::remove_file(root.join(&asset.path)).unwrap();
    }
    let missing = request(live.0, json!({"op":"missing_media_assets"}));
    assert_eq!(missing["assets"].as_array().unwrap().len(), 2);
    begin(live.0, "recover-missing", &paths);
    let ready_missing = ready(live.0, "recover-missing");
    assert_eq!(ready_missing["state"], "ready", "{ready_missing}");
    let recovered_receipt = finish(live.0, "recover-missing");
    assert_eq!(
        recovered_receipt["state"]["revision"].as_u64().unwrap(),
        missing["revision"].as_u64().unwrap() + 1
    );
    assert!(
        request(live.0, json!({"op":"missing_media_assets"}))["assets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let recovered = state(live.0);
    assert_eq!(recovered.layers, initial.layers);
    assert_eq!(render(live.0, &mut renderer, 7.5), replacement);
    ops::project::save(live.0).unwrap();
    drop(live);
    let reopened = Registered(host.create(root.clone(), "").unwrap());
    assert_eq!(state(reopened.0), recovered);
    assert_eq!(render(reopened.0, &mut renderer, 7.5), replacement);
    ops::editing::command(
        reopened.0,
        &json!([{"op":"delete","object":11},{"op":"delete","object":12}]).to_string(),
    )
    .unwrap();
    assert!(render(reopened.0, &mut renderer, 7.5)
        .iter()
        .all(|b| *b == 0));
    drop(reopened);

    let mut freezes = Vec::new();
    for (plan, revision, expected) in [
        (old_frozen, old_revision, &original),
        (new_frozen, new_revision, &replacement),
    ] {
        let receipt = plan.export(&ExportControl::default()).unwrap();
        assert_eq!(receipt["revision"], revision);
        let pixels = image::open(receipt["path"].as_str().unwrap())
            .unwrap()
            .into_rgba8();
        assert_eq!(pixels.dimensions(), (WIDTH, HEIGHT));
        assert_eq!(
            pixels.as_raw(),
            expected,
            "captured source handles must survive relink/deletion/session closure"
        );
        freezes.push(receipt);
    }
    if let Some(output) = std::env::var_os("MOTION_PROFESSIONAL_RELINK_ARTIFACTS") {
        let output = PathBuf::from(output);
        fs::create_dir_all(output.join("assets")).unwrap();
        for (name, project) in [
            ("initial-project.json", &initial),
            ("replaced-project.json", &replaced),
            ("recovered-project.json", &recovered),
        ] {
            fs::write(
                output.join(name),
                serde_json::to_vec_pretty(project).unwrap(),
            )
            .unwrap();
        }
        for asset in &recovered.assets {
            let path = output.join(&asset.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::copy(root.join(&asset.path), path).unwrap();
        }
        for i in 0..2 {
            source(&output.join(&initial.assets[i].path), i, 0);
            fs::copy(&paths[i], output.join(format!("replacement-{i}.png"))).unwrap();
        }
        for (name, pixels) in [
            ("initial.png", &original),
            ("relinked.png", &replacement),
            ("golden-initial.png", &golden(0, 7.5)),
            ("golden-relinked.png", &golden(1, 7.5)),
        ] {
            png(&output.join(name), pixels);
        }
        for (i, receipt) in freezes.iter().enumerate() {
            fs::copy(
                receipt["path"].as_str().unwrap(),
                output.join(format!("frozen-{i}.png")),
            )
            .unwrap();
        }
        fs::write(
            output.join("requests.json"),
            serde_json::to_vec_pretty(&json!([
                {"op":"missing_media_assets"},
                begin_request("two-plates", &paths, &before),
                {"op":"media_status","request_id":"two-plates"},
                {"op":"finish_asset_relink","request_id":"two-plates"},
                {"op":"release_asset_relink","request_id":"two-plates"}
            ]))
            .unwrap(),
        )
        .unwrap();
        fs::write(output.join("README.md"),
            "# Replaying the PNG relink example\n\nAll images are repository-authored MIT fixtures; no external assets.\n\nCopy initial-project.json to project.json in this artifact directory and open that project. The assets directory includes both initial and recovered sources. requests.json retains the actual initial successful protocol batch. For a new run, replace its two candidate paths with absolute paths to replacement-0.png and replacement-1.png here, and bind revision/document_generation from a fresh missing_media_assets response. Poll media_status until ready before finish_asset_relink; release afterward. Staging must leave pixels unchanged; one undo restores both original plates. recovered-project.json can be opened directly with the preserved assets.\n\nThe Rust integration test is the complete executable scenario, including malformed input, external source deletion, recovery, fresh-session reopen and independently frozen old/new PNGs. These are headless PNG outputs, not desktop screenshots or photographic footage. Undo of a healthy replacement is verified; undo cannot recreate externally deleted file contents.\n").unwrap();
        fs::write(output.join("result.json"),serde_json::to_vec_pretty(&json!({
            "scope":"Public multi-PNG relink staging/commit, actual native pixels, atomic undo/redo, failed decode, missing recovery, save/reopen and old/new frozen sources",
            "limits":["Repository-authored procedural PNGs, not photographed footage","Default Unorm8 headless, not desktop window capture","Missing recovery cannot restore externally deleted file contents on undo; healthy replacement undo/redo verified separately"],
            "author":"Motion Studio repository contributors","license":"MIT","externalAssets":false,
            "maximumChannelErrorBudget":MAX_CHANNEL_ERROR,"initialGoldenMax":original_error,
            "comparisons":comparisons,"capturedRevisions":[old_revision,new_revision],"frozenExact":true,
            "staged":staged,"prepared":prepared,"committed":committed,"rejected":rejected,"missing":missing,"recovered":recovered_receipt
        })).unwrap()).unwrap();
    }
}
