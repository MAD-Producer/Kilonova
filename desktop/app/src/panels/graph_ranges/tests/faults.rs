//! Exercise the real renderers, including their menu fit and cache boundaries.
use super::*;
use crate::panels::{curve_comparison, effect_timeline, key_selection, timeline_graph};
use std::{path::Path, sync::Arc};

const SITES: [&str; 12] = [
    "timeline.graph_view_bounds",
    "effect.graph_view_bounds",
    "curve.comparison.view_bounds",
    "timeline.graph_adaptive",
    "effect.graph_adaptive",
    "curve.comparison.adaptive",
    "timeline.graph_fit_all",
    "timeline.graph_fit_selection",
    "effect.graph_fit_all",
    "effect.graph_fit_selection",
    "curve.comparison.fit_all",
    "curve.comparison.fit_selection",
];

#[derive(Clone, Copy, Debug)]
enum Graph {
    Property,
    Effect,
    Comparison,
}
impl Graph {
    fn target(self) -> KeyframeTarget {
        if matches!(self, Self::Effect) {
            KeyframeTarget::Effect {
                object: 7,
                effect: 1,
                param: "speed".into(),
            }
        } else {
            KeyframeTarget::Property {
                object: 7,
                property: Property::Opacity,
                axis: None,
            }
        }
    }
    fn panels(self, state: &Value, catalogue: &Value) -> Panels {
        let mut p = effect_timeline::tests::setup(catalogue, Locale::En);
        p.timeline.set_graph_mode(true);
        match self {
            Self::Property => {
                p.animation_controls
                    .select_key_in(state, 7, Property::Opacity, None, 10)
            }
            Self::Effect => effect_timeline::project_key(&mut p, state, 7, 1, "speed", 10),
            Self::Comparison => {
                curve_comparison::tests::collect_targets(&mut p, state, &[self.target()]);
            }
        }
        p.selected_keys
            .apply(
                state,
                [10, 33009].map(|frame| key_selection::Key {
                    target: self.target(),
                    frame,
                }),
                key_selection::Mode::Replace,
            )
            .unwrap();
        p
    }
    fn render(
        self,
        p: &mut Panels,
        state: &Value,
        atlas: &mut TextAtlas,
        styles: &Arc<motion_ui::style::StyleSheet>,
        offset: f64,
        press: Option<[f32; 2]>,
    ) {
        let input = Input {
            mouse: press.unwrap_or([-1.; 2]),
            pressed: press.map(|point| (MouseButton::Right, point)),
            ..Default::default()
        };
        self.paint(p, state, atlas, styles, offset, 100., &input);
    }
    fn paint(
        self,
        p: &mut Panels,
        state: &Value,
        atlas: &mut TextAtlas,
        styles: &Arc<motion_ui::style::StyleSheet>,
        offset: f64,
        ppf: f32,
        input: &Input,
    ) -> PaintList {
        prepare(p, state, None, &Input::default());
        let mut list = PaintList::with_styles(styles.clone());
        let mut actions = vec![];
        let area = Rect::new(0., 100., 800., 400.);
        match self {
            Self::Property => timeline_graph::render(
                &mut list,
                atlas,
                area,
                p,
                state,
                input,
                offset,
                ppf,
                &mut actions,
            ),
            Self::Effect => assert!(effect_timeline::graph::render(
                &mut list,
                atlas,
                area,
                p,
                state,
                input,
                offset,
                ppf,
                &mut actions
            )),
            Self::Comparison => curve_comparison::render(
                &mut list,
                atlas,
                area,
                p,
                state,
                input,
                offset,
                ppf,
                &mut actions,
            ),
        }
        assert!(
            actions.is_empty(),
            "view operations must not submit edits: {self:?}"
        );
        list
    }
}

