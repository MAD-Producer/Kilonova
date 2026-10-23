//! Commands, gestures and temporal curve operations.
use crate::session::{Result, Session};
use motion_core::Command;
use serde_json::{json, Value};

// The model rolls back before returning a failed preparation. Keep the host's
// native error separately rather than flattening it to the model callback type.
fn apply_prepared(
    engine: &mut motion_core::Engine,
    commands: Vec<Command>,
    prepare: impl FnOnce(&motion_core::Project) -> Result<()>,
) -> Result<Vec<motion_core::EditResult>> {
    let mut primary = None;
    engine.apply_batch_with_prepare(commands, |project| {
        prepare(project).map_err(|error| {
            let bridge = motion_core::Error::Invalid(error.to_string());
            primary = Some(error);
            bridge
        })
    }).map_err(|error| primary.unwrap_or_else(|| error.into()))
}

impl Session {
    /// Apply a batch of commands with asset validation and one-step undo.
    pub fn apply_commands(&mut self, text: &str) -> Result<Value> {
        let commands = motion_core::parse_commands(text).map_err(crate::Error::from)?;
        self.apply_command_batch(commands)
    }
    pub fn apply_command_batch(&mut self, commands: Vec<Command>) -> Result<Value> {
        let resources = commands.iter().any(Command::changes_resources);
        if self.editor.as_ref().is_some_and(|e| e.gesture) {
            return Err("finish the plugin editor gesture before ordinary edits".into());
        }
        let results = apply_prepared(&mut self.engine, commands, |project| {
                if resources {
                    motion_core::storage::validate_assets(&self.root, project)?;
                    if let Some(g) = &mut self.graphics {
                        // configure_assets resolves every source before changing
                        // its catalog/caches, so Err leaves the old renderer intact.
                        g.renderer.configure_assets(project, &self.root)
                            .map_err(crate::Error::from)?;
                    }
                }
                Ok(())
            })?;
        if resources {
            self.effects.alpha_images.clear();
            self.editor_renderer = None;
            self.editor_target = None;
        }
        // The edit has committed. A frame-specific expression failure is reported
        // in renderError without pretending the stored edit failed or losing it.
        if let Err(error) = self.sample() {
            self.last_error = Some(error.to_string());
        }
        let mut snapshot = self.snapshot()?;
        if let Some(result) = results.last() {
            snapshot["edit_result"] = serde_json::to_value(result).map_err(crate::Error::from)?;
            snapshot["edit_results"] =
                serde_json::to_value(&results).map_err(crate::Error::from)?;
        }
        Ok(snapshot)
    }
}

pub fn command(id: i64, text: &str) -> Result<Value> {
    crate::session::with_session(id, |s| s.apply_commands(text))
}

pub fn drag(
    id: i64,
    object: i64,
    dx: f64,
    dy: f64,
    width: i32,
    height: i32,
) -> Result<Value> {
    crate::session::with_session(id, |s| {
        if width <= 0 || height <= 0 || !dx.is_finite() || !dy.is_finite() {
            return Err("invalid preview drag".into());
        }
        s.sample()?;
        let layer = s
            .engine
            .project()
            .layers
            .iter()
            .find(|l| l.id == object as u64)
            .ok_or("drag layer does not exist")?;
        if !layer.active(s.frame, s.engine.project().frames) {
            return Err("drag layer is outside its clip".into());
        }
        let position = layer.transform.position.sample(layer.local_frame(s.frame))?;
        let separated = layer.transform.position.axes.is_some();
        let world_position = s
            .scene
            .node_position(object as u64)
            .ok_or("object transform is unavailable")?;
        let offset = if layer.three_d {
            s.scene
                .screen_translation(
                    world_position,
                    [dx as f32, dy as f32],
                    [width as u32, height as u32],
                )
                .map_err(crate::Error::from)?
        } else {
            let p = s.engine.project();
            let scale = (width as f32 / p.width as f32).min(height as f32 / p.height as f32);
            [dx as f32 / scale, dy as f32 / scale, 0.0]
        };
        let offset =
            motion_core::scene_prefix_delta(s.engine.project(), object as u64, s.frame, offset)
                .map_err(crate::Error::from)?;
        let frame = s.frame.floor() as u32;
        let commands = if separated {
            [motion_core::Axis::X, motion_core::Axis::Y, motion_core::Axis::Z]
                .into_iter()
                .enumerate()
                .filter(|(i, _)| offset[*i].abs() > 1e-6)
                .map(|(i, axis)| Command::SetComponent {
                    object: object as u64,
                    property: motion_core::Property::Position,
                    axis,
                    frame,
                    value: position[i] + offset[i],
                })
                .collect()
        } else {
            vec![Command::SetVector {
                object: object as u64,
                property: motion_core::Property::Position,
                frame,
                value: std::array::from_fn(|i| position[i] + offset[i]),
            }]
        };
        s.engine
            .apply_batch(commands)
            .map_err(crate::Error::from)?;
        s.sample()?;
        s.snapshot()
    })
}

