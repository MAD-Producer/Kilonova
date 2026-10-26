//! Re-enter only isolated contexts of the *same* locked runtime. The existing
//! 64 MiB heap, 512 KiB stack, root 20 ms and frame 100 ms ceilings stay intact.
use super::super::{error, references::Session, PropertyExpression};
use super::*;
use rquickjs::context::Intrinsic;
use std::ptr::NonNull;

type Programs = Rc<HashMap<String, Rc<Vec<u8>>>>;

/// The parent Ctx proves that this thread already holds this runtime's lock.
/// HRTB prevents a child JS value/context lifetime from escaping in R. Context
/// creation and every refcount operation happen while that same lock is held.
fn isolated<R>(
    parent: &Ctx<'_>,
    run: impl for<'child> FnOnce(Ctx<'child>) -> Result<R>,
) -> Result<R> {
    // SAFETY: JS_GetRuntime obtains the parent's live runtime. A fresh context
    // owns one initial reference; Ctx::from_raw adds its own reference, after
    // which we release the initial one. All intrinsics, JS values and callbacks
    // remain under the parent's lock. No pointer or JS value is returned.
    let child = unsafe {
        let runtime = rquickjs::qjs::JS_GetRuntime(parent.as_raw().as_ptr());
        let pointer = NonNull::new(rquickjs::qjs::JS_NewContextRaw(runtime)).ok_or_else(|| {
            Error::Invalid("expression reference context allocation failed".into())
        })?;
        rquickjs::qjs::JS_AddIntrinsicBaseObjects(pointer.as_ptr());
        EvaluationIntrinsics::add_intrinsic(pointer);
        let child = Ctx::from_raw(pointer);
        rquickjs::qjs::JS_FreeContext(pointer.as_ptr());
        child
    };
    prepare_context(&child)?;
    run(child)
}

fn load<'js>(
    ctx: Ctx<'js>,
    programs: Programs,
    session: Rc<Session>,
    expression: &PropertyExpression,
    frame: f64,
) -> Result<Function<'js>> {
    let invocation = Rc::new(session.invocation(expression, frame)?);
    let operation = || -> rquickjs::Result<Function<'js>> {
        ctx.globals().set("__msData", invocation.data.as_str())?;
        let own_session = session.clone();
        let own_target = expression.target.clone();
        ctx.globals().set(
            "__msSample",
            Function::new(
                ctx.clone(),
                move |seconds: f64| -> rquickjs::Result<String> {
                    match invocation.sample(seconds) {
                        Ok(value) => Ok(value),
                        Err(cause) => {
                            own_session.record(&own_target, frame, cause);
                            Err(rquickjs::Error::new_from_js_message(
                                "sample time",
                                "property",
                                "valueAtTime track sampling failed",
                            ))
                        }
                    }
                },
            )?,
        )?;
        let dependency_programs = programs.clone();
        let dependency_session = session.clone();
        let owner = expression.target.clone();
        ctx.globals().set(
            "__msReference",
            Function::new(
                ctx.clone(),
                move |parent: Ctx<'js>,
                      descriptor: String,
                      seconds: f64|
                      -> rquickjs::Result<String> {
                    let result =
                        dependency_session.reference(&descriptor, seconds, &|expression, frame| {
                            isolated(&parent, |child| {
                                run(
                                    child,
                                    dependency_programs.clone(),
                                    dependency_session.clone(),
                                    expression,
                                    frame,
                                )
                            })
                        });
                    match result {
                        Ok(value) => Ok(value),
                        Err(cause) => {
                            dependency_session.record(&owner, frame, cause);
                            Err(rquickjs::Error::new_from_js_message(
                                "reference",
                                "property",
                                "property reference sampling failed",
                            ))
                        }
                    }
                },
            )?,
        )?;
        let code = programs.get(&expression.source).ok_or_else(|| {
            rquickjs::Error::new_from_js_message(
                "program",
                "expression",
                "referenced expression program is unavailable",
            )
        })?;
        // SAFETY: programs are compiled by this exact runtime; its cache keeps
        // all ROM_DATA allocations alive until after the runtime is dropped.
        let module = unsafe { Module::load(ctx.clone(), code) }?;
        let (module, promise) = module.eval()?;
        promise.finish::<()>()?;
        module.get("default")
    };
    operation().map_err(|e| caught(&ctx, e))
}
fn value(function: Function<'_>) -> Result<Vec<f32>> {
    let ctx = function.ctx().clone();
    let operation = || -> rquickjs::Result<Vec<f32>> {
        let result: Value = function.call(())?;
        if let Some(n) = result.as_number() {
            return Ok(vec![n as f32]);
        }
        if let Some(array) = result.as_array() {
            if !(1..=4).contains(&array.len()) {
                return Err(rquickjs::Error::new_from_js_message(
                    "Array",
                    "property",
                    "expected 1..4 components",
                ));
            }
            return (0..array.len())
                .map(|i| {
                    array
                        .get::<Value>(i)?
                        .as_number()
                        .map(|v| v as f32)
                        .ok_or_else(|| {
                            rquickjs::Error::new_from_js_message(
                                "component",
                                "number",
                                "expected numeric components",
                            )
                        })
                })
                .collect();
        }
        Err(rquickjs::Error::new_from_js_message(
            "expression",
            "property",
            "expected Number or Array",
        ))
    };
    let values = operation().map_err(|e| caught(&ctx, e))?;
    if values.iter().any(|v| !v.is_finite()) {
        return Err(Error::Invalid("expression result must be finite".into()));
    }
    Ok(values)
}
fn run(
    ctx: Ctx<'_>,
    programs: Programs,
    session: Rc<Session>,
    expression: &PropertyExpression,
    frame: f64,
) -> Result<Vec<f32>> {
    let _path = session.enter(&expression.target)?;
    let result = load(ctx, programs, session.clone(), expression, frame)
        .and_then(value)
        .and_then(|value| session.checked(expression, frame, value));
    result.map_err(|cause| error(&expression.target, frame, cause))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExpressionTarget, Layer, Project, Property, EXPRESSION_PROFILE};

    #[test]
    fn full_bytecode_cache_survives_warm_reference_evaluation_and_evicts_only_for_a_new_source() {
        super::super::ENGINE.with(|slot| *slot.borrow_mut() = None);
        for number in 0..CACHE {
            super::super::compile(&format!("{number} + value")).unwrap();
        }
        let cached = super::super::ENGINE
            .with(|slot| slot.borrow().as_ref().unwrap().code["255 + value"].clone());
        let mut project = Project::new(64, 64, 30, 60).unwrap();
        project.layers.push(Layer::solid(
            1,
            "Cache control",
            [16.; 2],
            [7., 0., 0.],
            [1.; 4],
        ));
        let expression = PropertyExpression {
            target: ExpressionTarget::Property {
                object: 1,
                property: Property::Position,
                axis: Some(crate::Axis::X),
            },
            source: "0 + value".into(),
            enabled: true,
            seed: 0,
            profile: EXPRESSION_PROFILE.into(),
        };
        project.expressions.push(expression.clone());
        assert_eq!(
            evaluate_linked(
                Rc::new(Session::new(project)),
                &expression,
                0.,
                Instant::now() + Duration::from_millis(100),
                Arc::new(AtomicBool::new(false))
            )
            .unwrap(),
            vec![7.]
        );
        super::super::ENGINE.with(|slot| {
            let slot = slot.borrow();
            let code = &slot.as_ref().unwrap().code;
            assert_eq!(code.len(), CACHE, "warm evaluation discarded a full cache");
            assert!(Rc::ptr_eq(&cached, &code["255 + value"]));
        });
        super::super::compile("value + 1000").unwrap();
        super::super::ENGINE.with(|slot| {
            let slot = slot.borrow();
            let code = &slot.as_ref().unwrap().code;
            assert_eq!(
                code.len(),
                1,
                "a new source must respect the fixed cache bound"
            );
            assert!(code.contains_key("value + 1000"));
        });
    }
}