#[test]
fn effect_and_comparison_readout_overflow_reports_the_real_hover_without_poisoning_the_curve() {
    const CHILD: &str = "MOTION_CURVE_READOUT_FAULT_CHILD";
    const NAME: &str = "panels::graph_ranges::tests::faults::effect_and_comparison_readout_overflow_reports_the_real_hover_without_poisoning_the_curve";
    const SITES: [&str; 2] = ["effect.graph_readout", "curve.comparison.readout"];
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let (engine, catalogue) = effect_timeline::tests::fixture();
        let original = serde_json::to_vec(engine.project()).unwrap();
        let history = (engine.revision(), engine.can_undo(), engine.can_redo());
        // This deliberately degraded provider snapshot shortens the composition
        // after its authored keys. The visible viewport still extends past the
        // composition-capped sampling interval, so hover must check its own result.
        let mut good = effect_timeline::tests::state(&engine);
        good["project"]["frames"] = 1.into();
        let mut bad = good.clone();
        bad["project"]["fps"] = serde_json::json!(1e308);
        bad["revision"] = 91.into();
        let target = Graph::Effect.target();
        let mut effect = Graph::Effect.panels(&good, &catalogue);
        let mut comparison = effect_timeline::tests::setup(&catalogue, Locale::En);
        comparison.timeline.set_graph_mode(true);
        curve_comparison::tests::collect_targets(&mut comparison, &good, &[target]);
        effect.timeline.speed_graph = true;
        comparison.timeline.speed_graph = true;
        let styles = PaintList::default().styles();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let hover = Input {
            mouse: [30., 200.],
            ..Default::default()
        };
        let _logger = crate::logging::init(root.clone()).unwrap();
        let file = root.join("motion-studio.log");
        for (graph, p) in [
            (Graph::Effect, &mut effect),
            (Graph::Comparison, &mut comparison),
        ] {
            graph.paint(p, &good, &mut atlas, &styles, 0., 1., &hover);
            assert!(!p.timeline.graph_ranges.probes.is_empty());
        }
        crate::logging::flush();
        assert_eq!(std::fs::read_to_string(&file).unwrap().lines().count(), 1);
        for (index, (graph, p)) in [
            (Graph::Effect, &mut effect),
            (Graph::Comparison, &mut comparison),
        ]
        .into_iter()
        .enumerate()
        {
            for occurrence in 1..=120 {
                graph.paint(p, &bad, &mut atlas, &styles, 0., 1., &hover);
                assert!(
                    !p.timeline.graph_ranges.probes.is_empty(),
                    "curve remains available: {graph:?}"
                );
                graph.paint(p, &good, &mut atlas, &styles, 0., 1., &hover);
                assert!(!p.timeline.graph_ranges.probes.is_empty());
                if [1, 59, 60, 120].contains(&occurrence) {
                    assert_eq!(
                        records(&file, SITES[index]).len(),
                        if occurrence < 60 { 1 } else { 2 }
                    );
                }
            }
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for site in SITES {
            let lines = records(&file, site);
            assert_eq!(lines.len(), 2);
            assert!(lines[0].contains("WARN") && lines[0].contains("count=1 "));
            assert!(lines[1].contains("ERROR") && lines[1].contains("count=60 "));
            assert!(lines.iter().all(|line| line.contains("object=7")
                && line.contains("generation=Some(1)")
                && line.contains("revision=Some(91)")
                && line.contains("frame=30")
                && line.contains("phase=readout")
                && line.contains("not finite")));
        }
        assert_eq!(
            text.lines().count(),
            5,
            "only initialization and the two bounded sites: {text}"
        );
        assert_eq!(serde_json::to_vec(engine.project()).unwrap(), original);
        assert_eq!(
            (engine.revision(), engine.can_undo(), engine.can_redo()),
            history
        );
        std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "sites":SITES,"actualHoverConsumers":true,"normalCurveStaysAvailable":true,
            "firstWarnAndErrorAtLifetime60":true,"suppressedAfter60":true,
            "recoveryPreservesCounterScope":true,"noEditsOrHistory":true,
            "limits":"fps1e308 and shortened composition are intentionally degraded provider snapshots, not a legal saved project."
        })).unwrap()).unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CURVE_READOUT_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    for name in ["first-process", "reset-process"] {
        let child = root.join(name);
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env_remove("MOTION_LOG")
            .env(CHILD, &child)
            .output()
            .unwrap();
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("stdout.log"), &result.stdout).unwrap();
        std::fs::write(child.join("stderr.log"), &result.stderr).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let stderr = String::from_utf8_lossy(&result.stderr);
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert_eq!(stderr.lines().count(), 2, "{stderr}");
        for site in SITES {
            assert_eq!(
                stderr
                    .lines()
                    .filter(|line| line.contains(&format!("site={site} ")))
                    .count(),
                1
            );
        }
        assert!(
            !stderr.contains("WARN")
                && !stdout.contains("panel degradation")
                && !stdout.contains("ERROR")
        );
    }
}

fn records(path: &Path, site: &str) -> Vec<String> {
    crate::logging::flush();
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| line.contains(&format!("site={site} ")))
        .map(str::to_owned)
        .collect()
}

