fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "target/node-demo/drifting-circle.msfx".into());
    let report = motion_graph::export_msfx(
        &motion_graph::demo_graph(),
        &motion_graph::CompileOptions::default(),
        &output,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
