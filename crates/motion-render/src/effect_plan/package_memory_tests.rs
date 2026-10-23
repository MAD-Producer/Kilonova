use super::*;

#[test]
fn actual_registry_and_program_package_owners_deduplicate_and_release_independently() {
    let package = motion_effects::builtin::legacy_current_package().unwrap();
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    let mut builder = PlanBuilder::new(registry).unwrap();
    let initial = builder.package_memory_info();
    let strong = Arc::strong_count(&package);
    let shader = Arc::new(package.shaders.values().next().unwrap().clone());
    builder.programs.push(EffectProgram {
        key: "authored-memory-observation".into(),
        shader,
        package: Some(package.clone()),
        resources: Vec::new(),
    });
    let observed = builder.package_memory_info();
    assert_eq!(observed.resources.len(), 1);
    assert_eq!(observed.known_capacity_bytes, initial.known_capacity_bytes);
    assert_eq!(observed.resources[0].registry_references, 1);
    assert_eq!(observed.resources[0].program_references, 1);
    assert_eq!(Arc::strong_count(&package), strong + 1);
    builder.registry.packages.clear();
    let retained = builder.package_memory_info();
    assert_eq!(retained.known_capacity_bytes, initial.known_capacity_bytes);
    assert_eq!(retained.resources[0].registry_references, 0);
    assert_eq!(retained.resources[0].program_references, 1);
    builder.programs.clear();
    assert_eq!(builder.package_memory_info().known_capacity_bytes, 0);
    assert!(
        !package.bytes.is_empty(),
        "external package remains alive but is not owned by this builder"
    );
}
