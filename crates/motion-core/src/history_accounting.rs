//! Exact compact-JSON history accounting. Payload sizes are immutable caches:
//! normal edits revisit only changed layers/cameras, and history moves reuse
//! the whole cache. This counts model JSON, not heap capacity or image/GPU data.
use crate::{Camera, Command, Layer, Project, PropertyExpression};
use serde::{
    ser::{Error as _, Impossible, SerializeStruct},
    Serialize, Serializer,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

type JsonResult<T> = std::result::Result<T, serde_json::Error>;

fn counted<T: Serialize + ?Sized>(value: &T) -> usize {
    let bytes = motion_model::serialized_bytes(value);
    #[cfg(test)]
    SERIALIZED_BYTES.with(|total| total.set(total.get().saturating_add(bytes)));
    bytes
}
#[cfg(test)]
thread_local! { static SERIALIZED_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
pub(crate) fn reset_serialized_bytes() {
    SERIALIZED_BYTES.with(|total| total.set(0));
}
#[cfg(test)]
pub(crate) fn serialized_visit_bytes() -> usize {
    SERIALIZED_BYTES.with(|total| total.get())
}

#[derive(Clone, Default)]
pub(crate) struct Changes {
    all: bool,
    nodes: HashMap<String, NodeChanges>,
}
#[derive(Clone, Default)]
struct NodeChanges {
    camera: bool,
    layers: HashSet<u64>,
}
impl Changes {
    pub(crate) fn note(&mut self, command: &Command, composition: &str) {
        if self.all {
            return;
        }
        let object = match command {
            Command::InComposition {
                composition,
                command,
            } => {
                self.note(command, composition);
                return;
            }
            Command::SetVector { object, .. }
            | Command::SetScalar { object, .. }
            | Command::SetComponent { object, .. }
            | Command::SeparateDimensions { object, .. }
            | Command::MergeDimensions { object, .. }
            | Command::Curve { object, .. }
            | Command::Spatial { object, .. }
            | Command::CopyKey { object, .. }
            | Command::MoveKey { object, .. }
            | Command::DeleteKey { object, .. }
            | Command::Animate { object, .. }
            | Command::Ease { object, .. }
            | Command::Rename { object, .. }
            | Command::Flags { object, .. }
            | Command::Anchor { object, .. }
            | Command::SetAudio { object, .. }
            | Command::AudioLevels { object, .. }
            | Command::AudioMarkers { object, .. }
            | Command::SetLayer3d { object, .. }
            | Command::SetRotationOrder { object, .. }
            | Command::MoveLayerClip { object, .. }
            | Command::TrimLayerClip { object, .. }
            | Command::SetLayerRange { object, .. }
            | Command::SetLayerBlend { object, .. }
            | Command::SetTrackMatte { object, .. }
            | Command::Vector { object, .. }
            | Command::Mask { object, .. } => *object,
            Command::SetColor { object: 0, .. } | Command::Reorder { .. } => return,
            Command::SetColor { object, .. } => *object,
            Command::CameraMode { object, .. }
            | Command::Dolly { object, .. }
            | Command::Pan { object, .. } => *object,
            // Hierarchy, expressions, effects, assets and composition graph
            // transactions can change several payloads. New commands safely
            // rebuild until their mutation scope is explicitly reviewed here.
            _ => {
                self.all = true;
                self.nodes.clear();
                return;
            }
        };
        let node = self.nodes.entry(composition.into()).or_default();
        if object == 0 {
            node.camera = true;
        } else {
            node.layers.insert(object);
        }
    }
    pub(crate) fn merge(&mut self, other: Self) {
        if self.all {
            return;
        }
        if other.all {
            self.all = true;
            self.nodes.clear();
            return;
        }
        for (id, changes) in other.nodes {
            let node = self.nodes.entry(id).or_default();
            node.camera |= changes.camera;
            node.layers.extend(changes.layers);
        }
    }
}

#[derive(Clone)]
struct NodeSize {
    camera: usize,
    expressions: usize,
    layers: Arc<HashMap<u64, usize>>,
    layer_array: usize,
}
impl NodeSize {
    fn new(camera: &Camera, layers: &[Layer], expressions: &[PropertyExpression]) -> Self {
        let sizes: HashMap<_, _> = layers
            .iter()
            .map(|layer| (layer.id, counted(layer)))
            .collect();
        let layer_array = array_size(sizes.values().copied());
        Self {
            camera: counted(camera),
            expressions: counted(expressions),
            layers: Arc::new(sizes),
            layer_array,
        }
    }
    fn update(&mut self, camera: &Camera, layers: &[Layer], changes: &NodeChanges) {
        if changes.camera {
            self.camera = counted(camera);
        }
        if !changes.layers.is_empty() {
            let sizes = Arc::make_mut(&mut self.layers);
            for id in &changes.layers {
                if let Some(layer) = layers.iter().find(|layer| layer.id == *id) {
                    let size = counted(layer);
                    // Single-payload commands do not add/remove IDs. A missing
                    // cache entry fails closed instead of undercounting history.
                    let Some(old) = sizes.insert(*id, size) else {
                        self.layer_array = usize::MAX;
                        continue;
                    };
                    self.layer_array = self
                        .layer_array
                        .checked_sub(old)
                        .and_then(|n| n.checked_add(size))
                        .unwrap_or(usize::MAX);
                } else {
                    self.layer_array = usize::MAX;
                }
            }
        }
    }
    fn fields(&self) -> HashMap<&'static str, usize> {
        HashMap::from([
            ("camera", self.camera),
            ("layers", self.layer_array),
            ("expressions", self.expressions),
        ])
    }
}
fn array_size(sizes: impl Iterator<Item = usize>) -> usize {
    let mut total: usize = 2;
    let mut first = true;
    for size in sizes {
        total = total
            .saturating_add(size)
            .saturating_add(usize::from(!first));
        first = false;
    }
    total
}

#[derive(Clone)]
pub(crate) struct ProjectSize {
    bytes: usize,
    globals: Arc<HashMap<&'static str, usize>>,
    nodes: Arc<HashMap<String, NodeSize>>,
}
impl ProjectSize {
    pub(crate) fn new(project: &Project) -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(
            project.composition_id.clone(),
            NodeSize::new(&project.camera, &project.layers, &project.expressions),
        );
        for node in &project.compositions {
            nodes.insert(
                node.id.clone(),
                NodeSize::new(&node.camera, &node.layers, &node.expressions),
            );
        }
        let globals = HashMap::from([
            ("assets", counted(&project.assets)),
            ("fonts", counted(&project.fonts)),
            ("audio_assets", counted(&project.audio_assets)),
            ("video_assets", counted(&project.video_assets)),
            ("plugin_dependencies", counted(&project.plugin_dependencies)),
        ]);
        let mut size = Self {
            bytes: 0,
            globals: Arc::new(globals),
            nodes: Arc::new(nodes),
        };
        size.refresh_header(project);
        size
    }
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn updated(&self, project: &Project, changes: &Changes) -> Self {
        if changes.all {
            return Self::new(project);
        }
        let mut next = self.clone();
        if !changes.nodes.is_empty() {
            let nodes = Arc::make_mut(&mut next.nodes);
            for (id, changed) in &changes.nodes {
                let Ok(view) = project.composition_view(id) else {
                    return Self::new(project);
                };
                let Some(node) = nodes.get_mut(id) else {
                    return Self::new(project);
                };
                node.update(view.camera, view.layers, changed);
            }
        }
        next.refresh_header(project);
        next
    }
    fn refresh_header(&mut self, project: &Project) {
        let Some(active) = self.nodes.get(&project.composition_id) else {
            self.bytes = usize::MAX;
            return;
        };
        let mut fields = active.fields();
        fields.extend(self.globals.iter().map(|(name, size)| (*name, *size)));
        let compositions = project.compositions.iter().map(|composition| {
            self.nodes
                .get(&composition.id)
                .map_or(usize::MAX, |node| struct_bytes(composition, &node.fields()))
        });
        fields.insert("compositions", array_size(compositions));
        self.bytes = struct_bytes(project, &fields);
    }
}

