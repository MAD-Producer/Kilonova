use motion_core::{
    Axis, Command, Ease, Engine, ExpressionTarget, Layer, Project, ProjectExpressions, Property,
    PropertyExpression, EXPRESSION_PROFILE,
};

fn target() -> ExpressionTarget {
    ExpressionTarget::Property {
        object: 1,
        property: Property::Position,
        axis: Some(Axis::X),
    }
}

fn fixture(width: u32, start: f32, end: f32) -> Project {
    let mut project = Project::new(width, 100, 30, 180).unwrap();
    let mut layer = Layer::solid(1, "Card", [20.; 2], [start, 0., 0.], [1.; 4]);
    layer
        .transform
        .position
        .upsert(0, [start, 0., 0.], Ease::Linear)
        .unwrap();
    layer
        .transform
        .position
        .upsert(30, [end, 0., 0.], Ease::Linear)
        .unwrap();
    project.layers.push(layer);
    project
}

fn command(source: &str) -> Command {
    Command::SetExpression {
        expression: PropertyExpression {
            target: target(),
            source: source.into(),
            enabled: true,
            seed: 42,
            profile: EXPRESSION_PROFILE.into(),
        },
        frame: 0,
    }
}

fn project(source: &str, width: u32, start: f32, end: f32) -> Project {
    let mut engine = Engine::new(fixture(width, start, end)).unwrap();
    engine.apply(command(source)).unwrap();
    engine.snapshot()
}

fn x(project: &Project, frame: f64) -> f32 {
    project.evaluated_at(frame).unwrap().layers[0]
        .transform
        .position
        .value[0]
}

#[test]
fn clocks_async_and_dynamic_compilation_globals_are_unavailable() {
    let mut reachable = Vec::new();
    for name in [
        "Date",
        "Promise",
        "Atomics",
        "SharedArrayBuffer",
        "performance",
        "eval",
        "Function",
        "fetch",
        "XMLHttpRequest",
        "WebSocket",
        "process",
        "require",
        "std",
        "os",
        "setTimeout",
        "setInterval",
        "queueMicrotask",
    ] {
        let source = format!("typeof globalThis['{name}'] === 'undefined' ? 1 : 0");
        let p = project(&source, 100, 10., 40.);
        if x(&p, 0.) != 1. {
            reachable.push(name);
        }
    }
    assert!(
        reachable.is_empty(),
        "reachable forbidden globals: {reachable:?}"
    );
}

