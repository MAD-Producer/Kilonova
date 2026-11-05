use motion_ui::{
    dock::DockId,
    interaction::Router,
    paint::{self, Bitmap, ImageQuad},
    panel::{PanelFrame, PanelHost, PanelLayout, PanelView},
    registry::{Language, PanelDescriptor},
    style::default_stylesheet,
    ui::{self, Align},
    Color, Input, PaintList, Rect, TextAtlas,
};
use std::{cell::Cell, rc::Rc, sync::Arc};

#[path = "panel_paint_reuse/allocations.rs"]
mod allocations;

struct Rows {
    count: usize,
    bitmap: Arc<Bitmap>,
    panic_next: Rc<Cell<bool>>,
}

fn rows(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    bounds: Rect,
    count: usize,
    bitmap: &Arc<Bitmap>,
) {
    let styles = list.styles();
    for index in 0..count {
        let y = bounds.min[1] + index as f32 * 20.;
        paint::rect(
            list,
            Rect::new(bounds.min[0], y, 220., 20.),
            styles.color("panel"),
        );
        ui::text(
            list,
            atlas,
            Rect::new(bounds.min[0] + 4., y, 120., 20.),
            "Value",
            styles.color("text"),
            Align::Left,
        );
        list.push_image(ImageQuad::new(
            bitmap.clone(),
            Rect::new(bounds.min[0] + 160., y + 4., 12., 12.),
        ));
    }
}

impl PanelView for Rows {
    type Action = ();
    fn draw(&mut self, frame: &mut PanelFrame<'_>) -> Vec<()> {
        rows(
            frame.paint,
            frame.atlas,
            frame.layout.bounds,
            self.count,
            &self.bitmap,
        );
        assert!(
            !self.panic_next.replace(false),
            "controlled panel draw panic"
        );
        vec![]
    }
}

fn host(count: usize) -> (PanelHost<()>, Arc<Bitmap>, Rc<Cell<bool>>) {
    let bitmap = Bitmap::new(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ],
    )
    .unwrap();
    let panic_next = Rc::new(Cell::new(false));
    let mut host = PanelHost::default();
    for id in 1..=2 {
        host.register(
            PanelDescriptor::new(DockId(id), "参数", "Parameters"),
            Rows {
                count,
                bitmap: bitmap.clone(),
                panic_next: panic_next.clone(),
            },
        )
        .unwrap();
    }
    (host, bitmap, panic_next)
}

fn layout(id: u32, width: f32, count: usize) -> PanelLayout {
    PanelLayout {
        window: u64::from(id),
        panel: DockId(id),
        bounds: Rect::new(0., 0., width, count as f32 * 20.),
        clip: Rect::new(0., 0., width, count as f32 * 20.),
    }
}

fn draw(
    host: &mut PanelHost<()>,
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    layout: PanelLayout,
) {
    assert!(host
        .draw(
            layout,
            Language::English,
            &Input::default(),
            &Router::default(),
            list,
            atlas
        )
        .is_empty());
}

#[test]
fn warm_panel_frames_reuse_buffers_at_twenty_two_hundred_and_one_thousand_rows() {
    assert!(
        allocations::measure(|| {
            std::hint::black_box(Box::new(7u64));
        })
        .0 > 0
    );
    let mut reports = vec![];
    for count in [20, 200, 1000] {
        let (mut host, _, _) = host(count);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut list = PaintList::default();
        let frame = |host: &mut PanelHost<()>, list: &mut PaintList, atlas: &mut TextAtlas| {
            list.clear();
            for id in 1..=2 {
                draw(host, list, atlas, layout(id, 220., count));
            }
        };
        for _ in 0..3 {
            frame(&mut host, &mut list, &mut atlas);
        }
        let measured = allocations::measure(|| {
            for _ in 0..20 {
                frame(&mut host, &mut list, &mut atlas);
            }
        });
        println!(
            "rows_per_panel={count} panels=2 frames=20 allocations={} requested_bytes={}",
            measured.0, measured.1
        );
        assert_eq!(list.images.len(), 2 * count);
        assert!(!list.glyphs.is_empty());
        reports.push(measured);
    }
    assert_eq!(
        reports,
        [(0, 0); 3],
        "warm host frames must retain their command storage"
    );
}

fn same_commands(actual: &PaintList, expected: &PaintList) {
    assert_eq!(actual.order, expected.order);
    assert_eq!(actual.colors, expected.colors);
    // Includes geometry, clips, UVs, image identity, colors and atlas dimensions.
    assert_eq!(
        format!("{:?}", actual.shapes),
        format!("{:?}", expected.shapes)
    );
    assert_eq!(
        format!("{:?}", actual.glyphs),
        format!("{:?}", expected.glyphs)
    );
    assert_eq!(
        format!("{:?}", actual.images),
        format!("{:?}", expected.images)
    );
}

