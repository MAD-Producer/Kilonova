//! A panel supplies UI state and typed actions, with no dependency on editor or rendering sessions.
use crate::{
    dock::DockId,
    interaction::{HitRegion, Router, Target},
    registry::{Language, PanelDescriptor, PanelRegistry},
    style::StyleSheet,
    Input, PaintList, Rect, TextAtlas,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub struct PanelLayout {
    pub window: u64,
    pub panel: DockId,
    pub bounds: Rect,
    pub clip: Rect,
}
impl PanelLayout {
    pub fn target(self, widget: u64) -> Target {
        Target {
            window: self.window,
            panel: self.panel,
            widget,
        }
    }
}
pub struct PanelFrame<'a> {
    pub styles: &'a StyleSheet,
    pub layout: PanelLayout,
    pub language: Language,
    pub paint: &'a mut PaintList,
    pub atlas: &'a mut TextAtlas,
    router: &'a Router,
    input: &'a Input,
}
impl PanelFrame<'_> {
    pub fn is_focused(&self, widget: u64) -> bool {
        self.router.focus() == Some(self.layout.target(widget))
    }
    pub fn input_for(&self, widget: u64) -> Input {
        self.router
            .input_for(self.layout.target(widget), self.input)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Lifecycle {
    Attached { window: u64 },
    Detached { window: u64 },
    Hidden,
    FocusLost,
    Closed,
}
pub trait PanelView {
    type Action;
    fn hit_regions(&self, layout: PanelLayout) -> Vec<HitRegion> {
        vec![HitRegion {
            target: layout.target(0),
            rect: layout.bounds,
            clip: layout.clip,
            layer: 0,
            keyboard: true,
        }]
    }
    fn draw(&mut self, frame: &mut PanelFrame<'_>) -> Vec<Self::Action>;
    fn lifecycle(&mut self, _event: Lifecycle) -> Vec<Self::Action> {
        vec![]
    }
}
pub struct PanelHost<A> {
    styles: std::sync::Arc<StyleSheet>,
    registry: PanelRegistry,
    views: BTreeMap<DockId, Box<dyn PanelView<Action = A>>>,
}
impl<A> Default for PanelHost<A> {
    fn default() -> Self {
        Self {
            styles: crate::style::default_stylesheet_arc(),
            registry: Default::default(),
            views: Default::default(),
        }
    }
}
impl<A> PanelHost<A> {
    pub fn styles(&self) -> &StyleSheet {
        &self.styles
    }
    pub fn set_stylesheet(&mut self, styles: StyleSheet) {
        self.styles = std::sync::Arc::new(styles);
    }
    pub fn register(
        &mut self,
        descriptor: PanelDescriptor,
        view: impl PanelView<Action = A> + 'static,
    ) -> Result<(), String> {
        let id = descriptor.id;
        self.registry.register(descriptor)?;
        self.views.insert(id, Box::new(view));
        Ok(())
    }
    pub fn registry(&self) -> &PanelRegistry {
        &self.registry
    }
    pub fn hit_regions(&self, layouts: &[PanelLayout]) -> Vec<HitRegion> {
        layouts
            .iter()
            .flat_map(|layout| {
                self.views
                    .get(&layout.panel)
                    .map(|v| v.hit_regions(*layout))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|r| r.target.window == layout.window && r.target.panel == layout.panel)
                    .map(|mut r| {
                        r.clip = r.clip.intersection(layout.bounds.intersection(layout.clip));
                        r
                    })
            })
            .collect()
    }
    pub fn draw(
        &mut self,
        layout: PanelLayout,
        language: Language,
        input: &Input,
        router: &Router,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
    ) -> Vec<A> {
        let Some(view) = self.views.get_mut(&layout.panel) else {
            return vec![];
        };
        let mut content = PaintList::with_styles(self.styles.clone());
        let actions = view.draw(&mut PanelFrame {
            styles: &self.styles,
            layout,
            language,
            paint: &mut content,
            atlas,
            router,
            input,
        });
        content.clip_to(layout.bounds.intersection(layout.clip));
        list.append(content);
        actions
    }
    pub fn lifecycle(&mut self, panel: DockId, event: Lifecycle) -> Vec<A> {
        self.views
            .get_mut(&panel)
            .map_or_else(Vec::new, |v| v.lifecycle(event))
    }
}
