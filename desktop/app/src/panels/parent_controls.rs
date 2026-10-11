use crate::{
    i18n::{Locale, Text as T},
    panels::Action,
};
use motion_core::Command;
use motion_ui::{
    input::{Input, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    theme::palette,
    ui::{self, Align},
};
use serde_json::Value;

#[derive(Default)]
pub struct Controls {
    pub open: bool,
    object: Option<u64>,
    scroll: usize,
}
fn clicked(input: &Input, rect: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
}
impl Controls {
    pub fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        object: u64,
        owner: &Value,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        if self.object != Some(object) {
            self.object = Some(object);
            self.open = false;
            self.scroll = 0;
        }
        let layers = state["project"]["layers"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let current = owner["parent"]["object"].as_u64();
        let name = current
            .and_then(|id| layers.iter().find(|l| l["id"].as_u64() == Some(id)))
            .and_then(|l| l["name"].as_str())
            .unwrap_or(locale.t(T::None));
        let rect = Rect::new(area.min[0] + 16., *y, area.width() - 32., 26.);
        paint::rect(list, rect, palette::SUNKEN);
        ui::text(
            list,
            atlas,
            rect.shrink(4.),
            &format!("{}  ›  {}", locale.t(T::ParentLink), name),
            palette::TEXT,
            Align::Left,
        );
        let locked = owner["locked"].as_bool().unwrap_or(false);
        if clicked(input, rect) && !locked {
            self.open = !self.open;
        }
        *y += 32.;
        if !self.open {
            return;
        }
        let mut options = vec![(None, locale.t(T::None))];
        for layer in layers {
            let id = layer["id"].as_u64().unwrap();
            let mut cursor = Some(id);
            let mut allowed = true;
            for _ in 0..=layers.len() {
                let Some(next) = cursor else {
                    break;
                };
                if next == object {
                    allowed = false;
                    break;
                }
                cursor = layers
                    .iter()
                    .find(|l| l["id"].as_u64() == Some(next))
                    .and_then(|l| l["parent"]["object"].as_u64());
            }
            if allowed {
                options.push((Some(id), layer["name"].as_str().unwrap_or("Layer")));
            }
        }
        if area.contains(input.mouse) {
            if input.wheel[1] > 0. {
                self.scroll = (self.scroll + 1).min(options.len().saturating_sub(8));
            } else if input.wheel[1] < 0. {
                self.scroll = self.scroll.saturating_sub(1);
            }
        }
        for (parent, name) in options.into_iter().skip(self.scroll).take(8) {
            let row = Rect::new(rect.min[0], *y, rect.width(), 24.);
            paint::rect(
                list,
                row,
                if parent == current {
                    palette::ACCENT.with_alpha(0.2)
                } else {
                    palette::CHROME
                },
            );
            ui::text(
                list,
                atlas,
                row.shrink(4.),
                name,
                palette::TEXT,
                Align::Left,
            );
            if clicked(input, row) {
                out.push(Action::Edit(Command::Parent {
                    object,
                    parent,
                    frame: state["frame"].as_f64().unwrap_or(0.0).floor() as u32,
                }));
                self.open = false;
            }
            *y += 26.;
        }
    }
}
