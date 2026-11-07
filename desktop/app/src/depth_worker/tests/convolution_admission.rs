#[path = "../admission.rs"]
mod admission;
#[path = "../convolution_admission.rs"]
mod convolution_admission;

use convolution_admission::{Attributes, Kind, plan};

fn attributes(kernel: [u32; 2], stride: [u32; 2], padding: [u32; 4]) -> Attributes {
    Attributes {
        kernel,
        stride,
        padding,
        dilation: [1, 1],
        group: 1,
        output_padding: [0, 0],
    }
}

#[test]
fn rectangular_nchw_padding_and_optional_bias_are_explicit() {
    let a = attributes([3, 3], [1, 1], [1, 1, 1, 1]);
    let p = plan(
        Kind::CrossCorrelation,
        [1, 2, 3, 5],
        [3, 2, 3, 3],
        Some(3),
        [1, 3, 3, 5],
        &a,
        &wgpu::Limits::downlevel_defaults(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(p.entry, "conv_nchw");
    assert_eq!(&p.parameters[..8], &[1, 2, 3, 5, 3, 3, 5, 3]);
    assert_eq!(&p.parameters[8..12], &[3, 1, 1, 1]);
    assert_eq!(p.parameters[12], 1);
    assert_eq!(p.parameters[14], 1);
    let without = plan(
        Kind::CrossCorrelation,
        [1, 2, 3, 5],
        [3, 2, 3, 3],
        None,
        [1, 3, 3, 5],
        &a,
        &wgpu::Limits::downlevel_defaults(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(without.parameters[14], 0);
    assert_eq!(p.logical_buffer_bytes - without.logical_buffer_bytes, 8);
}

#[test]
fn patch_embedding_stride14_has_the_original_37_by_37_grid() {
    let a = attributes([14, 14], [14, 14], [0; 4]);
    let p = plan(
        Kind::CrossCorrelation,
        [1, 3, 518, 518],
        [384, 3, 14, 14],
        Some(384),
        [1, 384, 37, 37],
        &a,
        &wgpu::Limits::downlevel_defaults(),
        256 << 20,
    )
    .unwrap();
    assert_eq!(&p.parameters[4..8], &[384, 37, 37, 14]);
    assert!(p.dispatch.iter().all(|&d| d > 0));
    assert!(
        plan(
            Kind::CrossCorrelation,
            [1, 3, 518, 518],
            [384, 3, 14, 14],
            Some(384),
            [1, 384, 38, 38],
            &a,
            &wgpu::Limits::downlevel_defaults(),
            256 << 20
        )
        .is_err()
    );
}

#[test]
fn transpose_uses_iohw_weights_and_nonoverlapping_spatial_extents() {
    let a = attributes([2, 2], [2, 2], [0; 4]);
    let p = plan(
        Kind::TransposeNonoverlap,
        [1, 2, 3, 5],
        [2, 3, 2, 2],
        Some(3),
        [1, 3, 6, 10],
        &a,
        &wgpu::Limits::downlevel_defaults(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(p.entry, "conv_transpose_no_overlap_nchw");
    assert_eq!(&p.parameters[4..8], &[3, 6, 10, 2]);
    assert!(
        plan(
            Kind::TransposeNonoverlap,
            [1, 2, 3, 5],
            [3, 2, 2, 2],
            Some(3),
            [1, 3, 6, 10],
            &a,
            &wgpu::Limits::downlevel_defaults(),
            1 << 20
        )
        .is_err()
    );
}

#[test]
fn unsupported_transpose_overlap_group_dilation_and_output_padding_are_refused() {
    let l = wgpu::Limits::downlevel_defaults();
    for which in 0..5 {
        let mut a = attributes([2, 2], [2, 2], [0; 4]);
        match which {
            0 => a.stride = [1, 1],
            1 => a.group = 2,
            2 => a.dilation = [2, 2],
            3 => a.padding = [1, 1, 1, 1],
            _ => a.output_padding = [1, 1],
        }
        assert!(
            plan(
                Kind::TransposeNonoverlap,
                [1, 2, 3, 5],
                [2, 3, 2, 2],
                Some(3),
                [1, 3, 6, 10],
                &a,
                &l,
                1 << 20
            )
            .is_err()
        );
    }
}

#[test]
fn allocation_budget_and_bias_width_are_checked_before_dispatch() {
    let a = attributes([1, 1], [1, 1], [0; 4]);
    let l = wgpu::Limits::downlevel_defaults();
    let p = plan(
        Kind::CrossCorrelation,
        [1, 2, 3, 5],
        [3, 2, 1, 1],
        Some(3),
        [1, 3, 3, 5],
        &a,
        &l,
        1 << 20,
    )
    .unwrap();
    assert!(
        plan(
            Kind::CrossCorrelation,
            [1, 2, 3, 5],
            [3, 2, 1, 1],
            Some(3),
            [1, 3, 3, 5],
            &a,
            &l,
            p.logical_buffer_bytes - 1
        )
        .is_err()
    );
    assert!(
        plan(
            Kind::CrossCorrelation,
            [1, 2, 3, 5],
            [3, 2, 1, 1],
            Some(2),
            [1, 3, 3, 5],
            &a,
            &l,
            1 << 20
        )
        .is_err()
    );
}
