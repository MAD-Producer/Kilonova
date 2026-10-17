use super::*;
use std::{fs, process::Command};

#[test]
fn action_deferral_evidence_is_bounded_per_window_and_resets_only_after_submission_or_close() {
    const CHILD: &str = "MOTION_PRESENT_DEFERRAL_TEST_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let path = std::path::PathBuf::from(&root).join("motion-studio.log");
        let _logging = crate::logging::init(root.into()).unwrap();
        let mut observed = ActionDeferrals::default();
        for _ in 0..59 {
            observed.deferred(7, "after_draw", 3, false, 4);
            observed.submitted(0); // A healthy main window must not hide a stuck float.
        }
        crate::logging::flush();
        let before = fs::read_to_string(&path).unwrap();
        assert!(
            !before.contains("persistent presentation deferral"),
            "{before}"
        );
        observed.deferred(7, "before_draw", 2, true, 8);
        let first = fs::read_to_string(&path).unwrap();
        assert!(
            first.contains("ERROR") && first.contains("window=7 count=60"),
            "{first}"
        );
        assert!(
            first.contains("phase=before_draw")
                && first.contains("queued_actions=2 editing=true worker_queue_depth=8"),
            "{first}"
        );
        assert!(first.contains("elapsed_ms="));
        for _ in 0..120 {
            observed.deferred(7, "before_draw", 1, false, 0);
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), first);
        observed.submitted(7);
        for _ in 0..59 {
            observed.deferred(7, "after_draw", 5, false, 0);
        }
        crate::logging::flush();
        assert_eq!(
            fs::read_to_string(&path)
                .unwrap()
                .matches("persistent presentation deferral")
                .count(),
            1
        );
        observed.deferred(7, "after_draw", 5, false, 0);
        observed.closed(7);
        for _ in 0..59 {
            observed.deferred(7, "before_draw", 1, false, 0);
        }
        // A recycled key starts a fresh episode; closing is not a submission.
        crate::logging::flush();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(
            text.matches("persistent presentation deferral").count(),
            2,
            "{text}"
        );
        assert_eq!(
            text.contains("presentation deferred site="),
            log::max_level() >= log::LevelFilter::Debug,
            "{text}"
        );
        return;
    }
    for debug in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap());
        child.args(["--exact", "shell::presentation_observation::tests::action_deferral_evidence_is_bounded_per_window_and_resets_only_after_submission_or_close"])
            .env(CHILD, root.path()).env_remove("MOTION_LOG");
        if debug {
            child.env("MOTION_LOG", "debug");
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(
            stderr.matches("persistent presentation deferral").count(),
            2,
            "{stderr}"
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("site=shell.present.actions"));
    }
}
