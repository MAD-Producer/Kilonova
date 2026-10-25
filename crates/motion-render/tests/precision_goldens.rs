#[path = "support/precision_golden.rs"]
mod reference;
use motion_core::{
    Camera, Composition, CompositionClip, Content, Ease, ExpressionEvaluator, Layer, Track,
    compositing::{BlendMode, BlendSpace, LayerBlend},
    masks::LayerMask,
    render_sampling::Shutter,
    vector::{PathNode, VectorPath},
};
use motion_effects::{
    AlphaMode, WorkingSpace,
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
};
use motion_render::{
    ExposureFrame, ExposurePoll, Renderer, Scene, TextureCategory,
    exposure_scene::{ExposureDocument, ExposureScenes},
};
use reference::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn render(renderer: &mut Renderer, p: &motion_core::Project) -> (Vec<[f64; 4]>, Vec<u8>) {
    let mut scene = Scene::new(p).unwrap();
    scene.sample(p, 0., None, &ExpressionEvaluator).unwrap();
    let raw = renderer
        .image_capture_target(
            W,
            H,
            ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Premultiplied,
                },
                p.working_precision,
            ),
        )
        .unwrap();
    renderer.draw(&scene, raw.view(), W, H).unwrap();
    let native = linear(&renderer.read_image_target(&raw).unwrap());
    let display = renderer.sdr_capture_target(W, H, TRANSFORM).unwrap();
    let bytes = renderer.capture_sdr(&scene, &display).unwrap().0.pixels;
    (native, bytes)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn transparent_gradient_and_two_pass_hdr_glow_match_independent_storage_and_sdr_goldens() {
    let package = package();
    for mode in MODES {
        let p = project(mode, vec![layer(1, "glow", &package)]);
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            mode,
            Default::default(),
        ))
        .unwrap();
        renderer.set_effect_registry(registry(package.clone()));
        let expected = pixels(|x, y| store(glow(x, y, mode), mode));
        let (actual, display) = render(&mut renderer, &p);
        compare(
            "transparent-gradient-glow",
            mode,
            &actual,
            &expected,
            &display,
        );
        let peak = actual.iter().map(|p| p[0]).fold(0_f64, f64::max);
        if matches!(mode, ChannelPrecision::Float16 | ChannelPrecision::Float32) {
            assert!(
                peak > 1.4,
                "HDR was clipped before display: {mode:?} peak={peak}"
            );
        } else {
            assert!(peak <= 1.);
        }
        assert!(actual.iter().any(|p| p[3] == 0.));
        assert!(
            actual.iter().any(|p| p[3] > 0. && p[3] < 0.2),
            "blurred translucent edge missing"
        );
        renderer.check_health().unwrap();
    }
}

fn mask() -> LayerMask {
    let mut mask = LayerMask::new(
        1,
        VectorPath {
            id: 1,
            closed: true,
            nodes: [
                [-2., -2.],
                [W as f32 + 2., -2.],
                [W as f32 + 2., H as f32 + 2.],
                [-2., H as f32 + 2.],
            ]
            .into_iter()
            .enumerate()
            .map(|(i, p)| PathNode {
                id: i as u64 + 1,
                geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
            })
            .collect(),
        },
    );
    mask.opacity.value = 50.;
    mask
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn masked_add_screen_multiply_stack_matches_independent_linear_goldens_in_four_modes() {
    let package = package();
    for mode in MODES {
        let mut glow = layer(2, "glow", &package);
        glow.masks.push(mask());
        glow.blend = LayerBlend {
            mode: BlendMode::Add,
            space: BlendSpace::Linear,
        };
        let mut screen = layer(3, "wash", &package);
        screen.blend = LayerBlend {
            mode: BlendMode::Screen,
            space: BlendSpace::Linear,
        };
        let mut multiply = layer(4, "wash", &package);
        multiply.blend = LayerBlend {
            mode: BlendMode::Multiply,
            space: BlendSpace::Linear,
        };
        let p = project(
            mode,
            vec![layer(1, "back", &package), glow, screen, multiply],
        );
        let expected = pixels(|x, y| {
            let back = store([0.125, 0.25, 0.375, 1.], mode);
            let coverage = scalar(0.5, mode);
            let field = glow_coverage(x, y, mode, coverage);
            let add = store(
                std::array::from_fn(|c| {
                    if c == 3 {
                        return 1.;
                    }
                    if matches!(mode, ChannelPrecision::Float16 | ChannelPrecision::Float32) {
                        return back[c] + field[c];
                    }
                    // Integer transfer modes operate on bounded straight colors;
                    // saturated Add is evaluated before source-over coverage.
                    let source = if field[3] > 0. {
                        (field[c] / field[3]).clamp(0., 1.)
                    } else {
                        0.
                    };
                    back[c] * (1. - field[3]) + (back[c] + source).min(1.) * field[3]
                }),
                mode,
            );
            let wash = store([0.125, 0.25, 0.0625, 0.5], mode);
            // Premultiplied source-over with an opaque destination.
            let screen = store(
                std::array::from_fn(|c| {
                    if c == 3 {
                        1.
                    } else {
                        add[c] + wash[c] * (1. - add[c])
                    }
                }),
                mode,
            );
            store(
                std::array::from_fn(|c| {
                    if c == 3 {
                        1.
                    } else {
                        screen[c] * (1. - wash[3] + wash[c])
                    }
                }),
                mode,
            )
        });
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            mode,
            Default::default(),
        ))
        .unwrap();
        renderer.set_effect_registry(registry(package.clone()));
        let (actual, display) = render(&mut renderer, &p);
        compare("masked-multiblend", mode, &actual, &expected, &display);
        assert!(renderer.texture_resource_ledger().resources().any(|r| {
            r.references
                .iter()
                .any(|v| v.category == TextureCategory::MaskOutputs)
        }));
        renderer.check_health().unwrap();
    }
}

