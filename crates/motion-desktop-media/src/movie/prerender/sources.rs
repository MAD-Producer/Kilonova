//! Owner-side metadata capture; all source IO belongs to background Freeze.
use super::*;

pub(super) struct CapturedSources {
    project: Project,
    root: PathBuf,
    registry: Registry,
    memory_policy: MemoryPolicy,
    allocation_budget: motion_render::AllocationBudget,
    revision: u64,
    platform: Arc<dyn motion_host::Platform>,
    depth: motion_host::depth_snapshot::DepthSnapshot,
    pub(super) observation_request: u64,
}
impl CapturedSources {
    pub(super) fn watch(&self) -> Result<source_watch::SourceWatch> {
        let mut paths: std::collections::BTreeSet<PathBuf> = self
            .project
            .assets
            .iter()
            .map(|a| PathBuf::from(&a.path))
            .chain(self.project.fonts.iter().map(|a| PathBuf::from(&a.path)))
            .chain(
                self.project
                    .video_assets
                    .iter()
                    .map(|a| PathBuf::from(&a.path)),
            )
            .chain(
                self.project
                    .model_assets
                    .iter()
                    .map(|a| PathBuf::from(&a.path)),
            )
            .collect();
        for video in &self.project.video_assets {
            paths.insert(
                motion_media::video_cache_path(&self.root, video)?
                    .strip_prefix(&self.root)
                    .map_err(|e| e.to_string())?
                    .to_path_buf(),
            );
        }
        for resource in self.project.node_resources().map_err(|e| e.to_string())? {
            paths.insert(PathBuf::from(resource.path));
        }
        source_watch::SourceWatch::arm(
            self.observation_request,
            &self.root,
            &paths.into_iter().collect::<Vec<_>>(),
        )
    }
    pub(super) fn capture(session: &Session, request: u64) -> Result<Self> {
        Ok(Self {
            project: session.engine.snapshot(),
            root: session.root.clone(),
            registry: session.effects.registry.clone(),
            memory_policy: session.effects.memory_policy(),
            allocation_budget: session.allocation_budget().clone(),
            revision: session.engine.revision(),
            platform: session.platform().clone(),
            depth: motion_host::depth_snapshot::DepthSnapshot::capture(session)
                .map_err(|e| e.to_string())?,
            observation_request: request,
        })
    }
    pub(super) fn open(self, control: &ExportControl) -> Result<MovieFreezePlan> {
        let request = self.observation_request;
        let mut sources = MovieFreezePlan::new_with_audio(
            self.project,
            &self.root,
            self.registry,
            self.memory_policy,
            self.revision,
            control,
            false,
        )?;
        sources.allocation_budget = self.allocation_budget;
        // Editable node resources are project-owned inputs too. Runtime
        // packages alone do not describe their complete source closure.
        for resource in sources
            .project
            .node_resources()
            .map_err(|e| e.to_string())?
        {
            control.check()?;
            let relative = PathBuf::from(&resource.path);
            if sources
                .sources
                .iter()
                .any(|(path, _, _, _)| path == &relative)
            {
                continue;
            }
            let source = sources
                .root
                .join(&relative)
                .canonicalize()
                .map_err(|e| format!("freeze node resource: {e}"))?;
            if !source.starts_with(&sources.root) {
                return Err("node resource resolves outside project".into());
            }
            let file = fs::File::open(&source).map_err(|e| format!("open node resource: {e}"))?;
            let metadata = file.metadata().map_err(|e| e.to_string())?;
            if !metadata.is_file() || metadata.len() != resource.bytes {
                return Err("node resource size differs from document".into());
            }
            sources.total_bytes = sources
                .total_bytes
                .checked_add(metadata.len())
                .ok_or("prerender source byte overflow")?;
            sources.source_paths.insert(source.clone());
            sources.sources.push((relative, source, file, metadata));
        }
        if sources.total_bytes > motion_core::storage::MAX_PACKAGE {
            return Err("prerender frozen source budget exceeded".into());
        }
        sources.platform = self.platform;
        sources.depth = self.depth;
        sources.observation_request = request;
        Ok(sources)
    }
}
