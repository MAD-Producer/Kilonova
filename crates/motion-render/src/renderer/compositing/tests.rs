use super::*;
use crate::{
    effect_plan::{EffectFramePlan, LayerComposite},
    test_allocations,
};

fn frame(count: usize) -> EffectFramePlan {
    EffectFramePlan {
        draws: (0..count)
            .map(|index| crate::effect_plan::PlannedDraw {
                layer: u64::MAX - index as u64,
                words: [0.; crate::effect_plan::DRAW_WORDS],
                pass_start: 0,
                pass_end: 0,
            })
            .collect(),
        compositing: (0..count)
            .map(|index| LayerComposite {
                blend: 0,
                space: 0,
                visible: true,
                matte: if index % 8 == 0 { -1 } else { index as i32 - 1 },
                mode: if index % 2 == 0 { 3 } else { 1 },
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn target_collection_reuses_storage_and_preserves_duplicates_and_order() {
    let mut planning = MattePlanning::default();
    for count in [1, 8, MAX_LAYERS] {
        let frame = frame(count);
        let expected: Vec<_> = frame
            .compositing
            .iter()
            .filter(|c| c.matte >= 0)
            .map(|c| (frame.draws[c.matte as usize].layer, c.mode >= 3))
            .collect();
        planning.collect_targets(&frame);
        let allocations = test_allocations::count(|| {
            for _ in 0..120 {
                planning.collect_targets(&frame);
                assert_eq!(planning.needed, expected);
            }
        });
        assert_eq!(allocations, 0);
    }
    let mut duplicate = frame(4);
    duplicate.compositing[2].matte = 0;
    duplicate.compositing[3].matte = 0;
    planning.collect_targets(&duplicate);
    assert_eq!(
        planning.needed,
        [(u64::MAX, false), (u64::MAX, true), (u64::MAX, false)]
    );
    planning.collect_targets(&EffectFramePlan::default());
    assert!(planning.needed.is_empty());
}

#[test]
fn reused_coverage_matches_reference_for_changing_graphs_and_recovers_after_cycle() {
    let mut seed = 0xa1d3_84c9u64;
    let mut planning = MattePlanning::default();
    for case in 0..512 {
        let mut frame = frame(case % (MAX_LAYERS + 1));
        for (index, composite) in frame.compositing.iter_mut().enumerate() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            composite.matte = if index == 0 || seed % 4 == 0 {
                -1
            } else {
                (seed as usize % index) as i32
            };
            composite.mode = (seed >> 32) as u32 % 4 + 1;
        }
        assert_eq!(
            planning.prepare_coverage(&frame).unwrap(),
            reference_order(&frame).unwrap()
        );
    }
    let mut cycle = frame(3);
    cycle.compositing[0].matte = 2;
    let expected = reference_order(&cycle).unwrap_err().to_string();
    assert_eq!(
        planning.prepare_coverage(&cycle).unwrap_err().to_string(),
        expected
    );
    let healthy = frame(MAX_LAYERS);
    assert_eq!(
        planning.prepare_coverage(&healthy).unwrap(),
        reference_order(&healthy).unwrap()
    );
    assert!(planning
        .prepare_coverage(&EffectFramePlan::default())
        .unwrap()
        .is_empty());
}

#[test]
fn warmed_coverage_order_reuses_its_traversal_storage() {
    for count in [1, 8, MAX_LAYERS] {
        let frame = frame(count);
        let expected = reference_order(&frame).unwrap();
        let mut planning = MattePlanning::default();
        assert_eq!(planning.prepare_coverage(&frame).unwrap(), expected);
        let allocations = test_allocations::count(|| {
            for _ in 0..120 {
                assert_eq!(planning.prepare_coverage(&frame).unwrap(), expected);
            }
        });
        println!("coverage layers={count} calls=120 allocations={allocations}");
        assert_eq!(
            allocations, 0,
            "warmed coverage traversal must reuse its storage"
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn warmed_matte_target_membership_preserves_resources_without_allocating() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.ensure_compositing_gpu(0., "initialize").unwrap();
    for count in [1, 8, MAX_LAYERS] {
        // Include duplicate requests, and both alpha and luma for one source.
        let needed: Vec<_> = (1..=count as u64)
            .flat_map(|id| [(id, false), (id, false), (id, true)])
            .collect();
        let gpu = renderer.compositing_gpu.as_mut().unwrap();
        assert!(!gpu
            .prepare_targets(
                &renderer.device,
                &renderer.image_layout,
                &renderer.sampler,
                [2, 2],
                &needed,
                0,
                1 << 20,
                false
            )
            .unwrap());
        let ledger = renderer.texture_resource_ledger();
        let bytes = renderer.texture_bytes();
        let allocations = test_allocations::count(|| {
            for _ in 0..120 {
                assert!(!renderer
                    .compositing_gpu
                    .as_mut()
                    .unwrap()
                    .prepare_targets(
                        &renderer.device,
                        &renderer.image_layout,
                        &renderer.sampler,
                        [2, 2],
                        &needed,
                        0,
                        1 << 20,
                        false
                    )
                    .unwrap());
            }
        });
        println!("matte targets={count} calls=120 allocations={allocations}");
        assert_eq!(
            allocations, 0,
            "warmed target membership must not allocate a fresh set"
        );
        assert_eq!(renderer.texture_resource_ledger(), ledger);
        assert_eq!(renderer.texture_bytes(), bytes);
        assert_eq!(
            renderer.compositing_gpu.as_ref().unwrap().mattes.len(),
            count * 2
        );
    }
    renderer.check_health().unwrap();
}

// Independent pre-change traversal, retained as the exact ordering oracle.
fn reference_order(frame: &crate::effect_plan::EffectFramePlan) -> Result<Vec<(usize, bool)>> {
    fn visit(
        frame: &crate::effect_plan::EffectFramePlan,
        index: usize,
        luma: bool,
        active: &mut std::collections::HashSet<usize>,
        done: &mut std::collections::HashSet<(usize, bool)>,
        out: &mut Vec<(usize, bool)>,
    ) -> Result<()> {
        if done.contains(&(index, luma)) {
            return Ok(());
        }
        if !active.insert(index) {
            return Err(RenderError::Invalid("track matte dependency cycle".into()));
        }
        let c = frame.compositing[index];
        if c.matte >= 0 {
            visit(frame, c.matte as usize, c.mode >= 3, active, done, out)?;
        }
        active.remove(&index);
        done.insert((index, luma));
        out.push((index, luma));
        Ok(())
    }
    let mut active = Default::default();
    let mut done = Default::default();
    let mut out = Vec::new();
    for c in &frame.compositing {
        if c.matte >= 0 {
            visit(
                frame,
                c.matte as usize,
                c.mode >= 3,
                &mut active,
                &mut done,
                &mut out,
            )?;
        }
    }
    Ok(out)
}
