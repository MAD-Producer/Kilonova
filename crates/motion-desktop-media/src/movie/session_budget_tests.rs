use super::*;
use motion_render::Renderer;

#[test]
fn frozen_package_movie_uses_captured_cpu_policy_and_preserves_previous_output_on_refusal() {
    let root = tempfile::tempdir().unwrap();
    let mut session = Session::new(package_budget_project(), root.path().into(), crate::platform()).unwrap();
    let before = session.engine.snapshot();
    let revision = session.engine.revision();
    let output = root.path().join("frozen-package.mp4");
    fs::write(&output, b"previous publication").unwrap();
    let original = session.effects.memory_policy();
    let low_policy = MemoryPolicy { decoded_image_budget_bytes: 4, ..original };
    session.configure_memory_policy(low_policy).unwrap();
    let control = ExportControl::default();
    let low = FrozenMovie::capture_plan(&session, &control).unwrap().freeze(&control).unwrap();
    session.configure_memory_policy(original).unwrap();
    let error = low.export(&output, &control, |_| {}).err().unwrap();
    for context in ["effect resource package=", "phase=decode", "budget_bytes=4"] {
        assert!(error.contains(context), "{error}");
    }
    assert_eq!(fs::read(&output).unwrap(), b"previous publication");
    let full = FrozenMovie::capture_plan(&session, &control).unwrap().freeze(&control).unwrap();
    session.configure_memory_policy(low_policy).unwrap();
    assert_eq!(session.engine.snapshot(), before);
    assert_eq!(session.engine.revision(), revision);
    assert!(!session.engine.can_undo());
    drop(session);
    let result = full.export(&output, &control, |_| {}).unwrap();
    assert_eq!((result.width, result.height, result.frames, result.revision), (32, 24, 2, revision));
    assert_eq!(fs::metadata(&output).unwrap().len(), result.bytes);
    let decoded = crate::verify_video_runtime(&output).unwrap();
    assert_eq!(decoded["decoded"], true);
    assert_eq!(decoded["sourceFrames"], 2);
}

#[test]
fn frozen_movie_keeps_cpu_decode_allowance_across_live_changes_and_session_close() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("assets")).unwrap();
    image::save_buffer(root.path().join("assets/image.png"), &[31, 129, 221, 255].repeat(4),
        4, 1, image::ColorType::Rgba8).unwrap();
    let mut project = Project::new(32, 24, 30, 2).unwrap();
    project.assets.push(motion_core::Asset { id: 17, path: "assets/image.png".into(), width: 4, height: 1 });
    let mut layer = motion_core::Layer::solid(1, "image", [32., 24.], [16., 12., 0.], [1.; 4]);
    layer.content = motion_core::Content::Image { asset: 17 };
    project.layers.push(layer);
    let mut session = Session::new(project, root.path().into(), crate::platform()).unwrap();
    let before = session.engine.snapshot();
    let revision = session.engine.revision();
    let output = root.path().join("frozen-cpu.mp4");
    fs::write(&output, b"previous publication").unwrap();
    let mut policy = session.effects.memory_policy();
    policy.decoded_image_budget_bytes = 8;
    session.configure_memory_policy(policy).unwrap();
    let control = ExportControl::default();
    let low = FrozenMovie::capture_plan(&session, &control).unwrap().freeze(&control).unwrap();
    policy.decoded_image_budget_bytes = 16;
    session.configure_memory_policy(policy).unwrap();
    let error = low.export(&output, &control, |_| {}).err().unwrap();
    assert!(error.contains("image 17") && error.contains("decoded image budget")
        && error.contains("required_bytes=16") && error.contains("budget_bytes=8"), "{error}");
    assert_eq!(fs::read(&output).unwrap(), b"previous publication");
    let full = FrozenMovie::capture_plan(&session, &control).unwrap().freeze(&control).unwrap();
    policy.decoded_image_budget_bytes = 8;
    session.configure_memory_policy(policy).unwrap();
    assert_eq!(session.engine.snapshot(), before);
    assert_eq!(session.engine.revision(), revision);
    assert!(!session.engine.can_undo());
    drop(session);
    let result = full.export(&output, &control, |_| {}).unwrap();
    assert_eq!((result.width, result.height, result.frames, result.revision), (32, 24, 2, revision));
    assert!(result.bytes > 0);
    assert_eq!(fs::metadata(output).unwrap().len(), result.bytes);
}

