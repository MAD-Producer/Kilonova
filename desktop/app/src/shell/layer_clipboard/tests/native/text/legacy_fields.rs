//! Real main-window field hits and modal input; no OS keyboard injection.
use super::*;
use motion_core::{
    masks::{LayerMask, MaskAction},
    vector::{PathNode, VectorPath},
    Command, Track,
};
use std::path::Path;

fn commit(events: &mut EventLoop<()>, shell: &mut Shell, actions: Vec<Action>) {
    assert_eq!(actions.len(), 1, "{actions:?}");
    shell
        .action(actions.into_iter().next().unwrap(), None)
        .unwrap();
    let (_, reply) = shell.editing.take().unwrap();
    shell.state = native_test_support::receive(events, &reply, Duration::from_secs(30))
        .unwrap()
        .unwrap();
}
fn release(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        released: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn saved(shell: &mut Shell) {
    shell.engine.save().unwrap();
    let root = shell.state["root"].as_str().unwrap();
    assert_eq!(
        serde_json::to_value(motion_core::storage::load(Path::new(root)).unwrap()).unwrap(),
        shell.state["project"]
    );
}
pub(super) fn run(
    events: &mut EventLoop<()>,
    shell: &mut Shell,
    size: [f32; 2],
    scale: f32,
    name: &str,
    root: &Path,
) {
    shell.panels.select_only(Some(7));
    let before = shell.engine.state().unwrap()["project"].clone();
    let area = panels::frames(shell.workspace.display_root(), size)
        .into_iter()
        .find(|frame| frame.panel == Panel::Properties)
        .unwrap()
        .content;
    let styles = shell
        .panels
        .styles
        .clone()
        .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
    let pad = styles.metric("padding.panel", 8.);
    let row = styles.metric("row.height", 22.);
    let slot = styles.metric("icon.slot", 20.);
    let caption = area.min[0] + pad + slot * 3. + styles.metric("tree.indent", 14.) + slot;
    let value = (caption + styles.metric("label.column", 72.) + pad)
        .min(area.max[0] - pad - 2. * styles.metric("value.min_width", 64.))
        .max(caption + styles.metric("label.column", 72.) * 0.5 + pad);
    let cell = ((area.max[0] - pad - value) / 2.).min(styles.metric("value.min_width", 64.));
    let position = [value + cell * 0.5, area.min[1] + pad + row * 4.5];
    assert_quiet(shell, size, 0, click(position));
    assert_quiet(shell, size, 0, release(position));
    let field = shell
        .panels
        .properties
        .field
        .as_ref()
        .expect("real position field click");
    assert_eq!(
        (field.object, field.property, field.axis),
        (7, motion_core::Property::Position, 0)
    );
    let original = field.text.clone();
    shell.text_clipboard.store("64").unwrap();
    assert_quiet(shell, size, 0, chord(Key::V, false));
    assert_eq!(shell.panels.properties.field.as_ref().unwrap().text, "64");
    assert_quiet(shell, size, 0, chord(Key::Z, false));
    assert_eq!(
        shell.panels.properties.field.as_ref().unwrap().text,
        original
    );
    assert_quiet(shell, size, 0, chord(Key::Y, false));
    capture(
        events,
        shell,
        size,
        scale,
        0,
        root.join(format!("{name}-property-number.png")),
    );
    let (_, actions) = draw(shell, size, 0, enter());
    assert!(
        matches!(actions.as_slice(),[Action::Edit(Command::SetVector {
            object:7,property:motion_core::Property::Position,frame:0,value
        })] if *value == [64.,0.,0.]),
        "{actions:?}"
    );
    commit(events, shell, actions);
    assert_eq!(
        shell.state["project"]["layers"][0]["transform"]["position"]["value"][0],
        64.
    );
    saved(shell);
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(shell.state["project"], before);

    // Add a temporary authored mask through the real engine and edit its opacity.
    let mask = LayerMask::new(
        1,
        VectorPath {
            id: 1,
            closed: true,
            nodes: [[0., 0.], [32., 0.], [0., 32.]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    );
    shell.state = shell
        .engine
        .edit(
            serde_json::to_string(&Command::Mask {
                object: 7,
                action: MaskAction::Add { mask },
            })
            .unwrap(),
        )
        .unwrap();
    let with_mask = shell.state["project"].clone();
    shell.panels.path_controls = Default::default();
    let heading = [area.min[0] + pad + slot * 2., area.min[1] + pad + row * 1.5];
    assert_quiet(shell, size, 0, click(heading));
    let mask_value = (area.min[0]
        + (pad + slot * 3. + styles.metric("label.column", 72.) + slot * 2.)
            .min(area.width() * 0.7))
    .max(area.min[0] + pad + slot * 3. + pad * 2. + slot + pad);
    let mask_cell = (area.max[0] - pad - mask_value).min(styles.metric("value.min_width", 64.));
    let opacity = [mask_value + mask_cell * 0.5, area.min[1] + pad + row * 5.5];
    let (_, actions) = draw(shell, size, 0, click(opacity));
    assert!(
        actions
            .iter()
            .all(|action| matches!(action,Action::Seek(frame) if *frame == 0.)),
        "{actions:?}"
    );
    shell.state = shell.engine.seek(0.).unwrap();
    assert_quiet(shell, size, 0, release(opacity));
    shell.text_clipboard.store("80").unwrap();
    assert_quiet(shell, size, 0, chord(Key::V, false));
    assert_quiet(shell, size, 0, chord(Key::Z, false));
    assert_quiet(shell, size, 0, chord(Key::Y, false));
    capture(
        events,
        shell,
        size,
        scale,
        0,
        root.join(format!("{name}-mask-number.png")),
    );
    let (_, actions) = draw(shell, size, 0, enter());
    assert!(
        matches!(actions.as_slice(),[Action::Edit(Command::Mask {
            object:7,action:MaskAction::Set {
                mask:1,property:motion_core::masks::MaskProperty::Opacity,frame:0,value,animated:None
            }
        })] if value == &[80.]),
        "{actions:?}"
    );
    commit(events, shell, actions);
    saved(shell);
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(shell.state["project"], with_mask);
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(shell.state["project"], before);
    shell.panels.path_controls = Default::default();

    shell.text_clipboard.store("剪贴板文字 Text").unwrap();
    shell.dialog = Some(crate::dialogs::Dialog::text(vec![]));
    assert_quiet(shell, size, 0, chord(Key::V, false));
    assert_quiet(shell, size, 0, chord(Key::Z, false));
    assert_quiet(shell, size, 0, chord(Key::Y, false));
    capture(
        events,
        shell,
        size,
        scale,
        0,
        root.join(format!("{name}-text-dialog.png")),
    );
    assert_quiet(
        shell,
        size,
        0,
        Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        },
    );
    assert!(shell.dialog.is_none());
    assert_eq!(shell.engine.state().unwrap()["project"], before);

    shell.dialog = Some(crate::dialogs::Dialog::settings(&shell.state));
    assert_quiet(shell, size, 0, chord(Key::V, false));
    assert_quiet(shell, size, 0, chord(Key::Z, false));
    assert_quiet(shell, size, 0, chord(Key::Y, false));
    capture(
        events,
        shell,
        size,
        scale,
        0,
        root.join(format!("{name}-composition-dialog.png")),
    );
    assert_quiet(shell, size, 0, enter());
    assert!(shell.dialog.is_none());
    assert_eq!(shell.state["project"]["name"], "剪贴板文字 Text");
    saved(shell);
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(shell.state["project"], before);
}
