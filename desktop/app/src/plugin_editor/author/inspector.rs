use super::*;
impl Designer {
    pub(super) fn inspector(
        &mut self,
        draft: &mut model::Draft,
        preview: &mut Preview,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        zh: bool,
        owner: Option<String>,
    ) -> Result<(), String> {
        let sheet = list.styles();
        let row = sheet.metric("row.height", 22.);
        let pad = sheet.metric("padding", 5.);
        let Some(id) = self
            .selected
            .clone()
            .or_else(|| Some(draft.project.interface.root.id.clone()))
        else {
            return Ok(());
        };
        let Some(n) = find(&draft.project.interface.root, &id).cloned() else {
            self.selected = None;
            return Ok(());
        };
        let disabled = neutral();
        let command = if self.busy() { &disabled } else { input };
        ui::text(
            list,
            atlas,
            Rect::new(area.min[0], area.min[1], area.width(), row),
            &format!("{} · {id}", tr(zh, "布局检查器", "Layout inspector")),
            sheet.color("muted"),
            Align::Left,
        );
        let mut y = area.min[1] + row;
        let value_x = area.min[0] + area.width() * 0.4;
        for (key, label, extent) in [
            ("width", tr(zh, "宽度", "Width"), n.width),
            ("height", tr(zh, "高度", "Height"), n.height),
        ] {
            ui::text(
                list,
                atlas,
                Rect::new(area.min[0], y, value_x - area.min[0], row),
                &label,
                sheet.color("text"),
                Align::Left,
            );
            let cell = sheet.metric("effect.number.cell_width", 44.);
            let menu = Rect::new(
                value_x,
                y,
                (area.max[0] - value_x - cell - pad).max(0.),
                row,
            );
            let current = match extent {
                LayoutExtent::Auto => 0,
                LayoutExtent::Points { .. } => 1,
                LayoutExtent::Weight { .. } => 2,
            };
            if let Some(next) = self.menu(
                key,
                list,
                atlas,
                menu,
                area,
                &[
                    tr(zh, "自动", "Auto"),
                    tr(zh, "像素", "Points"),
                    tr(zh, "权重", "Weight"),
                ],
                current,
                input,
                &owner,
            ) {
                draft.edit(|p| {
                    let n = find_mut(&mut p.interface.root, &id).ok_or("component missing")?;
                    let e = match next {
                        1 => LayoutExtent::Points {
                            value: sheet.metric("plugin.preview.row.min_width", 288.),
                        },
                        2 => LayoutExtent::Weight { value: 1. },
                        _ => LayoutExtent::Auto,
                    };
                    if key == "width" {
                        n.width = e
                    } else {
                        n.height = e
                    };
                    Ok(())
                })?;
            }
            if let LayoutExtent::Points { value } | LayoutExtent::Weight { value } = extent {
                let nr = Rect::new(menu.max[0] + pad, y, cell, row);
                for change in self.number(
                    &format!("{key}-value"),
                    list,
                    atlas,
                    nr,
                    value,
                    0.01,
                    8192.,
                    input,
                    &owner,
                ) {
                    match change {
                        NumberEvent::Begin { .. } => draft.begin(),
                        NumberEvent::Change { value } | NumberEvent::Commit { value } => {
                            draft.update(|p| {
                                let n = find_mut(&mut p.interface.root, &id)
                                    .ok_or("component missing")?;
                                let e = if current == 1 {
                                    LayoutExtent::Points {
                                        value: value as f32,
                                    }
                                } else {
                                    LayoutExtent::Weight {
                                        value: value as f32,
                                    }
                                };
                                if key == "width" {
                                    n.width = e
                                } else {
                                    n.height = e
                                };
                                Ok(())
                            })?;
                            if matches!(change, NumberEvent::Commit { .. }) {
                                draft.commit();
                            }
                        }
                        NumberEvent::Cancel { .. } => draft.cancel(),
                    }
                }
            }
            y += row;
        }
        let title = match &n.content {
            LayoutContent::Group { title, .. } => Some(title.clone()),
            LayoutContent::Note { text } => Some(text.clone()),
            _ => None,
        };
        if let Some(title) = title {
            let r = Rect::new(value_x, y, area.max[0] - value_x, row);
            ui::text(
                list,
                atlas,
                Rect::new(area.min[0], y, value_x - area.min[0], row),
                &tr(zh, "文本", "Text"),
                sheet.color("text"),
                Align::Left,
            );
            if self.text.is_none() && press(command, r) {
                let mut editor = TextEditor::default();
                editor.begin(&title);
                self.text = Some((id.clone(), editor));
            }
            if let Some((owner, editor)) = &mut self.text {
                if owner == &id {
                    match editor.input(input) {
                        Some(TextEvent::Commit(value)) => {
                            draft.edit(|p| {
                                match &mut find_mut(&mut p.interface.root, &id)
                                    .ok_or("component missing")?
                                    .content
                                {
                                    LayoutContent::Group { title, .. } => *title = value,
                                    LayoutContent::Note { text } => *text = value,
                                    _ => {}
                                }
                                Ok(())
                            })?;
                            self.text = None;
                        }
                        Some(TextEvent::Cancel) => self.text = None,
                        _ => {}
                    }
                }
            }
            if let Some((_, editor)) = &self.text {
                editor.draw_styled(list, atlas, r, &sheet, "field", Default::default());
            } else {
                ui::text(list, atlas, r, &title, sheet.color("text"), Align::Left);
            }
            y += row;
        }
        match n.content {
            LayoutContent::Row { gap, align, .. } | LayoutContent::Column { gap, align, .. } => {
                for (key, label, current, labels) in [
                    (
                        "gap",
                        tr(zh, "间距", "Gap"),
                        match gap {
                            LayoutSpacing::None => 0,
                            LayoutSpacing::Compact => 1,
                            LayoutSpacing::Normal => 2,
                            LayoutSpacing::Relaxed => 3,
                        },
                        vec![
                            tr(zh, "无", "None"),
                            tr(zh, "紧凑", "Compact"),
                            tr(zh, "标准", "Normal"),
                            tr(zh, "宽松", "Relaxed"),
                        ],
                    ),
                    (
                        "align",
                        tr(zh, "对齐", "Align"),
                        match align {
                            LayoutAlign::Start => 0,
                            LayoutAlign::Center => 1,
                            LayoutAlign::End => 2,
                            LayoutAlign::Stretch => 3,
                        },
                        vec![
                            tr(zh, "起始", "Start"),
                            tr(zh, "居中", "Center"),
                            tr(zh, "末端", "End"),
                            tr(zh, "拉伸", "Stretch"),
                        ],
                    ),
                ] {
                    ui::text(
                        list,
                        atlas,
                        Rect::new(area.min[0], y, value_x - area.min[0], row),
                        &label,
                        sheet.color("text"),
                        Align::Left,
                    );
                    if let Some(next) = self.menu(
                        key,
                        list,
                        atlas,
                        Rect::new(value_x, y, area.max[0] - value_x, row),
                        area,
                        &labels,
                        current,
                        input,
                        &owner,
                    ) {
                        draft.edit(|p| {
                            match &mut find_mut(&mut p.interface.root, &id)
                                .ok_or("component missing")?
                                .content
                            {
                                LayoutContent::Row { gap, align, .. }
                                | LayoutContent::Column { gap, align, .. } => {
                                    if key == "gap" {
                                        *gap = [
                                            LayoutSpacing::None,
                                            LayoutSpacing::Compact,
                                            LayoutSpacing::Normal,
                                            LayoutSpacing::Relaxed,
                                        ][next]
                                    } else {
                                        *align = [
                                            LayoutAlign::Start,
                                            LayoutAlign::Center,
                                            LayoutAlign::End,
                                            LayoutAlign::Stretch,
                                        ][next]
                                    }
                                }
                                _ => {}
                            }
                            Ok(())
                        })?;
                    }
                    y += row;
                }
            }
            LayoutContent::Parameter { parameter, .. } => {
                let mut parameters = draft.project.parameters().map_err(|e| e.to_string())?;
                let mut labels: Vec<_> = parameters
                    .iter()
                    .map(|p| format!("{} · {}", p.name, p.id))
                    .collect();
                let current = parameters
                    .iter()
                    .position(|p| p.id == parameter)
                    .unwrap_or_else(|| {
                        labels.push(format!("Missing: {parameter}"));
                        labels.len() - 1
                    });
                ui::text(
                    list,
                    atlas,
                    Rect::new(area.min[0], y, value_x - area.min[0], row),
                    &tr(zh, "绑定", "Binding"),
                    sheet.color("text"),
                    Align::Left,
                );
                if let Some(next) = self.menu(
                    "binding",
                    list,
                    atlas,
                    Rect::new(value_x, y, area.max[0] - value_x, row),
                    area,
                    &labels,
                    current,
                    input,
                    &owner,
                ) {
                    if next < parameters.len() {
                        let p = parameters.remove(next);
                        draft.edit(|draft| {
                            find_mut(&mut draft.interface.root, &id)
                                .ok_or("component missing")?
                                .content = LayoutContent::Parameter {
                                parameter: p.id,
                                control: ParameterControl::Auto,
                            };
                            Ok(())
                        })?;
                    }
                }
                y += row;
            }
            LayoutContent::Tabs { tabs } => {
                let active = *preview.view.tabs.get(&id).unwrap_or(&0);
                for (i, tab) in tabs.iter().enumerate() {
                    let r = Rect::new(area.min[0], y, area.width(), row);
                    if ui::button(list, atlas, r, &tab.title, command, true).clicked {
                        preview.view.tabs.insert(id.clone(), i);
                    }
                    y += row;
                }
                let half = (area.width() - pad) * 0.5;
                if ui::button(
                    list,
                    atlas,
                    Rect::new(area.min[0], y, half, row),
                    &tr(zh, "添加标签页", "Add tab"),
                    command,
                    true,
                )
                .clicked
                {
                    draft.edit(|p| {
                        let fresh = model::fresh_id(&p.interface.root, "tab");
                        let child = LayoutNode {
                            id: format!("{fresh}-page"),
                            width: LayoutExtent::Auto,
                            height: LayoutExtent::Auto,
                            content: LayoutContent::Column {
                                gap: LayoutSpacing::None,
                                align: LayoutAlign::Stretch,
                                children: vec![],
                            },
                        };
                        if let LayoutContent::Tabs { tabs } =
                            &mut find_mut(&mut p.interface.root, &id)
                                .ok_or("tabs missing")?
                                .content
                        {
                            tabs.push(LayoutTab {
                                id: fresh,
                                title: tr(zh, "新标签页", "New tab"),
                                child,
                            });
                        }
                        Ok(())
                    })?;
                }
                if ui::button(
                    list,
                    atlas,
                    Rect::new(area.min[0] + half + pad, y, half, row),
                    &tr(zh, "删除空页", "Remove empty"),
                    command,
                    tabs.len() > 1
                        && tabs
                            .get(active)
                            .is_some_and(|t| t.child.children().count() == 0),
                )
                .clicked
                {
                    draft.edit(|p| {
                        if let LayoutContent::Tabs { tabs } =
                            &mut find_mut(&mut p.interface.root, &id)
                                .ok_or("tabs missing")?
                                .content
                        {
                            tabs.remove(active);
                        }
                        Ok(())
                    })?;
                    preview.view.tabs.insert(id.clone(), 0);
                }
                y += row;
            }
            LayoutContent::Split { axis, ratio, .. } => {
                if let Some(next) = self.menu(
                    "axis",
                    list,
                    atlas,
                    Rect::new(area.min[0], y, area.width(), row),
                    area,
                    &[
                        tr(zh, "水平分栏", "Horizontal split"),
                        tr(zh, "垂直分栏", "Vertical split"),
                    ],
                    usize::from(axis == LayoutAxis::Vertical),
                    input,
                    &owner,
                ) {
                    draft.edit(|p| {
                        if let LayoutContent::Split { axis, .. } =
                            &mut find_mut(&mut p.interface.root, &id)
                                .ok_or("split missing")?
                                .content
                        {
                            *axis = if next == 0 {
                                LayoutAxis::Horizontal
                            } else {
                                LayoutAxis::Vertical
                            };
                        }
                        Ok(())
                    })?;
                }
                y += row;
                ui::text(
                    list,
                    atlas,
                    Rect::new(area.min[0], y, value_x - area.min[0], row),
                    &tr(zh, "分栏比例", "Split ratio"),
                    sheet.color("text"),
                    Align::Left,
                );
                for change in self.number(
                    "ratio",
                    list,
                    atlas,
                    Rect::new(value_x, y, area.max[0] - value_x, row),
                    ratio,
                    0.05,
                    0.95,
                    input,
                    &owner,
                ) {
                    match change {
                        NumberEvent::Begin { .. } => draft.begin(),
                        NumberEvent::Change { value } | NumberEvent::Commit { value } => {
                            draft.update(|p| {
                                if let LayoutContent::Split { ratio, .. } =
                                    &mut find_mut(&mut p.interface.root, &id)
                                        .ok_or("split missing")?
                                        .content
                                {
                                    *ratio = value as f32;
                                }
                                Ok(())
                            })?;
                            if matches!(change, NumberEvent::Commit { .. }) {
                                draft.commit();
                            }
                        }
                        NumberEvent::Cancel { .. } => draft.cancel(),
                    }
                }
                y += row;
            }
            _ => {}
        }
        if ui::button(
            list,
            atlas,
            Rect::new(area.min[0], y + pad, area.width(), row),
            &tr(zh, "删除组件", "Remove component"),
            command,
            id != draft.project.interface.root.id,
        )
        .clicked
        {
            draft.edit(|p| model::delete(p, &id))?;
            self.selected = None;
        }
        Ok(())
    }
}
