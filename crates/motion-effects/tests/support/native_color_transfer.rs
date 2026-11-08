//! Exclusive ownership qualification, isolated from a potentially stuck native call.
use super::*;
use motion_effects::native_color::NativeColorProgram;
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};

fn pixels(program: &NativeColorProgram, case: &Case, reference: &Reference) {
    for (sample, (input, expected)) in case.input.iter().zip(&case.expected).enumerate() {
        let alpha = input[3];
        let mut rgb = [input[0], input[1], input[2]];
        if case.processor.input_alpha == AlphaMode::Premultiplied {
            rgb = if alpha > 0. {
                rgb.map(|value| value / alpha)
            } else {
                [0.; 3]
            };
        }
        let mut actual = program.apply_rgb(rgb).unwrap();
        if case.processor.output_alpha == AlphaMode::Premultiplied {
            actual = actual.map(|value| value * alpha);
        }
        for channel in 0..3 {
            let tolerance = reference.absolute_tolerance
                + reference.relative_tolerance * expected[channel].abs();
            assert!((actual[channel]-expected[channel]).abs() <= tolerance,
                "transfer sample={sample} channel={channel} actual={actual:?} expected={expected:?} tolerance={tolerance}");
        }
        assert_eq!(alpha, expected[3]);
    }
}

fn transfer_and_release() {
    fn require_send<T: Send>() {}
    require_send::<NativeColorProgram>();
    let runtime = runtime();
    let oracle = reference();
    let mut programs = Vec::new();
    for case in &oracle.cases {
        let item = &oracle.configs[case.config];
        let program = runtime
            .prepare(
                &config(item),
                &item.selection,
                case.processor.phase,
                case.processor.input_alpha,
                case.processor.output_alpha,
            )
            .unwrap();
        pixels(&program, case, &oracle);
        programs.push(program);
    }
    drop(runtime);
    let snapshots: Vec<_> = programs
        .iter()
        .map(|program| serde_json::to_value(program.source()).unwrap())
        .collect();
    // Move the complete programs, never an address cast or a shared reference.
    let worker = std::thread::spawn(move || {
        let reference = reference();
        for (index, program) in programs.iter().enumerate() {
            assert_eq!(
                serde_json::to_value(program.source()).unwrap(),
                snapshots[index]
            );
            pixels(program, &reference.cases[index], &reference);
            assert!(matches!(
                program.apply_rgb([f32::NAN, 0., 0.]),
                Err(Error::Admission("nonfinite authored RGB input"))
            ));
            pixels(program, &reference.cases[index], &reference);
        }
        programs
    });
    let programs = worker.join().expect("original native worker panic");
    for (program, case) in programs.iter().zip(&oracle.cases) {
        pixels(program, case, &oracle);
    }
    let count = programs.len();
    // Release on a third thread after the creator and processing owner let go.
    std::thread::spawn(move || drop(programs))
        .join()
        .expect("native release worker");
    assert_eq!(count, 20);
    println!("exclusive native transfer cases={count} CPU reference pixels_per_pass=240 passes=4 foreign_thread_release=complete");
}

#[test]
#[ignore = "requires explicit checksum-verified native OCIO prefix"]
fn exclusive_native_programs_move_between_workers_and_release_after_runtime_drop() {
    const CHILD: &str = "MOTION_OCIO_TRANSFER_CHILD";
    const TEST: &str =
        "transfer::exclusive_native_programs_move_between_workers_and_release_after_runtime_drop";
    if std::env::var_os(CHILD).is_some() {
        transfer_and_release();
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
        if start.elapsed() > Duration::from_secs(20) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("native transfer exceeded 20s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = fs::read_to_string(&stdout).unwrap();
    let errors = fs::read_to_string(&stderr).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("native-transfer");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("child.stdout"), &output).unwrap();
        fs::write(directory.join("child.stderr"), &errors).unwrap();
    }
    assert!(
        status.success(),
        "native transfer child {status}: {output}\n{errors}"
    );
    assert!(
        output.contains(
            "cases=20 CPU reference pixels_per_pass=240 passes=4 foreign_thread_release=complete"
        ),
        "{output}"
    );
    println!("{output}");
}
