use motion_model::{Ease, Track};

#[test]
fn nan_sampling_with_zero_one_or_many_keys_falls_back_without_underflow() {
    let mut scalar = Track::constant(9.);
    assert_eq!(scalar.sample(f64::NAN).unwrap(), 9.);
    scalar.upsert(0, 1., Ease::Linear).unwrap();
    assert_eq!(scalar.sample(f64::NAN).unwrap(), 9.);
    scalar.upsert(10, 3., Ease::Linear).unwrap();
    assert_eq!(scalar.sample(f64::NAN).unwrap(), 9.);
    assert_eq!(scalar.sample(2.5).unwrap(), 1.5);
    assert_eq!(scalar.sample(f64::NEG_INFINITY).unwrap(), 1.);
    assert_eq!(scalar.sample(f64::INFINITY).unwrap(), 3.);
}

#[test]
fn vector_nan_sampling_preserves_component_fallback_values() {
    let mut vector = Track::constant([1., 2., 3.]);
    vector.upsert(0, [10., 20., 30.], Ease::Linear).unwrap();
    vector.upsert(10, [20., 30., 40.], Ease::Linear).unwrap();
    assert_eq!(vector.sample(f64::NAN).unwrap(), [1., 2., 3.]);
    assert_eq!(vector.sample(2.5).unwrap(), [12.5, 22.5, 32.5]);
    vector.separate().unwrap();
    assert_eq!(vector.sample(f64::NAN).unwrap(), [1., 2., 3.]);
    vector.axes.as_mut().unwrap().x.value = 8.;
    assert_eq!(vector.sample(f64::NAN).unwrap(), [8., 2., 3.]);
}
