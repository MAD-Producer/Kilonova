use super::*;
use crate::panels::keyframe_clipboard;
mod faults;

fn rail(h: &Harness, index: u8, slot: usize) -> [f32; 2] {
    let watch = if h.host == Panel::Masks {
        h.panels.path_controls.mask_property_rects[&(h.host as u32, 7, 1, index, 0)].0
    } else {
        h.rect().0
    };
    let styles = PaintList::default().styles();
    [
        styles.metric("padding.panel", 8.) + (slot as f32 + 0.5) * styles.metric("icon.slot", 20.),
        center(watch)[1],
    ]
}
pub(super) fn click_rail(h: &mut Harness, index: u8, slot: usize) {
    let pt = rail(h, index, slot);
    h.input(pointer(pt, true, false));
    h.input(pointer(pt, false, true));
}
pub(super) fn key(h: &mut Harness, key: Key) {
    h.input(Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    });
}
pub(super) fn selected(h: &Harness, index: u8, frame: i32) {
    assert!(h.panels.selected_keys.owns_keys());
    let selected = h.panels.selected_keys.local_selections();
    assert_eq!(
        selected.len(),
        if h.host == Panel::Masks && index == mask_path::CHANNEL {
            3
        } else {
            1
        }
    );
    for (target, frames) in &selected {
        assert_eq!(frames, &vec![frame]);
        assert!(
            matches!(
                target,
                motion_core::KeyframeTarget::Mask {
                    object: 7,
                    mask: 1,
                    ..
                }
            ) && h.host == Panel::Masks
                || matches!(target,motion_core::KeyframeTarget::Vector {object:7,item:Some(1),parameter} if parameter=="position")
                    && h.host == Panel::Vector
        );
    }
    let clipboard = keyframe_clipboard::selection(&h.panels, &h.state).unwrap();
    assert!(clipboard.selections.is_empty());
    assert_eq!(clipboard.local_selections.len(), selected.len());
    assert!(clipboard
        .local_selections
        .iter()
        .all(|selection| selection.frames == vec![frame]));
}

#[test]
fn mapped_path_rails_and_keyboard_keep_signed_keys_in_both_hosts_languages_and_directions() {
    for host in [Panel::Masks, Panel::Vector] {
        for reverse in [false, true] {
            for locale in [Locale::Zh, Locale::En] {
                for floating in [false, true] {
                    for index in if host == Panel::Masks {
                        vec![0, 1, 2, mask_path::CHANNEL]
                    } else {
                        vec![0]
                    } {
                        let mut h = Harness::new(host, floating, locale, reverse);
                        let before = h.state["project"].clone();
                        click_rail(&mut h, index, 0);
                        assert_eq!(h.state["frame"], 2.);
                        selected(&h, index, if reverse { 0 } else { -6 });
                        key(&mut h, Key::K);
                        assert_eq!(h.state["frame"], 6.5);
                        selected(&h, index, -3);
                        key(&mut h, Key::K);
                        assert_eq!(h.state["frame"], 11.);
                        selected(&h, index, if reverse { -6 } else { 0 });
                        key(&mut h, Key::J);
                        assert_eq!(h.state["frame"], 6.5);
                        selected(&h, index, -3);
                        assert_eq!(h.state["project"], before);
                        assert_eq!(h.state["history"]["undoEntries"], 0);
                    }
                }
            }
        }
    }
}
pub(super) fn remove_middle(expected: &mut Value, host: Panel, index: u8) {
    if host == Panel::Vector {
        expected["layers"][0]["content"]["vector"]["source"]["group"]["transform"]["position"]
            ["keys"]
            .as_array_mut()
            .unwrap()
            .remove(1);
    } else if index == mask_path::CHANNEL {
        for node in expected["layers"][0]["masks"][0]["path"]["nodes"]
            .as_array_mut()
            .unwrap()
        {
            node["geometry"]["keys"].as_array_mut().unwrap().remove(1);
        }
    } else {
        let name = match index {
            0 => "opacity",
            1 => "feather",
            _ => "expansion",
        };
        expected["layers"][0]["masks"][0][name]["keys"]
            .as_array_mut()
            .unwrap()
            .remove(1);
    }
}
#[test]
fn mapped_path_key_toggles_and_delete_are_atomic_and_fractional_local_insert_is_disabled() {
    for host in [Panel::Masks, Panel::Vector] {
        for reverse in [false, true] {
            for floating in [false, true] {
                for index in if host == Panel::Masks {
                    vec![0, 1, 2, mask_path::CHANNEL]
                } else {
                    vec![0]
                } {
                    let mut h = Harness::new(host, floating, Locale::En, reverse);
                    let before = h.state["project"].clone();
                    let mut expected = before.clone();
                    remove_middle(&mut expected, host, index);
                    click_rail(&mut h, index, 1);
                    assert_eq!(h.state["project"], expected);
                    assert_eq!(h.state["history"]["undoEntries"], 1);
                    h.state = h.engine.history(0).unwrap();
                    assert_eq!(h.state["project"], before);
                    // Select an actual signed key, then use the common Delete route.
                    click_rail(&mut h, index, 0);
                    key(&mut h, Key::K);
                    selected(&h, index, -3);
                    key(&mut h, Key::Delete);
                    assert_eq!(h.state["project"], expected);
                    h.state = h.engine.history(0).unwrap();
                    assert_eq!(h.state["project"], before);
                    h.state = h.engine.seek(8.75).unwrap();
                    let history = h.state["history"].clone();
                    click_rail(&mut h, index, 1);
                    assert_eq!(h.state["project"], before);
                    assert_eq!(h.state["history"], history);
                    // An absent integral local pose can be restored in one edit.
                    h.state = h.engine.history(1).unwrap();
                    h.state = h.engine.seek(6.5).unwrap();
                    click_rail(&mut h, index, 1);
                    assert_eq!(h.state["project"], before);
                    selected(&h, index, -3);
                    h.engine.save().unwrap();
                    assert_eq!(
                        serde_json::to_value(
                            motion_core::storage::load(&h.folder.path().join("project")).unwrap()
                        )
                        .unwrap(),
                        before
                    );
                }
            }
        }
    }
}

