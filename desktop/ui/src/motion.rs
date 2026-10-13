//! Short presentation transitions, independent of project time and undo data.
use std::{collections::HashMap, time::Instant};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Easing {
    Linear,
    #[default]
    CubicOut,
    CubicIn,
    Standard,
    Decelerate,
    Accelerate,
}
impl Easing {
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0., 1.);
        match self {
            Self::Linear => t,
            Self::CubicOut | Self::Decelerate => 1. - (1. - t).powi(3),
            Self::CubicIn | Self::Accelerate => t.powi(3),
            Self::Standard => t * t * (3. - 2. * t),
        }
    }
}
pub type Key = (u64, u64);
struct Channel {
    from: f32,
    to: f32,
    at: f64,
    seconds: f64,
    touched: u64,
    observed: Option<f64>,
    easing: Easing,
}
pub struct Motion {
    origin: Instant,
    channels: HashMap<Key, Channel>,
    clock: u64,
    easing: Easing,
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            channels: HashMap::new(),
            clock: 0,
            easing: Default::default(),
        }
    }
}
impl Motion {
    pub fn set_easing(&mut self, easing: Easing) {
        self.easing = easing;
    }
    pub fn value(
        &mut self,
        key: Key,
        initial: f32,
        target: f32,
        milliseconds: f32,
        direct: bool,
    ) -> f32 {
        self.value_at(
            key,
            initial,
            target,
            milliseconds,
            direct,
            self.origin.elapsed().as_secs_f64(),
        )
    }
    pub fn value_at(
        &mut self,
        key: Key,
        initial: f32,
        target: f32,
        milliseconds: f32,
        direct: bool,
        now: f64,
    ) -> f32 {
        if !target.is_finite() || !initial.is_finite() || !now.is_finite() {
            return initial;
        }
        self.clock = self.clock.wrapping_add(1);
        if !self.channels.contains_key(&key) && self.channels.len() >= 256 {
            if let Some(old) = self
                .channels
                .iter()
                .min_by_key(|(_, c)| c.touched)
                .map(|(k, _)| *k)
            {
                self.channels.remove(&old);
            }
        }
        let channel = self.channels.entry(key).or_insert(Channel {
            from: initial,
            to: initial,
            at: now,
            seconds: 0.,
            touched: self.clock,
            observed: None,
            easing: self.easing,
        });
        channel.touched = self.clock;
        let current = sample(channel, now);
        let seconds = if milliseconds.is_finite() {
            milliseconds.clamp(0., 1000.) as f64 / 1000.
        } else {
            0.
        };
        if direct || seconds == 0. {
            *channel = Channel {
                from: target,
                to: target,
                at: now,
                seconds: 0.,
                touched: self.clock,
                observed: channel.observed,
                easing: self.easing,
            };
            return target;
        }
        if target != channel.to {
            channel.from = current;
            channel.to = target;
            channel.at = now;
            channel.seconds = seconds;
            channel.easing = self.easing;
        }
        sample(channel, now)
    }
    pub fn active(&self) -> bool {
        let now = self.origin.elapsed().as_secs_f64();
        self.channels
            .values()
            .any(|c| c.from != c.to && now < c.at + c.seconds)
    }
    /// Feedback changes only the presentation, never the sampled data value.
    pub fn changed(&mut self, key: Key, value: f64, milliseconds: f32) -> f32 {
        let intensity = self.value(key, 0., 0., milliseconds, false);
        if !value.is_finite() {
            return intensity;
        }
        let c = self.channels.get_mut(&key).unwrap();
        let changed = c.observed.is_some_and(|previous| previous != value);
        c.observed = Some(value);
        if changed {
            c.easing = self.easing;
            c.from = 1.;
            c.to = 0.;
            c.at = self.origin.elapsed().as_secs_f64();
            c.seconds = if milliseconds.is_finite() {
                milliseconds.clamp(0., 1000.) as f64 / 1000.
            } else {
                0.
            };
            return if c.seconds > 0. { 1. } else { 0. };
        }
        intensity
    }
}
fn sample(c: &Channel, now: f64) -> f32 {
    if c.seconds <= 0. {
        return c.to;
    }
    let t = ((now - c.at) / c.seconds).clamp(0., 1.) as f32;
    c.from + (c.to - c.from) * c.easing.sample(t)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retargeting_is_continuous_and_direct_manipulation_does_not_lag() {
        let mut m = Motion::default();
        let key = (1, 2);
        assert_eq!(m.value_at(key, 0., 1., 100., false, 0.), 0.);
        let half = m.value_at(key, 0., 1., 100., false, 0.05);
        assert!(half > 0. && half < 1.);
        assert_eq!(m.value_at(key, 0., 0., 100., false, 0.05), half);
        assert_eq!(m.value_at(key, 0., 0., 100., false, 0.2), 0.);
        assert_eq!(m.value_at(key, 0., 500., 100., true, 0.2), 500.);
        assert_eq!(m.value_at(key, 0., 500., 100., false, 0.21), 500.);
    }
}
