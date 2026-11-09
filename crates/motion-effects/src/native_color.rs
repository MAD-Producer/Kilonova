//! Explicit pinned native OCIO loading and owned CPU/GPU processor preparation.
//! Call on a preparation worker under the host's in-flight observation. This
//! module neither chooses a default configuration nor authorizes scene rendering.
use crate::{
    color_config::{
        ColorConfigError, ColorSelection, ColorVersion, ResolvedColorOutput, VerifiedColorConfig,
    },
    color_processor::{
        ColorProcessorPhase, ColorProcessorSource, ColorProcessorTable, TableInterpolation,
        COLOR_PROCESSOR_VERSION,
    },
    AlphaMode,
};
use libloading::Library;
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    path::{Component, Path},
    ptr::NonNull,
    sync::{Arc, Mutex},
};
mod catalogue;
mod resources;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("native OCIO phase=catalogue field={field} name={name:?} expected={expected:?} actual={actual:?}")]
    Catalogue {
        field: &'static str,
        name: String,
        expected: String,
        actual: String,
    },
    #[error("native OCIO initialization lock poisoned")]
    Initialization(#[source] std::sync::PoisonError<()>),
    #[error("native OCIO phase={phase} path={path}")]
    Io {
        phase: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("native OCIO manifest JSON")]
    Json(#[from] serde_json::Error),
    #[error("native OCIO admission: {0}")]
    Admission(&'static str),
    #[error("native OCIO phase={phase} path={path}")]
    Load {
        phase: &'static str,
        path: String,
        #[source]
        source: libloading::Error,
    },
    #[error(transparent)]
    Config(#[from] ColorConfigError),
    #[error("native OCIO resource={name:?}: {reason}")]
    Resource { name: String, reason: &'static str },
    #[error("native OCIO resource={name:?}: {source}")]
    ResourceLut {
        name: String,
        #[source]
        source: crate::color_lut::LutError,
    },
    #[error("native OCIO phase={phase} category={category}: {cause}")]
    Native {
        phase: &'static str,
        category: u32,
        cause: String,
    },
}
type Result<T> = std::result::Result<T, Error>;
fn io(phase: &'static str, path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        phase,
        path: path.display().to_string(),
        source,
    }
}
fn load_error(phase: &'static str, path: &Path, source: libloading::Error) -> Error {
    Error::Load {
        phase,
        path: path.display().to_string(),
        source,
    }
}
unsafe fn open_library(path: &Path, phase: &'static str) -> Result<Library> {
    unsafe { Library::new(path) }.map_err(|source| load_error(phase, path, source))
}
unsafe fn symbol<T: Copy>(
    library: &Library,
    path: &Path,
    name: &'static [u8],
    phase: &'static str,
) -> Result<T> {
    unsafe { library.get::<T>(name) }
        .map(|value| *value)
        .map_err(|source| load_error(phase, path, source))
}
#[repr(C)]
struct NativeError {
    kind: u32,
    message: [c_char; 2048],
}
impl NativeError {
    fn empty() -> Self {
        Self {
            kind: 0,
            message: [0; 2048],
        }
    }
    fn check(&self, ok: bool, phase: &'static str) -> Result<()> {
        if ok {
            return Ok(());
        }
        let bytes: Vec<u8> = self
            .message
            .iter()
            .take_while(|v| **v != 0)
            .map(|v| *v as u8)
            .collect();
        Err(Error::Native {
            phase,
            category: self.kind,
            cause: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }
}
#[repr(C)]
#[derive(Default)]
struct NativeTable {
    texture: *const c_char,
    sampler: *const c_char,
    binding: u32,
    width: u32,
    height: u32,
    depth: u32,
    channels: u32,
    interpolation: u32,
    values: *const f32,
    count: usize,
}
type Prepare = unsafe extern "C" fn(
    *const u8,
    usize,
    *const c_char,
    *const c_char,
    *const c_char,
    *const resources::Resource,
    usize,
    *mut *mut c_void,
    *mut NativeError,
) -> bool;
type Shader = unsafe extern "C" fn(
    *const c_void,
    *mut *const c_char,
    *mut usize,
    *mut u32,
    *mut NativeError,
) -> bool;
type Table = unsafe extern "C" fn(*const c_void, u32, *mut NativeTable, *mut NativeError) -> bool;
type Apply = unsafe extern "C" fn(*const c_void, *mut f32, *mut NativeError) -> bool;
type Release = unsafe extern "C" fn(*mut c_void);

struct Api {
    prepare: Prepare,
    shader: Shader,
    table: Table,
    apply: Apply,
    release: Release,
    catalogue: catalogue::Query,
    // Libraries outlive all copied function pointers and native program handles.
    _bridge: Library,
    _ocio: Library,
}
// OCIO's global std::function manager lives in the bridge. Pin one runtime for
// the process instead of unloading that callback's code with a project.
static RUNTIME: Mutex<Option<(std::path::PathBuf, Arc<Api>)>> = Mutex::new(None);

/// Library handles are retained by every program. No C++ allocation is freed by
/// Rust, and no raw pointer or runtime function is exposed to callers.
#[derive(Clone)]
pub struct NativeColorRuntime(Arc<Api>);
/// An exclusively owned native program may move between workers. It cannot be
/// shared concurrently or cloned; no borrowed native handle is exposed.
///
/// ```compile_fail
/// fn require_sync<T: Sync>() {}
/// require_sync::<motion_effects::native_color::NativeColorProgram>();
/// ```
pub struct NativeColorProgram {
    raw: NonNull<c_void>,
    api: Arc<Api>,
    source: ColorProcessorSource,
}
// SAFETY: bridge.cpp publishes one heap-owned Program after preparation. It
// owns OCIO's CPU processor, shader-description data and catalogue strings, not
// a GPU device/context or a thread-affine platform handle. read_source copies
// all exported text/tables before Rust publishes the program; callers have no
// raw handle, native mutator or Clone. A move transfers that exclusive owner,
// and Arc<Api> pins both libraries through the eventual C++ release in Drop.
// Only Send is promised: NonNull keeps the program !Sync, preventing concurrent
// calls through shared references. See native-ocio-ownership.md for the pinned
// implementation review and actual native transfer/drop qualification.
unsafe impl Send for NativeColorProgram {}
impl Drop for NativeColorProgram {
    fn drop(&mut self) {
        unsafe { (self.api.release)(self.raw.as_ptr()) }
    }
}
unsafe extern "C" fn warning(message: *const c_char, size: usize) {
    if message.is_null() {
        return;
    }
    // Bridge bounds the callback to 2048 bytes. Never unwind across C ABI.
    let text = String::from_utf8_lossy(unsafe {
        std::slice::from_raw_parts(message.cast::<u8>(), size.min(2048))
    });
    if std::panic::catch_unwind(|| log::warn!("native OCIO: {}", text)).is_err() {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr().lock(),
            "native OCIO warning logger panicked: {text}"
        );
    }
}

fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| io("open", path, e))?;
    let mut data = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| io("read", path, e))?;
    if data.len() > limit {
        return Err(Error::Admission("manifest exceeds capacity"));
    }
    Ok(data)
}
fn verify_file(root: &Path, value: &serde_json::Value) -> Result<PathBufOwned> {
    use std::io::Read;
    let name = value["path"]
        .as_str()
        .ok_or(Error::Admission("missing runtime path"))?;
    let path = Path::new(name);
    if name.contains(['\\', ':'])
        || path
            .components()
            .any(|v| !matches!(v, Component::Normal(_)))
        || name.is_empty()
    {
        return Err(Error::Admission(
            "runtime path is not canonical and relative",
        ));
    }
    let joined = root.join(path);
    let actual = joined
        .canonicalize()
        .map_err(|e| io("canonicalize", &joined, e))?;
    if !actual.starts_with(root) {
        return Err(Error::Admission("runtime path escapes its prefix"));
    }
    let mut file = std::fs::File::open(&actual).map_err(|e| io("verify", &actual, e))?;
    if file
        .metadata()
        .map_err(|e| io("metadata", &actual, e))?
        .len()
        > 64 * 1024 * 1024
    {
        return Err(Error::Admission("runtime file exceeds capacity"));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| io("hash", &actual, e))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if value["sha256"].as_str() != Some(format!("{:x}", digest.finalize()).as_str()) {
        return Err(Error::Admission("runtime file checksum mismatch"));
    }
    Ok(actual)
}
type PathBufOwned = std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_loader_failure_keeps_the_library_path_and_original_cause() {
        use std::error::Error as _;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("invalid-native-color-library");
        std::fs::write(&path, b"independently authored non-library fixture").unwrap();
        let error = unsafe { open_library(&path, "load-core") }.err().unwrap();
        assert!(error.source().is_some());
        let Error::Load {
            phase,
            path: actual,
            source,
        } = error
        else {
            panic!("original dynamic loader error");
        };
        assert_eq!(phase, "load-core");
        assert_eq!(actual, path.display().to_string());
        assert!(!source.to_string().is_empty());
    }
    #[test]
    fn actual_missing_symbol_keeps_its_operation_path_and_original_cause() {
        use std::error::Error as _;
        #[cfg(windows)]
        let library: Library = libloading::os::windows::Library::this().unwrap().into();
        #[cfg(unix)]
        let library: Library = libloading::os::unix::Library::this().into();
        let path = std::env::current_exe().unwrap();
        let error = unsafe {
            symbol::<unsafe extern "C" fn()>(
                &library,
                &path,
                b"motion_nonexistent_native_color_export\0",
                "symbol:motion_nonexistent_native_color_export",
            )
        }
        .unwrap_err();
        assert!(error.source().is_some());
        let Error::Load {
            phase,
            path: actual,
            source,
        } = error
        else {
            panic!("original symbol error");
        };
        assert_eq!(phase, "symbol:motion_nonexistent_native_color_export");
        assert_eq!(actual, path.display().to_string());
        assert!(!source.to_string().is_empty());
    }
    #[test]
    fn runtime_hash_and_path_admission_precede_dynamic_loading() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let file = root.join("owned.dll");
        std::fs::write(&file, b"independent byte identity fixture").unwrap();
        let expected = format!("{:x}", Sha256::digest(std::fs::read(&file).unwrap()));
        let good = serde_json::json!({"path":"owned.dll", "sha256":expected});
        assert_eq!(verify_file(&root, &good).unwrap(), file);
        std::fs::write(&file, b"changed after inventory").unwrap();
        assert!(matches!(
            verify_file(&root, &good),
            Err(Error::Admission("runtime file checksum mismatch"))
        ));
        for name in [
            "../owned.dll",
            "C:/owned.dll",
            "sub\\owned.dll",
            "",
            "/owned.dll",
        ] {
            let item = serde_json::json!({"path":name, "sha256":expected});
            assert!(
                matches!(
                    verify_file(&root, &item),
                    Err(Error::Admission(
                        "runtime path is not canonical and relative"
                    ))
                ),
                "{name}"
            );
        }
    }
    #[test]
    fn stale_platform_or_source_identity_is_rejected_before_library_load() {
        let directory = tempfile::tempdir().unwrap();
        let manifest = directory.path().join("ocio-runtime.json");
        let lock: serde_json::Value =
            serde_json::from_slice(include_bytes!("../../../tools/ocio_sources.json")).unwrap();
        for (version, platform, components) in [
            ("2.5.0", "windows-x86_64", lock["components"].clone()),
            ("2.6.0", "unknown-platform", lock["components"].clone()),
            ("2.6.0", "windows-x86_64", serde_json::json!([])),
        ] {
            std::fs::write(&manifest, serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"version":version,"platform":platform,"components":components})).unwrap()).unwrap();
            assert!(matches!(
                NativeColorRuntime::load(&manifest),
                Err(Error::Admission(
                    "runtime version, platform or source identity differs"
                ))
            ));
        }
    }
}

