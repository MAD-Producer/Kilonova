use motion_host::ops::{catch_panic, dispatch};

#[test]
fn native_boundaries_distinguish_errors_format_utf8_and_bound_unknown_payloads() {
    assert_eq!(
        catch_panic(|| Err::<(), _>("ordinary error")),
        Ok(Err("ordinary error"))
    );
    let reason = "错".repeat(3000);
    let caught = catch_panic(|| std::panic::panic_any(reason)).unwrap_err();
    assert_eq!(caught.chars().count(), 2048);
    assert_eq!(
        catch_panic(|| std::panic::panic_any(42u32)).unwrap_err(),
        "unknown native error"
    );
    let result = dispatch(|| panic!("native failure"));
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"], "native failure");
    assert_eq!(
        dispatch(|| Err("ordinary error".into()))["error"],
        "ordinary error"
    );
}
