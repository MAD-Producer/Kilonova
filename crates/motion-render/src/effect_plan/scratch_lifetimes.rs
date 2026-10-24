//! Pure scratch placement. The caller supplies the real execution lifetimes,
//! including attachment loads, snapshot copies and final draw consumers.
//! This module neither submits work nor infers liveness from a numeric slot.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Descriptor<K> {
    /// Caller-defined compatibility. A retained logical value may have stricter
    /// semantics than storage reused after a full clear and its last consumer.
    pub class: K,
    pub width: u32,
    pub height: u32,
    /// Derived from the actual native format, never from a slot number.
    pub bytes_per_pixel: u32,
}

impl<K> Descriptor<K> {
    fn bytes(&self) -> Option<u64> {
        u64::from(self.width)
            .checked_mul(u64::from(self.height))?
            .checked_mul(u64::from(self.bytes_per_pixel))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Resource<K> {
    pub descriptor: Descriptor<K>,
    /// Inclusive execution points: an input consumed at a pass cannot alias
    /// that same pass's output, even if this is its last consumer.
    pub first_use: u32,
    pub last_use: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub resources: usize,
    pub allocations: usize,
    pub bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PlacementError {
    ResourceLimit {
        required: usize,
        maximum: usize,
    },
    InvalidResource {
        resource: usize,
        reason: &'static str,
    },
    Arithmetic {
        resource: usize,
    },
    AllocationLimit {
        resource: usize,
        maximum: usize,
    },
    Budget {
        resource: usize,
        required: u64,
        budget: u64,
    },
}

impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceLimit { required, maximum } => write!(
                f,
                "scratch logical resources require {required}; limit is {maximum}"
            ),
            Self::InvalidResource { resource, reason } => {
                write!(f, "scratch resource {resource} is invalid: {reason}")
            }
            Self::Arithmetic { resource } => write!(
                f,
                "scratch resource {resource} capacity arithmetic overflow"
            ),
            Self::AllocationLimit { resource, maximum } => write!(
                f,
                "scratch resource {resource} requires another allocation; limit is {maximum}"
            ),
            Self::Budget {
                resource,
                required,
                budget,
            } => write!(
                f,
                "scratch resource {resource} requires {required} bytes; budget is {budget}"
            ),
        }
    }
}
impl std::error::Error for PlacementError {}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Placement<K> {
    /// Physical descriptor table: admission and reporting use this same table.
    pub allocations: Vec<Descriptor<K>>,
    /// Indexed by the caller's original logical resource order.
    pub assignments: Vec<usize>,
    pub bytes: u64,
}

