//! Status identifies the emitted operation's target, not backend success.
use super::{Action, Edit, Panels};
use crate::i18n::{Locale, Text};

fn deletes_layer(mut command: &Edit) -> bool {
    while let Edit::InComposition { command: inner, .. } = command {
        command = inner;
    }
    matches!(command, Edit::Delete { .. })
}

pub(super) fn publish(panels: &mut Panels, actions: &[Action]) {
    if actions.is_empty() {
        return;
    }
    if actions.iter().any(|action| match action {
        Action::Edit(command) => deletes_layer(command),
        Action::EditBatch(commands) => commands.iter().any(deletes_layer),
        _ => false,
    }) {
        panels.message = Some(panels.locale.t(Text::LayerDeleteTarget).into());
    } else if panels.message.as_deref().is_some_and(|message| {
        [Locale::En, Locale::Zh]
            .iter()
            .any(|locale| message == locale.t(Text::LayerDeleteTarget))
    }) {
        panels.message = None;
    }
}
