//! Owned asynchronous media import, video caches and bounded PCM mixing. No UI dependencies.
mod avc;
mod error;
pub use error::Error;
pub mod fonts;
pub mod memory;
mod decode;
mod formats;
mod matroska;
pub use matroska::{matroska_metadata, MatroskaMetadata, MatroskaTrack};
mod jobs;
mod mixer;
mod waveform_view;
pub use waveform_view::{waveform_view, MAX_WAVEFORM_COLUMNS};
mod onsets;
pub use onsets::audio_references;
mod packages;
mod poison_observation;
mod publication;
mod progress;
mod registry_observation;
mod task_logging;
mod workers;
pub use packages::{PackageJobs, PackageTaskStatus};
mod mp4;
mod mp4_timing;
pub use mp4_timing::{
    sample_duration as mp4_sample_duration, sample_timing as mp4_sample_timing, Mp4SampleTiming,
    SampleDuration,
};
mod video;
mod video_timing;
pub use video_timing::{source_frame_rate, timestamp_scan_budget, video_end_us, VideoDuration};
mod video_metadata;
pub use avc::{avc_metadata, AvcMetadata};
pub use decode::{decode_audio, DecodeAudio};
pub use formats::{source_extension, NATIVE_AUDIO_MIMES, VIDEO_MIMES};
pub use jobs::{AudioJobs, ImportOptions, Limits, TaskStatus};
pub use mixer::{read_waveform, AudioMixer, WaveBucket};
pub use video::{
    load_video_index, save_index, video_cache_path, ProbeVideo, VideoImportOptions, VideoJobs,
    VideoProbe, VideoTaskStatus,
};
pub use video_metadata::{hevc_metadata, vp9_metadata};
/// Bounded native panic detail, shared by asynchronous media and host adapters.
pub fn panic_reason(payload: Box<dyn std::any::Any + Send>) -> String {
    panic_reason_ref(payload.as_ref())
}
pub(crate) fn panic_reason_ref(payload: &(dyn std::any::Any + Send)) -> String {
    let reason = payload.downcast_ref::<String>().map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("unknown native error");
    reason.chars().take(2048).collect()
}
pub type Result<T> = std::result::Result<T, Error>;
pub const OUTPUT_RATE: u64 = 48_000;
pub const MAX_BLOCK_FRAMES: usize = 48_000;
/// Decoded PCM layout and identity version. Bumping it orphans every older
/// `cache/audio-v*` directory, so a repaired key scheme never reads bytes that
/// another source wrote under the old, colliding name.
pub const PCM_VERSION: u32 = 4;
/// Video timestamp-index layout and identity version, bumped with `PCM_VERSION`.
pub const VIDEO_VERSION: u32 = 3;
/// Top-level sources with this extension keep the historical `<stem>` directory.
/// Every other path shape takes an exclusive namespaced directory instead.
const LEGACY_AUDIO_EXTENSION: &str = "wav";

pub fn cache_path(
    root: &std::path::Path,
    asset: &motion_model::AudioAsset,
) -> Result<std::path::PathBuf> {
    let key = cache_key(&asset.path, Some(LEGACY_AUDIO_EXTENSION))?;
    Ok(root
        .join(format!("cache/audio-v{PCM_VERSION}"))
        .join(key)
        .join("decoded.pcm"))
}

