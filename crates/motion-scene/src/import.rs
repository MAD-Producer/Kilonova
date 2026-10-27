use crate::{
    accessors::{Budget, Reader},
    invalid,
    resources::load_uri,
    schema, unsupported, AssetResolver, Cause, Error, Image, Mesh, Resource, ResourceKind, Result,
    Sampler, Scene, Texture,
};
use crate::{
    container::container,
    decode::{material, nodes, primitive},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, time::Instant};

#[derive(Clone, Copy, Debug)]
pub struct ImportContext {
    pub request: u64,
    pub source: u64,
}

#[derive(Clone, Debug)]
pub struct ImportLimits {
    pub max_source_bytes: usize,
    pub max_json_bytes: usize,
    pub max_resource_bytes: usize,
    pub max_retained_bytes: usize,
    pub max_items: usize,
    pub max_vertices: usize,
    pub max_indices: usize,
    pub max_node_depth: usize,
    /// Total decoded key timestamps across channels, including shared-sampler copies.
    pub max_animation_keys: usize,
}
impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 64 << 20,
            max_json_bytes: 16 << 20,
            max_resource_bytes: 64 << 20,
            max_retained_bytes: 256 << 20,
            max_items: 16384,
            max_vertices: 1_000_000,
            max_indices: 3_000_000,
            max_node_depth: 128,
            max_animation_keys: 1_000_000,
        }
    }
}

/// Pure CPU import with owned resource bytes. The resolver may block; the Info start is
/// published before parsing or calling it. Call from a host worker, never a paint handler.
pub fn import(
    context: ImportContext,
    source: &[u8],
    resolver: &mut impl AssetResolver,
    limits: &ImportLimits,
) -> Result<Scene> {
    let began = Instant::now();
    log::info!(
        "start #{} gltf_import source={}",
        context.request,
        context.source
    );
    let result = load(source, resolver, limits);
    match &result {
        Ok(scene) => {
            if !scene.ignored_optional_extensions.is_empty() {
                log::warn!(
                    "gltf_import #{} source={} ignored_optional_extensions={:?}",
                    context.request,
                    context.source,
                    scene.ignored_optional_extensions
                );
            }
            log::debug!(
                "done #{} gltf_import source={} elapsed_ms={} retained_bytes={}",
                context.request,
                context.source,
                began.elapsed().as_millis(),
                scene.retained_bytes
            );
        }
        Err(error) => {
            let cause: String = error.to_string().chars().take(1024).collect();
            log::error!(
                "gltf_import #{} source={} elapsed_ms={} cause={}",
                context.request,
                context.source,
                began.elapsed().as_millis(),
                cause
            );
        }
    }
    result
}

