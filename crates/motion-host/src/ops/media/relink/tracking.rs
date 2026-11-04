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
    platform: std::sync::Arc<dyn crate::Platform>,
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
            platform: s.platform().clone(),
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
    fn verify_inner(&self, _session: i64, request: &str) -> Result<()> {
        super::super::tracking::same_streams(&self.expected, &self.candidate, &self.assets)?;
        super::super::tracking::verify(
            &self.root,
            &self.workspace,
            &self.candidate,
            &self.assets,
            self.platform.as_ref(),
            request,
            "asset relink",
            None,
        )
    }
}
