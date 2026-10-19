//! Frozen repository-authored sources are an independent pre-refactor oracle.
use super::*;

pub const LEGACY: [(&str, &str, usize, usize); 3] = [
    (
        "soft_sphere",
        include_str!("legacy_examples/soft-sphere.msgraph"),
        9,
        40,
    ),
    (
        "soft_sphere_motion",
        include_str!("legacy_examples/soft-sphere-motion.msgraph"),
        15,
        96,
    ),
    (
        "floating_lights",
        include_str!("legacy_examples/floating-lights.msgraph"),
        7,
        46,
    ),
];

#[test]
fn compact_examples_preserve_exported_gpu_pixels_parameters_animation_and_seek() {
    let folder = tempfile::tempdir().unwrap();
    let artifacts =
        std::env::var_os("MOTION_COMPACT_EXAMPLE_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(root) = &artifacts {
        std::fs::create_dir_all(root).unwrap();
    }
    let mut captures = 0;
    let mut largest_delta = 0;
    for (id, source, root_nodes, operations) in LEGACY {
        let legacy = GraphDocument::from_json(source).unwrap();
        let compact = builtin_examples()
            .iter()
            .find(|e| e.id == id)
            .unwrap()
            .load()
            .unwrap();
        assert_eq!(compact.public_parameters, legacy.public_parameters);
        assert_eq!(compact.nodes.len(), root_nodes);
        let plan = validate(&compact, &CompileOptions::default()).unwrap();
        assert_eq!(plan.reachable_nodes.len(), operations);
        assert_eq!(
            compact
                .nodes
                .iter()
                .filter(|n| matches!(n.operation, NodeOp::Group { .. }))
                .count(),
            1
        );
        let group = compact
            .nodes
            .iter()
            .find_map(|n| match &n.operation {
                NodeOp::Group { graph } => Some(graph),
                _ => None,
            })
            .unwrap();
        assert!(group.nodes.iter().any(|n| matches!(
            n.operation,
            NodeOp::Gradient {
                mode: GradientMode::Radial
            }
        )));
        assert!(!group.nodes.iter().any(|n| matches!(
            n.operation,
            NodeOp::Float { .. } | NodeOp::Vec2 { .. } | NodeOp::Vec3 { .. } | NodeOp::Color { .. }
        )));
        let (old_project, mut old_renderer, _) =
            setup(&legacy, &folder.path().join(format!("{id}-legacy.msfx")));
        let (new_project, mut new_renderer, _) =
            setup(&compact, &folder.path().join(format!("{id}-compact.msfx")));
        for [width, height] in [[64, 64], [128, 64], [192, 192]] {
            let old_target = old_renderer.capture_target(width, height).unwrap();
            let new_target = new_renderer.capture_target(width, height).unwrap();
            for case in 0..4 {
                let configure = |original: &Project| {
                    let mut p = original.clone();
                    p.width = width;
                    p.height = height;
                    p.frames = 400;
                    p.layers[0].size = [width as f32, height as f32];
                    p.layers[0].transform.position.value =
                        [width as f32 * 0.5, height as f32 * 0.5, 0.];
                    for parameter in &compact.public_parameters {
                        let value = &mut p.layers[0].effects[0]
                            .params
                            .get_mut(&parameter.id)
                            .unwrap()
                            .track
                            .value;
                        if case == 1 && parameter.value_type == ValueType::Float {
                            value[0] = parameter.min;
                        } else if case == 2 && parameter.value_type == ValueType::Float {
                            value[0] = parameter.max;
                        } else if case == 3 && parameter.value_type == ValueType::Color {
                            // Color Mix alpha interpolation must remain intact;
                            // source-over Blend is deliberately limited to opaque shadow colors.
                            *value = [0.81, 0.12, 0.64, 0.35];
                        }
                    }
                    if case == 3 {
                        let param = &mut p.layers[0].effects[0]
                            .params
                            .get_mut(&compact.public_parameters[0].id)
                            .unwrap()
                            .track;
                        param
                            .upsert(0, [0.8 * param.value[0], 0., 0., 0.], Ease::Linear)
                            .unwrap();
                        param
                            .upsert(60, [1.1 * param.value[0], 0., 0., 0.], Ease::Linear)
                            .unwrap();
                    }
                    p
                };
                let old = configure(&old_project);
                let new = configure(&new_project);
                let mut first = None;
                for (index, frame) in [0., 15.5, 47.25, 290., 0.].into_iter().enumerate() {
                    let expected = render(&old, &mut old_renderer, &old_target, frame);
                    let actual = render(&new, &mut new_renderer, &new_target, frame);
                    let max_delta = expected
                        .iter()
                        .zip(&actual)
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    largest_delta = largest_delta.max(max_delta);
                    assert!(
                        max_delta <= 1,
                        "{id} {width}x{height} case={case} frame={frame} delta={max_delta}"
                    );
                    if index == 0 {
                        first = Some(actual.clone());
                    }
                    if index == 4 {
                        assert_eq!(
                            first.as_ref().unwrap(),
                            &actual,
                            "{id}: random seek changed pixels"
                        );
                    }
                    if width == 192 && case == 0 && index < 4 {
                        if let Some(root) = &artifacts {
                            for (label, pixels) in [("before", &expected), ("after", &actual)] {
                                image::save_buffer(
                                    root.join(format!("{id}-{label}-{index}.png")),
                                    pixels,
                                    width,
                                    height,
                                    image::ColorType::Rgba8,
                                )
                                .unwrap();
                            }
                        }
                    }
                    captures += 2;
                }
            }
        }
    }
    assert_eq!(captures, 360);
    if let Some(root) = &artifacts {
        std::fs::write(root.join("gpu-equivalence.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "captures": captures, "maxChannelDelta": largest_delta, "channelTolerance": 1,
            "cases": ["default", "float-minimums", "float-maximums", "translucent-color-and-animated-parameter"],
            "sizes": [[64,64], [128,64], [192,192]], "frames": [0, 15.5, 47.25, 290, 0]
        })).unwrap()).unwrap();
    }
}
