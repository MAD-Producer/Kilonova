/// Container declarations have different origins. libav reports a span from
/// the first visible PTS; MediaExtractor reports an end on the movie timeline.
#[derive(Clone, Copy, Debug)]
pub enum VideoDuration {
    Span { start_us: u64, duration_us: u64 },
    End(u64),
}

/// Exclusive end of the source presentation interval. A known final sample
/// duration is authoritative: a container declaration must not extend it and
/// repeat the tail frame (nor shorten a VFR tail using the median PTS step).
/// Declarations and the positive PTS step are fallbacks for missing durations.
pub fn video_end_us(
    last_pts_us: u64,
    last_sample_duration_us: Option<u64>,
    step_us: u64,
    declared: VideoDuration,
) -> crate::Result<u64> {
    if let Some(duration) = last_sample_duration_us.filter(|duration| *duration > 0) {
        return last_pts_us
            .checked_add(duration)
            .ok_or("video end overflow".into());
    }
    let declared_end = match declared {
        VideoDuration::Span {
            start_us,
            duration_us,
        } => start_us
            .checked_add(duration_us)
            .ok_or("video end overflow")?,
        VideoDuration::End(end) => end,
    };
    let inferred_end = last_pts_us
        .checked_add(step_us.max(1))
        .ok_or("video end overflow")?;
    Ok(declared_end.max(inferred_end))
}

/// Extractors may read compressed packets even when only scanning timestamps.
/// Retain a bounded deadline without treating a multi-GiB cold read as a hang.
pub fn timestamp_scan_budget(source_bytes: u64) -> std::time::Duration {
    let chunks = source_bytes
        .min(motion_model::storage::MAX_MEDIA_ASSET)
        .div_ceil(16 * 1024 * 1024);
    std::time::Duration::from_secs(30 + chunks)
}

/// Correct rounded container frame-rate hints using constant-rate source PTS.
/// Variable-rate streams keep a declared nominal rate; their sampling uses PTS.
pub fn source_frame_rate(declared: f64, pts: &[u64], step: u64, variable: bool) -> f64 {
    let valid = declared.is_finite() && declared > 0.0;
    let fallback = if valid {
        declared
    } else {
        1_000_000.0 / step.max(1) as f64
    };
    if variable || pts.len() < 2 {
        return fallback;
    }
    let span = pts.last().unwrap().saturating_sub(pts[0]);
    if span == 0 {
        return fallback;
    }
    let observed = (pts.len() - 1) as f64 * 1_000_000.0 / span as f64;
    // Both endpoints can round by one microsecond. Only round to an integer
    // inside this uncertainty, so 59.94/29.97 never become 60/30 by assumption.
    let uncertainty = observed * 2.0 / span as f64;
    if valid && (declared - observed).abs() <= uncertainty.max(0.00001) {
        return declared;
    }
    if (observed - observed.round()).abs() <= uncertainty {
        observed.round()
    } else {
        observed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonzero_pts_have_the_same_end_with_span_or_absolute_container_metadata() {
        let first = 250_000;
        let last = 3_208_333;
        let duration = Some(41_667);
        let desktop = video_end_us(
            last,
            duration,
            41_667,
            VideoDuration::Span {
                start_us: first,
                duration_us: 2_999_999,
            },
        )
        .unwrap();
        let android = video_end_us(last, duration, 41_667, VideoDuration::End(3_250_000)).unwrap();
        assert_eq!(desktop, 3_250_000);
        assert_eq!(desktop, android);
        // A declaration already containing the leading edit cannot append it
        // again when a valid final sample duration is available.
        assert_eq!(
            video_end_us(
                last,
                duration,
                41_667,
                VideoDuration::Span {
                    start_us: first,
                    duration_us: 3_250_000,
                }
            )
            .unwrap(),
            desktop
        );
    }

    #[test]
    fn variable_rate_tail_uses_its_own_duration_and_not_the_median_step() {
        for duration in [20_000, 300_000] {
            assert_eq!(
                video_end_us(
                    180_000,
                    Some(duration),
                    80_000,
                    VideoDuration::End(1_000_000)
                )
                .unwrap(),
                180_000 + duration,
            );
        }
    }

    #[test]
    fn absent_sample_duration_normalizes_declarations_and_keeps_a_positive_interval() {
        let desktop = VideoDuration::Span {
            start_us: 250_000,
            duration_us: 3_000_000,
        };
        let android = VideoDuration::End(3_250_000);
        assert_eq!(
            video_end_us(3_208_333, None, 41_667, desktop).unwrap(),
            3_250_000
        );
        assert_eq!(
            video_end_us(3_208_333, Some(0), 41_667, android).unwrap(),
            3_250_000
        );
        assert_eq!(
            video_end_us(83_333, None, 41_667, VideoDuration::End(125_000)).unwrap(),
            125_000
        );
        assert_eq!(
            video_end_us(250_000, None, 0, VideoDuration::End(0)).unwrap(),
            250_001
        );
    }

    #[test]
    fn video_end_rejects_overflow_and_ignores_unusable_metadata_with_measured_tail() {
        assert!(video_end_us(u64::MAX, Some(1), 1, VideoDuration::End(0)).is_err());
        assert!(video_end_us(u64::MAX, None, 0, VideoDuration::End(0)).is_err());
        let overflow = VideoDuration::Span {
            start_us: u64::MAX,
            duration_us: 1,
        };
        assert!(video_end_us(0, None, 1, overflow).is_err());
        assert_eq!(video_end_us(0, Some(10), 1, overflow).unwrap(), 10);
    }

    #[test]
    fn timestamp_scan_has_a_size_sensitive_bounded_deadline() {
        assert_eq!(timestamp_scan_budget(0).as_secs(), 30);
        assert_eq!(timestamp_scan_budget(16 * 1024 * 1024).as_secs(), 31);
        assert_eq!(timestamp_scan_budget(16 * 1024 * 1024 + 1).as_secs(), 32);
        assert_eq!(timestamp_scan_budget(512 * 1024 * 1024).as_secs(), 62);
        assert_eq!(
            timestamp_scan_budget(motion_model::storage::MAX_MEDIA_ASSET).as_secs(),
            158
        );
        assert_eq!(timestamp_scan_budget(u64::MAX).as_secs(), 158);
    }
    fn pts(fps: f64) -> Vec<u64> {
        (0..24)
            .map(|n| (n as f64 * 1_000_000.0 / fps).round() as u64)
            .collect()
    }
    #[test]
    fn rounded_headers_do_not_flatten_fractional_source_rates() {
        for (hint, actual) in [
            (60.0, 60000.0 / 1001.0),
            (30.0, 30000.0 / 1001.0),
            (120.0, 120000.0 / 1001.0),
        ] {
            assert!((source_frame_rate(hint, &pts(actual), 1, false) - actual).abs() < 0.001);
        }
        assert_eq!(source_frame_rate(240.0, &pts(240.0), 4167, false), 240.0);
        assert_eq!(source_frame_rate(0.0, &[0, 4167], 4167, false), 240.0);
        assert_eq!(source_frame_rate(0.0, &pts(300.0), 3333, false), 300.0);
        assert_eq!(
            source_frame_rate(30.0, &[0, 17000, 40000, 100000], 23000, true),
            30.0
        );
    }
}
