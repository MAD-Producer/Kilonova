//! Explicit CPU vector ownership. Capacity is not resident process memory.
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuBufferAllocation {
    pub id: String,
    pub category: &'static str,
    pub payload_bytes: u64,
    pub capacity_bytes: u64,
    pub references: Vec<Value>,
}
impl CpuBufferAllocation {
    pub(crate) fn vector<T>(id: String, category: &'static str, data: &Vec<T>) -> Self {
        Self {
            id,
            category,
            payload_bytes: data.len() as u64 * std::mem::size_of::<T>() as u64,
            capacity_bytes: data.capacity() as u64 * std::mem::size_of::<T>() as u64,
            references: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuMemoryInfo {
    pub scope: &'static str,
    pub known_payload_bytes: u64,
    pub known_capacity_bytes: u64,
    pub resources: Vec<CpuBufferAllocation>,
    pub complete_coverage: bool,
    #[serde(flatten)]
    pub details: Value,
}
impl CpuMemoryInfo {
    pub(crate) fn new(
        scope: &'static str,
        resources: Vec<CpuBufferAllocation>,
        details: Value,
    ) -> Self {
        Self {
            scope,
            known_payload_bytes: resources.iter().map(|r| r.payload_bytes).sum(),
            known_capacity_bytes: resources.iter().map(|r| r.capacity_bytes).sum(),
            resources,
            complete_coverage: false,
            details,
        }
    }
}
