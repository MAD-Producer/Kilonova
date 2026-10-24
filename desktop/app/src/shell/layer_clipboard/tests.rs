use super::*;
use crate::{i18n::Locale, panels::Action};
use std::{
    path::Path,
    time::{Duration, Instant},
};
#[cfg(target_os = "windows")]
mod native;
mod keyframes;
mod text;
mod text_search;

fn project(root: &Path, image: bool) {
    let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
    let mut layer = motion_core::Layer::solid(7, "Original", [32.; 2], [0.; 3], [1.; 4]);
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 10,
        out_frame: 30,
        offset_frame: 10,
    });
    if image {
        std::fs::create_dir_all(root.join("assets")).unwrap();
        image::save_buffer(
            root.join("assets/image.png"),
            &vec![255; 64 * 64 * 4],
            64,
            64,
            image::ColorType::Rgba8,
        )
        .unwrap();
        project.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/image.png".into(),
            width: 64,
            height: 64,
        });
        layer.content = motion_core::Content::Image { asset: 1 };
    }
    project.layers.push(layer);
    motion_core::storage::save(root, &project).unwrap();
}
fn shell(root: &Path, locale: Locale) -> Shell {
    let mut shell = Shell::new(root.into(), None, Some(locale)).unwrap();
    shell.home = None;
    shell.panels.select_only(Some(7));
    shell
}
fn wait(shell: &mut Shell) {
    let began = Instant::now();
    while shell.layer_clipboard.busy() {
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "clipboard hung at {:?}",
            shell.layer_clipboard.phase()
        );
        shell.poll_layer_clipboard();
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn run(shell: &mut Shell, intent: Intent) {
    shell.action(Action::LayerClipboard(intent), None).unwrap();
    assert!(shell.layer_clipboard.busy());
    wait(shell);
}

fn until_phase(shell: &mut Shell, phase: &str) {
    let began = Instant::now();
    while shell.layer_clipboard.phase() != Some(phase) {
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "expected {phase}, got {:?}",
            shell.layer_clipboard.phase()
        );
        shell.poll_layer_clipboard();
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn empty_clipboard_is_a_normal_miss_that_does_not_pause_playback() {
    let folder = tempfile::tempdir().unwrap();
    project(folder.path(), false);
    let mut shell = shell(folder.path(), Locale::En);
    shell.playback = Some((Instant::now(), 0.));
    shell.panels.playing = true;
    let message = shell.panels.message.clone();
    shell
        .action(
            Action::LayerClipboard(Intent::Paste { in_place: false }),
            None,
        )
        .unwrap();
    assert!(shell.playback.is_some());
    assert!(shell.panels.playing);
    assert!(!shell.layer_clipboard.busy());
    assert_eq!(shell.panels.message, message);
}

#[test]
fn pending_surface_acknowledgement_is_drained_before_clipboard_commit() {
    let folder = tempfile::tempdir().unwrap();
    project(folder.path(), false);
    let mut shell = shell(folder.path(), Locale::En);
    run(&mut shell, Intent::Copy);
    // Use an actual engine State reply to control the snapshot acknowledgement
    // boundary. This proves scheduling, not GPU surface resizing itself.
    shell.resizing = Some(shell.engine.request(crate::engine::Command::State).unwrap());
    shell
        .action(
            Action::LayerClipboard(Intent::Paste { in_place: true }),
            None,
        )
        .unwrap();
    until_phase(&mut shell, "ready");
    shell.poll_layer_clipboard();
    assert_eq!(shell.layer_clipboard.phase(), Some("ready"));
    assert_eq!(
        shell.engine.state().unwrap()["project"]["layers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let state = shell
        .resizing
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    shell.state = state;
    until_phase(&mut shell, "committing");
    shell.node_compile_due = Some(Instant::now());
    let due = shell.node_compile_due;
    shell.poll_nodes().unwrap();
    assert_eq!(
        shell.node_compile_due, due,
        "node compile stays queued until the clipboard reply is published"
    );
    wait(&mut shell);
    assert_eq!(
        shell.state["project"]["layers"].as_array().unwrap().len(),
        2
    );
    assert_eq!(shell.panels.selection_ids(), [8]);
}

#[test]
fn pasted_multiselection_preserves_its_primary_and_original_stack_order() {
    let folder = tempfile::tempdir().unwrap();
    project(folder.path(), false);
    let mut project = motion_core::storage::load(folder.path()).unwrap();
    project.layers.push(motion_core::Layer::solid(
        3, "Second", [32.; 2], [0.; 3], [1.; 4],
    ));
    motion_core::storage::save(folder.path(), &project).unwrap();
    let mut shell = shell(folder.path(), Locale::En);
    shell.panels.select_only(Some(7));
    shell.panels.add_selection(3);
    assert_eq!(shell.panels.selection_ids(), [3, 7]);
    run(&mut shell, Intent::Copy);
    run(&mut shell, Intent::Paste { in_place: true });
    assert_eq!(shell.panels.selected, Some(9));
    assert_eq!(shell.panels.selection_ids(), [9, 8]);
    assert_eq!(
        shell.state["project"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["id"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [7, 3, 8, 9]
    );
}

#[test]
fn cancellation_releases_staged_files_but_acknowledges_an_already_committed_paste() {
    let folder = tempfile::tempdir().unwrap();
    project(folder.path(), true);
    let mut shell = shell(folder.path(), Locale::En);
    run(&mut shell, Intent::Copy);
    let before = shell.state["project"].clone();
    let asset_count = || {
        std::fs::read_dir(folder.path().join("assets"))
            .unwrap()
            .count()
    };
    assert_eq!(asset_count(), 1);
    shell.actions.push_back(Action::RefreshState);
    shell
        .action(
            Action::LayerClipboard(Intent::Paste { in_place: true }),
            None,
        )
        .unwrap();
    until_phase(&mut shell, "ready");
    assert_eq!(asset_count(), 2, "real prepare staged a second file");
    assert!(
        !shell
            .layer_clipboard
            .start(
                &shell.engine,
                Intent::Copy,
                vec![7],
                folder.path().join("unused-cache")
            )
            .unwrap(),
        "a second request cannot replace an active IO slot"
    );
    shell.actions.push_back(Action::CancelLayerClipboard);
    shell.poll_layer_clipboard();
    shell.actions.clear();
    wait(&mut shell);
    assert_eq!(shell.engine.state().unwrap()["project"], before);
    assert_eq!(asset_count(), 1);
    assert!(shell.layer_clipboard.available());
    assert!(!folder.path().join("unused-cache").exists());

    shell
        .action(
            Action::LayerClipboard(Intent::Paste { in_place: true }),
            None,
        )
        .unwrap();
    until_phase(&mut shell, "committing");
    // A real serial-worker barrier proves the transaction has committed before
    // cancellation; cancelling its UI acknowledgement cannot undo that edit.
    let committed = shell.engine.state().unwrap();
    assert_eq!(committed["project"]["layers"].as_array().unwrap().len(), 2);
    shell.actions.push_back(Action::CancelLayerClipboard);
    wait(&mut shell);
    assert_eq!(shell.state["project"], committed["project"]);
    assert_eq!(shell.panels.message.as_deref(), Some("Layers pasted"));
    assert_eq!(asset_count(), 2);
    assert_eq!(shell.engine.history(0).unwrap()["project"], before);
}

#[test]
fn shell_copy_paste_selects_fresh_layers_and_each_transaction_undoes_once() {
    for locale in [Locale::En, Locale::Zh] {
        let folder = tempfile::tempdir().unwrap();
        project(folder.path(), false);
        let mut shell = shell(folder.path(), locale);
        let before = shell.state["project"].clone();
        let revision = shell.state["revision"].clone();
        run(&mut shell, Intent::Copy);
        assert!(shell.layer_clipboard.available());
        assert_eq!(shell.state["revision"], revision, "copy is not an edit");
        shell.state = shell.engine.seek(50.).unwrap();
        run(&mut shell, Intent::Paste { in_place: false });
        let first = shell.panels.selection_ids();
        assert_eq!(first, [8]);
        assert_eq!(
            shell.state["project"]["layers"][1]["timeline"]["in_frame"],
            50
        );
        run(&mut shell, Intent::Paste { in_place: true });
        assert_eq!(shell.panels.selection_ids(), [9]);
        assert_eq!(
            shell.state["project"]["layers"][2]["timeline"]["in_frame"],
            10
        );
        shell.engine.history(0).unwrap();
        assert_eq!(
            shell.engine.state().unwrap()["project"]["layers"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        shell.engine.history(0).unwrap();
        assert_eq!(shell.engine.state().unwrap()["project"], before);
        shell.state = shell.engine.history(1).unwrap();
        shell.panels.select_only(Some(8));
        let before_cut = shell.state["project"].clone();
        run(&mut shell, Intent::Cut);
        assert_eq!(
            shell.state["project"]["layers"].as_array().unwrap().len(),
            1
        );
        assert!(shell.panels.selection_ids().is_empty());
        assert_eq!(shell.engine.history(0).unwrap()["project"], before_cut);
    }
}

#[test]
fn cancelled_or_superseded_cut_never_deletes_and_keeps_the_successful_cache() {
    let folder = tempfile::tempdir().unwrap();
    project(folder.path(), false);
    let mut shell = shell(folder.path(), Locale::En);
    let before = shell.state["project"].clone();
    shell
        .action(Action::LayerClipboard(Intent::Cut), None)
        .unwrap();
    shell.actions.push_back(Action::CancelLayerClipboard);
    wait(&mut shell);
    assert_eq!(shell.engine.state().unwrap()["project"], before);
    assert!(!shell.layer_clipboard.available());

    // Hold ordinary action dispatch to leave the actual cache worker's output
    // ready, then change the model before allowing its atomic transaction.
    shell.actions.push_back(Action::RefreshState);
    shell
        .action(Action::LayerClipboard(Intent::Cut), None)
        .unwrap();
    let began = Instant::now();
    while shell.layer_clipboard.phase() != Some("ready") {
        assert!(began.elapsed() < Duration::from_secs(20));
        shell.poll_layer_clipboard();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(shell.layer_clipboard.available());
    shell.state = shell
        .engine
        .edit(
            serde_json::to_string(&motion_core::Command::Rename {
                object: 7,
                name: "Changed during copy".into(),
            })
            .unwrap(),
        )
        .unwrap();
    let changed = shell.state["project"].clone();
    shell.actions.clear();
    wait(&mut shell);
    assert_eq!(shell.state["project"], changed);
    assert_eq!(shell.engine.state().unwrap()["project"], changed);
    assert_eq!(
        shell.panels.message.as_deref(),
        Some("Project changed; clipboard operation was not applied")
    );
    assert!(shell.layer_clipboard.available());
}

#[test]
fn failed_replacement_preserves_frozen_bytes_across_actual_project_change_and_reopen() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    project(source.path(), true);
    let empty = motion_core::Project::new(64, 64, 30, 120).unwrap();
    motion_core::storage::save(target.path(), &empty).unwrap();
    let mut shell = shell(source.path(), Locale::En);
    let bytes = std::fs::read(source.path().join("assets/image.png")).unwrap();
    run(&mut shell, Intent::Copy);
    std::fs::remove_file(source.path().join("assets/image.png")).unwrap();
    run(&mut shell, Intent::Cut);
    assert!(shell.layer_clipboard.available());
    assert_eq!(
        shell.engine.state().unwrap()["project"]["layers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(shell
        .panels
        .message
        .as_deref()
        .unwrap()
        .contains("image.png"));

    shell
        .request_project_change(crate::project_lifecycle::Target::Open(target.path().into()))
        .unwrap();
    let began = Instant::now();
    while shell.project_pending.is_some() {
        assert!(began.elapsed() < Duration::from_secs(20));
        shell.poll_project_change();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(shell.project_prompt.is_none());
    assert_eq!(
        shell.state["project"],
        serde_json::to_value(&empty).unwrap()
    );
    run(&mut shell, Intent::Paste { in_place: true });
    assert_eq!(shell.panels.selection_ids(), [1]);
    shell.engine.save().unwrap();
    let reopened = motion_core::storage::load(target.path()).unwrap();
    assert_eq!(reopened.layers.len(), 1);
    assert_eq!(
        std::fs::read(target.path().join(&reopened.assets[0].path)).unwrap(),
        bytes
    );
}

#[test]
fn actual_shell_failures_show_the_original_cause_and_use_default_error_channels() {
    const CHILD: &str = "MOTION_LAYER_CLIPBOARD_SHELL_LOG_CHILD";
    const NAME: &str = "shell::layer_clipboard::tests::actual_shell_failures_show_the_original_cause_and_use_default_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let source = root.join("project");
        project(&source, true);
        let mut shell = shell(&source, Locale::En);
        shell.panels.select_only(Some(999));
        run(&mut shell, Intent::Copy);
        assert!(shell
            .panels
            .message
            .as_deref()
            .unwrap()
            .contains("copy layer missing: object=999"));
        shell.panels.select_only(Some(7));
        run(&mut shell, Intent::Copy);
        std::fs::remove_file(source.join("assets/image.png")).unwrap();
        let before = shell.state["project"].clone();
        run(&mut shell, Intent::Cut);
        assert!(shell
            .panels
            .message
            .as_deref()
            .unwrap()
            .contains("phase=resolve"));
        assert!(shell
            .panels
            .message
            .as_deref()
            .unwrap()
            .contains("image.png"));
        assert_eq!(shell.engine.state().unwrap()["project"], before);
        assert!(shell.layer_clipboard.available());
        shell
            .action(
                Action::LayerClipboard(Intent::Paste { in_place: true }),
                None,
            )
            .unwrap();
        shell.actions.push_back(Action::CancelLayerClipboard);
        wait(&mut shell);
        assert_eq!(shell.engine.state().unwrap()["project"], before);
        crate::logging::flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LAYER_CLIPBOARD_SHELL_LOG_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(file.contains("logging initialized level=INFO"));
    for cause in [
        "copy layer missing: object=999",
        "phase=resolve",
        "image.png",
    ] {
        assert!(file.contains(cause), "missing file cause: {cause}");
        assert!(stderr.contains(cause), "missing stderr cause: {cause}");
        assert!(!stdout.contains(cause), "stdout leaked cause: {cause}");
    }
    assert!(file.contains("layer clipboard start request="));
    assert!(!stderr.contains("layer clipboard start request="));
    assert!(
        !file
            .lines()
            .filter(|line| line.contains(" ERROR "))
            .any(|line| line.contains("cancelled") || line.contains("superseded")),
        "normal cancellation is not a fault"
    );
}
