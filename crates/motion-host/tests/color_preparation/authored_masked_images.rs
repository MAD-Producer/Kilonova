use super::super::mask_composition::rectangle;
use super::*;
use motion_core::{
    vector::{ShapeKind, VectorContent},
    Track,
};

#[test]
#[ignore = "requires the pinned native OCIO runtime and an actual GPU"]
fn masked_16_bit_images_and_vectors_retain_separate_color_and_coverage_contracts() {
    let reference = reference();
    let original = original();
    let mut renderer = renderer();
    let mut stages = std::collections::BTreeSet::new();
    let mut checked = 0;
    for case in &original.cases {
        if !stages.insert(case.stages[0]) {
            continue;
        }
        let root = tempfile::tempdir().unwrap();
        let input = &reference.cases[case.stages[0]];
        let alpha = input.processor.input_alpha;
        let mut p = image_document(&reference, root.path(), case.config, alpha);
        let mut mask = rectangle(1, [0., 0., 1., 4.]);
        mask.opacity.value = 50.;
        p.layers[0].masks.push(mask);
        let mut vector = VectorContent::shape(ShapeKind::Rectangle);
        vector.fill = Some(Track::constant(input.input[2]));
        let mut layer = Layer::solid(2, "masked working vector", [3., 4.], [1.5, 2., 0.], [1.; 4]);
        layer.content = Content::Vector { vector };
        let mut mask = rectangle(1, [1., 0., 1., 4.]);
        mask.opacity.value = 25.;
        layer.masks.push(mask);
        p.layers.push(layer);
        p.validate().unwrap();
        let snapshot = serde_json::to_vec(&p).unwrap();
        write_image(root.path(), &original.samples);
        let (mut prepared, images) = prepare(&p, root.path(), &mut renderer);
        std::fs::remove_file(root.path().join("assets/1.png")).unwrap();
        for resource in p.color_management.resources().unwrap() {
            std::fs::remove_file(root.path().join(resource.path)).unwrap();
        }
        let target = renderer
            .color_image_target(3, 4, prepared.authored.contract())
            .unwrap();
        for time in [0.25, 5.5] {
            let frame = prepared
                .authored
                .sample(&p, time, &ExpressionEvaluator)
                .unwrap();
            let reads = renderer.image_readback_info();
            renderer
                .draw_authored_scene_with_images(frame, &target, &images)
                .unwrap();
            assert_eq!(renderer.image_readback_info(), reads);
            let pixels = read(&renderer, &target);
            for y in 0..4 {
                for x in 0..3 {
                    let i = (y * 3 + x) as usize;
                    let expected = match x {
                        0 => premult(case.intermediate[i], alpha).map(|v| v * 0.5),
                        1 => premult(input.expected[2], input.processor.output_alpha)
                            .map(|v| v * 0.25),
                        _ => [0.; 4],
                    };
                    assert!(
                        close(pixels[i], expected),
                        "stage={} time={time} x={x} y={y} actual={:?} expected={expected:?}",
                        case.stages[0],
                        pixels[i]
                    );
                    checked += 1;
                }
            }
            assert_eq!(serde_json::to_vec(&p).unwrap(), snapshot);
        }
    }
    assert_eq!(checked, 144);
    eprintln!("managed masked 16-bit image and vector pixels={checked}");
}
