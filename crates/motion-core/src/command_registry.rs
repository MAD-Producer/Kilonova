//! The typed command enum is the editing protocol. No widget or host types.
use crate::Command;
use serde_json::{json, Value};

#[derive(Clone, Copy)]
enum FieldKind {
    Named(&'static str),
    FloatArray(usize),
}

macro_rules! field_kind {
    ([f32; $dimensions:literal]) => { FieldKind::FloatArray($dimensions) };
    ($name:literal) => { FieldKind::Named($name) };
}

#[derive(serde::Serialize)]
struct CommandDescriptor {
    id: &'static str,
    undoable: bool,
    batch_atomic: bool,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

pub struct CommandRegistry;
macro_rules! registry {
    ($($pattern:pat => $id:literal [$(( $name:literal,$kind:tt,$required:literal )),*];)*) => {
        impl CommandRegistry {
            /// Exhaustive matching makes every newly added command register here.
            pub fn id(command:&Command)->&'static str {match command {$($pattern=>$id,)*}}
            fn descriptors()->Vec<CommandDescriptor> {
                [$(descriptor($id,&[$(($name,field_kind!($kind),$required)),*])),*]
                    .into_iter().filter(|d| !d.id.starts_with('@')).collect()
            }
            pub fn catalogue()->Value {
                let commands=Self::descriptors();
                json!({"version":1,"protocol":"motion-core-command-v1","batch_atomic":true,
                    "command_id_field":"op","coordinates":"composition_frames",
                    "commands":commands})
            }
            /// Input schemas from the same typed descriptors as the catalogue.
            pub fn input_schemas()->Vec<Value> {
                Self::descriptors().into_iter().map(|d|d.input_schema).collect()
            }
            pub fn parse(text:&str)->crate::Result<Vec<Command>> {crate::parse_commands(text)}
            pub fn run(engine:&mut crate::Engine,commands:Vec<Command>)->crate::Result<Vec<crate::EditResult>> {
                engine.apply_batch_inner(commands,None)
            }
            /// Prepare external resources before committing model/history changes.
            /// The callback must leave its external state unchanged on error.
            pub fn run_prepared(engine:&mut crate::Engine,commands:Vec<Command>,prepare:impl FnOnce(&crate::Project)->crate::Result<()>)->crate::Result<Vec<crate::EditResult>> {
                engine.apply_batch_inner_prepared(commands,None,prepare)
            }
            pub(crate) fn run_saved(engine:&mut crate::Engine,commands:Vec<Command>,root:&std::path::Path)->crate::Result<Vec<crate::EditResult>> {
                engine.apply_batch_inner(commands,Some(root))
            }
        }
    }
}
fn named_schema(kind: &str) -> (bool, Value) {
    let (optional, kind) = if kind.starts_with("Option<") {
        (
            true,
            kind.trim_start_matches("Option<").trim_end_matches('>'),
        )
    } else {
        (false, kind)
    };
    (
        optional,
        match kind {
            "u64" | "u32" | "usize" => json!({"type":"integer","minimum":0}),
            "i32" => json!({"type":"integer"}),
            "f32" | "f64" => json!({"type":"number"}),
            "bool" => json!({"type":"boolean"}),
            "Vec<u64>" => json!({"type":"array","minItems":1,"maxItems":crate::MAX_LAYERS,"uniqueItems":true,"items":{"type":"integer","minimum":1}}),
            "String" => json!({"type":"string"}),
            "LayerSwitch" => json!({"type":"string","enum":["solo","effects","video","audio"]}),
            "Property" => {
                json!({"type":"string","enum":["anchor","position","rotation","scale","opacity","target","roll","fov","radius","azimuth","elevation"]})
            }
            "crate::Axis" => json!({"type":"string","enum":["x","y","z"]}),
            "Vec<KeyframeTarget>" => json!({"type":"array","minItems":1,"items":{"type":"object","description":"Typed KeyframeTarget channel address"}}),
            "Vec<KeyframeSelection>" => json!({"type":"array","minItems":1,"items":{"type":"object","additionalProperties":false,"required":["target","frames"],"properties":{"target":{"type":"object","description":"Typed KeyframeTarget channel address"},"frames":{"type":"array","minItems":1,"uniqueItems":true,"items":{"type":"integer","minimum":0}}}}}),
            "crate::geometry::RotationOrder" => {
                json!({"type":"string","enum":["local_xyz","fixed_xyz"]})
            }
            "CameraMode" => json!({"type":"string","enum":["position","orbit"]}),
            "ChannelPrecision" => json!({"type":"string","enum":["unorm8","unorm16","float16","float32"]}),
            "ParentMode" => json!({"type":"string","enum":["preserve_world","preserve_local"]}),
            "crate::compositing::BlendMode" => {
                json!({"type":"string","enum":["normal","add","multiply","screen","overlay","darken","lighten","difference","exclusion","subtract","divide","color_dodge","color_burn","hard_light","soft_light"]})
            }
            "crate::compositing::BlendSpace" => {
                json!({"type":"string","enum":["linear","srgb"]})
            }
            "Ease" => json!({"type":"string","enum":["linear","in","out","in_out","hold"]}),
            "crate::vector::ShapeKind" => json!({"type":"string"}),
            _ => json!({"type":"object","description":format!("Typed core {} payload",kind)}),
        },
    )
}

fn descriptor(id: &'static str, fields: &[(&str, FieldKind, bool)]) -> CommandDescriptor {
    let mut properties = serde_json::Map::new();
    properties.insert("op".into(), json!({"const":id}));
    // Composition routing is shared by GUI, CLI and MCP and remains explicit.
    properties.insert(
        "composition".into(),
        json!({"type":"string","description":"Stable composition ID; omitted means comp-main"}),
    );
    let mut required = vec!["op"];
    for (name, kind, is_required) in fields {
        let (optional, mut schema) = match *kind {
            FieldKind::FloatArray(dimensions) => (false,
                json!({"type":"array","minItems":dimensions,"maxItems":dimensions,"items":{"type":"number"}})),
            FieldKind::Named(kind) => named_schema(kind),
        };
        if optional {
            schema = json!({"anyOf":[schema,{"type":"null"}]});
        }
        properties.insert((*name).into(), schema);
        if *is_required && !optional {
            required.push(name);
        }
    }
    CommandDescriptor { id, undoable: true, batch_atomic: true,
        input_schema: json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}) }
}
registry! {
    Command::ApplyAnimationPreset { .. } => "apply_animation_preset" [("preset","AnimationPreset",true),("objects","Vec<u64>",true),("at_frame","u32",true)];
    Command::PasteLayers { .. } => "paste_layers" [("clipboard","crate::LayerClipboard",true),("at_frame","Option<u32>",false),("keep_external_references","bool",true)];
    Command::RemoveLayers { .. } => "remove_layers" [("objects","Vec<u64>",true),("frame","u32",true)];
    Command::AudioLevels { .. } => "audio_levels" [("object","u64",true), ("action","crate::AudioLevelsAction",true)];
    Command::AudioMarkers { .. } => "audio_markers" [("object","u64",true), ("action","crate::AudioMarkersAction",true)];
    Command::SplitAudioChannels { .. } => "split_audio_channels" [("object","u64",true)];
    Command::ExtractAudio { .. } => "extract_audio" [("object","u64",true)];
    Command::SetLayerRange { .. } => "set_layer_range" [("object","u64",true), ("in_frame","f64",true), ("out_frame","f64",true)];
    Command::SetColor { .. } => "set_color" [("object","u64",true), ("value",[f32;4],true)];
    Command::AddAdjustment { .. } => "add_adjustment" [("id","u64",true), ("name","String",true)];
    Command::AddShape { .. } => "add_shape" [("id","u64",true), ("name","String",true), ("shape","crate::vector::ShapeKind",true), ("size",[f32; 2],true), ("position",[f32; 3],true)];
    Command::Vector { .. } => "vector" [("object","u64",true), ("action","crate::vector::VectorAction",true)];
    Command::Mask { .. } => "mask" [("object","u64",true), ("action","crate::masks::MaskAction",true)];
    Command::SetLayerBlend { .. } => "set_layer_blend" [("object","u64",true), ("mode","Option<crate::compositing::BlendMode>",false), ("space","Option<crate::compositing::BlendSpace>",false)];
    Command::SetTrackMatte { .. } => "set_track_matte" [("object","u64",true), ("matte","Option<crate::compositing::TrackMatte>",false)];
    Command::InComposition { .. } => "@composition" [("composition","String",true), ("command","Box<Command>",true)];
    Command::Composition { .. } => "composition" [("action","crate::CompositionAction",true)];
    Command::RegisterAudioAsset { .. } => "register_audio_asset" [("asset","crate::AudioAsset",true)];
    Command::RegisterVideoAsset { .. } => "register_video_asset" [("asset","crate::VideoAsset",true)];
    Command::MediaAsset { .. } => "media_asset" [("action","crate::media_assets::AssetAction",true)];
    Command::SetAudio { .. } => "set_audio" [("object","u64",true), ("volume","Option<f32>",false), ("muted","Option<bool>",false)];
    Command::SetLayer3d { .. } => "set_layer_3d" [("object","u64",true), ("enabled","bool",true)];
    Command::SetLayerSampling { .. } => "set_layer_sampling" [("object","u64",true), ("sampling","crate::render_sampling::LayerSampling",true)];
    Command::SetRotationOrder { .. } => "set_rotation_order" [("object","u64",true), ("order","crate::geometry::RotationOrder",true)];
    Command::SetExpression { .. } => "set_expression" [("expression","crate::PropertyExpression",true), ("frame","u32",true)];
    Command::RemoveExpression { .. } => "remove_expression" [("target","crate::ExpressionTarget",true)];
    Command::Effect { .. } => "effect" [("object","u64",true), ("action","crate::EffectAction",true)];
    Command::SeparateDimensions { .. } => "separate_dimensions" [("object","u64",true), ("property","Property",true)];
    Command::MergeDimensions { .. } => "merge_dimensions" [("object","u64",true), ("property","Property",true)];
    Command::SetComponent { .. } => "set_component" [("object","u64",true), ("property","Property",true), ("axis","crate::Axis",true), ("frame","u32",true), ("value","f32",true)];
    Command::MoveLayerClip { .. } => "move_layer_clip" [("object","u64",true), ("in_frame","i32",true)];
    Command::TrimLayerClip { .. } => "trim_layer_clip" [("object","u64",true), ("in_frame","i32",true), ("out_frame","i32",true)];
    Command::SplitLayerClip { .. } => "split_layer_clip" [("object","u64",true), ("frame","u32",true)];
    Command::Curve { .. } => "curve" [("object","u64",true), ("property","Property",true), ("axis","Option<crate::Axis>",false), ("frame","u32",true), ("easing","Easing",true)];
    Command::Spatial { .. } => "spatial" [("object","u64",true), ("property","Property",true), ("frame","u32",true), ("tangents","Option<crate::SpatialTangents<[f32; 3]>",false)];
    Command::CreateCamera => "create_camera" [];
    Command::Remove { .. } => "remove" [("object","u64",true), ("frame","u32",true)];
    Command::Parent { .. } => "parent" [("object","u64",true), ("parent","Option<u64>",false), ("frame","u32",true)];
    Command::ParentWithMode { .. } => "parent_with_mode" [("object","u64",true), ("parent","Option<u64>",false), ("frame","u32",true), ("mode","ParentMode",true)];
    Command::CopyKey { .. } => "copy_key" [("object","u64",true), ("property","Property",true), ("axis","Option<crate::Axis>",false), ("from","u32",true), ("to","u32",true)];
    Command::PasteKeyframes { .. } => "paste_keyframes" [("clipboard","KeyframeClipboard",true), ("targets","Vec<KeyframeTarget>",true), ("at_frame","Option<u32>",false)];
    Command::RemoveKeyframes { .. } => "remove_keyframes" [("clipboard","KeyframeClipboard",true)];
    Command::KeyframeInterpolation { .. } => "keyframe_interpolation" [("selections","Vec<KeyframeSelection>",true),("ease","Ease",true)];
    Command::PasteEasing { .. } => "paste_easing" [("clipboard","CurveClipboard",true),("selections","Vec<KeyframeSelection>",true)];
    Command::KeyframeTiming { .. } => "keyframe_timing" [("selections","Vec<KeyframeSelection>",true),("settings","TimingSettings",true)];
    Command::EaseKeyframes { .. } => "ease_keyframes" [("selections","Vec<KeyframeSelection>",true),("incoming","bool",true),("outgoing","bool",true)];
    Command::DeleteKeyframes { .. } => "delete_keyframes" [("selections","Vec<KeyframeSelection>",true)];
    Command::TransformKeyframes { .. } => "transform_keyframes" [("selections","Vec<KeyframeSelection>",true),("delta_frame","i32",true),("value_offset","Option<KeyframeValueOffset>",false),("copy","bool",true)];
    Command::PastePaths { .. } => "paste_paths" [("clipboard","PathClipboard",true), ("target","PathDestination",true), ("at_frame","Option<u32>",false)];
    Command::RemovePaths { .. } => "remove_paths" [("clipboard","PathClipboard",true)];
    Command::CopyPathAs { .. } => "copy_path_as" [("target","PathTarget",true), ("path","u64",true)];
    Command::RegisterAsset { .. } => "register_asset" [("asset","Asset",true)];
    Command::RegisterModelAsset { .. } => "register_model_asset" [("asset","ModelAsset",true)];
    Command::RegisterFontAsset { .. } => "register_font_asset" [("asset","crate::FontAsset",true)];
    Command::Content { .. } => "content" [("object","u64",true), ("content","Content",true), ("size",[f32; 2],true)];
    Command::SetVector { .. } => "set_vector" [("object","u64",true), ("property","Property",true), ("frame","u32",true), ("value",[f32; 3],true)];
    Command::SetScalar { .. } => "set_scalar" [("object","u64",true), ("property","Property",true), ("frame","u32",true), ("value","f32",true)];
    Command::Animate { .. } => "animate" [("object","u64",true), ("property","Property",true), ("axis","Option<crate::Axis>",false), ("frame","u32",true), ("enabled","bool",true)];
    Command::MoveKey { .. } => "move_key" [("object","u64",true), ("property","Property",true), ("axis","Option<crate::Axis>",false), ("from","u32",true), ("to","u32",true)];
    Command::DeleteKey { .. } => "delete_key" [("object","u64",true), ("property","Property",true), ("axis","Option<crate::Axis>",false), ("frame","u32",true)];
    Command::Ease { .. } => "ease" [("object","u64",true), ("property","Property",true), ("axis","Option<crate::Axis>",false), ("frame","u32",true), ("ease","Ease",true)];
    Command::Add { .. } => "add" [("layer","Layer",true)];
    Command::Delete { .. } => "delete" [("object","u64",true)];
    Command::Duplicate { .. } => "duplicate" [("object","u64",true)];
    Command::Rename { .. } => "rename" [("object","u64",true), ("name","String",true)];
    Command::Reorder { .. } => "reorder" [("object","u64",true), ("index","usize",true)];
    Command::Flags { .. } => "flags" [("object","u64",true), ("visible","bool",true), ("locked","bool",true)];
    Command::SetLayerShy { .. } => "set_layer_shy" [("object","u64",true), ("shy","bool",true)];
    Command::SetLayerSwitch { .. } => "set_layer_switch" [("object","u64",true), ("switch","LayerSwitch",true), ("enabled","bool",true)];
    Command::SetWorkArea { .. } => "set_work_area" [("in_frame","u32",true), ("out_frame","u32",true)];
    Command::SetGuides { .. } => "set_guides" [("guides","Vec<Guide>",true)];
    Command::SetWorkingPrecision { .. } => "set_working_precision" [("precision","ChannelPrecision",true)];
    Command::Anchor { .. } => "anchor" [("object","u64",true), ("anchor",[f32; 2],true)];
    Command::AnchorAt { .. } => "anchor_at" [("object","u64",true), ("frame","u32",true), ("anchor",[f32; 2],true)];
    Command::CameraMode { .. } => "camera_mode" [("object","u64",false), ("mode","CameraMode",true)];
    Command::CameraLens { .. } => "camera_lens" [("object","u64",false), ("frame","u32",true), ("film_width_mm","f32",true), ("focal_length_mm","f32",true)];
    Command::Dolly { .. } => "dolly" [("object","u64",false), ("frame","u32",true), ("amount","f32",true)];
    Command::Pan { .. } => "pan" [("object","u64",false), ("frame","u32",true), ("x","f32",true), ("y","f32",true)];
}