/// Undo/redo and gesture bracketing. Operations are stable across hosts.
pub const HISTORY_UNDO: i32 = 0;
pub const HISTORY_REDO: i32 = 1;
pub const HISTORY_BEGIN: i32 = 2;
pub const HISTORY_COMMIT: i32 = 3;
pub const HISTORY_CANCEL: i32 = 4;

pub fn history(id: i64, op: i32) -> Result<Value> {
    crate::session::with_session(id, |s| {
        let active = s.engine.project().composition_id.clone();
        match op {
            HISTORY_UNDO => {
                s.engine.undo().map_err(crate::Error::from)?;
            }
            HISTORY_REDO => {
                s.engine.redo().map_err(crate::Error::from)?;
            }
            HISTORY_BEGIN => s.engine.begin_gesture().map_err(crate::Error::from)?,
            HISTORY_COMMIT if s.editor.as_ref().is_some_and(|e|e.gesture) => s.close_plugin_editor(true, "host_commit")?,
            HISTORY_CANCEL if s.editor.as_ref().is_some_and(|e|e.gesture) => s.close_plugin_editor(false, "host_cancel")?,
            HISTORY_COMMIT => s.engine.end_gesture(true).map_err(crate::Error::from)?,
            HISTORY_CANCEL => s.engine.end_gesture(false).map_err(crate::Error::from)?,
            _ => return Err("invalid history operation".into()),
        }
        if !s.engine.gesture_active() {
            let target = if s.engine.project().composition_ids().contains(&active) {
                active.as_str()
            } else {
                motion_core::MAIN_COMPOSITION
            };
            s.engine
                .activate_composition(target)
                .map_err(crate::Error::from)?;
        }
        s.frame = s.frame.min(f64::from(s.engine.project().frames - 1));
        s.audio_mixer = None;
        s.video_frames.clear();
        if matches!(op, HISTORY_UNDO | HISTORY_REDO | HISTORY_CANCEL) {
            s.effects.alpha_images.clear();
            s.editor_renderer = None;
            s.editor_target = None;
        }
        if let Some(g) = &mut s.graphics {
            g.renderer
                .configure_assets(s.engine.project(), &s.root)
                .map_err(crate::Error::from)?;
        }
        if let Err(error) = s.sample() {
            s.last_error = Some(error.to_string());
        }
        s.snapshot()
    })
}

/// Sample an easing into graph points without touching a session.
pub fn curve_graph(text: &str) -> Result<Value> {
    let easing: motion_core::Easing = serde_json::from_str(text).map_err(crate::Error::from)?;
    easing.validate().map_err(crate::Error::from)?;
    let points: Vec<_> = (0..=160).map(|i| easing.sample(i as f64 / 160.0)).collect();
    Ok(json!({"points": points,
        "definitionScale": easing.curve.map_or(1.0, |c| c.definition_scale())}))
}

#[cfg(test)]
mod preparation_failure_tests {
    use super::*;

    #[test]
    #[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
    fn native_renderer_prepare_error_survives_model_rollback_and_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let mut engine = motion_core::Engine::new(
            motion_core::Project::new(64, 64, 30, 60).unwrap()).unwrap();
        let before = engine.snapshot();
        let history = engine.history_diagnostics();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        renderer.configure_assets(engine.project(), directory.path()).unwrap();
        let commands = || vec![Command::RegisterAsset { asset: motion_core::Asset {
            id: 1, path: "assets/new.png".into(), width: 16, height: 16,
        }}];
        let error = apply_prepared(&mut engine, commands(), |project| {
            renderer.configure_assets(project, directory.path()).map_err(crate::Error::from)
        }).unwrap_err();
        assert!(matches!(&error,
            crate::Error::Render(motion_render::RenderError::ImageResource(
                motion_render::image_resources::Error::Io(cause)))
                if cause.kind() == std::io::ErrorKind::NotFound),
            "renderer error lost its identity at model callback: {error:?}");
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.revision(), 0);
        assert_eq!(engine.history_diagnostics(), history);
        assert!(!engine.can_undo());
        assert_eq!(renderer.asset_ids(), &[0]);

        std::fs::create_dir(directory.path().join("assets")).unwrap();
        image::DynamicImage::new_rgba8(16, 16)
            .save(directory.path().join("assets/new.png")).unwrap();
        apply_prepared(&mut engine, commands(), |project| {
            renderer.configure_assets(project, directory.path()).map_err(crate::Error::from)
        }).unwrap();
        assert_eq!(engine.revision(), 1);
        assert!(engine.can_undo());
        assert_eq!(renderer.asset_ids(), &[0, 1]);
        assert!(engine.undo().unwrap());
        assert_eq!(engine.snapshot(), before);
    }
}
