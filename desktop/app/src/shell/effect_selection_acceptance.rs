//! Real effect parameter clicks and both hosts' shared selection projection.
use crate::*;
use motion_ui::{dock::Panel, MouseButton};

impl Shell {
    pub(super) fn capture_effect_selection_screens(
        &mut self,
        window_size: [f32; 2],
        report: &std::path::Path,
        output: &mut serde_json::Map<String, Value>,
        object: u64,
    ) -> Result<(), String> {
        let before = self.engine.state()?;
        let catalogue = self.engine.plugin("{\"op\":\"catalogue\"}".into())?;
        let package = catalogue["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| {
                p["manifest"]["effects"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|e| e["id"] == "brightness_contrast")
            })
            .ok_or("selection capture effect package missing")?;
        self.engine.plugin(
            serde_json::json!({"op":"add","object":object,
            "plugin":package["manifest"]["id"],"version":package["manifest"]["version"],
            "hash":package["hash"],"effect":"brightness_contrast"})
            .to_string(),
        )?;
        let captured = (|| {
            let state = self.engine.state()?;
            for source in [Panel::EffectControls, Panel::Properties] {
                let mut panels = panels::Panels {
                    styles: self.panels.styles.clone(),
                    locale: self.workspace.locale,
                    focused: Some(source),
                    ..Default::default()
                };
                panels.select_only(Some(object));
                panels.effect_controls.catalogue = catalogue.clone().into();
                panels.properties.effects_collapsed = false;
                panels.properties.transform_collapsed = true;
                panels.path_controls.masks_open = false;
                if source == Panel::Properties {
                    panels.properties.effects.selection_probe.start();
                } else {
                    panels.effect_controls.selection_probe.start();
                }
                let draw = |atlas: &mut TextAtlas,
                            panels: &mut panels::Panels,
                            panel: Panel,
                            size: [f32; 2],
                            input: &Input|
                 -> Result<PaintList, String> {
                    let mut paint = PaintList::default();
                    paint.stylesheet = panels.styles.clone();
                    let actions = if source == Panel::EffectControls {
                        panels::chrome(
                            &mut paint,
                            atlas,
                            size,
                            &dock::Node::dock(panel.dock()),
                            panels,
                            &state,
                            input,
                        )
                    } else {
                        panels::floating_chrome(
                            &mut paint, atlas, panel, size, panels, &state, input,
                        )
                    };
                    if !actions.is_empty() {
                        return Err(format!(
                            "effect selection capture emitted edits: {actions:?}"
                        ));
                    }
                    Ok(paint)
                };
                draw(
                    &mut self.atlas,
                    &mut panels,
                    source,
                    [360., 560.],
                    &Input::default(),
                )?;
                let hit = if source == Panel::Properties {
                    panels.properties.effects.selection_probe.rect()
                } else {
                    panels.effect_controls.selection_probe.rect()
                }
                .ok_or("effect selection caption not visible")?;
                let point = [
                    (hit.min[0] + hit.max[0]) * 0.5,
                    (hit.min[1] + hit.max[1]) * 0.5,
                ];
                draw(
                    &mut self.atlas,
                    &mut panels,
                    source,
                    [360., 560.],
                    &Input {
                        mouse: point,
                        pressed: Some((MouseButton::Left, point)),
                        ..Default::default()
                    },
                )?;
                if !panels.animation_controls.foreign_track_selected() {
                    return Err(
                        "effect selection did not acquire property command ownership".into(),
                    );
                }
                let owner = if source == Panel::Properties {
                    "inspector"
                } else {
                    "controls"
                };
                for (panel, width, suffix) in [
                    (Panel::EffectControls, 360., "controls"),
                    (Panel::Properties, 360., "inspector"),
                    (Panel::Properties, 220., "narrow"),
                ] {
                    panels.focused = Some(panel);
                    let size = [width, 560.];
                    let paint = draw(&mut self.atlas, &mut panels, panel, size, &Input::default())?;
                    let mut ui = self.ui_frame(paint, window_size, true);
                    // This isolated capture overrides shell geometry; use the single-view frame protocol.
                    ui.views = None;
                    ui.composition = None;
                    ui.composition_image = None;
                    ui.node_preview = None;
                    let name = format!("effect-selection-from-{owner}-{suffix}");
                    let path = report.with_extension(format!("{name}.png"));
                    self.capture_presented_ui(ui, path.clone())?;
                    let pixels = image::open(&path).map_err(|e| e.to_string())?;
                    let width = (size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
                    let height = (size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
                    pixels
                        .crop_imm(0, 0, width, height)
                        .save(&path)
                        .map_err(|e| e.to_string())?;
                    output.insert(name, serde_json::json!(path));
                }
            }
            Ok(())
        })();
        let restored = self.engine.history(0).and_then(|state| {
            if state["project"] == before["project"] {
                Ok(())
            } else {
                Err("effect selection capture did not restore project".into())
            }
        });
        match (captured, restored) {
            (Err(primary), Err(cleanup)) => {
                Err(format!("{primary}; effect selection cleanup: {cleanup}"))
            }
            (Err(primary), _) => Err(primary),
            (_, Err(cleanup)) => Err(cleanup),
            _ => Ok(()),
        }
    }
}
