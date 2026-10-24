use super::*;
use motion_core::Command;
use motion_effects::image_contract::ChannelPrecision;
#[path = "../../../../motion-render/tests/support/sdr_fixture.rs"]
mod fixture;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn frozen_png_preserves_four_modes_and_exposure_output_after_live_edits_and_session_close() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        for exposure in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let (project, registry) = fixture::project(precision, exposure);
            let mut session = Session::new(project, root.path().into(), crate::platform()).unwrap();
            session.effects.set_registry(registry);
            session.frame = 2.25;
            session.sample().unwrap();
            let output = root.path().join("native-output.png");
            let control = ExportControl::default();
            let plan = FrameFreezePlan::capture(&session, Some(output.clone()), &control).unwrap();
            assert_eq!(plan.sources.project.working_precision, precision);
            let revision = session.engine.revision();
            session
                .engine
                .apply_batch(vec![Command::SetWorkingPrecision {
                    precision: ChannelPrecision::Unorm8,
                }])
                .unwrap();
            drop(session);
            let result = plan.export(&control).unwrap();
            assert_eq!(result["revision"].as_u64(), Some(revision));
            assert_eq!(result["frame"].as_f64(), Some(2.25));
            assert_eq!(
                Path::new(result["path"].as_str().unwrap())
                    .canonicalize()
                    .unwrap(),
                output.canonicalize().unwrap()
            );
            fixture::assert_png(image::open(&output).unwrap().into_rgba8().as_raw());
            if let Some(directory) = std::env::var_os("MOTION_PRECISION_OUTPUT_ARTIFACTS") {
                let directory = PathBuf::from(directory);
                fs::create_dir_all(&directory).unwrap();
                fs::copy(
                    &output,
                    directory.join(format!("{precision:?}-exposure{exposure}.png")),
                )
                .unwrap();
            }
        }
    }
}