#[test]
fn real_renderers_report_bounds_adaptive_and_fit_budget_failures_at_default_level_and_recover() {
    const CHILD: &str = "MOTION_CURVE_RANGE_FAULT_CHILD";
    const NAME: &str = "panels::graph_ranges::tests::faults::real_renderers_report_bounds_adaptive_and_fit_budget_failures_at_default_level_and_recover";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        // Both tracks are validated by the actual Engine. No sampler callbacks,
        // private failure counters, or successful return values are substituted.
        let (fixture, catalogue) = effect_timeline::tests::fixture();
        let mut project = fixture.project().clone();
        project.frames = 36000;
        let layer = &mut project.layers[0];
        layer.timeline.as_mut().unwrap().out_frame = 36000;
        layer.transform.opacity.keys = (0..33000)
            .map(|frame| Keyframe {
                auto_bezier: false,
                frame,
                value: if frame % 2 == 0 { 0.2 } else { 0.8 },
                ease: motion_core::Ease::InOut,
                curve: None,
                spatial: None,
            })
            .collect();
        layer.effects[0].params.get_mut("speed").unwrap().track.keys = (0..33000)
            .map(|frame| Keyframe {
                auto_bezier: false,
                frame,
                value: [if frame % 2 == 0 { 0.2 } else { 0.8 }, 0., 0., 0.],
                ease: motion_core::Ease::InOut,
                curve: None,
                spatial: None,
            })
            .collect();
        let engine = Engine::new(project).unwrap();
        let state = effect_timeline::tests::state(&engine);
        let before = serde_json::to_vec(engine.project()).unwrap();
        let history = (engine.revision(), engine.can_undo(), engine.can_redo());
        let styles = PaintList::default().styles();
        // The stylesheet loader currently accepts this nonnegative token.
        // Test its actual presentation boundary, not a legal-project failure.
        let invalid_styles = Arc::new(
            styles
                .with_overrides(br#"{"tokens":{"sizes":{"graph.sample.error":0}}}"#)
                .unwrap(),
        );
        assert_eq!(invalid_styles.metric("graph.sample.error", 0.35), 0.);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        // Collection creation is an explicit command with an expected Info
        // start record. Finish setup before measuring passive render silence.
        let mut renderers = [Graph::Property, Graph::Effect, Graph::Comparison]
            .map(|graph| (graph, graph.panels(&state, &catalogue)));
        let _logger = crate::logging::init(root.clone()).unwrap();
        let file = root.join("motion-studio.log");
        for (graph, p) in &mut renderers {
            graph.render(p, &state, &mut atlas, &styles, 0., None);
            assert!(
                !p.timeline.graph_ranges.probes.is_empty(),
                "healthy renderer: {graph:?}"
            );
        }
        crate::logging::flush();
        let healthy = std::fs::read_to_string(&file).unwrap();
        assert_eq!(
            healthy.lines().count(),
            1,
            "healthy renderers must be quiet: {healthy}"
        );
        assert!(healthy.contains("logging initialized level=INFO"));

        for (graph, mut p) in renderers {
            let site = SITES[match graph {
                Graph::Property => 0,
                Graph::Effect => 1,
                Graph::Comparison => 2,
            }];
            for occurrence in 1..=120 {
                // Unrepresentable legacy Y offsets / external viewport offsets
                // are guarded presentation states, not normal navigation output.
                p.timeline.graph_y_offset = if matches!(graph, Graph::Comparison) {
                    0.
                } else {
                    f32::MAX
                };
                graph.render(
                    &mut p,
                    &state,
                    &mut atlas,
                    &styles,
                    if matches!(graph, Graph::Comparison) {
                        1e300
                    } else {
                        0.
                    },
                    None,
                );
                assert!(
                    p.timeline.graph_ranges.probes.is_empty(),
                    "bad bounds: {graph:?}"
                );
                p.timeline.graph_y_offset = 0.;
                graph.render(&mut p, &state, &mut atlas, &styles, 0., None);
                assert!(
                    !p.timeline.graph_ranges.probes.is_empty(),
                    "bounds recovery: {graph:?}"
                );
                if [1, 59, 60, 120].contains(&occurrence) {
                    assert_eq!(
                        records(&file, site).len(),
                        if occurrence < 60 { 1 } else { 2 }
                    );
                }
            }

            let site = SITES[match graph {
                Graph::Property => 3,
                Graph::Effect => 4,
                Graph::Comparison => 5,
            }];
            for occurrence in 1..=120 {
                graph.render(&mut p, &state, &mut atlas, &invalid_styles, 0., None);
                // Comparison retains its group/menu after a failed curve;
                // the other renderers return before their header is recorded.
                if !matches!(graph, Graph::Comparison) {
                    assert!(p.timeline.graph_ranges.probes.is_empty());
                }
                if matches!(graph, Graph::Comparison) {
                    // Same failing source+geometry is negatively cached.
                    graph.render(&mut p, &state, &mut atlas, &invalid_styles, 0., None);
                }
                graph.render(&mut p, &state, &mut atlas, &styles, 0., None);
                assert!(!p.timeline.graph_ranges.probes.is_empty());
                if [1, 59, 60, 120].contains(&occurrence) {
                    assert_eq!(
                        records(&file, site).len(),
                        if occurrence < 60 { 1 } else { 2 }
                    );
                }
            }

            graph.render(&mut p, &state, &mut atlas, &styles, 0., None);
            let header = p.timeline.graph_ranges.probes[0].header;
            let point = [
                header.min[0] + header.width() * 0.5,
                header.min[1] + header.height() * 0.5,
            ];
            for occurrence in 1..=120 {
                graph.render(&mut p, &state, &mut atlas, &styles, 0., Some(point));
                let menu = p
                    .timeline
                    .graph_ranges
                    .menu
                    .as_ref()
                    .expect("real renderer menu");
                assert!(
                    menu.options.all.is_none() && menu.options.selection.is_none(),
                    "whole-track/selected-interval budget must not return a partial fit: {graph:?}"
                );
                p.timeline.graph_ranges.cancel();
                graph.render(&mut p, &state, &mut atlas, &styles, 0., None);
                assert!(!p.timeline.graph_ranges.probes.is_empty());
                let index = match graph {
                    Graph::Property => 6,
                    Graph::Effect => 8,
                    Graph::Comparison => 10,
                };
                if [1, 59, 60, 120].contains(&occurrence) {
                    for site in &SITES[index..index + 2] {
                        assert_eq!(
                            records(&file, site).len(),
                            if occurrence < 60 { 1 } else { 2 }
                        );
                    }
                }
            }
            // Narrowing the actual selection to one key restores both the
            // available selected fit and its real renderer menu.
            p.selected_keys
                .apply(
                    &state,
                    [key_selection::Key {
                        target: graph.target(),
                        frame: 10,
                    }],
                    key_selection::Mode::Replace,
                )
                .unwrap();
            graph.render(&mut p, &state, &mut atlas, &styles, 0., Some(point));
            let menu = p.timeline.graph_ranges.menu.as_ref().unwrap();
            assert!(menu.options.all.is_none() && menu.options.selection.is_some());
        }
        for (index, site) in SITES.iter().enumerate() {
            let lines = records(&file, site);
            assert_eq!(lines.len(), 2, "{site}: {lines:?}");
            assert!(lines[0].contains("WARN") && lines[0].contains("count=1 "));
            assert!(lines[1].contains("ERROR") && lines[1].contains("count=60 "));
            let cause = if index < 3 {
                "unrepresentable"
            } else if index < 6 {
                "graph sampling geometry is invalid"
            } else {
                "graph sampling budget exhausted"
            };
            assert!(lines
                .iter()
                .all(|line| line.contains(cause) && line.contains("phase=")));
            assert!(lines.iter().all(
                |line| line.contains("generation=Some(1)") && line.contains("revision=Some(0)")
            ));
            if !site.starts_with("curve.comparison") {
                assert!(lines.iter().all(|line| line.contains("object=7")));
            }
        }
        assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
        assert_eq!(
            (engine.revision(), engine.can_undo(), engine.can_redo()),
            history
        );
        std::fs::write(root.join("result.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "sites":SITES, "actualRenderers":true, "validatedKeysPerTrack":33000,
            "firstWarnAndErrorAtLifetime60":true,"suppressedAfter60":true,
            "healthyRecoveryPreservesCounterScope":true,"comparisonNegativeCache":true,
            "selectedFitRecoversOnShorterInterval":true,"noEditsOrHistory":true,
            "limits":"Invalid style tolerance and unrepresentable offsets are presentation boundaries, not legal stored project faults. Budget failures use Engine-validated project tracks. Other readout/Mini guard sites are not covered."
        })).unwrap()).unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CURVE_RANGE_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    for run in ["first-process", "reset-process"] {
        let child_root = root.join(run);
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env_remove("MOTION_LOG")
            .env(CHILD, &child_root)
            .output()
            .unwrap();
        std::fs::create_dir_all(&child_root).unwrap();
        std::fs::write(child_root.join("stdout.log"), &result.stdout).unwrap();
        std::fs::write(child_root.join("stderr.log"), &result.stderr).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let stderr = String::from_utf8_lossy(&result.stderr);
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert_eq!(stderr.lines().count(), SITES.len(), "{stderr}");
        for site in SITES {
            assert_eq!(
                stderr
                    .lines()
                    .filter(|line| line.contains(&format!("site={site} ")))
                    .count(),
                1,
                "{stderr}"
            );
        }
        assert!(
            !stderr.contains("WARN")
                && !stdout.contains("panel degradation")
                && !stdout.contains("ERROR")
        );
    }
}
