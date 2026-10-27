use crate::render_queue::controller::Intent;
use crate::*;

impl Shell {
    pub(crate) fn poll_render_queue(&mut self) {
        if self.render_queue.poll(&self.engine, &self.state) {
            self.dirty = true;
        }
        self.panels.render_queue = self.render_queue.presentation();
    }
    pub(crate) fn render_queue_action(&mut self, intent: Intent) -> Result<(), String> {
        if matches!(intent, Intent::Open) {
            self.render_queue.open();
            self.poll_render_queue();
            return Ok(());
        }
        if matches!(intent, Intent::Reload) {
            self.render_queue.reload();
            self.poll_render_queue();
            return Ok(());
        }
        if matches!(intent, Intent::CancelPreparation) {
            self.render_queue.cancel_preparation();
            return Ok(());
        }
        if !self.render_queue.has_capacity() {
            return Err("render queue UI command capacity reached".into());
        }
        let result = (|| {
            let ticket = match intent {
                Intent::Add(template) => {
                    if self.freezing_movie.is_some() || self.exporting_frame.is_some() {
                        return Err("another export is preparing its source".into());
                    }
                    let view = self.render_queue.queue()?.view();
                    let filename = template.filename(
                        self.state["project"]["name"]
                            .as_str()
                            .unwrap_or("Composition"),
                        view.document.next_id,
                        self.state["revision"]
                            .as_u64()
                            .ok_or("project revision is missing")?,
                    )?;
                    let output = rfd::FileDialog::new()
                        .add_filter("MP4", &["mp4"])
                        .set_file_name(&filename)
                        .save_file();
                    let Some(output) = output else {
                        return Ok(());
                    };
                    self.stop();
                    return self
                        .render_queue
                        .freeze(&self.engine, &self.state, template, output);
                }
                Intent::Start => self.render_queue.queue()?.start()?,
                Intent::Pause => self.render_queue.queue()?.pause()?,
                Intent::Cancel(id) => self.render_queue.queue()?.cancel(id)?,
                Intent::Retry(id) => self.render_queue.queue()?.retry(id, None)?,
                Intent::RetryAs(id) => {
                    let queue = self.render_queue.queue()?;
                    let view = queue.view();
                    let job = view
                        .document
                        .jobs
                        .iter()
                        .find(|job| job.id == id)
                        .ok_or("render task is missing")?;
                    let filename = job
                        .output
                        .file_name()
                        .and_then(|s| s.to_str())
                        .ok_or("render output filename is invalid")?;
                    let output = rfd::FileDialog::new()
                        .add_filter("MP4", &["mp4"])
                        .set_file_name(filename)
                        .save_file();
                    let Some(output) = output else {
                        return Ok(());
                    };
                    queue.retry(id, Some(output))?
                }
                Intent::Remove(id) => self.render_queue.queue()?.remove(id)?,
                Intent::SaveTemplate(template) => {
                    self.render_queue.queue()?.save_template(template)?
                }
                Intent::RemoveTemplate(id) => self.render_queue.queue()?.remove_template(id)?,
                Intent::Open | Intent::Reload | Intent::CancelPreparation => {
                    unreachable!("handled before dispatch")
                }
            };
            self.render_queue.accept(ticket, false)
        })();
        if let Err(cause) = &result {
            log::error!(
                "render queue UI dispatch failed root={} cause={}",
                fallback_observation::short(self.state["root"].as_str().unwrap_or("")),
                fallback_observation::error_cause(cause)
            );
        }
        self.dirty = true;
        result
    }
}
