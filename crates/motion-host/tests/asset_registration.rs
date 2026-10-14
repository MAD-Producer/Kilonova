use motion_core::{storage, Project};
use motion_host::{Platform, Result, Session, SurfaceTarget, VideoDecoder, VideoQuery};
use serde_json::{json, Value};
use std::{fs, path::Path, sync::Arc};

// Registration checks managed files and metadata without probing or decoding
// them again. Any unexpected platform media/surface work fails these tests.
struct Stub;
impl Platform for Stub {
    fn name(&self) -> &'static str {
        "asset-registration-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> Result<()>,
    ) -> Result<motion_media::VideoProbe> {
        panic!("registration must not probe media")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> Result<Box<dyn VideoDecoder>> {
        panic!("registration must not decode video")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(f64) -> Result<()>,
    ) -> Result<motion_core::AudioAsset> {
        panic!("registration must not decode audio")
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> Result<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::WindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> Result<SurfaceTarget> {
        panic!("registration must not create a surface")
    }
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Audio,
    Video,
    Font,
}
const KINDS: [Kind; 3] = [Kind::Audio, Kind::Video, Kind::Font];
impl Kind {
    fn path(self) -> &'static str {
        match self {
            Self::Audio => "assets/source.wav",
            Self::Video => "assets/source.mp4",
            Self::Font => "assets/fonts/source.ttf",
        }
    }
    fn registration(self, path: &str) -> Value {
        match self {
            Self::Audio => json!({"op":"register_audio_asset","asset":{
                "id":1,"path":path,"mime":"audio/wav","bytes":4,"track":0,
                "sample_rate":48000,"channels":2,"sample_frames":48000,"duration_us":1000000
            }}),
            Self::Video => json!({"op":"register_video_asset","asset":{
                "id":2,"path":path,"bytes":4,"mime":"video/avc","track":0,
                "width":16,"height":16,"rotation":0,"display_width":16,"display_height":16,
                "video_start_us":0,"video_end_us":1000000,"duration_us":1000000,
                "frame_count":30,"variable_frame_rate":false,"nominal_frame_rate":30.0,
                "color_standard":1,"color_range":1,"audio_asset":null
            }}),
            Self::Font => json!({"op":"register_font_asset","asset":{
                "id":"test-font","path":path,"name":"Test","face_index":0,"license":"Test"
            }}),
        }
    }
}

fn session(root: &Path) -> Session {
    fs::create_dir_all(root.join("assets/fonts")).unwrap();
    Session::new(
        Project::new(64, 64, 30, 60).unwrap(),
        root.to_path_buf(),
        Arc::new(Stub),
    )
    .unwrap()
}

fn save(session: &Session) {
    storage::save(&session.root, session.engine.project()).unwrap();
    assert_eq!(
        storage::load(&session.root).unwrap(),
        session.engine.snapshot()
    );
}

fn assert_rejected(session: &mut Session, commands: Value, expected_error: &str) {
    save(session);
    let project = session.engine.snapshot();
    let revision = session.engine.revision();
    let undo = session.engine.can_undo();
    let redo = session.engine.can_redo();
    let saved = fs::read(session.root.join("project.json")).unwrap();
    let error = session.apply_commands(&commands.to_string()).unwrap_err();
    assert!(error.contains(expected_error), "unexpected error: {error}");
    assert_eq!(session.engine.snapshot(), project);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.engine.can_undo(), undo);
    assert_eq!(session.engine.can_redo(), redo);
    assert_eq!(fs::read(session.root.join("project.json")).unwrap(), saved);
    save(session);
    assert_eq!(fs::read(session.root.join("project.json")).unwrap(), saved);
}

#[test]
fn valid_audio_video_and_font_registrations_save_and_undo_as_one_edit() {
    for kind in KINDS {
        let directory = tempfile::tempdir().unwrap();
        let mut session = session(&directory.path().join("project"));
        let before = session.engine.snapshot();
        fs::write(session.root.join(kind.path()), b"test").unwrap();
        // apply_commands wraps the wire command in InComposition; this also
        // verifies that resource classification follows composition routing.
        session
            .apply_commands(&kind.registration(kind.path()).to_string())
            .unwrap();
        assert_eq!(session.engine.revision(), 1, "{kind:?}");
        let registered = session.engine.snapshot();
        assert_ne!(registered, before);
        assert!(session.engine.can_undo());
        save(&session);
        assert!(session.engine.undo().unwrap());
        assert_eq!(session.engine.snapshot(), before);
        assert!(!session.engine.can_undo());
        save(&session);
        assert!(session.engine.redo().unwrap());
        assert_eq!(session.engine.snapshot(), registered);
        save(&session);
        assert_eq!(fs::read(session.root.join(kind.path())).unwrap(), b"test");
    }
}

#[test]
fn missing_files_and_directories_are_rejected_for_each_registration_kind() {
    for kind in KINDS {
        let directory = tempfile::tempdir().unwrap();
        let mut session = session(&directory.path().join("project"));
        assert_rejected(&mut session, kind.registration(kind.path()), "素材无法读取");
        fs::create_dir(session.root.join(kind.path())).unwrap();
        assert_rejected(
            &mut session,
            kind.registration(kind.path()),
            "asset is invalid or too large",
        );
    }
}

