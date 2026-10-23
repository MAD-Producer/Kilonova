//! One immutable catalogue projection and one query, owned by an open search.
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct Cache<T, L> {
    catalogue: Option<Arc<Value>>,
    locale: Option<L>,
    entries: Vec<(T, String)>,
    query: String,
    query_valid: bool,
    matches: Vec<usize>,
}

impl<T, L> Default for Cache<T, L> {
    fn default() -> Self {
        Self {
            catalogue: None,
            locale: None,
            entries: Vec::new(),
            query: String::new(),
            query_valid: false,
            matches: Vec::new(),
        }
    }
}

impl<T, L: PartialEq> Cache<T, L> {
    pub(crate) fn matching(
        &mut self,
        catalogue: &Arc<Value>,
        locale: L,
        query: &str,
        project: impl FnOnce() -> Vec<(T, String)>,
    ) -> Matches<'_, T> {
        if self.locale.as_ref() != Some(&locale)
            || self
                .catalogue
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, catalogue))
        {
            self.entries = project();
            self.catalogue = Some(Arc::clone(catalogue));
            self.locale = Some(locale);
            self.query_valid = false;
        }
        if !self.query_valid || self.query != query {
            let normalized = query.trim().to_lowercase();
            self.matches.clear();
            self.matches
                .extend(
                    self.entries
                        .iter()
                        .enumerate()
                        .filter_map(|(index, (_, terms))| {
                            normalized
                                .split_whitespace()
                                .all(|term| terms.contains(term))
                                .then_some(index)
                        }),
                );
            self.query.clear();
            self.query.push_str(query);
            self.query_valid = true;
        }
        Matches {
            entries: &self.entries,
            indices: &self.matches,
        }
    }
}

pub(crate) struct Matches<'a, T> {
    entries: &'a [(T, String)],
    indices: &'a [usize],
}

impl<T> Matches<'_, T> {
    pub(crate) fn len(&self) -> usize {
        self.indices.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.indices.iter().map(|index| &self.entries[*index].0)
    }
}
