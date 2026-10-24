//! Frozen authored geometry; generated outlines and paints are not path data.
//! Coordinates remain local to the addressed geometry. Track time is transferred
//! through composition time, independently of source/destination layer offsets.
use crate::{
    ensure,
    vector::{FillRule, VectorContent, VectorPath, VectorSource},
    Content, Error, Layer, Project, Result, MAX_LAYERS,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod editing;
pub const MAX_PATH_CLIPBOARD_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = MAX_LAYERS * crate::vector::MAX_PATHS;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathTarget {
    Vector { object: u64, item: Option<u64> },
    Mask { object: u64, mask: u64 },
}
impl PathTarget {
    pub fn object(&self) -> u64 {
        match self {
            Self::Vector { object, .. } | Self::Mask { object, .. } => *object,
        }
    }
    fn validate(&self) -> Result<()> {
        ensure(self.object() != 0, "path clipboard requires a layer")?;
        ensure(
            match self {
                Self::Vector { item, .. } => *item != Some(0),
                Self::Mask { mask, .. } => *mask != 0,
            },
            "path clipboard requires nonzero source IDs",
        )
    }
    fn read<'a>(&self, layer: &'a Layer, path: u64) -> Result<&'a VectorPath> {
        match self {
            Self::Vector { item, .. } => match &editing::vector(layer, *item)?.source {
                VectorSource::Paths { paths } => {
                    paths.iter().find(|p| p.id == path).ok_or_else(|| {
                        Error::Invalid(format!("authored vector path missing: path={path}"))
                    })
                }
                _ => Err(Error::Invalid(
                    "convert parametric geometry to an authored path before copying".into(),
                )),
            },
            Self::Mask { mask, .. } => layer
                .masks
                .iter()
                .find(|m| m.id == *mask && m.path.id == path)
                .map(|m| &m.path)
                .ok_or_else(|| {
                    Error::Invalid(format!("mask path missing: mask={mask} path={path}"))
                }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathSelection {
    pub target: PathTarget,
    pub path: u64,
    /// None copies the entire path. Some copies exactly these authored nodes.
    pub nodes: Option<Vec<u64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathDestination {
    Vector {
        object: u64,
        item: Option<u64>,
    },
    /// Replaces one existing mask's geometry, preserving its other properties.
    Mask {
        object: u64,
        mask: u64,
    },
    /// Creates an independent mask for each copied contour/connected fragment.
    NewMasks {
        object: u64,
    },
}
impl PathDestination {
    pub fn object(&self) -> u64 {
        match self {
            Self::Vector { object, .. } | Self::Mask { object, .. } | Self::NewMasks { object } => {
                *object
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathEntry {
    pub target: PathTarget,
    /// Full source identity allows cut to reject a modified source atomically.
    pub path: VectorPath,
    pub nodes: Option<Vec<u64>>,
    pub offset: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathClipboard {
    pub frame: u32,
    pub entries: Vec<PathEntry>,
}
fn failure(phase: &'static str, object: u64, path: u64, source: Error) -> Error {
    Error::PathClipboard {
        phase,
        object,
        path,
        source: Box::new(source),
    }
}
fn layer(project: &Project, object: u64) -> Result<&Layer> {
    project
        .layers
        .iter()
        .find(|l| l.id == object)
        .ok_or(Error::Missing(object))
}
fn validate_path(path: &VectorPath) -> Result<()> {
    VectorContent {
        source: VectorSource::Paths {
            paths: vec![path.clone()],
        },
        fill: None,
        fill_rule: FillRule::NonZero,
        stroke: None,
        trim: None,
    }
    .validate()
}
fn selected(entry: &PathEntry) -> Result<BTreeSet<u64>> {
    let available: BTreeSet<_> = entry.path.nodes.iter().map(|n| n.id).collect();
    let Some(nodes) = &entry.nodes else {
        return Ok(available);
    };
    let ids: BTreeSet<_> = nodes.iter().copied().collect();
    ensure(
        !ids.is_empty() && ids.len() == nodes.len() && ids.is_subset(&available),
        "path clipboard node selection is empty, duplicated, or missing",
    )?;
    Ok(ids)
}
impl PathClipboard {
    pub fn capture(project: &Project, selections: &[PathSelection], frame: u32) -> Result<Self> {
        ensure(
            frame < project.frames,
            "path clipboard source frame outside composition",
        )?;
        ensure(
            !selections.is_empty() && selections.len() <= MAX_ENTRIES,
            "invalid path clipboard selection count",
        )?;
        project.validate()?;
        let mut clipboard = Self {
            frame,
            entries: Vec::new(),
        };
        let mut bytes = 0usize;
        for selection in selections {
            let entry = (|| -> Result<_> {
                selection.target.validate()?;
                let layer = layer(project, selection.target.object())?;
                let path = selection.target.read(layer, selection.path)?;
                bytes = bytes
                    .saturating_add(crate::serialized_bytes(path))
                    .saturating_add(crate::serialized_bytes(selection))
                    .saturating_add(128);
                ensure(
                    bytes <= MAX_PATH_CLIPBOARD_BYTES,
                    "path clipboard exceeds 16 MiB",
                )?;
                Ok(PathEntry {
                    target: selection.target.clone(),
                    path: path.clone(),
                    nodes: selection.nodes.clone(),
                    offset: layer.clip(project.frames).offset_frame,
                })
            })()
            .map_err(|error| {
                failure("capture", selection.target.object(), selection.path, error)
            })?;
            clipboard.entries.push(entry);
        }
        clipboard.validate()?;
        Ok(clipboard)
    }
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.frame < crate::MAX_FRAMES
                && !self.entries.is_empty()
                && self.entries.len() <= MAX_ENTRIES,
            "invalid path clipboard frame or entry count",
        )?;
        ensure(
            crate::serialized_bytes(self) <= MAX_PATH_CLIPBOARD_BYTES,
            "path clipboard exceeds 16 MiB",
        )?;
        let mut seen = BTreeSet::new();
        for entry in &self.entries {
            (|| -> Result<()> {
                entry.target.validate()?;
                ensure(
                    seen.insert((entry.target.clone(), entry.path.id)),
                    "duplicate path clipboard source",
                )?;
                validate_path(&entry.path)?;
                ensure(!entry.path.nodes.is_empty(), "cannot copy an empty path")?;
                selected(entry)?;
                Ok(())
            })()
            .map_err(|error| failure("validate", entry.target.object(), entry.path.id, error))?;
        }
        Ok(())
    }
    /// Caller uses the ordinary core command transaction for validation/history.
    pub fn paste(
        &self,
        project: &mut Project,
        target: PathDestination,
        at_frame: Option<u32>,
    ) -> Result<()> {
        self.validate()?;
        editing::paste(self, project, &target, at_frame)
            .map_err(|error| failure("paste", target.object(), 0, error))
    }
    /// The clipboard must already have been retained before submitting this edit.
    pub fn remove(&self, project: &mut Project) -> Result<()> {
        self.validate()?;
        editing::remove(self, project)
    }
}

/// Selected runs never bridge an unselected vertex, including the closing seam.
fn fragments(entry: &PathEntry) -> Result<Vec<VectorPath>> {
    let selected = selected(entry)?;
    let path = &entry.path;
    if selected.len() == path.nodes.len() {
        return Ok(vec![path.clone()]);
    }
    let start = if path.closed {
        (path
            .nodes
            .iter()
            .position(|node| !selected.contains(&node.id))
            .unwrap_or(0)
            + 1)
            % path.nodes.len()
    } else {
        0
    };
    let mut runs = Vec::new();
    let mut nodes = Vec::new();
    for i in 0..path.nodes.len() {
        let node = &path.nodes[(start + i) % path.nodes.len()];
        if selected.contains(&node.id) {
            nodes.push(node.clone());
        } else if !nodes.is_empty() {
            runs.push(VectorPath {
                id: path.id,
                closed: false,
                nodes: std::mem::take(&mut nodes),
            });
        }
    }
    if !nodes.is_empty() {
        runs.push(VectorPath {
            id: path.id,
            closed: false,
            nodes,
        });
    }
    Ok(runs)
}
