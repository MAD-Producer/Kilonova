use motion_render::Scene;
use motion_core::{compositing::*, *};
use motion_render::Renderer;

fn render(renderer: &mut Renderer, project: &Project, frame: f64) -> Vec<u8> {
    let mut scene = Scene::new(project).unwrap();
    scene.sample(project, frame, None, &motion_core::ExpressionEvaluator).unwrap();
    renderer.retain_video_instances(&scene);
    let target = renderer.capture_target(64, 64).unwrap();
    renderer.capture(&scene, &target).unwrap().0
}
fn fixture(mode: MatteMode) -> Project {
    let mut p = Project::new(64, 64, 24, 120).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.background = [0.; 4];
    p.camera.created = false;
    p.layers = vec![
        Layer::solid(1, "Matte", [24.; 2], [32., 32., 0.], [0.5, 0.5, 0.5, 0.5]),
        Layer::solid(2, "Target", [32.; 2], [32., 32., 0.], [0.9, 0.2, 0.1, 0.7]),
    ];
    p.layers[0].visible = false;
    p.layers[1].track_matte = Some(TrackMatte {
        source: 1,
        mode,
        hide_source: true,
    });
    p.validate().unwrap();
    p
}
fn alpha(pixels: &[u8], x: usize, y: usize) -> u8 {
    pixels[(y * 64 + x) * 4 + 3]
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn track_matte_modes_use_source_coverage_and_do_not_composite_hidden_source() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    for (mode, center, outer) in [
        (MatteMode::Alpha, 89, 0),
        (MatteMode::AlphaInverted, 89, 179),
        (MatteMode::Luma, 45, 0),
        (MatteMode::LumaInverted, 134, 179),
    ] {
        let pixels = render(&mut renderer, &fixture(mode), 0.);
        assert!(
            alpha(&pixels, 32, 32).abs_diff(center) <= 3,
            "{mode:?}: center {}",
            alpha(&pixels, 32, 32)
        );
        assert!(
            alpha(&pixels, 17, 32).abs_diff(outer) <= 3,
            "{mode:?}: outside matte {}",
            alpha(&pixels, 17, 32)
        );
        assert_eq!(alpha(&pixels, 5, 5), 0);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn matte_source_out_of_range_is_zero_coverage_and_random_seek_is_deterministic() {
    let mut p = fixture(MatteMode::Alpha);
    p.layers[0].timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 1,
        offset_frame: 0,
    });
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let before = render(&mut renderer, &p, 0.);
    assert!(alpha(&before, 32, 32).abs_diff(89) <= 3);
    assert_eq!(alpha(&render(&mut renderer, &p, 2.), 32, 32), 0);
    assert_eq!(render(&mut renderer, &p, 0.), before);
    p.layers[1].track_matte.as_mut().unwrap().mode = MatteMode::AlphaInverted;
    assert!(alpha(&render(&mut renderer, &p, 2.), 32, 32).abs_diff(179) <= 3);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn multiply_and_screen_use_straight_colors_then_source_over_alpha() {
    let mut p = Project::new(64, 64, 24, 120).unwrap();
    p.background = [0.; 4];
    p.camera.created = false;
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.layers = vec![
        Layer::solid(1, "Back", [64.; 2], [32., 32., 0.], [0.2, 0.2, 0.2, 1.]),
        Layer::solid(2, "Front", [64.; 2], [32., 32., 0.], [0.7, 0.7, 0.7, 0.5]),
    ];
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    for (mode, expected) in [
        (BlendMode::Normal, 115u8),
        (BlendMode::Multiply, 43u8),
        (BlendMode::Screen, 122u8),
    ] {
        p.layers[1].blend = LayerBlend {
            mode,
            space: BlendSpace::Srgb,
        };
        let pixels = render(&mut renderer, &p, 0.);
        let offset = (32 * 64 + 32) * 4;
        assert_eq!(pixels[offset + 3], 255);
        for c in 0..3 {
            assert!(
                pixels[offset + c].abs_diff(expected) <= 3,
                "{mode:?}: {}",
                pixels[offset + c]
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn matte_uses_effected_pixels_and_authored_opacity_in_a_letterboxed_view() {
    let mut p = fixture(MatteMode::Luma);
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "fill")
        .unwrap();
    let mut effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [24.; 2],
    );
    effect
        .params
        .values_mut()
        .find(|p| p.kind == motion_effects::ParamKind::Color)
        .unwrap()
        .track
        .value = [1.; 4];
    p.layers[0].effects = vec![effect];
    p.layers[0].transform.opacity.value = 0.5;
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let mut scene = Scene::new(&p).unwrap();
    scene.sample(&p, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let target = r.capture_target(96, 64).unwrap();
    let pixels = r.capture(&scene, &target).unwrap().0;
    assert!(pixels[(32 * 96 + 48) * 4 + 3].abs_diff(45) <= 3);
    assert_eq!(pixels[(32 * 96 + 5) * 4 + 3], 0);
    assert_eq!(pixels[(32 * 96 + 29) * 4 + 3], 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn two_matte_levels_multiply_coverage_before_the_target_is_composited() {
    let mut p = fixture(MatteMode::Alpha);
    p.layers.push(Layer::solid(
        3,
        "inner matte",
        [12.; 2],
        [32., 32., 0.],
        [1., 1., 1., 0.25],
    ));
    p.layers[0].track_matte = Some(TrackMatte {
        source: 3,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    p.validate().unwrap();
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let pixels = render(&mut r, &p, 0.);
    assert!(alpha(&pixels, 32, 32).abs_diff(22) <= 3);
    assert_eq!(alpha(&pixels, 23, 32), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn rotated_three_d_matte_uses_the_same_camera_projection_as_the_target() {
    let mut p=fixture(MatteMode::Alpha);
    p.camera.created=true;
    for layer in &mut p.layers {layer.three_d=true;}
    p.layers[0].transform.rotation.value=[0.,0.,30.];
    p.validate().unwrap();
    let mut r=pollster::block_on(Renderer::headless()).unwrap();let pixels=render(&mut r,&p,0.);
    assert!(alpha(&pixels,32,32).abs_diff(89)<=3);
    assert_eq!(alpha(&pixels,44,44),0);
    assert!(alpha(&pixels,42,32)>80);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn normal_blend_and_matte_switches_reuse_pipelines_and_release_working_textures() {
    let mut plain = fixture(MatteMode::Alpha);
    plain.layers[0].visible = true;
    plain.layers[1].track_matte = None;
    let mut blended = plain.clone();
    blended.layers[1].blend = LayerBlend { mode: BlendMode::Multiply, space: BlendSpace::Srgb };
    let mut matted = plain.clone();
    matted.layers[1].track_matte = Some(TrackMatte { source: 1, mode: MatteMode::Alpha, hide_source: true });
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let before = renderer.texture_memory_breakdown();
    let ordinary = render(&mut renderer, &plain, 0.);
    assert_eq!(renderer.compositing_pipeline_set_builds(), 0);
    let blend_pixels = render(&mut renderer, &blended, 0.);
    let matte_pixels = render(&mut renderer, &matted, 0.);
    assert_eq!(renderer.compositing_pipeline_set_builds(), 1);
    for round in 0..8 {
        assert_eq!(render(&mut renderer, &plain, 0.), ordinary);
        let memory = renderer.texture_memory_breakdown();
        // Pipeline retention includes one permanent R8 zero-coverage pixel.
        assert_eq!(memory.matte_blend_scratch, 1);
        assert_eq!(memory.layer_compositing_scratch, 0);
        assert_eq!(memory.total_bytes(), before.total_bytes()+1, "{memory:?}");
        let (project, pixels) = if round % 2 == 0 { (&blended, &blend_pixels) } else { (&matted, &matte_pixels) };
        assert_eq!(&render(&mut renderer, project, 0.), pixels);
        assert_eq!(renderer.compositing_pipeline_set_builds(), 1);
    }
    renderer.reclaim_preview_working_set().unwrap();
    assert_eq!(render(&mut renderer, &plain, 0.), ordinary);
    assert_eq!(renderer.texture_memory_breakdown().total_bytes(), before.total_bytes()+1);
    assert_eq!(render(&mut renderer, &matted, 0.), matte_pixels);
    assert_eq!(renderer.compositing_pipeline_set_builds(), 1);
}
