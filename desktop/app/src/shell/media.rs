use crate::*;

impl Shell {
    pub(crate) fn import_file(&mut self, path: PathBuf) -> Result<(), String> {
        if self.importing_media.is_some() {
            return Err(self.workspace.locale.t(T::Importing).into());
        }
        self.stop();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
            self.importing_image = Some(
                self.engine
                    .request(|reply| engine::Command::ImportImage { file: path, reply })?,
            );
        } else {
            let kind = if matches!(ext.as_str(), "mp4" | "mov" | "mkv" | "webm" | "3gp") {
                "video"
            } else {
                "audio"
            };
            let request = format!(
                "desktop-import-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            );
            self.engine.media(serde_json::json!({"op":"import_media","request_id":request,"path":path,"kind":kind,"at_frame":self.frame.floor() as u32,"name":path.file_stem().and_then(|s|s.to_str()).unwrap_or("Media")}).to_string())?;
            self.importing_media = Some((request, Instant::now() - Duration::from_secs(1)));
        }
        self.panels.message = Some(self.workspace.locale.t(T::Importing).into());
        self.dirty = true;
        Ok(())
    }
    pub(crate) fn poll_import(&mut self) -> Result<(), String> {
        let result = self.poll_import_inner();
        if result.is_err() {
            self.replacing_asset = None;
            self.relinking_assets = false;
            if let Some((request, _)) = self.importing_media.take() {
                for op in ["cancel_media_task", "release_media_task"] {
                    let text = serde_json::json!({"op":op,"request_id":request}).to_string();
                    if let Err(error) = self.engine.request(|reply| engine::Command::Media {text,reply}) {
                        log::warn!("media import cleanup dispatch failed: request={request} op={op} reason={error}");
                    }
                }
            }
        }
        result
    }
    fn poll_import_inner(&mut self) -> Result<(), String> {
        if let Some(receiver) = &self.importing_image {
            match receiver.try_recv() {
                Ok(result) => {
                    self.importing_image = None;
                    let state = result?;
                    if self.replacing_asset.is_some() {
                        let cancelled=state["asset_operation_cancelled"]==true;
                        self.accept_asset_replacement(state,cancelled);
                        return Ok(());
                    }
                    self.state = state;
                    let selected = self.state["project"]["layers"]
                        .as_array()
                        .and_then(|ls| ls.last())
                        .and_then(|l| l["id"].as_u64());
                    self.panels.select_only(selected);
                    self.panels.message = Some(self.workspace.locale.t(T::Imported).into());
                    self.dirty = true;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.importing_image = None;
                    return Err("import worker stopped".into());
                }
                _ => {}
            }
        }
        if let Some((request, last)) = &mut self.importing_media {
            if last.elapsed() < Duration::from_millis(100) {
                return Ok(());
            }
            *last = Instant::now();
            let request = request.clone();
            let status = self
                .engine
                .media(serde_json::json!({"op":"media_status","request_id":request}).to_string())?;
            match checked_task_state(&status, &request)? {
                "ready" => {
                    let replacing = self.replacing_asset.clone();
                    let relinking = self.relinking_assets;
                    let finish = if relinking {
                        serde_json::json!({"op":"finish_asset_relink","request_id":request})
                    } else if let Some(source)=&replacing {
                        serde_json::json!({"op":"finish_media_replacement","request_id":request,"target":source.target,"revision":source.revision})
                    } else { serde_json::json!({"op":"finish_media_import","request_id":request}) };
                    let result = self.engine.media(
                        finish.to_string(),
                    );
                    self.importing_media = None;
                    if let Err(error) = self.engine.media(
                        serde_json::json!({"op":"release_media_task","request_id":request})
                            .to_string(),
                    ) {
                        log::warn!("media import task release failed: request={request} reason={error}");
                    }
                    let result=result?;
                    if relinking {
                        let cancelled=result["asset_operation_cancelled"]==true;
                        self.accept_asset_relink(result["state"].clone(),cancelled);
                        return Ok(());
                    }
                    if replacing.is_some() {
                        let cancelled=result["asset_operation_cancelled"]==true;
                        self.accept_asset_replacement(result["state"].clone(),cancelled);
                        return Ok(());
                    }
                    self.state = result["state"].clone();
                    let selected = self.state["project"]["layers"]
                        .as_array()
                        .and_then(|ls| ls.last())
                        .and_then(|l| l["id"].as_u64());
                    self.panels.select_only(selected);
                    self.panels.message = Some(self.workspace.locale.t(T::Imported).into());
                    self.dirty = true;
                }
                "failed" | "cancelled" => {
                    self.importing_media = None;
                    let relinking=std::mem::take(&mut self.relinking_assets);
                    if let Err(error) = self.engine.media(
                        serde_json::json!({"op":"release_media_task","request_id":request})
                            .to_string(),
                    ) {
                        log::warn!("media import task release failed: request={request} reason={error}");
                    }
                    if status["state"]=="cancelled" && relinking {
                        self.accept_asset_relink(self.state.clone(),true);
                        return Ok(());
                    }
                    if status["state"]=="cancelled" && self.replacing_asset.take().is_some() {
                        self.accept_asset_replacement(self.state.clone(),true);
                        return Ok(());
                    }
                    return Err(status["error"]
                        .as_str()
                        .unwrap_or("media import cancelled")
                        .into());
                }
                "running" => {
                    let label=if self.relinking_assets {
                        if self.workspace.locale==Locale::Zh {"检查丢失素材"}else{"Checking missing assets"}
                    } else {self.workspace.locale.t(T::Importing)};
                    self.panels.message = Some(format!(
                        "{} {:.0}%",
                        label,
                        status["progress"].as_f64().unwrap_or(0.) * 100.
                    ));
                    self.dirty = true;
                }
                state => return Err(format!("unexpected media import task state: request={request} state={state}")),
            }
        }
        Ok(())
    }
}

