use super::*;

impl TimeRemap {
    /// Find isolated continuous crossings of a source coordinate in a local
    /// time window. Constant spans have no isolated crossing. Hold jumps must
    /// be considered separately at the authored key times by the scheduler.
    /// Nonmonotone progress/velocity/elastic curves retain every crossing.
    /// Refuse excess results explicitly instead of silently dropping events.
    pub fn source_crossings(
        &self, source: f64, begin: f64, end: f64, max_results: usize,
    ) -> Result<Vec<f64>> {
        let mut work = MAX_TIME_REMAP_KEYS;
        self.source_crossings_with_work_budget(source, begin, end, max_results, &mut work)
    }
    /// Share a key-visit allowance across a scheduler's complete graph walk.
    /// Exhaustion is explicit; no partial result is presented as complete.
    pub fn source_crossings_with_work_budget(
        &self, source: f64, begin: f64, end: f64, max_results: usize, work: &mut usize,
    ) -> Result<Vec<f64>> {
        for time in [source, begin, end] { check_time(time, "source_crossings")?; }
        if begin > end || max_results == 0 || max_results > MAX_TIME_REMAP_KEYS {
            return Err(invalid("source_crossings", "invalid window or crossing budget"));
        }
        let mut result = Vec::new();
        let first = self.keys.partition_point(|key| key.frame <= begin).saturating_sub(1);
        for pair in self.keys[first..].windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if a.frame > end { break; }
            if *work == 0 { return Err(invalid("source_crossings", "source crossing work budget exceeded")); }
            *work -= 1;
            if b.frame < begin || a.source_frame == b.source_frame
                || (a.easing.curve.is_none() && a.easing.ease == Ease::Hold) { continue; }
            let duration = b.frame - a.frame;
            let lo = ((begin - a.frame) / duration).max(0.);
            let hi = ((end - a.frame) / duration).min(1.);
            if lo > hi { continue; }
            let mut cuts = vec![lo];
            if let Some(curve) = a.easing.curve {
                cuts.extend(curve.progress_turning_times().into_iter().filter(|&t| t > lo && t < hi));
            }
            cuts.push(hi);
            let value = |t| a.source_frame + (b.source_frame - a.source_frame) * precise_easing(a.easing, t).progress;
            for interval in cuts.windows(2) {
                let (mut left, mut right) = (interval[0], interval[1]);
                let (at_left, at_right) = (value(left), value(right));
                let mut candidates = Vec::new();
                if at_left == source { candidates.push(left); }
                if at_right == source { candidates.push(right); }
                if (at_left < source && at_right > source) || (at_left > source && at_right < source) {
                    for _ in 0..64 {
                        let mid = left + (right - left) / 2.;
                        if mid == left || mid == right { break; }
                        let at_mid = value(mid);
                        if at_mid == source { left = mid; right = mid; break; }
                        if (at_mid > source) == (at_left > source) { left = mid; } else { right = mid; }
                    }
                    candidates.push(left + (right - left) / 2.);
                }
                for t in candidates {
                    let local = if t == 0. { a.frame } else if t == 1. { b.frame } else { a.frame + duration * t };
                    if result.last() == Some(&local) { continue; }
                    if result.len() == max_results {
                        return Err(invalid("source_crossings", "source crossing budget exceeded"));
                    }
                    result.push(local);
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Curve, CurveShape, CurveSpace};

    fn remap(easing: Easing) -> TimeRemap {
        TimeRemap::new(vec![
            TimeRemapKey { frame: 10., source_frame: 0., easing },
            TimeRemapKey { frame: 110., source_frame: 1., easing: Easing::default() },
        ]).unwrap()
    }

    #[test]
    fn overshoot_enters_a_source_range_even_when_both_keys_are_outside_it() {
        let remap = remap(Easing { ease: Ease::Linear, curve: Some(Curve {
            space: CurveSpace::Progress, shape: CurveShape::Elastic { oscillations: 2., damping: 0.5 },
        }) });
        let crossings = remap.source_crossings(1.1, 10., 110., 16).unwrap();
        assert_eq!(crossings.len(), 4);
        for t in crossings { assert!((remap.sample(t).unwrap().frame - 1.1).abs() < 1e-12); }
        assert!(remap.source_crossings(1.1, 10., 110., 2).unwrap_err().to_string().contains("budget exceeded"));
    }

    #[test]
    fn clipped_window_reverse_and_hold_preserve_their_different_contracts() {
        let forward = remap(Easing::default());
        assert_eq!(forward.source_crossings(0.5, 0., 120., 2).unwrap(), vec![60.]);
        assert!(forward.source_crossings(0.5, 61., 120., 2).unwrap().is_empty());
        let mut keys = forward.keys().to_vec();
        keys[0].source_frame = 1.; keys[1].source_frame = 0.;
        let reverse = TimeRemap::new(keys).unwrap();
        assert_eq!(reverse.source_crossings(0.75, 10., 110., 2).unwrap(), vec![35.]);
        assert!(remap(Easing { ease: Ease::Hold, curve: None }).source_crossings(0.5, 0., 120., 2).unwrap().is_empty());
        assert!(forward.source_crossings(f64::NAN, 0., 120., 2).is_err());
        assert!(forward.source_crossings(0.5, 120., 0., 2).is_err());
        assert!(forward.source_crossings(0.5, 0., 120., 0).is_err());
    }

    #[test]
    fn shared_work_exhaustion_rejects_partial_crossings_and_fresh_budget_recovers() {
        let remap = TimeRemap::new([(0., 0.), (10., 1.), (20., 0.)].into_iter()
            .map(|(frame, source_frame)| TimeRemapKey {
                frame, source_frame, easing: Easing::default(),
            }).collect()).unwrap();
        let mut work = 1;
        let error = remap.source_crossings_with_work_budget(0.5, 0., 20., 8, &mut work)
            .unwrap_err();
        assert!(error.to_string().contains("source crossing work budget exceeded"));
        assert_eq!(work, 0);
        let mut work = 2;
        assert_eq!(remap.source_crossings_with_work_budget(0.5, 0., 20., 8, &mut work)
            .unwrap(), vec![5., 15.]);
        assert_eq!(work, 0);
        assert!(remap.source_crossings_with_work_budget(0.5, 0., 10., 8, &mut work)
            .unwrap_err().to_string().contains("source crossing work budget exceeded"));
        // Window slicing must leave distant segments out of the shared cost.
        let mut work = 1;
        assert_eq!(remap.source_crossings_with_work_budget(0.5, 11., 20., 8, &mut work)
            .unwrap(), vec![15.]);
        assert_eq!(work, 0);
    }
}
