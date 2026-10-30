use super::*;
use motion_core::{
    masks::{LayerMask, MaskProperty},
    vector::{groups::*, *},
    AudioAsset, AudioChannel, AudioClip, AudioLevels, Camera, Content, EffectInstance, Track,
    Tween,
};

fn seed<T: Tween>(track: &mut Track<T>) {
    track.set_animated(3, true).unwrap();
}
fn vector_target(item: Option<u64>, parameter: &str) -> KeyframeTarget {
    KeyframeTarget::Vector {
        object: 7,
        item,
        parameter: parameter.into(),
    }
}
fn check(project: Project, targets: Vec<KeyframeTarget>, frame: u32) {
    let engine = Engine::new(project).unwrap();
    let view = state(&engine);
    let mut selected = Set::default();
    for target in targets {
        assert!(present(&engine, &target, frame), "model rejects {target:?}");
        assert!(
            source::contains(&view, &target, frame),
            "UI cannot find {target:?}"
        );
        assert!(
            !source::contains(&view, &target, frame + 1),
            "UI invented key for {target:?}"
        );
        selected
            .apply(&view, [Key { target, frame }], Mode::Add)
            .unwrap();
    }
    assert_eq!(selected.status(&view), Status::Current);
    KeyframeClipboard::capture(engine.project(), &selected.selections()).unwrap();
}

#[test]
fn raw_authored_lookup_agrees_with_model_for_flat_and_nested_vector_channels() {
    for grouped in [false, true] {
        let mut project = fixture().snapshot();
        let mut vector = VectorContent::shape(ShapeKind::Rectangle);
        seed(vector.fill.as_mut().unwrap());
        let mut stroke = Stroke {
            color: Track::constant([1.; 4]),
            width: Track::constant(2.),
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.,
            dashes: Some(StrokeDashes::default()),
        };
        seed(&mut stroke.color);
        seed(&mut stroke.width);
        seed(&mut stroke.dashes.as_mut().unwrap().offset);
        seed(&mut stroke.dashes.as_mut().unwrap().pattern[0]);
        vector.stroke = Some(stroke);
        let mut trim = TrimPaths::default();
        seed(&mut trim.start);
        seed(&mut trim.end);
        seed(&mut trim.offset);
        vector.trim = Some(trim);
        if let VectorSource::Shape { parameters, .. } = &mut vector.source {
            seed(parameters.get_mut("angle").unwrap());
        }
        let mut targets: Vec<_> = [
            "fill",
            "stroke_color",
            "stroke_width",
            "dash_offset",
            "dash_0",
            "trim_start",
            "trim_end",
            "trim_offset",
            "shape:angle",
        ]
        .into_iter()
        .map(|p| vector_target(grouped.then_some(2), p))
        .collect();
        if grouped {
            vector = VectorGroup::wrap(vector, [64.; 2]);
            let VectorSource::Group { group } = &mut vector.source else {
                panic!()
            };
            seed(&mut group.transform.position);
            let mut nested = VectorGroup {
                id: 7,
                name: "Nested".into(),
                transform: Default::default(),
                items: vec![],
            };
            seed(&mut nested.transform.opacity);
            let mut repeater = Repeater::default();
            seed(&mut repeater.copies);
            group.items.push(GroupItem::Repeater {
                id: 6,
                name: "Copies".into(),
                repeater,
            });
            group.items.push(GroupItem::Group {
                group: Box::new(nested),
            });
            targets.extend(
                [(1, "position"), (7, "opacity"), (6, "copies")]
                    .into_iter()
                    .map(|(id, p)| vector_target(Some(id), p)),
            );
        }
        project.layers[0].content = Content::Vector { vector };
        check(project, targets, 10);
    }
}

#[test]
fn raw_authored_lookup_agrees_with_model_for_masks_and_effect_curve_tracks() {
    let mut project = fixture().snapshot();
    let path = VectorPath {
        tangents: Default::default(), id: 1,
        closed: true,
        nodes: [[0., 0.], [40., 0.], [0., 40.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let mut geometry = Track::constant([p[0], p[1], 0., 0., 0., 0.]);
                seed(&mut geometry);
                PathNode {
                    id: i as u64 + 1,
                    geometry,
                }
            })
            .collect(),
    };
    let mut mask = LayerMask::new(1, path);
    seed(&mut mask.opacity);
    seed(&mut mask.feather);
    seed(&mut mask.expansion);
    project.layers[0].masks.push(mask);
    let mut targets: Vec<_> = [
        MaskProperty::Opacity,
        MaskProperty::Feather,
        MaskProperty::Expansion,
        MaskProperty::Node(1),
    ]
    .into_iter()
    .map(|property| KeyframeTarget::Mask {
        object: 7,
        mask: 1,
        property,
    })
    .collect();
    let package = motion_effects::builtin::package().unwrap();
    for (id, name) in [(1, "posterize"), (2, "curves")] {
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|e| e.id == name)
            .unwrap();
        let mut effect = EffectInstance::new(
            id,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [64.; 2],
        );
        let (name, param) = effect
            .params
            .iter_mut()
            .find(|(_, p)| p.animatable && p.implemented && (name != "curves" || p.curve.is_some()))
            .unwrap();
        if let Some(curve) = &mut param.curve {
            curve.keys.push(Keyframe {
                frame: 3,
                value: curve.value.clone(),
                ease: Ease::Hold,
                curve: None,
                spatial: None,
            });
        } else {
            seed(&mut param.track);
        }
        targets.push(KeyframeTarget::Effect {
            object: 7,
            effect: id,
            param: name.clone(),
        });
        project.layers[0].effects.push(effect);
    }
    project.rebuild_plugin_dependencies();
    check(project, targets, 10);
}

