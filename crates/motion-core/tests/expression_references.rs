use motion_core::{
    expression_reference_source as reference, Axis, Command, Ease, Engine, ExpressionTarget, Layer,
    LayerTimeline, Project, ProjectExpressions, Property, PropertyExpression, EXPRESSION_PROFILE,
};

fn target(object: u64, property: Property, axis: Option<Axis>) -> ExpressionTarget {
    ExpressionTarget::Property {
        object,
        property,
        axis,
    }
}
fn expression(target: ExpressionTarget, source: &str) -> PropertyExpression {
    PropertyExpression {
        target,
        source: source.into(),
        enabled: true,
        seed: 42,
        profile: EXPRESSION_PROFILE.into(),
    }
}
fn set(engine: &mut Engine, target: ExpressionTarget, source: &str) -> motion_core::Result<()> {
    engine.apply(Command::SetExpression {
        expression: expression(target, source),
        frame: 0,
    })
}
fn fixture() -> Project {
    let mut project = Project::new(100, 100, 30, 180).unwrap();
    for (id, x) in [(1, 10.), (2, 20.), (3, 30.)] {
        project.layers.push(Layer::solid(
            id,
            &format!("Card {id}"),
            [20.; 2],
            [x, 5., 0.],
            [1.; 4],
        ));
    }
    project
}
fn x(project: &Project, object: u64, frame: f64) -> f32 {
    project
        .evaluated_at(frame)
        .unwrap()
        .layers
        .iter()
        .find(|l| l.id == object)
        .unwrap()
        .transform
        .position
        .value[0]
}

#[test]
fn full_precision_ids_rename_reorder_save_reopen_and_one_step_history_keep_the_same_target() {
    let large = u64::MAX;
    let mut project = fixture();
    project.layers[1].id = large;
    let mut engine = Engine::new(project).unwrap();
    let original = engine.snapshot();
    let source = reference(&target(large, Property::Position, Some(Axis::X))).unwrap();
    set(
        &mut engine,
        target(1, Property::Position, Some(Axis::X)),
        &format!("{source} + 7"),
    )
    .unwrap();
    let applied = engine.snapshot();
    assert_eq!(x(&applied, 1, 0.), 27.);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.snapshot(), original);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.snapshot(), applied);
    let mut changed = applied;
    changed
        .layers
        .iter_mut()
        .find(|l| l.id == large)
        .unwrap()
        .name = "重命名\" source".into();
    changed.layers.reverse();
    assert_eq!(x(&changed, 1, 17.), 27.);
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), &changed).unwrap();
    let reopened = motion_core::storage::load(root.path()).unwrap();
    assert_eq!(x(&reopened, 1, 17.), 27.);
}

#[test]
fn linked_expressions_sample_composition_seconds_and_each_layers_offset() {
    let mut project = fixture();
    project.layers[1].timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 180,
        offset_frame: -30,
    });
    project.layers[1]
        .transform
        .position
        .upsert(30, [0., 5., 0.], Ease::Linear)
        .unwrap();
    project.layers[1]
        .transform
        .position
        .upsert(60, [30., 5., 0.], Ease::Linear)
        .unwrap();
    let mut engine = Engine::new(project).unwrap();
    set(
        &mut engine,
        target(2, Property::Position, Some(Axis::X)),
        "value * 2 + time",
    )
    .unwrap();
    let source = reference(&target(2, Property::Position, Some(Axis::X))).unwrap();
    set(
        &mut engine,
        target(1, Property::Position, Some(Axis::X)),
        &format!("{source} + 3"),
    )
    .unwrap();
    assert_eq!(x(engine.project(), 1, 15.), 33.5);
    let source = source.strip_suffix(')').unwrap();
    set(
        &mut engine,
        target(1, Property::Position, Some(Axis::X)),
        &format!("{source}, time - 1) + 3"),
    )
    .unwrap();
    assert_eq!(x(engine.project(), 1, 45.), 33.5);
    assert_eq!(x(engine.project(), 1, 0.), 2.);
}

