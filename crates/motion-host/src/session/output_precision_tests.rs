use super::*;
use motion_effects::image_contract::ChannelPrecision;
#[path = "../../../motion-render/tests/support/sdr_fixture.rs"]
mod fixture;
struct Open(i64);
impl Drop for Open {
    fn drop(&mut self) {
        close(self.0);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_host_png_uses_project_precision_and_straight_sdr_pixels_without_editing_history() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let root = tempfile::tempdir().unwrap();
        let (project, registry) = fixture::project(precision, false);
        let id = Open(
            open(
                root.path().into(),
                project.clone(),
                Arc::new(last_error_observation_tests::Stub),
            )
            .unwrap(),
        );
        with_session(id.0, |s| {
            s.effects.set_registry(registry);
            s.frame = 2.;
            s.sample()?;
            Ok(())
        })
        .unwrap();
        let before = with_session(id.0, |s| {
            Ok((s.engine.revision(), s.view_revision, s.effects.precision()))
        })
        .unwrap();
        let result = crate::ops::export::capture(id.0).unwrap();
        let image = image::open(result["path"].as_str().unwrap())
            .unwrap()
            .into_rgba8();
        fixture::assert_png(image.as_raw());
        if let Some(directory) = std::env::var_os("MOTION_PRECISION_OUTPUT_ARTIFACTS") {
            let directory = PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::copy(
                result["path"].as_str().unwrap(),
                directory.join(format!("{precision:?}-host.png")),
            )
            .unwrap();
        }
        assert_eq!(
            (
                result["width"].as_u64(),
                result["height"].as_u64(),
                result["frame"].as_f64()
            ),
            (Some(18), Some(18), Some(2.))
        );
        with_session(id.0, |s| {
            assert_eq!(s.engine.snapshot(), project);
            assert_eq!(
                (s.engine.revision(), s.view_revision, s.effects.precision()),
                before
            );
            assert!(!s.engine.can_undo());
            Ok(())
        })
        .unwrap();
    }
}
