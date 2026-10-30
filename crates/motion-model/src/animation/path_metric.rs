//! Immutable geometric preparation for continuous spatial timing. Compile after
//! an authored edit; distance queries allocate nothing and never change keys.
//! This does not enable Roving or turn integer author addresses into key times.
use super::*;

type Point = [f64; 3];
type Cubic = [Point; 4];
const MAX_INTERVALS: usize = 16_384;
const MAX_NODES: usize = 65_536;
const MAX_DEPTH: usize = 24;

#[derive(Clone, Copy, Debug)]
pub struct SpatialPathBudget {
    /// Limits apply to the entire prepared path, not independently per segment.
    pub intervals: usize,
    pub nodes: usize,
    pub subdivisions: usize,
    pub depth: usize,
    /// World-coordinate length error and parameter-to-position error bounds.
    /// A relative bound remains meaningful for tiny and very large f32 paths.
    pub relative_tolerance: f64,
    pub absolute_tolerance: f64,
}
impl Default for SpatialPathBudget {
    fn default() -> Self {
        Self {
            intervals: MAX_INTERVALS,
            nodes: MAX_NODES,
            subdivisions: MAX_NODES,
            depth: MAX_DEPTH,
            relative_tolerance: 0.00001,
            absolute_tolerance: 0.,
        }
    }
}
impl SpatialPathBudget {
    fn validate(self) -> Result<()> {
        ensure(
            (1..=MAX_INTERVALS).contains(&self.intervals),
            "spatial path interval budget out of range",
        )?;
        ensure(
            (2..=MAX_NODES).contains(&self.nodes),
            "spatial path node budget out of range",
        )?;
        ensure(
            self.subdivisions <= MAX_NODES && self.depth <= MAX_DEPTH,
            "spatial path subdivision budget out of range",
        )?;
        ensure(
            self.relative_tolerance.is_finite()
                && self.relative_tolerance >= 0.
                && self.relative_tolerance <= 0.01
                && self.absolute_tolerance.is_finite()
                && self.absolute_tolerance >= 0.
                && (self.relative_tolerance > 0. || self.absolute_tolerance > 0.),
            "spatial path tolerance must be finite and positive",
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct Node {
    parameter: f64,
    distance: f64,
}
#[derive(Clone, Debug)]
struct Interval {
    cubic: Cubic,
    nodes: Vec<Node>,
    length: f64,
    linear: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialPathSample {
    pub interval: usize,
    pub parameter: f64,
    pub position: [f64; 3],
}

/// Author indices remain distinct even when repeated positions produce equal
/// effective times. No rounding, epsilon durations or key deletion is applied.
#[derive(Clone, Debug)]
pub struct SpatialPath {
    intervals: Vec<Interval>,
    cumulative: Vec<f64>,
    authored: Vec<i32>,
}

fn distance(a: Point, b: Point) -> f64 {
    a.into_iter()
        .zip(b)
        .fold(0., |length, (a, b)| length.hypot(b - a))
}
fn mix(a: Point, b: Point, t: f64) -> Point {
    std::array::from_fn(|i| a[i] * (1. - t) + b[i] * t)
}
fn split(c: Cubic) -> (Cubic, Cubic) {
    let a = mix(c[0], c[1], 0.5);
    let b = mix(c[1], c[2], 0.5);
    let d = mix(c[2], c[3], 0.5);
    let e = mix(a, b, 0.5);
    let f = mix(b, d, 0.5);
    let m = mix(e, f, 0.5);
    ([c[0], a, e, m], [m, f, d, c[3]])
}
fn point(c: Cubic, t: f64) -> Point {
    let a = mix(c[0], c[1], t);
    let b = mix(c[1], c[2], t);
    let d = mix(c[2], c[3], t);
    mix(mix(a, b, t), mix(b, d, t), t)
}
fn polygon(c: Cubic) -> f64 {
    distance(c[0], c[1]) + distance(c[1], c[2]) + distance(c[2], c[3])
}

#[derive(Clone, Copy, Default)]
struct Piece {
    cubic: Cubic,
    start: f64,
    end: f64,
    depth: usize,
}

fn push(nodes: &mut Vec<Node>, node: Node, used: &mut usize, limit: usize) -> Result<()> {
    ensure(*used < limit, "spatial path node budget exhausted")?;
    nodes
        .try_reserve(1)
        .map_err(|e| crate::Error::Invalid(format!("spatial path node allocation: {e}")))?;
    nodes.push(node);
    *used += 1;
    Ok(())
}

impl Interval {
    fn line(a: Point, b: Point, budget: SpatialPathBudget, used: &mut usize) -> Result<Self> {
        let length = distance(a, b);
        let mut nodes = Vec::new();
        push(
            &mut nodes,
            Node {
                parameter: 0.,
                distance: 0.,
            },
            used,
            budget.nodes,
        )?;
        push(
            &mut nodes,
            Node {
                parameter: 1.,
                distance: length,
            },
            used,
            budget.nodes,
        )?;
        Ok(Self {
            cubic: [a, mix(a, b, 1. / 3.), mix(a, b, 2. / 3.), b],
            nodes,
            length,
            linear: true,
        })
    }
    fn compile(
        cubic: Cubic,
        budget: SpatialPathBudget,
        used: &mut usize,
        splits: &mut usize,
    ) -> Result<Self> {
        let mut nodes = Vec::new();
        push(
            &mut nodes,
            Node {
                parameter: 0.,
                distance: 0.,
            },
            used,
            budget.nodes,
        )?;
        let tolerance = polygon(cubic) * budget.relative_tolerance + budget.absolute_tolerance;
        let mut stack = [Piece::default(); MAX_DEPTH + 1];
        stack[0] = Piece {
            cubic,
            start: 0.,
            end: 1.,
            depth: 0,
        };
        let mut count = 1;
        let mut length = 0.;
        while count > 0 {
            count -= 1;
            let piece = stack[count];
            let c = piece.cubic;
            let poly = polygon(c);
            let chord = distance(c[0], c[3]);
            // Arc flatness alone misses collinear nonuniform cubics (t^3).
            // Bound their parameterization error before using a linear LUT.
            let parameter_error = distance(c[1], mix(c[0], c[3], 1. / 3.))
                .max(distance(c[2], mix(c[0], c[3], 2. / 3.)));
            if (poly - chord).max(0.) <= 2. * tolerance * (piece.end - piece.start)
                && parameter_error <= tolerance
            {
                length += (poly + chord) * 0.5;
                push(
                    &mut nodes,
                    Node {
                        parameter: piece.end,
                        distance: length,
                    },
                    used,
                    budget.nodes,
                )?;
                continue;
            }
            ensure(
                piece.depth < budget.depth,
                "spatial path depth exhausted before tolerance was met",
            )?;
            ensure(
                *splits < budget.subdivisions,
                "spatial path subdivision budget exhausted",
            )?;
            *splits += 1;
            let (left, right) = split(c);
            let middle = (piece.start + piece.end) * 0.5;
            // DFS retains at most one pending sibling for each bounded depth.
            ensure(
                count + 2 <= stack.len(),
                "spatial path subdivision stack exhausted",
            )?;
            stack[count] = Piece {
                cubic: right,
                start: middle,
                end: piece.end,
                depth: piece.depth + 1,
            };
            stack[count + 1] = Piece {
                cubic: left,
                start: piece.start,
                end: middle,
                depth: piece.depth + 1,
            };
            count += 2;
        }
        ensure(length.is_finite(), "spatial path length is non-finite")?;
        Ok(Self {
            cubic,
            nodes,
            length,
            linear: false,
        })
    }
    fn position(&self, parameter: f64) -> Point {
        if self.linear {
            mix(self.cubic[0], self.cubic[3], parameter)
        } else {
            point(self.cubic, parameter)
        }
    }
    fn parameter(&self, distance: f64) -> f64 {
        if distance <= 0. {
            return 0.;
        }
        if distance >= self.length {
            return 1.;
        }
        let upper = self.nodes.partition_point(|n| n.distance <= distance);
        let a = self.nodes[upper - 1];
        let b = self.nodes[upper];
        a.parameter
            + (b.parameter - a.parameter) * (distance - a.distance) / (b.distance - a.distance)
    }
}

impl<T: Tween> Track<T> {
    /// Geometric preparation only. Hosts must separately verify that this track
    /// belongs to a spatial property; numeric storage alone cannot prove that.
    pub fn prepare_spatial_path(&self, budget: SpatialPathBudget) -> Result<SpatialPath> {
        budget.validate()?;
        ensure(
            T::spatial_dimensions().is_some(),
            "spatial timing requires coupled 2D/3D positions",
        )?;
        ensure(
            self.axes.is_none(),
            "spatial timing requires coupled dimensions",
        )?;
        ensure(
            self.keys.len() >= 2,
            "spatial timing requires at least two keys",
        )?;
        ensure(
            self.keys.len() - 1 <= budget.intervals,
            "spatial path interval budget exhausted",
        )?;
        self.validate_local()?;
        let position = |value: T| -> Result<Point> {
            value
                .spatial_components()
                .map(|p| p.map(f64::from))
                .ok_or_else(|| crate::Error::Invalid("spatial position components missing".into()))
        };
        let mut intervals = Vec::new();
        let mut cumulative = Vec::new();
        let mut authored = Vec::new();
        intervals
            .try_reserve_exact(self.keys.len() - 1)
            .and_then(|_| cumulative.try_reserve_exact(self.keys.len()))
            .and_then(|_| authored.try_reserve_exact(self.keys.len()))
            .map_err(|e| crate::Error::Invalid(format!("spatial path allocation: {e}")))?;
        cumulative.push(0.);
        authored.extend(self.keys.iter().map(|k| k.frame));
        let mut total = 0.;
        let mut used = 0;
        let mut splits = 0;
        for (index, pair) in self.keys.windows(2).enumerate() {
            let a = &pair[0];
            let b = &pair[1];
            let p0 = position(a.value)?;
            let p3 = position(b.value)?;
            let p1 = match a.spatial.as_ref().and_then(|s| s.outgoing) {
                Some(offset) => position(a.value.add(offset))?,
                None => position(a.value.mix(b.value, 1. / 3.))?,
            };
            let p2 = match b.spatial.as_ref().and_then(|s| s.incoming) {
                Some(offset) => position(b.value.add(offset))?,
                None => position(a.value.mix(b.value, 2. / 3.))?,
            };
            let linear = a.spatial.as_ref().and_then(|s| s.outgoing).is_none()
                && b.spatial.as_ref().and_then(|s| s.incoming).is_none();
            // The author sampler directly mixes an interval without handles;
            // do not turn its exact line into rounded implicit cubic handles.
            let prepared = if linear {
                Interval::line(p0, p3, budget, &mut used)
            } else {
                Interval::compile([p0, p1, p2, p3], budget, &mut used, &mut splits)
            };
            let interval = prepared.map_err(|e| {
                crate::Error::Invalid(format!("spatial path interval={index}: {e}"))
            })?;
            total += interval.length;
            cumulative.push(total);
            intervals.push(interval);
        }
        Ok(SpatialPath {
            intervals,
            cumulative,
            authored,
        })
    }
}

impl SpatialPath {
    pub fn total_length(&self) -> f64 {
        *self.cumulative.last().unwrap_or(&0.)
    }
    pub fn node_count(&self) -> usize {
        self.intervals.iter().map(|i| i.nodes.len()).sum()
    }
    pub fn interval_length(&self, index: usize) -> Result<f64> {
        self.intervals
            .get(index)
            .map(|i| i.length)
            .ok_or_else(|| crate::Error::Invalid("spatial path interval missing".into()))
    }
    /// Query a segment directly, without subtracting whole-path prefixes. This
    /// retains local precision when a short segment follows a very long one.
    pub fn sample_interval_distance(
        &self,
        interval: usize,
        distance: f64,
    ) -> Result<SpatialPathSample> {
        let segment = self
            .intervals
            .get(interval)
            .ok_or_else(|| crate::Error::Invalid("spatial path interval missing".into()))?;
        ensure(
            distance.is_finite() && distance >= 0. && distance <= segment.length,
            "spatial sample distance is outside the finite interval",
        )?;
        let parameter = segment.parameter(distance);
        Ok(SpatialPathSample {
            interval,
            parameter,
            position: segment.position(parameter),
        })
    }
    /// Continuous constant-speed candidate times, retaining one entry for each
    /// author key in the requested span. Temporal endpoint easing is a separate
    /// policy; this API does not silently apply it or mutate authored storage.
    pub fn constant_speed_times(
        &self,
        first: usize,
        last: usize,
        start: f64,
        end: f64,
    ) -> Result<Vec<f64>> {
        ensure(
            first < last && last < self.authored.len(),
            "spatial timing span is invalid",
        )?;
        ensure(
            start.is_finite() && end.is_finite() && start < end && (end - start).is_finite(),
            "spatial timing endpoints must be finite and increasing",
        )?;
        // Sum the local span: subtracting two huge whole-path prefixes can lose
        // the length of a tiny later interval completely.
        let total = self.intervals[first..last]
            .iter()
            .map(|i| i.length)
            .sum::<f64>();
        let mut times = Vec::new();
        times
            .try_reserve_exact(last - first + 1)
            .map_err(|e| crate::Error::Invalid(format!("spatial timing allocation: {e}")))?;
        let mut traveled = 0.;
        times.push(start);
        for index in first + 1..last {
            traveled += self.intervals[index - 1].length;
            let offset = if total > 0. {
                // Retain precision for simple rational positions by scaling
                // before division. Fall back when that intermediate would
                // overflow or underflow even though the final time is valid.
                let product = (end - start) * traveled;
                if product.is_finite() && (product >= f64::MIN_POSITIVE || traveled == 0.) {
                    product / total
                } else {
                    (end - start) * (traveled / total)
                }
            } else {
                (end - start)
                    * ((f64::from(self.authored[index]) - f64::from(self.authored[first]))
                        / (f64::from(self.authored[last]) - f64::from(self.authored[first])))
            };
            times.push(start + offset);
        }
        times.push(end);
        Ok(times)
    }
    /// Allocation-free arc-distance lookup. A zero-length path stays stationary.
    pub fn sample_distance(&self, distance: f64) -> Result<SpatialPathSample> {
        ensure(
            distance.is_finite() && distance >= 0. && distance <= self.total_length(),
            "spatial sample distance is outside the finite path",
        )?;
        if distance > 0. && distance == self.total_length() {
            let interval = self.intervals.len() - 1;
            return Ok(SpatialPathSample {
                interval,
                parameter: 1.,
                position: self.intervals[interval].cubic[3],
            });
        }
        let interval = if distance == 0. {
            0
        } else {
            self.cumulative
                .partition_point(|v| *v <= distance)
                .saturating_sub(1)
                .min(self.intervals.len() - 1)
        };
        self.sample_interval_distance(interval, distance - self.cumulative[interval])
    }
}
