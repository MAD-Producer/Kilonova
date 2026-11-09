//! Installed-font fallback. Font bytes are copied only after whole-grapheme
//! coverage succeeds. Cached misses never reopen font files on warm frames.
use super::{em_scale, is_sequence_control, shaping, SharedFontBytes, TextAtlas};
use ab_glyph::{Font, FontArc, FontRef, FontVec, PxScaleFont};
use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, OnceLock},
    time::Instant,
};

const MAX_FACES: usize = 32;
const MAX_FONT_BYTES: usize = 64 * 1024 * 1024;
const MAX_MISSES: usize = 128;
const MAX_MISS_BYTES: usize = 16 * 1024;

pub(super) struct Catalog {
    pub database: fontdb::Database,
    pub(super) selection_failures: super::font_error::SelectionFailures,
    candidates: Vec<fontdb::ID>,
}
impl Catalog {
    pub(super) fn new(database: fontdb::Database) -> Self {
        let mut ranked: Vec<_> = database
            .faces()
            .map(|f| {
                (
                    (
                        f.style != fontdb::Style::Normal,
                        f.weight.0.abs_diff(400),
                        f.monospaced,
                    ),
                    f.id,
                )
            })
            .collect();
        ranked.sort_by_key(|(rank, _)| *rank);
        let mut candidates: Vec<_> = ranked.into_iter().map(|(_, id)| id).collect();
        if let Some(preferred) = database.query(&fontdb::Query {
            families: &[fontdb::Family::SansSerif],
            ..Default::default()
        }) {
            candidates.retain(|id| *id != preferred);
            candidates.insert(0, preferred);
        }
        Self {
            database,
            candidates,
            selection_failures: Default::default(),
        }
    }
}

pub(super) fn system_catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let began = Instant::now();
        log::info!("UI installed font catalog start phase=discover");
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        log::debug!(
            "UI installed font catalog done faces={} elapsed_ms={}",
            database.len(),
            began.elapsed().as_millis()
        );
        Arc::new(Catalog::new(database))
    }))
}

pub(super) struct SystemFace {
    pub font: PxScaleFont<FontArc>,
    pub shaper: shaping::RunShaper,
}

#[derive(Debug)]
pub struct FallbackError {
    family: String,
    source: String,
    index: u32,
    cause: String,
    capacity: bool,
    original: Option<Arc<dyn std::error::Error + Send + Sync>>,
}
impl fmt::Display for FallbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "UI font fallback family={:?} source={:?} face_index={} capacity={} cause={}",
            self.family, self.source, self.index, self.capacity, self.cause
        )
    }
}
impl std::error::Error for FallbackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.original
            .as_ref()
            .map(|cause| cause.as_ref() as &dyn std::error::Error)
    }
}