#[test]
fn raw_authored_lookup_agrees_with_model_for_camera_offsets_and_stereo_audio() {
    let mut project = fixture().snapshot();
    project.camera = Camera::new(640, 360);
    seed(&mut project.camera.fov);
    check(
        project.clone(),
        vec![KeyframeTarget::Property {
            object: 0,
            property: Property::Fov,
            axis: None,
        }],
        3,
    );
    let mut camera = Layer::camera(11, "Camera", 640, 360);
    camera.timeline = project.layers[0].timeline;
    let Content::Camera { camera: data } = &mut camera.content else {
        panic!()
    };
    seed(&mut data.fov);
    seed(&mut data.position);
    project.layers.push(camera);
    check(
        project.clone(),
        vec![
            KeyframeTarget::Property {
                object: 11,
                property: Property::Fov,
                axis: None,
            },
            KeyframeTarget::Property {
                object: 11,
                property: Property::Position,
                axis: None,
            },
        ],
        10,
    );
    project.audio_assets.push(AudioAsset {
        id: 1,
        path: "assets/tone.wav".into(),
        bytes: 64,
        mime: "audio/wav".into(),
        track: 0,
        sample_rate: 48000,
        channels: 2,
        sample_frames: 192000,
        duration_us: 4000000,
    });
    let layer = &mut project.layers[0];
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    layer.size = [0.; 2];
    layer.transform = motion_core::Transform::new([0.; 3]);
    let mut levels = AudioLevels::default();
    seed(&mut levels.left);
    seed(&mut levels.right);
    layer.audio_levels = Some(levels);
    check(
        project,
        [AudioChannel::Left, AudioChannel::Right]
            .into_iter()
            .map(|channel| KeyframeTarget::Audio { object: 7, channel })
            .collect(),
        10,
    );
}

#[test]
fn malformed_authored_lookup_reaches_default_channels_without_turning_normal_misses_into_faults() {
    const NAME: &str = "panels::key_selection::tests::sources::malformed_authored_lookup_reaches_default_channels_without_turning_normal_misses_into_faults";
    const CHILD: &str = "MOTION_KEY_SELECTION_SOURCE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(root.clone().into()).unwrap();
        let view = state(&fixture());
        let target = key(7, None, 10).target;
        crate::logging::flush();
        let path = std::path::PathBuf::from(root).join("motion-studio.log");
        let before = std::fs::read_to_string(&path).unwrap();
        for frame in [10, 11, 120] {
            source::contains(&view, &target, frame);
        }
        let mut empty = view.clone();
        empty["project"]["layers"][0]["transform"]["position"]["keys"] = json!([]);
        assert!(!source::contains(&empty, &target, 10));
        assert!(!source::contains(&view, &key(999, None, 10).target, 10));
        crate::logging::flush();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        for invalid in [json!([{"frame":"bad"}]), Value::Null] {
            let mut malformed = view.clone();
            malformed["project"]["layers"][0]["transform"]["position"]["keys"] = invalid;
            for _ in 0..120 {
                assert!(!source::contains(&malformed, &target, 10));
                assert!(source::contains(&view, &target, 10));
                assert!(!source::contains(&view, &target, 11));
            }
        }
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_KEY_SELECTION_SOURCE_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for site in ["selection.authored_keys", "selection.authored_key_frame"] {
        let records: Vec<_> = file
            .lines()
            .filter(|line| line.contains(&format!("site={site} ")))
            .collect();
        assert_eq!(records.len(), 2, "{file}");
        assert!(records[0].contains("WARN") && records[0].contains("count=1 "));
        assert!(records[1].contains("ERROR") && records[1].contains("count=60 "));
        assert!(records
            .iter()
            .all(|line| line.contains("object=7") && line.contains("phase=selection")));
        assert!(stderr
            .lines()
            .any(|line| line.contains(site) && line.contains("count=60 ")));
    }
    assert!(
        file.contains("authored key frame is not an integer")
            && file.contains("authored key array missing")
    );
    assert!(!stderr.contains("count=1 ") && !stdout.contains("panel degradation"));
}
