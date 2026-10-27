use super::*;
use crate::render_queue::{controller::Presentation, model::Job, worker::View};
use std::sync::Arc;

#[cfg(target_os = "windows")]
mod native;

fn input(rect: Rect) -> Input {
    let mouse = [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ];
    Input {
        mouse,
        pressed: Some((MouseButton::Left, mouse)),
        ..Default::default()
    }
}
fn state() -> Value {
    serde_json::json!({"root":"queue-panel","composition":"comp-main","revision":7,"document_generation":1,
        "project":{"width":32,"height":24,"fps":30,"frames":2,"layers":[]}})
}
fn panels(locale: Locale) -> Panels {
    let mut document = Document::default();
    document.jobs.push(Job {
        id: 1,
        snapshot: "movie-panel".into(),
        source: motion_desktop_media::movie::durable::SnapshotInfo {
            composition: "comp-main".into(),
            name: "Frozen composition".into(),
            width: 32,
            height: 24,
            fps: 30,
            frames: 2,
            range: motion_core::WorkArea::full(2),
            revision: 7,
        },
        template: document.templates[0].clone(),
        output: std::env::temp_dir().join("panel-frozen.mp4"),
        attempt: 0,
        state: JobState::Queued,
    });
    document.next_id = 2;
    let mut panels = Panels::default();
    panels.locale = locale;
    panels.render_queue = Presentation {
        requested: true,
        view: Some(View {
            ready: true,
            running: false,
            stopped: false,
            document: Arc::new(document),
            flight: None,
            active_job: None,
            active_request: None,
            progress: 0.,
            error: None,
        }),
        ..Default::default()
    };
    panels
}
fn draw(
    panels: &mut Panels,
    atlas: &mut TextAtlas,
    input: &Input,
    floating: bool,
    size: [f32; 2],
) -> Vec<Action> {
    let mut paint = PaintList::default();
    if floating {
        crate::panels::floating_chrome(
            &mut paint,
            atlas,
            Panel::RenderQueue,
            size,
            panels,
            &state(),
            input,
        )
    } else {
        crate::panels::chrome(
            &mut paint,
            atlas,
            size,
            &dock::Node::dock(Panel::RenderQueue.dock()),
            panels,
            &state(),
            input,
        )
    }
}

#[test]
fn render_queue_actions_use_the_same_task_identity_in_both_windows_and_languages() {
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let mut panels = panels(locale);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            draw(
                &mut panels,
                &mut atlas,
                &Input::default(),
                floating,
                [600., 700.],
            );
            let rect = panels
                .auxiliary
                .render_queue
                .actions
                .iter()
                .find(|(name, _)| *name == "cancel")
                .unwrap()
                .1;
            let actions = draw(
                &mut panels,
                &mut atlas,
                &input(rect),
                floating,
                [600., 700.],
            );
            assert!(
                matches!(actions.as_slice(), [Action::RenderQueue(Intent::Cancel(1))]),
                "{actions:?}"
            );
            let rect = panels
                .auxiliary
                .render_queue
                .actions
                .iter()
                .find(|(name, _)| *name == "start")
                .unwrap()
                .1;
            let actions = draw(
                &mut panels,
                &mut atlas,
                &input(rect),
                floating,
                [600., 700.],
            );
            if cfg!(feature = "ffmpeg") {
                assert!(
                    matches!(actions.as_slice(), [Action::RenderQueue(Intent::Start)]),
                    "{actions:?}"
                );
            } else {
                assert!(
                    actions.is_empty(),
                    "unsupported rendering must stay disabled"
                );
            }
            let document = &mut panels.render_queue.view.as_mut().unwrap().document;
            Arc::make_mut(document).jobs[0].state = JobState::Failed {
                cause: "original IO failure".into(),
            };
            draw(
                &mut panels,
                &mut atlas,
                &Input::default(),
                floating,
                [600., 700.],
            );
            let rect = panels
                .auxiliary
                .render_queue
                .actions
                .iter()
                .find(|(name, _)| *name == "retry")
                .unwrap()
                .1;
            let actions = draw(
                &mut panels,
                &mut atlas,
                &input(rect),
                floating,
                [600., 700.],
            );
            assert!(
                matches!(actions.as_slice(), [Action::RenderQueue(Intent::Retry(1))]),
                "{actions:?}"
            );
        }
    }
}

#[test]
fn render_queue_template_text_owns_ime_cancel_and_passive_rows_without_touching_tasks() {
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let mut panels = panels(locale);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let original = panels.render_queue.view.as_ref().unwrap().document.clone();
            panels.auxiliary.render_queue.draft = Some(original.templates[0].clone());
            draw(
                &mut panels,
                &mut atlas,
                &Input::default(),
                floating,
                [600., 900.],
            );
            let rect = panels.auxiliary.render_queue.fields[0];
            assert!(draw(
                &mut panels,
                &mut atlas,
                &input(rect),
                floating,
                [600., 900.]
            )
            .is_empty());
            assert_eq!(panels.auxiliary.owner(), Some(Panel::RenderQueue));
            let actions = draw(
                &mut panels,
                &mut atlas,
                &Input {
                    text: "自己的模板".into(),
                    ..Default::default()
                },
                floating,
                [600., 900.],
            );
            assert!(actions.is_empty());
            let enter = Input {
                keys: vec![(Key::Enter, Default::default())],
                composing: true,
                ..Default::default()
            };
            assert!(draw(&mut panels, &mut atlas, &enter, floating, [600., 900.]).is_empty());
            assert!(panels.auxiliary.render_queue.editor.active());
            let enter = Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            };
            assert!(draw(&mut panels, &mut atlas, &enter, floating, [600., 900.]).is_empty());
            assert_eq!(
                panels.auxiliary.render_queue.draft.as_ref().unwrap().name,
                "自己的模板"
            );
            assert!(!panels.auxiliary.editing());
            assert_eq!(
                panels.render_queue.view.as_ref().unwrap().document.as_ref(),
                original.as_ref()
            );
            let rect = panels.auxiliary.render_queue.fields[1];
            draw(
                &mut panels,
                &mut atlas,
                &input(rect),
                floating,
                [600., 900.],
            );
            draw(
                &mut panels,
                &mut atlas,
                &Input {
                    focus_lost: true,
                    ..Default::default()
                },
                floating,
                [600., 900.],
            );
            assert!(!panels.auxiliary.editing());
            assert!(panels.auxiliary.render_queue.draft.is_some());
        }
    }
}

