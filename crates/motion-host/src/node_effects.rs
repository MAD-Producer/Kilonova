//! Restore last valid node runtimes referenced by project-owned authoring data.
//! No node compiler or GPU access is needed to reopen an unfinished graph.
use motion_effects::{EffectPackage, Registry};
use std::sync::Arc;
pub(crate) fn restore(
    project: &motion_core::Project,
    root: &std::path::Path,
    registry: &mut Registry,
) -> crate::Result<()> {
    restore_effects(project
        .layers
        .iter()
        .chain(project.compositions.iter().flat_map(|c| &c.layers))
        .flat_map(|l| &l.effects), root, registry)
}
pub(crate) fn restore_effects<'a>(
    effects: impl IntoIterator<Item = &'a motion_core::EffectInstance>,
    root: &std::path::Path,
    registry: &mut Registry,
) -> crate::Result<()> {
    for effect in effects {
        let Some(runtime) = effect.node_graph.as_ref().and_then(|s| s.get("runtime")) else {
            continue;
        };
        if registry
            .resolve(&effect.plugin, &effect.version, &effect.hash)
            .is_ok()
        {
            continue;
        }
        let resource: motion_core::NodeResource = serde_json::from_value(runtime.clone())?;
        resource.validate("msfx")?;
        let bytes = motion_core::storage::read_node_resource(root, &resource)
            .map_err(|e| crate::Error::with_context("read compiled node runtime", e))?;
        let package = EffectPackage::from_bytes(bytes)
            .map_err(|e| crate::Error::with_context("invalid compiled node runtime", e))?;
        if package.manifest.id != effect.plugin
            || package.manifest.version != effect.version
            || package.hash != effect.hash
            || !package
                .manifest
                .effects
                .iter()
                .any(|d| d.id == effect.effect)
        {
            return Err(
                "compiled node runtime does not match the project effect dependency".into(),
            );
        }
        registry
            .insert(Arc::new(package))
            .map_err(crate::Error::from)?;
    }
    Ok(())
}
