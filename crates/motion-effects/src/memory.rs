//! Explicit package ownership metadata, never resident process/GPU memory.
use crate::EffectPackage;
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};

const MAX_PACKAGES: usize = 1024;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageOwner {
    Registry,
    PlanProgram,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BufferMemory {
    pub payload_bytes: u64,
    pub capacity_bytes: u64,
}
impl BufferMemory {
    fn add(&mut self, len: usize, capacity: usize) {
        self.payload_bytes += len as u64;
        self.capacity_bytes += capacity as u64;
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageMemory {
    pub id: usize,
    pub plugin: String,
    pub hash: String,
    pub registry_references: u64,
    pub program_references: u64,
    pub archive: BufferMemory,
    pub expanded_files: BufferMemory,
    pub compiled_shader_text: BufferMemory,
    pub file_count: usize,
    pub shader_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageMemoryInfo {
    pub version: u32,
    pub scope: &'static str,
    pub known_payload_bytes: u64,
    pub known_capacity_bytes: u64,
    pub resources: Vec<PackageMemory>,
    pub unobserved_references: u64,
    pub all_package_owners_observed: bool,
    pub complete_coverage: bool,
    pub unknown_bytes: Option<u64>,
    pub excludes: [&'static str; 4],
}

/// Borrowed owners keep identities alive throughout this query. Equal hashes
/// are not allocation identity; independent parses own independent buffers.
/// No package, pointer key or observation map is retained after returning.
pub fn package_memory<'a>(
    owners: impl IntoIterator<Item = (PackageOwner, &'a Arc<EffectPackage>)>,
) -> PackageMemoryInfo {
    let mut identities = HashMap::new();
    let mut resources: Vec<PackageMemory> = Vec::new();
    let mut unobserved = 0u64;
    for (owner, package) in owners {
        let identity = Arc::as_ptr(package);
        let index = if let Some(index) = identities.get(&identity) {
            *index
        } else {
            if resources.len() == MAX_PACKAGES {
                unobserved = unobserved.saturating_add(1);
                continue;
            }
            let mut archive = BufferMemory::default();
            archive.add(package.bytes.len(), package.bytes.capacity());
            let mut files = BufferMemory::default();
            for bytes in package.files.values() {
                files.add(bytes.len(), bytes.capacity());
            }
            let mut text = BufferMemory::default();
            for shader in package.shaders.values() {
                for source in [&shader.wgsl, &shader.glsl.vertex, &shader.glsl.fragment] {
                    text.add(source.len(), source.capacity());
                }
            }
            let index = resources.len();
            resources.push(PackageMemory {
                id: index + 1,
                plugin: package.manifest.id.chars().take(160).collect(),
                hash: package.hash.chars().take(160).collect(),
                registry_references: 0,
                program_references: 0,
                archive,
                expanded_files: files,
                compiled_shader_text: text,
                file_count: package.files.len(),
                shader_count: package.shaders.len(),
            });
            identities.insert(identity, index);
            index
        };
        match owner {
            PackageOwner::Registry => resources[index].registry_references += 1,
            PackageOwner::PlanProgram => resources[index].program_references += 1,
        }
    }
    let mut payload = 0;
    let mut capacity = 0;
    for package in &resources {
        for buffer in [
            &package.archive,
            &package.expanded_files,
            &package.compiled_shader_text,
        ] {
            payload += buffer.payload_bytes;
            capacity += buffer.capacity_bytes;
        }
    }
    PackageMemoryInfo {
        version: 1,
        scope: "observed_package_owned_archive_files_and_shader_text",
        known_payload_bytes: payload,
        known_capacity_bytes: capacity,
        resources,
        unobserved_references: unobserved,
        all_package_owners_observed: unobserved == 0,
        complete_coverage: false,
        unknown_bytes: None,
        excludes: [
            "manifest_and_map_key_storage",
            "allocator_Arc_and_container_overhead",
            "external_package_and_compiled_shader_owners",
            "native_GPU_modules_and_driver_storage",
        ],
    }
}

#[cfg(test)]
mod tests;
