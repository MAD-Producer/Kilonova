use super::*;
use crate::{
    engine::Engine,
    panels::{chrome, floating_chrome, Panels},
};
use motion_core::{layer_time::*, Camera, Content, Ease, Layer, LayerTimeline, Project};
use motion_ui::{
    dock::{Node, Panel},
    input::{Key, Modifiers},
};
use serde_json::json;
mod faults;
mod stable;
#[cfg(target_os = "windows")]
mod native;

fn project(reverse: bool) -> Project {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    let mut camera = Camera::new(640, 360);
    camera.created = true;
    for (f, v) in [(-6, 40.), (-3, 60.), (0, 80.)] {
        camera.fov.upsert(f, v, Ease::Linear).unwrap();
        camera
            .position
            .upsert(f, [v, 180., -900.], Ease::Linear)
            .unwrap();
        camera
            .target
            .upsert(f, [320., v, 0.], Ease::Linear)
            .unwrap();
    }
    let mut l = Layer::camera(7, "Mapped camera", 640, 360);
    l.content = Content::Camera {
        camera: Box::new(camera),
    };
    l.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 120,
        offset_frame: 2,
    });
    l.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if reverse { 0. } else { -6. },
        )
        .unwrap(),
        remap: None,
    });
    p.layers.push(l);
    p.validate().unwrap();
    p
}
struct Harness {
    folder: tempfile::TempDir,
    engine: Engine,
    state: Value,
    panels: Panels,
    atlas: TextAtlas,
    floating: bool,
}
impl Harness {
    fn new(floating: bool, locale: Locale, reverse: bool) -> Self {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        motion_core::storage::save(&root, &project(reverse)).unwrap();
        let engine = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
        let state = engine.seek(6.5).unwrap();
        let mut panels = Panels {
            locale,
            ..Default::default()
        };
        panels.focused = Some(Panel::Properties);
        panels.select_only(Some(7));
        let mut h = Self {
            folder,
            engine,
            state,
            panels,
            atlas: TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap(),
            floating,
        };
        assert!(h.draw(Input::default()).is_empty());
        h
    }
    fn paint(&mut self, input: Input) -> (PaintList, Vec<Action>) {
        let mut paint = PaintList::default();
        let actions = if self.floating {
            floating_chrome(
                &mut paint,
                &mut self.atlas,
                Panel::Properties,
                [1000., 900.],
                &mut self.panels,
                &self.state,
                &input,
            )
        } else {
            chrome(
                &mut paint,
                &mut self.atlas,
                [1000., 900.],
                &Node::dock(Panel::Properties.dock()),
                &mut self.panels,
                &self.state,
                &input,
            )
        };
        (paint, actions)
    }
    fn draw(&mut self, input: Input) -> Vec<Action> {
        self.paint(input).1
    }
    fn run(&mut self, actions: Vec<Action>) {
        for a in actions {
            self.state = match a {
                Action::Edit(c) => self.engine.edit(serde_json::to_string(&c).unwrap()),
                Action::EditBatch(c) => self.engine.edit(serde_json::to_string(&c).unwrap()),
                Action::History(op) => self.engine.history(op),
                Action::Seek(f) => self.engine.seek(f),
                a => panic!("unexpected {a:?}"),
            }
            .unwrap();
        }
    }
    fn input(&mut self, input: Input) {
        let out = self.draw(input);
        self.run(out);
    }
    fn rect(&self, property: Property, axis: usize) -> Rect {
        self.panels
            .camera_controls
            .rects
            .iter()
            .find(|(p, a, _)| *p == property && *a == axis)
            .unwrap()
            .2
    }
    fn lens_rect(&self, p: lens::Parameter) -> Rect {
        self.panels
            .camera_controls
            .lens
            .rects
            .iter()
            .find(|(x, _)| *x == p)
            .unwrap()
            .1
    }
    fn type_at(&mut self, rect: Rect, value: &str) {
        let pt = center(rect);
        for input in [
            pointer(pt, 0),
            pointer(pt, 2),
            Input {
                text: value.into(),
                ..Default::default()
            },
            key(Key::Enter),
        ] {
            self.input(input);
        }
    }
}
fn center(r: Rect) -> [f32; 2] {
    [r.min[0] + r.width() * 0.5, r.min[1] + r.height() * 0.5]
}
fn pointer(pt: [f32; 2], phase: u8) -> Input {
    Input {
        mouse: pt,
        held: (phase < 2).then_some(MouseButton::Left),
        pressed: (phase == 0).then_some((MouseButton::Left, pt)),
        released: (phase == 2).then_some((MouseButton::Left, pt)),
        ..Default::default()
    }
}
fn key(k: Key) -> Input {
    Input {
        keys: vec![(k, Modifiers::default())],
        ..Default::default()
    }
}
fn camera_mut(p: &mut Value) -> &mut Value {
    &mut p["layers"][0]["content"]["camera"]
}

