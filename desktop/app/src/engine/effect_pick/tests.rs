use super::*;
use motion_core::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    Ease, Layer, LayerTimeline, Project,
};
use serde_json::json;
use std::{path::Path, time::Duration};
#[cfg(target_os = "windows")]
mod native;

fn project(reverse: bool, animated: bool) -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "gradient_ramp")
        .unwrap();
    let mut effect = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64.; 2],
    );
    effect.enabled = false;
    if animated {
        for (index, frame) in [-6, -3, 0].into_iter().enumerate() {
            effect
                .params
                .get_mut("start")
                .unwrap()
                .track
                .upsert(
                    frame,
                    [10. + index as f32, 20. + index as f32, 0., 0.],
                    Ease::Linear,
                )
                .unwrap();
            effect
                .params
                .get_mut("start_color")
                .unwrap()
                .track
                .upsert(
                    frame,
                    [0.1 + index as f32 * 0.1, 0.2, 0.3, 1.],
                    Ease::Linear,
                )
                .unwrap();
        }
    }
    let mut p = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(
        7,
        "Mapped effect pick",
        [64.; 2],
        [32., 32., 0.],
        [0.25, 0.5, 0.75, 1.],
    );
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 60,
        offset_frame: 2,
    });
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if reverse { 0. } else { -6. },
        )
        .unwrap(),
        remap: None,
    });
    layer.effects.push(effect);
    p.layers.push(layer);
    p.rebuild_plugin_dependencies();
    p
}
fn start(root: &Path, p: &Project, at: f64) -> Engine {
    motion_core::storage::save(root, p).unwrap();
    let engine = Engine::start(root.to_owned(), motion_desktop_media::platform(), 8 << 30).unwrap();
    engine.seek(at).unwrap();
    engine
}
fn pick(engine: &Engine, ticket: Ticket, point: [f64; 2]) -> Result<Value, crate::engine::Error> {
    engine
        .request(|reply| Command::EffectPick {
            ticket,
            point,
            reply,
        })
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
}
fn ticket(state: &Value, name: &str) -> Ticket {
    Ticket::new(
        state,
        7,
        1,
        name,
        if name == "start" {
            Kind::Position
        } else {
            Kind::Color
        },
    )
}
fn expected(before: &Value, name: &str, value: [f32; 4]) -> Value {
    let mut p = before["project"].clone();
    p["layers"][0]["effects"][0]["params"][name]["track"]["keys"][1]["value"] = json!(value);
    p
}

#[test]
fn mapped_position_pick_edits_only_the_exact_signed_local_key_and_persists_with_one_undo() {
    for reverse in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        let p = project(reverse, true);
        // Negative control: the former floor loses this valid identity.
        let mut legacy = motion_core::Engine::new(p.clone()).unwrap();
        let legacy_before = legacy.snapshot();
        assert!(legacy
            .apply(motion_core::Command::Effect {
                object: 7,
                action: motion_core::EffectAction::Set {
                    effect: 1,
                    param: "start".into(),
                    frame: 6,
                    value: [40., 15., 0., 0.]
                }
            })
            .is_err());
        assert_eq!(legacy.snapshot(), legacy_before);
        let engine = start(&root, &p, 6.5);
        let before = engine.state().unwrap();
        let after = pick(&engine, ticket(&before, "start"), [40., 15.]).unwrap();
        assert_eq!(
            after["project"],
            expected(&before, "start", [40., 15., 0., 0.])
        );
        assert_eq!(after["history"]["undoEntries"], 1);
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
        engine.save().unwrap();
        assert_eq!(
            serde_json::to_value(motion_core::storage::load(&root).unwrap()).unwrap(),
            after["project"]
        );
        // A separate worker loads the persisted bytes with no inherited history.
        // Release the document lock through the normal worker shutdown first.
        drop(engine);
        let reopened = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
        assert_eq!(reopened.state().unwrap()["project"], after["project"]);
        assert_eq!(reopened.state().unwrap()["history"]["undoEntries"], 0);
        reopened.request_shutdown().unwrap();
    }
}

#[test]
fn mapped_pick_rejects_unrepresentable_animated_time_but_accepts_static_values_without_keys() {
    for reverse in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let engine = start(
            &folder.path().join("animated"),
            &project(reverse, true),
            8.75,
        );
        let before = engine.state().unwrap();
        let error = pick(&engine, ticket(&before, "start"), [40., 15.]).unwrap_err();
        assert!(error.to_string().contains("continuous key addressing"), "{error}");
        let after = engine.state().unwrap();
        assert_eq!(after["project"], before["project"]);
        assert_eq!(after["history"], before["history"]);
        engine.request_shutdown().unwrap();
        let engine = start(
            &folder.path().join("static"),
            &project(reverse, false),
            8.75,
        );
        let before = engine.state().unwrap();
        let after = pick(&engine, ticket(&before, "start"), [40., 15.]).unwrap();
        let mut expected = before["project"].clone();
        expected["layers"][0]["effects"][0]["params"]["start"]["track"]["value"] =
            json!([40., 15., 0., 0.]);
        assert_eq!(after["project"], expected);
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        engine.request_shutdown().unwrap();
    }
}

#[test]
fn mapped_pick_cancels_stale_time_revision_and_missing_preview_without_history() {
    let folder = tempfile::tempdir().unwrap();
    let engine = start(&folder.path().join("project"), &project(false, true), 6.5);
    let before = engine.state().unwrap();
    let old = ticket(&before, "start");
    engine.seek(11.).unwrap();
    let cancelled = pick(&engine, old, [40., 15.]).unwrap();
    assert_eq!(cancelled["editorAction"]["reason"], "context_changed");
    assert_eq!(cancelled["project"], before["project"]);
    engine.seek(6.5).unwrap();
    let old = ticket(&engine.state().unwrap(), "start");
    let changed = engine
        .edit(
            serde_json::to_string(&motion_core::Command::Flags {
                object: 7,
                visible: true,
                locked: true,
            })
            .unwrap(),
        )
        .unwrap();
    let cancelled = pick(&engine, old, [40., 15.]).unwrap();
    assert_eq!(cancelled["editorAction"]["reason"], "context_changed");
    assert_eq!(cancelled["project"], changed["project"]);
    engine.history(0).unwrap();
    let before = engine.state().unwrap();
    let pending = pick(&engine, ticket(&before, "start_color"), [0.5; 2]).unwrap();
    assert_eq!(pending["editorAction"]["reason"], "preview_pending");
    assert_eq!(pending["project"], before["project"]);
    assert_eq!(pending["history"], before["history"]);
    engine.request_shutdown().unwrap();
}
