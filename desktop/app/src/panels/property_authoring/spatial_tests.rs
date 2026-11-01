//! Continuous storage needs exact playhead authoring even without time mapping.
use super::*;
use motion_core::{
    layer_time::*, Camera, Content, Ease, Engine, Layer, LayerTimeline, Project, Track,
};
use serde_json::json;

mod worker;

#[derive(Clone, Copy, Debug)]
enum Channel {
    Position,
    Anchor,
    CameraPosition,
    CameraTarget,
    GlobalPosition,
    GlobalTarget,
}
impl Channel {
    fn object(self) -> u64 {
        if matches!(self, Self::GlobalPosition | Self::GlobalTarget) {
            0
        } else {
            7
        }
    }
    fn property(self) -> Property {
        match self {
            Self::Anchor => Property::Anchor,
            Self::CameraTarget | Self::GlobalTarget => Property::Target,
            _ => Property::Position,
        }
    }
    fn target(self) -> KeyframeTarget {
        KeyframeTarget::Property {
            object: self.object(),
            property: self.property(),
            axis: None,
        }
    }
}
fn vector(base: [f32; 3], free: bool) -> Track<[f32; 3]> {
    let mut t = Track::constant(base);
    for (f, x) in [(0, 0.), (4, 1.), (10, 4.), (20, 8.)] {
        t.upsert(f, [base[0] + x, base[1], base[2]], Ease::Linear)
            .unwrap();
    }
    t.enable_roving(&[4]).unwrap();
    if !free {
        t.spatial_animation
            .as_mut()
            .unwrap()
            .set_fixed_time(2, 4.125)
            .unwrap();
    }
    t
}
fn fixture(channel: Channel, mode: &str, free: bool) -> (Project, f64) {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    let mut l = Layer::solid(
        7,
        "Continuous property",
        [100.; 2],
        [320., 180., 0.],
        [0.2, 0.5, 0.8, 1.],
    );
    let local;
    match channel {
        Channel::Position => {
            l.transform.position = vector([320., 180., 0.], free);
            local = l
                .transform
                .position
                .spatial_animation
                .as_ref()
                .unwrap()
                .prepared()
                .keys()
                .find(|(id, _)| *id == 2)
                .unwrap()
                .1;
        }
        Channel::Anchor => {
            let mut t = Track::constant([0.5; 2]);
            for (f, x) in [(0, 0.), (4, 0.125), (10, 0.5), (20, 1.)] {
                t.upsert(f, [x, 0.5], Ease::Linear).unwrap();
            }
            t.enable_roving(&[4]).unwrap();
            if !free {
                t.spatial_animation
                    .as_mut()
                    .unwrap()
                    .set_fixed_time(2, 4.125)
                    .unwrap();
            }
            local = t
                .spatial_animation
                .as_ref()
                .unwrap()
                .prepared()
                .keys()
                .find(|(id, _)| *id == 2)
                .unwrap()
                .1;
            l.transform.anchor = t;
        }
        _ => {
            let mut camera = Camera::new(640, 360);
            camera.created = true;
            let t = vector(
                if channel.property() == Property::Position {
                    [320., 180., -900.]
                } else {
                    [320., 180., 0.]
                },
                free,
            );
            local = t
                .spatial_animation
                .as_ref()
                .unwrap()
                .prepared()
                .keys()
                .find(|(id, _)| *id == 2)
                .unwrap()
                .1;
            if channel.property() == Property::Position {
                camera.position = t;
            } else {
                camera.target = t;
            }
            if channel.object() == 0 {
                p.camera = camera;
            } else {
                l = Layer::camera(7, "Continuous camera", 640, 360);
                l.content = Content::Camera {
                    camera: Box::new(camera),
                };
            }
        }
    }
    let at = if channel.object() == 0 {
        local
    } else {
        l.timeline = Some(LayerTimeline {
            in_frame: 0,
            out_frame: 120,
            offset_frame: 2,
        });
        if mode != "plain" {
            l.time_mapping = Some(LayerTimeMapping {
                clock: LayerClock::new(
                    TimeStretch::new(3, 2).unwrap(),
                    if mode == "reverse" {
                        TimeDirection::Reverse
                    } else {
                        TimeDirection::Forward
                    },
                    0.,
                    if mode == "reverse" { 20. } else { 0. },
                )
                .unwrap(),
                remap: None,
            });
        }
        let at = l.property_composition_frame(local).unwrap();
        p.layers.push(l);
        at
    };
    p.validate().unwrap();
    assert_ne!(at.fract(), 0.);
    (p, at)
}
fn state(p: &Project, at: f64) -> Value {
    json!({"root":"continuous-property","composition":p.composition_id,"document_generation":1,"revision":0,"frame":at,"project":p,"timeline_layers":p.timeline_layers(at).unwrap()})
}
fn expected(p: &Project, channel: Channel) -> Project {
    let mut p = p.clone();
    if matches!(channel, Channel::Anchor) {
        p.layers[0]
            .transform
            .anchor
            .spatial_animation
            .as_mut()
            .unwrap()
            .set_value(2, [0.25, 0.75])
            .unwrap();
    } else {
        let t = match channel {
            Channel::Position => &mut p.layers[0].transform.position,
            Channel::GlobalPosition => &mut p.camera.position,
            Channel::GlobalTarget => &mut p.camera.target,
            _ => {
                let Content::Camera { camera } = &mut p.layers[0].content else {
                    panic!("camera fixture")
                };
                if channel.property() == Property::Position {
                    &mut camera.position
                } else {
                    &mut camera.target
                }
            }
        };
        t.spatial_animation
            .as_mut()
            .unwrap()
            .set_value(
                2,
                if channel.property() == Property::Target {
                    [340., 185., 0.]
                } else if matches!(channel, Channel::Position) {
                    [340., 185., 0.]
                } else {
                    [340., 185., -900.]
                },
            )
            .unwrap();
    }
    p
}
fn command(channel: Channel, at: f64) -> Command {
    Command::SetVector {
        object: channel.object(),
        property: channel.property(),
        frame: at.floor() as u32,
        value: if matches!(channel, Channel::Anchor) {
            [0.25, 0.75, 0.]
        } else if matches!(channel, Channel::CameraPosition | Channel::GlobalPosition) {
            [340., 185., -900.]
        } else {
            [340., 185., 0.]
        },
    }
}

