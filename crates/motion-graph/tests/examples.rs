use motion_effects::{EffectPackage, Registry};
use motion_graph::*;
use std::{collections::BTreeSet, sync::Arc};

#[test]
fn embedded_examples_are_independent_complete_n1_documents() {
    let examples = builtin_examples();
    assert_eq!(examples.len(), 18);
    let mut ids = BTreeSet::new();
    for example in examples {
        assert!(ids.insert(example.id));
        for text in [
            example.name,
            example.english_name,
            example.description,
            example.english_description,
            example.lesson,
            example.english_lesson,
            example.limitations,
            example.english_limitations,
        ] {
            assert!(!text.trim().is_empty(), "{} has missing help", example.id);
        }
        let graph = example.load().unwrap();
        assert_eq!(graph.id, example.id);
        assert_eq!(graph.name, example.name);
        assert_eq!(graph.english_name, example.english_name);
        let plan = validate(&graph, &CompileOptions::default()).unwrap();
        assert!(
            plan.unused_nodes.is_empty(),
            "{} has unused teaching nodes",
            example.id
        );
        for node in &graph.nodes {
            assert!(!matches!(node.operation, NodeOp::Unsupported { .. }));
            node_ports(node, &graph.public_parameters).unwrap();
        }
        let compiled = compile(&graph, &CompileOptions::default()).unwrap();
        assert_eq!(compiled.report.passes, 1);
        assert!(compiled.shader.glsl.fragment.contains("#version 300 es"));
        assert_eq!(compiled.report.package_resource_textures, 0);
        let mut edited = example.load().unwrap();
        edited.nodes[0].position = [456., 789.];
        assert_eq!(
            example.load().unwrap(),
            graph,
            "loading must never mutate the index source"
        );
    }
    assert_eq!(examples[0].load().unwrap(), demo_graph());
}

#[test]
fn every_example_exports_real_packages_and_changed_public_defaults() {
    let folder = tempfile::tempdir().unwrap();
    for example in builtin_examples() {
        let mut graph = example.load().unwrap();
        // Exercise author edits before export, not just the original checked-in defaults.
        for parameter in &mut graph.public_parameters {
            parameter.default[0] = if parameter.value_type == ValueType::Bool {
                1. - parameter.default[0]
            } else if parameter.default[0] < parameter.max {
                (parameter.default[0] + parameter.step).min(parameter.max)
            } else {
                (parameter.default[0] - parameter.step).max(parameter.min)
            };
        }
        let options = CompileOptions {
            plugin_id: format!("test.motiongraph.examples.{}", example.id),
            ..Default::default()
        };
        let report = export_msfx(
            &graph,
            &options,
            &folder.path().join(format!("{}.msfx", example.id)),
        )
        .unwrap();
        assert_eq!(GraphDocument::load(&report.source_path).unwrap(), graph);
        let package = Arc::new(EffectPackage::open(&report.path).unwrap());
        assert_eq!(package.hash, report.package_hash);
        let params = &package.manifest.effects[0].params;
        assert_eq!(params.len(), graph.public_parameters.len());
        for (public, exported) in graph.public_parameters.iter().zip(params) {
            assert_eq!(public.id, exported.id);
            assert_eq!(public.default, exported.default);
            assert_eq!(public.animatable, exported.animatable);
        }
        let mut registry = Registry::default();
        registry.insert(package.clone()).unwrap();
        assert_eq!(
            registry
                .resolve(&report.plugin_id, &report.version, &report.package_hash)
                .unwrap()
                .bytes,
            package.bytes
        );
    }
}

#[test]
fn style_collections_have_stable_bilingual_metadata_without_source_payloads() {
    let catalogue = builtin_example_catalogue();
    assert_eq!(catalogue["version"], 1);
    assert_eq!(catalogue["collections"].as_array().unwrap().len(), 4);
    assert_eq!(catalogue["examples"].as_array().unwrap().len(), 18);
    for collection in [
        ExampleCollection::SoftSpace,
        ExampleCollection::GraphicMotion,
        ExampleCollection::GlowAtmosphere,
    ] {
        assert_eq!(
            builtin_examples()
                .iter()
                .filter(|e| e.collection == collection)
                .count(),
            if collection == ExampleCollection::SoftSpace {
                3
            } else {
                2
            }
        );
    }
    for entry in catalogue["examples"].as_array().unwrap() {
        assert!(entry.get("source").is_none());
        assert!(!entry["limitations"].as_str().unwrap().is_empty());
        assert!(!entry["english_limitations"].as_str().unwrap().is_empty());
    }
}
