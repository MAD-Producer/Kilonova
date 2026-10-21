//! Restore last valid node runtimes embedded with project-owned authoring data.
//! No node compiler or GPU access is needed to reopen an unfinished graph.
use motion_effects::{EffectPackage, Registry};
use std::sync::Arc;
pub(crate) fn restore(
    project: &motion_core::Project,
    registry: &mut Registry,
) -> Result<(), String> {
    for effect in project
        .layers
        .iter()
        .chain(project.compositions.iter().flat_map(|c| &c.layers))
        .flat_map(|l| &l.effects)
    {
        let Some(runtime) = effect.node_graph.as_ref().and_then(|s| s.get("runtime")) else {
            continue;
        };
        if registry
            .resolve(&effect.plugin, &effect.version, &effect.hash)
            .is_ok()
        {
            continue;
        }
        let encoded = runtime["package_hex"]
            .as_str()
            .ok_or("missing compiled node runtime archive")?;
        if encoded.len() % 2 != 0 || encoded.len() > 32 * 1024 * 1024 {
            return Err("invalid compiled node runtime archive length".into());
        }
        let mut bytes = Vec::with_capacity(encoded.len() / 2);
        for pair in encoded.as_bytes().chunks_exact(2) {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                _ => None,
            };
            let high = digit(pair[0]).ok_or("invalid compiled node runtime archive encoding")?;
            let low = digit(pair[1]).ok_or("invalid compiled node runtime archive encoding")?;
            bytes.push((high << 4) | low);
        }
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
