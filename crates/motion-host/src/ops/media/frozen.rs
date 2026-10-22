//! Typed frozen-reader operations. Hosts retain the existing wire messages.
use super::frozen_registry::{Context, Registry};
pub use super::frozen_registry::{Kind, ReaderInfo, RegistryInfo};
use motion_media::AudioMixer;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Instant,
};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{0} registry poisoned")]
    RegistryPoisoned(Kind),
    #[error("{kind} reader {handle} poisoned")]
    ReaderPoisoned { kind: Kind, handle: i64 },
    #[error("{0} observation poisoned")]
    ObservationPoisoned(Kind),
    #[error("{}", .kind.closed())]
    Closed { kind: Kind, handle: i64 },
    #[error("{}", .0.full())]
    Limit(Kind),
    #[error("{0} handles exhausted")]
    HandleExhausted(Kind),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("{0}")]
    Superseded(&'static str),
    #[error("{0}")]
    Session(#[source] Box<crate::Error>),
    #[error(transparent)]
    Media(#[from] motion_media::Error),
    #[error(transparent)]
    Model(#[from] motion_core::Error),
    /// Retain the conversion cause alongside the frozen-reader request identity.
    #[error("frozen composition video handle={handle} object={object} sequence={sequence} phase=blend_read: {cause}")]
    BlendRead {
        handle: i64,
        object: u64,
        sequence: u64,
        #[source]
        cause: Box<crate::Error>,
    },
}

struct FrozenAudio {
    mixer: AudioMixer,
    pcm: Vec<f32>,
}
struct FrozenVideo {
    project: motion_core::Project,
    root: PathBuf,
    frames: crate::video_frames::VideoFrames,
    prepared: HashMap<u64, crate::video_frame::PreparedVideoFrame>,
    sequence: u64,
    time: f64,
}
pub const MAX_FROZEN_AUDIO: usize = 8;
pub const MAX_FROZEN_VIDEO: usize = 4;
fn audio() -> &'static Registry<FrozenAudio> {
    static AUDIO: OnceLock<Registry<FrozenAudio>> = OnceLock::new();
    AUDIO.get_or_init(|| Registry::new(Kind::Audio, MAX_FROZEN_AUDIO))
}
fn video() -> &'static Registry<FrozenVideo> {
    static VIDEO: OnceLock<Registry<FrozenVideo>> = OnceLock::new();
    VIDEO.get_or_init(|| Registry::new(Kind::Video, MAX_FROZEN_VIDEO))
}
fn context(operation: &'static str, object: u64, sequence: u64) -> Context {
    Context {
        operation,
        object,
        sequence,
    }
}

pub fn audio_info() -> Result<RegistryInfo> {
    audio().info()
}
pub fn video_info() -> Result<RegistryInfo> {
    video().info()
}

/// Run constructors outside both the session and frozen registry locks. Their
/// start is visible before filesystem/mixer work; completion never replaces it.
pub fn freeze_audio(id: i64) -> Result<Value> {
    let began = Instant::now();
    log::info!("frozen reader start site=host.frozen.create operation=freeze_audio session={id}");
    let result = (|| {
        let (project, root, revision) = crate::session::with_session(id, |s| {
            Ok((s.engine.snapshot(), s.root.clone(), s.engine.revision()))
        })
        .map_err(|error| Error::Session(Box::new(error)))?;
        let mut total = 0;
        let handle = audio().create(|| {
            let mixer = AudioMixer::new(project, &root)?;
            total = mixer.total_frames();
            Ok(FrozenAudio {
                mixer,
                pcm: Vec::new(),
            })
        })?;
        Ok(
            json!({"handle":handle,"total_frames":total,"sample_rate":48000,"channels":2,"revision":revision}),
        )
    })();
    finish_create(id, "freeze_audio", began, result)
}
pub fn freeze_video(id: i64) -> Result<Value> {
    let began = Instant::now();
    log::info!("frozen reader start site=host.frozen.create operation=freeze_video session={id}");
    let result = (|| {
        let (project, root, revision, platform) = crate::session::with_session(id, |s| {
            Ok((
                s.engine.snapshot(),
                s.root.clone(),
                s.engine.revision(),
                s.platform().clone(),
            ))
        })
        .map_err(|error| Error::Session(Box::new(error)))?;
        let report = project.clone();
        let handle = video().create(|| {
            Ok(FrozenVideo {
                project,
                root,
                frames: crate::video_frames::VideoFrames::new(platform),
                prepared: HashMap::new(),
                sequence: 0,
                time: -1.,
            })
        })?;
        Ok(json!({"handle":handle,"revision":revision,"project":report}))
    })();
    finish_create(id, "freeze_video", began, result)
}
fn finish_create(
    id: i64,
    operation: &'static str,
    began: Instant,
    result: Result<Value>,
) -> Result<Value> {
    // All session, registry and reader guards have left scope before logging.
    match &result {
        Ok(_) => log::debug!("frozen reader done site=host.frozen.create operation={operation} session={id} elapsed_us={}", began.elapsed().as_micros()),
        Err(Error::Limit(_)) => log::debug!("frozen reader capacity refused operation={operation} session={id}"),
        Err(error) => {
            let cause: String = error.to_string().chars().take(2048).collect();
            log::error!("frozen reader failed site=host.frozen.create operation={operation} session={id} elapsed_us={} cause={cause}", began.elapsed().as_micros());
        }
    }
    result
}
pub fn release_frozen_audio(handle: i64) -> Result<Value> {
    Ok(json!({"released":audio().release(handle)?}))
}
pub fn release_frozen_video(handle: i64) -> Result<Value> {
    Ok(json!({"released":video().release(handle)?}))
}

pub fn read_frozen_pcm_into(
    handle: i64,
    start: u64,
    frames: usize,
    out: &mut [u8],
) -> Result<Value> {
    if frames == 0 || frames > motion_media::MAX_BLOCK_FRAMES {
        return Err(Error::Invalid("invalid PCM block range"));
    }
    if out.len() < frames * 8 {
        return Err(Error::Invalid(
            "PCM output requires a sufficient direct buffer",
        ));
    }
    audio().with(handle, context("mix_pcm", 0, start), |stream| {
        stream.pcm.resize(frames * 2, 0.);
        let count = stream.mixer.mix(start, &mut stream.pcm)?;
        Ok(super::pcm_envelope(
            &stream.pcm,
            count,
            start,
            stream.mixer.total_frames(),
            out,
        ))
    })
}
pub fn request_frozen_composition_frame(handle: i64, frame: f64, sequence: u64) -> Result<Value> {
    if sequence == 0 {
        return Err(Error::Invalid("invalid frozen video sequence"));
    }
    video().with(handle, context("prepare_composition", 0, sequence), |reader| {
        if sequence < reader.sequence || (sequence == reader.sequence && frame != reader.time) {
            return Err(Error::Superseded("frozen composition request superseded"));
        }
        let mut scene = motion_render::Scene::new(&reader.project);
        scene.sample(&reader.project, frame, None, &motion_core::ExpressionEvaluator)?;
        if sequence != reader.sequence { reader.prepared.clear(); reader.sequence = sequence; reader.time = frame; }
        match reader.frames.prepare_scene_exact(&reader.project, &reader.root, &scene)? {
            None => Ok(json!({"state":"pending","sequence":sequence})),
            Some(frames) => {
                reader.prepared = frames.into_iter().collect();
                Ok(json!({"state":"ready","sequence":sequence,"instances":reader.prepared.keys().collect::<Vec<_>>()}))
            }
        }
    })
}
pub fn read_frozen_composition_video_into(
    handle: i64,
    object: u64,
    sequence: u64,
) -> Result<Arc<crate::video_frame::DecodedFrame>> {
    let prepared = video().with(
        handle,
        context("read_composition", object, sequence),
        |reader| {
            if sequence == 0 || reader.sequence != sequence {
                return Err(Error::Superseded("frozen composition video superseded"));
            }
            reader.prepared.get(&object).cloned().ok_or(Error::Invalid(
                "frozen composition video is pending or absent",
            ))
        },
    )?;
    prepared.decoded().map_err(|cause| Error::BlendRead {
        handle,
        object,
        sequence,
        cause: Box::new(cause),
    })
}
pub fn request_frozen_video_frame(
    handle: i64,
    object: u64,
    frame: f64,
    sequence: u64,
) -> Result<Value> {
    if object == 0 || sequence == 0 {
        return Err(Error::Invalid("invalid video object/sequence"));
    }
    video().with(
        handle,
        context("request_video", object, sequence),
        |reader| {
            reader
                .frames
                .request(&reader.project, &reader.root, object, frame, sequence)
                .map_err(Error::Media)
        },
    )
}
pub fn read_frozen_video_frame_into(
    handle: i64,
    object: u64,
    sequence: u64,
) -> Result<Arc<crate::video_frame::DecodedFrame>> {
    if object == 0 || sequence == 0 {
        return Err(Error::Invalid("invalid video object/sequence"));
    }
    video().with(handle, context("read_video", object, sequence), |reader| {
        reader
            .frames
            .frame(&reader.project, object, sequence)
            .map_err(Error::Media)
    })
}

#[cfg(test)]
mod tests;
