use super::*;

fn job(id: u64, output: PathBuf) -> Job {
    Job {
        id,
        snapshot: format!("movie-test{id}"),
        source: SnapshotInfo {
            composition: "comp-main".into(),
            name: "中英文".into(),
            width: 64,
            height: 32,
            fps: 30,
            frames: 6,
            range: motion_core::WorkArea::full(6),
            revision: 7,
        },
        template: Document::default().templates.remove(0),
        output,
        attempt: 0,
        state: State::Queued,
    }
}

#[test]
fn render_templates_expand_only_authored_tokens_and_keep_composition_text_literal() {
    let template = Document::default().templates.remove(0);
    assert_eq!(
        template.filename("镜头 {id}/1", 42, 7).unwrap(),
        "镜头 {id}_1-42.mp4"
    );
    for pattern in [
        "../{id}.mp4",
        "a\\b.mp4",
        "{unknown}.mp4",
        "{id.mp4",
        "a}.mp4",
        "CON.mp4",
        "NUL.MP4",
        "LPT1.mp4",
        "a.png",
    ] {
        let mut invalid = template.clone();
        invalid.pattern = pattern.into();
        assert!(invalid.validate().is_err(), "{pattern}");
    }
    let mut custom = template;
    custom.pattern = "r{revision}-{id}.MP4".into();
    assert_eq!(custom.filename("Name", 42, 7).unwrap(), "r7-42.MP4");
    assert!(custom.filename("", 1, 0).is_ok());
}

#[test]
fn render_queue_rejects_duplicate_identity_paths_and_impossible_active_state() {
    let root = tempfile::tempdir().unwrap();
    let mut document = Document::default();
    document.jobs.push(job(1, root.path().join("one.mp4")));
    document.next_id = 2;
    document.validate().unwrap();
    for field in [
        "id",
        "snapshot",
        "output",
        "active",
        "version",
        "range",
        "traversal",
    ] {
        let mut invalid = document.clone();
        match field {
            "version" => invalid.version = 999,
            "range" => invalid.jobs[0].source.range.out_frame = 0,
            "traversal" => invalid.jobs[0].snapshot = "movie-../foreign".into(),
            _ => {
                let mut second = job(2, root.path().join("two.mp4"));
                invalid.next_id = 3;
                match field {
                    "id" => second.id = 1,
                    "snapshot" => second.snapshot = invalid.jobs[0].snapshot.clone(),
                    "output" => second.output = invalid.jobs[0].output.clone(),
                    "active" => {
                        invalid.jobs[0].state = State::Running;
                        invalid.jobs[0].attempt = 1;
                        second.state = State::Cancelling;
                        second.attempt = 1;
                    }
                    _ => unreachable!(),
                }
                invalid.jobs.push(second);
            }
        }
        assert!(invalid.validate().is_err(), "{field}");
    }
}

#[test]
fn render_queue_json_requires_current_version_and_explicit_persistent_fields() {
    let document = Document::default();
    let value = serde_json::to_value(&document).unwrap();
    for field in ["version", "next_id", "next_template", "jobs", "templates"] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<Document>(missing).is_err(),
            "{field}"
        );
    }
    let round: Document = serde_json::from_value(value).unwrap();
    assert_eq!(round, document);
}
