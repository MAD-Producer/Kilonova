//! Exercise actual native metadata, including declarations unused by selection.
use super::*;
use motion_effects::color_config::ReferenceSpace;

#[test]
#[ignore = "requires explicit checksum-verified native OCIO prefix"]
fn unselected_catalogue_mutations_are_refused_before_publication_and_recover() {
    let runtime = runtime();
    let reference = reference();
    let item = &reference.configs[0];
    let original = config(item);
    let prepare = |config: &VerifiedColorConfig| {
        let mut selection = item.selection.clone();
        selection.config_key = config.key();
        runtime.prepare(
            config,
            &selection,
            ColorProcessorPhase::InputToWorking,
            AlphaMode::Straight,
            AlphaMode::Straight,
        )
    };
    let sample = [0.2, 0.4, 1.5];
    let expected = prepare(&original).unwrap().apply_rgb(sample).unwrap();
    let mut mutations: Vec<(&str, &str, ColorConfigManifest)> = Vec::new();
    let mut add = |label, field, change: fn(&mut ColorConfigManifest)| {
        let mut manifest = item.manifest.clone();
        change(&mut manifest);
        mutations.push((label, field, manifest));
    };
    add("unused encoding", "encoding", |m| {
        m.spaces
            .get_mut("Gamma 1.8 Encoded Rec.709")
            .unwrap()
            .encoding = "log".into();
    });
    add("unused reference", "reference", |m| {
        m.spaces
            .get_mut("Gamma 1.8 Encoded Rec.709")
            .unwrap()
            .reference = ReferenceSpace::Display;
    });
    add("unused data flag", "is_data", |m| {
        m.spaces.get_mut("Raw").unwrap().is_data = false;
    });
    add("unused role target", "role_target", |m| {
        m.roles.insert("matte_paint".into(), "ACEScg".into());
    });
    add("additional role", "roles", |m| {
        m.roles.insert("invented_role".into(), "ACEScg".into());
    });
    add("missing unused role", "roles", |m| {
        m.roles.remove("matte_paint").unwrap();
    });
    add("missing unused space", "spaces", |m| {
        m.spaces.remove("Gamma 1.8 Encoded Rec.709").unwrap();
    });
    add("additional space", "spaces", |m| {
        m.spaces.insert(
            "invented canonical space".into(),
            m.spaces["ACEScg"].clone(),
        );
    });
    add("missing unused display", "displays", |m| {
        m.displays.remove("Display P3 - Display").unwrap();
    });
    add("additional display", "displays", |m| {
        m.displays
            .insert("invented display".into(), vec!["Raw".into()]);
    });
    add("additional view", "views", |m| {
        m.displays
            .get_mut("sRGB - Display")
            .unwrap()
            .push("invented view".into());
    });
    add("missing unused view", "views", |m| {
        m.displays
            .get_mut("sRGB - Display")
            .unwrap()
            .retain(|v| v != "Raw");
    });
    for (label, field, manifest) in &mutations {
        // Same original configuration bytes; rekey the valid selection so this
        // reaches native catalogue verification, not the identity precondition.
        let bad = VerifiedColorConfig::new(
            manifest.clone(),
            Arc::from(item.config.as_bytes()),
            BTreeMap::new(),
        )
        .unwrap();
        assert_ne!(bad.key(), original.key());
        let error = prepare(&bad).err().expect(label);
        assert!(
            matches!(&error, Error::Catalogue { field: actual, name, expected, actual: native }
            if actual == field && !name.is_empty() && expected != native),
            "{label}: {error:?}"
        );
        let recovered = prepare(&original).unwrap().apply_rgb(sample).unwrap();
        assert_eq!(recovered, expected, "{label}: no poisoned retained runtime");
    }
    // Display view order is a presentation choice, not a different declaration.
    let mut reordered = item.manifest.clone();
    for views in reordered.displays.values_mut() {
        views.reverse();
    }
    let reordered = VerifiedColorConfig::new(
        reordered,
        Arc::from(item.config.as_bytes()),
        BTreeMap::new(),
    )
    .unwrap();
    assert_ne!(reordered.key(), original.key());
    assert_eq!(
        prepare(&reordered).unwrap().apply_rgb(sample).unwrap(),
        expected
    );
    println!(
        "native catalogue negative_controls={} recovery_checks={} reordered_views=accepted",
        mutations.len(),
        mutations.len()
    );
}
