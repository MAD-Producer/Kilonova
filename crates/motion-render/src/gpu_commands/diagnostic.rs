//! Explicit native corruption injection for sink/threshold acceptance. This
//! module does not exist in Release builds and is never called by profiling
//! initialization or ordinary rendering. It corrupts diagnostic words only.
use super::*;

struct RetireDiagnosticBuffer {
    queue: wgpu::Queue,
    buffer: Option<AllocationBuffer>,
}
impl Drop for RetireDiagnosticBuffer {
    fn drop(&mut self) {
        let buffer = self.buffer.take();
        // Preserve admission through the real queue fence, including submit
        // unwind, without masking the original failure or retaining Pool.
        self.queue.on_submitted_work_done(move || drop(buffer));
    }
}

impl AttachedCommandProfiler {
    pub(crate) fn debug_inject_invalid_command_words(
        &self,
        renderer: &Renderer,
    ) -> Result<CommandIdentity, RenderError> {
        let span = self
            .0
            .begin(
                CommandStage::ReadbackCopy,
                renderer.allocation_budget().gpu_command_contexts(),
            )
            .ok_or_else(|| {
                RenderError::Invalid("diagnostic command observation unavailable".into())
            })?;
        let slot = span.slot.clone();
        let zero = AllocationBuffer::new_init(
            &renderer.device,
            renderer.allocation_budget(),
            &wgpu::util::BufferInitDescriptor {
                label: Some("Motion Studio diagnostic invalid timestamp words"),
                contents: &[0; WORD_BYTES as usize],
                usage: wgpu::BufferUsages::COPY_SRC,
            },
        )?;
        let _retire = RetireDiagnosticBuffer {
            queue: renderer.queue.clone(),
            buffer: Some(zero.clone()),
        };
        let mut encoder = ObservedEncoder::new(renderer.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Motion Studio diagnostic timestamp query"),
            },
        ));
        encoder.observe(Some(span));
        let commands = encoder.finish();
        let identity = commands.command_identity().ok_or_else(|| {
            RenderError::Invalid("diagnostic command identity unavailable".into())
        })?;
        let mut overwrite =
            renderer
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Motion Studio diagnostic timestamp corruption"),
                });
        // Two actual native commands, ordered on the original queue. The real
        // map callback reads the overwritten GPU buffer; no success/error result
        // or readback counter is fabricated on the CPU.
        overwrite.copy_buffer_to_buffer(&zero, 0, &slot.mapped, 0, WORD_BYTES);
        commands.submit(|commands| renderer.queue.submit([commands, overwrite.finish()]));
        Ok(identity)
    }
}
