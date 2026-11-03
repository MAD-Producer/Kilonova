//! Independent measured-observation oracle for the committed Surface pixels.
use super::*;

pub(super) fn pixels(image: &image::RgbaImage, data: &Asset, index: usize) -> Value {
    let sample = |name: &str| {
        data.tracks.iter().find(|t| t.id == name).unwrap().samples[index]
            .observation
            .unwrap()
    };
    let Observation::Point(point) = sample("feature") else {
        panic!("frozen feature observation is not a point")
    };
    let Observation::Plane(matrix) = sample("plane") else {
        panic!("frozen plane observation is not a homography")
    };
    // Deliberately do not call the product's project_quad or sampling helpers.
    let quad = [[32., 24.], [160., 24.], [160., 128.], [32., 128.]].map(|p| {
        let denominator = matrix[2][0] * p[0] + matrix[2][1] * p[1] + matrix[2][2];
        [0, 1].map(|axis| {
            (matrix[axis][0] * p[0] + matrix[axis][1] * p[1] + matrix[axis][2]) / denominator
        })
    });
    let mut interior = 0;
    let mut mismatched = 0;
    let mut red = Vec::new();
    let mut plate_pixels = 0;
    let mut plate_levels = std::collections::BTreeSet::new();
    for (x, y, color) in image.enumerate_pixels() {
        let pixel = [f64::from(x) + 0.5, f64::from(y) + 0.5];
        let mut inside = true;
        let mut stable = true;
        for edge in 0..4 {
            let a = quad[edge];
            let b = quad[(edge + 1) % 4];
            let cross = (b[0] - a[0]) * (pixel[1] - a[1]) - (b[1] - a[1]) * (pixel[0] - a[0]);
            inside &= cross > 0.;
            stable &= cross.abs() / (b[0] - a[0]).hypot(b[1] - a[1]) > 2.;
        }
        stable &= (pixel[0] - point[0]).abs() > 6. || (pixel[1] - point[1]).abs() > 6.;
        let green = color[1] > 180 && color[0] < 40 && color[2] < 40;
        if stable {
            interior += usize::from(inside);
            mismatched += usize::from(inside != green);
            // Outside the colored oracles, require the decoded grayscale
            // texture itself. A blank successful video draw must not pass.
            if !inside
                && color[0] > 32
                && color[0] < 224
                && color[0].abs_diff(color[1]) <= 2
                && color[0].abs_diff(color[2]) <= 2
            {
                plate_pixels += 1;
                plate_levels.insert(color[0]);
            }
        }
        if color[0] > 180 && color[1] < 40 && color[2] < 40 {
            red.push(pixel);
        }
    }
    // Same thresholds as the earlier native offscreen qualification, now on
    // CaptureUi's committed front. The grayscale plate cannot satisfy either.
    assert!(
        interior > 5000,
        "index={index} stable plane area={interior}"
    );
    assert_eq!(mismatched, 0, "index={index} plane pixel mismatch");
    assert!(red.len() >= 25, "index={index} point marker absent");
    assert!(
        plate_pixels > 5000 && plate_levels.len() > 32,
        "index={index} native video texture absent pixels={plate_pixels} levels={}",
        plate_levels.len()
    );
    let center = [0, 1].map(|axis| red.iter().map(|p| p[axis]).sum::<f64>() / red.len() as f64);
    assert!(
        (center[0] - point[0])
            .abs()
            .max((center[1] - point[1]).abs())
            < 0.75,
        "index={index} point center={center:?} measured={point:?}"
    );
    json!({"index":index,"point":point,"center":center,"pointPixels":red.len(),
        "quad":quad,"planeInterior":interior,"mismatched":mismatched,"edgeExclusionPixels":2.,
        "nativePlatePixels":plate_pixels,"nativePlateLevels":plate_levels.len()})
}
