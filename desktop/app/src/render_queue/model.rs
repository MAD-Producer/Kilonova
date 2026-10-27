use motion_core::FrameScope;
use motion_desktop_media::movie::durable::SnapshotInfo;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

pub(crate) const VERSION: u32 = 1;
pub(crate) const MAX_JOBS: usize = 256;
pub(crate) const MAX_TEMPLATES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Format {
    Mp4H264Aac,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Template {
    pub id: u64,
    pub name: String,
    pub format: Format,
    pub scope: FrameScope,
    pub pattern: String,
}
impl Template {
    pub fn validate(&self) -> Result<(), String> {
        if self.id == 0
            || self.name.trim().is_empty()
            || self.name.len() > 512
            || self.pattern.len() > 512
        {
            return Err("invalid render output template identity or size".into());
        }
        self.filename("Composition", 1, 0)?;
        Ok(())
    }
    pub fn filename(&self, composition: &str, id: u64, revision: u64) -> Result<String, String> {
        let mut name: String = composition
            .chars()
            .take(64)
            .map(|c| if forbidden(c) { '_' } else { c })
            .collect();
        name = name.trim_matches([' ', '.']).to_owned();
        if name.is_empty() {
            name = "Composition".into();
        }
        let mut file = String::new();
        let mut rest = self.pattern.as_str();
        while let Some(start) = rest.find('{') {
            let literal = &rest[..start];
            if literal.contains('}') {
                return Err("unmatched render filename token".into());
            }
            file.push_str(literal);
            let end = rest[start + 1..]
                .find('}')
                .map(|n| start + 1 + n)
                .ok_or("unmatched render filename token")?;
            match &rest[start + 1..end] {
                "composition" => file.push_str(&name),
                "id" => file.push_str(&id.to_string()),
                "revision" => file.push_str(&revision.to_string()),
                _ => {
                    return Err(
                        "unknown render filename token; use {composition}, {id}, {revision}".into(),
                    )
                }
            }
            rest = &rest[end + 1..];
        }
        if rest.contains('}') {
            return Err("unmatched render filename token".into());
        }
        file.push_str(rest);
        validate_filename(&file)?;
        if !file.to_ascii_lowercase().ends_with(".mp4") {
            return Err("MP4 output template must end in .mp4".into());
        }
        Ok(file)
    }
}
fn forbidden(c: char) -> bool {
    c.is_control() || "<>:\"/\\|?*".contains(c)
}
pub(crate) fn validate_filename(file: &str) -> Result<(), String> {
    let stem = file.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| {
            stem.strip_prefix(p)
                .is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        });
    if file.is_empty()
        || file.encode_utf16().count() > 240
        || file.chars().any(forbidden)
        || file.ends_with([' ', '.'])
        || reserved
        || matches!(file, "." | "..")
    {
        return Err("invalid render output filename".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum State {
    Queued,
    Running,
    Cancelling,
    Succeeded {
        bytes: u64,
        encoder: String,
    },
    Cancelled,
    Failed {
        cause: String,
    },
    Interrupted,
    /// Removal is durable before deleting input files. A failed final save can
    /// safely resume removal without treating the missing input as a render job.
    Removing,
}
impl State {
    pub fn active(&self) -> bool {
        matches!(self, Self::Running | Self::Cancelling)
    }
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Failed { .. } | Self::Cancelled | Self::Interrupted
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Job {
    pub id: u64,
    /// One generated directory directly below this queue's snapshots directory.
    pub snapshot: String,
    pub source: SnapshotInfo,
    /// A copied template; later template edits cannot change a queued task.
    pub template: Template,
    pub output: PathBuf,
    pub attempt: u32,
    pub state: State,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Document {
    pub version: u32,
    pub next_id: u64,
    pub next_template: u64,
    pub templates: Vec<Template>,
    pub jobs: Vec<Job>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            version: VERSION,
            next_id: 1,
            next_template: 3,
            jobs: Vec::new(),
            templates: vec![
                Template {
                    id: 1,
                    name: "MP4 · Full composition".into(),
                    format: Format::Mp4H264Aac,
                    scope: FrameScope::Composition,
                    pattern: "{composition}-{id}.mp4".into(),
                },
                Template {
                    id: 2,
                    name: "MP4 · Work area".into(),
                    format: Format::Mp4H264Aac,
                    scope: FrameScope::WorkArea,
                    pattern: "{composition}-range-{id}.mp4".into(),
                },
            ],
        }
    }
}
impl Document {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION
            || self.next_id == 0
            || self.next_template == 0
            || self.jobs.len() > MAX_JOBS
            || self.templates.len() > MAX_TEMPLATES
        {
            return Err("invalid render queue version, identity or capacity".into());
        }
        let mut ids = BTreeSet::new();
        for template in &self.templates {
            template.validate()?;
            if template.id >= self.next_template || !ids.insert(template.id) {
                return Err("duplicate or invalid render template ID".into());
            }
        }
        let mut ids = BTreeSet::new();
        let mut snapshots = BTreeSet::new();
        let mut outputs = BTreeSet::new();
        let mut active = 0;
        for job in &self.jobs {
            job.template.validate()?;
            let s = &job.source;
            motion_core::Project::new(s.width, s.height, s.fps, s.frames)
                .map_err(|e| e.to_string())?
                .validate()
                .map_err(|e| e.to_string())?;
            s.range.validate(s.frames).map_err(|e| e.to_string())?;
            if s.name.len() > 512
                || s.composition.len() > 512
                || s.composition.is_empty()
                || (job.template.scope == FrameScope::Composition
                    && s.range != motion_core::WorkArea::full(s.frames))
            {
                return Err("invalid queued composition metadata or frame range".into());
            }
            if job.template.id >= self.next_template
                || job.id == 0
                || job.id >= self.next_id
                || !ids.insert(job.id)
                || !snapshots.insert(job.snapshot.clone())
                || !outputs.insert(job.output.to_string_lossy().to_lowercase())
            {
                return Err("duplicate or invalid render task identity, input or output".into());
            }
            if !job.snapshot.starts_with("movie-")
                || job.snapshot.len() > 128
                || !job
                    .snapshot
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Err("invalid render task snapshot directory".into());
            }
            validate_output(&job.output)?;
            if job.state.active() {
                active += 1;
            }
            match &job.state {
                State::Succeeded { bytes, encoder }
                    if *bytes == 0 || encoder.is_empty() || encoder.len() > 128 =>
                {
                    return Err("invalid successful render task result".into())
                }
                State::Failed { cause } if cause.is_empty() || cause.len() > 16 * 1024 => {
                    return Err("invalid render task failure context".into())
                }
                State::Running
                | State::Cancelling
                | State::Succeeded { .. }
                | State::Failed { .. }
                    if job.attempt == 0 =>
                {
                    return Err("render task has no recorded attempt".into())
                }
                _ => {}
            }
        }
        if active > 1 {
            return Err("more than one render task is active".into());
        }
        Ok(())
    }
}
pub(crate) fn validate_output(output: &Path) -> Result<(), String> {
    if !output.is_absolute()
        || output
            .components()
            .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
    {
        return Err("render output must be an absolute path without traversal".into());
    }
    validate_filename(
        output
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("render output has no UTF-8 filename")?,
    )?;
    if !output
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mp4"))
    {
        return Err("queued movie output must be .mp4".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
