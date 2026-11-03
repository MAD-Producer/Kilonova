//! Whole-interval certificate for the unchanged q = d - (F(q) - q) iteration.
//!
//! On a disk, the radial part of J(F-I) is symmetric with eigenvalues
//! k1*s+k2*s²+k3*s³ and 3*k1*s+5*k2*s²+7*k3*s³, s=|q|².
//! The tangential Jacobian has norm <= 8*(|p1|+|p2|)*R. Thus B<0.45
//! proves contraction. Separately D+sup|F(q)-q|<=R proves self-mapping.
//! Both must hold: a derivative bound alone does not prove inverse coverage.
use super::{Calibration, MAX_DERIVATIVE};

// Directed rounding encloses the complete polynomial interval, including
// cancellation. Cells cover a continuum; their endpoints are not test samples.
#[derive(Clone, Copy)]
struct Interval(f64, f64);
impl Interval {
    fn point(v: f64) -> Self {
        Self(v, v)
    }
    fn add(self, other: Self) -> Self {
        Self((self.0 + other.0).next_down(), (self.1 + other.1).next_up())
    }
    fn mul(self, other: Self) -> Self {
        let p = [
            self.0 * other.0,
            self.0 * other.1,
            self.1 * other.0,
            self.1 * other.1,
        ];
        Self(
            p.into_iter().fold(f64::INFINITY, f64::min).next_down(),
            p.into_iter().fold(f64::NEG_INFINITY, f64::max).next_up(),
        )
    }
    fn scale(self, v: f64) -> Self {
        self.mul(Self::point(v))
    }
    fn abs_upper(self) -> f64 {
        self.0.abs().max(self.1.abs())
    }
}
fn radial(s: Interval, k: [Interval; 3]) -> Interval {
    k[2].mul(s).add(k[1]).mul(s).add(k[0]).mul(s)
}
fn bounds(c: Calibration, radius: f64) -> (f64, f64) {
    let k = c.radial().map(Interval::point);
    let derivative = [k[0].scale(3.), k[1].scale(5.), k[2].scale(7.)];
    let r = Interval::point(radius);
    let r2 = r.mul(r).1;
    let mut jacobian = 0f64;
    let mut displacement = 0f64;
    for cell in 0..32 {
        let s = Interval(
            (r2 * f64::from(cell) / 32.).next_down().max(0.),
            (r2 * f64::from(cell + 1) / 32.).next_up(),
        );
        let value = radial(s, k).abs_upper();
        jacobian = jacobian.max(value).max(radial(s, derivative).abs_upper());
        displacement = displacement.max(Interval::point(value).scale(s.1.sqrt().next_up()).1);
    }
    let [p1, p2] = c.tangential();
    let p = Interval::point(p1.abs()).add(Interval::point(p2.abs()));
    // Each tangential component vector has norm <=3*|pi|*R².
    let displacement = Interval::point(displacement)
        .add(p.scale(3.).mul(r).mul(r))
        .1;
    let jacobian = Interval::point(jacobian).add(p.scale(8.).mul(r)).1;
    (jacobian, displacement)
}
pub(super) fn enclosing_disk(c: Calibration, domain: f64) -> Option<(f64, f64)> {
    // Never broaden the original numeric domain or iteration budget. Failure
    // to prove a disk remains an explicit error, including folding lenses.
    for step in 1..=64 {
        let radius = domain * (1. + f64::from(step) / 64.);
        let (bound, displacement) = bounds(c, radius);
        if bound.is_finite()
            && bound < MAX_DERIVATIVE
            && (domain + displacement).next_up() <= radius
        {
            return Some((radius, bound));
        }
    }
    None
}
