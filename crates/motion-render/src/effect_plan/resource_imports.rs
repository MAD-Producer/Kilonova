//! Package resources describe shader-visible numeric channels, never inferred
//! working color. Metadata is retained with resolved code before CPU geometry.
use super::*;
use motion_effects::{
    execution_plan::{CoordinateDomain, ImageValueType, PixelRect},
    image_contract::{DataChannels, ImageContract, ImageMeaning, ImageStorage},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum ResourceEncoding {
    NumericChannels,
    /// The shader explicitly reconstructs a little-endian f32 from four bytes.
    /// The sampled image itself remains four UNORM8 channels, not float color.
    LittleEndianF32 {
        scalars: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PackageImageImport {
    package_hash: String,
    path: String,
    value_type: ImageValueType,
    storage: ImageStorage,
    encoding: ResourceEncoding,
}
impl PackageImageImport {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn value_type(&self) -> ImageValueType {
        self.value_type
    }
    pub fn storage(&self) -> ImageStorage {
        self.storage
    }
    pub fn encoding(&self) -> ResourceEncoding {
        self.encoding
    }
    pub fn bytes(&self) -> u64 {
        u64::from(self.value_type.rect.size[0]) * u64::from(self.value_type.rect.size[1]) * 4
    }
    pub(crate) fn validate_texture(
        &self,
        width: u32,
        height: u32,
        storage: ImageStorage,
    ) -> Result<(), RenderError> {
        if [width, height] != self.value_type.rect.size || storage != self.storage {
            return Err(format!(
                "phase=resource_import: {} actual={width}x{height}/{storage:?} expected={:?}/{:?}",
                self.path.chars().take(192).collect::<String>(),
                self.value_type.rect.size,
                self.storage
            )
            .into());
        }
        Ok(())
    }
}

pub(super) fn package_images(
    package: &EffectPackage,
    def: &EffectDefinition,
) -> Result<Arc<[PackageImageImport]>, RenderError> {
    if def.resources.len() > 4 {
        return Err("package resource binding limit exceeded".into());
    }
    let mut imports = Vec::with_capacity(def.resources.len());
    for path in &def.resources {
        let bytes = package
            .files
            .get(path)
            .ok_or("package image resource is missing")?;
        let (width, height) =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png)
                .into_dimensions()
                .map_err(|cause| {
                    let cause = RenderError::from(cause);
                    let message = format!(
                        "package={} resource={} phase=resource_import_dimensions: {cause}",
                        package.hash.chars().take(64).collect::<String>(),
                        path.chars().take(192).collect::<String>()
                    );
                    cause.with_diagnostic(message)
                })?;
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err("package resource rectangle exceeds logical coordinate limits".into());
        }
        let encoding = def
            .shader_data
            .as_ref()
            .and_then(|s| s.overflow.as_ref())
            .filter(|s| &s.path == path)
            .map_or(ResourceEncoding::NumericChannels, |s| {
                ResourceEncoding::LittleEndianF32 { scalars: s.scalars }
            });
        if let ResourceEncoding::LittleEndianF32 { scalars } = encoding {
            if [width, height]
                != [
                    motion_effects::shader_data::SHADER_DATA_TEXTURE_WIDTH,
                    scalars.div_ceil(motion_effects::shader_data::SHADER_DATA_TEXTURE_WIDTH),
                ]
            {
                return Err("private byte-container extent differs from its declaration".into());
            }
        }
        imports.push(PackageImageImport {
            package_hash: package.hash.clone(),
            path: path.clone(),
            storage: ImageStorage::Rgba8Unorm,
            encoding,
            value_type: ImageValueType {
                image: ImageContract::new(
                    ImageMeaning::Data {
                        channels: DataChannels::Vector4,
                    },
                    ChannelPrecision::Unorm8,
                ),
                rect: PixelRect {
                    origin: [0, 0],
                    size: [width, height],
                },
                coordinates: CoordinateDomain::Resource,
                depth: None,
            },
        });
    }
    Ok(imports.into())
}

pub(super) fn validate_program(program: &EffectProgram) -> Result<(), RenderError> {
    if program.resources.len() != program.imports.len()
        || program
            .resources
            .iter()
            .zip(program.imports.iter())
            .any(|(path, import)| path != import.path())
    {
        return Err("phase=resource_import: native resource declarations differ from resolved image contracts".into());
    }
    for import in program.imports.iter() {
        if program
            .package
            .as_ref()
            .is_none_or(|package| package.hash != import.package_hash)
            || import.value_type.image.preferred_storage()? != import.storage
        {
            return Err("phase=resource_import: native package identity/storage differs from its logical contract".into());
        }
    }
    Ok(())
}

pub(super) fn lower_package_images(
    nodes: &mut Vec<motion_effects::execution_plan::ExecutionNode>,
    bindings: &mut Vec<EffectOperationBinding>,
    frame: &EffectFramePlan,
    programs: &[EffectProgram],
    scene: &Scene,
) -> Result<(), RenderError> {
    use motion_effects::execution_plan::*;
    use std::collections::BTreeMap;
    let mut sources = BTreeMap::new();
    let operations = bindings.len();
    for operation in 0..operations {
        for pass in &frame.passes[bindings[operation].original_passes.clone()] {
            let program = programs
                .get(pass.program as usize)
                .ok_or("import program is absent")?;
            validate_program(program)?;
            // These modes replace the complete package resource bind group.
            // Package defaults may still occupy a native cache, but are not
            // sampled inputs of this operation.
            if pass.resource_input != 0 || pass.instance_data != 0 {
                continue;
            }
            for (slot, import) in program.imports.iter().enumerate() {
                let package = program.package.as_ref().ok_or("import package is absent")?;
                let key = (package.hash.clone(), import.path.clone(), import.encoding);
                let value = if let Some(&value) = sources.get(&key) {
                    value
                } else {
                    if nodes.len() >= MAX_EXECUTION_NODES {
                        return Err(ExecutionPlanError::Budget {
                            node: None,
                            resource: "nodes",
                            required: nodes.len() as u64 + 1,
                            maximum: MAX_EXECUTION_NODES as u64,
                        }
                        .into());
                    }
                    let node = ExecutionNodeId(nodes.len() as u32);
                    let value = ImageValueId(node.0);
                    let draw = bindings[operation].draw;
                    nodes.push(ExecutionNode {
                        id: node,
                        output: value,
                        output_type: import.value_type,
                        origin: OperationOrigin {
                            composition: scene.composition_id.clone(),
                            layer: scene.layers[draw].id,
                            effect: None,
                        },
                        inputs: vec![],
                        semantics: OperationSemantics {
                            domain: ExecutionDomain::Source,
                            time: TimeDependency::Independent,
                            halo: Some([0; 4]),
                            stateful: false,
                            side_effects: false,
                        },
                        cost: OperationCost {
                            shader_bytes: 0,
                            static_loop_work: 0,
                            samples_per_fragment: Some(0),
                            passes: 0,
                        },
                        code_identity: [0; 32],
                        uniform_cells: 0,
                    });
                    bindings.push(EffectOperationBinding {
                        node,
                        draw,
                        scene_effect: None,
                        original_passes: 0..0,
                        output_region: [
                            0.,
                            0.,
                            import.value_type.rect.size[0] as f32,
                            import.value_type.rect.size[1] as f32,
                        ],
                        resource_tokens: vec![],
                        lookup_tables: vec![],
                        instance_data: vec![],
                        imported_image: Some(import.clone()),
                    });
                    sources.insert(key, value);
                    value
                };
                let role = ImagePortRole::Resource {
                    // Package slots and dynamic image/private overrides share
                    // the same actual group-2 bindings and are alternatives.
                    index: slot as u16,
                };
                if !nodes[operation]
                    .inputs
                    .iter()
                    .any(|p| p.role == role && p.value == value)
                {
                    nodes[operation].inputs.push(ImageInputPort {
                        role,
                        value,
                        expected: import.value_type,
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
