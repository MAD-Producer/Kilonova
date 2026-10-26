//! Def/use graph for atomic draw executions. A draw's final value stays live
//! through its snapshot copy or composition consumer. The scheduler may replay
//! or reorder whole draws, but must not interleave their temporary resources.

use super::scratch_lifetimes::{Descriptor, Resource};
use std::{fmt, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Load {
    Clear,
    /// Preserve an attachment written earlier in the same draw. This is not a
    /// fresh SSA value and must not alias a value still read by another pass.
    Preserve,
}

#[derive(Clone, Debug)]
pub(crate) struct Pass<K> {
    pub input: i32,
    pub source: i32,
    pub output: usize,
    pub descriptor: Descriptor<K>,
    pub load: Load,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Input {
    /// Preserve the entire external token domain, including asset/mask tokens.
    External(i32),
    Resource(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PassResources {
    pub input: Input,
    pub source: Input,
    pub output: usize,
}

#[derive(Debug)]
pub(crate) struct Graph<K> {
    pub resources: Vec<Resource<K>>,
    /// Same indexing as the original pass list. These IDs are logical values,
    /// not physical texture IDs and not original semantic slot numbers.
    pub passes: Vec<PassResources>,
    pub draw_outputs: Vec<Option<usize>>,
}

/// Bounded, allocation-independent use information for optimization admission.
/// This is not a pointwise equivalence proof; those proofs remain mandatory.
pub(crate) struct GroupingDependencies {
    outputs: Vec<usize>,
    uses: Vec<GroupUse>,
}

#[derive(Default)]
struct GroupUse {
    writes: usize,
    last_reader: Option<usize>,
    terminal: bool,
}

impl<K> Graph<K> {
    pub fn grouping_dependencies(&self) -> GroupingDependencies {
        let mut uses: Vec<_> = (0..self.resources.len())
            .map(|_| GroupUse::default())
            .collect();
        for (index, pass) in self.passes.iter().enumerate() {
            uses[pass.output].writes += 1;
            for input in [pass.input, pass.source] {
                if let Input::Resource(id) = input {
                    // Two ports reading the same value are one consuming
                    // operation, not a branch to two independent consumers.
                    uses[id].last_reader = Some(index);
                }
            }
        }
        for &id in self.draw_outputs.iter().flatten() {
            uses[id].terminal = true;
        }
        GroupingDependencies {
            outputs: self.passes.iter().map(|p| p.output).collect(),
            uses,
        }
    }
}

impl GroupingDependencies {
    /// A replacement may expose only the last value. Every other value it
    /// removes must be consumed inside the group. Terminal snapshot/composition
    /// uses and preserved attachments are explicit boundaries. Imported values
    /// and the final value may have other readers; the separate shader proof
    /// must establish that their meaning and quantization remain unchanged.
    pub fn is_closed_group(&self, range: Range<usize>) -> bool {
        let Some(outputs) = self.outputs.get(range.clone()).filter(|v| !v.is_empty()) else {
            return false;
        };
        let final_value = outputs[outputs.len() - 1];
        outputs.iter().all(|&id| {
            let value = &self.uses[id];
            value.writes == 1
                && (id == final_value
                    || (!value.terminal
                        && value.last_reader.is_none_or(|reader| reader < range.end)))
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GraphError {
    ResourceLimit { required: usize, maximum: usize },
    SlotLimit,
    ExecutionPointOverflow,
    DrawRange { draw: usize },
    OutputSlot { pass: usize, slot: usize },
    UninitializedInput { pass: usize, slot: usize },
    UninitializedLoad { pass: usize, slot: usize },
    ChangedLoadDescriptor { pass: usize, slot: usize },
    SampledAttachment { pass: usize, slot: usize },
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceLimit { required, maximum } => write!(
                f,
                "scratch graph has {required} passes; resource limit is {maximum}"
            ),
            Self::SlotLimit => f.write_str("scratch graph requires at least one semantic slot"),
            Self::ExecutionPointOverflow => f.write_str("scratch graph execution points overflow"),
            Self::DrawRange { draw } => write!(
                f,
                "scratch graph draw {draw} pass range is invalid or not contiguous"
            ),
            Self::OutputSlot { pass, slot } => {
                write!(f, "scratch graph pass {pass} output slot {slot} is invalid")
            }
            Self::UninitializedInput { pass, slot } => write!(
                f,
                "scratch graph pass {pass} reads uninitialized slot {slot}"
            ),
            Self::UninitializedLoad { pass, slot } => write!(
                f,
                "scratch graph pass {pass} loads uninitialized slot {slot}"
            ),
            Self::ChangedLoadDescriptor { pass, slot } => write!(
                f,
                "scratch graph pass {pass} changes loaded slot {slot} descriptor"
            ),
            Self::SampledAttachment { pass, slot } => write!(
                f,
                "scratch graph pass {pass} samples its loaded attachment slot {slot}"
            ),
        }
    }
}
impl std::error::Error for GraphError {}

/// Each draw has an explicit terminal consumer after all of its passes.
/// Dependency snapshots live outside scratch ownership; their consumers must
/// bind an external texture, not a scratch resource from a different draw.
/// Validate the complete input before publishing any graph. Resource count is
/// bounded by the caller's pass cap, including preserved-attachment passes.
pub(crate) fn build<K: Clone + Eq>(
    passes: &[Pass<K>],
    draws: &[Range<usize>],
    slots: usize,
    maximum_passes: usize,
) -> Result<Graph<K>, GraphError> {
    if passes.len() > maximum_passes {
        return Err(GraphError::ResourceLimit {
            required: passes.len(),
            maximum: maximum_passes,
        });
    }
    if slots == 0 {
        return Err(GraphError::SlotLimit);
    }
    let mut cursor = 0;
    for (draw, range) in draws.iter().enumerate() {
        if range.start != cursor || range.end < range.start || range.end > passes.len() {
            return Err(GraphError::DrawRange { draw });
        }
        cursor = range.end;
    }
    if cursor != passes.len() {
        return Err(GraphError::DrawRange { draw: draws.len() });
    }
    let points = passes
        .len()
        .checked_add(draws.len())
        .ok_or(GraphError::ExecutionPointOverflow)?;
    u32::try_from(points).map_err(|_| GraphError::ExecutionPointOverflow)?;
    let mut graph = Graph {
        resources: Vec::<Resource<K>>::with_capacity(passes.len()),
        passes: Vec::with_capacity(passes.len()),
        draw_outputs: Vec::with_capacity(draws.len()),
    };
    for (draw, range) in draws.iter().enumerate() {
        // A new atomic execution cannot depend on the previous draw's scratch.
        let mut current: Vec<Option<usize>> = vec![None; slots];
        let mut final_output = None;
        for index in range.clone() {
            let pass = &passes[index];
            let point = (index + draw) as u32;
            if pass.output >= slots {
                return Err(GraphError::OutputSlot {
                    pass: index,
                    slot: pass.output,
                });
            }
            let mut read = |token: i32| -> Result<Input, GraphError> {
                if token < 0 {
                    return Ok(Input::External(token));
                }
                let slot = token as usize;
                let id = current
                    .get(slot)
                    .copied()
                    .flatten()
                    .ok_or(GraphError::UninitializedInput { pass: index, slot })?;
                graph.resources[id].last_use = point;
                Ok(Input::Resource(id))
            };
            // Reads precede writes. Inclusive points prevent a pass from
            // sampling an allocation that also becomes its render attachment.
            let input = read(pass.input)?;
            let source = read(pass.source)?;
            let output = match pass.load {
                Load::Clear => {
                    let id = graph.resources.len();
                    graph.resources.push(Resource {
                        descriptor: pass.descriptor.clone(),
                        first_use: point,
                        last_use: point,
                    });
                    id
                }
                Load::Preserve => {
                    let id = current[pass.output].ok_or(GraphError::UninitializedLoad {
                        pass: index,
                        slot: pass.output,
                    })?;
                    if graph.resources[id].descriptor != pass.descriptor {
                        return Err(GraphError::ChangedLoadDescriptor {
                            pass: index,
                            slot: pass.output,
                        });
                    }
                    if input == Input::Resource(id) || source == Input::Resource(id) {
                        return Err(GraphError::SampledAttachment {
                            pass: index,
                            slot: pass.output,
                        });
                    }
                    graph.resources[id].last_use = point;
                    id
                }
            };
            current[pass.output] = Some(output);
            final_output = Some(output);
            graph.passes.push(PassResources {
                input,
                source,
                output,
            });
        }
        if let Some(output) = final_output {
            graph.resources[output].last_use = (range.end + draw) as u32;
        }
        graph.draw_outputs.push(final_output);
    }
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::super::scratch_lifetimes::{Limits, place};
    use super::*;

    fn pass(input: i32, source: i32, output: usize) -> Pass<u8> {
        Pass {
            input,
            source,
            output,
            descriptor: Descriptor {
                class: 0,
                width: 16,
                height: 16,
                bytes_per_pixel: 4,
            },
            load: Load::Clear,
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
    fn reads_use_the_previous_value_and_include_the_same_pass_point() {
        let graph = build(&[pass(-1, -1, 0), pass(0, 0, 0)], &[0..2], 8, 1024).unwrap();
        assert_eq!(graph.passes[1].input, Input::Resource(0));
        assert_eq!(graph.passes[1].source, Input::Resource(0));
        assert_eq!(graph.passes[1].output, 1);
        assert_eq!(graph.resources[0].last_use, 1);
        assert_eq!(graph.resources[1].first_use, 1);
        assert_eq!(graph.resources[1].last_use, 2);
        let placement = place(&graph.resources, limits()).unwrap();
        assert_ne!(placement.assignments[0], placement.assignments[1]);
    }

    #[test]
    fn grouping_retains_an_intermediate_read_by_a_later_branch() {
        let graph = build(
            &[pass(-1, -1, 0), pass(0, 0, 1), pass(1, 0, 2), pass(2, 1, 3)],
            &[0..4],
            8,
            1024,
        )
        .unwrap();
        let dependencies = graph.grouping_dependencies();
        // The source port of pass 3 still needs the intermediate value from 1.
        assert!(!dependencies.is_closed_group(1..3));
        assert!(dependencies.is_closed_group(1..4));
        // Imported values may have additional consumers outside the group.
        assert!(dependencies.is_closed_group(2..4));
    }

    #[test]
    fn grouping_uses_value_identity_and_does_not_count_duplicate_ports_as_branches() {
        let graph = build(
            &[pass(-1, -1, 0), pass(0, 0, 0), pass(0, 0, 0)],
            &[0..3],
            8,
            1024,
        )
        .unwrap();
        assert_ne!(graph.passes[0].output, graph.passes[1].output);
        let dependencies = graph.grouping_dependencies();
        // Reusing slot 0 does not keep the earlier SSA value live, and both
        // ports of the next pass are a single consuming operation.
        assert!(dependencies.is_closed_group(0..2));
        assert!(dependencies.is_closed_group(0..3));
        assert!(!dependencies.is_closed_group(1..1));
        assert!(!dependencies.is_closed_group(0..4));
    }

    #[test]
    fn grouping_cannot_remove_a_terminal_snapshot_or_preserved_attachment() {
        let graph = build(&[pass(-1, -1, 0), pass(-1, -1, 0)], &[0..1, 1..2], 8, 1024).unwrap();
        let dependencies = graph.grouping_dependencies();
        assert!(!dependencies.is_closed_group(0..2));
        assert!(dependencies.is_closed_group(0..1));
        assert!(dependencies.is_closed_group(1..2));

        let mut preserve = pass(-1, -1, 0);
        preserve.load = Load::Preserve;
        let graph = build(&[pass(-1, -1, 0), preserve], &[0..2], 8, 1024).unwrap();
        let dependencies = graph.grouping_dependencies();
        assert!(!dependencies.is_closed_group(0..2));
        assert!(!dependencies.is_closed_group(1..2));
    }

    #[test]
    fn source_value_survives_multiple_ping_pong_inputs() {
        let graph = build(
            &[
                pass(-1, -1, 1),
                pass(1, 1, 2),
                pass(2, 1, 3),
                pass(3, 1, 2),
                pass(2, 1, 0),
            ],
            &[0..5],
            8,
            1024,
        )
        .unwrap();
        assert_eq!(graph.resources[0].last_use, 4);
        let placement = place(&graph.resources, limits()).unwrap();
        for id in 1..graph.resources.len() {
            assert_ne!(placement.assignments[0], placement.assignments[id]);
        }
        assert_eq!(placement.assignments[1], placement.assignments[3]);
        assert_eq!(graph.draw_outputs, [Some(4)]);
        assert_eq!(graph.resources[4].last_use, 5);
    }

    #[test]
    fn snapshots_and_final_draws_are_terminal_consumers_before_next_execution() {
        let graph = build(&[pass(-1, -1, 0), pass(-1, -1, 0)], &[0..1, 1..2], 8, 1024).unwrap();
        assert_eq!(graph.resources[0].last_use, 1);
        assert_eq!(graph.resources[1].first_use, 2);
        let placement = place(&graph.resources, limits()).unwrap();
        assert_eq!(placement.assignments, [0, 0]);
        assert_eq!(graph.draw_outputs, [Some(0), Some(1)]);
    }

    #[test]
    fn external_asset_and_mask_domains_are_preserved_not_allocated() {
        let graph = build(&[pass(-2, -100_003, 0)], &[0..1], 8, 1024).unwrap();
        assert_eq!(graph.passes[0].input, Input::External(-2));
        assert_eq!(graph.passes[0].source, Input::External(-100_003));
        assert_eq!(graph.resources.len(), 1);
    }

    #[test]
    fn preserved_attachment_is_one_continuous_value_until_last_consumer() {
        let mut load = pass(-1, -1, 7);
        load.load = Load::Preserve;
        let graph = build(
            &[pass(-1, -1, 7), pass(-1, -1, 2), load, pass(7, 7, 0)],
            &[0..4],
            8,
            1024,
        )
        .unwrap();
        assert_eq!(graph.resources.len(), 3);
        assert_eq!(graph.passes[2].output, graph.passes[0].output);
        assert_eq!(
            (graph.resources[0].first_use, graph.resources[0].last_use),
            (0, 3)
        );
        let placement = place(&graph.resources, limits()).unwrap();
        assert_ne!(placement.assignments[0], placement.assignments[1]);
    }

    #[test]
    fn a_preserved_attachment_cannot_appear_without_an_earlier_clear_or_change_extent() {
        let mut load = pass(-1, -1, 0);
        load.load = Load::Preserve;
        assert_eq!(
            build(&[load.clone()], &[0..1], 8, 1024).unwrap_err(),
            GraphError::UninitializedLoad { pass: 0, slot: 0 }
        );
        load.descriptor.width = 32;
        assert_eq!(
            build(&[pass(-1, -1, 0), load], &[0..2], 8, 1024).unwrap_err(),
            GraphError::ChangedLoadDescriptor { pass: 1, slot: 0 }
        );
    }

    #[test]
    fn a_draw_must_not_read_a_previous_draws_temporary() {
        assert_eq!(
            build(&[pass(-1, -1, 0), pass(0, 0, 1)], &[0..1, 1..2], 8, 1024).unwrap_err(),
            GraphError::UninitializedInput { pass: 1, slot: 0 }
        );
    }

    #[test]
    fn malformed_ranges_slots_and_resource_caps_do_not_publish_a_partial_graph() {
        let passes = [pass(-1, -1, 0), pass(0, 0, 1)];
        for draws in [vec![0..3], vec![1..2], vec![0..1], vec![0..2, 1..2]] {
            assert!(matches!(
                build(&passes, &draws, 8, 1024),
                Err(GraphError::DrawRange { .. })
            ));
        }
        assert_eq!(
            build(&passes, &[0..2], 8, 1).unwrap_err(),
            GraphError::ResourceLimit {
                required: 2,
                maximum: 1
            }
        );
        assert_eq!(
            build(&[pass(-1, -1, 8)], &[0..1], 8, 1024).unwrap_err(),
            GraphError::OutputSlot { pass: 0, slot: 8 }
        );
        assert_eq!(
            build(&[pass(8, -1, 0)], &[0..1], 8, 1024).unwrap_err(),
            GraphError::UninitializedInput { pass: 0, slot: 8 }
        );
        assert_eq!(
            build::<u8>(&[], &[], 0, 1024).unwrap_err(),
            GraphError::SlotLimit
        );
    }

    #[test]
    fn preserved_attachment_cannot_also_be_a_sampled_texture_in_the_same_pass() {
        let mut load = pass(0, -1, 0);
        load.load = Load::Preserve;
        assert_eq!(
            build(&[pass(-1, -1, 0), load], &[0..2], 8, 1024).unwrap_err(),
            GraphError::SampledAttachment { pass: 1, slot: 0 }
        );
    }

    #[test]
    fn physical_assignments_survive_atomic_draw_reordering_and_replay() {
        let mut passes = Vec::new();
        let mut draws = Vec::new();
        for draw in 0..3 {
            let start = passes.len();
            for (input, source, output) in [(-1, -1, 0), (0, 0, 1), (1, 0, 2), (2, 0, 1), (1, 0, 0)]
            {
                let mut p = pass(input, source, output);
                p.descriptor.width = [32, 8, 128][draw];
                p.descriptor.height = [8, 64, 16][draw];
                passes.push(p);
            }
            draws.push(start..passes.len());
        }
        let graph = build(&passes, &draws, 8, 1024).unwrap();
        let placement = place(&graph.resources, limits()).unwrap();
        let mut physical = vec![None; placement.allocations.len()];
        // Simulate values, not pixels or native allocation. Final consumers
        // finish before the next atomic execution, even when a draw is replayed.
        for draw in [2, 0, 1, 2, 1, 0] {
            let mut logical = [None; 8];
            let mut final_value = None;
            for index in draws[draw].clone() {
                let p = &passes[index];
                let mapped = graph.passes[index];
                let read = |token: i32, mapped: Input| {
                    let expected = if token < 0 {
                        token as i64
                    } else {
                        logical[token as usize].unwrap()
                    };
                    let actual = match mapped {
                        Input::External(token) => token as i64,
                        Input::Resource(id) => physical[placement.assignments[id]].unwrap(),
                    };
                    assert_eq!(actual, expected, "draw={draw} pass={index}");
                    expected
                };
                let value =
                    read(p.input, mapped.input) * 3 + read(p.source, mapped.source) + index as i64;
                physical[placement.assignments[mapped.output]] = Some(value);
                logical[p.output] = Some(value);
                final_value = Some(value);
            }
            let output = graph.draw_outputs[draw].unwrap();
            assert_eq!(physical[placement.assignments[output]], final_value);
        }
    }

    #[test]
    fn empty_draws_keep_original_indices_and_add_no_allocations() {
        let graph = build(&[pass(-1, -1, 0)], &[0..0, 0..1, 1..1], 8, 1024).unwrap();
        assert_eq!(graph.draw_outputs, [None, Some(0), None]);
        assert_eq!(
            (graph.resources[0].first_use, graph.resources[0].last_use),
            (1, 2)
        );
    }
}
