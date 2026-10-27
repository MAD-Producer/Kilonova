//! Immutable, content-addressed images owned by a graph. PNG bytes travel with
//! portable source and packages; importing a file never retains its absolute path.
use crate::{Error, ErrorCode, GraphDocument, Node, NodeOp, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::ImageEncoder;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Cursor, Read},
    path::Path,
    sync::Arc,
};

pub const IMAGE_ASSETS_FORMAT_VERSION: u32 = 3;
pub const MAX_GRAPH_IMAGES: usize = 32;
// The existing 16 MiB effect-package boundary also needs room for code/manifest.
pub const MAX_GRAPH_IMAGE_BYTES: usize = 15 * 1024 * 1024;
pub const MAX_IMAGE_SOURCE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_GRAPH_METADATA_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_GRAPH_FILE_BYTES: usize =
    MAX_GRAPH_METADATA_BYTES + MAX_GRAPH_IMAGE_BYTES.div_ceil(3) * 4;

#[derive(Clone, PartialEq)]
pub struct GraphImage {
    width: u32,
    height: u32,
    png: Arc<[u8]>,
    sha256: Arc<str>,
}
impl std::fmt::Debug for GraphImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("sha256", &self.sha256)
            .field("png_bytes", &self.png.len())
            .finish()
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageData {
    width: u32,
    height: u32,
    png: String,
}
fn invalid(code: ErrorCode, cause: impl Into<String>) -> Error {
    Error::invalid(code, None, Some("images"), cause)
}
fn dimensions(width: u32, height: u32) -> Result<u64> {
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|v| v.checked_mul(4))
        .filter(|v| *v > 0 && *v <= motion_effects::DEFAULT_DECODED_IMAGE_BUDGET)
        .ok_or_else(|| {
            invalid(
                ErrorCode::Budget,
                "graph image exceeds the host decoded-image budget",
            )
        })?;
    Ok(bytes)
}
fn decode(bytes: &[u8], format: Option<image::ImageFormat>) -> Result<image::RgbaImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes));
    if let Some(format) = format {
        reader.set_format(format);
    } else {
        reader = reader.with_guessed_format().map_err(Error::Io)?;
    }
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(motion_effects::DEFAULT_DECODED_IMAGE_BUDGET);
    reader.limits(limits);
    let pixels = reader.decode().map_err(|cause| {
        Error::effect(
            ErrorCode::InvalidValue,
            "",
            motion_effects::Error::ImageResource {
                phase: "node_image_decode",
                path: "graph image".into(),
                cause: Box::new(cause),
            },
        )
    })?;
    dimensions(pixels.width(), pixels.height())?;
    Ok(pixels.into_rgba8())
}
impl GraphImage {
    fn verified(width: u32, height: u32, png: Vec<u8>) -> Result<Self> {
        dimensions(width, height)?;
        if png.len() > MAX_GRAPH_IMAGE_BYTES {
            return Err(invalid(ErrorCode::Budget, "graph PNG data exceeds 15 MiB"));
        }
        Ok(Self {
            width,
            height,
            sha256: format!("{:x}", Sha256::digest(&png)).into(),
            png: png.into(),
        })
    }
    pub fn from_png(png: Vec<u8>) -> Result<Self> {
        if png.len() > MAX_GRAPH_IMAGE_BYTES {
            return Err(invalid(ErrorCode::Budget, "graph PNG data exceeds 15 MiB"));
        }
        let pixels = decode(&png, Some(image::ImageFormat::Png))?;
        Self::verified(pixels.width(), pixels.height(), png)
    }
    /// Filesystem and codec work belongs on the existing node IO worker.
    pub fn import(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let before = file.metadata()?;
        if !before.is_file() || before.len() > MAX_IMAGE_SOURCE_BYTES as u64 {
            return Err(invalid(
                ErrorCode::Budget,
                "image input must be a regular file of at most 32 MiB",
            ));
        }
        let mut bytes = Vec::new();
        file.by_ref()
            .take(MAX_IMAGE_SOURCE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        if bytes.len() > MAX_IMAGE_SOURCE_BYTES
            || before.len() != after.len()
            || before.modified()? != after.modified()?
        {
            return Err(invalid(
                ErrorCode::Io,
                "image input changed while being read",
            ));
        }
        let pixels = decode(&bytes, None)?;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                pixels.as_raw(),
                pixels.width(),
                pixels.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|cause| {
                Error::effect(
                    ErrorCode::InvalidValue,
                    "",
                    motion_effects::Error::ImageResource {
                        phase: "node_image_encode",
                        path: "graph image".into(),
                        cause: Box::new(cause),
                    },
                )
            })?;
        Self::verified(pixels.width(), pixels.height(), png)
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn png(&self) -> &[u8] {
        &self.png
    }
    pub(crate) fn shared_png(&self) -> Arc<[u8]> {
        self.png.clone()
    }
    pub fn id(&self) -> &str {
        &self.sha256
    }
    pub fn resource_path(&self) -> String {
        format!("assets/image-{}.png", self.sha256)
    }
}
pub(crate) fn sample_helper(
    id: &str,
    slot: usize,
    space: motion_effects::WorkingSpace,
    alpha: motion_effects::AlphaMode,
) -> String {
    // Resources store straight sRGB. Convert each texel before bilinear filtering
    // so transparent colored pixels cannot bleed through their straight RGB.
    let space = u8::from(space == motion_effects::WorkingSpace::Srgb);
    let alpha = u8::from(alpha == motion_effects::AlphaMode::Straight);
    format!(
        r#"
fn ng_texel_{id}(at:vec2<i32>,last:vec2<i32>)->vec4<f32>{{
    return convert_pixel(textureLoad(resource{slot},clamp(at,vec2(0),last),0),vec2(1.0,1.0),vec2({space}.0,0.0));
}}
fn ng_image_{id}(uv:vec2<f32>)->vec4<f32>{{
    let size=vec2<i32>(textureDimensions(resource{slot}));let last=size-vec2(1);
    let p=clamp(uv,vec2(0.0),vec2(1.0))*vec2<f32>(size)-vec2(0.5);
    let base=vec2<i32>(floor(p));let f=fract(p);
    let a=mix(ng_texel_{id}(base,last),ng_texel_{id}(base+vec2(1,0),last),f.x);
    let b=mix(ng_texel_{id}(base+vec2(0,1),last),ng_texel_{id}(base+vec2(1,1),last),f.x);
    let c=select(mix(a,b,f.y),vec4(0.0),any(uv<vec2(0.0))||any(uv>vec2(1.0)));
    return convert_pixel(c,vec2({space}.0,0.0),vec2({space}.0,{alpha}.0));
}}
"#
    )
}
impl Serialize for GraphImage {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        ImageData {
            width: self.width,
            height: self.height,
            png: STANDARD.encode(&self.png),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for GraphImage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let data = ImageData::deserialize(deserializer)?;
        from_data(data).map_err(serde::de::Error::custom)
    }
}
fn from_data(data: ImageData) -> Result<GraphImage> {
    dimensions(data.width, data.height)?;
    if data.png.len() > MAX_GRAPH_IMAGE_BYTES.div_ceil(3) * 4 {
        return Err(invalid(
            ErrorCode::Budget,
            "graph image encoded data exceeds budget",
        ));
    }
    let png = STANDARD
        .decode(data.png)
        .map_err(|cause| Error::ImageEncoding {
            diagnostic: crate::Diagnostic {
                code: ErrorCode::InvalidValue,
                node: None,
                port: Some("images".into()),
                message: format!("graph image encoding failed: {cause}"),
            },
            cause,
        })?;
    let image = GraphImage::from_png(png)?;
    if (data.width, data.height) != (image.width, image.height) {
        return Err(invalid(
            ErrorCode::InvalidValue,
            "graph image dimensions do not match PNG contents",
        ));
    }
    Ok(image)
}
pub(crate) fn take_json_images(
    value: &mut serde_json::Value,
) -> Result<std::collections::BTreeMap<String, GraphImage>> {
    let Some(images) = value.as_object_mut().and_then(|root| root.remove("images")) else {
        return Ok(Default::default());
    };
    let records: std::collections::BTreeMap<String, ImageData> = serde_json::from_value(images)?;
    records
        .into_iter()
        .map(|(id, data)| {
            if id.len() != 64
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err(invalid(
                    ErrorCode::InvalidValue,
                    "invalid graph image content identity",
                ));
            }
            let image = from_data(data).map_err(|mut error| {
                match &mut error {
                    Error::Effect { diagnostic, .. }
                    | Error::ImageEncoding { diagnostic, .. }
                    | Error::Invalid(diagnostic) => diagnostic.port = Some(format!("images.{id}")),
                    _ => {}
                }
                error
            })?;
            Ok((id, image))
        })
        .collect()
}
pub(crate) fn references(nodes: &[Node], images: &mut BTreeSet<String>) {
    for node in nodes {
        match &node.operation {
            NodeOp::ImageAsset { image } => {
                images.insert(image.clone());
            }
            NodeOp::Group { graph } => references(&graph.nodes, images),
            _ => {}
        }
    }
}
impl GraphDocument {
    /// Validate metadata without base64-encoding image payloads on an editor's
    /// interactive path. Images have their separate, content-owned budget.
    pub fn validate_metadata_budget(&self) -> Result<()> {
        let mut metadata = self.clone();
        metadata.images.clear();
        let mut value = serde_json::to_value(&metadata)?;
        if !self.images.is_empty() {
            value["images"] = self
                .images
                .iter()
                .map(|(id, image)| {
                    (
                        id.clone(),
                        serde_json::json!({
                            "width": image.width, "height": image.height, "png": ""
                        }),
                    )
                })
                .collect::<serde_json::Map<_, _>>()
                .into();
        }
        if serde_json::to_vec(&value)?.len() > MAX_GRAPH_METADATA_BYTES {
            return Err(invalid(ErrorCode::Budget, "graph metadata exceeds 4 MiB"));
        }
        Ok(())
    }
    pub fn referenced_images(&self) -> BTreeSet<String> {
        let mut images = BTreeSet::new();
        references(&self.nodes, &mut images);
        images
    }
    pub fn validate_images(&self) -> Result<()> {
        if self.images.len() > MAX_GRAPH_IMAGES {
            return Err(invalid(ErrorCode::Budget, "graph has more than 32 images"));
        }
        let mut encoded = 0usize;
        let mut decoded = 0u64;
        for (id, image) in &self.images {
            if id != image.id() {
                return Err(invalid(
                    ErrorCode::InvalidValue,
                    "graph image key does not match its SHA-256 content identity",
                ));
            }
            encoded = encoded.saturating_add(image.png.len());
            decoded = decoded.saturating_add(dimensions(image.width, image.height)?);
        }
        if encoded > MAX_GRAPH_IMAGE_BYTES || decoded > motion_effects::DEFAULT_DECODED_IMAGE_BUDGET
        {
            return Err(invalid(
                ErrorCode::Budget,
                "graph images exceed package byte or decoded-image budget",
            ));
        }
        fn check(
            nodes: &[Node],
            images: &std::collections::BTreeMap<String, GraphImage>,
        ) -> Result<()> {
            for node in nodes {
                match &node.operation {
                    NodeOp::ImageAsset { image } if !images.contains_key(image) => {
                        return Err(Error::invalid(
                            ErrorCode::InvalidValue,
                            Some(&node.id),
                            Some("image"),
                            "graph image resource is missing",
                        ));
                    }
                    NodeOp::Group { graph } => check(&graph.nodes, images)?,
                    _ => {}
                }
            }
            Ok(())
        }
        check(&self.nodes, &self.images)
    }
}
/// Reject oversized metadata and aggregate dimensions before base64/PNG decode.
pub(crate) fn preflight_json(value: &serde_json::Value, bytes: usize) -> Result<()> {
    let mut encoded = 0usize;
    let mut decoded = 0u64;
    if let Some(images) = value.get("images").and_then(|v| v.as_object()) {
        if images.len() > MAX_GRAPH_IMAGES {
            return Err(invalid(ErrorCode::Budget, "graph has more than 32 images"));
        }
        for image in images.values() {
            let w = image["width"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| invalid(ErrorCode::InvalidValue, "invalid graph image width"))?;
            let h = image["height"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| invalid(ErrorCode::InvalidValue, "invalid graph image height"))?;
            decoded = decoded.saturating_add(dimensions(w, h)?);
            encoded = encoded.saturating_add(image["png"].as_str().map_or(0, str::len));
        }
    }
    if encoded > MAX_GRAPH_IMAGE_BYTES.div_ceil(3) * 4
        || decoded > motion_effects::DEFAULT_DECODED_IMAGE_BUDGET
        || bytes.saturating_sub(encoded) > MAX_GRAPH_METADATA_BYTES
    {
        return Err(invalid(
            ErrorCode::Budget,
            "graph metadata or embedded images exceed their independent budgets",
        ));
    }
    Ok(())
}
