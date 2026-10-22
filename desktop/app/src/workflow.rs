//! Desktop file and layer operations use the shared session's atomic history.
use motion_core::{Asset, Command, Content, Layer, LayerTimeline};
use motion_host::Session;
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn import_image(session: &mut Session, source: &Path) -> motion_host::Result<Value> {
    let source_info = motion_host::ops::images::image_info(&source.to_string_lossy())?;
    let width = source_info["width"].as_u64().ok_or("image width missing")? as u32;
    let height = source_info["height"]
        .as_u64()
        .ok_or("image height missing")? as u32;
    let maximum = source_info["maxEncodedBytes"]
        .as_u64()
        .ok_or("image limit missing")?;
    let project = session.engine.project();
    let asset_id = project
        .assets
        .iter()
        .map(|a| a.id)
        .chain(project.audio_assets.iter().map(|a| a.id))
        .chain(project.video_assets.iter().map(|a| a.id))
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("asset IDs exhausted")?;
    let object = next_layer_id(&session.snapshot()?)?;
    let frame = (session.frame.floor() as u32).min(project.frames - 1);
    let position = [project.width as f32 / 2.0, project.height as f32 / 2.0, 0.0];
    let end = project.frames;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let ext = if source_info["format"] == "Png" {
        "png"
    } else {
        "jpg"
    };
    let relative = format!("assets/image-{asset_id}-{stamp}.{ext}");
    let folder = session.root.join("assets");
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    if !folder
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(session.root.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err("asset folder resolves outside project".into());
    }
    let destination = session.root.join(&relative);
    let mut input = fs::File::open(source)
        .map_err(|e| e.to_string())?
        .take(maximum + 1);
    let mut output = crate::owned_file::OwnedFile::create(&destination)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let copied = std::io::copy(&mut input, output.file()).map_err(|e| e.to_string())?;
        output.file().flush().map_err(|e| e.to_string())?;
        if copied > maximum {
            return Err("image exceeds source byte budget".into());
        }
        output.close();
        let validated =
            motion_host::ops::images::prepare_image(session.root.clone(), relative.clone())?;
        if validated["width"] != source_info["width"]
            || validated["height"] != source_info["height"]
        {
            return Err("image changed during import".into());
        }
        let name = source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Image");
        let mut layer = Layer::solid(
            object,
            name,
            [width as f32, height as f32],
            position,
            [1.; 4],
        );
        layer.content = Content::Image { asset: asset_id };
        layer.timeline = Some(LayerTimeline {
            in_frame: frame as i32,
            out_frame: end as i32,
            offset_frame: frame as i32,
        });
        session.apply_command_batch(vec![
            Command::RegisterAsset {
                asset: Asset {
                    id: asset_id,
                    path: relative,
                    width,
                    height,
                },
            },
            Command::Add { layer },
        ])
    })();
    if result.is_ok() {
        output.keep();
    }
    result
}

pub fn next_layer_id(state: &Value) -> Result<u64, String> {
    state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["id"].as_u64())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("layer IDs exhausted".into())
}

