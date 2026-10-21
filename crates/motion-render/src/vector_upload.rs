//! Encoder-ordered uploads. Queue writes would all run before the first draw.
#[cfg(test)]
mod tests;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

const IDLE_LIMIT: u64 = 16 * 1024 * 1024;
const IDLE_BUFFERS: usize = 128;

#[derive(Clone, Debug, thiserror::Error)]
pub enum VectorUploadError {
    #[error("vector upload size {bytes} must be nonzero and four-byte aligned")]
    Alignment { bytes: u64 },
    #[error("vector upload requires {required_bytes} bytes; buffer limit is {limit_bytes}")]
    Size {
        required_bytes: u64,
        limit_bytes: u64,
    },
    #[error("vector upload remap failed: {0}")]
    Remap(#[source] wgpu::BufferAsyncError),
    #[error("vector upload mapped range failed: {0}")]
    Range(#[source] Arc<wgpu::MapRangeError>),
    #[error("vector upload pool lock poisoned")]
    Poisoned,
}

/// Logical buffer capacities, separate from textures and external driver memory.
/// Totals saturate over the renderer's lifetime; release retires idle buffers but
/// does not reset totals. Pending includes encoded work not yet submitted.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorUploadInfo {
    pub staging_buffers_created_total: u64,
    pub reused_uploads_total: u64,
    pub encoded_upload_bytes_total: u64,
    pub completed_upload_bytes_total: u64,
    pub abandoned_uploads_total: u64,
    pub remap_failures_total: u64,
    pub pending_buffers: u64,
    pub pending_bytes: u64,
    pub idle_buffers: u64,
    pub idle_bytes: u64,
    pub idle_limit_bytes: u64,
    pub idle_limit_buffers: usize,
    pub vertex_capacity_bytes: u64,
    pub opacity_capacity_bytes: u64,
    pub destination_buffers_created_total: u64,
}

#[derive(Default)]
struct State {
    idle: Vec<wgpu::Buffer>,
    generation: u64,
    failure: Option<VectorUploadError>,
    info: VectorUploadInfo,
}
#[derive(Default)]
pub(crate) struct UploadPool(Arc<Mutex<State>>);

pub(crate) fn capacity(required: u64, limit: u64) -> Result<u64, VectorUploadError> {
    if required == 0 || required % wgpu::COPY_BUFFER_ALIGNMENT != 0 {
        return Err(VectorUploadError::Alignment { bytes: required });
    }
    if required > limit {
        return Err(VectorUploadError::Size {
            required_bytes: required,
            limit_bytes: limit,
        });
    }
    Ok(required
        .checked_next_power_of_two()
        .unwrap_or(limit)
        .min(limit))
}

#[derive(Default)]
pub(crate) struct ReusableBuffer {
    buffer: Option<wgpu::Buffer>,
    pub(crate) created: u64,
}
impl ReusableBuffer {
    pub(crate) fn get(
        &mut self,
        device: &wgpu::Device,
        bytes: u64,
        usage: wgpu::BufferUsages,
        label: &'static str,
    ) -> Result<wgpu::Buffer, VectorUploadError> {
        let size = capacity(bytes, device.limits().max_buffer_size)?;
        if self
            .buffer
            .as_ref()
            .is_none_or(|buffer| buffer.size() < bytes)
        {
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.created = self.created.saturating_add(1);
        }
        self.buffer.clone().ok_or(VectorUploadError::Size {
            required_bytes: bytes,
            limit_bytes: size,
        })
    }
    pub(crate) fn bytes(&self) -> u64 {
        self.buffer.as_ref().map_or(0, wgpu::Buffer::size)
    }
    pub(crate) fn clear(&mut self) {
        self.buffer = None;
    }
}

impl UploadPool {
    pub(crate) fn info(&self) -> Result<VectorUploadInfo, VectorUploadError> {
        let state = self.0.lock().map_err(|_| VectorUploadError::Poisoned)?;
        let mut info = state.info;
        info.idle_buffers = state.idle.len() as u64;
        info.idle_limit_bytes = IDLE_LIMIT;
        info.idle_limit_buffers = IDLE_BUFFERS;
        Ok(info)
    }
    pub(crate) fn release(&self) {
        let idle = {
            // Cleanup may retire resources from a poisoned pool, but never
            // clears poison or resumes uploads after a previous failure.
            let mut state = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.generation = state.generation.wrapping_add(1);
            state.info.idle_bytes = 0;
            std::mem::take(&mut state.idle)
        };
        drop(idle);
    }
    pub(crate) fn write(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Buffer,
        data: &[u8],
        layer: u64,
        frame: f64,
    ) -> Result<(), VectorUploadError> {
        let size = capacity(data.len() as u64, device.limits().max_buffer_size)?;
        if data.len() as u64 > target.size() {
            return Err(VectorUploadError::Size {
                required_bytes: data.len() as u64,
                limit_bytes: target.size(),
            });
        }
        let (source, generation) = {
            let mut state = self.0.lock().map_err(|_| VectorUploadError::Poisoned)?;
            if let Some(error) = &state.failure {
                return Err(error.clone());
            }
            let fit = state
                .idle
                .iter()
                .enumerate()
                .filter(|(_, b)| b.size() >= data.len() as u64)
                .min_by_key(|(_, b)| b.size())
                .map(|(index, _)| index);
            let source = fit.map(|index| {
                let source = state.idle.swap_remove(index);
                state.info.idle_bytes -= source.size();
                state.info.reused_uploads_total = state.info.reused_uploads_total.saturating_add(1);
                source
            });
            (source, state.generation)
        };
        let created = source.is_none();
        let source = source.unwrap_or_else(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pooled vector upload staging"),
                size,
                usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: true,
            })
        });
        {
            let mut state = self.0.lock().map_err(|_| VectorUploadError::Poisoned)?;
            state.info.staging_buffers_created_total = state
                .info
                .staging_buffers_created_total
                .saturating_add(u64::from(created));
            state.info.pending_buffers = state.info.pending_buffers.saturating_add(1);
            state.info.pending_bytes = state.info.pending_bytes.saturating_add(source.size());
        }
        let pending = Pending {
            buffer: Some(source),
            pool: Arc::downgrade(&self.0),
            generation,
            layer,
            frame,
            began: Instant::now(),
            encoded_bytes: data.len() as u64,
        };
        if let Some(source) = &pending.buffer {
            source
                .slice(..data.len() as u64)
                .get_mapped_range_mut()
                .map_err(|error| VectorUploadError::Range(Arc::new(error)))?
                .copy_from_slice(data);
            source.unmap();
            encoder.copy_buffer_to_buffer(source, 0, target, 0, data.len() as u64);
            {
                let mut state = self.0.lock().map_err(|_| VectorUploadError::Poisoned)?;
                state.info.encoded_upload_bytes_total = state
                    .info
                    .encoded_upload_bytes_total
                    .saturating_add(data.len() as u64);
            }
            // The closure owns cancellation cleanup too: dropping an unsubmitted
            // encoder retires this source instead of stranding it in the cache.
            let source = source.clone();
            encoder.map_buffer_on_submit(&source, wgpu::MapMode::Write, .., move |result| {
                pending.complete(result)
            });
        }
        Ok(())
    }
}

