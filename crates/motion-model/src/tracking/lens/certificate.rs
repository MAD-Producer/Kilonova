//! Whole-interval certificate for q <- q + step * (d - F(q)).
//!
//! On a disk, the radial part of J(F-I) is symmetric with eigenvalues
//! k1*s+k2*s²+k3*s³ and 3*k1*s+5*k2*s²+7*k3*s³, s=|q|².
//! The tangential Jacobian has norm <= 8*(|p1|+|p2|)*R. We bound
//! I-step*J(F), retaining B<0.45. Separately step*D+sup|q-step*F(q)|<=R
//! proves self-mapping. Step one retains the original iteration.
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
    fn third(self) -> Self {
        Self((self.0 / 3.).next_down(), (self.1 / 3.).next_up())
    }
    fn abs_upper(self) -> f64 {
        self.0.abs().max(self.1.abs())
    }
}
fn radial(s: Interval, k: [Interval; 3]) -> Interval {
    k[2].mul(s).add(k[1]).mul(s).add(k[0]).mul(s)
}
fn cubic_range(s: Interval, k: [Interval; 3]) -> Interval {
    // A cubic's four Bernstein controls enclose its whole interval by the
    // convex-hull property. Interval endpoints alone would miss interior extrema.
    // Controls are p(a), p(a)+(b-a)*p'(a)/3,
    // p(b)-(b-a)*p'(b)/3, p(b), each with directed arithmetic.
    let derivative = |s: Interval| k[2].scale(3.).mul(s).add(k[1].scale(2.)).mul(s).add(k[0]);
    let a = Interval::point(s.0);
    let b = Interval::point(s.1);
    let width = b.add(a.scale(-1.)).third();
    let pa = radial(a, k);
    let pb = radial(b, k);
    let controls = [
        pa,
        pa.add(width.mul(derivative(a))),
        pb.add(width.mul(derivative(b)).scale(-1.)),
        pb,
    ];
    Interval(
        controls.iter().map(|v| v.0).fold(f64::INFINITY, f64::min),
        controls
            .iter()
            .map(|v| v.1)
            .fold(f64::NEG_INFINITY, f64::max),
    )
}
fn bounds(c: Calibration, radius: f64, step: f64) -> (f64, f64) {
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
        let range = |k| {
            if step == 1. {
                radial(s, k)
            } else {
                cubic_range(s, k)
            }
        };
        let offset = Interval::point(1.).add(Interval::point(-step));
        let value = offset.add(range(k).scale(-step)).abs_upper();
        let derivative_bound = offset.add(range(derivative).scale(-step)).abs_upper();
        jacobian = jacobian.max(value).max(derivative_bound);
        displacement = displacement.max(Interval::point(value).scale(s.1.sqrt().next_up()).1);
    }
    let [p1, p2] = c.tangential();
    let p = Interval::point(p1.abs())
        .add(Interval::point(p2.abs()))
        .scale(step);
    // Each tangential component vector has norm <=3*|pi|*R².
    let displacement = Interval::point(displacement)
        .add(p.scale(3.).mul(r).mul(r))
        .1;
    let jacobian = Interval::point(jacobian).add(p.scale(8.).mul(r)).1;
    (jacobian, displacement)
}
pub(super) fn enclosing_disk(c: Calibration, domain: f64) -> Option<(f64, f64, f64)> {
    // Never broaden the original numeric domain or iteration budget. Failure
    // to prove a disk remains an explicit error, including folding lenses.
    // Exact binary fractions have the same value in the CPU and shader.
    // Search step one first, so existing admitted lenses keep their algorithm.
    for step in [1., 1.25, 1.375, 1.5, 1.625, 1.75, 0.875, 0.75, 0.625, 0.5] {
        for cell in 1..=64 {
            let radius = domain * (1. + f64::from(cell) / 64.);
            let (bound, displacement) = bounds(c, radius, step);
            let image = Interval::point(domain)
                .scale(step)
                .add(Interval::point(displacement));
            if bound.is_finite() && bound < MAX_DERIVATIVE && image.1 <= radius {
                return Some((radius, bound, step));
            }
        }
    }
    None
}
