use super::*;
use motion_desktop_media::movie::{ExportControl, FrozenMovie};

fn prepare(root: &Path, store: &Store) -> PreparedMovie {
    FrozenMovie::new(
        motion_core::Project::new(64, 32, 30, 6).unwrap(),
        root,
        motion_effects::Registry::new_with_builtins().unwrap(),
        motion_render::resource_policy::MAX_SCRATCH_BUDGET,
        7,
    )
    .unwrap()
    .prepare_persistent(&store.snapshots(), &ExportControl::default())
    .unwrap()
}
fn enqueue(root: &Path, store: &mut Store, name: &str) -> u64 {
    let prepared = prepare(root, store);
    store
        .add(
            prepared,
            store.document.templates[0].clone(),
            root.join(name),
        )
        .unwrap()
}

#[test]
fn render_queue_persists_frozen_template_and_interrupted_state_without_automatic_retry() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path()).unwrap();
    let a = enqueue(root.path(), &mut store, "one.mp4");
    let b = enqueue(root.path(), &mut store, "two.mp4");
    let authored = store.document.jobs[0].clone();
    let mut template = store.document.templates[0].clone();
    template.name = "Custom template".into();
    template.pattern = "custom-{id}.mp4".into();
    store.save_template(template).unwrap();
    store.remove_template(1).unwrap();
    assert_eq!(store.document.jobs[0], authored);
    assert_eq!(store.begin_next().unwrap().unwrap().id, a);
    assert!(store.begin_next().is_err());
    drop(store);
    let mut reopened = Store::open(root.path()).unwrap();
    assert_eq!(reopened.document.jobs[0].state, State::Interrupted);
    assert_eq!(reopened.document.jobs[0].source, authored.source);
    assert_eq!(reopened.document.jobs[0].template, authored.template);
    assert_eq!(reopened.document.jobs[0].attempt, 1);
    assert_eq!(reopened.document.jobs[1].state, State::Queued);
    // Opening does not start either task. Retry is an explicit transition.
    reopened.retry(a, None).unwrap();
    assert_eq!(reopened.begin_next().unwrap().unwrap().id, a);
    reopened
        .finish(
            a,
            State::Failed {
                cause: "controlled export failure".into(),
            },
        )
        .unwrap();
    assert_eq!(reopened.begin_next().unwrap().unwrap().id, b);
    assert!(reopened.cancel(b).unwrap());
    reopened.finish(b, State::Cancelled).unwrap();
    assert!(!reopened.cancel(b).unwrap());
    assert!(reopened.begin_next().unwrap().is_none());
}

#[test]
fn render_queue_refuses_existing_outputs_and_preserves_inputs_on_failed_journal_publication() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path()).unwrap();
    let output = root.path().join("existing.mp4");
    fs::write(&output, b"existing output").unwrap();
    let prepared = prepare(root.path(), &store);
    let input = prepared.path().to_owned();
    assert!(store
        .add(
            prepared,
            store.document.templates[0].clone(),
            output.clone()
        )
        .is_err());
    assert!(!input.exists());
    assert_eq!(fs::read(&output).unwrap(), b"existing output");
    assert!(store.document.jobs.is_empty());
    let id = enqueue(root.path(), &mut store, "new.mp4");
    let input = store.snapshot_path(&store.document.jobs[0]).unwrap();
    let before = store.document.clone();
    let journal = store.directory.join("queue.json");
    let backup = store.directory.join("queue.keep.json");
    fs::rename(&journal, &backup).unwrap();
    fs::create_dir(&journal).unwrap();
    assert!(store.begin_next().is_err());
    assert_eq!(store.document, before);
    assert!(input.is_dir());
    let prepared = prepare(root.path(), &store);
    let discarded = prepared.path().to_owned();
    assert!(store
        .add(
            prepared,
            store.document.templates[1].clone(),
            root.path().join("another.mp4")
        )
        .is_err());
    assert!(!discarded.exists());
    assert_eq!(store.document, before);
    fs::remove_dir(&journal).unwrap();
    fs::rename(backup, &journal).unwrap();
    assert_eq!(store.begin_next().unwrap().unwrap().id, id);
    store
        .finish(
            id,
            State::Failed {
                cause: "injected failure".into(),
            },
        )
        .unwrap();
    fs::write(root.path().join("new.mp4"), b"foreign completed output").unwrap();
    assert!(store.retry(id, None).is_err());
    assert!(matches!(store.document.jobs[0].state, State::Failed { .. }));
    store
        .retry(id, Some(root.path().join("retry.mp4")))
        .unwrap();
    assert_eq!(
        fs::read(root.path().join("new.mp4")).unwrap(),
        b"foreign completed output"
    );
}

