//! Actual Shell paint and GPU composition capture, independent of desktop screen
//! capture availability. The optional project input exercises an authored demo;
//! the default test creates its own saved, animated OBJ fixture.
use crate::*;
use motion_core::{Content, Ease, Layer, ModelAsset, ModelClip, Project};
use motion_scene::{AssetResolver, ImportContext, ImportLimits, ResourceKind};
use sha2::{Digest, Sha256};
use winit::platform::windows::EventLoopBuilderExtWindows;

const TEST: &str = "shell::model_layer_tests::saved_model_keyframes_render_inside_the_editor";
const CHILD: &str = "MOTION_MODEL_UI_CHILD";

struct Materials;
impl AssetResolver for Materials {
    fn read(&mut self, path: &str, _: ResourceKind, _: usize) -> std::io::Result<Vec<u8>> {
        assert_eq!(path, "paint.mtl");
        Ok(b"newmtl Red\nKd 1 0 0\nillum 0\n".to_vec())
    }
}

fn fixture(root: &std::path::Path) {
    let source = b"mtllib paint.mtl\nv -1 -1 0\nv 1 -1 0\nv 0 1 0\nusemtl Red\nf 1 2 3\n";
    let imported = motion_scene::obj::import_obj(
        ImportContext {
            request: 8705,
            source: 87,
        },
        source,
        &mut Materials,
        &Default::default(),
        &ImportLimits::default(),
    )
    .unwrap();
    let frozen = imported
        .freeze(
            source,
            motion_core::model_assets::MAX_MODEL_ASSET_BYTES as usize,
        )
        .unwrap();
    std::fs::create_dir_all(root.join("assets/models")).unwrap();
    std::fs::write(root.join("assets/models/triangle.mscn"), &frozen).unwrap();
    let mut project = Project::new(960, 540, 30, 180).unwrap();
    project.model_assets.push(ModelAsset {
        id: 87,
        path: "assets/models/triangle.mscn".into(),
        bytes: frozen.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&frozen)),
    });
    let mut layer = Layer::solid(1, "Editable model", [200.; 2], [320., 270., 0.], [1.; 4]);
    layer.three_d = true;
    layer.content = Content::Model {
        model: ModelClip {
            asset: 87,
            scene: 0,
            pixels_per_metre: 80.,
            pivot: [0.; 3],
            samples: 4,
        },
    };
    layer
        .transform
        .position
        .upsert(0, [320., 270., 0.], Ease::InOut)
        .unwrap();
    layer
        .transform
        .position
        .upsert(179, [640., 270., 0.], Ease::InOut)
        .unwrap();
    layer
        .transform
        .rotation
        .upsert(0, [0.; 3], Ease::InOut)
        .unwrap();
    layer
        .transform
        .rotation
        .upsert(179, [0., 0., 20.], Ease::InOut)
        .unwrap();
    project.layers.push(layer);
    motion_core::storage::save(root, &project).unwrap();
}

#[test]
fn saved_model_keyframes_render_inside_the_editor() {
    if let Some(output) = std::env::var_os(CHILD) {
        capture_editor(PathBuf::from(output));
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let output = std::env::var_os("MOTION_MODEL_UI_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&output).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, &output)
        .output()
        .unwrap();
    std::fs::write(output.join("child.stdout.txt"), &result.stdout).unwrap();
    std::fs::write(output.join("child.stderr.txt"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "status={}\n{}\n{}",
        result.status,
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

fn capture_editor(output: PathBuf) {
    let _logging = crate::logging::init(output.join("logs")).unwrap();
    let project_root = match std::env::var_os("MOTION_MODEL_UI_PROJECT") {
        Some(path) => PathBuf::from(path),
        None => {
            let root = output.join("project");
            fixture(&root);
            root
        }
    };
    let saved = motion_core::storage::load(&project_root).unwrap();
    let model = saved
        .layers
        .iter()
        .find(|layer| matches!(layer.content, Content::Model { .. }))
        .unwrap();
    assert!(model.three_d);
    assert!(model.transform.position.keys.len() >= 2);
    assert!(model.transform.rotation.keys.len() >= 2);
    let mut shell = Shell::new(project_root.clone(), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    shell.panels.select_only(Some(model.id));
    shell.panels.timeline.expanded.insert(model.id);
    shell.panels.timeline.collapsed_transforms.remove(&model.id);
    let mut events = EventLoop::<()>::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1600, 1000)),
            )
            .unwrap(),
    );
    let pending = shell
        .engine
        .attach_async(window.clone(), 1600, 1000)
        .unwrap();
    shell.state =
        crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
            .unwrap()
            .unwrap();
    shell.window = Some(window);
    shell.attached = true;
    let mut images = Vec::new();
    for frame in [0., 45., 90., 135., 179.] {
        shell.state = shell.engine.seek(frame).unwrap();
        shell.frame = frame;
        let (paint, _) = shell.draw_commands([1600., 1000.]);
        let present = shell.ui_frame(paint, [1600., 1000.], true);
        let pending = shell.engine.present_async(present).unwrap();
        assert!(
            crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
                .unwrap()
                .unwrap(),
            "the actual editor surface must present the model"
        );
        let (paint, _) = shell.draw_commands([1600., 1000.]);
        let ui = shell.ui_frame(paint, [1600., 1000.], true);
        let path = output.join(format!("editor-model-{frame:03.0}.png"));
        crate::native_test_support::capture_ui(&mut events, &shell.engine, ui, path.clone())
            .unwrap();
        let image = image::open(&path).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (1600, 1000));
        let view = panels::composition_view(shell.workspace.display_root(), [1600., 1000.]);
        let region = image::imageops::crop_imm(
            &image,
            view.min[0] as u32,
            view.min[1] as u32,
            view.width() as u32,
            view.height() as u32,
        )
        .to_image();
        assert!(
            region.pixels().filter(|p| p[0] > 160).count() > 1000,
            "the model itself must be visible, independently of blue selection outlines"
        );
        images.push(region);
    }
    assert!(
        images[0]
            .pixels()
            .zip(images[2].pixels())
            .filter(|(a, b)| a != b)
            .count()
            > 1000,
        "saved model transform keys must change the actual editor composition pixels"
    );
    assert_eq!(
        motion_core::storage::load(&project_root).unwrap(),
        saved,
        "UI capture and seeking must preserve the editable project and its keys"
    );
    assert_eq!(
        shell.engine.state().unwrap()["project"],
        serde_json::to_value(&saved).unwrap()
    );
    shell.engine.request_shutdown().unwrap();
}
