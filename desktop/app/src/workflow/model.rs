//! Bounded model ingestion publishes an owned frozen asset and layer atomically.
use motion_core::{Command, Content, Layer, LayerTimeline, ModelAsset, ModelClip};
use motion_host::{Result, Session};
use motion_scene::{DirectoryResolver, ImportContext, ImportLimits};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy)]
enum Destination {
    Layer,
    Library { folder: Option<u64> },
}

pub(crate) fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| {
            ["gltf", "glb", "obj"]
                .iter()
                .any(|kind| ext.eq_ignore_ascii_case(kind))
        })
}

pub(crate) fn import(session: &mut Session, path: &Path) -> Result<Value> {
    publish(session, path, Destination::Layer)
}

pub(crate) fn register(session: &mut Session, path: &Path, folder: Option<u64>) -> Result<Value> {
    publish(session, path, Destination::Library { folder })
}

fn publish(session: &mut Session, path: &Path, destination: Destination) -> Result<Value> {
    let request = crate::engine::current_command().map_or(0, |c| c.0);
    let source: String = path.to_string_lossy().chars().take(1024).collect();
    let began = Instant::now();
    let operation = match destination {
        Destination::Layer => "model_import",
        Destination::Library { .. } => "model_register",
    };
    log::info!("start #{request} {operation} source={source:?} phase=read");
    let result = load(session, path, request, destination);
    match &result {
        Ok(_) => log::debug!(
            "done #{request} {operation} elapsed_ms={}",
            began.elapsed().as_millis()
        ),
        Err(cause) => log::error!(
            "{operation} #{request} source={source:?} elapsed_ms={} cause={}",
            began.elapsed().as_millis(),
            cause.to_string().chars().take(1024).collect::<String>()
        ),
    }
    result.map_err(|cause| {
        motion_host::Error::with_context(format!("{operation} #{request} source={source:?}"), cause)
    })
}

fn load(
    session: &mut Session,
    path: &Path,
    request: u64,
    destination: Destination,
) -> Result<Value> {
    if !supported(path) {
        return Err("model source must be glTF, GLB or OBJ".into());
    }
    if let Destination::Library {
        folder: Some(folder),
    } = destination
    {
        if !session
            .engine
            .project()
            .library
            .folders
            .iter()
            .any(|f| f.id == folder)
        {
            return Err("model destination folder no longer exists".into());
        }
    }
    let limits = ImportLimits::default();
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limits.max_source_bytes as u64 {
        return Err("model source must be a regular file within the source byte limit".into());
    }
    // Bound the actual read as well as metadata; a source may grow while read.
    let mut bytes = Vec::new();
    file.take(limits.max_source_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limits.max_source_bytes {
        return Err("model source exceeds source byte limit".into());
    }
    let project = session.engine.project();
    let asset = project
        .assets
        .iter()
        .map(|a| a.id)
        .chain(project.video_assets.iter().map(|a| a.id))
        .chain(project.audio_assets.iter().map(|a| a.id))
        .chain(project.model_assets.iter().map(|a| a.id))
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("asset IDs exhausted")?;
    let object = match destination {
        Destination::Layer => Some(super::next_layer_id(&session.snapshot()?)?),
        Destination::Library { .. } => None,
    };
    let mut resolver = DirectoryResolver::new(path.parent().ok_or("model source parent missing")?)?;
    let context = ImportContext {
        request,
        source: asset,
    };
    let scene = if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("obj"))
    {
        motion_scene::obj::import_obj(context, &bytes, &mut resolver, &Default::default(), &limits)?
    } else {
        motion_scene::import(context, &bytes, &mut resolver, &limits)?
    };
    // Respect explicit scene selection; glTF permits no default, in which case
    // this import operation explicitly selects scene zero, without autoplay.
    let selected = scene.default_scene().unwrap_or(0);
    if scene.instances(selected, 1.)?.is_empty() {
        return Err("selected model scene has no mesh instances".into());
    }
    let frozen = scene.freeze(
        &bytes,
        motion_core::model_assets::MAX_MODEL_ASSET_BYTES as usize,
    )?;
    let root = session.root.canonicalize()?;
    // Check each parent before creating the next component. In particular,
    // an assets-directory junction must not create models outside the project.
    for relative in ["assets", "assets/models"] {
        let directory = root.join(relative);
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(cause) if cause.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(cause) => return Err(cause.into()),
        }
        if !directory.canonicalize()?.starts_with(&root) || !directory.is_dir() {
            return Err("model asset folder resolves outside project or is not a directory".into());
        }
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let relative = format!("assets/models/model-{asset}-{stamp}.mscn");
    let mut owned = crate::owned_file::OwnedFile::create(&root.join(&relative))?;
    let publication: Result<Value> = (|| {
        owned.file().write_all(&frozen)?;
        owned.file().sync_all()?;
        owned.close();
        let mut commands = vec![Command::RegisterModelAsset {
            asset: ModelAsset {
                id: asset,
                path: relative,
                bytes: frozen.len() as u64,
                sha256: format!("{:x}", Sha256::digest(&frozen)),
            },
        }];
        if let Some(object) = object {
            let project = session.engine.project();
            let at = (session.frame.floor().max(0.) as u32).min(project.frames - 1);
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model")
                .chars()
                .take(128)
                .collect::<String>();
            commands.push(Command::Add {
                layer: model_layer(project, object, asset, selected as u32, &name, at),
            });
        } else if let Destination::Library { folder } = destination {
            commands.push(Command::ProjectLibrary {
                action: motion_core::LibraryAction::MoveItems {
                    items: vec![motion_core::ProjectItem::Model { id: asset }],
                    folder,
                },
            });
        }
        let mut state = session.apply_command_batch(commands)?;
        state["modelImport"] = json!({"asset":asset,"object":object,"scene":selected,
            "pixelsPerMetre":100,"samples":1,"animation":null,"owned":true});
        if let Destination::Library { folder } = destination {
            state["assetRegistration"] =
                json!({"cancelled":false,"asset":asset,"kind":"model","folder":folder});
        }
        Ok(state)
    })();
    match publication {
        Ok(state) => {
            owned.keep();
            Ok(state)
        }
        Err(primary) => {
            if let Err(cleanup) = owned.discard() {
                log::error!("model_import #{request} asset={asset} object={object:?} phase=rollback primary={primary} cleanup={cleanup}");
            }
            Err(primary)
        }
    }
}

