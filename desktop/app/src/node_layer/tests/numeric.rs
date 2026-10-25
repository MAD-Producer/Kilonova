use super::*;
use motion_render::{Renderer, Scene};

fn pixels(
    renderer: &mut Renderer,
    session: &motion_host::Session,
) -> (Vec<u8>, motion_render::RenderStats) {
    renderer.set_effect_registry(session.effects.registry.clone());
    let mut scene = Scene::new(session.engine.project()).unwrap();
    scene
        .sample(
            session.engine.project(),
            0.,
            None,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    renderer.capture(&scene, &target).unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn node_bound_numeric_data_reuses_package_and_native_pipeline_with_independent_instances() {
    let mut rows = vec![];
    for overflow in [false, true] {
        let (_folder, mut session, mut editor) = setup();
        let mut graph = motion_graph::demo_graph();
        let parameter = graph.public_parameters[0].clone();
        graph.nodes = vec![
            Node::new(
                "color",
                NodeOp::Color {
                    value: [0.2, 0.3, 0.8, 1.],
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
        let target = editor.layer_target.clone();
        editor.replace_document(graph, None);
        editor.layer_target = target;
        let mut compiler = motion_graph::EditableCompiler::default();
        let snapshot = editor.snapshot();
        let first = compiler
            .compile(&snapshot.document, &snapshot.options)
            .unwrap();
        apply(&mut session, &snapshot, &first).unwrap();
        let before = session.engine.project().clone();
        let effect = &before.layers[0].effects[0];
        let package = session
            .effects
            .registry
            .resolve(&effect.plugin, &effect.version, &effect.hash)
            .unwrap();
        let mut scene = Scene::new(&before).unwrap();
        scene
            .sample(&before, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let mut plans =
            motion_render::effect_plan::PlanBuilder::new(session.effects.registry.clone()).unwrap();
        let frame = plans.build(&scene, &[0], 64, 64, true).unwrap();
        let mut packed = vec![0; frame.buffer_bytes(&scene)];
        if overflow {
            let reason = frame.write(&scene, &mut packed).unwrap_err().to_string();
            assert!(
                reason.contains("v12") && reason.contains("instance_shader_data"),
                "{reason}"
            );
        } else {
            assert_eq!(frame.write(&scene, &mut packed).unwrap(), packed.len());
        }
        assert_eq!(
            effect.shader_data.as_ref().unwrap().texture.is_empty(),
            !overflow
        );
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let (previous, _) = pixels(&mut renderer, &session);
        let builds = renderer.effect_pipeline_builds();
        for color in [[0.8, 0.1, 0.2, 1.], [0.1, 0.5, 0.8, 0.5]] {
            editor.set_constant("color", 0, color[0]).unwrap();
            editor.set_constant("color", 1, color[1]).unwrap();
            editor.set_constant("color", 2, color[2]).unwrap();
            editor.set_constant("color", 3, color[3]).unwrap();
            let snapshot = editor.snapshot();
            let compiled = compiler
                .compile(&snapshot.document, &snapshot.options)
                .unwrap();
            assert_eq!(compiled.report.portable_compilations, 0);
            apply(&mut session, &snapshot, &compiled).unwrap();
            let current = &session.engine.project().layers[0].effects[0];
            assert_eq!(
                (&current.plugin, &current.version, &current.hash),
                (&effect.plugin, &effect.version, &effect.hash)
            );
            assert!(Arc::ptr_eq(
                &package,
                &session
                    .effects
                    .registry
                    .resolve(&current.plugin, &current.version, &current.hash)
                    .unwrap()
            ));
            let (actual, _) = pixels(&mut renderer, &session);
            assert_ne!(actual, previous);
            assert_eq!(
                renderer.effect_pipeline_builds(),
                builds,
                "numeric updates must not rebuild native effect programs"
            );
            let (_, warm_stats) = pixels(&mut renderer, &session);
            assert_eq!(
                warm_stats.parameter_resource_upload_bytes, 0,
                "unchanged data must not upload each frame"
            );
            // Compare with a separately validated package whose literals are baked.
            let baked = snapshot.compile().unwrap();
            let oracle_package = Arc::new(
                motion_effects::EffectPackage::from_bytes(package_bytes(
                    &baked,
                    &snapshot.layer_target.as_ref().unwrap(),
                ))
                .unwrap(),
            );
            let mut oracle_project = session.engine.project().clone();
            let e = &mut oracle_project.layers[0].effects[0];
            e.plugin = oracle_package.manifest.id.clone();
            e.version = oracle_package.manifest.version.clone();
            e.hash = oracle_package.hash.clone();
            e.shader_data = None;
            oracle_project.rebuild_plugin_dependencies();
            let mut oracle = pollster::block_on(Renderer::headless()).unwrap();
            let mut registry = session.effects.registry.clone();
            registry.insert(oracle_package).unwrap();
            oracle.set_effect_registry(registry);
            let mut scene = Scene::new(&oracle_project).unwrap();
            scene
                .sample(&oracle_project, 0., None, &motion_core::ExpressionEvaluator)
                .unwrap();
            let capture = oracle.capture_target(64, 64).unwrap();
            assert_eq!(actual, oracle.capture(&scene, &capture).unwrap().0);
            let json = serde_json::to_string(session.engine.project()).unwrap();
            let reopened: motion_core::Project = serde_json::from_str(&json).unwrap();
            reopened.validate().unwrap();
            assert_eq!(&reopened, session.engine.project());
            rows.push(serde_json::json!({"overflow":overflow,"color":color,"portable_compilations":0,"native_pipeline_builds":0,"package_arc_reused":true,"matches_baked_pixels":true}));
        }
        // Two layer instances share the exact package but retain independent
        // payloads, including overflow resource zero. Updating the left instance
        // must not overwrite the right instance's texture or cached binding.
        let mut pair = session.engine.project().clone();
        pair.layers[0].size = [32., 64.];
        pair.layers[0].transform.position.value = [16., 32., 0.];
        let mut right = pair.layers[0].clone();
        right.id = 2;
        right.effects[0].id = 2;
        right.transform.position.value = [48., 32., 0.];
        pair.layers[0].effects[0].shader_data = before.layers[0].effects[0].shader_data.clone();
        pair.layers.push(right);
        pair.rebuild_plugin_dependencies();
        pair.validate().unwrap();
        let mut scene = Scene::new(&pair).unwrap();
        scene
            .sample(&pair, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let capture = renderer.capture_target(64, 64).unwrap();
        let first_pair = renderer.capture(&scene, &capture).unwrap().0;
        let left = (32 * 64 + 16) * 4;
        let right = (32 * 64 + 48) * 4;
        assert_ne!(&first_pair[left..left + 4], &first_pair[right..right + 4]);
        pair.layers[0].effects[0].shader_data = pair.layers[1].effects[0].shader_data.clone();
        scene
            .sample(&pair, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let second_pair = renderer.capture(&scene, &capture).unwrap().0;
        assert_eq!(
            &second_pair[right..right + 4],
            &first_pair[right..right + 4]
        );
        assert_eq!(&second_pair[left..left + 4], &second_pair[right..right + 4]);
        assert_eq!(renderer.effect_pipeline_builds(), builds);
        session.engine.undo().unwrap();
        session.engine.undo().unwrap();
        assert_eq!(session.engine.project(), &before);
        assert_eq!(pixels(&mut renderer, &session).0, previous);
    }
    if let Some(out) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            PathBuf::from(out).join("bound-numeric-reuse.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}

fn package_bytes(compiled: &CompiledGraph, target: &Target) -> Vec<u8> {
    super::super::package(compiled, target).unwrap()
}
