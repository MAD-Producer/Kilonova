use super::*;

impl ColorLut {
    /// Single 1D or 3D Cube table. Mixed shaper+3D and unknown directives are
    /// rejected explicitly. Cube files do not declare interpolation or purpose;
    /// callers must supply both, rather than inferring a color pipeline.
    pub fn from_cube(
        text: &str,
        interpolation: LutInterpolation,
        purpose: LutPurpose,
    ) -> Result<Self, LutError> {
        if text.len() > MAX_CUBE_BYTES {
            return Err(LutError::Capacity {
                requested: text.len(),
                limit: MAX_CUBE_BYTES,
            });
        }
        let mut dimension = None;
        let mut min = None;
        let mut max = None;
        let mut values = Vec::new();
        let mut expected = 0;
        for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = index + 1;
            let failure = |reason| LutError::Cube { line, reason };
            if raw.len() > 512 {
                return Err(failure("line exceeds 512 bytes"));
            }
            let content = raw.split('#').next().unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }
            if content.starts_with("TITLE ") {
                if !values.is_empty()
                    || !content[6..].trim().starts_with('"')
                    || !content.ends_with('"')
                {
                    return Err(failure("invalid TITLE directive"));
                }
                continue;
            }
            let mut words = content.split_ascii_whitespace();
            let first = words.next().ok_or_else(|| failure("empty directive"))?;
            if first == "LUT_1D_SIZE" || first == "LUT_3D_SIZE" {
                if dimension.is_some() || !values.is_empty() {
                    return Err(failure("duplicate or mixed LUT dimensions"));
                }
                let size = words
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .ok_or_else(|| failure("invalid LUT size"))?;
                if words.next().is_some() {
                    return Err(failure("trailing size fields"));
                }
                let kind = if first == "LUT_1D_SIZE" {
                    LutKind::OneD
                } else {
                    LutKind::ThreeD
                };
                expected = LutDescriptor {
                    kind,
                    size,
                    interpolation,
                    purpose,
                    domain_min: [0.; 3],
                    domain_max: [1.; 3],
                }
                .entry_count()?;
                dimension = Some((kind, size));
                values.reserve_exact(expected);
            } else if first == "DOMAIN_MIN" || first == "DOMAIN_MAX" {
                if !values.is_empty() {
                    return Err(failure("domain must precede values"));
                }
                let slot = if first == "DOMAIN_MIN" {
                    &mut min
                } else {
                    &mut max
                };
                if slot.is_some() {
                    return Err(failure("duplicate domain directive"));
                }
                *slot = Some(triplet(words, line)?);
            } else {
                if dimension.is_none() {
                    return Err(failure("expected LUT dimension before values"));
                }
                if values.len() == expected {
                    return Err(failure("too many values or unsupported directive"));
                }
                values.push(triplet(std::iter::once(first).chain(words), line)?);
            }
        }
        let (kind, size) = dimension.ok_or(LutError::Cube {
            line: 0,
            reason: "missing LUT dimension",
        })?;
        Self::new(
            LutDescriptor {
                kind,
                size,
                interpolation,
                purpose,
                domain_min: min.unwrap_or([0.; 3]),
                domain_max: max.unwrap_or([1.; 3]),
            },
            values,
        )
    }

    /// Data/domain round-trip only. Purpose and interpolation remain explicit
    /// caller metadata and are not silently encoded as nonstandard directives.
    pub fn to_cube(&self) -> String {
        let d = self.descriptor;
        let mut text = format!(
            "# Motion Studio color lookup\n{}_SIZE {}\n",
            if d.kind == LutKind::OneD {
                "LUT_1D"
            } else {
                "LUT_3D"
            },
            d.size
        );
        for (name, value) in [("DOMAIN_MIN", d.domain_min), ("DOMAIN_MAX", d.domain_max)] {
            text.push_str(&format!(
                "{name} {:e} {:e} {:e}\n",
                value[0], value[1], value[2]
            ));
        }
        for value in self.values.iter() {
            text.push_str(&format!("{:e} {:e} {:e}\n", value[0], value[1], value[2]));
        }
        text
    }
}

fn triplet<'a>(
    mut words: impl Iterator<Item = &'a str>,
    line: usize,
) -> Result<[f32; 3], LutError> {
    let failure = || LutError::Cube {
        line,
        reason: "expected exactly three finite numeric values",
    };
    let mut value = [0.; 3];
    for channel in &mut value {
        *channel = words
            .next()
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .ok_or_else(failure)?;
    }
    if words.next().is_some() {
        return Err(failure());
    }
    Ok(value)
}