#[test]
fn continuous_property_edits_preserve_ids_in_plain_forward_and_reverse_clocks() {
    for channel in [
        Channel::Position,
        Channel::Anchor,
        Channel::CameraPosition,
        Channel::CameraTarget,
        Channel::GlobalPosition,
        Channel::GlobalTarget,
    ] {
        for mode in ["plain", "forward", "reverse"] {
            if channel.object() == 0 && mode != "plain" {
                continue;
            }
            for free in [false, true] {
                let (p, at) = fixture(channel, mode, free);
                let s = state(&p, at);
                let edit = at_playhead(&s, command(channel, at));
                assert!(
                    matches!(&edit,Command::TrackAt {target,frame,..} if target==&channel.target() && *frame==at),
                    "{channel:?} {mode} {free}: {edit:?}"
                );
                let mut e = Engine::new(p.clone()).unwrap();
                e.apply(edit).unwrap();
                let next = expected(&p, channel);
                assert_eq!(e.project(), &next);
                e.undo().unwrap();
                assert_eq!(e.project(), &p);
                assert!(!e.can_undo());
                e.redo().unwrap();
                assert_eq!(e.project(), &next);
                let folder = tempfile::tempdir().unwrap();
                motion_core::storage::save(folder.path(), &next).unwrap();
                assert_eq!(motion_core::storage::load(folder.path()).unwrap(), next);
                if mode == "plain" {
                    let mut old = Engine::new(p.clone()).unwrap();
                    old.apply(command(channel, at)).unwrap();
                    assert_ne!(
                        old.project(),
                        &next,
                        "negative control must expose the rounded insertion"
                    );
                    let old = json!(old.project());
                    let source = match channel {
                        Channel::Position | Channel::Anchor => &old["layers"][0]["transform"],
                        Channel::GlobalPosition | Channel::GlobalTarget => &old["camera"],
                        _ => &old["layers"][0]["content"]["camera"],
                    };
                    let name = match channel.property() {
                        Property::Anchor => "anchor",
                        Property::Target => "target",
                        _ => "position",
                    };
                    assert_eq!(
                        source[name]["spatial_animation"]["keys"]
                            .as_array()
                            .unwrap()
                            .len(),
                        5,
                        "legacy insertion adds a fifth key"
                    );
                }
            }
        }
    }
}

