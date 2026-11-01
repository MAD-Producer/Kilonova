use super::*;
use crate::tracking::{
    asset::{Coordinates, Kind, Observation, Rate, Sample, Track, Visibility, VERSION},
    encode,
    plate::Extent,
};
use motion_core::{media_assets::AssetKind, storage, Engine};
use std::{error::Error as _, fs, io::Cursor};

fn frozen_point_binding() -> motion_core::tracking::point::PointBinding {
    use motion_core::tracking::{plate::Rotation, point::*};
    PointBinding {
        asset: 2,
        track: "manually corrected point".into(),
        anchor_local_frame: 0,
        source_anchor_pts: -3003,
        speed_numerator: 0,
        speed_denominator: 1,
        reference_pixels: [1., 1.],
        crop: [0., 0., 8., 6.],
        rotation: Rotation::None,
        interpolation: Interpolation::Linear,
        max_gap_ticks: 0,
        minimum_confidence: 0.8,
        missing: Missing::Error,
    }
}

#[test]
fn point_binding_is_a_real_undoable_command_and_survives_save_package_and_reopen() {
    let f = Fixture::new();
    let (mut engine, _) = f.attached();
    let mut layer =
        motion_core::Layer::solid(7, "Tracked object", [4.; 2], [30., 25., 0.], [1.; 4]);
    layer.content = motion_core::Content::Null;
    engine.apply(motion_core::Command::Add { layer }).unwrap();
    let before = engine.project().clone();
    let command = motion_core::Command::SetTrackingPoint {
        object: 7,
        binding: Some(frozen_point_binding()),
    };
    let wire = serde_json::to_string(&command).unwrap();
    assert!(wire.contains("set_tracking_point"));
    assert!(!serde_json::from_str::<motion_core::Command>(&wire)
        .unwrap()
        .changes_resources());
    engine.apply(command).unwrap();
    let attached = engine.project().clone();
    let position = attached.layers[0].sampled_position(&attached, 0.).unwrap();
    assert!((position[0] - 22.).abs() < 1e-5);
    assert!((position[1] - (25. + 2.125 * 64. / 6.)).abs() < 1e-5);
    assert!(attached.layers[0].transform.position.keys.is_empty());
    assert!(before.tracking_assets[0]
        .data
        .shares_storage(&attached.tracking_assets[0].data));
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &attached);
    storage::save(f.root.path(), &attached).unwrap();
    assert_eq!(storage::load(f.root.path()).unwrap(), attached);
    let package = f.root.path().join("tracked.msproj");
    storage::export_package(f.root.path(), &attached, &package).unwrap();
    let reopened = tempfile::tempdir().unwrap();
    let destination = reopened.path().join("unpacked");
    storage::import_package(&package, &destination).unwrap();
    let restored = storage::load(&destination).unwrap();
    assert_eq!(
        restored.layers[0].sampled_position(&restored, 0.).unwrap(),
        position
    );
    assert_eq!(
        fs::read(destination.join(&attached.tracking_assets[0].path)).unwrap(),
        f.original
    );
}

#[test]
fn point_binding_command_rejects_invalid_or_locked_targets_atomically_and_detaches_explicitly() {
    let f = Fixture::new();
    let (mut engine, _) = f.attached();
    engine
        .apply(motion_core::Command::Add {
            layer: motion_core::Layer::solid(7, "Object", [4.; 2], [0.; 3], [1.; 4]),
        })
        .unwrap();
    let before = engine.project().clone();
    let mut bad = frozen_point_binding();
    bad.track = "absent".into();
    assert!(engine
        .apply(motion_core::Command::SetTrackingPoint {
            object: 7,
            binding: Some(bad)
        })
        .is_err());
    assert_eq!(engine.project(), &before);
    assert!(serde_json::from_str::<motion_core::Command>(
        r#"{"op":"set_tracking_point","object":7}"#
    )
    .is_err());
    engine
        .apply(motion_core::Command::SetTrackingPoint {
            object: 7,
            binding: Some(frozen_point_binding()),
        })
        .unwrap();
    engine
        .apply(motion_core::Command::SetTrackingPoint {
            object: 7,
            binding: None,
        })
        .unwrap();
    assert_eq!(engine.project(), &before);
    let mut locked = before;
    locked.layers[0].locked = true;
    let mut engine = Engine::new(locked.clone()).unwrap();
    assert!(matches!(
        engine.apply(motion_core::Command::SetTrackingPoint {
            object: 7,
            binding: Some(frozen_point_binding())
        }),
        Err(motion_core::Error::Locked(7))
    ));
    assert_eq!(engine.project(), &locked);
}

