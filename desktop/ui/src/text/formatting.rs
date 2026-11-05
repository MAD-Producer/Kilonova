//! Scratch for bounded, primitive numeric formatting on the UI drawing path.
use super::TextAtlas;
use std::fmt;

// One buffer per atlas, not per control. Current callers format f32/f64 with
// at most eight decimal places (at most 319 bytes). Discard exceptional large
// capacities rather than permanently retaining an accidental large argument.
const MAX_RETAINED_BYTES: usize = 1024;

impl TextAtlas {
    pub(crate) fn with_formatted_text<R>(
        &mut self,
        args: fmt::Arguments<'_>,
        draw: impl FnOnce(&mut Self, &str) -> R,
    ) -> Result<R, fmt::Error> {
        let mut text = std::mem::take(&mut self.formatting_scratch);
        text.clear();
        let result = fmt::write(&mut text, args).map(|()| draw(self, &text));
        text.clear();
        if text.capacity() <= MAX_RETAINED_BYTES {
            self.formatting_scratch = text;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Failed;
    impl fmt::Display for Failed {
        fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
            out.write_str("partial formatting probe")?;
            Err(fmt::Error)
        }
    }

    #[test]
    fn bounded_numeric_scratch_preserves_extremes_nested_draws_and_errors() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        for value in [
            0.,
            -0.,
            12.345,
            f64::MAX,
            -f64::MAX,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            -f64::from_bits(1),
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ] {
            for precision in 0..=8 {
                let expected = format!("{:.*}", precision, value);
                atlas
                    .with_formatted_text(format_args!("{:.*}", precision, value), |_, text| {
                        assert_eq!(text, expected);
                    })
                    .unwrap();
                assert!(atlas.formatting_scratch.is_empty());
                assert!(atlas.formatting_scratch.capacity() <= MAX_RETAINED_BYTES);
            }
        }
        let pointer = atlas.formatting_scratch.as_ptr();
        assert!(atlas
            .with_formatted_text(format_args!("{Failed}"), |_, _| {
                panic!("partial formatting must not reach drawing")
            })
            .is_err());
        assert_eq!(atlas.formatting_scratch.as_ptr(), pointer);
        assert!(atlas.formatting_scratch.is_empty());
        atlas
            .with_formatted_text(format_args!("12.50"), |atlas, outer| {
                atlas
                    .with_formatted_text(format_args!("34.57"), |_, inner| {
                        assert_eq!(inner, "34.57");
                        assert_eq!(outer, "12.50");
                    })
                    .unwrap();
                assert_eq!(outer, "12.50");
            })
            .unwrap();
        // An accidental oversized argument may not enlarge retained storage.
        atlas
            .with_formatted_text(format_args!("{:0>2048}", "1"), |_, text| {
                assert_eq!(text.len(), 2048);
            })
            .unwrap();
        assert_eq!(atlas.formatting_scratch.capacity(), 0);
        atlas
            .with_formatted_text(format_args!("7.25"), |_, text| assert_eq!(text, "7.25"))
            .unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            atlas.with_formatted_text(format_args!("8.50"), |_, _| panic!("callback probe"))
        }));
        assert!(panic.is_err(), "draw panics must propagate unchanged");
        // A callback unwind may discard scratch, but never corrupt subsequent text.
        atlas
            .with_formatted_text(format_args!("9.75"), |_, text| assert_eq!(text, "9.75"))
            .unwrap();
    }
}
