use super::*;
use crate::shader::CompiledShader;

const MAX_SHADERS: usize = 4096;
const MAX_CONTEXTS: usize = 8;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderMemory {
    pub id: usize,
    pub program_references: u64,
    pub programs: Vec<String>,
    pub omitted_program_contexts: u64,
    pub text: BufferMemory,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderMemoryInfo {
    pub version: u32,
    pub scope: &'static str,
    pub known_payload_bytes: u64,
    pub known_capacity_bytes: u64,
    pub resources: Vec<ShaderMemory>,
    pub unobserved_references: u64,
    pub all_program_owners_observed: bool,
    pub complete_coverage: bool,
    pub unknown_bytes: Option<u64>,
    pub excludes: [&'static str; 3],
}

/// Package shader structs and independently owned program shader Arcs are
/// distinct allocations. Deduplicate the latter by actual Arc identity only.
pub fn compiled_shader_memory<'a>(
    owners: impl IntoIterator<Item = (&'a str, &'a Arc<CompiledShader>)>,
) -> ShaderMemoryInfo {
    let mut identities = HashMap::new();
    let mut resources: Vec<ShaderMemory> = Vec::new();
    let mut unobserved = 0u64;
    for (key, shader) in owners {
        let identity = Arc::as_ptr(shader);
        let index = if let Some(index) = identities.get(&identity) {
            *index
        } else {
            if resources.len() == MAX_SHADERS {
                unobserved = unobserved.saturating_add(1);
                continue;
            }
            let index = resources.len();
            resources.push(ShaderMemory {
                id: index + 1,
                program_references: 0,
                programs: Vec::new(),
                omitted_program_contexts: 0,
                text: BufferMemory::shader(shader),
            });
            identities.insert(identity, index);
            index
        };
        let resource = &mut resources[index];
        resource.program_references = resource.program_references.saturating_add(1);
        let context: String = key.chars().take(160).collect();
        if !resource.programs.contains(&context) {
            if resource.programs.len() < MAX_CONTEXTS {
                resource.programs.push(context);
            } else {
                resource.omitted_program_contexts =
                    resource.omitted_program_contexts.saturating_add(1);
            }
        }
    }
    ShaderMemoryInfo {
        version: 1,
        scope: "retained_program_shader_text_vectors",
        known_payload_bytes: resources.iter().map(|r| r.text.payload_bytes).sum(),
        known_capacity_bytes: resources.iter().map(|r| r.text.capacity_bytes).sum(),
        resources,
        unobserved_references: unobserved,
        all_program_owners_observed: unobserved == 0,
        complete_coverage: false,
        unknown_bytes: None,
        excludes: [
            "map_keys_Arc_allocator_and_container_overhead",
            "external_shader_owners",
            "native_GPU_modules_and_driver_storage",
        ],
    }
}

#[cfg(test)]
mod tests;
