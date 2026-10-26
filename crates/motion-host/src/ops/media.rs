//! Media import, export, waveform and frozen-stream operations.
//!
//! Android resolves `content://` URIs through the platform ContentResolver and
//! encodes video with MediaCodec. The desktop host resolves real paths and
//! encodes with libav. Both reach this module through the same request enum, so
//! the import pipeline, task state machine and cache layout are shared.
use crate::session::Result;
use motion_media::{AudioMixer, ImportOptions, MAX_BLOCK_FRAMES};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    sync::Arc,
};

/// Opens a user-selected media file.
///
/// Android supplies `content://` URIs from the system picker; desktop supplies
/// absolute paths. Both hand back a plain reader so the shared import pipeline
/// never learns which platform it is running on.
pub type Opener =
    Box<dyn FnOnce() -> motion_media::Result<(Box<dyn Read + Send>, Option<u64>)> + Send + 'static>;

pub fn file_opener(path: PathBuf) -> Opener {
    Box::new(move || {
        let file = File::open(&path)
            .map_err(|e| motion_media::Error::from(e).context(format!("open media {}", path.display())))?;
        let metadata = file.metadata()
            .map_err(|e| motion_media::Error::from(e).context(format!("inspect media {}", path.display())))?;
        if !metadata.is_file() {
            return Err(format!("media source is not a regular file: {}", path.display()).into());
        }
        Ok((Box::new(file), Some(metadata.len())))
    })
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    MediaCapabilities {
        #[serde(default)]
        video_query: Option<crate::platform::VideoQuery>,
    },
    ImportMedia {
        request_id: String,
        /// Host-resolved location. `content://` URIs are normalised by the
        /// Android adapter before they reach this enum.
        #[serde(alias = "uri")]
        path: String,
        kind: String,
        #[serde(default)]
        at_frame: u32,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        track: Option<u32>,
        #[serde(default = "with_audio")]
        with_audio: bool,
        #[serde(default)]
        audio_track: Option<u32>,
    },
    ProbeMedia {
        request_id: String,
        #[serde(alias = "uri")]
        path: String,
        kind: String,
        #[serde(default)]
        track: Option<u32>,
        #[serde(default = "with_audio")]
        with_audio: bool,
        #[serde(default)]
        audio_track: Option<u32>,
    },
    MediaStatus {
        request_id: String,
    },
    ExportProject {
        request_id: String,
    },
    FinishMediaImport {
        request_id: String,
    },
    FinishMediaReplacement {
        request_id: String,
        target: motion_core::media_assets::AssetRef,
        revision: u64,
    },
    MediaAssetUsages {
        target: motion_core::media_assets::AssetRef,
    },
    #[serde(alias = "cancel_media_task")]
    CancelMediaImport {
        request_id: String,
    },
    ReleaseMediaTask {
        request_id: String,
    },
    PrepareAudio {
        request_id: String,
        asset: u64,
    },
    AudioWaveform {
        asset: u64,
        #[serde(default)]
        first_bucket: u64,
        count: usize,
    },
    PrepareVideo {
        request_id: String,
        asset: u64,
    },
    VideoThumbnail {
        asset: u64,
    },
    RequestVideoFrame {
        object: u64,
        frame: f64,
        sequence: u64,
    },
    ReleaseVideoFrames,
}

fn with_audio() -> bool {
    true
}

pub fn package_limits() -> Value {
    json!({"schema_version":1,
        "max_source_bytes":motion_core::storage::MAX_MEDIA_ASSET,
        "max_payload_bytes":motion_core::storage::MAX_PACKAGE,
        "max_archive_bytes":motion_core::storage::MAX_PACKAGE_ARCHIVE,
        "transfer_buffer_bytes":motion_core::storage::TRANSFER_BUFFER_BYTES,
        "async_export":true,"frozen_export":true,"async_import":false,
        "max_export_tasks_per_session":1,"max_export_workers":2})
}

