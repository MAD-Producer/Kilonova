//! Independently authored analytic image and f64 storage/output reference.
//! The oracle never reads GPU pixels or calls the production shader/compositor.
use motion_core::{EffectInstance, Layer, Project};
use motion_effects::{
    AlphaMode, EdgeMode, EffectPackage, PassDefinition, Registry, WorkingSpace,
    image_contract::ChannelPrecision,
};
use motion_render::{CapturedImage, OutputTransform, ToneMap};
use std::sync::Arc;

pub const W: u32 = 65;
pub const H: u32 = 33;
pub const MODES: [ChannelPrecision; 4] = [
    ChannelPrecision::Unorm8,
    ChannelPrecision::Unorm16,
    ChannelPrecision::Float16,
    ChannelPrecision::Float32,
];
pub const TRANSFORM: OutputTransform = OutputTransform {
    tone_map: ToneMap::Reinhard,
    exposure_stops: 0.,
};

pub fn package() -> Arc<EffectPackage> {
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let base = manifest
        .effects
        .iter()
        .find(|d| d.id == "tint")
        .unwrap()
        .clone();
    manifest.id = "com.motionstudio.tests.precision-goldens".into();
    manifest.effects.clear();
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    for (id, sources) in [
        (
            "glow",
            vec![
                "fn main_fx(p:vec2<f32>)->vec4<f32>{
                let u=p.x/fx.size.x;let v=p.y/fx.size.y;
                let a=select(0.25+0.5*v,0.0,u<0.0625||u>0.9375)*sample_input(p).a;
                let g=max(0.0,1.0-abs(u-0.5)*4.0-abs(v-0.5)*2.0);
                return vec4(vec3(0.08+3.0*g,0.1+0.7*u+1.5*g,0.15+0.4*v+0.75*g)*a,a);}",
                "fn main_fx(p:vec2<f32>)->vec4<f32>{
                return sample_input(p)*0.5+(sample_input(p+vec2(1.0,0.0))+
                sample_input(p-vec2(1.0,0.0))+sample_input(p+vec2(0.0,1.0))+
                sample_input(p-vec2(0.0,1.0)))*0.125;}",
            ],
        ),
        (
            "back",
            vec!["fn main_fx(p:vec2<f32>)->vec4<f32>{return vec4(0.125,0.25,0.375,1.0);}"],
        ),
        (
            "wash",
            vec!["fn main_fx(p:vec2<f32>)->vec4<f32>{return vec4(0.125,0.25,0.0625,0.5);}"],
        ),
    ] {
        let mut definition = base.clone();
        definition.id = id.into();
        definition.working_space = WorkingSpace::Linear;
        definition.alpha_mode = AlphaMode::Premultiplied;
        definition.edge_mode = EdgeMode::Clamp;
        definition.passes.clear();
        for (index, source) in sources.iter().enumerate() {
            let shader = format!("shaders/{id}-{index}.wgsl");
            std::fs::write(directory.path().join(&shader), source).unwrap();
            definition.passes.push(PassDefinition {
                shader,
                entry: "main_fx".into(),
            });
        }
        manifest.effects.push(definition);
    }
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(directory.path()).unwrap())
            .unwrap(),
    )
}
pub fn layer(id: u64, effect: &str, package: &EffectPackage) -> Layer {
    let mut layer = Layer::solid(
        id,
        effect,
        [W as f32, H as f32],
        [W as f32 / 2., H as f32 / 2., 0.],
        [1.; 4],
    );
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        package
            .manifest
            .effects
            .iter()
            .find(|d| d.id == effect)
            .unwrap(),
        layer.size,
    ));
    layer
}
pub fn project(mode: ChannelPrecision, layers: Vec<Layer>) -> Project {
    let mut p = Project::new(W, H, 30, 12).unwrap();
    p.working_precision = mode;
    p.background = [0.; 4];
    p.layers = layers;
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    p
}
pub fn registry(package: Arc<EffectPackage>) -> Registry {
    let mut r = Registry::new_with_builtins().unwrap();
    r.insert(package).unwrap();
    r
}
pub fn encode(v: f64) -> f64 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}
fn decode(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn half(v: f64) -> f64 {
    if v == 0. {
        return v;
    }
    let step = 2_f64.powi((v.abs().log2().floor() as i32 - 10).max(-24));
    (v / step).round_ties_even() * step
}
pub fn scalar(v: f64, mode: ChannelPrecision) -> f64 {
    match mode {
        ChannelPrecision::Unorm8 => (v.clamp(0., 1.) * 255.).round_ties_even() / 255.,
        ChannelPrecision::Unorm16 => (v.clamp(0., 1.) * 65535.).round_ties_even() / 65535.,
        ChannelPrecision::Float16 => half(v),
        ChannelPrecision::Float32 => v as f32 as f64,
    }
}
pub fn store(v: [f64; 4], mode: ChannelPrecision) -> [f64; 4] {
    std::array::from_fn(|c| {
        if mode == ChannelPrecision::Unorm8 && c < 3 {
            decode((encode(v[c].clamp(0., 1.)) * 255.).round_ties_even() / 255.)
        } else {
            scalar(v[c], mode)
        }
    })
}
pub fn resolve(v: [f64; 4], mode: ChannelPrecision) -> [f64; 4] {
    if mode != ChannelPrecision::Unorm8 {
        return store(v, mode);
    }
    // The 8-bit temporal delivery contract bounds straight SDR color and
    // reassociates it with representable byte alpha at each child boundary.
    let alpha = scalar(v[3], mode);
    store(
        std::array::from_fn(|c| {
            if c == 3 {
                alpha
            } else if v[3] > 0. {
                (v[c] / v[3]).clamp(0., 1.) * alpha
            } else {
                0.
            }
        }),
        mode,
    )
}
fn source(x: i32, y: i32, mode: ChannelPrecision, coverage: f64) -> [f64; 4] {
    let u = (f64::from(x.clamp(0, W as i32 - 1)) + 0.5) / f64::from(W);
    let v = (f64::from(y.clamp(0, H as i32 - 1)) + 0.5) / f64::from(H);
    let a = if !(0.0625..=0.9375).contains(&u) {
        0.
    } else {
        0.25 + 0.5 * v
    };
    let a = a * coverage;
    let g = (1. - (u - 0.5).abs() * 4. - (v - 0.5).abs() * 2.).max(0.);
    store(
        [
            (0.08 + 3. * g) * a,
            (0.1 + 0.7 * u + 1.5 * g) * a,
            (0.15 + 0.4 * v + 0.75 * g) * a,
            a,
        ],
        mode,
    )
}
pub fn glow(x: i32, y: i32, mode: ChannelPrecision) -> [f64; 4] {
    glow_coverage(x, y, mode, 1.)
}
pub fn glow_coverage(x: i32, y: i32, mode: ChannelPrecision, coverage: f64) -> [f64; 4] {
    let center = source(x, y, mode, coverage);
    let neighbors = [
        source(x - 1, y, mode, coverage),
        source(x + 1, y, mode, coverage),
        source(x, y - 1, mode, coverage),
        source(x, y + 1, mode, coverage),
    ];
    store(
        std::array::from_fn(|c| {
            center[c] * 0.5 + neighbors.iter().map(|v| v[c]).sum::<f64>() * 0.125
        }),
        mode,
    )
}
pub fn pixels(mut pixel: impl FnMut(i32, i32) -> [f64; 4]) -> Vec<[f64; 4]> {
    (0..H as i32)
        .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
        .map(|(x, y)| pixel(x, y))
        .collect()
}
pub fn linear(image: &CapturedImage) -> Vec<[f64; 4]> {
    let values: Vec<f64> = match image.contract.precision {
        ChannelPrecision::Unorm8 => image.pixels.iter().map(|v| f64::from(*v) / 255.).collect(),
        ChannelPrecision::Unorm16 => image
            .pixels
            .chunks_exact(2)
            .map(|v| f64::from(u16::from_le_bytes(v.try_into().unwrap())) / 65535.)
            .collect(),
        ChannelPrecision::Float16 => image
            .pixels
            .chunks_exact(2)
            .map(|v| {
                let bits = u16::from_le_bytes(v.try_into().unwrap());
                let exponent = i32::from((bits >> 10) & 31);
                assert_ne!(exponent, 31);
                let mantissa = f64::from(bits & 1023);
                let value = if exponent == 0 {
                    mantissa * 2_f64.powi(-24)
                } else {
                    (1024. + mantissa) * 2_f64.powi(exponent - 25)
                };
                if bits & 0x8000 == 0 { value } else { -value }
            })
            .collect(),
        ChannelPrecision::Float32 => image
            .pixels
            .chunks_exact(4)
            .map(|v| f64::from(f32::from_le_bytes(v.try_into().unwrap())))
            .collect(),
    };
    values
        .chunks_exact(4)
        .map(|v| {
            std::array::from_fn(|c| {
                if c < 3 && image.contract.precision == ChannelPrecision::Unorm8 {
                    decode(v[c])
                } else {
                    v[c]
                }
            })
        })
        .collect()
}
pub fn png(pixels: &[[f64; 4]]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|p| {
            std::array::from_fn::<_, 4, _>(|c| {
                let value = if c == 3 {
                    p[3].clamp(0., 1.)
                } else if p[3] > 0.000001 {
                    let v = (p[c] / p[3]).max(0.);
                    encode(v / (1. + v))
                } else {
                    0.
                };
                (value.clamp(0., 1.) * 255.).round_ties_even() as u8
            })
        })
        .collect()
}
pub fn compare(
    name: &str,
    mode: ChannelPrecision,
    actual: &[[f64; 4]],
    expected: &[[f64; 4]],
    display: &[u8],
) {
    assert_eq!(actual.len(), expected.len());
    let tolerance = match mode {
        ChannelPrecision::Unorm8 => 0.035,
        ChannelPrecision::Unorm16 => 0.00025,
        ChannelPrecision::Float16 => 0.015,
        ChannelPrecision::Float32 => 0.00005,
    };
    let (index, error) = actual
        .iter()
        .zip(expected)
        .enumerate()
        .flat_map(|(i, (a, b))| (0..4).map(move |c| (i * 4 + c, (a[c] - b[c]).abs())))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    let golden = png(expected);
    let (png_index, max_png) = display
        .iter()
        .zip(&golden)
        .enumerate()
        // Straight RGB is unobservable when both quantized alphas are zero.
        // Tiny interpolation residues can retain valid straight color there;
        // alpha itself and every nontransparent output channel remain checked.
        .filter(|(i, _)| i % 4 == 3 || display[i / 4 * 4 + 3] != 0 || golden[i / 4 * 4 + 3] != 0)
        .map(|(i, (a, b))| (i, a.abs_diff(*b)))
        .max_by_key(|(_, error)| *error)
        .unwrap();
    // A one-byte alpha difference at nearly transparent pixels can change
    // straight RGB substantially without changing the displayed edge. Compare
    // decoded premultiplied linear output (the compositing contract), and bound
    // alpha independently. Keep raw straight-byte differences as diagnostics.
    let alpha_error = display
        .chunks_exact(4)
        .zip(golden.chunks_exact(4))
        .map(|(a, b)| a[3].abs_diff(b[3]))
        .max()
        .unwrap();
    let output_error = display
        .chunks_exact(4)
        .zip(golden.chunks_exact(4))
        .flat_map(|(a, b)| {
            (0..3).map(move |c| {
                let a = decode(f64::from(a[c]) / 255.) * f64::from(a[3]) / 255.;
                let b = decode(f64::from(b[c]) / 255.) * f64::from(b[3]) / 255.;
                (a - b).abs()
            })
        })
        .fold(0_f64, f64::max);
    let output_tolerance = if mode == ChannelPrecision::Unorm8 {
        4. / 255.
    } else {
        2. / 255.
    };
    let alpha_tolerance = if mode == ChannelPrecision::Unorm8 {
        2
    } else {
        1
    };
    if let Some(root) = std::env::var_os("MOTION_PRECISION_GOLDEN_ARTIFACTS") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        let prefix = format!("{name}-{mode:?}");
        for (suffix, data) in [("actual", display), ("expected", golden.as_slice())] {
            image::save_buffer(
                root.join(format!("{prefix}-{suffix}.png")),
                data,
                W,
                H,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        std::fs::write(root.join(format!("{prefix}-metrics.json")),serde_json::to_vec_pretty(&serde_json::json!({
            "mode":format!("{mode:?}"),"size":[W,H],"maxLinearError":error,"linearTolerance":tolerance,
            "maxStraightByteDifference":max_png,"maxOutputPremultipliedLinearError":output_error,
            "outputPremultipliedLinearTolerance":output_tolerance,"maxAlphaByteError":alpha_error,"alphaByteTolerance":alpha_tolerance,
            "outputComparison":"decoded premultiplied linear RGB plus independent alpha; raw straight-byte difference is diagnostic",
            "oracle":"independent f64 analytic field, explicit format stores and Reinhard/sRGB output",
            "actualLinear":actual,"expectedLinear":expected})).unwrap()).unwrap();
    }
    assert!(
        error <= tolerance,
        "{name} {mode:?} linear error={error} tolerance={tolerance} pixel={} actual={:?} expected={:?}",
        index / 4,
        actual[index / 4],
        expected[index / 4]
    );
    assert!(
        output_error <= output_tolerance && alpha_error <= alpha_tolerance,
        "{name} {mode:?} SDR premultiplied error={output_error} tolerance={output_tolerance} alpha error={alpha_error} tolerance={alpha_tolerance}; largest straight-byte difference={max_png} pixel={} actual={:?} expected={:?}",
        png_index / 4,
        &display[png_index / 4 * 4..png_index / 4 * 4 + 4],
        &golden[png_index / 4 * 4..png_index / 4 * 4 + 4]
    );
}
