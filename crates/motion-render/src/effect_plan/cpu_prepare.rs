//! Pure CPU work only. A batch retains at most four bounded tessellation
//! results; cache admission and the frame-wide vertex budget stay ordered.
use super::*;
use crate::vector_mesh::{VectorGeometryError, VectorMesh, MAX_TRIANGLE_VERTICES};
use std::{
    hash::{Hash, Hasher},
    panic::{catch_unwind, AssertUnwindSafe},
};

const MAX_WORKERS: usize = 4;

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct CpuPreparationInfo {
    /// Per-build observations, reset even when preparation subsequently fails.
    pub vector_jobs: usize,
    pub vector_cache_hits: usize,
    pub largest_batch: usize,
    pub worker_limit: usize,
    /// Logical vertex bytes retained by completed jobs in one batch. Excludes
    /// Lyon's temporary buffers, cached meshes and allocator/driver overhead.
    pub peak_pending_vertex_bytes: usize,
}

struct VectorJob {
    layer: usize,
    id: u64,
    vector: Arc<motion_model::vector::SampledVector>,
    size: [f32; 2],
    origin: [f32; 2],
    scale: f32,
    width: u32,
    height: u32,
}

fn ordered_batch<T: Send, R: Send>(
    jobs: Vec<T>,
    work: impl Fn(T) -> R + Sync,
) -> Result<Vec<R>, std::io::Error> {
    if jobs.len() > MAX_WORKERS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "CPU preparation batch exceeds worker limit",
        ));
    }
    std::thread::scope(|scope| {
        let mut jobs = jobs.into_iter();
        let Some(first) = jobs.next() else {
            return Ok(Vec::new());
        };
        let mut handles = Vec::new();
        let mut spawn_error = None;
        for job in jobs {
            let work = &work;
            match std::thread::Builder::new()
                .name("motion-vector-prepare".into())
                .spawn_scoped(scope, move || work(job))
            {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    spawn_error = Some(error);
                    break;
                }
            }
        }
        // Join every worker before propagating the first panic in scene order.
        // This only delays unwinding for cleanup; the original payload and the
        // hook on the originating thread are preserved.
        let mut results = vec![catch_unwind(AssertUnwindSafe(|| work(first)))];
        results.extend(handles.into_iter().map(|handle| handle.join()));
        let mut output = Vec::with_capacity(results.len());
        for result in results {
            match result {
                Ok(value) => output.push(value),
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }
        if let Some(error) = spawn_error {
            return Err(error);
        }
        Ok(output)
    })
}

impl VectorJob {
    fn rasterize(self, limit: usize) -> (Self, Result<VectorMesh, VectorGeometryError>) {
        let geometry = crate::vector_mesh::rasterize_with_limit(
            &self.vector,
            self.size,
            self.scale,
            self.origin,
            limit,
        );
        let mesh = geometry.map(|geometry| {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            bytemuck::cast_slice::<_, u8>(&geometry.vertices).hash(&mut hash);
            bytemuck::cast_slice::<_, u8>(&geometry.commands).hash(&mut hash);
            geometry.root_opacity.to_bits().hash(&mut hash);
            self.width.hash(&mut hash);
            self.height.hash(&mut hash);
            VectorMesh {
                layer: self.layer,
                width: self.width,
                height: self.height,
                fingerprint: hash.finish(),
                vertices: Arc::new(geometry.vertices),
                commands: Arc::new(geometry.commands),
                root_opacity: geometry.root_opacity,
                origin: self.origin,
            }
        });
        (self, mesh)
    }
}

impl PlanBuilder {
    pub fn cpu_preparation_info(&self) -> CpuPreparationInfo {
        self.cpu_preparation
    }

