//! Effect package, plugin editor and color curve operations.
use crate::session::{next_token, Result, Session};
use motion_core::Command;
use motion_render::Renderer;
use serde_json::{json, Value};
use std::path::PathBuf;

pub(crate) const PLUGIN_EDITOR_LEASE: std::time::Duration = std::time::Duration::from_secs(120);

impl Session {
    pub fn plugin_request(&mut self, request: &str) -> Result<Value> {
        self.recover_plugin_editor(std::time::Instant::now())?;
        if request.len() > 256 * 1024 {
            return Err("plugin request exceeds 256 KiB".into());
        }
        let v: Value = serde_json::from_str(&request).map_err(|e| e.to_string())?;
        if v.get("composition")
            .is_some_and(|v| v.as_str() != Some(self.engine.project().composition_id.as_str()))
        {
            return Err(self.composition_error(
                "context_mismatch",
                "Open the requested composition before plugin access",
                json!({}),
            ));
        }
        if v["op"].as_str().is_some_and(|op| op.starts_with("editor_")) {
            return self.editor_operation(&v);
        }
        if v["op"] == "cancel_plugin_gesture" {
            self.close_plugin_editor(false, "host_cancel")?;
            return Ok(self.snapshot());
        }
        let text = |name: &str| v[name].as_str().ok_or_else(|| format!("missing {name}"));
        // Available through the existing desktop/Android JSON bridge. Query
        // omits policy; writes validate all limits before changing runtime state.
        if text("op")? == "resource_tracking" {
            return Ok(self.configure_resource_tracking(v["enabled"].as_bool().ok_or("missing boolean enabled")?));
        }
        if text("op")? == "resource_ledger" {
            return Ok(self.texture_resource_ledger());
        }
        if text("op")? == "memory_policy" {
            if let Some(policy) = v.get("policy") {
                let policy = serde_json::from_value(policy.clone()).map_err(|error| error.to_string())?;
                return self.configure_memory_policy(policy);
            }
            return Ok(json!({"policyVersion":2,"memoryPolicy":self.effects.memory_policy()}));
        }
        if self.editor.as_ref().is_some_and(|e| e.gesture) && v["op"] != "catalogue" {
            return Err("finish the plugin editor gesture before other plugin operations".into());
        }
        if text("op")?.starts_with("font_") {return self.font_request(&v);}
        let registry = &mut self.effects.registry;
        match text("op")? {
            "image_input" => {
                let object=v["object"].as_u64().ok_or("missing object")?;
                let instance=v["instance"].as_u64().ok_or("missing instance")?;
                let layer=self.engine.project().layers.iter().find(|l|l.id==object).ok_or("layer does not exist")?;
                let e=layer.effects.iter().find(|e|e.id==instance).ok_or("effect instance does not exist")?;
                let p=registry.resolve(&e.plugin,&e.version,&e.hash).map_err(|e|e.to_string())?;
                if !p.manifest.effects.iter().find(|d|d.id==e.effect).is_some_and(|d|d.required_capabilities.iter().any(|c|c=="image_input")) {
                    return Err("effect does not accept an image input".into());
                }
                let input:Option<motion_core::EffectImageInput>=serde_json::from_value(v["input"].clone()).map_err(|e|e.to_string())?;
                self.engine.apply(Command::Effect {object,action:motion_core::EffectAction::SetImageInput {effect:instance,input}}).map_err(|e|e.to_string())?;
                self.sample()?;return Ok(self.snapshot());
            }
            "catalogue" => {
                return Ok(
                    json!({"packages":registry.packages.iter().map(|(key,p)|json!({"manifest":p.manifest,"hash":p.hash,"enabled":!registry.disabled.contains(key)})).collect::<Vec<_>>(),"aliases":motion_effects::builtin::effect_aliases(),"errors":registry.diagnostics}),
                )
            }
            "install" => {
                registry
                    .install(&self.plugin_root, &PathBuf::from(text("path")?))
                    .map_err(|e| e.to_string())?;
            }
            "enable" => registry
                .enable(
                    &self.plugin_root,
                    text("plugin")?,
                    text("version")?,
                    text("hash")?,
                    v["enabled"].as_bool().ok_or("missing enabled")?,
                )
                .map_err(|e| e.to_string())?,
            "uninstall" => registry
                .uninstall(
                    &self.plugin_root,
                    text("plugin")?,
                    text("version")?,
                    text("hash")?,
                )
                .map_err(|e| e.to_string())?,
            "add" | "upgrade" => {
                let p = registry
                    .resolve(text("plugin")?, text("version")?, text("hash")?)
                    .map_err(|e| e.to_string())?;
                let def = p
                    .manifest
                    .effects
                    .iter()
                    .find(|d| Some(d.id.as_str()) == v["effect"].as_str())
                    .ok_or("unknown effect")?;
                let object = v["object"].as_u64().ok_or("missing layer")?;
                let layer = self
                    .engine
                    .project()
                    .layers
                    .iter()
                    .find(|l| l.id == object)
                    .ok_or("layer does not exist")?;
                let upgrading = text("op")? == "upgrade";
                if matches!(layer.content, motion_core::Content::Adjustment)
                    && def.renderer != motion_effects::RendererKind::Image
                {
                    return Err("adjustment layers support image effects only".into());
                }
                let instance = if upgrading {
                    v["instance"].as_u64().ok_or("missing instance")?
                } else {
                    layer.effects.iter().map(|e| e.id).max().unwrap_or(0) + 1
                };
                let index = if upgrading {
                    layer
                        .effects
                        .iter()
                        .position(|e| e.id == instance)
                        .ok_or("instance does not exist")?
                } else {
                    layer.effects.len()
                };
                let mut effect = motion_core::EffectInstance::new(
                    instance,
                    &p.manifest.id,
                    &p.manifest.version,
                    &p.hash,
                    def,
                    if matches!(layer.content, motion_core::Content::Adjustment) {
                        [
                            self.engine.project().width as f32,
                            self.engine.project().height as f32,
                        ]
                    } else {
                        layer.size
                    },
                );
                let preserve = v
                    .get("preserve_parameters")
                    .map(|v| v.as_bool().ok_or("preserve_parameters must be a boolean"))
                    .transpose()?
                    .unwrap_or(false);
                if upgrading && preserve {
                    let old = &layer.effects[index];
                    let old_package = registry
                        .resolve(&old.plugin, &old.version, &old.hash)
                        .map_err(|e| e.to_string())?;
                    let old_def = old_package
                        .manifest
                        .effects
                        .iter()
                        .find(|d| d.id == old.effect)
                        .ok_or("old effect definition is missing")?;
                    if old.plugin != p.manifest.id
                        || old.effect != def.id
                        || old_def.params != def.params
                        || old_def.renderer != def.renderer
                    {
                        return Err("effect parameter contract differs; preserving parameters requires an explicit migration".into());
                    }
                    effect.params = old.params.clone();
                    effect.seed = old.seed;
                    effect.enabled = old.enabled;
                    effect.scene = old.scene.clone();
                    effect.image_input = old.image_input;
                }
                let mut cmds = Vec::new();
                if upgrading {
                    cmds.push(Command::Effect {
                        object,
                        action: motion_core::EffectAction::Remove { effect: instance },
                    });
                }
                cmds.push(Command::Effect {
                    object,
                    action: motion_core::EffectAction::Insert { instance: effect },
                });
                if upgrading {
                    cmds.push(Command::Effect {
                        object,
                        action: motion_core::EffectAction::Move {
                            effect: instance,
                            index,
                        },
                    });
                }
                self.engine.apply_batch(cmds).map_err(|e| e.to_string())?;
                self.sample()?;
                return Ok(self.snapshot());
            }
            _ => return Err("unknown plugin operation".into()),
        }
        self.editor_renderer = None;
        self.editor_target = None;
        let next = registry.clone();
        self.effects.set_registry(next.clone());
        if let Some(g) = &mut self.graphics {
            g.renderer.set_effect_registry(next);
        }
        self.view_revision += 1;
        self.last_presented_frame = None;
        Ok(self.snapshot())
    }

