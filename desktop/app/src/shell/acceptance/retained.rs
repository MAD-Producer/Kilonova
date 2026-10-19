//! Keep preview quality fixed while measuring UI-only repaint reuse.
use super::*;

impl Shell {
    pub(super) fn retained_ui_probe(&mut self, size: [f32; 2]) -> Result<Value, String> {
        let previous = self.engine.preview_info()?;
        let mode = match previous["mode"].as_str() {
            Some("auto") => 0,
            Some("high") => 1,
            Some("balanced") => 2,
            Some("economy") => 3,
            other => {
                return Err(format!(
                    "unknown preview mode in repaint fixture: {other:?}"
                ))
            }
        };
        // Auto may change tiers on the first render and invalidate its retained stamp.
        // This probe measures repaint reuse at unchanged quality, not adaptive preview.
        self.engine.preview_mode(2, 0)?;
        let result = (|| {
            let mut reuse_times = vec![];
            let before = self.engine.state()?;
            for index in 0..9 {
                let (paint, _) = self.draw_commands(size);
                let ui = self.ui_frame(paint, size, false);
                let began = Instant::now();
                if !self
                    .engine
                    .present_async(ui)?
                    .recv_timeout(Duration::from_secs(10))
                    .map_err(|e| e.to_string())??
                {
                    return Err("retained UI presentation failed".into());
                }
                if index > 0 {
                    reuse_times.push(began.elapsed().as_micros() as u64);
                }
            }
            let state = self.engine.state()?;
            let reuse_delta = state["desktop"]["uiPerformance"]["compositionReuses"]
                .as_u64()
                .unwrap_or(0)
                - before["desktop"]["uiPerformance"]["compositionReuses"]
                    .as_u64()
                    .unwrap_or(0);
            let render_delta = state["desktop"]["uiPerformance"]["compositionRenders"]
                .as_u64()
                .unwrap_or(0)
                - before["desktop"]["uiPerformance"]["compositionRenders"]
                    .as_u64()
                    .unwrap_or(0);
            if reuse_delta != 8 || render_delta != 1 {
                return Err(format!("UI repaint re-executed the composition: renders={render_delta}, reuses={reuse_delta}, stage={}", self.smoke_stage));
            }
            if before["preview"]["tier"] != state["preview"]["tier"]
                || state["preview"]["mode"] != "balanced"
            {
                return Err("retained repaint fixture changed preview quality".into());
            }
            Ok(
                serde_json::json!({"reuses":reuse_delta,"renders":render_delta,
                "roundTripUs":reuse_times,"mode":"balanced","tier":state["preview"]["tier"]}),
            )
        })();
        let restored = self.engine.preview_mode(mode, 0);
        match (result, restored) {
            (Err(primary), Err(cleanup)) => {
                log::error!("repaint fixture restore failed: primary={primary}; cleanup={cleanup}");
                Err(primary)
            }
            (Err(primary), _) => Err(primary),
            (Ok(_), Err(cleanup)) => Err(format!("repaint fixture restore failed: {cleanup}")),
            (Ok(report), Ok(_)) => Ok(report),
        }
    }
}