#[test]
fn frozen_movie_shares_live_capacity_and_keeps_admission_after_session_close() {
    let root=tempfile::tempdir().unwrap();
    let mut project=Project::new(32,24,30,2).unwrap();
    project.background=[0.,1.,0.,1.];
    let mut session=Session::new(project,root.path().to_owned(),crate::platform()).unwrap();
    let before=session.engine.snapshot();
    let revision=session.engine.revision();
    let budget=session.allocation_budget().clone();
    session.editor_renderer=Some(pollster::block_on(
        Renderer::headless_with_allocation_budget(budget.clone())).unwrap());
    let occupied=budget.info().unwrap();
    let device=session.editor_renderer.as_ref().unwrap().device.clone();
    let queue=session.editor_renderer.as_ref().unwrap().queue.clone();
    let output=root.path().join("protected.mp4");
    fs::write(&output,b"previous publication").unwrap();
    let control=ExportControl::default();
    let frozen=FrozenMovie::capture_plan(&session,&control).unwrap()
        .with_observation_request(8001).freeze(&control).unwrap();
    assert!(frozen.allocation_budget.same_budget(&budget));
    let mut policy=session.effects.memory_policy();
    policy.session_allocation_budget_bytes=occupied.admitted_bytes;
    session.configure_memory_policy(policy).unwrap();
    let error=frozen.export(&output,&control,|_|{}).err().expect("movie must share live capacity");
    assert!(error.contains("export GPU:") && error.contains("admitted render allocations"),"{error}");
    assert_eq!(fs::read(&output).unwrap(),b"previous publication");
    assert_eq!(budget.info().unwrap().admitted_bytes,occupied.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations,occupied.live_allocations);
    assert_eq!(session.engine.snapshot(),before);
    assert_eq!(session.engine.revision(),revision);
    assert!(!session.engine.can_undo());
    let peaks=budget.frame_peaks().unwrap();
    assert!(peaks.active.is_empty());
    assert_eq!(peaks.recent.len(),1);
    assert_eq!(peaks.recent[0].request,8001);
    assert_eq!(peaks.recent[0].operation,motion_render::FrameOperation::ExportJob);
    assert_eq!(peaks.recent[0].outcome,Some(motion_render::AllocationFrameOutcome::Failed));
    assert_eq!(peaks.recent[0].start_admitted_bytes,occupied.admitted_bytes);
    assert_eq!(peaks.recent[0].peak_admitted_bytes,occupied.admitted_bytes);
    policy.session_allocation_budget_bytes=16<<20;
    session.configure_memory_policy(policy).unwrap();
    let control=ExportControl::default();
    let frozen=FrozenMovie::from_session_cancellable(&session,&control).unwrap();
    drop(session);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes,0);
    assert_eq!(budget.info().unwrap().budget_bytes,16<<20);
    let result=frozen.export(&output,&control,|_|{}).unwrap();
    assert_eq!((result.width,result.height,result.frames),(32,24,2));
    assert_eq!(result.revision,revision);
    assert!(result.bytes>0);
    assert_eq!(fs::metadata(&output).unwrap().len(),result.bytes);
    assert!(budget.info().unwrap().peak_admitted_bytes>occupied.admitted_bytes);
    drop(queue);
}