// Serde still decides which root fields are omitted. Only field payloads with
// cached exact sizes are substituted, so JSON punctuation/escaping and future
// model fields retain the real serializer's contract without mirrored schemas.
fn struct_bytes<T: Serialize>(value: &T, cached: &HashMap<&'static str, usize>) -> usize {
    value
        .serialize(StructSizer { cached })
        .unwrap_or(usize::MAX)
}
struct StructSizer<'a> {
    cached: &'a HashMap<&'static str, usize>,
}
struct FieldsSizer<'a> {
    cached: &'a HashMap<&'static str, usize>,
    bytes: usize,
    fields: usize,
}
impl SerializeStruct for FieldsSizer<'_> {
    type Ok = usize;
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> JsonResult<()> {
        let size = self
            .cached
            .get(key)
            .copied()
            .unwrap_or_else(|| counted(value));
        self.bytes = self
            .bytes
            .saturating_add(counted(key))
            .saturating_add(1)
            .saturating_add(usize::from(self.fields > 0))
            .saturating_add(size);
        self.fields += 1;
        Ok(())
    }
    fn end(self) -> JsonResult<usize> {
        Ok(self.bytes)
    }
}
fn unsupported<T>() -> JsonResult<T> {
    Err(serde_json::Error::custom(
        "expected project/composition struct",
    ))
}
macro_rules! primitive {
    ($($name:ident($ty:ty);)*) => { $(fn $name(self, _: $ty) -> JsonResult<usize> { unsupported() })* };
}
impl<'a> Serializer for StructSizer<'a> {
    type Ok = usize;
    type Error = serde_json::Error;
    type SerializeSeq = Impossible<usize, serde_json::Error>;
    type SerializeTuple = Impossible<usize, serde_json::Error>;
    type SerializeTupleStruct = Impossible<usize, serde_json::Error>;
    type SerializeTupleVariant = Impossible<usize, serde_json::Error>;
    type SerializeMap = Impossible<usize, serde_json::Error>;
    type SerializeStruct = FieldsSizer<'a>;
    type SerializeStructVariant = Impossible<usize, serde_json::Error>;
    primitive! { serialize_bool(bool); serialize_i8(i8); serialize_i16(i16); serialize_i32(i32); serialize_i64(i64);
    serialize_u8(u8); serialize_u16(u16); serialize_u32(u32); serialize_u64(u64); serialize_f32(f32); serialize_f64(f64);
    serialize_char(char); serialize_str(&str); serialize_bytes(&[u8]); }
    fn serialize_none(self) -> JsonResult<usize> {
        unsupported()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> JsonResult<usize> {
        unsupported()
    }
    fn serialize_unit(self) -> JsonResult<usize> {
        unsupported()
    }
    fn serialize_unit_struct(self, _: &'static str) -> JsonResult<usize> {
        unsupported()
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> JsonResult<usize> {
        unsupported()
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: &T,
    ) -> JsonResult<usize> {
        unsupported()
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> JsonResult<usize> {
        unsupported()
    }
    fn serialize_seq(self, _: Option<usize>) -> JsonResult<Self::SerializeSeq> {
        unsupported()
    }
    fn serialize_tuple(self, _: usize) -> JsonResult<Self::SerializeTuple> {
        unsupported()
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> JsonResult<Self::SerializeTupleStruct> {
        unsupported()
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> JsonResult<Self::SerializeTupleVariant> {
        unsupported()
    }
    fn serialize_map(self, _: Option<usize>) -> JsonResult<Self::SerializeMap> {
        unsupported()
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> JsonResult<Self::SerializeStruct> {
        Ok(FieldsSizer {
            cached: self.cached,
            bytes: 2,
            fields: 0,
        })
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> JsonResult<Self::SerializeStructVariant> {
        unsupported()
    }
}
