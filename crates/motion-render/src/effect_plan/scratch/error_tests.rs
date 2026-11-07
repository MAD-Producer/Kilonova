use super::*;
use crate::effect_plan::{EffectPass, PlannedDraw, ScratchGraphError, ScratchPlacementError};
use bytemuck::Zeroable;
use std::error::Error;

fn frame() -> EffectFramePlan {
    EffectFramePlan {
        width: 8,
        height: 8,
        passes: vec![EffectPass {
            instance_data: 0,
            resource_input: 0,
            program: 0,
            input: -1,
            source: -1,
            output: 0,
            width: 8,
            height: 8,
            lut: -1,
            uniform: super::super::EffectUniform::zeroed(),
            sprite: false,
            sprite_start: 0,
            sprite_count: 0,
        }],
        draws: vec![PlannedDraw {
            layer: 17,
            words: [0.; super::super::DRAW_WORDS],
            pass_start: 0,
            pass_end: 1,
        }],
        ..Default::default()
    }
}

#[test]
fn logical_scratch_failures_keep_native_fields_source_and_recover() {
    let mut frame = frame();
    let expected = layout(&frame, None).unwrap();
    let original_pass = frame.passes[0];
    frame.passes[0].input = 0;
    let before = format!("{frame:?}");
    let error = layout(&frame, None).unwrap_err();
    let RenderError::ScratchGraph(cause) = &error else {
        panic!("logical scratch error was flattened: {error:?}");
    };
    assert_eq!(
        *cause,
        ScratchGraphError::UninitializedInput { pass: 0, slot: 0 }
    );
    assert!(std::ptr::eq(
        error
            .source()
            .unwrap()
            .downcast_ref::<ScratchGraphError>()
            .unwrap(),
        cause
    ));
    assert_eq!(error.to_string(), format!("GPU validation: {cause}"));
    assert_eq!(error.planning_message(), cause.to_string());
    assert!(error.resource_rejection().is_none());
    assert_eq!(format!("{frame:?}"), before);
    let text: RenderError = cause.to_string().into();
    assert!(matches!(text, RenderError::Invalid(_)));
    assert!(
        text.source().is_none(),
        "text must not manufacture graph identity"
    );

    frame.passes[0] = original_pass;
    frame.draws[0].pass_end = 2;
    let error = layout(&frame, None).unwrap_err();
    assert!(matches!(
        error,
        RenderError::ScratchGraph(ScratchGraphError::DrawRange { draw: 0 })
    ));
    frame.draws[0].pass_end = 1;
    let recovered = layout(&frame, None).unwrap();
    assert_eq!(recovered.allocations, expected.allocations);
    assert_eq!(recovered.bindings, expected.bindings);
    assert_eq!(recovered.outputs, expected.outputs);
    assert_eq!(recovered.bytes, expected.bytes);
}

#[test]
fn physical_scratch_failures_keep_native_fields_without_publishing_a_layout() {
    let mut frame = frame();
    let expected = layout(&frame, None).unwrap();
    for (extent, native) in [
        (
            [0, 8],
            ScratchPlacementError::InvalidResource {
                resource: 0,
                reason: "zero extent",
            },
        ),
        (
            [u32::MAX, u32::MAX],
            ScratchPlacementError::Arithmetic { resource: 0 },
        ),
    ] {
        frame.passes[0].width = extent[0];
        frame.passes[0].height = extent[1];
        let before = format!("{frame:?}");
        let error = layout(&frame, None).unwrap_err();
        let RenderError::ScratchPlacement(cause) = &error else {
            panic!("physical scratch error was flattened: {error:?}");
        };
        assert_eq!(*cause, native);
        assert!(std::ptr::eq(
            error
                .source()
                .unwrap()
                .downcast_ref::<ScratchPlacementError>()
                .unwrap(),
            cause
        ));
        assert_eq!(error.to_string(), format!("GPU validation: {native}"));
        assert_eq!(error.planning_message(), native.to_string());
        assert!(
            error.resource_rejection().is_none(),
            "planning overflow is not a runtime budget refusal"
        );
        assert_eq!(format!("{frame:?}"), before);
    }
    frame.passes[0].width = 8;
    frame.passes[0].height = 8;
    let recovered = layout(&frame, None).unwrap();
    assert_eq!(recovered.allocations, expected.allocations);
    assert_eq!(recovered.bindings, expected.bindings);
    assert_eq!(recovered.bytes, expected.bytes);
}
