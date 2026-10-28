use glam::Vec3;
use motion_core::{Command, Project};
use motion_host::{session, Platform, Session, SurfaceTarget, VideoDecoder, VideoQuery};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

#[path = "../../motion-render/tests/support/imported_camera.rs"]
mod fixture;

struct CpuOnly;
impl Platform for CpuOnly {
    fn name(&self) -> &'static str {
        "imported-camera-contract"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("no video")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        panic!("no decoder")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        panic!("no audio")
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> motion_media::Result<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _: motion_render::AllocationBudget,
    ) -> motion_host::Result<SurfaceTarget> {
        panic!("no window surface")
    }
}

#[test]
fn source_camera_parented_layer_drag_uses_live_rig_and_undo_without_source_reads() {
    let root = tempfile::tempdir().unwrap();
    let mut project = fixture::project(root.path(), true);
    let mut rig =
        motion_core::Layer::solid(3, "Animated camera rig", [1.; 2], [48., 32., 0.], [0.; 4]);
    rig.content = motion_core::Content::Null;
    rig.three_d = true;
    rig.transform.rotation = motion_core::Track::constant([0., 0., 90.]);
    project.camera.parent = Some(motion_core::ParentLink {
        object: Some(3),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    let mut child = motion_core::Layer::solid(
        2,
        "Camera child",
        [5.; 2],
        [48., 32., 60.],
        [0., 1., 0., 1.],
    );
    child.three_d = true;
    child.parent = Some(motion_core::ParentLink {
        object: Some(0),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    project.layers.extend([child, rig]);
    let id = session::open(root.path().into(), project.clone(), Arc::new(CpuOnly)).unwrap();
    let before = session::with_session(id, |session| {
        session.frame = 30.;
        session.sample()?;
        std::fs::remove_file(root.path().join(&project.model_assets[0].path))?;
        Ok(session.scene.project_node(2).unwrap())
    })
    .unwrap();
    motion_host::ops::editing::drag(id, 2, 6., -3., 96, 64).unwrap();
    session::with_session(id, |session| {
        let after = session.scene.project_node(2).unwrap();
        assert!(
            (after[0] - before[0] - 6.).abs() < 1e-5,
            "before={before:?} after={after:?}"
        );
        assert!(
            (after[1] - before[1] + 3.).abs() < 1e-5,
            "before={before:?} after={after:?}"
        );
        assert!(session.last_error.is_none());
        Ok(())
    })
    .unwrap();
    motion_host::ops::editing::history(id, motion_host::ops::editing::HISTORY_UNDO).unwrap();
    session::with_session(id, |session| {
        assert_eq!(session.engine.project(), &project);
        assert_eq!(session.scene.project_node(2).unwrap(), before);
        Ok(())
    })
    .unwrap();
    session::with_session(id, |session| {
        let world = session.scene.world_matrix(2).unwrap();
        session.apply_command_batch(vec![Command::Parent {
            object: 2,
            parent: None,
            frame: 30,
        }])?;
        assert!(world.abs_diff_eq(session.scene.world_matrix(2).unwrap(), 1e-6));
        Ok(())
    })
    .unwrap();
    motion_host::ops::editing::history(id, motion_host::ops::editing::HISTORY_UNDO).unwrap();
    session::with_session(id, |session| {
        let revision = session.engine.revision();
        let mut invalid = project.camera.imported.clone().unwrap();
        invalid.node = 99;
        let error = session
            .apply_command_batch(vec![
                Command::CameraImported {
                    object: 0,
                    binding: Some(invalid),
                },
                Command::Parent {
                    object: 2,
                    parent: None,
                    frame: 30,
                },
            ])
            .unwrap_err();
        assert!(error.to_string().contains("source=7") && error.to_string().contains("node=99"));
        assert_eq!(session.engine.project(), &project);
        assert_eq!(session.engine.revision(), revision);
        assert_eq!(session.scene.project_node(2).unwrap(), before);
        Ok(())
    })
    .unwrap();
    session::close(id);
}

#[test]
fn source_camera_parented_2d_layers_align_and_distribute_with_exact_undo() {
    use motion_host::ops::alignment::{align, distribute, Distribution, Edge, Target};
    let root = tempfile::tempdir().unwrap();
    let mut project = fixture::project(root.path(), false);
    let mut rig = motion_core::Layer::solid(5, "Camera rig", [1.; 2], [48., 32., 0.], [0.; 4]);
    rig.content = motion_core::Content::Null;
    rig.three_d = true;
    rig.transform.rotation = motion_core::Track::constant([0., 0., 90.]);
    project.camera.parent = Some(motion_core::ParentLink {
        object: Some(5),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    for (id, position) in [
        (2, [25., 18., 0.]),
        (3, [44., 26., 0.]),
        (4, [63., 42., 0.]),
    ] {
        let mut child =
            motion_core::Layer::solid(id, "Flat camera child", [8., 6.], position, [1.; 4]);
        child.parent = Some(motion_core::ParentLink {
            object: Some(0),
            bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
        });
        project.layers.push(child);
    }
    project.layers.push(rig);
    let id = session::open(root.path().into(), project.clone(), Arc::new(CpuOnly)).unwrap();
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    let rectangles = |session: &Session| -> Vec<[[f32; 2]; 2]> {
        (2..=4)
            .map(|object| {
                let layer = session
                    .scene
                    .layers
                    .iter()
                    .find(|layer| layer.id == object)
                    .unwrap();
                let polygon = motion_render::selection_geometry::polygon(
                    layer,
                    &session.scene,
                    &session.effects.registry,
                )
                .unwrap();
                [
                    std::array::from_fn(|axis| {
                        polygon
                            .iter()
                            .map(|point| point[axis])
                            .fold(f32::INFINITY, f32::min)
                    }),
                    std::array::from_fn(|axis| {
                        polygon
                            .iter()
                            .map(|point| point[axis])
                            .fold(f32::NEG_INFINITY, f32::max)
                    }),
                ]
            })
            .collect()
    };
    let before = session::with_session(id, |session| {
        session.frame = 30.;
        session.sample()?;
        Ok(rectangles(session))
    })
    .unwrap();
    align(id, vec![2, 3, 4], Edge::Left, Target::Selection).unwrap();
    session::with_session(id, |session| {
        let after = rectangles(session);
        let destination = before.iter().map(|bounds| bounds[0][0]).fold(f32::INFINITY, f32::min);
        for index in 0..3 {
            assert!((after[index][0][0] - destination).abs() < 1e-5,
                "index={index} before={before:?} after={after:?}");
            assert!((after[index][0][1] - before[index][0][1]).abs() < 1e-5);
        }
        assert!(session.last_error.is_none());
        Ok(())
    })
    .unwrap();
    motion_host::ops::editing::history(id, motion_host::ops::editing::HISTORY_UNDO).unwrap();
    distribute(id, vec![2, 3, 4], Distribution::HorizontalCenters).unwrap();
    session::with_session(id, |session| {
        let after = rectangles(session);
        let center = |index: usize| (after[index][0][0] + after[index][1][0]) * 0.5;
        assert!((center(1) - (center(0) + center(2)) * 0.5).abs() < 1e-5);
        assert_eq!(after[0], before[0]);
        assert_eq!(after[2], before[2]);
        Ok(())
    })
    .unwrap();
    motion_host::ops::editing::history(id, motion_host::ops::editing::HISTORY_UNDO).unwrap();
    session::with_session(id, |session| {
        assert_eq!(session.engine.project(), &project);
        assert_eq!(rectangles(session), before);
        Ok(())
    })
    .unwrap();
    session::close(id);
}

#[test]
fn imported_camera_cpu_session_selection_failure_is_atomic_and_recovery_uses_owned_sources() {
    let root = tempfile::tempdir().unwrap();
    let project = fixture::project(root.path(), false);
    motion_core::storage::save(root.path(), &project).unwrap();
    let reopened = motion_core::storage::load(root.path()).unwrap();
    let mut session = Session::new(reopened, root.path().into(), Arc::new(CpuOnly)).unwrap();
    assert!(session.graphics.is_none());
    assert_eq!(session.scene.camera.eye, Vec3::new(0., 0., 60.));
    assert!(session.last_error.is_none());
    let catalog = session.scene.model_source_catalog();
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    let binding = project.camera.imported.as_ref().unwrap();
    let mut changed = binding.clone();
    changed.animation.as_mut().unwrap().offset_seconds = 1.;
    session
        .apply_command_batch(vec![Command::CameraImported {
            object: 0,
            binding: Some(changed.clone()),
        }])
        .unwrap();
    assert_eq!(session.scene.camera.eye, Vec3::new(10., 0., 60.));
    let before = session.engine.project().clone();
    let revision = session.engine.revision();
    let history = session.engine.history_diagnostics();
    changed.node = 0;
    let error = session
        .apply_command_batch(vec![
            Command::Rename {
                object: 1,
                name: "Must roll back".into(),
            },
            Command::CameraImported {
                object: 0,
                binding: Some(changed),
            },
        ])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("phase=selection") && error.contains("source=7") && error.contains("node=0"),
        "{error}"
    );
    assert_eq!(session.engine.project(), &before);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.engine.history_diagnostics(), history);
    assert_eq!(session.scene.camera.eye, Vec3::new(10., 0., 60.));
    assert!(catalog.shares_sources_with(&session.scene.model_source_catalog()));
    session.engine.undo().unwrap();
    session.sample().unwrap();
    assert_eq!(session.scene.camera.eye, Vec3::new(0., 0., 60.));
    session.engine.redo().unwrap();
    session.sample().unwrap();
    assert_eq!(session.scene.camera.eye, Vec3::new(10., 0., 60.));
    session.frame = 30.;
    session.sample().unwrap();
    assert_eq!(session.scene.camera.eye, Vec3::new(20., 0., 60.));
    // A root replacement cannot borrow identical metadata from the previous root.
    let absent = tempfile::tempdir().unwrap();
    let error = session
        .replace_project(
            motion_core::Engine::new(Project::clone(&before)).unwrap(),
            absent.path().into(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("source=7"), "{error}");
    assert!(
        matches!(&error, motion_host::Error::Render(motion_render::RenderError::Context { cause, .. })
        if matches!(cause.as_ref(), motion_render::RenderError::Io(_))),
        "IO cause must retain its native type: {error:?}"
    );
    assert_eq!(session.engine.project(), &before);
    assert_eq!(session.root, root.path());
    assert_eq!(session.scene.camera.eye, Vec3::new(20., 0., 60.));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_camera_host_capture_matches_native_pixels_after_sources_are_removed() {
    let root = tempfile::tempdir().unwrap();
    let project = fixture::project(root.path(), true);
    let id = session::open(root.path().into(), project, Arc::new(CpuOnly)).unwrap();
    let (expected, mut binding) = session::with_session(id, |session| {
        session.frame = 30.;
        session.sample()?;
        assert_eq!(session.scene.camera.eye, Vec3::new(10., 0., 60.));
        assert_eq!(session.scene.camera_viewport, [16., 0., 64., 64.]);
        let catalog = session.scene.model_source_catalog();
        std::fs::remove_file(
            session
                .root
                .join(&session.engine.project().model_assets[0].path),
        )?;
        let mut renderer = pollster::block_on(motion_render::Renderer::headless())?;
        renderer.configure_assets_with_model_catalog(
            session.engine.project(),
            &session.root,
            &catalog,
        )?;
        let target =
            renderer.sdr_capture_target(96, 64, motion_render::OutputTransform::SDR_CLIP)?;
        renderer.retain_video_instances(&session.scene);
        Ok((
            renderer.capture_sdr(&session.scene, &target)?.0.pixels,
            session.engine.project().camera.imported.clone().unwrap(),
        ))
    })
    .unwrap();
    binding.animation.as_mut().unwrap().offset_seconds = 0.25;
    motion_host::ops::editing::command(
        id,
        &json!({"op":"camera_imported","binding":binding}).to_string(),
    )
    .unwrap();
    for (operation, x) in [
        (motion_host::ops::editing::HISTORY_UNDO, 10.),
        (motion_host::ops::editing::HISTORY_REDO, 12.5),
        (motion_host::ops::editing::HISTORY_UNDO, 10.),
    ] {
        motion_host::ops::editing::history(id, operation).unwrap();
        session::with_session(id, |session| {
            assert_eq!(session.scene.camera.eye.x, x);
            assert!(session.last_error.is_none());
            Ok(())
        })
        .unwrap();
    }
    let actual = motion_host::ops::export::capture(id).unwrap();
    let pixels = image::open(actual["path"].as_str().unwrap())
        .unwrap()
        .to_rgba8();
    assert_eq!(pixels.as_raw(), &expected);
    assert!(
        expected
            .chunks_exact(4)
            .filter(|pixel| pixel[0] == 255 && pixel[3] == 255)
            .count()
            > 100
    );
    session::close(id);
}