/// Cache directory for one asset, derived from its validated relative path and
/// never from the file stem alone: `assets/a/tone.wav` and `assets/b/tone.wav`
/// are different sources and must not share one decoded entry.
///
/// The key shapes occupy different depths, so two distinct asset paths cannot
/// produce the same key: `<stem>` for the legacy top-level source,
/// `flat/<file name>` for any other top-level source, and
/// `dirs/<component>/.../<file name>` for a source below `assets/`.
/// `validate_relative_path` runs first, so every copied component is non-empty,
/// is not `.` or `..`, and carries no separator, drive prefix or NUL: the key
/// always stays inside the cache root.
pub(crate) fn cache_key(
    relative: &str,
    legacy_extension: Option<&str>,
) -> Result<std::path::PathBuf> {
    motion_model::storage::validate_relative_path(relative).map_err(crate::Error::from)?;
    let mut components = relative.split('/');
    // `validate_relative_path` guarantees the leading `assets` component.
    let _directory = components.next();
    let name = components.next().ok_or("invalid media source path")?;
    let nested = components.collect::<Vec<_>>();
    let file = std::path::Path::new(name);
    if nested.is_empty() {
        let extension = file.extension().and_then(|e| e.to_str());
        if let (Some(extension), Some(legacy)) = (extension, legacy_extension) {
            if extension == legacy {
                let stem = file
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .filter(|s| !s.is_empty())
                    .ok_or("invalid media source name")?;
                return Ok(std::path::PathBuf::from(stem));
            }
        }
        return Ok(std::path::Path::new("flat").join(name));
    }
    let mut key = std::path::PathBuf::from("dirs");
    key.push(name);
    for component in nested {
        key.push(component);
    }
    Ok(key)
}

pub(crate) fn contained_dir(root: &std::path::Path, path: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(crate::Error::from)?;
    if !path
        .canonicalize()
        .map_err(crate::Error::from)?
        .starts_with(root.canonicalize().map_err(crate::Error::from)?)
    {
        return Err("media directory resolves outside project".into());
    }
    Ok(())
}

#[cfg(test)]
mod cache_key_tests {
    use super::*;
    fn asset(path: &str) -> motion_model::AudioAsset {
        motion_model::AudioAsset {
            id: 1,
            path: path.into(),
            mime: "audio/wav".into(),
            bytes: 64,
            track: 0,
            sample_rate: 48_000,
            channels: 2,
            sample_frames: 48_000,
            duration_us: 1_000_000,
        }
    }
    fn key(path: &str) -> std::path::PathBuf {
        cache_path(std::path::Path::new("project"), &asset(path)).unwrap()
    }
    fn expected(components: &[&str]) -> std::path::PathBuf {
        let mut path = std::path::Path::new("project").join(format!("cache/audio-v{PCM_VERSION}"));
        for component in components {
            path.push(component);
        }
        path.join("decoded.pcm")
    }

    #[test]
    fn cache_keys_keep_same_named_sources_apart_and_reject_hostile_paths() {
        // Two sources with one basename in different directories never share a key.
        assert_eq!(key("assets/a/tone.wav"), expected(&["dirs", "a", "tone.wav"]));
        assert_eq!(key("assets/b/tone.wav"), expected(&["dirs", "b", "tone.wav"]));
        assert_ne!(key("assets/a/tone.wav"), key("assets/b/tone.wav"));
        assert_ne!(key("assets/a/tone.wav"), key("assets/a/deeper/tone.wav"));
        // The historical top-level layout is kept, but it is not the stem alone.
        assert_eq!(key("assets/tone.wav"), expected(&["tone"]));
        assert_eq!(key("assets/tone.mp3"), expected(&["flat", "tone.mp3"]));
        assert_ne!(key("assets/tone.wav"), key("assets/tone.mp3"));
        assert_ne!(key("assets/tone.wav"), key("assets/a/tone.wav"));
        // Every key stays under the cache root.
        for path in ["assets/tone.wav", "assets/tone.mp3", "assets/a/tone.wav"] {
            assert!(key(path).starts_with("project/cache"));
        }
        // Absolute, escaping, repeated and separator-carrying paths are rejected.
        for hostile in [
            "assets/../tone.wav",
            "assets/a/../../tone.wav",
            "assets/a/./tone.wav",
            "assets//tone.wav",
            "assets\\tone.wav",
            "/etc/tone.wav",
            "C:/tone.wav",
            "assets",
        ] {
            assert!(
                cache_path(std::path::Path::new("project"), &asset(hostile)).is_err(),
                "{hostile}"
            );
        }
    }
}
