//! Shared asset rows and window-owned overlays for Project and Media.
use super::*;
use crate::media_assets::{Intent, Source};
use motion_core::media_assets::{self, AssetKind, AssetRef, AssetUsage, MediaAsset};
use motion_ui::{
    style::{box_style, text_style, State, States},
    widgets::{ListItem, ListView, ScrollArea},
};

#[derive(Default)]
pub(crate) struct Controls {
    selected: Option<AssetRef>,
    document: Option<(String, u64)>,
    lists: [ListView; 2],
    cache: Option<Cache>,
    menu: Option<Menu>,
    window: Option<Panel>,
    #[cfg(test)]
    pub(crate) rows: Vec<(AssetRef, Rect)>,
    #[cfg(test)]
    pub(crate) menu_rows: Vec<Rect>,
}
struct Cache {
    stamp: (String, u64, u64),
    project: Result<motion_core::Project, String>,
    names: Vec<(AssetRef, String)>,
}
struct Menu {
    source: Source,
    owner: Option<Panel>,
    anchor: [f32; 2],
    usage_view: bool,
    scroll: ScrollArea,
    focus: usize,
}

fn word(locale: Locale, zh: &str, en: &str) -> String {
    if locale == Locale::Zh {
        zh.into()
    } else {
        en.into()
    }
}
fn kind_name(locale: Locale, kind: AssetKind) -> &'static str {
    match (locale, kind) {
        (Locale::Zh, AssetKind::Image) => "图片",
        (Locale::Zh, AssetKind::Audio) => "音频",
        (Locale::Zh, AssetKind::Video) => "视频",
        (_, AssetKind::Image) => "Image",
        (_, AssetKind::Audio) => "Audio",
        (_, AssetKind::Video) => "Video",
    }
}
fn source_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|p| !p.is_empty())
        .unwrap_or(path)
}
impl Controls {
    pub(super) fn open(&self) -> bool {
        self.menu.is_some()
    }
    pub(super) fn cancel(&mut self) {
        self.menu = None;
    }
    pub(super) fn select(&mut self, target: AssetRef) { self.selected = Some(target); }
    pub(super) fn open_for(&mut self, state:&Value, target:AssetRef, anchor:[f32;2])->Result<(),String>{
        self.selected=Some(target);
        self.menu=Some(Menu {source:Source::read(state,target)?,owner:self.window,anchor,
            usage_view:false,scroll:Default::default(),focus:0});
        Ok(())
    }
    /// True for the entire open/closing frame, including input from another window.
    pub(super) fn prepare(&mut self, owner: Option<Panel>, state: &Value, input: &Input) -> bool {
        self.observe_document(state);
        self.window = owner;
        let owned = self.menu.is_some();
        if self.menu.as_ref().is_some_and(|menu| {
            !menu.source.current(state)
                || menu.owner == owner && input.focus_lost
                || menu.owner != owner
                    && (input.pressed.is_some() || !input.keys.is_empty() || !input.text.is_empty())
        }) {
            self.menu = None;
        }
        owned
    }
    fn observe_document(&mut self, state: &Value) {
        let document = (
            state["root"].as_str().unwrap_or_default().to_owned(),
            state["document_generation"].as_u64().unwrap_or_default(),
        );
        if self.document.as_ref().is_some_and(|previous| previous != &document) {
            self.selected = None;
            self.lists = Default::default();
        }
        self.document = Some(document);
    }
    fn project(&mut self, state: &Value) -> &Result<motion_core::Project, String> {
        self.observe_document(state);
        let stamp = (
            state["root"].as_str().unwrap_or_default().to_owned(),
            state["document_generation"].as_u64().unwrap_or_default(),
            state["revision"].as_u64().unwrap_or_default(),
        );
        if self.cache.as_ref().is_none_or(|cache| cache.stamp != stamp) {
            let project = serde_json::from_value::<motion_core::Project>(state["project"].clone())
                .map_err(|error| {
                    format!("asset metadata revision={} phase=decode: {error}", stamp.2)
                });
            if let Err(error) = &project {
                // One record per immutable snapshot, bounded by replacing this cache.
                log::error!("{error}");
            }
            let names = project
                .as_ref()
                .map(|p| {
                    p.assets
                        .iter()
                        .cloned()
                        .map(MediaAsset::Image)
                        .chain(p.video_assets.iter().cloned().map(MediaAsset::Video))
                        .chain(p.audio_assets.iter().cloned().map(MediaAsset::Audio))
                        .map(|asset| {
                            let usages = media_assets::usages(p, asset.target());
                            let name = usages.first().map_or_else(
                                || source_name(asset.path()).to_owned(),
                                |usage| usage.name.clone(),
                            );
                            (asset.target(), name)
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.cache = Some(Cache {
                stamp,
                project,
                names,
            });
        }
        &self.cache.as_ref().expect("cache installed").project
    }
}

pub(super) fn render_list(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    host: Panel,
    query: &str,
) {
    let styles = list.styles();
    let locale = panels.locale;
    let controls = &mut panels.media_assets;
    let _ = controls.project(state);
    let cache = controls.cache.as_ref().expect("cache installed");
    let project = match &cache.project {
        Ok(project) => project,
        Err(error) => {
            label(list, atlas, area, &locale.error(error), true);
            return;
        }
    };
    let query = query.to_lowercase();
    let assets: Vec<MediaAsset> = project
        .assets
        .iter()
        .cloned()
        .map(MediaAsset::Image)
        .chain(project.video_assets.iter().cloned().map(MediaAsset::Video))
        .chain(project.audio_assets.iter().cloned().map(MediaAsset::Audio))
        .filter(|asset| {
            source_name(asset.path()).to_lowercase().contains(&query)
                || cache
                    .names
                    .iter()
                    .find(|(target, _)| *target == asset.target())
                    .is_some_and(|(_, name)| name.to_lowercase().contains(&query))
        })
        .collect();
    let mut labels: Vec<String> = assets
        .iter()
        .map(|asset| {
            let name = cache
                .names
                .iter()
                .find(|(target, _)| *target == asset.target())
                .map_or(source_name(asset.path()), |(_, name)| name.as_str());
            format!("{name} · {}", kind_name(locale, asset.target().kind))
        })
        .collect();
    // Fonts keep their existing visibility; their string IDs are not media IDs.
    if host == Panel::Media {
        labels.extend(
            project
                .fonts
                .iter()
                .map(|font| format!("{} · {}", font.name, word(locale, "字体", "Font"))),
        );
    }
    let index = usize::from(host == Panel::Media);
    let view = &mut controls.lists[index];
    view.selected = assets
        .iter()
        .position(|asset| Some(asset.target()) == controls.selected);
    let items: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| ListItem {
            label,
            depth: 0,
            expanded: None,
            enabled: i < assets.len(),
        })
        .collect();
    let response = view.draw(
        list,
        atlas,
        area,
        &items,
        input,
        panels.focused == Some(host) && !panels.project.search,
    );
    let row = styles.metric("row.height", 22.);
    #[cfg(test)]
    {
        controls.rows.clear();
    }
    let mut chosen = response.selected;
    for (i, asset) in assets.iter().enumerate() {
        let rect = Rect::new(
            area.min[0],
            area.min[1] + i as f32 * row - view.scroll.offset,
            area.width(),
            row,
        );
        #[cfg(test)]
        {
            if rect.max[1] > area.min[1] && rect.min[1] < area.max[1] {
                controls.rows.push((asset.target(), rect));
            }
        }
        if area.contains(input.mouse) && right_clicked(input, rect) {
            chosen = Some(i);
            match Source::read(state, asset.target()) {
                Ok(source) => {
                    controls.menu = Some(Menu {
                        source,
                        owner: controls.window,
                        anchor: input.mouse,
                        usage_view: false,
                        scroll: Default::default(),
                        focus: 0,
                    })
                }
                Err(error) => {
                    log::error!(
                        "asset menu open failed asset={} kind={:?} cause={error}",
                        asset.target().id,
                        asset.target().kind
                    );
                    panels.message = Some(locale.error(&error));
                }
            }
            panels.project.context_menu = None;
        }
    }
    if let Some(i) = chosen {
        controls.selected = assets.get(i).map(MediaAsset::target);
    }
}

pub(super) fn overlay(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    bounds: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    owner: Option<Panel>,
    out: &mut Vec<Action>,
) {
    let controls = &mut panels.media_assets;
    let Some(mut menu) = controls.menu.take() else {
        return;
    };
    if menu.owner != owner {
        controls.menu = Some(menu);
        return;
    }
    if !menu.source.current(state) || input.focus_lost {
        return;
    }
    let Ok(project) = controls.project(state) else {
        return;
    };
    let Some(asset) = media_assets::lookup(project, menu.source.target) else {
        return;
    };
    let usages = media_assets::usages(project, menu.source.target);
    let linked = menu.source.target.kind == AssetKind::Audio
        && project
            .video_assets
            .iter()
            .any(|v| v.audio_asset == Some(menu.source.target.id));
    let locale = panels.locale;
    let title = source_name(asset.path()).to_owned();
    let mut entries: Vec<(String, bool, Option<Intent>)> = if menu.usage_view {
        let mut entries = vec![(
            word(locale, "返回素材操作", "Back to asset actions"),
            true,
            None,
        )];
        entries.extend(usages.iter().map(|usage| {
            let composition = if usage.composition == project.composition_id {
                project.name.as_str()
            } else {
                project
                    .compositions
                    .iter()
                    .find(|c| c.id == usage.composition)
                    .map_or(usage.composition.as_str(), |c| c.name.as_str())
            };
            (
                format!(
                    "{composition} / {} · {}",
                    usage.name,
                    usage_role(locale, usage)
                ),
                true,
                Some(Intent::RevealUsage(usage.clone())),
            )
        }));
        if usages.is_empty() {
            entries.push((
                word(locale, "未被图层使用", "No layer uses this asset"),
                false,
                None,
            ));
        }
        entries
    } else {
        vec![
            (
                word(locale, "重链接 / 替换素材…", "Relink / replace asset…"),
                !linked,
                Some(Intent::Replace),
            ),
            (
                word(locale, "显示文件位置", "Show file location"),
                true,
                Some(Intent::RevealFile),
            ),
            (
                word(
                    locale,
                    &format!("显示使用处（{}）", usages.len()),
                    &format!("Show usages ({})", usages.len()),
                ),
                true,
                None,
            ),
            (
                word(locale, "从工程中移除", "Remove from project"),
                usages.is_empty() && !linked,
                Some(Intent::Remove),
            ),
            (
                word(locale, "重链接丢失素材…", "Relink missing assets…"),
                true,
                Some(Intent::RelinkMissing),
            ),
            (
                word(locale, "放入当前合成（播放头）", "Place in composition at playhead"),
                true,
                Some(Intent::Place),
            ),
            (
                word(locale, "刷新缩略图", "Refresh thumbnail"),
                matches!(menu.source.target.kind, AssetKind::Image | AssetKind::Video),
                Some(Intent::RefreshThumbnail),
            ),
            (
                word(locale, "预览素材…", "Preview asset…"),
                matches!(menu.source.target.kind, AssetKind::Image | AssetKind::Video),
                Some(Intent::Preview { owner }),
            ),
        ]
    };
    let note = if linked {
        word(
            locale,
            "关联音频：请操作所属视频素材",
            "Linked audio: use the owning video asset",
        )
    } else if !usages.is_empty() {
        word(
            locale,
            "有引用的素材不能移除",
            "Referenced assets cannot be removed",
        )
    } else {
        word(
            locale,
            "移除保留文件，可撤销",
            "Removal keeps the file and can be undone",
        )
    };
    let styles = list.styles();
    let padding = styles.metric("padding", 5.);
    let row = styles.metric("context.row.height", 20.);
    let width = styles
        .metric("menu.popup.min_width", 280.)
        .min(bounds.width());
    let height = ((entries.len() + 2) as f32 * row + padding * 2.).min(bounds.height());
    let card = Rect::new(
        menu.anchor[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
        menu.anchor[1].clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
        width,
        height,
    );
    if !input.composing
        && !input.ime_consumed
        && input
            .key_presses()
            .any(|(key, _, repeat)| key == Key::Escape && !repeat)
        || input
            .pressed
            .is_some_and(|(_, point)| !card.contains(point))
    {
        return;
    }
    box_style(
        list,
        card,
        styles.resolve("context_menu", States::default()).unwrap(),
    );
    text_style(
        list,
        atlas,
        Rect::new(
            card.min[0] + padding,
            card.min[1],
            width - padding * 2.,
            row,
        ),
        &title,
        styles
            .resolve("context_menu.row", States::default())
            .unwrap(),
        Align::Left,
    );
    let body = Rect::new(
        card.min[0] + padding,
        card.min[1] + row,
        width - padding * 2.,
        (height - row * 2. - padding).max(0.),
    );
    let neutral = Input::default();
    let interactive = !input.composing && !input.ime_consumed;
    if interactive {
        let old_focus = menu.focus;
        for (key, _, repeat) in input.key_presses() {
            if repeat && !matches!(key, Key::Up | Key::Down) {
                continue;
            }
            if key == Key::Down {
                menu.focus = (menu.focus + 1).min(entries.len().saturating_sub(1));
            }
            if key == Key::Up {
                menu.focus = menu.focus.saturating_sub(1);
            }
        }
        if menu.focus != old_focus {
            menu.scroll.offset = menu
                .scroll
                .offset
                .min(menu.focus as f32 * row)
                .max(((menu.focus + 1) as f32 * row - body.height()).max(0.));
        }
    }
    let frame = menu.scroll.begin_view(
        list,
        body,
        entries.len() as f32 * row,
        if interactive { input } else { &neutral },
        true,
    );
    #[cfg(test)]
    {
        controls.menu_rows.clear();
    }
    let mut child = list.child();
    let mut chosen = None;
    for (i, (text, enabled, _)) in entries.iter().enumerate() {
        let rect = Rect::new(
            frame.content.min[0],
            frame.content.min[1] + i as f32 * row,
            frame.content.width(),
            row,
        );
        #[cfg(test)]
        {
            controls.menu_rows.push(rect);
        }
        let mut states = States::default();
        if !enabled {
            states = states.with(State::Disabled);
        } else if rect.contains(input.mouse) || i == menu.focus {
            states = states.with(State::Selected);
        }
        let style = styles.resolve("context_menu.row", states).unwrap();
        box_style(&mut child, rect, style);
        text_style(
            &mut child,
            atlas,
            rect.shrink(styles.metric("border", 1.)),
            text,
            style,
            Align::Left,
        );
        if *enabled
            && interactive
            && (input.pressed.is_some_and(|(b, p)| {
                b == MouseButton::Left && body.contains(p) && rect.contains(p)
            }) || i == menu.focus
                && input
                    .key_presses()
                    .any(|(k, m, r)| k == Key::Enter && !m.control && !m.alt && !r))
        {
            chosen = Some(i);
        }
    }
    child.clip_to(frame.viewport);
    list.append(child);
    text_style(
        list,
        atlas,
        Rect::new(
            card.min[0] + padding,
            card.max[1] - row,
            width - padding * 2.,
            row,
        ),
        &note,
        styles
            .resolve("context_menu.row", States::default().with(State::Disabled))
            .unwrap(),
        Align::Left,
    );
    if let Some(i) = chosen {
        if let Some(intent) = entries[i].2.take() {
            out.push(Action::MediaAsset {
                source: menu.source,
                intent,
            });
            return;
        }
        menu.usage_view = !menu.usage_view;
        menu.scroll = Default::default();
        menu.focus = 0;
    }
    controls.menu = Some(menu);
}

fn usage_role(locale: Locale, usage: &AssetUsage) -> String {
    match usage.role.as_str() {
        "image" => word(locale, "图片", "Image"),
        "text_raster" => word(locale, "文字栅格", "Text raster"),
        "audio" => word(locale, "音频", "Audio"),
        "video" => word(locale, "视频", "Video"),
        "video_audio" => word(locale, "视频音轨", "Video audio"),
        _ => word(locale, "效果输入", "Effect input"),
    }
}

#[cfg(test)]
mod tests;
