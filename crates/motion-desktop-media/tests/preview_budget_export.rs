use motion_core::{Command, Layer, Project};
use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};
use motion_host::Session;
use motion_render::resource_policy::MemoryPolicy;

#[test]
fn frozen_png_keeps_full_resolution_with_the_minimum_preview_allowance() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.layers.push(Layer::solid(
        1,
        "frozen red",
        [64.; 2],
        [32., 32., 0.],
        [1., 0., 0., 1.],
    ));
    let mut session =
        Session::new(project.clone(), root, motion_desktop_media::platform()).unwrap();
    let policy = MemoryPolicy {
        preview_target_budget_bytes: 8,
        ..Default::default()
    };
    session.configure_memory_policy(policy).unwrap();
    let control = ExportControl::default();
    let output = directory.path().join("full-resolution.png");
    let plan = FrameFreezePlan::capture(&session, Some(output.clone()), &control).unwrap();
    assert_eq!(
        session.engine.snapshot(),
        project,
        "configuring and freezing must not edit the project"
    );

    // The exported frame owns its snapshot; a live edit, policy change and
    // session closure must not replace it or reduce its output resolution.
    session
        .engine
        .apply(Command::SetColor {
            object: 1,
            value: [0., 0., 1., 1.],
        })
        .unwrap();
    session
        .configure_memory_policy(MemoryPolicy::default())
        .unwrap();
    drop(session);
    plan.export(&control).unwrap();
    let actual = image::open(&output).unwrap().to_rgba8();
    assert_eq!(actual.dimensions(), (64, 64));
    assert!(actual.pixels().all(|pixel| pixel.0 == [255, 0, 0, 255]));
}
