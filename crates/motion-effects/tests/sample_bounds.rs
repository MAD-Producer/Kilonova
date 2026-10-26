use motion_effects::shader;

fn bound(source: &str) -> u64 {
    shader::compile(source, "main_fx")
        .unwrap()
        .samples_per_fragment
        .unwrap()
}

#[test]
fn fragment_bounds_include_sdk_sampling_loads_and_repeated_helper_calls() {
    assert_eq!(
        bound("fn main_fx(p:vec2<f32>)->vec4<f32>{return vec4(1.0);}"),
        1
    );
    assert_eq!(
        bound("fn main_fx(p:vec2<f32>)->vec4<f32>{return sample_input(p);}"),
        2
    );
    assert_eq!(
        bound("fn main_fx(p:vec2<f32>)->vec4<f32>{return textureLoad(resource0,vec2<i32>(0),0);}"),
        2
    );
    assert_eq!(bound("fn helper(p:vec2<f32>)->vec4<f32>{return sample_input(p)+textureLoad(resource0,vec2<i32>(0),0);}fn main_fx(p:vec2<f32>)->vec4<f32>{return helper(p)+helper(p);}"), 5);
    assert_eq!(
        bound("fn main_fx(p:vec2<f32>)->vec4<f32>{return curve_lookup(vec4(0.5));}"),
        9
    );
    let sprite = shader::compile_mode(
        "fn main_fx(p:vec2<f32>,c:vec4<f32>,s:vec4<f32>)->vec4<f32>{return c;}",
        "main_fx",
        true,
        false,
    )
    .unwrap();
    assert_eq!(sprite.samples_per_fragment, Some(0));
}

#[test]
fn fragment_bounds_count_both_branches_and_conservatively_expand_loop_calls() {
    assert_eq!(bound("fn main_fx(p:vec2<f32>)->vec4<f32>{if p.x>0.0{return sample_input(p);}return sample_source(p);}"), 3);
    assert_eq!(bound("fn helper(p:vec2<f32>)->vec4<f32>{var c=vec4(0.0);for(var i:i32=0;i<3;i=i+1){c+=sample_input(p);}return c;}fn main_fx(p:vec2<f32>)->vec4<f32>{return helper(p)+helper(p);}"), 9);
    assert_eq!(bound("fn main_fx(p:vec2<f32>)->vec4<f32>{var c=vec4(0.0);for(var i:i32=0;i<3;i=i+1){for(var j:i32=0;j<5;j=j+1){c+=sample_input(p);}}return c;}"), 30);
}

#[test]
fn reachable_call_multiplicity_overflow_refuses_instead_of_faking_a_bound() {
    let mut source = String::from("fn read_level_0(p:vec2<f32>)->vec4<f32>{return sample_input(p);}");
    for i in 1..=64 {
        source.push_str(&format!(
            "fn read_level_{i}(p:vec2<f32>)->vec4<f32>{{return read_level_{}(p)+read_level_{}(p);}}",
            i - 1,
            i - 1
        ));
    }
    let unreachable = format!("{source}fn main_fx(p:vec2<f32>)->vec4<f32>{{return vec4(1.0);}}");
    assert_eq!(
        bound(&unreachable),
        1,
        "unreachable helper work is not attributed to the fragment"
    );
    source.push_str("fn main_fx(p:vec2<f32>)->vec4<f32>{return read_level_64(p);}");
    let error = shader::compile(&source, "main_fx").unwrap_err();
    assert!(
        error.to_string().contains("texture-read bound overflows"),
        "{error}"
    );
}