#[test]
fn audio_and_video_declared_bytes_must_match_the_file() {
    for kind in [Kind::Audio, Kind::Video] {
        let directory = tempfile::tempdir().unwrap();
        let mut session = session(&directory.path().join("project"));
        fs::write(session.root.join(kind.path()), b"test").unwrap();
        for bytes in [3, 5] {
            let mut command = kind.registration(kind.path());
            command["asset"]["bytes"] = json!(bytes);
            assert_rejected(&mut session, command, "asset is invalid or too large");
        }
    }
}

#[test]
fn font_file_size_limit_is_checked_before_commit() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(&directory.path().join("project"));
    // Exercise the disk size limit without allocating the payload in memory.
    fs::File::create(session.root.join(Kind::Font.path()))
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    assert_rejected(
        &mut session,
        Kind::Font.registration(Kind::Font.path()),
        "asset is invalid or too large",
    );
}

#[test]
fn traversal_paths_are_rejected_for_each_registration_kind() {
    for kind in KINDS {
        let directory = tempfile::tempdir().unwrap();
        let mut session = session(&directory.path().join("project"));
        fs::write(directory.path().join("outside.ttf"), b"test").unwrap();
        assert_rejected(
            &mut session,
            kind.registration("assets/fonts/../../../outside.ttf"),
            "invalid asset path component",
        );
    }
}

#[cfg(any(unix, windows))]
fn directory_link(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        // Junctions do not require the Windows symlink privilege. Pass paths
        // via environment variables so they are never interpolated as code.
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "New-Item -ItemType Junction -Path $env:MOTION_ASSET_TEST_LINK -Target $env:MOTION_ASSET_TEST_TARGET -ErrorAction Stop | Out-Null",
            ])
            .env("MOTION_ASSET_TEST_LINK", link)
            .env("MOTION_ASSET_TEST_TARGET", target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction creation failed: {output:?}"
        );
    }
}

#[cfg(any(unix, windows))]
#[test]
fn linked_sources_outside_the_project_are_rejected_for_each_kind() {
    for kind in KINDS {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let outside = directory.path().join("outside-assets");
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(outside.join("fonts")).unwrap();
        directory_link(&outside, &root.join("assets"));
        let mut session = session(&root);
        fs::write(session.root.join(kind.path()), b"test").unwrap();
        assert_rejected(
            &mut session,
            kind.registration(kind.path()),
            "asset resolves outside project directory",
        );
    }
}

#[test]
fn failed_batch_preserves_edits_revision_undo_redo_and_saveability() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(&directory.path().join("project"));
    let original = session.engine.snapshot();
    let color = |red| json!({"op":"set_color","object":0,"value":[red,0.0,0.0,1.0]});
    session.apply_commands(&color(0.25).to_string()).unwrap();
    let first_edit = session.engine.snapshot();
    session.apply_commands(&color(0.5).to_string()).unwrap();
    let second_edit = session.engine.snapshot();
    assert!(session.engine.undo().unwrap());
    assert!(session.engine.can_undo() && session.engine.can_redo());
    fs::write(session.root.join(Kind::Audio.path()), b"test").unwrap();
    assert_rejected(
        &mut session,
        json!([
            color(0.75),
            Kind::Audio.registration(Kind::Audio.path()),
            Kind::Video.registration(Kind::Video.path()),
            color(1.0)
        ]),
        "素材无法读取",
    );
    assert!(session.engine.undo().unwrap());
    assert_eq!(session.engine.snapshot(), original);
    assert!(!session.engine.can_undo());
    assert!(session.engine.redo().unwrap());
    assert_eq!(session.engine.snapshot(), first_edit);
    assert!(session.engine.redo().unwrap());
    assert_eq!(session.engine.snapshot(), second_edit);
    assert!(!session.engine.can_redo());
    save(&session);
}

#[test]
fn shared_video_audio_source_and_font_faces_remain_compatible() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(&directory.path().join("project"));
    fs::write(session.root.join("assets/shared.mp4"), b"test").unwrap();
    fs::write(session.root.join(Kind::Font.path()), b"font").unwrap();
    let mut audio = Kind::Audio.registration("assets/shared.mp4");
    audio["asset"]["mime"] = json!("audio/mp4");
    let mut video = Kind::Video.registration("assets/shared.mp4");
    video["asset"]["audio_asset"] = json!(1);
    let font = Kind::Font.registration(Kind::Font.path());
    let mut second_face = font.clone();
    second_face["asset"]["id"] = json!("test-font-1");
    second_face["asset"]["face_index"] = json!(1);
    session
        .apply_commands(&json!([audio, video, font, second_face]).to_string())
        .unwrap();
    assert_eq!(session.engine.revision(), 1);
    assert_eq!(session.engine.project().audio_assets.len(), 1);
    assert_eq!(session.engine.project().video_assets.len(), 1);
    assert_eq!(session.engine.project().fonts.len(), 2);
    save(&session);
    assert!(session.engine.undo().unwrap());
    assert!(session.engine.project().audio_assets.is_empty());
    assert!(session.engine.project().video_assets.is_empty());
    assert!(session.engine.project().fonts.is_empty());
    save(&session);
}
