use super::*;

pub(super) struct RetiredStream {
    pub(super) worker: JoinHandle<()>,
    pub(super) object: u64,
    pub(super) source: u64,
    path: PathBuf,
    track: u32,
    began: Instant,
    // One bounded counter for this worker's retirement episode. It is removed
    // when join reaps the worker; a later retirement starts a fresh episode.
    pending: motion_render::observability::FailureCounter,
}

impl RetiredStream {
    pub(super) fn new(worker: JoinHandle<()>, stream: &Stream) -> Self {
        Self {
            worker,
            object: stream.object,
            source: stream.asset.id,
            path: stream.path.clone(),
            track: stream.asset.track,
            began: Instant::now(),
            pending: motion_render::observability::FailureCounter::new(),
        }
    }
    pub(super) fn blocks(&self, path: &Path, asset: &VideoAsset) -> bool {
        self.path == path && self.track == asset.track
    }
    pub(super) fn is_finished(&self) -> bool {
        self.worker.is_finished()
    }
    pub(super) fn join(self) -> std::thread::Result<()> {
        self.worker.join()
    }
    pub(super) fn status(&self) -> Value {
        json!({"object":self.object,"source":self.source,"phase":"retirement",
            "elapsed_ms":self.began.elapsed().as_millis(),"finished":self.is_finished()})
    }
    pub(super) fn pending(&self, asset: &VideoAsset, object: u64, sequence: u64) -> Value {
        // This is scheduling backpressure, not a failed decode. Only sustained
        // polling escalates, with no per-frame file IO on healthy retirements.
        self.pending.failure("video_decoder_retirement", format_args!(
            "object={object} source={} sequence={sequence} blocked_object={} blocked_source={} phase=retirement elapsed_ms={}",
            asset.id, self.object, self.source, self.began.elapsed().as_millis()));
        json!({"state":"pending","reason":"decoder_retirement","sequence":sequence,
            "object":object,"width":asset.display_width,"height":asset.display_height,
            "blocked_object":self.object,"blocked_source":self.source,"retiring_ms":self.began.elapsed().as_millis()})
    }
}
