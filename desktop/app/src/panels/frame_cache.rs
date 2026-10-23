//! Bounded structural keys; cache hits keep the caller's owned-vector contract.
use motion_ui::dock::{DockId, Edge, Node};

// Counts, not UI dimensions. Current workspace trees fit well below this bound.
pub(super) const MAX_ITEMS: usize = 128;

#[derive(Clone, Copy)]
enum Part {
    Split(Edge, u32),
    Dock(DockId, usize, usize),
    Tab(DockId),
    Fixed(Edge, u32),
}

struct Entry<T> {
    size: [u32; 2],
    key: Vec<Part>,
    values: Vec<T>,
}

pub(super) struct Cache<T> {
    entry: Option<Entry<T>>,
}

impl<T> Cache<T> {
    pub(super) const fn new() -> Self {
        Self { entry: None }
    }
}

impl<T: Copy> Cache<T> {
    pub(super) fn get(&self, layout: &Node, size: [f32; 2]) -> Option<Vec<T>> {
        let entry = self.entry.as_ref()?;
        if entry.size != size.map(f32::to_bits) {
            return None;
        }
        let mut parts = entry.key.iter();
        if !matches_layout(layout, &mut parts) || parts.next().is_some() {
            return None;
        }
        Some(entry.values.clone())
    }

    pub(super) fn store(&mut self, layout: &Node, size: [f32; 2], values: &[T]) -> bool {
        if values.len() > MAX_ITEMS || size.iter().any(|value| !value.is_finite()) {
            return false;
        }
        let mut remaining = MAX_ITEMS;
        if !admissible(layout, &mut remaining) {
            return false;
        }
        // Reuse both bounded buffers when dragging/resizing changes the key.
        let entry = self.entry.get_or_insert_with(|| Entry {
            size: [0; 2],
            key: Vec::with_capacity(MAX_ITEMS),
            values: Vec::with_capacity(MAX_ITEMS),
        });
        entry.key.clear();
        write_key(layout, &mut entry.key);
        entry.values.clear();
        entry.values.extend(values.iter().copied());
        entry.size = size.map(f32::to_bits);
        true
    }
}

fn admissible(node: &Node, remaining: &mut usize) -> bool {
    if *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    match node {
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

fn write_key(node: &Node, parts: &mut Vec<Part>) {
    match node {
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

fn matches_layout(node: &Node, parts: &mut std::slice::Iter<'_, Part>) -> bool {
    match (node, parts.next()) {
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
                && matches_layout(first, parts)
                && matches_layout(second, parts)
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