#[test]
fn movie_windows_cover_setup_every_authored_frame_and_post_freeze_cancellation() {
    use motion_render::{AllocationFrameOutcome as Outcome, FrameOperation};
    let root=tempfile::tempdir().unwrap();
    let mut project=Project::new(32,24,30,6).unwrap();
    project.background=[0.,1.,0.,1.];
    let session=Session::new(project,root.path().to_owned(),crate::platform()).unwrap();
    let original=session.engine.snapshot();
    let revision=session.engine.revision();
    let budget=session.allocation_budget().clone();
    let output=root.path().join("six-observed-frames.mp4");
    let cancelled=ExportControl::default();
    let frozen=FrozenMovie::capture_plan(&session,&cancelled).unwrap()
        .with_observation_request(8002).freeze(&cancelled).unwrap();
    assert!(cancelled.cancel());
    fs::write(&output,b"previous publication").unwrap();
    assert_eq!(frozen.export(&output,&cancelled,|_|{}).err().unwrap(),"movie export cancelled");
    assert_eq!(fs::read(&output).unwrap(),b"previous publication");
    let peaks=budget.frame_peaks().unwrap();
    assert_eq!(peaks.recent.len(),1);
    assert_eq!(peaks.recent[0].outcome,Some(Outcome::Cancelled));
    assert_eq!(peaks.recent[0].submissions,0);
    assert_eq!(peaks.recent[0].request,8002);

    let control=ExportControl::default();
    let frozen=FrozenMovie::capture_plan(&session,&control).unwrap()
        .with_observation_request(8003).freeze(&control).unwrap();
    let mut active_seen=false;
    let result=frozen.export(&output,&control,|progress| {
        // The initial report happens after native initialization but before frame0.
        // A blocked encoder/decoder must leave a readable job window as well.
        if progress == 0.1 {
            let snapshot=budget.frame_peaks().unwrap();
            let job=snapshot.active.iter().find(|w| w.operation==FrameOperation::ExportJob).unwrap();
            assert_eq!(job.request,8003);
            assert_eq!(job.source_frame,None);
            assert!(job.peak_admitted_bytes>0);
            assert!(job.outcome.is_none());
            active_seen=true;
        }
    }).unwrap();
    assert!(active_seen);
    assert_eq!((result.width,result.height,result.frames,result.fps),(32,24,6,30));
    assert_eq!(result.revision,revision);
    assert!(!control.cancel());
    let decoded=crate::verify_video_runtime(&output).unwrap();
    assert_eq!(decoded["decoded"],true);
    assert_eq!(decoded["sourceFrames"],6);
    let peaks=budget.frame_peaks().unwrap();
    assert!(peaks.active.is_empty());
    assert_eq!(peaks.omitted_frames,0);
    assert_eq!(peaks.unconfirmed_submissions,0);
    let frames:Vec<_>=peaks.recent.iter().filter(|w| w.request==8003 && w.operation==FrameOperation::Export).collect();
    assert_eq!(frames.len(),6);
    for (index,frame) in frames.iter().enumerate() {
        assert_eq!(frame.source_frame,Some(index as f64));
        assert_eq!(frame.outcome,Some(Outcome::Rendered));
        assert!(frame.submissions>0);
        assert!(frame.peak_admitted_bytes>=frame.start_admitted_bytes);
        assert!(frame.end_admitted_bytes.is_some());
    }
    let job=peaks.recent.iter().find(|w| w.request==8003 && w.operation==FrameOperation::ExportJob).unwrap();
    assert_eq!(job.outcome,Some(Outcome::Rendered));
    assert_eq!(job.start_admitted_bytes,0);
    assert_eq!(job.end_admitted_bytes,Some(0));
    assert!(job.peak_admitted_bytes>0);
    assert!(job.submissions>=6);
    assert!(frames.iter().all(|w| w.peak_admitted_bytes<=job.peak_admitted_bytes));
    assert_eq!(budget.info().unwrap().admitted_bytes,0);
    assert_eq!(session.engine.snapshot(),original);
    assert_eq!(session.engine.revision(),revision);
    assert!(!session.engine.can_undo());
}
