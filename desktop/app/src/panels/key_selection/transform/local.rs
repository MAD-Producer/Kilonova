//! One common composition-time lattice for integer authored keys. Computing
//! each channel's rounded delta independently would drift a mixed selection.
use super::*;
use motion_core::layer_time::LayerClock;

#[derive(Clone, Copy)]
pub(super) struct Grid {
    numerator: u64,
    denominator: u64,
}
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
impl Grid {
    pub fn identity() -> Self {
        Self {
            numerator: 1,
            denominator: 1,
        }
    }
    fn ratio(clock: LayerClock) -> Self {
        let [a, b] = clock.stretch().ratio();
        Self {
            numerator: u64::from(a),
            denominator: u64::from(b),
        }
    }
    fn common(self, other: Self) -> motion_core::Result<Self> {
        let numerator = (self.numerator / gcd(self.numerator, other.numerator))
            .checked_mul(other.numerator)
            .ok_or_else(|| {
                Error::Invalid("selected layer clocks exceed the common timing grid range".into())
            })?;
        Ok(Self {
            numerator,
            denominator: gcd(self.denominator, other.denominator),
        })
    }
    pub fn scale(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
    fn local_stride(self, clock: LayerClock) -> motion_core::Result<i32> {
        let [a, b] = clock.stretch().ratio();
        let numerator = u128::from(self.numerator) * u128::from(b);
        let denominator = u128::from(self.denominator) * u128::from(a);
        if numerator % denominator != 0 {
            return Err(Error::Invalid(
                "selected layer clocks have an inconsistent timing grid".into(),
            ));
        }
        let stride = i32::try_from(numerator / denominator).map_err(|_| {
            Error::Invalid("common timing grid exceeds the local key delta range".into())
        })?;
        Ok(if clock.rate() < 0. { -stride } else { stride })
    }
}

pub(super) struct Channel {
    target: KeyframeTarget,
    selected: BTreeSet<i32>,
    authored: BTreeSet<i32>,
    clock: LayerClock,
    offset: i32,
    stride: i32,
    times: Vec<f64>,
}
pub(super) struct Local {
    channels: BTreeMap<String, Channel>,
    pub grid: Grid,
}
impl Local {
    pub fn capture(selected: &Set, state: &Value, bytes: &mut usize) -> motion_core::Result<Self> {
        let mut channels = BTreeMap::new();
        // Frame-addressed and spatial commands retain their composition-frame
        // grid. Local-only selections can retain a fractional common step.
        let mut grid = selected
            .channels
            .values()
            .any(|c| !c.frames.is_empty() || !c.ids.is_empty())
            .then_some(Grid::identity());
        for (address, selection) in &selected.channels {
            if selection.local_frames.is_empty() {
                continue;
            }
            let target = selection
                .target
                .as_ref()
                .ok_or_else(|| Error::Invalid("local key transform target missing".into()))?;
            let (track, offset) = source::track(state, target).ok_or_else(|| {
                Error::Invalid("local key transform authored channel missing".into())
            })?;
            if track["spatial_animation"].is_object() {
                return Err(Error::Invalid(
                    "spatial keys require their persistent identity transform".into(),
                ));
            }
            let offset = i32::try_from(offset).map_err(|_| {
                Error::Invalid("local key transform clip offset exceeds i32".into())
            })?;
            let clock = source::clock(state, target)?;
            let ratio = Grid::ratio(clock);
            grid = Some(match grid {
                Some(previous) => previous.common(ratio)?,
                None => ratio,
            });
            let keys = track["keys"].as_array().ok_or_else(|| {
                Error::Invalid("local key transform authored keys missing".into())
            })?;
            *bytes = bytes
                .saturating_add(keys.len().saturating_mul(std::mem::size_of::<i32>()))
                .saturating_add(address.len());
            if *bytes > MAX_KEYFRAME_CLIPBOARD_BYTES {
                return Err(Error::Invalid(
                    "key transform occupancy exceeds metadata limit".into(),
                ));
            }
            let mut authored = BTreeSet::new();
            let mut previous = None;
            for key in keys {
                let frame = key["frame"]
                    .as_i64()
                    .and_then(|f| i32::try_from(f).ok())
                    .ok_or_else(|| {
                        Error::Invalid("local key transform authored time is not i32".into())
                    })?;
                if previous.is_some_and(|p| p >= frame) {
                    return Err(Error::Invalid(
                        "local key transform authored times are not strictly increasing".into(),
                    ));
                }
                previous = Some(frame);
                authored.insert(frame);
            }
            if !selection.local_frames.is_subset(&authored) {
                return Err(Error::Invalid(
                    "local key transform selected identity missing".into(),
                ));
            }
            let times = selection
                .local_frames
                .iter()
                .map(|f| clock.projected_composition_frame(f64::from(*f), offset))
                .collect::<motion_core::Result<_>>()?;
            channels.insert(
                address.clone(),
                Channel {
                    target: target.clone(),
                    selected: selection.local_frames.clone(),
                    authored,
                    clock,
                    offset,
                    stride: 0,
                    times,
                },
            );
        }
        let grid = grid.unwrap_or_else(Grid::identity);
        for c in channels.values_mut() {
            c.stride = grid.local_stride(c.clock)?;
        }
        Ok(Self { channels, grid })
    }
    pub fn allows(&self, steps: i32, total: u32, copy: bool) -> bool {
        self.channels.values().all(|c| {
            let Some(delta) = c.stride.checked_mul(steps) else {
                return false;
            };
            c.selected.iter().all(|from| {
                let Some(to) = from.checked_add(delta) else {
                    return false;
                };
                if c.authored.contains(&to) && (copy || !c.selected.contains(&to)) {
                    return false;
                }
                c.clock
                    .projected_composition_frame(f64::from(to), c.offset)
                    .is_ok_and(|time| time >= 0. && time < f64::from(total))
            })
        })
    }
    pub fn commands(
        &self,
        steps: i32,
        value: Option<&motion_core::KeyframeValueOffset>,
        copy: bool,
    ) -> motion_core::Result<Vec<motion_core::Command>> {
        self.channels
            .values()
            .map(|c| {
                let delta_frame = c
                    .stride
                    .checked_mul(steps)
                    .ok_or_else(|| Error::Invalid("local key transform delta overflow".into()))?;
                Ok(motion_core::Command::LocalKeyframes {
                    target: c.target.clone(),
                    edit: motion_core::LocalKeyEdit::Translate {
                        frames: c.selected.iter().copied().collect(),
                        delta_frame,
                        copy,
                        value_offset: value.filter(|v| v.target == c.target).map(|v| {
                            motion_core::LocalValueOffset {
                                component: v.component,
                                delta: v.delta,
                            }
                        }),
                    },
                })
            })
            .collect()
    }
    pub fn translated(&self, selection: &mut Set, steps: i32) -> motion_core::Result<()> {
        for (address, c) in &self.channels {
            let delta = c
                .stride
                .checked_mul(steps)
                .ok_or_else(|| Error::Invalid("local key transform delta overflow".into()))?;
            let shifted = c
                .selected
                .iter()
                .map(|f| {
                    f.checked_add(delta)
                        .ok_or_else(|| Error::Invalid("local key selection time overflow".into()))
                })
                .collect::<motion_core::Result<_>>()?;
            let channel = selection.channels.get_mut(address).ok_or_else(|| {
                Error::Invalid("local key transform selection channel missing".into())
            })?;
            channel.local_frames = shifted;
            if let Some(primary) = &mut selection.local_primary {
                if primary.target == c.target {
                    primary.frame = primary
                        .frame
                        .checked_add(delta)
                        .ok_or_else(|| Error::Invalid("local primary key time overflow".into()))?;
                }
            }
        }
        Ok(())
    }
    pub fn times(&self) -> impl Iterator<Item = f64> + '_ {
        self.channels.values().flat_map(|c| c.times.iter().copied())
    }
}
