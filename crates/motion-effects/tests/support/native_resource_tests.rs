use super::*;
#[path = "native_resources.rs"]
mod fixture;

#[test]
#[ignore = "requires freshly qualified native OCIO resources"]
fn closed_resources_match_file_cpu_for_all_phases_and_retain_verified_bytes() {
    let runtime = runtime();
    let mut checked = 0;
    for case in fixture::cases() {
        let config = fixture::config(&case);
        let selection = fixture::selection(&config);
        for phase in [
            ColorProcessorPhase::InputToWorking,
            ColorProcessorPhase::WorkingToDisplay,
            ColorProcessorPhase::WorkingToOutput,
        ] {
            let program = runtime
                .prepare(
                    &config,
                    &selection,
                    phase,
                    AlphaMode::Straight,
                    AlphaMode::Straight,
                )
                .unwrap();
            assert_eq!(program.source().config_key, config.key());
            for (input, expected) in case.input.iter().zip(&case.expected) {
                let actual = program.apply_rgb([input[0], input[1], input[2]]).unwrap();
                for c in 0..3 {
                    assert!(
                        (actual[c] - expected[c]).abs() <= 0.0001 + 0.0002 * expected[c].abs(),
                        "{phase:?} {actual:?} {expected:?}"
                    );
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 180);
    println!(
        "native resource CPU pixels={checked}; sources already removed; all three phases passed"
    );
}

#[test]
#[ignore = "requires freshly qualified native OCIO resources"]
fn resource_failures_refuse_publication_and_same_name_changes_do_not_use_file_cache() {
    let runtime = runtime();
    let case = fixture::cases().remove(0);
    let prepare = |case: &fixture::Case| {
        let config = fixture::config(case);
        runtime.prepare(
            &config,
            &fixture::selection(&config),
            ColorProcessorPhase::InputToWorking,
            AlphaMode::Straight,
            AlphaMode::Straight,
        )
    };
    let original = prepare(&case).unwrap();
    let expected = original.apply_rgb([0.3, 0.4, 0.5]).unwrap();
    let mut negatives = 0;
    for text in [
        "not a Cube file",
        "LUT_3D_SIZE 4294967295\n",
        "LUT_1D_SIZE 2\n0 0 0\nNaN 1 1\n",
        "LUT_1D_SIZE 2\n0 0 0\n1 1 1\nREFERENCE ../other.clf\n",
    ] {
        let mut bad = case.clone();
        bad.cube = text.into();
        assert!(matches!(prepare(&bad), Err(Error::ResourceLut { .. })));
        assert_eq!(
            prepare(&case).unwrap().apply_rgb([0.3, 0.4, 0.5]).unwrap(),
            expected
        );
        negatives += 1;
    }
    for replacement in ["../outside.cube", "$UNDECLARED/file.cube"] {
        let mut bad = case.clone();
        bad.config = bad.config.replace(&case.name, replacement);
        assert_ne!(bad.config, case.config);
        let error = prepare(&bad).err().unwrap();
        assert!(
            matches!(error,Error::Native {phase:"prepare",category:1,ref cause} if cause.contains("external FileTransform")),
            "{error:?}"
        );
        negatives += 1;
    }
    let config = fixture::config(&case);
    let mut manifest = config.manifest().clone();
    manifest.resources.clear();
    let missing =
        VerifiedColorConfig::new(manifest, Arc::from(case.config.as_bytes()), BTreeMap::new())
            .unwrap();
    let mut selection = fixture::selection(&missing);
    selection.input = selection.working.clone();
    assert!(matches!(
        runtime.prepare(
            &missing,
            &selection,
            ColorProcessorPhase::InputToWorking,
            AlphaMode::Straight,
            AlphaMode::Straight
        ),
        Err(Error::Native { category: 1, .. })
    ));
    negatives += 1;
    let mut changed = case.clone();
    let lut = motion_effects::color_lut::ColorLut::from_cube(
        &case.cube,
        motion_effects::color_lut::LutInterpolation::Linear,
        motion_effects::color_lut::LutPurpose::Look,
    )
    .unwrap();
    let values = lut.values().iter().map(|p| p.map(|v| v + 0.25)).collect();
    changed.cube = motion_effects::color_lut::ColorLut::new(lut.descriptor(), values)
        .unwrap()
        .to_cube();
    let changed_program = prepare(&changed).unwrap();
    assert_ne!(
        original.source().config_key,
        changed_program.source().config_key
    );
    let alternate = changed_program.apply_rgb([0.3, 0.4, 0.5]).unwrap();
    assert!(alternate
        .iter()
        .zip(expected)
        .any(|(a, b)| (a - b).abs() > 0.2));
    assert_eq!(original.apply_rgb([0.3, 0.4, 0.5]).unwrap(), expected);
    assert_eq!(
        prepare(&case).unwrap().apply_rgb([0.3, 0.4, 0.5]).unwrap(),
        expected
    );
    println!("native resource negatives={negatives}; same logical name different content isolated; recovery passed");
}
