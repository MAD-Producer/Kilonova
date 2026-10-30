//! Independent preview values. Source defaults and the host project are never mutated.
use motion_effects::ParamDefinition;
use std::collections::VecDeque;

const HISTORY_LIMIT: usize = 128;
type Values = Vec<[f32; 4]>;

pub struct Session {
    pub definitions: Vec<ParamDefinition>,
    values: Values,
    before: Option<Values>,
    undo: VecDeque<Values>,
    redo: Vec<Values>,
    pub revision: u64,
}
impl Session {
    pub fn new(definitions: Vec<ParamDefinition>) -> Result<Self, String> {
        if definitions.len() > motion_effects::MAX_PARAMS
            || definitions.iter().enumerate().any(|(i, p)| {
                !p.kind.valid_value(&p.default, p.min, p.max)
                    || definitions[..i].iter().any(|q| q.id == p.id)
            })
        {
            return Err("invalid plugin preview parameter contract".into());
        }
        Ok(Self {
            values: definitions.iter().map(|p| p.default).collect(),
            definitions,
            before: None,
            undo: VecDeque::new(),
            redo: Vec::new(),
            revision: 0,
        })
    }
    pub fn values(&self) -> &[[f32; 4]] {
        &self.values
    }
    pub fn active(&self) -> bool {
        self.before.is_some()
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn begin(&mut self) {
        if self.before.is_none() {
            self.before = Some(self.values.clone());
        }
    }
    pub fn set(&mut self, index: usize, value: [f32; 4]) -> Result<(), String> {
        let p = self
            .definitions
            .get(index)
            .ok_or("preview parameter slot missing")?;
        if !p.kind.valid_value(&value, p.min, p.max) {
            return Err(format!(
                "preview parameter={} rejects value outside its contract",
                p.id
            ));
        }
        if self.values[index] != value {
            self.begin();
            self.values[index] = value;
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(())
    }
    pub fn commit(&mut self) {
        if let Some(before) = self.before.take() {
            if before != self.values {
                if self.undo.len() == HISTORY_LIMIT {
                    self.undo.pop_front();
                }
                self.undo.push_back(before);
                self.redo.clear();
            }
        }
    }
    pub fn cancel(&mut self) {
        if let Some(before) = self.before.take() {
            if before != self.values {
                self.values = before;
                self.revision = self.revision.wrapping_add(1);
            }
        }
    }
    pub fn undo(&mut self) {
        self.cancel();
        if let Some(previous) = self.undo.pop_back() {
            self.redo
                .push(std::mem::replace(&mut self.values, previous));
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub fn redo(&mut self) {
        self.cancel();
        if let Some(next) = self.redo.pop() {
            self.undo
                .push_back(std::mem::replace(&mut self.values, next));
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub fn reset(&mut self) {
        self.cancel();
        self.begin();
        let defaults: Vec<_> = self.definitions.iter().map(|p| p.default).collect();
        if self.values != defaults {
            self.values = defaults;
            self.revision = self.revision.wrapping_add(1);
        }
        self.commit();
    }
}