#[test]
fn edited_runtime_copy_cannot_replace_the_managed_original_during_save_load_or_packaging() {
    let f = Fixture::new();
    let (engine, _) = f.attached();
    storage::save(f.root.path(), engine.project()).unwrap();
    let saved = fs::read(f.root.path().join("project.json")).unwrap();
    let mut forged = engine.project().clone();
    let mut data = (*forged.tracking_assets[0].data).clone();
    data.tracks[0].samples[0].observation = Some(Observation::Point([1., 1.]));
    forged.tracking_assets[0].data = motion_core::tracking::shared::SharedData::new(data).unwrap();
    forged.validate().unwrap(); // Typed shape is valid; original equivalence is an IO boundary.
    assert!(storage::save(f.root.path(), &forged)
        .unwrap_err()
        .to_string()
        .contains("differs from original"));
    assert_eq!(fs::read(f.root.path().join("project.json")).unwrap(), saved);
    assert!(storage::PackageSnapshot::new(f.root.path(), &forged).is_err());
    fs::write(
        f.root.path().join("project.json"),
        serde_json::to_vec(&forged).unwrap(),
    )
    .unwrap();
    assert!(storage::load(f.root.path())
        .unwrap_err()
        .to_string()
        .contains("differs from original"));
}

#[test]
fn actual_expression_references_and_scene_hierarchy_include_source_displacement_exactly_once() {
    use motion_core::{
        Axis, Command, Content, ExpressionTarget, Layer, ProjectExpressions, Property,
        PropertyExpression,
    };
    let f = Fixture::new();
    let (mut engine, _) = f.attached();
    let mut tracked = Layer::solid(7, "Tracked null", [1.; 2], [30., 25., 0.], [0.; 4]);
    tracked.content = Content::Null;
    engine.apply(Command::Add { layer: tracked }).unwrap();
    engine
        .apply(Command::Add {
            layer: Layer::solid(8, "Reference", [4.; 2], [32., 32., 0.], [1.; 4]),
        })
        .unwrap();
    engine
        .apply(Command::SetTrackingPoint {
            object: 7,
            binding: Some(frozen_point_binding()),
        })
        .unwrap();
    let target = |object, axis| ExpressionTarget::Property {
        object,
        property: Property::Position,
        axis,
    };
    let reference = motion_core::expression_reference_source(&target(7, None)).unwrap();
    let mut p = engine.project().clone();
    p.expressions.push(PropertyExpression {
        target: target(7, None),
        source: "value + [2,3,0]".into(),
        enabled: true,
        seed: 0,
        profile: motion_core::EXPRESSION_PROFILE.into(),
    });
    p.expressions.push(PropertyExpression {
        target: target(8, None),
        source: reference,
        enabled: true,
        seed: 0,
        profile: motion_core::EXPRESSION_PROFILE.into(),
    });
    p.validate().unwrap();
    let evaluated = p.evaluated_at(1.).unwrap();
    let a = evaluated.layers[0]
        .sampled_position(&evaluated, 1.)
        .unwrap();
    let b = evaluated.layers[1]
        .sampled_position(&evaluated, 1.)
        .unwrap();
    assert_eq!(a, b);
    assert!((a[0] - 24.).abs() < 1e-5);
    // Axis reference also sees the complete position, while its expression remains a correction.
    p.expressions[1].target = target(8, Some(Axis::X));
    p.expressions[1].source =
        motion_core::expression_reference_source(&target(7, Some(Axis::X))).unwrap();
    let evaluated = p.evaluated_at(1.).unwrap();
    assert!(
        (evaluated.layers[1]
            .sampled_position(&evaluated, 1.)
            .unwrap()[0]
            - 24.)
            .abs()
            < 1e-5
    );
    let mut scene = motion_render::Scene::new(&p).unwrap();
    scene
        .sample(&p, 1., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert!((scene.world_matrix(7).unwrap().w_axis.x - (24. - 32.)).abs() < 1e-5);
}

struct Fixture {
    root: tempfile::TempDir,
    project: Project,
    plate: Vec<u8>,
    solve: Asset,
    original: Vec<u8>,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("assets")).unwrap();
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(8, 6, |x, y| {
            image::Rgba([x as u8 * 21, y as u8 * 31, 93, 255])
        }))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let plate = png.into_inner();
        fs::write(root.path().join("assets/plate.png"), &plate).unwrap();
        let mut project = Project::new(64, 64, 30, 90).unwrap();
        project.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/plate.png".into(),
            width: 8,
            height: 6,
        });
        let hash = storage::resource_sha256(&plate);
        let solve = Asset {
            version: VERSION,
            source_id: "original calibration plate".into(),
            source_sha256: std::array::from_fn(|i| {
                u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16).unwrap()
            }),
            source_extent: Extent::new(8, 6).unwrap(),
            coordinates: Coordinates::RightDownForward,
            ticks_per_second: 90000,
            nominal_rate: Rate::new(30000, 1001).unwrap(),
            source_pts: vec![-3003],
            meters_per_world_unit: None,
            tracks: vec![
                Track {
                    id: "manually corrected point".into(),
                    kind: Kind::Point,
                    reference_index: None,
                    samples: vec![Sample {
                        source_index: 0,
                        visibility: Visibility::Visible,
                        confidence: 0.8123456789012345,
                        observation: Some(Observation::Point([-0.0, 3.125])),
                        manual_correction: true,
                    }],
                },
                Track {
                    id: "occluded".into(),
                    kind: Kind::Point,
                    reference_index: None,
                    samples: vec![Sample {
                        source_index: 0,
                        visibility: Visibility::Occluded,
                        confidence: 0.,
                        observation: None,
                        manual_correction: true,
                    }],
                },
                Track {
                    id: "lost".into(),
                    kind: Kind::Point,
                    reference_index: None,
                    samples: vec![Sample {
                        source_index: 0,
                        visibility: Visibility::Lost,
                        confidence: 0.,
                        observation: None,
                        manual_correction: false,
                    }],
                },
            ],
        };
        // Deliberate legal original formatting, different from the encoder output.
        let mut original = b" \n\t".to_vec();
        original.extend(encode::to_vec(&solve).unwrap());
        original.extend(b"\r\n ");
        Self {
            root,
            project,
            plate,
            solve,
            original,
        }
    }
    fn reference() -> AssetRef {
        AssetRef {
            kind: AssetKind::Image,
            id: 1,
        }
    }
    fn prepare(&self, id: u64) -> Result<Attachment, Error> {
        let proof =
            source::verify(&self.solve, &mut Cursor::new(&self.plate), 4096, || false).unwrap();
        Attachment::prepare(&self.project, id, Self::reference(), &self.original, proof)
    }
    fn attached(&self) -> (Engine, TrackingAsset) {
        let attachment = self.prepare(2).unwrap();
        let descriptor = attachment.descriptor().clone();
        let path = self.root.path().join(&descriptor.path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, attachment.original()).unwrap();
        let mut engine = Engine::new(self.project.clone()).unwrap();
        engine.apply(attachment.registration()).unwrap();
        (engine, descriptor)
    }
}

