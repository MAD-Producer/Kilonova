use super::*;

#[test]
fn native_half_store_specialization_is_bounded_validated_and_refuses_missing_policy() {
    let shader = compile(
        "fn main_fx(p:vec2<f32>)->vec4<f32>{return sample_input(p);}",
        "main_fx",
    )
    .unwrap();
    let before = compiled_code_identity(&shader);
    assert!(matches!(
        native_output_wgsl(&shader, false).unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
    let selected = native_output_wgsl(&shader, true).unwrap();
    assert_eq!(
        selected
            .matches("const SDK_STORE_FLOAT16: bool = true;")
            .count(),
        1
    );
    assert!(!selected.contains("const SDK_STORE_FLOAT16: bool = false;"));
    assert!(
        selected.len() <= shader.wgsl.len(),
        "the admitted code byte bound must cover specialization"
    );
    let module = naga::front::wgsl::parse_str(&selected).unwrap();
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .unwrap();
    assert_eq!(
        sample_bounds::fragment_reads(&module, shader.loop_work).unwrap(),
        shader.samples_per_fragment.unwrap()
    );
    let glsl = host_glsl(&module, &info, "sdk_vertex", "sdk_fragment").unwrap();
    assert!(!glsl.fragment.is_empty());
    assert_eq!(
        compiled_code_identity(&shader).digest,
        before.digest,
        "selection must not mutate shared compiled ownership"
    );

    for replacement in [
        "",
        "const SDK_STORE_FLOAT16: bool = false;\n// const SDK_STORE_FLOAT16: bool = false;",
    ] {
        let mut broken = shader.clone();
        broken.wgsl = broken
            .wgsl
            .replace("const SDK_STORE_FLOAT16: bool = false;", replacement);
        assert!(
            native_output_wgsl(&broken, true)
                .unwrap_err()
                .to_string()
                .contains("unique SDK store policy")
        );
    }
    assert_eq!(
        native_output_wgsl(&shader, true).unwrap(),
        selected,
        "refusal must not poison a valid program"
    );
}