#[test]
fn camera_chrome_writes_exact_signed_pose_and_fov_keys_in_both_hosts_languages_and_directions() {
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for reverse in [false, true] {
                let mut h = Harness::new(floating, locale, reverse);
                for (property, name, axis) in [
                    (Property::Position, "position", 0),
                    (Property::Target, "target", 1),
                    (Property::Fov, "fov", 0),
                ] {
                    let before = h.state["project"].clone();
                    h.type_at(h.rect(property, axis), "75");
                    let mut expected = before.clone();
                    if property == Property::Fov {
                        camera_mut(&mut expected)[name]["keys"][1]["value"] = json!(75.);
                    } else {
                        camera_mut(&mut expected)[name]["keys"][1]["value"][axis] = json!(75.);
                    }
                    assert_eq!(
                        h.state["project"], expected,
                        "{property:?} {locale:?} {floating} {reverse}"
                    );
                    assert_eq!(h.engine.history(0).unwrap()["project"], before);
                    h.state = h.engine.history(1).unwrap();
                    assert_eq!(h.state["project"], expected);
                    h.engine.save().unwrap();
                    assert_eq!(
                        serde_json::to_value(
                            motion_core::storage::load(&h.folder.path().join("project")).unwrap()
                        )
                        .unwrap(),
                        expected
                    );
                }
                for parameter in [
                    lens::Parameter::Focal,
                    lens::Parameter::Film,
                    lens::Parameter::Zoom,
                ] {
                    h.input(Input::default());
                    let before = h.state["project"].clone();
                    let (original, _) = lens::snapshot(&h.state, 7).unwrap();
                    let value = match parameter {
                        lens::Parameter::Focal => 35.,
                        lens::Parameter::Film => 24.,
                        lens::Parameter::Zoom => 400.,
                    };
                    let (film, focal) = match parameter {
                        lens::Parameter::Focal => (original.film_width_mm, value),
                        lens::Parameter::Film => (value, original.focal_length_mm),
                        lens::Parameter::Zoom => (
                            original.film_width_mm,
                            value * original.focal_length_mm / original.zoom_px,
                        ),
                    };
                    // The public lens inputs are f32; compare the complete
                    // project against that protocol, without an epsilon mask.
                    let fov = motion_core::lens::Lens::from_focal_length(
                        f64::from(film as f32),
                        f64::from(focal as f32),
                        640,
                        360,
                    )
                    .unwrap()
                    .vertical_fov_degrees as f32;
                    h.type_at(h.lens_rect(parameter), &value.to_string());
                    let mut expected = before.clone();
                    camera_mut(&mut expected)["film_width_mm"] = json!(film as f32);
                    camera_mut(&mut expected)["fov"]["keys"][1]["value"] = json!(fov);
                    assert_eq!(h.state["project"], expected, "{parameter:?}");
                    assert_eq!(h.engine.history(0).unwrap()["project"], before);
                    h.state = h.engine.history(1).unwrap();
                    assert_eq!(h.state["project"], expected);
                }
            }
        }
    }
}

