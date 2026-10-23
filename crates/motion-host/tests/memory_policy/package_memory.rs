use super::*;

#[test]
fn real_editor_and_session_package_owners_count_once_without_mutating_project_or_gpu_budget() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::new(
        Project::new(64, 64, 30, 3).unwrap(),
        directory.path().into(),
        Arc::new(Stub),
    )
    .unwrap();
    let snapshot = session.engine.snapshot();
    let revision = session.engine.revision();
    let view = session.view_revision;
    let packages: Vec<_> = session
        .effects
        .registry
        .packages
        .values()
        .cloned()
        .collect();
    assert!(!packages.is_empty());
    // Derive expected capacities from the actual retained buffers, not hashes
    // or compressed file sizes. Startup registry keys refer to unique packages.
    let expected_capacity: u64 = packages
        .iter()
        .map(|p| {
            p.bytes.capacity() as u64
                + p.files.values().map(|v| v.capacity() as u64).sum::<u64>()
                + p.shaders
                    .values()
                    .map(|s| {
                        (s.wgsl.capacity() + s.glsl.vertex.capacity() + s.glsl.fragment.capacity())
                            as u64
                    })
                    .sum::<u64>()
        })
        .sum();
    let initial = session.resource_memory_ledger().unwrap();
    let program_capacity = |builder: &motion_render::effect_plan::PlanBuilder| {
        builder
            .programs
            .iter()
            .map(|p| {
                (p.shader.wgsl.capacity()
                    + p.shader.glsl.vertex.capacity()
                    + p.shader.glsl.fragment.capacity()) as u64
            })
            .sum::<u64>()
    };
    let session_shader_capacity = program_capacity(&session.effects);
    assert!(session_shader_capacity > 0);
    assert_eq!(
        initial["cpu"]["effectPackages"]["knownCapacityBytes"],
        expected_capacity
    );
    assert_eq!(
        initial["cpu"]["knownCapacityBytes"],
        expected_capacity + session_shader_capacity
    );
    assert_eq!(
        initial["cpu"]["effectProgramShaders"]["knownCapacityBytes"],
        session_shader_capacity
    );
    assert!(session.graphics.is_none());
    assert!(session.editor_renderer.is_none());
    let budget = session.allocation_budget().clone();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    renderer.set_effect_registry(session.effects.registry.clone());
    let mut shader_identities = std::collections::HashSet::new();
    let total_shader_capacity: u64 = session
        .effects
        .compiled_shader_owners()
        .chain(renderer.compiled_shader_owners())
        .filter(|(_, s)| shader_identities.insert(Arc::as_ptr(s)))
        .map(|(_, s)| {
            (s.wgsl.capacity() + s.glsl.vertex.capacity() + s.glsl.fragment.capacity()) as u64
        })
        .sum();
    let device = renderer.device.clone();
    session.editor_renderer = Some(renderer);
    let budget_before = budget.info().unwrap();
    let gpu_before = session.texture_resource_ledger();
    let strong: Vec<_> = packages.iter().map(Arc::strong_count).collect();
    let observed = session
        .plugin_request(r#"{"op":"resource_memory"}"#)
        .unwrap();
    assert_eq!(
        observed["cpu"]["effectPackages"]["knownCapacityBytes"],
        expected_capacity
    );
    assert_eq!(
        observed["cpu"]["knownCapacityBytes"],
        expected_capacity + total_shader_capacity
    );
    assert_eq!(
        observed["cpu"]["effectProgramShaders"]["knownCapacityBytes"],
        total_shader_capacity
    );
    let resources = observed["cpu"]["effectPackages"]["resources"]
        .as_array()
        .unwrap();
    assert_eq!(resources.len(), packages.len());
    for package in resources {
        assert_eq!(package["registryReferences"], 2);
        assert_eq!(package["programReferences"], 0);
    }
    assert_eq!(
        packages.iter().map(Arc::strong_count).collect::<Vec<_>>(),
        strong
    );
    assert_eq!(budget.info().unwrap(), budget_before);
    assert_eq!(session.texture_resource_ledger(), gpu_before);
    assert_eq!(session.engine.snapshot(), snapshot);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.view_revision, view);
    assert!(!session.engine.can_undo());

    // Clearing one CPU owner must not make a still-retained package disappear.
    session.effects.registry.packages.clear();
    let retained = session.resource_memory_ledger().unwrap();
    assert_eq!(
        retained["cpu"]["knownCapacityBytes"],
        expected_capacity + total_shader_capacity
    );
    for package in retained["cpu"]["effectPackages"]["resources"]
        .as_array()
        .unwrap()
    {
        assert_eq!(package["registryReferences"], 1);
    }
    session.editor_renderer = None;
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let released = session.resource_memory_ledger().unwrap();
    assert_eq!(
        released["cpu"]["effectPackages"]["resources"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        released["cpu"]["knownCapacityBytes"],
        session_shader_capacity
    );
    session.effects.programs.clear();
    assert_eq!(
        session.resource_memory_ledger().unwrap()["cpu"]["knownCapacityBytes"],
        0
    );
    assert_eq!(released["completeCoverage"], false);
    assert!(released["external"]["driverResidentBytes"].is_null());
    assert!(
        packages.iter().all(|p| !p.bytes.is_empty()),
        "external/global packages remain outside Session ownership"
    );
}
