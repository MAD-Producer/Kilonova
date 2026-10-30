//! Runtime-only derived allocations. Payload sizes remain cached by owner;
//! history unions allocation identities rather than counting shared Arcs twice.
use super::*;
use motion_model::{SpatialCacheUsage, Tween};

#[derive(Clone, Default)]
struct NodeCaches {
    camera: Vec<SpatialCacheUsage>,
    layers: Arc<HashMap<u64, Vec<SpatialCacheUsage>>>,
}
fn track<T: Tween>(track: &motion_model::Track<T>, out: &mut Vec<SpatialCacheUsage>) {
    if let Some(animation) = &track.spatial_animation {
        out.push(animation.cache_usage());
    }
}
fn camera(camera: &Camera) -> Vec<SpatialCacheUsage> {
    let mut out = Vec::new();
    track(&camera.position, &mut out);
    track(&camera.target, &mut out);
    out
}
fn layer(layer: &Layer) -> Vec<SpatialCacheUsage> {
    let mut out = Vec::new();
    track(&layer.transform.position, &mut out);
    track(&layer.transform.anchor, &mut out);
    if let Some(data) = layer.camera_data() {
        out.extend(camera(data));
    }
    out
}
impl NodeCaches {
    fn new(data: &Camera, layers: &[Layer]) -> Self {
        Self {
            camera: camera(data),
            layers: Arc::new(
                layers
                    .iter()
                    .filter_map(|owner| {
                        let caches = layer(owner);
                        (!caches.is_empty()).then_some((owner.id, caches))
                    })
                    .collect(),
            ),
        }
    }
    fn update(&mut self, data: &Camera, layers: &[Layer], changes: &NodeChanges) {
        if changes.camera {
            self.camera = camera(data);
        }
        if !changes.layers.is_empty() {
            let cached = Arc::make_mut(&mut self.layers);
            for id in &changes.layers {
                let next = layers
                    .iter()
                    .find(|owner| owner.id == *id)
                    .map(layer)
                    .unwrap_or_default();
                if next.is_empty() {
                    cached.remove(id);
                } else {
                    cached.insert(*id, next);
                }
            }
        }
    }
    fn resources(&self) -> impl Iterator<Item = &SpatialCacheUsage> {
        self.camera.iter().chain(self.layers.values().flatten())
    }
}

#[derive(Clone, Default)]
pub(super) struct SpatialSizes(Arc<HashMap<String, NodeCaches>>);
impl SpatialSizes {
    pub(super) fn new(project: &Project) -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(
            project.composition_id.clone(),
            NodeCaches::new(&project.camera, &project.layers),
        );
        for node in &project.compositions {
            nodes.insert(node.id.clone(), NodeCaches::new(&node.camera, &node.layers));
        }
        Self(Arc::new(nodes))
    }
    pub(super) fn updated(&self, project: &Project, changes: &Changes) -> Self {
        if changes.all {
            return Self::new(project);
        }
        let mut next = self.clone();
        if !changes.nodes.is_empty() {
            let nodes = Arc::make_mut(&mut next.0);
            for (id, changed) in &changes.nodes {
                // A stale owner cache must never make a budget look smaller.
                let Ok(view) = project.composition_view(id) else {
                    return Self::new(project);
                };
                let Some(node) = nodes.get_mut(id) else {
                    return Self::new(project);
                };
                node.update(view.camera, view.layers, changed);
            }
        }
        next
    }
    fn resources(&self) -> impl Iterator<Item = &SpatialCacheUsage> {
        self.0.values().flat_map(NodeCaches::resources)
    }
}

#[derive(Default)]
pub(crate) struct HistoryUsage {
    pub(crate) model_bytes: usize,
    pub(crate) spatial_bytes: usize,
}
impl HistoryUsage {
    pub(crate) fn total(&self) -> usize {
        self.model_bytes.saturating_add(self.spatial_bytes)
    }
}
pub(crate) fn history_usage<'a>(sizes: impl Iterator<Item = &'a ProjectSize>) -> HistoryUsage {
    let mut usage = HistoryUsage::default();
    let mut seen = HashSet::new();
    for size in sizes {
        usage.model_bytes = usage.model_bytes.saturating_add(size.bytes());
        for cache in size.spatial.resources() {
            if seen.insert(cache) {
                usage.spatial_bytes = usage.spatial_bytes.saturating_add(cache.retained_bytes());
            }
        }
    }
    usage
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_byte_overflow_saturates_instead_of_wrapping_below_budget() {
        let mut size = ProjectSize::new(&Project::new(64, 64, 30, 60).unwrap());
        size.bytes = usize::MAX - 2;
        let usage = history_usage([&size, &size].into_iter());
        assert_eq!(usage.model_bytes, usize::MAX);
        assert_eq!(usage.total(), usize::MAX);
        assert_eq!(
            HistoryUsage {
                model_bytes: usize::MAX - 1,
                spatial_bytes: 2
            }
            .total(),
            usize::MAX
        );
    }
}