    pub(crate) fn close_plugin_editor(&mut self, commit: bool, reason: &str) -> Result<()> {
        let Some(mut editor) = self.editor.take() else { return Ok(()); };
        let cancelled = editor.gesture && self.engine.gesture_active() && !commit;
        let result = editor.close(&mut self.engine, commit).map_err(|e|e.to_string());
        self.editor_token.clear();
        self.editor_activity = None;
        self.editor_renderer = None;
        self.editor_target = None;
        self.editor_recovery = Some(json!({"reason":reason,"cancelled":cancelled,"revision":self.engine.revision()}));
        if let Err(error) = self.sample() { self.last_error = Some(error); }
        result
    }
    pub(crate) fn recover_plugin_editor(&mut self, now: std::time::Instant) -> Result<()> {
        if self.editor.as_ref().is_some_and(|editor| editor.gesture) && self.editor_activity
            .is_some_and(|last| now.saturating_duration_since(last) >= PLUGIN_EDITOR_LEASE) {
            self.close_plugin_editor(false, "lease_expired")?;
        }
        Ok(())
    }
    fn editor_operation(&mut self, v: &Value) -> Result<Value> {
        let op = v["op"].as_str().ok_or("missing editor operation")?;
        if op == "editor_open" {
            if self.editor.is_some() {
                return Err("close the current plugin editor first".into());
            }
            let object = v["object"].as_u64().ok_or("missing editor layer")?;
            let instance = v["instance"].as_u64().ok_or("missing editor effect")?;
            let editor = motion_core::plugin_editor::PluginEditorSession::open(
                self.engine.project(),
                &self.effects.registry,
                object,
                instance,
            )
            .map_err(|e| e.to_string())?;
            let package = self
                .effects
                .registry
                .resolve(
                    &editor.dependency.plugin,
                    &editor.dependency.version,
                    &editor.dependency.hash,
                )
                .map_err(|e| e.to_string())?;
            let definition = package
                .manifest
                .effects
                .iter()
                .find(|d| d.id == editor.effect)
                .ok_or("editor definition missing")?;
            let state = editor
                .state(&self.engine, self.frame.floor() as u32)
                .map_err(|e| e.to_string())?;
            self.editor_token = format!("editor-{}-{}", next_token(), self.engine.revision());
            self.editor = Some(editor);
            self.editor_activity = Some(std::time::Instant::now());
            self.editor_recovery = None;
            return Ok(json!({"protocol":1,"token":self.editor_token,"definition":definition,"state":state}));
        }
        if self.editor.is_none() || v["token"].as_str() != Some(self.editor_token.as_str()) {
            return Err("plugin editor session is stale or missing".into());
        }
        if op == "editor_close" || op == "editor_disconnected" {
            self.close_plugin_editor(op == "editor_close" && v["commit"].as_bool().unwrap_or(false),
                if op == "editor_disconnected" { "transport_disconnected" } else { "page_closed" })?;
            return Ok(self.snapshot());
        }
        if op == "editor_heartbeat" {
            self.editor_activity = Some(std::time::Instant::now());
            return Ok(json!({"leaseMillis":PLUGIN_EDITOR_LEASE.as_millis(),"gesture":self.editor.as_ref().is_some_and(|e|e.gesture)}));
        }
        let editor = self.editor.as_ref().unwrap();
        let package = self
            .effects
            .registry
            .resolve(
                &editor.dependency.plugin,
                &editor.dependency.version,
                &editor.dependency.hash,
            )
            .map_err(|e| e.to_string())?;
        match op {
            "editor_asset" => {
                let path = v["path"].as_str().ok_or("missing editor asset path")?;
                let definition = package
                    .manifest
                    .effects
                    .iter()
                    .find(|d| d.id == editor.effect)
                    .and_then(|d| d.editor.as_ref())
                    .ok_or("editor definition missing")?;
                if !definition.files.iter().any(|p| p == path) {
                    return Err("asset is outside this plugin editor".into());
                }
                let bytes = package.files.get(path).ok_or("editor asset missing")?;
                self.editor_activity = Some(std::time::Instant::now());
                Ok(json!({"mime":motion_effects::editor_mime(path),"base64":crate::encode_base64(bytes)}))
            }
            "editor_message" => {
                if v["message"]["op"] == "preview" {
                    let result = self.editor_preview(&v["message"]);
                    if result.is_ok() { self.editor_activity = Some(std::time::Instant::now()); }
                    return result;
                }
                let request: motion_core::plugin_editor::EditorRequest =
                    serde_json::from_value(v["message"].clone()).map_err(|e| e.to_string())?;
                let was_gesture = self.editor.as_ref().unwrap().gesture;
                let result = self.editor.as_mut().unwrap().request(&mut self.engine, self.frame.floor() as u32, request);
                let mut state = match result {
                    Ok(state) => state,
                    Err(error) => {
                        if error.to_string().starts_with("plugin editor request panicked:") {
                            self.close_plugin_editor(false, "request_panicked")?;
                            if let Some(recovery) = &mut self.editor_recovery { recovery["cancelled"] = json!(was_gesture); }
                        }
                        return Err(error.to_string());
                    }
                };
                self.editor_activity = Some(std::time::Instant::now());
                if let Err(error) = self.sample() {
                    state["render_error"] = json!(error);
                }
                Ok(state)
            }
            _ => Err("unknown editor operation".into()),
        }
    }

