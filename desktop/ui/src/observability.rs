//! Bounded evidence owned by an immutable stylesheet or a font atlas.
use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Instant,
};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
const MAX_KEYS: usize = 128;

pub(crate) struct Failures {
    owner: u64,
    entries: Mutex<BTreeMap<(&'static str, String), Episode>>,
}
struct Episode {
    count: usize,
    began: Instant,
}
impl Default for Failures {
    fn default() -> Self {
        Self {
            owner: NEXT_OWNER.fetch_add(1, Ordering::Relaxed),
            entries: Mutex::new(BTreeMap::new()),
        }
    }
}
impl Failures {
    /// Healthy lookups never enter this map. A replacement sheet/atlas starts
    /// fresh; clones of the same immutable sheet share the original evidence.
    pub(crate) fn record(&self, site: &'static str, key: &str, context: fmt::Arguments<'_>) {
        let key = key.chars().take(128).collect::<String>();
        let (count, elapsed, overflow) = {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            let mut map_key = (site, key.clone());
            let overflow = entries.len() >= MAX_KEYS && !entries.contains_key(&map_key);
            if overflow {
                map_key = ("ui.observation_overflow", String::new());
            }
            let episode = entries.entry(map_key).or_insert_with(|| Episode {
                count: 0,
                began: Instant::now(),
            });
            episode.count = episode.count.saturating_add(1);
            (episode.count, episode.began.elapsed().as_millis(), overflow)
        };
        // The logger can block or panic; no observation mutex is held here.
        if count == 1 {
            log::warn!("UI degradation owner={} site={site} key={key:?} count=1 overflow={overflow} {context}", self.owner);
        } else if count == 60 {
            log::error!("persistent UI degradation owner={} site={site} key={key:?} count=60 elapsed_ms={elapsed} overflow={overflow} {context}", self.owner);
        }
    }
}
