use motion_model::layer_time::{
    LayerClock, SourceTimeSample, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    MAX_TIME_REMAP_KEYS,
};
use motion_model::{
    Curve, CurveShape, CurveSpace, Ease, Easing, Layer, LayerTimeline, Track, VideoClip,
};
use std::error::Error as _;

fn key(frame: f64, source_frame: f64) -> TimeRemapKey {
    TimeRemapKey {
        frame,
        source_frame,
        easing: Easing::default(),
    }
}
fn clock(numerator: u32, denominator: u32, direction: TimeDirection) -> LayerClock {
    LayerClock::new(
        TimeStretch::new(numerator, denominator).unwrap(),
        direction,
        0.,
        0.,
    )
    .unwrap()
}
fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-10 * expected.abs().max(1.),
        "{actual} != {expected}"
    );
}

#[test]
fn stretch_is_a_canonical_duration_ratio_and_has_explicit_bounds() {
    let stretch = TimeStretch::new(300, 200).unwrap();
    assert_eq!(stretch.ratio(), [3, 2]);
    assert_eq!(stretch.duration_scale(), 1.5);
    assert_eq!(stretch.speed(), 2. / 3.);
    assert_eq!(TimeStretch::new(1000, 1).unwrap().speed(), 0.001);
    assert_eq!(TimeStretch::new(1, 1000).unwrap().speed(), 1000.);
    for (n, d) in [
        (0, 1),
        (1, 0),
        (1001, 1),
        (1, 1001),
        (1_000_001, 1_000_001),
        (u32::MAX, 1),
        (1, u32::MAX),
    ] {
        let error = TimeStretch::new(n, d).unwrap_err();
        assert!(error.to_string().contains("phase=stretch"));
        assert!(error
            .source()
            .unwrap()
            .to_string()
            .contains("duration ratio"));
    }
}

#[test]
fn identity_matches_the_actual_unmapped_layer_and_media_clock_bit_for_bit() {
    let mut layer = Layer::solid(7, "clock", [100.; 2], [0.; 3], [1.; 4]);
    let mut clip = VideoClip::new(3);
    clip.source_offset_us = 321_987;
    for offset in [-36_000, -13, 0, 9, 36_000] {
        layer.timeline = Some(LayerTimeline {
            in_frame: 0,
            out_frame: 100,
            offset_frame: offset,
        });
        for time in [0., 0.125, 3. / 7., 29.5, 99., 36_000.] {
            let legacy = layer.local_frame(time);
            let local = LayerClock::default().local_frame(time, offset).unwrap();
            assert_eq!(local.to_bits(), legacy.to_bits());
            let inverse = LayerClock::default()
                .composition_frame(local, offset)
                .unwrap();
            // Subtracting a large integer offset may already lose low bits in
            // the old local clock. Compare its inverse to that same legacy
            // arithmetic, then bound the unavoidable original-time round trip.
            assert_eq!(inverse.to_bits(), (legacy + f64::from(offset)).to_bits());
            assert!(
                (inverse - time).abs()
                    <= f64::EPSILON * (legacy.abs() + f64::from(offset).abs() + 1.)
            );
            let mapped = SourceTimeSample::new(time, 1.)
                .unwrap()
                .through(LayerClock::default(), offset, None)
                .unwrap();
            assert_eq!(mapped.frame.to_bits(), legacy.to_bits());
            assert_eq!(mapped.rate, 1.);
            for fps in [24, 25, 30, 60, 240] {
                assert_eq!(
                    clip.source_time_us(mapped.frame, fps),
                    clip.source_time_us(legacy, fps)
                );
            }
        }
    }
}

#[test]
fn stretch_and_offset_have_an_independent_fractional_time_oracle() {
    let slow = clock(3, 2, TimeDirection::Forward);
    // C=20.75, offset=7 -> E=13.75, L=55/6. Key at L=5 is at C=14.5.
    near(slow.local_frame(20.75, 7).unwrap(), 55. / 6.);
    assert_eq!(slow.composition_frame(5., 7).unwrap(), 14.5);
    assert_eq!(slow.integer_key_frame(14.5, 7).unwrap(), 5);
    assert!(slow
        .integer_key_frame(14., 7)
        .unwrap_err()
        .to_string()
        .contains("continuous key addressing"));
    // Two local keys remain distinct even when both appear between comp frames.
    let fast = clock(1, 8, TimeDirection::Forward);
    assert_eq!(fast.composition_frame(1., 10).unwrap(), 10.125);
    assert_eq!(fast.composition_frame(2., 10).unwrap(), 10.25);
    assert_eq!(fast.integer_key_frame(10.125, 10).unwrap(), 1);
    assert_eq!(fast.integer_key_frame(10.25, 10).unwrap(), 2);
}

