use super::*;
use crate::engine;
use motion_ui::input::Modifiers;
use std::time::Duration;
#[cfg(target_os = "windows")]
mod native;

fn fixture() -> (tempfile::TempDir, engine::Engine, Value, ExpressionTarget) {
    let root = tempfile::tempdir().unwrap();
    let mut project = motion_core::Project::new(640, 360, 30, 120).unwrap();
    project.layers.push(motion_core::Layer::solid(
        7,
        "Expression study",
        [120.; 2],
        [0.; 3],
        [1.; 4],
    ));
    motion_core::storage::save(root.path(), &project).unwrap();
    let host = engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let state = host.state().unwrap();
    let target = ExpressionTarget::Property {
        object: 7,
        property: Property::Position,
        axis: None,
    };
    (root, host, state, target)
}
fn key(key: Key, control: bool) -> Input {
    Input {
        keys: vec![(
            key,
            Modifiers {
                control,
                ..Default::default()
            },
        )],
        ..Default::default()
    }
}
fn click(area: Rect) -> Input {
    let p = [
        (area.min[0] + area.max[0]) * 0.5,
        (area.min[1] + area.max[1]) * 0.5,
    ];
    Input {
        mouse: p,
        pressed: Some((MouseButton::Left, p)),
        ..Default::default()
    }
}
fn draw(
    panels: &mut Panels,
    state: &Value,
    atlas: &mut TextAtlas,
    input: Input,
    floating: bool,
) -> Vec<Action> {
    if floating {
        floating_chrome(
            &mut PaintList::default(),
            atlas,
            Panel::Timeline,
            [900., 600.],
            panels,
            state,
            &input,
        )
    } else {
        chrome(
            &mut PaintList::default(),
            atlas,
            [900., 600.],
            &dock::Node::dock(Panel::Timeline.dock()),
            panels,
            state,
            &input,
        )
    }
}
fn replace(panels: &mut Panels, state: &Value, atlas: &mut TextAtlas, text: &str, floating: bool) {
    assert!(draw(panels, state, atlas, key(Key::A, true), floating).is_empty());
    assert!(draw(
        panels,
        state,
        atlas,
        Input {
            text: text.into(),
            ..Default::default()
        },
        floating
    )
    .is_empty());
}
fn execute(host: &engine::Engine, actions: Vec<Action>) -> (Ticket, Result<Value, String>) {
    assert_eq!(actions.len(), 1, "{actions:?}");
    match actions.into_iter().next().unwrap() {
        Action::ExpressionApply {
            ticket,
            source,
            expression,
            frame,
        } => (
            ticket,
            host.call(|reply| engine::Command::EditorAction {
                source,
                commands: vec![Command::SetExpression { expression, frame }],
                reply,
            }),
        ),
        Action::ExpressionCheck {
            ticket,
            target,
            text,
        } => (
            ticket,
            host.call(|reply| engine::Command::ExpressionCheck {
                target,
                source: text,
                reply,
            }),
        ),
        a => panic!("unexpected action {a:?}"),
    }
}
#[test]
fn expanded_editor_routes_both_windows_and_locales_and_persists_one_transaction() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            let (root, host, state, target) = fixture();
            let mut panels = Panels {
                selected: Some(7),
                focused: Some(Panel::Timeline),
                locale,
                ..Default::default()
            };
            panels.timeline.expanded.insert(7);
            panels
                .expressions
                .open(&state, target.clone(), Panel::Timeline);
            panels
                .expressions
                .open_expanded(&state, target.clone(), Panel::Timeline);
            assert!(draw(&mut panels, &state, &mut atlas, Input::default(), floating).is_empty());
            let snippets = panels.expressions.expanded.as_ref().unwrap().areas.unwrap()[2];
            assert!(draw(&mut panels, &state, &mut atlas, click(snippets), floating).is_empty());
            assert!(draw(
                &mut panels,
                &state,
                &mut atlas,
                key(Key::Escape, false),
                floating
            )
            .is_empty());
            assert!(
                panels.expressions.expanded.is_some(),
                "Escape must first close the snippet popup"
            );
            assert!(!panels.expressions.expanded.as_ref().unwrap().snippets.open);
            let source = "const offset = [30, 40, 50];\nvalue + offset";
            replace(&mut panels, &state, &mut atlas, source, floating);
            assert!(draw(
                &mut panels,
                &state,
                &mut atlas,
                key(Key::Enter, false),
                floating
            )
            .is_empty());
            assert_eq!(
                panels.expressions.expanded.as_ref().unwrap().source.text,
                format!("{source}\n")
            );
            let seed_area = panels.expressions.expanded.as_ref().unwrap().areas.unwrap()[1];
            assert!(draw(&mut panels, &state, &mut atlas, click(seed_area), floating).is_empty());
            assert!(draw(
                &mut panels,
                &state,
                &mut atlas,
                Input {
                    text: "42".into(),
                    ..Default::default()
                },
                floating
            )
            .is_empty());
            assert!(draw(&mut panels, &state, &mut atlas, click(snippets), floating).is_empty());
            assert!(draw(
                &mut panels,
                &state,
                &mut atlas,
                key(Key::Escape, false),
                floating
            )
            .is_empty());
            assert!(
                panels.expressions.expanded.as_ref().unwrap().seed.active(),
                "closing snippets must preserve the seed field's input session"
            );
            assert!(draw(
                &mut panels,
                &state,
                &mut atlas,
                key(Key::Enter, false),
                floating
            )
            .is_empty());
            let mut ime = key(Key::Enter, true);
            ime.ime_consumed = true;
            assert!(draw(&mut panels, &state, &mut atlas, ime, floating).is_empty());
            let actions = draw(
                &mut panels,
                &state,
                &mut atlas,
                key(Key::Enter, true),
                floating,
            );
            // New typing while a transaction is queued must survive its reply.
            assert!(draw(
                &mut panels,
                &state,
                &mut atlas,
                Input {
                    text: "// next draft".into(),
                    ..Default::default()
                },
                floating
            )
            .is_empty());
            let (ticket, result) = execute(&host, actions);
            let applied = result.as_ref().unwrap().clone();
            panels
                .expressions
                .expression_response(&ticket, &result, locale);
            assert!(panels
                .expressions
                .expanded
                .as_ref()
                .unwrap()
                .source
                .text
                .ends_with("// next draft"));
            assert_eq!(
                applied["project"]["expressions"][0]["source"],
                format!("{source}\n")
            );
            assert_eq!(applied["project"]["expressions"][0]["seed"], 42);
            assert_eq!(
                applied["sampledLayers"][0]["position"],
                serde_json::json!([30., 40., 50.])
            );
            assert_eq!(
                applied["project"]["layers"][0]["name"],
                state["project"]["layers"][0]["name"]
            );
            assert!(draw(
                &mut panels,
                &applied,
                &mut atlas,
                key(Key::Escape, false),
                floating
            )
            .is_empty());
            assert!(panels.expressions.expanded.is_none());
            host.save().unwrap();
            let reopened = motion_core::storage::load(root.path()).unwrap();
            assert_eq!(reopened.expressions[0].source, format!("{source}\n"));
            assert_eq!(reopened.expressions[0].seed, 42);
            assert_eq!(host.history(0).unwrap()["project"], state["project"]);
            assert_eq!(host.history(1).unwrap()["project"], applied["project"]);
        }
    }
}
#[test]
fn syntax_location_runtime_failure_stale_apply_and_old_replies_preserve_the_draft() {
    let (_root, host, state, target) = fixture();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut panels = Panels {
        selected: Some(7),
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    panels
        .expressions
        .open_expanded(&state, target.clone(), Panel::Timeline);
    draw(&mut panels, &state, &mut atlas, Input::default(), false);
    let source = "const 名称 = 1;\nvalue + )";
    replace(&mut panels, &state, &mut atlas, source, false);
    let check = panels.expressions.expanded.as_ref().unwrap().areas.unwrap()[3];
    let actions = draw(&mut panels, &state, &mut atlas, click(check), false);
    let (ticket, result) = execute(&host, actions);
    assert_eq!(
        result.as_ref().unwrap()["diagnostic"]["byte_offset"],
        source.find(')').unwrap()
    );
    panels
        .expressions
        .expression_response(&ticket, &result, Locale::En);
    let editor = panels.expressions.expanded.as_ref().unwrap();
    assert!(editor.diagnostic.is_some());
    assert_eq!(host.state().unwrap()["revision"], state["revision"]);
    let areas = editor.areas.unwrap();
    draw(&mut panels, &state, &mut atlas, click(areas[1]), false);
    let diagnostic = Rect::new(
        areas[0].min[0],
        areas[4].min[1] - 22.,
        areas[0].width(),
        22.,
    );
    draw(&mut panels, &state, &mut atlas, click(diagnostic), false);
    assert_eq!(
        panels
            .expressions
            .expanded
            .as_ref()
            .unwrap()
            .source
            .caret_byte_index(),
        source.find(')').unwrap()
    );
    assert!(!panels.expressions.expanded.as_ref().unwrap().seed.active());
    replace(
        &mut panels,
        &state,
        &mut atlas,
        "throw new Error('expression-test-runtime')",
        false,
    );
    let (ticket, result) = execute(
        &host,
        draw(
            &mut panels,
            &state,
            &mut atlas,
            key(Key::Enter, true),
            false,
        ),
    );
    assert!(result
        .as_ref()
        .unwrap_err()
        .contains("expression-test-runtime"));
    panels
        .expressions
        .expression_response(&ticket, &result, Locale::En);
    assert!(panels
        .expressions
        .expanded
        .as_ref()
        .unwrap()
        .source
        .text
        .contains("expression-test-runtime"));
    assert_eq!(host.state().unwrap()["project"], state["project"]);
    replace(&mut panels, &state, &mut atlas, "value + [1, 2, 3]", false);
    let actions = draw(
        &mut panels,
        &state,
        &mut atlas,
        key(Key::Enter, true),
        false,
    );
    let changed = host
        .edit(
            serde_json::to_string(&Command::Rename {
                object: 7,
                name: "Changed outside".into(),
            })
            .unwrap(),
        )
        .unwrap();
    let (ticket, result) = execute(&host, actions);
    assert_eq!(result.as_ref().unwrap()["editorAction"]["cancelled"], true);
    panels
        .expressions
        .expression_response(&ticket, &result, Locale::En);
    assert_eq!(host.state().unwrap()["project"], changed["project"]);
    let mut other = changed.clone();
    other["document_generation"] =
        serde_json::json!(changed["document_generation"].as_u64().unwrap() + 1);
    assert!(draw(
        &mut panels,
        &other,
        &mut atlas,
        key(Key::Enter, true),
        false
    )
    .is_empty());
    assert_eq!(
        panels.expressions.expanded.as_ref().unwrap().source.text,
        "value + [1, 2, 3]"
    );
    // A new editor's ticket cannot be mistaken for the old request.
    panels
        .expressions
        .open_expanded(&changed, target, Panel::Timeline);
    panels
        .expressions
        .expression_response(&ticket, &Err("obsolete failure".into()), Locale::En);
    assert!(panels
        .expressions
        .expanded
        .as_ref()
        .unwrap()
        .message
        .is_none());
}
#[test]
fn blocked_worker_keeps_check_and_apply_polling_nonblocking_and_reports_real_failures() {
    const CHILD: &str = "MOTION_EXPRESSION_EDITOR_OBSERVATION_CHILD";
    const NAME:&str="panels::expressions::expanded::tests::blocked_worker_keeps_check_and_apply_polling_nonblocking_and_reports_real_failures";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_EXPRESSION_ERROR_ARTIFACTS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
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
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stdout.contains(" ERROR ") && !stdout.contains(" INFO "),
            "{stdout}"
        );
        assert!(stderr.contains("expression-test-runtime"), "{stderr}");
        return;
    };
    let root = std::path::PathBuf::from(root);
    let _logging = crate::logging::init(root.join("logs")).unwrap();
    let (project_root, host, _state, target) = fixture();
    drop(host);
    let mut shell = crate::Shell::new(project_root.path().into(), None, Some(Locale::En)).unwrap();
    let state = shell.state.clone();
    shell
        .panels
        .expressions
        .open_expanded(&state, target, Panel::Timeline);
    shell.panels.focused = Some(Panel::Timeline);
    shell.panels.selected = Some(7);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    draw(
        &mut shell.panels,
        &state,
        &mut atlas,
        Input::default(),
        false,
    );
    replace(&mut shell.panels, &state, &mut atlas, "value + )", false);
    let (entered, entry) = std::sync::mpsc::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let held = shell
        .engine
        .request(|reply| engine::Command::TestWait {
            entered,
            release: gate,
            reply,
        })
        .unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    let check = shell
        .panels
        .expressions
        .expanded
        .as_ref()
        .unwrap()
        .areas
        .unwrap()[3];
    let mut actions = draw(&mut shell.panels, &state, &mut atlas, click(check), false);
    shell.action(actions.pop().unwrap(), None).unwrap();
    shell.poll_expression_task();
    assert!(shell.expression_task.is_some());
    replace(
        &mut shell.panels,
        &state,
        &mut atlas,
        "value + [1, 2, 3]",
        false,
    );
    assert_eq!(
        shell.engine.native_wait_snapshot()["in_flight"]["operation"],
        "test_wait"
    );
    release.send(()).unwrap();
    held.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while shell.expression_task.is_some() {
        assert!(std::time::Instant::now() < deadline);
        shell.poll_expression_task();
        std::thread::yield_now();
    }
    assert!(shell
        .panels
        .expressions
        .expanded
        .as_ref()
        .unwrap()
        .diagnostic
        .is_none());
    assert_eq!(
        shell
            .panels
            .expressions
            .expanded
            .as_ref()
            .unwrap()
            .source
            .text,
        "value + [1, 2, 3]"
    );
    replace(
        &mut shell.panels,
        &state,
        &mut atlas,
        "throw new Error('expression-test-runtime')",
        false,
    );
    let mut actions = draw(
        &mut shell.panels,
        &state,
        &mut atlas,
        key(Key::Enter, true),
        false,
    );
    shell.action(actions.pop().unwrap(), None).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while shell.expression_task.is_some() {
        assert!(std::time::Instant::now() < deadline);
        shell.poll_expression_task();
        std::thread::yield_now();
    }
    assert!(
        shell
            .panels
            .expressions
            .expanded
            .as_ref()
            .unwrap()
            .message
            .as_ref()
            .unwrap()
            .0
            .contains("expression-test-runtime"),
        "{:?}",
        shell.panels.expressions.expanded.as_ref().unwrap().message
    );
    replace(
        &mut shell.panels,
        &state,
        &mut atlas,
        "while (true) {}",
        false,
    );
    let mut actions = draw(
        &mut shell.panels,
        &state,
        &mut atlas,
        key(Key::Enter, true),
        false,
    );
    shell.action(actions.pop().unwrap(), None).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while shell.expression_task.is_some() {
        assert!(std::time::Instant::now() < deadline);
        shell.poll_expression_task();
        std::thread::yield_now();
    }
    assert!(shell
        .panels
        .expressions
        .expanded
        .as_ref()
        .unwrap()
        .message
        .as_ref()
        .unwrap()
        .0
        .contains("budget exceeded"));
    assert_eq!(shell.engine.state().unwrap()["project"], state["project"]);
    replace(
        &mut shell.panels,
        &state,
        &mut atlas,
        "value + [1, 2, 3]",
        false,
    );
    let mut actions = draw(
        &mut shell.panels,
        &state,
        &mut atlas,
        key(Key::Enter, true),
        false,
    );
    shell.action(actions.pop().unwrap(), None).unwrap();
    draw(
        &mut shell.panels,
        &state,
        &mut atlas,
        key(Key::Escape, false),
        false,
    );
    assert!(shell.panels.expressions.expanded.is_none());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while shell.expression_task.is_some() {
        assert!(std::time::Instant::now() < deadline);
        shell.poll_expression_task();
        std::thread::yield_now();
    }
    assert_eq!(
        shell.state["project"]["expressions"][0]["source"],
        "value + [1, 2, 3]"
    );
    assert_eq!(
        shell.engine.history(0).unwrap()["project"],
        state["project"]
    );
    crate::logging::flush();
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    assert!(
        file.lines().any(|l| l.contains(" INFO ")
            && l.contains("start #")
            && l.ends_with(" expression_check")),
        "{file}"
    );
    assert!(
        file.lines().any(|l| l.contains(" ERROR ")
            && l.contains("editor_action")
            && l.contains("expression-test-runtime")),
        "{file}"
    );
    assert!(
        !file
            .lines()
            .any(|l| l.contains(" ERROR ") && l.contains("expression_check")),
        "syntax feedback is a normal result: {file}"
    );
    assert!(
        file.lines().any(|l| l.contains(" ERROR ")
            && l.contains("editor_action")
            && l.contains("budget exceeded")),
        "{file}"
    );
}
