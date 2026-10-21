// Archived first-party packages used only by SDK/host regression tests.
// No product crate or default registry embeds these bytes.
#![allow(dead_code)]
use motion_effects::{EffectPackage, Error, Registry, Result};
use std::sync::{Arc, OnceLock};

pub fn particle_package() -> Result<Arc<EffectPackage>> {
    static PACKAGE: OnceLock<std::result::Result<Arc<EffectPackage>, String>> = OnceLock::new();
    PACKAGE
        .get_or_init(|| {
            EffectPackage::from_bytes(
                include_bytes!("../fixtures/particle-library/particle-effects.msfx").to_vec(),
            )
            .map(Arc::new)
            .map_err(|e| e.to_string())
        })
        .clone()
        .map_err(Error::Invalid)
}

pub fn legacy_particle_package() -> Result<Arc<EffectPackage>> {
    static PACKAGE: OnceLock<std::result::Result<Arc<EffectPackage>, String>> = OnceLock::new();
    PACKAGE
        .get_or_init(|| {
            EffectPackage::from_bytes(
                include_bytes!("../fixtures/particle-library/legacy/particle-effects-1.0.0.msfx")
                    .to_vec(),
            )
            .map(Arc::new)
            .map_err(|e| e.to_string())
        })
        .clone()
        .map_err(Error::Invalid)
}

pub fn legacy_scene_package() -> Result<Arc<EffectPackage>> {
    static PACKAGE: OnceLock<std::result::Result<Arc<EffectPackage>, String>> = OnceLock::new();
    PACKAGE
        .get_or_init(|| {
            EffectPackage::from_bytes(
                include_bytes!("../fixtures/scene-library/legacy/scene-effects-1.0.0.msfx")
                    .to_vec(),
            )
            .map(Arc::new)
            .map_err(|e| e.to_string())
        })
        .clone()
        .map_err(Error::Invalid)
}

pub fn legacy_scene_editor_package() -> Result<Arc<EffectPackage>> {
    static PACKAGE: OnceLock<std::result::Result<Arc<EffectPackage>, String>> = OnceLock::new();
    PACKAGE
        .get_or_init(|| {
            EffectPackage::from_bytes(
                include_bytes!("../fixtures/scene-library/legacy/scene-effects-1.1.0.msfx")
                    .to_vec(),
            )
            .map(Arc::new)
            .map_err(|e| e.to_string())
        })
        .clone()
        .map_err(Error::Invalid)
}

pub fn scene_package() -> Result<Arc<EffectPackage>> {
    static PACKAGE: OnceLock<std::result::Result<Arc<EffectPackage>, String>> = OnceLock::new();
    PACKAGE
        .get_or_init(|| {
            EffectPackage::from_bytes(
                include_bytes!("../fixtures/scene-library/scene-effects.msfx").to_vec(),
            )
            .map(Arc::new)
            .map_err(|e| e.to_string())
        })
        .clone()
        .map_err(Error::Invalid)
}

pub fn registry() -> Result<Registry> {
    let mut registry = Registry::new_with_builtins()?;
    for package in [
        legacy_scene_package()?,
        legacy_scene_editor_package()?,
        scene_package()?,
        legacy_particle_package()?,
        particle_package()?,
    ] {
        registry.insert(package)?;
    }
    Ok(registry)
}