#[test]
fn reverse_and_pivot_changes_preserve_local_keys_and_current_sample() {
    let original = clock(3, 2, TimeDirection::Forward);
    let changed = original
        .with_rate_at(
            TimeStretch::new(2, 1).unwrap(),
            TimeDirection::Reverse,
            40.,
            10,
        )
        .unwrap();
    assert_eq!(original.local_frame(40., 10).unwrap(), 20.);
    assert_eq!(changed.local_frame(40., 10).unwrap(), 20.);
    assert_eq!(changed.local_frame(44., 10).unwrap(), 18.);
    assert_eq!(changed.composition_frame(18., 10).unwrap(), 44.);
    assert_eq!(changed.rate(), -0.5);
    // The inverse ignores the source remap. A frozen source cannot collapse
    // the property timeline or create an ambiguous property-key identity.
    let frozen = TimeRemap::new(vec![key(0., 70.)]).unwrap();
    for local in [-3., 0., 8., 18., 20., 20.75, 42.] {
        let composition = changed.composition_frame(local, 10).unwrap();
        near(changed.local_frame(composition, 10).unwrap(), local);
        assert_eq!(
            SourceTimeSample::new(composition, 1.)
                .unwrap()
                .through(changed, 10, Some(&frozen))
                .unwrap(),
            SourceTimeSample {
                frame: 70.,
                rate: 0.
            }
        );
    }
}

#[test]
fn forward_and_reverse_round_trip_signed_local_and_subframe_key_times() {
    for stretch in [
        TimeStretch::default(),
        TimeStretch::new(3, 2).unwrap(),
        TimeStretch::new(1, 1000).unwrap(),
        TimeStretch::new(1000, 1).unwrap(),
    ] {
        for direction in [TimeDirection::Forward, TimeDirection::Reverse] {
            let mapping = LayerClock::new(stretch, direction, 7.25, -19.5).unwrap();
            for offset in [-123, 0, 170] {
                for local in [-500., -1., 0., 0.0625, 5.75, 417.] {
                    let time = mapping.composition_frame(local, offset).unwrap();
                    near(mapping.local_frame(time, offset).unwrap(), local);
                }
            }
        }
    }
}

#[test]
fn invalid_and_overflowing_clocks_refuse_input_without_clamping_or_rounding() {
    for bad in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        2147483648.,
    ] {
        assert!(LayerClock::new(TimeStretch::default(), TimeDirection::Forward, bad, 0.).is_err());
        assert!(LayerClock::new(TimeStretch::default(), TimeDirection::Forward, 0., bad).is_err());
        assert!(LayerClock::default().local_frame(bad, 0).is_err());
        assert!(LayerClock::default().composition_frame(bad, 0).is_err());
        assert!(SourceTimeSample::new(bad, 1.).is_err());
    }
    for bad_rate in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(SourceTimeSample::new(0., bad_rate).is_err());
    }
    // Coordinate bounds are not derivative bounds. Finite derivatives remain
    // unquantized here; consumers separately bound preparation/read work.
    assert!(SourceTimeSample::new(0., f64::MAX).is_ok());
    let fast = clock(1, 1000, TimeDirection::Forward);
    assert!(fast
        .local_frame(3_000_000., 0)
        .unwrap_err()
        .to_string()
        .contains("phase=property_output"));
    let slow = clock(1000, 1, TimeDirection::Forward);
    assert!(slow
        .composition_frame(3_000_000., 0)
        .unwrap_err()
        .to_string()
        .contains("phase=composition_output"));
}

