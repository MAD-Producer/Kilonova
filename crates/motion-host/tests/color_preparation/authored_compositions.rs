//! Composition activation publishes a fully prepared managed candidate or none.
use super::*;
use motion_core::{Composition, ExpressionTarget, Property, PropertyExpression};
use motion_host::Session;
use serde_json::{json, Value};

pub(super) fn body(project: &Project, id: &str) -> Composition {
    Composition {
        id: id.into(),
        name: id.into(),
        width: project.width,
        height: project.height,
        fps: project.fps,
        frames: project.frames,
        work_area: project.work_area,
        guides: project.guides.clone(),
        background: project.background,
        camera: project.camera.clone(),
        layers: project.layers.clone(),
        expressions: project.expressions.clone(),
    }
}
pub(super) fn open(session: &mut Session, id: &str) -> motion_host::Result<Value> {
    session.composition_request(json!({"version":1,"composition":id,"op":"open"}))
}
pub(super) fn state(session: &Session) -> (Value, Option<ColorImageContract>) {
    let contexts: std::collections::BTreeMap<_, _> = session
        .composition_contexts
        .iter()
        .map(|(id, context)| {
            (
                id,
                json!({"frame":context.frame,"selection":context.selection,
            "timeline":context.timeline,"path":context.path}),
            )
        })
        .collect();
    (
        json!({"project":session.engine.project(),"contexts":contexts,"frame":session.frame,
        "revision":session.engine.revision(),"viewRevision":session.view_revision,
        "document":session.document_generation,"surfaceEpoch":session.surface_epoch,
        "sceneFrame":session.scene.frame,
        "observing":session.observing,"lastPresented":session.last_presented_frame,
        "lastError":session.last_error}),
        session.scene.color_contract(),
    )
}

#[test]
#[ignore = "requires the pinned native OCIO runtime"]
fn managed_composition_activation_reuses_verified_bytes_and_preserves_failed_contexts() {
    let reference = reference();
    let root = tempfile::tempdir().unwrap();
    let case = &reference.cases[0];
    let mut project = document(&reference, 0, AlphaMode::Straight, root.path());
    project.layers.push(Layer::solid(
        1,
        "main",
        [64.; 2],
        [32., 32., 0.],
        case.input[3],
    ));
    let main = project.composition_id.clone();
    let mut second = body(&Project::new(32, 48, 24, 10).unwrap(), "comp-second");
    second.layers.push(Layer::solid(
        2,
        "comp-second",
        [32., 48.],
        [16., 24., 0.],
        case.input[4],
    ));
    let mut unsupported = second.clone();
    unsupported.id = "comp-unsupported".into();
    unsupported.layers[0].three_d = true;
    let mut failing = second.clone();
    failing.id = "comp-expression".into();
    failing.expressions.push(PropertyExpression {
        target: ExpressionTarget::Property {
            object: 2,
            property: Property::Opacity,
            axis: None,
        },
        source: "throw new Error('composition-activation-negative-control')".into(),
        enabled: true,
        seed: 0,
        profile: motion_core::EXPRESSION_PROFILE.into(),
    });
    project.compositions.extend([second, unsupported, failing]);
    project.validate().unwrap();
    let mut session = Session::new(
        project.clone(),
        root.path().to_owned(),
        Arc::new(session_installation::NativePlatform(runtime())),
    )
    .unwrap();
    session.frame = 7.5;
    session.sample().unwrap();
    session
        .composition_contexts
        .entry(main.clone())
        .or_default()
        .selection = vec![1];
    session
        .composition_contexts
        .get_mut(&main)
        .unwrap()
        .timeline = json!({"zoom":2});
    session
        .composition_contexts
        .entry("comp-second".into())
        .or_default()
        .frame = 45.;
    session
        .composition_contexts
        .get_mut("comp-second")
        .unwrap()
        .selection = vec![2, 999];
    let history = session.engine.history_diagnostics();
    let revision = session.engine.revision();
    let generation = session.document_generation;
    let display = session.color_programs().unwrap().display().clone();
    // No runtime/config reopening is permitted while switching an installed document.
    std::fs::remove_file(
        root.path()
            .join(&project.color_management.resources().unwrap()[0].path),
    )
    .unwrap();
    open(&mut session, "comp-second").unwrap();
    assert_eq!(
        (
            session.engine.project().width,
            session.engine.project().height
        ),
        (32, 48)
    );
    assert_eq!(session.frame, 9.);
    assert_eq!(session.scene.frame, 9.);
    assert_eq!(
        session.composition_context_snapshot()["selection"],
        json!([2])
    );
    assert!(Arc::ptr_eq(
        &display,
        session.color_programs().unwrap().display()
    ));
    assert!(close(session.scene.layers[0].color, case.expected[4]));
    session.frame = 3.25;
    session.sample().unwrap();
    open(&mut session, &main).unwrap();
    assert_eq!(session.frame, 7.5);
    assert_eq!(
        session.composition_context_snapshot()["timeline"],
        json!({"zoom":2})
    );
    assert_eq!(
        session.composition_context_snapshot()["selection"],
        json!([1])
    );
    assert!(close(session.scene.layers[0].color, case.expected[3]));
    open(&mut session, &main).unwrap();
    assert_eq!(
        session.frame, 7.5,
        "reopening current composition retains its live frame"
    );
    for id in ["comp-unsupported", "comp-expression", "comp-missing"] {
        let before = state(&session);
        let scene = session.scene.clone();
        let error = open(&mut session, id).unwrap_err();
        let mut causes = vec![error.to_string()];
        let mut source = error.source();
        while let Some(cause) = source {
            causes.push(cause.to_string());
            source = cause.source();
        }
        if id == "comp-expression" {
            assert!(
                causes
                    .iter()
                    .any(|s| s.contains("composition-activation-negative-control")),
                "{causes:?}"
            );
        }
        if id == "comp-unsupported" {
            assert!(
                matches!(
                    error,
                    motion_host::Error::ColorPreparation(prep::Error::Composition { .. })
                ),
                "{error:?}"
            );
        }
        assert_eq!(state(&session), before, "failed target={id}");
        assert_eq!(session.scene.layers[0].color, scene.layers[0].color);
        assert_eq!(session.engine.history_diagnostics(), history);
    }
    open(&mut session, "comp-second").unwrap();
    assert_eq!(session.frame, 3.25);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.engine.history_diagnostics(), history);
    assert_eq!(session.document_generation, generation);
    let mut legacy = project;
    legacy.color_management = Arc::new(ProjectColorManagement::Unmanaged {});
    let mut legacy = Session::new(
        legacy,
        root.path().to_owned(),
        Arc::new(session_installation::NativePlatform(runtime())),
    )
    .unwrap();
    legacy.frame = 7.5;
    legacy.sample().unwrap();
    open(&mut legacy, "comp-second").unwrap();
    assert_eq!(
        (
            legacy.engine.project().width,
            legacy.engine.project().height
        ),
        (32, 48)
    );
    assert!(legacy.scene.color_contract().is_none());
    assert!(legacy.color_programs().is_none());
    open(&mut legacy, &main).unwrap();
    assert_eq!(legacy.frame, 7.5);
    println!("managed composition activation: independent CPU colors=3 dimensions=2 context_restore=passed config_absent=passed failed_targets=3 history_unchanged=true");
}
