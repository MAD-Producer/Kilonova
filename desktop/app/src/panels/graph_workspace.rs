//! Graph mode shares the timeline chrome, ruler and editable layer properties.
use super::*;
pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    panels.timeline.set_graph_mode(true);
    super::timeline::render(list, atlas, area, panels, state, input, out);
}
