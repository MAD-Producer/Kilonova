use super::*;
use crate::node_editor::tests::simple;
use std::hint::black_box;
use std::time::Instant;

fn linear<'a>(cards: &'a [Card], port: &PortRef, output: bool) -> Option<&'a Socket> {
    cards
        .iter()
        .flat_map(|card| &card.sockets)
        .find(|socket| socket.output == output && socket.port == *port)
}

#[test]
fn direction_identity_missing_and_duplicate_ports_preserve_first_match() {
    let editor = simple();
    let mut cards = editor.cards(Rect::new(0., 0., 2000., 1600.));
    let original = cards[0].sockets[0].clone();
    let mut opposite = original.clone();
    opposite.output = !original.output;
    opposite.point = [800., 900.];
    cards[0].sockets.insert(0, opposite);
    let mut duplicate = original.clone();
    duplicate.point = [1000., 1100.];
    cards[0].sockets.push(duplicate);
    let index = SocketIndex::new(&cards);
    for socket in cards.iter().flat_map(|card| &card.sockets) {
        for output in [false, true] {
            let expected = linear(&cards, &socket.port, output);
            let actual = index.find(&socket.port, output);
            assert_eq!(actual.map(|s| s.point), expected.map(|s| s.point));
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert!(std::ptr::eq(actual, expected));
            }
        }
    }
    for missing in [
        PortRef::new("missing", "value"),
        PortRef::new("color", "missing"),
    ] {
        assert!(index.find(&missing, false).is_none());
        assert!(index.find(&missing, true).is_none());
    }
    assert_eq!(
        index.find(&original.port, original.output).unwrap().point,
        original.point
    );
    assert!(SocketIndex::new(&[])
        .curve(&Connection::new("a", "out", "b", "in"))
        .is_empty());
}

#[test]
fn fresh_geometry_keeps_painted_selected_and_cut_curves_identical() {
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let mut editor = simple();
    let canvas = Rect::new(0., 0., 2400., 1800.);
    let edge = editor.document.connections[0].clone();
    for (zoom, collapsed, position) in [
        (0.4, false, [380., 80.]),
        (1., true, [500., 160.]),
        (2.5, false, [450., 200.]),
    ] {
        editor.view.zoom = zoom;
        let target = editor
            .document
            .nodes
            .iter_mut()
            .find(|n| n.id == "output")
            .unwrap();
        target.collapsed = collapsed;
        target.position = position;
        let cards = editor.cards(canvas);
        let a = linear(&cards, &edge.from, true).unwrap();
        let b = linear(&cards, &edge.to, false).unwrap();
        let expected = curve(a.point, b.point);
        assert_eq!(wire_curve(&cards, &edge), expected);
        let point = expected[12];
        assert_eq!(editor.wire_at(&cards, canvas, point), Some(edge.to.clone()));
        let mut hits = BTreeSet::new();
        editor.collect_cut_inputs(
            &cards,
            canvas,
            [point[0], point[1] - 8.],
            [point[0], point[1] + 8.],
            &mut hits,
        );
        assert_eq!(hits, BTreeSet::from([edge.to.clone()]));
        let before = editor.snapshot();
        for language in [Language::English, Language::Chinese] {
            let mut paint = PaintList::default();
            editor.paint_canvas(
                &mut paint,
                &mut atlas,
                &cards,
                canvas,
                &Input {
                    mouse: [-100.; 2],
                    ..Default::default()
                },
                language,
            );
            assert_eq!(
                paint
                    .shapes
                    .iter()
                    .filter(|shape| matches!(shape,
                paint::Shape::Line { points, .. } if points == &expected))
                    .count(),
                1
            );
        }
        assert_eq!(editor.document, before.document);
        assert_eq!(editor.revision, before.revision);
    }
    // A stale endpoint is a normal miss, just as it was in the linear search.
    editor.document.nodes.retain(|node| node.id != "output");
    let cards = editor.cards(canvas);
    assert!(wire_curve(&cards, &edge).is_empty());
    assert!(editor.wire_at(&cards, canvas, [500., 300.]).is_none());
    let mut hits = BTreeSet::new();
    editor.collect_cut_inputs(&cards, canvas, [0., 0.], [2000., 1600.], &mut hits);
    assert!(hits.is_empty());
}

#[test]
fn coincident_connections_keep_hit_priority_and_all_cut_inputs() {
    let mut editor = simple();
    let mut duplicate = editor
        .document
        .nodes
        .iter()
        .find(|node| node.id == "output")
        .unwrap()
        .clone();
    duplicate.id = "second-output".into();
    editor.document.nodes.push(duplicate);
    editor
        .document
        .connections
        .push(Connection::new("color", "value", "second-output", "color"));
    let canvas = Rect::new(0., 0., 2000., 1600.);
    let cards = editor.cards(canvas);
    let first = wire_curve(&cards, &editor.document.connections[0]);
    let second = wire_curve(&cards, &editor.document.connections[1]);
    assert_eq!(first, second);
    let point = first[12];
    assert_eq!(
        editor.wire_at(&cards, canvas, point),
        Some(PortRef::new("second-output", "color"))
    );
    let mut hits = BTreeSet::new();
    editor.collect_cut_inputs(
        &cards,
        canvas,
        [point[0], point[1] - 8.],
        [point[0], point[1] + 8.],
        &mut hits,
    );
    assert_eq!(
        hits,
        BTreeSet::from([
            PortRef::new("output", "color"),
            PortRef::new("second-output", "color")
        ])
    );
}

#[test]
fn socket_lookup_scaling_matches_the_linear_oracle() {
    let editor = simple();
    let template = editor.cards(Rect::new(0., 0., 2000., 1600.))[0].sockets[0].clone();
    for count in [128, 512, 2048] {
        let cards: Vec<_> = (0..count)
            .map(|i| {
                let mut socket = template.clone();
                socket.port = PortRef::new(format!("node-{i}"), "value");
                socket.point = [i as f32, 2. * i as f32];
                Card {
                    id: socket.port.node.clone(),
                    rect: Rect::default(),
                    sockets: vec![socket],
                }
            })
            .collect();
        let queries: Vec<_> = cards
            .iter()
            .map(|card| card.sockets[0].port.clone())
            .collect();
        let start = Instant::now();
        let mut comparisons = 0usize;
        let expected: Vec<_> = queries
            .iter()
            .map(|port| {
                cards
                    .iter()
                    .flat_map(|card| &card.sockets)
                    .find(|socket| {
                        comparisons += 1;
                        socket.port == *port && socket.output == template.output
                    })
                    .unwrap()
                    .point
            })
            .collect();
        let linear_time = start.elapsed();
        let start = Instant::now();
        let index = SocketIndex::new(&cards);
        let actual: Vec<_> = queries
            .iter()
            .map(|port| black_box(index.find(black_box(port), template.output).unwrap().point))
            .collect();
        let indexed_time = start.elapsed();
        assert_eq!(actual, expected);
        assert_eq!(comparisons, count * (count + 1) / 2);
        println!("sockets={count} queries={count} linear_comparisons={comparisons} linear_us={} index_build_and_lookup_us={}",
            linear_time.as_micros(), indexed_time.as_micros());
    }
}
