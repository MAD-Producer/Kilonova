use super::*;

fn snapshot(workspace: &Workspace) -> serde_json::Value {
    serde_json::to_value(workspace).unwrap()
}

#[test]
fn projection_preserves_ratios_tabs_ownership_and_serialized_layout() {
    let mut workspace = Workspace::default();
    workspace.select_preset(Preset::All);
    let before = snapshot(&workspace);
    let target = dock::Panel::Timeline.dock();
    assert!(workspace.toggle_panel_maximize(target));
    assert_eq!(workspace.maximized_panel(), Some(target));
    assert_eq!(workspace.display_root().docks().len(), 1);
    assert_eq!(snapshot(&workspace), before);
    assert!(workspace.valid());
    let dock::Node::Dock { id, tabs, .. } = workspace.display_root() else {
        panic!()
    };
    let id = *id;
    assert!(tabs.len() > 1);
    assert!(workspace.root.activate(id, dock::Panel::Audio.dock()));
    // Tab activation while projected is a real workspace action and survives restore.
    let activated = snapshot(&workspace);
    assert!(workspace.toggle_panel_maximize(target));
    assert_eq!(workspace.maximized_panel(), None);
    assert_eq!(snapshot(&workspace), activated);
    assert_eq!(
        workspace.display_root().docks().len(),
        workspace.root.docks().len()
    );
}

#[test]
fn projection_is_transient_across_save_reload_and_preset_switches() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("layout.json");
    let mut workspace = Workspace::default();
    let before = snapshot(&workspace);
    assert!(workspace.toggle_panel_maximize(dock::Panel::Composition.dock()));
    workspace.save(&path).unwrap();
    let reopened = Workspace::load(&path);
    assert_eq!(reopened.maximized_panel(), None);
    assert_eq!(snapshot(&reopened), before);
    workspace.save_named("full layout".into());
    assert!(workspace.named_layouts["full layout"].valid());
    workspace.select_preset(Preset::Nodes);
    assert_eq!(workspace.maximized_panel(), None);
    workspace.select_preset(Preset::Default);
    assert_eq!(
        serde_json::to_value(&workspace.root).unwrap(),
        before["root"]
    );
    assert!(workspace.toggle_panel_maximize(dock::Panel::Composition.dock()));
    workspace.reset_preset();
    assert_eq!(workspace.maximized_panel(), None);
}

#[test]
fn invalid_floating_hidden_or_removed_targets_cannot_hide_the_layout() {
    let mut workspace = Workspace::default();
    assert!(!workspace.toggle_panel_maximize(DockId(u32::MAX)));
    let target = dock::Panel::Composition.dock();
    assert!(workspace.float(target));
    assert!(!workspace.toggle_panel_maximize(target));
    workspace.redock(target);
    assert!(workspace.toggle_panel_maximize(target));
    assert!(workspace.close(target));
    assert_eq!(workspace.maximized_panel(), None);
    assert_eq!(workspace.display_root().docks(), workspace.root.docks());
    workspace.restore_panel();
    assert!(!workspace.toggle_panel_maximize(target));
}

#[test]
fn expanded_nodes_preview_fits_both_native_acceptance_sizes_and_restores_layout() {
    for size in [[900., 600.], [1600., 1000.]] {
        let mut workspace = Workspace::default();
        workspace.select_preset(Preset::Nodes);
        let before = snapshot(&workspace);
        let normal = crate::panels::node_view_bound(workspace.display_root(), size, false);
        let frames = crate::panels::frames(workspace.display_root(), size);
        let node = frames.iter().find(|frame| frame.panel == dock::Panel::Nodes).unwrap();
        assert!(workspace.toggle_panel_maximize(dock::Panel::Nodes.dock()));
        let expanded = crate::panels::node_view_bound(workspace.display_root(), size, false)
            .expect("expanded Nodes view must fit the existing preview controls");
        assert!(Rect::new(0., 0., size[0], size[1]).contains_rect(expanded));
        assert!(expanded.width() > 1. && expanded.height() > 1.);
        println!("{}", serde_json::json!({"size":size,"normalPreview":normal.map(|r|[r.min,r.max]),
            "nodeContent":[node.content.min,node.content.max],"expandedPreview":[expanded.min,expanded.max]}));
        assert_eq!(snapshot(&workspace), before);
        assert!(workspace.restore_panel());
        assert_eq!(snapshot(&workspace), before);
        assert_eq!(crate::panels::node_view_bound(workspace.display_root(), size, false), normal);
    }
}