#[test]
fn attachment_is_one_undoable_registration_and_preserves_byte_exact_original() {
    let f = Fixture::new();
    let attachment = f.prepare(2).unwrap();
    assert_eq!(attachment.original(), f.original);
    assert_ne!(
        attachment.original(),
        encode::to_vec(attachment.solve()).unwrap()
    );
    let (mut engine, descriptor) = f.attached();
    assert_eq!(engine.project().tracking_assets, vec![descriptor.clone()]);
    let attached = engine.project().clone();
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &f.project);
    assert_eq!(
        engine.history_diagnostics()["modelBytes"].as_u64(),
        Some(serde_json::to_vec(&attached).unwrap().len() as u64)
    );
    assert!(!engine.undo().unwrap());
    assert_eq!(
        fs::read(f.root.path().join(&descriptor.path)).unwrap(),
        f.original
    );
    assert!(engine.redo().unwrap());
    assert_eq!(
        engine.history_diagnostics()["modelBytes"].as_u64(),
        Some(serde_json::to_vec(&f.project).unwrap().len() as u64)
    );
    assert!(!engine.redo().unwrap());
    assert_eq!(engine.project().tracking_assets, vec![descriptor]);
}

#[test]
fn save_reopen_and_archive_keep_raw_solve_metadata_and_source_identity() {
    let f = Fixture::new();
    let (engine, descriptor) = f.attached();
    storage::save(f.root.path(), engine.project()).unwrap();
    let reopened = storage::load(f.root.path()).unwrap();
    assert_eq!(reopened, *engine.project());
    let archive = f.root.path().join("tracking.msproj");
    storage::export_package(f.root.path(), &reopened, &archive).unwrap();
    let destination = f.root.path().join("restored");
    let restored = storage::import_package(&archive, &destination).unwrap();
    assert_eq!(restored, reopened);
    assert_eq!(
        fs::read(destination.join(&descriptor.path)).unwrap(),
        f.original
    );
    assert_eq!(
        fs::read(destination.join("assets/plate.png")).unwrap(),
        f.plate
    );
    let solve = load_original(
        &destination,
        &restored,
        2,
        &mut Cursor::new(&f.plate),
        4096,
        || false,
    )
    .unwrap();
    assert_eq!(
        encode::to_vec(&solve).unwrap(),
        encode::to_vec(&f.solve).unwrap()
    );
    assert_eq!(solve.source_pts, vec![-3003]);
    assert_eq!(
        solve.tracks[0].samples[0].confidence.to_bits(),
        0.8123456789012345f64.to_bits()
    );
    assert!(solve.tracks[0].samples[0].manual_correction);
    assert_eq!(solve.tracks[1].samples[0].visibility, Visibility::Occluded);
    assert!(solve.tracks[1].samples[0].observation.is_none());
    assert_eq!(solve.tracks[2].samples[0].visibility, Visibility::Lost);
}

