#[path = "support/dag_cost_fixture.rs"]
mod fixture;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn numeric_edits_compile_nothing_and_structural_edits_preserve_an_independent_group() {
    let rows: Vec<_> = fixture::MODES
        .into_iter()
        .map(|mode| fixture::run_case(mode, 65, 33))
        .collect();
    if let Some(out) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            std::path::PathBuf::from(out).join("multi-group-invalidation.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
