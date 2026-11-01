use super::super::properties::{
    mapped_anchor_tests::{apply, fixture},
    AnchorControls,
};
use super::*;
const VIEW: Rect = Rect {
    min: [0., 0.],
    max: [640., 360.],
};
fn pointer(point: [f32; 2], press: bool, release: bool) -> Input {
    Input {
        mouse: point,
        held: (!release).then_some(MouseButton::Left),
        pressed: press.then_some((MouseButton::Left, point)),
        released: release.then_some((MouseButton::Left, point)),
        ..Default::default()
    }
}
#[test]
fn mapped_canvas_anchor_preserves_corners_and_cancels_when_the_clock_changes() {
    for reverse in [false, true] {
        for cause in ["offset", "clock", "release"] {
            let (_folder, engine) = fixture(reverse);
            let before = engine.state().unwrap();
            let pose = Pose::new(&before, 7, VIEW).unwrap();
            let at = pose.pivot;
            let mut c = Controls::default();
            let mut out = vec![];
            c.choose(Tool::Anchor, &mut out);
            c.draw(
                &mut PaintList::default(),
                VIEW,
                &before,
                Some(7),
                &pointer(at, true, false),
                &mut out,
            );
            apply(&engine, out);
            let end = [at[0] + 20., at[1] + 10.];
            let mut out = vec![];
            c.draw(
                &mut PaintList::default(),
                VIEW,
                &before,
                Some(7),
                &pointer(end, false, false),
                &mut out,
            );
            assert!(out
                .iter()
                .any(|a| matches!(a,Action::Edit(Command::AnchorAt{frame,..}) if *frame==6.5)));
            apply(&engine, out);
            let preview = engine.state().unwrap();
            let after = Pose::new(&preview, 7, VIEW).unwrap();
            for (a, b) in pose.corners.iter().zip(after.corners) {
                assert!(distance(*a, b) < 0.002, "{a:?} {b:?}");
            }
            let mut changed = preview.clone();
            match cause {
                "offset" => changed["project"]["layers"][0]["timeline"]["offset_frame"] = 3.into(),
                "clock" => {
                    changed["project"]["layers"][0]["time_mapping"]["clock"]["local_pivot"] =
                        3.into()
                }
                "release" => {}
                _ => unreachable!(),
            }
            let mut out = vec![];
            c.draw(
                &mut PaintList::default(),
                VIEW,
                &changed,
                Some(7),
                &pointer(end, false, true),
                &mut out,
            );
            if cause == "release" {
                assert!(matches!(out.as_slice(), [Action::History(3)]));
            } else {
                assert!(matches!(out.as_slice(), [Action::History(4)]));
            }
            apply(&engine, out);
            if cause == "release" {
                assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
            } else {
                assert_eq!(engine.state().unwrap()["project"], before["project"]);
            }
            assert!(!c.gesture_active());
        }
    }
}

#[test]
fn anchor_time_faults_and_worker_rejections_reach_default_logs_without_stdout() {
    const NAME:&str="panels::transform_controls::mapped_anchor_tests::anchor_time_faults_and_worker_rejections_reach_default_logs_without_stdout";
    const CHILD: &str = "MOTION_ANCHOR_TIME_FAULT_CHILD";
    const SITES: [&str; 4] = [
        "properties.anchor_time_begin",
        "properties.anchor_time_continue",
        "transform.numeric_time_begin",
        "transform.numeric_time_continue",
    ];
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let (_folder, engine) = fixture(false);
        let healthy = engine.state().unwrap();
        let before = healthy["project"].clone();
        let mut bad = healthy.clone();
        bad["project"]["layers"][0]["time_mapping"]["clock"] = serde_json::json!({"broken":true});
        let pose = Pose::new(&healthy, 7, VIEW).unwrap();
        let at = pose.pivot;
        let styles = PaintList::default().styles();
        let area = Rect::new(0., 0., 720., 120.);
        let rect = super::super::properties::inspector_layout(&styles, area, 0., 0, 2).number;
        let numeric = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
        let mut atlas = motion_ui::text::TextAtlas::from_system(12.).unwrap();
        for _ in 0..120 {
            let mut control = AnchorControls::default();
            let mut out = vec![];
            control.draw(
                &mut PaintList::default(),
                &mut atlas,
                area,
                0.,
                motion_ui::dock::Panel::Properties,
                &bad["project"]["layers"][0],
                &bad,
                &pointer(numeric, true, false),
                false,
                &mut out,
            );
            assert!(out.is_empty() && !control.editing());
            control.draw(
                &mut PaintList::default(),
                &mut atlas,
                area,
                0.,
                motion_ui::dock::Panel::Properties,
                &healthy["project"]["layers"][0],
                &healthy,
                &pointer(numeric, true, false),
                false,
                &mut out,
            );
            assert!(control.editing());
            control.process(&styles, &bad, &Input::default(), &mut out);
            assert!(out.is_empty() && !control.editing());
            assert!(Drag::at_time(
                Seed::new(at, &bad, VIEW),
                pose.clone(),
                Handle::Anchor,
                &bad
            )
            .is_none());
            let mut c = Controls::default();
            c.choose(Tool::Anchor, &mut out);
            c.draw(
                &mut PaintList::default(),
                VIEW,
                &healthy,
                Some(7),
                &pointer(at, true, false),
                &mut out,
            );
            assert!(c.gesture_active());
            c.draw(
                &mut PaintList::default(),
                VIEW,
                &bad,
                Some(7),
                &Input::default(),
                &mut out,
            );
            assert!(out.is_empty() && !c.gesture_active());
        }
        let error = engine
            .edit(
                serde_json::to_string(&Command::AnchorAt {
                    object: 7,
                    frame: 8.75,
                    anchor: [0.7, 0.2],
                })
                .unwrap(),
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("continuous key addressing"),
            "{error}"
        );
        assert_eq!(engine.state().unwrap()["project"], before);
        crate::logging::flush();
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in SITES {
            let rows: Vec<_> = file
                .lines()
                .filter(|l| l.contains(&format!("site={site} ")))
                .collect();
            assert_eq!(rows.len(), 2, "{site}: {file}");
            assert!(rows[0].contains("WARN") && rows[0].contains("count=1 "));
            assert!(rows[1].contains("ERROR") && rows[1].contains("count=60 "));
            assert!(rows
                .iter()
                .all(|l| l.contains("object=7") && l.contains("reason=")));
        }
        assert!(
            file.contains("start #")
                && file.contains("anchor_at")
                && file.contains("continuous key addressing")
        );
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_ANCHOR_TIME_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    for site in SITES {
        let rows: Vec<_> = stderr
            .lines()
            .filter(|l| l.contains(&format!("site={site} ")))
            .collect();
        assert_eq!(rows.len(), 1, "{stderr}");
        assert!(rows[0].contains("ERROR") && rows[0].contains("count=60 "));
        assert!(!stdout.contains(site));
    }
    assert!(stderr.contains("continuous key addressing"));
    assert!(!stdout.contains(" INFO [") && !stdout.contains(" ERROR "));
}
