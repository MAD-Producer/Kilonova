//! One bounded, structural entry per thread, shared by paint and hit testing.
use super::{Edge, Node, Placement, Rect};
use std::cell::RefCell;

// Counts, not UI dimensions. Rejected inputs keep the original calculation.
const MAX_ITEMS: usize = 128;

#[derive(Clone, Copy)]
enum Part {
    Split(Edge, u32),
    Dock(super::DockId, usize, usize),
    Tab(super::DockId),
    Fixed(Edge, u32),
}

struct Entry {
    area: [u32; 4],
    key: Vec<Part>,
    placements: Vec<Placement>,
}

thread_local! {
    static CACHE: RefCell<Option<Entry>> = const { RefCell::new(None) };
}

fn area_key(area: Rect) -> [u32; 4] {
    [
        area.min[0].to_bits(),
        area.min[1].to_bits(),
        area.max[0].to_bits(),
        area.max[1].to_bits(),
    ]
}

pub(super) fn get(root: &Node, area: Rect) -> Option<Vec<Placement>> {
    CACHE
        .try_with(|slot| {
            let cache = slot.try_borrow().ok()?;
            let entry = cache.as_ref()?;
            if entry.area != area_key(area) {
                return None;
            }
            let mut parts = entry.key.iter();
            if !matches(root, &mut parts) || parts.next().is_some() {
                return None;
            }
            Some(entry.placements.clone())
        })
        .ok()
        .flatten()
}

pub(super) fn store(root: &Node, area: Rect, placements: &[Placement]) {
    let mut remaining = MAX_ITEMS;
    if placements.len() > MAX_ITEMS
        || area
            .min
            .into_iter()
            .chain(area.max)
            .any(|value| !value.is_finite())
        || !admissible(root, &mut remaining)
    {
        return;
    }
    // A busy or destroyed optional slot is a normal cache miss. The caller
    // already owns the complete original result; there is no retry or failure.
    let _ = CACHE.try_with(|slot| {
        if let Ok(mut cache) = slot.try_borrow_mut() {
            let entry = cache.get_or_insert_with(|| Entry {
                area: [0; 4],
                key: Vec::with_capacity(MAX_ITEMS),
                placements: Vec::with_capacity(MAX_ITEMS),
            });
            entry.key.clear();
            write_key(root, &mut entry.key);
            entry.placements.clear();
            entry.placements.extend_from_slice(placements);
            entry.area = area_key(area);
        }
    });
}

fn admissible(root: &Node, remaining: &mut usize) -> bool {
    if *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    match root {
        Node::Split {
            split,
            first,
            second,
            ..
        } => split.is_finite() && admissible(first, remaining) && admissible(second, remaining),
        Node::Dock { tabs, .. } => {
            if tabs.len() > *remaining {
                return false;
            }
            *remaining -= tabs.len();
            true
        }
        Node::Fixed { size, .. } => size.is_finite(),
    }
}

fn write_key(root: &Node, parts: &mut Vec<Part>) {
    match root {
        Node::Split {
            edge,
            split,
            first,
            second,
        } => {
            parts.push(Part::Split(*edge, split.to_bits()));
            write_key(first, parts);
            write_key(second, parts);
        }
        Node::Dock { id, tabs, active } => {
            parts.push(Part::Dock(*id, *active, tabs.len()));
            parts.extend(tabs.iter().copied().map(Part::Tab));
        }
        Node::Fixed { edge, size } => parts.push(Part::Fixed(*edge, size.to_bits())),
    }
}