pub(in crate::expressions) fn evaluate_linked(
    session: Rc<Session>,
    expression: &PropertyExpression,
    frame: f64,
    frame_deadline: Instant,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<f32>> {
    if cancel.load(Ordering::Relaxed) || Instant::now() >= frame_deadline {
        return Err(error(
            &expression.target,
            frame,
            Error::Invalid("expression cancelled or frame budget exceeded".into()),
        ));
    }
    let result = with_engine(|engine| {
        let sources: std::collections::HashSet<_> = session
            .project
            .expressions
            .iter()
            .filter(|e| motion_model::expressions::active(&session.project, e))
            .map(|e| e.source.as_str())
            .chain(std::iter::once(expression.source.as_str()))
            .collect();
        let missing = sources
            .iter()
            .filter(|source| !engine.code.contains_key(**source))
            .count();
        if engine.code.len() + missing > CACHE {
            *engine = JsEngine::new()?;
        }
        for source in sources {
            engine.compile(source)?;
        }
        let programs = Rc::new(engine.code.clone());
        let expired = session.interrupted.clone();
        let execution_deadline = Arc::new(OnceLock::<Instant>::new());
        let clock = execution_deadline.clone();
        let mark = expired.clone();
        let cancellation = cancel.clone();
        engine.runtime.set_interrupt_handler(Some(Box::new(move || {
            let now = Instant::now();
            let stop = cancellation.load(Ordering::Relaxed)
                || now >= frame_deadline
                || clock.get().is_some_and(|deadline| now >= *deadline);
            if stop {
                mark.store(true, Ordering::Relaxed);
            }
            stop
        })));
        let result = (|| {
            let context = evaluation_context(&engine.runtime)?;
            context.with(|ctx| {
                let _path = session.enter(&expression.target)?;
                let function = load(ctx, programs, session.clone(), expression, frame)?;
                let deadline = frame_deadline.min(Instant::now() + Duration::from_millis(20));
                if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
                    expired.store(true, Ordering::Relaxed);
                    return Err(Error::Invalid(
                        "expression cancelled or execution budget exceeded".into(),
                    ));
                }
                execution_deadline.set(deadline).map_err(|_| {
                    Error::Invalid("execution deadline was already initialized".into())
                })?;
                value(function).and_then(|values| session.checked(expression, frame, values))
            })
        })();
        engine.runtime.set_interrupt_handler(None);
        engine.runtime.run_gc();
        if let Some(cause) = session.take_failure() {
            return Err(cause);
        }
        if expired.load(Ordering::Relaxed) {
            return Err(Error::Invalid(
                "expression cancelled or execution budget exceeded".into(),
            ));
        }
        result
    });
    result.map_err(|cause| error(&expression.target, frame, cause))
}
