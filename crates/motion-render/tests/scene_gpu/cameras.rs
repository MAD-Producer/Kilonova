use super::*;
use motion_render::scene_gpu::{ScenePass, ScenePassFrame, ScenePassInstance};

fn source(camera: Value) -> Arc<motion_scene::Scene> {
    fixture_with(
        vec![emissive([1.; 3], 1., "OPAQUE")],
        &[(0., 0)],
        None,
        false,
        |doc, _| {
            doc["cameras"] = json!([camera]);
            let nodes = doc["nodes"].as_array_mut().unwrap();
            nodes.push(json!({"camera":0,"translation":[0,0,3]}));
            let id = nodes.len() - 1;
            doc["scenes"][0]["nodes"]
                .as_array_mut()
                .unwrap()
                .push(json!(id));
        },
    )
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_orthographic_camera_letterboxes_single_and_shared_native_draws() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let a = renderer.render_target(SIZE, SIZE).unwrap();
    let b = renderer.render_target(SIZE, SIZE).unwrap();
    let scene = source(json!({"type":"orthographic","orthographic":{
        "xmag":1.,"ymag":0.5,"znear":0.1,"zfar":10.}}));
    let view = scene.camera_view(0, 1, 1., None, [SIZE, SIZE]).unwrap();
    let mut gpu = GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
    let mut frame = SceneFrame::from_imported_camera(&view, 0, 1.);
    frame.lights = &[];
    frame.ambient = [0.; 3];
    gpu.draw(&renderer, REQUEST, &a, &frame).unwrap();
    let first = pixels(&renderer, &a, ChannelPrecision::Unorm8);
    assert_eq!(center8(&first), [255; 4]);
    for y in [0, 10, SIZE - 11, SIZE - 1] {
        assert!(
            first.pixels[(y * SIZE * 4) as usize..((y + 1) * SIZE * 4) as usize]
                .iter()
                .all(|v| *v == 0),
            "authored aspect must leave actual transparent letterbox rows"
        );
    }
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    let mut item = ScenePassInstance::new(0, REQUEST, 0, 1.);
    item.lights = &[];
    item.ambient = [0.; 3];
    let mut shared = ScenePassFrame::from_imported_camera(&view);
    pass.draw(&renderer, REQUEST, &b, &mut [&mut gpu], &[item], &shared)
        .unwrap();
    assert_eq!(
        pixels(&renderer, &b, ChannelPrecision::Unorm8).pixels,
        first.pixels
    );
    let values = super::shared::depth(&renderer, pass.depth().unwrap());
    assert!(
        (values[(SIZE / 2 * SIZE + SIZE / 2) as usize] - (3. - 0.1) / (10. - 0.1)).abs() < 1e-6
    );
    assert_eq!(
        values[(5 * SIZE + SIZE / 2) as usize],
        1.,
        "letterbox must not write depth"
    );
    frame.viewport = Some([0., 0., SIZE as f32 + 1., SIZE as f32]);
    assert!(gpu
        .draw(&renderer, REQUEST, &a, &frame)
        .unwrap_err()
        .to_string()
        .contains("viewport"));
    assert_eq!(
        pixels(&renderer, &a, ChannelPrecision::Unorm8).pixels,
        first.pixels
    );
    shared.viewport = Some([f32::NAN, 0., 1., 1.]);
    assert!(pass
        .draw(&renderer, REQUEST, &b, &mut [], &[], &shared)
        .unwrap_err()
        .to_string()
        .contains("viewport"));
    assert_eq!(
        pixels(&renderer, &b, ChannelPrecision::Unorm8).pixels,
        first.pixels
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_finite_and_infinite_perspective_cameras_drive_real_hardware_depth() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    for far in [Some(10.), None] {
        let mut camera = json!({"type":"perspective","perspective":{"yfov":1.,"znear":0.1}});
        if let Some(far) = far {
            camera["perspective"]["zfar"] = json!(far);
        }
        let scene = source(camera);
        let view = scene.camera_view(0, 1, 1., None, [SIZE, SIZE]).unwrap();
        let mut gpu =
            GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
        let mut item = ScenePassInstance::new(0, REQUEST, 0, 1.);
        item.lights = &[];
        item.ambient = [0.; 3];
        pass.draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut gpu],
            &[item],
            &ScenePassFrame::from_imported_camera(&view),
        )
        .unwrap();
        assert_eq!(
            center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
            [255; 4]
        );
        let values = super::shared::depth(&renderer, pass.depth().unwrap());
        let expected = far.map_or(1. - 0.1 / 3., |far| far / (far - 0.1) * (1. - 0.1 / 3.));
        assert!((values[(SIZE / 2 * SIZE + SIZE / 2) as usize] - expected).abs() < 1e-6);
    }
}
