//! Temporary remaps share the unused tail of the owner's command buffer.
use super::PaintOp;

#[derive(Clone, Copy)]
pub(super) struct IndexMap {
    start: usize,
    len: usize,
}

pub(super) struct Scratch<'a> {
    commands: &'a mut Vec<PaintOp>,
    original_len: usize,
}
impl<'a> Scratch<'a> {
    pub fn new(commands: &'a mut Vec<PaintOp>) -> Self {
        let original_len = commands.len();
        Self {
            commands,
            original_len,
        }
    }
    pub fn map(&mut self, len: usize, prefix: usize) -> IndexMap {
        let start = self.commands.len();
        // Shape encodes Some(index); Image is the private None tag. These
        // entries never reach the renderer, including on callback unwind.
        self.commands.extend((0..len).map(|i| {
            if i < prefix {
                PaintOp::Shape(i)
            } else {
                PaintOp::Image(0)
            }
        }));
        IndexMap { start, len }
    }
    pub fn set(&mut self, map: IndexMap, old: usize, new: usize) {
        self.commands[map.start + old] = PaintOp::Shape(new);
    }
    fn decode(entry: &PaintOp) -> Option<usize> {
        if let PaintOp::Shape(index) = *entry {
            Some(index)
        } else {
            None
        }
    }
    pub fn at(&self, map: IndexMap, index: usize) -> Option<usize> {
        // retain_glyphs historically panics on an invalid public glyph index.
        Self::decode(&self.commands[map.start..map.start + map.len][index])
    }
    fn get(&self, map: IndexMap, index: usize) -> Option<usize> {
        self.commands[map.start..map.start + map.len]
            .get(index)
            .and_then(Self::decode)
    }
    pub fn original(&self) -> &[PaintOp] {
        &self.commands[..self.original_len]
    }
    pub fn compact_clipped(&mut self, maps: [Option<IndexMap>; 3], lengths: [usize; 3]) {
        let mut next = 0;
        for old in 0..self.original_len {
            let (kind, index) = match self.commands[old] {
                PaintOp::Shape(i) => (0, i),
                PaintOp::Glyph(i) => (1, i),
                PaintOp::Image(i) => (2, i),
            };
            let mapped = match maps[kind] {
                Some(map) => self.get(map, index),
                None => (index < lengths[kind]).then_some(index),
            };
            if let Some(index) = mapped {
                self.commands[next] = match kind {
                    0 => PaintOp::Shape(index),
                    1 => PaintOp::Glyph(index),
                    _ => PaintOp::Image(index),
                };
                next += 1;
            }
        }
        self.commands.truncate(next);
    }
    pub fn compact_glyphs(&mut self, map: IndexMap) {
        let mut next = 0;
        for old in 0..self.original_len {
            let op = match self.commands[old] {
                PaintOp::Glyph(index) => self.at(map, index).map(PaintOp::Glyph),
                other => Some(other),
            };
            if let Some(op) = op {
                self.commands[next] = op;
                next += 1;
            }
        }
        self.commands.truncate(next);
    }
}
impl Drop for Scratch<'_> {
    fn drop(&mut self) {
        // Successful compaction already shortened the buffer. If keep() or
        // another operation panics, discard all temporary map entries.
        self.commands.truncate(self.original_len);
    }
}
