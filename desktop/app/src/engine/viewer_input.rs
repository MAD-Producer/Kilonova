//! Explicit per-view geometry queries. Superseded UI work is cancellation;
//! invalid cameras and geometry retain their original error through the worker.
use super::viewer_frame::Camera;
use crate::layer_clipboard::KeyframeSource;
use motion_host::session::Session;
use serde_json::{json, Value};

pub(crate) struct Request {
    pub window: u64,
    pub source: KeyframeSource,
    pub frame: f64,
    pub query: Query,
}
pub(crate) enum Query {
    States(Vec<Camera>),
    Pick { camera: Camera, point: [f64; 2] },
}
impl Request {
    pub(super) fn operation(&self) -> &'static str {
        match self.query {
            Query::States(_) => "viewer_states",
            Query::Pick { .. } => "viewer_pick",
        }
    }
}
pub(super) fn apply(session: &Session, request: Request, closed: bool) -> motion_host::Result<Value> {
    if closed || !request.source.current(session) || request.frame.to_bits() != session.frame.to_bits() {
        return Ok(json!({"cancelled":true, "window":request.window,
            "reason":if closed {"window_closed"} else {"context_changed"}}));
    }
    let mut result = json!({"cancelled":false,"window":request.window,"frame":session.frame});
    match request.query {
        Query::States(cameras) => {
            if !matches!(cameras.len(), 1 | 2 | 4) {
                return Err("multiview phase=input_states: expected 1, 2, or 4 cameras".into());
            }
            let states = cameras.iter().enumerate().map(|(index, camera)| {
                session.snapshot_for_view(camera.borrow()).map_err(|cause|
                    motion_host::Error::with_context(format!("multiview view={index} phase=input_states"), cause))
            }).collect::<motion_host::Result<Vec<_>>>()?;
            result["views"] = Value::Array(states);
        }
        Query::Pick { camera, point } => {
            result["pick"] = session.hit_candidates_for_view(camera.borrow(), point)?;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
