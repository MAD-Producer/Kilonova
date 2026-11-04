use std::hint::black_box;

#[test]
fn integer_overflow_is_checked_in_optimized_and_development_builds() {
    // Run with --release as well: debug builds alone cannot prove the policy.
    // Keep the inputs opaque to the optimizer so this exercises real arithmetic.
    let cases: [(&str, fn()); 6] = [
        ("unsigned add", || {
            black_box(black_box(u64::MAX) + black_box(1));
        }),
        ("unsigned subtract", || {
            black_box(black_box(0_u64) - black_box(1));
        }),
        ("unsigned multiply", || {
            black_box(black_box(u64::MAX) * black_box(2));
        }),
        ("signed add", || {
            black_box(black_box(i64::MAX) + black_box(1));
        }),
        ("signed subtract", || {
            black_box(black_box(i64::MIN) - black_box(1));
        }),
        ("signed multiply", || {
            black_box(black_box(i64::MAX) * black_box(2));
        }),
    ];
    for (name, operation) in cases {
        assert!(
            std::panic::catch_unwind(operation).is_err(),
            "{name} silently wrapped; integer overflow checks must stay enabled"
        );
    }
    assert_eq!(black_box(17_u64) + black_box(25), 42);
    assert_eq!(black_box(-17_i64) + black_box(25), 8);
}