#[test]
fn remap_is_non_recursive_and_is_only_applied_after_the_property_clock() {
    let remap = TimeRemap::new(vec![key(0., 100.), key(10., 120.)]).unwrap();
    let slow = clock(2, 1, TimeDirection::Forward);
    let input = SourceTimeSample::new(14., 1.).unwrap();
    // E=14-10=4; L=2; S=104. Remapping twice would incorrectly hold at 120.
    assert_eq!(
        input.through(slow, 10, Some(&remap)).unwrap(),
        SourceTimeSample {
            frame: 104.,
            rate: 1.
        }
    );
    assert_eq!(slow.local_frame(14., 10).unwrap(), 2.);
    assert_eq!(slow.composition_frame(2., 10).unwrap(), 14.);
}

#[test]
fn non_monotonic_remap_reports_the_actual_signed_rate_of_each_interval() {
    let remap = TimeRemap::new(vec![
        key(0., 0.),
        key(10., 20.),
        key(20., 0.),
        key(30., 0.),
        key(40., 5.),
    ])
    .unwrap();
    for (input, frame, rate) in [
        (-1., 0., 0.),
        (0., 0., 2.),
        (5., 10., 2.),
        (10., 20., -2.),
        (15., 10., -2.),
        (20., 0., 0.),
        (25., 0., 0.),
        (30., 0., 0.5),
        (35., 2.5, 0.5),
        (40., 5., 0.),
        (99., 5., 0.),
    ] {
        assert_eq!(
            remap.sample(input).unwrap(),
            SourceTimeSample { frame, rate }
        );
    }
}

#[test]
fn hold_is_right_continuous_and_never_turns_into_a_pcm_dc_rate() {
    let mut a = key(0., 11.);
    a.easing.ease = Ease::Hold;
    let remap = TimeRemap::new(vec![a, key(10., 51.), key(20., 71.)]).unwrap();
    assert_eq!(
        remap.sample(0.).unwrap(),
        SourceTimeSample {
            frame: 11.,
            rate: 0.
        }
    );
    assert_eq!(
        remap.sample(9.999).unwrap(),
        SourceTimeSample {
            frame: 11.,
            rate: 0.
        }
    );
    assert_eq!(
        remap.sample(10.).unwrap(),
        SourceTimeSample {
            frame: 51.,
            rate: 2.
        }
    );
    assert_eq!(
        remap.sample(20.).unwrap(),
        SourceTimeSample {
            frame: 71.,
            rate: 0.
        }
    );
    // Tiny intervals are valid freezes; their undefined nominal 0/dt or huge
    // skipped jump slope must not create NaN before the hold branch.
    let mut a = key(0., 0.);
    a.easing.ease = Ease::Hold;
    let tiny = TimeRemap::new(vec![a, key(f64::from_bits(1), 1.)]).unwrap();
    assert_eq!(
        tiny.sample(0.).unwrap(),
        SourceTimeSample {
            frame: 0.,
            rate: 0.
        }
    );
}

#[test]
fn precise_source_values_preserve_individual_pcm_samples_at_long_times() {
    // At 30fps a one-sample step at 192kHz is 1/6400 frame. f32 storage at
    // frame 100000 cannot preserve it, but this mapping must do so.
    let step = 30. / 192_000.;
    let remap = TimeRemap::new(vec![key(0., 100_000.), key(1., 100_000. + step)]).unwrap();
    let a = remap.sample(0.).unwrap().frame;
    let b = remap.sample(1.).unwrap().frame;
    assert_eq!(a as f32, b as f32); // Negative control for f32 source coordinates.
    assert!(((b - a) * 192_000. / 30. - 1.).abs() < 1e-7);
    let serialized = serde_json::to_string(&remap).unwrap();
    let reopened: TimeRemap = serde_json::from_str(&serialized).unwrap();
    assert_eq!(reopened, remap);
    assert_eq!(reopened.sample(1.).unwrap().frame.to_bits(), b.to_bits());
}