pub fn create_text(
    session: &mut Session,
    text: String,
    size: f32,
    width: u32,
    font: Option<(std::path::PathBuf, u32)>,
) -> motion_host::Result<Value> {
    session.engine.begin_gesture().map_err(|e| e.to_string())?;
    let result = (|| {
        let font_id = if let Some((path, index)) = font {
            session.plugin_request(&serde_json::json!({"op":"font_import","path":path,"face_index":index,"license":"User-selected system or imported font; original licensing applies"}).to_string())?["font"]["id"].as_str().ok_or("font ID missing")?.to_string()
        } else {
            motion_media::fonts::FontStore::builtin_id()
        };
        let raster=session.plugin_request(&serde_json::json!({"op":"font_text","font":font_id,"text":text,"size":size,"width":width}).to_string())?;
        let asset =
            serde_json::from_value::<Asset>(raster["asset"].clone()).map_err(|e| e.to_string())?;
        let id = next_layer_id(&session.snapshot()?)?;
        let project = session.engine.project();
        let mut layer = Layer::solid(
            id,
            &text.chars().take(32).collect::<String>(),
            [asset.width as f32, asset.height as f32],
            [project.width as f32 / 2., project.height as f32 / 2., 0.],
            [1.; 4],
        );
        layer.content = Content::Text {
            text,
            font: font_id,
            color: [1.; 4],
            raster_asset: asset.id,
        };
        session.apply_command_batch(vec![Command::Add { layer }])?;
        Ok::<_, motion_host::Error>(())
    })();
    if let Err(error) = &result {
        log::warn!("create text operation failed revision={} size={size} width={width} reason={error}", session.engine.revision());
    }
    session
        .engine
        .end_gesture(result.is_ok())
        .map_err(|e| {
            log::error!("create text gesture close failed revision={} operation_error={:?} reason={e}", session.engine.revision(), result.as_ref().err());
            e.to_string()
        })?;
    result?;
    session.sample()?;
    session.snapshot()
}
pub fn null_layer(state: &Value, name: &str) -> Result<Command, String> {
    let id = next_layer_id(state)?;
    let mut layer = Layer::solid(
        id,
        name,
        [100., 100.],
        [
            state["project"]["width"].as_f64().unwrap_or(1920.) as f32 / 2.,
            state["project"]["height"].as_f64().unwrap_or(1080.) as f32 / 2.,
            0.,
        ],
        [1.; 4],
    );
    layer.content = Content::Null;
    Ok(Command::Add { layer })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    #[test]
    fn text_import_error_keeps_original_reason_and_closes_gesture() {
        const CHILD: &str = "MOTION_TEXT_ERROR_OBSERVATION_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.join("logs")).unwrap();
            let engine = Engine::start(root.join("project"), motion_desktop_media::platform(), 8 << 30).unwrap();
            let original = engine.state().unwrap()["project"].clone();
            let reason = engine.create_text("failure".into(), 48., 320, Some((root.join("missing-font.ttf"), 0))).unwrap_err();
            let state = engine.state().unwrap();
            assert_eq!(state["project"], original);
            assert_eq!(state["history"]["gestureActive"], false);
            assert!(state["history"]["gestureElapsedMs"].is_null());
            let log = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
            let event = log.lines().find(|line|line.contains("create text operation failed")).unwrap();
            assert!(event.contains(&reason), "{event}\n{reason}");
            assert!(event.contains("revision=0 size=48 width=320"));
            assert!(!event.contains("gesture close failed"));
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "workflow::tests::text_import_error_keeps_original_reason_and_closes_gesture"])
            .env_remove("MOTION_LOG").env(CHILD, temp.path()).output().unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    }
    #[test]
    fn text_creation_uses_shared_fonts_and_rolls_back_as_one_command_group() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Engine::start(
            temp.path().join("text"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let original = engine.state().unwrap()["project"].clone();
        let state = engine
            .create_text("Shared text".into(), 48., 320, None)
            .unwrap();
        assert_eq!(state["project"]["layers"][0]["content"]["kind"], "text");
        assert_eq!(state["project"]["fonts"].as_array().unwrap().len(), 1);
        assert_eq!(state["project"]["assets"].as_array().unwrap().len(), 1);
        engine.history(0).unwrap();
        assert_eq!(engine.state().unwrap()["project"], original);
        engine.history(1).unwrap();
        engine.save().unwrap();
    }
    #[test]
    fn image_import_owns_source_and_preserves_pixels_history_and_packages() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let source = temp.path().join("portrait.png");
        image::RgbaImage::from_pixel(9, 17, image::Rgba([32, 80, 120, 128]))
            .save(&source)
            .unwrap();
        let engine =
            Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        let previous = engine.state().unwrap();
        let state = engine.import_image(source.clone()).unwrap();
        let asset = &state["project"]["assets"][0];
        assert_eq!(asset["width"], 9);
        assert_eq!(asset["height"], 17);
        let layers = state["project"]["layers"].as_array().unwrap();
        let imported = layers.last().unwrap();
        assert_eq!(imported["size"], serde_json::json!([9.0, 17.0]));
        assert_eq!(imported["three_d"], false);
        fs::remove_file(source).unwrap();
        engine.history(0).unwrap();
        assert_eq!(
            engine.state().unwrap()["project"]["assets"],
            previous["project"]["assets"]
        );
        engine.history(1).unwrap();
        engine.save().unwrap();
        let package = engine.pack().unwrap();
        let imported_project = motion_core::storage::import_package(
            Path::new(package["path"].as_str().unwrap()),
            &temp.path().join("reopened"),
        )
        .unwrap();
        assert_eq!(imported_project.assets[0].width, 9);
    }
}
