use super::*;

pub(super) fn layout() -> dock::Node {
    dock::Node::Split {
        edge: dock::Edge::Top,
        split: 0.65,
        first: Box::new(dock::Node::Split {
            edge: dock::Edge::Left,
            split: 0.32,
            first: Box::new(dock::Node::dock(dock::Panel::Properties.dock())),
            second: Box::new(dock::Node::dock(dock::Panel::Composition.dock())),
        }),
        second: Box::new(dock::Node::dock(dock::Panel::Timeline.dock())),
    }
}
pub(super) fn cursor(point: [f32; 2]) -> WindowEvent {
    WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(f64::from(point[0]), f64::from(point[1])),
    }
}
pub(super) fn button(state: ElementState) -> WindowEvent {
    WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state,
        button: MouseButton::Left,
    }
}
struct InputBatch<'a> {
    shell: &'a mut Shell,
    events: Option<Vec<WindowEvent>>,
}
impl InputBatch<'_> {
    fn send(&mut self, active: &ActiveEventLoop) {
        if let Some(events) = self.events.take() {
            let id = self.shell.window.as_ref().unwrap().id();
            for event in events {
                self.shell.window_event(active, id, event);
            }
        }
    }
}
impl ApplicationHandler for InputBatch<'_> {
    fn resumed(&mut self, active: &ActiveEventLoop) {
        self.send(active);
    }
    fn about_to_wait(&mut self, active: &ActiveEventLoop) {
        self.send(active);
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
pub(super) fn feed(events: &mut EventLoop<()>, shell: &mut Shell, batch: Vec<WindowEvent>) {
    let mut driver = InputBatch {
        shell,
        events: Some(batch),
    };
    let began = Instant::now();
    while driver.events.is_some() {
        events.pump_app_events(Some(Duration::ZERO), &mut driver);
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "native input dispatch deadline"
        );
    }
}
pub(super) fn draw_dispatch(events: &mut EventLoop<()>, shell: &mut Shell) -> usize {
    let (_, actions) = shell.draw_commands(SIZE);
    let count = actions.len();
    shell.actions.extend(actions);
    while let Some(action) = shell::actions::pop_action(&mut shell.actions) {
        shell.action(action, None).unwrap();
        if let Some((_, reply)) = shell.editing.take() {
            let state = receive(events, &reply);
            shell.accept_edit_state(state);
        }
    }
    count
}
pub(super) fn show(events: &mut EventLoop<()>, shell: &mut Shell) -> u32 {
    let began = Instant::now();
    let mut polls = 0;
    loop {
        let (paint, actions) = shell.draw_commands(SIZE);
        assert!(
            actions.is_empty(),
            "presentation must not discard pending actions"
        );
        let ui = shell.ui_frame(paint, SIZE, false);
        assert!(ui.composition.is_some());
        polls += 1;
        if receive(events, &shell.present_ui_async(ui).unwrap()) {
            return polls;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "successful native Present deadline"
        );
    }
}
pub(super) fn control_point(shell: &mut Shell, scenario: &str, number: usize) -> [f32; 2] {
    let (paint, actions) = shell.draw_commands(SIZE);
    assert!(actions.is_empty());
    let target = if scenario == "property_drag" {
        dock::Panel::Properties
    } else {
        dock::Panel::Timeline
    };
    let area = panels::frames(&shell.workspace.root, SIZE)
        .into_iter()
        .find(|frame| frame.panel == target)
        .unwrap()
        .content;
    if scenario == "property_drag" {
        let styles = paint.styles();
        [
            area.min[0]
                + styles.metric("padding.panel", 8.) * 2.
                + styles.metric("tree.indent", 14.)
                + styles.metric("icon.slot", 20.) * 4.
                + styles.metric("label.column", 72.)
                + 10.,
            area.min[1] + 8. + 4. * styles.metric("row.height", 22.) + 10.,
        ]
    } else {
        let track = shell
            .panels
            .timeline
            .zoom_track
            .expect("rendered timeline track");
        [
            track.min[0] + (number as f32 + 10.) * track.width() / 300.,
            area.min[1] + 50.,
        ]
    }
}
pub(super) fn warm(events: &mut EventLoop<()>, shell: &Shell, enabled: bool) -> Value {
    let began = Instant::now();
    loop {
        let state = shell.engine.state().unwrap();
        let status = &state["desktop"]["prerender"];
        assert!(status["fault"].is_null(), "{status}");
        if !enabled || status["cache"]["resident_frames"].as_u64().unwrap_or(0) > 0 {
            return status.clone();
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "optional work must actually warm: {status}"
        );
        // Keep the HWND serviced while waiting for the genuine native owner.
        struct Idle;
        impl ApplicationHandler for Idle {
            fn resumed(&mut self, _: &ActiveEventLoop) {}
            fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
        }
        events.pump_app_events(Some(Duration::from_millis(10)), &mut Idle);
    }
}
pub(super) fn observation(shell: &Shell) -> Value {
    match shell.engine.prerender_observation().unwrap() {
        None => Value::Null,
        Some(s) => serde_json::json!({"completionWaiting":s.completion_waiting,"stopped":s.stopped,
            "retainedCpuBytes":s.retained_cpu_bytes,"cpuLimitBytes":s.cpu_limit_bytes,
            "lastFailure":s.last_failure,"flight":s.in_flight.map(|f|serde_json::json!({
                "id":f.id,"operation":f.operation,"phase":f.phase,"frame":f.frame,"elapsedUs":f.elapsed.as_micros()}))}),
    }
}
