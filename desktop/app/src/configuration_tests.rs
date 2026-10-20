//! Run with controlled child-process preferences, never the developer's files.
use super::*;

const CHILD_ROOT: &str = "MOTION_TEST_PREFERENCES_ROOT";
const SENTINEL: &str = "configuration-isolation-sentinel";

#[test]
fn shell_preferences_ignore_external_configuration() {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        check_shell_preferences(PathBuf::from(root));
        return;
    }

    let fixture = tempfile::tempdir().unwrap();
    let data = fixture.path().join("data");
    let home = fixture.path().join("home");
    let explicit = fixture.path().join("explicit-layout.json");
    #[cfg(target_os = "windows")]
    let implicit = data.join("MotionStudio/desktop-layout-v2.json");
    #[cfg(target_os = "macos")]
    let implicit = home.join("Library/Application Support/MotionStudio/desktop-layout-v2.json");
    #[cfg(all(unix, not(target_os = "macos")))]
    let implicit = data.join("motion-studio/desktop-layout-v2.json");

    let mut external = workspace::Workspace::default();
    external.locale = Locale::Zh;
    external.favorites.insert(SENTINEL.into());
    external.timeline_column_blend = false;
    external.timeline_column_parent = false;
    external.timeline_column_matte = false;
    assert!(external.valid());
    external.save(&explicit).unwrap();
    external.save(&implicit).unwrap();
    assert!(workspace::Workspace::load(&implicit)
        .favorites
        .contains(SENTINEL));
    let original = std::fs::read(&explicit).unwrap();

    let mut failures = Vec::new();
    for use_override in [false, true] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "configuration_tests::shell_preferences_ignore_external_configuration",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_ROOT, fixture.path())
            .env("LOCALAPPDATA", &data)
            .env("XDG_DATA_HOME", &data)
            .env("HOME", &home)
            .env("MOTION_LOCALE", "zh")
            .env_remove("MOTION_UI_LAYOUT");
        if use_override {
            child.env("MOTION_UI_LAYOUT", &explicit);
        }
        let output = child.output().unwrap();
        if !output.status.success() {
            failures.push(format!(
                "override={use_override} status={}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        assert_eq!(std::fs::read(&explicit).unwrap(), original);
        assert_eq!(std::fs::read(&implicit).unwrap(), original);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn check_shell_preferences(root: PathBuf) {
    let mut first = Shell::new(root.join("first-project"), None, None).unwrap();
    assert!(
        first.workspace.favorites.is_empty(),
        "loaded external favorites"
    );
    assert!(first.panels.browser.favorites.is_empty());
    assert!(first.workspace.timeline_column_blend);
    assert!(first.workspace.timeline_column_parent);
    assert!(first.workspace.timeline_column_matte);
    assert!(first.workspace.locale == Locale::En);
    assert!(first.panels.locale == Locale::En);
    let first_path = first.layout_path.clone();
    let first_directory = first_path.parent().unwrap().to_owned();
    assert!(
        first_directory.is_dir(),
        "temporary preferences outlived their owner"
    );
    assert!(!first_path.exists(), "fresh Shell reused a saved layout");

    first.workspace.favorites.insert(SENTINEL.into());
    first.workspace.save(&first_path).unwrap();
    assert!(workspace::Workspace::load(&first_path)
        .favorites
        .contains(SENTINEL));
    let second = Shell::new(root.join("second-project"), None, None).unwrap();
    let second_directory = second.layout_path.parent().unwrap().to_owned();
    assert_ne!(first_directory, second_directory);
    assert!(second.workspace.favorites.is_empty());
    assert!(second.workspace.locale == Locale::En);
    drop(first);
    assert!(
        !first_directory.exists(),
        "dropped Shell retained preferences"
    );
    assert!(
        second_directory.is_dir(),
        "one Shell removed another's preferences"
    );
    drop(second);
    assert!(!second_directory.exists());

    let explicit_locale = Shell::new(root.join("zh-project"), None, Some(Locale::Zh)).unwrap();
    assert!(explicit_locale.workspace.locale == Locale::Zh);
    assert!(explicit_locale.panels.locale == Locale::Zh);
}
