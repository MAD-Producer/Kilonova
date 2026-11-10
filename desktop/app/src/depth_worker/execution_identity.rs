//! Identity of the actual admitted native execution, before cache lookup and
//! again in the inference child. This reads owned executable files and public
//! adapter metadata; it neither dispatches a model nor inspects process memory.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    pub backend: String,
    pub provider: String,
    pub precision: String,
    pub runtime_sha256: [u8; 32],
    pub plan_sha256: [u8; 32],
}

fn program_digest(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid native execution program digest".into());
    }
    let mut result = [0; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(result)
}

pub fn worker_digest(path: &Path) -> Result<[u8; 32]> {
    let mut file = File::open(path).map_err(|e| format!("open native worker identity: {e}"))?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size == 0 || size > 512 << 20 {
        return Err("native worker identity requires a nonempty file up to 512 MiB".into());
    }
    let mut hash = Sha256::new();
    let mut remaining = size;
    let mut chunk = [0u8; 65536];
    while remaining != 0 {
        let take = remaining.min(chunk.len() as u64) as usize;
        file.read_exact(&mut chunk[..take])
            .map_err(|e| format!("native worker changed during identity read: {e}"))?;
        hash.update(&chunk[..take]);
        remaining -= take as u64;
    }
    if file.read(&mut chunk[..1]).map_err(|e| e.to_string())? != 0 {
        return Err("native worker grew during identity read".into());
    }
    Ok(hash.finalize().into())
}

fn bounded_environment(
    values: impl IntoIterator<Item = (String, String)>,
) -> Result<BTreeMap<String, String>> {
    let mut environment = BTreeMap::new();
    for (key, value) in values {
        if numerical_key(&key) {
            if key.len() > 128 || value.len() > 1024 || environment.len() >= 64 {
                return Err("unbounded native numerical environment".into());
            }
            environment.insert(key, value);
        }
    }
    Ok(environment)
}

fn numerical_key(key: &str) -> bool {
    ["VK_", "WGPU_", "D3D_", "DXVK_", "MESA_"]
        .iter()
        .any(|prefix| key.starts_with(prefix))
}

fn current_environment() -> Result<BTreeMap<String, String>> {
    let mut values = Vec::new();
    for (key, value) in std::env::vars_os() {
        if numerical_key(&key.to_string_lossy()) {
            let key = key
                .into_string()
                .map_err(|_| "non-Unicode native numerical environment key")?;
            let value = value
                .into_string()
                .map_err(|_| "non-Unicode native numerical environment value")?;
            if values.len() >= 64 {
                return Err("unbounded native numerical environment".into());
            }
            values.push((key, value));
        }
    }
    bounded_environment(values)
}

