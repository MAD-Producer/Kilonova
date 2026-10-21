use super::*;

fn empty() -> SampledPath {
    SampledPath {
        closed: false,
        nodes: vec![],
    }
}

fn invalid<T>(result: Result<T>, context: &str) {
    match result {
        Err(crate::Error::Invalid(message)) => assert!(message.contains(context), "{message}"),
        Err(error) => panic!("unexpected model failure: {error:?}"),
        Ok(_) => panic!("broken path invariant was accepted: {context}"),
    }
}

#[test]
fn empty_join_pieces_return_phase_errors_instead_of_panicking() {
    let node = SampledPath {
        closed: false,
        nodes: vec![[0.; 6]],
    };
    invalid(join(empty(), node.clone()), "phase=join empty left piece");
    invalid(join(node, empty()), "phase=join empty right piece");
}

#[test]
fn malformed_measure_tables_propagate_through_partial_cuts() {
    for table in [vec![], vec![(0., 0.)]] {
        let segment = Segment {
            curve: [V::ZERO, V::new(1., 0.), V::new(2., 0.), V::new(3., 0.)],
            line: true,
            start: 0.,
            length: 3.,
            table,
        };
        invalid(parameter(&segment, 1.), "phase=parameter");
        let measured = Measured {
            path: SampledPath {
                closed: false,
                nodes: vec![[0.; 6], [3., 0., 0., 0., 0., 0.]],
            },
            segments: vec![segment],
            length: 3.,
        };
        invalid(cut(&measured, 0., 1.), "phase=parameter");
    }
}
