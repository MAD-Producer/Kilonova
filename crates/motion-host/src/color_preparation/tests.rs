use super::*;
use motion_effects::color_config::ContentIdentity;
#[path = "../../../motion-core/tests/support/color_project.rs"]
mod fixture;

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
