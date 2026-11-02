//! Keep authored interval identity separate from signed screen-time direction.
use super::*;
use motion_core::{layer_time::LayerClock, Error, KeyframeTarget, LocalKeyEdit};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Context {
    pub mapped: bool,
    clock: LayerClock,
    offset: i32,
    playhead: f64,
}
pub(super) struct Selected {
    pub object: u64,
    pub property: Property,
    pub axis: Option<Axis>,
    pub frame: i64,
}
#[derive(Clone)]
pub(super) struct Guard {
    context: Context,
    selection: super::super::key_selection::Set,
    component: usize,
    plot: Plot,
}
impl Context {
    pub fn read(state: &Value, object: u64) -> motion_core::Result<Self> {
        let mapped = super::super::property_authoring::precise(state, object);
        if !mapped {
            return Ok(Self {
                mapped,
                clock: LayerClock::default(),
                offset: 0,
                playhead: 0.,
            });
        }
        let (clock, offset) = super::super::property_authoring::clock(state, object)?;
        let playhead = state["frame"]
            .as_f64()
            .filter(|f| f.is_finite())
            .ok_or_else(|| Error::Invalid("invalid graph handle playhead".into()))?;
        Ok(Self {
            mapped,
            clock,
            offset,
            playhead,
        })
    }
    pub fn frame(self, key: &Value) -> Option<i64> {
        key[if self.mapped { "local_frame" } else { "frame" }].as_i64()
    }
    pub fn identity(self, key: &Value) -> motion_core::Result<i64> {
        self.frame(key)
            .ok_or_else(|| Error::Invalid("graph handle interval identity missing".into()))
    }
    pub fn command_key(
        self,
        target: KeyframeTarget,
        key: &Value,
        incoming: bool,
        easing: Easing,
    ) -> motion_core::Result<Command> {
        self.command(target, self.identity(key)?, incoming, easing)
    }
    pub fn validate(
        self,
        keys: &[Value],
        scalar: bool,
        component: usize,
    ) -> motion_core::Result<()> {
        if !self.mapped {
            return Ok(());
        }
        if keys.len() > motion_core::MAX_FRAMES as usize {
            return Err(Error::Invalid(
                "graph handle key count exceeds limit".into(),
            ));
        }
        let mut previous = None;
        for key in keys {
            let frame = self
                .frame(key)
                .and_then(|f| i32::try_from(f).ok())
                .ok_or_else(|| Error::Invalid("graph handle local identity missing".into()))?;
            if previous.is_some_and(|old| old >= frame) {
                return Err(Error::Invalid(
                    "graph handle local keys are not ordered and unique".into(),
                ));
            }
            let composition = self
                .clock
                .projected_composition_frame(f64::from(frame), self.offset)?;
            if key["frame"].as_f64() != Some(composition) {
                return Err(Error::Invalid(
                    "graph handle projection disagrees with layer clock".into(),
                ));
            }
            let value = if scalar {
                key["value"].as_f64()
            } else {
                key["value"][component].as_f64()
            };
            if value.is_none_or(|v| !v.is_finite()) {
                return Err(Error::Invalid(
                    "graph handle component is missing or nonfinite".into(),
                ));
            }
            previous = Some(frame);
        }
        Ok(())
    }
    pub fn selected(
        self,
        panels: &Panels,
        state: &Value,
        target: &KeyframeTarget,
    ) -> Option<Selected> {
        if self.mapped {
            if panels.selected_keys.status(state) != super::super::key_selection::Status::Current {
                return None;
            }
            let super::super::key_selection::AddressedKey::Local(key) =
                panels.selected_keys.addressed_primary()?
            else {
                return None;
            };
            if &key.target != target {
                return None;
            }
            let KeyframeTarget::Property {
                object,
                property,
                axis,
            } = target
            else {
                return None;
            };
            Some(Selected {
                object: *object,
                property: *property,
                axis: *axis,
                frame: i64::from(key.frame),
            })
        } else {
            panels.animation_controls.user_key().map(|k| Selected {
                object: k.object,
                property: k.property,
                axis: k.axis,
                frame: i64::from(k.frame),
            })
        }
    }
    pub fn guard(self, panels: &Panels, component: usize, plot: Plot) -> Option<Guard> {
        self.mapped.then(|| Guard {
            context: self,
            selection: panels.selected_keys.clone(),
            component,
            plot,
        })
    }
    pub fn command(
        self,
        target: KeyframeTarget,
        frame: i64,
        incoming: bool,
        easing: Easing,
    ) -> motion_core::Result<Command> {
        if self.mapped {
            Ok(Command::LocalKeyframes {
                target,
                edit: LocalKeyEdit::Handle {
                    frame: i32::try_from(frame)
                        .map_err(|_| Error::Invalid("graph handle local frame overflow".into()))?,
                    incoming,
                    control: super::handle_control(incoming, easing)?,
                },
            })
        } else {
            super::handle_command(
                target,
                u32::try_from(frame).map_err(|_| {
                    Error::Invalid("graph handle composition frame overflow".into())
                })?,
                incoming,
                easing,
            )
        }
    }
}
impl Guard {
    pub fn current(
        &self,
        timing: Context,
        panels: &Panels,
        state: &Value,
        component: usize,
        plot: Plot,
    ) -> bool {
        self.context == timing
            && self.component == component
            && self.plot == plot
            && self.selection.same_selection(&panels.selected_keys)
            && panels.selected_keys.writable(state)
    }
}
pub(super) fn cancel(panels: &mut Panels, out: &mut Vec<Action>) {
    if panels
        .timeline
        .graph_drag
        .take()
        .is_some_and(|drag| drag.begun())
    {
        out.push(Action::History(4));
    }
}