#[test]
fn continuous_numeric_drafts_capture_fractional_time_and_cancel_on_context_changes() {
    for mode in ["plain", "forward", "reverse"] {
        let (p, at) = fixture(Channel::Position, mode, false);
        let s = state(&p, at);
        let captured = NumericTime::read(&s, 7).unwrap();
        assert!(captured.is_some());
        for field in [
            "frame",
            "root",
            "composition",
            "generation",
            "offset",
            "clock",
        ] {
            let mut changed = s.clone();
            match field {
                "frame" => changed["frame"] = json!(at + 0.25),
                "root" => changed["root"] = json!("other"),
                "composition" => changed["composition"] = json!("other"),
                "generation" => changed["document_generation"] = json!(2),
                "offset" => changed["project"]["layers"][0]["timeline"]["offset_frame"] = json!(3),
                _ => {
                    changed["project"]["layers"][0]["time_mapping"] = json!(LayerTimeMapping {
                        clock: LayerClock::new(
                            TimeStretch::new(2, 1).unwrap(),
                            TimeDirection::Forward,
                            0.,
                            0.
                        )
                        .unwrap(),
                        remap: None
                    })
                }
            }
            assert!(
                !NumericTime::current(&captured, &changed, 7).unwrap(),
                "{mode} {field}"
            );
        }
        let mut own = s.clone();
        own["revision"] = json!(99);
        assert!(NumericTime::current(&captured, &own, 7).unwrap());
    }
}

#[test]
fn ordinary_unmapped_channels_keep_legacy_authoring_and_partial_static_views() {
    let (mut p, at) = fixture(Channel::Position, "plain", false);
    p.layers[0].transform.position = Track::constant([320., 180., 0.]);
    let s = state(&p, at);
    assert!(!precise(&s, 7));
    assert!(NumericTime::read(&s, 7).unwrap().is_none());
    assert!(matches!(
        at_playhead(&s, command(Channel::Position, at)),
        Command::SetVector { .. }
    ));
    let partial = json!({"frame":3.,"project":{"layers":[]}});
    assert!(!precise(&partial, 7));
    let mut invalid = s;
    invalid["frame"] = Value::Null;
    assert!(precise(&invalid, 7));
    assert!(NumericTime::read(&invalid, 7).is_err());
}

#[test]
fn malformed_continuous_storage_never_selects_the_rounded_legacy_path() {
    let (p, at) = fixture(Channel::Position, "plain", false);
    let mut s = state(&p, at);
    s["project"]["layers"][0]["transform"]["position"]["spatial_animation"] = json!("broken");
    assert!(precise(&s, 7));
    assert!(
        matches!(at_playhead(&s, command(Channel::Position, at)), Command::TrackAt {frame, ..} if frame == at)
    );
    let captured = NumericTime::read(&s, 7).unwrap();
    s["project"]["layers"][0]["transform"]["position"]["spatial_animation"] = Value::Null;
    assert!(!NumericTime::current(&captured, &s, 7).unwrap());
}

#[test]
fn continuous_objects_do_not_round_other_animated_properties_into_adjacent_keys() {
    let (mut p, at) = fixture(Channel::Position, "plain", false);
    for (frame, value) in [(0, 0.), (10, 30.)] {
        p.layers[0]
            .transform
            .rotation
            .upsert(frame, [0., 0., value], Ease::Linear)
            .unwrap();
    }
    let s = state(&p, at);
    let c = at_playhead(
        &s,
        Command::SetVector {
            object: 7,
            property: Property::Rotation,
            frame: at.floor() as u32,
            value: [0., 0., 20.],
        },
    );
    assert!(matches!(&c, Command::TrackAt {frame, ..} if *frame == at));
    let mut engine = Engine::new(p.clone()).unwrap();
    let error = engine.apply(c).unwrap_err();
    assert!(
        error.to_string().contains("continuous key addressing"),
        "{error}"
    );
    assert_eq!(engine.project(), &p);
    assert!(!engine.can_undo());
}
