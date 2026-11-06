use super::*;
use crate::engine::Engine;
use crate::panels::{chrome, floating_chrome, Panels};
use motion_core::{layer_time::*, vector::groups::*, Ease, Project};
use motion_ui::{
    dock::{Node, Panel},
    input::Modifiers,
};
use serde_json::json;
mod faults;
#[cfg(target_os = "windows")]
mod native;
mod navigation;

fn project(reverse: bool) -> Project {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    let path = VectorPath {
        id: 1,
        closed: true,
        tangents: Default::default(),
        nodes: [[10., 10.], [90., 10.], [50., 80.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| PathNode {
                id: i as u64 + 1,
                geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
            })
            .collect(),
    };
    let mut group = VectorGroup {
        id: 1,
        name: "Mapped group".into(),
        transform: Default::default(),
        items: vec![
            GroupItem::Geometry {
                id: 2,
                name: "Triangle".into(),
                vector: Box::new(new_vector(path.clone())),
                size: Track::constant([100.; 2]),
                position: Track::constant([0.; 2]),
            },
            GroupItem::Fill {
                id: 3,
                name: "Fill".into(),
                color: Track::constant([0.2, 0.4, 0.6, 1.]),
                fill_rule: FillRule::NonZero,
                composite: Composite::Above,
            },
        ],
    };
    for (f, v) in [(-6, [10., 20.]), (-3, [30., 40.]), (0, [50., 60.])] {
        group.transform.position.upsert(f, v, Ease::Linear).unwrap();
    }
    if let GroupItem::Fill { color, .. } = &mut group.items[1] {
        for (f, v) in [
            (-6, [0.2, 0.4, 0.6, 1.]),
            (-3, [0.4, 0.5, 0.6, 1.]),
            (0, [0.6, 0.6, 0.6, 1.]),
        ] {
            color.upsert(f, v, Ease::Linear).unwrap();
        }
    }
    let mut layer = Layer::solid(7, "Mapped paths", [100.; 2], [320., 180., 0.], [1.; 4]);
    let mut vector = new_vector(path.clone());
    vector.source = VectorSource::Group {
        group: Box::new(group),
    };
    vector.fill = None;
    vector.stroke = None;
    vector.trim = None;
    layer.content = Content::Vector { vector };
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 0,
        out_frame: 120,
        offset_frame: 2,
    });
    layer.time_mapping = Some(LayerTimeMapping {
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
    let mut mask = LayerMask::new(1, path);
    mask.feather_linked = false;
    for (i, f) in [-6, -3, 0].into_iter().enumerate() {
        mask.opacity
            .upsert(f, 40. + i as f32 * 20., Ease::Linear)
            .unwrap();
        mask.feather
            .upsert(f, [8. + i as f32 * 4., 16. + i as f32 * 8.], Ease::Linear)
            .unwrap();
        mask.expansion
            .upsert(f, i as f32 * 4., Ease::Linear)
            .unwrap();
        for node in &mut mask.path.nodes {
            let mut g = node.geometry.value;
            g[0] += i as f32 * 3.;
            node.geometry.upsert(f, g, Ease::Linear).unwrap();
        }
    }
    layer.masks.push(mask);
    p.layers.push(layer);
    p.validate().unwrap();
    p
}
struct Harness {
    folder: tempfile::TempDir,
    engine: Engine,
    state: Value,
    panels: Panels,
    atlas: TextAtlas,
    host: Panel,
    floating: bool,
}
impl Harness {
    fn new(host: Panel, floating: bool, locale: Locale, reverse: bool) -> Self {
        Self::with_project(host, floating, locale, project(reverse))
    }
    fn with_project(host: Panel, floating: bool, locale: Locale, p: Project) -> Self {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        motion_core::storage::save(&root, &p).unwrap();
        let engine = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
        let state = engine.seek(6.5).unwrap();
        let mut panels = Panels {
            locale,
            ..Default::default()
        };
        panels.focused = Some(host);
        panels.select_only(Some(7));
        panels.path_controls.masks_open = true;
        if let Content::Vector { vector } = &p.layers[0].content {
            let tree = groups::parameters::tree(vector, p.layers[0].size, -3., locale).unwrap();
            panels.path_controls.groups.reveal_tree(7, &tree);
        }
        let mut h = Self {
            folder,
            engine,
            state,
            panels,
            atlas: TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap(),
            host,
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
                self.host,
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
                &Node::dock(self.host.dock()),
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
    fn run(&mut self, actions: Vec<Action>) -> Vec<String> {
        let mut errors = vec![];
        for action in actions {
            let result = match action {
                Action::Edit(c) => self.engine.edit(serde_json::to_string(&c).unwrap()),
                Action::EditBatch(c) => self.engine.edit(serde_json::to_string(&c).unwrap()),
                Action::Seek(f) => self.engine.seek(f),
                Action::History(op) => self.engine.history(op),
                Action::EditorAction { source, commands } => {
                    self.engine
                        .call(|reply| crate::engine::Command::EditorAction {
                            source,
                            commands,
                            reply,
                        })
                }
                _ => panic!("unexpected path action {action:?}"),
            };
            match result {
                Ok(s) => self.state = s,
                Err(e) => {
                    errors.push(e.to_string());
                    self.state = self.engine.state().unwrap();
                }
            }
        }
        errors
    }
    fn input(&mut self, input: Input) {
        let out = self.draw(input);
        assert!(self.run(out).is_empty());
    }
    fn rect(&self) -> (Rect, Rect) {
        if self.host == Panel::Vector {
            self.panels.path_controls.groups.rects[&(self.host as u32, 7, 1, "position".into(), 0)]
        } else {
            self.panels.path_controls.mask_property_rects[&(self.host as u32, 7, 1, 1, 0)]
        }
    }
    fn type_number(&mut self, text: &str) -> Vec<String> {
        self.type_at(self.rect().1, text)
    }
    fn type_at(&mut self, rect: Rect, text: &str) -> Vec<String> {
        let pt = center(rect);
        let mut errors = vec![];
        for input in [
            pointer(pt, true, false),
            pointer(pt, false, true),
            Input {
                text: text.into(),
                ..Default::default()
            },
            Input {
                keys: vec![(Key::Enter, Modifiers::default())],
                ..Default::default()
            },
        ] {
            let out = self.draw(input);
            errors.extend(self.run(out));
        }
        errors
    }
    fn expected(&self, before: &Value, value: f64) -> Value {
        let mut expected = before.clone();
        if self.host == Panel::Vector {
            expected["layers"][0]["content"]["vector"]["source"]["group"]["transform"]
                ["position"]["keys"][1]["value"][0] = json!(value);
        } else {
            expected["layers"][0]["masks"][0]["feather"]["keys"][1]["value"][0] = json!(value);
        }
        expected
    }
}
fn center(r: Rect) -> [f32; 2] {
    [r.min[0] + r.width() * 0.5, r.min[1] + r.height() * 0.5]
}
fn pointer(pt: [f32; 2], pressed: bool, released: bool) -> Input {
    Input {
        mouse: pt,
        held: (!released).then_some(MouseButton::Left),
        pressed: pressed.then_some((MouseButton::Left, pt)),
        released: released.then_some((MouseButton::Left, pt)),
        ..Default::default()
    }
}

#[test]
fn mapped_mask_and_vector_numeric_chrome_edits_preserve_the_other_component_and_all_other_tracks() {
    for host in [Panel::Masks, Panel::Vector] {
        for reverse in [false, true] {
            for locale in [Locale::Zh, Locale::En] {
                for floating in [false, true] {
                    let mut h = Harness::new(host, floating, locale, reverse);
                    let before = h.state["project"].clone();
                    assert!(h.type_number("17.25").is_empty());
                    assert_eq!(h.state["frame"], 6.5);
                    assert_eq!(h.state["project"], h.expected(&before, 17.25));
                    assert_eq!(h.engine.history(0).unwrap()["project"], before);
                    let after = h.engine.history(1).unwrap();
                    assert_eq!(after["project"], h.expected(&before, 17.25));
                    h.engine.save().unwrap();
                    assert_eq!(
                        serde_json::to_value(
                            motion_core::storage::load(&h.folder.path().join("project")).unwrap()
                        )
                        .unwrap(),
                        after["project"]
                    );
                }
            }
        }
    }
}

#[test]
fn mapped_color_popup_and_linked_mask_feather_preserve_the_exact_companion_components() {
    for reverse in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for floating in [false, true] {
                let mut h = Harness::new(Panel::Vector, floating, locale, reverse);
                let preview = h.panels.path_controls.groups.color_rects
                    [&(Panel::Vector as u32, 7, 3, "color".into())];
                h.input(pointer(center(preview), true, false));
                h.input(pointer(center(preview), false, true));
                let before = h.state["project"].clone();
                let number = h.panels.path_controls.groups.rects
                    [&(Panel::Vector as u32, 7, 3, "color".into(), 0)]
                    .1;
                assert!(h.type_at(number, "0.75").is_empty());
                let mut expected = before.clone();
                expected["layers"][0]["content"]["vector"]["source"]["group"]["items"][1]
                    ["color"]["keys"][1]["value"][0] = json!(0.75);
                assert_eq!(h.state["project"], expected);
                assert_eq!(h.engine.history(0).unwrap()["project"], before);
                assert_eq!(h.engine.history(1).unwrap()["project"], expected);
                let mut h = Harness::new(Panel::Masks, floating, locale, reverse);
                h.state = h
                    .engine
                    .edit(
                        serde_json::to_string(&Command::Mask {
                            object: 7,
                            action: MaskAction::FeatherLink {
                                mask: 1,
                                linked: true,
                            },
                        })
                        .unwrap(),
                    )
                    .unwrap();
                let before = h.state["project"].clone();
                assert!(h.type_number("17.25").is_empty());
                let mut expected = h.expected(&before, 17.25);
                expected["layers"][0]["masks"][0]["feather"]["keys"][1]["value"][1] = json!(34.5);
                assert_eq!(h.state["project"], expected);
                assert_eq!(h.engine.history(0).unwrap()["project"], before);
                assert_eq!(h.engine.history(1).unwrap()["project"], expected);
            }
        }
    }
}

#[test]
fn mapped_path_numeric_preview_cancels_once_on_subframe_or_clock_identity_change() {
    for host in [Panel::Masks, Panel::Vector] {
        for reverse in [false, true] {
            for floating in [false, true] {
                for changed in ["subframe", "clock", "generation"] {
                    let mut h = Harness::new(host, floating, Locale::En, reverse);
                    let before = h.state["project"].clone();
                    let start = center(h.rect().1);
                    let end = [start[0] + 20., start[1]];
                    h.input(pointer(start, true, false));
                    h.input(pointer(end, false, false));
                    assert_ne!(h.state["project"], before);
                    match changed {
                        "subframe" => h.state = h.engine.seek(6.75).unwrap(),
                        "clock" => {
                            let clock = LayerClock::new(
                                TimeStretch::new(3, 2).unwrap(),
                                if reverse {
                                    TimeDirection::Reverse
                                } else {
                                    TimeDirection::Forward
                                },
                                1.,
                                if reverse { -2. / 3. } else { -6. + 2. / 3. },
                            )
                            .unwrap();
                            assert!((clock.key_edit_time(6.5, 2).unwrap() + 3.).abs() < 1e-12);
                            h.state["project"]["layers"][0]["time_mapping"]["clock"] =
                                serde_json::to_value(clock).unwrap();
                        }
                        _ => {
                            let n = h.state["document_generation"].as_u64().unwrap();
                            h.state["document_generation"] = json!(n + 1);
                        }
                    }
                    let out = h.draw(pointer(end, false, false));
                    assert!(
                        matches!(out.as_slice(), [Action::History(4)]),
                        "{host:?}/{changed}: {out:?}"
                    );
                    assert!(h.run(out).is_empty());
                    assert_eq!(h.state["project"], before);
                    assert!(h.draw(pointer(end, false, true)).is_empty());
                    assert_eq!(h.engine.state().unwrap()["history"]["undoEntries"], 0);
                }
            }
        }
    }
}

#[test]
fn mapped_mask_pose_and_vector_stopwatches_freeze_the_exact_fractional_sample_with_single_undo() {
    for reverse in [false, true] {
        for floating in [false, true] {
            for host in [Panel::Masks, Panel::Vector] {
                let mut h = Harness::new(host, floating, Locale::Zh, reverse);
                h.state = h.engine.seek(8.75).unwrap();
                assert!(h.draw(Input::default()).is_empty());
                let p: Project = serde_json::from_value(h.state["project"].clone()).unwrap();
                let local = p.layers[0].local_frame(8.75);
                for index in if host == Panel::Masks {
                    vec![0, 1, 2, 3]
                } else {
                    vec![0]
                } {
                    let mut expected = p.clone();
                    let watch = if host == Panel::Vector {
                        if let Content::Vector { vector } = &mut expected.layers[0].content {
                            if let VectorSource::Group { group } = &mut vector.source {
                                group.transform.position = Track::constant(
                                    group.transform.position.sample(local).unwrap(),
                                );
                            }
                        }
                        h.rect().0
                    } else {
                        let m = &mut expected.layers[0].masks[0];
                        match index {
                            0 => m.opacity = Track::constant(m.opacity.sample(local).unwrap()),
                            1 => m.feather = Track::constant(m.feather.sample(local).unwrap()),
                            2 => m.expansion = Track::constant(m.expansion.sample(local).unwrap()),
                            _ => {
                                for n in &mut m.path.nodes {
                                    n.geometry = Track::constant(n.geometry.sample(local).unwrap());
                                }
                            }
                        }
                        h.panels.path_controls.mask_property_rects[&(host as u32, 7, 1, index, 0)].0
                    };
                    h.input(pointer(center(watch), true, false));
                    assert_eq!(h.state["project"], serde_json::to_value(expected).unwrap());
                    assert_eq!(
                        h.engine.history(0).unwrap()["project"],
                        serde_json::to_value(&p).unwrap()
                    );
                    h.state = h.engine.state().unwrap();
                }
            }
        }
    }
}
