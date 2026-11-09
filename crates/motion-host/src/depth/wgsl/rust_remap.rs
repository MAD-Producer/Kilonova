//! Owned scalar bilinear remapping, with explicit float32 intermediates.
//! RGB preparation remains Python/OpenCV. This procedure removes no failure
//! record and does not assert the cause of an external runtime access violation.
use super::*;

fn plan() -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"MotionStudio/depth/rust-align-corners-remap/v1\0");
    hash.update(include_str!("rust_remap.rs").as_bytes());
    hash.finalize().into()
}

pub(super) fn resolve(input: &Input, cancellation: &Cancellation) -> Result<Execution> {
    log::info!(
        "operation=depth_wgsl asset={} phase=resolve_host_remap start",
        input.key().source.asset
    );
    log::logger().flush();
    let executable = std::env::current_exe()?;
    let binary = identity::worker_digest(&executable, cancellation)?;
    let mut runtime = Sha256::new();
    runtime.update(b"MotionStudio/depth/rust-remap-host-runtime/v1\0");
    runtime.update(binary);
    runtime.update(std::env::consts::ARCH.as_bytes());
    runtime.update([0]);
    runtime.update(std::env::consts::OS.as_bytes());
    let execution = Execution {
        backend: "host-remap".into(),
        provider: "MotionStudio:Rust".into(),
        precision: "float32".into(),
        runtime_sha256: runtime.finalize().into(),
        plan_sha256: plan(),
    };
    execution.validate()?;
    Ok(execution)
}

pub(super) fn combine(gpu: &Execution, remap: &Execution) -> Result<Execution> {
    gpu.validate()?;
    remap.validate()?;
    if gpu.backend != "wgsl"
        || gpu.precision != "float32"
        || !gpu.provider.starts_with("WGSL:")
        || remap.backend != "host-remap"
        || remap.provider != "MotionStudio:Rust"
        || remap.precision != "float32"
        || remap.plan_sha256 != plan()
    {
        return Err(Error::Invalid("host remap execution identity differs"));
    }
    let mut execution = gpu.clone();
    let mut runtime = Sha256::new();
    runtime.update(b"MotionStudio/depth/wgsl-retained-host-remap-runtime/v1\0");
    runtime.update(serde_json::to_vec(&(gpu, remap))?);
    execution.runtime_sha256 = runtime.finalize().into();
    let mut procedure = Sha256::new();
    procedure.update(b"MotionStudio/depth/wgsl-retained-host-remap-plan/v1\0");
    procedure.update(gpu.plan_sha256);
    procedure.update(remap.plan_sha256);
    execution.plan_sha256 = procedure.finalize().into();
    execution.validate()?;
    Ok(execution)
}

fn rectangle(size: [u32; 2]) -> Result<[usize; 4]> {
    if size.contains(&0) || size.iter().any(|&n| n > 4096) {
        return Err(Error::Invalid("host remap dimensions exceed 1..4096"));
    }
    let [width, height] = size.map(f64::from);
    let side = SIDE as usize;
    let scale = (side as f64 / width).min(side as f64 / height);
    let rw = ((width * scale + 0.5).floor() as usize).clamp(1, side);
    let rh = ((height * scale + 0.5).floor() as usize).clamp(1, side);
    Ok([(side - rw) / 2, (side - rh) / 2, rw, rh])
}

// Endpoints are exact. Interior coordinates round the double-precision step
// product once to float32, before deriving the float32 interpolation weight.
fn axis(source: usize, destination: usize) -> Vec<(usize, usize, f32)> {
    let step = if destination > 1 {
        (source - 1) as f64 / (destination - 1) as f64
    } else {
        0.
    };
    (0..destination)
        .map(|i| {
            let x = if destination > 1 && i == destination - 1 {
                (source - 1) as f32
            } else {
                (i as f64 * step) as f32
            };
            let low = x.floor() as usize;
            (low, (low + 1).min(source - 1), x - low as f32)
        })
        .collect()
}

fn remap(plane: &[f32], size: [u32; 2], cancellation: &Cancellation) -> Result<Vec<f32>> {
    cancellation.check()?;
    let [ox, oy, iw, ih] = rectangle(size)?;
    let side = SIDE as usize;
    if plane.len() != side * side || plane.iter().any(|n| !n.is_finite()) {
        return Err(Error::Invalid(
            "host remap requires one finite 518-square plane",
        ));
    }
    let [width, height] = size.map(|n| n as usize);
    let xs = axis(iw, width);
    let ys = axis(ih, height);
    let mut output = Vec::with_capacity(width * height);
    for &(y0, y1, ty) in &ys {
        cancellation.check()?;
        for &(x0, x1, tx) in &xs {
            let a = plane[(oy + y0) * side + ox + x0];
            let b = plane[(oy + y0) * side + ox + x1];
            let c = plane[(oy + y1) * side + ox + x0];
            let d = plane[(oy + y1) * side + ox + x1];
            // Keep the same two horizontal float32 interpolations followed by
            // the vertical interpolation. Do not replace them with mul_add or
            // a differently associated/f64 expression.
            let top = a * (1. - tx) + b * tx;
            let bottom = c * (1. - tx) + d * tx;
            let value = top * (1. - ty) + bottom * ty;
            if !value.is_finite() {
                return Err(Error::Invalid("nonfinite host-remapped depth"));
            }
            output.push(value);
        }
    }
    cancellation.check()?;
    Ok(output)
}

