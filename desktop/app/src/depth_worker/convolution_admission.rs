//! Next-stage convolution contract. Not an installed inference backend.
use crate::admission::Result;

#[derive(Clone, Copy)]
pub enum Kind {
    CrossCorrelation,
    TransposeNonoverlap,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attributes {
    pub kernel: [u32; 2],
    pub stride: [u32; 2],
    pub padding: [u32; 4], // top,left,bottom,right
    pub dilation: [u32; 2],
    pub group: u32,
    pub output_padding: [u32; 2],
}
pub struct Plan {
    pub entry: &'static str,
    pub parameters: [u32; 16],
    pub dispatch: [u32; 3],
    pub logical_buffer_bytes: u64,
}
fn require(value: bool, cause: &str) -> Result<()> {
    if value { Ok(()) } else { Err(cause.into()) }
}
fn bytes(shape: &[u32]) -> Result<u64> {
    shape
        .iter()
        .try_fold(1u32, |count, &d| {
            require(d > 0, "zero convolution dimension")?;
            count
                .checked_mul(d)
                .ok_or_else(|| "convolution element count overflow".into())
        })
        .map(|n| u64::from(n) * 4)
}

pub fn plan(
    kind: Kind,
    input: [u32; 4],
    weights: [u32; 4],
    bias_elements: Option<u32>,
    expected: [u32; 4],
    attributes: &Attributes,
    limits: &wgpu::Limits,
    budget: u64,
) -> Result<Plan> {
    let image_bytes = bytes(&input)?;
    let weights_bytes = bytes(&weights)?;
    let output_bytes = bytes(&expected)?;
    require(
        attributes.group == 1 && attributes.dilation == [1, 1],
        "only group1/dilation1 convolution admitted",
    )?;
    require(
        attributes.kernel == weights[2..] && !attributes.stride.contains(&0),
        "convolution kernel/stride mismatch",
    )?;
    require(
        attributes.output_padding == [0, 0],
        "output padding is not implemented",
    )?;
    let [n, ci, ih, iw] = input;
    let (co, entry) = match kind {
        Kind::CrossCorrelation => {
            require(weights[1] == ci, "Conv OIHW input channels differ")?;
            (weights[0], "conv_nchw")
        }
        Kind::TransposeNonoverlap => {
            require(weights[0] == ci, "ConvTranspose IOHW input channels differ")?;
            require(
                attributes.stride == attributes.kernel && attributes.padding == [0, 0, 0, 0],
                "only nonoverlapping unpadded transpose convolution admitted",
            )?;
            require(
                matches!(attributes.kernel, [2, 2] | [4, 4]),
                "transpose kernel outside audited Small model contract",
            )?;
            (weights[1], "conv_transpose_no_overlap_nchw")
        }
    };
    let spatial = match kind {
        Kind::CrossCorrelation => {
            let dimension =
                |size: u32, k: u32, stride: u32, before: u32, after: u32| -> Result<u32> {
                    let padded = u64::from(size) + u64::from(before) + u64::from(after);
                    require(
                        padded >= u64::from(k) && padded <= i32::MAX as u64,
                        "invalid or signed-overflowing convolution padding",
                    )?;
                    let out = (padded - u64::from(k)) / u64::from(stride) + 1;
                    u32::try_from(out).map_err(|_| "convolution output dimension overflow".into())
                };
            [
                dimension(
                    ih,
                    weights[2],
                    attributes.stride[0],
                    attributes.padding[0],
                    attributes.padding[2],
                )?,
                dimension(
                    iw,
                    weights[3],
                    attributes.stride[1],
                    attributes.padding[1],
                    attributes.padding[3],
                )?,
            ]
        }
        Kind::TransposeNonoverlap => [
            ih.checked_mul(weights[2])
                .ok_or("transpose height overflow")?,
            iw.checked_mul(weights[3])
                .ok_or("transpose width overflow")?,
        ],
    };
    require(
        expected == [n, co, spatial[0], spatial[1]],
        "convolution output extent mismatch",
    )?;
    // The signed shader coordinates must remain representable before padding
    // subtraction, including windows at the final output pixel.
    for axis in 0..2 {
        let maximum = u64::from(spatial[axis] - 1) * u64::from(attributes.stride[axis])
            + u64::from(attributes.kernel[axis] - 1);
        require(
            maximum <= i32::MAX as u64,
            "convolution signed sampling coordinate overflow",
        )?;
    }
    let bias_bytes = match bias_elements {
        Some(elements) => {
            require(elements == co, "convolution bias width differs")?;
            u64::from(elements) * 4
        }
        None => 4,
    };
    for count in [image_bytes, weights_bytes, output_bytes, bias_bytes] {
        require(
            count <= limits.max_storage_buffer_binding_size && count <= limits.max_buffer_size,
            "convolution buffer exceeds device limits",
        )?;
    }
    let logical = output_bytes
        .checked_mul(2)
        .and_then(|n| n.checked_add(image_bytes))
        .and_then(|n| n.checked_add(weights_bytes))
        .and_then(|n| n.checked_add(bias_bytes))
        .and_then(|n| n.checked_add(64))
        .ok_or("convolution total allocation overflow")?;
    require(
        logical <= budget,
        "convolution logical buffer budget exhausted",
    )?;
    require(
        limits.max_compute_invocations_per_workgroup >= 64
            && limits.max_compute_workgroup_size_x >= 64
            && limits.max_storage_buffers_per_shader_stage >= 4
            && limits.max_uniform_buffers_per_shader_stage >= 1,
        "convolution device compute/binding limits insufficient",
    )?;
    let groups = (output_bytes / 4).div_ceil(64);
    let maximum = u64::from(limits.max_compute_workgroups_per_dimension);
    require(maximum > 0, "zero compute dispatch limit")?;
    let x = groups.min(maximum);
    let y = groups.div_ceil(x);
    require(y <= maximum, "convolution dispatch exceeds device limits")?;
    let flat_x = u32::try_from(
        x.checked_mul(64)
            .ok_or("convolution dispatch stride overflow")?,
    )
    .map_err(|_| "convolution dispatch stride exceeds u32")?;
    let mut p = [0; 16];
    p[..4].copy_from_slice(&input);
    p[4..8].copy_from_slice(&[co, spatial[0], spatial[1], weights[2]]);
    p[8..12].copy_from_slice(&[
        weights[3],
        attributes.stride[0],
        attributes.stride[1],
        attributes.padding[0],
    ]);
    p[12..16].copy_from_slice(&[
        attributes.padding[1],
        flat_x,
        u32::from(bias_elements.is_some()),
        0,
    ]);
    Ok(Plan {
        entry,
        parameters: p,
        dispatch: [x as u32, y as u32, 1],
        logical_buffer_bytes: logical,
    })
}
