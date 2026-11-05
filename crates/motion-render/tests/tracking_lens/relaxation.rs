//! Nonconstant, actual native pixels exercise the new certified inverse step.
use super::*;

fn channel(bytes: &[u8], precision: ChannelPrecision, pixel: usize, component: usize) -> f64 {
    match precision {
        ChannelPrecision::Unorm8 => {
            let v = f64::from(bytes[pixel * 4 + component]) / 255.;
            if component == 3 {
                v
            } else if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        ChannelPrecision::Unorm16 | ChannelPrecision::Float16 => {
            let at = (pixel * 4 + component) * 2;
            let v = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
            if precision == ChannelPrecision::Unorm16 {
                return f64::from(v) / 65535.;
            }
            // Independent IEEE binary16 decoding; no product output conversion.
            let sign = if v & 0x8000 == 0 { 1. } else { -1. };
            let exponent = i32::from((v >> 10) & 31);
            let fraction = f64::from(v & 1023);
            assert_ne!(exponent, 31, "finite probe channels required");
            sign * if exponent == 0 {
                fraction * 2f64.powi(-24)
            } else {
                (1. + fraction / 1024.) * 2f64.powi(exponent - 15)
            }
        }
        ChannelPrecision::Float32 => {
            let at = (pixel * 4 + component) * 4;
            f64::from(f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()))
        }
    }
}

fn linear_bilinear(
    bytes: &[u8],
    precision: ChannelPrecision,
    size: [u32; 2],
    at: [f64; 2],
) -> [f64; 4] {
    let x = at[0].floor() as i32;
    let y = at[1].floor() as i32;
    let fx = at[0] - f64::from(x);
    let fy = at[1] - f64::from(y);
    let mut result = [0.; 4];
    for (dx, dy, weight) in [
        (0, 0, (1. - fx) * (1. - fy)),
        (1, 0, fx * (1. - fy)),
        (0, 1, (1. - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let px = x + dx;
        let py = y + dy;
        if px < 0 || py < 0 || px >= size[0] as i32 || py >= size[1] as i32 {
            continue;
        }
        let pixel = py as usize * size[0] as usize + px as usize;
        for (i, v) in result.iter_mut().enumerate() {
            *v += channel(bytes, precision, pixel, i) * weight;
        }
    }
    result
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn strong_photographed_lens_matches_independent_rays_in_all_precisions_and_rotations() {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../motion-model/tests/fixtures/tracking-opencv-inverse.json"
    ))
    .unwrap();
    let values = |key: &str| {
        data[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>()
    };
    let source_extent = Extent::new(640, 480).unwrap();
    let calibration = Calibration::new(
        source_extent,
        values("focal").try_into().unwrap(),
        values("principal").try_into().unwrap(),
        values("radial").try_into().unwrap(),
        values("tangential").try_into().unwrap(),
    )
    .unwrap();
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut rows = Vec::new();
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::HalfTurn,
        Rotation::Clockwise270,
    ] {
        let lens = LensWarp::new(
            calibration,
            PlateTransform::new(
                source_extent,
                [0., 0., 640., 480.],
                rotation,
                Extent::new(W, H).unwrap(),
            )
            .unwrap(),
            1.,
        )
        .unwrap();
        assert!(lens.inverse_step() > 1. && lens.derivative_bound() < 0.45);
        let raster = lens.raster(1.).unwrap();
        let size = raster.size();
        let rect = raster.rect();
        for precision in [
            ChannelPrecision::Unorm8,
            ChannelPrecision::Unorm16,
            ChannelPrecision::Float16,
            ChannelPrecision::Float32,
        ] {
            let source = renderer
                .image_capture_target(size[0], size[1], contract(precision))
                .unwrap();
            let destination = renderer
                .image_capture_target(W, H, contract(precision))
                .unwrap();
            let pipeline = LensOutputPipeline::new(
                &renderer,
                image_formats::native_format(destination.storage()),
            )
            .unwrap();
            let hdr = matches!(
                precision,
                ChannelPrecision::Float16 | ChannelPrecision::Float32
            );
            let body=format!("let ideal=vec2({:.10},{:.10})+p.xy*vec2({:.10},{:.10})/vec2({:.10},{:.10});let t=ideal/vec2({:.10},{:.10});return {};",
                rect[0],rect[1],rect[2],rect[3],f64::from(size[0]),f64::from(size[1]),f64::from(W),f64::from(H),
                if hdr {"vec4(-0.4+0.05*t.x,2.+0.1*t.y,0.0625,0.5)"}else{"vec4(0.1+0.15*t.x,0.2+0.15*t.y,0.0625,0.5)"});
            fill(&renderer, &source, &body);
            let original = renderer.read_image_target(&source).unwrap();
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            pipeline
                .encode_images(&renderer, &mut encoder, lens, raster, &source, &destination)
                .unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    ledger.merge(source.texture_resource_ledger());
                    ledger.merge(destination.texture_resource_ledger());
                })
                .unwrap();
            let actual = renderer.read_image_target(&destination).unwrap();
            let mut maximum = 0f64;
            for y in 0..H {
                for x in 0..W {
                    // Existing separate analytic-Jacobian Newton oracle, never LensWarp::inverse.
                    let q = super::oracle(lens, [f64::from(x) + 0.5, f64::from(y) + 0.5]);
                    let at = [
                        (q[0] - rect[0]) / rect[2] * f64::from(size[0]) - 0.5,
                        (q[1] - rect[1]) / rect[3] * f64::from(size[1]) - 0.5,
                    ];
                    let expected = linear_bilinear(&original.pixels, precision, size, at);
                    let pixel = y as usize * W as usize + x as usize;
                    for (i, v) in expected.into_iter().enumerate() {
                        let got = channel(&actual.pixels, precision, pixel, i);
                        let error = match precision {
                            ChannelPrecision::Unorm8 => {
                                let encoded = if i == 3 {
                                    v
                                } else if v <= 0.0031308 {
                                    12.92 * v
                                } else {
                                    1.055 * v.powf(1. / 2.4) - 0.055
                                };
                                (f64::from(actual.pixels[pixel * 4 + i]) - (encoded * 255.).round())
                                    .abs()
                            }
                            ChannelPrecision::Unorm16 => (got - v).abs() * 65535.,
                            _ => (got - v).abs(),
                        };
                        let tolerance = match precision {
                            ChannelPrecision::Unorm8 => 2.,
                            ChannelPrecision::Unorm16 => 3.,
                            ChannelPrecision::Float16 => 0.002,
                            ChannelPrecision::Float32 => 0.00002,
                        };
                        maximum = maximum.max(error);
                        assert!(error<=tolerance,"{rotation:?} {precision:?} pixel {x},{y} channel {i}: {got} != {v}, error={error}");
                    }
                    if hdr {
                        assert!(
                            channel(&actual.pixels, precision, pixel, 0) < 0.
                                && channel(&actual.pixels, precision, pixel, 1) > 1.
                        );
                    }
                }
            }
            rows.push(serde_json::json!({"rotation":format!("{rotation:?}"),"precision":format!("{precision:?}"),"inverseStep":lens.inverse_step(),
                "contractionBound":lens.derivative_bound(),"checkedPixels":W*H,"maximumError":maximum,"negativeHdrAndAlpha":hdr}));
        }
    }
    if let Some(directory) = std::env::var_os("MOTION_LENS_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("strong-lens-precision.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
