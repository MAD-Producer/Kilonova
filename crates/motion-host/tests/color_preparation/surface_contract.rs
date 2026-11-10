//! Native program preparation for an explicitly declared SDR window endpoint.
use super::*;

#[test]
#[ignore = "requires the pinned native OCIO runtime"]
fn surface_display_program_is_explicit_atomic_and_retained_without_config_io() {
    let reference = reference();
    for config in 0..reference.configs.len() {
        let root = tempfile::tempdir().unwrap();
        let mut project = document(&reference, config, AlphaMode::Straight, root.path());
        assert_eq!(
            settings(&mut project).selection.display.display,
            "sRGB - Display"
        );
        // Explicit authoring, not inference from the display's label or OCIO category.
        settings(&mut project).display.transfer = ColorTransfer::Srgb;
        let original = serde_json::to_vec(&project).unwrap();
        let observation = Observation::new(9500 + config as u64);
        let mut prepared =
            prep::prepare_native_authored_project(&project, root.path(), &runtime(), &observation)
                .unwrap()
                .unwrap();
        assert!(!observation.running());
        assert_eq!(prepared.colors.program_count(), 4);
        let surface_key = prepared.colors.surface_display().unwrap().key();
        assert!(prepared
            .colors
            .surface_display()
            .unwrap()
            .is_opaque_display());
        assert!(!prepared.colors.display().is_opaque_display());
        assert_ne!(surface_key, prepared.colors.display().key());
        assert_eq!(serde_json::to_vec(&project).unwrap(), original);
        let path = root
            .path()
            .join(&project.color_management.resources().unwrap()[0].path);
        let config_bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        prepared
            .authored
            .sample(&project, 0., &ExpressionEvaluator)
            .unwrap();
        assert_eq!(
            prepared.colors.surface_display().unwrap().key(),
            surface_key
        );
        assert!(matches!(
            prep::prepare_native_authored_project(&project, root.path(), &runtime(), &observation),
            Err(prep::Error::Resources(_))
        ));
        assert!(!observation.running());
        assert_eq!(serde_json::to_vec(&project).unwrap(), original);
        prepared
            .authored
            .sample(&project, 1., &ExpressionEvaluator)
            .unwrap();
        assert_eq!(
            prepared.colors.surface_display().unwrap().key(),
            surface_key
        );
        std::fs::write(&path, config_bytes).unwrap();

        for mutation in 0..4 {
            let mut incompatible = project.clone();
            let signal = &mut settings(&mut incompatible).display;
            match mutation {
                0 => signal.transfer = ColorTransfer::ConfigDefined,
                1 => signal.transfer = ColorTransfer::Linear,
                2 => signal.primaries = [[0.68, 0.32], [0.265, 0.69], [0.15, 0.06]],
                3 => signal.white_point = [0.32168, 0.33767],
                _ => unreachable!(),
            }
            let colors = prep::prepare_native_authored_project(
                &incompatible,
                root.path(),
                &runtime(),
                &observation,
            )
            .unwrap()
            .unwrap();
            assert!(
                colors.colors.surface_display().is_none(),
                "mutation={mutation}: no inferred sRGB surface fallback"
            );
            assert_eq!(colors.colors.program_count(), 3);
            assert!(
                !colors.colors.display().is_opaque_display(),
                "ordinary explicit display endpoint remains available"
            );
            assert_eq!(serde_json::to_vec(&project).unwrap(), original);
            assert_eq!(
                prepared.colors.surface_display().unwrap().key(),
                surface_key
            );
        }
        for transfer in [ColorTransfer::Srgb, ColorTransfer::ConfigDefined] {
            let mut nonlinear = project.clone();
            settings(&mut nonlinear).working.transfer = transfer;
            let error = prep::prepare_native_authored_project(
                &nonlinear,
                root.path(),
                &runtime(),
                &observation,
            )
            .err()
            .unwrap();
            let prep::Error::Provider { source, .. } = error else {
                panic!("authored working admission must preserve its original cause")
            };
            assert!(matches!(
                *source,
                motion_host::Error::Render(motion_render::RenderError::AuthoredScene(
                    AuthoredSceneError::Admission {
                        phase: "working",
                        ..
                    }
                ))
            ));
            assert!(!observation.running());
            assert_eq!(serde_json::to_vec(&project).unwrap(), original);
            assert_eq!(
                prepared.colors.surface_display().unwrap().key(),
                surface_key
            );
        }
    }
}
