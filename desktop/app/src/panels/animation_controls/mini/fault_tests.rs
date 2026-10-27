use super::*;

fn read(
    view: &mut View,
    state: &Value,
    target: &KeyframeTarget,
    speed: bool,
) -> (String, Vec<String>) {
    view.actual_readout(
        state,
        Locale::En,
        target,
        17,
        Some(77),
        0.5,
        speed,
        "t=0.5".into(),
        vec![],
    )
}
fn count(path: &std::path::Path, site: &str) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| line.contains(&format!("site={site} ")))
        .map(str::to_owned)
        .collect()
}

#[test]
fn default_failure_channels_cache_suppression_recovery_and_process_reset_use_real_consumers() {
    const CHILD: &str = "MOTION_CURVE_MINI_FAULT_CHILD";
    const NAME: &str = "panels::animation_controls::mini::tests::fault_tests::default_failure_channels_cache_suppression_recovery_and_process_reset_use_real_consumers";
    const SITES: [&str; 4] = [
        "curve.mini.fit_all",
        "curve.mini.readout_load",
        "curve.mini.readout_fps",
        "curve.mini.readout_sample",
    ];
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let good = scalar_state(0.2, 0.8, 60);
        let original = good.clone();
        let target = KeyframeTarget::Property {
            object: 7,
            property: Property::Opacity,
            axis: None,
        };
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let _logger = crate::logging::init(root.clone()).unwrap();
        let file = root.join("motion-studio.log");
        let mut view = View::default();
        for _ in 0..120 {
            assert!(read(&mut view, &good, &target, false).0.contains("47.00 f"));
        }
        let missing = KeyframeTarget::Property {
            object: 999,
            property: Property::Opacity,
            axis: None,
        };
        assert!(read(&mut view, &good, &missing, false).1[0].contains("unavailable"));
        assert!(view.readout.as_ref().unwrap().data.is_err());
        // Missing channels and a missing outgoing interval are normal states.
        let (_, lines) = view.actual_readout(
            &good,
            Locale::En,
            &target,
            77,
            None,
            0.5,
            false,
            "t=0.5".into(),
            vec![],
        );
        assert!(lines.is_empty());
        crate::logging::flush();
        let healthy_log = std::fs::read_to_string(&file).unwrap();
        assert_eq!(
            healthy_log.lines().count(),
            1,
            "healthy branches must add no records beyond logger initialization: {healthy_log}"
        );
        assert!(
            healthy_log.contains("logging initialized level=INFO")
                && !healthy_log.contains("panel degradation")
        );

        let mut malformed = good.clone();
        malformed["revision"] = 1.into();
        malformed["project"]["layers"][0]["transform"]["opacity"]["keys"][0]["frame"] =
            "not-a-frame".into();
        for _ in 0..120 {
            assert!(read(&mut view, &malformed, &target, false).1[0].contains("unavailable"));
        }
        assert_eq!(
            count(&file, SITES[1]).len(),
            1,
            "same failed source must stay negatively cached"
        );
        for revision in 2..=60 {
            assert!(read(&mut view, &good, &target, false).0.contains("47.00 f"));
            malformed["revision"] = revision.into();
            assert!(read(&mut view, &malformed, &target, false).1[0].contains("unavailable"));
        }
        assert_eq!(count(&file, SITES[1]).len(), 2);
        for revision in 61..=120 {
            malformed["revision"] = revision.into();
            assert!(read(&mut view, &malformed, &target, false).1[0].contains("unavailable"));
        }
        assert!(read(&mut view, &good, &target, false).0.contains("47.00 f"));
        malformed["revision"] = 121.into();
        read(&mut view, &malformed, &target, false);
        assert_eq!(
            count(&file, SITES[1]).len(),
            2,
            "documented process-lifetime counters do not reset on recovery"
        );

        let mut bad_fps = good.clone();
        bad_fps["project"]["fps"] = 0.into();
        for _ in 0..120 {
            assert!(read(&mut view, &bad_fps, &target, true).1[0].contains("unavailable"));
            assert!(read(&mut view, &good, &target, true).1[0].ends_with("%/s"));
        }
        // A degraded provider can carry finite fps outside the model's legal
        // composition bounds. The actual Core derivative and f32 unit sample
        // reject its overflow, instead of returning a successful partial card.
        let mut overflow = good.clone();
        overflow["project"]["fps"] = serde_json::json!(1e308);
        for _ in 0..120 {
            assert!(read(&mut view, &overflow, &target, true).1[0].contains("unavailable"));
            assert!(read(&mut view, &good, &target, true).0.contains("47.00 f"));
        }

        let mut geometry = View::default();
        let mut draw = |view: &mut View, width: f32, height: f32| {
            view.draw(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 0., width, height),
                &good,
                &Input::default(),
                Locale::En,
                Easing::default(),
                target.clone(),
                17,
                Some(77),
                None,
                None,
                None,
            );
        };
        draw(&mut geometry, 500., 0.);
        assert!(geometry.shape.as_ref().unwrap().automatic.is_none());
        for _ in 0..120 {
            draw(&mut geometry, 500., 0.);
        }
        assert_eq!(
            count(&file, SITES[0]).len(),
            1,
            "failed geometry is cached for an unchanged view"
        );
        for i in 1..120 {
            draw(&mut geometry, 500., 130.);
            assert!(geometry.shape.as_ref().unwrap().automatic.is_some());
            draw(&mut geometry, 500. + i as f32, 0.);
            assert!(geometry.shape.as_ref().unwrap().automatic.is_none());
        }
        draw(&mut geometry, 500., 130.);
        assert!(geometry.shape.as_ref().unwrap().automatic.is_some());
        assert!(geometry.tessellation.as_ref().unwrap().points.is_some());
        for site in SITES {
            let lines = count(&file, site);
            assert_eq!(lines.len(), 2, "{site}: {lines:?}");
            assert!(lines[0].contains("WARN") && lines[0].contains("count=1 "));
            assert!(lines[1].contains("ERROR") && lines[1].contains("count=60 "));
            assert!(lines.iter().all(|line| line.contains("object: 7")
                && line.contains("generation=Some(1)")
                && line.contains("revision=Some(")
                && line.contains("phase=")));
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for cause in [
            "not-a-frame",
            "composition fps unavailable",
            "comparison sample is not finite",
            "graph sampling geometry is invalid",
        ] {
            assert!(text.contains(cause), "missing original cause: {cause}");
        }
        assert!(
            !text.contains("site=curve.mini.adaptive ")
                && !text.contains("site=curve.mini.normalized_readout ")
        );
        assert_eq!(good, original);
        std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "sites":SITES,"sameSourceFailuresCached":true,"firstWarnAndErrorAtLifetime60":true,
            "suppressedAfter60":true,"healthyRecoveryPreservesCounterScope":true,"actualConsumers":true,
            "normalMissingAndNoIntervalQuiet":true,"originalCausesRetained":true,"sourceUnchanged":true,
            "limits":"Invalid geometry is a presentation boundary; fps 0/1e308 model-provider faults do not describe legal stored projects. Adaptive/normalized_readout defensive branches and other surfaces remain unverified here."
        })).unwrap()).unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CURVE_MINI_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    for run in ["first-process", "reset-process"] {
        let child_root = root.join(run);
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env_remove("MOTION_LOG")
            .env(CHILD, &child_root)
            .output()
            .unwrap();
        std::fs::create_dir_all(&child_root).unwrap();
        std::fs::write(child_root.join("stdout.log"), &result.stdout).unwrap();
        std::fs::write(child_root.join("stderr.log"), &result.stderr).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let stderr = String::from_utf8_lossy(&result.stderr);
        let stdout = String::from_utf8_lossy(&result.stdout);
        for site in SITES {
            assert_eq!(
                stderr
                    .lines()
                    .filter(|line| line.contains(&format!("site={site} ")))
                    .count(),
                1,
                "{stderr}"
            );
        }
        assert_eq!(
            stderr.lines().count(),
            4,
            "only Error records belong to stderr"
        );
        assert!(
            !stderr.contains("WARN")
                && !stdout.contains("panel degradation")
                && !stdout.contains("ERROR")
        );
    }
}
