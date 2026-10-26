//! Exact successful-code reuse. Package identity never stands in for comparing
//! actual code, reflection, static layout and non-numeric resource bytes.
use super::*;
use motion_effects::{EffectPackage, Registry, shader_data::ShaderDataValues};

pub(super) fn values(compiled: &CompiledGraph) -> Result<Option<ShaderDataValues>, String> {
    let definition = compiled
        .manifest
        .effects
        .first()
        .ok_or("node effect has no output")?;
    definition
        .shader_data
        .as_ref()
        .map(|data| {
            data.values(
                data.overflow
                    .as_ref()
                    .and_then(|texture| compiled.resource(&texture.path)),
            )
            .map_err(|e| e.to_string())
        })
        .transpose()
}

pub(super) fn source(bytes: &[u8]) -> Value {
    serde_json::json!({"package_hex":bytes.iter().map(|byte|format!("{byte:02x}")).collect::<String>()})
}

pub(super) fn package(
    registry: &Registry,
    compiled: &CompiledGraph,
    target: &Target,
    old: Option<&EffectInstance>,
) -> Result<Option<Arc<EffectPackage>>, String> {
    let Some(old) = old.filter(|old| old.plugin == bound_id(target)) else {
        return Ok(None);
    };
    let Ok(package) = registry.resolve(&old.plugin, &old.version, &old.hash) else {
        return Ok(None);
    };
    if compiled.manifest.effects.len() != 1 || package.manifest.effects.len() != 1 {
        return Ok(None);
    }
    let mut candidate = compiled.manifest.clone();
    candidate.id = package.manifest.id.clone();
    candidate.version = package.manifest.version.clone();
    let next = &mut candidate.effects[0];
    let previous = &package.manifest.effects[0];
    if next.id != old.effect || next.passes.len() != 1 || next.params.len() != previous.params.len()
    {
        return Ok(None);
    }
    // New public defaults initialize the instance; they do not specialize code.
    for (next, previous) in next.params.iter_mut().zip(&previous.params) {
        next.default = previous.default;
    }
    match (&mut next.shader_data, &previous.shader_data) {
        (Some(next), Some(previous))
            if next.version == previous.version
                && next.uniforms.len() == previous.uniforms.len()
                && next.overflow == previous.overflow =>
        {
            next.uniforms.clone_from(&previous.uniforms);
        }
        (None, None) => {}
        _ => return Ok(None),
    }
    if candidate != package.manifest {
        return Ok(None);
    }
    let Some(shader) = package.shaders.get(&(previous.id.clone(), 0)) else {
        return Ok(None);
    };
    if package
        .files
        .get(&previous.passes[0].shader)
        .map(Vec::as_slice)
        != Some(compiled.source.as_bytes())
        || shader.wgsl != compiled.shader.wgsl
        || shader.sprite != compiled.shader.sprite
        || shader.additive != compiled.shader.additive
        || shader.loop_work != compiled.shader.loop_work
        || shader.glsl.vertex != compiled.shader.glsl.vertex
        || shader.glsl.fragment != compiled.shader.glsl.fragment
        || shader.glsl.blocks != compiled.shader.glsl.blocks
        || shader.glsl.textures != compiled.shader.glsl.textures
    {
        return Ok(None);
    }
    let numeric = previous
        .shader_data
        .as_ref()
        .and_then(|data| data.overflow.as_ref())
        .map(|t| &t.path);
    for path in &previous.resources {
        if Some(path) != numeric
            && compiled.resource(path) != package.files.get(path).map(Vec::as_slice)
        {
            return Ok(None);
        }
    }
    Ok(Some(package))
}
