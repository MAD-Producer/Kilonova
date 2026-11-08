use motion_core::{color_management::*, Project};
use motion_effects::{
    color_config::*, color_processor::*, image_contract::ImageMeaning, AlphaMode,
};
use motion_host::color_preparation::{self as prep, Observation, PreparedInput, Target};
use serde::Deserialize;
use std::{
    error::Error as _,
    path::Path,
    sync::{mpsc, Arc},
    time::Duration,
};
#[path = "color_preparation/authored.rs"]
mod authored;
#[path = "color_preparation/images.rs"]
mod images;

#[derive(Deserialize)]
struct Reference {
    configs: Vec<Config>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Config {
    manifest: ColorConfigManifest,
    config: String,
    selection: ColorSelection,
}
#[derive(Deserialize)]
struct Case {
    config: usize,
    processor: ColorProcessorSource,
    input: Vec<[f32; 4]>,
    expected: Vec<[f32; 4]>,
}
fn reference() -> Reference {
    serde_json::from_str(include_str!(
        "../../motion-effects/tests/color_processor/reference.json"
    ))
    .unwrap()
}
fn signal(manifest: &ColorConfigManifest, space: &SpaceReference, alpha: AlphaMode) -> ColorSignal {
    let name = manifest.declared_color_space(space).unwrap();
    let space = &manifest.spaces[name];
    let working = name == "ACEScg";
    ColorSignal {
        encoding: space.encoding.clone(),
        transfer: if space.encoding == "scene-linear" {
            ColorTransfer::Linear
        } else {
            ColorTransfer::ConfigDefined
        },
        reference: space.reference,
        primaries: if working {
            [[0.713, 0.293], [0.165, 0.83], [0.128, 0.044]]
        } else {
            [[0.64, 0.33], [0.3, 0.6], [0.15, 0.06]]
        },
        white_point: if working {
            [0.32168, 0.33767]
        } else {
            [0.3127, 0.3290]
        },
        range: SignalRange::Full {},
        alpha,
    }
}
fn project(config: &Config, alpha: AlphaMode, root: &Path) -> Project {
    let default_input = signal(&config.manifest, &config.selection.input, alpha);
    let working = signal(&config.manifest, &config.selection.working, alpha);
    let mut display = default_input.clone();
    display.reference = ReferenceSpace::Display;
    let settings = OcioProject {
        manifest: config.manifest.clone(),
        selection: config.selection.clone(),
        authored_input: ColorInput {
            space: config.selection.input.clone(),
            signal: default_input.clone(),
        },
        default_input: default_input.clone(),
        working,
        display,
        output: default_input,
        sources: Vec::new(),
    };
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.assets = (1..=4)
        .map(|id| motion_core::Asset {
            id,
            path: format!("assets/{id}.png"),
            width: 8,
            height: 8,
        })
        .collect();
    project.video_assets.push(motion_core::VideoAsset {
        id: 5,
        path: "assets/video.mp4".into(),
        bytes: 1,
        mime: "video/avc".into(),
        track: 0,
        width: 8,
        height: 8,
        rotation: 0,
        display_width: 8,
        display_height: 8,
        video_start_us: 0,
        video_end_us: 1_000_000,
        duration_us: 1_000_000,
        frame_count: 30,
        variable_frame_rate: false,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    project.color_management = Arc::new(ProjectColorManagement::Ocio {
        settings: Box::new(settings),
    });
    let resource = &project.color_management.resources().unwrap()[0];
    let path = root.join(&resource.path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    assert_eq!(
        ContentIdentity::of(config.config.as_bytes()),
        resource.identity
    );
    std::fs::write(path, &config.config).unwrap();
    project.validate().unwrap();
    project
}
fn settings(project: &mut Project) -> &mut OcioProject {
    let ProjectColorManagement::Ocio { settings } = Arc::make_mut(&mut project.color_management)
    else {
        panic!("managed fixture")
    };
    settings
}
fn provider(
    reference: &Reference,
    request: prep::Request<'_>,
) -> motion_host::Result<ColorProcessorSource> {
    let source = &reference
        .cases
        .iter()
        .find(|c| {
            reference.configs[c.config].selection == *request.selection
                && c.processor.phase == request.phase
                && c.processor.input_alpha == request.input_alpha
                && c.processor.output_alpha == request.output_alpha
        })
        .expect("explicit native OCIO fixture exists")
        .processor;
    Ok(source.clone())
}

#[test]
fn actual_aces_documents_prepare_all_phases_deduplicate_and_preserve_data_routes() {
    let reference = reference();
    for (config_index, config) in reference.configs.iter().enumerate() {
        for alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
            let root = tempfile::tempdir().unwrap();
            let mut project = project(config, alpha, root.path());
            let settings = settings(&mut project);
            settings.sources.push(SourceColorAssignment {
                asset: 2,
                interpretation: SourceInterpretation::Color {
                    input: settings.authored_input.clone(),
                },
            });
            settings.sources.push(SourceColorAssignment {
                asset: 3,
                interpretation: SourceInterpretation::Data {
                    meaning: ImageMeaning::LinearDepth {},
                },
            });
            let original = project.clone();
            let observation = Observation::new(100 + config_index as u64);
            let mut calls = Vec::new();
            let prepared = prep::prepare_project_colors(
                &project,
                root.path(),
                |request| {
                    assert!(observation.running());
                    assert_eq!(request.config.key(), config.selection.config_key);
                    calls.push(request.target);
                    provider(&reference, request)
                },
                &observation,
            )
            .unwrap()
            .unwrap();
            assert!(!observation.running());
            assert_eq!(
                calls,
                [Target::DefaultInput, Target::Display, Target::Output]
            );
            assert_eq!(prepared.program_count(), 3);
            assert_eq!(
                prepared.default_input().phase(),
                ColorProcessorPhase::InputToWorking
            );
            assert_eq!(
                prepared.display().phase(),
                ColorProcessorPhase::WorkingToDisplay
            );
            assert_eq!(
                prepared.output().phase(),
                ColorProcessorPhase::WorkingToOutput
            );
            assert_eq!(
                prepared.display().output_space(),
                prepared
                    .config()
                    .display_identity(&config.selection.display)
                    .unwrap()
            );
            let OutputSelection::Space { space } = &config.selection.output else {
                panic!("native fixture output space")
            };
            assert_eq!(
                prepared.output().output_space(),
                prepared.config().space_identity(space).unwrap()
            );
            assert_eq!(
                prepared.payload_bytes(),
                prepared.default_input().payload_bytes()
                    + prepared.display().payload_bytes()
                    + prepared.output().payload_bytes()
            );
            assert!(Arc::ptr_eq(
                prepared.default_input(),
                prepared.authored_input()
            ));
            for id in [1, 2, 4, 5] {
                let Some(PreparedInput::Color(program)) = prepared.source(id) else {
                    panic!("color route")
                };
                assert!(Arc::ptr_eq(prepared.default_input(), program));
                assert_eq!(program.output_space(), prepared.display().input_space());
                assert_eq!(program.output_space(), prepared.output().input_space());
            }
            assert!(matches!(
                prepared.source(3),
                Some(PreparedInput::Data(ImageMeaning::LinearDepth {}))
            ));
            assert!(prepared.source(99).is_none());
            assert_eq!(prepared.default_input().input_alpha(), alpha);
            assert_eq!(prepared.settings(), project.color_management.as_ref());
            assert_eq!(project, original);
            std::fs::remove_file(
                root.path()
                    .join(&project.color_management.resources().unwrap()[0].path),
            )
            .unwrap();
            assert_eq!(prepared.config().config_bytes(), config.config.as_bytes());
        }
    }
}

#[test]
fn explicit_code_ranges_cover_every_role_and_are_part_of_program_identity() {
    let reference = reference();
    let limited8 = SignalRange::Limited {
        black: 16. / 255.,
        white: 235. / 255.,
    };
    let limited10 = SignalRange::Limited {
        black: 64. / 1023.,
        white: 940. / 1023.,
    };
    let range8 = CodeValueRange::new(16. / 255., 235. / 255.).unwrap();
    let range10 = CodeValueRange::new(64. / 1023., 940. / 1023.).unwrap();
    for config in &reference.configs {
        for alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
            let root = tempfile::tempdir().unwrap();
            let mut project = project(config, alpha, root.path());
            let settings = settings(&mut project);
            settings.default_input.range = limited8.clone();
            settings.working.range = limited10.clone();
            settings.output.range = limited8.clone();
            let mut source = settings.authored_input.clone();
            source.signal.range = limited10.clone();
            settings.sources = vec![
                SourceColorAssignment {
                    asset: 2,
                    interpretation: SourceInterpretation::Color { input: source },
                },
                SourceColorAssignment {
                    asset: 3,
                    interpretation: SourceInterpretation::Data {
                        meaning: ImageMeaning::LinearDepth {},
                    },
                },
            ];
            let original = project.clone();
            let observation = Observation::new(601);
            let mut calls = Vec::new();
            let prepared = prep::prepare_project_colors(
                &project,
                root.path(),
                |request| {
                    assert!(observation.running());
                    calls.push(request.target);
                    provider(&reference, request)
                },
                &observation,
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                calls,
                [
                    Target::DefaultInput,
                    Target::AuthoredInput,
                    Target::Source(2),
                    Target::Display,
                    Target::Output
                ]
            );
            assert_eq!(prepared.program_count(), 5);
            assert_eq!(
                (
                    prepared.default_input().input_range(),
                    prepared.default_input().output_range()
                ),
                (range8, range10)
            );
            assert_eq!(
                (
                    prepared.authored_input().input_range(),
                    prepared.authored_input().output_range()
                ),
                (CodeValueRange::FULL, range10)
            );
            let Some(PreparedInput::Color(source)) = prepared.source(2) else {
                panic!("explicit source color");
            };
            assert_eq!(
                (source.input_range(), source.output_range()),
                (range10, range10)
            );
            assert_ne!(source.key(), prepared.default_input().key());
            assert_ne!(source.key(), prepared.authored_input().key());
            assert_ne!(
                prepared.default_input().key(),
                prepared.authored_input().key()
            );
            for asset in [1, 4, 5] {
                let Some(PreparedInput::Color(source)) = prepared.source(asset) else {
                    panic!("default source color");
                };
                assert!(Arc::ptr_eq(source, prepared.default_input()));
            }
            assert_eq!(
                (
                    prepared.display().input_range(),
                    prepared.display().output_range()
                ),
                (range10, CodeValueRange::FULL)
            );
            assert_eq!(
                (
                    prepared.output().input_range(),
                    prepared.output().output_range()
                ),
                (range10, range8)
            );
            assert!(matches!(
                prepared.source(3),
                Some(PreparedInput::Data(ImageMeaning::LinearDepth {}))
            ));
            assert!(!observation.running());
            assert_eq!(project, original);
        }
    }
}

#[test]
fn every_unrepresentable_role_refuses_before_provider_with_its_original_cause() {
    let reference = reference();
    let root = tempfile::tempdir().unwrap();
    let original = project(&reference.configs[0], AlphaMode::Straight, root.path());
    for target in [
        Target::DefaultInput,
        Target::AuthoredInput,
        Target::Working,
        Target::Display,
        Target::Output,
        Target::Source(2),
    ] {
        let mut project = original.clone();
        let settings = settings(&mut project);
        let bad = SignalRange::Limited {
            black: 0.5,
            white: 0.5 + f64::EPSILON,
        };
        match target {
            Target::DefaultInput => settings.default_input.range = bad,
            Target::AuthoredInput => settings.authored_input.signal.range = bad,
            Target::Working => settings.working.range = bad,
            Target::Display | Target::SurfaceDisplay => settings.display.range = bad,
            Target::Output => settings.output.range = bad,
            Target::Source(asset) => {
                let mut input = settings.authored_input.clone();
                input.signal.range = bad;
                settings.sources.push(SourceColorAssignment {
                    asset,
                    interpretation: SourceInterpretation::Color { input },
                });
            }
        }
        project.validate().unwrap();
        let snapshot = project.clone();
        let observation = Observation::new(602);
        let error = prep::prepare_project_colors(
            &project,
            root.path(),
            |_| panic!("unrepresentable range entered provider"),
            &observation,
        )
        .err()
        .unwrap();
        assert!(
            matches!(&error, prep::Error::Range { target: got, source: ColorProcessorError::Invalid { field: "code range", reason }, .. } if *got == target && *reason == "black/white interval is not a normal positive Float32 value")
        );
        assert!(error.source().is_some());
        assert!(!observation.running());
        assert_eq!(project, snapshot);
    }
}

#[test]
fn distinct_authored_and_source_spaces_do_not_inherit_the_default_input() {
    let reference = reference();
    let root = tempfile::tempdir().unwrap();
    let mut project = project(&reference.configs[2], AlphaMode::Straight, root.path());
    let settings = settings(&mut project);
    settings.authored_input = ColorInput {
        space: settings.selection.working.clone(),
        signal: settings.working.clone(),
    };
    settings.sources.push(SourceColorAssignment {
        asset: 2,
        interpretation: SourceInterpretation::Color {
            input: settings.authored_input.clone(),
        },
    });
    let prepared = prep::prepare_project_colors(
        &project,
        root.path(),
        |request| {
            if request.selection.input == request.selection.working {
                assert_eq!(request.target, Target::AuthoredInput);
                // Independently authored exact identity: both endpoints are the same space.
                Ok(ColorProcessorSource {
                    version: COLOR_PROCESSOR_VERSION,
                    ocio_version: request.config.manifest().ocio_version,
                    config_key: request.config.key(),
                    selection_key: request.config.resolve(request.selection).unwrap().key,
                    phase: request.phase,
                    input_alpha: request.input_alpha,
                    output_alpha: request.output_alpha,
                    shader: "vec4 motion_ocio_transform(vec4 color) { return color; }".into(),
                    tables: Vec::new(),
                })
            } else {
                provider(&reference, request)
            }
        },
        &Observation::new(201),
    )
    .unwrap()
    .unwrap();
    assert_eq!(prepared.program_count(), 4);
    assert_ne!(
        prepared.default_input().input_space(),
        prepared.authored_input().input_space()
    );
    assert_eq!(
        prepared.authored_input().input_space(),
        prepared.authored_input().output_space()
    );
    let Some(PreparedInput::Color(program)) = prepared.source(2) else {
        panic!("override route")
    };
    assert!(Arc::ptr_eq(program, prepared.authored_input()));
}

#[test]
fn wrong_phase_alpha_identity_shader_and_provider_causes_refuse_without_partial_result() {
    let reference = reference();
    let root = tempfile::tempdir().unwrap();
    let project = project(&reference.configs[0], AlphaMode::Straight, root.path());
    let before = project.clone();
    let observation = Observation::new(301);
    for fault in 0..5 {
        let error = prep::prepare_project_colors(
            &project,
            root.path(),
            |request| {
                let mut source = provider(&reference, request)?;
                match fault {
                    0 => source.phase = ColorProcessorPhase::WorkingToOutput,
                    1 => source.input_alpha = AlphaMode::Premultiplied,
                    2 => source.config_key[0] ^= 1,
                    3 => source.shader = "invalid original GLSL".into(),
                    4 => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "original provider missing config",
                        )
                        .into())
                    }
                    _ => unreachable!(),
                }
                Ok(source)
            },
            &observation,
        )
        .err()
        .unwrap();
        match fault {
            0 | 1 => assert!(matches!(
                error,
                prep::Error::Binding {
                    target: Target::DefaultInput,
                    ..
                }
            )),
            2 | 3 => assert!(matches!(
                error,
                prep::Error::Compile {
                    target: Target::DefaultInput,
                    ..
                }
            )),
            4 => {
                let prep::Error::Provider { source, .. } = error else {
                    panic!("typed provider error")
                };
                let motion_host::Error::Io(cause) = source.as_ref() else {
                    panic!("original IO type")
                };
                assert_eq!(cause.kind(), std::io::ErrorKind::NotFound);
                assert_eq!(cause.to_string(), "original provider missing config");
            }
            _ => unreachable!(),
        }
        assert!(!observation.running());
        assert_eq!(project, before);
    }
    let result = prep::prepare_project_colors(
        &project,
        root.path(),
        |request| provider(&reference, request),
        &observation,
    )
    .unwrap()
    .unwrap();
    assert_eq!(result.program_count(), 3);
    assert!(!observation.running());
}

