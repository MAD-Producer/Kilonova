//! Conservative fragment coverage from the already validated module, never
//! from source text. An unused helper may still make this proof conservative.
pub(super) fn may_discard(module: &naga::Module) -> bool {
    module.functions.iter().any(|(_, function)| kills(&function.body))
        || module.entry_points.iter().any(|entry| kills(&entry.function.body))
}

fn kills(block: &naga::Block) -> bool {
    block.iter().any(|statement| match statement {
        naga::Statement::Kill => true,
        naga::Statement::Block(block) => kills(block),
        naga::Statement::If { accept, reject, .. } => kills(accept) || kills(reject),
        naga::Statement::Switch { cases, .. } => cases.iter().any(|case| kills(&case.body)),
        naga::Statement::Loop { body, continuing, .. } => kills(body) || kills(continuing),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn sdk_compilation_uses_ir_for_direct_nested_and_called_discards() {
        for source in [
            "fn main_fx(p:vec2<f32>)->vec4<f32>{discard;}",
            "fn main_fx(p:vec2<f32>)->vec4<f32>{if p.x>0.0 {switch i32(p.y){case 0:{discard;}default:{}}}return vec4(1.0);}",
            "fn main_fx(p:vec2<f32>)->vec4<f32>{for(var i=0;i<2;i=i+1){if p.x>f32(i){discard;}}return vec4(1.0);}",
            "fn hidden(p:vec2<f32>){if p.x>0.0{discard;}}fn main_fx(p:vec2<f32>)->vec4<f32>{hidden(p);return vec4(1.0);}",
            "fn unused(){discard;}fn main_fx(p:vec2<f32>)->vec4<f32>{return vec4(1.0);}",
        ] {
            let shader = crate::shader::compile(source, "main_fx").unwrap();
            assert_eq!(shader.may_discard, Some(true), "{source}");
        }
    }

    #[test]
    fn comments_and_similar_identifiers_do_not_invent_a_kill() {
        let shader = crate::shader::compile(
            "// discard;\nfn main_fx(p:vec2<f32>)->vec4<f32>{let discarded=p.x;return vec4(discarded);}",
            "main_fx",
        ).unwrap();
        assert_eq!(shader.may_discard, Some(false));
    }

    #[test]
    fn nested_blocks_and_loop_continuations_are_walked_independently_of_portable_syntax() {
        let mut module = naga::front::wgsl::parse_str("fn helper(){loop {break;}}").unwrap();
        let (_, function) = module.functions.iter_mut().next().unwrap();
        let naga::Statement::Loop { continuing, .. } = function.body.iter_mut().next().unwrap() else {
            panic!("parser did not preserve loop");
        };
        // Structural walker fixture only: never validated or executed by a GPU.
        continuing.push(naga::Statement::Kill, naga::Span::UNDEFINED);
        assert!(super::may_discard(&module));
    }
}