#[test]
fn render_queue_can_create_a_valid_template_after_deleting_the_entire_library() {
    let mut panels = panels(Locale::Zh);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    Arc::make_mut(&mut panels.render_queue.view.as_mut().unwrap().document)
        .templates
        .clear();
    let original = panels.render_queue.view.as_ref().unwrap().document.clone();
    draw(
        &mut panels,
        &mut atlas,
        &Input::default(),
        false,
        [600., 900.],
    );
    let rect = panels
        .auxiliary
        .render_queue
        .actions
        .iter()
        .find(|(name, _)| *name == "new_template")
        .unwrap()
        .1;
    assert!(draw(&mut panels, &mut atlas, &input(rect), false, [600., 900.]).is_empty());
    draw(
        &mut panels,
        &mut atlas,
        &Input::default(),
        false,
        [600., 900.],
    );
    let rect = panels
        .auxiliary
        .render_queue
        .actions
        .iter()
        .find(|(name, _)| *name == "save_template")
        .unwrap()
        .1;
    let actions = draw(&mut panels, &mut atlas, &input(rect), false, [600., 900.]);
    let [Action::RenderQueue(Intent::SaveTemplate(template))] = actions.as_slice() else {
        panic!("{actions:?}");
    };
    assert_eq!(template.id, 0);
    assert_eq!(template.pattern, "{composition}-{id}.mp4");
    let mut valid = template.clone();
    valid.id = original.next_template;
    valid.validate().unwrap();
    assert_eq!(
        panels.render_queue.view.as_ref().unwrap().document.as_ref(),
        original.as_ref()
    );
    panels
        .auxiliary
        .render_queue
        .draft
        .as_mut()
        .unwrap()
        .pattern = "../outside.mp4".into();
    assert!(draw(&mut panels, &mut atlas, &input(rect), false, [600., 900.]).is_empty());
    assert!(panels.auxiliary.render_queue.error.is_some());
}

#[test]
fn stopped_queue_can_be_explicitly_reloaded_but_active_work_cannot() {
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let mut panels = panels(locale);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let view = panels.render_queue.view.as_mut().unwrap();
            view.ready = false;
            view.stopped = true;
            view.error = Some("parse render queue: original cause".into());
            for (stopped, pending, expected) in [
                (true, false, true),
                (false, false, false),
                (true, true, false),
            ] {
                let view = panels.render_queue.view.as_mut().unwrap();
                view.stopped = stopped;
                view.ready = !stopped;
                view.active_job = Some(1);
                panels.render_queue.pending = pending;
                draw(
                    &mut panels,
                    &mut atlas,
                    &Input::default(),
                    floating,
                    [600., 900.],
                );
                let rect = panels
                    .auxiliary
                    .render_queue
                    .actions
                    .iter()
                    .find(|(name, _)| *name == "reload")
                    .unwrap()
                    .1;
                let actions = draw(
                    &mut panels,
                    &mut atlas,
                    &input(rect),
                    floating,
                    [600., 900.],
                );
                assert_eq!(
                    matches!(actions.as_slice(), [Action::RenderQueue(Intent::Reload)]),
                    expected,
                    "{actions:?}"
                );
                assert_eq!(
                    panels.render_queue.view.as_ref().unwrap().error.as_deref(),
                    Some("parse render queue: original cause")
                );
            }
        }
    }
}

#[test]
#[cfg(feature = "ffmpeg")]
fn frozen_jobs_can_start_after_live_composition_dimensions_change() {
    for locale in [Locale::Zh, Locale::En] {
        for dimension in ["width", "height"] {
            let mut panels = panels(locale);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let mut project = state();
            project["project"][dimension] = 33.into();
            let mut paint = PaintList::default();
            let mut actions = Vec::new();
            render(
                &mut paint,
                &mut atlas,
                Rect::new(0., 0., 600., 900.),
                &mut panels,
                &project,
                &Input::default(),
                &mut actions,
            );
            for (name, expected) in [("start", true), ("add", false)] {
                let rect = panels
                    .auxiliary
                    .render_queue
                    .actions
                    .iter()
                    .find(|(action, _)| *action == name)
                    .unwrap()
                    .1;
                actions.clear();
                render(
                    &mut paint,
                    &mut atlas,
                    Rect::new(0., 0., 600., 900.),
                    &mut panels,
                    &project,
                    &input(rect),
                    &mut actions,
                );
                assert_eq!(
                    matches!(actions.as_slice(), [Action::RenderQueue(Intent::Start)]),
                    expected,
                    "{dimension}: {actions:?}"
                );
                if !expected {
                    assert!(actions.is_empty());
                }
            }
        }
    }
}

#[test]
fn passive_queue_paint_does_not_dispatch_a_file_io_intent() {
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let mut panels = Panels::default();
            panels.locale = locale;
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            assert!(draw(
                &mut panels,
                &mut atlas,
                &Input::default(),
                floating,
                [600., 900.],
            )
            .is_empty());
            assert!(!panels.render_queue.requested);
        }
    }
}
