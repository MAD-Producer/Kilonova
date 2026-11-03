//! Read the authoritative project clock without cloning a per-frame snapshot.
//! Display-only approximations belong at the caller, after this validation.
use motion_core::{FrameRate, RateError};
use serde_json::Value;
use std::fmt;

#[derive(Debug)]
pub(crate) enum Error {
    Fields,
    Component(&'static str),
    Rate(RateError),
    CompositionRange(FrameRate),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fields => f.write_str("project.fps requires exactly numerator and denominator"),
            Self::Component(field) => {
                write!(f, "project.fps.{field} requires an unsigned 32-bit integer")
            }
            Self::Rate(cause) => write!(f, "project.fps: {cause}"),
            Self::CompositionRange(rate) => write!(
                f,
                "project.fps={rate} is outside 1..={}",
                motion_core::MAX_COMPOSITION_FPS
            ),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rate(cause) => Some(cause),
            _ => None,
        }
    }
}

pub(crate) fn read(state: &Value) -> Result<FrameRate, Error> {
    let fields = state["project"]["fps"].as_object().ok_or(Error::Fields)?;
    if fields.len() != 2 || !fields.contains_key("numerator") || !fields.contains_key("denominator")
    {
        return Err(Error::Fields);
    }
    let component = |name: &'static str| {
        fields[name]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(Error::Component(name))
    };
    let rate =
        FrameRate::new(component("numerator")?, component("denominator")?).map_err(Error::Rate)?;
    if !rate.is_between(1, motion_core::MAX_COMPOSITION_FPS) {
        return Err(Error::CompositionRange(rate));
    }
    Ok(rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::error::Error as _;

    #[test]
    fn canonical_and_reduced_snapshot_rates_keep_exact_clock_identity() {
        for ([numerator, denominator], expected) in [
            ([30u32, 1], [30, 1]),
            ([24000, 1001], [24000, 1001]),
            ([30000, 1001], [30000, 1001]),
            ([60000, 1001], [60000, 1001]),
            ([60000, 2002], [30000, 1001]),
            ([4_000_000_000, 2_000_000_000], [2, 1]),
        ] {
            let state =
                json!({"project":{"fps":{"numerator":numerator,"denominator":denominator}}});
            let before = state.clone();
            assert_eq!(read(&state).unwrap().ratio(), expected);
            assert_eq!(state, before);
        }
        let a = read(&json!({"project":{"fps":{"numerator":30000,"denominator":1001}}})).unwrap();
        let b = read(&json!({"project":{"fps":{"numerator":2997,"denominator":100}}})).unwrap();
        assert_ne!(a, b);
        assert_ne!(a.as_f64(), b.as_f64());
    }

    #[test]
    fn incomplete_numeric_and_unknown_field_clocks_never_become_thirty_fps() {
        for fps in [
            Value::Null,
            json!(30),
            json!(29.97),
            json!({}),
            json!({"numerator":30000}),
            json!({"denominator":1001}),
            json!({"numerator":30000,"denominator":1001,"unexpected":true}),
        ] {
            assert!(matches!(
                read(&json!({"project":{"fps":fps}})),
                Err(Error::Fields)
            ));
        }
        assert!(matches!(read(&json!({})), Err(Error::Fields)));
    }

    #[test]
    fn component_errors_identify_the_field_without_rendering_user_payloads() {
        for (component, invalid) in [
            ("numerator", json!(-1)),
            ("denominator", json!(1.5)),
            ("numerator", json!(u64::MAX)),
            ("denominator", json!("private text")),
        ] {
            let mut state = json!({"project":{"fps":{"numerator":30,"denominator":1}}});
            state["project"]["fps"][component] = invalid;
            let error = read(&state).unwrap_err();
            assert!(matches!(error, Error::Component(field) if field == component));
            let message = error.to_string();
            assert!(message.contains(component));
            assert!(!message.contains("private text"));
        }
    }

    #[test]
    fn zero_and_component_limit_failures_retain_the_native_rate_cause() {
        for ([numerator, denominator], expected) in [
            ([30, 0], RateError::Zero),
            ([0, 1], RateError::Zero),
            ([1_000_001, 1], RateError::ComponentRange),
        ] {
            let error =
                read(&json!({"project":{"fps":{"numerator":numerator,"denominator":denominator}}}))
                    .unwrap_err();
            assert!(matches!(&error, Error::Rate(cause) if *cause == expected));
            assert_eq!(
                error.source().unwrap().downcast_ref::<RateError>(),
                Some(&expected)
            );
        }
    }

    #[test]
    fn composition_bounds_compare_exact_ratios_instead_of_integer_truncation() {
        for [numerator, denominator] in [[1, 1], [240, 1], [239999, 1000], [1001, 1000]] {
            assert!(read(
                &json!({"project":{"fps":{"numerator":numerator,"denominator":denominator}}})
            )
            .is_ok());
        }
        for [numerator, denominator] in [[1000, 1001], [240001, 1000], [241, 1]] {
            assert!(matches!(
                read(&json!({"project":{"fps":{"numerator":numerator,"denominator":denominator}}})),
                Err(Error::CompositionRange(_))
            ));
        }
    }
}
