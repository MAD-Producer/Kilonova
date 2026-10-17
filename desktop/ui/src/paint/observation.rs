use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

static NEXT: AtomicU64 = AtomicU64::new(1);
#[derive(Default)]
struct Episode {
    count: usize,
    began: Option<Instant>,
}
impl Episode {
    fn record(&mut self, owner: u64, site: &str, failed: bool, context: fmt::Arguments<'_>) {
        if !failed {
            *self = Self::default();
            return;
        }
        self.count = self.count.saturating_add(1);
        if self.count == 1 {
            self.began = Some(Instant::now());
            log::warn!("UI paint degradation painter={owner} site={site} count=1 {context}");
        } else if self.count == 60 {
            let elapsed = self.began.map_or(0, |began| began.elapsed().as_millis());
            log::error!("persistent UI paint degradation painter={owner} site={site} count=60 elapsed_ms={elapsed} {context}");
        }
    }
}
pub(super) struct Observation {
    owner: u64,
    order: Episode,
    atlas: Episode,
}
impl Default for Observation {
    fn default() -> Self {
        Self {
            owner: NEXT.fetch_add(1, Ordering::Relaxed),
            order: Episode::default(),
            atlas: Episode::default(),
        }
    }
}
impl Observation {
    // Once per render, rather than once per rejected operation/batch. A healthy
    // render resets each independent episode; a new Painter starts fresh.
    pub(super) fn order(
        &mut self,
        invalid: usize,
        first: Option<super::PaintOp>,
        shapes: usize,
        glyphs: usize,
    ) {
        self.order.record(self.owner,"paint.order",invalid != 0,format_args!("invalid_ops={invalid} first={first:?} shapes={shapes} glyphs={glyphs} fallback=skip_invalid_ops"));
    }
    pub(super) fn atlas(&mut self, missing: bool, glyphs: usize) {
        self.atlas.record(
            self.owner,
            "paint.atlas",
            missing,
            format_args!("glyphs={glyphs} fallback=skip_glyph_batches"),
        );
    }
}