impl NativeColorRuntime {
    /// Load only a caller-selected content-verified prefix. There is no PATH,
    /// Python, environment-config or installed-library fallback.
    pub fn load(manifest: &Path) -> Result<Self> {
        let path = manifest
            .canonicalize()
            .map_err(|e| io("manifest", manifest, e))?;
        let root = path
            .parent()
            .ok_or(Error::Admission("manifest has no parent"))?;
        let value: serde_json::Value = serde_json::from_slice(&read(&path, 65536)?)?;
        let lock: serde_json::Value =
            serde_json::from_slice(include_bytes!("../../../tools/ocio_sources.json"))?;
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => "windows-x86_64",
            ("linux", "x86_64") => "linux-x86_64",
            ("macos", "x86_64") => "macos-x86_64",
            ("macos", "aarch64") => "macos-arm64",
            _ => return Err(Error::Admission("unsupported native platform")),
        };
        let components: Vec<_> = lock["components"]
            .as_array()
            .ok_or(Error::Admission("invalid compiled source lock"))?
            .iter()
            .filter(|component| {
                component.get("platforms").is_none_or(|platforms| {
                    platforms
                        .as_array()
                        .is_some_and(|items| items.iter().any(|item| item == platform))
                })
            })
            .collect();
        if value["schemaVersion"] != 1
            || value["version"] != "2.6.0"
            || value["platform"] != platform
            || value["components"] != serde_json::to_value(&components)?
        {
            return Err(Error::Admission(
                "runtime version, platform or source identity differs",
            ));
        }
        let bridge_hash = format!(
            "{:x}",
            Sha256::digest(include_bytes!("../../../tools/ocio-native/bridge.cpp"))
        );
        if value["bridgeSourceSha256"] != bridge_hash {
            return Err(Error::Admission("runtime bridge source differs"));
        }
        let entries = value["files"]
            .as_array()
            .ok_or(Error::Admission("runtime files missing"))?;
        let licenses = value["licenses"]
            .as_array()
            .ok_or(Error::Admission("runtime licenses missing"))?;
        if !(2..=8).contains(&entries.len()) || licenses.len() != components.len() {
            return Err(Error::Admission(
                "runtime library or license set incomplete",
            ));
        }
        for component in components {
            let expected = format!(
                "licenses/ocio/{}-LICENSE.txt",
                component["name"]
                    .as_str()
                    .ok_or(Error::Admission("invalid component name"))?
            );
            let item = licenses
                .iter()
                .find(|item| item["path"] == expected)
                .ok_or(Error::Admission("dependency license missing"))?;
            if item["sha256"] != component["license_sha256"] {
                return Err(Error::Admission(
                    "dependency license differs from pinned source",
                ));
            }
            verify_file(root, item)?;
        }
        let mut ocio = None;
        let mut bridge = None;
        for item in entries {
            let file = verify_file(root, item)?;
            let name = file
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or(Error::Admission("invalid library filename"))?;
            if name.starts_with("OpenColorIO") || name.starts_with("libOpenColorIO") {
                ocio = Some(file);
            } else if name.starts_with("motion_ocio_bridge")
                || name.starts_with("libmotion_ocio_bridge")
            {
                bridge = Some(file);
            } else {
                return Err(Error::Admission("unexpected runtime library"));
            }
        }
        let ocio = ocio.ok_or(Error::Admission("OCIO library missing"))?;
        let bridge = bridge.ok_or(Error::Admission("bridge library missing"))?;
        let mut runtime = RUNTIME
            .lock()
            .map_err(|_| Error::Initialization(std::sync::PoisonError::new(())))?;
        if let Some((loaded, api)) = runtime.as_ref() {
            if loaded != &path {
                return Err(Error::Admission(
                    "native runtime was already initialized from another prefix",
                ));
            }
            return Ok(Self(api.clone()));
        }
        // SAFETY: Both explicitly selected native libraries have been hash and
        // source-identity verified. ABI functions are independently owned above.
        unsafe {
            let ocio_library = open_library(&ocio, "load-core")?;
            let bridge_library = open_library(&bridge, "load-bridge")?;
            let initialize = symbol::<
                unsafe extern "C" fn(
                    unsafe extern "C" fn(*const c_char, usize),
                    *mut NativeError,
                ) -> bool,
            >(
                &bridge_library,
                &bridge,
                b"motion_ocio_initialize\0",
                "symbol:motion_ocio_initialize",
            )?;
            let api = Api {
                prepare: symbol(
                    &bridge_library,
                    &bridge,
                    b"motion_ocio_prepare\0",
                    "symbol:motion_ocio_prepare",
                )?,
                shader: symbol(
                    &bridge_library,
                    &bridge,
                    b"motion_ocio_shader\0",
                    "symbol:motion_ocio_shader",
                )?,
                table: symbol(
                    &bridge_library,
                    &bridge,
                    b"motion_ocio_table\0",
                    "symbol:motion_ocio_table",
                )?,
                apply: symbol(
                    &bridge_library,
                    &bridge,
                    b"motion_ocio_apply_rgb\0",
                    "symbol:motion_ocio_apply_rgb",
                )?,
                release: symbol(
                    &bridge_library,
                    &bridge,
                    b"motion_ocio_release\0",
                    "symbol:motion_ocio_release",
                )?,
                catalogue: symbol(
                    &bridge_library,
                    &bridge,
                    b"motion_ocio_catalogue_row\0",
                    "symbol:motion_ocio_catalogue_row",
                )?,
                _bridge: bridge_library,
                _ocio: ocio_library,
            };
            // All symbols are resolved before registering the global callback.
            // The pinned initializer emits no log/callback under this mutex.
            let mut error = NativeError::empty();
            let ok = initialize(warning, &mut error);
            error.check(ok, "initialize")?;
            let api = Arc::new(api);
            *runtime = Some((path, api.clone()));
            Ok(Self(api))
        }
    }
    pub fn prepare(
        &self,
        config: &VerifiedColorConfig,
        selection: &ColorSelection,
        phase: ColorProcessorPhase,
        input_alpha: AlphaMode,
        output_alpha: AlphaMode,
    ) -> Result<NativeColorProgram> {
        if !config.manifest().context.is_empty() {
            return Err(Error::Admission(
                "requires a closed catalogue without ambient context",
            ));
        }
        let resources = resources::Resources::new(config)?;
        let native_resources = resources.native();
        let resolved = config.resolve(selection)?;
        let (from, to, view) = match phase {
            ColorProcessorPhase::InputToWorking => {
                (resolved.input.as_str(), resolved.working.as_str(), None)
            }
            ColorProcessorPhase::WorkingToDisplay => (
                resolved.working.as_str(),
                resolved.display.display.as_str(),
                Some(resolved.display.view.as_str()),
            ),
            ColorProcessorPhase::WorkingToOutput => match &resolved.output {
                ResolvedColorOutput::Space(space) => {
                    (resolved.working.as_str(), space.as_str(), None)
                }
                ResolvedColorOutput::Display(display) => (
                    resolved.working.as_str(),
                    display.display.as_str(),
                    Some(display.view.as_str()),
                ),
            },
        };
        let string = |v| CString::new(v).map_err(|_| Error::Admission("NUL in selection"));
        let from = string(from)?;
        let to = string(to)?;
        let view = view.map(string).transpose()?;
        let mut raw = std::ptr::null_mut();
        let mut error = NativeError::empty();
        let ok = unsafe {
            (self.0.prepare)(
                config.config_bytes().as_ptr(),
                config.config_bytes().len(),
                from.as_ptr(),
                to.as_ptr(),
                view.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                native_resources.as_ptr(),
                native_resources.len(),
                &mut raw,
                &mut error,
            )
        };
        error.check(ok, "prepare")?;
        let raw = NonNull::new(raw).ok_or(Error::Admission(
            "native preparation returned a null program",
        ))?;
        let mut program = NativeColorProgram {
            raw,
            api: self.0.clone(),
            source: ColorProcessorSource {
                version: COLOR_PROCESSOR_VERSION,
                ocio_version: ColorVersion {
                    major: 2,
                    minor: 6,
                    patch: 0,
                },
                config_key: config.key(),
                selection_key: resolved.key,
                phase,
                input_alpha,
                output_alpha,
                shader: String::new(),
                tables: Vec::new(),
            },
        };
        catalogue::verify(&program, config.manifest())?;
        program.read_source()?;
        Ok(program)
    }
}
impl NativeColorProgram {
    fn read_source(&mut self) -> Result<()> {
        let mut text = std::ptr::null();
        let mut size = 0;
        let mut count = 0;
        let mut error = NativeError::empty();
        let ok = unsafe {
            (self.api.shader)(
                self.raw.as_ptr(),
                &mut text,
                &mut size,
                &mut count,
                &mut error,
            )
        };
        error.check(ok, "shader")?;
        if text.is_null() || size > 256 * 1024 || count > 4 {
            return Err(Error::Admission("native shader exceeds capacity"));
        }
        self.source.shader =
            std::str::from_utf8(unsafe { std::slice::from_raw_parts(text.cast(), size) })
                .map_err(|_| Error::Admission("native shader is not UTF-8"))?
                .to_owned();
        for index in 0..count {
            let mut table = NativeTable::default();
            let ok = unsafe { (self.api.table)(self.raw.as_ptr(), index, &mut table, &mut error) };
            error.check(ok, "table")?;
            if table.values.is_null()
                || table.texture.is_null()
                || table.sampler.is_null()
                || table.count > 262144 * 3
            {
                return Err(Error::Admission("native table exceeds capacity"));
            }
            // Native owner retains all borrowed table bytes through this copy.
            let name = |ptr| {
                unsafe { CStr::from_ptr(ptr) }
                    .to_str()
                    .map(str::to_owned)
                    .map_err(|_| Error::Admission("table name is not UTF-8"))
            };
            self.source.tables.push(ColorProcessorTable {
                texture_name: name(table.texture)?,
                sampler_name: name(table.sampler)?,
                binding: table.binding,
                width: table.width,
                height: table.height,
                depth: table.depth,
                channels: table.channels,
                interpolation: match table.interpolation {
                    0 => TableInterpolation::Nearest,
                    1 => TableInterpolation::Linear,
                    _ => {
                        return Err(Error::Admission(
                            "native table interpolation is unsupported",
                        ))
                    }
                },
                values: unsafe { std::slice::from_raw_parts(table.values, table.count) }.to_vec(),
            });
        }
        Ok(())
    }
    pub fn source(&self) -> &ColorProcessorSource {
        &self.source
    }
    pub fn apply_rgb(&self, mut rgb: [f32; 3]) -> Result<[f32; 3]> {
        if rgb.iter().any(|v| !v.is_finite()) {
            return Err(Error::Admission("nonfinite authored RGB input"));
        }
        let mut error = NativeError::empty();
        let ok = unsafe { (self.api.apply)(self.raw.as_ptr(), rgb.as_mut_ptr(), &mut error) };
        error.check(ok, "cpu-rgb")?;
        if rgb.iter().any(|v| !v.is_finite()) {
            return Err(Error::Admission("native RGB output is nonfinite"));
        }
        Ok(rgb)
    }
}
