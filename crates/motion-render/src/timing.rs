use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};
const SLOTS: usize = 4;
const QUERIES: u32 = 4;
const TIMESTAMP_WORD_BYTES: usize = std::mem::size_of::<u64>();
const TIMESTAMP_BUFFER_BYTES: usize = QUERIES as usize * TIMESTAMP_WORD_BYTES;

fn read_timestamp_words(mapped: &[u8]) -> Option<[u64; QUERIES as usize]> {
    let mut words = [0; QUERIES as usize];
    for (index, word) in words.iter_mut().enumerate() {
        let start = index * TIMESTAMP_WORD_BYTES;
        let bytes = mapped.get(start..start + TIMESTAMP_WORD_BYTES)?;
        *word = u64::from_le_bytes(bytes.try_into().ok()?);
    }
    Some(words)
}
struct Slot {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    sequence: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct GpuTiming {
    pub sequence: u64,
    pub composition_us: f64,
    pub presentation_us: f64,
    pub total_us: f64,
}
/// Four reusable timestamp staging buffers. Busy slots are skipped, never
/// waited on by the frame loop. These are timing words, not image readbacks.
pub struct GpuTimer {
    map_failures: crate::observability::FailureCounter,
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: [Slot; SLOTS],
    period: f64,
    pub skipped: u64,
    pub errors: u64,
}
impl GpuTimer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let period = f64::from(queue.get_timestamp_period());
        if !period.is_finite() || period <= 0.0 {
            return None;
        }
        Some(Self {
            map_failures: crate::observability::FailureCounter::new(),
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("Motion Studio pass timings"),
                ty: wgpu::QueryType::Timestamp,
                count: SLOTS as u32 * QUERIES,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Motion Studio timing resolve"),
                size: SLOTS as u64 * wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            slots: std::array::from_fn(|_| Slot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Motion Studio timing words"),
                    size: TIMESTAMP_BUFFER_BYTES as u64,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                state: Arc::new(AtomicU8::new(0)),
                sequence: 0,
            }),
            period,
            skipped: 0,
            errors: 0,
        })
    }
    pub fn begin(&mut self, sequence: u64) -> Option<usize> {
        for (i, s) in self.slots.iter_mut().enumerate() {
            if s.state.load(Ordering::Acquire) == 0 {
                s.sequence = sequence;
                s.state.store(1, Ordering::Release);
                return Some(i);
            }
        }
        self.skipped += 1;
        None
    }
    /// Release a reservation when encoding failed and the command buffer was
    /// discarded. Must never be called after submission or `map`.
    pub fn cancel_unsubmitted(&mut self, slot: usize) {
        self.slots[slot].state.store(0, Ordering::Release);
    }
    pub fn writes(&self, slot: usize, pass: u32) -> wgpu::RenderPassTimestampWrites<'_> {
        let first = slot as u32 * QUERIES + pass * 2;
        wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(first),
            end_of_pass_write_index: Some(first + 1),
        }
    }
    pub fn resolve(&self, slot: usize, encoder: &mut wgpu::CommandEncoder) {
        let offset = slot as u64 * wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT;
        let first = slot as u32 * QUERIES;
        encoder.resolve_query_set(&self.queries, first..first + QUERIES, &self.resolve, offset);
        encoder.copy_buffer_to_buffer(&self.resolve, offset, &self.slots[slot].buffer, 0, 32);
    }
    pub fn map(&self, slot: usize) {
        let state = self.slots[slot].state.clone();
        self.slots[slot]
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                state.store(if result.is_ok() { 2 } else { 3 }, Ordering::Release)
            });
    }
    pub fn collect(&mut self) -> [Option<GpuTiming>; SLOTS] {
        std::array::from_fn(|i| {
            let s = &mut self.slots[i];
            let state = s.state.load(Ordering::Acquire);
            if state < 2 {
                return None;
            }
            let result = if state == 2 {
                let words = {
                    match s.buffer.slice(..).get_mapped_range() {
                        Ok(mapped) => { self.map_failures.success(); read_timestamp_words(&mapped) },
                        Err(error) => { self.map_failures.failure("render.timestamp.readback", format_args!("slot={i} sequence={} cause={error}", s.sequence)); None },
                    }
                };
                s.buffer.unmap();
                if let Some(words) = words.filter(|words| {
                    words[1] > words[0] && words[3] > words[2] && words[3] > words[0]
                }) {
                    Some(GpuTiming {
                        sequence: s.sequence,
                        composition_us: (words[1] - words[0]) as f64 * self.period / 1000.0,
                        presentation_us: (words[3] - words[2]) as f64 * self.period / 1000.0,
                        total_us: (words[3] - words[0]) as f64 * self.period / 1000.0,
                    })
                } else {
                    self.errors += 1;
                    None
                }
            } else {
                self.errors += 1;
                None
            };
            s.state.store(0, Ordering::Release);
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{read_timestamp_words, TIMESTAMP_BUFFER_BYTES};

    #[test]
    fn timestamp_reader_rejects_short_buffers_without_panicking() {
        assert_eq!(read_timestamp_words(&[]), None);
        assert_eq!(read_timestamp_words(&vec![0; TIMESTAMP_BUFFER_BYTES - 1]), None);
    }

    #[test]
    fn timestamp_reader_accepts_exact_buffer_size() {
        let mut bytes = vec![0; TIMESTAMP_BUFFER_BYTES];
        bytes[0..8].copy_from_slice(&11u64.to_le_bytes());
        bytes[8..16].copy_from_slice(&22u64.to_le_bytes());
        bytes[16..24].copy_from_slice(&33u64.to_le_bytes());
        bytes[24..32].copy_from_slice(&44u64.to_le_bytes());
        assert_eq!(read_timestamp_words(&bytes), Some([11, 22, 33, 44]));
    }
}

#[cfg(test)]
#[path = "timing/observation_tests.rs"]
mod observation_tests;
