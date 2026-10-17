use std::{sync::mpsc, time::Instant};

type Outcome = Result<Vec<u32>, String>;
struct Observation {
    request: String,
    object: u64,
    source: u64,
    began: Instant,
    finished: bool,
}
impl Drop for Observation {
    fn drop(&mut self) {
        if !self.finished {
            let reason = if std::thread::panicking() {
                "unwind"
            } else {
                "unexpected_return"
            };
            log::error!("audio analysis worker exited unexpectedly request={} operation=audio_reference_analysis object={} source={} {}ms reason={reason}",
                self.request, self.object, self.source, self.began.elapsed().as_millis());
        }
    }
}

pub(super) fn spawn(
    request: String,
    object: u64,
    source: u64,
    work: impl FnOnce() -> Outcome + Send + 'static,
) -> Result<mpsc::Receiver<Outcome>, String> {
    let (tx, rx) = mpsc::channel();
    let worker_request = request.clone();
    std::thread::Builder::new()
        .name("audio-reference-analysis".into())
        .spawn(move || {
            let mut observation = Observation {
                request: worker_request, object, source, began: Instant::now(), finished: false,
            };
            let request = &observation.request;
            log::info!("audio analysis start request={request} operation=audio_reference_analysis object={object} source={source}");
            crate::logging::flush();
            // Keep the original panic/disconnection contract. The process hook
            // preserves the stack; the outer guard adds task identity on unwind.
            let result = work();
            let error = result.as_ref().err().cloned();
            let count = result.as_ref().ok().map(Vec::len);
            let _ = tx.send(result);
            observation.finished = true;
            let elapsed = observation.began.elapsed().as_millis();
            match error {
                Some(error) if error != "cancelled" => log::error!(
                    "audio analysis failed request={request} operation=audio_reference_analysis object={object} source={source} {elapsed}ms reason={error}"),
                Some(_) => log::debug!("audio analysis done request={request} operation=audio_reference_analysis object={object} source={source} {elapsed}ms phase=cancelled"),
                None => log::debug!("audio analysis done request={request} operation=audio_reference_analysis object={object} source={source} {elapsed}ms phase=complete markers={count:?}"),
            }
        })
        .map_err(|error| {
            log::error!("audio analysis spawn failed request={request} operation=audio_reference_analysis object={object} source={source} reason={error}");
            error.to_string()
        })?;
    Ok(rx)
}

#[cfg(test)]
mod tests;
