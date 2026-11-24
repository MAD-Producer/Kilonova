//! Actual ordinary Session lifecycle, registry and managed drawing qualification.
use super::*;
use motion_core::{
    vector::{ShapeKind, VectorContent},
    Command, Content, Track,
};
use motion_host::{Platform, SurfaceTarget, VideoDecoder, VideoQuery};
use std::{
    fs, path::PathBuf, process::Command as Process, sync::atomic::AtomicBool, time::Instant,
};

pub(super) struct NativePlatform(pub(super) PathBuf);
impl Platform for NativePlatform {
    fn name(&self) -> &'static str {
        "explicit-ocio-session-test"
    }
    fn color_runtime_manifest(&self) -> motion_host::Result<Option<PathBuf>> {
        Ok(Some(self.0.clone()))
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("static color session must not probe video")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        panic!("static color session must not decode video")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        panic!("static color session must not decode audio")
    }
    fn media_capabilities(
        &self,
        _: Option<&VideoQuery>,
    ) -> motion_media::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _: motion_render::AllocationBudget,
    ) -> motion_host::Result<SurfaceTarget> {
        panic!("headless color qualification must not create a surface")
    }
}

fn consumer(error: &motion_render::RenderError) -> bool {
    matches!(
        error,
        motion_render::RenderError::AuthoredScene(AuthoredSceneError::Admission {
            phase: "consumer",
            ..
        })
    )
}

