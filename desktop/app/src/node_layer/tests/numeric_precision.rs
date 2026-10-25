//! Real bound instances, format-preserving GPU output and actual disk reopen.
use super::*;
use motion_effects::{
    AlphaMode, WorkingSpace,
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
};
use motion_render::{CapturedImage, RenderStats, Renderer, Scene};

#[allow(dead_code)]
#[path = "../../../../../crates/motion-render/tests/support/precision_golden.rs"]
mod reference;

fn capture(
    renderer: &mut Renderer,
    session: &motion_host::Session,
) -> (CapturedImage, RenderStats) {
    capture_project(
        renderer,
        session.engine.project(),
        session.effects.registry.clone(),
    )
}

fn capture_project(
    renderer: &mut Renderer,
    project: &motion_core::Project,
    registry: motion_effects::Registry,
) -> (CapturedImage, RenderStats) {
    renderer.set_effect_registry(registry);
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let contract = ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        project.working_precision,
    );
    let target = renderer.image_capture_target(64, 64, contract).unwrap();
    renderer.preflight_effects(&scene, 64, 64).unwrap();
    let stats = renderer.draw(&scene, target.view(), 64, 64).unwrap();
    let image = renderer.read_image_target(&target).unwrap();
    assert_eq!(
        image.storage,
        motion_render::image_formats::color_storage(project.working_precision)
    );
    assert_eq!(
        image.pixels.len(),
        64 * 64 * image.storage.bytes_per_pixel() as usize
    );
    (image, stats)
}

