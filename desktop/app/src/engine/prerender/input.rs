//! Bounded pointer state; only a frame queued after the latest input may release it.
#[derive(Clone, Copy)]
pub(crate) enum InputActivity {
    Button { index: u8, pressed: bool },
    Pulse,
    Cancel,
}

#[derive(Default)]
pub(super) struct InputGate {
    buttons: u8,
    epoch: u64,
    awaiting_present: bool,
}
impl InputGate {
    pub fn observe(&mut self, activity: InputActivity) -> Result<bool, String> {
        let buttons = match activity {
            InputActivity::Button { index, pressed } => {
                if index >= 3 {
                    return Err("prerender input button outside supported range".into());
                }
                let bit = 1 << index;
                if pressed {
                    self.buttons | bit
                } else {
                    self.buttons & !bit
                }
            }
            InputActivity::Cancel => 0,
            InputActivity::Pulse => self.buttons,
        };
        if buttons == self.buttons && !matches!(activity, InputActivity::Pulse) {
            return Ok(false);
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("prerender input epoch exhausted")?;
        self.buttons = buttons;
        self.epoch = epoch;
        self.awaiting_present = true;
        Ok(true)
    }
    pub fn token(&self) -> u64 {
        self.epoch
    }
    pub fn pending(&self) -> bool {
        self.buttons != 0 || self.awaiting_present
    }
    pub fn presented(&mut self, token: u64) {
        if token == self.epoch && self.buttons == 0 {
            self.awaiting_present = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_frames_and_released_buttons_do_not_acknowledge_new_input() {
        let mut gate = InputGate::default();
        let old_frame = gate.token();
        gate.observe(InputActivity::Button {
            index: 0,
            pressed: true,
        })
        .unwrap();
        let pressed_frame = gate.token();
        gate.presented(pressed_frame);
        assert!(gate.pending(), "a held button must keep admission closed");
        gate.observe(InputActivity::Button {
            index: 0,
            pressed: false,
        })
        .unwrap();
        gate.presented(old_frame);
        gate.presented(pressed_frame);
        assert!(
            gate.pending(),
            "release must be followed by its own successful frame"
        );
        gate.presented(gate.token());
        assert!(!gate.pending());
        let before_pulse = gate.token();
        gate.observe(InputActivity::Pulse).unwrap();
        gate.presented(before_pulse);
        assert!(gate.pending());
        gate.presented(gate.token());
        assert!(!gate.pending());
    }
    #[test]
    fn multiple_buttons_focus_cancellation_and_invalid_events_are_bounded() {
        let mut gate = InputGate::default();
        gate.observe(InputActivity::Button {
            index: 0,
            pressed: true,
        })
        .unwrap();
        gate.observe(InputActivity::Button {
            index: 1,
            pressed: true,
        })
        .unwrap();
        gate.observe(InputActivity::Button {
            index: 0,
            pressed: false,
        })
        .unwrap();
        gate.presented(gate.token());
        assert!(gate.pending());
        let before = gate.token();
        assert!(
            gate.observe(InputActivity::Button {
                index: 3,
                pressed: true
            })
            .is_err()
        );
        assert_eq!(gate.token(), before);
        gate.observe(InputActivity::Cancel).unwrap();
        assert!(gate.pending());
        gate.presented(gate.token());
        assert!(!gate.pending());
        assert!(!gate.observe(InputActivity::Cancel).unwrap());
    }
}
