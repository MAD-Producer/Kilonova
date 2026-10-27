//! One task per row, with immutable source details and explicit queue actions.
use super::*;
use crate::render_queue::{
    controller::Intent,
    model::{Document, State as JobState, Template},
};
use motion_ui::widgets::{ScrollArea, TextEditor, TextEvent};

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Name,
    Pattern,
}
#[derive(Default)]
pub(crate) struct Controls {
    selected: Option<u64>,
    last_added: Option<u64>,
    template: Option<u64>,
    templates: Dropdown,
    scope: Dropdown,
    scroll: ScrollArea,
    draft: Option<Template>,
    editor: TextEditor,
    field: Option<Field>,
    field_rect: Option<Rect>,
    error: Option<String>,
    content_height: f32,
    #[cfg(test)]
    pub(super) task_rows: Vec<(u64, Rect)>,
    #[cfg(test)]
    pub(super) actions: Vec<(&'static str, Rect)>,
    #[cfg(test)]
    pub(super) fields: Vec<Rect>,
    #[cfg(test)]
    pub(super) details: Vec<Rect>,
}
impl Controls {
    pub fn editing(&self) -> bool {
        self.templates.open || self.scope.open || self.editor.active()
    }
    pub fn cancel_field(&mut self) {
        self.editor.cancel();
        self.field = None;
        self.templates.open = false;
        self.scope.open = false;
    }
    pub fn text_menu(&mut self, request: &mut super::super::text_menu::Request<'_>) {
        request.visit(&mut self.editor);
    }
    fn commit_field(&mut self) {
        if let (Some(draft), Some(field)) = (&mut self.draft, self.field) {
            match field {
                Field::Name => draft.name = self.editor.text.clone(),
                Field::Pattern => draft.pattern = self.editor.text.clone(),
            }
        }
        self.editor.cancel();
        self.field = None;
    }
}
fn grid_columns(width: f32, gap: f32, minimum: f32, count: usize) -> usize {
    (((width + gap) / (minimum + gap)).floor() as usize).clamp(1, count.max(1))
}
fn grid_height(width: f32, gap: f32, minimum: f32, control: f32, count: usize) -> f32 {
    let columns = grid_columns(width, gap, minimum, count);
    count.div_ceil(columns) as f32 * (control + gap)
}
fn grid(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    y: &mut f32,
    input: &Input,
    items: Vec<(&str, bool, Action)>,
    controls: &mut Controls,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let gap = styles.metric("auxiliary.gap", 0.);
    let control = styles
        .metric("auxiliary.control.height", 0.)
        .max(styles.metric("row.height", 0.));
    let minimum = items
        .iter()
        .map(|(text, _, _)| atlas.measure(text) + styles.metric("padding.panel", 0.) * 2.)
        .fold(styles.metric("text.style.action.width", 0.), f32::max);
    let columns = grid_columns(area.width(), gap, minimum, items.len());
    let width = ((area.width() - (columns - 1) as f32 * gap) / columns as f32)
        .max(0.)
        .min(minimum);
    let count = items.len();
    for (index, (text, enabled, action)) in items.into_iter().enumerate() {
        let rect = Rect::new(
            area.min[0] + (index % columns) as f32 * (width + gap),
            *y + (index / columns) as f32 * (control + gap),
            width,
            control,
        );
        #[cfg(test)]
        if let Action::RenderQueue(intent) = &action {
            let name = match intent {
                Intent::Add(_) => "add",
                Intent::Start => "start",
                Intent::Pause => "pause",
                Intent::Reload => "reload",
                Intent::Cancel(_) => "cancel",
                Intent::Retry(_) => "retry",
                Intent::RetryAs(_) => "retry_as",
                Intent::Remove(_) => "remove",
                _ => "other",
            };
            controls.actions.push((name, rect));
        }
        let _ = &controls;
        button(list, atlas, rect, text, input, enabled, action, out);
    }
    *y += grid_height(area.width(), gap, minimum, control, count);
}
fn state_label(state: &JobState, locale: Locale) -> &'static str {
    let t = |zh, en| localized(locale, zh, en);
    match state {
        JobState::Queued => t("待渲染", "Queued"),
        JobState::Running => t("渲染中", "Rendering"),
        JobState::Cancelling => t("正在取消", "Cancelling"),
        JobState::Succeeded { .. } => t("已完成", "Done"),
        JobState::Cancelled => t("已取消", "Cancelled"),
        JobState::Failed { .. } => t("失败", "Failed"),
        JobState::Interrupted => t("已中断", "Interrupted"),
        JobState::Removing => t("正在移除", "Removing"),
    }
}