fn owner(precision: ChannelPrecision) -> Renderer {
    pollster::block_on(Renderer::headless_with_working_precision(
        precision,
        Default::default(),
    ))
    .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn node_private_numeric_instances_match_baked_and_analytic_pixels_in_four_precisions() {
    let mut rows = vec![];
    let mut cases = vec![];
    for precision in reference::MODES {
        for overflow in [false, true] {
            let folder = tempfile::tempdir().unwrap();
            let root = folder.path().join("project");
            std::fs::create_dir_all(&root).unwrap();
            let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
            project.background = [0.; 4];
            project.working_precision = precision;
            project.layers.push(motion_core::Layer::solid(
                1,
                "Private data",
                [64.; 2],
                [32., 32., 0.],
                [1.; 4],
            ));
            let mut session =
                motion_host::Session::new(project, root.clone(), motion_desktop_media::platform())
                    .unwrap();
            let mut graph = motion_graph::demo_graph();
            let parameter = graph.public_parameters[0].clone();
            graph.working_space = WorkingSpace::Linear;
            graph.alpha_mode = AlphaMode::Premultiplied;
            graph.nodes = vec![
                Node::new(
                    "color",
                    NodeOp::Color {
                        value: [0.25, 0.5, 0.75, 1.],
                    },
                ),
                Node::new("output", NodeOp::ColorOutput),
            ];
            graph.connections = vec![Connection::new("color", "value", "output", "color")];
            graph.public_parameters = if overflow {
                (0..32)
                    .map(|i| {
                        let mut p = parameter.clone();
                        p.id = format!("p{i}");
                        p
                    })
                    .collect()
            } else {
                vec![]
            };
            let mut editor = node_editor::NodeEditor::new(graph);
            editor.layer_target = Some(Target {
                root: root.clone(),
                composition: session.engine.project().composition_id.clone(),
                object: 1,
                effect: 1,
                key: "precision-private".into(),
                label: "Private data".into(),
                creating: true,
            });
            let mut compiler = motion_graph::EditableCompiler::default();
            let snapshot = editor.snapshot();
            let compiled = compiler
                .compile(&snapshot.document, &snapshot.options)
                .unwrap();
            assert_eq!(compiled.report.portable_compilations, 1);
            apply(&mut session, &snapshot, &compiled).unwrap();
            editor.layer_target.as_mut().unwrap().creating = false;
            let original = session.engine.project().clone();
            let effect = &original.layers[0].effects[0];
            assert_eq!(
                effect.shader_data.as_ref().unwrap().texture.is_empty(),
                !overflow
            );
            let package = session
                .effects
                .registry
                .resolve(&effect.plugin, &effect.version, &effect.hash)
                .unwrap();
            let mut renderer = owner(precision);
            let (initial, initial_stats) = capture(&mut renderer, &session);
            assert_eq!(initial_stats.parameter_resource_upload_bytes > 0, overflow);
            let builds = renderer.effect_pipeline_builds();
            let mut previous = initial.pixels.clone();
            for color in [[0.75, 0.25, 0.5, 0.5], [0.5, 0.75, 0.25, 0.25]] {
                for (index, value) in color.into_iter().enumerate() {
                    editor.set_constant("color", index, value).unwrap();
                }
                let snapshot = editor.snapshot();
                let compiled = compiler
                    .compile(&snapshot.document, &snapshot.options)
                    .unwrap();
                assert_eq!(compiled.report.portable_compilations, 0);
                apply(&mut session, &snapshot, &compiled).unwrap();
                let current = &session.engine.project().layers[0].effects[0];
                assert_eq!(current.hash, effect.hash);
                assert!(Arc::ptr_eq(
                    &package,
                    &session
                        .effects
                        .registry
                        .resolve(&current.plugin, &current.version, &current.hash)
                        .unwrap()
                ));
                let (actual, changed_stats) = capture(&mut renderer, &session);
                assert_ne!(actual.pixels, previous);
                assert_eq!(changed_stats.parameter_resource_upload_bytes > 0, overflow);
                assert_eq!(renderer.effect_pipeline_builds(), builds);
                let (warm, warm_stats) = capture(&mut renderer, &session);
                assert_eq!(actual.pixels, warm.pixels);
                assert_eq!(warm_stats.parameter_resource_upload_bytes, 0);
                let expected = reference::store(
                    [
                        f64::from(color[0]) * f64::from(color[3]),
                        f64::from(color[1]) * f64::from(color[3]),
                        f64::from(color[2]) * f64::from(color[3]),
                        f64::from(color[3]),
                    ],
                    precision,
                );
                let tolerance = match precision {
                    ChannelPrecision::Unorm8 => 0.012,
                    ChannelPrecision::Unorm16 => 0.0001,
                    ChannelPrecision::Float16 => 0.001,
                    ChannelPrecision::Float32 => 0.000002,
                };
                let mut worst = 0.0_f64;
                for pixel in reference::linear(&actual) {
                    for channel in 0..4 {
                        worst = worst.max((pixel[channel] - expected[channel]).abs());
                    }
                }
                assert!(
                    worst <= tolerance,
                    "{precision:?} overflow={overflow} color={color:?}: worst={worst}"
                );
                let baked = snapshot.compile().unwrap();
                let baked_package = Arc::new(
                    motion_effects::EffectPackage::from_bytes(
                        super::super::package(&baked, snapshot.layer_target.as_ref().unwrap())
                            .unwrap(),
                    )
                    .unwrap(),
                );
                let mut baked_project = session.engine.project().clone();
                let replacement = &mut baked_project.layers[0].effects[0];
                replacement.plugin = baked_package.manifest.id.clone();
                replacement.version = baked_package.manifest.version.clone();
                replacement.hash = baked_package.hash.clone();
                replacement.shader_data = None;
                baked_project.rebuild_plugin_dependencies();
                let mut baked_registry = session.effects.registry.clone();
                baked_registry.insert(baked_package).unwrap();
                let (baked_image, _) =
                    capture_project(&mut owner(precision), &baked_project, baked_registry);
                assert_eq!(
                    actual.pixels, baked_image.pixels,
                    "private values must match independently baked code"
                );
                rows.push(
                    serde_json::json!({"precision":precision,"overflow":overflow,"color":color,
                    "format":actual.storage,"worst_linear_error":worst,"portable_compilations":0,
                    "native_pipeline_builds":0,"warm_upload_bytes":0,"matches_baked_pixels":true,
                    "package_arc_reused":true}),
                );
                previous = actual.pixels;
            }
            motion_core::storage::save(&root, session.engine.project()).unwrap();
            let reopened = motion_core::storage::load(&root).unwrap();
            assert_eq!(&reopened, session.engine.project());
            let reopened_session =
                motion_host::Session::new(reopened, root.clone(), motion_desktop_media::platform())
                    .unwrap();
            assert_eq!(
                capture(&mut owner(precision), &reopened_session).0.pixels,
                previous
            );

            let mut pair = session.engine.project().clone();
            pair.layers[0].size = [32., 64.];
            pair.layers[0].transform.position.value = [16., 32., 0.];
            let mut right = pair.layers[0].clone();
            right.id = 2;
            right.effects[0].id = 2;
            right.transform.position.value = [48., 32., 0.];
            pair.layers[0].effects[0].shader_data =
                original.layers[0].effects[0].shader_data.clone();
            pair.layers.push(right);
            pair.rebuild_plugin_dependencies();
            pair.validate().unwrap();
            let mut pair_session =
                motion_host::Session::new(pair, root, motion_desktop_media::platform()).unwrap();
            let (first_pair, _) = capture(&mut renderer, &pair_session);
            let bytes = first_pair.storage.bytes_per_pixel() as usize;
            let left = (32 * 64 + 16) * bytes;
            let right = (32 * 64 + 48) * bytes;
            assert_ne!(
                &first_pair.pixels[left..left + bytes],
                &first_pair.pixels[right..right + bytes]
            );
            let mut pair = pair_session.engine.project().clone();
            pair.layers[0].effects[0].shader_data = pair.layers[1].effects[0].shader_data.clone();
            pair_session = motion_host::Session::new(
                pair,
                pair_session.root.clone(),
                motion_desktop_media::platform(),
            )
            .unwrap();
            let (second_pair, _) = capture(&mut renderer, &pair_session);
            assert_eq!(
                &second_pair.pixels[right..right + bytes],
                &first_pair.pixels[right..right + bytes]
            );
            assert_eq!(
                &second_pair.pixels[left..left + bytes],
                &second_pair.pixels[right..right + bytes]
            );
            assert_eq!(renderer.effect_pipeline_builds(), builds);
            session.engine.undo().unwrap();
            session.engine.undo().unwrap();
            assert_eq!(session.engine.project(), &original);
            assert_eq!(capture(&mut renderer, &session).0.pixels, initial.pixels);
            cases.push(
                serde_json::json!({"precision":precision,"overflow":overflow,
                "disk_reopened_pixels_equal":true,"independent_instance_preserved":true,
                "undo_pixels_restored":true,"native_pipeline_builds":0}),
            );
        }
    }
    assert_eq!(rows.len(), 16);
    assert_eq!(cases.len(), 8);
    if let Some(out) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            PathBuf::from(out).join("private-instance-four-precision.json"),
            serde_json::to_vec_pretty(&serde_json::json!({"updates":rows,"cases":cases})).unwrap(),
        )
        .unwrap();
    }
}
