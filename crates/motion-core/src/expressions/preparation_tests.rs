use super::*;
use crate::{Layer, Property, EXPRESSION_PROFILE};
use std::{sync::mpsc, thread};

fn project(source: &str) -> Project {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.layers.push(Layer::solid(
        1, "Compilation boundary", [16.; 2], [1., 2., 3.], [1.; 4],
    ));
    project.expressions.push(PropertyExpression {
        target: ExpressionTarget::Property {
            object: 1,
            property: Property::Position,
            axis: None,
        },
        source: source.into(),
        enabled: true,
        seed: 0,
        profile: EXPRESSION_PROFILE.into(),
    });
    project
}

#[test]
fn cold_project_compilation_finishes_before_the_frame_clock_starts() {
    let (entered, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let worker = thread::spawn(move || {
        runtime::pause_next_compilation(entered, held);
        let project = project("value + [time, 0, 0]");
        let evaluated = project.evaluated_at(7.25)?;
        evaluated.layers[0].transform.position.sample(7.25)
    });
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    // Hold actual cold compilation beyond the unchanged 100 ms frame budget.
    // No expression executes while this controlled initialization gate is held.
    thread::sleep(Duration::from_millis(120));
    release.send(()).unwrap();
    let sampled = worker.join().unwrap().unwrap();
    assert!((sampled[0] - (1. + 7.25 / 30.)).abs() < 1e-6);
    assert_eq!([sampled[1], sampled[2]], [2., 3.]);
}

#[test]
fn cancelled_project_does_not_begin_cold_compilation() {
    let (entered, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let worker = thread::spawn(move || {
        runtime::pause_next_compilation(entered, held);
        project("value + [time, 0, 0]")
            .evaluated_at_cancellable(7.25, Arc::new(AtomicBool::new(true)))
            .map(|_| ())
    });
    let admission = observed.recv_timeout(Duration::from_secs(5));
    let _ = release.send(());
    let error = worker.join().unwrap().unwrap_err();
    assert!(matches!(admission, Err(mpsc::RecvTimeoutError::Disconnected)));
    assert!(matches!(error, Error::Expression { frame: 7.25, .. }));
    assert!(error.to_string().contains("cancelled"));
}

#[test]
fn cold_source_errors_keep_the_requested_frame_and_target() {
    let error = project("unfinished (").evaluated_at(7.25).unwrap_err();
    let Error::Expression { target, frame, source } = error else {
        panic!("expected a contextual expression error");
    };
    assert_eq!(frame, 7.25);
    assert!(target.contains("position") && target.contains("1"));
    assert!(source.to_string().contains("expression") || source.to_string().contains("JavaScript"));
}