#[test]
fn equal_length_document_tampering_is_rejected_without_overwriting_saved_project() {
    let f = Fixture::new();
    let (engine, descriptor) = f.attached();
    storage::save(f.root.path(), engine.project()).unwrap();
    let saved = fs::read(f.root.path().join("project.json")).unwrap();
    let mut changed = f.original.clone();
    changed[0] = b'\t'; // Still valid JSON, same size, different identity.
    fs::write(f.root.path().join(&descriptor.path), changed).unwrap();
    assert!(storage::load(f.root.path())
        .unwrap_err()
        .to_string()
        .contains("content mismatch"));
    assert!(storage::save(f.root.path(), engine.project()).is_err());
    assert_eq!(fs::read(f.root.path().join("project.json")).unwrap(), saved);
    assert!(matches!(
        load_original(
            f.root.path(),
            engine.project(),
            2,
            &mut Cursor::new(&f.plate),
            4096,
            || false
        ),
        Err(Error::Project(_))
    ));
}

#[test]
fn archive_rechecks_document_changed_after_snapshot_and_keeps_existing_output() {
    let f = Fixture::new();
    let (engine, descriptor) = f.attached();
    let snapshot = storage::PackageSnapshot::new(f.root.path(), engine.project()).unwrap();
    assert_eq!(snapshot.source_count(), 2);
    let mut changed = f.original.clone();
    changed[0] = b'\t';
    fs::write(f.root.path().join(descriptor.path), changed).unwrap();
    let output = f.root.path().join("existing.msproj");
    fs::write(&output, b"previous valid output").unwrap();
    let error = match snapshot.prepare(&output, |_, _| Ok(())) {
        Ok(_) => panic!("changed original document was packaged"),
        Err(error) => error,
    };
    assert!(error
        .to_string()
        .contains("tracking resource content changed while packaging"));
    assert_eq!(fs::read(output).unwrap(), b"previous valid output");
}

