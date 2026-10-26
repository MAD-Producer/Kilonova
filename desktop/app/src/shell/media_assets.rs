//! Asset operations share the existing engine, import worker and history.
use crate::media_assets::{Intent, Source};
use crate::*;
use motion_core::media_assets::{AssetAction, AssetKind};

impl Shell {
    fn asset_message(&mut self, zh: &str, en: &str) {
        self.panels.message = Some(
            if self.workspace.locale == Locale::Zh {
                zh
            } else {
                en
            }
            .into(),
        );
        self.dirty = true;
    }
    pub(super) fn asset_action(
        &mut self,
        source: Source,
        intent: Intent,
        events: Option<&ActiveEventLoop>,
    ) -> Result<(), String> {
        let asset = source.target;
        let revision = source.revision;
        let began = Instant::now();
        let result = self.asset_action_inner(source, intent, events);
        if let Err(cause) = &result {
            log::error!("asset action failed asset={} kind={:?} revision={revision} elapsed_ms={} cause={cause}",asset.id,asset.kind,began.elapsed().as_millis());
        }
        result
    }
    fn asset_action_inner(
        &mut self,
        source: Source,
        intent: Intent,
        events: Option<&ActiveEventLoop>,
    ) -> Result<(), String> {
        if !source.current(&self.state) {
            self.asset_message(
                "工程已变化，请重新选择素材",
                "Project changed; select the asset again",
            );
            return Ok(());
        }
        match intent {
            Intent::Replace => {
                if self.importing_image.is_some() || self.importing_media.is_some() {
                    return Err(self.workspace.locale.t(T::Importing).into());
                }
                let title = if self.workspace.locale == Locale::Zh {
                    "重链接 / 替换素材"
                } else {
                    "Relink / replace asset"
                };
                let extensions: &[&str] = match source.target.kind {
                    AssetKind::Image => &["png", "jpg", "jpeg"],
                    AssetKind::Audio => {
                        &["wav", "flac", "ogg", "opus", "mp3", "m4a", "aac", "aiff"]
                    }
                    AssetKind::Video => &["mp4", "mov", "mkv", "webm", "3gp"],
                };
                log::info!(
                    "asset picker start asset={} kind={:?} revision={} phase=select",
                    source.target.id,
                    source.target.kind,
                    source.revision
                );
                match rfd::FileDialog::new()
                    .set_title(title)
                    .add_filter(title, extensions)
                    .pick_file()
                {
                    Some(path) => self.replace_asset_file(source, path)?,
                    None => self.asset_message(
                        "已取消替换，原素材保持不变",
                        "Replacement cancelled; original asset retained",
                    ),
                }
            }
            Intent::RevealFile => {
                let project: motion_core::Project =
                    serde_json::from_value(self.state["project"].clone())
                        .map_err(|e| e.to_string())?;
                let asset = motion_core::media_assets::lookup(&project, source.target)
                    .ok_or("asset missing")?;
                let path = PathBuf::from(&source.root).join(asset.path());
                self.poll_file_location()?;
                if self.file_location.is_some() {
                    self.asset_message("文件位置正在打开", "File location is already opening");
                    return Ok(());
                }
                self.file_location = reveal_file(&path, source.target.id)?;
            }
            Intent::Remove => {
                self.stop();
                self.state = self.engine.edit_at_revision(
                    serde_json::to_string(&Command::MediaAsset {
                        action: AssetAction::Remove {
                            target: source.target,
                        },
                    })
                    .map_err(|e| e.to_string())?,
                    source.revision,
                )?;
                self.asset_message(
                    "已从工程中移除素材，可撤销；文件保留",
                    "Asset removed from project; file retained for undo",
                );
            }
            Intent::RevealUsage(usage) => {
                self.stop();
                let actual = self.engine.media(
                    serde_json::json!({"op":"media_asset_usages","target":source.target})
                        .to_string(),
                )?;
                let usages: Vec<motion_core::media_assets::AssetUsage> =
                    serde_json::from_value(actual["usages"].clone()).map_err(|e| e.to_string())?;
                if !usages.iter().any(|u| {
                    u.composition == usage.composition
                        && u.object == usage.object
                        && u.role == usage.role
                }) {
                    self.asset_message(
                        "使用处已变化，请重新打开菜单",
                        "Usage changed; reopen the asset menu",
                    );
                    return Ok(());
                }
                self.state = self.engine.composition(
                    serde_json::json!({"version":1,"op":"open","composition":usage.composition})
                        .to_string(),
                )?;
                self.panels.normalize_selection(&self.state);
                self.panels.select_only(Some(usage.object));
                self.panels.focused = Some(dock::Panel::Timeline);
                self.panels.timeline.reveal_layer = Some(usage.object);
                self.action(
                    panels::Action::ShowPanel(dock::Panel::Timeline.dock()),
                    events,
                )?;
                self.asset_message("已定位素材使用图层", "Selected the layer using this asset");
            }
        }
        self.dirty = true;
        Ok(())
    }
    pub(crate) fn replace_asset_file(
        &mut self,
        source: Source,
        path: PathBuf,
    ) -> Result<(), String> {
        if !source.current(&self.state) {
            self.asset_message(
                "工程已变化，已取消替换",
                "Project changed; replacement cancelled",
            );
            return Ok(());
        }
        if self.importing_image.is_some() || self.importing_media.is_some() {
            return Err(self.workspace.locale.t(T::Importing).into());
        }
        self.stop();
        match source.target.kind {
            AssetKind::Image => {
                self.importing_image =
                    Some(self.engine.request(|reply| engine::Command::ReplaceImage {
                        file: path,
                        asset: source.target.id,
                        revision: source.revision,
                        reply,
                    })?)
            }
            AssetKind::Audio | AssetKind::Video => {
                let request = format!(
                    "desktop-replace-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| e.to_string())?
                        .as_nanos()
                );
                let kind = if source.target.kind == AssetKind::Audio {
                    "audio"
                } else {
                    "video"
                };
                self.engine.media(serde_json::json!({"op":"import_media","request_id":request,"path":path,"kind":kind,"at_frame":0}).to_string())?;
                self.importing_media = Some((request, Instant::now() - Duration::from_secs(1)));
            }
        }
        self.replacing_asset = Some(source);
        self.asset_message("正在检查替换素材…", "Checking replacement asset…");
        Ok(())
    }
    pub(super) fn accept_asset_replacement(&mut self, state: Value, cancelled: bool) {
        self.state = state;
        self.replacing_asset = None;
        if cancelled {
            self.asset_message(
                "工程已变化，已取消替换",
                "Project changed; replacement cancelled",
            );
        } else {
            self.asset_message(
                "素材已替换，图层和引用保持不变",
                "Asset replaced; layers and references retained",
            );
        }
    }
    pub(crate) fn poll_file_location(&mut self) -> Result<(), String> {
        let Some(task) = &mut self.file_location else {
            return Ok(());
        };
        if task.polled.elapsed() < Duration::from_millis(100) {
            return Ok(());
        }
        task.polled = Instant::now();
        let result = match task.child.try_wait() {
            Ok(None) => return Ok(()), // A live external utility is not a fault.
            Ok(Some(status)) if status.success() => Ok(()),
            Ok(Some(status)) => Err(format!(
                "file location asset={} phase=utility_exit path={:?}: {status}",
                task.asset, task.path
            )),
            Err(cause) => Err(format!(
                "file location asset={} phase=utility_wait path={:?}: {cause}",
                task.asset, task.path
            )),
        };
        if let Err(cause) = &result {
            log::error!(
                "file location failed elapsed_ms={} cause={cause}",
                task.began.elapsed().as_millis()
            );
        } else {
            log::debug!(
                "file location done asset={} elapsed_ms={}",
                task.asset,
                task.began.elapsed().as_millis()
            );
        }
        self.file_location = None;
        result
    }
}