/// Execute one media request.
///
/// `open` is only invoked for import and probe operations, so status polling
/// and export completion never touch the file system.
pub fn request(id: i64, mut value: Value, open: Option<Opener>) -> Result<Value> {
    let requested = value
        .as_object_mut()
        .ok_or("media request must be an object")?
        .remove("composition")
        .map(|v| v.as_str().map(str::to_owned).ok_or("invalid media composition"))
        .transpose()?;
    let op = value["op"].as_str().unwrap_or("").to_owned();
    let request_id = value["request_id"].as_str().map(str::to_owned);
    crate::session::with_session(id, |s| {
        let active = s.engine.project().composition_id.clone();
        if requested
            .as_ref()
            .is_some_and(|r| r != &active)
        {
            return Err(s.composition_error(
                "context_mismatch",
                "Open the requested composition before media access",
                json!({"requested":requested}),
            ));
        }
        if matches!(op.as_str(), "import_media" | "probe_media") {
            if let Some(id) = &request_id {
                s.media_compositions
                    .entry(id.clone())
                    .or_insert_with(|| active.clone());
            }
        }
        if matches!(op.as_str(), "finish_media_import" | "finish_media_replacement")
            && request_id
                .as_ref()
                .and_then(|id| s.media_compositions.get(id))
                .is_some_and(|c| c != &active)
        {
            return Err(s.composition_error(
                "context_mismatch",
                "Return to the media task's composition before finishing import",
                json!({"composition":request_id
                    .as_ref()
                    .and_then(|id|s.media_compositions.get(id))}),
            ));
        }
        Ok(())
    })?;
    let request: Request = serde_json::from_value(value).map_err(crate::Error::from)?;
    match request {
        Request::MediaCapabilities { video_query } => crate::session::with_session(id, |s| {
            s.platform().media_capabilities(video_query.as_ref()).map_err(Into::into)
        }),
        Request::ExportProject { request_id } => crate::session::with_session(id, |s| {
            if s.audio_jobs.contains(&request_id)
                || s.video_jobs.contains(&request_id)
                || s.package_jobs.contains(&request_id)
            {
                return Err("media request id already exists".into());
            }
            let snapshot = motion_core::storage::PackageSnapshot::new(&s.root, s.engine.project())
                .map_err(crate::Error::from)?;
            let stamp = super::stamp()?;
            let output = s
                .root
                .join("exports")
                .join(format!("project-{stamp}.msproj"));
            Ok(json!(s.package_jobs
                .start_export(&request_id, snapshot, output, s.engine.revision())?))
        }),
        Request::ImportMedia {
            request_id,
            // The location itself is reached through `open`; the field stays in
            // the request so hosts can key task status and error messages on it.
            path: _,
            kind,
            at_frame,
            name,
            track,
            with_audio,
            audio_track,
        } => {
            if kind != "audio" && kind != "video" {
                return Err("media kind must be audio or video".into());
            }
            let name = name.unwrap_or_else(|| default_name(&kind));
            let open = open.ok_or("media import requires a file opener")?;
            crate::session::with_session(id, |s| {
                if s.audio_jobs.contains(&request_id)
                    || s.video_jobs.contains(&request_id)
                    || s.package_jobs.contains(&request_id)
                {
                    return Err("media request id already exists".into());
                }
                Ok(())
            })?;
            crate::session::with_session(id, |s| {
                if at_frame >= s.engine.project().frames {
                    return Err("audio insertion outside composition".into());
                }
                Ok(())
            })?;
            let platform = crate::session::with_session(id, |s| Ok(s.platform().clone()))?;
            let probe = probe_fn(&platform);
            if kind == "video" {
                return crate::session::with_session(id, |s| {
                    Ok(json!(s.video_jobs.start(
                        open,
                        motion_media::VideoImportOptions {
                            request_id,
                            at_frame,
                            name,
                            track,
                            audio_track,
                            with_audio,
                        },
                        false,
                        probe,
                    )?))
                });
            }
            crate::session::with_session(id, |s| {
                Ok(json!(s.audio_jobs.start_with_source(
                    open,
                    ImportOptions {
                        request_id,
                        at_frame,
                        name,
                        track,
                    },
                    false,
                )?))
            })
        }
        Request::ProbeMedia {
            request_id,
            path: _,
            kind,
            track,
            with_audio,
            audio_track,
        } => {
            if kind != "audio" && kind != "video" {
                return Err("media kind must be audio or video".into());
            }
            let open = open.ok_or("media probe requires a file opener")?;
            crate::session::with_session(id, |s| {
                if s.audio_jobs.contains(&request_id)
                    || s.video_jobs.contains(&request_id)
                    || s.package_jobs.contains(&request_id)
                {
                    return Err("media request id already exists".into());
                }
                Ok(())
            })?;
            let platform = crate::session::with_session(id, |s| Ok(s.platform().clone()))?;
            let probe = probe_fn(&platform);
            if kind == "video" {
                return crate::session::with_session(id, |s| {
                    Ok(json!(s.video_jobs.start(
                        open,
                        motion_media::VideoImportOptions {
                            request_id,
                            at_frame: 0,
                            name: default_name("video"),
                            track,
                            audio_track,
                            with_audio,
                        },
                        true,
                        probe,
                    )?))
                });
            }
            crate::session::with_session(id, |s| {
                Ok(json!(s.audio_jobs.start_with_source(
                    open,
                    ImportOptions {
                        request_id,
                        at_frame: 0,
                        name: default_name("audio"),
                        track,
                    },
                    true,
                )?))
            })
        }
        Request::MediaStatus { request_id } => crate::session::with_session(id, |s| {
            if s.package_jobs.contains(&request_id) {
                Ok(json!(s.package_jobs.status(&request_id)?))
            } else if s.video_jobs.contains(&request_id) {
                Ok(json!(s.video_jobs.status(&request_id)?))
            } else {
                Ok(json!(s.audio_jobs.status(&request_id)?))
            }
        }),
        Request::FinishMediaImport { request_id } => crate::session::with_session(id, |s| {
            if s.package_jobs.contains(&request_id) {
                return Err("export_project completes automatically; poll media_status".into());
            }
            let task = if s.video_jobs.contains(&request_id) {
                json!(s.video_jobs.commit(&request_id, &mut s.engine)?)
            } else {
                json!(s.audio_jobs.commit(&request_id, &mut s.engine)?)
            };
            s.audio_mixer = None;
            s.sample()?;
            Ok(json!({"task":task,"state":s.snapshot()?}))
        }),
        Request::MediaAssetUsages { target } => crate::session::with_session(id, |s| {
            use motion_core::media_assets::{lookup, usages};
            let project = s.engine.project();
            let source = lookup(project, target).ok_or("asset no longer exists")?;
            Ok(json!({"target":target,"source":source,"usages":usages(project,target),"revision":s.engine.revision()}))
        }),
        Request::FinishMediaReplacement { request_id, target, revision } => crate::session::with_session(id, |s| {
            use motion_core::media_assets::{lookup, AssetKind};
            log::info!("media replacement commit request={} asset={} kind={:?} revision={} expected_revision={revision}",
                request_id.chars().take(128).collect::<String>(), target.id, target.kind, s.engine.revision());
            if revision < s.engine.revision() {
                let task = match target.kind {
                    AssetKind::Video if s.video_jobs.contains(&request_id) => json!(s.video_jobs.cancel(&request_id)?),
                    AssetKind::Audio if s.audio_jobs.contains(&request_id) => json!(s.audio_jobs.cancel(&request_id)?),
                    _ => return Err("replacement task and asset kinds do not match".into()),
                };
                return Ok(json!({"asset_operation_cancelled":true,"asset_operation_reason":"superseded","task":task,"state":s.snapshot()?}));
            }
            if s.engine.revision() != revision {
                return Err("project changed during media replacement; original asset retained".into());
            }
            lookup(s.engine.project(),target).ok_or("replacement asset no longer exists")?;
            let prepared = match target.kind {
                AssetKind::Video if s.video_jobs.contains(&request_id) => json!(s.video_jobs.status(&request_id)?),
                AssetKind::Audio if s.audio_jobs.contains(&request_id) => json!(s.audio_jobs.status(&request_id)?),
                _ => return Err("replacement task and asset kinds do not match".into()),
            };
            if prepared["state"] != "ready" {
                return Err(prepared["error"].as_str().unwrap_or("replacement task is not ready; original asset retained").into());
            }
            let task = match target.kind {
                AssetKind::Video if s.video_jobs.contains(&request_id) => json!(s.video_jobs.replace(&request_id, &mut s.engine, target.id)?),
                AssetKind::Audio if s.audio_jobs.contains(&request_id) => json!(s.audio_jobs.replace(&request_id, &mut s.engine, target.id)?),
                _ => return Err("replacement task and asset kinds do not match".into()),
            };
            if task["state"] != "succeeded" {
                return Err(task["error"].as_str().unwrap_or("replacement did not publish; original asset retained").into());
            }
            s.audio_mixer = None;
            s.video_frames.clear();
            if let Err(error) = s.sample() {
                log::error!("media replacement committed but sampling failed asset={} kind={:?} revision={} phase=sample reason={error}",target.id,target.kind,s.engine.revision());
            }
            Ok(json!({"task":task,"target":target,"state":s.snapshot()?}))
        }).map_err(|cause| crate::Error::with_context(
            format!("media replacement request={} asset={} kind={:?} phase=commit expected_revision={revision}",
                request_id.chars().take(128).collect::<String>(),target.id,target.kind),cause)),
        Request::CancelMediaImport { request_id } => crate::session::with_session(id, |s| {
            if s.package_jobs.contains(&request_id) {
                Ok(json!(s.package_jobs.cancel(&request_id)?))
            } else if s.video_jobs.contains(&request_id) {
                Ok(json!(s.video_jobs.cancel(&request_id)?))
            } else {
                Ok(json!(s.audio_jobs.cancel(&request_id)?))
            }
        }),
        Request::ReleaseMediaTask { request_id } => crate::session::with_session(id, |s| {
            if s.package_jobs.contains(&request_id) {
                s.package_jobs.release(&request_id)?;
            } else if s.video_jobs.contains(&request_id) {
                s.video_jobs.release(&request_id)?;
            } else {
                s.audio_jobs.release(&request_id)?;
            }
            s.media_compositions.remove(&request_id);
            Ok(json!({"released":request_id}))
        }),
        Request::PrepareAudio { request_id, asset } => {
            crate::session::with_session(id, |s| {
                if s.video_jobs.contains(&request_id) || s.package_jobs.contains(&request_id) {
                    return Err("media request id already exists".into());
                }
                let asset = s
                    .engine
                    .project()
                    .audio_assets
                    .iter()
                    .find(|a| a.id == asset)
                    .cloned()
                    .ok_or("audio asset missing")?;
                Ok(json!(s.audio_jobs.prepare_cache(&request_id, asset)?))
            })
        }
        Request::AudioWaveform {
            asset,
            first_bucket,
            count,
        } => crate::session::with_session(id, |s| {
            let a = s
                .engine
                .project()
                .audio_assets
                .iter()
                .find(|a| a.id == asset)
                .ok_or("audio asset missing")?;
            Ok(json!({"asset":asset,"first_bucket":first_bucket,"bucket_duration_us":10000,
                "source_duration_us":a.duration_us,
                "buckets":motion_media::read_waveform(&s.root, a, first_bucket, count)?}))
        }),
        Request::PrepareVideo { request_id, asset } => {
            let platform = crate::session::with_session(id, |s| Ok(s.platform().clone()))?;
            let probe = probe_fn(&platform);
            crate::session::with_session(id, |s| {
                if s.audio_jobs.contains(&request_id) || s.package_jobs.contains(&request_id) {
                    return Err("media request id already exists".into());
                }
                let asset = s
                    .engine
                    .project()
                    .video_assets
                    .iter()
                    .find(|a| a.id == asset)
                    .cloned()
                    .ok_or("video asset missing")?;
                s.video_frames.clear();
                Ok(json!(s.video_jobs
                    .prepare_cache(&request_id, asset, probe)?))
            })
        }
        Request::VideoThumbnail { asset } => crate::session::with_session(id, |s| {
            let a = s
                .engine
                .project()
                .video_assets
                .iter()
                .find(|a| a.id == asset)
                .ok_or("video asset missing")?;
            let path = motion_media::video_cache_path(&s.root, a)?.with_extension("png");
            let path = path
                .canonicalize()
                .map_err(|_| "thumbnail missing; call prepare_video")?;
            if !path.starts_with(s.root.canonicalize().map_err(crate::Error::from)?) {
                return Err("thumbnail outside project".into());
            }
            Ok(json!({"asset":asset,"path":path,"source_time_us":a.video_start_us,
                "width":a.display_width,"height":a.display_height}))
        }),
        Request::RequestVideoFrame {
            object,
            frame,
            sequence,
        } => crate::session::with_session(id, |s| {
            s.video_frames
                .request(s.engine.project(), &s.root, object, frame, sequence)
                .map_err(Into::into)
        }),
        Request::ReleaseVideoFrames => crate::session::with_session(id, |s| {
            s.video_frames.clear();
            Ok(json!({"released":true}))
        }),
    }
}

