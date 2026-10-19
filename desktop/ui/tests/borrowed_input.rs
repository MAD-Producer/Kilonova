use motion_ui::{
    dock::DockId,
    input::Modifiers,
    interaction::{HitRegion, Router, Target},
    widgets::clipped_input,
    Input, Key, MouseButton, Rect,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
// Count only allocations on the measuring test thread. All memory still goes
// through System, including allocations by the GPU and other test threads.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct AllocationScope;
impl Drop for AllocationScope {
    fn drop(&mut self) {
        ALLOCATIONS.with(|c| c.set(None));
    }
}
fn allocations(run: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|c| {
        assert!(c.get().is_none());
        c.set(Some(0));
    });
    let scope = AllocationScope;
    run();
    let count = ALLOCATIONS.with(|c| c.get().unwrap());
    drop(scope);
    count
}

fn fixture() -> (Input, HitRegion) {
    let rect = Rect::new(0., 0., 100., 30.);
    (
        Input {
            mouse: [10., 10.],
            pressed: Some((MouseButton::Left, [10., 10.])),
            held: Some(MouseButton::Left),
            wheel: [2., 3.],
            long_press: Some([10., 10.]),
            pointer_origin: Some([10., 10.]),
            pointer_travel: 20.,
            pending: true,
            keys: vec![
                (Key::A, Modifiers::default()),
                (Key::Enter, Modifiers::default()),
            ],
            key_repeats: vec![false, true],
            held_keys: vec![Key::A],
            released_keys: vec![Key::Space],
            text: "中文输入é👩‍🚀".repeat(100),
            ..Default::default()
        },
        HitRegion {
            target: Target {
                window: 1,
                panel: DockId(1),
                widget: 1,
            },
            rect,
            clip: rect,
            layer: 0,
            keyboard: true,
        },
    )
}
#[test]
fn routing_many_controls_does_not_clone_owned_payloads() {
    let (input, region) = fixture();
    let mut router = Router::default();
    router.begin_frame(1, &input, &[region]);
    for focused in [true, false] {
        let target = if focused {
            region.target
        } else {
            Target {
                widget: 99,
                ..region.target
            }
        };
        let measured = allocations(|| {
            for _ in 0..200 {
                std::hint::black_box(router.input_for(target, &input));
            }
        });
        println!("routed focused={focused}: allocations(200)={measured}");
        assert_eq!(
            measured, 0,
            "routing must borrow payloads even for unfocused controls"
        );
    }
}
#[test]
fn nested_clips_borrow_payloads_and_preserve_release_for_capture() {
    let (mut input, region) = fixture();
    input.released = Some((MouseButton::Left, [150., 10.]));
    input.mouse = [150., 10.];
    let measured = allocations(|| {
        for _ in 0..200 {
            std::hint::black_box(clipped_input(&input, region.rect));
        }
    });
    println!("clipped allocations(200)={measured}");
    assert_eq!(measured, 0);
    let first = clipped_input(&input, region.rect);
    let second = clipped_input(&first, Rect::new(20., 0., 40., 30.));
    assert!(second.pressed.is_none());
    assert_eq!(second.wheel, [0., 0.]);
    assert_eq!(second.released, input.released);
    assert_eq!(second.held, input.held);
    assert_eq!(second.text.as_ptr(), input.text.as_ptr());
    assert_eq!(second.keys.as_ptr(), input.keys.as_ptr());
    assert_eq!(second.key_repeats.as_ptr(), input.key_repeats.as_ptr());
    assert_eq!(second.held_keys.as_ptr(), input.held_keys.as_ptr());
    assert_eq!(second.released_keys.as_ptr(), input.released_keys.as_ptr());
}
#[test]
fn routed_owner_borrows_text_and_keys_without_mutating_source() {
    let (input, region) = fixture();
    let mut router = Router::default();
    router.begin_frame(1, &input, &[region]);
    let routed = router.input_for(region.target, &input);
    assert_eq!(routed.text.as_ptr(), input.text.as_ptr());
    assert_eq!(routed.keys.as_ptr(), input.keys.as_ptr());
    assert_eq!(routed.key_repeats.as_ptr(), input.key_repeats.as_ptr());
    assert_eq!(routed.held_keys.as_ptr(), input.held_keys.as_ptr());
    assert_eq!(routed.released_keys.as_ptr(), input.released_keys.as_ptr());
    let other = router.input_for(
        Target {
            window: 2,
            ..region.target
        },
        &input,
    );
    assert!(other.text.is_empty() && other.keys.is_empty() && other.pressed.is_none());
    assert!(other.released.is_none() && other.held.is_none() && other.long_press.is_none());
    assert_eq!(other.wheel, [0., 0.]);
    assert_eq!(other.pointer_origin, input.pointer_origin);
    assert_eq!(other.pointer_travel, input.pointer_travel);
    assert_eq!(other.key_repeats.as_ptr(), input.key_repeats.as_ptr());
    assert_eq!(input.keys.len(), 2);
    assert!(!input.text.is_empty());
}

fn assert_same(actual: &motion_ui::InputView<'_>, expected: &Input) {
    macro_rules! field {($($name:ident),*) => { $(assert_eq!(actual.$name,expected.$name,stringify!($name));)* };}
    field!(
        pending,
        focus_lost,
        held,
        mouse,
        pointer_origin,
        pointer_travel,
        pressed,
        released,
        wheel,
        modifiers,
        long_press,
        keys,
        key_repeats,
        held_keys,
        released_keys,
        composing,
        ime_consumed,
        text
    );
    assert_eq!(
        actual.key_presses().collect::<Vec<_>>(),
        expected.key_presses().collect::<Vec<_>>()
    );
    assert_eq!(actual.dragging(), expected.dragging());
}