    pub(super) fn layer_scales(
        &self,
        scene: &Scene,
        size: [u32; 2],
        scale: f32,
        preview: bool,
    ) -> Vec<f32> {
        let image_sources: std::collections::HashSet<_> = scene
            .effects
            .iter()
            .filter(|e| e.enabled)
            .filter_map(|e| {
                e.image_input
                    .and_then(motion_model::EffectImageInput::layer)
            })
            .collect();
        let [_, _, viewport_width, viewport_height] = crate::fit_viewport(
            [0., 0., size[0] as f32, size[1] as f32],
            [scene.width, scene.height],
        );
        scene
            .layers
            .iter()
            .map(|layer| {
                let effects = || {
                    scene
                        .effects
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| e.layer == layer.id && e.enabled)
                };
                let scale = if preview
                    && !layer.adjustment
                    && !layer.composition
                    && !image_sources.contains(&layer.id)
                    && !effects().any(|(i, _)| {
                        self.resolved[i].as_ref().is_some_and(|r| {
                            r.definition.renderer != motion_effects::RendererKind::Image
                        })
                    }) {
                    crate::quality::preview_layer_scale(layer, scene.width, scene.height, scale)
                } else {
                    scale
                };
                if !layer.adjustment
                    && !layer.composition
                    && effects().next().is_some()
                    && effects().all(|(i, _)| {
                        self.resolved[i].as_ref().is_some_and(|r| {
                            r.definition.renderer == motion_effects::RendererKind::Image
                        })
                    })
                {
                    crate::quality::magnified_effect_scale(
                        layer,
                        [viewport_width, viewport_height],
                        scale,
                    )
                } else {
                    scale
                }
            })
            .collect()
    }

    pub(super) fn prepare_vectors(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
        scales: &[f32],
    ) -> Result<(), RenderError> {
        let workers = self.cpu_preparation.worker_limit.clamp(1, MAX_WORKERS);
        let mut remaining = MAX_TRIANGLE_VERTICES;
        let mut cursor = 0;
        while cursor < scene.layers.len() {
            let mut jobs = Vec::new();
            let mut ready = Vec::new();
            // Keep both pending work and completed results bounded. Cache hits
            // are admitted in the same scene order as newly prepared geometry.
            while cursor < scene.layers.len() && jobs.len() + ready.len() < workers {
                let index = cursor;
                cursor += 1;
                let layer = &scene.layers[index];
                let Some(vector) = &layer.vector else {
                    continue;
                };
                let scale = match crate::raster_density::vector_scale(
                    layer,
                    size[0],
                    size[1],
                    scales[index],
                ) {
                    Ok(scale) => scale,
                    Err(error) => {
                        ready.push((index, Err(RenderError::from(error))));
                        break;
                    }
                };
                let origin = [
                    layer.source_rect[0] + layer.size[0] * 0.5 - layer.source_size[0] * 0.5,
                    layer.source_rect[1] + layer.size[1] * 0.5 - layer.source_size[1] * 0.5,
                ];
                if let Some((_, _, _, mesh)) =
                    self.vector_cache.get(&layer.id).filter(|(v, s, r, mesh)| {
                        v == vector && *s == layer.size && *r == scale && mesh.origin == origin
                    })
                {
                    let mut mesh = mesh.clone();
                    mesh.layer = index;
                    ready.push((index, Ok(mesh)));
                    self.cpu_preparation.vector_cache_hits += 1;
                    continue;
                }
                let width = (layer.size[0] * scale).ceil() as u32;
                let height = (layer.size[1] * scale).ceil() as u32;
                if width > self.device_dimension || height > self.device_dimension {
                    let rejection = ResourceRejection::TextureDimensions {
                        width,
                        height,
                        max_dimension: self.device_dimension,
                    };
                    ready.push((
                        index,
                        Err(RenderError::Resource {
                            rejection,
                            message: format!(
                                "layer {}: vector raster exceeds device dimensions",
                                layer.id
                            ),
                        }),
                    ));
                    break;
                }
                jobs.push(VectorJob {
                    layer: index,
                    id: layer.id,
                    vector: vector.clone(),
                    size: layer.size,
                    origin,
                    scale,
                    width,
                    height,
                });
            }
            self.cpu_preparation.vector_jobs += jobs.len();
            self.cpu_preparation.largest_batch = self.cpu_preparation.largest_batch.max(jobs.len());
            let results = ordered_batch(jobs, |job| job.rasterize(remaining)).map_err(|cause| {
                RenderError::from(cause)
                    .with_diagnostic(format!("frame {}: vector CPU worker launch", scene.frame))
            })?;
            let pending_bytes = results
                .iter()
                .filter_map(|(_, result)| result.as_ref().ok())
                .map(|mesh| {
                    mesh.vertices.len() * std::mem::size_of::<crate::vector_mesh::VectorVertex>()
                })
                .sum();
            self.cpu_preparation.peak_pending_vertex_bytes = self
                .cpu_preparation
                .peak_pending_vertex_bytes
                .max(pending_bytes);
            let mut results = results.into_iter().peekable();
            let mut ready = ready.into_iter().peekable();
            while results.peek().is_some() || ready.peek().is_some() {
                let use_ready = ready
                    .peek()
                    .is_some_and(|(i, _)| results.peek().is_none_or(|(job, _)| i < &job.layer));
                let mesh = if use_ready {
                    let (_, result) = ready.next().ok_or("CPU vector result is absent")?;
                    match result {
                        Ok(mesh) => mesh,
                        Err(error) => {
                            if let RenderError::Resource { rejection, .. } = &error {
                                self.last_resource_rejection = Some(*rejection);
                            }
                            return Err(error);
                        }
                    }
                } else {
                    let (job, result) = results.next().ok_or("CPU vector result is absent")?;
                    let mesh = result.map_err(|cause| vector_error(scene, job.id, cause))?;
                    if mesh.vertices.len() > remaining {
                        return Err(vector_error(scene, job.id, VectorGeometryError::Invalid(format!(
                            "vector triangle vertex limit exceeded (262144; remaining output limit {remaining})"))));
                    }
                    self.vector_cache
                        .insert(job.id, (job.vector, job.size, job.scale, mesh.clone()));
                    mesh
                };
                remaining = remaining
                    .checked_sub(mesh.vertices.len())
                    .ok_or("frame vector vertex limit exceeded (262144)")?;
                self.frame.vectors.push(mesh);
            }
        }
        Ok(())
    }
}

fn vector_error(scene: &Scene, layer: u64, cause: VectorGeometryError) -> RenderError {
    let context = format!(
        "layer {layer}, vector rasterize at frame {}: {cause}",
        scene.frame
    );
    RenderError::from(cause).with_diagnostic(context)
}

pub(super) fn default_worker_limit() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(MAX_WORKERS)
}

#[cfg(test)]
mod tests;
