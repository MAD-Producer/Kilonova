use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn grouped_extents_share_one_ticket_and_all_captures_match_a_cold_renderer_without_native_creation() {
    let input = scene();
    let observer = motion_core::Observer::new(16, 16);
    let variants: Vec<_> = [[16, 16], [8, 16], [16, 8], [8, 8]].into_iter().enumerate()
        .map(|(index, size)| PipelinePreparationView {
            camera: if index == 0 { PipelinePreparationCamera::Composition }
                else { PipelinePreparationCamera::Observer(observer.clone()) }, size,
        }).collect();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_pointwise_fusion(true);
    let job = renderer.prepare_effect_pipeline_job(vec![input.clone()], 16, 16, CONTEXT).unwrap()
        .for_views(variants.clone()).unwrap();
    let ticket = job.ticket();
    let prepared = run(job).unwrap();
    assert!(prepared.pipeline_count() > 0);
    assert!(!prepared.limited());
    assert!(prepared.pipeline_count() <= MAX_PREPARED_PIPELINES);
    renderer.install_prepared_effect_pipelines(prepared, CONTEXT).unwrap();
    assert_eq!(ticket.status().phase, PipelinePreparationPhase::Installed);
    let before = renderer.effect_pipeline_cache_info().foreground_builds;
    let mut reference = pollster::block_on(Renderer::headless()).unwrap();
    reference.set_pointwise_fusion(true);
    for view in &variants {
        let selected = input.fork_view(view.camera.borrow()).unwrap();
        let target = renderer.capture_target(view.size[0], view.size[1]).unwrap();
        let actual = renderer.capture(&selected, &target).unwrap().0;
        let expected = reference.capture_target(view.size[0], view.size[1]).unwrap();
        assert_eq!(actual, reference.capture(&selected, &expected).unwrap().0);
        assert!(actual.chunks_exact(4).any(|pixel| pixel[3] != 0));
        assert_eq!(renderer.effect_pipeline_cache_info().foreground_builds, before,
            "group cell rebuilt a supposedly prepared pipeline");
    }
    assert!(reference.effect_pipeline_cache_info().foreground_builds > 0,
        "cold negative control did not exercise native creation");
    renderer.check_health().unwrap();
    reference.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalid_group_counts_and_zero_extents_do_not_install_or_leave_a_live_ticket() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    for (count, size) in [(0, [16,16]), (3, [16,16]), (5, [16,16]), (1, [0,16]), (2, [16,0])] {
        let job = renderer.prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT).unwrap();
        let ticket = job.ticket();
        assert!(job.for_views(vec![PipelinePreparationView {
            camera: PipelinePreparationCamera::Composition, size,
        }; count]).is_err());
        assert!(ticket.cancelled());
        assert_eq!(ticket.status().phase, PipelinePreparationPhase::Cancelled);
        assert_eq!(renderer.effect_pipeline_cache_info().previous_entries, 0);
        assert_eq!(renderer.effect_pipeline_cache_info().foreground_builds, 0);
    }
}
