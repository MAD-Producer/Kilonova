use std::time::Instant;

/// Lifecycle IO belongs to the producer, never the realtime device callback.
pub(super) struct Observation {
    playback: u64,
    began: Instant,
    reason: Option<&'static str>,
}
impl Observation {
    pub(super) fn new(playback: u64) -> Self {
        let observation = Self {
            playback,
            began: Instant::now(),
            reason: None,
        };
        log::info!("audio mixer start playback={playback} operation=audio_mix");
        crate::logging::flush();
        observation
    }
    pub(super) fn stopped(&mut self, reason: &'static str) {
        self.reason = Some(reason);
    }
    pub(super) fn failed(&mut self, phase: &'static str, error: &str) {
        self.stopped(phase);
        log::error!(
            "audio mixer failed playback={} operation=audio_mix {}ms phase={phase} reason={error}",
            self.playback,
            self.began.elapsed().as_millis()
        );
    }
}
impl Drop for Observation {
    fn drop(&mut self) {
        if let Some(reason) = self.reason {
            log::info!(
                "audio mixer stopped playback={} operation=audio_mix {}ms reason={reason}",
                self.playback,
                self.began.elapsed().as_millis()
            );
            crate::logging::flush();
        } else {
            let reason = if std::thread::panicking() {
                "unwind"
            } else {
                "unexpected_return"
            };
            log::error!("audio mixer exited unexpectedly playback={} operation=audio_mix {}ms reason={reason}",
                self.playback, self.began.elapsed().as_millis());
        }
    }
}

#[cfg(test)]
mod tests;
