use super::*;
use motion_effects::color_config::ContentIdentity;
#[path = "../../../motion-core/tests/support/color_project.rs"]
mod fixture;

#[test]
#[ignore = "requires explicit checksum-verified native OCIO 2.6.0 prefix"]
fn native_runtime_prepares_real_project_programs_without_modifying_the_document() {
    use motion_effects::color_config::{
        ColorConfigManifest, ColorSelection, OutputSelection, ReferenceSpace,
    };
    #[derive(serde::Deserialize)]
    struct Config {
        manifest: ColorConfigManifest,
        config: String,
        selection: ColorSelection,
    }
    #[derive(serde::Deserialize)]
    struct Reference {
        configs: Vec<Config>,
    }
    let reference: Reference = serde_json::from_slice(include_bytes!(
        "../../../motion-effects/tests/color_processor/reference.json"
    ))
    .unwrap();
    let manifest = std::path::PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_RUNTIME").expect("explicit native runtime manifest"),
    );
    for (index, item) in reference.configs.into_iter().enumerate() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = fixture::settings();
        settings.manifest = item.manifest;
        settings.selection = item.selection;
        let signal = |space: &motion_effects::color_config::SpaceReference| {
            let name = settings.manifest.declared_color_space(space).unwrap();
            let declared = &settings.manifest.spaces[name];
            fixture::signal(&declared.encoding, declared.reference)
        };
        settings.default_input = signal(&settings.selection.input);
        settings.authored_input = ColorInput {
            space: settings.selection.input.clone(),
            signal: settings.default_input.clone(),
        };
        settings.working = signal(&settings.selection.working);
        settings.display = fixture::signal("sdr-video", ReferenceSpace::Display);
        settings.output = match &settings.selection.output {
            OutputSelection::Space { space } => signal(space),
            OutputSelection::Display { .. } => settings.display.clone(),
        };
        let mut project = Project::new(64, 64, 30, 120).unwrap();
        project.color_management = fixture::managed(settings);
        project.validate().unwrap();
        for resource in project.color_management.resources().unwrap() {
            let path = root.path().join(resource.path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            assert_eq!(
                ContentIdentity::of(item.config.as_bytes()),
                resource.identity
            );
            std::fs::write(path, item.config.as_bytes()).unwrap();
        }
        let before = serde_json::to_value(&project).unwrap();
        let observation = Observation::new(8300 + index as u64);
        let prepared =
            prepare_native_project_colors(&project, root.path(), &manifest, &observation)
                .unwrap()
                .unwrap();
        assert!(!observation.running());
        assert_eq!(prepared.program_count(), 3);
        assert!(prepared.payload_bytes() > 0);
        assert_eq!(
            prepared.config().key(),
            match project.color_management.as_ref() {
                ProjectColorManagement::Ocio { settings } => settings.selection.config_key,
                _ => unreachable!(),
            }
        );
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
    }
}

#[test]
fn program_count_and_retained_payload_limits_refuse_without_publishing_partial_entries() {
    let settings = fixture::settings();
    let config = VerifiedColorConfig::new(
        settings.manifest.clone(),
        Arc::from(fixture::CONFIG),
        BTreeMap::from([("looks/example.cube".into(), Arc::from(fixture::LUT))]),
    )
    .unwrap();
    assert_eq!(
        ContentIdentity::of(config.config_bytes()),
        settings.manifest.config
    );
    let observation = Observation::new(601);
    let mut calls = 0;
    let mut provider = |request: Request<'_>| {
        calls += 1;
        Ok(ColorProcessorSource {
            version: motion_effects::color_processor::COLOR_PROCESSOR_VERSION,
            ocio_version: config.manifest().ocio_version,
            config_key: config.key(),
            selection_key: config.resolve(request.selection).unwrap().key,
            phase: request.phase,
            input_alpha: request.input_alpha,
            output_alpha: request.output_alpha,
            shader: "vec4 motion_ocio_transform(vec4 color) { return color; }".into(),
            tables: Vec::new(),
        })
    };
    let mut programs = Programs {
        config: &config,
        provider: &mut provider,
        observation: &observation,
        programs: BTreeMap::new(),
        payload_bytes: MAX_PROGRAM_PAYLOAD_BYTES,
    };
    let error = programs
        .get(
            &settings.selection,
            Target::DefaultInput,
            ColorProcessorPhase::InputToWorking,
            AlphaMode::Straight,
            AlphaMode::Straight,
            CodeValueRange::FULL,
            CodeValueRange::FULL,
        )
        .err()
        .unwrap();
    assert!(
        matches!(error, Error::Capacity { field: "compiled program payload bytes", requested, limit: MAX_PROGRAM_PAYLOAD_BYTES } if requested > MAX_PROGRAM_PAYLOAD_BYTES)
    );
    assert!(programs.programs.is_empty());
    assert_eq!(programs.payload_bytes, MAX_PROGRAM_PAYLOAD_BYTES);
    programs.payload_bytes = 0;
    let program = programs
        .get(
            &settings.selection,
            Target::DefaultInput,
            ColorProcessorPhase::InputToWorking,
            AlphaMode::Straight,
            AlphaMode::Straight,
            CodeValueRange::FULL,
            CodeValueRange::FULL,
        )
        .unwrap();
    programs.programs.clear();
    for index in 0..MAX_PROGRAMS {
        programs
            .programs
            .insert((index as u64).to_le_bytes().to_vec(), program.clone());
    }
    let payload = programs.payload_bytes;
    let error = programs
        .get(
            &settings.selection,
            Target::DefaultInput,
            ColorProcessorPhase::InputToWorking,
            AlphaMode::Straight,
            AlphaMode::Straight,
            CodeValueRange::FULL,
            CodeValueRange::FULL,
        )
        .err()
        .unwrap();
    assert!(matches!(
        error,
        Error::Capacity {
            field: "programs",
            requested: 257,
            limit: MAX_PROGRAMS
        }
    ));
    assert_eq!(programs.programs.len(), MAX_PROGRAMS);
    assert_eq!(programs.payload_bytes, payload);
    drop(programs);
    assert_eq!(
        calls, 2,
        "program-count admission must precede the provider"
    );
}

#[test]
fn diagnostic_cause_is_bounded_on_utf8_boundaries_without_replacing_native_error() {
    let message = format!("original cause: {}", "色".repeat(4096));
    let error = Error::Provider {
        target: Target::Source(73),
        phase: ColorProcessorPhase::InputToWorking,
        source: Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, message.clone()).into()),
    };
    let summary = brief(&error);
    assert!(summary.len() <= 1024);
    assert!(summary.contains("Source(73)"));
    assert!(summary.contains("original cause:"));
    let Error::Provider { source, .. } = error else {
        unreachable!()
    };
    let crate::Error::Io(cause) = *source else {
        panic!("native source type")
    };
    assert_eq!(cause.to_string(), message);
}