/// Place an existing immutable source; never read the original model again.
pub(crate) fn place(session: &mut Session, asset: u64, at_frame: u32) -> Result<Value> {
    let project = session.engine.project();
    if at_frame >= project.frames {
        return Err("model placement outside composition".into());
    }
    if !project.model_assets.iter().any(|source| source.id == asset) {
        return Err("placement model source no longer exists".into());
    }
    let catalog = session.scene.model_source_catalog();
    let source = catalog
        .scene(asset)
        .ok_or("placement model source is not prepared")?;
    let selected = source.default_scene().unwrap_or(0);
    if source.instances(selected, 1.)?.is_empty() {
        return Err("selected model scene has no mesh instances".into());
    }
    let object = super::next_layer_id(&session.snapshot()?)?;
    let layer = model_layer(
        project,
        object,
        asset,
        selected as u32,
        &format!("Model {asset}"),
        at_frame,
    );
    let mut state = session.apply_command_batch(vec![Command::Add { layer }])?;
    state["assetPlacement"] = json!({"cancelled":false,"object":object,
        "target":{"kind":"model","id":asset},"frame":at_frame});
    Ok(state)
}

fn model_layer(
    project: &motion_core::Project,
    object: u64,
    asset: u64,
    scene: u32,
    name: &str,
    at: u32,
) -> Layer {
    let mut layer = Layer::solid(
        object,
        name,
        [200.; 2],
        [project.width as f32 / 2., project.height as f32 / 2., 0.],
        [1.; 4],
    );
    layer.three_d = true;
    layer.content = Content::Model {
        model: ModelClip {
            asset,
            scene,
            pixels_per_metre: 100.,
            pivot: [0.; 3],
            samples: 1,
            animation: None,
        },
    };
    layer.timeline = Some(LayerTimeline {
        in_frame: at as i32,
        out_frame: project.frames as i32,
        offset_frame: at as i32,
    });
    layer
}
