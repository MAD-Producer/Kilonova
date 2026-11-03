//! One immutable catalogue projection and one query, owned by an open search.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct Cache<T, L> {
    catalogue: Option<Arc<Value>>,
    locale: Option<L>,
    entries: Vec<(T, String)>,
    query: String,
    query_valid: bool,
    matches: Vec<usize>,
    #[cfg(test)]
    projections: u64,
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
            #[cfg(test)]
            projections: 0,
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
            #[cfg(test)]
            {
                self.projections = self.projections.saturating_add(1);
            }
            self.entries = project();
            self.catalogue = Some(Arc::clone(catalogue));
            self.locale = Some(locale);
            self.query_valid = false;
        }
        let changed = !self.query_valid || self.query != query;
        if changed {
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
            changed,
        }
    }
}

pub(crate) struct Matches<'a, T> {
    entries: &'a [(T, String)],
    indices: &'a [usize],
    changed: bool,
}

/// Grouping metadata is fixed for the lifetime of one cache instance.
pub(crate) struct Fields<T> {
    pub key: fn(&T) -> &str,
    pub name: fn(&T) -> &str,
    pub category: fn(&T) -> &str,
}

pub(crate) struct Selection<'a> {
    pub tab: usize,
    pub favorites: &'a BTreeSet<String>,
    pub recent: &'a [String],
    pub sort: Sort,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Sort {
    #[default]
    Name,
    Recent,
    FavoritesFirst,
}

struct Group {
    name: String,
    indices: Vec<usize>,
    count: String,
}

/// Reuse the modal search's immutable projection and query cache. The effect
/// library adds one current grouping; old queries and selections are replaced.
pub(crate) struct GroupedCache<T, L> {
    search: Cache<T, L>,
    valid: bool,
    tab: usize,
    sort: Sort,
    favorites: BTreeSet<String>,
    recent: Vec<String>,
    groups: Vec<Group>,
    shown: usize,
    #[cfg(test)]
    groupings: u64,
}

impl<T, L> Default for GroupedCache<T, L> {
    fn default() -> Self {
        Self {
            search: Cache::default(),
            valid: false,
            tab: 0,
            sort: Sort::Name,
            favorites: BTreeSet::new(),
            recent: Vec::new(),
            groups: Vec::new(),
            shown: 0,
            #[cfg(test)]
            groupings: 0,
        }
    }
}

impl<T, L: PartialEq> GroupedCache<T, L> {
    pub(crate) fn grouped(
        &mut self,
        catalogue: &Arc<Value>,
        locale: L,
        query: &str,
        selection: Selection<'_>,
        project: impl FnOnce() -> Vec<(T, String)>,
        fields: Fields<T>,
    ) -> GroupedMatches<'_, T> {
        let matches = self.search.matching(catalogue, locale, query, project);
        if matches.changed
            || !self.valid
            || self.tab != selection.tab
            || self.sort != selection.sort
            || self.favorites != *selection.favorites
            || self.recent != selection.recent
        {
            #[cfg(test)]
            {
                self.groupings = self.groupings.saturating_add(1);
            }
            let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
            for &index in matches.indices {
                let entry = &matches.entries[index].0;
                let key = (fields.key)(entry);
                if match selection.tab {
                    1 => !selection.favorites.contains(key),
                    2 => !selection.recent.iter().any(|value| value == key),
                    _ => false,
                } {
                    continue;
                }
                groups
                    .entry((fields.category)(entry).to_owned())
                    .or_default()
                    .push(index);
            }
            self.shown = 0;
            self.groups = groups
                .into_iter()
                .map(|(name, mut indices)| {
                    indices.sort_by(|a, b| {
                        let a = &matches.entries[*a].0;
                        let b = &matches.entries[*b].0;
                        match selection.sort {
                            Sort::Recent => selection
                                .recent
                                .iter()
                                .position(|key| key == (fields.key)(a))
                                .cmp(
                                    &selection
                                        .recent
                                        .iter()
                                        .position(|key| key == (fields.key)(b)),
                                ),
                            Sort::Name => (fields.name)(a)
                                .cmp((fields.name)(b))
                                .then_with(|| (fields.key)(a).cmp((fields.key)(b))),
                            Sort::FavoritesFirst => (
                                !selection.favorites.contains((fields.key)(a)),
                                (fields.name)(a),
                            )
                                .cmp(&(
                                    !selection.favorites.contains((fields.key)(b)),
                                    (fields.name)(b),
                                )),
                        }
                    });
                    self.shown += indices.len();
                    Group {
                        name,
                        count: indices.len().to_string(),
                        indices,
                    }
                })
                .collect();
            self.tab = selection.tab;
            self.sort = selection.sort;
            self.favorites.clone_from(selection.favorites);
            selection.recent.clone_into(&mut self.recent);
            self.valid = true;
        }
        GroupedMatches {
            entries: &self.search.entries,
            groups: &self.groups,
            shown: self.shown,
            searching: !query.trim().is_empty(),
        }
    }

    #[cfg(test)]
    pub(crate) fn builds(&self) -> (u64, u64) {
        (self.search.projections, self.groupings)
    }
}

pub(crate) struct GroupedMatches<'a, T> {
    entries: &'a [(T, String)],
    groups: &'a [Group],
    pub shown: usize,
    pub searching: bool,
}

impl<T> GroupedMatches<'_, T> {
    pub(crate) fn all(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().map(|(entry, _)| entry)
    }
    pub(crate) fn total(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn groups(&self) -> impl Iterator<Item = GroupView<'_, T>> {
        self.groups.iter().map(|group| GroupView {
            group,
            entries: self.entries,
        })
    }
}

pub(crate) struct GroupView<'a, T> {
    group: &'a Group,
    entries: &'a [(T, String)],
}

impl<T> GroupView<'_, T> {
    pub(crate) fn name(&self) -> &str {
        &self.group.name
    }
    pub(crate) fn len(&self) -> usize {
        self.group.indices.len()
    }
    pub(crate) fn count_text(&self) -> &str {
        &self.group.count
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.group
            .indices
            .iter()
            .map(|index| &self.entries[*index].0)
    }
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
