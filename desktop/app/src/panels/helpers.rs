use super::*;

pub(super) fn label(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    text: &str,
    muted: bool,
) {
    let styles = list.styles();

    motion_ui::style::text_style(
        list,
        atlas,
        area,
        text,
        styles
            .as_ref()
            .resolve(
                if muted {
                    "label.small.muted"
                } else {
                    "label.small"
                },
                Default::default(),
            )
            .unwrap(),
        Align::Left,
    );
}
pub(super) fn clicked(input: &Input, area: Rect) -> bool {
    matches!(input.pressed, Some((MouseButton::Left, p)) if area.contains(p))
}
pub(super) fn right_clicked(input: &Input, area: Rect) -> bool {
    matches!(input.pressed, Some((MouseButton::Right, p)) if area.contains(p))
}
pub(super) fn button(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    text: &str,
    input: &Input,
    enabled: bool,
    action: Action,
    out: &mut Vec<Action>,
) {
    if ui::button(list, atlas, area, text, input, enabled).clicked {
        out.push(action);
    }
}
// One bounded process-lifetime observation site covers this shared project
// clock boundary. Healthy reads do not reset its counter; caller location
// identifies the panel without retaining a dynamic observation-key map.
#[track_caller]
pub(super) fn project_rate(state: &Value) -> Option<motion_core::FrameRate> {
    let caller = std::panic::Location::caller();
    observe!("panel.project_clock", crate::project_clock::read(state),
        "caller={}:{} generation={:?} revision={:?} phase=project_clock",
        caller.file(), caller.line(), state["document_generation"].as_u64(),
        state["revision"].as_u64()).ok()
}

pub fn timecode(frame: f64, fps: Option<motion_core::FrameRate>) -> String {
    let Some(fps) = fps else { return "—".into(); };
    if !frame.is_finite() || frame.max(0.).floor() >= i64::MAX as f64 {
        let _ = observe!("panel.timecode_frame", Err::<(), _>("frame is not a finite signed-64-bit value"),
            "frame={frame} fps={fps} phase=timecode");
        return "—".into();
    }
    let frame = frame.max(0.).floor() as i64;
    match fps.format_timecode(frame, motion_core::TimecodeMode::NonDrop) {
        Ok(text) => text,
        // Valid arbitrary rational clocks have no supported SMPTE label rule.
        // Display their frame count, never invent a nearby integer FPS.
        Err(motion_core::TimecodeError::UnsupportedRate) => format!("{frame} f"),
        Err(cause) => {
            let _ = observe!("panel.timecode_format", Err::<(), _>(cause),
                "frame={frame} fps={fps} phase=timecode");
            "—".into()
        }
    }
}

// Preserve the graph's existing compact seconds:frames presentation, but
// derive its nominal labels from the exact clock rather than physical seconds.
pub(super) fn compact_timecode(frame: f64, fps: Option<motion_core::FrameRate>) -> String {
    let text=timecode(frame,fps);
    let fields=text.split(':').collect::<Vec<_>>();
    if let [hours,minutes,seconds,frames]=fields.as_slice() {
        if let (Ok(hours),Ok(minutes),Ok(seconds))=(hours.parse::<u64>(),minutes.parse::<u64>(),seconds.parse::<u64>()) {
            return format!("{}:{frames}",hours*3600+minutes*60+seconds);
        }
    }
    text
}

#[cfg(test)]
mod rational_clock_tests {
    use super::*;
    use motion_core::FrameRate;
    use serde_json::json;

