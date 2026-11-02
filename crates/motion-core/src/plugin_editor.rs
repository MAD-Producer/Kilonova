//! Scoped, version-pinned editor bridge. Plugin code never receives general commands.
use crate::{ensure, Command, EffectAction, Engine, Error, Project, Result};
use motion_effects::{Registry, SceneSettings};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorRequest {
    State,
    ColorBegin { revision: u64, param: String },
    ColorFinish { revision: u64, commit: bool },
    ParameterBegin { revision: u64, param: String },
    ParameterFinish { revision: u64, commit: bool },
    Spatial { revision: u64, param: String, frame: u32, tangents: Option<crate::SpatialTangents<[f32; 4]>> },
    SetAt { revision: u64, param: String, frame: u32, value: [f32; 4] },
    Key { revision: u64, param: String },
    Transform {
        revision: u64,
        property: crate::Property,
        value: [f32; 3],
    },
    Begin {
        revision: u64,
    },
    Commit {
        revision: u64,
    },
    Cancel {
        revision: u64,
    },
    Set {
        revision: u64,
        param: String,
        value: [f32; 4],
    },
    Animate {
        revision: u64,
        param: String,
        enabled: bool,
    },
    Curve {
        revision: u64,
        param: String,
        value: crate::CurveObject,
    },
    Scene {
        revision: u64,
        settings: SceneSettings,
    },
    Seed {
        revision: u64,
        seed: u32,
    },
}
pub struct PluginEditorSession {
    pub object: u64,
    pub instance: u64,
    pub dependency: crate::PluginDependency,
    pub effect: String,
    pub gesture: bool,
    color_edit: Option<ColorEdit>,
    parameter_edit: Option<ColorEdit>,
}
#[derive(Clone)]
struct ColorEdit {
    param: String,
    frame: u32,
    track: crate::Track<[f32; 4]>,
}
impl PluginEditorSession {
    pub fn open(
        project: &Project,
        registry: &Registry,
        object: u64,
        instance: u64,
    ) -> Result<Self> {
        let e = project
            .layers
            .iter()
            .find(|l| l.id == object)
            .and_then(|l| l.effects.iter().find(|e| e.id == instance))
            .ok_or_else(|| Error::Invalid("editor effect instance missing".into()))?;
        let package = registry
            .resolve(&e.plugin, &e.version, &e.hash)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        ensure(
            package
                .manifest
                .effects
                .iter()
                .any(|d| d.id == e.effect && (d.editor.is_some() || d.native_editor.is_some())),
            "effect has no custom editor",
        )?;
        Ok(Self {
            object,
            instance,
            dependency: e.dependency(),
            effect: e.effect.clone(),
            gesture: false,
            color_edit: None,
            parameter_edit: None,
        })
    }
    fn check<'a>(
        &self,
        project: &'a Project,
    ) -> Result<(&'a crate::Layer, &'a crate::EffectInstance)> {
        let layer = project
            .layers
            .iter()
            .find(|l| l.id == self.object)
            .ok_or_else(|| Error::Invalid("editor layer was removed".into()))?;
        let e = layer
            .effects
            .iter()
            .find(|e| e.id == self.instance)
            .ok_or_else(|| Error::Invalid("editor effect was removed".into()))?;
        ensure(
            e.dependency() == self.dependency && e.effect == self.effect,
            "editor dependency changed; reopen the editor",
        )?;
        Ok((layer, e))
    }
    pub fn state(&self, engine: &Engine, frame: u32) -> Result<Value> {
        ensure(
            frame < engine.project().frames,
            "editor frame outside composition",
        )?;
        let (layer, e) = self.check(engine.project())?;
        let values = e
            .params
            .iter()
            .map(|(id, p)| -> Result<_> { Ok((id.clone(), json!(p.sample(layer.local_frame(frame as f64))?))) })
            .collect::<Result<serde_json::Map<_, _>>>()?;
        Ok(
            json!({"revision":engine.revision(),"frame":frame,"object":self.object,"instance":self.instance,
            "plugin":self.dependency,"effect":self.effect,"values":values,"params":e.params,"scene":e.scene,"seed":e.seed,
            "expressions":engine.project().expressions.iter().filter(|e|matches!(&e.target,crate::ExpressionTarget::Effect {object,effect,..} if *object==self.object && *effect==self.instance)).collect::<Vec<_>>(),
            "transform":layer.transform,
            "transform_values":{"position":layer.transform.position.sample(layer.local_frame(frame as f64))?,
                "rotation":layer.transform.rotation.sample(layer.local_frame(frame as f64))?,
                "scale":layer.transform.scale.sample(layer.local_frame(frame as f64))?},
            "dimensions":[engine.project().width,engine.project().height],"camera":engine.project().camera,
            "fps":engine.project().fps,"frames":engine.project().frames,"timeline_offset":layer.timeline.map_or(0,|t|t.offset_frame),
            "images":engine.project().assets.iter().map(|a|json!({"id":a.id,"width":a.width,"height":a.height})).collect::<Vec<_>>(),
            "locked":layer.locked,"gesture":self.gesture,
            "color_edit":self.color_edit.as_ref().map(|c|json!({"param":c.param,"frame":c.frame})),
            "parameter_edit":self.parameter_edit.as_ref().map(|c|json!({"param":c.param,"frame":c.frame})),
            "layers":engine.project().layers.iter().map(|l|json!({"id":l.id,"name":l.name,"particle_source":!matches!(l.content,crate::Content::Audio { .. })})).collect::<Vec<_>>()}),
        )
    }
    pub fn request(
        &mut self, engine: &mut Engine, frame: u32, request: EditorRequest,
    ) -> Result<Value> {
        let automatic = !engine.gesture_active() && matches!(&request,
            EditorRequest::Transform { .. } | EditorRequest::Set { .. } | EditorRequest::SetAt { .. }
            | EditorRequest::Spatial { .. } | EditorRequest::Animate { .. } | EditorRequest::Key { .. }
            | EditorRequest::Curve { .. } | EditorRequest::Scene { .. } | EditorRequest::Seed { .. });
        let starting = !engine.gesture_active() && matches!(&request, EditorRequest::Begin { .. });
        self.request_boundary(engine, automatic, starting, |editor, engine| editor.request_inner(engine, frame, request))
    }
    fn request_boundary(
        &mut self, engine: &mut Engine, automatic: bool, starting: bool,
        operation: impl FnOnce(&mut Self, &mut Engine) -> Result<Value>,
    ) -> Result<Value> {
        if automatic { engine.begin_gesture()?; }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(self, engine)));
        match outcome {
            Ok(result) => {
                if let Err(error) = &result {
                    log::warn!("plugin editor operation rejected object={} instance={} revision={} automatic={automatic} reason={error}", self.object, self.instance, engine.revision());
                }
                if automatic {
                    engine.end_gesture(result.is_ok()).map_err(|error| {
                        log::error!("plugin editor gesture close failed object={} instance={} revision={} operation_error={:?} reason={error}", self.object, self.instance, engine.revision(), result.as_ref().err());
                        error
                    })?;
                }
                result
            }
            Err(payload) => {
                let reason = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown panic");
                let reason = reason.chars().take(2048).collect::<String>();
                log::error!("plugin editor operation panicked object={} instance={} revision={} automatic={automatic} starting={starting} reason={reason}", self.object, self.instance, engine.revision());
                if (automatic || starting || self.gesture) && engine.gesture_active() {
                    engine.end_gesture(false).map_err(|error| {
                        log::error!("plugin editor panic rollback failed object={} instance={} revision={} panic={reason:?} reason={error}", self.object, self.instance, engine.revision());
                        error
                    })?;
                }
                self.gesture = false;
                self.color_edit = None;
                self.parameter_edit = None;
                Err(Error::Invalid(format!(
                    "plugin editor request panicked: {reason}"
                )))
            }
        }
    }
    fn request_inner(
        &mut self,
        engine: &mut Engine,
        frame: u32,
        request: EditorRequest,
    ) -> Result<Value> {
        ensure(
            frame < engine.project().frames,
            "editor frame outside composition",
        )?;
        self.check(engine.project())?;
        let revision = match &request {
            EditorRequest::State => return self.state(engine, frame),
            EditorRequest::ColorBegin { revision, .. }
            | EditorRequest::ColorFinish { revision, .. }
            | EditorRequest::ParameterBegin { revision, .. }
            | EditorRequest::ParameterFinish { revision, .. }
            | EditorRequest::Spatial { revision, .. }
            | EditorRequest::SetAt { revision, .. }
            |
            EditorRequest::Begin { revision }
            | EditorRequest::Commit { revision }
            | EditorRequest::Cancel { revision }
            | EditorRequest::Transform { revision, .. }
            | EditorRequest::Set { revision, .. }
            | EditorRequest::Animate { revision, .. }
            | EditorRequest::Curve { revision, .. }
            | EditorRequest::Scene { revision, .. }
            | EditorRequest::Key { revision, .. }
            | EditorRequest::Seed { revision, .. } => *revision,
        };
        ensure(
            revision == engine.revision(),
            "stale editor revision; request state and retry",
        )?;
        let (_, e) = self.check(engine.project())?;
        let effect = e.id;
        if let Some(color) = self.color_edit.as_ref().or(self.parameter_edit.as_ref()) {
            match &request {
                EditorRequest::ColorFinish { .. } | EditorRequest::ParameterFinish { .. } | EditorRequest::Commit { .. } | EditorRequest::Cancel { .. } => {}
                EditorRequest::Spatial { param, .. } | EditorRequest::SetAt { param, .. } if self.parameter_edit.is_some() => {
                    ensure(*param == color.param, "finish active parameter edit before editing another parameter")?;
                }
                EditorRequest::Set { param, .. } | EditorRequest::Key { param, .. } | EditorRequest::Animate { param, .. } => {
                    ensure(*param == color.param && frame == color.frame, "finish active color edit before editing another parameter or frame")?;
                }
                _ => return Err(Error::Invalid("finish active color edit first".into())),
            }
        }
        let action = match request {
            EditorRequest::State => unreachable!(),
            EditorRequest::ParameterBegin { param, .. } => {
                ensure(self.gesture, "parameter scope requires an active editor page gesture")?;
                let (layer, instance) = self.check(engine.project())?;
                ensure(!layer.locked, "object is locked")?;
                let p = instance.params.get(&param).ok_or_else(|| Error::Invalid("editor parameter missing".into()))?;
                ensure(p.implemented && matches!(p.kind, motion_effects::ParamKind::Vec2 | motion_effects::ParamKind::Vec3), "parameter scope requires an editable vector parameter")?;
                self.parameter_edit = Some(ColorEdit { param, frame, track: p.track.clone() });
                return self.state(engine, frame);
            }
            EditorRequest::ParameterFinish { commit, .. } => {
                let saved = self.parameter_edit.clone().ok_or_else(|| Error::Invalid("no active parameter edit".into()))?;
                if !commit {
                    engine.apply(Command::Effect { object: self.object, action: EffectAction::RestoreParameterTrack {
                        effect, param: saved.param, track: saved.track,
                    } })?;
                }
                self.parameter_edit = None;
                return self.state(engine, frame);
            }
            EditorRequest::Spatial { param, frame, tangents, .. } => EffectAction::Spatial { effect, param, frame, tangents },
            EditorRequest::SetAt { param, frame, value, .. } => {
                let (_, instance) = self.check(engine.project())?;
                ensure(instance.params.get(&param).is_some_and(|p|matches!(p.kind,motion_effects::ParamKind::Vec2|motion_effects::ParamKind::Vec3)), "set_at requires a vector parameter")?;
                EffectAction::Set { effect, param, frame, value }
            }
            EditorRequest::ColorBegin { param, .. } => {
                ensure(self.gesture, "color picker requires an active editor page gesture")?;
                let (layer, instance) = self.check(engine.project())?;
                ensure(!layer.locked, "object is locked")?;
                let p = instance.params.get(&param).ok_or_else(|| Error::Invalid("editor parameter missing".into()))?;
                ensure(p.kind == motion_effects::ParamKind::Color, "color picker requires a color parameter")?;
                self.color_edit = Some(ColorEdit { param, frame, track: p.track.clone() });
                return self.state(engine, frame);
            }
            EditorRequest::ColorFinish { commit, .. } => {
                let color = self.color_edit.clone().ok_or_else(|| Error::Invalid("no active color picker".into()))?;
                if !commit {
                    engine.apply(Command::Effect { object: self.object, action: EffectAction::RestoreColorTrack {
                        effect, param: color.param, track: color.track,
                    } })?;
                }
                self.color_edit = None;
                self.parameter_edit = None;
                return self.state(engine, frame);
            }
            EditorRequest::Begin { .. } => {
                ensure(!self.gesture, "editor gesture already active")?;
                engine.begin_gesture()?;
                self.gesture = true;
                return self.state(engine, frame);
            }
            EditorRequest::Commit { .. } | EditorRequest::Cancel { .. } => {
                ensure(self.gesture, "no active editor gesture")?;
                engine.end_gesture(matches!(request, EditorRequest::Commit { .. }))?;
                self.gesture = false;
                self.color_edit = None;
                self.parameter_edit = None;
                return self.state(engine, frame);
            }
            EditorRequest::Transform {
                property, value, ..
            } => {
                ensure(
                    matches!(
                        property,
                        crate::Property::Position
                            | crate::Property::Rotation
                            | crate::Property::Scale
                    ),
                    "editor can edit only its own layer position, rotation and scale",
                )?;
                engine.apply(Command::SetVector {
                    object: self.object,
                    property,
                    frame,
                    value,
                })?;
                return self.state(engine, frame);
            }
            EditorRequest::Set { param, value, .. } => EffectAction::Set {
                effect,
                param,
                frame,
                value,
            },
            EditorRequest::Key { param, .. } => {
                let (layer, instance) = self.check(engine.project())?;
                let p=instance.params.get(&param).ok_or_else(||Error::Invalid("editor parameter missing".into()))?;
                ensure(p.animatable && p.curve.is_none(),"parameter does not support numeric keys")?;
                let local=layer.edit_frame(frame)?;
                if p.track.keys.is_empty() { EffectAction::Animate {effect,param,frame,enabled:true} }
                else if p.track.keys.iter().any(|k|k.frame==local) {EffectAction::DeleteKey {effect,param,frame}}
                else {EffectAction::Set {effect,param,frame,value:p.sample(local as f64)?}}
            },
            EditorRequest::Animate { param, enabled, .. } => EffectAction::Animate {
                effect,
                param,
                frame,
                enabled,
            },
            EditorRequest::Curve { param, value, .. } => EffectAction::SetCurveObject {
                effect,
                param,
                frame,
                value,
            },
            EditorRequest::Scene { settings, .. } => EffectAction::SetScene {
                effect,
                scene: settings,
            },
            EditorRequest::Seed { seed, .. } => EffectAction::Seed { effect, seed },
        };
        engine.apply(Command::Effect {
            object: self.object,
            action,
        })?;
        self.state(engine, frame)
    }
    pub fn close(&mut self, engine: &mut Engine, commit: bool) -> Result<()> {
        if self.gesture && engine.gesture_active() { engine.end_gesture(commit)?; }
        self.gesture = false;
        self.color_edit = None;
        self.parameter_edit = None;
        Ok(())
    }
}

