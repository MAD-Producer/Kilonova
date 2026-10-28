//! Peak hold between UI polls. The device callback publishes two bounded
//! atomics, with no allocation, locking, logging or source-file access.
use super::{AtomicU32, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Levels {
    pub peak: [f32; 2],
}
impl Levels {
    pub fn full_scale(self, channel: usize) -> bool {
        self.peak[channel] >= 1.
    }
    pub fn db(self, channel: usize) -> f32 {
        if self.peak[channel] > 0. {
            20. * self.peak[channel].log10()
        } else {
            f32::NEG_INFINITY
        }
    }
}
#[derive(Default)]
pub(super) struct Meter {
    peak: [AtomicU32; 2],
}
impl Meter {
    pub fn publish(&self, peak: [f32; 2]) {
        for (slot, value) in self.peak.iter().zip(peak) {
            // These magnitudes come from validated mixer PCM. Nonnegative f32
            // bit order matches numeric order, including saturated infinity.
            slot.fetch_max(value.to_bits(), Ordering::Relaxed);
        }
    }
    pub fn take(&self) -> Levels {
        Levels {
            peak: self
                .peak
                .each_ref()
                .map(|p| f32::from_bits(p.swap(0, Ordering::Relaxed))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    #[test]
    fn audio_meter_measures_consumed_pcm_channels_and_resets_after_poll_or_silence() {
        let shared = Shared::new();
        shared.ready.store(true, Ordering::Release);
        shared.ring.push([0.25, -0.5]);
        shared.ring.push([-1., 0.125]);
        let mut output = [0f32; 4];
        let mut consumed = 0;
        render_callback(&shared, &mut output, 2, &mut consumed, 10, 0);
        assert_eq!(output, [0.25, -0.5, -1., 0.125]);
        let levels = shared.meter.take();
        assert_eq!(levels.peak, [1., 0.5]);
        assert!(levels.full_scale(0));
        assert!(!levels.full_scale(1));
        assert!((levels.db(1) + 6.0206).abs() < 0.0001);
        assert_eq!(shared.meter.take(), Levels::default());
        render_callback(&shared, &mut output, 2, &mut consumed, 20, 0);
        assert_eq!(output, [0.; 4]);
        assert_eq!(shared.meter.take().db(0), f32::NEG_INFINITY);
    }
    #[test]
    fn audio_meter_keeps_inter_callback_peak_but_reports_actual_mono_downmix_and_stop() {
        let shared = Shared::new();
        shared.ready.store(true, Ordering::Release);
        let mut output = [0f32; 1];
        let mut consumed = 0;
        for pcm in [[0.75, 0.25], [0.1, 0.1]] {
            shared.ring.push(pcm);
            render_callback(&shared, &mut output, 1, &mut consumed, 10, 0);
        }
        assert_eq!(shared.meter.take().peak, [0.5; 2]);
        shared.ring.push([1.; 2]);
        shared.stop.store(true, Ordering::Release);
        render_callback(&shared, &mut output, 1, &mut consumed, 20, 0);
        assert_eq!(output, [0.]);
        assert_eq!(shared.meter.take(), Levels::default());
    }
}