fn load(source: &[u8], resolver: &mut impl AssetResolver, limits: &ImportLimits) -> Result<Scene> {
    if source.len() > limits.max_source_bytes {
        return Err(invalid("container", "source", "source byte limit exceeded"));
    }
    let (json, bin) = container(source)?;
    if json.len() > limits.max_json_bytes {
        return Err(invalid("json", "source", "JSON byte limit exceeded"));
    }
    let root: schema::Root = serde_json::from_slice(json).map_err(|cause| Error {
        phase: "json",
        object: "source".into(),
        cause: Cause::Json(cause),
    })?;
    if root.asset.version != "2.0" || root.asset.min_version.as_ref().is_some_and(|v| v != "2.0") {
        return Err(unsupported(
            "asset",
            "version",
            "only glTF 2.0 is supported",
        ));
    }
    for count in [
        root.buffers.len(),
        root.buffer_views.len(),
        root.accessors.len(),
        root.images.len(),
        root.textures.len(),
        root.samplers.len(),
        root.materials.len(),
        root.meshes.len(),
        root.animations.len(),
        root.cameras.len(),
        root.nodes.len(),
        root.scenes.len(),
    ] {
        if count > limits.max_items {
            return Err(invalid(
                "budget",
                "items",
                "descriptor count limit exceeded",
            ));
        }
    }
    if root.extensions_used.len() > 32
        || root.extensions_required.len() > 32
        || root
            .extensions_used
            .iter()
            .chain(&root.extensions_required)
            .any(|s| s.len() > 128 || !s.is_ascii())
    {
        return Err(invalid(
            "extensions",
            "root",
            "extension metadata exceeds limit",
        ));
    }
    if !root.extensions_required.is_empty() {
        return Err(unsupported(
            "extensions",
            "required",
            root.extensions_required.join(", "),
        ));
    }
    for (feature, count) in [("skins", root.skins.len())] {
        if count != 0 {
            return Err(unsupported(
                "profile",
                feature,
                format!("{feature} are not implemented by the import profile"),
            ));
        }
    }
    let mut budget = Budget {
        bytes: 0,
        vertices: 0,
        indices: 0,
    };
    let mut buffers = Vec::with_capacity(root.buffers.len());
    for (id, description) in root.buffers.iter().enumerate() {
        if description.byte_length == 0 || description.byte_length > limits.max_resource_bytes {
            return Err(invalid("buffer", id, "invalid declared byte length"));
        }
        let resource = if let Some(uri) = &description.uri {
            let remaining = limits
                .max_retained_bytes
                .saturating_sub(budget.bytes)
                .min(limits.max_resource_bytes);
            let (mut resource, _) = load_uri(uri, ResourceKind::Buffer, remaining, resolver)?;
            if resource.bytes.len() < description.byte_length {
                return Err(invalid(
                    "buffer",
                    id,
                    "resource shorter than declared byte length",
                ));
            }
            // Hash and own all source bytes, including any unused trailing bytes, for relinking.
            resource.bytes.shrink_to_fit();
            resource
        } else {
            let payload = bin
                .filter(|_| id == 0)
                .ok_or_else(|| invalid("buffer", id, "missing GLB BIN payload"))?;
            if payload.len() < description.byte_length
                || payload.len() - description.byte_length > 3
                || payload[description.byte_length..].iter().any(|&b| b != 0)
            {
                return Err(invalid("buffer", id, "GLB payload length/padding mismatch"));
            }
            budget.reserve(description.byte_length, 1, limits)?;
            buffers.push(Resource::new(
                None,
                payload[..description.byte_length].to_vec(),
            ));
            continue;
        };
        budget.reserve(resource.bytes.len(), 1, limits)?;
        buffers.push(resource);
    }
    let reader = Reader {
        root: &root,
        buffers: &buffers,
        limits,
    };
    for id in 0..root.buffer_views.len() {
        reader.view(id)?;
    }
    let mut images = Vec::with_capacity(root.images.len());
    for (id, image) in root.images.iter().enumerate() {
        let (resource, embedded_mime) = match (&image.uri, image.buffer_view) {
            (Some(uri), None) => load_uri(
                uri,
                ResourceKind::Image,
                limits
                    .max_resource_bytes
                    .min(limits.max_retained_bytes.saturating_sub(budget.bytes)),
                resolver,
            )?,
            (None, Some(view)) => {
                if image.mime_type.is_none() {
                    return Err(invalid("image", id, "buffer-view image requires mimeType"));
                }
                if root
                    .buffer_views
                    .get(view)
                    .is_some_and(|v| v.byte_stride.is_some() || v.target.is_some())
                {
                    return Err(invalid(
                        "image",
                        id,
                        "image view cannot be strided or GPU-targeted",
                    ));
                }
                let bytes = reader.view(view)?;
                if bytes.len() > limits.max_resource_bytes {
                    return Err(invalid("image", id, "image byte limit exceeded"));
                }
                // Reserve before cloning potentially large image data from a buffer.
                budget.reserve(bytes.len(), 1, limits)?;
                let resource = Resource::new(None, bytes.to_vec());
                let mime = image_mime(id, &resource.bytes, image.mime_type.as_deref(), None)?;
                images.push(Image {
                    resource,
                    mime_type: mime,
                });
                continue;
            }
            _ => {
                return Err(invalid(
                    "image",
                    id,
                    "exactly one of uri or bufferView is required",
                ))
            }
        };
        let mime = image_mime(
            id,
            &resource.bytes,
            image.mime_type.as_deref(),
            embedded_mime.as_deref(),
        )?;
        budget.reserve(resource.bytes.len(), 1, limits)?;
        images.push(Image {
            resource,
            mime_type: mime,
        });
    }
    let samplers = root
        .samplers
        .iter()
        .enumerate()
        .map(|(id, s)| {
            if s.mag_filter.is_some_and(|f| ![9728, 9729].contains(&f))
                || s.min_filter
                    .is_some_and(|f| ![9728, 9729, 9984, 9985, 9986, 9987].contains(&f))
                || ![33071, 33648, 10497].contains(&s.wrap_s)
                || ![33071, 33648, 10497].contains(&s.wrap_t)
            {
                return Err(invalid("sampler", id, "invalid filter or wrap mode"));
            }
            Ok(Sampler {
                mag_filter: s.mag_filter,
                min_filter: s.min_filter,
                wrap_s: s.wrap_s,
                wrap_t: s.wrap_t,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let textures = root
        .textures
        .iter()
        .enumerate()
        .map(|(id, t)| {
            let image = t
                .source
                .filter(|&n| n < images.len())
                .ok_or_else(|| invalid("texture", id, "missing or invalid image source"))?;
            if t.sampler.is_some_and(|n| n >= samplers.len()) {
                return Err(invalid("texture", id, "invalid sampler index"));
            }
            Ok(Texture {
                image,
                sampler: t.sampler,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let materials = root
        .materials
        .iter()
        .enumerate()
        .map(|(id, m)| material(id, m, textures.len()))
        .collect::<Result<Vec<_>>>()?;
    let mut meshes = Vec::with_capacity(root.meshes.len());
    let mut primitive_count = 0usize;
    let mut target_count = 0usize;
    for (mesh_id, mesh) in root.meshes.iter().enumerate() {
        primitive_count = primitive_count
            .checked_add(mesh.primitives.len())
            .filter(|&n| n <= limits.max_items)
            .ok_or_else(|| invalid("budget", "primitives", "primitive count limit exceeded"))?;
        if mesh.primitives.is_empty() {
            return Err(invalid("mesh", mesh_id, "empty primitive list"));
        }
        let count = mesh.primitives[0].targets.len();
        if mesh.primitives.iter().any(|p| p.targets.len() != count) {
            return Err(invalid("mesh", mesh_id, "inconsistent morph target counts"));
        }
        target_count = mesh.primitives.iter().try_fold(target_count, |n, p| {
            n.checked_add(p.targets.len())
                .filter(|&n| n <= limits.max_items)
                .ok_or_else(|| {
                    invalid(
                        "budget",
                        "morph targets",
                        "target descriptor limit exceeded",
                    )
                })
        })?;
        if let Some(weights) = &mesh.weights {
            crate::morph::validate_weights("mesh", mesh_id, weights, count)?;
        }
        budget.reserve(count, std::mem::size_of::<f32>(), limits)?;
        let weights = mesh.weights.clone().unwrap_or_else(|| vec![0.; count]);
        let primitives = mesh
            .primitives
            .iter()
            .enumerate()
            .map(|(id, p)| {
                primitive(id, p, &reader, &materials, &mut budget).map_err(|mut error| {
                    error.object = format!("mesh={mesh_id}/primitive={id}/{}", error.object);
                    error
                })
            })
            .collect::<Result<Vec<_>>>()?;
        meshes.push(Mesh {
            primitives,
            weights,
        });
    }
    let nodes = nodes(&root, limits, &mut budget)?;
    let cameras = crate::camera::decode(&root, &mut budget, limits)?;
    let animations = crate::animation::decode(&reader, &mut budget)?;
    let mut scenes = Vec::with_capacity(root.scenes.len());
    let mut parents = vec![false; nodes.len()];
    for node in &nodes {
        for &child in &node.children {
            parents[child] = true;
        }
    }
    for (id, scene) in root.scenes.iter().enumerate() {
        let mut unique = BTreeSet::new();
        if scene.nodes.len() > limits.max_items
            || scene
                .nodes
                .iter()
                .any(|&n| n >= nodes.len() || parents[n] || !unique.insert(n))
        {
            return Err(invalid("scene", id, "invalid or repeated root node"));
        }
        scenes.push(scene.nodes.clone());
    }
    if root.scene.is_some_and(|n| n >= scenes.len()) {
        return Err(invalid("scene", "default", "scene index out of range"));
    }
    Ok(Scene {
        format: crate::SourceFormat::Gltf,
        obj_metadata: None,
        extra_resources: Vec::new(),
        source_sha256: Sha256::digest(source).into(),
        buffers,
        images,
        textures,
        samplers,
        materials,
        meshes,
        nodes,
        animations,
        cameras,
        scenes,
        default_scene: root.scene,
        ignored_optional_extensions: root.extensions_used,
        retained_bytes: budget.bytes,
    })
}

pub(crate) fn image_mime(
    id: usize,
    bytes: &[u8],
    declared: Option<&str>,
    embedded: Option<&str>,
) -> Result<String> {
    let detected = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else {
        return Err(unsupported(
            "image",
            id,
            "only encoded PNG/JPEG images are supported",
        ));
    };
    if declared.is_some_and(|m| m != detected) || embedded.is_some_and(|m| m != detected) {
        return Err(invalid(
            "image",
            id,
            "declared image type does not match payload",
        ));
    }
    // Signature checks are not image decode validation. The later texture decoder needs pixel limits.
    Ok(detected.into())
}