fn reference(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    styles: Arc<motion_ui::style::StyleSheet>,
    bitmap: &Arc<Bitmap>,
    layout: PanelLayout,
    count: usize,
) {
    // Previous host path: fresh buffer, panel clipping, consuming append.
    let mut content = PaintList::with_styles(styles);
    rows(&mut content, atlas, layout.bounds, count, bitmap);
    content.clip_to(layout.bounds.intersection(layout.clip));
    list.append(content);
}

#[test]
fn alternating_panels_keep_clipping_order_theme_and_image_ownership() {
    let (mut host, bitmap, _) = host(3);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut actual = PaintList::default();
    let mut expected = PaintList::default();
    for epoch in 0..3 {
        let styles = if epoch == 1 {
            default_stylesheet()
                .with_overrides(br##"{"tokens":{"colors":{"panel":"#345678","text":"#abcdef"}}}"##)
                .unwrap()
        } else {
            default_stylesheet().clone()
        };
        host.set_stylesheet(styles.clone());
        let styles = Arc::new(styles);
        actual.clear();
        expected.clear();
        paint::rect(&mut actual, Rect::new(0., 0., 1., 1.), Color::grey(12));
        paint::rect(&mut expected, Rect::new(0., 0., 1., 1.), Color::grey(12));
        for (id, width) in [(2, 170.), (1, 100.), (2, 220.)] {
            let mut layout = layout(id, width, 3);
            layout.clip = Rect::new(6., 4., width - 12., 43.);
            draw(&mut host, &mut actual, &mut atlas, layout);
            reference(
                &mut expected,
                &mut atlas,
                styles.clone(),
                &bitmap,
                layout,
                3,
            );
        }
        same_commands(&actual, &expected);
        assert_eq!(actual.images.len(), 6);
        actual.clear();
        expected.clear();
        assert_eq!(
            Arc::strong_count(&bitmap),
            3,
            "scratch must not retain drawn image payloads"
        );
    }
    drop(host);
    assert_eq!(Arc::strong_count(&bitmap), 1);
}

#[test]
fn panicking_panel_still_propagates_and_next_draw_cannot_publish_stale_commands() {
    let (mut host, bitmap, panic_next) = host(3);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut actual = PaintList::default();
    paint::rect(&mut actual, Rect::new(0., 0., 1., 1.), Color::grey(12));
    let before = format!("{:?}", actual.shapes);
    panic_next.set(true);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        draw(&mut host, &mut actual, &mut atlas, layout(1, 220., 3));
    }));
    assert!(panic.is_err(), "the host must not swallow a view panic");
    assert_eq!(format!("{:?}", actual.shapes), before);
    assert!(actual.glyphs.is_empty() && actual.images.is_empty());
    actual.clear();
    draw(&mut host, &mut actual, &mut atlas, layout(2, 220., 3));
    let mut expected = PaintList::default();
    reference(
        &mut expected,
        &mut atlas,
        Arc::new(default_stylesheet().clone()),
        &bitmap,
        layout(2, 220., 3),
        3,
    );
    same_commands(&actual, &expected);
}

#[test]
fn open_dropdown_reuses_popup_storage_with_small_and_scrolled_large_menus() {
    use motion_ui::widgets::{Dropdown, DropdownItem};
    for count in [3, 20, 200] {
        let mut dropdown = Dropdown::default();
        dropdown.open = true;
        let items: Vec<_> = (0..count)
            .map(|_| DropdownItem {
                label: "Value",
                enabled: true,
            })
            .collect();
        let mut list = PaintList::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let input = Input {
            mouse: [-1., -1.],
            ..Default::default()
        };
        let frame = |dropdown: &mut Dropdown, list: &mut PaintList, atlas: &mut TextAtlas| {
            list.clear();
            assert_eq!(
                dropdown.draw(
                    list,
                    atlas,
                    Rect::new(8., 8., 160., 24.),
                    Rect::new(0., 0., 220., 300.),
                    &items,
                    0,
                    &input,
                    true,
                    false
                ),
                None
            );
        };
        for _ in 0..3 {
            frame(&mut dropdown, &mut list, &mut atlas);
        }
        let result = allocations::measure(|| {
            for _ in 0..20 {
                frame(&mut dropdown, &mut list, &mut atlas);
            }
        });
        println!(
            "open_dropdown items={count} frames=20 allocations={} requested_bytes={}",
            result.0, result.1
        );
        assert_eq!(
            result,
            (0, 0),
            "open dropdown must retain its popup command storage"
        );
        assert!(dropdown.open && !list.glyphs.is_empty());
    }
}
