//! Structure-keyed private values for the editable compiler. Values never enter
//! shader strings or CSE keys. Small graphs use spare uniform components;
//! overflow preserves each f32 bit pattern in a bounded, unfiltered PNG.
use crate::{Error, ErrorCode, Result, ValueType};
use image::ImageEncoder;
use motion_effects::shader_data::*;
use std::collections::BTreeMap;

pub(crate) const DATA_PATH: &str = "assets/graph-data.png";
pub(crate) const HELPER: &str = "fn ng_literal_f32(i:u32)->f32{let bytes=vec4<u32>(round(textureLoad(resource0,vec2<i32>(i32(i%256u),i32(i/256u)),0)*255.0));return bitcast<f32>(bytes.x|(bytes.y<<8u)|(bytes.z<<16u)|(bytes.w<<24u));}\n";

pub(crate) struct NumericData {
    public_cells: usize,
    offsets: BTreeMap<(String, String, bool), usize>,
    values: Vec<f32>,
    exceeded: bool,
}

pub(crate) struct RuntimeData {
    pub declaration: ShaderData,
    pub resources: BTreeMap<String, Vec<u8>>,
}

impl NumericData {
    pub fn new(public_cells: usize) -> Self {
        Self {
            public_cells,
            offsets: BTreeMap::new(),
            values: vec![],
            exceeded: false,
        }
    }

    pub fn expression(
        &mut self,
        node: &str,
        port: &str,
        input: bool,
        ty: ValueType,
        values: &[f32],
    ) -> String {
        let key = (node.into(), port.into(), input);
        let start = if let Some(&offset) = self.offsets.get(&key) {
            offset
        } else {
            if self.values.len() + values.len() > MAX_SHADER_DATA_SCALARS {
                self.exceeded = true;
                return "0.0".into(); // finish() refuses before shader compilation
            }
            let start = self.values.len();
            self.values.extend_from_slice(values);
            self.offsets.insert(key, start);
            start
        };
        let uniform_capacity = (motion_effects::MAX_PARAMS - self.public_cells) * 4;
        let components: Vec<_> = (start..start + values.len())
            .map(|offset| {
                if offset < uniform_capacity {
                    format!(
                        "fx.params[{}].{}",
                        self.public_cells + offset / 4,
                        ["x", "y", "z", "w"][offset % 4]
                    )
                } else {
                    format!("ng_literal_f32({}u)", offset - uniform_capacity)
                }
            })
            .collect();
        if ty == ValueType::Bool {
            format!("({}>=0.5)", components[0])
        } else if components.len() == 1 {
            components[0].clone()
        } else {
            format!("{}({})", ty.wgsl(), components.join(","))
        }
    }

    pub fn has_overflow(&self) -> bool {
        self.values.len() > (motion_effects::MAX_PARAMS - self.public_cells) * 4
    }

    pub fn finish(&self) -> Result<RuntimeData> {
        if self.exceeded
            || self
                .values
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1.0e12)
        {
            return Err(Error::invalid(
                ErrorCode::Budget,
                None,
                None,
                "editable shader data exceeds its finite 16384-scalar budget",
            ));
        }
        let count = self
            .values
            .len()
            .min((motion_effects::MAX_PARAMS - self.public_cells) * 4);
        let mut uniforms = vec![[0.; 4]; count.div_ceil(4)];
        for (index, value) in self.values[..count].iter().enumerate() {
            uniforms[index / 4][index % 4] = *value;
        }
        let overflow = &self.values[count..];
        let mut resources = BTreeMap::new();
        let texture = if overflow.is_empty() {
            None
        } else {
            let width = SHADER_DATA_TEXTURE_WIDTH;
            let height = (overflow.len() as u32).div_ceil(width);
            let mut pixels = vec![0u8; width as usize * height as usize * 4];
            for (index, value) in overflow.iter().enumerate() {
                pixels[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
            let mut png = vec![];
            image::codecs::png::PngEncoder::new(&mut png)
                .write_image(&pixels, width, height, image::ExtendedColorType::Rgba8)
                .map_err(|cause| {
                    Error::effect(
                        ErrorCode::Publication,
                        "shader data encoding: ",
                        motion_effects::Error::ImageResource {
                            phase: "shader_data_encode",
                            path: DATA_PATH.into(),
                            cause: Box::new(cause),
                        },
                    )
                })?;
            resources.insert(DATA_PATH.into(), png);
            Some(ShaderDataTexture {
                path: DATA_PATH.into(),
                scalars: overflow.len() as u32,
            })
        };
        Ok(RuntimeData {
            declaration: ShaderData {
                version: SHADER_DATA_VERSION,
                uniforms,
                overflow: texture,
            },
            resources,
        })
    }
}