fn matches(root: &Node, parts: &mut std::slice::Iter<'_, Part>) -> bool {
    match (root, parts.next()) {
        (
            Node::Split {
                edge,
                split,
                first,
                second,
            },
            Some(Part::Split(saved_edge, bits)),
        ) => {
            edge == saved_edge
                && split.to_bits() == *bits
                && matches(first, parts)
                && matches(second, parts)
        }
        (Node::Dock { id, tabs, active }, Some(Part::Dock(saved_id, saved_active, count))) => {
            id == saved_id
                && active == saved_active
                && tabs.len() == *count
                && tabs
                    .iter()
                    .all(|tab| matches!(parts.next(), Some(Part::Tab(saved)) if saved == tab))
        }
        (Node::Fixed { edge, size }, Some(Part::Fixed(saved_edge, bits))) => {
            edge == saved_edge && size.to_bits() == *bits
        }
        _ => false,
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::*;
    use crate::dock::solve_cache_tests::{measure, original, stamp};

    #[test]
    fn busy_slot_returns_original_geometry_and_recovers_without_panicking() {
        let root = Node::dock(super::super::DockId(10001));
        let area = Rect::new(13., 19., 850., 650.);
        let expected = stamp(&original(&root, area));
        CACHE.with(|slot| {
            let _busy = slot.borrow_mut();
            let (rows, count) = measure(|| super::super::solve(&root, area));
            assert_eq!(count, 1);
            assert_eq!(stamp(&rows), expected);
        });
        assert_eq!(stamp(&super::super::solve(&root, area)), expected);
        let (_, count) = measure(|| assert_eq!(stamp(&super::super::solve(&root, area)), expected));
        assert_eq!(count, 0);
    }

    #[test]
    fn oversized_and_nonfinite_keys_stay_uncached_and_retention_stays_bounded() {
        let area = Rect::new(7., 11., 700., 500.);
        let oversized = Node::Dock {
            id: super::super::DockId(1),
            tabs: (0..MAX_ITEMS as u32).map(super::super::DockId).collect(),
            active: 0,
        };
        for (root, area) in [
            (oversized, area),
            (
                Node::Fixed {
                    edge: Edge::Top,
                    size: f32::INFINITY,
                },
                area,
            ),
            (
                Node::dock(super::super::DockId(12)),
                Rect::new(f32::NAN, 0., 700., 500.),
            ),
        ] {
            let expected = stamp(&original(&root, area));
            let (_, count) = measure(|| {
                for _ in 0..3 {
                    assert_eq!(stamp(&super::super::solve(&root, area)), expected);
                }
            });
            assert_eq!(count, 3);
            CACHE.with(|slot| {
                if let Some(entry) = slot.borrow().as_ref() {
                    assert!(entry.key.len() <= MAX_ITEMS && entry.placements.len() <= MAX_ITEMS);
                    assert!(
                        entry.key.capacity() <= MAX_ITEMS
                            && entry.placements.capacity() <= MAX_ITEMS
                    );
                }
            });
        }
    }

    #[test]
    fn destroyed_thread_slot_still_returns_the_original_geometry() {
        type Stamp = Vec<(u32, [u32; 4])>;
        struct Late(std::sync::mpsc::Sender<(bool, Stamp)>);
        impl Drop for Late {
            fn drop(&mut self) {
                let unavailable = CACHE.try_with(|_| ()).is_err();
                let rows = super::super::solve(
                    &Node::dock(super::super::DockId(20001)),
                    Rect::new(3., 5., 800., 600.),
                );
                let _ = self.0.send((unavailable, stamp(&rows)));
            }
        }
        thread_local! {static LATE:RefCell<Option<Late>>=const {RefCell::new(None)};}
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            LATE.with(|slot| *slot.borrow_mut() = Some(Late(send)));
            super::super::solve(
                &Node::dock(super::super::DockId(20001)),
                Rect::new(3., 5., 800., 600.),
            );
        })
        .join()
        .unwrap();
        let (unavailable, rows) = receive.recv().unwrap();
        assert!(unavailable, "thread cache destruction must be exercised");
        assert_eq!(
            rows,
            stamp(&original(
                &Node::dock(super::super::DockId(20001)),
                Rect::new(3., 5., 800., 600.)
            ))
        );
    }
}