fn tooltip(list: &mut PaintList, atlas: &mut TextAtlas, area: Rect, point: [f32; 2], text: &str) {
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 0.);
    let row = styles.metric("row.height", 0.);
    let gap = styles.metric("auxiliary.gap", 0.);
    if row <= 0. {
        return;
    }
    let width = (area.width() - pad * 2.).max(0.);
    let text_width = (width - pad * 2.).max(0.);
    let count = ((area.height() - pad * 2.) / row).floor().max(0.) as usize;
    if text_width <= 0. || count == 0 {
        return;
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    for character in text.chars() {
        let mut next = line.clone();
        next.push(character);
        if character == '\n' || !line.is_empty() && atlas.measure(&next) > text_width {
            lines.push(std::mem::take(&mut line));
            if lines.len() == count {
                break;
            }
        }
        if character != '\n' {
            line.push(character);
        }
    }
    if !line.is_empty() && lines.len() < count {
        lines.push(line);
    }
    let height = lines.len() as f32 * row + pad * 2.;
    let rect = Rect::new(
        (point[0] + gap).min(area.max[0] - width).max(area.min[0]),
        (point[1] + row).min(area.max[1] - height).max(area.min[1]),
        width,
        height,
    );
    let style = styles
        .resolve("tooltip", States::default())
        .expect("native tooltip style");
    box_style(list, rect, style);
    for (index, text) in lines.into_iter().enumerate() {
        text_style(
            list,
            atlas,
            Rect::new(
                rect.min[0] + pad,
                rect.min[1] + pad + index as f32 * row,
                text_width,
                row,
            ),
            &text,
            style,
            Align::Left,
        );
    }
}

pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    project: &Value,
    original: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();
    let pad = styles.metric("padding.panel", 0.);
    let gap = styles.metric("auxiliary.gap", 0.);
    let row = styles.metric("row.height", 0.);
    let control = styles.metric("auxiliary.control.height", 0.).max(row);
    let action_width = styles.metric("text.style.action.width", 0.);
    let x = area.min[0] + pad;
    let width = (area.width() - pad * 2.).max(0.);
    let locale = panels.locale;
    let t = |zh, en| localized(locale, zh, en);
    let projection = panels.render_queue.clone();
    let mut controls = std::mem::take(&mut panels.auxiliary.render_queue);
    #[cfg(test)]
    {
        controls.task_rows.clear();
        controls.actions.clear();
        controls.fields.clear();
        controls.details.clear();
    }
    if original.focus_lost {
        controls.cancel_field();
    }
    if controls.editor.active() {
        match controls.editor.input(original) {
            Some(TextEvent::Commit(_)) => controls.commit_field(),
            Some(TextEvent::Cancel) => controls.cancel_field(),
            None => {
                if !original.composing
                    && !original.ime_consumed
                    && original.pressed.is_some_and(|(button, point)| {
                        button == MouseButton::Left
                            && controls
                                .field_rect
                                .is_some_and(|rect| !rect.contains(point))
                    })
                {
                    controls.commit_field();
                }
            }
        }
    }
    let neutral = Input::default();
    let popup_input = if projection.pending {
        &neutral
    } else {
        original
    };
    let input = if controls.templates.open
        || controls.scope.open
        || controls.editor.active()
        || panels.auxiliary.export_scope.open
    {
        &neutral
    } else {
        original
    };
    let fallback = Document::default();
    let document = projection
        .view
        .as_ref()
        .map_or(&fallback, |view| view.document.as_ref());
    if controls.last_added != projection.last_added {
        controls.selected = projection.last_added;
        controls.last_added = projection.last_added;
    }
    if !document
        .jobs
        .iter()
        .any(|job| Some(job.id) == controls.selected)
    {
        controls.selected = document.jobs.first().map(|job| job.id);
    }
    let selected = document
        .jobs
        .iter()
        .find(|job| Some(job.id) == controls.selected);
    if !document
        .templates
        .iter()
        .any(|template| Some(template.id) == controls.template)
    {
        controls.template = document.templates.first().map(|template| template.id);
    }
    let template = document
        .templates
        .iter()
        .find(|template| Some(template.id) == controls.template);
    let ready = projection
        .view
        .as_ref()
        .is_some_and(|view| view.ready && !view.stopped)
        && !projection.retiring;
    let idle = ready && !projection.pending;
    let can_render = cfg!(feature = "ffmpeg") && project["observing"] != true;
    let capable = can_render
        && project["project"]["width"]
            .as_u64()
            .is_some_and(|n| n > 0 && n % 2 == 0)
        && project["project"]["height"]
            .as_u64()
            .is_some_and(|n| n > 0 && n % 2 == 0);
    let status = if projection.retiring {
        t("正在结束旧工程任务…", "Finishing the previous project…").into()
    } else if let Some(view) = &projection.view {
        if let Some(id) = view.active_job {
            format!(
                "#{id} · {} {:.0}%",
                t("渲染", "Rendering"),
                view.progress * 100.
            )
        } else if view.stopped {
            t("队列已停止", "Queue stopped").into()
        } else {
            format!(
                "{} · {} {}",
                t("渲染队列", "Render queue"),
                document.jobs.len(),
                t("项", "tasks")
            )
        }
    } else {
        t("正在读取队列…", "Opening render queue…").into()
    };
    let mut y = area.min[1] + pad;
    label(list, atlas, Rect::new(x, y, width, row), &status, false);
    y += row;
    let running = projection.view.as_ref().is_some_and(|view| view.running);
    grid(
        list,
        atlas,
        Rect::new(x, y, width, 0.),
        &mut y,
        input,
        vec![
            (
                t("加入队列", "Add task"),
                idle && capable && template.is_some(),
                Action::RenderQueue(Intent::Add(
                    template
                        .cloned()
                        .unwrap_or_else(|| fallback.templates[0].clone()),
                )),
            ),
            (
                t("开始", "Start"),
                idle && can_render
                    && !running
                    && document
                        .jobs
                        .iter()
                        .any(|job| job.state == JobState::Queued),
                Action::RenderQueue(Intent::Start),
            ),
            (
                t("暂停", "Pause"),
                ready && running,
                Action::RenderQueue(Intent::Pause),
            ),
        ],
        &mut controls,
        out,
    );
    let body = Rect::new(x, y, width, (area.max[1] - y - pad).max(0.));
    let rows = document.jobs.len().max(1)
        + if selected.is_some() { 7 } else { 0 }
        + if controls.draft.is_some() { 6 } else { 0 }
        + 10;
    let height = if controls.content_height > 0. {
        controls.content_height
    } else {
        rows as f32 * (row + gap) + grid_height(width, gap, action_width, control, 4) * 2.
    };
    let scroll = controls.scroll.begin(list, body, height, input, true);
    let mut child = list.child();
    let viewport = scroll.viewport;
    let body_input = if viewport.contains(input.mouse) {
        &scroll.input
    } else {
        &neutral
    };
    let input = body_input;
    let mut note = None;
    y = scroll.content.min[1];
    let value_x = viewport.min[0]
        + styles
            .metric("label.column", 0.)
            .min(viewport.width() * 0.4);
    let template_rect = Rect::new(value_x, y, (viewport.max[0] - value_x).max(0.), control);
    label(
        &mut child,
        atlas,
        Rect::new(
            viewport.min[0],
            y,
            (value_x - viewport.min[0]).max(0.),
            control,
        ),
        t("输出模板", "Template"),
        true,
    );
    y += control + gap;
    for job in &document.jobs {
        let rect = Rect::new(viewport.min[0], y, viewport.width(), row);
        #[cfg(test)]
        controls.task_rows.push((job.id, rect));
        let mut states = States::default();
        if Some(job.id) == controls.selected {
            states = states.with(motion_ui::style::State::Selected);
        } else if rect.contains(input.mouse) {
            states = states.with(motion_ui::style::State::Hovered);
        }
        let style = styles.resolve("row", states).expect("native row style");
        box_style(&mut child, rect, style);
        let text = format!(
            "#{}  {}  ·  {}",
            job.id,
            job.source.name,
            state_label(&job.state, locale)
        );
        text_style(
            &mut child,
            atlas,
            rect.shrink(styles.metric("border", 0.)),
            &text,
            style,
            Align::Left,
        );
        if rect.contains(input.mouse) && atlas.measure(&text) > rect.width() {
            note = Some(text);
        }
        if clicked(input, rect) {
            controls.selected = Some(job.id);
        }
        y += row;
    }
    if document.jobs.is_empty() {
        label(
            &mut child,
            atlas,
            Rect::new(viewport.min[0], y, viewport.width(), row),
            t(
                "尚无任务；加入后保存冻结输入",
                "No tasks; adding saves frozen inputs",
            ),
            true,
        );
        y += row;
    }
    if let Some(job) = selected {
        for text in [
            format!(
                "#{} · {} × {} · {} fps · r{}",
                job.id, job.source.width, job.source.height, job.source.fps, job.source.revision
            ),
            format!(
                "{} [{} , {}) · {} {}",
                t("帧范围", "Frames"),
                job.source.range.in_frame,
                job.source.range.out_frame,
                t("尝试", "Attempt"),
                job.attempt
            ),
            job.output.display().to_string(),
        ] {
            let rect = Rect::new(viewport.min[0], y, viewport.width(), row);
            #[cfg(test)]
            controls.details.push(rect);
            label(&mut child, atlas, rect, &text, true);
            if rect.contains(input.mouse) && atlas.measure(&text) > rect.width() {
                note = Some(text);
            }
            y += row;
        }
        if let JobState::Failed { cause } = &job.state {
            let rect = Rect::new(viewport.min[0], y, viewport.width(), row);
            label(&mut child, atlas, rect, cause, false);
            if rect.contains(input.mouse) && atlas.measure(cause) > rect.width() {
                note = Some(cause.clone());
            }
            y += row;
        }
        grid(
            &mut child,
            atlas,
            viewport,
            &mut y,
            input,
            vec![
                (
                    t("取消任务", "Cancel task"),
                    ready && (job.state.active() || job.state == JobState::Queued),
                    Action::RenderQueue(Intent::Cancel(job.id)),
                ),
                (
                    t("重试", "Retry"),
                    idle && job.state.retryable(),
                    Action::RenderQueue(Intent::Retry(job.id)),
                ),
                (
                    t("另存重试", "Retry as…"),
                    idle && job.state.retryable(),
                    Action::RenderQueue(Intent::RetryAs(job.id)),
                ),
                (
                    t("移除任务", "Remove"),
                    idle && !job.state.active(),
                    Action::RenderQueue(Intent::Remove(job.id)),
                ),
            ],
            &mut controls,
            out,
        );
    }
    if projection.pending {
        label(
            &mut child,
            atlas,
            Rect::new(viewport.min[0], y, viewport.width(), row),
            t("正在保存任务或模板…", "Saving task or template…"),
            true,
        );
        y += row;
        button(
            &mut child,
            atlas,
            Rect::new(viewport.min[0], y, viewport.width(), control),
            t("取消准备", "Cancel preparation"),
            input,
            true,
            Action::RenderQueue(Intent::CancelPreparation),
            out,
        );
        y += control + gap;
    }
    if let Some(error) = controls
        .error
        .as_deref()
        .or(projection.error.as_deref())
        .or_else(|| projection.view.as_ref().and_then(|v| v.error.as_deref()))
    {
        label(
            &mut child,
            atlas,
            Rect::new(viewport.min[0], y, viewport.width(), row),
            error,
            false,
        );
        y += row;
    }
    if !capable {
        let reason = if !cfg!(feature = "ffmpeg") {
            t("此版本未启用 FFmpeg", "This build does not include FFmpeg")
        } else if project["observing"] == true {
            t("当前工程只读", "The current project is read-only")
        } else {
            t(
                "加入任务需要当前合成宽高为偶数",
                "Adding requires even dimensions in the current composition",
            )
        };
        label(
            &mut child,
            atlas,
            Rect::new(viewport.min[0], y, viewport.width(), row),
            reason,
            true,
        );
        y += row;
    }
    let edit_width = viewport.width().min(action_width * 2. + gap);
    let edit_template_rect = Rect::new(viewport.min[0], y, edit_width, control);
    #[cfg(test)]
    controls.actions.push(("edit_template", edit_template_rect));
    if ui::button(
        &mut child,
        atlas,
        edit_template_rect,
        t("编辑模板", "Edit template"),
        input,
        idle && template.is_some(),
    )
    .clicked
    {
        controls.draft = template.cloned();
        controls.error = None;
    }
    let next_line = viewport.width() < edit_width + gap + action_width;
    let new_rect = Rect::new(
        if next_line {
            viewport.min[0]
        } else {
            viewport.min[0] + edit_width + gap
        },
        y + if next_line { control + gap } else { 0. },
        action_width.min(viewport.width()),
        control,
    );
    #[cfg(test)]
    controls.actions.push(("new_template", new_rect));
    if ui::button(
        &mut child,
        atlas,
        new_rect,
        t("新建模板", "New preset"),
        input,
        idle,
    )
    .clicked
    {
        controls.draft = Some(Template {
            id: 0,
            name: t("新模板", "New preset").into(),
            format: crate::render_queue::model::Format::Mp4H264Aac,
            scope: motion_core::FrameScope::Composition,
            pattern: "{composition}-{id}.mp4".into(),
        });
        controls.error = None;
    }
    if next_line {
        y += control + gap;
    }
    y += control + gap;
    let mut scope_rect = None;
    if controls.draft.is_some() {
        for (field, caption) in [
            (Field::Name, t("名称", "Name")),
            (Field::Pattern, t("文件名", "Filename")),
        ] {
            label(
                &mut child,
                atlas,
                Rect::new(viewport.min[0], y, (value_x - viewport.min[0]).max(0.), row),
                caption,
                true,
            );
            let rect = Rect::new(value_x, y, (viewport.max[0] - value_x).max(0.), row);
            #[cfg(test)]
            controls.fields.push(rect);
            if controls.field == Some(field) && controls.editor.active() {
                controls.field_rect = Some(rect);
                controls.editor.draw(&mut child, atlas, rect);
            } else {
                let draft = controls.draft.as_ref().expect("template draft");
                let text = match field {
                    Field::Name => &draft.name,
                    Field::Pattern => &draft.pattern,
                };
                label(&mut child, atlas, rect, text, false);
                if idle && clicked(input, rect) {
                    controls.editor.begin_bounded(text, 512);
                    controls.field = Some(field);
                    controls.field_rect = Some(rect);
                }
            }
            y += row;
        }
        label(
            &mut child,
            atlas,
            Rect::new(
                viewport.min[0],
                y,
                (value_x - viewport.min[0]).max(0.),
                control,
            ),
            t("范围", "Range"),
            true,
        );
        scope_rect = Some(Rect::new(
            value_x,
            y,
            (viewport.max[0] - value_x).max(0.),
            control,
        ));
        y += control + gap;
        label(
            &mut child,
            atlas,
            Rect::new(viewport.min[0], y, viewport.width(), row),
            "{composition} · {id} · {revision} · MP4 / H.264 / AAC",
            true,
        );
        y += row;
        let mut save = None;
        let count = 3;
        let columns = grid_columns(viewport.width(), gap, action_width, count);
        let w = ((viewport.width() - (columns - 1) as f32 * gap) / columns as f32)
            .max(0.)
            .min(action_width * 2.);
        for (i, text) in [
            t("保存模板", "Save"),
            t("另存模板", "Save as new"),
            t("取消编辑", "Cancel edit"),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect::new(
                viewport.min[0] + (i % columns) as f32 * (w + gap),
                y + (i / columns) as f32 * (control + gap),
                w,
                control,
            );
            #[cfg(test)]
            if i == 0 {
                controls.actions.push(("save_template", rect));
            }
            if ui::button(&mut child, atlas, rect, text, input, idle).clicked {
                save = Some(i);
            }
        }
        y += grid_height(viewport.width(), gap, action_width, control, count);
        if let Some(which) = save {
            controls.commit_field();
            if which == 2 {
                controls.draft = None;
                controls.error = None;
            } else {
                let mut draft = controls.draft.clone().expect("template draft");
                let mut validation = draft.clone();
                if validation.id == 0 {
                    validation.id = document.next_template;
                }
                match validation.validate() {
                    Ok(()) => {
                        if which == 1 {
                            draft.id = 0;
                        }
                        out.push(Action::RenderQueue(Intent::SaveTemplate(draft)));
                        controls.error = None;
                    }
                    Err(cause) => controls.error = Some(cause),
                }
            }
        }
    }
    grid(
        &mut child,
        atlas,
        viewport,
        &mut y,
        input,
        vec![
            (
                t("删除模板", "Delete template"),
                idle && template.is_some(),
                Action::RenderQueue(Intent::RemoveTemplate(template.map_or(0, |t| t.id))),
            ),
            (
                t("重读队列", "Reload queue"),
                !projection.pending
                    && !projection.retiring
                    && projection.view.as_ref().is_some_and(|view| {
                        view.stopped || (view.ready && view.active_job.is_none())
                    }),
                Action::RenderQueue(Intent::Reload),
            ),
        ],
        &mut controls,
        out,
    );
    let border = styles.metric("border", 0.);
    paint::rect(
        &mut child,
        Rect::new(viewport.min[0], y, viewport.width(), border),
        colors.BORDER,
    );
    y += gap + border;
    label(
        &mut child,
        atlas,
        Rect::new(viewport.min[0], y, viewport.width(), row),
        t("单次导出", "Single export"),
        true,
    );
    y += row;
    label(
        &mut child,
        atlas,
        Rect::new(
            viewport.min[0],
            y,
            (value_x - viewport.min[0]).max(0.),
            control,
        ),
        t("MP4 范围", "MP4 range"),
        true,
    );
    let export_rect = Rect::new(value_x, y, (viewport.max[0] - value_x).max(0.), control);
    #[cfg(test)]
    {
        panels.auxiliary.movie_scope_rect = Some(export_rect);
    }
    y += control + gap;
    grid(
        &mut child,
        atlas,
        viewport,
        &mut y,
        input,
        vec![
            ("PNG", true, Action::ExportPng),
            ("MP4", cfg!(feature = "ffmpeg"), Action::ExportMovie),
            (t("打包", "Package"), true, Action::Pack),
            (t("取消导出", "Cancel export"), true, Action::CancelExport),
        ],
        &mut controls,
        out,
    );
    if let Some(message) = &panels.message {
        label(
            &mut child,
            atlas,
            Rect::new(viewport.min[0], y, viewport.width(), row),
            message,
            true,
        );
        y += row;
    }
    controls.content_height = y - scroll.content.min[1];
    child.clip_to(viewport);
    list.append(child);
    // Popups paint last, above the body clip, and exclusively own input.
    let items: Vec<_> = document
        .templates
        .iter()
        .map(|t| DropdownItem {
            label: t.name.as_str(),
            enabled: true,
        })
        .collect();
    let index = document
        .templates
        .iter()
        .position(|t| Some(t.id) == controls.template)
        .unwrap_or(0);
    if template_rect.min[1] >= viewport.min[1] && template_rect.max[1] <= viewport.max[1] {
        if let Some(index) = controls.templates.draw(
            list,
            atlas,
            template_rect,
            body,
            &items,
            index,
            popup_input,
            idle && !controls.scope.open && !panels.auxiliary.export_scope.open,
            false,
        ) {
            controls.template = document.templates.get(index).map(|t| t.id);
            controls.draft = None;
            controls.cancel_field();
        }
    }
    let scopes = [
        t("整段合成", "Full composition"),
        t("工作范围", "Work area"),
    ]
    .map(|label| DropdownItem {
        label,
        enabled: true,
    });
    if let (Some(rect), Some(draft)) = (scope_rect, &mut controls.draft) {
        if rect.min[1] >= viewport.min[1] && rect.max[1] <= viewport.max[1] {
            if let Some(index) = controls.scope.draw(
                list,
                atlas,
                rect,
                body,
                &scopes,
                usize::from(draft.scope == motion_core::FrameScope::WorkArea),
                popup_input,
                idle && !controls.templates.open && !panels.auxiliary.export_scope.open,
                false,
            ) {
                draft.scope = if index == 0 {
                    motion_core::FrameScope::Composition
                } else {
                    motion_core::FrameScope::WorkArea
                };
            }
        }
    }
    if export_rect.min[1] >= viewport.min[1] && export_rect.max[1] <= viewport.max[1] {
        if let Some(index) = panels.auxiliary.export_scope.draw(
            list,
            atlas,
            export_rect,
            body,
            &scopes,
            usize::from(panels.movie_export_scope == motion_core::FrameScope::WorkArea),
            original,
            !controls.templates.open && !controls.scope.open && !controls.editor.active(),
            false,
        ) {
            out.push(Action::MovieExportScope(if index == 0 {
                motion_core::FrameScope::Composition
            } else {
                motion_core::FrameScope::WorkArea
            }));
        }
    }
    if !controls.editing() && !panels.auxiliary.export_scope.open {
        if let Some(text) = note {
            tooltip(list, atlas, body, original.mouse, &text);
        }
    }
    panels.auxiliary.render_queue = controls;
}

#[cfg(test)]
mod tests;