#[test]
fn whole_vectors_separated_axes_and_opacity_use_evaluated_property_units() {
    let mut engine = Engine::new(fixture()).unwrap();
    set(
        &mut engine,
        target(2, Property::Position, Some(Axis::X)),
        "value + 10",
    )
    .unwrap();
    let vector = reference(&target(2, Property::Position, None)).unwrap();
    set(
        &mut engine,
        target(1, Property::Position, None),
        &format!("{vector} + [1,2,3]"),
    )
    .unwrap();
    let p = engine.project().evaluated_at(0.).unwrap();
    assert_eq!(p.layers[0].transform.position.value, [31., 7., 3.]);
    let mut whole = Engine::new(fixture()).unwrap();
    set(
        &mut whole,
        target(2, Property::Position, None),
        "value + [10,20,30]",
    )
    .unwrap();
    let axis = reference(&target(2, Property::Position, Some(Axis::Y))).unwrap();
    set(
        &mut whole,
        target(1, Property::Position, Some(Axis::X)),
        &axis,
    )
    .unwrap();
    assert_eq!(x(whole.project(), 1, 0.), 25.);
    set(&mut whole, target(2, Property::Opacity, None), "250").unwrap();
    let opacity = reference(&target(2, Property::Opacity, None)).unwrap();
    set(
        &mut whole,
        target(1, Property::Position, Some(Axis::X)),
        &opacity,
    )
    .unwrap();
    assert_eq!(x(whole.project(), 1, 0.), 100.);
}

#[test]
fn self_temporal_and_transitive_cycles_reject_the_candidate_and_keep_history_and_original_expression(
) {
    let mut engine = Engine::new(fixture()).unwrap();
    let a = target(1, Property::Position, Some(Axis::X));
    let b = target(2, Property::Position, Some(Axis::X));
    let c = target(3, Property::Position, Some(Axis::X));
    set(&mut engine, a.clone(), &reference(&b).unwrap()).unwrap();
    set(&mut engine, b.clone(), &reference(&c).unwrap()).unwrap();
    set(&mut engine, c.clone(), "value + 5").unwrap();
    let before = engine.snapshot();
    let history = engine.history_diagnostics();
    for source in [
        reference(&a).unwrap(),
        reference(&c).unwrap(),
        format!(
            "{}, time - 0.1)",
            reference(&c).unwrap().strip_suffix(')').unwrap()
        ),
    ] {
        let error = set(&mut engine, c.clone(), &source).unwrap_err();
        assert!(error.to_string().contains("dependency cycle"), "{error}");
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.history_diagnostics(), history);
        assert_eq!(x(engine.project(), 1, 10.), 35.);
    }
}

#[test]
fn missing_references_and_invalid_times_remain_errors_even_when_javascript_catches_them() {
    let mut engine = Engine::new(fixture()).unwrap();
    let a = target(1, Property::Position, Some(Axis::X));
    set(&mut engine, a.clone(), "value + 1").unwrap();
    let before = engine.snapshot();
    let history = engine.history_diagnostics();
    for source in [
        reference(&target(999, Property::Position, Some(Axis::X))).unwrap(),
        format!(
            "try {{ {}; }} catch (e) {{}} 7",
            reference(&target(999, Property::Position, Some(Axis::X))).unwrap()
        ),
        "try { globalThis['__msReference']('{}', NaN); } catch (e) {} 7".into(),
    ] {
        assert!(set(&mut engine, a.clone(), &source).is_err(), "{source}");
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.history_diagnostics(), history);
    }
    let healthy = reference(&target(2, Property::Position, Some(Axis::X))).unwrap();
    set(&mut engine, a, &healthy).unwrap();
    assert_eq!(x(engine.project(), 1, 0.), 20.);
}

