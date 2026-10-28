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
pub const PCM_VERSION: u32 = 3;

pub fn cache_path(
    root: &std::path::Path,
    asset: &motion_model::AudioAsset,
) -> Result<std::path::PathBuf> {
    motion_model::storage::validate_relative_path(&asset.path).map_err(crate::Error::from)?;
    let stem = std::path::Path::new(&asset.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("invalid audio source name")?;
    Ok(root.join(format!("cache/audio-v{PCM_VERSION}")).join(stem).join("decoded.pcm"))
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
