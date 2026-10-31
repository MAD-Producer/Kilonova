//! One bounded projection group and navigation settings for the Composition panel.
//! A projection reply never replaces the shared project state or its selection.
use super::*;
use crate::{
    layer_clipboard::KeyframeSource,
    viewer_views::{Layout, Mode, Orbit, Settings},
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Key {
    pub source: KeyframeSource,
    pub frame: u64,
    pub window: u64,
    surface: u64,
    cameras: Vec<(Mode, Option<Orbit>)>,
}
impl Key {
    pub(crate) fn same_context(&self, other: &Self) -> bool {
        self.source == other.source
            && self.frame == other.frame
            && self.window == other.window
            && self.surface == other.surface
            && self.cameras.len() == other.cameras.len()
            && self
                .cameras
                .iter()
                .zip(&other.cameras)
                .all(|(a, b)| a.0 == b.0)
    }
}
#[derive(Default)]
pub(crate) struct Controls {
    pub settings: Settings,
    pub window: u64,
    pub cache: Option<(Key, Vec<Value>)>,
    pub failed: Option<Key>,
    pub loaded: Option<(usize, u64)>,
    pub history: [crate::viewer_views::CameraHistory; 4],
    pub navigation_capture: Option<(Key, Rect, crate::viewer_navigation::Navigation)>,
    layout: motion_ui::widgets::Dropdown,
    mode: motion_ui::widgets::Dropdown,
    quality: motion_ui::widgets::Dropdown,
    anchors: Option<(Rect, Rect, Rect, Rect)>,
}
impl Controls {
    pub(crate) fn key(&self, state: &Value) -> Key {
        Key {
            source: KeyframeSource::of(state),
            frame: state["frame"].as_f64().unwrap_or(0.).to_bits(),
            window: self.window,
            surface: state["surfaceEpoch"].as_u64().unwrap_or(0),
            cameras: (0..self.settings.layout().count())
                .filter_map(|i| self.settings.view(i))
                .map(|view| (view.mode, view.orbit.clone()))
                .collect(),
        }
    }
    pub(crate) fn popup(&self) -> bool {
        self.layout.open || self.mode.open || self.quality.open
    }
    pub(super) fn begin_frame(&mut self) {
        self.anchors = None;
    }
    pub(crate) fn prepare(&mut self, window: u64) {
        if self.window != window {
            self.layout.open = false;
            self.mode.open = false;
            self.quality.open = false;
            self.anchors = None;
        }
        self.window = window;
    }
    pub(super) fn fields(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        layout: Rect,
        mode: Rect,
        quality: Rect,
        state: &Value,
        locale: Locale,
        _input: &Input,
    ) {
        self.anchors = Some((bounds, layout, mode, quality));
        // Popups paint last, above all viewport handles. Keep their anchor controls
        // passive here so one input can never hit both a viewport and a menu row.
        let neutral = Input::default();
        let labels = if locale == Locale::Zh {
            ["1 个视图", "2 个视图", "4 个视图"]
        } else {
            ["1 view", "2 views", "4 views"]
        };
        let items: Vec<_> = labels
            .iter()
            .map(|label| motion_ui::widgets::DropdownItem {
                label,
                enabled: true,
            })
            .collect();
        let selected = match self.settings.layout() {
            Layout::Single => 0,
            Layout::Two => 1,
            Layout::Four => 2,
        };
        let mut passive = motion_ui::widgets::Dropdown::default();
        passive.draw(
            list, atlas, layout, bounds, &items, selected, &neutral, true, false,
        );
        let modes = modes(state, locale);
        let items: Vec<_> = modes
            .iter()
            .map(|(_, label)| motion_ui::widgets::DropdownItem {
                label,
                enabled: true,
            })
            .collect();
        let selected = modes
            .iter()
            .position(|(mode, _)| {
                Some(*mode) == self.settings.view(self.settings.active()).map(|v| v.mode)
            })
            .unwrap_or(0);
        passive.draw(
            list, atlas, mode, bounds, &items, selected, &neutral, true, false,
        );
        let labels = [
            locale.t(T::Auto),
            locale.t(T::Full),
            locale.t(T::Smooth),
            locale.t(T::Economy),
        ];
        let items: Vec<_> = labels
            .iter()
            .map(|label| motion_ui::widgets::DropdownItem {
                label,
                enabled: true,
            })
            .collect();
        let selected = quality_index(state);
        passive.draw(
            list, atlas, quality, bounds, &items, selected, &neutral, true, false,
        );
    }
    pub(super) fn overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        state: &Value,
        locale: Locale,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        let Some((bounds, layout, mode, quality)) = self.anchors else {
            self.layout.open = false;
            self.mode.open = false;
            self.quality.open = false;
            return;
        };
        let neutral = Input::default();
        let owner = if self.layout.open {
            Some(0)
        } else if self.mode.open {
            Some(1)
        } else if self.quality.open {
            Some(2)
        } else {
            None
        };
        let mut next = self.settings.clone();
        let labels = if locale == Locale::Zh {
            ["1 个视图", "2 个视图", "4 个视图"]
        } else {
            ["1 view", "2 views", "4 views"]
        };
        let items: Vec<_> = labels
            .iter()
            .map(|label| motion_ui::widgets::DropdownItem {
                label,
                enabled: true,
            })
            .collect();
        let selected = match self.settings.layout() {
            Layout::Single => 0,
            Layout::Two => 1,
            Layout::Four => 2,
        };
        if let Some(i) = self.layout.draw(
            list,
            atlas,
            layout,
            bounds,
            &items,
            selected,
            if owner.is_none() || owner == Some(0) {
                input
            } else {
                &neutral
            },
            true,
            false,
        ) {
            next.set_layout([Layout::Single, Layout::Two, Layout::Four][i]);
        }
        let modes = modes(state, locale);
        let items: Vec<_> = modes
            .iter()
            .map(|(_, label)| motion_ui::widgets::DropdownItem {
                label,
                enabled: true,
            })
            .collect();
        let selected = modes
            .iter()
            .position(|(mode, _)| {
                Some(*mode) == self.settings.view(self.settings.active()).map(|v| v.mode)
            })
            .unwrap_or(0);
        if let Some(i) = self.mode.draw(
            list,
            atlas,
            mode,
            bounds,
            &items,
            selected,
            if owner.is_none() || owner == Some(1) {
                input
            } else {
                &neutral
            },
            true,
            false,
        ) {
            let active = next.active();
            if let Some(view) = next.view_mut(active) {
                view.mode = modes[i].0;
                view.orbit = None;
            }
        }
        let labels = [
            locale.t(T::Auto),
            locale.t(T::Full),
            locale.t(T::Smooth),
            locale.t(T::Economy),
        ];
        let items: Vec<_> = labels
            .iter()
            .map(|label| motion_ui::widgets::DropdownItem {
                label,
                enabled: true,
            })
            .collect();
        if let Some(i) = self.quality.draw(
            list,
            atlas,
            quality,
            bounds,
            &items,
            quality_index(state),
            if owner.is_none() || owner == Some(2) {
                input
            } else {
                &neutral
            },
            true,
            false,
        ) {
            if i != quality_index(state) {
                out.push(Action::PreviewMode(i as i32));
            }
        }
        if self.settings != next {
            out.push(Action::ViewerSettings(next));
        }
    }
}
fn quality_index(state: &Value) -> usize {
    match state["preview"]["mode"].as_str() {
        Some("high") => 1,
        Some("balanced") => 2,
        Some("economy") => 3,
        _ => 0,
    }
}
pub(crate) fn modes(state: &Value, locale: Locale) -> Vec<(Mode, String)> {
    let mut modes = vec![
        (Mode::Composition, locale.t(T::ViewComposition).into()),
        (Mode::Free, locale.t(T::ViewFree).into()),
        (Mode::Top, locale.t(T::ViewTop).into()),
        (Mode::Side, locale.t(T::ViewSide).into()),
    ];
    if state["project"]["camera"]["created"] == true {
        modes.push((
            Mode::Camera(0),
            if locale == Locale::Zh {
                "工程摄影机"
            } else {
                "Project camera"
            }
            .into(),
        ));
    }
    if let Some(layers) = state["project"]["layers"].as_array() {
        for layer in layers {
            if !layer["content"]["camera"].is_null() {
                if let Some(id) = layer["id"].as_u64() {
                    modes.push((
                        Mode::Camera(id),
                        layer["name"].as_str().unwrap_or("Camera").into(),
                    ));
                }
            }
        }
    }
    modes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_dropdown_starts_from_the_actual_layout_and_only_commits_that_setting() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for locale in [Locale::En, Locale::Zh] {
            let mut controls = Controls::default();
            controls.settings.set_layout(Layout::Four);
            let before = controls.settings.clone();
            let mut paint = PaintList::default();
            let mut out = vec![];
            let anchor = Rect::new(300., 270., 88., 22.);
            controls.fields(
                &mut paint,
                &mut atlas,
                Rect::new(0., 0., 400., 300.),
                anchor,
                Rect::new(100., 270., 98., 22.),
                Rect::new(0., 270., 68., 22.),
                &Value::Null,
                locale,
                &Input::default(),
            );
            controls.overlay(
                &mut paint,
                &mut atlas,
                &Value::Null,
                locale,
                &Input {
                    mouse: [330., 280.],
                    pressed: Some((MouseButton::Left, [330., 280.])),
                    ..Default::default()
                },
                &mut out,
            );
            assert!(controls.popup());
            assert!(out.is_empty());
            for key in [motion_ui::input::Key::Up, motion_ui::input::Key::Enter] {
                controls.overlay(
                    &mut paint,
                    &mut atlas,
                    &Value::Null,
                    locale,
                    &Input {
                        keys: vec![(key, Default::default())],
                        ..Default::default()
                    },
                    &mut out,
                );
            }
            let [Action::ViewerSettings(next)] = out.as_slice() else {
                panic!("unexpected dropdown commands: {out:?}")
            };
            let mut expected = before.clone();
            expected.set_layout(Layout::Two);
            assert_eq!(*next, expected);
            assert_eq!(controls.settings, before);
        }
    }
    #[test]
    fn hidden_composition_clears_its_popup_and_cannot_keep_old_footer_hit_regions() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut panels = Panels::default();
        let composition = dock::Node::dock(Panel::Composition.dock());
        let state = serde_json::json!({"frame":0,"project":{"width":640,"height":480,"layers":[]}});
        let mut paint = PaintList::default();
        super::super::chrome(
            &mut paint,
            &mut atlas,
            [900., 600.],
            &composition,
            &mut panels,
            &state,
            &Input::default(),
        );
        let anchor = panels.viewer_views.anchors.unwrap().1;
        let p = [
            (anchor.min[0] + anchor.max[0]) * 0.5,
            (anchor.min[1] + anchor.max[1]) * 0.5,
        ];
        let actions = super::super::chrome(
            &mut paint,
            &mut atlas,
            [900., 600.],
            &composition,
            &mut panels,
            &state,
            &Input {
                mouse: p,
                pressed: Some((MouseButton::Left, p)),
                ..Default::default()
            },
        );
        assert!(actions.is_empty());
        assert!(panels.viewer_views.popup());
        let timeline = dock::Node::dock(Panel::Timeline.dock());
        let actions = super::super::chrome(
            &mut paint,
            &mut atlas,
            [900., 600.],
            &timeline,
            &mut panels,
            &state,
            &Input {
                mouse: p,
                pressed: Some((MouseButton::Left, p)),
                ..Default::default()
            },
        );
        assert!(!panels.viewer_views.popup());
        assert!(panels.viewer_views.anchors.is_none());
        assert!(!actions
            .iter()
            .any(|action| matches!(action, Action::ViewerSettings(_))));
    }
}
