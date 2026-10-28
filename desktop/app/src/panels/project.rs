//! Project inventory, independent of layer selection and the active composition.
use super::*;
use motion_core::project_library::{LibraryAction, ProjectItem};
use motion_ui::{
    style::{box_style, text_style, State, States},
    widgets::{ScrollArea, TextEditor},
};
mod catalogue;
mod popup;
pub(crate) mod thumbnails;
#[cfg(test)]
mod tests;
use catalogue::{Catalogue, RowKey};
use popup::Popup;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ViewPreferences {
    pub root: String,
    pub folder: Option<u64>,
    pub query: String,
    pub search_all: bool,
}
impl ViewPreferences {
    pub(crate) fn valid(&self) -> bool {
        self.root.len() <= 4096 && self.query.len() <= 256
    }
}
#[derive(Default)]
pub(crate) struct Controls {
    pub view: ViewPreferences,
    cache: Option<Result<Catalogue, String>>,
    stamp: Option<crate::layer_clipboard::KeyframeSource>,
    selected: Option<RowKey>,
    scroll: ScrollArea,
    popup: Option<Popup>,
    popup_fresh: bool,
    context: Option<RowKey>,
    context_focus: usize,
    owner: Option<Panel>,
    last_click: Option<(RowKey, std::time::Instant)>,
    document: Option<(String, u64)>,
    persisted: Option<ViewPreferences>,
    #[cfg(test)]
    rows: Vec<(RowKey, Rect)>,
    #[cfg(test)]
    buttons: Vec<Rect>,
}
pub(super) fn word<'a>(locale: Locale, zh: &'a str, en: &'a str) -> &'a str {
    if locale == Locale::Zh {
        zh
    } else {
        en
    }
}
impl Controls {
    pub(crate) fn preview(&mut self, state:&Value, target:motion_core::media_assets::AssetRef, owner:Option<Panel>) {
        self.refresh(state);
        self.owner=owner;
        let item=if target.kind==motion_core::media_assets::AssetKind::Video {ProjectItem::Video{id:target.id}}else{ProjectItem::Image{id:target.id}};
        self.popup=Some(Popup::Preview { key:RowKey::Item(item), seek:Default::default() });
        self.popup_fresh=true;
    }
    pub(crate) fn modal(&self) -> bool {
        self.popup.is_some()
    }
    pub(crate) fn video_preview(&self) -> Option<u64> {
        match self.popup.as_ref() {
            Some(Popup::Preview { key:RowKey::Item(ProjectItem::Video {id}), .. })=>Some(*id),
            _=>None,
        }
    }
    pub(crate) fn preview_in(&self, owner:Option<Panel>) -> bool {
        matches!(self.popup,Some(Popup::Preview{..})) && self.owner==owner
    }
    pub fn cancel(&mut self) {
        self.popup = None;
        self.context = None;
        self.popup_fresh = false;
    }
    fn refresh(&mut self, state: &Value) {
        let source = crate::layer_clipboard::KeyframeSource::of(state);
        if self.stamp.as_ref() == Some(&source) {
            return;
        }
        let root = state["root"].as_str().unwrap_or_default();
        let document = (
            root.to_owned(),
            state["document_generation"].as_u64().unwrap_or_default(),
        );
        if self
            .document
            .as_ref()
            .is_some_and(|previous| previous != &document)
        {
            self.selected = None;
            self.last_click = None;
            self.cancel();
        }
        self.document = Some(document);
        if self.view.root != root {
            self.view = ViewPreferences {
                root: root.into(),
                ..Default::default()
            };
            self.scroll = Default::default();
            self.selected = None;
            self.cancel();
        }
        let cache = Catalogue::read(state);
        if let Err(error) = &cache {
            log::error!(
                "project inventory revision={} reason={error}",
                state["revision"]
            );
        }
        if let Ok(catalogue) = &cache {
            if self
                .view
                .folder
                .is_some_and(|id| catalogue.folder(id).is_none())
            {
                self.view.folder = None;
                self.scroll.offset = 0.;
            }
            if self
                .selected
                .as_ref()
                .is_some_and(|key| !catalogue.entries.iter().any(|entry| &entry.key == key))
            {
                self.selected = None;
            }
        }
        self.stamp = Some(source);
        self.cache = Some(cache);
    }
}
/// A popup owns its entire closing frame, including input from another window.
pub(super) fn prepare(
    panels: &mut Panels,
    owner: Option<Panel>,
    state: &Value,
    input: &Input,
) -> bool {
    let controls = &mut panels.project.browser;
    let owned = controls.modal() || panels.project.context_menu.is_some();
    let source = crate::layer_clipboard::KeyframeSource::of(state);
    let stale = controls
        .stamp
        .as_ref()
        .is_some_and(|stamp| stamp != &source);
    let other = controls.owner != owner
        && (input.pressed.is_some() || !input.keys.is_empty() || !input.text.is_empty());
    if owned && (stale || other || controls.owner == owner && input.focus_lost) {
        controls.cancel();
        panels.project.context_menu = None;
    }
    if !owned {
        controls.owner = owner;
    }
    owned
}
pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();
    let locale = panels.locale;
    paint::rect(list, area, colors.PANEL);
    let pad = styles.metric("padding.panel", 8.);
    let row = styles.metric("row.height", 22.);
    let tool = styles.metric("project.toolbar.height", 26.);
    let border = styles.metric("border", 1.);
    let action = styles.metric("project.action.width", 64.);
    let slot = styles.metric("project.item.preview.slot", 40.);
    let browser = &mut panels.project.browser;
    let old_root = browser.view.root.clone();
    browser.refresh(state);
    if old_root != browser.view.root {
        panels.project.query.clone_from(&browser.view.query);
        browser.persisted = Some(browser.view.clone());
    }
    if browser.persisted.is_none() {
        browser.persisted = Some(browser.view.clone());
    }
    let before = browser.view.clone();
    let Some(cache) = browser.cache.as_ref() else {
        return;
    };
    let catalogue = match cache {
        Ok(c) => c,
        Err(error) => {
            label(list, atlas, area, &locale.error(error), true);
            return;
        }
    };
    let bar = Rect::new(
        area.min[0] + pad,
        area.min[1] + pad,
        (area.width() - pad * 2.).max(0.),
        tool,
    );
    let up = Rect::new(bar.min[0], bar.min[1], tool, tool);
    let parent = browser
        .view
        .folder
        .and_then(|id| catalogue.folder(id))
        .and_then(|f| f.parent);
    if ui::button(list, atlas, up, "<", input, browser.view.folder.is_some()).clicked {
        browser.view.folder = parent;
        browser.scroll.offset = 0.;
        browser.selected = None;
    }
    let path = Rect::new(
        up.max[0] + pad,
        bar.min[1],
        (bar.width() - tool - pad).max(0.),
        tool,
    );
    label(
        list,
        atlas,
        path,
        &catalogue.breadcrumb(browser.view.folder, locale),
        true,
    );
    let search = Rect::new(bar.min[0], bar.max[1] + pad, bar.width(), tool);
    let response = panels.project.search_field.draw(
        list,
        atlas,
        search,
        &mut panels.project.query,
        locale.t(T::Search),
        input,
        true,
    );
    panels.project.search = response.active;
    while panels.project.query.len() > 256 {
        panels.project.query.pop();
    }
    if browser.view.query != panels.project.query {
        browser.view.query.clone_from(&panels.project.query);
        browser.scroll.offset = 0.;
    }
    let scope = Rect::new(bar.min[0], search.max[1] + border, bar.width(), row);
    ui::checkbox(
        list,
        atlas,
        scope,
        word(locale, "搜索所有目录", "Search all folders"),
        &mut browser.view.search_all,
        input,
    );
    let footer_height = styles.metric("project.footer.height", 44.);
    let footer = Rect::new(
        area.min[0] + pad,
        (area.max[1] - footer_height - pad).max(scope.max[1]),
        bar.width(),
        footer_height,
    );
    let controls = Rect::new(bar.min[0], scope.max[1] + pad, bar.width(), tool);
    let add = Rect::new(
        controls.min[0],
        controls.min[1],
        action.min(controls.width()),
        tool,
    );
    let mv = Rect::new(
        add.max[0] + pad,
        controls.min[1],
        action.min((controls.max[0] - add.max[0] - pad).max(0.)),
        tool,
    );
    let import = Rect::new(
        mv.max[0] + pad,
        controls.min[1],
        (controls.max[0] - mv.max[0] - pad).max(0.),
        tool,
    );
    #[cfg(test)]
    {
        browser.buttons = vec![up, add, mv, import, scope];
        browser.rows.clear();
        panels.media_assets.rows.clear();
    }
    let folder = browser.view.folder;
    if ui::button(
        list,
        atlas,
        add,
        word(locale, "新目录", "Folder +"),
        input,
        true,
    )
    .clicked
    {
        browser.popup = Some(Popup::name(catalogue, None, folder, locale));
        browser.popup_fresh = true;
    }
    if ui::button(
        list,
        atlas,
        mv,
        word(locale, "移动…", "Move…"),
        input,
        browser.selected.is_some(),
    )
    .clicked
    {
        browser.popup = browser
            .selected
            .clone()
            .map(|key| Popup::moving(catalogue, key));
        browser.popup_fresh = true;
    }
    if ui::button(
        list,
        atlas,
        import,
        word(locale, "导入…", "Import…"),
        input,
        true,
    )
    .clicked
    {
        out.push(Action::ProjectImport {
            source: catalogue.source.clone(),
            folder: browser.view.folder,
            path: None,
        });
    }
    let viewport = Rect::new(
        bar.min[0],
        controls.max[1] + pad,
        bar.width(),
        (footer.min[1] - controls.max[1] - pad * 2.).max(0.),
    );
    let entries = catalogue.visible(&browser.view, &browser.view.query);
    let focused = panels.focused == Some(Panel::Project) && !panels.project.search;
    let mut activate_key = None;
    let mut menu_key = None;
    if focused {
        let current = browser
            .selected
            .as_ref()
            .and_then(|key| entries.iter().position(|entry| &entry.key == key));
        for (key, _, repeat) in input.key_presses() {
            let next = match key {
                Key::Down => {
                    Some(current.map_or(0, |i| (i + 1).min(entries.len().saturating_sub(1))))
                }
                Key::Up => Some(current.unwrap_or(0).saturating_sub(1)),
                _ => None,
            };
            if let Some((i, entry)) = next.and_then(|i| entries.get(i).map(|entry| (i, entry))) {
                browser.selected = Some(entry.key.clone());
                browser.scroll.offset = browser
                    .scroll
                    .offset
                    .min(i as f32 * row)
                    .max(((i + 1) as f32 * row - viewport.height()).max(0.));
            }
            if key == Key::Enter && !repeat {
                activate_key = current
                    .and_then(|i| entries.get(i))
                    .map(|entry| entry.key.clone());
            }
        }
    }
    let frame =
        browser
            .scroll
            .begin_view(list, viewport, entries.len() as f32 * row, input, focused);
    let mut child = list.child();
    let first = (browser.scroll.offset / row).floor() as usize;
    let type_width = styles.metric("project.type.width", 72.);
    let narrow = viewport.width() < styles.metric("project.browser.min_width", 240.);
    for (i, entry) in entries
        .iter()
        .enumerate()
        .skip(first)
        .take((viewport.height() / row).ceil() as usize + 1)
    {
        let rect = Rect::new(
            frame.content.min[0],
            frame.content.min[1] + i as f32 * row,
            frame.content.width(),
            row,
        );
        let mut states = States::default();
        if rect.contains(input.mouse) && frame.viewport.contains(input.mouse) {
            states = states.with(State::Hovered);
        }
        if browser.selected.as_ref() == Some(&entry.key) {
            states = states.with(State::Selected);
        }
        let style = styles.resolve("row", states).expect("row style");
        box_style(&mut child, rect, style);
        let source_frame=match &entry.key {
            RowKey::Item(ProjectItem::Video{id}) if panels.project.preview.current(state,*id)=>panels.project.preview.bitmap(),
            _=>None,
        };
        if let Some(bitmap)=source_frame.or_else(||panels.project.thumbnails.get(state,&entry.key)) {
            let [width,height]=bitmap.dimensions();
            let fit=(styles.metric("project.item.preview.width",32.)/width as f32).min(styles.metric("project.item.preview.height",18.)/height as f32);
            let w=width as f32*fit; let h=height as f32*fit;
            child.push_image(motion_ui::paint::ImageQuad::new(bitmap,Rect::new(rect.min[0]+(slot-w)*0.5,rect.min[1]+(row-h)*0.5,w,h)));
        } else { motion_ui::icons::draw(
            &mut child,
            Rect::new(rect.min[0], rect.min[1], slot, row),
            entry.key.icon(),
            styles.metric("project.item.icon_size", 14.),
            style.foreground,
        ); }
        let width = if narrow {
            rect.width() - slot - pad
        } else {
            rect.width() - slot - type_width - pad
        };
        text_style(
            &mut child,
            atlas,
            Rect::new(rect.min[0] + slot, rect.min[1], width.max(0.), row),
            if entry.name == "Untitled Project" {
                locale.t(T::Untitled)
            } else {
                &entry.name
            },
            style,
            Align::Left,
        );
        if !narrow {
            label(
                &mut child,
                atlas,
                Rect::new(rect.max[0] - type_width, rect.min[1], type_width, row),
                entry.key.caption(locale),
                true,
            );
        }
        #[cfg(test)]
        {
            browser.rows.push((entry.key.clone(), rect));
            if let Some(target) = entry.key.media() {
                panels.media_assets.rows.push((target, rect));
            }
        }
        if frame.viewport.contains(input.mouse)
            && input.pressed.is_some_and(|(_, p)| rect.contains(p))
        {
            browser.selected = Some(entry.key.clone());
            if let Some(target) = entry.key.media() {
                panels.media_assets.select(target);
            }
            if right_clicked(input, rect) {
                menu_key = Some(entry.key.clone());
            } else if input
                .pressed
                .is_some_and(|(button, _)| button == MouseButton::Left)
            {
                if let Some(target) = entry.key.media() {
                    panels.project_drag.begin(target, input.mouse, state);
                }
                let now = std::time::Instant::now();
                if browser.last_click.as_ref().is_some_and(|(key, last)| {
                    key == &entry.key
                        && now.duration_since(*last).as_millis()
                            < u128::from(styles.metric("project.double_click_ms", 400.) as u32)
                }) {
                    activate_key = Some(entry.key.clone());
                    browser.last_click = None;
                } else {
                    browser.last_click = Some((entry.key.clone(), now));
                }
            }
        }
    }
    child.clip_to(frame.viewport);
    list.append(child);
    if let Some(key) = activate_key {
        activate(browser, &key, state, out);
    }
    if let Some(key) = menu_key {
        if let Some(target) = key.media() {
            match panels.media_assets.open_for(state, target, input.mouse) {
                Ok(()) => {
                    panels.project.context_menu = None;
                    browser.context = None;
                }
                Err(error) => {
                    log::error!("project asset menu target={target:?} reason={error}");
                    panels.message = Some(locale.error(&error));
                }
            }
        } else {
            panels.media_assets.cancel();
            browser.context = Some(key);
            browser.context_focus = 0;
            panels.project.context_menu = Some(input.mouse);
            panels.project.context_owner = browser.owner;
        }
    } else if right_clicked(input, viewport) {
        browser.context = None;
        browser.context_focus = 0;
        panels.media_assets.cancel();
        panels.project.context_menu = Some(input.mouse);
        panels.project.context_owner = browser.owner;
    }
    label(
        list,
        atlas,
        Rect::new(footer.min[0], footer.min[1], footer.width(), row),
        &format!(
            "{} {} · {} {}",
            entries.len(),
            locale.t(T::Items),
            state["project"]["layers"].as_array().map_or(0, Vec::len),
            locale.t(T::Layers)
        ),
        true,
    );
    let p = &state["project"];
    let fps = p["fps"].as_u64().unwrap_or(30) as u32;
    label(
        list,
        atlas,
        Rect::new(footer.min[0], footer.min[1] + row, footer.width(), row),
        &format!(
            "{} × {} · {} {} · {}",
            p["width"],
            p["height"],
            fps,
            locale.t(T::Fps),
            timecode(p["frames"].as_f64().unwrap_or(0.), fps)
        ),
        true,
    );
    if (before.folder != browser.view.folder
        || before.search_all != browser.view.search_all
        || !panels.project.search)
        && browser.persisted.as_ref() != Some(&browser.view)
    {
        browser.persisted = Some(browser.view.clone());
        out.push(Action::ProjectViewChanged(browser.view.clone()));
    }
}
fn activate(browser: &mut Controls, key: &RowKey, state: &Value, out: &mut Vec<Action>) {
    match key {
        RowKey::Folder(id) => {
            browser.view.folder = Some(*id);
            browser.scroll.offset = 0.;
            browser.selected = None;
        }
        RowKey::Item(ProjectItem::Composition { id }) => out.push(Action::ProjectOpen {
            source: crate::layer_clipboard::KeyframeSource::of(state),
            composition: id.clone(),
        }),
        _ => {}
    }
}
pub(super) fn finish_context_menu(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    bounds: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    locale: Locale,
    owner: Option<Panel>,
    out: &mut Vec<Action>,
) {
    popup::finish(
        list, atlas, bounds, panels, state, input, locale, owner, out,
    );
}
