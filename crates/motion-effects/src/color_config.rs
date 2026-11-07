//! Content-verified OCIO catalogues and explicit selection, without ambient paths
//! or fallback roles. An authoring OCIO runtime supplies the catalogue; this module
//! verifies its closed byte set and references, not OCIO syntax or pixel transforms.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

pub const CONFIG_MANIFEST_VERSION: u32 = 1;
pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
pub const MAX_CONFIG_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RESOURCE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_BUNDLE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_CONFIG_RESOURCES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConfigOrigin {
    Builtin {
        id: String,
        aces: ColorVersion,
    },
    /// A descriptive local identifier, never a file path to resolve implicitly.
    Authored {
        id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentIdentity {
    pub bytes: usize,
    pub sha256: String,
}
impl ContentIdentity {
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.len(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceSpace {
    Scene,
    Display,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigColorSpace {
    /// OCIO's declared encoding, not an inferred transfer/primaries/white point.
    pub encoding: String,
    pub reference: ReferenceSpace,
    pub is_data: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorConfigManifest {
    pub version: u32,
    pub ocio_version: ColorVersion,
    pub origin: ConfigOrigin,
    pub config: ContentIdentity,
    #[serde(deserialize_with = "unique_map")]
    pub resources: BTreeMap<String, ContentIdentity>,
    /// Explicit resolved values. No environment lookup occurs while resolving.
    #[serde(deserialize_with = "unique_map")]
    pub context: BTreeMap<String, String>,
    #[serde(deserialize_with = "unique_map")]
    pub spaces: BTreeMap<String, ConfigColorSpace>,
    #[serde(deserialize_with = "unique_map")]
    pub roles: BTreeMap<String, String>,
    #[serde(deserialize_with = "unique_map")]
    pub displays: BTreeMap<String, Vec<String>>,
}

fn unique_map<'de, D, T>(deserializer: D) -> std::result::Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct MapVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for MapVisitor<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map with unique names")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut access: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = access.next_entry::<String, T>()? {
                if result.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate color configuration name",
                    ));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
}

#[derive(Debug, thiserror::Error)]
pub enum ColorConfigError {
    #[error("color config phase=manifest line={} column={}", .0.line(), .0.column())]
    Json(#[from] serde_json::Error),
    #[error("color config invalid field={field}: {reason}")]
    Invalid {
        field: &'static str,
        reason: &'static str,
    },
    #[error("color config capacity field={field} requested={requested} limit={limit}")]
    Capacity {
        field: &'static str,
        requested: usize,
        limit: usize,
    },
    #[error("color config resource={resource}: {reason}")]
    Resource {
        resource: String,
        reason: &'static str,
    },
    #[error("color config selected content identity does not match loaded configuration")]
    ConfigMismatch,
    #[error("color config selected ACES version differs from loaded configuration")]
    AcesMismatch,
    #[error("color config missing {kind}={name}")]
    Missing { kind: &'static str, name: String },
    #[error("color config data space={name} cannot be used as a color transform")]
    DataSpace { name: String },
}

type Result<T> = std::result::Result<T, ColorConfigError>;
fn invalid(field: &'static str, reason: &'static str) -> ColorConfigError {
    ColorConfigError::Invalid { field, reason }
}
fn bounded(field: &'static str, requested: usize, limit: usize) -> Result<()> {
    if requested > limit {
        Err(ColorConfigError::Capacity {
            field,
            requested,
            limit,
        })
    } else {
        Ok(())
    }
}
fn name(field: &'static str, value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(invalid(
            field,
            "requires 1–256 UTF-8 bytes without control characters",
        ));
    }
    Ok(())
}
fn resource_name(value: &str) -> Result<()> {
    name("resource", value)?;
    if value.contains(['\\', ':'])
        || value
            .split('/')
            .any(|s| s.is_empty() || matches!(s, "." | ".."))
    {
        return Err(invalid(
            "resource",
            "requires a canonical relative slash-separated name",
        ));
    }
    Ok(())
}
fn identity(value: &ContentIdentity, limit: usize) -> Result<()> {
    bounded("resource bytes", value.bytes, limit)?;
    if value.bytes == 0
        || value.sha256.len() != 64
        || !value
            .sha256
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(invalid(
            "identity",
            "requires nonempty content and lowercase SHA-256",
        ));
    }
    Ok(())
}

impl ColorConfigManifest {
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        bounded("manifest bytes", bytes.len(), MAX_MANIFEST_BYTES)?;
        let manifest: Self = serde_json::from_slice(bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }
    pub fn validate(&self) -> Result<()> {
        if self.version != CONFIG_MANIFEST_VERSION {
            return Err(invalid("version", "unsupported manifest schema"));
        }
        if self.ocio_version.major < 2 {
            return Err(invalid("ocio_version", "requires OCIO 2 or newer"));
        }
        match &self.origin {
            ConfigOrigin::Builtin { id, aces } => {
                name("builtin", id)?;
                if !(1..=2).contains(&aces.major) {
                    return Err(invalid(
                        "aces",
                        "requires an explicit ACES 1.x or 2.x version",
                    ));
                }
            }
            ConfigOrigin::Authored { id } => name("authored", id)?,
        }
        identity(&self.config, MAX_CONFIG_BYTES)?;
        bounded("resources", self.resources.len(), MAX_CONFIG_RESOURCES)?;
        let mut total = self.config.bytes;
        for (path, item) in &self.resources {
            resource_name(path)?;
            identity(item, MAX_RESOURCE_BYTES)?;
            total = total
                .checked_add(item.bytes)
                .ok_or_else(|| invalid("bundle", "byte count overflow"))?;
        }
        bounded("bundle bytes", total, MAX_BUNDLE_BYTES)?;
        bounded("context", self.context.len(), 128)?;
        bounded("spaces", self.spaces.len(), 512)?;
        bounded("roles", self.roles.len(), 128)?;
        bounded("displays", self.displays.len(), 32)?;
        if self.spaces.is_empty() {
            return Err(invalid("spaces", "empty catalogue"));
        }
        for (key, value) in &self.context {
            name("context key", key)?;
            if !value.is_empty() {
                name("context value", value)?;
            }
        }
        for (key, value) in &self.spaces {
            name("space", key)?;
            // OCIO permits unspecified encoding. Preserve it rather than guessing.
            if !value.encoding.is_empty() {
                name("encoding", &value.encoding)?;
            }
        }
        for (role, space) in &self.roles {
            name("role", role)?;
            name("role target", space)?;
            if !self.spaces.contains_key(space) {
                return Err(ColorConfigError::Missing {
                    kind: "role target",
                    name: space.clone(),
                });
            }
        }
        for (display, views) in &self.displays {
            name("display", display)?;
            bounded("views", views.len(), 64)?;
            if views.is_empty() {
                return Err(invalid("views", "display has no views"));
            }
            let mut seen = std::collections::BTreeSet::new();
            for view in views {
                name("view", view)?;
                if !seen.insert(view) {
                    return Err(invalid("view", "duplicate view"));
                }
            }
        }
        Ok(())
    }
}

/// Retains the exact verified resource bytes. Replacing a file on disk does not
/// alter this object's identity. Construction is preparation, never a Present op.
pub struct VerifiedColorConfig {
    manifest: ColorConfigManifest,
    key: [u8; 32],
    config: Arc<[u8]>,
    resources: BTreeMap<String, Arc<[u8]>>,
}
impl VerifiedColorConfig {
    pub fn new(
        manifest: ColorConfigManifest,
        config: Arc<[u8]>,
        resources: BTreeMap<String, Arc<[u8]>>,
    ) -> Result<Self> {
        manifest.validate()?;
        // Bound actual bytes too; a hostile declared size must not cause an
        // unbounded hashing pass over already supplied input.
        bounded("actual config bytes", config.len(), MAX_CONFIG_BYTES)?;
        bounded("actual resources", resources.len(), MAX_CONFIG_RESOURCES)?;
        let mut total = config.len();
        for bytes in resources.values() {
            bounded("actual resource bytes", bytes.len(), MAX_RESOURCE_BYTES)?;
            total = total
                .checked_add(bytes.len())
                .ok_or_else(|| invalid("bundle", "byte count overflow"))?;
        }
        bounded("actual bundle bytes", total, MAX_BUNDLE_BYTES)?;
        if ContentIdentity::of(&config) != manifest.config {
            return Err(ColorConfigError::Resource {
                resource: "config".into(),
                reason: "content identity mismatch",
            });
        }
        for path in resources.keys() {
            if !manifest.resources.contains_key(path) {
                return Err(ColorConfigError::Resource {
                    resource: path.chars().take(256).collect(),
                    reason: "undeclared resource",
                });
            }
        }
        for (path, expected) in &manifest.resources {
            let bytes = resources
                .get(path)
                .ok_or_else(|| ColorConfigError::Resource {
                    resource: path.clone(),
                    reason: "missing resource",
                })?;
            if ContentIdentity::of(bytes) != *expected {
                return Err(ColorConfigError::Resource {
                    resource: path.clone(),
                    reason: "content identity mismatch",
                });
            }
        }
        let mut digest = Sha256::new();
        digest.update(b"motion-color-config-v1\0");
        // BTreeMap keys and struct field order define a stable, path-independent
        // representation. OCIO cache IDs alone are not content identities.
        let manifest_bytes = serde_json::to_vec(&manifest)?;
        bounded("manifest bytes", manifest_bytes.len(), MAX_MANIFEST_BYTES)?;
        digest.update(manifest_bytes);
        Ok(Self {
            manifest,
            key: digest.finalize().into(),
            config,
            resources,
        })
    }
    pub fn key(&self) -> [u8; 32] {
        self.key
    }
    pub fn manifest(&self) -> &ColorConfigManifest {
        &self.manifest
    }
    pub fn config_bytes(&self) -> &[u8] {
        &self.config
    }
    pub fn resource_bytes(&self, path: &str) -> Option<&[u8]> {
        self.resources.get(path).map(AsRef::as_ref)
    }

    fn color_space(&self, reference: &SpaceReference) -> Result<&str> {
        let target = match reference {
            SpaceReference::Space { name: value } => {
                name("selected space", value)?;
                value.as_str()
            }
            SpaceReference::Role { name: value } => {
                name("selected role", value)?;
                self.manifest
                    .roles
                    .get(value)
                    .ok_or_else(|| ColorConfigError::Missing {
                        kind: "role",
                        name: value.clone(),
                    })?
                    .as_str()
            }
        };
        let (canonical, space) = self.manifest.spaces.get_key_value(target).ok_or_else(|| {
            ColorConfigError::Missing {
                kind: "space",
                name: target.into(),
            }
        })?;
        if space.is_data {
            return Err(ColorConfigError::DataSpace {
                name: canonical.clone(),
            });
        }
        Ok(canonical)
    }
    fn display_view(&self, value: &DisplayView) -> Result<()> {
        name("selected display", &value.display)?;
        name("selected view", &value.view)?;
        let views = self.manifest.displays.get(&value.display).ok_or_else(|| {
            ColorConfigError::Missing {
                kind: "display",
                name: value.display.clone(),
            }
        })?;
        if !views.contains(&value.view) {
            return Err(ColorConfigError::Missing {
                kind: "view",
                name: value.view.clone(),
            });
        }
        Ok(())
    }
    pub fn resolve(&self, selection: &ColorSelection) -> Result<ResolvedColorSelection> {
        if selection.config_key != self.key {
            return Err(ColorConfigError::ConfigMismatch);
        }
        let aces = match &self.manifest.origin {
            ConfigOrigin::Builtin { aces, .. } => Some(*aces),
            ConfigOrigin::Authored { .. } => None,
        };
        if selection.aces != aces {
            return Err(ColorConfigError::AcesMismatch);
        }
        let input = self.color_space(&selection.input)?.to_owned();
        let working = self.color_space(&selection.working)?.to_owned();
        self.display_view(&selection.display)?;
        let output = match &selection.output {
            OutputSelection::Space { space } => {
                ResolvedColorOutput::Space(self.color_space(space)?.to_owned())
            }
            OutputSelection::Display { display_view } => {
                self.display_view(display_view)?;
                ResolvedColorOutput::Display(display_view.clone())
            }
        };
        let mut digest = Sha256::new();
        digest.update(b"motion-color-selection-v1\0");
        digest.update(serde_json::to_vec(selection)?);
        Ok(ResolvedColorSelection {
            key: digest.finalize().into(),
            input,
            working,
            display: selection.display.clone(),
            output,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpaceReference {
    Space { name: String },
    Role { name: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayView {
    pub display: String,
    pub view: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutputSelection {
    Space { space: SpaceReference },
    Display { display_view: DisplayView },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorSelection {
    pub config_key: [u8; 32],
    pub aces: Option<ColorVersion>,
    pub input: SpaceReference,
    pub working: SpaceReference,
    pub display: DisplayView,
    pub output: OutputSelection,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedColorOutput {
    Space(String),
    Display(DisplayView),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedColorSelection {
    pub key: [u8; 32],
    pub input: String,
    pub working: String,
    pub display: DisplayView,
    pub output: ResolvedColorOutput,
}
