//! Alternate editors of the canonical camera FOV; no second animation channel.
use super::*;
use motion_core::lens::{self, Lens};
use motion_ui::widgets::{Checkbox, Dropdown, DropdownItem, NumberOptions};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Parameter {
    Focal,
    Film,
    Zoom,
}
struct LensField {
    parameter: Parameter,
    number: super::super::numeric::Field,
    rect: Rect,
}
#[derive(Default)]
pub(super) struct Controls {
    field: Option<LensField>,
    presets: Dropdown,
    context: Option<(String, String, u64)>,
    failures: crate::fallback_observation::Counter,
    pub(super) cursor: bool,
    pub(super) drag_readout: Option<([f32; 2], String)>,
    #[cfg(test)]
    pub(super) rects: Vec<(Parameter, Rect)>,
    #[cfg(test)]
    pub(super) preset_rect: Option<Rect>,
}
fn text(locale: Locale, zh: &'static str, en: &'static str) -> &'static str {
    if locale == Locale::Zh {
        zh
    } else {
        en
    }
}
pub(super) fn snapshot(state: &Value, object: u64) -> Result<(Lens, (u32, u32)), String> {
    let camera = camera(state, object);
    if !camera.is_object() {
        return Err("camera lens: camera missing".into());
    }
    let film = match camera.get("film_width_mm") {
        None => f64::from(lens::default_film_width()),
        Some(value) => value
            .as_f64()
            .ok_or("camera lens: film width is not numeric")?,
    };
    let fov = sample(state, object)["fov"]
        .as_f64()
        .ok_or("camera lens: sampled FOV missing")?;
    let width = state["project"]["width"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or("camera lens: composition width missing")?;
    let height = state["project"]["height"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or("camera lens: composition height missing")?;
    Lens::from_fov(film, fov, width, height)
        .map(|lens| (lens, (width, height)))
        .map_err(|error| format!("camera lens: {error}"))
}
fn command(
    state: &Value,
    object: u64,
    original: Lens,
    parameter: Parameter,
    value: f64,
) -> Command {
    let (film, focal) = match parameter {
        Parameter::Film => (value, original.focal_length_mm),
        Parameter::Focal => (original.film_width_mm, value),
        Parameter::Zoom => (
            original.film_width_mm,
            value * original.focal_length_mm / original.zoom_px,
        ),
    };
    Command::CameraLens {
        object,
        frame: state["frame"].as_f64().unwrap_or(0.).round().max(0.) as u32,
        film_width_mm: film as f32,
        focal_length_mm: focal as f32,
    }
}
impl Controls {
    pub(super) fn editing(&self) -> bool {
        self.field.is_some() || self.presets.open
    }
    pub(super) fn menu_open(&self) -> bool {
        self.presets.open
    }
    pub(super) fn cancel(&mut self) -> bool {
        self.presets.open = false;
        self.field
            .take()
            .is_some_and(|mut field| field.number.cancel())
    }
    pub(super) fn prepare(
        &mut self,
        state: &Value,
        object: u64,
        input: &Input,
        locked: bool,
        out: &mut Vec<Action>,
    ) {
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            object,
        );
        if self.context.as_ref() != Some(&context) {
            self.failures.success();
        }
        if self.context.as_ref() != Some(&context)
            || locked
            || input.focus_lost
            || self.field.as_ref().is_some_and(|field| {
                input
                    .pressed
                    .is_some_and(|(_, point)| !field.rect.contains(point))
            })
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
        }
        self.context = Some(context);
    }
    pub(super) fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        state: &Value,
        object: u64,
        input: &Input,
        locale: Locale,
        locked: bool,
        selection: &mut super::super::animation_controls::Controls,
        graph_component: &mut usize,
        out: &mut Vec<Action>,
    ) {
        self.cursor = false;
        self.drag_readout = None;
        let styles = list.styles();
        let colors = styles.colors();
        let row = styles.metric("camera.row.height", 28.);
        let field_height = styles.metric("camera.field.height", 26.);
        let pad = styles.metric("padding.panel", 8.);
        let slot = styles.metric("icon.slot", 20.);
        let layout = |y| {
            let mut layout = super::super::properties::inspector_layout(&styles, area, y, 0, 1);
            layout.caption.max[1] = y + row;
            layout.number.min[1] = y + (row - field_height) * 0.5;
            layout.number.max[1] = layout.number.min[1] + field_height;
            layout
        };
        let title = Rect::new(
            area.min[0] + pad,
            *y,
            (area.width() - pad * 2.).max(0.),
            row,
        );
        ui::text(
            list,
            atlas,
            title,
            text(
                locale,
                "镜头 · 水平胶片 / 垂直视角",
                "Lens · horizontal gate / vertical FOV",
            ),
            colors.TEXT_MUTED,
            Align::Left,
        );
        *y += row;
        let (lens, dimensions) = match snapshot(state, object) {
            Ok(value) => {
                self.failures.success();
                value
            }
            Err(error) => {
                self.failures.failure(
                    "camera.lens.snapshot",
                    format_args!("object={object} reason={error}"),
                );
                if self.cancel() {
                    out.push(Action::History(4));
                }
                ui::text(
                    list,
                    atlas,
                    Rect::new(title.min[0], *y, title.width(), row),
                    text(locale, "镜头参数不可用", "Lens values unavailable"),
                    colors.TEXT_MUTED,
                    Align::Left,
                );
                *y += row;
                return;
            }
        };
        let first = layout(*y);
        ui::text(
            list,
            atlas,
            first.caption,
            text(locale, "焦距预设", "Lens preset"),
            colors.TEXT_MUTED,
            Align::Left,
        );
        let focal_presets = [15., 20., 24., 28., 35., 50., 85., 135.];
        let mut names = vec![text(locale, "自定义", "Custom").to_string()];
        names.extend(focal_presets.iter().map(|value| format!("{value:.0} mm")));
        let selected = focal_presets
            .iter()
            .position(|value| (lens.focal_length_mm - value).abs() < 0.001)
            .map_or(0, |i| i + 1);
        let items: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(index, label)| DropdownItem {
                label,
                enabled: index == 0
                    || Lens::from_focal_length(
                        lens.film_width_mm,
                        focal_presets[index - 1],
                        dimensions.0,
                        dimensions.1,
                    )
                    .is_ok(),
            })
            .collect();
        let was_open = self.presets.open;
        let mut overlay = list.child();
        #[cfg(test)]
        {
            self.preset_rect = Some(first.number);
            self.rects.clear();
        }
        if let Some(index) = self.presets.draw_inline(
            &mut overlay,
            atlas,
            first.number,
            area,
            &items,
            selected,
            input,
            !locked,
            false,
        ) {
            if index > 0 {
                selection.choose_track_in(state, object, Property::Fov, None);
                *graph_component = 0;
                out.push(Action::Edit(command(
                    state,
                    object,
                    lens,
                    Parameter::Focal,
                    focal_presets[index - 1],
                )));
            }
        }
        let neutral = Input::default();
        let input = if was_open || self.presets.open {
            &neutral
        } else {
            input
        };
        *y += row;
        let min_zoom = f64::from(dimensions.1) / (2. * (120_f64.to_radians() * 0.5).tan());
        let max_zoom = f64::from(dimensions.1) / (2. * (10_f64.to_radians() * 0.5).tan());
        for (parameter, name, value, min, max, speed) in [
            (
                Parameter::Focal,
                text(locale, "焦距 mm", "Focal mm"),
                lens.focal_length_mm,
                min_zoom * lens.film_width_mm / f64::from(dimensions.0),
                max_zoom * lens.film_width_mm / f64::from(dimensions.0),
                0.1,
            ),
            (
                Parameter::Film,
                text(locale, "胶片宽 mm", "Film mm"),
                lens.film_width_mm,
                (lens.focal_length_mm * f64::from(dimensions.0) / max_zoom)
                    .max(lens::MIN_FILM_WIDTH_MM),
                (lens.focal_length_mm * f64::from(dimensions.0) / min_zoom)
                    .min(lens::MAX_FILM_WIDTH_MM),
                0.1,
            ),
            (
                Parameter::Zoom,
                text(locale, "缩放 px", "Zoom px"),
                lens.zoom_px,
                min_zoom,
                max_zoom,
                1.,
            ),
        ] {
            let r = layout(*y);
            ui::text(list, atlas, r.caption, name, colors.TEXT_MUTED, Align::Left);
            #[cfg(test)]
            self.rects.push((parameter, r.number));
            if !locked && clicked(input, r.number) && self.field.is_none() {
                selection.choose_track_in(state, object, Property::Fov, None);
                *graph_component = 0;
                self.field = Some(LensField {
                    parameter,
                    number: Default::default(),
                    rect: r.number,
                });
            }
            if let Some(field) = self
                .field
                .as_mut()
                .filter(|field| field.parameter == parameter)
            {
                field.rect = r.number;
                self.cursor |= field.number.editor.dragging() || r.number.contains(input.mouse);
                let events = field.number.draw(
                    list,
                    atlas,
                    r.number,
                    value,
                    NumberOptions {
                        min,
                        max,
                        speed,
                        decimals: 3,
                    },
                    input,
                );
                if let Some(note) = field.number.readout() {
                    self.drag_readout = Some((input.mouse, note));
                }
                if super::super::numeric::apply(events, out, |value| {
                    command(state, object, lens, parameter, f64::from(value))
                }) {
                    self.field = None;
                }
            } else {
                self.cursor |= !locked && r.number.contains(input.mouse);
                super::super::numeric::passive(list, atlas, r.number, value, input, !locked);
            }
            *y += row;
        }
        let dof = layout(*y);
        ui::text(
            list,
            atlas,
            dof.caption,
            text(locale, "景深", "DOF"),
            colors.TEXT_MUTED,
            Align::Left,
        );
        Checkbox::draw(
            list,
            atlas,
            Rect::new(dof.number.min[0], *y, slot, row),
            "",
            false,
            input,
            false,
            false,
            None,
        );
        ui::text(
            list,
            atlas,
            Rect::new(
                dof.number.min[0] + slot,
                *y,
                (area.max[0] - pad - dof.number.min[0] - slot).max(0.),
                row,
            ),
            text(locale, "尚未支持", "Not supported"),
            colors.TEXT_MUTED,
            Align::Left,
        );
        *y += row;
        ui::text(
            list,
            atlas,
            Rect::new(title.min[0], *y, title.width(), row),
            text(
                locale,
                "光圈、焦点距离与散景尚未渲染",
                "Aperture, focus distance and bokeh are not rendered",
            ),
            colors.TEXT_MUTED,
            Align::Left,
        );
        *y += row;
        list.append(overlay);
    }
}
