use super::MAX_PREFETCH_IMAGES;
use std::{borrow::Cow, collections::BTreeSet};

/// Borrow demand unless speculation actually adds a permitted source. This is
/// one preparation's membership set, never a retained resource or decode cache.
pub(crate) fn allowed_assets<'a>(
    wanted: &'a BTreeSet<u64>,
    prefetch: &[u64],
    enabled: bool,
) -> Cow<'a, BTreeSet<u64>> {
    if !enabled {
        return Cow::Borrowed(wanted);
    }
    // Preserve the original cap before deduplication: repeated future IDs
    // consume lookahead slots just as they did before collecting candidates.
    let mut candidates = prefetch
        .iter()
        .copied()
        .filter(|id| !wanted.contains(id))
        .take(MAX_PREFETCH_IMAGES);
    let Some(first) = candidates.next() else {
        return Cow::Borrowed(wanted);
    };
    let mut allowed = wanted.clone();
    allowed.insert(first);
    allowed.extend(candidates);
    Cow::Owned(allowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_permission_borrows_the_original_demand() {
        for wanted in [BTreeSet::new(), BTreeSet::from([0, 7, u64::MAX])] {
            let present: Vec<_> = wanted.iter().copied().collect();
            for (enabled, future) in [(false, vec![9, 10]), (true, vec![]), (true, present)] {
                let allowed = allowed_assets(&wanted, &future, enabled);
                assert!(matches!(allowed, Cow::Borrowed(_)));
                assert!(std::ptr::eq(allowed.as_ref(), &wanted));
            }
        }
    }

    #[test]
    fn prefetch_cap_precedes_deduplication_and_demand_is_preserved() {
        let wanted = BTreeSet::from([2, 7]);
        let mut future = vec![2];
        future.extend(vec![9; MAX_PREFETCH_IMAGES]);
        future.push(8);
        let allowed = allowed_assets(&wanted, &future, true);
        assert!(matches!(allowed, Cow::Owned(_)));
        assert_eq!(allowed.as_ref(), &BTreeSet::from([2, 7, 9]));
        assert_eq!(wanted, BTreeSet::from([2, 7]));
    }

    #[test]
    fn bounded_membership_matches_the_original_union_for_random_inputs() {
        let mut seed = 0x126u32;
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            u64::from(seed >> 24)
        };
        for _ in 0..512 {
            let wanted: BTreeSet<_> = (0..32).map(|_| next()).collect();
            let future: Vec<_> = (0..64).map(|_| next()).collect();
            for enabled in [false, true] {
                let candidates: BTreeSet<_> = future
                    .iter()
                    .copied()
                    .filter(|id| !wanted.contains(id))
                    .take(MAX_PREFETCH_IMAGES)
                    .collect();
                let expected: BTreeSet<_> = if enabled {
                    wanted.union(&candidates).copied().collect()
                } else {
                    wanted.clone()
                };
                let actual = allowed_assets(&wanted, &future, enabled);
                assert_eq!(actual.as_ref(), &expected);
                assert!(actual.len() <= wanted.len() + MAX_PREFETCH_IMAGES);
                assert_eq!(
                    actual.iter().copied().collect::<Vec<_>>(),
                    expected.into_iter().collect::<Vec<_>>()
                );
            }
        }
    }
}
