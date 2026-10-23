use super::*;

#[test]
fn parsed_package_capacity_deduplicates_real_arc_owners_but_not_equal_hashes() {
    let original = crate::builtin::legacy_current_package().unwrap();
    let mut parsed = EffectPackage::from_bytes(original.bytes.clone()).unwrap();
    parsed.bytes.reserve(128);
    let file = parsed.files.values_mut().next().unwrap();
    file.reserve(64);
    let parsed = Arc::new(parsed);
    assert_eq!(parsed.hash, original.hash);
    let archive_capacity = parsed.bytes.capacity() as u64;
    let files_capacity = parsed
        .files
        .values()
        .map(|v| v.capacity() as u64)
        .sum::<u64>();
    let shader_capacity = parsed
        .shaders
        .values()
        .map(|s| (s.wgsl.capacity() + s.glsl.vertex.capacity() + s.glsl.fragment.capacity()) as u64)
        .sum::<u64>();
    let before = Arc::strong_count(&parsed);
    let observed = package_memory([
        (PackageOwner::Registry, &parsed),
        (PackageOwner::PlanProgram, &parsed),
        (PackageOwner::PlanProgram, &parsed),
        (PackageOwner::Registry, &original),
    ]);
    assert_eq!(observed.resources.len(), 2);
    assert_eq!(observed.resources[0].registry_references, 1);
    assert_eq!(observed.resources[0].program_references, 2);
    assert_eq!(
        observed.resources[0].archive.capacity_bytes,
        archive_capacity
    );
    assert_eq!(
        observed.resources[0].expanded_files.capacity_bytes,
        files_capacity
    );
    assert_eq!(
        observed.resources[0].compiled_shader_text.capacity_bytes,
        shader_capacity
    );
    assert_eq!(Arc::strong_count(&parsed), before);
    assert!(observed.known_capacity_bytes > observed.known_payload_bytes);
    assert!(observed.all_package_owners_observed);
    assert!(!observed.complete_coverage);
    assert!(observed.unknown_bytes.is_none());
    let empty = package_memory(std::iter::empty());
    assert_eq!(empty.known_capacity_bytes, 0);
    assert!(empty.resources.is_empty());
    assert!(Arc::strong_count(&parsed) == before);
}

#[test]
fn observation_limit_keeps_existing_alias_counts_and_marks_new_owners_unknown() {
    let template = crate::builtin::legacy_current_package().unwrap();
    // The observation-bound fixture only needs package-owned byte vectors;
    // avoid cloning the full effect catalog 1025 times.
    let mut manifest = template.manifest.clone();
    manifest.effects.clear();
    let packages: Vec<_> = (0..MAX_PACKAGES + 1)
        .map(|_| {
            Arc::new(EffectPackage {
                manifest: manifest.clone(),
                hash: "same-content-different-allocation".repeat(10),
                files: Default::default(),
                shaders: Default::default(),
                bytes: vec![1; 8],
            })
        })
        .collect();
    let observed = package_memory(
        packages
            .iter()
            .map(|p| (PackageOwner::Registry, p))
            .chain(std::iter::once((PackageOwner::PlanProgram, &packages[0]))),
    );
    assert_eq!(observed.resources.len(), MAX_PACKAGES);
    assert_eq!(observed.known_capacity_bytes, (MAX_PACKAGES * 8) as u64);
    assert_eq!(observed.unobserved_references, 1);
    assert!(!observed.all_package_owners_observed);
    assert_eq!(observed.resources[0].program_references, 1);
    assert_eq!(observed.resources[0].hash.len(), 160);
    assert!(observed.unknown_bytes.is_none());
}