struct Miss {
    issue: Option<Arc<FallbackError>>,
    blocked: bool,
    candidate_failures: usize,
}
pub(super) struct FontFallback {
    catalog: Arc<Catalog>,
    pub faces: Vec<SystemFace>,
    bytes: usize,
    misses: HashMap<String, Miss>,
    miss_bytes: usize,
    max_faces: usize,
    max_bytes: usize,
    #[cfg(test)]
    probes: usize,
}
impl FontFallback {
    pub fn new(catalog: Arc<Catalog>) -> Self {
        Self {
            catalog,
            faces: Vec::new(),
            bytes: 0,
            misses: HashMap::new(),
            miss_bytes: 0,
            max_faces: MAX_FACES,
            max_bytes: MAX_FONT_BYTES,
            #[cfg(test)]
            probes: 0,
        }
    }
    fn issue(&self, id: fontdb::ID, cause: impl fmt::Display, capacity: bool) -> FallbackError {
        let face = self.catalog.database.face(id);
        let family = face
            .and_then(|f| f.families.first())
            .map_or("unknown", |f| f.0.as_str());
        let source = face
            .map(|f| match &f.source {
                fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
                    path.to_string_lossy().into_owned()
                }
                fontdb::Source::Binary(_) => "memory".to_owned(),
            })
            .unwrap_or_else(|| "missing catalog face".into());
        FallbackError {
            family: family.chars().take(96).collect(),
            source: source.chars().take(384).collect(),
            index: face.map_or(0, |f| f.index),
            cause: cause.to_string().chars().take(256).collect(),
            capacity,
            original: None,
        }
    }
    fn caused(
        &self,
        id: fontdb::ID,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Arc<FallbackError> {
        let mut issue = self.issue(id, &cause, false);
        issue.original = Some(Arc::new(cause));
        Arc::new(issue)
    }
    fn unavailable(&self, id: fontdb::ID) -> Arc<FallbackError> {
        let mut issue = self.issue(
            id,
            "fontdb face data unavailable; mapping cause unavailable from fontdb",
            false,
        );
        // fontdb's mapping API returns None without its IO cause. Keep that
        // boundary explicit, and attach the real open error when this read-only
        // diagnostic can establish one. Do not label it the lost mapping cause.
        if let Some(face) = self.catalog.database.face(id) {
            if let fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) = &face.source {
                if let Err(cause) = std::fs::File::open(path) {
                    issue.cause =
                        format!("fontdb face data unavailable; diagnostic open failed: {cause}")
                            .chars()
                            .take(256)
                            .collect();
                    issue.original = Some(Arc::new(cause));
                }
            }
        }
        Arc::new(issue)
    }
    fn remember(&mut self, text: &str, miss: Miss) {
        if text.len() > MAX_MISS_BYTES {
            return;
        }
        // The bounded negative set is discarded as a unit. Its contents carry
        // no font slots; retained layouts and font faces remain valid.
        if self.misses.len() >= MAX_MISSES || self.miss_bytes + text.len() > MAX_MISS_BYTES {
            self.misses.clear();
            self.miss_bytes = 0;
        }
        self.miss_bytes += text.len();
        self.misses.insert(text.to_owned(), miss);
    }
    fn observe(
        issue: &FallbackError,
        observations: &crate::observability::Failures,
        candidate_failures: usize,
    ) {
        observations.record(
            "text.font_fallback",
            if issue.capacity { "capacity" } else { "source" },
            format_args!("candidate_failures={candidate_failures} {issue}"),
        );
    }
    pub fn resolve(
        &mut self,
        text: &str,
        size: f32,
        observations: &crate::observability::Failures,
    ) -> Result<Option<super::glyph_index::FontSlot>, Arc<FallbackError>> {
        use super::glyph_index::FontSlot;
        if let Some(index) = self
            .faces
            .iter()
            .position(|f| super::layout::coverage(&f.shaper, text))
        {
            return Ok(Some(FontSlot::System(index as u16)));
        }
        if let Some(miss) = self.misses.get(text) {
            if let Some(issue) = &miss.issue {
                Self::observe(issue, observations, miss.candidate_failures);
                if miss.blocked {
                    return Err(Arc::clone(issue));
                }
            }
            return Ok(None);
        }
        let mut first_issue: Option<Arc<FallbackError>> = None;
        let mut capacity_issue: Option<Arc<FallbackError>> = None;
        let mut candidate_failures = 0usize;
        for &id in &self.catalog.candidates {
            #[cfg(test)]
            {
                self.probes += 1;
            }
            let candidate = self.catalog.database.with_face_data(id, |data, index| {
                let font = FontRef::try_from_slice_and_index(data, index)
                    .map_err(|cause| self.caused(id, cause))?;
                if !text.chars().filter(|c| !c.is_control() && !is_sequence_control(*c))
                    .all(|c| font.glyph_id(c).0 != 0) { return Ok(None); }
                if self.faces.len() >= self.max_faces || data.len() > self.max_bytes.saturating_sub(self.bytes) {
                    return Err(Arc::new(self.issue(id, format_args!("retained_faces={} face_limit={} retained_bytes={} candidate_bytes={} byte_limit={}",
                        self.faces.len(), self.max_faces, self.bytes, data.len(), self.max_bytes), true)));
                }
                let font = FontArc::new(FontVec::try_from_vec_and_index(data.to_vec(), index)
                    .map_err(|cause| self.caused(id, cause))?);
                let shaper = shaping::RunShaper::from_source(Arc::new(SharedFontBytes(font.clone())), index)
                    .map_err(|cause| self.caused(id, cause))?;
                let scale = em_scale(&font, size);
                Ok(Some((SystemFace { font: PxScaleFont { font, scale }, shaper }, data.len())))
            }).unwrap_or_else(|| Err(self.unavailable(id)));
            match candidate {
                Ok(Some((face, bytes))) => {
                    if let Some(issue) = &first_issue {
                        Self::observe(issue, observations, candidate_failures);
                    }
                    let slot = FontSlot::System(self.faces.len() as u16);
                    self.faces.push(face);
                    self.bytes += bytes;
                    return Ok(Some(slot));
                }
                Ok(None) => {}
                Err(issue) if issue.capacity => {
                    candidate_failures = candidate_failures.saturating_add(1);
                    capacity_issue.get_or_insert(issue);
                }
                Err(issue) => {
                    candidate_failures = candidate_failures.saturating_add(1);
                    first_issue.get_or_insert(issue);
                }
            }
        }
        if let Some(issue) = capacity_issue {
            if let Some(source_issue) = &first_issue {
                Self::observe(source_issue, observations, candidate_failures);
            }
            Self::observe(&issue, observations, candidate_failures);
            self.remember(
                text,
                Miss {
                    issue: Some(Arc::clone(&issue)),
                    blocked: true,
                    candidate_failures,
                },
            );
            return Err(issue);
        }
        if let Some(issue) = &first_issue {
            Self::observe(issue, observations, candidate_failures);
        }
        self.remember(
            text,
            Miss {
                issue: first_issue,
                blocked: false,
                candidate_failures,
            },
        );
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::format_controls_tests::Capture;
    use std::{fs, sync::Mutex};

    fn capacity_catalog() -> Arc<Catalog> {
        let installed = system_catalog();
        let face = installed.database.faces().find(|face| {
            installed.database.with_face_data(face.id, |data,index| {
                FontRef::try_from_slice_and_index(data,index)
                    .is_ok_and(|font| font.glyph_id('A').0 != 0)
            }).unwrap_or(false)
        }).expect("capacity fixture needs one parseable installed face covering A").clone();
        let mut database = fontdb::Database::new();
        database.push_face_info(face);
        Arc::new(Catalog::new(database))
    }

    #[test]
    fn capacity_fixture_isolates_capacity_without_discarding_real_source_faults() {
        let isolated = capacity_catalog();
        let good = isolated.database.faces().next().unwrap().clone();
        let mut bad = good.clone();
        bad.source = fontdb::Source::Binary(Arc::new(Vec::<u8>::new()));
        let mut database = fontdb::Database::new();
        database.push_face_info(bad);
        database.push_face_info(good);
        let mut mixed = Catalog::new(database);
        mixed.candidates = mixed.database.faces().map(|f| f.id).collect();
        for (catalog, failures) in [(isolated,1), (Arc::new(mixed),2)] {
            let mut healthy = FontFallback::new(Arc::clone(&catalog));
            assert!(healthy.resolve("A",12.,&Default::default()).unwrap().is_some());
            assert_eq!(healthy.faces.len(),1);
            let mut pool = FontFallback::new(catalog);
            pool.max_bytes = 1;
            assert!(pool.resolve("A",12.,&Default::default()).unwrap_err().capacity);
            assert_eq!(pool.misses.get("A").unwrap().candidate_failures,failures);
            assert_eq!(pool.probes,failures);
            assert!(pool.faces.is_empty() && pool.bytes == 0);
        }
    }

    fn missing_catalog(path: &std::path::Path) -> Arc<Catalog> {
        let mut face = system_catalog().database.faces().next().unwrap().clone();
        face.source = fontdb::Source::File(path.to_owned());
        let mut database = fontdb::Database::new();
        database.push_face_info(face);
        Arc::new(Catalog::new(database))
    }

    #[test]
    fn negative_queries_are_bounded_and_warm_misses_never_reopen_sources() {
        let path = std::env::temp_dir().join(format!("missing-ui-font-{}.ttf", std::process::id()));
        assert!(!path.exists());
        let mut pool = FontFallback::new(missing_catalog(&path));
        let observations = Default::default();
        for _ in 0..120 {
            assert!(pool
                .resolve("\u{10ffff}", 12., &observations)
                .unwrap()
                .is_none());
        }
        assert_eq!(pool.probes, 1, "cached source failure reopened its file");
        for i in 0..MAX_MISSES * 2 {
            assert!(pool
                .resolve(&format!("{i}\u{10ffff}"), 12., &observations)
                .unwrap()
                .is_none());
            assert!(pool.misses.len() <= MAX_MISSES && pool.miss_bytes <= MAX_MISS_BYTES);
        }
        let long = "\u{10ffff}".repeat(MAX_MISS_BYTES / 4);
        assert!(pool.resolve(&long, 12., &observations).unwrap().is_none());
        assert_eq!(pool.misses.len(), 1);
        assert_eq!(pool.miss_bytes, MAX_MISS_BYTES);
    }

    #[test]
    fn invalid_candidate_preserves_the_original_parser_error() {
        let mut face = system_catalog().database.faces().next().unwrap().clone();
        face.source = fontdb::Source::Binary(Arc::new(Vec::<u8>::new()));
        let mut database = fontdb::Database::new();
        database.push_face_info(face.clone());
        database.push_face_info(face);
        let mut pool = FontFallback::new(Arc::new(Catalog::new(database)));
        assert!(pool
            .resolve("A", 12., &Default::default())
            .unwrap()
            .is_none());
        let issue = pool.misses.get("A").unwrap().issue.as_ref().unwrap();
        assert_eq!(pool.misses.get("A").unwrap().candidate_failures, 2);
        assert!(std::error::Error::source(issue.as_ref())
            .unwrap()
            .is::<ab_glyph::InvalidFont>());
        assert!(!issue.capacity && pool.faces.is_empty());
    }

    #[test]
    fn exhausted_font_capacity_preserves_loaded_slots_and_new_atlas_recovers() {
        let observations = Default::default();
        let mut pool = FontFallback::new(system_catalog());
        pool.max_faces = 1;
        let first = pool.resolve("A", 12., &observations).unwrap().unwrap();
        let missing = ["中", "س", "अ", "ก"]
            .into_iter()
            .find(|s| !super::super::layout::coverage(&pool.faces[0].shaper, s))
            .unwrap();
        let before = pool.bytes;
        let cause = pool.resolve(missing, 12., &observations).unwrap_err();
        assert!(cause.capacity);
        assert_eq!(pool.faces.len(), 1);
        assert_eq!(pool.bytes, before);
        assert_eq!(pool.resolve("A", 12., &observations).unwrap(), Some(first));
        let probes = pool.probes;
        assert!(
            pool.resolve(missing, 12., &observations)
                .unwrap_err()
                .capacity
        );
        assert_eq!(pool.probes, probes);
        let mut replacement = FontFallback::new(system_catalog());
        assert!(replacement
            .resolve(missing, 12., &observations)
            .unwrap()
            .is_some());
        let mut bytes = FontFallback::new(system_catalog());
        bytes.max_bytes = 1;
        assert!(bytes.resolve("A", 12., &observations).unwrap_err().capacity);
        assert!(bytes.faces.is_empty());
        assert_eq!(bytes.bytes, 0);
    }

    #[test]
    fn system_slots_keep_requested_size_across_scale_changes_and_reject_invalid_indices() {
        use crate::text::glyph_index::{FontSlot, RasterFailure};
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        let slot = atlas
            .system_fallback
            .as_mut()
            .unwrap()
            .resolve("A", 12., &atlas.observations)
            .unwrap()
            .unwrap();
        let FontSlot::System(index) = slot else {
            panic!("system pool must own its slot");
        };
        let id = atlas.system_fallback.as_ref().unwrap().faces[index as usize]
            .shaper
            .nominal_glyph('A')
            .unwrap();
        let pixels = |atlas: &TextAtlas, entry: super::super::GlyphEntry| {
            let config = atlas.config();
            let x0 = (entry.uv[0] * config.width as f32).round() as usize;
            let x1 = (entry.uv[2] * config.width as f32).round() as usize;
            let y0 = (entry.uv[1] * config.height as f32).round() as usize;
            let y1 = (entry.uv[3] * config.height as f32).round() as usize;
            let data: Vec<_> = (y0..y1)
                .flat_map(|y| {
                    atlas.pixel_data()
                        [(y * config.width as usize + x0) * 4..(y * config.width as usize + x1) * 4]
                        .iter()
                        .copied()
                })
                .collect();
            (x1 - x0, y1 - y0, data)
        };
        let before = atlas.glyph_by_id(slot, id, 12., 'A').unwrap();
        let original = pixels(&atlas, before);
        assert!(original.2.iter().any(|v| *v != 0));
        atlas.set_scale(24.);
        let after = atlas.glyph_by_id(slot, id, 12., 'A').unwrap();
        assert_eq!(before.advance, after.advance);
        assert_eq!(before.offset, after.offset);
        assert_eq!(original, pixels(&atlas, after));
        atlas.set_pixel_ratio(2.);
        let hidpi = atlas.glyph_by_id(slot, id, 12., 'A').unwrap();
        let high = pixels(&atlas, hidpi);
        assert_eq!(before.advance, hidpi.advance);
        assert!(high.0 + 2 >= original.0 * 2 && high.1 + 2 >= original.1 * 2);
        assert!(high.2.iter().any(|v| *v != 0));
        assert_eq!(
            atlas
                .glyph_by_id(FontSlot::System(u16::MAX), id, 12., 'A')
                .err()
                .expect("invalid system slot must fail")
                .failure,
            RasterFailure::MissingFace
        );
        assert_eq!(high, pixels(&atlas, hidpi));
    }

    #[test]
    fn source_failures_keep_default_file_stderr_evidence_without_text_or_reopening() {
        const CHILD: &str = "MOTION_UI_FONT_SOURCE_CHILD";
        const NAME: &str = "text::fallback::tests::source_failures_keep_default_file_stderr_evidence_without_text_or_reopening";
        if let Some(path) = std::env::var_os(CHILD) {
            let path = std::path::PathBuf::from(path);
            log::set_logger(Box::leak(Box::new(Capture(Mutex::new(
                fs::File::create(&path).unwrap(),
            )))))
            .unwrap();
            log::set_max_level(log::LevelFilter::Info);
            let missing = path.with_extension("missing.ttf");
            let catalog = missing_catalog(&missing);
            let mut pool = FontFallback::new(Arc::clone(&catalog));
            let observations = Default::default();
            let private = "PRIVATE-FONT-LABEL\u{10ffff}";
            for _ in 0..120 {
                assert!(pool.resolve(private, 12., &observations).unwrap().is_none());
            }
            assert_eq!(pool.probes, 1);
            let issue = pool.misses.get(private).unwrap().issue.as_ref().unwrap();
            let cause = std::error::Error::source(issue.as_ref()).unwrap();
            assert_eq!(
                cause.downcast_ref::<std::io::Error>().unwrap().kind(),
                std::io::ErrorKind::NotFound
            );
            let events = fs::read_to_string(&path).unwrap();
            let lines: Vec<_> = events.lines().collect();
            assert_eq!(lines.len(), 2, "{events}");
            assert!(lines[0].starts_with("WARN ") && lines[0].contains("count=1 "));
            assert!(lines[1].starts_with("ERROR ") && lines[1].contains("count=60 "));
            assert!(
                events.contains("site=text.font_fallback")
                    && events.contains("diagnostic open failed")
            );
            assert!(events.contains("missing.ttf") && !events.contains("PRIVATE-FONT-LABEL"));
            assert!(events.contains("candidate_failures=1"));
            let mut replacement = FontFallback::new(catalog);
            let fresh = Default::default();
            for _ in 0..60 {
                assert!(replacement.resolve(private, 12., &fresh).unwrap().is_none());
            }
            assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 4);
            let mut limited = FontFallback::new(capacity_catalog());
            limited.max_bytes = 1;
            let capacity_owner = Default::default();
            for _ in 0..120 {
                assert!(
                    limited
                        .resolve("A", 12., &capacity_owner)
                        .unwrap_err()
                        .capacity
                );
            }
            assert!(limited.faces.is_empty() && limited.bytes == 0);
            let mut invalid_face = system_catalog().database.faces().next().unwrap().clone();
            invalid_face.source = fontdb::Source::Binary(Arc::new(Vec::<u8>::new()));
            let mut invalid_db = fontdb::Database::new();
            invalid_db.push_face_info(invalid_face);
            let mut invalid = FontFallback::new(Arc::new(Catalog::new(invalid_db)));
            let parser_owner = Default::default();
            for _ in 0..120 {
                assert!(invalid.resolve("A", 12., &parser_owner).unwrap().is_none());
            }
            assert_eq!(invalid.probes, 1);
            let final_events = fs::read_to_string(&path).unwrap();
            assert_eq!(final_events.lines().count(), 8, "{final_events}");
            assert!(
                final_events.contains("key=\"capacity\"") && final_events.contains("byte_limit=1")
            );
            assert!(final_events.contains("InvalidFont"));
            return;
        }
        let explicit = std::env::var_os("MOTION_UI_FONT_SOURCE_ARTIFACTS");
        let root = explicit
            .clone()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::temp_dir().join(format!(
                    "motion-ui-font-source-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                ))
            });
        fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env_remove("MOTION_LOG")
            .env(CHILD, root.join("events.log"))
            .output()
            .unwrap();
        fs::write(root.join("child-stdout.log"), &output.stdout).unwrap();
        fs::write(root.join("child-stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr
                .lines()
                .filter(|s| s.starts_with("ERROR ") && s.contains("site=text.font_fallback"))
                .count(),
            4,
            "{stderr}"
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("UI degradation"));
        assert!(!stderr.contains("WARN ") && !stderr.contains("PRIVATE-FONT-LABEL"));
        if explicit.is_none() {
            for name in ["events.log", "child-stdout.log", "child-stderr.log"] {
                fs::remove_file(root.join(name)).unwrap();
            }
            fs::remove_dir(root).unwrap();
        }
    }
}

impl TextAtlas {
    pub(super) fn font_for_grapheme(
        &mut self,
        text: &str,
    ) -> Result<super::glyph_index::FontSlot, Arc<FallbackError>> {
        use super::glyph_index::FontSlot;
        if super::layout::coverage(&self.shaper, text) {
            return Ok(FontSlot::Primary);
        }
        if self
            .fallback_shaper
            .as_ref()
            .is_some_and(|f| super::layout::coverage(f, text))
        {
            return Ok(FontSlot::Fallback);
        }
        if let Some(pool) = &mut self.system_fallback {
            if let Some(slot) = pool.resolve(text, self.font_size, &self.observations)? {
                return Ok(slot);
            }
        }
        Ok(FontSlot::Primary)
    }
}