#[test]
fn routed_masks_preserve_all_legacy_fields_across_windows_modal_capture_and_ime() {
    let (mut input, a) = fixture();
    input.modifiers = Modifiers {
        control: true,
        shift: true,
        alt: false,
    };
    let b = HitRegion {
        target: Target {
            panel: DockId(2),
            ..a.target
        },
        rect: Rect::new(110., 0., 100., 30.),
        clip: Rect::new(110., 0., 100., 30.),
        ..a
    };
    let foreign = Target {
        window: 2,
        ..a.target
    };
    for window in [1, 2] {
        for modal in [false, true] {
            for lost in [false, true] {
                for composing in [false, true] {
                    for consumed in [false, true] {
                        let mut router = Router::default();
                        let mut source = input.clone();
                        source.focus_lost = false;
                        router.begin_frame(1, &source, &[a, b]);
                        assert_eq!(router.captured(), Some(a.target));
                        if modal {
                            router.set_modal(Some((1, DockId(2))));
                        }
                        let previous = router.focus();
                        source.pressed = None;
                        source.released = Some((MouseButton::Right, [150., 10.]));
                        source.mouse = [150., 10.];
                        source.focus_lost = lost;
                        source.composing = composing;
                        source.ime_consumed = consumed;
                        router.begin_frame(window, &source, &[a, b]);
                        for target in [a.target, b.target, foreign] {
                            let mut expected = source.clone();
                            let permitted =
                                window == target.window && (!modal || target.panel == DockId(2));
                            let pointer = permitted
                                && (router.captured() == Some(target)
                                    || router.captured().is_none() && target == b.target);
                            if !pointer {
                                expected.pressed = None;
                                expected.released = None;
                                expected.held = None;
                                expected.wheel = [0.; 2];
                                expected.long_press = None;
                            }
                            if !permitted || router.focus() != Some(target) {
                                expected.keys.clear();
                                expected.text.clear();
                            }
                            expected.focus_lost = (lost && window == target.window)
                                || (previous == Some(target) && router.focus() != Some(target))
                                || (modal && target == a.target);
                            assert_same(&router.input_for(target, &source), &expected);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn panel_frames_route_repeated_controls_without_allocating_payloads() {
    use motion_ui::{
        panel::{PanelFrame, PanelHost, PanelLayout, PanelView},
        registry::{Language, PanelDescriptor},
        PaintList, TextAtlas,
    };
    struct Probe;
    impl PanelView for Probe {
        type Action = ();
        fn draw(&mut self, frame: &mut PanelFrame<'_>) -> Vec<()> {
            let first = frame.input_for(1);
            let address = first.text.as_ptr();
            let count = allocations(|| {
                for _ in 0..200 {
                    let view = frame.input_for(1);
                    assert_eq!(view.text.as_ptr(), address);
                    assert!(!view.keys.is_empty());
                    std::hint::black_box(view);
                }
            });
            assert_eq!(count, 0);
            vec![]
        }
    }
    let (input, region) = fixture();
    let mut host = PanelHost::default();
    // Descriptor metadata and setup allocations are outside the routing probe.
    host.register(PanelDescriptor::new(DockId(1), "探针", "Probe"), Probe)
        .unwrap();
    let mut router = Router::default();
    router.begin_frame(1, &input, &[region]);
    let mut list = PaintList::default();
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    host.draw(
        PanelLayout {
            window: 1,
            panel: DockId(1),
            bounds: region.rect,
            clip: region.rect,
        },
        Language::English,
        &input,
        &router,
        &mut list,
        &mut atlas,
    );
}

#[test]
fn scroll_views_and_owned_compatibility_frames_apply_identical_masks() {
    use motion_ui::{widgets::ScrollArea, InputSource, PaintList};
    let (mut input, region) = fixture();
    for mouse in [[10., 10.], [150., 10.]] {
        input.mouse = mouse;
        input.released = Some((MouseButton::Left, mouse));
        let mut borrowed_scroll = ScrollArea::default();
        let mut owned_scroll = ScrollArea::default();
        let mut borrowed_paint = PaintList::default();
        let mut owned_paint = PaintList::default();
        let borrowed =
            borrowed_scroll.begin_view(&mut borrowed_paint, region.rect, 200., &input, true);
        let owned = owned_scroll.begin(&mut owned_paint, region.rect, 200., &input, true);
        let mut expected = input.clone();
        expected.pressed = expected
            .pressed
            .filter(|(_, point)| borrowed.viewport.contains(*point));
        // The scroll container consumes wheel input before exposing its child.
        // This is unconditional in the original main implementation too.
        expected.wheel = [0.; 2];
        assert_same(&borrowed.input, &expected);
        assert_eq!(borrowed.viewport, Rect::new(0., 0., 92., 30.));
        let expected_offset = if region.rect.contains(mouse) { 3. } else { 0. };
        assert_eq!(borrowed_scroll.offset, expected_offset);
        assert_eq!(borrowed.content, Rect::new(0., -expected_offset, 92., 200.));
        assert_same(&borrowed.input, &owned.input);
        assert_eq!(borrowed.viewport, owned.viewport);
        assert_eq!(borrowed.content, owned.content);
        assert_eq!(borrowed_scroll.offset, owned_scroll.offset);
        assert_eq!(borrowed.input.text.as_ptr(), input.text.as_ptr());
        assert_ne!(owned.input.text.as_ptr(), input.text.as_ptr());
        let independent = borrowed.input.to_owned();
        assert_same(&borrowed.input, &independent);
        assert_eq!(independent.input_view().text, borrowed.input.text);
    }
}