#[test]
fn nested_clocks_compose_rates_before_any_media_quantization() {
    let parent = SourceTimeSample::new(10., 1.)
        .unwrap()
        .through(clock(2, 1, TimeDirection::Forward), 0, None)
        .unwrap();
    let child_clock = LayerClock::new(
        TimeStretch::new(1, 2).unwrap(),
        TimeDirection::Reverse,
        0.,
        100.,
    )
    .unwrap();
    let child = parent.through(child_clock, 0, None).unwrap();
    assert_eq!(
        child,
        SourceTimeSample {
            frame: 90.,
            rate: -1.
        }
    );
    let grandchild_clock =
        LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 200.).unwrap();
    assert_eq!(
        child.through(grandchild_clock, 0, None).unwrap(),
        SourceTimeSample {
            frame: 110.,
            rate: 1.
        }
    );
    assert!(SourceTimeSample {
        frame: 0.,
        rate: f64::MAX
    }
    .through(clock(1, 2, TimeDirection::Forward), 0, None)
    .is_err());
}

#[test]
fn remap_curves_reuse_progress_and_velocity_curve_definitions() {
    for space in [CurveSpace::Progress, CurveSpace::Velocity] {
        let curve = Curve {
            space,
            shape: CurveShape::Cubic {
                control1: [0.2, 0.],
                control2: [0.8, 1.],
                start: 0.,
                end: 1.,
            },
        };
        let mut a = key(0., 10.);
        a.easing.curve = Some(curve);
        let remap = TimeRemap::new(vec![a, key(20., 70.)]).unwrap();
        for time in [0., 1.25, 5., 10., 18.75] {
            let expected = curve.sample(time / 20.);
            let actual = remap.sample(time).unwrap();
            near(actual.frame, 10. + expected.progress * 60.);
            near(actual.rate, expected.velocity * 3.);
        }
        assert_eq!(
            remap.sample(20.).unwrap(),
            SourceTimeSample {
                frame: 70.,
                rate: 0.
            }
        );
    }
}

#[test]
fn analytic_ease_derivatives_match_finite_differences_without_f32_timing_loss() {
    for ease in [Ease::Linear, Ease::In, Ease::Out, Ease::InOut] {
        let mut a = key(-10., 70.);
        a.easing.ease = ease;
        let remap = TimeRemap::new(vec![a, key(20., -20.)]).unwrap();
        for time in [-9., -1.25, 0., 7., 19.] {
            let h = 1e-4;
            let numeric = (remap.sample(time + h).unwrap().frame
                - remap.sample(time - h).unwrap().frame)
                / (2. * h);
            assert!((remap.sample(time).unwrap().rate - numeric).abs() < 1e-8);
        }
    }
}

#[test]
fn remap_rejects_invalid_storage_and_preserves_original_curve_failure() {
    assert!(TimeRemap::new(vec![]).is_err());
    assert!(TimeRemap::new(vec![key(0., 0.); MAX_TIME_REMAP_KEYS + 1]).is_err());
    for keys in [
        vec![key(1., 0.), key(0., 0.)],
        vec![key(0., 0.), key(0., 1.)],
        vec![key(f64::NAN, 1.)],
        vec![key(0., f64::INFINITY)],
    ] {
        assert!(TimeRemap::new(keys).is_err());
    }
    let mut a = key(0., 0.);
    a.easing.curve = Some(Curve {
        space: CurveSpace::Progress,
        shape: CurveShape::Elastic {
            oscillations: f64::NAN,
            damping: 1.,
        },
    });
    let error = TimeRemap::new(vec![a, key(10., 10.)]).unwrap_err();
    assert!(error.to_string().contains("phase=remap_easing"));
    assert!(error.source().unwrap().to_string().contains("elastic"));
    let remap = TimeRemap::new(vec![key(0., 5.)]).unwrap();
    for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(remap.sample(time).is_err());
    }
    let too_fast = TimeRemap::new(vec![key(0., 0.), key(f64::from_bits(1), 1.)]).unwrap();
    assert!(too_fast
        .sample(0.)
        .unwrap_err()
        .to_string()
        .contains("source derivative"));
}