#[test]
fn mapped_partial_mask_pose_removes_existing_nodes_without_inserting_missing_keys() {
    for reverse in [false, true] {
        let mut p = project(reverse);
        p.layers[0].masks[0].path.nodes[1].geometry.keys.remove(1);
        let mut h = Harness::with_project(Panel::Masks, false, Locale::En, p);
        let before = h.state["project"].clone();
        let mut expected = before.clone();
        click_rail(&mut h, mask_path::CHANNEL, 0);
        key(&mut h, Key::K);
        assert_eq!(h.state["frame"], 6.5);
        assert_eq!(h.panels.selected_keys.local_selections().len(), 3);
        assert_eq!(
            h.panels.selected_keys.status(&h.state),
            crate::panels::key_selection::Status::Stale
        );
        let clipboard = keyframe_clipboard::selection(&h.panels, &h.state).unwrap();
        assert!(!clipboard.enabled(
            &h.state,
            crate::layer_clipboard::controller::Intent::Copy,
            false
        ));
        for node in expected["layers"][0]["masks"][0]["path"]["nodes"]
            .as_array_mut()
            .unwrap()
        {
            node["geometry"]["keys"]
                .as_array_mut()
                .unwrap()
                .retain(|key| key["frame"] != -3);
        }
        click_rail(&mut h, mask_path::CHANNEL, 1);
        assert_eq!(h.state["project"], expected);
        assert_eq!(h.state["history"]["undoEntries"], 1);
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
    }
}

fn spatial_project(reverse: bool) -> Project {
    let mut p = project(reverse);
    let Content::Vector { vector } = &mut p.layers[0].content else {
        panic!()
    };
    let VectorSource::Group { group } = &mut vector.source else {
        panic!()
    };
    group.transform.position.keys[1].value = [25., 90.];
    group.transform.position.enable_roving(&[-3]).unwrap();
    p
}
#[test]
fn mapped_vector_group_spatial_storage_is_explicitly_rejected_before_authoring() {
    for reverse in [false, true] {
        let p = spatial_project(reverse);
        let before = serde_json::to_value(&p).unwrap();
        let error = p.validate().unwrap_err().to_string();
        assert!(
            error.contains("vector group parameters do not yet support spatial-key identities"),
            "{error}"
        );
        assert_eq!(serde_json::to_value(&p).unwrap(), before);
        assert!(
            matches!(motion_core::Engine::new(p),Err(error) if error.to_string().contains("vector group parameters do not yet support spatial-key identities"))
        );
    }
}

#[test]
fn mapped_path_toggle_respects_readonly_lock_and_focus_cancellation() {
    for host in [Panel::Masks, Panel::Vector] {
        for floating in [false, true] {
            let mut h = Harness::new(host, floating, Locale::En, false);
            let index = if host == Panel::Masks { 1 } else { 0 };
            let pt = rail(&h, index, 1);
            let before = h.state.clone();
            let mut input = pointer(pt, true, false);
            input.focus_lost = true;
            assert!(h.draw(input).is_empty());
            h.state["observing"] = true.into();
            assert!(h.draw(pointer(pt, true, false)).is_empty());
            h.state = before;
            h.state = h
                .engine
                .edit(
                    serde_json::to_string(&Command::Flags {
                        object: 7,
                        visible: true,
                        locked: true,
                    })
                    .unwrap(),
                )
                .unwrap();
            let locked = h.state.clone();
            assert!(h.draw(pointer(pt, true, false)).is_empty());
            assert_eq!(h.engine.state().unwrap()["project"], locked["project"]);
            assert_eq!(h.engine.state().unwrap()["history"], locked["history"]);
        }
    }
}
