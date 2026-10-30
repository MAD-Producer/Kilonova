use super::*;

#[test]
fn supported_content_plans_keep_real_channels_and_exclude_unimplemented_properties() {
    let mut project = Project::new(640, 360, 30, 180).unwrap();
    project.assets.push(motion_core::Asset {
        id: 1,
        path: "assets/text.png".into(),
        width: 100,
        height: 100,
    });
    let mut text = Layer::solid(7, "Text", [100.; 2], [0.; 3], [1.; 4]);
    text.content = motion_core::Content::Text {
        text: "Text · 文字".into(),
        font: "system".into(),
        style: motion_core::TextStyle::new(24., 100),
        color: [1.; 4],
        raster_asset: 1,
    };
    text.transform.position.set_animated(0, true).unwrap();
    text.transform.position.set_at(60, [30., 40., 0.]).unwrap();
    let mut spatial = Layer::solid(8, "3D", [100.; 2], [0.; 3], [1.; 4]);
    spatial.three_d = true;
    let camera = Layer::camera(9, "Camera", 640, 360);
    let mut orbit = Layer::camera(10, "Orbit", 640, 360);
    let motion_core::Content::Camera { camera: data } = &mut orbit.content else {
        unreachable!()
    };
    data.mode = motion_core::CameraMode::Orbit;
    project.audio_assets.push(motion_core::AudioAsset {
        id: 2,
        path: "assets/sound.wav".into(),
        mime: "audio/wav".into(),
        bytes: 100,
        track: 0,
        sample_rate: 48000,
        channels: 2,
        sample_frames: 288000,
        duration_us: 6000000,
    });
    let mut audio = Layer::solid(11, "Audio", [0.; 2], [0.; 3], [1.; 4]);
    audio.content = motion_core::Content::Audio {
        audio: motion_core::AudioClip::new(2),
    };
    project.layers.extend([text, spatial, camera, orbit, audio]);
    let engine = Engine::new(project).unwrap();
    let state = state(&engine, 30.);
    let rows = timeline::rows(&state);
    for locale in [Locale::En, Locale::Zh] {
        let mut p = panels();
        p.timeline.expanded.extend([7, 8, 9, 10, 11]);
        for (id, expected) in [
            (
                7,
                vec![
                    Property::Anchor,
                    Property::Position,
                    Property::Scale,
                    Property::Rotation,
                    Property::Opacity,
                ],
            ),
            (
                8,
                vec![
                    Property::Anchor,
                    Property::Position,
                    Property::Scale,
                    Property::Rotation,
                    Property::Opacity,
                ],
            ),
            (
                9,
                vec![
                    Property::Position,
                    Property::Target,
                    Property::Roll,
                    Property::Fov,
                ],
            ),
            (
                10,
                vec![
                    Property::Target,
                    Property::Roll,
                    Property::Fov,
                    Property::Radius,
                    Property::Azimuth,
                    Property::Elevation,
                ],
            ),
            (11, vec![]),
        ] {
            let row = rows.iter().find(|row| row.id == id).unwrap();
            let plan = timeline_layout::LayerRows::new(row, &p.timeline, 30., locale);
            assert_eq!(
                plan.properties
                    .iter()
                    .map(|index| row.properties[*index].property)
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(plan.masks.is_empty() && plan.vectors.is_empty());
            if id == 8 {
                assert_eq!(
                    row.properties
                        .iter()
                        .find(|property| property.property == Property::Position)
                        .unwrap()
                        .axes,
                    [0, 1, 2]
                );
            }
            if id == 9 || id == 10 {
                assert!(plan.effects.is_empty() && plan.audio.is_empty());
            }
            if id == 11 {
                assert!(!plan.audio.is_empty() && plan.transform.is_empty());
            }
        }
        let row = rows.iter().find(|row| row.id == 7).unwrap();
        p.timeline
            .filters
            .insert(7, types::PropertyFilter::Animated);
        let plan = timeline_layout::LayerRows::new(row, &p.timeline, 30., locale);
        assert_eq!(
            plan.properties
                .iter()
                .map(|index| row.properties[*index].property)
                .collect::<Vec<_>>(),
            [Property::Position]
        );
        p.timeline.collapsed_transforms.insert(7);
        assert!(
            timeline_layout::LayerRows::new(row, &p.timeline, 30., locale)
                .properties
                .is_empty()
        );
    }
}
