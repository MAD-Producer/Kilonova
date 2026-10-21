use crate::GpuTiming;
use serde_json::{json, Value};
use std::time::Instant;
/// Samples at successful frame boundaries, not transient/driver VRAM peaks.
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameResources {
    pub textures: crate::TextureMemoryBreakdown,
    pub vector_upload: crate::VectorUploadInfo,
    pub preview_target_bytes: u64,
    pub exposure_target_bytes: u64,
    /// Combined media preparation/upload wall time across exposure polls.
    pub exposure_media_us: u64,
    pub cpu_video_cache_bytes: u64,
    pub cpu_video_proxy_bytes: u64,
    pub video_prepare_us: u64,
    pub video_upload_us: u64,
    pub image_upload_bytes_total: u64,
    pub video_upload_bytes_total: u64,
    pub pipeline_builds_total: u64,
    pub memory_quality_level: u8,
    pub resource_refusals_total: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct FrameMeasurement {
    pub sequence: u64,
    pub frame: f64,
    pub elapsed_us: u64,
    pub cpu_prepare_us: u64,
    pub acquire_us: u64,
    pub submit_us: u64,
    pub present_call_us: u64,
    pub render_wall_us: u64,
    pub render_width: u32,
    pub render_height: u32,
    pub preview_fps: u32,
    pub gpu: Option<GpuTiming>,
    pub resources: Option<FrameResources>,
}
pub struct FrameRecorder {
    start: Instant,
    first_sequence: u64,
    limit: usize,
    frames: Vec<FrameMeasurement>,
    pub overflow: u64,
}
impl FrameRecorder {
    pub fn new(first_sequence: u64, limit: usize) -> Self {
        let limit = limit.clamp(1, 65_536);
        Self {
            start: Instant::now(),
            first_sequence,
            limit,
            frames: Vec::with_capacity(limit),
            overflow: 0,
        }
    }
    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
    pub fn record(&mut self, frame: FrameMeasurement) {
        if self.frames.len() < self.limit {
            self.frames.push(frame)
        } else {
            self.overflow += 1;
        }
    }
    pub fn timing(&mut self, t: GpuTiming) {
        if let Some(i) = t.sequence.checked_sub(self.first_sequence) {
            if let Some(frame) = self.frames.get_mut(i as usize) {
                if frame.sequence == t.sequence {
                    frame.gpu = Some(t)
                }
            }
        }
    }
    pub fn report(&self, metadata: Value) -> Value {
        let gpu_count = self.frames.iter().filter(|f| f.gpu.is_some()).count();
        let metric = |field: fn(&FrameMeasurement) -> Option<f64>| {
            distribution(self.frames.iter().filter_map(field))
        };
        let resource_count = self.frames.iter().filter(|f| f.resources.is_some()).count();
        let frames = self.frames.iter().map(|f|json!({"sequence":f.sequence,"frame":f.frame,"elapsedUs":f.elapsed_us,"cpuPrepareUs":f.cpu_prepare_us,
                "acquireUs":f.acquire_us,"submitUs":f.submit_us,"presentCallUs":f.present_call_us,"renderWallUs":f.render_wall_us,
                "renderWidth":f.render_width,"renderHeight":f.render_height,"previewFps":f.preview_fps,
                "resources":f.resources,
                "gpuCompositionUs":f.gpu.map(|g|g.composition_us),"gpuPresentationUs":f.gpu.map(|g|g.presentation_us),"gpuTotalUs":f.gpu.map(|g|g.total_us)})).collect::<Vec<_>>();
        json!({"metadata":metadata,"durationSeconds":self.start.elapsed().as_secs_f64(),"frameCount":self.frames.len(),"overflowFrames":self.overflow,
            "gpuSampleCount":gpu_count,"gpuSampleCoverage":if self.frames.is_empty(){0.0}else{gpu_count as f64/self.frames.len() as f64},
            "cpuPrepareUs":metric(|f|Some(f.cpu_prepare_us as f64)),"acquireUs":metric(|f|Some(f.acquire_us as f64)),
            "submitUs":metric(|f|Some(f.submit_us as f64)),"presentCallUs":metric(|f|Some(f.present_call_us as f64)),"renderWallUs":metric(|f|Some(f.render_wall_us as f64)),
            "gpuTotalUs":metric(|f|f.gpu.map(|g|g.total_us)),"gpuCompositionUs":metric(|f|f.gpu.map(|g|g.composition_us)),"gpuPresentationUs":metric(|f|f.gpu.map(|g|g.presentation_us)),
            "resourceSampleCount":resource_count,
            "resourceSampleScope":"Successful frame boundaries; logical application texture capacity, separate vector upload buffers and CPU video caches. Excludes transient peaks, other buffers, decoder external allocations and driver residency. Upload and pipeline counters are cumulative, not per-frame bytes/counts.",
            "logicalTextureBytes":metric(|f|f.resources.map(|r| (r.textures.total_bytes()+r.preview_target_bytes+r.exposure_target_bytes) as f64)),
            "exposureTargetBytes":metric(|f|f.resources.map(|r|r.exposure_target_bytes as f64)),
            "exposureMediaUs":metric(|f|f.resources.map(|r|r.exposure_media_us as f64)),
            "cpuVideoCacheBytes":metric(|f|f.resources.map(|r|r.cpu_video_cache_bytes as f64)),
            "cpuVideoProxyBytes":metric(|f|f.resources.map(|r|r.cpu_video_proxy_bytes as f64)),
            "vectorBufferCapacityBytes":metric(|f|f.resources.map(|r| {
                let u=r.vector_upload;
                u.vertex_capacity_bytes.saturating_add(u.opacity_capacity_bytes)
                    .saturating_add(u.pending_bytes).saturating_add(u.idle_bytes) as f64
            })),
            "vectorEncodedUploadBytesTotal":metric(|f|f.resources.map(|r|r.vector_upload.encoded_upload_bytes_total as f64)),
            "vectorCompletedUploadBytesTotal":metric(|f|f.resources.map(|r|r.vector_upload.completed_upload_bytes_total as f64)),
            "vectorUploadBufferCreatesTotal":metric(|f|f.resources.map(|r|
                r.vector_upload.staging_buffers_created_total.saturating_add(r.vector_upload.destination_buffers_created_total) as f64)),
            "videoPrepareUs":metric(|f|f.resources.map(|r|r.video_prepare_us as f64)),
            "videoUploadUs":metric(|f|f.resources.map(|r|r.video_upload_us as f64)),
            "presentationScope":"Native present() calls and GPU pass timestamps. Actual display deadlines require platform presentation tracing (for example PresentMon or FrameTimeline).",
            "frames":frames})
    }
}
pub fn distribution(values: impl Iterator<Item = f64>) -> Value {
    let mut values: Vec<_> = values.filter(|v| v.is_finite()).collect();
    if values.is_empty() {
        return Value::Null;
    }
    values.sort_by(f64::total_cmp);
    let percentile = |p: f64| {
        values[((values.len() as f64 * p).ceil() as usize)
            .saturating_sub(1)
            .min(values.len() - 1)]
    };
    json!({"count":values.len(),"p50":percentile(0.5),"p95":percentile(0.95),"p99":percentile(0.99),"max":values.last()})
}