#[test]
fn missing_changed_and_unrepresentable_inputs_refuse_before_provider_and_unmanaged_is_a_normal_none(
) {
    let reference = reference();
    let root = tempfile::tempdir().unwrap();
    let mut project = project(&reference.configs[0], AlphaMode::Straight, root.path());
    let observation = Observation::new(401);
    settings(&mut project).default_input.range = SignalRange::Limited {
        black: 0.5,
        white: 0.5 + f64::EPSILON,
    };
    assert!(matches!(
        prep::prepare_project_colors(
            &project,
            root.path(),
            |_| panic!("unrepresentable range cannot compile"),
            &observation
        ),
        Err(prep::Error::Range {
            target: Target::DefaultInput,
            ..
        })
    ));
    settings(&mut project).default_input.range = SignalRange::Full {};
    let resource = root
        .path()
        .join(&project.color_management.resources().unwrap()[0].path);
    let original = std::fs::read(&resource).unwrap();
    std::fs::write(&resource, vec![b'x'; original.len()]).unwrap();
    let error = prep::prepare_project_colors(
        &project,
        root.path(),
        |_| panic!("unverified bytes cannot compile"),
        &observation,
    )
    .err()
    .unwrap();
    assert!(matches!(error, prep::Error::Resources(_)));
    assert!(error.source().is_some());
    std::fs::remove_file(&resource).unwrap();
    assert!(matches!(
        prep::prepare_project_colors(
            &project,
            root.path(),
            |_| panic!("missing bytes cannot compile"),
            &observation
        ),
        Err(prep::Error::Resources(_))
    ));
    let unmanaged = Project::new(64, 64, 30, 120).unwrap();
    assert!(prep::prepare_project_colors(
        &unmanaged,
        Path::new("does-not-exist"),
        |_| panic!("unmanaged cannot call OCIO"),
        &observation
    )
    .unwrap()
    .is_none());
    assert!(!observation.running());
}

#[test]
fn blocking_and_unwinding_providers_publish_and_clear_inflight_without_swallowing_panic() {
    let reference = reference();
    let root = tempfile::tempdir().unwrap();
    let project = project(&reference.configs[0], AlphaMode::Straight, root.path());
    let observation = Observation::new(501);
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let project = &project;
        let root = root.path();
        let observation = &observation;
        let worker = scope.spawn(move || {
            prep::prepare_project_colors(
                project,
                root,
                |_| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    Err(std::io::Error::new(std::io::ErrorKind::Other, "controlled release").into())
                },
                observation,
            )
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(observation.request(), 501);
        assert!(observation.running());
        assert!(matches!(
            prep::prepare_project_colors(
                project,
                root,
                |_| panic!("busy must not reenter"),
                observation
            ),
            Err(prep::Error::Busy)
        ));
        assert!(observation.running());
        release_tx.send(()).unwrap();
        assert!(worker.join().unwrap().is_err());
    });
    assert!(!observation.running());
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = prep::prepare_project_colors(
            &project,
            root.path(),
            |_| panic!("original provider panic"),
            &observation,
        );
    }))
    .unwrap_err();
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"original provider panic")
    );
    assert!(!observation.running());
}
