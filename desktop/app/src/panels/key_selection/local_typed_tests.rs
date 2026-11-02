//! Typed timeline lanes keep the same authored addresses as their summaries.
use super::*;
use motion_core::{
    masks::{LayerMask, MaskProperty},
    vector::{groups::VectorGroup, PathNode, ShapeKind, VectorContent, VectorPath, VectorSource},
    AudioAsset, AudioChannel, AudioClip, AudioLevels, Content, EffectInstance, Track, Tween,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Case {
    Masks,
    FlatVector,
    GroupVector,
    Effects,
    Audio,
}
pub(super) const CASES: [Case; 5] = [
    Case::Masks,
    Case::FlatVector,
    Case::GroupVector,
    Case::Effects,
    Case::Audio,
];
fn seed<T: Tween>(track: &mut Track<T>) {
    for frame in [3, 13] {
        track
            .upsert(frame, track.value.clone(), Ease::Linear)
            .unwrap();
    }
}
pub(super) fn typed(case: Case, reverse: bool) -> (Engine, Value, Vec<KeyframeTarget>) {
    let mut p = mapped(reverse).snapshot();
    p.layers.retain(|l| l.id == 7);
    p.layers[0].transform.position.keys.clear();
    let mut targets = Vec::new();
    let mut catalogue = json!({});
    match case {
        Case::Masks => {
            let nodes = [[0., 0.], [40., 0.], [0., 40.]]
                .into_iter()
                .enumerate()
                .map(|(i, v)| {
                    let mut geometry = Track::constant([v[0], v[1], 0., 0., 0., 0.]);
                    seed(&mut geometry);
                    PathNode {
                        id: i as u64 + 1,
                        geometry,
                    }
                })
                .collect();
            let mut mask = LayerMask::new(
                1,
                VectorPath {
                    id: 1,
                    closed: true,
                    nodes,
                    tangents: Default::default(),
                },
            );
            seed(&mut mask.opacity);
            seed(&mut mask.feather);
            seed(&mut mask.expansion);
            targets = [
                MaskProperty::Opacity,
                MaskProperty::Feather,
                MaskProperty::Expansion,
                MaskProperty::Node(1),
                MaskProperty::Node(2),
                MaskProperty::Node(3),
            ]
            .into_iter()
            .map(|property| KeyframeTarget::Mask {
                object: 7,
                mask: 1,
                property,
            })
            .collect();
            p.layers[0].masks.push(mask);
        }
        Case::FlatVector | Case::GroupVector => {
            let mut vector = VectorContent::shape(ShapeKind::Rectangle);
            seed(vector.fill.as_mut().unwrap());
            let VectorSource::Shape { parameters, .. } = &mut vector.source else {
                panic!()
            };
            seed(parameters.get_mut("angle").unwrap());
            let grouped = matches!(case, Case::GroupVector);
            targets = ["fill", "shape:angle"]
                .into_iter()
                .map(|parameter| KeyframeTarget::Vector {
                    object: 7,
                    item: grouped.then_some(2),
                    parameter: parameter.into(),
                })
                .collect();
            if grouped {
                vector = VectorGroup::wrap(vector, [64.; 2]);
                let VectorSource::Group { group } = &mut vector.source else {
                    panic!()
                };
                seed(&mut group.transform.position);
                targets.push(KeyframeTarget::Vector {
                    object: 7,
                    item: Some(1),
                    parameter: "position".into(),
                });
            }
            p.layers[0].content = Content::Vector { vector };
        }
        Case::Effects => {
            let package = motion_effects::builtin::package().unwrap();
            for (id, name) in [(1, "posterize"), (2, "curves")] {
                let definition = package
                    .manifest
                    .effects
                    .iter()
                    .find(|e| e.id == name)
                    .unwrap();
                let mut effect = EffectInstance::new(
                    id,
                    &package.manifest.id,
                    &package.manifest.version,
                    &package.hash,
                    definition,
                    [64.; 2],
                );
                let (name, parameter) = effect
                    .params
                    .iter_mut()
                    .find(|(_, p)| {
                        p.animatable && p.implemented && (name != "curves" || p.curve.is_some())
                    })
                    .unwrap();
                if let Some(curve) = &mut parameter.curve {
                    for frame in [3, 13] {
                        curve.keys.push(Keyframe {
                            frame,
                            value: curve.value.clone(),
                            ease: Ease::Hold,
                            curve: None,
                            spatial: None,
                            auto_bezier: false,
                        });
                    }
                } else {
                    seed(&mut parameter.track);
                }
                targets.push(KeyframeTarget::Effect {
                    object: 7,
                    effect: id,
                    param: name.clone(),
                });
                p.layers[0].effects.push(effect);
            }
            catalogue = json!({"packages":[{"hash":package.hash,"manifest":package.manifest}]});
            p.rebuild_plugin_dependencies();
        }
        Case::Audio => {
            p.audio_assets.push(AudioAsset {
                id: 1,
                path: "assets/tone.wav".into(),
                bytes: 64,
                mime: "audio/wav".into(),
                track: 0,
                sample_rate: 48000,
                channels: 2,
                sample_frames: 192000,
                duration_us: 4_000_000,
            });
            let layer = &mut p.layers[0];
            layer.content = Content::Audio {
                audio: AudioClip::new(1),
            };
            layer.size = [0.; 2];
            layer.transform = motion_core::Transform::new([0.; 3]);
            let mut levels = AudioLevels::default();
            seed(&mut levels.left);
            seed(&mut levels.right);
            layer.audio_levels = Some(levels);
            targets = [AudioChannel::Left, AudioChannel::Right]
                .into_iter()
                .map(|channel| KeyframeTarget::Audio { object: 7, channel })
                .collect();
        }
    }
    (Engine::new(p).unwrap(), catalogue, targets)
}
pub(super) fn ready(
    case: Case,
    catalogue: &Value,
    s: &Value,
    locale: Locale,
    expanded: bool,
) -> Panels {
    let mut p = setup(locale, false, expanded);
    p.timeline.collapsed_transforms.insert(7);
    p.timeline.expanded_effects.insert(7);
    p.timeline
        .filters
        .insert(7, crate::panels::types::PropertyFilter::Animated);
    p.effect_controls.catalogue = catalogue.clone().into();
    if expanded && matches!(case, Case::FlatVector | Case::GroupVector) {
        crate::panels::path_controls::groups::timeline::reveal_animated(&mut p, s, 7);
    }
    p
}
pub(super) fn points(paint: &PaintList, panels: &Panels, time: f64) -> Vec<[f32; 2]> {
    let track = panels.timeline.zoom_track.unwrap();
    let x = track.min[0] + time as f32 * track.width() / 120.;
    let mut y: Vec<_> = paint
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Procedural {
                area,
                kind: Pattern::Diamond,
                ..
            } if ((area.min[0] + area.max[0]) * 0.5 - x).abs() < 0.02 => {
                Some((area.min[1] + area.max[1]) * 0.5)
            }
            _ => None,
        })
        .collect();
    y.sort_by(f32::total_cmp);
    y.dedup();
    y.into_iter().map(|y| [x, y]).collect()
}
#[test]
fn row_cache_projects_all_typed_addresses_and_keeps_static_vector_settings_normal() {
    for case in CASES {
        for reverse in [false, true] {
            let (engine, _, targets) = typed(case, reverse);
            let s = view(&engine);
            let row = crate::panels::timeline::rows(&s).remove(0);
            let keys = row.mapped_keys.as_ref().expect("valid typed cache");
            assert_eq!(keys.len(), targets.len() * 2, "{case:?}");
            for target in targets {
                for frame in [3, 13] {
                    let time = engine.project().layers[0]
                        .property_composition_frame(f64::from(frame))
                        .unwrap();
                    assert!(
                        keys.contains(&(
                            LocalKey {
                                target: target.clone(),
                                frame
                            }
                            .into(),
                            time
                        )),
                        "{case:?} {target:?}"
                    );
                }
            }
            assert!(row.keys.windows(2).all(|pair| pair[0] < pair[1]));
            assert_eq!(row.keys.len(), 2);
            assert_eq!(
                row.keys,
                if reverse {
                    vec![22.5, 37.5]
                } else {
                    vec![6.5, 21.5]
                }
            );
        }
    }
}
#[test]
fn typed_lanes_and_summaries_drag_the_authored_cohort_once_in_both_owners_and_languages() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for case in CASES {
        for reverse in [false, true] {
            for expanded in [false, true] {
                for locale in [Locale::Zh, Locale::En] {
                    for floating in [false, true] {
                        let (engine, catalogue, targets) = typed(case, reverse);
                        let baseline = engine.snapshot();
                        let s = view(&engine);
                        let time = engine.project().layers[0]
                            .property_composition_frame(3.)
                            .unwrap();
                        let mut panels = ready(case, &catalogue, &s, locale, expanded);
                        let (paint, actions) =
                            draw(&mut panels, &s, Input::default(), floating, &mut atlas);
                        selection_only(&actions);
                        let starts = points(&paint, &panels, time);
                        assert!(
                            !starts.is_empty(),
                            "{case:?} expanded={expanded} reverse={reverse}"
                        );
                        let mut covered = BTreeSet::new();
                        for start in starts {
                            let mut engine = Engine::new(baseline.clone()).unwrap();
                            let mut panels = ready(case, &catalogue, &s, locale, expanded);
                            draw(&mut panels, &s, Input::default(), floating, &mut atlas);
                            let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
                            let modifiers = Modifiers {
                                alt: true,
                                ..Default::default()
                            };
                            let (_, actions) = draw(
                                &mut panels,
                                &s,
                                pointer(start, true, false, modifiers),
                                floating,
                                &mut atlas,
                            );
                            selection_only(&actions);
                            let selected = panels.selected_keys.local_selections();
                            assert!(!selected.is_empty(), "{case:?} {start:?}");
                            assert!(selected.iter().all(|(_, keys)| keys == &[3]));
                            for (target, _) in &selected {
                                assert!(targets.contains(target));
                                covered.insert(format!("{target:?}"));
                            }
                            let next = [start[0] + 6. * ppf, start[1]];
                            let (_, actions) = draw(
                                &mut panels,
                                &s,
                                pointer(next, false, false, modifiers),
                                floating,
                                &mut atlas,
                            );
                            selection_only(&actions);
                            assert_eq!(engine.project(), &baseline);
                            let (_, actions) = draw(
                                &mut panels,
                                &s,
                                pointer(next, false, true, modifiers),
                                floating,
                                &mut atlas,
                            );
                            let commands = atomic(actions);
                            assert_eq!(commands.len(), selected.len());
                            assert!(commands.iter().all(|c|matches!(c,Command::LocalKeyframes {edit:LocalKeyEdit::Translate {frames,delta_frame,copy:false,value_offset:None},..}
                    if frames==&[3]&&*delta_frame==if reverse {-4}else{4})));
                            engine.apply_batch(commands).unwrap();
                            assert!(engine.undo().unwrap());
                            assert_eq!(engine.project(), &baseline);
                            assert!(!engine.can_undo());
                        }
                        assert_eq!(
                            covered,
                            targets.iter().map(|t| format!("{t:?}")).collect(),
                            "{case:?} expanded={expanded}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn reopening_the_same_root_and_revision_refreshes_clocked_rows_in_both_owners() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        let (forward, catalogue, _) = typed(Case::Masks, false);
        let (reverse, _, _) = typed(Case::Masks, true);
        let a = view(&forward);
        let mut b = view(&reverse);
        assert_eq!(a["root"], b["root"]);
        assert_eq!(a["revision"], b["revision"]);
        b["document_generation"] = json!(8);
        let mut panels = ready(Case::Masks, &catalogue, &a, Locale::Zh, true);
        draw(&mut panels, &a, Input::default(), floating, &mut atlas);
        let original = panels.timeline.rows_cache.as_ref().unwrap().1.clone();
        assert_eq!(original[0].keys, [6.5, 21.5]);
        draw(&mut panels, &b, Input::default(), floating, &mut atlas);
        let replacement = panels.timeline.rows_cache.as_ref().unwrap().1.clone();
        assert!(!std::sync::Arc::ptr_eq(&original, &replacement));
        assert_eq!(replacement[0].keys, [22.5, 37.5]);
        draw(&mut panels, &b, Input::default(), floating, &mut atlas);
        assert!(std::sync::Arc::ptr_eq(
            &replacement,
            &panels.timeline.rows_cache.as_ref().unwrap().1
        ));
    }
}

#[test]
fn failed_typed_cache_is_atomic_observed_at_default_level_and_restores_without_retries() {
    const NAME:&str="panels::key_selection::tests::ui::local_timing::typed::failed_typed_cache_is_atomic_observed_at_default_level_and_restores_without_retries";
    const CHILD: &str = "MOTION_TYPED_KEYS_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let (engine, _, targets) = typed(Case::Effects, false);
        let good = view(&engine);
        let mut bad = good.clone();
        let KeyframeTarget::Effect { effect, param, .. } = &targets[0] else {
            panic!()
        };
        let effect = bad["project"]["layers"][0]["effects"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["id"] == *effect)
            .unwrap();
        effect["params"][param]["track"]["keys"] = Value::Null;
        for _ in 0..120 {
            let row = crate::panels::timeline::rows(&bad).remove(0);
            assert!(row.mapped_keys.is_none());
            assert!(row.keys.is_empty());
        }
        for _ in 0..120 {
            assert!(crate::panels::timeline::rows(&good)[0]
                .mapped_keys
                .is_some());
        }
        for _ in 0..120 {
            assert!(crate::panels::timeline::rows(&bad)[0].mapped_keys.is_none());
        }
        crate::logging::flush();
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let lines: Vec<_> = file
            .lines()
            .filter(|l| l.contains("site=timeline.mapped_keys "))
            .collect();
        assert_eq!(
            lines.len(),
            2,
            "lifetime row rebuild counter stays bounded after recovery"
        );
        assert!(lines[0].contains("WARN") && lines[0].contains("count=1"));
        assert!(lines[1].contains("ERROR") && lines[1].contains("count=60"));
        assert!(lines.iter().all(|l| l.contains("object=7")
            && l.contains("phase=row_cache")
            && l.contains("authored key array missing")));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_TYPED_KEYS_LOG_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
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
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .matches("site=timeline.mapped_keys ")
            .count(),
        1
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("site=timeline.mapped_keys "));
}
