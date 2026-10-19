//! The same perspective filtering contract is used by native and exported
//! compositor shaders. Appending shared WGSL keeps the draw/vertex ABI intact.
pub const BASIC: &str = concat!(
    include_str!("plane.wgsl"),
    include_str!("plane_sampling.wgsl")
);
pub const MASK: &str = concat!(
    include_str!("plane_mask.wgsl"),
    include_str!("plane_sampling.wgsl")
);
pub const MATTE: &str = concat!(
    include_str!("plane_matte.wgsl"),
    include_str!("plane_sampling.wgsl")
);

#[cfg(test)]
mod tests {
    #[test]
    fn shared_filter_keeps_all_portable_plane_programs_compilable() {
        for source in [super::BASIC, super::MASK, super::MATTE] {
            // Packed plans retain the unexpanded geometry contract.
            let source = crate::plane_coverage::shader(source, false);
            let program = motion_effects::shader::compile_host_program(
                &source,
                "vertex_main",
                "fragment_main",
            )
            .unwrap();
            assert!(!program.vertex.is_empty() && !program.fragment.is_empty());
        }
        assert_eq!(crate::compositing_plan::plane_programs().unwrap().len(), 3);
    }
}
