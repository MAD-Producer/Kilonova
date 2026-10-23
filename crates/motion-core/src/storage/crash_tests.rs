//! Interrupt the actual save path without running Rust cleanup destructors.
//! These checkpoints and all environment handling are absent from production.
use super::*;
use std::{
    cell::Cell,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    TemporaryCreated,
    BytesWritten,
    FileSynced,
    Published,
    Acknowledged,
}

const PHASES: [(Phase, &str); 5] = [
    (Phase::TemporaryCreated, "temporary_created"),
    (Phase::BytesWritten, "bytes_written"),
    (Phase::FileSynced, "file_synced"),
    (Phase::Published, "published"),
    (Phase::Acknowledged, "acknowledged"),
];
const INTERRUPTED: i32 = 86;
thread_local! {
    static CUT: Cell<Option<Phase>> = const { Cell::new(None) };
}

pub(super) fn reached(phase: Phase) {
    if CUT.get() == Some(phase) {
        // Unlike panic/unwinding, exit does not run ProjectTemporary::drop.
        // Do not invoke a recovery save or manually clean the temporary file.
        std::process::exit(INTERRUPTED);
    }
}

fn replacement() -> Project {
    let mut project = Project::new(96, 80, 24, 120).unwrap();
    project.background = [0.125, 0.25, 0.75, 1.];
    project
}

fn collect(mut child: Child, phase: &str) -> Output {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().expect("collect save child output"),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            result => {
                let kill = child.kill();
                // Preserve the wait/timeout failure even if termination fails.
                // Only wait after a successful kill; do not wait on a live child.
                let output = kill.is_ok().then(|| child.wait_with_output());
                panic!("save checkpoint {phase} not reached: wait={result:?}; kill={kill:?}; output={output:?}");
            }
        }
    }
}

#[test]
fn interrupted_save_reopens_only_complete_old_or_new_bytes_and_explicit_save_recovers() {
    const CHILD_ROOT: &str = "MOTION_STORAGE_CRASH_ROOT";
    const CHILD_PHASE: &str = "MOTION_STORAGE_CRASH_PHASE";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let requested = std::env::var(CHILD_PHASE).unwrap();
        let phase = PHASES
            .iter()
            .find(|(_, name)| *name == requested)
            .unwrap()
            .0;
        CUT.set(Some(phase));
        save(Path::new(&root), &replacement()).unwrap();
        panic!("save returned without reaching checkpoint {requested}");
    }

    let temp = tempfile::tempdir().unwrap();
    let original = Project::new(64, 64, 30, 60).unwrap();
    let updated = replacement();
    let oracle = temp.path().join("complete-new");
    save(&oracle, &updated).unwrap();
    let new_bytes = fs::read(oracle.join("project.json")).unwrap();
    assert_eq!(load(&oracle).unwrap(), updated);

    for (phase, name) in PHASES {
        let root = temp.path().join(format!("工程-{name}"));
        save(&root, &original).unwrap();
        let destination = root.join("project.json");
        let temporary = root.join("project.json.tmp");
        let old_bytes = fs::read(&destination).unwrap();
        assert_ne!(old_bytes, new_bytes);

        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::crash_tests::interrupted_save_reopens_only_complete_old_or_new_bytes_and_explicit_save_recovers",
                "--test-threads=1",
            ])
            .env(CHILD_ROOT, &root)
            .env(CHILD_PHASE, name)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let output = collect(child, name);
        assert_eq!(
            output.status.code(),
            Some(INTERRUPTED),
            "{name}: {output:?}"
        );
        assert!(output.stderr.is_empty(), "{name}: {output:?}");

        let published = matches!(phase, Phase::Published | Phase::Acknowledged);
        let expected_bytes = if published { &new_bytes } else { &old_bytes };
        let expected_project = if published { &updated } else { &original };
        assert_eq!(&fs::read(&destination).unwrap(), expected_bytes, "{name}");
        assert_eq!(&load(&root).unwrap(), expected_project, "{name}");
        if published {
            assert!(
                !temporary.exists(),
                "{name}: replacement must consume temporary"
            );
        } else {
            // The child did not unwind or silently clean up after the failure.
            let bytes = fs::read(&temporary).unwrap();
            if phase == Phase::TemporaryCreated {
                assert!(bytes.is_empty());
            } else {
                assert_eq!(bytes, new_bytes);
            }
        }

        // Explicit caller action after reopening; no automatic retry/restart.
        save(&root, &updated).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), new_bytes);
        assert_eq!(load(&root).unwrap(), updated);
        assert!(
            !temporary.exists(),
            "{name}: next explicit save consumes stale temporary"
        );
    }
}

#[test]
fn partial_temporary_is_ignored_on_load_and_fully_replaced_by_explicit_save() {
    let temp = tempfile::tempdir().unwrap();
    let original = Project::new(64, 64, 30, 60).unwrap();
    let updated = replacement();
    let oracle = temp.path().join("complete-new");
    save(&oracle, &updated).unwrap();
    let new_bytes = fs::read(oracle.join("project.json")).unwrap();
    let mut trailing = new_bytes.clone();
    trailing.extend_from_slice(b"uncommitted trailing bytes");
    let cases = [
        ("empty", Vec::new()),
        ("partial", new_bytes[..new_bytes.len() / 2].to_vec()),
        ("invalid", vec![0xff; new_bytes.len() + 256]),
        ("trailing", trailing),
    ];
    for (name, leftover) in cases {
        let root = temp.path().join(format!("残留-{name}"));
        save(&root, &original).unwrap();
        let destination = root.join("project.json");
        let temporary = root.join("project.json.tmp");
        let old_bytes = fs::read(&destination).unwrap();
        fs::write(&temporary, leftover).unwrap();
        assert_eq!(load(&root).unwrap(), original, "{name}");
        assert_eq!(fs::read(&destination).unwrap(), old_bytes, "{name}");

        save(&root, &updated).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), new_bytes, "{name}");
        assert_eq!(load(&root).unwrap(), updated, "{name}");
        assert!(!temporary.exists(), "{name}: temporary must be consumed");
    }
}
