//! Application-owned panel descriptions. Registering a panel never opens a project.
use crate::dock::DockId;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    Chinese,
    #[default]
    English,
}
#[derive(Clone, Debug)]
pub struct LocalizedText {
    pub zh: String,
    pub en: String,
}
impl LocalizedText {
    pub fn new(zh: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            zh: zh.into(),
            en: en.into(),
        }
    }
    pub fn get(&self, language: Language) -> &str {
        match language {
            Language::Chinese => &self.zh,
            Language::English => &self.en,
        }
    }
}
#[derive(Clone, Debug)]
pub struct PanelDescriptor {
    pub id: DockId,
    pub title: LocalizedText,
    pub minimum_size: [f32; 2],
    pub floating_size: [f32; 2],
    pub closable: bool,
    pub floatable: bool,
}
impl PanelDescriptor {
    pub fn new(id: DockId, zh: &str, en: &str) -> Self {
        Self {
            id,
            title: LocalizedText::new(zh, en),
            minimum_size: [160., 90.],
            floating_size: [640., 480.],
            closable: true,
            floatable: true,
        }
    }
}
#[derive(Default)]
pub struct PanelRegistry {
    panels: BTreeMap<DockId, PanelDescriptor>,
}
impl PanelRegistry {
    pub fn register(&mut self, panel: PanelDescriptor) -> Result<(), String> {
        if panel.id.0 == 0 || self.panels.contains_key(&panel.id) {
            return Err("panel ID must be unique and nonzero".into());
        }
        if panel.title.zh.is_empty() || panel.title.en.is_empty() {
            return Err("panel needs both language titles".into());
        }
        if panel
            .minimum_size
            .into_iter()
            .chain(panel.floating_size)
            .any(|v| !v.is_finite() || v <= 0.)
            || (0..2).any(|i| panel.floating_size[i] < panel.minimum_size[i])
        {
            return Err("invalid panel size".into());
        }
        self.panels.insert(panel.id, panel);
        Ok(())
    }
    pub fn get(&self, id: DockId) -> Option<&PanelDescriptor> {
        self.panels.get(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = &PanelDescriptor> {
        self.panels.values()
    }
}
