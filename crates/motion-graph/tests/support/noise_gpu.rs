use super::*;
#[path = "noise_fixture.rs"]
mod fixture;

fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = ((x as u32) ^ seed)
        .wrapping_mul(1_000_003)
        .wrapping_add((y as u32).wrapping_add(65_537).wrapping_mul(1_000_033));
    h = (h ^ (h >> 16)).wrapping_mul(1_000_037);
    h = (h ^ (h >> 13)).wrapping_mul(1_000_039);
    h ^ (h >> 16)
}

// f64 weighted corner sum, with trigonometric gradients rather than shader's
// lookup/switch and nested mixes. No GPU output is used to construct the oracle.
fn reference(p: [f64; 2], seed: u32, mode: NoiseMode) -> f64 {
    let p = p.map(|v| v.clamp(-1_000_000., 1_000_000.));
    let cell = p.map(|v| v.floor() as i32);
    let t = [p[0] - p[0].floor(), p[1] - p[1].floor()];
    let fade = t.map(|v| 6. * v.powi(5) - 15. * v.powi(4) + 10. * v.powi(3));
    let mut sum = 0.;
    for y in 0..=1 {
        for x in 0..=1 {
            let h = hash(cell[0] + x, cell[1] + y, seed);
            let value = if mode == NoiseMode::Value {
                f64::from(h >> 8) / 16_777_215.
            } else {
                let angle = f64::from(h & 7) * std::f64::consts::FRAC_PI_4;
                angle.cos() * (t[0] - f64::from(x)) + angle.sin() * (t[1] - f64::from(y))
            };
            sum += value
                * (if x == 0 { 1. - fade[0] } else { fade[0] })
                * (if y == 0 { 1. - fade[1] } else { fade[1] });
        }
    }
    if mode == NoiseMode::Perlin {
        (0.5 + 0.5 * sum).clamp(0., 1.)
    } else {
        sum
    }
}

#[test]
fn noise_modes_and_four_octaves_match_independent_cpu_pixels() {
    let dir = tempfile::tempdir().unwrap();
    for mode in [NoiseMode::Value, NoiseMode::Perlin] {
        for octaves in 1..=4 {
            let mut graph = fixture::noise(mode, octaves);
            let defaults = &mut graph.nodes[1].input_defaults;
            defaults.insert("offset".into(), InputValue::Vec2([-0.13, 0.27]));
            defaults.insert("scale".into(), InputValue::Float(7.3));
            defaults.insert("seed".into(), InputValue::Float(21.75));
            let (p, mut r, t) = setup(&graph, &dir.path().join(format!("{mode:?}-{octaves}.msfx")));
            let pixels = render(&p, &mut r, &t, 0.);
            for y in 0..64 {
                for x in 0..64 {
                    let p = [
                        ((x as f64 + 0.5) / 64. - 0.13) * 7.3,
                        ((y as f64 + 0.5) / 64. + 0.27) * 7.3,
                    ];
                    let mut expected = 0.;
                    let mut weight = 0.;
                    for octave in 0..octaves {
                        let frequency = (1u32 << octave) as f64;
                        expected += reference(
                            p.map(|v| v * frequency),
                            21 + u32::from(octave) * 7919,
                            mode,
                        ) / frequency;
                        weight += 1. / frequency;
                    }
                    let expected = (expected / weight * 255.).round() as u8;
                    let actual = pixel(&pixels, x, y);
                    assert!(actual[..3].iter().all(|v| v.abs_diff(expected) <= 2),
                        "{mode:?} octaves={octaves} ({x},{y}) actual={actual:?} expected={expected}");
                    assert_eq!(actual[3], 255);
                }
            }
        }
    }
}

#[test]
fn noise_seed_flooring_recovery_bounds_and_determinism_hold_on_gpu() {
    let dir = tempfile::tempdir().unwrap();
    let sequence = std::cell::Cell::new(0usize);
    for mode in [NoiseMode::Value, NoiseMode::Perlin] {
        let capture = |seed, scale, offset| {
            let mut graph = fixture::noise(mode, 4);
            let defaults = &mut graph.nodes[1].input_defaults;
            defaults.insert("seed".into(), InputValue::Float(seed));
            defaults.insert("scale".into(), InputValue::Float(scale));
            defaults.insert("offset".into(), InputValue::Vec2(offset));
            let index = sequence.get();
            sequence.set(index + 1);
            let (p, mut r, t) = setup(&graph, &dir.path().join(format!("seed-{index}.msfx")));
            let a = render(&p, &mut r, &t, 0.);
            assert_eq!(a, render(&p, &mut r, &t, 3.));
            a
        };
        let a = capture(21.1, 8., [0.; 2]);
        assert_eq!(a, capture(21.9, 8., [0.; 2]));
        assert_ne!(a, capture(22., 8., [0.; 2]));
        let zero_scale = capture(0., 0., [0.; 2]);
        assert_eq!(capture(-10., -1., [0.; 2]), zero_scale);
        let constant = |position: [f64; 2], seed: u32| {
            let mut sum = 0.;
            for octave in 0..4u32 {
                sum += reference(position, seed + octave * 7919, mode) / f64::from(1u32 << octave);
            }
            (sum / 1.875 * 255.).round() as u8
        };
        let expected = constant([0.; 2], 0);
        assert!(zero_scale.chunks_exact(4).all(|p|
            p[..3].iter().all(|v| v.abs_diff(expected) <= 2) && p[3] == 255));
        assert_eq!(
            capture(100_000., 5000., [0.; 2]),
            capture(65535., 4096., [0.; 2])
        );
        let bounded = capture(3., 4096., [1e6, -1e6]);
        let expected = constant([1e6, -1e6], 3);
        assert!(bounded
            .chunks_exact(4)
            .all(|p| p[..3].iter().all(|v| v.abs_diff(expected) <= 2) && p[3] == 255));
    }
}
