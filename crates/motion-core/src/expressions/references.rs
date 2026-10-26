//! Stable, independently authored property references. Object and effect IDs
//! cross JavaScript as decimal strings, never lossy IEEE-754 numbers.
use super::*;
use motion_model::expressions::RawTrack;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const MAX_DESCRIPTOR_BYTES: usize = 2048;

/// Generate an editable reference at composition time (`time`, in seconds).
/// IDs are quoted decimal strings so a rename/reorder or JS Number rounding
/// cannot change the property being read. This does not mutate the project.
pub fn expression_reference_source(target: &ExpressionTarget) -> Result<String> {
    let mut descriptor = serde_json::to_value(target)?;
    let fields = descriptor
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("expression reference target must be an object".into()))?;
    fields.insert("object".into(), Value::String(target.object().to_string()));
    if let ExpressionTarget::Effect { effect, .. } = target {
        fields.insert("effect".into(), Value::String(effect.to_string()));
    }
    let descriptor = serde_json::to_string(&descriptor)?;
    ensure(
        descriptor.len() <= MAX_DESCRIPTOR_BYTES,
        "expression reference descriptor is too large",
    )?;
    Ok(format!("propertyValue({descriptor})"))
}

fn parse_descriptor(text: &str) -> Result<ExpressionTarget> {
    ensure(
        text.len() <= MAX_DESCRIPTOR_BYTES,
        "expression reference descriptor is too large",
    )?;
    let mut descriptor: Value = serde_json::from_str(text)?;
    let fields = descriptor.as_object_mut().ok_or_else(|| {
        Error::Invalid("expression reference descriptor must be an object".into())
    })?;
    for field in ["object", "effect"] {
        if field == "effect" && fields.get("kind").and_then(Value::as_str) != Some("effect") {
            continue;
        }
        let text = fields.get(field).and_then(Value::as_str).ok_or_else(|| {
            Error::Invalid(format!(
                "expression reference {field} must be a decimal ID string"
            ))
        })?;
        ensure(
            !text.is_empty()
                && text.bytes().all(|b| b.is_ascii_digit())
                && (text == "0" || !text.starts_with('0')),
            "expression reference ID is not canonical decimal",
        )?;
        let id: u64 = text.parse().map_err(|_| {
            Error::Invalid(format!(
                "expression reference {field} is outside the u64 ID range"
            ))
        })?;
        fields.insert(field.into(), Value::from(id));
    }
    Ok(serde_json::from_value(descriptor)?)
}

pub(super) struct Invocation {
    pub(super) data: String,
    pub(super) track: RawTrack,
    pub(super) fps: f64,
    pub(super) offset: f64,
    pub(super) percent: bool,
}
impl Invocation {
    pub(super) fn sample(&self, seconds: f64) -> Result<String> {
        ensure(
            seconds.is_finite(),
            "invalid sample time: seconds must be finite",
        )?;
        let frame = seconds * self.fps - self.offset;
        ensure(
            frame.is_finite(),
            "invalid sample time: local frame must be finite",
        )?;
        js_sample(self.track.sample(frame)?, self.percent)
    }
}
fn js_sample(mut values: Vec<f32>, percent: bool) -> Result<String> {
    if percent {
        values.iter_mut().for_each(|v| *v *= 100.);
    }
    Ok(if values.len() == 1 {
        serde_json::to_string(&values[0])?
    } else {
        serde_json::to_string(&values)?
    })
}
fn percent(target: &ExpressionTarget) -> bool {
    matches!(
        target,
        ExpressionTarget::Property {
            property: Property::Opacity,
            ..
        }
    )
}

