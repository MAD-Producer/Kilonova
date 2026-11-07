//! Speculative graph enumeration belongs exclusively to the preparation worker.
use super::*;
use motion_core::{Content, FrameRate, Layer, layer_time::LayerClock};

const MAX_WORK: usize = 16_384;
const MAX_EVENTS: usize = 1024;
const MAX_CROSSINGS: usize = 128;

pub(super) enum Failure {
    Cancelled,
    Failed(motion_render::RenderError),
}
type Result<T> = std::result::Result<T, Failure>;
#[derive(Clone, Copy)]
struct Link<'a> {
    layer: &'a Layer,
    parent_fps: FrameRate,
    child_fps: FrameRate,
    frames: u32,
    source_start: i32,
}
struct Scan<'a> {
    ticket: &'a PipelinePreparationTicket,
    work: usize,
    events: Vec<f64>,
    begin: f64,
    end: f64,
}
impl Scan<'_> {
    fn check(&mut self) -> Result<()> {
        if self.ticket.status().cancelled { return Err(Failure::Cancelled); }
        if self.work == 0 { return Err(Failure::Failed(motion_render::RenderError::Invalid(
            "pipeline lookahead phase=enumerate: graph work budget exceeded".into()))); }
        self.work -= 1;
        Ok(())
    }
    fn event(&mut self, path: &[Link<'_>], time: f64) -> Result<()> {
        self.check()?;
        let mut values = vec![time];
        for link in path.iter().rev() {
            let mut parents = Vec::new();
            let interval = link.layer.interval(link.frames);
            let clock = link.layer.time_mapping.as_ref().map_or_else(LayerClock::default, |m| m.clock);
            let offset = link.layer.timeline.map_or(0, |t| t.offset_frame);
            let a = clock.local_frame(interval.in_frame, offset).map_err(|e| Failure::Failed(e.into()))?;
            let b = clock.local_frame(interval.out_frame, offset).map_err(|e| Failure::Failed(e.into()))?;
            for value in values {
                self.check()?;
                let source = link.child_fps.map_frame(value - f64::from(link.source_start), link.parent_fps)
                    .map_err(|e| Failure::Failed(motion_render::RenderError::Invalid(e.to_string())))?;
                let locals = if let Some(remap) = link.layer.time_mapping.as_ref().and_then(|m| m.remap.as_ref()) {
                    remap.source_crossings_with_work_budget(source, a.min(b), a.max(b), MAX_CROSSINGS, &mut self.work)
                        .map_err(|e| Failure::Failed(e.into()))?
                } else { vec![source] };
                for local in locals {
                    let parent = clock.composition_frame(local, offset).map_err(|e| Failure::Failed(e.into()))?;
                    if parent >= interval.in_frame && parent <= interval.out_frame {
                        if parents.len() == MAX_CROSSINGS { return Err(Failure::Failed(motion_render::RenderError::Invalid(
                            "pipeline lookahead phase=invert: path crossing budget exceeded".into()))); }
                        parents.push(parent);
                    }
                }
            }
            parents.sort_by(f64::total_cmp);
            parents.dedup();
            values = parents;
        }
        for time in values {
            if time > self.begin && time <= self.end {
                if self.events.len() == MAX_EVENTS { return Err(Failure::Failed(motion_render::RenderError::Invalid(
                    "pipeline lookahead phase=events: event budget exceeded".into()))); }
                self.events.push(time);
            }
        }
        Ok(())
    }
    fn walk<'a>(&mut self, project: &'a Project, id: &str, path: &mut Vec<Link<'a>>) -> Result<()> {
        self.check()?;
        if path.len() >= motion_core::composition::MAX_COMPOSITION_DEPTH {
            return Err(Failure::Failed(motion_render::RenderError::Invalid("pipeline lookahead composition depth exceeded".into())));
        }
        let node = project.composition_view(id).map_err(|e| Failure::Failed(e.into()))?;
        // Composition entry/exit also matters when its visible layers cover
        // only part of the body, including a reference initially out of range.
        for time in [0., f64::from(node.frames)] { self.event(path, time)?; }
        for layer in node.layers {
            self.check()?;
            let interval = layer.interval(node.frames);
            for time in [interval.in_frame, interval.out_frame] { self.event(path, time)?; }
            let Content::Composition { clip } = &layer.content else { continue; };
            // Hold discontinuities and changes of remap piece are authored in
            // property time. They do not require the jump to hit a child edge.
            if let Some(mapping) = &layer.time_mapping {
                if let Some(remap) = &mapping.remap {
                    let a = mapping.clock.local_frame(interval.in_frame, layer.timeline.map_or(0, |t| t.offset_frame))
                        .map_err(|e| Failure::Failed(e.into()))?;
                    let b = mapping.clock.local_frame(interval.out_frame, layer.timeline.map_or(0, |t| t.offset_frame))
                        .map_err(|e| Failure::Failed(e.into()))?;
                    let first = remap.keys().partition_point(|key| key.frame < a.min(b));
                    let last = remap.keys().partition_point(|key| key.frame <= a.max(b));
                    for key in &remap.keys()[first..last] {
                        self.check()?;
                        let time = mapping.clock.composition_frame(key.frame, layer.timeline.map_or(0, |t| t.offset_frame))
                            .map_err(|e| Failure::Failed(e.into()))?;
                        if time >= interval.in_frame && time <= interval.out_frame { self.event(path, time)?; }
                    }
                }
            }
            let child = project.composition_view(&clip.composition).map_err(|e| Failure::Failed(e.into()))?;
            path.push(Link { layer, parent_fps: node.fps, child_fps: child.fps,
                frames: node.frames, source_start: clip.source_start_frame });
            self.walk(project, &clip.composition, path)?;
            path.pop();
        }
        Ok(())
    }
}

pub(super) fn frames(project: &Project, frame: f64, ticket: &PipelinePreparationTicket) -> Result<Vec<f64>> {
    if ticket.status().cancelled { return Err(Failure::Cancelled); }
    if project.compositions.is_empty() { return Ok(upcoming(project, frame)); }
    let end = (frame + project.fps.as_f64() * 2.).min(f64::from(project.frames.saturating_sub(1)));
    let mut scan = Scan { ticket, work: MAX_WORK, events: Vec::new(), begin: frame, end };
    scan.walk(project, &project.composition_id, &mut Vec::new())?;
    scan.events.sort_by(f64::total_cmp);
    scan.events.dedup();
    // Sample inside each following interval, not at a numerically inverted
    // boundary: rounding and reverse half-open membership must not select the
    // wrong side. Root-only sampling retains its existing exact boundaries.
    let mut result = Vec::new();
    for i in 0..scan.events.len() {
        let left = scan.events[i];
        let right = scan.events.get(i + 1).copied().unwrap_or(end);
        result.push(if left < right { left + (right - left) / 2. } else { left });
        if result.len() == 2 { break; }
    }
    if result.is_empty() && end > frame { result.push(end); }
    Ok(result)
}
