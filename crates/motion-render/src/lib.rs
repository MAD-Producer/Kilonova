//! Native GPU rendering. Image decoding/upload happens on resource changes;
//! playback uploads only fixed-size layer uniforms.
mod effect_gpu;
pub mod effect_plan;
pub mod image_formats;
mod image_capture;
pub use image_capture::{CapturedImage, ImageCaptureTarget};
mod color_output;
pub use color_output::{ColorOutput, ColorOutputDescriptor, OutputTransform, SdrCaptureTarget, ToneMap};
pub mod observability;
pub mod resource_policy;
mod resource_error;
pub use resource_error::{ResourceBudgetScope, ResourceRejection};
pub mod preview_memory;
mod preview_targets;
pub use preview_targets::PreviewFrameTargets;
mod effect_plan_cache;
pub mod composition_plan;
mod measurement;
mod readback;
pub use readback::{ImageReadbackInfo, TimestampReadbackInfo};
mod memory;
mod memory_pressure;
pub use memory_pressure::{MemoryPressureLogScope, MemoryPressurePool, MemoryPressureReading, MemoryPressureSnapshot,
    MemoryPressureSource, MemoryPressureUnavailable};
mod resource_ledger;
mod submission_ledger;
pub use submission_ledger::{SubmissionMemoryInfo, SubmissionMemorySnapshot, MAX_SUBMISSION_SNAPSHOTS};
mod frame_memory;
pub use frame_memory::FrameTextureCapacity;
mod allocation_budget;
mod allocation_buffer;
mod allocation_uploads;
mod native_allocation;
pub use native_allocation::{AdmittedBuffer, AdmittedTexture, AllocationContext};
pub use allocation_budget::{AllocationBudget, AllocationBudgetInfo, AllocationEncoder,
    AllocationLogScope,
    AllocationCategory, AllocationCategoryInfo, AllocationCategoryFrameInfo,
    AllocationFrame, AllocationFrameInfo, AllocationFrameOutcome, AllocationFramePeaks,
    FrameOperation, MAX_ACTIVE_ALLOCATION_FRAMES, MAX_RECENT_ALLOCATION_FRAMES,
    AllocatedCommandBuffer, AdmittedTextureView, DEFAULT_RENDER_ALLOCATION_BUDGET,
    MAX_RENDER_ALLOCATION_BUDGET};
pub use resource_ledger::{TextureCategory, TextureState, TextureReference, TextureResource, TextureResourceLedger};
mod presenter;
mod viewport;
pub use viewport::fit_viewport;
mod quality;
mod raster_density;
mod renderer;
pub mod image_resources;
pub mod preview_input;
pub mod scene_generator;
mod particle_emitter;
mod timing;
pub mod gpu_commands;
mod video_pixels;
mod video_gpu;
mod vector_mesh;
pub use vector_mesh::VectorGeometryError;
mod layer_gpu;
mod vector_upload;
pub use vector_upload::{VectorUploadError, VectorUploadInfo};
pub mod mask_plan;
pub mod compositing_plan;
mod compositing_gpu;
mod mask_gpu;
pub use video_pixels::{rgb_video_to_rgba, ChromaLayout, VideoPlane, VideoRgbFormat, Yuv420Frame};
pub use video_gpu::EncodedVideoUpload;
pub use measurement::{FrameMeasurement, FrameRecorder, FrameResources};
pub use memory::TextureMemoryBreakdown;
pub use presenter::Presenter;
pub use quality::{PreviewMode, PreviewPolicy, PreviewTier};
pub use renderer::premultiply_pixels;
pub use renderer::{CaptureTarget, ExposureTarget, LayerRaster, RenderError, RenderStats, RenderTarget, Renderer};
pub use renderer::FIXED_RENDERER_ALLOCATION_BYTES;
pub use renderer::{ExposureFrame, ExposurePoll, ExposureProgress, ExposureStage};
pub use timing::{GpuTimer, GpuTiming};

mod scene;
pub mod exposure_scene;
mod compositor;
mod plane_sampling;
mod plane_coverage;
mod plane_coordinates;
pub mod selection_geometry;
pub mod position_path;
pub use scene::{DrawLayer, HitCandidate, NestedScene, Scene};
pub use compositor::{PlaneBatch, PlaneCompositor, PlaneVertex};
