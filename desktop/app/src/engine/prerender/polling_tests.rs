//! CPU protocol controls. Native cancellation/recovery is tested separately.
use super::*;

fn enabled() -> Runtime {
    let mut runtime = Runtime::new(Arc::new(Signals::default()));
    runtime.configure(Policy { enabled: true, ..Default::default() }).unwrap();
    runtime
}

#[test]
fn acknowledged_idle_owner_parks_until_the_input_present_arrives() {
    let mut runtime = enabled();
    assert!(runtime.polling());
    runtime.signals.input(InputActivity::Pulse).unwrap();
    assert!(!runtime.polling(),
        "pending input with no outstanding lease must park optional timer polling");
    assert_eq!(runtime.status()["hold"], "input");
    assert_eq!(runtime.status()["inputPending"], true);
    let token = runtime.signals.preempt_for_present().unwrap();
    runtime.foreground_presented(token);
    assert!(runtime.polling(), "successful input Present must rearm optional work");
    assert_eq!(runtime.status()["inputPending"], false);
    runtime.configure(Default::default()).unwrap();
    assert!(!runtime.polling());
}

#[test]
fn a_stale_present_cannot_wake_a_newer_input_hold() {
    let mut runtime = enabled();
    let older = runtime.signals.preempt_for_present().unwrap();
    runtime.signals.input(InputActivity::Pulse).unwrap();
    runtime.foreground_presented(older);
    assert!(!runtime.polling(), "old Present cannot acknowledge newer input");
    let current = runtime.signals.preempt_for_present().unwrap();
    runtime.foreground_presented(current);
    assert!(runtime.polling());
}

#[test]
fn pending_input_keeps_polling_while_an_acknowledgment_is_outstanding() {
    let mut runtime = enabled();
    runtime.signals.input(InputActivity::Pulse).unwrap();
    // Controlled protocol state, not a claim that a native job was created.
    // Explicit native completed/late-input tests verify actual lease release.
    runtime.lease = Some(7);
    assert!(runtime.polling(), "input must not strand completion acknowledgment");
    runtime.lease = None;
    assert!(!runtime.polling());
    runtime.configure(Default::default()).unwrap();
}

#[test]
fn a_fault_parks_only_after_acknowledgment_and_explicit_configuration_wakes_it() {
    let mut runtime = enabled();
    runtime.fault = Some("controlled protocol fault".into());
    runtime.lease = Some(7);
    assert!(runtime.polling());
    runtime.lease = None;
    assert!(!runtime.polling(), "a terminal optional fault cannot require a timer retry");
    runtime.configure(Policy { enabled: true, ..Default::default() }).unwrap();
    assert!(runtime.polling(), "explicit configuration is the recovery boundary");
}
