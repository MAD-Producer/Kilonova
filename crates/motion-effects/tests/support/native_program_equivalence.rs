//! Platform qualification: exact structure/identity and bounded float data.
//! Actual generated programs additionally pass unchanged CPU/GPU pixel oracles.
use motion_effects::color_processor::ColorProcessorSource;

fn close(actual: f64, expected: f64, absolute: f64, relative: f64) -> bool {
    actual.is_finite()
        && expected.is_finite()
        && (actual - expected).abs() <= absolute + relative * expected.abs()
}

/// Preserve every identifier, integer, operator, comment and layout byte. Only
/// floating literals are separated for finite numerical comparison. This is
/// deliberately not a source normalizer or a general GLSL equivalence claim.
fn floats(source: &str) -> Result<(Vec<&str>, Vec<f64>), String> {
    let b = source.as_bytes();
    let mut i = 0;
    let mut last = 0;
    let mut text = Vec::new();
    let mut values = Vec::new();
    while i < b.len() {
        if b[i..].starts_with(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i..].starts_with(b"/*") {
            i += 2;
            while i + 1 < b.len() && !b[i..].starts_with(b"*/") {
                i += 1;
            }
            if i + 1 >= b.len() {
                return Err("unterminated shader comment".into());
            }
            i += 2;
            continue;
        }
        if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            continue;
        }
        if b[i].is_ascii_digit() || (b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            let mut floating = false;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if b.get(i) == Some(&b'.') {
                floating = true;
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if b.get(i).is_some_and(|v| *v == b'e' || *v == b'E') {
                floating = true;
                i += 1;
                if b.get(i).is_some_and(|v| *v == b'+' || *v == b'-') {
                    i += 1;
                }
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if floating {
                let end = i;
                if b.get(i).is_some_and(|v| *v == b'f' || *v == b'F') {
                    i += 1;
                }
                let value = source[start..end]
                    .parse::<f64>()
                    .map_err(|_| "invalid shader floating literal")?;
                text.push(&source[last..start]);
                values.push(value);
                last = end;
                // Keep the suffix in the exact text segment.
            }
            continue;
        }
        i += 1;
    }
    text.push(&source[last..]);
    Ok((text, values))
}

pub fn compare(
    actual: &ColorProcessorSource,
    expected: &ColorProcessorSource,
    absolute: f32,
    relative: f32,
) -> Result<(), String> {
    let mut a = serde_json::to_value(actual).map_err(|e| e.to_string())?;
    let mut e = serde_json::to_value(expected).map_err(|e| e.to_string())?;
    for v in [&mut a, &mut e] {
        let object = v.as_object_mut().ok_or("program object")?;
        object.remove("shader");
        object.remove("tables");
    }
    if a != e {
        return Err("program identity/phase/alpha/version differs".into());
    }
    let (at, av) = floats(&actual.shader)?;
    let (et, ev) = floats(&expected.shader)?;
    if at != et || av.len() != ev.len() {
        return Err("shader structure differs".into());
    }
    for (index, (a, e)) in av.into_iter().zip(ev).enumerate() {
        if !close(a, e, absolute.into(), relative.into()) {
            return Err(format!("shader floating literal {index} differs"));
        }
    }
    if actual.tables.len() != expected.tables.len() {
        return Err("table count differs".into());
    }
    for (index, (a, e)) in actual.tables.iter().zip(&expected.tables).enumerate() {
        let mut am = serde_json::to_value(a).map_err(|e| e.to_string())?;
        let mut em = serde_json::to_value(e).map_err(|e| e.to_string())?;
        am.as_object_mut().ok_or("table object")?.remove("values");
        em.as_object_mut().ok_or("table object")?.remove("values");
        if am != em || a.values.len() != e.values.len() {
            return Err(format!("table {index} structure differs"));
        }
        for (sample, (a, e)) in a.values.iter().zip(&e.values).enumerate() {
            if !close((*a).into(), (*e).into(), absolute.into(), relative.into()) {
                return Err(format!("table {index} sample {sample} differs"));
            }
        }
    }
    Ok(())
}

#[test]
fn comparison_scope_rejects_identity_structure_and_numeric_mutations() {
    let reference: serde_json::Value =
        serde_json::from_slice(include_bytes!("../color_processor/reference.json")).unwrap();
    let original: ColorProcessorSource =
        serde_json::from_value(reference["cases"][8]["processor"].clone()).unwrap();
    assert!(compare(&original, &original, 0.0001, 0.0002).is_ok());
    for change in 0..9 {
        let mut bad = original.clone();
        match change {
            0 => bad.config_key[0] ^= 1,
            1 => bad.input_alpha = motion_effects::AlphaMode::Premultiplied,
            2 => bad.tables[0].width += 1,
            3 => bad.tables[0].values[0] += 1.,
            4 => bad.tables[0].values[0] = f32::NAN,
            5 => {
                bad.shader = bad
                    .shader
                    .replacen("outColor = inPixel", "outColor = -inPixel", 1)
            }
            6 => bad.shader = bad.shader.replacen("0.0323680267", "2.0323680267", 1),
            7 => bad.shader = bad.shader.replacen("binding = 1", "binding = 2", 1),
            8 => bad.tables[0].values.pop().map(|_| ()).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            compare(&bad, &original, 0.0001, 0.0002).is_err(),
            "negative control={change}"
        );
    }
    // Identifiers/comments containing digits are exact, not floating literals.
    let (text, values) = floats("vec3 x12 = vec3(1.25e-2f, 3., .5); // 2.5\n").unwrap();
    assert_eq!(values, [0.0125, 3., 0.5]);
    assert_eq!(text, ["vec3 x12 = vec3(", "f, ", ", ", "); // 2.5\n"]);
}