#[test]
fn missing_document_preserves_typed_io_cause_at_load_and_save_boundaries() {
    let f = Fixture::new();
    let (engine, descriptor) = f.attached();
    fs::remove_file(f.root.path().join(&descriptor.path)).unwrap();
    for error in [
        storage::save(f.root.path(), engine.project()).unwrap_err(),
        storage::read_tracking_resource(f.root.path(), &descriptor).unwrap_err(),
    ] {
        assert!(matches!(
            error,
            motion_core::Error::TrackingResourceIo {
                phase: "resolve",
                ..
            }
        ));
        assert!(error.source().is_some());
        assert!(error.to_string().contains(&descriptor.path));
    }
}

#[test]
fn replaced_plate_and_cancelled_verification_do_not_modify_project_or_original() {
    let f = Fixture::new();
    let (mut engine, descriptor) = f.attached();
    let before = engine.project().clone();
    let mut changed = f.plate.clone();
    changed[0] ^= 1;
    assert!(matches!(
        load_original(
            f.root.path(),
            engine.project(),
            2,
            &mut Cursor::new(changed),
            4096,
            || false
        ),
        Err(Error::Source(source::Error::DigestMismatch))
    ));
    assert!(matches!(
        load_original(
            f.root.path(),
            engine.project(),
            2,
            &mut Cursor::new(&f.plate),
            4096,
            || true
        ),
        Err(Error::Source(source::Error::Cancelled))
    ));
    assert_eq!(engine.project(), &before);
    assert_eq!(
        storage::read_tracking_resource(f.root.path(), &descriptor).unwrap(),
        f.original
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &f.project);
}

#[test]
fn explicit_binding_refuses_wrong_extent_missing_source_and_duplicate_resource_ids() {
    let f = Fixture::new();
    for (id, source, bad_extent) in [
        (1, Fixture::reference(), false), // Collision with image ID.
        (
            2,
            AssetRef {
                kind: AssetKind::Image,
                id: 7,
            },
            false,
        ),
        (
            2,
            AssetRef {
                kind: AssetKind::Audio,
                id: 1,
            },
            false,
        ),
        (2, Fixture::reference(), true),
    ] {
        let mut project = f.project.clone();
        if bad_extent {
            project.assets[0].width = 9;
        }
        let proof = source::verify(&f.solve, &mut Cursor::new(&f.plate), 4096, || false).unwrap();
        assert!(Attachment::prepare(&project, id, source, &f.original, proof).is_err());
        assert!(project.tracking_assets.is_empty());
    }
    let (mut engine, descriptor) = f.attached();
    let before = engine.project().clone();
    assert!(engine
        .apply(motion_core::Command::RegisterTrackingAsset { asset: descriptor })
        .is_err());
    assert_eq!(engine.project(), &before);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &f.project);
}