fn default_name(kind: &str) -> String {
    if kind == "video" {
        "视频".into()
    } else {
        "音频".into()
    }
}

fn probe_fn(platform: &Arc<dyn crate::platform::Platform>) -> motion_media::ProbeVideo {
    let platform = platform.clone();
    Arc::new(move |path, selected, audio, check| platform.probe_video(path, selected, audio, check))
}

// ---------------------------------------------------------------------------
// Frozen streams used by video export
// ---------------------------------------------------------------------------

pub mod frozen;
mod frozen_registry;
pub use frozen::{MAX_FROZEN_AUDIO, MAX_FROZEN_VIDEO};

pub fn freeze_audio(id: i64) -> Result<Value> {
    frozen::freeze_audio(id).map_err(crate::Error::from)
}

pub fn release_frozen_audio(handle: i64) -> Result<Value> {
    frozen::release_frozen_audio(handle).map_err(crate::Error::from)
}

/// Mix a block of 48 kHz stereo f32 PCM into `out` as little-endian floats.
///
/// The byte layout is the host-independent `f32le_interleaved` format that the
/// video exporter muxes, so every host writes the same bytes for the same block.
pub fn read_pcm_into(id: i64, start: u64, frames: usize, out: &mut [u8]) -> Result<Value> {
    check_pcm_range(start, frames, out)?;
    crate::session::with_session(id, |s| {
        if s.audio_mixer
            .as_ref()
            .is_none_or(|(rev, _)| *rev != s.engine.revision())
        {
            s.audio_mixer =
                Some((s.engine.revision(), AudioMixer::new(s.engine.snapshot(), &s.root)?));
        }
        s.audio_pcm.resize(frames * 2, 0.0);
        let count = s
            .audio_mixer
            .as_mut()
            .unwrap()
            .1
            .mix(start, &mut s.audio_pcm)?;
        let total = s.audio_mixer.as_ref().unwrap().1.total_frames();
        Ok(pcm_envelope(&s.audio_pcm, count, start, total, out))
    })
}

