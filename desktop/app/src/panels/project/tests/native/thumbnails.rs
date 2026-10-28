use super::*;
use crate::{
    layer_clipboard::KeyframeSource,
    media_assets::{Intent, Source},
};
use motion_core::media_assets::{AssetKind, AssetRef};

fn imports(shell: &mut Shell, events: &mut EventLoop<()>) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Some((_, receive)) = shell.editing.take() {
            shell.accept_edit_state(
                native_test_support::receive(events, &receive, Duration::from_secs(30))
                    .unwrap()
                    .unwrap(),
            );
        }
        shell.poll_project_import().unwrap();
        if shell.library_import.is_none()
            && shell.project_image.is_none()
            && shell.editing.is_none()
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn video_ready(shell: &mut Shell, owner: Option<Panel>, size: [f32; 2]) -> PaintList {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        shell.poll_project_preview();
        let (paint, actions) = frame(shell, owner, size, Input::default());
        assert!(actions.is_empty());
        assert!(
            shell.panels.project.preview.message.is_none(),
            "{:?}",
            shell.panels.project.preview.message
        );
        if !shell.panels.project.preview.busy() && shell.panels.project.preview.bitmap().is_some() {
            return paint;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "video preview did not finish decoding"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
#[ignore = "manual native real image and video poster preview screenshots"]
fn project_thumbnails_and_first_frame_previews_render_real_assets_in_both_languages_and_windows() {
    const CHILD: &str = "MOTION_PROJECT_THUMBNAILS_NATIVE_CHILD";
    const NAME:&str="panels::project::tests::native::thumbnails::project_thumbnails_and_first_frame_previews_render_real_assets_in_both_languages_and_windows";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_PROJECT_THUMBNAILS_NATIVE_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let selected = root.join("reference.png");
    image::RgbaImage::from_fn(160, 90, |x, y| {
        image::Rgba([((x * 255) / 159) as u8, ((y * 255) / 89) as u8, 150, 255])
    })
    .save(&selected)
    .unwrap();
    let mut shell = Shell::new(root.join("project"), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(900, 600)),
            )
            .unwrap(),
    );
    let attach = shell.engine.attach_async(main.clone(), 900, 600).unwrap();
    shell.state = native_test_support::receive(&mut events, &attach, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    shell.window = Some(main);
    shell.attached = true;
    shell.scale = 1.;
    shell.workspace.root = Node::dock(Panel::Project.dock());
    #[allow(deprecated)]
    let floating = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(600, 420)),
            )
            .unwrap(),
    );
    shell.engine.float_window(1, floating, 600, 420).unwrap();
    shell
        .start_project_import(KeyframeSource::of(&shell.state), None, Some(selected))
        .unwrap();
    imports(&mut shell, &mut events);
    let video = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video/sound-24fps.mp4");
    shell
        .start_project_import(KeyframeSource::of(&shell.state), None, Some(video))
        .unwrap();
    imports(&mut shell, &mut events);
    assert!(shell.state["project"]["layers"]
        .as_array()
        .unwrap()
        .is_empty());
    let image_id = shell.state["project"]["assets"][0]["id"].as_u64().unwrap();
    let video_id = shell.state["project"]["video_assets"][0]["id"]
        .as_u64()
        .unwrap();
    // A project reopened after derived-cache cleanup must still show its poster.
    let video_asset =
        serde_json::from_value(shell.state["project"]["video_assets"][0].clone()).unwrap();
    let poster = motion_media::video_cache_path(&root.join("project"), &video_asset)
        .unwrap()
        .with_extension("png");
    std::fs::remove_file(&poster).unwrap();
    shell.panels.project.thumbnails.refresh();
    let before = shell.state["project"].clone();
    let mut cases = Vec::new();
    for (locale, lang) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        for owner in [None, Some(Panel::Project)] {
            let size = if owner.is_some() {
                [600., 420.]
            } else {
                [900., 600.]
            };
            let key = u64::from(owner.is_some());
            shell.panels.locale = locale;
            shell.workspace.locale = locale;
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            let inventory = loop {
                shell.panels.project.thumbnails.poll();
                let (paint, actions) = frame(&mut shell, owner, size, Input::default());
                run(&mut shell, &mut events, actions);
                if paint.images.len() == 2 {
                    assert!(shell.panels.project.thumbnails.message.is_none());
                    break paint;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "real thumbnails did not load; {:?}",
                    shell.panels.project.thumbnails.message
                );
                std::thread::sleep(Duration::from_millis(2));
            };
            let suffix = format!(
                "{lang}-{}",
                if owner.is_some() { "floating" } else { "main" }
            );
            capture(
                &mut shell,
                &mut events,
                inventory,
                size,
                key,
                root.join(format!("{suffix}-inventory.png")),
            );
            let row = rect(
                &shell.panels,
                &RowKey::Item(ProjectItem::Video { id: video_id }),
            );
            let (paint, actions) = frame(&mut shell, owner, size, click(row, MouseButton::Right));
            assert!(actions.is_empty());
            capture(
                &mut shell,
                &mut events,
                paint,
                size,
                key,
                root.join(format!("{suffix}-asset-menu.png")),
            );
            let preview = shell.panels.media_assets.menu_rows[7];
            let (_, actions) = frame(&mut shell, owner, size, click(preview, MouseButton::Left));
            assert!(
                matches!(actions.as_slice(),[Action::MediaAsset{intent:Intent::Preview{owner:actual},..}] if *actual==owner)
            );
            run(&mut shell, &mut events, actions);
            let paint = video_ready(&mut shell, owner, size);
            assert!(paint.images.iter().any(|p| p.area.width() > 100.));
            let preview_area = paint
                .images
                .iter()
                .find(|p| p.area.width() > 100.)
                .unwrap()
                .area;
            capture(
                &mut shell,
                &mut events,
                paint,
                size,
                key,
                root.join(format!("{suffix}-video-first-frame.png")),
            );
            let play = shell.panels.project.browser.buttons[1];
            let (_, actions) = frame(&mut shell, owner, size, click(play, MouseButton::Left));
            assert!(actions.is_empty());
            assert!(shell.panels.project.preview.playing());
            assert!(
                shell.playback.is_none(),
                "source play started composition playback"
            );
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            while shell.panels.project.preview.seconds()
                < shell.panels.project.preview.duration().min(1.) * 0.5
            {
                shell.poll_project_preview();
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            }
            let (_, actions) = frame(&mut shell, owner, size, key_input(Key::Space));
            assert!(actions.is_empty());
            assert!(!shell.panels.project.preview.playing());
            let paint = video_ready(&mut shell, owner, size);
            capture(
                &mut shell,
                &mut events,
                paint,
                size,
                key,
                root.join(format!("{suffix}-video-paused.png")),
            );
            let first = image::open(root.join(format!("{suffix}-video-first-frame.png")))
                .unwrap()
                .into_rgba8();
            let later = image::open(root.join(format!("{suffix}-video-paused.png")))
                .unwrap()
                .into_rgba8();
            let changed = (preview_area.min[1].ceil() as u32..preview_area.max[1].floor() as u32)
                .any(|y| {
                    (preview_area.min[0].ceil() as u32..preview_area.max[0].floor() as u32)
                        .any(|x| first.get_pixel(x, y) != later.get_pixel(x, y))
                });
            assert!(
                changed,
                "decoded playback did not change actual video pixels"
            );
            let time = shell.panels.project.browser.buttons[2];
            frame(&mut shell, owner, size, click(time, MouseButton::Left));
            frame(
                &mut shell,
                owner,
                size,
                Input {
                    mouse: point(time),
                    released: Some((MouseButton::Left, point(time))),
                    ..Default::default()
                },
            );
            frame(
                &mut shell,
                owner,
                size,
                Input {
                    text: "0.250".into(),
                    ..Default::default()
                },
            );
            let (_, actions) = frame(&mut shell, owner, size, key_input(Key::Enter));
            assert!(actions.is_empty());
            assert_eq!(shell.panels.project.preview.seconds(), 0.25);
            let paint = video_ready(&mut shell, owner, size);
            capture(
                &mut shell,
                &mut events,
                paint,
                size,
                key,
                root.join(format!("{suffix}-video-seek.png")),
            );
            let (_, actions) = frame(&mut shell, owner, size, key_input(Key::Escape));
            assert!(actions.is_empty());
            assert!(!shell.panels.project.browser.modal());
            shell.poll_project_preview();
            assert!(!shell.panels.project.preview.busy());
            let source = Source::read(
                &shell.state,
                AssetRef {
                    kind: AssetKind::Image,
                    id: image_id,
                },
            )
            .unwrap();
            run(
                &mut shell,
                &mut events,
                vec![Action::MediaAsset {
                    source,
                    intent: Intent::Preview { owner },
                }],
            );
            let (paint, actions) = frame(&mut shell, owner, size, Input::default());
            assert!(actions.is_empty());
            assert!(paint.images.iter().any(|p| p.area.width() > 100.));
            capture(
                &mut shell,
                &mut events,
                paint,
                size,
                key,
                root.join(format!("{suffix}-image-preview.png")),
            );
            frame(&mut shell, owner, size, key_input(Key::Escape));
            assert_eq!(
                shell.state["project"], before,
                "observational previews mutated project"
            );
            cases.push(serde_json::json!({"language":lang,"owner":format!("{owner:?}"),"realImage":true,"videoFirstFrame":true,"videoPlaybackPixelsChanged":changed,"videoPause":true,"sourceSeekSeconds":0.25,"compositionPlaybackUntouched":true,"menuPreview":true,"projectUntouched":true}));
        }
    }
    std::fs::write(
        root.join("cases.json"),
        serde_json::to_vec_pretty(&cases).unwrap(),
    )
    .unwrap();
    shell.engine.close_window(1).unwrap();
}
