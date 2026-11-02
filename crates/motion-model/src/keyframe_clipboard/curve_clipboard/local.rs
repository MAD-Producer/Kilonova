//! Authored integer addresses stay local even when their screen time is fractional.
use super::*;

impl CurveClipboard {
    /// A single atomic command may contain both address spaces, but never two
    /// aliases for the same channel. Preflight every interval before any write.
    pub fn paste_all(
        &self,
        project: &mut Project,
        selections: &[KeyframeSelection],
        local: &[LocalKeyframeSelection],
    ) -> Result<()> {
        if local.is_empty() {
            return self.paste(project, selections);
        }
        self.validate()?;
        ensure(
            crate::serialized_bytes(&(selections, local)) <= MAX_KEYFRAME_CLIPBOARD_BYTES,
            "easing selection exceeds metadata limit",
        )?;
        let legacy = if selections.is_empty() {
            vec![]
        } else {
            interpolation::preflight(project, selections, "paste_easing")?
        };
        let mut all: Vec<_> = selections
            .iter()
            .zip(legacy)
            .map(|(selection, mut frames)| {
                // Reverse clocks invert legacy composition order, not storage order.
                frames.sort_unstable();
                LocalKeyframeSelection {
                    target: selection.target.clone(),
                    frames,
                }
            })
            .collect();
        all.extend_from_slice(local);
        let mut seen = BTreeSet::new();
        let mut plans = Vec::with_capacity(all.len());
        for selection in &all {
            let plan = (|| {
                selection.target.editable(project)?;
                selection.target.signature(project)?;
                ensure(
                    seen.insert(selection.target.address()?),
                    "duplicate easing channel across address spaces",
                )?;
                ensure(
                    !selection.frames.is_empty(),
                    "no keyframes selected in channel",
                )?;
                ensure(
                    selection.frames.windows(2).all(|pair| pair[0] < pair[1]),
                    "local easing frames must be sorted and unique",
                )?;
                if let KeyframeTarget::Effect {
                    object,
                    effect,
                    param,
                } = &selection.target
                {
                    let parameter = project
                        .layers
                        .iter()
                        .find(|layer| layer.id == *object)
                        .and_then(|layer| {
                            layer.effects.iter().find(|instance| instance.id == *effect)
                        })
                        .and_then(|instance| instance.params.get(param))
                        .ok_or_else(|| Error::Invalid("easing effect parameter missing".into()))?;
                    ensure(
                        !parameter.kind.discrete()
                            || self.easing
                                == Easing {
                                    ease: crate::Ease::Hold,
                                    curve: None,
                                },
                        "boolean and enum parameters support hold interpolation only",
                    )?;
                }
                macro_rules! check {
                    ($track:expr) => {{
                        ensure(
                            $track.axes.is_none(),
                            "choose a separated keyframe axis explicitly",
                        )?;
                        ensure(
                            $track.spatial_animation.is_none(),
                            "spatial easing requires stable key identities",
                        )?;
                        outgoing(&$track.keys, &selection.frames)
                    }};
                }
                match read(project, &selection.target)? {
                    ReadChannel::Scalar(t) => check!(t),
                    ReadChannel::Vec2(t) => check!(t),
                    ReadChannel::Vec3(t) => check!(t),
                    ReadChannel::Vec4(t) => check!(t),
                    ReadChannel::Geometry(t) => check!(t),
                    ReadChannel::ColorCurve(t) => outgoing(&t.keys, &selection.frames),
                }
            })()
            .map_err(|cause| selection.target.failure("paste_easing_local", cause))?;
            plans.push(plan);
        }
        // All writable targets and outgoing keys were resolved above. Core's
        // transaction additionally validates the complete project and rolls back.
        for (selection, frames) in all.iter().zip(plans) {
            (|| {
                macro_rules! edit {
                    ($track:expr) => {
                        for frame in frames {
                            $track.set_curve(frame, self.easing)?;
                        }
                    };
                }
                match write(project, &selection.target)? {
                    WriteChannel::Scalar(t) => edit!(t),
                    WriteChannel::Vec2(t) => edit!(t),
                    WriteChannel::Vec3(t) => edit!(t),
                    WriteChannel::Vec4(t) => edit!(t),
                    WriteChannel::Geometry(t) => edit!(t),
                    WriteChannel::ColorCurve(t) => {
                        for frame in frames {
                            let index = t
                                .keys
                                .binary_search_by_key(&frame, |key| key.frame)
                                .map_err(|_| {
                                    Error::Invalid(
                                        "keyframe not found after easing preflight".into(),
                                    )
                                })?;
                            t.keys[index].ease = self.easing.ease;
                            t.keys[index].curve = self.easing.curve;
                        }
                    }
                }
                Ok(())
            })()
            .map_err(|cause| selection.target.failure("paste_easing_local", cause))?;
        }
        Ok(())
    }
}
