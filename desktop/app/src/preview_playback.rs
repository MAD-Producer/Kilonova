//! One frame-range policy for the desktop clock and the audio producer.
use motion_core::{FrameScope, Project, WorkArea};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Start {
    #[default]
    Current,
    RangeStart,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Skip {
    #[default]
    None,
    One,
    Two,
    Three,
    Four,
    Five,
}
impl Skip {
    pub(crate) fn step(self) -> u32 {
        self as u32 + 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Options {
    pub scope: FrameScope,
    pub start: Start,
    pub looping: bool,
    pub skip: Skip,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            scope: FrameScope::Composition,
            start: Start::Current,
            looping: true,
            skip: Skip::None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Plan {
    pub range: WorkArea,
    pub fps: u32,
    pub origin: f64,
    pub looping: bool,
    pub step: u32,
}

impl Plan {
    pub(crate) fn new(project: &Project, options: Options, current: f64) -> Result<Self, String> {
        if !current.is_finite() {
            return Err("preview start must be finite".into());
        }
        if !(1..=motion_core::MAX_COMPOSITION_FPS).contains(&project.fps) {
            return Err("invalid preview frame rate".into());
        }
        let range = project.frame_range(options.scope);
        range
            .validate(project.frames)
            .map_err(|error| error.to_string())?;
        let origin = if options.start == Start::RangeStart
            || current < f64::from(range.in_frame)
            || current >= f64::from(range.out_frame)
        {
            f64::from(range.in_frame)
        } else {
            current
        };
        Ok(Self {
            range,
            fps: project.fps,
            origin,
            looping: options.looping,
            step: options.skip.step(),
        })
    }

    /// Advances source time independently of presentation frequency. Skipping
    /// drops visual samples, never PCM or elapsed composition time.
    pub(crate) fn position(self, elapsed: Duration) -> (f64, bool) {
        let source = self.origin + elapsed.as_secs_f64() * f64::from(self.fps);
        let start = f64::from(self.range.in_frame);
        let end = f64::from(self.range.out_frame);
        let finished = !self.looping && source >= end;
        let source = if self.looping {
            start + (source - start).rem_euclid(end - start)
        } else {
            source.min(end - 1.)
        };
        (
            if finished {
                end - 1.
            } else {
                self.display_frame(source)
            },
            finished,
        )
    }

    pub(crate) fn display_frame(self, source: f64) -> f64 {
        if self.step == 1 {
            return source;
        }
        let start = f64::from(self.range.in_frame);
        start + ((source - start) / f64::from(self.step)).floor() * f64::from(self.step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> Project {
        let mut project = Project::new(64, 64, 30, 120).unwrap();
        project.work_area = WorkArea {
            in_frame: 20,
            out_frame: 50,
        };
        project
    }
    #[test]
    fn current_start_range_start_and_outside_current_have_explicit_boundaries() {
        let project = project();
        let options = Options {
            scope: FrameScope::WorkArea,
            ..Default::default()
        };
        let plan = Plan::new(&project, options, 29.5).unwrap();
        assert_eq!(plan.position(Duration::ZERO), (29.5, false));
        assert_eq!(Plan::new(&project, options, 50.).unwrap().origin, 20.);
        assert_eq!(Plan::new(&project, options, 0.).unwrap().origin, 20.);
        assert_eq!(
            Plan::new(
                &project,
                Options {
                    start: Start::RangeStart,
                    ..options
                },
                29.5
            )
            .unwrap()
            .origin,
            20.
        );
        assert_eq!(
            Plan::new(&project, Options::default(), 90.).unwrap().origin,
            90.
        );
        assert!(Plan::new(&project, options, f64::NAN).is_err());
    }
    #[test]
    fn loop_and_stop_use_exclusive_out_edge_without_a_duplicate_endpoint() {
        let project = project();
        let options = Options {
            scope: FrameScope::WorkArea,
            ..Default::default()
        };
        let plan = Plan::new(&project, options, 20.).unwrap();
        assert_eq!(plan.position(Duration::from_secs(1)), (20., false));
        assert_eq!(plan.position(Duration::from_secs(101)), (20., false));
        let once = Plan::new(
            &project,
            Options {
                looping: false,
                ..options
            },
            20.,
        )
        .unwrap();
        assert_eq!(once.position(Duration::from_secs(1)), (49., true));
        assert_eq!(once.position(Duration::from_secs(10)), (49., true));
    }
    #[test]
    fn skipping_changes_visual_samples_without_slowing_source_time() {
        let project = project();
        for skip in [
            Skip::None,
            Skip::One,
            Skip::Two,
            Skip::Three,
            Skip::Four,
            Skip::Five,
        ] {
            let options = Options {
                scope: FrameScope::WorkArea,
                skip,
                ..Default::default()
            };
            let plan = Plan::new(&project, options, 20.).unwrap();
            let (frame, ended) = plan.position(Duration::from_millis(900));
            assert_eq!(frame, 20. + (27 / skip.step() * skip.step()) as f64);
            assert!(!ended);
            assert_eq!(plan.position(Duration::from_secs(1)), (20., false));
        }
    }
}
