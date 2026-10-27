//! Ephemeral axis ranges shared by graph surfaces, never authored project data.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Range {
    pub low: f64,
    pub high: f64,
}

impl Range {
    pub fn new(low: f64, high: f64) -> Option<Self> {
        let span = high - low;
        (low.is_finite() && high.is_finite() && span.is_finite() && span > 0.)
            .then_some(Self { low, high })
    }

    pub fn span(self) -> f64 {
        self.high - self.low
    }

    pub fn fraction(self, value: f64) -> f64 {
        (value - self.low) / self.span()
    }

    pub fn at(self, fraction: f64) -> f64 {
        self.low + self.span() * fraction
    }

    /// A constant track still needs a usable range. The caller supplies units,
    /// including its minimum span and padding, instead of mixing them with px.
    pub fn fitted(low: f64, high: f64, padding: f64, minimum_span: f64) -> Option<Self> {
        if !low.is_finite() || !high.is_finite() || high < low
            || !padding.is_finite() || padding < 0.
            || !minimum_span.is_finite() || minimum_span <= 0.
        {
            return None;
        }
        let span = high - low;
        if !span.is_finite() {
            return None;
        }
        let extra = (span * padding).max((minimum_span - span).max(0.) * 0.5);
        Self::new(low - extra, high + extra)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct AxisView {
    // None follows automatic bounds. Capturing a lock or explicit navigation
    // retains a range until the user asks to fit automatically again.
    range: Option<Range>,
    locked: bool,
}

impl AxisView {
    pub fn range(self, automatic: Range) -> Range {
        self.range.unwrap_or(automatic)
    }

    pub fn locked(self) -> bool {
        self.locked
    }

    pub fn lock(&mut self, locked: bool, automatic: Range) {
        if locked {
            self.range = Some(self.range(automatic));
        }
        self.locked = locked;
    }

    pub fn fit(&mut self, range: Range) -> bool {
        if self.locked || self.range == Some(range) {
            return false;
        }
        self.range = Some(range);
        true
    }

    pub fn automatic(&mut self) -> bool {
        if self.locked {
            return false;
        }
        self.range.take().is_some()
    }

    pub fn pan(&mut self, distance: f64, automatic: Range) -> bool {
        if self.locked || !distance.is_finite() || distance == 0. {
            return false;
        }
        let current = self.range(automatic);
        let Some(next) = Range::new(current.low + distance, current.high + distance) else {
            return false;
        };
        self.fit(next)
    }

    pub fn zoom(&mut self, factor: f64, anchor: f64, automatic: Range) -> bool {
        if self.locked || !factor.is_finite() || factor <= 0. || factor == 1. || !anchor.is_finite() {
            return false;
        }
        let current = self.range(automatic);
        let Some(next) = Range::new(
            anchor + (current.low - anchor) / factor,
            anchor + (current.high - anchor) / factor,
        ) else {
            return false;
        };
        self.fit(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(low: f64, high: f64) -> Range {
        Range::new(low, high).unwrap()
    }

    #[test]
    fn locking_captures_the_displayed_range_before_playback_or_scroll_changes_auto_bounds() {
        let mut axis = AxisView::default();
        assert_eq!(axis.range(range(10., 30.)), range(10., 30.));
        axis.lock(true, range(10., 30.));
        assert!(axis.locked());
        for automatic in [range(-100., 100.), range(0., 1000.), range(20., 21.)] {
            assert_eq!(axis.range(automatic), range(10., 30.));
            assert!(!axis.pan(5., automatic));
            assert!(!axis.zoom(2., 18., automatic));
            assert!(!axis.fit(automatic));
            assert!(!axis.automatic());
        }
    }

    #[test]
    fn unlocking_keeps_the_visible_range_until_explicit_automatic_fit() {
        let mut axis = AxisView::default();
        axis.lock(true, range(10., 30.));
        axis.lock(false, range(-100., 100.));
        assert!(!axis.locked());
        assert_eq!(axis.range(range(-100., 100.)), range(10., 30.));
        assert!(axis.automatic());
        assert_eq!(axis.range(range(-100., 100.)), range(-100., 100.));
    }

    #[test]
    fn pointer_anchor_does_not_move_when_zooming_then_panning_in_data_units() {
        let mut axis = AxisView::default();
        let automatic = range(10., 30.);
        assert!(axis.zoom(4., 15., automatic));
        let zoomed = axis.range(automatic);
        assert_eq!(zoomed, range(13.75, 18.75));
        assert_eq!(zoomed.fraction(15.), 0.25);
        assert_eq!(zoomed.at(0.25), 15.);
        assert!(axis.pan(-2., automatic));
        assert_eq!(axis.range(automatic), range(11.75, 16.75));
    }

    #[test]
    fn selection_fit_can_update_one_axis_while_the_other_is_locked() {
        let mut x = AxisView::default();
        let mut y = AxisView::default();
        x.lock(true, range(0., 240.));
        assert!(!x.fit(range(60., 90.)));
        assert!(y.fit(Range::fitted(50., 100., 0.1, 1.).unwrap()));
        assert_eq!(x.range(range(60., 90.)), range(0., 240.));
        assert_eq!(y.range(range(0., 1.)), range(45., 105.));
    }

    #[test]
    fn constant_tracks_keep_a_finite_invertible_range_in_the_callers_units() {
        assert_eq!(Range::fitted(100., 100., 0.1, 2.), Some(range(99., 101.)));
        assert_eq!(Range::fitted(0.5, 0.5, 0.1, 0.02), Some(range(0.49, 0.51)));
        assert_eq!(Range::fitted(-10., 30., 0.1, 1.), Some(range(-14., 34.)));
    }

    #[test]
    fn invalid_and_unrepresentable_navigation_preserves_the_previous_view() {
        let mut axis = AxisView::default();
        let automatic = range(10., 30.);
        axis.fit(range(12., 18.));
        let original = axis;
        for factor in [0., -1., f64::NAN, f64::INFINITY, f64::MIN_POSITIVE] {
            assert!(!axis.zoom(factor, 15., automatic));
            assert_eq!(axis, original);
        }
        assert!(!axis.pan(f64::NAN, automatic));
        assert!(!axis.pan(f64::MAX, automatic));
        assert_eq!(axis, original);
        for bounds in [[0., 0.], [1., 0.], [f64::NAN, 1.], [-f64::MAX, f64::MAX]] {
            assert!(Range::new(bounds[0], bounds[1]).is_none());
        }
    }

    #[test]
    fn zero_navigation_does_not_accidentally_capture_an_automatic_axis() {
        let mut axis = AxisView::default();
        let automatic = range(10., 30.);
        assert!(!axis.pan(0., automatic));
        assert!(!axis.zoom(1., 15., automatic));
        assert_eq!(axis, AxisView::default());
        assert_eq!(axis.range(range(20., 80.)), range(20., 80.));
    }
}
