use motion_model::{
    Camera, Composition, CompositionClip, Content, Layer, Project, MAIN_COMPOSITION,
};

fn id(index: usize) -> String {
    if index == 0 {
        MAIN_COMPOSITION.into()
    } else {
        format!("comp-{index}")
    }
}

fn project(edges: &[Vec<usize>]) -> Project {
    let layers = |source: usize| {
        edges[source]
            .iter()
            .enumerate()
            .map(|(index, &target)| {
                let mut layer = Layer::solid(
                    index as u64 + 1,
                    "Reference",
                    [32.; 2],
                    [16., 16., 0.],
                    [1.; 4],
                );
                layer.content = Content::Composition {
                    clip: CompositionClip::new(id(target)),
                };
                layer
            })
            .collect()
    };
    let mut result = Project::new(32, 32, 30, 60).unwrap();
    result.layers = layers(0);
    result.compositions = (1..edges.len())
        .map(|source| Composition {
            id: id(source),
            name: format!("Node {source}"),
            width: 32,
            height: 32,
            fps: 30,
            frames: 60,
            work_area: motion_model::WorkArea::full(60),
            background: [0.; 4],
            camera: Camera::new(32, 32),
            layers: layers(source),
            expressions: vec![],
        })
        .collect();
    result
}

// Independent specification: materialize the proposed edge first, then expand
// every reference occurrence. No memo table, Project validator, or special
// hypothetical-edge branch is shared with the implementation under test.
fn valid(edges: &[Vec<usize>]) -> bool {
    fn expand(edges: &[Vec<usize>], node: usize, path: &mut Vec<usize>) -> Option<usize> {
        if path.contains(&node) || path.len() == 16 {
            return None;
        }
        path.push(node);
        let mut instances = 1;
        for &child in &edges[node] {
            instances += expand(edges, child, path)?;
            if instances > 1024 {
                return None;
            }
        }
        path.pop();
        Some(instances)
    }
    (0..edges.len()).all(|root| expand(edges, root, &mut Vec::new()).is_some())
}

fn permitted(edges: &[Vec<usize>], source: usize, target: usize) -> bool {
    let mut proposed = edges.to_vec();
    proposed[source].push(target);
    valid(&proposed)
}

fn check(edges: &[Vec<usize>], active: usize, reverse: bool) -> usize {
    let mut project = project(edges);
    project.validate().unwrap();
    project.activate_composition(&id(active)).unwrap();
    if reverse {
        project.compositions.reverse();
    }
    project.validate().unwrap();
    let before = serde_json::to_vec(&project).unwrap();
    let visit_order = project.composition_ids();
    let mut comparisons = 0;
    for source in 0..edges.len() {
        let expected: Vec<_> = visit_order
            .iter()
            .filter(|target| {
                let target = (0..edges.len())
                    .find(|&index| id(index) == **target)
                    .unwrap();
                permitted(edges, source, target)
            })
            .cloned()
            .collect();
        assert_eq!(
            project
                .composition_reference_candidates(&id(source))
                .unwrap(),
            expected,
            "active={active} reverse={reverse} source={source} graph={edges:?}"
        );
        comparisons += edges.len();
    }
    assert_eq!(serde_json::to_vec(&project).unwrap(), before);
    comparisons
}

#[test]
fn all_four_node_dags_match_an_independent_expansion_in_every_active_view_and_both_orders() {
    let possible: Vec<_> = (0..4)
        .flat_map(|from| {
            (0..4)
                .filter(move |&to| from != to)
                .map(move |to| (from, to))
        })
        .collect();
    let mut comparisons = 0;
    let mut graphs = 0;
    for mask in 0..(1 << possible.len()) {
        let mut edges = vec![vec![]; 4];
        for (bit, &(from, to)) in possible.iter().enumerate() {
            if mask & (1 << bit) != 0 {
                edges[from].push(to);
            }
        }
        if !valid(&edges) {
            continue;
        }
        graphs += 1;
        for active in 0..4 {
            for reverse in [false, true] {
                comparisons += check(&edges, active, reverse);
            }
        }
    }
    assert_eq!(graphs, 543);
    assert_eq!(comparisons, 69504);
}

#[test]
fn cached_subtrees_respect_the_remaining_depth_in_every_active_view_and_both_orders() {
    let mut chain = vec![vec![]; 17];
    for (node, targets) in chain.iter_mut().enumerate().take(15) {
        targets.push(node + 1);
    }
    assert!(!permitted(&chain, 15, 16));
    assert!(permitted(&chain, 14, 16));

    let mut shared = vec![vec![]; 18];
    shared[0] = vec![1, 2];
    shared[1] = vec![3];
    shared[2] = vec![3];
    for (node, targets) in shared.iter_mut().enumerate().take(16).skip(3) {
        targets.push(node + 1);
    }
    assert!(!permitted(&shared, 16, 17));
    assert!(permitted(&shared, 15, 17));
    let mut comparisons = 0;
    for edges in [&chain, &shared] {
        for active in 0..edges.len() {
            for reverse in [false, true] {
                comparisons += check(edges, active, reverse);
            }
        }
    }
    assert_eq!(comparisons, 21490);
}

#[test]
fn repeated_instances_keep_exactly_the_last_available_slot_without_deduplicating_sources() {
    let mut edges = vec![vec![]; 11];
    for (node, targets) in edges.iter_mut().enumerate().take(9) {
        targets.extend([node + 1, node + 1]);
    }
    // The root contains 1023 expanded instances. A root leaf reaches 1024;
    // a leaf at the shared bottom adds 512 occurrences and must be refused.
    assert!(permitted(&edges, 0, 10));
    assert!(!permitted(&edges, 9, 10));
    let mut comparisons = 0;
    for active in 0..edges.len() {
        for reverse in [false, true] {
            comparisons += check(&edges, active, reverse);
        }
    }
    assert_eq!(comparisons, 2662);
}
