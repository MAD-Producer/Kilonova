use super::*;
use motion_core::SampledEffect;
use motion_effects::{RendererKind, SceneSettings, MAX_PARAMS};
use motion_render::{scene_generator::validate_settings, RenderError};

mod alpha;
mod history;
mod numeric;

fn effect() -> SampledEffect {
    SampledEffect {
        layer: 7,
        local_frame: 5.5,
        instance: 11,
        plugin: "probe".into(),
        effect: "probe".into(),
        version: "1".into(),
        hash: String::new(),
        enabled: true,
        seed: 1,
        image_input: None,
        param_ids: vec!["rate".into(), "lifetime".into()],
        values: [[1., 0., 0., 0.]; MAX_PARAMS],
        lut: None,
        scene: Some(SceneSettings::default()),
        particle_history: None,
        particle_history_error: None,
    }
}

#[test]
fn actual_generator_refusals_keep_native_sources_and_default_reply_log_channels() {
    const CHILD: &str = "MOTION_GENERATOR_SETTINGS_ERROR_CHILD";
    const NAME: &str = "engine::failure::composition_tests::plan_error::generator_settings::actual_generator_refusals_keep_native_sources_and_default_reply_log_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let healthy = effect();
        validate_settings(&healthy, RendererKind::Particles).unwrap();
        let mut bad_settings = effect();
        bad_settings.scene.as_mut().unwrap().source_layer = Some(0);
        let settings = validate_settings(&bad_settings, RendererKind::Particles).unwrap_err();
        assert!(matches!(&settings, RenderError::Context { cause, .. }
            if matches!(cause.as_ref(), RenderError::Effects(motion_effects::Error::Invalid(_)))));
        let mut bad_index = effect();
        bad_index.param_ids = vec!["unused".into(); MAX_PARAMS];
        bad_index.param_ids.push("rate".into());
        let index = validate_settings(&bad_index, RendererKind::Particles).unwrap_err();
        assert!(matches!(&index, RenderError::Context { cause, .. }
            if matches!(cause.as_ref(), RenderError::Invalid(_))));
        let mut failures = vec![settings, index];
        failures.extend(alpha::actual_refusals());
        failures.extend(numeric::actual_refusals());
        let mut causes = Vec::new();
        for (index, failure) in failures.into_iter().enumerate() {
            let request = 901 + index as u64;
            let original = failure.to_string();
            let host = motion_host::Error::from(failure);
            assert!(matches!(
                &host,
                motion_host::Error::Render(RenderError::Context { .. })
            ));
            assert_eq!(host.to_string(), original);
            let (reply, receive) = channel::<Result<Value, String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_generator_settings",
                started: Instant::now(),
            }));
            respond(reply, Err(host));
            CURRENT_COMMAND.set(None);
            assert_eq!(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err(),
                original
            );
            causes.push(original);
        }
        validate_settings(&healthy, RendererKind::Particles).unwrap();
        assert_eq!(bad_settings.scene.as_ref().unwrap().source_layer, Some(0));
        assert_eq!(bad_index.param_ids.len(), MAX_PARAMS + 1);
        std::fs::write(
            root.join("primary-causes.json"),
            serde_json::to_vec(&causes).unwrap(),
        )
        .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let causes: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.path().join("primary-causes.json")).unwrap())
            .unwrap();
    assert_eq!(causes.len(), 14);
    assert!(file.contains("logging initialized level=INFO"));
    for (request, cause) in (901..915).zip(causes) {
        let marker = format!("error #{request} test_generator_settings ");
        for text in [&file, &stderr] {
            let lines: Vec<_> = text.lines().filter(|line| line.contains(&marker)).collect();
            assert_eq!(lines.len(), 1, "{text}");
            let frame = match request {
                901..=912 => "frame 5.5".to_string(),
                913 => "frame inf".to_string(),
                914 => format!("frame {}", f64::MAX),
                _ => unreachable!(),
            };
            for context in [&cause, "layer 7", "effect 11", &frame, "ms:"] {
                assert!(lines[0].contains(context), "{text}");
            }
        }
        assert!(!stdout.contains(&cause));
        assert!(!stdout.contains(&marker));
    }
    assert!(!stderr.contains("logging initialized"));
    assert!(!stdout.contains("logging initialized"));
}
