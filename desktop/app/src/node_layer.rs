//! Project-owned node effect sources and canonical runtime packages.
use crate::*;
use motion_core::{Command, EffectAction, EffectInstance};
use motion_graph::{
    CompiledGraph, Connection, GraphDocument, ImageSource, Node, NodeOp, SampleCoordinates,
};
use std::time::{SystemTime, UNIX_EPOCH};
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Target {
    pub root: PathBuf,
    pub composition: String,
    pub object: u64,
    pub effect: u64,
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub creating: bool,
}
fn hash(text: &[u8]) -> u64 {
    text.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}
pub fn identity() -> GraphDocument {
    let mut graph = motion_graph::demo_graph();
    graph.id = "layer-node-effect".into();
    graph.name = "节点效果".into();
    graph.english_name = "Node Effect".into();
    graph.public_parameters.clear();
    graph.nodes = vec![
        Node::new(
            "input",
            NodeOp::Image {
                source: ImageSource::Input,
            },
        ),
        Node::new("uv", NodeOp::Uv),
        Node::new(
            "sample",
            NodeOp::SampleImage {
                coordinates: SampleCoordinates::Uv,
            },
        ),
        Node::new("output", NodeOp::ColorOutput),
    ];
    for (i, node) in graph.nodes.iter_mut().enumerate() {
        node.position = [i as f32 * 240., if node.id == "uv" { 180. } else { 0. }];
    }
    graph.connections = vec![
        Connection::new("input", "image", "sample", "image"),
        Connection::new("uv", "value", "sample", "coordinate"),
        Connection::new("sample", "color", "output", "color"),
    ];
    graph
}
fn source_session(snapshot: &node_editor::EditorSnapshot) -> String {
    static NONCE: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    let nonce = NONCE.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    });
    format!("{nonce}:{}", snapshot.epoch)
}
pub fn source(snapshot: &node_editor::EditorSnapshot, target: &Target) -> Result<Value, String> {
    Ok(
        serde_json::json!({"format_version":1,"key":target.key,"document":snapshot.document,"options":snapshot.options,"session":source_session(snapshot),"revision":snapshot.revision,"semantic_revision":snapshot.semantic_revision}),
    )
}
pub fn package(compiled: &CompiledGraph, target: &Target) -> Result<Vec<u8>, String> {
    let mut manifest = compiled.manifest.clone();
    manifest.id = format!(
        "local.motionstudio.bound.{:016x}",
        hash(
            format!(
                "{}/{}/{}/{}",
                target.composition, target.object, target.effect, target.key
            )
            .as_bytes()
        )
    );
    manifest.version = "0.0.0".into();
    let neutral = compiled.package_bytes(&manifest)
        .map_err(|e| e.to_string())?;
    let fingerprint = motion_effects::EffectPackage::from_bytes(neutral)
        .map_err(|e| e.to_string())?
        .hash;
    manifest.version = format!("0.0.0+{}", &fingerprint[..16]);
    compiled.package_bytes(&manifest).map_err(|e| e.to_string())
}
fn check<'a>(
    s: &'a motion_host::Session,
    target: &Target,
) -> Result<&'a motion_core::Layer, String> {
    if s.root != target.root || s.engine.project().composition_id != target.composition {
        return Err("node effect target composition changed".into());
    }
    let layer = s
        .engine
        .project()
        .layers
        .iter()
        .find(|l| l.id == target.object)
        .ok_or("node effect target layer was removed")?;
    if layer.locked {
        return Err("node effect target layer is locked".into());
    }
    if matches!(
        layer.content,
        motion_core::Content::Null | motion_core::Content::Camera { .. } | motion_core::Content::Audio { .. }
    ) {
        return Err("select a visual layer for a node effect".into());
    }
    if let Some(effect) = layer.effects.iter().find(|e| e.id == target.effect) {
        if effect.node_graph.as_ref().and_then(|v| v["key"].as_str()) != Some(target.key.as_str()) {
            return Err("node effect target instance was replaced".into());
        }
    }
    if !target.creating && !layer.effects.iter().any(|e| e.id == target.effect) {
        return Err("node effect target instance was removed".into());
    }
    Ok(layer)
}
pub fn persist(
    s: &mut motion_host::Session,
    snapshot: &node_editor::EditorSnapshot,
) -> motion_host::Result<Value> {
    let target = snapshot
        .layer_target
        .as_ref()
        .ok_or("node graph is not bound")?;
    let layer = check(s, target)?;
    let Some(effect) = layer.effects.iter().find(|e| e.id == target.effect) else {
        return Err("node effect has not finished its first compile".into());
    };
    let mut value = source(snapshot, target)?;
    if let Some(runtime) = effect.node_graph.as_ref().and_then(|v| v.get("runtime")) {
        value["runtime"] = runtime.clone();
    }
    if effect.node_graph.as_ref().is_some_and(|old| {
        (old["document"] == value["document"] && old["options"] == value["options"])
            || (old["session"] == source_session(snapshot)
                && old["revision"].as_u64().unwrap_or(0) > snapshot.revision)
    }) {
        return s.snapshot();
    }
    s.apply_command_batch(vec![Command::Effect {
        object: target.object,
        action: EffectAction::SetNodeGraph {
            effect: target.effect,
            source: value,
        },
    }])
}
pub fn apply(
    s: &mut motion_host::Session,
    snapshot: &node_editor::EditorSnapshot,
    compiled: &CompiledGraph,
) -> motion_host::Result<Value> {
    let target = snapshot
        .layer_target
        .as_ref()
        .ok_or("node graph is not bound")?;
    let layer = check(s, target)?;
    let old = layer
        .effects
        .iter()
        .find(|e| e.id == target.effect)
        .cloned();
    let size = layer.size;
    let bytes = package(compiled, target)?;
    let packaged =
        motion_effects::EffectPackage::from_bytes(bytes.clone()).map_err(|e| e.to_string())?;
    let definition = packaged
        .manifest
        .effects
        .first()
        .ok_or("node effect has no output")?;
    let mut instance = EffectInstance::new(
        target.effect,
        &packaged.manifest.id,
        &packaged.manifest.version,
        &packaged.hash,
        definition,
        size,
    );
    let mut source_value = source(snapshot, target)?;
    if let Some(old) = &old {
        instance.enabled = old.enabled;
        instance.seed = old.seed;
        instance.image_input = old.image_input;
        for (id, param) in &old.params {
            if let Some(next) = instance.params.get_mut(id) {
                if next.kind == param.kind
                    && (!param.track.keys.is_empty() || param.track.value != param.default)
                {
                    next.track = param.track.clone();
                    next.curve = param.curve.clone();
                }
            }
        }
        if let Some(value) = &old.node_graph {
            if (value["document"]
                == serde_json::to_value(&snapshot.document).map_err(|e| e.to_string())?
                && value["options"]
                    == serde_json::to_value(&snapshot.options).map_err(|e| e.to_string())?)
                || (value["session"] == source_session(snapshot)
                    && value["semantic_revision"] == snapshot.semantic_revision
                    && value["revision"].as_u64().unwrap_or(0) > snapshot.revision)
            {
                source_value = value.clone();
            }
        }
    }
    source_value["runtime"] = serde_json::json!({"package_hex":bytes.iter().map(|byte|format!("{byte:02x}")).collect::<String>()});
    instance.node_graph = Some(source_value);
    instance
        .validate(s.engine.project().frames)
        .map_err(|e| e.to_string())?;
    if old.as_ref() == Some(&instance) {
        return s.snapshot();
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let staged = s.root.join(format!(".node-runtime-{stamp}.msfx"));
    let mut file = crate::owned_file::OwnedFile::create(&staged)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    file.file().write_all(&bytes).map_err(|error| error.to_string())?;
    file.close();
    let install = s.plugin_request(&serde_json::json!({"op":"install","path":staged}).to_string());
    drop(file);
    install?;
    s.apply_command_batch(vec![Command::Effect {
        object: target.object,
        action: if old.is_some() {
            EffectAction::Replace { instance }
        } else {
            EffectAction::Insert { instance }
        },
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (
        tempfile::TempDir,
        motion_host::Session,
        node_editor::NodeEditor,
    ) {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            1,
            "Layer",
            [32.; 2],
            [32., 32., 0.],
            [1., 0., 0., 1.],
        ));
        let session =
            motion_host::Session::new(project, root.clone(), motion_desktop_media::platform())
                .unwrap();
        let mut editor = node_editor::NodeEditor::new(identity());
        editor.layer_target = Some(Target {
            root,
            composition: session.engine.project().composition_id.clone(),
            object: 1,
            effect: 1,
            key: "test-node-source".into(),
            label: "Layer".into(),
            creating: true,
        });
        (folder, session, editor)
    }
    #[test]
    fn layer_effect_source_persists_and_runtime_updates_one_instance_as_one_undo() {
        let (_folder, mut session, mut editor) = setup();
        let snapshot = editor.snapshot();
        let compiled = snapshot.compile().unwrap();
        apply(&mut session, &snapshot, &compiled).unwrap();
        let before = session.engine.project().clone();
        assert_eq!(before.version, motion_model::CURRENT_PROJECT_VERSION);
        assert_eq!(before.layers[0].effects.len(), 1);
        assert!(before.layers[0].effects[0].node_graph.is_some());
        let mut doc = editor.snapshot().document;
        doc.nodes.push(Node::new(
            "invert",
            NodeOp::ColorMath {
                operation: motion_graph::ColorOp::Invert,
            },
        ));
        doc.connections.retain(|c| c.to.node != "output");
        doc.connections.extend([
            Connection::new("sample", "color", "invert", "color"),
            Connection::new("invert", "color", "output", "color"),
        ]);
        editor.replace_document(doc, None);
        editor.layer_target = snapshot.layer_target.clone();
        let next = editor.snapshot();
        let compiled = next.compile().unwrap();
        apply(&mut session, &next, &compiled).unwrap();
        assert_eq!(session.engine.project().layers[0].effects.len(), 1);
        assert_ne!(
            session.engine.project().layers[0].effects[0].hash,
            before.layers[0].effects[0].hash
        );
        session.engine.undo().unwrap();
        assert_eq!(session.engine.project(), &before);
        let json = serde_json::to_string(session.engine.project()).unwrap();
        let reopened: motion_core::Project = serde_json::from_str(&json).unwrap();
        reopened.validate().unwrap();
        let mut loaded = node_editor::NodeEditor::default();
        loaded
            .load_layer_source(
                reopened.layers[0].effects[0].node_graph.as_ref().unwrap(),
                snapshot.layer_target.unwrap(),
            )
            .unwrap();
        assert_eq!(loaded.snapshot().document, identity());
    }
    #[test]
    fn invalid_authoring_source_is_kept_without_replacing_valid_runtime() {
        let (_folder, mut session, mut editor) = setup();
        let snapshot = editor.snapshot();
        apply(&mut session, &snapshot, &snapshot.compile().unwrap()).unwrap();
        let hash = session.engine.project().layers[0].effects[0].hash.clone();
        let mut doc = editor.snapshot().document;
        doc.connections.retain(|c| c.to.node != "output");
        editor.replace_document(doc, None);
        editor.layer_target = snapshot.layer_target;
        let broken = editor.snapshot();
        assert!(broken.compile().is_err());
        persist(&mut session, &broken).unwrap();
        assert_eq!(session.engine.project().layers[0].effects[0].hash, hash);
        assert_eq!(
            session.engine.project().layers[0].effects[0]
                .node_graph
                .as_ref()
                .unwrap()["document"],
            serde_json::to_value(&broken.document).unwrap()
        );
    }
    #[test]
    fn public_parameter_animation_survives_recompile_and_source_reopen_is_noop() {
        let (_folder, mut session, mut editor) = setup();
        editor.replace_document(motion_graph::demo_graph(), None);
        editor.layer_target = Some(Target {
            root: session.root.clone(),
            composition: session.engine.project().composition_id.clone(),
            object: 1,
            effect: 1,
            key: "parameter-source".into(),
            label: "Layer".into(),
            creating: true,
        });
        let first = editor.snapshot();
        apply(&mut session, &first, &first.compile().unwrap()).unwrap();
        session
            .apply_command_batch(vec![
                Command::Effect {
                    object: 1,
                    action: EffectAction::Animate {
                        effect: 1,
                        param: "color".into(),
                        frame: 0,
                        enabled: true,
                    },
                },
                Command::Effect {
                    object: 1,
                    action: EffectAction::Set {
                        effect: 1,
                        param: "color".into(),
                        frame: 45,
                        value: [0.8, 0.1, 0.2, 1.],
                    },
                },
            ])
            .unwrap();
        let track = session.engine.project().layers[0].effects[0].params["color"]
            .track
            .clone();
        editor.set_parameter_default("color", 0, 0.4).unwrap();
        let next = editor.snapshot();
        apply(&mut session, &next, &next.compile().unwrap()).unwrap();
        assert_eq!(
            session.engine.project().layers[0].effects[0].params["color"].track,
            track
        );
        let project = session.engine.project().clone();
        let source = project.layers[0].effects[0].node_graph.as_ref().unwrap();
        let mut target = first.layer_target.unwrap();
        target.creating = false;
        editor.load_layer_source(source, target).unwrap();
        let reopened = editor.snapshot();
        let revision = session.engine.revision();
        persist(&mut session, &reopened).unwrap();
        apply(&mut session, &reopened, &reopened.compile().unwrap()).unwrap();
        assert_eq!(session.engine.revision(), revision);
        assert_eq!(session.engine.project(), &project);
    }
    #[test]
    fn runtime_and_unfinished_source_survive_project_package() {
        let (folder, mut session, mut editor) = setup();
        let first = editor.snapshot();
        apply(&mut session, &first, &first.compile().unwrap()).unwrap();
        editor.disconnect_input(&motion_graph::PortRef::new("output", "color"));
        let broken = editor.snapshot();
        persist(&mut session, &broken).unwrap();
        let archive = folder.path().join("project.msproj");
        motion_core::storage::export_package(&session.root, session.engine.project(), &archive)
            .unwrap();
        let imported = motion_core::storage::import_package(
            &archive,
            &folder.path().join("isolated/reopened"),
        )
        .unwrap();
        assert_eq!(&imported, session.engine.project());
        let reopened = motion_host::Session::new(
            imported,
            folder.path().join("isolated/reopened"),
            motion_desktop_media::platform(),
        )
        .unwrap();
        let e = &reopened.engine.project().layers[0].effects[0];
        assert!(reopened
            .effects
            .registry
            .resolve(&e.plugin, &e.version, &e.hash)
            .is_ok());
    }
    #[test]
    fn removed_existing_target_cannot_be_recreated_by_stale_compile() {
        let (_folder, mut session, mut editor) = setup();
        let first = editor.snapshot();
        apply(&mut session, &first, &first.compile().unwrap()).unwrap();
        editor.layer_target.as_mut().unwrap().creating = false;
        session
            .apply_command_batch(vec![Command::Effect {
                object: 1,
                action: EffectAction::Remove { effect: 1 },
            }])
            .unwrap();
        let before = session.engine.project().clone();
        let snapshot = editor.snapshot();
        assert!(apply(&mut session, &snapshot, &snapshot.compile().unwrap()).is_err());
        assert!(persist(&mut session, &snapshot).is_err());
        assert_eq!(session.engine.project(), &before);
    }
    #[test]
    fn independent_editors_do_not_share_async_stamps() {
        let mut first = node_editor::NodeEditor::new(identity());
        let snapshot = first.snapshot();
        let mut second = node_editor::NodeEditor::new(identity());
        assert_ne!(snapshot.preview_stamp(), second.stamp());
        assert!(!second.accept_compile(&snapshot, Ok(snapshot.compile().unwrap().report)));
        first.replace_document(identity(), None);
        assert_ne!(snapshot.preview_stamp(), first.stamp());
    }
    #[test]
    fn invalid_embedded_runtime_is_rejected_when_no_cached_package_exists() {
        let (folder, mut session, editor) = setup();
        let snapshot = editor.snapshot();
        apply(&mut session, &snapshot, &snapshot.compile().unwrap()).unwrap();
        let mut project = session.engine.project().clone();
        project.layers[0].effects[0].hash = "0".repeat(64);
        project.rebuild_plugin_dependencies();
        let root = folder.path().join("empty-cache/project");
        std::fs::create_dir_all(&root).unwrap();
        let result = motion_host::Session::new(project, root, motion_desktop_media::platform());
        assert!(matches!(result,Err(message)if message.to_string().contains("does not match")));
    }
}