/// One external launcher is retained and polled; no detached wait thread.
pub(crate) struct Location {
    child: std::process::Child,
    asset: u64,
    path: String,
    began: Instant,
    polled: Instant,
}
fn reveal_file(path: &std::path::Path, asset: u64) -> Result<Option<Location>, String> {
    let began = Instant::now();
    let bounded: String = path.to_string_lossy().chars().take(1024).collect();
    log::info!("asset file location start asset={asset} phase=launch path={bounded:?}");
    let result = (|| {
        // Missing sources reveal the closest existing parent so relinking remains useful.
        let folder = existing_parent(path)?;
        #[cfg(target_os = "windows")]
        return open_windows_folder(folder).map(|()| None);
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut c = std::process::Command::new("open");
            if path.is_file() {
                c.arg("-R").arg(path);
            } else {
                c.arg(folder);
            }
            c
        };
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let mut command = {
            let mut c = std::process::Command::new("xdg-open");
            c.arg(folder);
            c
        };
        // Explorer/open own their windows; detach their inherited protocol streams.
        #[cfg(not(target_os = "windows"))]
        let child = command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| {
                format!("file location asset={asset} phase=launch path={bounded:?}: {error}")
            })?;
        #[cfg(not(target_os = "windows"))]
        Ok(Some(Location {
            child,
            asset,
            path: bounded.clone(),
            began,
            polled: Instant::now(),
        }))
    })();
    match &result {
        Ok(_) => log::debug!(
            "asset file location launched asset={asset} elapsed_ms={}",
            began.elapsed().as_millis()
        ),
        Err(error) => log::error!(
            "asset file location failed asset={asset} elapsed_ms={} cause={error}",
            began.elapsed().as_millis()
        ),
    }
    result
}

fn existing_parent(path: &std::path::Path) -> Result<&std::path::Path, String> {
    let mut folder = path.parent().ok_or("asset path has no parent")?;
    loop {
        match std::fs::metadata(folder) {
            Ok(info) if info.is_dir() => return Ok(folder),
            Ok(_) => {
                return Err(format!(
                    "asset parent is not a directory: {}",
                    folder.display()
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                folder = folder.parent().ok_or("no existing parent for asset path")?
            }
            Err(error) => {
                return Err(format!(
                    "inspect asset parent {}: {error}",
                    folder.display()
                ))
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn open_windows_folder(folder: &std::path::Path) -> Result<(), String> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> *mut c_void;
    }
    let mut path: Vec<u16> = folder.as_os_str().encode_wide().collect();
    if path.contains(&0) {
        return Err("file location contains a NUL character".into());
    }
    path.push(0);
    let verb: Vec<u16> = "explore".encode_utf16().chain(Some(0)).collect();
    // The shell API acknowledges a request; its value is not a process handle.
    // https://learn.microsoft.com/windows/win32/api/shellapi/nf-shellapi-shellexecutew
    let code = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    } as isize;
    if code <= 32 {
        Err(format!(
            "ShellExecuteW rejected file location: shell_error={code}"
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