/// Deterministic greedy placement, bounded by the caller's resource/allocation
/// caps. At most resources * allocations candidates are examined. Dimensions
/// are finalized before native allocation; there is no mid-frame replacement.
/// A larger backing extent does not change a resource's logical valid region.
pub(crate) fn place<K: Clone + Eq>(
    resources: &[Resource<K>],
    limits: Limits,
) -> Result<Placement<K>, PlacementError> {
    if resources.len() > limits.resources {
        return Err(PlacementError::ResourceLimit {
            required: resources.len(),
            maximum: limits.resources,
        });
    }
    // Validate before constructing a partial placement. Inputs are never edited
    // and a rejected plan publishes no physical allocation table.
    for (index, resource) in resources.iter().enumerate() {
        let descriptor = &resource.descriptor;
        let reason = if descriptor.width == 0 || descriptor.height == 0 {
            Some("zero extent")
        } else if descriptor.bytes_per_pixel == 0 {
            Some("zero format byte size")
        } else if resource.first_use > resource.last_use {
            Some("last use precedes first use")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(PlacementError::InvalidResource {
                resource: index,
                reason,
            });
        }
        descriptor
            .bytes()
            .ok_or(PlacementError::Arithmetic { resource: index })?;
    }
    let mut order: Vec<_> = (0..resources.len()).collect();
    order.sort_unstable_by_key(|&index| (resources[index].first_use, index));
    let mut placement = Placement {
        allocations: Vec::<Descriptor<K>>::new(),
        assignments: vec![0; resources.len()],
        bytes: 0,
    };
    let mut ends = Vec::<u32>::new();
    for index in order {
        let resource = &resources[index];
        let descriptor = &resource.descriptor;
        let arithmetic = || PlacementError::Arithmetic { resource: index };
        let new_bytes = descriptor.bytes().ok_or_else(arithmetic)?;
        // Prefer reuse on equal cost, then the first physical ID. Growing a
        // wide/tall bounding rectangle is rejected when a new extent costs less.
        let mut choice = None;
        let mut delta = if placement.allocations.len() < limits.allocations {
            Some(new_bytes)
        } else {
            None
        };
        for (id, backing) in placement.allocations.iter().enumerate() {
            if ends[id] >= resource.first_use
                || backing.class != descriptor.class
                || backing.bytes_per_pixel != descriptor.bytes_per_pixel
            {
                continue;
            }
            let width = backing.width.max(descriptor.width);
            let height = backing.height.max(descriptor.height);
            let grown = u64::from(width)
                .checked_mul(u64::from(height))
                .and_then(|pixels| pixels.checked_mul(u64::from(backing.bytes_per_pixel)));
            // An overflowing candidate is unusable, not a reason to reject a
            // different representable candidate or a new exact-sized extent.
            let Some(grown) = grown else { continue };
            let old = backing.bytes().ok_or_else(arithmetic)?;
            let cost = grown.checked_sub(old).ok_or_else(arithmetic)?;
            if delta.is_none_or(|best| cost < best || (cost == best && choice.is_none())) {
                delta = Some(cost);
                choice = Some(id);
            }
        }
        let delta = delta.ok_or(PlacementError::AllocationLimit {
            resource: index,
            maximum: limits.allocations,
        })?;
        let total = placement.bytes.checked_add(delta).ok_or_else(arithmetic)?;
        if total > limits.bytes {
            return Err(PlacementError::Budget {
                resource: index,
                required: total,
                budget: limits.bytes,
            });
        }
        let id = match choice {
            Some(id) => {
                let backing = &mut placement.allocations[id];
                backing.width = backing.width.max(descriptor.width);
                backing.height = backing.height.max(descriptor.height);
                ends[id] = resource.last_use;
                id
            }
            None => {
                let id = placement.allocations.len();
                placement.allocations.push(descriptor.clone());
                ends.push(resource.last_use);
                id
            }
        };
        placement.assignments[index] = id;
        placement.bytes = total;
    }
    Ok(placement)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Class {
        format: u8,
        alpha: u8,
        usage: u8,
    }
    fn resource(width: u32, height: u32, first: u32, last: u32) -> Resource<Class> {
        Resource {
            descriptor: Descriptor {
                class: Class {
                    format: 0,
                    alpha: 0,
                    usage: 0,
                },
                width,
                height,
                bytes_per_pixel: 4,
            },
            first_use: first,
            last_use: last,
        }
    }
    fn limits() -> Limits {
        Limits {
            resources: 1024,
            allocations: 64,
            bytes: u64::MAX,
        }
    }

    #[test]
    fn inclusive_last_consumer_prevents_same_pass_read_write_alias() {
        let resources = [
            resource(16, 16, 0, 1),
            resource(16, 16, 1, 2),
            resource(16, 16, 3, 4),
        ];
        let plan = place(&resources, limits()).unwrap();
        assert_eq!(plan.assignments, [0, 1, 0]);
        assert_eq!(plan.allocations.len(), 2);
        assert_eq!(plan.bytes, 2 * 16 * 16 * 4);
    }

    #[test]
    fn large_small_large_keeps_logical_regions_and_reuses_one_backing() {
        let resources = [
            resource(1024, 1024, 0, 0),
            resource(64, 64, 1, 1),
            resource(1024, 1024, 2, 2),
        ];
        let plan = place(&resources, limits()).unwrap();
        assert_eq!(plan.assignments, [0, 0, 0]);
        assert_eq!(plan.allocations, [resources[0].descriptor.clone()]);
        assert_eq!(plan.bytes, 1024 * 1024 * 4);
        assert_eq!(
            (
                resources[1].descriptor.width,
                resources[1].descriptor.height
            ),
            (64, 64)
        );
    }

    #[test]
    fn disjoint_aspect_ratios_do_not_force_an_expensive_bounding_square() {
        let resources = [resource(1024, 16, 0, 0), resource(16, 1024, 1, 1)];
        let plan = place(&resources, limits()).unwrap();
        assert_eq!(plan.assignments, [0, 1]);
        assert_eq!(plan.bytes, 2 * 1024 * 16 * 4);
        assert!(plan.bytes < 1024 * 1024 * 4);
    }

    #[test]
    fn cheap_planned_growth_is_not_a_native_reallocation() {
        let plan = place(&[resource(64, 64, 0, 0), resource(64, 96, 1, 1)], limits()).unwrap();
        assert_eq!(plan.assignments, [0, 0]);
        assert_eq!(
            (plan.allocations[0].width, plan.allocations[0].height),
            (64, 96)
        );
        assert_eq!(plan.bytes, 64 * 96 * 4);
    }

    #[test]
    fn overflowing_growth_does_not_hide_a_representable_new_extent() {
        let resources = [resource(u32::MAX, 1, 0, 0), resource(1, u32::MAX, 1, 1)];
        let plan = place(&resources, limits()).unwrap();
        assert_eq!(plan.assignments, [0, 1]);
        assert_eq!(plan.bytes, 2 * u64::from(u32::MAX) * 4);
    }

    #[test]
    fn format_alpha_usage_and_format_size_do_not_alias() {
        let mut resources: Vec<_> = (0..5).map(|i| resource(16, 16, i, i)).collect();
        resources[1].descriptor.class.format = 1;
        resources[2].descriptor.class.alpha = 1;
        resources[3].descriptor.class.usage = 1;
        resources[4].descriptor.bytes_per_pixel = 16;
        let plan = place(&resources, limits()).unwrap();
        assert_eq!(plan.assignments, [0, 1, 2, 3, 4]);
        assert_eq!(plan.bytes, 16 * 16 * (4 * 4 + 16));
    }

    #[test]
    fn original_resource_order_is_preserved_after_lifetime_sorting() {
        let resources = [
            resource(16, 16, 5, 5),
            resource(16, 16, 0, 3),
            resource(16, 16, 1, 4),
        ];
        let plan = place(&resources, limits()).unwrap();
        assert_eq!(plan.assignments, [0, 0, 1]);
        assert_eq!(plan, place(&resources, limits()).unwrap());
    }

    #[test]
    fn budget_rejection_retains_exact_resource_and_required_bytes() {
        let resources = [resource(16, 16, 0, 2), resource(16, 16, 1, 3)];
        let result = place(
            &resources,
            Limits {
                bytes: 1024,
                ..limits()
            },
        );
        assert_eq!(
            result,
            Err(PlacementError::Budget {
                resource: 1,
                required: 2048,
                budget: 1024
            })
        );
        assert_eq!(resources[0].last_use, 2);
    }

    #[test]
    fn caps_do_not_publish_a_partial_plan_or_reuse_a_live_allocation() {
        let resources = [resource(16, 16, 0, 2), resource(16, 16, 1, 3)];
        assert_eq!(
            place(
                &resources,
                Limits {
                    resources: 1,
                    ..limits()
                }
            ),
            Err(PlacementError::ResourceLimit {
                required: 2,
                maximum: 1
            })
        );
        assert_eq!(
            place(
                &resources,
                Limits {
                    allocations: 1,
                    ..limits()
                }
            ),
            Err(PlacementError::AllocationLimit {
                resource: 1,
                maximum: 1
            })
        );
        let disjoint = [resource(1024, 16, 0, 0), resource(16, 1024, 1, 1)];
        let plan = place(
            &disjoint,
            Limits {
                allocations: 1,
                ..limits()
            },
        )
        .unwrap();
        assert_eq!(plan.assignments, [0, 0]);
        assert_eq!(plan.bytes, 1024 * 1024 * 4);
    }

    #[test]
    fn invalid_empty_and_overflowing_resources_have_explicit_outcomes() {
        let empty: [Resource<Class>; 0] = [];
        let plan = place(
            &empty,
            Limits {
                resources: 0,
                allocations: 0,
                bytes: 0,
            },
        )
        .unwrap();
        assert_eq!(plan.bytes, 0);
        assert!(plan.allocations.is_empty());
        let mut zero_format_size = resource(16, 16, 0, 0);
        zero_format_size.descriptor.bytes_per_pixel = 0;
        for (invalid, reason) in [
            (resource(0, 16, 0, 0), "zero extent"),
            (zero_format_size, "zero format byte size"),
            (resource(16, 16, 2, 1), "last use precedes first use"),
        ] {
            assert_eq!(
                place(&[invalid], limits()),
                Err(PlacementError::InvalidResource {
                    resource: 0,
                    reason
                })
            );
        }
        let mut huge = resource(u32::MAX, u32::MAX, 0, 0);
        huge.descriptor.bytes_per_pixel = 16;
        assert_eq!(
            place(&[huge], limits()),
            Err(PlacementError::Arithmetic { resource: 0 })
        );
    }

    #[test]
    fn every_assigned_pair_obeys_liveness_semantics_extents_and_exact_bytes() {
        let resources: Vec<_> = (0..200u32)
            .map(|i| {
                let mut r = resource(8 + ((i * 17) % 8) * 16, 8 + ((i * 31) % 8) * 16, i, i + 3);
                r.descriptor.class.format = (i % 3) as u8;
                r.descriptor.bytes_per_pixel = [4, 8, 16][(i % 3) as usize];
                r
            })
            .collect();
        let plan = place(&resources, limits()).unwrap();
        assert_eq!(
            plan.bytes,
            plan.allocations
                .iter()
                .map(|d| d.bytes().unwrap())
                .sum::<u64>()
        );
        for (i, resource) in resources.iter().enumerate() {
            let backing = &plan.allocations[plan.assignments[i]];
            assert_eq!(backing.class, resource.descriptor.class);
            assert_eq!(backing.bytes_per_pixel, resource.descriptor.bytes_per_pixel);
            assert!(
                backing.width >= resource.descriptor.width
                    && backing.height >= resource.descriptor.height
            );
            for (j, other) in resources.iter().enumerate().take(i) {
                if plan.assignments[i] == plan.assignments[j] {
                    assert!(
                        resource.last_use < other.first_use || other.last_use < resource.first_use
                    );
                }
            }
        }
    }
}
