use super::*;
use motion_render::Renderer;

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
    let frozen=FrozenMovie::from_session_cancellable(&session,&control).unwrap();
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