#[test]
fn camera_numeric_drafts_cancel_on_clock_generation_time_focus_and_lock_changes() {
    for floating in [false, true] {
        for field in 0..2 {
            for case in 0..6 {
                let mut h = Harness::new(floating, Locale::En, false);
                let before = h.state["project"].clone();
                let pt = center(if field == 0 {
                    h.rect(Property::Fov, 0)
                } else {
                    h.lens_rect(lens::Parameter::Zoom)
                });
                h.input(pointer(pt, 0));
                h.input(pointer([pt[0] + 12., pt[1]], 1));
                assert_ne!(h.state["project"], before);
                let mut next = Input::default();
                match case {
                    0 => {
                        h.state["document_generation"] =
                            json!(h.state["document_generation"].as_u64().unwrap() + 1)
                    }
                    1 => {
                        h.state["project"]["layers"][0]["time_mapping"]["clock"]["local_pivot"] =
                            json!(3.)
                    }
                    2 => h.state["frame"] = json!(7.),
                    3 => next.focus_lost = true,
                    4 => h.state["project"]["layers"][0]["locked"] = json!(true),
                    _ => next = key(Key::Escape),
                }
                let out = h.draw(next);
                assert!(
                    matches!(out.as_slice(), [Action::History(4)]),
                    "{case}: {out:?}"
                );
                h.run(out);
                assert_eq!(h.state["project"], before);
                assert!(!h.panels.camera_controls.editing());
            }
        }
    }
}

#[test]
fn mapped_camera_reset_menu_writes_only_the_clicked_local_key_with_one_undo() {
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for reverse in [false, true] {
                let mut h = Harness::new(floating, locale, reverse);
                let before = h.state["project"].clone();
                let point = center(h.rect(Property::Fov, 0));
                h.input(Input {
                    mouse: point,
                    pressed: Some((MouseButton::Right, point)),
                    ..Default::default()
                });
                assert!(h.panels.camera_controls.reset.open);
                h.input(key(Key::Enter));
                let mut expected = before.clone();
                camera_mut(&mut expected)["fov"]["keys"][1]["value"] = json!(45.);
                assert_eq!(h.state["project"], expected);
                assert_eq!(h.engine.history(0).unwrap()["project"], before);
            }
        }
    }
}

#[test]
fn fractional_local_camera_keys_are_disabled_but_static_values_and_animation_freeze_remain_exact() {
    let mut h = Harness::new(false, Locale::Zh, false);
    h.state = h.engine.seek(7.).unwrap();
    h.input(Input::default());
    let before = h.state["project"].clone();
    for r in [h.rect(Property::Fov, 0), h.lens_rect(lens::Parameter::Zoom)] {
        assert!(h.draw(pointer(center(r), 0)).is_empty());
        assert!(!h.panels.camera_controls.editing());
    }
    let number = h.rect(Property::Fov, 0);
    // The watch occupies the existing token-defined column before the caption.
    let styles = PaintList::default().styles();
    let area = Rect::new(0., 80., 1000., 800.);
    let watch =
        super::super::properties::inspector_layout(&styles, area, number.min[1], 0, 1).watch;
    let sampled = h.state["sampledLayers"][0]["fov"].clone();
    h.input(pointer(center(watch), 0));
    let mut expected = before.clone();
    camera_mut(&mut expected)["fov"]["value"] = sampled;
    camera_mut(&mut expected)["fov"]["keys"] = json!([]);
    assert_eq!(h.state["project"], expected);
    h.type_at(h.rect(Property::Fov, 0), "55");
    camera_mut(&mut expected)["fov"]["value"] = json!(55.);
    assert_eq!(h.state["project"], expected);
}