fn qualify() {
    let reference = reference();
    let case = &reference.cases[0];
    let library = tempfile::tempdir().unwrap();
    let root = library.path().join("project");
    fs::create_dir(&root).unwrap();
    let mut p = document(&reference, 0, AlphaMode::Straight, &root);
    p.layers.push(Layer::solid(
        1,
        "managed base",
        [64.; 2],
        [32., 32., 0.],
        case.input[3],
    ));
    let mut vector = Layer::solid(2, "managed vector", [8., 48.], [32., 32., 0.], [1.; 4]);
    vector.content = Content::Vector {
        vector: VectorContent {
            fill: Some(Track::constant(case.input[4])),
            ..VectorContent::shape(ShapeKind::Rectangle)
        },
    };
    p.layers.push(vector);
    let original = serde_json::to_value(&p).unwrap();
    let id =
        motion_host::session::open(root.clone(), p.clone(), Arc::new(NativePlatform(runtime())))
            .unwrap();
    let wrong =
        std::thread::spawn(move || motion_host::session::with_session(id, |_| Ok(())).unwrap_err())
            .join()
            .unwrap();
    assert_eq!(
        wrong.to_string(),
        "native session must run on its owning worker thread"
    );
    let (mut renderer, target, images, pixels) = motion_host::session::with_session(id, |s| {
        assert_eq!(serde_json::to_value(s.engine.project()).unwrap(), original);
        assert!(s.color_programs().is_some());
        let contract = s.scene.color_contract().unwrap();
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            ChannelPrecision::Float32,
            s.allocation_budget().clone(),
        ))
        .unwrap();
        let observation = Observation::new(9400);
        let images = s
            .prepare_color_sources(&mut renderer, &AtomicBool::new(false), &observation)
            .unwrap();
        assert!(!observation.running());
        let resource = p.color_management.resources().unwrap()[0].path.clone();
        fs::remove_file(root.join(&resource)).unwrap();
        let target = renderer.color_image_target(64, 64, contract).unwrap();
        s.frame = 0.25;
        let reads = renderer.image_readback_info();
        s.draw_color_frame(&mut renderer, &target, &images).unwrap();
        let display = endpoint(
            &mut renderer,
            &target,
            s.color_programs().unwrap().display(),
        );
        assert_eq!(renderer.image_readback_info(), reads);
        let before = read(&renderer, &target);
        assert!(close(before[32 * 64 + 2], case.expected[3]));
        assert!(close(
            before[32 * 64 + 32],
            over(
                premult(case.expected[4], AlphaMode::Straight),
                case.expected[3]
            )
        ));
        assert!(read(&renderer, &display)
            .iter()
            .flatten()
            .all(|v| v.is_finite()));
        assert_eq!(s.scene.frame, 0.25);
        let context = s.scene.clone();
        assert_eq!(context.color_contract(), Some(contract));
        assert!(consumer(
            &renderer
                .draw(&context, target.image().view(), 64, 64)
                .unwrap_err()
        ));
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        assert!(consumer(
            &renderer
                .encode(&context, target.image().view(), 64, 64, &mut encoder, None)
                .unwrap_err()
        ));
        assert!(consumer(
            &renderer
                .encode_preview(&context, target.image().view(), 64, 64, &mut encoder, None)
                .unwrap_err()
        ));
        // Submit the rejected encoding's empty encoder; it must contain no writes.
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        let mut unmanaged = p.clone();
        unmanaged.color_management = Arc::new(ProjectColorManagement::Unmanaged {});
        let mut legacy = motion_render::Scene::new(&unmanaged).unwrap();
        legacy
            .sample(&unmanaged, 0.25, None, &ExpressionEvaluator)
            .unwrap();
        s.effects.build_preview(&legacy, &[0], 64, 64).unwrap();
        assert!(consumer(
            &s.effects
                .build_preview(&context, &[0], 64, 64)
                .err()
                .unwrap()
        ));
        assert!(consumer(
            &s.effects.build(&context, &[0], 64, 64, true).err().unwrap()
        ));
        assert_eq!(read(&renderer, &target), before);

        // An ordinary edit reuses the installed program; the config is absent.
        s.apply_command_batch(vec![Command::SetColor {
            object: 1,
            value: case.input[4],
        }])
        .unwrap();
        s.frame = 0.5;
        s.draw_color_frame(&mut renderer, &target, &images).unwrap();
        let pixels = read(&renderer, &target);
        let green = premult(case.expected[4], AlphaMode::Straight);
        assert!(close(pixels[32 * 64 + 2], green));
        assert!(close(pixels[32 * 64 + 32], over(green, green)));
        let before_project = serde_json::to_value(s.engine.project()).unwrap();
        let history = s.engine.history_diagnostics();
        let generation = s.document_generation;
        let sampled = s.scene.frame;
        let mut changed = p.clone();
        settings(&mut changed).output.alpha = AlphaMode::Premultiplied;
        let error = s
            .apply_command_batch(vec![Command::SetColorManagement {
                settings: changed.color_management.clone(),
            }])
            .unwrap_err();
        assert!(
            matches!(
                error,
                motion_host::Error::ColorPreparation(prep::Error::Resources(_))
            ),
            "{error:?}"
        );
        assert_eq!(
            serde_json::to_value(s.engine.project()).unwrap(),
            before_project
        );
        assert_eq!(s.engine.history_diagnostics(), history);
        assert_eq!(s.document_generation, generation);
        assert_eq!(s.scene.frame, sampled);
        let error = s
            .replace_project(
                motion_core::Engine::new(p.clone()).unwrap(),
                root.join("absent"),
            )
            .unwrap_err();
        assert!(
            matches!(
                error,
                motion_host::Error::ColorPreparation(prep::Error::Resources(_))
            ),
            "{error:?}"
        );
        assert_eq!(
            serde_json::to_value(s.engine.project()).unwrap(),
            before_project
        );
        assert_eq!(s.engine.history_diagnostics(), history);
        assert_eq!(s.document_generation, generation);
        s.draw_color_frame(&mut renderer, &target, &images).unwrap();
        assert_eq!(read(&renderer, &target), pixels);

        // Explicitly clearing management clears the sampled identity as well.
        s.apply_command_batch(vec![Command::SetColorManagement {
            settings: unmanaged.color_management.clone(),
        }])
        .unwrap();
        assert!(s.color_programs().is_none());
        assert_eq!(s.scene.color_contract(), None);
        let history = s.engine.history_diagnostics();
        let before_project = serde_json::to_value(s.engine.project()).unwrap();
        assert!(s
            .apply_command_batch(vec![Command::SetColorManagement {
                settings: p.color_management.clone()
            }])
            .is_err());
        assert_eq!(
            serde_json::to_value(s.engine.project()).unwrap(),
            before_project
        );
        assert_eq!(s.engine.history_diagnostics(), history);
        assert_eq!(s.scene.color_contract(), None);
        fs::write(root.join(&resource), &reference.configs[0].config).unwrap();
        s.apply_command_batch(vec![Command::SetColorManagement {
            settings: p.color_management.clone(),
        }])
        .unwrap();
        s.draw_color_frame(&mut renderer, &target, &images).unwrap();
        assert_eq!(read(&renderer, &target), pixels);
        Ok((renderer, target, images, pixels))
    })
    .unwrap();
    use motion_host::ops::editing::{
        history, HISTORY_BEGIN, HISTORY_CANCEL, HISTORY_REDO, HISTORY_UNDO,
    };
    history(id, HISTORY_UNDO).unwrap();
    let history_before = motion_host::session::with_session(id, |s| {
        assert!(s.color_programs().is_none());
        assert_eq!(s.scene.color_contract(), None);
        Ok((
            s.engine.snapshot(),
            s.engine.history_diagnostics(),
            s.engine.revision(),
            s.document_generation,
        ))
    })
    .unwrap();
    let resource = p.color_management.resources().unwrap()[0].path.clone();
    fs::remove_file(root.join(&resource)).unwrap();
    assert!(matches!(
        history(id, HISTORY_REDO).unwrap_err(),
        motion_host::Error::ColorPreparation(prep::Error::Resources(_))
    ));
    motion_host::session::with_session(id, |s| {
        assert_eq!(
            (
                s.engine.snapshot(),
                s.engine.history_diagnostics(),
                s.engine.revision(),
                s.document_generation
            ),
            history_before
        );
        assert!(s.color_programs().is_none());
        Ok(())
    })
    .unwrap();
    fs::write(root.join(&resource), &reference.configs[0].config).unwrap();
    history(id, HISTORY_REDO).unwrap();
    history(id, HISTORY_BEGIN).unwrap();
    motion_host::session::with_session(id, |s| {
        let before = s.engine.snapshot();
        let error = s
            .apply_command_batch(vec![Command::SetColorManagement {
                settings: Arc::new(ProjectColorManagement::Unmanaged {}),
            }])
            .unwrap_err();
        // Changing the document color contract during a gesture is already
        // prohibited by the model. Preserve that protocol and its original Err.
        assert!(matches!(
            error,
            motion_host::Error::Model(motion_core::Error::Gesture)
        ));
        assert_eq!(s.engine.snapshot(), before);
        assert!(s.color_programs().is_some());
        s.apply_command_batch(vec![Command::SetColor {
            object: 1,
            value: case.input[3],
        }])
        .unwrap();
        Ok(())
    })
    .unwrap();
    fs::remove_file(root.join(&resource)).unwrap();
    // Reverting an ordinary gesture uses the retained binding, with no file IO.
    history(id, HISTORY_CANCEL).unwrap();
    motion_host::session::with_session(id, |s| {
        assert!(!s.engine.gesture_active());
        assert!(s.color_programs().is_some());
        s.draw_color_frame(&mut renderer, &target, &images).unwrap();
        assert_eq!(read(&renderer, &target), pixels);
        let blank = Project::new(64, 64, 30, 120).unwrap();
        s.replace_project(
            motion_core::Engine::new(blank.clone()).unwrap(),
            root.clone(),
        )
        .unwrap();
        assert_eq!(s.engine.project(), &blank);
        assert!(s.color_programs().is_none());
        assert_eq!(s.scene.color_contract(), None);
        assert_ne!(s.document_generation, history_before.3);
        Ok(())
    })
    .unwrap();
    motion_host::session::close(id);
    assert_eq!(
        motion_host::session::with_session(id, |_| Ok(()))
            .unwrap_err()
            .to_string(),
        "native session is closed"
    );
    println!("ordinary managed Session original_cpu_pixels=4 legacy_consumers=5 failed_edit_and_replace=atomic no_config_io_on_sample=true registry_owner=preserved explicit_recovery=passed");
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn ordinary_session_installs_samples_edits_and_replaces_managed_documents_transactionally() {
    const CHILD: &str = "MOTION_MANAGED_SESSION_CHILD";
    const TEST:&str="authored::session_installation::ordinary_session_installs_samples_edits_and_replaces_managed_documents_transactionally";
    if std::env::var_os(CHILD).is_some() {
        qualify();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout = root.path().join("child.stdout");
    let stderr = root.path().join("child.stderr");
    let mut child = Process::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(90) {
            let killed = child.kill();
            let waited = child.wait();
            panic!("managed Session qualification exceeded 90s: kill={killed:?} wait={waited:?}\n{}\n{}",fs::read_to_string(&stdout).unwrap(),fs::read_to_string(&stderr).unwrap());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = fs::read_to_string(&stdout).unwrap();
    let errors = fs::read_to_string(&stderr).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("managed-session");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("child.stdout"), &output).unwrap();
        fs::write(directory.join("child.stderr"), &errors).unwrap();
    }
    assert!(status.success(), "{status}\n{output}\n{errors}");
    assert!(output.contains("ordinary managed Session original_cpu_pixels=4 legacy_consumers=5 failed_edit_and_replace=atomic no_config_io_on_sample=true registry_owner=preserved explicit_recovery=passed"),"{output}");
    println!("{output}");
}