#[test]
fn document_and_source_digest_mismatch_is_refused_even_when_each_is_well_formed() {
    let f = Fixture::new();
    let (engine, _) = f.attached();
    let mut project = engine.project().clone();
    project.tracking_assets[0].source_sha256 = "00".repeat(32);
    project.validate().unwrap();
    assert!(matches!(
        load_original(
            f.root.path(),
            &project,
            2,
            &mut Cursor::new(&f.plate),
            4096,
            || false
        ),
        Err(Error::Binding(
            "document and descriptor source digests differ"
        ))
    ));
    let mut other_solve = f.solve.clone();
    other_solve.source_sha256 = [0; 32];
    let other = encode::to_vec(&other_solve).unwrap();
    let proof = source::verify(&f.solve, &mut Cursor::new(&f.plate), 4096, || false).unwrap();
    assert!(matches!(
        Attachment::prepare(&f.project, 2, Fixture::reference(), &other, proof),
        Err(Error::Binding(
            "verification belongs to a different solve/source"
        ))
    ));
}

#[test]
fn current_schema_requires_attachment_list_and_rejects_invalid_descriptors() {
    let f = Fixture::new();
    let mut json = serde_json::to_value(&f.project).unwrap();
    json.as_object_mut().unwrap().remove("tracking_assets");
    assert!(serde_json::from_value::<Project>(json).is_err());
    let valid = f.prepare(2).unwrap().descriptor().clone();
    for change in 0..7 {
        let mut descriptor = valid.clone();
        match change {
            0 => descriptor.path = "../outside.mstrack".into(),
            1 => descriptor.path = "assets/solve.mstrack".into(),
            2 => descriptor.bytes = 0,
            3 => descriptor.bytes = motion_core::tracking_assets::MAX_TRACKING_ASSET_BYTES + 1,
            4 => descriptor.sha256 = "FF".repeat(32),
            5 => descriptor.source_sha256 = "0".repeat(63),
            _ => descriptor.source.kind = AssetKind::Audio,
        }
        assert!(
            descriptor.validate().is_err(),
            "accepted invalid case {change}"
        );
    }
    let mut over_limit = f.project.clone();
    over_limit.tracking_assets = (0..=motion_core::tracking_assets::MAX_TRACKING_ASSETS)
        .map(|i| TrackingAsset {
            id: i as u64 + 2,
            ..valid.clone()
        })
        .collect();
    assert!(over_limit
        .validate()
        .unwrap_err()
        .to_string()
        .contains("tracking asset limit exceeded"));
}

#[test]
fn composition_queries_and_resource_accounting_keep_project_wide_attachments() {
    let f = Fixture::new();
    let attachment = f.prepare(2).unwrap();
    let mut engine = Engine::new(f.project.clone()).unwrap();
    let before = engine.history_diagnostics();
    engine.apply(attachment.registration()).unwrap();
    let after = engine.history_diagnostics();
    assert_ne!(after, before);
    let descriptor = attachment.descriptor().clone();
    let composition = engine
        .project()
        .composition(motion_core::MAIN_COMPOSITION)
        .unwrap();
    assert_eq!(composition.tracking_assets, vec![descriptor.clone()]);
    let mut clone = engine.project().clone();
    clone
        .activate_composition(motion_core::MAIN_COMPOSITION)
        .unwrap();
    assert_eq!(clone.tracking_assets, vec![descriptor]);
    let text = serde_json::to_string(&attachment.registration()).unwrap();
    assert!(text.contains("register_tracking_asset"));
    assert_eq!(
        serde_json::from_str::<motion_core::Command>(&text)
            .unwrap()
            .changes_resources(),
        true
    );
}

