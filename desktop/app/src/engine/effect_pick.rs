use super::*;
use crate::panels::effect_pick::{Kind, Ticket};

pub(super) fn apply(
    id: i64,
    ticket: Ticket,
    point: [f64; 2],
    retained: Option<&CompositionStamp>,
) -> motion_host::Result<Value> {
    let context = format!(
        "effect pick object={} effect={} parameter={} frame={}",
        ticket.object, ticket.effect, ticket.parameter, ticket.frame
    );
    motion_host::session::with_session(id, |s| {
        let cancelled = |s: &mut motion_host::session::Session, reason: &str| {
            let mut state = s.snapshot()?;
            state["editorAction"] =
                serde_json::json!({"operation":"effect_pick","cancelled":true,"reason":reason});
            Ok(state)
        };
        if !ticket.source.current(s) || s.frame != ticket.frame {
            return cancelled(s, "context_changed");
        }
        if s.observing || !point.iter().all(|v| v.is_finite()) {
            return Err(
                "effect pick requires a finite composition point in composition view".into(),
            );
        }
        let layer = s
            .engine
            .project()
            .layers
            .iter()
            .find(|l| l.id == ticket.object)
            .ok_or("effect pick layer is missing")?;
        if layer.locked {
            return cancelled(s, "target_unavailable");
        }
        let effect = layer
            .effects
            .iter()
            .find(|e| e.id == ticket.effect)
            .ok_or("effect pick instance is missing")?;
        let parameter = effect
            .params
            .get(&ticket.parameter)
            .ok_or("effect pick parameter is missing")?;
        if !parameter.implemented {
            return cancelled(s, "target_unavailable");
        }
        let mut value = parameter.sample(layer.local_frame(s.frame))?;
        match ticket.kind {
            Kind::Position => {
                if parameter.kind != motion_effects::ParamKind::Vec2 {
                    return Err("position pick requires a Vec2 parameter".into());
                }
                value[0] = point[0] as f32;
                value[1] = point[1] as f32;
            }
            Kind::Color => {
                if parameter.kind != motion_effects::ParamKind::Color {
                    return Err("color pick requires a color parameter".into());
                }
                if !point.iter().all(|v| *v >= 0. && *v < 1.) {
                    return Err("color pick is outside the composition".into());
                }
                let current = retained.is_some_and(|stamp| {
                    stamp.revision == s.engine.revision()
                        && stamp.frame == s.frame.to_bits()
                        && stamp.view == s.view_revision
                        && stamp.surface == s.surface_epoch
                });
                let Some(graphics) = s.graphics.as_ref().filter(|g| current && g.scratch.valid())
                else {
                    return cancelled(s, "preview_pending");
                };
                let pixel = [
                    (point[0] * graphics.scratch.width as f64).floor() as u32,
                    (point[1] * graphics.scratch.height as f64).floor() as u32,
                ];
                value = graphics
                    .renderer
                    .sample_preview_pixel(&graphics.scratch, pixel)
                    .map_err(|cause| {
                        motion_host::Error::with_context(
                            format!(
                                "effect color pick object={} effect={} parameter={} phase=readback",
                                ticket.object, ticket.effect, ticket.parameter
                            ),
                            cause,
                        )
                    })?;
            }
        }
        if !parameter
            .kind
            .valid_value(&value, parameter.min, parameter.max)
        {
            return cancelled(s, "out_of_range");
        }
        let frame = s.frame.floor() as u32;
        s.apply_command_batch(vec![motion_core::Command::Effect {
            object: ticket.object,
            action: motion_core::EffectAction::Set {
                effect: ticket.effect,
                param: ticket.parameter,
                frame,
                value,
            },
        }])
    })
    .map_err(|cause| motion_host::Error::with_context(context, cause))
}