    fn editor_preview(&mut self, message: &Value) -> Result<Value> {
        self.editor
            .as_ref()
            .ok_or("editor session missing")?
            .state(&self.engine, self.frame.floor() as u32)
            .map_err(|e| e.to_string())?;
        let width = message["width"].as_u64().ok_or("missing preview width")?;
        let height = message["height"].as_u64().ok_or("missing preview height")?;
        if !(1..=512).contains(&width) || !(1..=512).contains(&height) {
            return Err("editor preview must be 1..512 pixels per dimension".into());
        }
        self.sample()?;
        if self.editor_renderer.is_none() {
            let mut renderer =
                pollster::block_on(Renderer::headless()).map_err(|e| e.to_string())?;
            renderer.set_effect_registry(self.effects.registry.clone());
            renderer
                .set_memory_policy(self.effects.memory_policy())
                .map_err(|e| e.to_string())?;
            renderer
                .configure_assets(self.engine.project(), &self.root)
                .map_err(|e| e.to_string())?;
            renderer.set_submission_tracking(self.resource_tracking || self.recorder.is_some());
            self.editor_renderer = Some(renderer);
        }
        let renderer = self.editor_renderer.as_mut().unwrap();
        renderer
            .prepare_scene_assets(
                &self.scene,
                motion_render::image_resources::Resolution::Preview(1024),
                false,
            )
            .map_err(|e| e.to_string())?;
        renderer
            .preflight_effects(&self.scene, width as u32, height as u32)
            .map_err(|e| e.to_string())?;
        if self
            .editor_target
            .as_ref()
            .is_none_or(|t| t.width != width as u32 || t.height != height as u32)
        {
            self.editor_target = Some(
                renderer
                    .capture_target(width as u32, height as u32)
                    .map_err(|e| e.to_string())?,
            );
        }
        let (pixels, stats) = renderer
            .capture(&self.scene, self.editor_target.as_ref().unwrap())
            .map_err(|e| e.to_string())?;
        let mut png = Vec::new();
        image::ImageEncoder::write_image(
            image::codecs::png::PngEncoder::new(&mut png),
            &pixels,
            width as u32,
            height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
        Ok(json!({"width":width,"height":height,"png":crate::encode_base64(&png),
            "revision":self.engine.revision(),"frame":self.frame,
            "instances":{"alive":stats.particles_alive,"visible":stats.particles_visible,
                "culled":stats.particles_culled,"upload_bytes":stats.instance_upload_bytes}}))
    }
}

pub fn plugin(id: i64, request: &str) -> Result<Value> {
    crate::session::with_session(id, |s| s.plugin_request(request))
}

pub fn plugin_pixels(id: i64, program: i32, resource: i32) -> Result<Vec<u8>> {
    crate::session::with_session(id, |s| {
        let program = s
            .effects
            .programs
            .get(program as usize)
            .ok_or("unknown program")?;
        let path = program
            .resources
            .get(resource as usize)
            .ok_or("unknown resource")?;
        let package = program.package.as_ref().ok_or("program has no resources")?;
        image::load_from_memory(&package.files[path])
            .map(|v| v.into_rgba8().into_raw())
            .map_err(|e| e.to_string())
    })
}

/// Sample a five-channel colour curve object into its graph.
pub fn color_curve_graph(text: &str) -> Result<Value> {
    let value: motion_core::CurveObject = serde_json::from_str(text).map_err(|e| e.to_string())?;
    value.validate().map_err(|e| e.to_string())?;
    Ok(value.graph())
}

#[cfg(test)]
#[path = "effects_recovery_tests.rs"]
mod recovery_tests;