pub(super) fn checked_task_state<'a>(status: &'a Value, request: &str) -> Result<&'a str, String> {
    match status["state"].as_str() {
        Some(state @ ("running" | "ready" | "succeeded" | "failed" | "cancelled")) => Ok(state),
        state => Err(format!("invalid media task state: request={request} state={state:?}")),
    }
}

#[cfg(test)]
mod failure_tests {
    use super::*;

    #[test]
    fn invalid_task_states_are_terminal_protocol_errors() {
        for state in [Value::Null, serde_json::json!(3),serde_json::json!("unexpected")] {
            let status = serde_json::json!({"state":state});
            let error = checked_task_state(&status,"test-request").unwrap_err();
            assert!(error.contains("test-request"));
            assert!(error.contains("invalid media task state"));
        }
    }

    #[test]
    fn failed_status_query_clears_import_slot_instead_of_blocking_future_imports() {
        let directory = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(directory.path().join("project"),None,Some(Locale::En)).unwrap();
        let before = shell.state["project"].clone();
        shell.importing_media = Some(("missing-task".into(),Instant::now()-Duration::from_secs(1)));
        assert!(shell.poll_import().is_err());
        assert!(shell.importing_media.is_none());
        assert!(shell.poll_import().is_ok());
        assert_eq!(shell.state["project"],before);
    }

    #[test]
    fn actual_failed_import_releases_its_slot_and_preserves_the_project() {
        let directory = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(directory.path().join("project"),None,Some(Locale::En)).unwrap();
        let before = shell.state["project"].clone();
        let selected = directory.path().join("broken.wav");
        std::fs::write(&selected,b"invalid audio").unwrap();
        shell.import_file(selected).unwrap();
        let deadline = Instant::now()+Duration::from_secs(5);
        loop {
            if let Err(error) = shell.poll_import() {
                assert!(!error.contains("media import cancelled"),"{error}");
                break;
            }
            assert!(Instant::now()<deadline,"import did not report failure");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(shell.importing_media.is_none());
        assert_eq!(shell.state["project"],before);
        assert!(shell.poll_import().is_ok());
    }
}
