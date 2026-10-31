//! Packaged numeric-curve probe with a feature absent from the 256-point grid.
//! Explicit lookup input isolates LUT precision from source-image quantization.
use motion_core::{
    CurveObject, CurveTrack, Ease, EffectInstance, HIGH_PRECISION_CURVE_SAMPLES as N, Keyframe,
    Layer, Project, color_curves::ColorInterpolation, render_sampling::Shutter,
};
use motion_effects::{
    AlphaMode, EffectPackage, PassDefinition, Registry, WorkingSpace,
    image_contract::ChannelPrecision,
};
use std::sync::Arc;

pub fn project(mode: ChannelPrecision, exposure: bool) -> (Project, Registry) {
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let mut definition = manifest
        .effects
        .iter()
        .find(|e| e.id == "curves")
        .unwrap()
        .clone();
    definition.working_space = WorkingSpace::Linear;
    definition.alpha_mode = AlphaMode::Premultiplied;
    definition.resources.clear();
    definition.passes = vec![PassDefinition {
        shader: "shaders/dense.wgsl".into(),
        entry: "main_fx".into(),
    }];
    manifest.id = "com.motionstudio.tests.dense-output".into();
    manifest.effects = vec![definition];
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    std::fs::write(directory.path().join("shaders/dense.wgsl"),
        "fn main_fx(p:vec2<f32>)->vec4<f32>{let c=curve_lookup(vec4(2049.0/4080.0,0.25,0.125,1.0));return vec4(c.r,0.25,0.125,1.0);}").unwrap();
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let package = Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(directory.path()).unwrap())
            .unwrap(),
    );
    let mut effect = EffectInstance::new(
        1,
        &manifest.id,
        &manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [64.; 2],
    );
    let curve = effect
        .params
        .values_mut()
        .find_map(|p| p.curve.as_mut())
        .unwrap();
    let source = |height| {
        let x = 2049_f32 / (N - 1) as f32;
        let mut c = CurveObject::default();
        c.interpolation = [ColorInterpolation::Linear; 5];
        c.channels[1] = vec![
            [0., 0.],
            [x - 0.0001, 0.],
            [x, height],
            [x + 0.0001, 0.],
            [1., 0.],
        ];
        assert!(c.lut().unwrap().iter().all(|row| row[0] == 0.));
        let sampled = CurveTrack {
            value: c.clone(),
            keys: vec![],
        }
        .sampled_curve(0.)
        .unwrap();
        assert!((sampled.high_precision_lut()[2049][0] - height).abs() < 1e-6);
        c
    };
    *curve = CurveTrack {
        value: source(1.),
        keys: [(0, 1.), (4, 0.5)]
            .into_iter()
            .map(|(frame, height)| Keyframe {
                auto_bezier: false,
                frame,
                value: source(height),
                ease: Ease::Linear,
                curve: None,
                spatial: None,
            })
            .collect(),
    };
    let mut p = Project::new(32, 32, 30, 6).unwrap();
    p.working_precision = mode;
    p.background = [0.; 4];
    let mut l = Layer::solid(1, "Frozen dense curve", [64.; 2], [16., 16., 0.], [1.; 4]);
    l.effects.push(effect);
    if exposure {
        l.sampling.motion_blur = Some(Shutter {
            frames: 0.5,
            samples: 4,
        });
    }
    p.layers.push(l);
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let mut r = Registry::new_with_builtins().unwrap();
    r.insert(package).unwrap();
    (p, r)
}
pub fn color(mode: ChannelPrecision, frame: f64, coverage: f64) -> [u8; 4] {
    let height = if mode == ChannelPrecision::Unorm8 {
        0.
    } else {
        1. - frame.min(4.) / 8.
    };
    let encode = |x: f64| {
        let x = x * coverage;
        let x = if x <= 0.0031308 {
            12.92 * x
        } else {
            1.055 * x.powf(1. / 2.4) - 0.055
        };
        (x * 255.).round() as u8
    };
    [encode(height), encode(0.25), encode(0.125), 255]
}
