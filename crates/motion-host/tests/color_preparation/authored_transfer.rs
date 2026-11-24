//! Move a real prepared document through CPU preparation and GPU consumption owners.
use super::*;
use motion_core::{
    vector::{ShapeKind, VectorContent},
    Content, Track,
};
use std::{fs, process::Command, time::Instant};

fn consume_on_worker() {
    let reference = reference();
    let case = &reference.cases[0];
    let root = tempfile::tempdir().unwrap();
    let mut p = document(&reference, 0, AlphaMode::Straight, root.path());
    p.layers.push(Layer::solid(
        1,
        "transfer base",
        [64.; 2],
        [32., 32., 0.],
        case.input[3],
    ));
    let mut layer = Layer::solid(2, "transfer vector", [8., 48.], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Vector {
        vector: VectorContent {
            fill: Some(Track::constant(case.input[4])),
            ..VectorContent::shape(ShapeKind::Rectangle)
        },
    };
    p.layers.push(layer);
    let before = serde_json::to_value(&p).unwrap();
    let observation = Observation::new(9300);
    let prepared = prep::prepare_native_authored_project(&p, root.path(), &runtime(), &observation)
        .unwrap()
        .unwrap();
    assert!(!observation.running());
    let resource = &p.color_management.resources().unwrap()[0];
    fs::remove_file(root.path().join(&resource.path)).unwrap();
    let expected = over(
        premult(case.expected[4], AlphaMode::Straight),
        case.expected[3],
    );
    let base = case.expected[3];
    let (mut prepared, p) = std::thread::spawn(move || {
        let mut prepared = prepared;
        let mut renderer = renderer();
        let frame = prepared
            .authored
            .sample(&p, 0.25, &ExpressionEvaluator)
            .unwrap();
        let target = renderer
            .color_image_target(64, 64, frame.contract())
            .unwrap();
        let readbacks = renderer.image_readback_info();
        renderer.draw_authored_scene(frame, &target).unwrap();
        let display = endpoint(&mut renderer, &target, prepared.colors.display());
        assert_eq!(renderer.image_readback_info(), readbacks);
        let pixels = read(&renderer, &target);
        assert!(
            close(pixels[32 * 64 + 32], expected),
            "actual mixed paints after ownership transfer"
        );
        assert!(close(pixels[32 * 64 + 2], base));
        assert!(read(&renderer, &display)
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
        (prepared, p)
    })
    .join()
    .expect("prepared document consumer worker");
    assert_eq!(
        prepared
            .authored
            .sample(&p, 0.75, &ExpressionEvaluator)
            .unwrap()
            .frame(),
        0.75
    );
    let mut stale = p.clone();
    settings(&mut stale).display.alpha = AlphaMode::Premultiplied;
    assert!(matches!(
        prepared.authored.sample(&stale, 0.8, &ExpressionEvaluator),
        Err(AuthoredSceneError::Admission {
            phase: "binding",
            ..
        })
    ));
    assert_eq!(prepared.authored.current().unwrap().frame(), 0.75);
    assert_eq!(serde_json::to_value(&p).unwrap(), before);
    std::thread::spawn(move || drop(prepared))
        .join()
        .expect("prepared document release worker");
    println!("prepared authored transfer original_cpu_pixels=2 display_endpoint=executed no_configuration_io=true stale_binding=preserved complete_project=unchanged");
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn prepared_authored_document_moves_to_consumer_without_changing_session_thread_policy() {
    const CHILD: &str = "MOTION_AUTHORED_TRANSFER_CHILD";
    const TEST:&str="authored::transfer::prepared_authored_document_moves_to_consumer_without_changing_session_thread_policy";
    if std::env::var_os(CHILD).is_some() {
        consume_on_worker();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout = root.path().join("child.stdout");
    let stderr = root.path().join("child.stderr");
    let mut child = Command::new(std::env::current_exe().unwrap())
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
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("authored transfer child exceeded 60s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = fs::read_to_string(&stdout).unwrap();
    let errors = fs::read_to_string(&stderr).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = std::path::PathBuf::from(directory).join("authored-transfer");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("child.stdout"), &output).unwrap();
        fs::write(directory.join("child.stderr"), &errors).unwrap();
    }
    assert!(
        status.success(),
        "authored transfer child {status}: {output}\n{errors}"
    );
    assert!(output.contains("prepared authored transfer original_cpu_pixels=2 display_endpoint=executed no_configuration_io=true stale_binding=preserved complete_project=unchanged"),"{output}");
    println!("{output}");
}
