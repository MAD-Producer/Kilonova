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
        if let Some(receiver) = &self.importing_image {
            match receiver.try_recv() {
                Ok(result) => {
                    self.importing_image = None;
                    self.state = result?;
                    self.panels.selected = self.state["project"]["layers"]
                        .as_array()
                        .and_then(|ls| ls.last())
                        .and_then(|l| l["id"].as_u64());
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
            match status["state"].as_str().unwrap_or("") {
                "ready" => {
                    let result = self.engine.media(
                        serde_json::json!({"op":"finish_media_import","request_id":request})
                            .to_string(),
                    );
                    self.importing_media = None;
                    self.engine.media(
                        serde_json::json!({"op":"release_media_task","request_id":request})
                            .to_string(),
                    )?;
                    self.state = result?["state"].clone();
                    self.panels.selected = self.state["project"]["layers"]
                        .as_array()
                        .and_then(|ls| ls.last())
                        .and_then(|l| l["id"].as_u64());
                    self.panels.message = Some(self.workspace.locale.t(T::Imported).into());
                    self.dirty = true;
                }
                "failed" | "cancelled" => {
                    self.importing_media = None;
                    self.engine.media(
                        serde_json::json!({"op":"release_media_task","request_id":request})
                            .to_string(),
                    )?;
                    return Err(status["error"]
                        .as_str()
                        .unwrap_or("media import cancelled")
                        .into());
                }
                _ => {
                    self.panels.message = Some(format!(
                        "{} {:.0}%",
                        self.workspace.locale.t(T::Importing),
                        status["progress"].as_f64().unwrap_or(0.) * 100.
                    ));
                    self.dirty = true;
                }
            }
        }
        Ok(())
    }
}