struct Pending {
    buffer: Option<wgpu::Buffer>,
    pool: Weak<Mutex<State>>,
    generation: u64,
    layer: u64,
    frame: f64,
    began: Instant,
    encoded_bytes: u64,
}
impl Pending {
    fn complete(mut self, result: Result<(), wgpu::BufferAsyncError>) {
        let Some(buffer) = self.buffer.take() else {
            return;
        };
        let Some(pool) = self.pool.upgrade() else {
            return;
        };
        let bytes = buffer.size();
        let mut buffer = Some(buffer);
        let report = {
            // Recovery here only accounts for retired work; a poisoned pool
            // remains poisoned and subsequent writes return an explicit error.
            let mut state = pool
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.info.pending_buffers = state.info.pending_buffers.saturating_sub(1);
            state.info.pending_bytes = state.info.pending_bytes.saturating_sub(bytes);
            if result.is_ok() {
                state.info.completed_upload_bytes_total = state
                    .info
                    .completed_upload_bytes_total
                    .saturating_add(self.encoded_bytes);
            }
            if let Err(error) = result {
                state.info.remap_failures_total = state.info.remap_failures_total.saturating_add(1);
                let first = state.failure.is_none();
                state
                    .failure
                    .get_or_insert_with(|| VectorUploadError::Remap(error.clone()));
                first.then_some(error)
            } else if self.generation != state.generation {
                None // Retired work completes normally without repopulating the cache.
            } else {
                if state.failure.is_none()
                    && state.info.idle_bytes + bytes <= IDLE_LIMIT
                    && state.idle.len() < IDLE_BUFFERS
                {
                    state.info.idle_bytes += bytes;
                    if let Some(buffer) = buffer.take() {
                        state.idle.push(buffer);
                    }
                }
                None
            }
        };
        drop(buffer);
        if let Some(error) = report {
            log::error!("vector upload failed site=render.vector_upload phase=remap layer={} frame={} bytes={bytes} elapsed_us={} cause={error}",
                self.layer, self.frame, self.began.elapsed().as_micros());
        }
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let (Some(buffer), Some(pool)) = (&self.buffer, self.pool.upgrade()) {
            let mut state = pool
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.info.pending_buffers = state.info.pending_buffers.saturating_sub(1);
            state.info.pending_bytes = state.info.pending_bytes.saturating_sub(buffer.size());
            state.info.abandoned_uploads_total =
                state.info.abandoned_uploads_total.saturating_add(1);
        }
    }
}