fn composition(id: &str, layers: Vec<Layer>) -> Composition {
    Composition {
        id: id.into(),
        name: id.into(),
        width: W,
        height: H,
        fps: 30,
        frames: 12,
        work_area: motion_core::WorkArea::full(12),
        background: [0.; 4],
        camera: Camera::new(W, H),
        layers,
        expressions: vec![],
    }
}
fn nested(id: u64, composition: &str) -> Layer {
    let mut l = Layer::solid(
        id,
        composition,
        [W as f32, H as f32],
        [W as f32 / 2., H as f32 / 2., 0.],
        [1.; 4],
    );
    l.content = Content::Composition {
        clip: CompositionClip::new(composition.into()),
    };
    l
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn retained_two_level_nested_shutter_keeps_hdr_and_matches_independent_temporal_golden() {
    let package = package();
    for mode in MODES {
        let mut leaf = layer(1, "glow", &package);
        for (frame, value) in [(0, 0.), (1, 1.), (2, 0.)] {
            leaf.transform
                .opacity
                .upsert(frame, value, Ease::Linear)
                .unwrap();
        }
        leaf.sampling.motion_blur = Some(Shutter {
            frames: 2.,
            samples: 4,
        });
        let mut middle = nested(1, "comp-leaf");
        middle.transform.opacity.value = 0.5;
        let mut outer = nested(1, "comp-middle");
        outer.transform.opacity.value = 0.75;
        let mut p = project(mode, vec![]);
        p.layers = vec![outer];
        p.compositions = vec![
            composition("comp-leaf", vec![leaf]),
            composition("comp-middle", vec![middle]),
        ];
        p.rebuild_plugin_dependencies();
        p.validate().unwrap();
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            mode,
            Default::default(),
        ))
        .unwrap();
        renderer.set_effect_registry(registry(package.clone()));
        let scenes = ExposureScenes::new(
            ExposureDocument::new(Arc::new(p)).unwrap(),
            motion_core::MAIN_COMPOSITION,
            1.,
            false,
            None,
            ExpressionEvaluator,
        )
        .unwrap();
        let mut task = ExposureFrame::new(&mut renderer, scenes, W, H).unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if matches!(
                task.poll(&mut renderer, Duration::from_millis(8), |_, _| Ok(true))
                    .unwrap(),
                ExposurePoll::Ready(_)
            ) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "nested temporal work never completed"
            );
        }
        assert!(
            task.progress().work_units >= 4,
            "shutter receivers were not materialized"
        );
        let expected = pixels(|x, y| {
            let value = glow(x, y, mode);
            // Four centred midpoint taps at 0.25,0.75,1.25,1.75; a triangular
            // authored opacity has values .25,.75,.75,.25, unlike centre=1.
            let samples =
                [0.25, 0.75, 0.75, 0.25].map(|opacity| store(value.map(|v| v * opacity), mode));
            let mean = resolve(
                std::array::from_fn(|c| samples.iter().map(|p| p[c]).sum::<f64>() * 0.25),
                mode,
            );
            let middle = resolve(store(mean.map(|v| v * 0.5), mode), mode);
            resolve(store(middle.map(|v| v * 0.75), mode), mode)
        });
        let actual = linear(&renderer.capture_exposure_image(task.target()).unwrap());
        let display = renderer.sdr_capture_target(W, H, TRANSFORM).unwrap();
        let bytes = renderer
            .capture_exposure_sdr(task.target(), &display)
            .unwrap()
            .pixels;
        compare("retained-nested-shutter", mode, &actual, &expected, &bytes);
        let before = renderer.allocation_budget().info().unwrap();
        for _ in 0..3 {
            assert!(matches!(
                task.poll(&mut renderer, Duration::from_millis(1), |_, _| panic!(
                    "completed task restarted media preparation"
                ))
                .unwrap(),
                ExposurePoll::Ready(_)
            ));
        }
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            before.admitted_bytes
        );
        assert_eq!(
            renderer
                .capture_exposure_sdr(task.target(), &display)
                .unwrap()
                .pixels,
            bytes
        );
        drop(task);
        assert_eq!(renderer.exposure_texture_bytes(), 0);
        renderer.check_health().unwrap();
    }
}