pub(super) fn run(
    root: &Path,
    input: &Input,
    rgb: &str,
    depth: Option<&str>,
    cancellation: &Cancellation,
) -> Result<Transform> {
    log::info!(
        "operation=depth_wgsl asset={} phase=remap_rust start",
        input.key().source.asset
    );
    log::logger().flush();
    let began = Instant::now();
    let result = (|| {
        cancellation.check()?;
        let hash = depth.ok_or(Error::Invalid("host remap missing native depth digest"))?;
        let bytes = read_owned(&root.join("depth518.f32"), DEPTH_BYTES, hash)?;
        let plane: Vec<_> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let values = remap(&plane, input.size(), cancellation)?;
        let output: Vec<_> = values.iter().flat_map(|n| n.to_le_bytes()).collect();
        cancellation.check()?;
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("output.f32"))?;
        for chunk in output.chunks(64 << 10) {
            cancellation.check()?;
            file.write_all(chunk)?;
        }
        cancellation.check()?;
        Ok(Transform {
            process_memory: Some(crate::process_memory::ProcessPeak::current()),
            schema: 1,
            stage: "remap".into(),
            processing: super::super::onnx::PROCESSING.into(),
            width: input.size()[0],
            height: input.size()[1],
            rgb_sha256: rgb.into(),
            depth_sha256: Some(hash.into()),
            output_sha256: digest(&output),
            output_bytes: output.len() as u64,
            elapsed_ns: u64::try_from(began.elapsed().as_nanos()).unwrap_or(u64::MAX),
        })
    })();
    result.map_err(|error| {
        if matches!(error, Error::Canceled) {
            error
        } else {
            Error::Backend(format!(
                "WGSL phase=remap_rust elapsed_ms={} cause={error}",
                began.elapsed().as_millis()
            ))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Input {
        use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, DepthTime};
        Input::new(
            Key {
                source: DepthSource {
                    asset: 75,
                    revision: 1,
                    pts: DepthTime {
                        ticks: 0,
                        timescale: 1,
                    },
                },
                geometry: DepthGeometry {
                    source_size: [7, 5],
                    crop: [0, 0, 7, 5],
                    quarter_turns: 0,
                    mirror_x: false,
                },
                model: DepthModel {
                    id: "owned-remap-control".into(),
                    revision: "r1".into(),
                    sha256: [7; 32],
                    processing: super::super::super::onnx::PROCESSING.into(),
                },
            },
            [7, 5],
            std::sync::Arc::from(vec![127; 7 * 5 * 3]),
        )
        .unwrap()
    }

    #[test]
    fn owned_output_verifies_digest_and_preserves_an_existing_publication() {
        let root = tempfile::tempdir().unwrap();
        let input = sample();
        let values = vec![16f32; SIDE as usize * SIDE as usize];
        let bytes: Vec<_> = values.iter().flat_map(|n| n.to_le_bytes()).collect();
        fs::write(root.path().join("depth518.f32"), &bytes).unwrap();
        let rgb = digest(input.rgb());
        let depth = digest(&bytes);
        assert!(
            run(
                root.path(),
                &input,
                &rgb,
                Some(&"0".repeat(64)),
                &Cancellation::default()
            )
            .is_err()
        );
        assert!(!root.path().join("output.f32").exists());
        let report = run(
            root.path(),
            &input,
            &rgb,
            Some(&depth),
            &Cancellation::default(),
        )
        .unwrap();
        report
            .validate(&input, "remap", &rgb, Some(&depth))
            .unwrap();
        let output = fs::read(root.path().join("output.f32")).unwrap();
        assert_eq!(output.len(), 7 * 5 * 4);
        assert_eq!(digest(&output), report.output_sha256);
        assert!(
            output
                .chunks_exact(4)
                .all(|b| f32::from_le_bytes(b.try_into().unwrap()) == 16.)
        );
        assert!(
            run(
                root.path(),
                &input,
                &rgb,
                Some(&depth),
                &Cancellation::default()
            )
            .is_err()
        );
        assert_eq!(fs::read(root.path().join("output.f32")).unwrap(), output);
    }

    #[test]
    fn canceled_host_remap_does_not_read_input_or_create_an_output() {
        let root = tempfile::tempdir().unwrap();
        let canceled = Cancellation::default();
        canceled.cancel();
        assert!(matches!(
            run(root.path(), &sample(), "unused", None, &canceled),
            Err(Error::Canceled)
        ));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn exact_endpoints_and_single_pixel_axes_do_not_sample_padding() {
        let mut plane = vec![-999.; SIDE as usize * SIDE as usize];
        let [x, y, w, h] = rectangle([7, 5]).unwrap();
        for j in 0..h {
            for i in 0..w {
                plane[(y + j) * SIDE as usize + x + i] = (j * 1000 + i) as f32;
            }
        }
        let output = remap(&plane, [7, 5], &Cancellation::default()).unwrap();
        assert_eq!(output.len(), 35);
        assert_eq!(output[0], 0.);
        assert_eq!(output[6], (w - 1) as f32);
        assert_eq!(output[28], ((h - 1) * 1000) as f32);
        assert_eq!(output[34], ((h - 1) * 1000 + w - 1) as f32);
        assert!(output.iter().all(|n| *n >= 0.));
        for size in [[1, 1], [1, 4096], [4096, 1]] {
            let [x, y, _, _] = rectangle(size).unwrap();
            let output = remap(&plane, size, &Cancellation::default()).unwrap();
            assert_eq!(output[0], plane[y * SIDE as usize + x]);
            assert_eq!(output.len(), size[0] as usize * size[1] as usize);
        }
    }

    #[test]
    fn constant_and_affine_planes_follow_independent_source_coordinates() {
        let side = SIDE as usize;
        for size in [[518, 518], [640, 480], [3, 7], [37, 19], [23, 51]] {
            let constant =
                remap(&vec![17.25; side * side], size, &Cancellation::default()).unwrap();
            assert!(constant.iter().all(|n| (*n - 17.25).abs() <= 0.000004));
            let plane: Vec<_> = (0..side * side)
                .map(|i| (i % side) as f32 + 2. * (i / side) as f32)
                .collect();
            let output = remap(&plane, size, &Cancellation::default()).unwrap();
            let [ox, oy, iw, ih] = rectangle(size).unwrap();
            for row in 0..size[1] as usize {
                for col in 0..size[0] as usize {
                    let x = ox as f64 + col as f64 * (iw - 1) as f64 / (size[0] - 1) as f64;
                    let y = oy as f64 + row as f64 * (ih - 1) as f64 / (size[1] - 1) as f64;
                    assert!(
                        (f64::from(output[row * size[0] as usize + col]) - (x + 2. * y)).abs()
                            < 0.0005
                    );
                }
            }
        }
    }

    #[test]
    fn wrong_extent_nonfinite_input_invalid_dimensions_and_cancellation_are_refused() {
        let plane = vec![1.; SIDE as usize * SIDE as usize];
        for size in [[0, 1], [1, 0], [4097, 1], [1, 4097], [u32::MAX, u32::MAX]] {
            assert!(remap(&plane, size, &Cancellation::default()).is_err());
        }
        assert!(remap(&plane[..plane.len() - 1], [2, 2], &Cancellation::default()).is_err());
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut bad = plane.clone();
            bad[0] = value;
            assert!(remap(&bad, [2, 2], &Cancellation::default()).is_err());
        }
        let canceled = Cancellation::default();
        canceled.cancel();
        assert!(matches!(
            remap(&plane, [2, 2], &canceled),
            Err(Error::Canceled)
        ));
    }

    #[test]
    fn host_procedure_and_runtime_cannot_masquerade_as_the_python_only_mode() {
        let base = Execution {
            backend: "wgsl".into(),
            provider: "WGSL:Vulkan:controlled".into(),
            precision: "float32".into(),
            runtime_sha256: [7; 32],
            plan_sha256: [8; 32],
        };
        let local = Execution {
            backend: "host-remap".into(),
            provider: "MotionStudio:Rust".into(),
            precision: "float32".into(),
            runtime_sha256: [9; 32],
            plan_sha256: plan(),
        };
        let combined = combine(&base, &local).unwrap();
        assert_eq!(combined.provider, base.provider);
        assert_ne!(combined.runtime_sha256, base.runtime_sha256);
        assert_ne!(combined.plan_sha256, base.plan_sha256);
        let mut other = local.clone();
        other.runtime_sha256 = [10; 32];
        assert_ne!(
            combined.runtime_sha256,
            combine(&base, &other).unwrap().runtime_sha256
        );
        other.plan_sha256 = [11; 32];
        assert!(combine(&base, &other).is_err());
    }
}
