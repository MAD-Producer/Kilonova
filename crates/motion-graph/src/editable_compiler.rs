//! A compiler-thread owner, independent of documents, devices and package IDs.
//! Cache entries are private compiler results; mutable returned shaders cannot
//! authorize later reuse. Every request still lowers and checks its own graph,
//! manifest, numeric data and budgets before returning a result.
use crate::{
    compiler, CompileOptions, CompiledGraph, Error, ErrorCode, GraphDocument, PortRef, Result,
};
use motion_effects::shader::{self, CompiledShader};
use std::collections::VecDeque;

pub const EDITABLE_CACHE_ENTRIES: usize = 2; // canonical and diagnostic output
pub const EDITABLE_CACHE_CODE_BYTES: u64 = 8 << 20;

#[derive(Clone, Copy, Debug, Default)]
pub struct EditableCompilerStats {
    pub portable_compilations: u64,
    pub reuses: u64,
    pub entries: usize,
    /// Retained UTF-8 code bytes, not a whole-process heap or GPU measurement.
    pub retained_code_bytes: u64,
}

struct Entry {
    source: String,
    shader: CompiledShader,
    code_bytes: u64,
}

#[derive(Default)]
pub struct EditableCompiler {
    entries: VecDeque<Entry>,
    stats: EditableCompilerStats,
}

impl EditableCompiler {
    pub fn compile(
        &mut self,
        document: &GraphDocument,
        options: &CompileOptions,
    ) -> Result<CompiledGraph> {
        compiler::compile_mode(document, options, true, Some(self))
    }

    pub fn compile_preview(
        &mut self,
        document: &GraphDocument,
        options: &CompileOptions,
        target: &PortRef,
    ) -> Result<CompiledGraph> {
        compiler::compile_preview_mode(document, options, target, true, Some(self))
    }

    pub fn stats(&self) -> EditableCompilerStats {
        self.stats
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.stats.entries = 0;
        self.stats.retained_code_bytes = 0;
    }

    pub(crate) fn shader(&mut self, source: &str) -> Result<(CompiledShader, u64, u64)> {
        // Entry point and SDK wrapper are fixed by this compile API. Source is
        // compared in full; author-supplied IDs and hashes do not prove equality.
        if let Some(entry) = self
            .entries
            .iter()
            .position(|entry| entry.source == source)
            .and_then(|index| self.entries.remove(index))
        {
            let shader = entry.shader.clone();
            self.entries.push_front(entry);
            self.stats.reuses = self.stats.reuses.saturating_add(1);
            return Ok((shader, 0, self.stats.retained_code_bytes));
        }
        self.stats.portable_compilations = self.stats.portable_compilations.saturating_add(1);
        let shader = shader::compile(source, "graph_fx")
            .map_err(|cause| Error::effect(ErrorCode::Shader, "", cause))?;
        let code_bytes = (source.len()
            + shader.wgsl.len()
            + shader.glsl.vertex.len()
            + shader.glsl.fragment.len()
            + shader.glsl.blocks.keys().map(String::len).sum::<usize>()
            + shader.glsl.textures.keys().map(String::len).sum::<usize>())
            as u64;
        if code_bytes <= EDITABLE_CACHE_CODE_BYTES {
            while self.entries.len() >= EDITABLE_CACHE_ENTRIES
                || self.stats.retained_code_bytes + code_bytes > EDITABLE_CACHE_CODE_BYTES
            {
                if let Some(entry) = self.entries.pop_back() {
                    self.stats.retained_code_bytes -= entry.code_bytes;
                } else {
                    break;
                }
            }
            self.entries.push_front(Entry {
                source: source.into(),
                shader: shader.clone(),
                code_bytes,
            });
            self.stats.retained_code_bytes += code_bytes;
            self.stats.entries = self.entries.len();
        }
        Ok((shader, 1, self.stats.retained_code_bytes))
    }
}
