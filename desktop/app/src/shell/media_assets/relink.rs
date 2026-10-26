//! The event thread only queues work and collects replies. Preparation and the
//! full-catalog commit remain on the existing observable serial engine worker.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RelinkPhase {
    Prepare,
    Status,
    Commit,
    Cancel,
}

impl Shell {
    pub(super) fn queue_asset_relink(
        &mut self,
        phase: RelinkPhase,
        value: Value,
    ) -> Result<(), String> {
        let text = value.to_string();
        let reply = self
            .engine
            .request(|reply| engine::Command::Media { text, reply })?;
        self.relink_reply = Some((phase, reply));
        Ok(())
    }

    pub(crate) fn cancel_asset_relink(&mut self) -> Result<(), String> {
        if self
            .relink_reply
            .as_ref()
            .is_some_and(|(phase, _)| *phase == RelinkPhase::Cancel)
        {
            return Ok(());
        }
        let request = &self
            .importing_media
            .as_ref()
            .ok_or("asset relink slot missing")?
            .0;
        // Replacing a pending receiver is an explicit cancellation boundary.
        // The queued worker still completes; cancellation reports its true state,
        // including a commit which completed before this request reached it.
        self.queue_asset_relink(
            RelinkPhase::Cancel,
            serde_json::json!({"op":"cancel_asset_relink","request_id":request}),
        )?;
        self.asset_message("正在取消重链接…", "Cancelling relink…");
        Ok(())
    }

    fn finish_asset_relink_reply(
        &mut self,
        request: &str,
        value: Value,
        cancelled: bool,
    ) -> Result<(), String> {
        if !value["project"].is_object() {
            return Err(format!("invalid asset relink snapshot: request={request}"));
        }
        let text =
            serde_json::json!({"op":"release_asset_relink","request_id":request}).to_string();
        // Release is explicitly idempotent and its reply is best effort. Worker
        // faults retain the normal engine error channel. Dispatch failures must
        // not turn an already committed edit into a reported rollback.
        if let Err(cause) = self
            .engine
            .request(|reply| engine::Command::Media { text, reply })
        {
            log::error!("asset relink cleanup dispatch failed request={request} phase=release cause={cause}");
        }
        self.accept_asset_relink(value, cancelled);
        Ok(())
    }

    pub(crate) fn poll_asset_relink(&mut self) -> Result<(), String> {
        let request = self
            .importing_media
            .as_ref()
            .ok_or("asset relink slot missing")?
            .0
            .clone();
        let Some((phase, receiver)) = &self.relink_reply else {
            let last = &mut self
                .importing_media
                .as_mut()
                .ok_or("asset relink slot missing")?
                .1;
            if last.elapsed() < Duration::from_millis(100) {
                return Ok(());
            }
            *last = Instant::now();
            return self.queue_asset_relink(
                RelinkPhase::Status,
                serde_json::json!({"op":"media_status","request_id":request}),
            );
        };
        let phase = *phase;
        let value = match receiver.try_recv() {
            Ok(value) => value?,
            Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err(format!(
                    "asset relink worker disconnected request={request} phase={phase:?}"
                ));
            }
        };
        self.relink_reply = None;
        if phase == RelinkPhase::Commit {
            let cancelled = value["asset_operation_cancelled"] == true;
            return self.finish_asset_relink_reply(&request, value["state"].clone(), cancelled);
        }
        match super::super::media::checked_task_state(&value, &request)? {
            "running" => {
                if phase == RelinkPhase::Cancel {
                    return Err(format!(
                        "asset relink cancellation still running request={request}"
                    ));
                }
                let label = if self.workspace.locale == Locale::Zh {
                    "检查丢失素材"
                } else {
                    "Checking missing assets"
                };
                self.panels.message = Some(format!(
                    "{label} {:.0}%",
                    value["progress"].as_f64().unwrap_or(0.) * 100.
                ));
                self.dirty = true;
                Ok(())
            }
            "ready" => self.queue_asset_relink(
                RelinkPhase::Commit,
                serde_json::json!({"op":"finish_asset_relink","request_id":request}),
            ),
            "cancelled" => {
                self.finish_asset_relink_reply(&request, value["snapshot"].clone(), true)
            }
            "succeeded" if phase == RelinkPhase::Cancel => {
                self.finish_asset_relink_reply(&request, value["snapshot"].clone(), false)
            }
            "failed" => Err(value["error"]
                .as_str()
                .unwrap_or("asset relink failed without a cause")
                .into()),
            state => Err(format!(
                "unexpected asset relink state request={request} phase={phase:?} state={state}"
            )),
        }
    }
}
