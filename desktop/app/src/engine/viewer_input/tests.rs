use super::*;
use crate::engine::Engine;
use std::path::PathBuf;

fn request(state: &Value, query: Query) -> Request {
    Request { source: KeyframeSource::of(state), frame: state["frame"].as_f64().unwrap(), window: 0, query }
}
fn run(engine: &Engine, request: Request) -> Result<Value, String> {
    engine.viewer_input_async(request).unwrap().recv_timeout(std::time::Duration::from_secs(30)).unwrap()
}

#[test]
fn real_worker_view_queries_cancel_stale_sources_and_preserve_default_error_channels() {
    const TEST: &str = "engine::viewer_input::tests::real_worker_view_queries_cancel_stale_sources_and_preserve_default_error_channels";
    const CHILD: &str = "MOTION_VIEWER_INPUT_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact",TEST,"--nocapture","--test-threads=1"])
            .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
        if let Some(dest) = std::env::var_os("MOTION_VIEWER_INPUT_ARTIFACTS") {
            let dest = PathBuf::from(dest);
            std::fs::create_dir_all(&dest).unwrap();
            for entry in std::fs::read_dir(root.path()).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    std::fs::copy(entry.path(),dest.join(entry.file_name())).unwrap();
                }
            }
            std::fs::write(dest.join("stderr.log"), &output.stderr).unwrap();
            std::fs::write(dest.join("stdout.log"), &output.stdout).unwrap();
        }
        assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        assert!(file.lines().any(|l| l.contains("start #") && l.ends_with(" viewer_states")));
        assert!(file.lines().any(|l| l.contains("start #") && l.ends_with(" viewer_pick")));
        assert!(file.lines().any(|l| l.contains("start #") && l.ends_with(" viewer_drag")));
        for marker in ["expected 1, 2, or 4 cameras", "object=999", "invalid hit-test point", "invalid preview drag"] {
            assert!(file.contains(marker),"file missing {marker}");
            assert!(String::from_utf8_lossy(&output.stderr).contains(marker),"stderr missing {marker}");
            assert!(!String::from_utf8_lossy(&output.stdout).contains(marker),"stdout leaked {marker}");
        }
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let mut project = motion_core::Project::new(64,64,30,60).unwrap();
    for (id,z) in [(1,-80.),(2,80.)] {
        let mut layer = motion_core::Layer::camera(id,"Query camera",64,64);
        if let motion_core::Content::Camera { camera } = &mut layer.content { camera.position.value=[32.,32.,z]; }
        project.layers.push(layer);
    }
    for (id,z) in [(7,-5.),(8,5.)] {
        let mut layer=motion_core::Layer::solid(id,"Query plane",[12.;2],[32.,32.,z],[1.;4]);
        layer.three_d=true;
        project.layers.push(layer);
    }
    motion_core::storage::save(&root.join("project"),&project).unwrap();
    let engine = Engine::start(root.join("project"),motion_desktop_media::platform(),8<<30).unwrap();
    let before = engine.state().unwrap();
    let states = run(&engine,request(&before,Query::States(vec![Camera::Camera(1),Camera::Camera(2)]))).unwrap();
    assert_eq!(states["cancelled"],false);
    assert_eq!(states["views"].as_array().unwrap().len(),2);
    assert_ne!(states["views"][0]["source_transforms"],states["views"][1]["source_transforms"]);
    for (camera,expected) in [(1,7),(2,8)] {
        let picked=run(&engine,request(&before,Query::Pick { camera:Camera::Camera(camera),point:[32.,32.] })).unwrap();
        assert_eq!(picked["pick"]["candidates"][0]["id"],expected);
    }
    let missing=run(&engine,request(&before,Query::States(vec![Camera::Camera(999)]))).unwrap_err();
    assert!(missing.contains("view=0 phase=input_states"));
    assert!(missing.contains("object=999"));
    let count=run(&engine,request(&before,Query::States(vec![]))).unwrap_err();
    assert!(count.contains("expected 1, 2, or 4 cameras"));
    let point=run(&engine,request(&before,Query::Pick { camera:Camera::Camera(1),point:[f64::NAN,0.] })).unwrap_err();
    assert!(point.contains("invalid hit-test point"));
    let drag=|state:&Value,camera,window,size|Drag{source:KeyframeSource::of(state),frame:state["frame"].as_f64().unwrap(),window,camera,object:7,delta:[10.,0.],size};
    let invalid_drag=engine.viewer_drag_async(drag(&before,Camera::Camera(1),0,[0,64])).unwrap().recv_timeout(std::time::Duration::from_secs(30)).unwrap().unwrap_err();
    assert!(invalid_drag.contains("invalid preview drag"));
    let missing_drag=engine.viewer_drag_async(drag(&before,Camera::Camera(999),0,[64,64])).unwrap().recv_timeout(std::time::Duration::from_secs(30)).unwrap().unwrap_err();
    assert!(missing_drag.contains("object=999"));
    let closed_drag=engine.viewer_drag_async(drag(&before,Camera::Camera(999),71,[64,64])).unwrap().recv_timeout(std::time::Duration::from_secs(30)).unwrap().unwrap();
    assert_eq!(closed_drag["viewerDrag"]["reason"],"window_closed");
    let after = engine.state().unwrap();
    for field in ["project","root","document_generation","composition","frame","revision","history",
        "canUndo","canRedo","observing","observationCamera","source_transforms","projectedLayers"] {
        assert_eq!(after[field],before[field],"query changed {field}");
    }
    engine.seek(10.).unwrap();
    let stale_drag=engine.viewer_drag_async(drag(&before,Camera::Camera(999),0,[64,64])).unwrap().recv_timeout(std::time::Duration::from_secs(30)).unwrap().unwrap();
    assert_eq!(stale_drag["viewerDrag"]["reason"],"context_changed");
    // Superseded requests cancel before resolving even an invalid camera.
    let old_frame=run(&engine,request(&before,Query::States(vec![Camera::Camera(999)]))).unwrap();
    assert_eq!(old_frame["cancelled"],true);
    assert_eq!(old_frame["reason"],"context_changed");
    let current=engine.state().unwrap();
    let mut foreign=request(&current,Query::States(vec![Camera::Camera(999)]));
    foreign.source=KeyframeSource::of(&json!({"root":"obsolete","document_generation":0}));
    assert_eq!(run(&engine,foreign).unwrap()["reason"],"context_changed");
    let mut closed=request(&current,Query::States(vec![Camera::Camera(999)]));
    closed.window=71;
    assert_eq!(run(&engine,closed).unwrap()["reason"],"window_closed");
    let recovered=run(&engine,request(&current,Query::Pick {camera:Camera::Camera(2),point:[32.,32.]})).unwrap();
    assert_eq!(recovered["pick"]["candidates"][0]["id"],8);
    std::fs::write(root.join("views.json"),serde_json::to_vec_pretty(&states).unwrap()).unwrap();
    std::fs::write(root.join("report.json"),serde_json::to_vec_pretty(&json!({
        "sharedStatePreserved":true,"queries":states["views"].as_array().unwrap().len(),
        "sourceCancellation":true,"frameCancellation":old_frame,"closedWindowCancellation":true,
        "missingCamera":missing,"invalidCount":count,"invalidPoint":point,"recoveredPick":recovered,
        "invalidDrag":invalid_drag,"missingDragCamera":missing_drag,"closedDrag":closed_drag["viewerDrag"],"staleDrag":stale_drag["viewerDrag"],
        "scope":"Production worker projection/picking and guarded drag failures; native panel integration is tested separately."
    })).unwrap()).unwrap();
    drop(engine);
    crate::logging::flush();
}
