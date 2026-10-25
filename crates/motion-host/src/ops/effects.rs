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
        let v: Value = serde_json::from_str(&request).map_err(crate::Error::from)?;
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
            return self.snapshot();
        }
        let text = |name: &str| v[name].as_str().ok_or_else(|| format!("missing {name}"));
        if text("op")? == "memory_pressure" {
            return Ok(json!({"memoryPressure":self.memory_pressure_info()}));
        }
        // Available through the existing desktop/Android JSON bridge. Query
        // omits policy; writes validate all limits before changing runtime state.
        if text("op")? == "resource_tracking" {
            return Ok(self.configure_resource_tracking(
                v["enabled"].as_bool().ok_or("missing boolean enabled")?,
            ));
        }
        if text("op")? == "resource_ledger" {
            return Ok(self.texture_resource_ledger());
        }
        if text("op")? == "resource_memory" {
            return self.resource_memory_ledger();
        }
        if text("op")? == "allocation_budget" {
            return Ok(json!({"version":1,"scope":"session_enrolled_native_capacity",
                "allocationBudget":self.allocation_budget_info()?,
                "isFramePeak":false,"isDriverResidency":false,"completeCoverage":false}));
        }
        if text("op")? == "memory_policy" {
            if let Some(policy) = v.get("policy") {
                let policy =
                    serde_json::from_value(policy.clone()).map_err(crate::Error::from)?;
                return self.configure_memory_policy(policy);
            }
            return Ok(json!({"policyVersion":motion_render::resource_policy::MEMORY_POLICY_VERSION,"memoryPolicy":self.effects.memory_policy()}));
        }
        if self.editor.as_ref().is_some_and(|e| e.gesture) && v["op"] != "catalogue" {
            return Err("finish the plugin editor gesture before other plugin operations".into());
        }
        if text("op")?.starts_with("font_") {
            return self.font_request(&v);
        }
        let package_limits = motion_effects::PackageReadLimits::new(
            self.effects.memory_policy().decoded_image_budget_bytes,
        )?;
        let registry = &mut self.effects.registry;
        match text("op")? {
            "image_input" => {
                let object = v["object"].as_u64().ok_or("missing object")?;
                let instance = v["instance"].as_u64().ok_or("missing instance")?;
                let layer = self
                    .engine
                    .project()
                    .layers
                    .iter()
                    .find(|l| l.id == object)
                    .ok_or("layer does not exist")?;
                let e = layer
                    .effects
                    .iter()
                    .find(|e| e.id == instance)
                    .ok_or("effect instance does not exist")?;
                let p = registry
                    .resolve(&e.plugin, &e.version, &e.hash)
                    .map_err(crate::Error::from)?;
                if !p
                    .manifest
                    .effects
                    .iter()
                    .find(|d| d.id == e.effect)
                    .is_some_and(|d| d.required_capabilities.iter().any(|c| c == "image_input"))
                {
                    return Err("effect does not accept an image input".into());
                }
                let input: Option<motion_core::EffectImageInput> =
                    serde_json::from_value(v["input"].clone()).map_err(crate::Error::from)?;
                self.engine
                    .apply(Command::Effect {
                        object,
                        action: motion_core::EffectAction::SetImageInput {
                            effect: instance,
                            input,
                        },
                    })
                    .map_err(crate::Error::from)?;
                self.sample()?;
                return self.snapshot();
            }
            "catalogue" => {
                return Ok(
                    json!({"packages":registry.packages.iter().map(|(key,p)|json!({"manifest":p.manifest,"hash":p.hash,"enabled":!registry.disabled.contains(key)})).collect::<Vec<_>>(),"aliases":motion_effects::builtin::effect_aliases(),"errors":registry.diagnostics}),
                )
            }
            "install" => {
                registry
                    .install_with_limits(&self.plugin_root, &PathBuf::from(text("path")?), package_limits)
                    .map_err(crate::Error::from)?;
            }
            "enable" => registry
                .enable(
                    &self.plugin_root,
                    text("plugin")?,
                    text("version")?,
                    text("hash")?,
                    v["enabled"].as_bool().ok_or("missing enabled")?,
                )
                .map_err(crate::Error::from)?,
            "uninstall" => registry
                .uninstall(
                    &self.plugin_root,
                    text("plugin")?,
                    text("version")?,
                    text("hash")?,
                )
                .map_err(crate::Error::from)?,
            "add" | "upgrade" => {
                let p = registry
                    .resolve(text("plugin")?, text("version")?, text("hash")?)
                    .map_err(crate::Error::from)?;
                let def = p
                    .manifest
                    .effects
                    .iter()
                    .find(|d| Some(d.id.as_str()) == v["effect"].as_str())
                    .ok_or("unknown effect")?;
                let objects = if let Some(values) = v.get("objects") {
                    if text("op")? != "add" || v.get("object").is_some() {
                        return Err("objects is supported only for add, without object".into());
                    }
                    values
                        .as_array()
                        .ok_or("objects must be an array")?
                        .iter()
                        .map(|value| {
                            value
                                .as_u64()
                                .ok_or_else(|| crate::Error::from("objects must contain layer ids"))
                        })
                        .collect::<Result<Vec<_>>>()?
                } else {
                    vec![v["object"].as_u64().ok_or("missing layer")?]
                };
                if objects.is_empty() {
                    return Err("at least one layer is required".into());
                }
                let mut seen = std::collections::HashSet::new();
                let objects = objects
                    .into_iter()
                    .filter(|id| seen.insert(*id))
                    .collect::<Vec<_>>();
                let upgrading = text("op")? == "upgrade";
                let preserve = v
                    .get("preserve_parameters")
                    .map(|v| v.as_bool().ok_or("preserve_parameters must be a boolean"))
                    .transpose()?
                    .unwrap_or(false);
                let mut cmds = Vec::new();
                for object in objects {
                    let layer = self
                        .engine
                        .project()
                        .layers
                        .iter()
                        .find(|l| l.id == object)
                        .ok_or("layer does not exist")?;
                    if layer.locked {
                        return Err("object is locked".into());
                    }
                    if matches!(layer.content, motion_core::Content::Audio { .. }) {
                        return Err("audio layers do not support visual effects".into());
                    }
                    if matches!(layer.content, motion_core::Content::Adjustment)
                        && def.renderer != motion_effects::RendererKind::Image
                    {
                        return Err("adjustment layers support image effects only".into());
                    }
                    let instance = if upgrading {
                        v["instance"].as_u64().ok_or("missing instance")?
                    } else {
                        layer
                            .effects
                            .iter()
                            .map(|e| e.id)
                            .max()
                            .unwrap_or(0)
                            .checked_add(1)
                            .ok_or("effect ID space exhausted")?
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
                    if upgrading && preserve {
                        let old = &layer.effects[index];
                        let old_package = registry
                            .resolve(&old.plugin, &old.version, &old.hash)
                            .map_err(crate::Error::from)?;
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
                }
                self.engine.apply_batch(cmds).map_err(crate::Error::from)?;
                self.sample()?;
                return self.snapshot();
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
        self.snapshot()
    }

    pub(crate) fn close_plugin_editor(&mut self, commit: bool, reason: &str) -> Result<()> {
        let Some(mut editor) = self.editor.take() else {
            return Ok(());
        };
        let cancelled = editor.gesture && self.engine.gesture_active() && !commit;
        let result = editor
            .close(&mut self.engine, commit)
            .map_err(crate::Error::from);
        self.editor_token.clear();
        self.editor_activity = None;
        self.editor_renderer = None;
        self.editor_target = None;
        self.editor_recovery =
            Some(json!({"reason":reason,"cancelled":cancelled,"revision":self.engine.revision()}));
        if let Err(error) = self.sample() {
            self.last_error = Some(error.to_string());
        }
        result
    }
    pub(crate) fn recover_plugin_editor(&mut self, now: std::time::Instant) -> Result<()> {
        if self.editor.as_ref().is_some_and(|editor| editor.gesture)
            && self
                .editor_activity
                .is_some_and(|last| now.saturating_duration_since(last) >= PLUGIN_EDITOR_LEASE)
        {
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
            .map_err(crate::Error::from)?;
            let package = self
                .effects
                .registry
                .resolve(
                    &editor.dependency.plugin,
                    &editor.dependency.version,
                    &editor.dependency.hash,
                )
                .map_err(crate::Error::from)?;
            let definition = package
                .manifest
                .effects
                .iter()
                .find(|d| d.id == editor.effect)
                .ok_or("editor definition missing")?;
            let state = editor
                .state(&self.engine, self.frame.floor() as u32)
                .map_err(crate::Error::from)?;
            self.editor_token = format!("editor-{}-{}", next_token(), self.engine.revision());
            self.editor = Some(editor);
            self.editor_activity = Some(std::time::Instant::now());
            self.editor_recovery = None;
            return Ok(
                json!({"protocol":1,"token":self.editor_token,"definition":definition,"state":state}),
            );
        }
        if self.editor.is_none() || v["token"].as_str() != Some(self.editor_token.as_str()) {
            return Err("plugin editor session is stale or missing".into());
        }
        if op == "editor_close" || op == "editor_disconnected" {
            self.close_plugin_editor(
                op == "editor_close" && v["commit"].as_bool().unwrap_or(false),
                if op == "editor_disconnected" {
                    "transport_disconnected"
                } else {
                    "page_closed"
                },
            )?;
            return self.snapshot();
        }
        if op == "editor_heartbeat" {
            self.editor_activity = Some(std::time::Instant::now());
            return Ok(
                json!({"leaseMillis":PLUGIN_EDITOR_LEASE.as_millis(),"gesture":self.editor.as_ref().is_some_and(|e|e.gesture)}),
            );
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
            .map_err(crate::Error::from)?;
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
                Ok(
                    json!({"mime":motion_effects::editor_mime(path),"base64":crate::encode_base64(bytes)}),
                )
            }
            "editor_message" => {
                if v["message"]["op"] == "preview" {
                    let result = self.editor_preview(&v["message"]);
                    if result.is_ok() {
                        self.editor_activity = Some(std::time::Instant::now());
                    }
                    return result;
                }
                let request: motion_core::plugin_editor::EditorRequest =
                    serde_json::from_value(v["message"].clone()).map_err(crate::Error::from)?;
                let was_gesture = self.editor.as_ref().unwrap().gesture;
                let result = self.editor.as_mut().unwrap().request(
                    &mut self.engine,
                    self.frame.floor() as u32,
                    request,
                );
                let mut state = match result {
                    Ok(state) => state,
                    Err(error) => {
                        if error
                            .to_string()
                            .starts_with("plugin editor request panicked:")
                        {
                            self.close_plugin_editor(false, "request_panicked")?;
                            if let Some(recovery) = &mut self.editor_recovery {
                                recovery["cancelled"] = json!(was_gesture);
                            }
                        }
                        return Err(error.into());
                    }
                };
                self.editor_activity = Some(std::time::Instant::now());
                if let Err(error) = self.sample() {
                    state["render_error"] = json!(error.to_string());
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
            .map_err(crate::Error::from)?;
        let width = message["width"].as_u64().ok_or("missing preview width")?;
        let height = message["height"].as_u64().ok_or("missing preview height")?;
        if !(1..=512).contains(&width) || !(1..=512).contains(&height) {
            return Err("editor preview must be 1..512 pixels per dimension".into());
        }
        self.sample()?;
        let precision = self.engine.project().working_precision;
        if self.editor_renderer.is_none() {
            let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
                precision, self.allocation_budget().clone(),
            )).map_err(crate::Error::from)?;
            renderer.set_effect_registry(self.effects.registry.clone());
            renderer
                .set_memory_policy(self.effects.memory_policy())
                .map_err(crate::Error::from)?;
            renderer
                .configure_assets(self.engine.project(), &self.root)
                .map_err(crate::Error::from)?;
            renderer.set_submission_tracking(self.resource_tracking || self.recorder.is_some());
            self.editor_renderer = Some(renderer);
        }
        let renderer = self.editor_renderer.as_mut().unwrap();
        if let Some(change) = renderer.prepare_working_precision(precision).map_err(crate::Error::from)? {
            renderer.commit_working_precision(change).map_err(crate::Error::from)?;
        }
        renderer
            .prepare_scene_assets(
                &self.scene,
                motion_render::image_resources::Resolution::Preview(1024),
                false,
            )
            .map_err(crate::Error::from)?;
        renderer
            .preflight_effects(&self.scene, width as u32, height as u32)
            .map_err(crate::Error::from)?;
        if self
            .editor_target
            .as_ref()
            .is_none_or(|t| t.width() != width as u32 || t.height() != height as u32
                || t.working_format() != renderer.target_format)
        {
            self.editor_target = Some(
                renderer
                    .sdr_capture_target(width as u32, height as u32, motion_render::OutputTransform::SDR_CLIP)
                    .map_err(crate::Error::from)?,
            );
        }
        let (image, stats) = renderer
            .capture_sdr(&self.scene, self.editor_target.as_ref().unwrap())
            .map_err(crate::Error::from)?;
        let mut png = Vec::new();
        image::ImageEncoder::write_image(
            image::codecs::png::PngEncoder::new(&mut png),
            &image.pixels,
            width as u32,
            height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(crate::Error::from)?;
        Ok(
            json!({"width":width,"height":height,"png":crate::encode_base64(&png),
            "revision":self.engine.revision(),"frame":self.frame,
            "instances":{"alive":stats.particles_alive,"visible":stats.particles_visible,
                "culled":stats.particles_culled,"upload_bytes":stats.instance_upload_bytes}}),
        )
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
        motion_render::image_resources::decode_png_with_budget(
            &package.files[path], s.effects.memory_policy().decoded_image_budget_bytes,
        )
            .map(|v| v.into_raw())
            .map_err(crate::Error::from)
    })
}

/// Sample a five-channel colour curve object into its graph.
pub fn color_curve_graph(text: &str) -> Result<Value> {
    let value: motion_core::CurveObject = serde_json::from_str(text).map_err(crate::Error::from)?;
    value.validate().map_err(crate::Error::from)?;
    value.graph().map_err(crate::Error::from)
}

#[cfg(test)]
#[path = "effects_recovery_tests.rs"]
mod recovery_tests;
