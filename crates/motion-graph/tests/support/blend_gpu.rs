use super::*;
#[path = "blend_fixture.rs"]
mod fixture;

// f64 oracle, expressed independently of the WGSL switch and premultiplied sum.
// The transfer is evaluated on straight RGB, then overlap/non-overlap areas are
// composed before division by output alpha.
fn reference(mode: ColorBlendMode, b: [f32; 4], s: [f32; 4], strength: f32) -> [f64; 4] {
    let b = b.map(|v| f64::from(v).clamp(0., 1.));
    let s = s.map(|v| f64::from(v).clamp(0., 1.));
    let sa = s[3] * f64::from(strength).clamp(0., 1.);
    let ba = b[3];
    let alpha = 1. - (1. - sa) * (1. - ba);
    if alpha == 0. {
        return [0.; 4];
    }
    let hard = |b: f64, s: f64| {
        if s <= 0.5 {
            2. * b * s
        } else {
            2. * (b + s - b * s) - 1.
        }
    };
    let transfer = |b: f64, s: f64| match mode {
        ColorBlendMode::Normal => s,
        ColorBlendMode::Add => (b + s).min(1.),
        ColorBlendMode::Multiply => b * s,
        ColorBlendMode::Screen => 1. - (1. - b) * (1. - s),
        ColorBlendMode::Overlay => hard(s, b),
        ColorBlendMode::Darken => b.min(s),
        ColorBlendMode::Lighten => b.max(s),
        ColorBlendMode::Difference => (b - s).abs(),
        ColorBlendMode::Exclusion => b * (1. - s) + s * (1. - b),
        ColorBlendMode::Subtract => (b - s).max(0.),
        ColorBlendMode::Divide => {
            if s == 0. {
                1.
            } else {
                (b / s).min(1.)
            }
        }
        ColorBlendMode::ColorDodge => {
            if b == 0. {
                0.
            } else if s == 1. {
                1.
            } else {
                (b / (1. - s)).min(1.)
            }
        }
        ColorBlendMode::ColorBurn => {
            if b == 1. {
                1.
            } else if s == 0. {
                0.
            } else {
                ((b + s - 1.) / s).max(0.)
            }
        }
        ColorBlendMode::HardLight => hard(b, s),
        ColorBlendMode::SoftLight => {
            if s <= 0.5 {
                b + (2. * s - 1.) * b * (1. - b)
            } else {
                let d = if b <= 0.25 {
                    16. * b.powi(3) - 12. * b.powi(2) + 4. * b
                } else {
                    b.sqrt()
                };
                b + (2. * s - 1.) * (d - b)
            }
        }
    };
    let mut result = [0.; 4];
    for c in 0..3 {
        result[c] =
            ((1. - sa) * ba * b[c] + sa * ((1. - ba) * s[c] + ba * transfer(b[c], s[c]))) / alpha;
    }
    result[3] = alpha;
    result
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn blend_all_modes_match_source_over_oracle_in_both_working_spaces_and_alpha_modes() {
    let dir = tempfile::tempdir().unwrap();
    let colors = [
        ([0.12, 0.5, 0.87, 1.], [0.81, 0.23, 0.55, 1.]),
        ([0., 0.25, 1., 1.], [1., 0.5, 0., 1.]),
        ([0.2, 0.4, 0.8, 0.5], [0.9, 0.1, 0.3, 0.5]),
        ([0.8, 0.1, 0.3, 0.], [0.1, 0.7, 0.4, 0.5]),
        ([0.3, 0.5, 0.7, 0.5], [0.8, 0.2, 0.9, 0.]),
        ([1., 0., 1., 0.], [0., 1., 0., 0.]),
    ];
    for mode in ColorBlendMode::ALL {
        for alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
            for space in [WorkingSpace::Srgb, WorkingSpace::Linear] {
                let mut graph = fixture::parameterized(mode);
                graph.alpha_mode = alpha;
                graph.working_space = space;
                let (mut project, mut renderer, target) = setup(
                    &graph,
                    &dir.path()
                        .join(format!("{mode:?}-{alpha:?}-{space:?}.msfx")),
                );
                for (b, s) in colors {
                    for strength in [-1., 0., 0.5, 1., 2.] {
                        let params = &mut project.layers[0].effects[0].params;
                        params.get_mut("backdrop").unwrap().track.value = b;
                        params.get_mut("source").unwrap().track.value = s;
                        params.get_mut("factor").unwrap().track.value = [strength, 0., 0., 0.];
                        let actual = render(&project, &mut renderer, &target, 0.);
                        let mut expected = reference(mode, b, s, strength);
                        if space == WorkingSpace::Linear {
                            for c in &mut expected[..3] {
                                *c = if *c <= 0.0031308 {
                                    12.92 * *c
                                } else {
                                    1.055 * c.powf(1. / 2.4) - 0.055
                                };
                            }
                        }
                        let sample = pixel(&actual, 32, 32);
                        for c in 0..4 {
                            let error = (f64::from(sample[c]) - expected[c] * 255.).abs();
                            assert!(error<=if c==3 {1.} else {3.},
                                "{mode:?} {alpha:?} {space:?} b={b:?} s={s:?} strength={strength} channel={c} actual={sample:?} expected={expected:?} error={error}");
                        }
                        assert!(actual.chunks_exact(4).all(|p| p == sample));
                    }
                }
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn blend_strength_animates_and_random_seek_restores_exported_example_pixels() {
    let dir = tempfile::tempdir().unwrap();
    let mut graph =
        GraphDocument::from_json(include_str!("../../examples/compact-blend.msgraph")).unwrap();
    let mut parameter = fixture::parameterized(ColorBlendMode::Normal);
    let factor = parameter.public_parameters.pop().unwrap();
    graph.public_parameters.push(factor);
    graph.nodes.push(Node::new(
        "factor",
        NodeOp::Parameter {
            parameter: "factor".into(),
        },
    ));
    graph
        .connections
        .push(Connection::new("factor", "value", "blend", "factor"));
    let (mut project, mut renderer, target) =
        setup(&graph, &dir.path().join("animated-blend.msfx"));
    let track = &mut project.layers[0].effects[0]
        .params
        .get_mut("factor")
        .unwrap()
        .track;
    track.upsert(0, [0.; 4], Ease::Linear).unwrap();
    track.upsert(30, [1., 0., 0., 0.], Ease::Linear).unwrap();
    let a = render(&project, &mut renderer, &target, 0.);
    let b = render(&project, &mut renderer, &target, 15.5);
    let c = render(&project, &mut renderer, &target, 30.);
    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_eq!(a, render(&project, &mut renderer, &target, 0.));
    if let Some(directory) = std::env::var_os("MOTION_BLEND_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        for (name, pixels) in [("gradient", a), ("half-strength", b), ("full-strength", c)] {
            image::save_buffer(
                directory.join(format!("{name}.png")),
                &pixels,
                64,
                64,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
