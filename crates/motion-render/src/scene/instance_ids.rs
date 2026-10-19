//! Stable aliases for the document's bounded authored reference paths.
//! Inactive paths reserve IDs, not GPU resources. Exact topology comparison
//! avoids per-frame graph expansion and preserves fresh/reused export equality.
use motion_model::{composition, Content, Layer, Project, Result, MAX_LAYERS};
use std::collections::HashMap;

#[derive(Clone, Debug)]
struct Node {
    id: String,
    layers: Vec<(u64, Option<String>)>,
    indices: HashMap<u64, u64>,
}
impl Node {
    fn new(id: &str, layers: &[Layer]) -> Result<Self> {
        if layers.len() > MAX_LAYERS {
            return limit(id, "Instance source layer limit exceeded");
        }
        let mut indices = HashMap::with_capacity(layers.len());
        for (index, layer) in layers.iter().enumerate() {
            if layer.id == 0 || indices.insert(layer.id, index as u64).is_some() {
                return fault(
                    id,
                    "Invalid instance source object ID",
                    serde_json::json!({"object":layer.id}),
                );
            }
        }
        Ok(Self {
            id: id.into(),
            layers: layers
                .iter()
                .map(|l| (l.id, reference(l).map(str::to_owned)))
                .collect(),
            indices,
        })
    }
    fn matches(&self, id: &str, layers: &[Layer]) -> bool {
        self.id == id
            && self.layers.len() == layers.len()
            && self.layers.iter().zip(layers).all(|((id, source), layer)| {
                *id == layer.id && source.as_deref() == reference(layer)
            })
    }
}
fn reference(layer: &Layer) -> Option<&str> {
    match &layer.content {
        Content::Composition { clip } => Some(&clip.composition),
        _ => None,
    }
}
#[derive(Clone, Copy, Debug)]
struct Path {
    node: usize,
    ordinal: u64,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct InstanceIds {
    nodes: Vec<Node>,
    paths: HashMap<Vec<u64>, Path>,
    base: u64,
}
impl InstanceIds {
    #[cfg(test)]
    pub fn prepare(&mut self, project: &Project) -> Result<()> {
        self.prepare_composition(project, project)
    }
    pub fn prepare_composition(&mut self, document: &Project, root: &Project) -> Result<()> {
        let sources = || {
            std::iter::once((root.composition_id.as_str(), root.layers.as_slice()))
                .chain(
                    std::iter::once((document.composition_id.as_str(), document.layers.as_slice()))
                        .filter(|(id, _)| *id != root.composition_id),
                )
                .chain(
                    document
                        .compositions
                        .iter()
                        .filter(|c| {
                            root.composition_id == document.composition_id
                                || c.id != root.composition_id
                        })
                        .map(|c| (c.id.as_str(), c.layers.as_slice())),
                )
        };
        if self.nodes.len() == document.compositions.len() + 1
            && self
                .nodes
                .iter()
                .zip(sources())
                .all(|(n, (id, layers))| n.matches(id, layers))
        {
            return Ok(());
        }
        // Build off to the side: invalid edits cannot leave a partial layout.
        let candidate = Self::build(document, root, sources())?;
        *self = candidate;
        Ok(())
    }
    fn build<'a>(
        project: &Project,
        root: &Project,
        sources: impl Iterator<Item = (&'a str, &'a [Layer])>,
    ) -> Result<Self> {
        if project.compositions.len() >= composition::MAX_COMPOSITIONS {
            return limit(
                &project.composition_id,
                "Instance composition limit exceeded",
            );
        }
        let mut nodes = Vec::with_capacity(project.compositions.len() + 1);
        for (id, layers) in sources {
            nodes.push(Node::new(id, layers)?);
        }
        let mut indices = HashMap::with_capacity(nodes.len());
        for (index, node) in nodes.iter().enumerate() {
            if indices.insert(node.id.as_str(), index).is_some() {
                return fault(
                    &node.id,
                    "Duplicate instance composition ID",
                    serde_json::json!({}),
                );
            }
        }
        let mut paths = HashMap::new();
        visit(
            &nodes,
            &indices,
            0,
            &mut Vec::new(),
            &mut vec![0],
            &mut paths,
        )?;
        // Reserve one contiguous range for every authored path's layer slots.
        // A bounded document always leaves ample gaps, including when authored
        // IDs happen to lie in the usual high-ID namespace.
        let span = paths.len() as u64 * MAX_LAYERS as u64;
        let mut base = 1u64 << 40;
        if span > 0 {
            let mut authored: Vec<_> = nodes
                .iter()
                .flat_map(|n| n.layers.iter().map(|l| l.0))
                .collect();
            authored.sort_unstable();
            for id in authored {
                if id < base {
                    continue;
                }
                let Some(end) = base.checked_add(span - 1) else {
                    return limit(&root.composition_id, "Instance alias range exhausted");
                };
                if id > end {
                    break;
                }
                let Some(next) = id.checked_add(1) else {
                    return limit(&root.composition_id, "Instance alias range exhausted");
                };
                base = next;
            }
            if base.checked_add(span - 1).is_none() {
                return limit(&root.composition_id, "Instance alias range exhausted");
            }
        }
        Ok(Self { nodes, paths, base })
    }
    pub fn mapping(
        &self,
        path: &[u64],
        composition: &str,
        objects: &[u64],
    ) -> Result<HashMap<u64, u64>> {
        let Some(address) = self.paths.get(path) else {
            return fault(
                composition,
                "Instance reference path is missing",
                serde_json::json!({"reference_path":bounded_path(path)}),
            );
        };
        let node = &self.nodes[address.node];
        if node.id != composition || objects.len() > MAX_LAYERS + 1 {
            return fault(
                composition,
                "Instance source differs from the authored reference path",
                serde_json::json!({"reference_path":bounded_path(path),"authored_composition":node.id,"sampled_objects":objects.len()}),
            );
        }
        let mut mapping = HashMap::with_capacity(objects.len());
        for &object in objects.iter().filter(|&&id| id != 0) {
            let Some(&index) = node.indices.get(&object) else {
                return composition::fail(
                    composition,
                    "sample_failed",
                    "Instance source object is missing",
                    serde_json::json!({"phase":"instance_alias","object":object}),
                );
            };
            let alias = self.base + address.ordinal * MAX_LAYERS as u64 + index;
            if mapping.insert(object, alias).is_some() {
                return fault(
                    composition,
                    "Duplicate sampled instance source object",
                    serde_json::json!({"object":object,"reference_path":bounded_path(path)}),
                );
            }
        }
        Ok(mapping)
    }
}
fn visit(
    nodes: &[Node],
    indices: &HashMap<&str, usize>,
    index: usize,
    path: &mut Vec<u64>,
    ancestors: &mut Vec<usize>,
    paths: &mut HashMap<Vec<u64>, Path>,
) -> Result<()> {
    let node = &nodes[index];
    for (object, source) in &node.layers {
        let Some(source) = source else {
            continue;
        };
        let Some(&child) = indices.get(source.as_str()) else {
            return composition::fail(
                source,
                "composition_missing",
                "Composition does not exist",
                serde_json::json!({"phase":"instance_alias","parent_composition":node.id,"reference":object}),
            );
        };
        if ancestors.contains(&child) {
            return composition::fail(
                source,
                "cycle",
                "Composition reference cycle in instance layout",
                serde_json::json!({"phase":"instance_alias","parent_composition":node.id,"reference":object,"reference_path":path}),
            );
        }
        if ancestors.len() >= composition::MAX_COMPOSITION_DEPTH {
            return limit(source, "Instance reference depth exceeded");
        }
        if paths.len() + 1 >= composition::MAX_COMPOSITION_INSTANCES {
            return limit(source, "Instance reference expansion limit exceeded");
        }
        path.push(*object);
        paths.insert(
            path.clone(),
            Path {
                node: child,
                ordinal: paths.len() as u64,
            },
        );
        ancestors.push(child);
        let result = visit(nodes, indices, child, path, ancestors, paths);
        ancestors.pop();
        path.pop();
        result?;
    }
    Ok(())
}
fn limit<T>(composition: &str, message: &str) -> Result<T> {
    composition::fail(
        composition,
        "render_resource_limit",
        message,
        serde_json::json!({"phase":"instance_alias","max_compositions":composition::MAX_COMPOSITIONS,
            "max_instances":composition::MAX_COMPOSITION_INSTANCES,"max_depth":composition::MAX_COMPOSITION_DEPTH}),
    )
}
fn bounded_path(path: &[u64]) -> &[u64] {
    &path[..path.len().min(composition::MAX_COMPOSITION_DEPTH)]
}
fn fault<T>(composition: &str, message: &str, mut details: serde_json::Value) -> Result<T> {
    details["phase"] = "instance_alias".into();
    composition::fail(composition, "sample_failed", message, details)
}
pub(crate) fn alias(mapping: &HashMap<u64, u64>, composition: &str, object: u64) -> Result<u64> {
    match mapping.get(&object).copied() {
        Some(alias) => Ok(alias),
        None => composition::fail(
            composition,
            "sample_failed",
            "Instance source object is missing",
            serde_json::json!({"phase":"instance_alias","object":object}),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion_model::{Composition, CompositionClip, Error};
    use std::collections::HashSet;

    fn reference(id: u64, source: &str) -> Layer {
        let mut layer = Layer::solid(id, "reference", [16.; 2], [8., 8., 0.], [1.; 4]);
        layer.content = Content::Composition {
            clip: CompositionClip::new(source.into()),
        };
        layer
    }
    fn node(p: &Project, id: &str, layers: Vec<Layer>) -> Composition {
        Composition {
            id: id.into(),
            name: id.into(),
            width: 16,
            height: 16,
            fps: 30,
            frames: 60,
            background: p.background,
            camera: p.camera.clone(),
            layers,
            expressions: vec![],
        }
    }
    fn fixture() -> Project {
        let mut p = Project::new(16, 16, 30, 60).unwrap();
        p.layers = vec![reference(1, "child"), reference(2, "child")];
        p.compositions.push(node(
            &p,
            "child",
            vec![Layer::solid(1, "source", [16.; 2], [8., 8., 0.], [1.; 4])],
        ));
        p
    }
    fn error(result: Result<()>, code: &str) -> composition::CompositionError {
        let Error::Composition(error) = result.unwrap_err() else {
            panic!("expected structured composition failure")
        };
        assert_eq!(error.code, code);
        assert_eq!(error.details["phase"], "instance_alias");
        error
    }

    #[test]
    fn authored_high_ids_do_not_collide_and_fresh_layouts_agree() {
        let mut p = fixture();
        let high = (1u64 << 40) + 10;
        p.layers
            .push(Layer::solid(high, "high root", [1.; 2], [0.; 3], [1.; 4]));
        for id in [high + 20, u64::MAX] {
            p.compositions[0].layers.push(Layer::solid(
                id,
                "high child",
                [1.; 2],
                [0.; 3],
                [1.; 4],
            ));
        }
        let authored: HashSet<_> = p
            .layers
            .iter()
            .chain(&p.compositions[0].layers)
            .map(|l| l.id)
            .collect();
        let mut first = InstanceIds::default();
        first.prepare(&p).unwrap();
        let mut fresh = InstanceIds::default();
        fresh.prepare(&p).unwrap();
        let mut aliases = HashSet::new();
        for path in [[1], [2]] {
            let map = first
                .mapping(&path, "child", &[0, 1, high + 20, u64::MAX])
                .unwrap();
            assert_eq!(
                map,
                fresh
                    .mapping(&path, "child", &[0, 1, high + 20, u64::MAX])
                    .unwrap()
            );
            assert_eq!(map.len(), 3);
            for alias in map.values() {
                assert!(!authored.contains(alias));
                assert!(aliases.insert(*alias));
            }
        }
    }

    #[test]
    fn topology_changes_rebuild_and_invalid_edits_leave_the_previous_layout_intact() {
        let mut p = fixture();
        let mut ids = InstanceIds::default();
        ids.prepare(&p).unwrap();
        let before = ids.mapping(&[2], "child", &[1]).unwrap();
        p.layers[0].visible = false;
        p.layers[0].transform.position.value[0] = 100.;
        ids.prepare(&p).unwrap();
        assert_eq!(before, ids.mapping(&[2], "child", &[1]).unwrap());
        let invalid = [
            ("absent", "composition_missing"),
            (p.composition_id.as_str(), "cycle"),
        ];
        for (target, code) in invalid {
            let mut broken = p.clone();
            broken.layers[0] = reference(1, target);
            let failure = error(ids.prepare(&broken), code);
            assert_eq!(failure.details["reference"], 1);
            assert_eq!(before, ids.mapping(&[2], "child", &[1]).unwrap());
        }
        p.compositions.push(node(
            &p,
            "replacement",
            vec![Layer::solid(99, "new source", [1.; 2], [0.; 3], [1.; 4])],
        ));
        p.layers[0] = reference(1, "replacement");
        ids.prepare(&p).unwrap();
        assert!(ids.mapping(&[1], "replacement", &[99]).is_ok());
        assert!(ids.mapping(&[1], "child", &[1]).is_err());
        assert_eq!(before, ids.mapping(&[2], "child", &[1]).unwrap());
        p.layers.swap(0, 1);
        ids.prepare(&p).unwrap();
        let mut fresh = InstanceIds::default();
        fresh.prepare(&p).unwrap();
        assert_eq!(
            ids.mapping(&[2], "child", &[1]).unwrap(),
            fresh.mapping(&[2], "child", &[1]).unwrap()
        );
    }

    #[test]
    fn depth_and_static_expansion_use_model_limits_including_inactive_paths() {
        let mut p = Project::new(16, 16, 30, 60).unwrap();
        p.layers = vec![reference(1, "level-1")];
        for i in 1..composition::MAX_COMPOSITION_DEPTH {
            let layers = if i + 1 < composition::MAX_COMPOSITION_DEPTH {
                vec![reference(1, &format!("level-{}", i + 1))]
            } else {
                vec![]
            };
            p.compositions.push(node(&p, &format!("level-{i}"), layers));
        }
        let mut ids = InstanceIds::default();
        ids.prepare(&p).unwrap();
        p.compositions
            .last_mut()
            .unwrap()
            .layers
            .push(reference(1, "too-deep"));
        p.compositions.push(node(&p, "too-deep", vec![]));
        error(ids.prepare(&p), "render_resource_limit");

        let mut p = Project::new(16, 16, 30, 60).unwrap();
        p.layers = (1..=8)
            .map(|id| reference(id, "branch"))
            .chain((9..=15).map(|id| reference(id, "leaf")))
            .collect();
        p.layers[0].visible = false;
        p.compositions.push(node(
            &p,
            "branch",
            (1..=126).map(|id| reference(id, "leaf")).collect(),
        ));
        p.compositions.push(node(&p, "leaf", vec![]));
        // 1 root + 8 branches + 8*126 leaves + 7 direct leaves = 1024.
        ids.prepare(&p).unwrap();
        p.layers.push(reference(16, "leaf"));
        error(ids.prepare(&p), "render_resource_limit");
    }

    #[test]
    fn library_layer_and_mapping_failures_remain_bounded_and_diagnosable() {
        let mut p = fixture();
        let mut ids = InstanceIds::default();
        ids.prepare(&p).unwrap();
        for (path, source, objects) in [
            (&[99][..], "child", vec![1]),
            (&[1][..], "wrong", vec![1]),
            (&[1][..], "child", vec![1, 1]),
            (&[1][..], "child", vec![999]),
        ] {
            error(
                ids.mapping(path, source, &objects).map(|_| ()),
                "sample_failed",
            );
        }
        let mut bad = p.clone();
        bad.compositions[0].layers[0].id = 0;
        let failure = error(ids.prepare(&bad), "sample_failed");
        assert_eq!(failure.details["object"], 0);
        bad = p.clone();
        bad.compositions.push(bad.compositions[0].clone());
        error(ids.prepare(&bad), "sample_failed");
        bad.compositions.last_mut().unwrap().id = bad.composition_id.clone();
        error(ids.prepare(&bad), "sample_failed");
        p.compositions[0].layers = (1..=MAX_LAYERS as u64)
            .map(|id| Layer::solid(id, "source", [1.; 2], [0.; 3], [1.; 4]))
            .collect();
        ids.prepare(&p).unwrap();
        p.compositions[0].layers.push(Layer::solid(
            MAX_LAYERS as u64 + 1,
            "overflow",
            [1.; 2],
            [0.; 3],
            [1.; 4],
        ));
        error(ids.prepare(&p), "render_resource_limit");
        p.compositions[0].layers.clear();
        for i in 1..composition::MAX_COMPOSITIONS - 1 {
            p.compositions
                .push(node(&p, &format!("unused-{i}"), vec![]));
        }
        ids.prepare(&p).unwrap();
        p.compositions.push(node(&p, "one-too-many", vec![]));
        error(ids.prepare(&p), "render_resource_limit");
    }
}
