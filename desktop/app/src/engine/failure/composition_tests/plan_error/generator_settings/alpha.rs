use super::*;
use motion_core::{ExpressionEvaluator, Layer, Project};
use motion_effects::{LensElement, LensShape};
use motion_render::{
    scene_generator::{generate, AlphaImage, GeneratorScratch},
    Scene,
};
use std::collections::HashMap;

pub(super) fn actual_refusals() -> Vec<RenderError> {
    let mut project = Project::new(128, 128, 30, 120).unwrap();
    for id in [7, 8] {
        project.layers.push(Layer::solid(
            id,
            "Occlusion",
            [128.; 2],
            [64., 64., 0.],
            [1.; 4],
        ));
    }
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &ExpressionEvaluator)
        .unwrap();
    let blocker = scene.layers.iter_mut().find(|layer| layer.id == 8).unwrap();
    blocker.model.w_axis.z = 0.5;
    blocker.asset = Some(99);
    let mut effect = effect();
    effect.param_ids = [
        "position",
        "intensity",
        "attenuation",
        "reference_distance",
        "occlusion_radius",
        "scale",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    effect.values = [[0.; 4]; MAX_PARAMS];
    effect.values[0] = [64., 64., 0., 0.];
    effect.values[1][0] = 1.;
    effect.values[3][0] = 100.;
    effect.values[5][0] = 100.;
    effect.scene = Some(SceneSettings {
        occlusion: true,
        elements: vec![LensElement {
            id: 1,
            shape: LensShape::Glow,
            enabled: true,
            offset: 0.,
            size: [16.; 2],
            color: [1.; 4],
            intensity: 1.,
            rays: 2,
            chromatic: 0.,
        }],
        ..Default::default()
    });
    let mut alpha = HashMap::from([(
        99,
        AlphaImage {
            width: 2,
            height: 2,
            pixels: vec![0, 64, 128, 255],
        },
    )]);
    let mut output = Vec::new();
    let mut scratch = GeneratorScratch::default();
    generate(
        &scene,
        &scene.layers[0],
        &effect,
        RendererKind::LensFlare,
        &alpha,
        &mut output,
        &mut scratch,
    )
    .unwrap();
    let expected = output.clone();
    assert!(!expected.is_empty());
    let mut failures = Vec::new();
    for (width, height, pixels) in [
        (2, 2, vec![]),
        (2, 2, vec![255]),
        (0, 2, vec![]),
        (2, 0, vec![]),
        (u32::MAX, u32::MAX, vec![255]),
    ] {
        alpha.insert(
            99,
            AlphaImage {
                width,
                height,
                pixels: pixels.clone(),
            },
        );
        let failure = generate(
            &scene,
            &scene.layers[0],
            &effect,
            RendererKind::LensFlare,
            &alpha,
            &mut output,
            &mut scratch,
        )
        .unwrap_err();
        assert!(matches!(&failure, RenderError::Context { cause, .. }
            if matches!(cause.as_ref(), RenderError::Context { cause, .. }
                if matches!(cause.as_ref(), RenderError::Invalid(_)))));
        assert!(failure.to_string().contains("asset 99"));
        assert_eq!(output, expected);
        assert_eq!(alpha[&99].pixels, pixels);
        assert_eq!((alpha[&99].width, alpha[&99].height), (width, height));
        failures.push(failure);
    }
    alpha.insert(
        99,
        AlphaImage {
            width: 2,
            height: 2,
            pixels: vec![0, 64, 128, 255],
        },
    );
    let blocker = scene.layers.iter_mut().find(|layer| layer.id == 8).unwrap();
    let size = blocker.size;
    blocker.size[0] = 0.;
    let failure = generate(
        &scene,
        &scene.layers[0],
        &effect,
        RendererKind::LensFlare,
        &alpha,
        &mut output,
        &mut scratch,
    )
    .unwrap_err();
    assert!(failure.to_string().contains("coordinates are not finite"));
    assert_eq!(output, expected);
    failures.push(failure);
    scene
        .layers
        .iter_mut()
        .find(|layer| layer.id == 8)
        .unwrap()
        .size = size;
    output.clear();
    generate(
        &scene,
        &scene.layers[0],
        &effect,
        RendererKind::LensFlare,
        &alpha,
        &mut output,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(output, expected);
    failures
}
