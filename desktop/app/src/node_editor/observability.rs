use super::NodeEditor;
use std::{collections::BTreeMap, fmt, sync::Mutex};

// Keys are fixed source sites, never document IDs or arbitrary input strings.
#[derive(Default)]
pub(super) struct Observations(Mutex<BTreeMap<&'static str, usize>>);

impl NodeEditor {
    pub(super) fn observe_failure(&self, site: &'static str, context: fmt::Arguments<'_>) {
        let count = {
            let mut counts = self
                .observations
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let count = counts.entry(site).or_default();
            *count = count.saturating_add(1);
            *count
        };
        if count == 1 {
            log::warn!("node degradation site={site} epoch={} revision={} preview_revision={} count=1 {context}", self.epoch, self.revision, self.preview_revision);
        } else if count == 60 {
            log::error!("repeated node degradation site={site} epoch={} revision={} preview_revision={} count=60 {context}", self.epoch, self.revision, self.preview_revision);
        }
    }
    pub(super) fn trace_noop(&self, operation: &str, reason: &str) {
        log::debug!("node no-op operation={operation} epoch={} revision={} editing={} selected={} depth={} reason={reason}", self.epoch, self.revision, self.editing(), self.selected.len(), self.parents.len());
    }
}

impl NodeEditor {
    pub(super) fn observed<T>(&self, site: &'static str, node: &str, result: motion_graph::Result<T>) -> motion_graph::Result<T> {
        if let Err(error) = &result {
            self.observe_failure(site, format_args!("node={} reason={} policy=preserve_caller_fallback", node.chars().take(128).collect::<String>(), error.to_string().chars().take(384).collect::<String>()));
        }
        result
    }
}