// Both limits apply to a single frame/session, reset when that session is
// dropped; time-shifted recursion is still a dependency cycle. No worker restarts.
const MAX_REFERENCE_DEPTH: usize = 16;
const MAX_REFERENCE_CALLS: usize = 1024;
pub(super) struct Session {
    pub(super) project: Project,
    path: RefCell<Vec<ExpressionTarget>>,
    calls: Cell<usize>,
    failure: RefCell<Option<Error>>,
    pub(super) interrupted: Arc<AtomicBool>,
}
pub(super) struct PathGuard(Rc<Session>);
impl Drop for PathGuard {
    fn drop(&mut self) {
        self.0.path.borrow_mut().pop();
    }
}
impl Session {
    pub(super) fn new(project: Project) -> Self {
        Self {
            project,
            path: RefCell::new(Vec::new()),
            calls: Cell::new(0),
            failure: RefCell::new(None),
            interrupted: Arc::new(AtomicBool::new(false)),
        }
    }
    pub(super) fn enter(self: &Rc<Self>, target: &ExpressionTarget) -> Result<PathGuard> {
        let mut path = self.path.borrow_mut();
        if path.contains(target) {
            let chain = path
                .iter()
                .chain(std::iter::once(target))
                .map(|target| {
                    serde_json::to_string(target).map(|s| s.chars().take(256).collect::<String>())
                })
                .collect::<std::result::Result<Vec<_>, _>>()?
                .join(" -> ");
            return Err(Error::Invalid(format!(
                "expression dependency cycle: {chain}"
            )));
        }
        ensure(
            path.len() < MAX_REFERENCE_DEPTH,
            "expression reference depth limit exceeded",
        )?;
        path.push(target.clone());
        Ok(PathGuard(self.clone()))
    }
    pub(super) fn record(&self, target: &ExpressionTarget, frame: f64, cause: Error) {
        // The interrupt signal is already a primary failure. Do not replace
        // it with QuickJS's secondary, less specific "interrupted" exception.
        // A native failure recorded before that signal remains untouched.
        if self.interrupted.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let mut first = self.failure.borrow_mut();
        if first.is_none() {
            *first = Some(Error::ExpressionSampling {
                target: serde_json::to_string(target)
                    .unwrap_or_else(|_| "invalid reference target".into())
                    .chars()
                    .take(512)
                    .collect(),
                frame,
                source: Box::new(cause),
            });
        }
    }
    pub(super) fn take_failure(&self) -> Option<Error> {
        self.failure.borrow_mut().take()
    }
    pub(super) fn invocation(
        &self,
        expression: &PropertyExpression,
        frame: f64,
    ) -> Result<Invocation> {
        prepare(&self.project, expression, frame)
    }
    pub(super) fn checked(
        &self,
        expression: &PropertyExpression,
        frame: f64,
        result: Vec<f32>,
    ) -> Result<Vec<f32>> {
        // Use exactly the existing write contracts; a dependency may not bypass
        // dimensions, ranges or opacity clamping merely because it is sampled.
        let local = self.local_frame(&expression.target, frame)?;
        match &expression.target {
            ExpressionTarget::Property {
                object,
                property,
                axis,
            } => {
                let layer = self.project.layers.iter().find(|l| l.id == *object);
                if *object == 0 || layer.is_some_and(|l| l.camera_data().is_some()) {
                    let mut camera = self.project.camera_for(*object)?.clone();
                    write_camera(
                        &mut camera,
                        *property,
                        *axis,
                        local,
                        &result,
                        (*object == 0).then_some(self.project.frames),
                    )?;
                } else {
                    let mut transform = layer.ok_or(Error::Missing(*object))?.transform.clone();
                    write_transform(&mut transform, *property, *axis, local, &result)?;
                }
            }
            ExpressionTarget::Effect {
                object,
                effect,
                param,
            } => {
                let mut parameter = self
                    .project
                    .layers
                    .iter()
                    .find(|l| l.id == *object)
                    .ok_or(Error::Missing(*object))?
                    .effects
                    .iter()
                    .find(|e| e.id == *effect)
                    .and_then(|e| e.params.get(param))
                    .ok_or_else(|| {
                        Error::Invalid("expression effect parameter does not exist".into())
                    })?
                    .clone();
                write_parameter(&mut parameter, &result)?;
            }
        }
        // Return the same visible unit and clamping as the written property.
        if percent(&expression.target) {
            Ok(vec![(result[0] / 100.).clamp(0., 1.) * 100.])
        } else {
            Ok(result)
        }
    }
    pub(super) fn reference(
        &self,
        descriptor: &str,
        seconds: f64,
        execute: &dyn Fn(&PropertyExpression, f64) -> Result<Vec<f32>>,
    ) -> Result<String> {
        ensure(
            seconds.is_finite(),
            "invalid reference sample time: seconds must be finite",
        )?;
        ensure(
            self.calls.get() < MAX_REFERENCE_CALLS,
            "expression reference call limit exceeded",
        )?;
        self.calls.set(self.calls.get() + 1);
        let target = parse_descriptor(descriptor)?;
        let frame = seconds * f64::from(self.project.fps);
        ensure(
            frame.is_finite(),
            "invalid reference sample time: composition frame must be finite",
        )?;
        // Validate the actual requested target, including missing IDs and scalar axes.
        let track = raw(&self.project, &target)?;
        if let Some(expression) = self
            .project
            .expressions
            .iter()
            .find(|e| e.target == target && active(&self.project, e))
        {
            return js_sample(execute(expression, frame)?, false);
        }
        if let ExpressionTarget::Property {
            object,
            property,
            axis,
        } = &target
        {
            if let Some(axis) = axis {
                let whole = ExpressionTarget::Property {
                    object: *object,
                    property: *property,
                    axis: None,
                };
                if let Some(expression) = self
                    .project
                    .expressions
                    .iter()
                    .find(|e| e.target == whole && active(&self.project, e))
                {
                    let values = execute(expression, frame)?;
                    let component = values.get(axis.index()).ok_or_else(|| {
                        Error::Invalid("referenced vector is missing its component".into())
                    })?;
                    return js_sample(vec![*component], false);
                }
            } else if matches!(
                property,
                Property::Position | Property::Rotation | Property::Scale | Property::Target
            ) {
                let local = self.local_frame(&target, frame)?;
                let mut values = track.sample(local)?;
                for (index, axis) in [Axis::X, Axis::Y, Axis::Z].into_iter().enumerate() {
                    let component = ExpressionTarget::Property {
                        object: *object,
                        property: *property,
                        axis: Some(axis),
                    };
                    if let Some(expression) = self
                        .project
                        .expressions
                        .iter()
                        .find(|e| e.target == component && active(&self.project, e))
                    {
                        let result = execute(expression, frame)?;
                        ensure(
                            result.len() == 1,
                            "referenced axis expression must return a scalar",
                        )?;
                        values[index] = result[0];
                    }
                }
                return js_sample(values, false);
            }
        }
        js_sample(
            track.sample(self.local_frame(&target, frame)?)?,
            percent(&target),
        )
    }
    fn local_frame(&self, target: &ExpressionTarget, frame: f64) -> Result<f64> {
        if target.object() == 0 {
            return Ok(frame);
        }
        let layer = self
            .project
            .layers
            .iter()
            .find(|l| l.id == target.object())
            .ok_or(Error::Missing(target.object()))?;
        let local = layer.local_frame(frame);
        ensure(
            local.is_finite(),
            "invalid reference sample time: local frame must be finite",
        )?;
        Ok(local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_references_keep_full_u64_ids_and_escape_parameter_names() {
        for target in [
            ExpressionTarget::Property {
                object: u64::MAX,
                property: Property::Position,
                axis: Some(Axis::X),
            },
            ExpressionTarget::Effect {
                object: u64::MAX - 1,
                effect: u64::MAX,
                param: "量\"\\\n); throw Error('injection');".into(),
            },
        ] {
            let source = expression_reference_source(&target).unwrap();
            assert!(super::super::expression_source_diagnostic(&source).is_none());
            let descriptor = source
                .strip_prefix("propertyValue(")
                .unwrap()
                .strip_suffix(')')
                .unwrap();
            assert_eq!(parse_descriptor(descriptor).unwrap(), target);
            assert!(descriptor.contains("\"18446744073709551615\""));
        }
    }

    #[test]
    fn descriptors_reject_lossy_numeric_ids_unknown_fields_and_unbounded_payloads() {
        for descriptor in [
            r#"{"kind":"property","object":9007199254740993,"property":"position"}"#,
            r#"{"kind":"property","object":"01","property":"position"}"#,
            r#"{"kind":"property","object":"+1","property":"position"}"#,
            r#"{"kind":"property","object":"18446744073709551616","property":"position"}"#,
            r#"{"kind":"property","object":"1","property":"position","unknown":true}"#,
            r#"{"kind":"effect","object":"1","effect":2,"param":"amount"}"#,
            "null",
        ] {
            assert!(parse_descriptor(descriptor).is_err(), "{descriptor}");
        }
        assert!(parse_descriptor(&"x".repeat(MAX_DESCRIPTOR_BYTES + 1)).is_err());
        assert!(expression_reference_source(&ExpressionTarget::Effect {
            object: 1,
            effect: 2,
            param: "x".repeat(MAX_DESCRIPTOR_BYTES),
        })
        .is_err());
    }

    #[test]
    fn session_depth_and_query_limits_are_bounded_and_reset_for_the_next_frame() {
        let mut project = Project::new(64, 64, 30, 60).unwrap();
        project.layers.push(crate::Layer::solid(
            1,
            "Bounded references",
            [20.; 2],
            [10., 0., 0.],
            [1.; 4],
        ));
        let session = Rc::new(Session::new(project.clone()));
        let mut guards = Vec::new();
        for object in 1..=MAX_REFERENCE_DEPTH as u64 {
            guards.push(
                session
                    .enter(&ExpressionTarget::Property {
                        object,
                        property: Property::Position,
                        axis: None,
                    })
                    .unwrap(),
            );
        }
        let target = ExpressionTarget::Property {
            object: 99,
            property: Property::Position,
            axis: None,
        };
        assert!(session
            .enter(&target)
            .err()
            .unwrap()
            .to_string()
            .contains("depth limit"));
        drop(guards);
        assert!(session.path.borrow().is_empty());
        assert!(session.enter(&target).is_ok());
        let target = ExpressionTarget::Property {
            object: 1,
            property: Property::Position,
            axis: Some(Axis::X),
        };
        let source = expression_reference_source(&target).unwrap();
        let descriptor = source
            .strip_prefix("propertyValue(")
            .unwrap()
            .strip_suffix(')')
            .unwrap();
        for _ in 0..MAX_REFERENCE_CALLS {
            assert_eq!(
                session
                    .reference(descriptor, 0., &|_, _| panic!(
                        "raw target must not execute JS"
                    ))
                    .unwrap(),
                "10.0"
            );
        }
        assert!(session
            .reference(descriptor, 0., &|_, _| panic!("limit must fail first"))
            .unwrap_err()
            .to_string()
            .contains("call limit"));
        assert_eq!(
            Session::new(project)
                .reference(descriptor, 0., &|_, _| panic!("raw target"))
                .unwrap(),
            "10.0"
        );
    }
}
