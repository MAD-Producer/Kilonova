# External native allocations

`Renderer::allocation_context()` clones that renderer's existing native owner,
live `AllocationBudget`, and queued-upload retention ledger. It does not create
an independent allowance or a GPU device. The context remains on its owning
thread and can outlive the renderer; clones share the same identity.

- `texture(descriptor)` reserves the checked texture descriptor capacity before
  native creation, including format blocks, dimensions, mips and sample count
- `buffer_init(descriptor)` reserves the actual mapped-at-creation capacity,
  including `COPY_BUFFER_ALIGNMENT` padding; empty/overflowing input is refused
- `write_texture` writes the base mip and `write_buffer` writes an existing
  admitted buffer after retaining the destination's existing reservation
- Uploads from another renderer are refused before the native queue call, even
  when both renderers share the same logical allowance
- `AdmittedTexture::view()` and its clones retain the original single reservation

The buffer/texture wrappers implement `Deref` for trusted native integration.
Raw views, bind groups and handle aliases must remain inside the lifetime of an
admitted owner, admitted view, or allocated command scope. These APIs are not an
untrusted resource sandbox. Use `allocation_command_encoder` and
`submit_allocated` for commands referencing enrolled resources. Open encoders
also retain subsequently admitted resources; abandoned encoders, unsubmitted
command buffers and completed submissions retire through their existing paths.

Queued writes retain their destination after resource wrappers drop and are
released after the renderer's next submission completes. Polling alone does not
retire unsubmitted writes. If the last shared upload ledger is dropped with
pending writes, it submits them and installs a completion callback without a
blocking wait or retry. GPU staging and driver padding/residency are outside
this descriptor-capacity accounting.

The desktop's independent node preview uses this context for its sample image,
LUT, graph uniform buffer, 512-square RGBA16Float output, and display uniform
buffer. Its color-contract buffer is reused when the presentation format
changes. Node capture, UI capture and both presentation branches use allocated
submissions so replacing a graph cannot release live command reservations early.

Budget refusal retains its typed `ResourceRejection` through the native renderer
and host boundary, then uses the existing string reply only at the desktop
protocol boundary. A failed installation preserves the installed graph and its
stamp. Restoring the allowance permits a normal new installation at the same
raster dimensions and precision. Construction validation errors retain the
primary admission cause if a secondary GPU scope error also occurs.

This extends enrollment, not the complete #61 policy loop. Presenter/UI buffers,
swapchain images, driver overhead and other documented categories remain outside
this increment. `allocation_budget.completeCoverage` remains false. Native
pressure queries do not automatically change this allowance, and no proxy,
quality selection, retry, timeout or project/undo mutation is added here.

Exact-source runtime acceptance and remaining platform limits are recorded in
`engineering/validation/frame-memory-admission.md`; compilation alone does not
prove native retirement or preservation of the last valid pixels.
