//! Explicit real-GPU cost study, outside the default test/performance matrix.
//! cargo run -p motion-render --example render_dag_costs -- <output.json>
#[path = "../tests/support/dag_cost_fixture.rs"]
mod fixture;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .ok_or("provide an output JSON path")?;
    let mut rows = Vec::new();
    for [width, height] in [[65, 33], [513, 257]] {
        for mode in fixture::MODES {
            rows.push(fixture::run_case(mode, width, height));
        }
    }
    let path = std::path::PathBuf::from(output);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&rows)?)?;
    Ok(())
}
