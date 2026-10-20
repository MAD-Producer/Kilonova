//! Nonblocking PNG task lifecycle; presentation uses existing status styles.
use crate::*;

impl Shell {
    pub(crate) fn start_frame_export(&mut self, destination: PathBuf) -> Result<(), String> {
        if self.exporting_frame.is_some()
            || self.freezing_movie.is_some()
            || self
                .exporting_movie
                .as_ref()
                .is_some_and(|task| !task.finished())
        {
            return Err(self.workspace.locale.t(T::Exporting).into());
        }
        let control = motion_desktop_media::movie::ExportControl::default();
        self.exporting_frame = Some((
            self.engine.capture_async(destination, control.clone())?,
            control,
        ));
        self.panels.message = Some(self.workspace.locale.t(T::Exporting).into());
        self.dirty = true;
        Ok(())
    }

    pub(crate) fn poll_frame_export(&mut self) {
        let Some((receive, control)) = &self.exporting_frame else {
            return;
        };
        if let Some(result) = self.engine.poll_reply(receive, "capture") {
            self.exporting_frame = None;
            self.panels.message = Some(match result {
                Ok(result) => match result["path"].as_str() {
                    Some(path) => format!("{}: {}", self.workspace.locale.t(T::Saved), path),
                    None => {
                        log::error!("PNG export reply is missing its published path");
                        self.workspace
                            .locale
                            .error("PNG export reply is missing its published path")
                    }
                },
                Err(error) if error == "PNG export cancelled" => {
                    self.workspace.locale.t(T::ExportCancelled).into()
                }
                Err(error) => self.workspace.locale.error(&error),
            });
        } else {
            self.panels.message = Some(format!(
                "{} {:.0}%",
                self.workspace.locale.t(T::Exporting),
                control.progress() * 100.
            ));
        }
        self.dirty = true;
    }
}

#[cfg(all(test, target_os = "windows"))]
#[test]
fn png_shell_shows_export_and_completion_in_both_languages() {
    const CHILD: &str = "MOTION_PNG_SHELL_CAPTURE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "shell::frame_export::png_shell_shows_export_and_completion_in_both_languages",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let screenshots = std::env::var_os("MOTION_FRAME_UI_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.path().join("screens"));
    std::fs::create_dir_all(&screenshots).unwrap();
    for (language, locale) in [("zh", Locale::Zh), ("en", Locale::En)] {
        let directory = root.path().join(language);
        let mut project = Project::new(64, 64, 30, 60).unwrap();
        project.version = 21;
        let mut layer = Layer::solid(
            1,
            "Motion study",
            [12., 24.],
            [32., 32., 0.],
            [1., 0.5, 0.25, 1.],
        );
        layer
            .transform
            .position
            .upsert(0, [16., 32., 0.], motion_core::Ease::Linear)
            .unwrap();
        layer
            .transform
            .position
            .upsert(2, [48., 32., 0.], motion_core::Ease::Linear)
            .unwrap();
        layer.sampling.motion_blur = Some(motion_core::render_sampling::Shutter {
            frames: 2.,
            samples: 8,
        });
        project.layers.push(layer);
        std::fs::create_dir_all(&directory).unwrap();
        motion_core::storage::save(&directory, &project).unwrap();
        let mut shell = Shell::new(directory, None, Some(locale)).unwrap();
        shell.workspace = workspace::Workspace::default();
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        shell.panels.select_only(Some(1));
        #[allow(deprecated)]
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1280, 800)),
                )
                .unwrap(),
        );
        shell.state = shell
            .engine
            .attach_async(window, 1280, 800)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        shell.state = shell.engine.seek(1.).unwrap();
        let size = [1280., 800.];
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let (paint, actions) = shell.draw_commands(size);
            assert!(actions.is_empty());
            let ui = shell.ui_frame(paint, size, true);
            if shell
                .engine
                .present_async(ui)
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap()
            {
                break;
            }
            assert!(Instant::now() < deadline);
        }
        shell
            .start_frame_export(screenshots.join(format!("{language}-frame.png")))
            .unwrap();
        assert_eq!(
            shell.panels.message.as_deref(),
            Some(locale.t(T::Exporting))
        );
        assert!(shell
            .start_frame_export(screenshots.join("duplicate.png"))
            .is_err());
        shell.panels.menu = Some(0);
        let (paint, actions) = shell.draw_commands(size);
        assert!(actions.is_empty());
        let ui = shell.ui_frame(paint, size, true);
        shell
            .engine
            .capture_ui(ui, screenshots.join(format!("{language}-export-menu.png")))
            .unwrap();
        shell.panels.menu = None;
        let deadline = Instant::now() + Duration::from_secs(30);
        while shell.exporting_frame.is_some() {
            shell.poll_frame_export();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(shell
            .panels
            .message
            .as_ref()
            .unwrap()
            .starts_with(locale.t(T::Saved)));
        let (paint, actions) = shell.draw_commands(size);
        assert!(actions.is_empty());
        let ui = shell.ui_frame(paint, size, true);
        shell
            .engine
            .capture_ui(ui, screenshots.join(format!("{language}-saved.png")))
            .unwrap();
        assert!(image::open(screenshots.join(format!("{language}-frame.png"))).is_ok());
        let (entered, waiting) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let blocked = shell
            .engine
            .request(|reply| engine::Command::TestWait {
                entered,
                release: gate,
                reply,
            })
            .unwrap();
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        let cancelled = screenshots.join(format!("{language}-must-not-publish.png"));
        shell.start_frame_export(cancelled.clone()).unwrap();
        shell.action(panels::Action::CancelExport, None).unwrap();
        assert!(
            shell.exporting_frame.is_some(),
            "keep observing until the task acknowledges cancellation"
        );
        release.send(()).unwrap();
        blocked
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while shell.exporting_frame.is_some() {
            shell.poll_frame_export();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            shell.panels.message.as_deref(),
            Some(locale.t(T::ExportCancelled))
        );
        assert!(!cancelled.exists());
        let (paint, actions) = shell.draw_commands(size);
        assert!(actions.is_empty());
        let ui = shell.ui_frame(paint, size, true);
        shell
            .engine
            .capture_ui(ui, screenshots.join(format!("{language}-cancelled.png")))
            .unwrap();
    }
}