#[test]
fn direct_indirect_and_constructor_compilation_are_rejected_atomically() {
    let mut accepted = Vec::new();
    for source in [
        "eval('41')",
        "(0, eval)('41')",
        "globalThis['ev'+'al']('41')",
        "Function('return 41')()",
        "(function(){}).constructor('return 41')()",
        "(()=>1).constructor('return 41')()",
        "[].map.constructor('return 41')()",
        "Object.getPrototypeOf(function(){}).constructor('return 41')()",
        "(function*(){}).constructor('yield 41')().next().value",
        "Reflect.construct((()=>1).constructor, ['return 41'])()",
        "Reflect.apply((()=>1).constructor, null, ['return 41'])()",
        "new Proxy((()=>1).constructor, {})('return 41')()",
        "(()=>1).constructor.bind(null, 'return 41')()()",
        "(()=>1).constructor('return (async()=>41)().constructor')().resolve(41); 41",
    ] {
        let mut engine = Engine::new(fixture(100, 10., 40.)).unwrap();
        let before = engine.snapshot();
        match engine.apply(command(source)) {
            Ok(_) => accepted.push(source),
            Err(error) => {
                assert!(
                    error.to_string().contains("JavaScript"),
                    "{source}: {error}"
                );
                assert_eq!(engine.snapshot(), before, "{source}");
                assert!(!engine.can_undo(), "rejection added history: {source}");
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "dynamic compilation accepted: {accepted:?}"
    );
}

#[test]
fn dynamic_import_is_rejected_before_it_can_create_async_work() {
    let mut engine = Engine::new(fixture(100, 10., 40.)).unwrap();
    let before = engine.snapshot();
    let error = engine
        .apply(command("var pending = import('sandbox:unavailable'); 41"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("asynchronous JavaScript"), "{error}");
    assert_eq!(engine.snapshot(), before);
    assert!(!engine.can_undo());
}

#[test]
fn deferred_compilation_failure_preserves_target_frame_animation_and_recovery() {
    let p = project(
        "time < 1 ? value : (()=>1).constructor('return 41')()",
        100,
        10.,
        40.,
    );
    let before = p.clone();
    assert_eq!(x(&p, 0.), 10.);
    let result = p.evaluated_at(30.);
    assert!(
        result.is_err(),
        "later-frame dynamic compilation was accepted"
    );
    match result.unwrap_err() {
        motion_core::Error::Expression {
            target: metadata,
            frame,
            message,
        } => {
            assert_eq!(frame, 30.);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&metadata).unwrap(),
                serde_json::to_value(target()).unwrap()
            );
            assert!(message.contains("JavaScript"), "{message}");
        }
        other => panic!("missing expression failure context: {other}"),
    }
    assert_eq!(p, before);
    assert_eq!(x(&p, 0.), 10.);
}

#[test]
fn ordinary_javascript_and_base64_helpers_still_execute() {
    for (source, expected) in [
        (
            "function square(v){return v*v;} [1,2,3].map(square).reduce((a,b)=>a+b,0)",
            14.,
        ),
        (
            "function* values(){yield 3;yield 5;} Array.from(values()).reduce((a,b)=>a+b,0)",
            8.,
        ),
        (
            "JSON.parse('{\"x\":7}').x + new Map([['a',3]]).get('a')",
            10.,
        ),
        ("new Set([1,2,2]).size + new Uint8Array([3,4])[1]", 6.),
        (
            "new RegExp('^card$', 'i').test('CARD') && /\\d+/.test('42') ? 1 : 0",
            1.,
        ),
        ("atob(btoa('Card')).length", 4.),
    ] {
        let p = project(source, 100, 10., 40.);
        assert_eq!(x(&p, 17.), expected, "{source}");
    }
}

const SHARED_SOURCE: &str = "var contaminated = [globalThis.sandboxMark, Object.prototype.sandboxMark, Array.prototype.sandboxMark, Math.sandboxMark].some(v=>v!==undefined); globalThis.sandboxMark=thisComp.width; Object.prototype.sandboxMark=1; Array.prototype.sandboxMark=2; Math.sandboxMark=3; contaminated ? -999 : valueAtTime(0.5)+thisComp.width";

#[test]
fn cached_source_keeps_projects_globals_prototypes_and_sample_callbacks_independent() {
    // Identical source, object IDs and target on the same thread intentionally share bytecode.
    let a = project(SHARED_SOURCE, 100, 10., 40.);
    let b = project(SHARED_SOURCE, 200, 100., 160.);
    let original_a = a.clone();
    let original_b = b.clone();
    for frame in [0., 59., 17., 2., 59.] {
        assert_eq!(x(&a, frame), 125.);
        assert_eq!(x(&b, frame), 330.);
        assert_eq!(x(&a, frame), 125.);
    }
    assert_eq!(a, original_a);
    assert_eq!(b, original_b);
}

#[test]
fn failed_and_interrupted_projects_cannot_poison_the_next_evaluation() {
    let healthy = project(SHARED_SOURCE, 200, 100., 160.);
    for source in [
        "globalThis.sandboxMark=7; Object.prototype.sandboxMark=8; throw Error('controlled failure');",
        "globalThis.sandboxMark=7; Array.prototype.sandboxMark=8; while(true){}",
    ] {
        let mut engine = Engine::new(fixture(100, 10., 40.)).unwrap();
        let before = engine.snapshot();
        assert!(engine.apply(command(source)).is_err(), "{source}");
        assert_eq!(engine.snapshot(), before);
        assert!(!engine.can_undo());
        assert_eq!(x(&healthy, 17.), 330.);
    }
}