pub fn read_frozen_pcm_into(
    handle: i64,
    start: u64,
    frames: usize,
    out: &mut [u8],
) -> Result<Value> {
    frozen::read_frozen_pcm_into(handle, start, frames, out).map_err(crate::Error::from)
}

fn check_pcm_range(start: u64, frames: usize, out: &[u8]) -> Result<()> {
    if frames == 0 || frames > MAX_BLOCK_FRAMES {
        return Err("invalid PCM block range".into());
    }
    if out.len() < frames * 8 {
        return Err("PCM output requires a sufficient direct buffer".into());
    }
    let _ = start;
    Ok(())
}

fn pcm_envelope(pcm: &[f32], count: usize, start: u64, total: u64, out: &mut [u8]) -> Value {
    for (i, value) in pcm[..count * 2].iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    json!({"frames":count,"bytes":count*8,"sample_rate":48000,"channels":2,"format":"f32le_interleaved",
        "start_sample":start,"pts_us":start*1_000_000/48000,
        "end_of_stream":start + count as u64 == total})
}

pub fn freeze_video(id: i64) -> Result<Value> {
    frozen::freeze_video(id).map_err(crate::Error::from)
}

pub fn release_frozen_video(handle: i64) -> Result<Value> {
    frozen::release_frozen_video(handle).map_err(crate::Error::from)
}

