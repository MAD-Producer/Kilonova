//! Transform sampled paint colors without mutating authored tracks or geometry.
use super::*;
use motion_model::vector::SampledVector;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct WorkingVectors {
    // Two source identities per layer cover the sampler's two reusable scenes.
    // A sampler owns one immutable native/alpha/range contract for its lifetime.
    cache: HashMap<u64, CachedPaints>,
}
type Pair = (Arc<SampledVector>, Arc<SampledVector>);
#[derive(Default)]
struct CachedPaints {
    recent: Option<Pair>,
    previous: Option<Pair>,
}
impl WorkingVectors {
    pub(super) fn apply(
        &mut self,
        scene: &mut Scene,
        native: &NativeColorProgram,
        range: CodeValueRange,
        alpha: AlphaMode,
    ) -> Result<()> {
        self.cache.retain(|id, _| {
            scene
                .layers
                .iter()
                .any(|layer| layer.id == *id && layer.vector.is_some())
        });
        for layer in &mut scene.layers {
            let Some(source) = &layer.vector else {
                continue;
            };
            let working = if let Some((_, converted)) =
                self.cache.get(&layer.id).and_then(|entry| {
                    entry
                        .recent
                        .iter()
                        .chain(entry.previous.iter())
                        .find(|(original, _)| Arc::ptr_eq(original, source))
                }) {
                converted.clone()
            } else {
                let mut converted = source.as_ref().clone();
                convert(&mut converted, native, range, alpha, layer.id)?;
                let converted = Arc::new(converted);
                let entry = self.cache.entry(layer.id).or_default();
                entry.previous = entry.recent.take();
                entry.recent = Some((source.clone(), converted.clone()));
                converted
            };
            layer.vector = Some(working);
        }
        Ok(())
    }
}
fn convert(
    vector: &mut SampledVector,
    native: &NativeColorProgram,
    range: CodeValueRange,
    alpha: AlphaMode,
    object: u64,
) -> Result<()> {
    if let Some(color) = &mut vector.fill {
        *color = transform(native, range, alpha, Some(object), *color)?;
    }
    if let Some(stroke) = &mut vector.stroke {
        stroke.0 = transform(native, range, alpha, Some(object), stroke.0)?;
    }
    if let Some(batches) = &mut vector.batches {
        // Group sampling flattens nested groups into paint batches. Refuse a
        // malformed recursive sampled producer before walking an unbounded tree.
        for batch in batches {
            if batch.vector.batches.is_some() {
                return Err(refuse(
                    "vector_paint",
                    Some(object),
                    "nested sampled paint batches are not a flattened vector",
                ));
            }
            convert(&mut batch.vector, native, range, alpha, object)?;
        }
    }
    Ok(())
}
