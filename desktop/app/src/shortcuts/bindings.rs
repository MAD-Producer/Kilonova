//! Validated user overrides. Defaults stay in the existing command resolver.
use super::{Command, Scope};
use motion_ui::input::{Key, KeyChord, Modifiers};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::OnceLock};

pub(crate) const MAX_PRESET_BYTES: usize = 64 * 1024;
const MAX_OVERRIDES: usize = 128;
pub(crate) const KEYS: &[Key] = &[
    Key::B,
    Key::C,
    Key::E,
    Key::H,
    Key::L,
    Key::Q,
    Key::V,
    Key::W,
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
    Key::F1,
    Key::F2,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F10,
    Key::F11,
    Key::F12,
    Key::A,
    Key::F,
    Key::G,
    Key::T,
    Key::Num0,
    Key::N,
    Key::O,
    Key::I,
    Key::S,
    Key::Z,
    Key::Y,
    Key::D,
    Key::K,
    Key::P,
    Key::R,
    Key::U,
    Key::J,
    Key::F3,
    Key::F9,
    Key::M,
    Key::X,
    Key::Equal,
    Key::Minus,
    Key::Semicolon,
    Key::Backquote,
    Key::PageUp,
    Key::PageDown,
    Key::Escape,
    Key::Enter,
    Key::Tab,
    Key::Space,
    Key::Delete,
    Key::Backspace,
    Key::Left,
    Key::Right,
    Key::Up,
    Key::Down,
    Key::Home,
    Key::End,
    Key::Comma,
    Key::Period,
    Key::BracketLeft,
    Key::BracketRight,
    Key::F4,
];
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub scope: Scope,
    pub command: Command,
    pub defaults: Vec<KeyChord>,
}
pub(crate) fn catalogue() -> &'static [Entry] {
    static ITEMS: OnceLock<Vec<Entry>> = OnceLock::new();
    ITEMS.get_or_init(|| {
        let mut entries: Vec<Entry> = vec![];
        for scope in [Scope::Application, Scope::Animation, Scope::Nodes, Scope::Viewer] {
            for key in KEYS {
                for flags in 0..8 {
                    let modifiers = Modifiers {
                        control: flags & 1 != 0,
                        shift: flags & 2 != 0,
                        alt: flags & 4 != 0,
                    };
                    if let Some(command) = super::resolve(scope, *key, modifiers, false) {
                        if scope == Scope::Animation
                            && matches!(
                                command,
                                Command::Open
                                    | Command::Save
                                    | Command::Undo
                                    | Command::Redo
                                    | Command::Duplicate
                            )
                        {
                            continue;
                        }
                        let chord = KeyChord {
                            key: *key,
                            modifiers,
                        };
                        if let Some(entry) = entries
                            .iter_mut()
                            .find(|e| e.scope == scope && e.command == command)
                        {
                            entry.defaults.push(chord);
                        } else {
                            entries.push(Entry {
                                scope,
                                command,
                                defaults: vec![chord],
                            });
                        }
                    }
                }
            }
        }
        entries
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Override {
    scope: Scope,
    command: Command,
    chord: Option<KeyChord>,
}
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "RawBindings")]
pub(crate) struct Bindings {
    overrides: Vec<Override>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBindings {
    #[serde(default)]
    overrides: Vec<Override>,
}
impl TryFrom<RawBindings> for Bindings {
    type Error = String;
    fn try_from(raw: RawBindings) -> Result<Self, String> {
        let mut value = Self {
            overrides: raw.overrides,
        };
        // Version-1 settings and presets predate the new layer defaults. Preserve
        // explicit user mappings on load; leave a conflicting new command unbound.
        // Interactive bind/reset still use strict conflict checks, without this migration.
        for entry in catalogue().iter().filter(|entry| matches!(entry.command,
            Command::SelectLayerAbove | Command::SelectLayerBelow | Command::OrderLayersUp
                | Command::OrderLayersDown | Command::OrderLayersTop | Command::OrderLayersBottom)) {
            if value.overrides.iter().any(|item| item.scope == entry.scope && item.command == entry.command) {
                continue;
            }
            let occupied = entry.defaults.iter().any(|default| value.overrides.iter().any(|item| {
                item.chord.is_some_and(|chord| chord == *default
                    && catalogue().iter().find(|other| other.scope == item.scope && other.command == item.command)
                        .is_some_and(|other| domains(entry, *default) & domains(other, chord) != 0))
            }));
            if occupied {
                value.overrides.push(Override {scope: entry.scope, command: entry.command, chord: None});
            }
        }
        value.validate()?;
        Ok(value)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preset {
    version: u32,
    bindings: Bindings,
}

impl Bindings {
    pub(crate) fn hint(&self, scope: Scope, command: Command) -> String {
        catalogue()
            .iter()
            .find(|e| e.scope == scope && e.command == command)
            .map(|e| {
                self.chords(e)
                    .into_iter()
                    .map(format_chord)
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_default()
    }
    pub(crate) fn label(&self, label: &str, scope: Scope, command: Command) -> String {
        let hint = self.hint(scope, command);
        if hint.is_empty() {
            label.to_owned()
        } else {
            format!("{label}    {hint}")
        }
    }
    /// Compatibility boundary for property handlers whose local input vocabulary is
    /// J/K/Delete. Never translate the input sent to fields, widgets or the canvas.
    pub(crate) fn property_input(
        &self,
        input: &motion_ui::input::Input,
    ) -> Option<motion_ui::input::Input> {
        if self.overrides.is_empty() || input.keys.is_empty() {
            return None;
        }
        let mut translated = input.clone();
        translated.keys.clear();
        translated.key_repeats.clear();
        for (key, m, repeat) in input.key_presses() {
            let canonical = match self.resolve(Scope::Animation, key, m, repeat) {
                Some(Command::PreviousKey) => Key::J,
                Some(Command::NextKey) => Key::K,
                Some(Command::Delete) => Key::Delete,
                _ => continue,
            };
            translated.keys.push((canonical, Modifiers::default()));
            translated.key_repeats.push(repeat);
        }
        Some(translated)
    }
    pub(crate) fn resolve(
        &self,
        scope: Scope,
        key: Key,
        modifiers: Modifiers,
        repeat: bool,
    ) -> Option<Command> {
        if let Some(item) = self.overrides.iter().find(|item| {
            item.scope == scope && item.chord.is_some_and(|c| c.matches(key, modifiers))
        }) {
            return (!repeat || repeatable(item.command)).then_some(item.command);
        }
        // Viewer tools were added after animation overrides. A new default must
        // not steal an explicit user chord in the shared composition context.
        if scope == Scope::Viewer && self.overrides.iter().any(|item|
            item.scope == Scope::Animation && item.chord.is_some_and(|c|c.matches(key,modifiers))) {
            return None;
        }
        let command = super::resolve(scope, key, modifiers, repeat)?;
        (!self
            .overrides
            .iter()
            .any(|item| item.scope == scope && item.command == command))
        .then_some(command)
    }
    pub(crate) fn chords(&self, entry: &Entry) -> Vec<KeyChord> {
        match self
            .overrides
            .iter()
            .find(|item| item.scope == entry.scope && item.command == entry.command)
        {
            Some(item) => item.chord.into_iter().collect(),
            None if entry.scope == Scope::Viewer => entry.defaults.iter().copied().filter(|chord|
                !self.overrides.iter().any(|item|item.scope==Scope::Animation && item.chord==Some(*chord))).collect(),
            None => entry.defaults.clone(),
        }
    }
    pub(crate) fn bind(
        &mut self,
        scope: Scope,
        command: Command,
        chord: Option<KeyChord>,
    ) -> Result<(), String> {
        let mut candidate = self.clone();
        candidate
            .overrides
            .retain(|item| item.scope != scope || item.command != command);
        candidate.overrides.push(Override {
            scope,
            command,
            chord,
        });
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub(crate) fn reset_command(&mut self, scope: Scope, command: Command) -> Result<(), String> {
        let mut candidate = self.clone();
        candidate
            .overrides
            .retain(|item| item.scope != scope || item.command != command);
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.overrides.len() > MAX_OVERRIDES {
            return Err("shortcut preset exceeds 128 overrides".into());
        }
        for (index, item) in self.overrides.iter().enumerate() {
            if !catalogue()
                .iter()
                .any(|entry| entry.scope == item.scope && entry.command == item.command)
            {
                return Err(format!(
                    "shortcut command is unavailable: {:?}/{:?}",
                    item.scope, item.command
                ));
            }
            if self.overrides[..index]
                .iter()
                .any(|e| e.scope == item.scope && e.command == item.command)
            {
                return Err("duplicate shortcut command override".into());
            }
            if let Some(chord) = item.chord {
                let default = catalogue()
                    .iter()
                    .find(|entry| entry.scope == item.scope && entry.command == item.command)
                    .is_some_and(|entry| entry.defaults.contains(&chord));
                if !default {
                    validate_chord(chord, item.command)?;
                }
            }
        }
        // Compare all effective defaults and overrides, never silently steal a key.
        for (index, a) in catalogue().iter().enumerate() {
            for b in &catalogue()[index + 1..] {
                if a.command == b.command {
                    continue;
                }
                for chord in self.chords(a) {
                    let explicit_shared_view = matches!((a.scope,b.scope),(Scope::Animation,Scope::Viewer)|(Scope::Viewer,Scope::Animation))
                        && [a,b].into_iter().all(|entry|self.overrides.iter().any(|item|item.scope==entry.scope && item.command==entry.command && item.chord==Some(chord)));
                    if self.chords(b).contains(&chord) && (domains(a, chord) & domains(b, chord) != 0 || explicit_shared_view)
                    {
                        return Err(format!(
                            "shortcut conflict: {} ({:?}/{:?}) and {:?}/{:?}",
                            format_chord(chord),
                            a.scope,
                            a.command,
                            b.scope,
                            b.command
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn import(path: &Path) -> Result<Self, String> {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|e| format!("read shortcut preset: {e}"))?;
        let mut bytes = vec![];
        file.take((MAX_PRESET_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > MAX_PRESET_BYTES {
            return Err("shortcut preset exceeds 64 KiB".into());
        }
        let preset: Preset =
            serde_json::from_slice(&bytes).map_err(|e| format!("invalid shortcut preset: {e}"))?;
        if preset.version != 1 {
            return Err(format!(
                "unsupported shortcut preset version {}",
                preset.version
            ));
        }
        Ok(preset.bindings)
    }
    pub(crate) fn export_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        serde_json::to_vec_pretty(&Preset {
            version: 1,
            bindings: self.clone(),
        })
        .map_err(|e| e.to_string())
    }
}
fn repeatable(command: Command) -> bool {
    matches!(
        command,
        Command::PreviousFrame
            | Command::NextFrame
            | Command::MoveLeft
            | Command::MoveRight
            | Command::MoveUp
            | Command::MoveDown
            | Command::PreviousKey
            | Command::NextKey
            | Command::SelectLayerAbove
            | Command::SelectLayerBelow
    )
}
fn domains(entry: &Entry, chord: KeyChord) -> u8 {
    match entry.scope {
        Scope::Animation => 1,
        Scope::Nodes => 2,
        Scope::Viewer => 4,
        Scope::Application => {
            if matches!(
                entry.command,
                Command::ReloadStyle
                    | Command::TogglePanelMaximize
                    | Command::NewProject
                    | Command::NewComposition
                    | Command::Import
                    | Command::Settings
                    | Command::Preferences
                    | Command::Export
                    | Command::Solid
                    | Command::Text
                    | Command::Null
                    | Command::Adjustment
                    | Command::LastFrame
            ) || matches!(entry.command, Command::PreviousFrame | Command::NextFrame)
                && (!matches!(chord.key, Key::Left | Key::Right) || chord.modifiers.control)
            {
                7
            } else {
                5
            }
        }
    }
}
fn validate_chord(chord: KeyChord, command: Command) -> Result<(), String> {
    use Key::*;
    let m = chord.modifiers;
    let reserved = matches!(chord.key, Shift | Control | Alt | Escape | Enter)
        || chord.key == Tab && m.alt
        || chord.key == F4 && m.alt
        || chord.key == Delete && m.control && m.alt
        || chord.key == Space && (m.control || m.alt)
        || chord.key == A
            && m.control
            && !matches!(command, Command::SelectAll | Command::DeselectAll)
        || matches!(chord.key, C | V | X) && m.control
        || chord.key == Z
            && m.control
            && (!m.shift && command != Command::Undo || m.shift && command != Command::Redo)
        || chord.key == Y && m.control && command != Command::Redo;
    if reserved {
        Err(format!(
            "reserved system or text shortcut: {}",
            format_chord(chord)
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn format_chord(chord: KeyChord) -> String {
    let mut text = String::new();
    if chord.modifiers.control {
        text.push_str("Ctrl+");
    }
    if chord.modifiers.alt {
        text.push_str("Alt+");
    }
    if chord.modifiers.shift {
        text.push_str("Shift+");
    }
    let key = match chord.key {
        Key::Num0 => "0".into(),
        Key::Num1 => "1".into(),
        Key::Num2 => "2".into(),
        Key::Num3 => "3".into(),
        Key::Num4 => "4".into(),
        Key::Num5 => "5".into(),
        Key::Num6 => "6".into(),
        Key::Num7 => "7".into(),
        Key::Num8 => "8".into(),
        Key::Num9 => "9".into(),
        Key::Comma => ",".into(),
        Key::Period => ".".into(),
        Key::Equal => "=".into(),
        Key::Minus => "-".into(),
        Key::Semicolon => ";".into(),
        Key::Backquote => "`".into(),
        Key::BracketLeft => "[".into(),
        Key::BracketRight => "]".into(),
        other => format!("{other:?}"),
    };
    text.push_str(&key);
    text
}

#[cfg(test)]
mod tests;
