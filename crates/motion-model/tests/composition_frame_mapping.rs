use motion_model::{composition::CompositionError, CompositionClip, Error};

fn detail(error: Error) -> CompositionError {
    match error {
        Error::Composition(error) => error,
        other => panic!("expected structured composition error: {other}"),
    }
}

#[test]
fn zero_rates_return_context_instead_of_non_finite_or_collapsed_time() {
    let clip = CompositionClip::new("child".into());
    for (parent, child) in [(0, 30), (30, 0), (0, 0)] {
        for frame in [0., 1., -1.] {
            let error = detail(clip.source_frame(frame, parent, child).unwrap_err());
            assert_eq!(error.code, "invalid_frame_rate");
            assert_eq!(error.composition, "child");
            assert_eq!(error.details["parent_fps"], parent);
            assert_eq!(error.details["child_fps"], child);
        }
    }
}

#[test]
fn non_finite_inputs_and_overflow_are_errors_with_bounded_context() {
    let clip = CompositionClip::new("child".into());
    for frame in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
        let error = detail(clip.source_frame(frame, 1, 2).unwrap_err());
        assert_eq!(error.code, "invalid_time");
        assert_eq!(error.composition, "child");
        assert_eq!(error.details["local_frame"], frame.to_string());
        assert!(serde_json::to_string(&error).unwrap().len() < 1024);
    }
}

#[test]
fn mixed_rate_offsets_and_fractional_or_negative_times_keep_existing_arithmetic() {
    let mut clip = CompositionClip::new("child".into());
    for offset in [i32::MIN, -37, 0, 37, i32::MAX] {
        clip.source_start_frame = offset;
        for parent in [1, 24, 25, 29, 30, 59, 144, 239] {
            for child in [1, 24, 25, 29, 30, 59, 144, 239] {
                for frame in [-120., -0.5, 0., 0.5, 17.25, 300.] {
                    let expected = f64::from(offset) + frame * f64::from(child) / f64::from(parent);
                    assert_eq!(clip.source_frame(frame, parent, child).unwrap(), expected);
                }
            }
        }
    }
}
