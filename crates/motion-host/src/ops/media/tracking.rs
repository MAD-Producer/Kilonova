//! Cold source-identity and native clock checks, always outside business locks.
use crate::{Platform, Result};
use motion_core::{
    media_assets::{lookup, MediaAsset},
    Project,
};
use std::{fs, path::Path};

pub(super) fn same_streams(expected: &Project, candidate: &Project, assets: &[u64]) -> Result<()> {
    for &id in assets {
        let descriptor = candidate
            .tracking_assets
            .iter()
            .find(|a| a.id == id)
            .ok_or("staged tracking attachment missing")?;
        if let Some(MediaAsset::Video(video)) = lookup(candidate, descriptor.source) {
            let Some(MediaAsset::Video(old)) = lookup(expected, descriptor.source) else {
                return Err("original tracking video missing".into());
            };
            // Two streams in the same container may share dimensions and PTS
            // while containing different images. Encoded-file identity alone
            // cannot authorize switching the stream used by an existing solve.
            if old.track != video.track {
                return Err(format!("tracking_asset={id} source=Video:{} original_track={} candidate_track={} phase=verify_tracking: replacement selects a different video stream", video.id, old.track, video.track).into());
            }
        }
    }
    Ok(())
}

pub(super) fn verify(
    root: &Path,
    staging: &Path,
    project: &Project,
    assets: &[u64],
    platform: &dyn Platform,
    request: &str,
    operation: &str,
    source_override: Option<&Path>,
) -> Result<()> {
    let staging = staging.canonicalize()?;
    for &id in assets {
        let descriptor = project
            .tracking_assets
            .iter()
            .find(|a| a.id == id)
            .ok_or("staged tracking attachment missing")?;
        let source = lookup(project, descriptor.source).ok_or("staged tracking source missing")?;
        log::info!("{operation} tracking start request={request} tracking_asset={id} source={:?}:{} phase=verify_tracking",
            descriptor.source.kind, descriptor.source.id);
        let result = (|| -> Result<()> {
            let path = staging
                .join(source_override.unwrap_or_else(|| Path::new(source.path())))
                .canonicalize()?;
            if !path.starts_with(&staging) {
                return Err("tracking source escapes staging".into());
            }
            let mut file = fs::File::open(&path)?;
            let before = file.metadata()?;
            if !before.is_file()
                || before.len() == 0
                || before.len() > motion_core::storage::MAX_MEDIA_ASSET
            {
                return Err("tracking source exceeds file budget".into());
            }
            let solve = crate::tracking::project::load_original(
                root,
                project,
                id,
                &mut file,
                before.len(),
                || false,
            )?;
            if let MediaAsset::Video(video) = &source {
                log::info!("{operation} tracking start request={request} tracking_asset={id} source=Video:{} track={} phase=decode_tracking", video.id, video.track);
                platform.verify_tracking_video(&path, video, &solve, &|| Ok(()))?;
                // The native decoder opens its own handle. Recheck the current
                // path after decoding, including any rename during acquisition.
                let mut current = fs::File::open(&path)?;
                crate::tracking::source::verify(&solve, &mut current, before.len(), || false)
                    .map_err(crate::tracking::project::Error::Source)?;
            }
            let after = file.metadata()?;
            if before.len() != after.len() || before.modified()? != after.modified()? {
                return Err("tracking source changed during verification".into());
            }
            Ok(())
        })();
        result.map_err(|cause| {
            crate::Error::with_context(
                format!(
                    "{operation} tracking_asset={id} source={:?}:{} phase=verify_tracking",
                    descriptor.source.kind, descriptor.source.id
                ),
                cause,
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion_core::tracking::{asset::*, plate::Extent, shared::SharedData};
    fn fixture() -> Project {
        let mut p = Project::new(8, 6, 24, 24).unwrap();
        p.video_assets.push(motion_core::VideoAsset {
            id: 1,
            path: "assets/plate.mp4".into(),
            bytes: 100,
            mime: "video/avc".into(),
            track: 0,
            width: 8,
            height: 6,
            rotation: 0,
            display_width: 8,
            display_height: 6,
            video_start_us: 0,
            video_end_us: 1_000_000,
            duration_us: 1_000_000,
            frame_count: 1,
            variable_frame_rate: false,
            nominal_frame_rate: 24.,
            color_standard: 1,
            color_range: 2,
            audio_asset: None,
        });
        let data = SharedData::new(Asset {
            version: VERSION,
            source_id: "authored stream-identity fixture".into(),
            source_sha256: [7; 32],
            source_extent: Extent::new(8, 6).unwrap(),
            coordinates: Coordinates::RightDownForward,
            ticks_per_second: 24,
            nominal_rate: Rate::new(24, 1).unwrap(),
            source_pts: vec![0],
            meters_per_world_unit: None,
            tracks: vec![Track {
                id: "point".into(),
                kind: Kind::Point,
                reference_index: None,
                samples: vec![Sample {
                    source_index: 0,
                    visibility: Visibility::Visible,
                    confidence: 1.,
                    manual_correction: false,
                    observation: Some(Observation::Point([2., 3.])),
                }],
            }],
        })
        .unwrap();
        p.tracking_assets.push(motion_core::TrackingAsset {
            id: 2,
            path: "assets/tracking/control.mstrack".into(),
            bytes: 100,
            sha256: "0".repeat(64),
            format: motion_core::TrackingFormat::MotionTrackingJsonV1,
            source: motion_core::media_assets::AssetRef {
                kind: motion_core::media_assets::AssetKind::Video,
                id: 1,
            },
            source_sha256: "07".repeat(32),
            data,
        });
        p.validate().unwrap();
        p
    }

    #[test]
    fn identical_container_with_a_different_stream_cannot_keep_the_old_solve() {
        let expected = fixture();
        let mut candidate = expected.clone();
        let assets = [expected.tracking_assets[0].id];
        let original = expected.video_assets[0].track;
        candidate.video_assets[0].track = original + 1;
        let cause = same_streams(&expected, &candidate, &assets).unwrap_err();
        assert!(
            cause.to_string().contains("different video stream"),
            "{cause}"
        );
        assert!(
            cause
                .to_string()
                .contains("tracking_asset=2 source=Video:1"),
            "{cause}"
        );
        assert_eq!(candidate.tracking_assets, expected.tracking_assets);
        assert_eq!(
            candidate.video_assets[0].bytes,
            expected.video_assets[0].bytes
        );
        candidate.video_assets[0].track = original;
        same_streams(&expected, &candidate, &assets).unwrap();
    }
}
