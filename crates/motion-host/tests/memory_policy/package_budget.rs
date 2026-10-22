use super::Stub;
use motion_host::session;
use motion_render::resource_policy::MemoryPolicy;
use std::sync::Arc;

#[test]
fn package_install_and_pixel_reads_follow_current_session_cpu_policy_without_authoring_changes() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let builtin = motion_effects::builtin::package().unwrap();
    let mut manifest = builtin.manifest.clone();
    manifest.id = "test.host-package-budget".into();
    manifest.effects.truncate(1);
    manifest.effects[0].resources = vec!["assets/pixels.png".into()];
    for pass in &manifest.effects[0].passes {
        let path = source.join(&pass.shader);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &builtin.files[&pass.shader]).unwrap();
    }
    std::fs::create_dir(source.join("assets")).unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([17, 43, 91, 255]))
        .save(source.join("assets/pixels.png"))
        .unwrap();
    std::fs::write(
        source.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let bytes = motion_effects::package_directory(&source).unwrap();
    let input = directory.path().join("input.msfx");
    std::fs::write(&input, &bytes).unwrap();
    let project = directory.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let id = session::open(
        project,
        motion_core::Project::new(16, 16, 30, 6).unwrap(),
        Arc::new(Stub),
    )
    .unwrap();
    struct Open(i64);
    impl Drop for Open {
        fn drop(&mut self) {
            session::close(self.0);
        }
    }
    let _open = Open(id);
    let request = serde_json::json!({"op":"install", "path":input}).to_string();
    let (snapshot, revision, original) = session::with_session(id, |s| {
        Ok((
            s.engine.snapshot(),
            s.engine.revision(),
            s.effects.memory_policy(),
        ))
    })
    .unwrap();
    motion_host::ops::preview::configure_memory_policy(
        id,
        MemoryPolicy {
            decoded_image_budget_bytes: 15,
            ..original
        },
    )
    .unwrap();
    let error = motion_host::ops::effects::plugin(id, &request).unwrap_err();
    assert!(matches!(
        error,
        motion_host::Error::Effects(motion_effects::Error::DecodedResourceBudget {
            required_bytes: 16,
            budget_bytes: 15,
            ..
        })
    ));
    session::with_session(id, |s| {
        assert!(!s
            .effects
            .registry
            .packages
            .values()
            .any(|p| p.manifest.id == manifest.id));
        assert_eq!(s.engine.snapshot(), snapshot);
        assert_eq!(s.engine.revision(), revision);
        assert!(!s.engine.can_undo());
        Ok(())
    })
    .unwrap();
    motion_host::ops::preview::configure_memory_policy(
        id,
        MemoryPolicy {
            decoded_image_budget_bytes: 16,
            ..original
        },
    )
    .unwrap();
    motion_host::ops::effects::plugin(id, &request).unwrap();
    let program = session::with_session(id, |s| {
        let package = s
            .effects
            .registry
            .packages
            .values()
            .find(|p| p.manifest.id == manifest.id)
            .unwrap()
            .clone();
        let index = s.effects.programs.len();
        s.effects
            .programs
            .push(motion_render::effect_plan::EffectProgram {
                key: "test.host-package-budget".into(),
                shader: Arc::new(package.shaders.values().next().unwrap().clone()),
                resources: vec!["assets/pixels.png".into()],
                package: Some(package),
            });
        Ok(index as i32)
    })
    .unwrap();
    let expected = [17, 43, 91, 255].repeat(4);
    assert_eq!(
        motion_host::ops::effects::plugin_pixels(id, program, 0).unwrap(),
        expected
    );
    motion_host::ops::preview::configure_memory_policy(
        id,
        MemoryPolicy {
            decoded_image_budget_bytes: 15,
            ..original
        },
    )
    .unwrap();
    let error = motion_host::ops::effects::plugin_pixels(id, program, 0).unwrap_err();
    match error {
        motion_host::Error::ImageResource(error) => assert_eq!(
            error.resource_rejection(),
            Some(&motion_render::ResourceRejection::Budget {
                scope: motion_render::ResourceBudgetScope::DecodedImages,
                required_bytes: 16,
                budget_bytes: 15,
            })
        ),
        other => panic!("expected native decode refusal, got {other}"),
    }
    motion_host::ops::preview::configure_memory_policy(id, original).unwrap();
    assert_eq!(
        motion_host::ops::effects::plugin_pixels(id, program, 0).unwrap(),
        expected
    );
    assert_eq!(std::fs::read(input).unwrap(), bytes);
    session::with_session(id, |s| {
        assert_eq!(s.engine.snapshot(), snapshot);
        assert_eq!(s.engine.revision(), revision);
        assert!(!s.engine.can_undo());
        Ok(())
    })
    .unwrap();
}
