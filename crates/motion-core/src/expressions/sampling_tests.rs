use super::*;
use crate::{Ease, Tween};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq)]
struct IntegralVector([f32; 3]);

impl Tween for IntegralVector {
    fn mix(self, other: Self, t: f32) -> Self {
        Self(std::array::from_fn(|i| {
            (self.0[i] * (1. - t) + other.0[i] * t).round()
        }))
    }
    fn finite(self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }
    fn bounded(self, bound: f32) -> bool {
        self.0.iter().all(|v| v.is_finite() && v.abs() <= bound)
    }
    fn add(self, other: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + other.0[i]))
    }
    fn components(self) -> Option<[f32; 3]> {
        Some(self.0)
    }
    fn from_components(value: [f32; 3]) -> Option<Self> {
        value
            .iter()
            .all(|v| v.is_finite() && v.fract() == 0.)
            .then_some(Self(value))
    }
}

#[test]
fn actual_js_callback_retains_first_native_sample_refusal_even_when_script_catches_it() {
    let mut track = Track::constant(IntegralVector([0.; 3]));
    track
        .upsert(0, IntegralVector([0.; 3]), Ease::Linear)
        .unwrap();
    track
        .upsert(10, IntegralVector([10.; 3]), Ease::Linear)
        .unwrap();
    track.separate().unwrap();
    track.validate(20).unwrap();
    let original = track.sample(5.5).unwrap_err().to_string();
    let expression = PropertyExpression {
        target: ExpressionTarget::Property {
            object: 7,
            property: Property::Position,
            axis: None,
        },
        source:
            "try { valueAtTime(5.5); } catch (e) {} try { valueAtTime(6.5); } catch (e) {} [1,2,3];"
                .into(),
        enabled: true,
        seed: 42,
        profile: crate::EXPRESSION_PROFILE.into(),
    };
    // Compile before starting the existing frame deadline; no budget is increased.
    runtime::compile(&expression.source).unwrap();
    let data = json!({"time":0,"value":[0,0,0],"index":1,"keys":[],"seed":42,
        "comp":{"width":100,"height":100,"frameRate":30,"frameDuration":1.0/30.0,"duration":1},
        "layer":{"name":"test","index":1,"inPoint":0,"outPoint":1,"startTime":0}})
    .to_string();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let captured = calls.clone();
    let error = evaluate_source(
        &expression,
        17.,
        &data,
        move |time| {
            captured.borrow_mut().push(time);
            Ok(serde_json::to_string(&track.sample(time)?.0)?)
        },
        Instant::now() + Duration::from_millis(100),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap_err();
    assert_eq!(*calls.borrow(), [5.5, 6.5]);
    let Error::ExpressionSampling {
        target,
        frame,
        source,
    } = &error
    else {
        panic!("expected native sampling source, got {error:?}");
    };
    assert_eq!(*frame, 17.);
    assert!(target.contains('7'));
    assert_eq!(source.to_string(), original);
    assert!(matches!(source.as_ref(), Error::Invalid(_)));
    let native = std::error::Error::source(&error)
        .unwrap()
        .downcast_ref::<Box<Error>>()
        .expect("sampling source must retain its declared Box<Error>")
        .as_ref();
    assert!(matches!(native, Error::Invalid(_)));
    assert_eq!(native.to_string(), original);
    assert!(!native.to_string().contains("frame=6.5"));

    let healthy = evaluate_source(
        &expression,
        17.,
        &data,
        |_| Ok("[1,2,3]".into()),
        Instant::now() + Duration::from_millis(100),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(healthy, [1., 2., 3.]);
}