#[test]
fn dependencies_keep_their_own_seed_and_isolate_globals_and_prototypes() {
    let mut engine = Engine::new(fixture()).unwrap();
    let dependency = target(2, Property::Position, Some(Axis::X));
    set(&mut engine, dependency.clone(), "var dirty = globalThis.mark || Object.prototype.mark; globalThis.mark=1; Object.prototype.mark=1; dirty ? -999 : random(10,20)").unwrap();
    let before = x(engine.project(), 2, 17.);
    let reference = reference(&dependency).unwrap();
    let root = target(1, Property::Position, Some(Axis::X));
    set(
        &mut engine,
        root,
        &format!("globalThis.mark=8; Object.prototype.mark=8; {reference} + {reference}"),
    )
    .unwrap();
    for frame in [0., 17., 29., 17.] {
        let evaluated = engine.project().evaluated_at(frame).unwrap();
        assert_eq!(
            evaluated.layers[0].transform.position.value[0],
            evaluated.layers[1].transform.position.value[0] * 2.
        );
    }
    assert_eq!(x(engine.project(), 2, 17.), before);
}

#[test]
fn effect_references_keep_instance_ids_units_and_disabled_effect_semantics() {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "tint")
        .unwrap();
    let mut project = fixture();
    let effect = motion_core::EffectInstance::new(
        17,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [100.; 2],
    );
    let (parameter, value) = effect
        .params
        .iter()
        .find_map(|(name, parameter)| {
            (parameter.kind.dimensions() == 1
                && !parameter.kind.discrete()
                && parameter.animatable
                && parameter.implemented
                && parameter.min <= 25.
                && parameter.max >= 25.)
                .then(|| (name.clone(), parameter.track.value[0]))
        })
        .expect("tint must expose a continuous scalar spanning 25");
    project.layers[1].effects.push(effect);
    project.rebuild_plugin_dependencies();
    let mut engine = Engine::new(project).unwrap();
    let source = ExpressionTarget::Effect {
        object: 2,
        effect: 17,
        param: parameter,
    };
    set(&mut engine, source.clone(), "25").unwrap();
    set(
        &mut engine,
        target(1, Property::Position, Some(Axis::X)),
        &reference(&source).unwrap(),
    )
    .unwrap();
    assert_eq!(x(engine.project(), 1, 10.), 25.);
    let mut disabled = engine.snapshot();
    disabled.layers[1].effects[0].enabled = false;
    assert_eq!(x(&disabled, 1, 10.), value);
    disabled.layers[1].effects[0].id = 18;
    let error = disabled.evaluated_at(10.).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("effect parameter does not exist"),
        "{error}"
    );
    assert_eq!(x(engine.project(), 1, 10.), 25.);
}

#[test]
fn deferred_missing_targets_and_timed_out_dependencies_preserve_the_model_and_recover() {
    let mut engine = Engine::new(fixture()).unwrap();
    let a = target(1, Property::Position, Some(Axis::X));
    let b = target(2, Property::Position, Some(Axis::X));
    let missing = reference(&target(999, Property::Position, Some(Axis::X))).unwrap();
    set(
        &mut engine,
        a.clone(),
        &format!("time < 1 ? value : {missing}"),
    )
    .unwrap();
    let before = engine.snapshot();
    assert!(before
        .evaluated_at(30.)
        .unwrap_err()
        .to_string()
        .contains("999"));
    assert_eq!(x(&before, 1, 0.), 10.);
    assert_eq!(engine.snapshot(), before);
    set(
        &mut engine,
        b.clone(),
        "if (time < 1) { value; } else { while (true) {} }",
    )
    .unwrap();
    set(&mut engine, a, &reference(&b).unwrap()).unwrap();
    let before = engine.snapshot();
    let history = engine.history_diagnostics();
    let error = before.evaluated_at(30.).unwrap_err();
    assert!(error.to_string().contains("budget exceeded"), "{error}");
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.history_diagnostics(), history);
    assert_eq!(x(&before, 1, 0.), 20.);
}
