use motion_effects::{package_directory_with_limits, package_source_plan, EffectPackage, PackageReadLimits};
use std::{env, fs, path::Path};
fn limits(args: &[String], end: usize) -> Result<PackageReadLimits, Box<dyn std::error::Error>> {
    if args.len() == end {
        return Ok(PackageReadLimits::default());
    }
    if args.len() == end + 2 && args[end] == "--decoded-image-budget-bytes" {
        return Ok(PackageReadLimits::new(args[end + 1].parse()?)?);
    }
    Err("expected --decoded-image-budget-bytes <4..2147483648>".into())
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("plan") if args.len()==3=>{println!("{}",serde_json::to_string(&package_source_plan(Path::new(&args[2]))?)?);}
        Some("builtin") if args.len()==3=>{let p=motion_effects::builtin::package()?;fs::write(&args[2],&p.bytes)?;println!("{} {} {}",p.manifest.id,p.manifest.version,p.hash);}
        Some("pack") if args.len()>=4=>{let bytes=package_directory_with_limits(Path::new(&args[2]),limits(&args,4)?)?;fs::write(&args[3],bytes)?;}
        Some("check") if args.len()>=3=>{let p=EffectPackage::open_with_limits(Path::new(&args[2]),limits(&args,3)?)?;println!("{} {} {} ({} effects)",p.manifest.id,p.manifest.version,p.hash,p.manifest.effects.len());}
        Some("glsl") if args.len()>=4=>{let p=EffectPackage::open_with_limits(Path::new(&args[2]),limits(&args,4)?)?;let out=Path::new(&args[3]);fs::create_dir_all(out)?;for ((id,index),shader) in &p.shaders {fs::write(out.join(format!("{id}-{index}.vert")),&shader.glsl.vertex)?;fs::write(out.join(format!("{id}-{index}.frag")),&shader.glsl.fragment)?;fs::write(out.join(format!("{id}-{index}.json")),serde_json::to_vec_pretty(&shader.glsl)?)?;}}
        _=>return Err("usage: effect_tool plan <manifest.json> | builtin <output.msfx> | pack <directory> <output.msfx> [--decoded-image-budget-bytes N] | check <package> [--decoded-image-budget-bytes N] | glsl <package> <directory> [--decoded-image-budget-bytes N]".into())
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