#[test]
fn serialized_clocks_and_remaps_validate_before_becoming_sampleable() {
    let mapping = LayerClock::new(
        TimeStretch::new(3, 2).unwrap(),
        TimeDirection::Reverse,
        1.25,
        100.,
    )
    .unwrap();
    let value = serde_json::to_value(mapping).unwrap();
    let reopened: LayerClock = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(reopened, mapping);
    assert_eq!(
        mapping.local_frame(18.75, -2).unwrap(),
        reopened.local_frame(18.75, -2).unwrap()
    );
    for (path, replacement) in [
        ("/stretch/numerator", 0.into()),
        ("/stretch/denominator", 0.into()),
        ("/direction", "other".into()),
        ("/local_pivot", serde_json::Value::Null),
    ] {
        let mut bad = value.clone();
        *bad.pointer_mut(path).unwrap() = replacement;
        assert!(serde_json::from_value::<LayerClock>(bad).is_err(), "{path}");
    }
    let remap = TimeRemap::new(vec![key(0., 10.), key(1., 20.)]).unwrap();
    let mut bad = serde_json::to_value(&remap).unwrap();
    bad["keys"][1]["frame"] = 0.into();
    assert!(serde_json::from_value::<TimeRemap>(bad).is_err());
    let mut bad = serde_json::to_value(&remap).unwrap();
    bad["unknown"] = true.into();
    assert!(serde_json::from_value::<TimeRemap>(bad).is_err());
    let mut missing = value;
    missing.as_object_mut().unwrap().remove("stretch");
    assert!(serde_json::from_value::<LayerClock>(missing).is_err());
}

#[test]
fn deserialization_stops_before_decoding_an_excess_key_payload() {
    let one = serde_json::to_string(&key(0., 0.)).unwrap();
    let prefix = std::iter::repeat_n(one.as_str(), MAX_TIME_REMAP_KEYS)
        .collect::<Vec<_>>()
        .join(",");
    // The last item is deliberately an invalid (and incomplete) key. Budget
    // refusal must happen before its type/schema/payload is even decoded.
    let input = format!("{{\"keys\":[{prefix},{{\"unboundedPayload\":");
    let error = serde_json::from_str::<TimeRemap>(&input).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("source remap key budget exceeded"),
        "{error}"
    );
}

#[test]
fn retimed_property_sampling_uses_the_same_local_clock_without_rewriting_keys() {
    let mut position = Track::constant([0., 0., 0.]);
    position.upsert(0, [0., 20., 0.], Ease::Linear).unwrap();
    position.upsert(10, [100., 40., 0.], Ease::Linear).unwrap();
    let before = serde_json::to_value(&position).unwrap();
    let mapping = LayerClock::new(
        TimeStretch::new(2, 1).unwrap(),
        TimeDirection::Reverse,
        0.,
        10.,
    )
    .unwrap();
    assert_eq!(
        position
            .sample(mapping.local_frame(10., 0).unwrap())
            .unwrap(),
        [50., 30., 0.]
    );
    assert_eq!(mapping.composition_frame(0., 0).unwrap(), 20.);
    assert_eq!(mapping.composition_frame(10., 0).unwrap(), 0.);
    assert_eq!(serde_json::to_value(&position).unwrap(), before);
}

#[test]
fn reverse_media_uses_each_native_quantum_instead_of_losing_a_comp_frame() {
    let mapping = LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 30.).unwrap();
    let clip = VideoClip::new(1);
    let mut first = None;
    let mut last = None;
    // One second at 30fps, 48000 native PCM samples: a whole-frame subtraction
    // would lose 1600 samples. Compare every position to its integer oracle.
    for sample in 0..48_000 {
        let time = f64::from(sample) * 30. / 48_000.;
        let source = SourceTimeSample::new(time, 1.)
            .unwrap()
            .through(mapping, 0, None)
            .unwrap();
        let position = source.pcm_position(30, 48_000, 0).unwrap().unwrap();
        assert!(
            (position - f64::from(47_999 - sample)).abs() < 1e-8,
            "sample={sample} position={position}"
        );
        first.get_or_insert(position);
        last = Some(position);
    }
    assert_eq!(first, Some(47_999.));
    assert!(last.unwrap().abs() < 1e-8);
    let head = SourceTimeSample::new(0., 1.)
        .unwrap()
        .through(mapping, 0, None)
        .unwrap();
    assert_eq!(head.video_time_us(&clip, 30).unwrap(), 999_999);
    assert_eq!(head.pcm_position(30, 192_000, 0).unwrap(), Some(191_999.));
}

