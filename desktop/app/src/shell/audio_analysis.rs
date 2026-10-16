use crate::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

pub(crate) struct Analysis {
    object: u64,
    request: String,
    signature: Value,
    asset: motion_core::AudioAsset,
    source_offset: f64,
    offset: f64,
    begin: f64,
    end: f64,
    fps: u32,
    last: Instant,
    cancelled: Arc<AtomicBool>,
    worker: Option<mpsc::Receiver<Result<Vec<u32>, String>>>,
    media: Option<mpsc::Receiver<Result<Value, String>>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell() -> (tempfile::TempDir, Shell) {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let samples = 48_000u32 * 4;
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((36 + samples * 4).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(48_000u32.to_le_bytes());
        wav.extend(192_000u32.to_le_bytes());
        wav.extend(4u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend((samples * 4).to_le_bytes());
        let attacks = [36_000, 60_000, 84_000, 132_000, 168_000];
        for i in 0..samples {
            let gain: i16 = if attacks.iter().any(|t| *t <= i && i < *t + 480) {
                8192
            } else {
                0
            };
            wav.extend(gain.to_le_bytes());
            wav.extend((-gain).to_le_bytes());
        }
        std::fs::write(root.join("assets/attacks.wav"), &wav).unwrap();
        let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        project.audio_assets.push(motion_core::AudioAsset {
            id: 1,
            path: "assets/attacks.wav".into(),
            mime: "audio/wav".into(),
            bytes: wav.len() as u64,
            track: 0,
            sample_rate: 48_000,
            channels: 2,
            sample_frames: u64::from(samples),
            duration_us: 4_000_000,
        });
        let mut layer = Layer::solid(7, "Attacks", [0.; 2], [0.; 3], [1.; 4]);
        let mut audio = motion_core::AudioClip::new(1);
        audio.source_offset_us = 500_000;
        layer.content = motion_core::Content::Audio { audio };
        layer.timeline = Some(motion_core::LayerTimeline {
            in_frame: 10,
            out_frame: 100,
            offset_frame: 10,
        });
        project.layers.push(layer);
        motion_core::storage::save(&root, &project).unwrap();
        let mut shell = Shell::new(root, None, Some(Locale::En)).unwrap();
        shell.home = None;
        (folder, shell)
    }
    #[test]
    fn real_waveform_analysis_maps_source_offset_trim_and_clip_origin_and_undoes_once() {
        let (_folder, mut shell) = shell();
        shell.state = shell
            .engine
            .edit(
                serde_json::to_string(&Command::AudioMarkers {
                    object: 7,
                    action: motion_core::AudioMarkersAction::Add {
                        frame: 25,
                        label: "Manual".into(),
                    },
                })
                .unwrap(),
            )
            .unwrap();
        let before = shell.state["project"].clone();
        shell.action(panels::Action::AnalyzeAudio(7), None).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while shell.audio_analysis.is_some() {
            assert!(
                Instant::now() < deadline,
                "audio analysis timed out: media={} worker={} actions={:?} editing={} prompt={}",
                shell.audio_analysis.as_ref().unwrap().media.is_some(),
                shell.audio_analysis.as_ref().unwrap().worker.is_some(),
                shell.actions,
                shell.editing.is_some(),
                shell.project_prompt.is_some()
            );
            shell.poll_audio_analysis().unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        let action = shell.actions.pop_front().unwrap();
        assert!(matches!(&action, panels::Action::EditChecked { .. }));
        shell.action(action, None).unwrap();
        shell.state = shell
            .editing
            .take()
            .unwrap()
            .1
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let markers = shell.state["project"]["layers"][0]["audio_markers"]
            .as_array()
            .unwrap();
        assert_eq!(
            markers
                .iter()
                .filter(|m| m["generated"] == true)
                .map(|m| m["frame"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            [8, 23, 38, 68]
        );
        assert!(markers
            .iter()
            .any(|m| m["label"] == "Manual" && m["frame"] == 15));
        let after = shell.state["project"].clone();
        assert_eq!(shell.engine.save().unwrap()["project"], after);
        assert_eq!(shell.engine.history(0).unwrap()["project"], before);
        assert_eq!(shell.engine.history(1).unwrap()["project"], after);
    }
    #[test]
    fn cancelled_or_stale_analysis_never_edits_a_changed_clip() {
        let (_folder, mut shell) = shell();
        let before = shell.state["project"].clone();
        shell.analyze_audio(7).unwrap();
        shell.cancel_audio_analysis();
        assert!(shell.audio_analysis.is_none());
        assert!(shell.actions.is_empty());
        assert_eq!(shell.engine.state().unwrap()["project"], before);
        shell.analyze_audio(7).unwrap();
        shell.state = shell
            .engine
            .edit(
                serde_json::to_string(&Command::MoveLayerClip {
                    object: 7,
                    in_frame: 20,
                })
                .unwrap(),
            )
            .unwrap();
        shell.poll_audio_analysis().unwrap();
        assert!(shell.audio_analysis.is_none());
        assert!(shell.actions.is_empty());
        assert!(shell.state["project"]["layers"][0]["audio_markers"].is_null());
    }
}
impl Drop for Analysis {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
fn signature(state: &Value, object: u64) -> Option<Value> {
    let project = &state["project"];
    let layer = project["layers"]
        .as_array()?
        .iter()
        .find(|l| l["id"] == object && l["locked"] != true)?;
    let kind = layer["content"]["kind"].as_str()?;
    if !matches!(kind, "audio" | "video") {
        return None;
    }
    let source = &layer["content"][kind];
    let asset_id = if kind == "audio" {
        source["asset"].as_u64()?
    } else {
        project["video_assets"]
            .as_array()?
            .iter()
            .find(|a| a["id"] == source["asset"])?["audio_asset"]
            .as_u64()?
    };
    let asset = project["audio_assets"]
        .as_array()?
        .iter()
        .find(|a| a["id"] == asset_id)?;
    Some(serde_json::json!({
        "root":state["root"],"composition":state["composition"],"fps":project["fps"],"frames":project["frames"],
        "source":asset_id,"source_offset":source["source_offset_us"],"asset":asset,
        "timeline":layer["timeline"],"range":layer["subframe_range"],"channel":layer["audio_source_channel"],
    }))
}
fn word(locale: Locale, zh: &str, en: &str) -> String {
    if locale == Locale::Zh { zh } else { en }.into()
}
impl Shell {
    pub(crate) fn analyze_audio(&mut self, object: u64) -> Result<(), String> {
        if self.audio_analysis.is_some() {
            return Err(word(
                self.workspace.locale,
                "音频分析进行中，可在音频右键菜单取消",
                "Audio analysis is running; cancel it in the Audio menu",
            ));
        }
        if self.state["observing"] == true {
            return Err("audio analysis is unavailable while observing".into());
        }
        let expected = signature(&self.state, object).ok_or("select an unlocked audio source")?;
        let project: motion_core::Project =
            serde_json::from_value(self.state["project"].clone()).map_err(|e| e.to_string())?;
        let layer = project
            .layers
            .iter()
            .find(|l| l.id == object)
            .ok_or("audio layer missing")?;
        let audio = project.layer_audio(layer).ok_or("audio source missing")?;
        let asset = project
            .audio_assets
            .iter()
            .find(|a| a.id == audio.asset)
            .ok_or("audio asset missing")?
            .clone();
        let clip = layer.interval(project.frames);
        let request = format!(
            "desktop-audio-references-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        );
        let text = serde_json::json!({"op":"prepare_audio","request_id":request,"asset":asset.id})
            .to_string();
        let media = self
            .engine
            .request(|reply| engine::Command::Media { text, reply })?;
        self.audio_analysis = Some(Analysis {
            object,
            request,
            signature: expected,
            asset,
            source_offset: audio.source_offset_us as f64 / 1_000_000.,
            offset: f64::from(layer.clip(project.frames).offset_frame),
            begin: clip.in_frame.max(0.),
            end: clip.out_frame.min(f64::from(project.frames)),
            fps: project.fps,
            last: Instant::now() - Duration::from_secs(1),
            cancelled: Arc::new(AtomicBool::new(false)),
            worker: None,
            media: Some(media),
        });
        self.panels.timeline.expanded.insert(object);
        self.panels.timeline.collapsed_audio.remove(&object);
        self.panels.message = Some(word(
            self.workspace.locale,
            "正在分析音频节奏参考…",
            "Analyzing audio references…",
        ));
        self.dirty = true;
        Ok(())
    }
    pub(crate) fn cancel_audio_analysis(&mut self) {
        if let Some(task) = self.audio_analysis.take() {
            if task.worker.is_none() {
                for op in ["cancel_media_task", "release_media_task"] {
                    let text = serde_json::json!({"op":op,"request_id":task.request}).to_string();
                    let _ = self
                        .engine
                        .request(|reply| engine::Command::Media { text, reply });
                }
            }
            self.panels.message = Some(word(
                self.workspace.locale,
                "已取消音频分析",
                "Audio analysis cancelled",
            ));
            self.dirty = true;
        }
    }
    pub(crate) fn poll_audio_analysis(&mut self) -> Result<(), String> {
        let result = self.poll_audio_analysis_inner();
        if result.is_err() {
            self.cancel_audio_analysis();
        }
        result
    }
    fn poll_audio_analysis_inner(&mut self) -> Result<(), String> {
        self.state["desktop_audio_analysis"] = self.audio_analysis.as_ref().map_or(
            Value::Null,
            |task| serde_json::json!({"object":task.object}),
        );
        let Some(task) = self.audio_analysis.as_mut() else {
            return Ok(());
        };
        if signature(&self.state, task.object).as_ref() != Some(&task.signature)
            || self.state["observing"] == true
        {
            self.cancel_audio_analysis();
            return Ok(());
        }
        // Finish only after pending edits are acknowledged, so the source check
        // sees the same clip that the atomic marker command will edit.
        if self.editing.is_some() || !self.actions.is_empty() || self.project_prompt.is_some() {
            return Ok(());
        }
        if let Some(receiver) = &task.worker {
            match receiver.try_recv() {
                Ok(result) => {
                    let object = task.object;
                    self.audio_analysis = None;
                    let frames = result?;
                    let count = frames.len();
                    self.actions.push_back(panels::Action::EditChecked {
                        revision: self.state["revision"]
                            .as_u64()
                            .ok_or("project revision missing")?,
                        command: Command::InComposition {
                            composition: self.state["composition"]
                                .as_str()
                                .ok_or("composition missing")?
                                .into(),
                            command: Box::new(Command::AudioMarkers {
                                object,
                                action: motion_core::AudioMarkersAction::ReplaceGenerated {
                                    frames,
                                },
                            }),
                        },
                    });
                    self.panels.message = Some(format!(
                        "{} {count}",
                        word(
                            self.workspace.locale,
                            "音频参考标记：",
                            "Audio reference markers:"
                        )
                    ));
                    self.dirty = true;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.audio_analysis = None;
                    return Err("audio analysis worker stopped".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            return Ok(());
        }
        let status = if let Some(receiver) = &task.media {
            match receiver.try_recv() {
                Ok(result) => {
                    task.media = None;
                    result?
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(()),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("audio preparation worker stopped".into())
                }
            }
        } else {
            if task.last.elapsed() < Duration::from_millis(100) {
                return Ok(());
            }
            task.last = Instant::now();
            let text =
                serde_json::json!({"op":"media_status","request_id":task.request}).to_string();
            task.media = Some(
                self.engine
                    .request(|reply| engine::Command::Media { text, reply })?,
            );
            return Ok(());
        };
        match super::media::checked_task_state(&status, &task.request)? {
            "ready" | "succeeded" => {
                let text = serde_json::json!({"op":"release_media_task","request_id":task.request})
                    .to_string();
                let _ = self
                    .engine
                    .request(|reply| engine::Command::Media { text, reply })?;
                let root =
                    PathBuf::from(self.state["root"].as_str().ok_or("project root missing")?);
                let asset = task.asset.clone();
                let cancel = task.cancelled.clone();
                let (offset, source, begin, end, fps) = (
                    task.offset,
                    task.source_offset,
                    task.begin,
                    task.end,
                    task.fps,
                );
                let (tx, rx) = mpsc::channel();
                std::thread::Builder::new()
                    .name("audio-reference-analysis".into())
                    .spawn(move || {
                        let result = motion_media::audio_references(
                            &root,
                            &asset,
                            source + (begin - offset) / f64::from(fps),
                            source + (end - offset) / f64::from(fps),
                            &mut |_| {
                                if cancel.load(Ordering::Relaxed) {
                                    Err("cancelled".into())
                                } else {
                                    Ok(())
                                }
                            },
                        )
                        .and_then(|times| {
                            let mut frames: Vec<_> = times
                                .into_iter()
                                .map(|t| ((t - source) * f64::from(fps) + offset).round())
                                .filter(|f| *f >= begin && *f < end)
                                .map(|f| f as u32)
                                .collect();
                            frames.sort_unstable();
                            frames.dedup();
                            if frames.len() > motion_core::audio_edit::MAX_AUDIO_MARKERS {
                                return Err("too many references; analyze a shorter clip".into());
                            }
                            Ok(frames)
                        });
                        let _ = tx.send(result);
                    })
                    .map_err(|e| e.to_string())?;
                task.worker = Some(rx);
            }
            "failed" | "cancelled" => {
                let error = status["error"]
                    .as_str()
                    .unwrap_or("audio preparation failed")
                    .to_owned();
                self.cancel_audio_analysis();
                return Err(error);
            }
            "running" => {}
            state => return Err(format!("unexpected audio preparation state: request={} state={state}", task.request)),
        }
        Ok(())
    }
}