/// Prepare every video layer of one composition frame for a frozen export.
pub fn request_frozen_composition_frame(
    handle: i64,
    frame: f64,
    sequence: u64,
) -> Result<Value> {
    frozen::request_frozen_composition_frame(handle, frame, sequence).map_err(crate::Error::from)
}

pub fn read_frozen_composition_video_into(
    handle: i64,
    object: u64,
    sequence: u64,
) -> Result<Arc<crate::video_frame::DecodedFrame>> {
    frozen::read_frozen_composition_video_into(handle, object, sequence).map_err(crate::Error::from)
}

pub fn request_frozen_video_frame(
    handle: i64,
    object: u64,
    frame: f64,
    sequence: u64,
) -> Result<Value> {
    frozen::request_frozen_video_frame(handle, object, frame, sequence).map_err(crate::Error::from)
}

pub fn read_frozen_video_frame_into(
    handle: i64,
    object: u64,
    sequence: u64,
) -> Result<Arc<crate::video_frame::DecodedFrame>> {
    frozen::read_frozen_video_frame_into(handle, object, sequence).map_err(crate::Error::from)
}

/// Decode metadata for a single video frame, used by pixel readback hosts.
pub fn read_video_frame_into(
    id: i64,
    object: u64,
    sequence: u64,
) -> Result<Arc<crate::video_frame::DecodedFrame>> {
    if object == 0 || sequence == 0 {
        return Err("invalid video object/sequence".into());
    }
    crate::session::with_session(id, |s| {
        s.video_frames
            .frame(s.engine.project(), object, sequence)
            .map_err(Into::into)
    })
}

