//! A whole group shares the Session's existing bounded preparation worker.
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Scope {
    owner: u64,
    cameras: Vec<CameraKey>,
    sizes: Vec<[u32; 2]>,
    cell: Option<usize>,
}

pub(super) fn context(key: &Key) -> motion_render::PipelinePreparationContext {
    motion_render::PipelinePreparationContext {
        document_generation: key.document,
        project_revision: key.revision,
    }
}

pub(super) fn key(key: &Key, project: &Project, cell: Option<usize>) -> pipeline_preparation::Key {
    let size = key.sizes[cell.unwrap_or(0)];
    pipeline_preparation::Key {
        context: context(key), surface: key.surface, view: key.view_revision,
        width: size[0], height: size[1], preview: key.preview,
        window: (f64::from_bits(key.frame) / (project.fps.as_f64() * 2.)).floor() as u64,
        exposure: None,
        views: Some(Scope { owner: key.owner, cameras: key.cameras.clone(),
            sizes: key.sizes.clone(), cell }),
    }
}

pub(super) fn views(views: &[View<'_>], sizes: &[[u32; 2]]) -> Vec<motion_render::PipelinePreparationView> {
    views.iter().zip(sizes).map(|(view, &size)| motion_render::PipelinePreparationView {
        camera: view.camera.into(), size,
    }).collect()
}
