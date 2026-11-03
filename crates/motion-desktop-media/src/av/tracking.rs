//! Full, sequential original decoding for tracking import. Preview's rounded
//! microsecond index, source FPS and proxy decoders are not used as PTS evidence.
use super::*;
use motion_host::tracking::{asset::Asset, decoded::VideoTimeline};

pub fn verify(
    path: &Path,
    expected: &VideoAsset,
    solve: &Asset,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    verify_inner(path, expected, solve, check).map_err(|cause| {
        cause.context(format!(
            "tracking original video track={} path={}",
            expected.track,
            path.to_string_lossy().chars().take(256).collect::<String>()
        ))
    })
}
fn verify_inner(
    path: &Path,
    expected: &VideoAsset,
    solve: &Asset,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    check()?;
    expected.validate().map_err(MediaError::from)?;
    if expected.frame_count as usize != solve.source_pts.len() {
        return Err("tracking source count differs from managed video".into());
    }
    let input = input(path)?;
    let stream = video_stream(&input, Some(expected.track))?;
    let base = stream.time_base();
    if mime(stream.parameters().id())? != expected.mime || rotation(&stream)? != expected.rotation {
        return Err("tracking original codec/rotation differs from managed video".into());
    }
    let video = codec::context::Context::from_parameters(stream.parameters())
        .and_then(|c| c.decoder().video())
        .map_err(av_error)?;
    let aspect = video.aspect_ratio();
    if aspect.numerator() > 0 && aspect.numerator() != aspect.denominator() {
        return Err("tracking original requires square source pixels".into());
    }
    if [video.width(), video.height()] != [expected.width, expected.height]
        || solve.source_extent.pixels() != [expected.width, expected.height]
    {
        return Err("tracking original raw dimensions differ from registered source".into());
    }
    let mut timeline = VideoTimeline::new(solve, base.numerator(), base.denominator())
        .map_err(|e| MediaError::backend("tracking", e))?;
    // Only next() is used. No preview index, seeking, frame reuse or pixel
    // conversion is involved; each decoded presentation frame is consumed once.
    let mut decoder = Decoder {
        input,
        video,
        base,
        asset: expected.clone(),
        timestamps: Vec::new(),
        draining: false,
        last_decoded: None,
        seeks: 0,
        last_rgb_encoding: None,
    };
    let mut count = 0usize;
    while let Some(frame) = decoder.next(check)? {
        check()?;
        let pts = frame
            .pts()
            .ok_or("tracking decoded frame has no original PTS")?;
        if frame.timestamp().is_some_and(|best| best != pts) {
            return Err(format!(
                "tracking decoded PTS differs from inferred timestamp frame={count}"
            )
            .into());
        }
        timeline
            .observe(pts, frame.width(), frame.height())
            .map_err(|e| MediaError::backend("tracking", e))?;
        count += 1;
    }
    check()?;
    timeline
        .finish()
        .map_err(|e| MediaError::backend("tracking", e))
}

#[cfg(test)]
mod tests;