#[test]
fn forward_video_conversion_preserves_existing_rounding_and_source_offsets() {
    let mut clip = VideoClip::new(1);
    for offset in [0, 31, 17_431, 3_600_000_000] {
        clip.source_offset_us = offset;
        for fps in [24, 25, 30, 60, 240] {
            for frame in [-0.5, 0., 0.125, 10. / 3., 47.9, 10_000.] {
                let source = SourceTimeSample::new(frame, 1.).unwrap();
                assert_eq!(
                    source.video_time_us(&clip, fps).unwrap(),
                    clip.source_time_us(frame, fps)
                );
                let reverse = SourceTimeSample::new(frame, -1.).unwrap();
                assert_eq!(
                    reverse.video_time_us(&clip, fps).unwrap(),
                    clip.source_time_us(frame, fps) - 1
                );
                assert_eq!(
                    reverse.pcm_position(fps, 48_000, offset).unwrap().unwrap(),
                    source.pcm_position(fps, 48_000, offset).unwrap().unwrap() - 1.
                );
            }
        }
    }
}

#[test]
fn freeze_keeps_picture_and_explicitly_requests_silence_with_no_pcm_dc() {
    let source = SourceTimeSample::new(15., 0.).unwrap();
    let clip = VideoClip::new(1);
    assert_eq!(source.video_time_us(&clip, 30).unwrap(), 500_000);
    for rate in [8_000, 44_100, 48_000, 192_000] {
        assert_eq!(source.pcm_position(30, rate, 0).unwrap(), None);
    }
    assert!(source.pcm_position(0, 48_000, 0).is_err());
    assert!(source.pcm_position(30, 0, 0).is_err());
    assert!(source.pcm_position(30, 48_000, u64::MAX).is_err());
}

#[test]
fn nested_frame_rate_conversion_keeps_seconds_and_quantizes_only_at_the_leaf() {
    let mut composition = motion_model::CompositionClip::new("comp-child".into());
    composition.source_start_frame = 6;
    let source = SourceTimeSample::new(15., -1.)
        .unwrap()
        .through_composition(&composition, 30, 24)
        .unwrap();
    assert_eq!(
        source,
        SourceTimeSample {
            frame: 18.,
            rate: -0.8
        }
    );
    assert_eq!(source.pcm_position(24, 48_000, 0).unwrap(), Some(35_999.));
    let twice_reversed = source
        .through(clock(1, 1, TimeDirection::Reverse), 0, None)
        .unwrap();
    assert_eq!(
        twice_reversed,
        SourceTimeSample {
            frame: -18.,
            rate: 0.8
        }
    );
    assert_eq!(
        twice_reversed.pcm_position(24, 48_000, 0).unwrap(),
        Some(-36_000.)
    );
    // Reversing each intermediate media request would incorrectly subtract two
    // source samples. Only the final signed rate selects the native endpoint.
    let original = SourceTimeSample::new(3.25, 1.).unwrap();
    let child = original.through_composition(&composition, 30, 24).unwrap();
    assert_eq!(
        child.frame.to_bits(),
        composition.source_frame(3.25, 30, 24).unwrap().to_bits()
    );
}

#[test]
fn media_boundaries_return_typed_errors_for_invalid_rates_offsets_and_coordinates() {
    let source = SourceTimeSample::new(0., 1.).unwrap();
    let mut clip = VideoClip::new(1);
    for fps in [0, 241, u32::MAX] {
        assert!(source.video_time_us(&clip, fps).is_err());
        assert!(source.pcm_position(fps, 48_000, 0).is_err());
    }
    for rate in [0, 7_999, 192_001, u32::MAX] {
        assert!(source.pcm_position(30, rate, 0).is_err());
    }
    clip.source_offset_us = u64::MAX;
    let error = source.video_time_us(&clip, 30).unwrap_err();
    assert!(error.to_string().contains("phase=media_offset"));
    assert!(error.source().unwrap().to_string().contains("one hour"));
    let invalid = SourceTimeSample {
        frame: f64::NAN,
        rate: 1.,
    };
    assert!(invalid.pcm_position(30, 48_000, 0).is_err());
    assert!(invalid.video_time_us(&VideoClip::new(1), 30).is_err());
    let composition = motion_model::CompositionClip::new("comp-child".into());
    assert!(source.through_composition(&composition, 0, 30).is_err());
    assert!(source.through_composition(&composition, 30, 0).is_err());
}