#[cfg(test)]
mod request_panic_tests {
    use crate::scene_fixtures;
    use super::*;
    #[test]
    fn rejected_operation_evidence_survives_a_gesture_close_error() {
        const CHILD: &str = "MOTION_PLUGIN_BOUNDARY_OBSERVATION_CHILD";
        if let Some(path) = std::env::var_os(CHILD) {
            struct Capture(std::sync::Mutex<std::fs::File>);
            impl log::Log for Capture {
                fn enabled(&self, _: &log::Metadata<'_>) -> bool {
                    true
                }
                fn log(&self, record: &log::Record<'_>) {
                    use std::io::Write;
                    let mut file = self.0.lock().unwrap();
                    writeln!(file, "{} {}", record.level(), record.args()).unwrap();
                    file.flush().unwrap();
                }
                fn flush(&self) {}
            }
            log::set_logger(Box::leak(Box::new(Capture(std::sync::Mutex::new(
                std::fs::File::create(&path).unwrap(),
            )))))
            .unwrap();
            log::set_max_level(log::LevelFilter::Info);
            let (mut engine, mut editor) = setup();
            let original = engine.snapshot();
            let result = editor.request_boundary(&mut engine, true, false, |_, engine| {
                // Exercise the real close API twice so the outer close fails.
                engine.end_gesture(false)?;
                Err(Error::Invalid("original font import failure".into()))
            });
            assert!(matches!(result, Err(Error::Gesture)));
            assert_eq!(engine.snapshot(), original);
            let text = std::fs::read_to_string(path).unwrap();
            let rejection = text.find("plugin editor operation rejected").unwrap();
            let closure = text.find("plugin editor gesture close failed").unwrap();
            assert!(rejection < closure);
            assert!(text[rejection..closure].contains("original font import failure"));
            assert!(text[closure..].contains("original font import failure"));
            assert!(text.contains("object=1 instance=1 revision=0"));
            let result = editor.request_boundary(&mut engine, true, false, |_, _| {
                panic!("caught operation panic evidence")
            });
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("caught operation panic evidence"));
            assert!(!engine.gesture_active());
            assert!(std::fs::read_to_string(std::env::var_os(CHILD).unwrap()).unwrap().contains("operation panicked object=1 instance=1 revision=0 automatic=true starting=false reason=caught operation panic evidence"));
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "plugin_editor::request_panic_tests::rejected_operation_evidence_survives_a_gesture_close_error"])
            .env(CHILD, temp.path().join("events.log")).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn setup() -> (Engine, PluginEditorSession) {
        let package = scene_fixtures::particle_package().unwrap();
        let registry = scene_fixtures::registry().unwrap();
        let mut project = Project::new(64,64,30,120).unwrap();
        let mut layer = crate::Layer::solid(1,"editor",[64.;2],[32.,32.,0.],[0.;4]);
        layer.effects.push(crate::EffectInstance::new(1,&package.manifest.id,&package.manifest.version,&package.hash,&package.manifest.effects[0],[64.;2]));
        project.layers.push(layer);project.rebuild_plugin_dependencies();
        let engine = Engine::new(project).unwrap();
        let editor = PluginEditorSession::open(engine.project(),&registry,1,1).unwrap();
        (engine,editor)
    }
    #[test]
    fn a_panic_after_an_editor_edit_restores_the_whole_page_and_clears_scopes() {
        for page in [false,true] {
            let (mut engine, mut editor) = setup();
            engine.apply(Command::SetColor { object:1,value:[0.2,0.3,0.4,1.] }).unwrap();
            let original = engine.snapshot();
            if page {
                editor.request(&mut engine,0,EditorRequest::Begin { revision:1 }).unwrap();
                let revision = engine.revision();
                editor.request(&mut engine,0,EditorRequest::ColorBegin { revision,param:"color".into() }).unwrap();
            }
            let result = editor.request_boundary(&mut engine,!page,false,|_,engine| {
                engine.apply(Command::SetColor { object:1,value:[0.9,0.8,0.7,1.] })?;
                panic!("injected plugin request failure")
            });
            assert!(result.unwrap_err().to_string().contains("plugin editor request panicked"));
            assert_eq!(engine.snapshot(),original);
            assert!(!engine.gesture_active());
            assert!(!editor.gesture);
            assert!(editor.color_edit.is_none() && editor.parameter_edit.is_none());
            assert!(engine.can_undo());
            assert!(engine.undo().unwrap());
            assert!(engine.redo().unwrap());
            assert_eq!(engine.snapshot(),original);
        }
    }
    #[test]
    fn close_clears_parameter_and_color_state_even_after_host_cancellation() {
        let (mut engine, mut editor) = setup();
        editor.request(&mut engine,0,EditorRequest::Begin { revision:0 }).unwrap();
        let track = engine.project().layers[0].effects[0].params["color"].track.clone();
        editor.color_edit = Some(ColorEdit {param:"color".into(),frame:0,track:track.clone()});
        editor.parameter_edit = Some(ColorEdit {param:"center".into(),frame:0,track});
        engine.end_gesture(false).unwrap();
        editor.close(&mut engine,false).unwrap();
        assert!(!editor.gesture);
        assert!(editor.color_edit.is_none() && editor.parameter_edit.is_none());
    }
}