pub fn resolve(
    program: &str,
    worker: &Path,
    adapter: &wgpu::AdapterInfo,
    limits: &wgpu::Limits,
) -> Result<Execution> {
    if !matches!(
        adapter.device_type,
        wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu
    ) || !matches!(
        adapter.backend,
        wgpu::Backend::Vulkan | wgpu::Backend::Dx12 | wgpu::Backend::Metal
    ) {
        return Err("persistent WGSL identity requires an explicit hardware backend".into());
    }
    for (value, maximum) in [
        (&adapter.name, 96),
        (&adapter.driver, 1024),
        (&adapter.driver_info, 1024),
        (&adapter.device_pci_bus_id, 128),
    ] {
        if value.len() > maximum || value.chars().any(char::is_control) {
            return Err("unbounded native adapter identity".into());
        }
    }
    if adapter.name.is_empty() || (adapter.driver.is_empty() && adapter.driver_info.is_empty()) {
        return Err(
            "native adapter exposes no usable name/driver identity for persistent caching".into(),
        );
    }
    let environment = current_environment()?;
    let worker_sha256 = worker_digest(worker)?;
    let context = serde_json::json!({"worker_sha256":worker_sha256,"adapter_capabilities":format!("{adapter:?}"),
        "adapter":{"name":adapter.name,"vendor":adapter.vendor,"device":adapter.device,
            "device_type":format!("{:?}",adapter.device_type),"backend":format!("{:?}",adapter.backend),
            "driver":adapter.driver,"driver_info":adapter.driver_info},
        "required_features":"empty","required_limits":format!("{limits:?}"),
        "environment":environment,"architecture":std::env::consts::ARCH,"os":std::env::consts::OS});
    let bytes = serde_json::to_vec(&context).map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("native execution context exceeds 64 KiB".into());
    }
    let mut hash = Sha256::new();
    hash.update(b"MotionStudio/depth/wgsl-native-runtime/v1\0");
    hash.update(bytes);
    Ok(Execution {
        backend: "wgsl".into(),
        provider: format!("WGSL:{:?}:{}", adapter.backend, adapter.name),
        precision: "float32".into(),
        runtime_sha256: hash.finalize().into(),
        plan_sha256: program_digest(program)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> wgpu::AdapterInfo {
        let mut result =
            wgpu::AdapterInfo::new(wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan);
        result.name = "authored GPU".into();
        result.driver = "authored driver 1".into();
        result
    }

    #[test]
    fn actual_worker_driver_backend_limits_and_program_participate_in_identity() {
        let directory = tempfile::tempdir().unwrap();
        let worker = directory.path().join("owned-worker");
        std::fs::write(&worker, b"owned executable control").unwrap();
        let info = adapter();
        let limits = wgpu::Limits::downlevel_defaults();
        let first = resolve(&"ab".repeat(32), &worker, &info, &limits).unwrap();
        assert_eq!(
            first,
            resolve(&"ab".repeat(32), &worker, &info, &limits).unwrap()
        );
        let mut changed = info.clone();
        changed.driver = "authored driver 2".into();
        assert_ne!(
            first.runtime_sha256,
            resolve(&"ab".repeat(32), &worker, &changed, &limits)
                .unwrap()
                .runtime_sha256
        );
        let mut changed = limits.clone();
        changed.max_buffer_size += 4;
        assert_ne!(
            first.runtime_sha256,
            resolve(&"ab".repeat(32), &worker, &info, &changed)
                .unwrap()
                .runtime_sha256
        );
        std::fs::write(&worker, b"changed executable control").unwrap();
        assert_ne!(
            first.runtime_sha256,
            resolve(&"ab".repeat(32), &worker, &info, &limits)
                .unwrap()
                .runtime_sha256
        );
        assert_ne!(
            first.plan_sha256,
            resolve(&"cd".repeat(32), &worker, &info, &limits)
                .unwrap()
                .plan_sha256
        );
        let mut changed = info;
        changed.backend = wgpu::Backend::Dx12;
        let dx12 = resolve(&"ab".repeat(32), &worker, &changed, &limits).unwrap();
        assert_ne!(first.provider, dx12.provider);
    }

    #[test]
    fn missing_driver_and_software_devices_are_not_cache_attestable() {
        let missing = Path::new("missing-worker");
        let limits = wgpu::Limits::downlevel_defaults();
        let mut info = adapter();
        info.driver.clear();
        assert!(
            resolve(&"ab".repeat(32), missing, &info, &limits)
                .unwrap_err()
                .contains("driver identity")
        );
        info.driver = "owned driver".into();
        info.device_type = wgpu::DeviceType::Cpu;
        assert!(
            resolve(&"ab".repeat(32), missing, &info, &limits)
                .unwrap_err()
                .contains("hardware backend")
        );
    }

    #[test]
    fn numerical_environment_is_bounded_sorted_and_does_not_retain_other_values() {
        let first = bounded_environment([
            ("WGPU_TEST".into(), "1".into()),
            ("PATH".into(), "private".into()),
            ("VK_TEST".into(), "2".into()),
        ])
        .unwrap();
        let second = bounded_environment([
            ("VK_TEST".into(), "2".into()),
            ("WGPU_TEST".into(), "1".into()),
        ])
        .unwrap();
        assert_eq!(first, second);
        assert!(!first.contains_key("PATH"));
        assert!(bounded_environment([("VK_TEST".into(), "x".repeat(1025))]).is_err());
        assert!(bounded_environment((0..65).map(|n| (format!("VK_{n}"), "1".into()))).is_err());
    }

    #[test]
    fn execution_receipts_reject_unknown_fields_and_invalid_hashes() {
        let value = serde_json::json!({"backend":"wgsl","provider":"WGSL:Vulkan:authored",
            "precision":"float32","runtime_sha256":vec![1u8;32],"plan_sha256":vec![2u8;32]});
        let mut bad = value.clone();
        bad["extra"] = true.into();
        assert!(serde_json::from_value::<Execution>(bad).is_err());
        let mut bad = value;
        bad["runtime_sha256"] = serde_json::json!(vec![true; 32]);
        assert!(serde_json::from_value::<Execution>(bad).is_err());
        for invalid in [
            "".to_owned(),
            "A".repeat(64),
            "z".repeat(64),
            "a".repeat(63),
        ] {
            assert!(program_digest(&invalid).is_err());
        }
        assert_eq!(program_digest(&"ab".repeat(32)).unwrap(), [0xab; 32]);
    }
}