#[test]
fn render_queue_removal_keeps_outputs_and_can_resume_after_input_or_final_save_disappears() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path()).unwrap();
    let id = enqueue(root.path(), &mut store, "done.mp4");
    let input = store.snapshot_path(&store.document.jobs[0]).unwrap();
    fs::write(root.path().join("done.mp4"), b"keep published movie").unwrap();
    // A process can exit after input deletion, before the final document save.
    store
        .change(|document| {
            job_mut(document, id)?.state = State::Removing;
            Ok(())
        })
        .unwrap();
    fs::remove_dir_all(&input).unwrap();
    drop(store);
    let mut reopened = Store::open(root.path()).unwrap();
    assert_eq!(reopened.document.jobs[0].state, State::Removing);
    reopened.remove(id).unwrap();
    assert!(reopened.document.jobs.is_empty());
    assert_eq!(
        fs::read(root.path().join("done.mp4")).unwrap(),
        b"keep published movie"
    );
    let id = enqueue(root.path(), &mut reopened, "next.mp4");
    reopened.begin_next().unwrap();
    assert!(reopened.remove(id).is_err());
    reopened.finish(id, State::Cancelled).unwrap();
    reopened.remove(id).unwrap();
    assert!(reopened.document.jobs.is_empty());
    assert_eq!(fs::read_dir(reopened.snapshots()).unwrap().count(), 0);
}

#[test]
fn render_queue_invalid_saved_state_is_reported_and_preserved() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let journal = store.directory.join("queue.json");
    fs::write(&journal, b"{broken queue").unwrap();
    assert!(Store::open(root.path()).is_err());
    assert_eq!(fs::read(journal).unwrap(), b"{broken queue");
}

#[test]
fn render_queue_uses_the_captured_template_when_the_library_changes_during_freezing() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path()).unwrap();
    let captured = store.document.templates[0].clone();
    let prepared = prepare(root.path(), &store);
    let mut later = captured.clone();
    later.name = "Changed while freezing".into();
    later.pattern = "changed-{id}.mp4".into();
    store.save_template(later).unwrap();
    store.remove_template(captured.id).unwrap();
    store
        .add(prepared, captured.clone(), root.path().join("captured.mp4"))
        .unwrap();
    assert_eq!(store.document.jobs[0].template, captured);
    assert_eq!(Store::open(root.path()).unwrap().document, store.document);
}

#[cfg(unix)]
#[test]
fn render_queue_refuses_replaced_storage_before_cleanup_or_journal_mutation() {
    let root = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path()).unwrap();
    let id = enqueue(root.path(), &mut store, "owned.mp4");
    let job = store.document.jobs[0].clone();
    let outside = foreign.path().join(&job.snapshot);
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), b"foreign input").unwrap();
    let snapshots = store.snapshots();
    fs::rename(&snapshots, store.directory.join("snapshots.retained")).unwrap();
    std::os::unix::fs::symlink(foreign.path(), &snapshots).unwrap();
    let before = store.document.clone();
    assert!(store.remove(id).is_err());
    assert!(store.snapshot_path(&job).is_err());
    assert!(store.begin_next().is_err());
    assert_eq!(store.document, before);
    assert_eq!(fs::read(outside.join("keep")).unwrap(), b"foreign input");
    fs::remove_file(&snapshots).unwrap();
}
