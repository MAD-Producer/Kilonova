use super::*;
use motion_core::{
    vector::{
        FillRule, PathNode, TangentMode, VectorAction, VectorContent, VectorPath, VectorSource,
    },
    Command, Content, Layer, Track,
};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_mp4_retains_frozen_automatic_tangents_after_live_mode_changes() {
    let root = tempfile::tempdir().unwrap();
    let mut project = Project::new(64, 64, 30, 6).unwrap();
    let path = VectorPath {
        id: 3,
        closed: true,
        tangents: TangentMode::Automatic,
        nodes: [[-20., -20.], [20., -20.], [20., 20.], [-20., 20.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| PathNode {
                id: i as u64 + 1,
                geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
            })
            .collect(),
    };
    let mut layer = Layer::solid(7, "Automatic path", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Vector {
        vector: VectorContent {
            source: VectorSource::Paths { paths: vec![path] },
            fill: Some(Track::constant([1.; 4])),
            fill_rule: FillRule::NonZero,
            stroke: None,
            trim: None,
        },
    };
    project.layers.push(layer);
    let mut session = Session::new(project.clone(), root.path().into(), crate::platform()).unwrap();
    let frozen = FrozenMovie::from_session(&session).unwrap();
    session
        .engine
        .apply(Command::Vector {
            object: 7,
            action: VectorAction::SetPathTangents {
                item: None,
                path: 3,
                mode: TangentMode::Manual,
            },
        })
        .unwrap();
    let manual = FrozenMovie::from_session(&session).unwrap();
    assert_eq!(frozen.project, project);
    assert_ne!(manual.project, project);
    drop(session);
    let auto_file = root.path().join("frozen-auto.mp4");
    let manual_file = root.path().join("live-manual.mp4");
    frozen
        .export(&auto_file, &ExportControl::default(), |_| {})
        .unwrap();
    manual
        .export(&manual_file, &ExportControl::default(), |_| {})
        .unwrap();
    let auto = super::tests::decoded_pixels(&auto_file);
    let manual_pixels = super::tests::decoded_pixels(&manual_file);
    for (a, m) in [(&auto.0, &manual_pixels.0), (&auto.1, &manual_pixels.1)] {
        assert_eq!(a.len(), 64 * 64 * 4);
        let different = a
            .chunks_exact(4)
            .zip(m.chunks_exact(4))
            .filter(|(a, m)| a[0].abs_diff(m[0]) > 32)
            .count();
        assert!(
            different > 40,
            "negative control must distinguish actual automatic and manual contours: {different}"
        );
    }
    let reference_session = Session::new(project, root.path().into(), crate::platform()).unwrap();
    let reference = FrozenMovie::from_session(&reference_session).unwrap();
    let reference_file = root.path().join("reference-auto.mp4");
    reference
        .export(&reference_file, &ExportControl::default(), |_| {})
        .unwrap();
    assert_eq!(auto, super::tests::decoded_pixels(&reference_file));
    if let Some(directory) = std::env::var_os("MOTION_PATH_TANGENT_MOVIE_ARTIFACTS") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory).unwrap();
        for (name, path) in [
            ("frozen-auto.mp4", auto_file),
            ("manual-negative-control.mp4", manual_file),
            ("reference-auto.mp4", reference_file),
        ] {
            fs::copy(path, directory.join(name)).unwrap();
        }
    }
}
