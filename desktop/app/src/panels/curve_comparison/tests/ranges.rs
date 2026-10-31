use super::*;

fn entry<'a>(p: &'a Panels, property: Property, component: usize) -> &'a Entry {
    p.timeline
        .comparison
        .entries
        .iter()
        .find(|entry| {
            entry.channel.target == super::property(property)
                && entry.channel.component == component
        })
        .unwrap()
}
fn points(p: &Panels, property: Property) -> Arc<Vec<(f64, f32)>> {
    entry(p, property, 0)
        .tessellation
        .as_ref()
        .unwrap()
        .points
        .as_ref()
        .unwrap()
        .clone()
}

#[test]
fn comparison_tessellation_cache_tracks_ordinate_geometry_and_time_scale_even_when_end_is_capped() {
    let engine = fixture();
    let state = state(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut p = setup(&state, Locale::En);
    assert!(draw(&mut p, &state, &Input::default(), &mut atlas, false).is_empty());
    let pixels = points(&p, Property::Position);
    let percent = points(&p, Property::Opacity);
    assert!(draw(&mut p, &state, &Input::default(), &mut atlas, false).is_empty());
    assert!(Arc::ptr_eq(&pixels, &points(&p, Property::Position)));
    assert!(Arc::ptr_eq(&percent, &points(&p, Property::Opacity)));
    let coarse = entry(&p, Property::Opacity, 0)
        .points
        .as_ref()
        .unwrap()
        .1
        .clone();
    p.timeline.zoom = 1.001;
    assert!(draw(&mut p, &state, &Input::default(), &mut atlas, false).is_empty());
    assert!(Arc::ptr_eq(
        &coarse,
        &entry(&p, Property::Opacity, 0).points.as_ref().unwrap().1
    ));
    assert!(
        !Arc::ptr_eq(&percent, &points(&p, Property::Opacity)),
        "ppf still changes when end remains at the final frame"
    );
    let pixels = points(&p, Property::Position);
    let percent = points(&p, Property::Opacity);
    let header = p.timeline.comparison.test_range_header("%");
    let mouse = [header.min[0] + header.width() * 0.5, header.max[1] + 40.];
    assert!(draw(
        &mut p,
        &state,
        &Input {
            mouse,
            wheel: [0., -80.],
            modifiers: Modifiers {
                control: true,
                alt: true,
                ..Default::default()
            },
            ..Default::default()
        },
        &mut atlas,
        false
    )
    .is_empty());
    assert!(!Arc::ptr_eq(&percent, &points(&p, Property::Opacity)));
    assert!(
        Arc::ptr_eq(&pixels, &points(&p, Property::Position)),
        "one unit's Y zoom cannot change another unit's cache"
    );
    assert!(!engine.can_undo());
}

#[test]
fn comparison_context_snapshot_notices_visibility_collection_and_group_changes_without_allocating_each_match(
) {
    let engine = fixture();
    let state = state(&engine);
    let mut p = setup(&state, Locale::En);
    let context = p.timeline.comparison.range_context();
    assert!(p.timeline.comparison.matches_range_context(&context));
    p.timeline.comparison.entries[0].visible = false;
    assert!(!p.timeline.comparison.matches_range_context(&context));
    p.timeline.comparison.entries[0].visible = true;
    p.timeline.comparison.unit = Some(data::Unit::Percent);
    assert!(!p.timeline.comparison.matches_range_context(&context));
    p.timeline.comparison.unit = None;
    p.timeline.comparison.enabled = false;
    assert!(!p.timeline.comparison.matches_range_context(&context));
    p.timeline.comparison.enabled = true;
    p.timeline.comparison.entries.pop();
    assert!(!p.timeline.comparison.matches_range_context(&context));
    assert!(!engine.can_undo());
}

#[test]
fn comparison_actual_samples_keep_dense_high_zoom_error_bounds_for_values_and_physical_speed() {
    let mut engine = fixture();
    engine
        .apply(Command::Curve {
            object: 7,
            property: Property::Opacity,
            axis: None,
            frame: 15,
            easing: motion_core::Easing {
                ease: Ease::Linear,
                curve: Some(motion_core::Curve {
                    space: motion_core::CurveSpace::Progress,
                    shape: motion_core::CurveShape::Elastic {
                        oscillations: 8.,
                        damping: 0.5,
                    },
                }),
            },
        })
        .unwrap();
    let state = state(&engine);
    let data = data::load(&state, &property(Property::Opacity), Locale::En).unwrap();
    for speed in [false, true] {
        for [start, end] in [[15., 45.], [17., 17.1], [44.9, 45.]] {
            let scale = [800. / (end - start), 40.];
            let points = data
                .adaptive([start, end], 0, 30., speed, scale, 0.35)
                .unwrap();
            for pair in points.windows(2) {
                let delta = [
                    (pair[1].0 - pair[0].0) * scale[0],
                    f64::from(pair[1].1 - pair[0].1) * scale[1],
                ];
                let squared = delta[0] * delta[0] + delta[1] * delta[1];
                for i in 0..=128 {
                    let frame = pair[0].0 + (pair[1].0 - pair[0].0) * f64::from(i) / 128.;
                    let probe = [
                        (frame - pair[0].0) * scale[0],
                        f64::from(data.sample(frame, 0, 30., speed).unwrap() - pair[0].1)
                            * scale[1],
                    ];
                    let t = if squared > 0. {
                        ((probe[0] * delta[0] + probe[1] * delta[1]) / squared).clamp(0., 1.)
                    } else {
                        0.
                    };
                    let error = (probe[0] - t * delta[0]).hypot(probe[1] - t * delta[1]);
                    assert!(
                        error <= 0.36,
                        "speed={speed} range={start}..{end} frame={frame} error={error}"
                    );
                }
            }
        }
    }
}

#[test]
fn comparison_sampling_budget_failure_is_not_a_partial_curve_and_is_cached_until_the_view_changes()
{
    let mut project = Project::new(640, 360, 30, 36000).unwrap();
    let mut layer = Layer::solid(7, "Budget fixture", [100.; 2], [0.; 3], [1.; 4]);
    layer.transform.opacity.keys = (0..33000)
        .map(|frame| motion_core::Keyframe {
            auto_bezier: false,
            frame,
            value: if frame % 2 == 0 { 0.2 } else { 0.8 },
            ease: Ease::InOut,
            curve: None,
            spatial: None,
        })
        .collect();
    project.layers.push(layer);
    let engine = Engine::new(project).unwrap();
    let state = state(&engine);
    let mut p = Panels {
        selected: Some(7),
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    p.timeline.set_graph_mode(true);
    p.timeline.comparison.sync(&state, Locale::En);
    p.timeline
        .comparison
        .add(&state, Locale::En, &[property(Property::Opacity)])
        .unwrap();
    p.timeline.comparison.enabled = true;
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    assert!(draw(&mut p, &state, &Input::default(), &mut atlas, false).is_empty());
    let first = entry(&p, Property::Opacity, 0);
    assert!(first.tessellation.as_ref().unwrap().points.is_none());
    assert_eq!(first.tessellation_attempts, 1);
    assert!(draw(&mut p, &state, &Input::default(), &mut atlas, false).is_empty());
    let second = entry(&p, Property::Opacity, 0);
    assert!(second.tessellation.as_ref().unwrap().points.is_none());
    assert_eq!(second.tessellation_attempts, 1);
    p.timeline.zoom = 100.;
    assert!(draw(&mut p, &state, &Input::default(), &mut atlas, false).is_empty());
    let visible = entry(&p, Property::Opacity, 0);
    assert!(visible.tessellation.as_ref().unwrap().points.is_some());
    assert_eq!(visible.tessellation_attempts, 2);
    assert!(!engine.can_undo());
}