    #[test]
    fn project_clock_is_strict_and_timecode_uses_nominal_labels_without_decimal_rounding() {
        let rate = FrameRate::new(30_000, 1001).unwrap();
        let state = json!({"project":{"fps":{"numerator":60_000,"denominator":2002}}});
        assert_eq!(project_rate(&state), Some(rate));
        assert_eq!(timecode(1800.75, Some(rate)), "00:01:00:00");
        assert_eq!(compact_timecode(1800.75, Some(rate)), "60:00");
        assert_eq!(compact_timecode(108000., Some(rate)), "3600:00");
        assert_eq!(compact_timecode(1800.,Some(FrameRate::new(2997,100).unwrap())),"1800 f");
        assert_eq!(timecode(108000., Some(rate)), "01:00:00:00");
        assert_eq!(timecode(1800., Some(FrameRate::new(2997,100).unwrap())), "1800 f");
        for fps in [json!(30), json!(null), json!({"numerator":30,"denominator":0}),
            json!({"numerator":30,"denominator":1,"extra":true})] {
            assert_eq!(project_rate(&json!({"project":{"fps":fps}})), None);
        }
        assert_eq!(timecode(1800., None), "—");
        assert_eq!(timecode(f64::NAN, Some(rate)), "—");
        assert_eq!(timecode(f64::INFINITY, Some(rate)), "—");
    }
    #[test]
    fn project_clock_real_faults_emit_default_file_and_stderr_once_per_lifetime_and_reset_per_process() {
        const CHILD: &str = "MOTION_PANEL_CLOCK_FAULT_CHILD";
        const NAME: &str = "panels::helpers::rational_clock_tests::project_clock_real_faults_emit_default_file_and_stderr_once_per_lifetime_and_reset_per_process";
        const SITES: [&str;2] = ["panel.project_clock", "panel.timecode_frame"];
        if let Some(root) = std::env::var_os(CHILD) {
            let root=std::path::PathBuf::from(root);
            let _logger=crate::logging::init(root.clone()).unwrap();
            let file=root.join("motion-studio.log");
            let good=json!({"document_generation":77,"revision":99,"project":{"fps":{"numerator":30_000,"denominator":1001}}});
            let bad=json!({"document_generation":77,"revision":99,"project":{"fps":30}});
            log::logger().flush();
            let before=std::fs::read_to_string(&file).unwrap();
            let rate=project_rate(&good).unwrap();
            assert_eq!(timecode(1800.,Some(rate)),"00:01:00:00");
            assert_eq!(timecode(1800.,Some(FrameRate::new(2997,100).unwrap())),"1800 f");
            log::logger().flush();
            assert_eq!(std::fs::read_to_string(&file).unwrap(),before,"healthy and valid non-SMPTE reads must stay quiet");
            for index in 0..65 {
                if index==58 {
                    assert_eq!(project_rate(&good),Some(rate));
                    assert_eq!(timecode(0.,Some(rate)),"00:00:00:00");
                }
                assert_eq!(project_rate(&bad),None);
                assert_eq!(timecode(f64::INFINITY,Some(rate)),"—");
            }
            log::logger().flush();
            let logs=std::fs::read_to_string(&file).unwrap();
            for site in SITES {
                let rows=logs.lines().filter(|line|line.contains(&format!("site={site} "))).collect::<Vec<_>>();
                assert_eq!(rows.len(),2,"{site}: {rows:?}");
                assert!(rows[0].contains("count=1 "));
                assert!(rows[1].contains("count=60 "));
            }
            assert!(logs.contains("project.fps requires exactly numerator and denominator"));
            assert!(logs.contains("caller=") && logs.contains("generation=Some(77) revision=Some(99)"));
            assert!(logs.contains("frame is not a finite signed-64-bit value"));
            return;
        }
        let temp=tempfile::tempdir().unwrap();
        let root=std::env::var_os("MOTION_PANEL_CLOCK_FAULT_ARTIFACTS")
            .map(std::path::PathBuf::from).unwrap_or_else(||temp.path().to_path_buf());
        std::fs::create_dir_all(&root).unwrap();
        for process in ["first-process","reset-process"] {
            let child=root.join(process);
            std::fs::create_dir_all(&child).unwrap();
            let output=std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact",NAME,"--nocapture","--test-threads=1"])
                .env(CHILD,&child).env_remove("MOTION_LOG").output().unwrap();
            std::fs::write(child.join("stdout.log"),&output.stdout).unwrap();
            std::fs::write(child.join("stderr.log"),&output.stderr).unwrap();
            let stdout=String::from_utf8_lossy(&output.stdout);
            let stderr=String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(),"{process}: {stdout} {stderr}");
            let file=std::fs::read_to_string(child.join("motion-studio.log")).unwrap();
            for site in SITES {
                let marker=format!("persistent panel degradation site={site} count=60 ");
                assert_eq!(file.matches(&marker).count(),1);
                assert_eq!(stderr.matches(&marker).count(),1);
            }
            assert!(!stderr.contains("count=1 "),"Warn records must remain file-only");
            assert!(!stdout.contains("panel degradation") && !stdout.contains("persistent panel degradation"));
            assert!(!stdout.contains("project.fps requires exactly numerator and denominator"));
        }
    }

}

pub(super) fn last_frame(state: &Value) -> f64 {
    state["project"]["frames"].as_f64().unwrap_or(1.0).max(1.0) - 1.0
}
