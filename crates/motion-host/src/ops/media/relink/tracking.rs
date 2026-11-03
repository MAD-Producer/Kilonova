//! Verify immutable staged plate bytes before publishing a relink. This proof
//! belongs to one ready batch, revision and document generation, not to a path
//! forever. Identical bytes preserve an admitted decoded timeline and lens solve.
use super::*;

pub(super) struct Check {
    root: PathBuf,
    workspace: PathBuf,
    revision: u64,
    generation: u64,
    expected: Project,
    candidate: Project,
    assets: Vec<u64>,
}
impl Check {
    pub(super) fn capture(s: &Session, request: &str) -> Result<Option<Self>> {
        let batch = s
            .asset_relink
            .as_ref()
            .filter(|b| b.request == request)
            .ok_or("asset relink task missing")?;
        // Preserve normal stale/cancelled/not-ready handling without source IO.
        if batch.state != "ready"
            || batch.generation != s.document_generation
            || batch.revision != s.engine.revision()
        {
            return Ok(None);
        }
        let candidate = batch.draft.engine.project();
        let assets: Vec<_> = candidate
            .tracking_assets
            .iter()
            .filter(|a| batch.candidates.iter().any(|c| c.target == a.source))
            .map(|a| a.id)
            .collect();
        if assets.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            root: s.root.clone(),
            workspace: batch.workspace.path.clone(),
            revision: batch.revision,
            generation: batch.generation,
            expected: s.engine.project().clone(),
            candidate: candidate.clone(),
            assets,
        }))
    }
    pub(super) fn matches(&self, s: &Session, request: &str) -> bool {
        self.revision == s.engine.revision()
            && self.generation == s.document_generation
            && self.expected == *s.engine.project()
            && s.asset_relink.as_ref().is_some_and(|b| {
                b.request == request
                    && b.state == "ready"
                    && b.workspace.path == self.workspace
                    && b.draft.engine.project() == &self.candidate
            })
    }
    pub(super) fn verify(&self, session: i64, request: &str) -> Result<()> {
        let began = Instant::now();
        log::info!("asset relink tracking check request={request} session={session} phase=verify_tracking count={}", self.assets.len());
        let result = self.verify_inner(session, request);
        if let Err(cause) = &result {
            log::error!("asset relink tracking failed request={request} session={session} phase=verify_tracking elapsed_ms={} cause={}",
                began.elapsed().as_millis(), cause.to_string().chars().take(8192).collect::<String>());
        } else {
            log::debug!("asset relink tracking done request={request} session={session} phase=verify_tracking elapsed_ms={}", began.elapsed().as_millis());
        }
        result
    }
    fn verify_inner(&self, session: i64, request: &str) -> Result<()> {
        let workspace = self.workspace.canonicalize()?;
        for &id in &self.assets {
            let descriptor = self
                .candidate
                .tracking_assets
                .iter()
                .find(|a| a.id == id)
                .ok_or("relink tracking attachment missing")?;
            let source = lookup(&self.candidate, descriptor.source)
                .ok_or("relink tracking source missing")?;
            // Starts are emitted with no business guard before file IO, including
            // original-document read and the potentially blocked source reader.
            log::info!("asset relink tracking start request={request} session={session} tracking_asset={id} source={:?}:{} phase=verify_tracking",
                descriptor.source.kind, descriptor.source.id);
            let result = (|| -> Result<()> {
                let path = self.workspace.join(source.path()).canonicalize()?;
                if !path.starts_with(&workspace) {
                    return Err("relink tracking source escapes staging".into());
                }
                let mut file = fs::File::open(&path)?;
                let before = file.metadata()?;
                if !before.is_file()
                    || before.len() == 0
                    || before.len() > motion_core::storage::MAX_MEDIA_ASSET
                {
                    return Err("relink tracking source exceeds file budget".into());
                }
                // Reuses original-document hash/runtime consistency, complete
                // source SHA-256, dimensions and video-frame-count validation.
                // Staging has already decoded the media. Exact encoded identity
                // retains the PTS verified at original import; no FPS inference.
                crate::tracking::project::load_original(
                    &self.root,
                    &self.candidate,
                    id,
                    &mut file,
                    before.len(),
                    || false,
                )?;
                let after = file.metadata()?;
                if before.len() != after.len() || before.modified()? != after.modified()? {
                    return Err("relink tracking source changed during verification".into());
                }
                Ok(())
            })();
            result.map_err(|cause| {
                crate::Error::with_context(
                    format!(
                        "relink tracking_asset={id} source={:?}:{} phase=verify_tracking",
                        descriptor.source.kind, descriptor.source.id
                    ),
                    cause,
                )
            })?;
        }
        Ok(())
    }
}
