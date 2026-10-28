use super::*;
use motion_ui::widgets::{ListItem, ListView, TextEvent};

fn child_preview(list:&mut PaintList,bitmap:std::sync::Arc<motion_ui::paint::Bitmap>,area:Rect) {
    list.push_image(motion_ui::paint::ImageQuad::new(bitmap,area));
}

pub(super) enum Popup {
    Preview {key:RowKey},
    Name {
        source: crate::layer_clipboard::KeyframeSource,
        id: Option<u64>,
        parent: Option<u64>,
        editor: TextEditor,
    },
    Move {
        source: crate::layer_clipboard::KeyframeSource,
        key: RowKey,
        destination: Option<u64>,
        list: ListView,
    },
}
impl Popup {
    pub fn name(
        catalogue: &Catalogue,
        id: Option<u64>,
        parent: Option<u64>,
        locale: Locale,
    ) -> Self {
        let mut editor = TextEditor::default();
        editor.begin_bounded(
            id.and_then(|id| catalogue.folder(id))
                .map_or(word(locale, "新文件夹", "New folder"), |f| {
                    f.name.as_str()
                }),
            256,
        );
        Self::Name {
            source: catalogue.source.clone(),
            id,
            parent,
            editor,
        }
    }
    pub fn moving(catalogue: &Catalogue, key: RowKey) -> Self {
        let destination = catalogue
            .entries
            .iter()
            .find(|entry| entry.key == key)
            .and_then(|entry| entry.folder);
        Self::Move {
            source: catalogue.source.clone(),
            key,
            destination,
            list: Default::default(),
        }
    }
}
fn command(
    out: &mut Vec<Action>,
    source: crate::layer_clipboard::KeyframeSource,
    action: LibraryAction,
) {
    out.push(Action::EditorAction {
        source,
        commands: vec![Command::ProjectLibrary { action }],
    });
}
pub(super) fn finish(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    bounds: Rect,
    panels: &mut Panels,
    _state: &Value,
    input: &Input,
    locale: Locale,
    owner: Option<Panel>,
    out: &mut Vec<Action>,
) {
    if panels.project.browser.owner != owner {
        return;
    }
    let browser = &mut panels.project.browser;
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let row = styles.metric("row.height", 22.);
    let Some(Ok(catalogue)) = browser.cache.as_ref() else {
        return;
    };
    if let Some(mut popup) = browser.popup.take() {
        let neutral = Input::default();
        let input = if std::mem::take(&mut browser.popup_fresh) {
            &neutral
        } else {
            input
        };
        let moving = matches!(popup, Popup::Move { .. });
        let preview=matches!(popup, Popup::Preview{..});
        let width = if preview {styles.metric("project.preview.width",400.)}else{styles
            .metric("project.folder.dialog.width", 400.)
        }
            .min(bounds.width());
        let height = if preview {styles.metric("project.preview.height",320.)} else if moving {
            styles.metric("project.folder.move.height", 360.)
        } else {
            styles.metric("project.folder.dialog.height", 180.)
        }
        .min(bounds.height());
        let card = Rect::new(
            bounds.min[0] + (bounds.width() - width) * 0.5,
            bounds.min[1] + (bounds.height() - height) * 0.5,
            width,
            height,
        );
        if input.focus_lost
            || input
                .key_presses()
                .any(|(k, _, repeat)| k == Key::Escape && !repeat)
            || input
                .pressed
                .is_some_and(|(_, point)| !card.contains(point))
        {
            return;
        }
        box_style(
            list,
            card,
            styles
                .resolve("panel", States::default())
                .expect("panel style"),
        );
        paint::stroke(
            list,
            card,
            styles.colors().BORDER,
            styles.metric("border", 1.),
        );
        let title = Rect::new(
            card.min[0] + pad,
            card.min[1] + pad,
            (width - pad * 2.).max(0.),
            row,
        );
        let button_width = styles
            .metric("dialog.notice.button_width", 96.)
            .min((width - pad * 3.).max(0.) * 0.5);
        let apply = Rect::new(
            card.max[0] - pad - button_width,
            card.max[1] - pad - row,
            button_width,
            row,
        );
        let cancel = Rect::new(
            apply.min[0] - pad - button_width,
            apply.min[1],
            button_width,
            row,
        );
        #[cfg(test)]
        {
            browser.buttons = vec![apply, cancel];
        }
        if let Popup::Preview {key}= &popup {
            let video=matches!(key,RowKey::Item(ProjectItem::Video{..}));
            label(list,atlas,title,if video {word(locale,"视频首帧预览","Video first frame")}else{word(locale,"图像预览","Image preview")},false);
            let content=Rect::new(title.min[0],title.max[1]+pad,title.width(),(apply.min[1]-title.max[1]-pad*2.).max(0.));
            if let Some(bitmap)=panels.project.thumbnails.get(_state,key) {
                let [w,h]=bitmap.dimensions();
                let fit=(content.width()/w as f32).min(content.height()/h as f32);
                let width=w as f32*fit;let height=h as f32*fit;
                child_preview(list,bitmap,Rect::new(content.min[0]+(content.width()-width)*0.5,content.min[1]+(content.height()-height)*0.5,width,height));
            } else {label(list,atlas,content,if panels.project.thumbnails.failed(_state,key) {word(locale,"预览读取失败，请关闭后刷新缩略图或重链接素材","Preview failed; close and refresh thumbnail or relink asset")}else{word(locale,"正在准备预览…","Preparing preview…")},true);}
            if ui::button(list,atlas,apply,word(locale,"关闭","Close"),input,true).clicked {return;}
            browser.popup=Some(popup);
            return;
        }
        let mut submit = false;
        let enabled;
        match &mut popup {
            Popup::Preview {..}=>unreachable!("preview handled above"),
            Popup::Name { id, editor, .. } => {
                label(
                    list,
                    atlas,
                    title,
                    if id.is_some() {
                        word(locale, "重命名文件夹", "Rename folder")
                    } else {
                        word(locale, "新建文件夹", "New folder")
                    },
                    false,
                );
                let field = Rect::new(
                    title.min[0],
                    title.max[1] + pad,
                    title.width(),
                    styles.metric("field.height", 18.),
                );
                let event = editor.input(input);
                if matches!(event, Some(TextEvent::Cancel)) {
                    return;
                }
                if let Some(TextEvent::Commit(value)) = event {
                    editor.begin_at_end(&value);
                    submit = true;
                }
                enabled =
                    !editor.text.trim().is_empty() && !editor.text.chars().any(char::is_control);
                editor.draw(list, atlas, field);
                if !enabled {
                    label(
                        list,
                        atlas,
                        Rect::new(field.min[0], field.max[1] + pad, field.width(), row),
                        word(
                            locale,
                            "名称不能为空或含控制字符",
                            "Enter a name without control characters",
                        ),
                        true,
                    );
                }
            }
            Popup::Move {
                key,
                destination,
                list: view,
                ..
            } => {
                label(
                    list,
                    atlas,
                    title,
                    word(locale, "移动到文件夹", "Move to folder"),
                    false,
                );
                let mut folders = vec![(None, catalogue.breadcrumb(None, locale))];
                folders.extend(
                    catalogue
                        .library
                        .folders
                        .iter()
                        .map(|f| (Some(f.id), catalogue.breadcrumb(Some(f.id), locale))),
                );
                let items: Vec<_> = folders
                    .iter()
                    .map(|(folder, name)| ListItem {
                        label: name,
                        depth: 0,
                        expanded: None,
                        enabled: catalogue.allows_move(key, *folder),
                    })
                    .collect();
                view.selected = folders.iter().position(|(folder, _)| folder == destination);
                let choices = Rect::new(
                    title.min[0],
                    title.max[1] + pad,
                    title.width(),
                    (apply.min[1] - title.max[1] - pad * 2.).max(0.),
                );
                let response = view.draw(list, atlas, choices, &items, input, true);
                if let Some(index) = response.selected {
                    *destination = folders[index].0;
                }
                enabled = catalogue.allows_move(key, *destination);
                submit = response.activated.is_some()
                    && input.key_presses().any(|(key, mods, repeat)| {
                        key == Key::Enter && !repeat && !mods.control && !mods.alt
                    });
            }
        }
        if ui::button(
            list,
            atlas,
            cancel,
            word(locale, "取消", "Cancel"),
            input,
            true,
        )
        .clicked
        {
            return;
        }
        submit |= ui::button(
            list,
            atlas,
            apply,
            word(locale, "确定", "Apply"),
            input,
            enabled,
        )
        .clicked;
        if submit && enabled {
            match popup {
                Popup::Preview {..}=>unreachable!("preview does not submit document edits"),
                Popup::Name {
                    source,
                    id,
                    parent,
                    editor,
                } => {
                    let action = if let Some(id) = id {
                        LibraryAction::RenameFolder {
                            id,
                            name: editor.text.trim().into(),
                        }
                    } else {
                        let Some(id) = (1..=motion_core::project_library::MAX_FOLDERS as u64 + 1)
                            .find(|id| {
                                catalogue
                                    .library
                                    .folders
                                    .iter()
                                    .all(|folder| folder.id != *id)
                            })
                        else {
                            log::error!("project folder creation phase=allocate reason=bounded_id_pool_exhausted");
                            panels.message = Some(
                                word(locale, "文件夹编号已耗尽", "Folder IDs exhausted").into(),
                            );
                            return;
                        };
                        LibraryAction::CreateFolder {
                            id,
                            parent,
                            name: editor.text.trim().into(),
                        }
                    };
                    command(out, source, action);
                }
                Popup::Move {
                    source,
                    key,
                    destination,
                    ..
                } => {
                    let action = match key {
                        RowKey::Folder(id) => LibraryAction::MoveFolder {
                            id,
                            parent: destination,
                        },
                        RowKey::Item(item) => LibraryAction::MoveItems {
                            items: vec![item],
                            folder: destination,
                        },
                    };
                    command(out, source, action);
                }
            }
        } else {
            browser.popup = Some(popup);
        }
        return;
    }
    if panels.project.context_owner != owner {
        return;
    }
    let Some(anchor) = panels.project.context_menu else {
        return;
    };
    #[derive(Clone, Copy)]
    enum Choice {
        Open,
        Rename,
        Move,
        Remove,
        Create,
        Import,
        Settings,
    }
    let folder = matches!(browser.context, Some(RowKey::Folder(_)));
    let composition = matches!(
        browser.context,
        Some(RowKey::Item(ProjectItem::Composition { .. }))
    );
    let mut choices = Vec::new();
    if folder || composition {
        choices.push((word(locale, "打开", "Open"), Choice::Open));
    }
    if folder {
        choices.push((word(locale, "重命名…", "Rename…"), Choice::Rename));
    }
    if browser.context.is_some() {
        choices.push((word(locale, "移动到…", "Move to…"), Choice::Move));
    }
    if folder {
        choices.push((
            word(
                locale,
                "移除文件夹（保留内容）",
                "Remove folder (keep contents)",
            ),
            Choice::Remove,
        ));
    }
    if browser.context.as_ref().is_some_and(|key|matches!(key,RowKey::Item(ProjectItem::Composition{id}) if _state["project"]["composition_id"]==*id)){
        choices.push((word(locale,"合成设置…","Composition Settings…"),Choice::Settings));
    }
    choices.push((word(locale, "新建文件夹…", "New folder…"), Choice::Create));
    choices.push((word(locale, "导入素材…", "Import media…"), Choice::Import));
    let width = styles
        .metric("context.menu.width", 220.)
        .min(bounds.width());
    let row = styles.metric("context.row.height", 20.);
    let height = (choices.len() as f32 * row + pad * 2.).min(bounds.height());
    let card = Rect::new(
        anchor[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
        anchor[1].clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
        width,
        height,
    );
    if input
        .key_presses()
        .any(|(key, _, repeat)| key == Key::Escape && !repeat)
        || input
            .pressed
            .is_some_and(|(_, point)| !card.contains(point))
    {
        panels.project.context_menu = None;
        return;
    }
    box_style(
        list,
        card,
        styles
            .resolve("context_menu", States::default())
            .expect("context style"),
    );
    let mut chosen = None;
    for (key, mods, repeat) in input.key_presses() {
        if mods.control || mods.alt {
            continue;
        }
        match key {
            Key::Up => browser.context_focus = browser.context_focus.saturating_sub(1),
            Key::Down => {
                browser.context_focus =
                    (browser.context_focus + 1).min(choices.len().saturating_sub(1))
            }
            Key::Enter if !repeat => {
                chosen = choices
                    .get(browser.context_focus)
                    .map(|(_, choice)| *choice)
            }
            _ => {}
        }
    }
    #[cfg(test)]
    {
        browser.buttons.clear();
    }
    for (index, (caption, choice)) in choices.iter().enumerate() {
        let rect = Rect::new(
            card.min[0] + pad,
            card.min[1] + pad + index as f32 * row,
            (width - pad * 2.).max(0.),
            row,
        );
        let states = if rect.contains(input.mouse) || index == browser.context_focus {
            States::default().with(State::Selected)
        } else {
            States::default()
        };
        let style = styles
            .resolve("context_menu.row", states)
            .expect("context row");
        box_style(list, rect, style);
        text_style(list, atlas, rect, *caption, style, Align::Left);
        #[cfg(test)]
        {
            browser.buttons.push(rect);
        }
        if input
            .pressed
            .is_some_and(|(button, point)| button == MouseButton::Left && rect.contains(point))
        {
            chosen = Some(*choice);
        }
    }
    if let Some(choice) = chosen {
        panels.project.context_menu = None;
        match choice {
            Choice::Create => {
                browser.popup = Some(Popup::name(catalogue, None, browser.view.folder, locale))
            }
            Choice::Rename => {
                if let Some(RowKey::Folder(id)) = browser.context {
                    browser.popup = Some(Popup::name(
                        catalogue,
                        Some(id),
                        browser.view.folder,
                        locale,
                    ));
                }
            }
            Choice::Move => {
                browser.popup = browser
                    .context
                    .clone()
                    .map(|key| Popup::moving(catalogue, key))
            }
            Choice::Remove => {
                if let Some(RowKey::Folder(id)) = browser.context {
                    command(
                        out,
                        catalogue.source.clone(),
                        LibraryAction::RemoveFolder { id },
                    );
                }
            }
            Choice::Open => {
                if let Some(key) = browser.context.clone() {
                    let before = browser.view.clone();
                    activate(browser, &key, _state, out);
                    if before != browser.view {
                        out.push(Action::ProjectViewChanged(browser.view.clone()));
                    }
                }
            }
            Choice::Import => out.push(Action::ProjectImport {
                source: catalogue.source.clone(),
                folder: match browser.context {
                    Some(RowKey::Folder(id)) => Some(id),
                    _ => browser.view.folder,
                },
                path: None,
            }),
            Choice::Settings => {
                if let Some(RowKey::Item(ProjectItem::Composition { id })) = &browser.context {
                    if _state["project"]["composition_id"] == *id {
                        out.push(Action::CompositionSettings);
                    } else {
                        out.push(Action::ProjectOpen {
                            source: catalogue.source.clone(),
                            composition: id.clone(),
                        });
                    }
                }
            }
        }
    }
}