/// Describe a decoded frame in the shared JSON shape.
pub fn frame_report(
    frame: &crate::video_frame::DecodedFrame,
    object: u64,
    sequence: u64,
) -> Value {
    json!({"object":object,"sequence":sequence,"width":frame.width,"height":frame.height,
        "bytes":u64::from(frame.width)*u64::from(frame.height)*4,"format":"rgba8","pts_us":frame.pts,"end_us":frame.end,
        "decode_us":frame.decode_us,"codec_us":frame.codec_us,"transfer_us":frame.transfer_us,
        "pack_us":frame.pack_us,"source_transfer":frame.source_transfer,
        "decoder":frame.decoder_name})
}

#[cfg(test)]
mod request_tests {
    use super::Request;
    #[test]
    fn mobile_uri_and_desktop_path_preserve_the_same_import_and_probe_contract() {
        for op in ["import_media", "probe_media"] {
            for (field, location) in [("uri", "content://media/selected"), ("path", "/selected/movie.webm")] {
                let mut value = serde_json::json!({"op":op,"request_id":"selected","kind":"video","audio_track":2});
                value[field] = location.into();
                let request: Request = serde_json::from_value(value).unwrap();
                let (path, audio) = match request {
                    Request::ImportMedia {path, audio_track, ..} | Request::ProbeMedia {path, audio_track, ..} => (path, audio_track),
                    _ => panic!("wrong media operation"),
                };
                assert_eq!(path,location); assert_eq!(audio,Some(2));
            }
        }
        let duplicate=serde_json::json!({"op":"import_media","request_id":"selected","kind":"audio","uri":"content://one","path":"/two"});
        assert!(serde_json::from_value::<Request>(duplicate).is_err());
    }
    #[test]
    fn file_opener_requires_a_regular_file_and_preserves_length_and_path_errors() {
        use std::io::Read;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("selected.wav");
        std::fs::write(&path, b"media").unwrap();
        let (mut reader, bytes) = super::file_opener(path.clone())().unwrap();
        assert_eq!(bytes, Some(5));
        let mut content = Vec::new();
        reader.read_to_end(&mut content).unwrap();
        assert_eq!(content, b"media");
        assert!(super::file_opener(directory.path().to_path_buf())().is_err());
        let missing = directory.path().join("missing.wav");
        let error = super::file_opener(missing)().err().unwrap();
        assert!(error.to_string().contains("missing.wav"));
    }
    #[test]
    fn pixel_readback_reports_rgba_bytes_instead_of_native_yuv_cache_bytes() {
        use crate::video_frame::{DecodedFrame,VideoPixels};
        use motion_render::{VideoPlane,Yuv420Frame};
        let y=[16,80,192,235];let chroma=[128];
        let pixels=Yuv420Frame::pack(2,2,[0,0],0,4,2,[
            VideoPlane {data:&y,row_stride:2,pixel_stride:1},
            VideoPlane {data:&chroma,row_stride:1,pixel_stride:1},
            VideoPlane {data:&chroma,row_stride:1,pixel_stride:1},
        ]).unwrap();
        let frame=DecodedFrame {pixels:VideoPixels::Yuv(pixels),pts:0,end:33333,width:2,height:2,decode_us:0,codec_us:0,transfer_us:0,pack_us:0,source_transfer:"yuv",decoder_name:"test".into()};
        assert_eq!(frame.bytes(),6);
        let rgba=frame.rgba().unwrap();
        assert_eq!(rgba.len(),16);
        let report=super::frame_report(&frame,1,1);
        assert_eq!(report["format"],"rgba8");
        assert_eq!(report["bytes"].as_u64().unwrap() as usize,rgba.len());
    }
}