#[test]
fn subsequent_image_paste_skips_tracking_resource_ids() {
    let f = Fixture::new();
    let (attached, _) = f.attached();
    let mut foreign = f.project.clone();
    foreign.assets[0].path = "assets/clipboard-other.png".into();
    let mut layer = motion_core::Layer::solid(7, "Foreign image", [8., 6.], [0.; 3], [1.; 4]);
    layer.content = motion_core::Content::Image { asset: 1 };
    foreign.layers.push(layer);
    let clipboard = motion_core::LayerClipboard::capture(&foreign, vec![7]).unwrap();
    for id in [2, u64::MAX] {
        let mut target = attached.project().clone();
        target.tracking_assets[0].id = id;
        let before = target.clone();
        let mut engine = Engine::new(target).unwrap();
        let result = engine.apply(motion_core::Command::PasteLayers {
            clipboard: Box::new(clipboard.clone()),
            at_frame: None,
            keep_external_references: false,
        });
        if id == u64::MAX {
            assert!(result.unwrap_err().to_string().contains("asset ID"));
            assert_eq!(engine.project(), &before);
            assert!(!engine.can_undo());
            continue;
        }
        result.unwrap();
        assert_eq!(engine.project().tracking_assets, before.tracking_assets);
        assert_eq!(engine.project().assets.last().unwrap().id, 3);
        assert!(matches!(
            engine.project().layers[0].content,
            motion_core::Content::Image { asset: 3 }
        ));
        assert!(engine.undo().unwrap());
        assert_eq!(engine.project(), &before);
        assert!(engine.redo().unwrap());
        assert_eq!(engine.project().assets.last().unwrap().id, 3);
    }
}

#[test]
fn video_metadata_binding_uses_original_extent_and_refuses_count_or_byte_mismatch() {
    let f = Fixture::new();
    let mut project = f.project.clone();
    project.assets.clear();
    // Opaque encoded bytes exercise verification and metadata admission only.
    // This is explicitly not a video decoder or source-PTS authenticity proof.
    project.video_assets.push(motion_core::VideoAsset {
        id: 1,
        path: "assets/opaque-video.mp4".into(),
        bytes: f.plate.len() as u64,
        mime: "video/avc".into(),
        track: 0,
        width: 8,
        height: 6,
        rotation: 90,
        display_width: 6,
        display_height: 8,
        video_start_us: 0,
        video_end_us: 100_000,
        duration_us: 100_000,
        frame_count: 3,
        variable_frame_rate: true,
        nominal_frame_rate: 30000. / 1001.,
        color_standard: 1,
        color_range: 1,
        audio_asset: None,
    });
    let mut solve = f.solve.clone();
    solve.source_pts = vec![-3003, 0, 6007];
    let original = encode::to_vec(&solve).unwrap();
    let reference = AssetRef {
        kind: AssetKind::Video,
        id: 1,
    };
    let prepare = |p: &Project, reference| {
        let proof = source::verify(&solve, &mut Cursor::new(&f.plate), 4096, || false).unwrap();
        Attachment::prepare(p, 2, reference, &original, proof)
    };
    let attachment = prepare(&project, reference).unwrap();
    assert_eq!(attachment.solve().source_pts, vec![-3003, 0, 6007]);
    assert_eq!(attachment.solve().source_extent.pixels(), [8, 6]);
    for (change, cause) in [
        (0, "managed video byte count differs"),
        (
            1,
            "tracking source timestamp count differs from managed video",
        ),
        (2, "original source extent differs"),
    ] {
        let mut changed = project.clone();
        let video = &mut changed.video_assets[0];
        match change {
            0 => video.bytes += 1,
            1 => video.frame_count += 1,
            _ => {
                video.width = 6;
                video.height = 8;
                video.display_width = 8;
                video.display_height = 6;
            }
        }
        changed.validate().unwrap();
        assert!(
            matches!(prepare(&changed, reference), Err(Error::Binding(actual)) if actual == cause)
        );
    }
    assert!(matches!(
        prepare(&f.project, Fixture::reference()),
        Err(Error::Binding("one image requires one source timestamp"))
    ));
}
