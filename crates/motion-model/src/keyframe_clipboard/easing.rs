//! Quick easing edits intervals incident to selected authored keys. Adjacent
//! selected keys contribute to the same interval before any write occurs.
use super::*;
use crate::Easing;
use std::collections::BTreeMap;

pub fn ease_keys(
    project: &mut Project,
    selections: &[KeyframeSelection],
    incoming: bool,
    outgoing: bool,
) -> Result<()> {
    ensure(
        incoming || outgoing,
        "choose at least one keyframe easing side",
    )?;
    let frames = interpolation::preflight(project, selections, "quick_ease")?;
    let mut plans = Vec::with_capacity(selections.len());
    for (selected, frames) in selections.iter().zip(&frames) {
        let plan = (|| -> Result<_> {
            macro_rules! plan {
                ($t:expr) => {
                    intervals(&$t.keys, frames, incoming, outgoing, |i| $t.effective_easing(i))
                };
            }
            match read(project, &selected.target)? {
                ReadChannel::Scalar(t) => plan!(t),
                ReadChannel::Vec2(t) => plan!(t),
                ReadChannel::Vec3(t) => plan!(t),
                ReadChannel::Vec4(t) => plan!(t),
                ReadChannel::Geometry(t) => plan!(t),
                ReadChannel::ColorCurve(t) => plan!(t),
            }
        })()
        .map_err(|cause| interpolation::failure("quick_ease", selected, cause))?;
        plans.push(plan);
    }
    for (selected, plan) in selections.iter().zip(plans) {
        let result = (|| -> Result<()> {
            macro_rules! edit {
                ($t:expr) => {
                    for (frame, easing, start, end) in plan {
                        $t.set_curve_sides(frame, easing, start, end)?;
                    }
                };
            }
            match write(project, &selected.target)? {
                WriteChannel::Scalar(t) => edit!(t),
                WriteChannel::Vec2(t) => edit!(t),
                WriteChannel::Vec3(t) => edit!(t),
                WriteChannel::Vec4(t) => edit!(t),
                WriteChannel::Geometry(t) => edit!(t),
                WriteChannel::ColorCurve(t) => {
                    for (frame, easing, _, _) in plan {
                        let index =
                            t.keys
                                .binary_search_by_key(&frame, |k| k.frame)
                                .map_err(|_| {
                                    Error::Invalid("keyframe not found after preflight".into())
                                })?;
                        t.keys[index].ease = easing.ease;
                        t.keys[index].curve = easing.curve;
                    }
                }
            }
            Ok(())
        })();
        result.map_err(|cause| interpolation::failure("quick_ease", selected, cause))?;
    }
    Ok(())
}

fn intervals<T>(
    keys: &[Keyframe<T>],
    frames: &[i32],
    incoming: bool,
    outgoing: bool,
    effective: impl Fn(usize) -> Result<Easing>,
) -> Result<Vec<(i32, Easing, bool, bool)>> {
    let mut sides: BTreeMap<usize, (bool, bool)> = BTreeMap::new();
    for frame in frames {
        let index = keys
            .binary_search_by_key(frame, |key| key.frame)
            .map_err(|_| Error::Invalid("selected keyframe does not exist".into()))?;
        if incoming {
            if let Some(index) = index.checked_sub(1) {
                sides.entry(index).or_default().0 = true;
            }
        }
        if outgoing && index + 1 < keys.len() {
            sides.entry(index).or_default().1 = true;
        }
    }
    sides
        .into_iter()
        .map(|(index, (incoming, outgoing))| {
            let key = &keys[index];
            // The interval can start before composition zero while its selected end
            // is visible. Local addressing preserves that valid authored interval.
            let easing = effective(index)?.ease_sides(incoming, outgoing);
            easing.validate()?;
            Ok((key.frame, easing, outgoing, incoming))
        })
        .collect()
}

pub fn delete_keys(project: &mut Project, selections: &[KeyframeSelection]) -> Result<()> {
    // The same preflight rejects duplicate aliases and unsorted frames before
    // the existing transfer removal checks and atomic channel writes.
    interpolation::preflight(project, selections, "delete")?;
    let keys = KeyframeClipboard::capture(project, selections)?;
    remove(project, &keys)
}
