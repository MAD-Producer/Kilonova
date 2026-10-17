//! Optional libav probing and YUV decoding. Export encoding remains pending.
use av::{codec, color, format, frame, media, Error, Packet, Rational};
use ffmpeg_next as av;
use motion_core::VideoAsset;
use motion_host::{
    platform::VideoDecoder,
    video_decode_policy,
    video_frame::{DecodedFrame, VideoPixels},
    Result,
};
use motion_media::VideoProbe;
use motion_render::{VideoPlane, Yuv420Frame};
use std::{path::Path, time::Instant};

fn input(path: &Path) -> Result<format::context::Input> {
    av::init().map_err(|e| e.to_string())?;
    // Sniff source bytes: owned imports need not retain their original extension.
    format::input(path).map_err(|e| format!("cannot open video: {e}"))
}
fn video_stream(
    input: &format::context::Input,
    selected: Option<u32>,
) -> Result<format::stream::Stream<'_>> {
    let stream = match selected {
        Some(index) => input.stream(index as usize),
        None => input.streams().best(media::Type::Video),
    }
    .ok_or("selected video track missing")?;
    if stream.parameters().medium() != media::Type::Video {
        return Err("selected track is not video".into());
    }
    Ok(stream)
}
fn mime(id: codec::Id) -> Result<&'static str> {
    match id {
        codec::Id::H264 => Ok("video/avc"),
        codec::Id::HEVC => Ok("video/hevc"),
        codec::Id::VP8 => Ok("video/x-vnd.on2.vp8"),
        codec::Id::VP9 => Ok("video/x-vnd.on2.vp9"),
        _ => Err(format!("unsupported video codec: {id:?}")),
    }
}
fn micros(pts: i64, base: Rational) -> Result<u64> {
    if pts < 0 || base.numerator() <= 0 || base.denominator() <= 0 {
        return Err("invalid video timestamp or time base".into());
    }
    let n = i128::from(pts)
        .checked_mul(i128::from(base.numerator()))
        .and_then(|n| n.checked_mul(1_000_000))
        .and_then(|n| n.checked_add(i128::from(base.denominator()) / 2))
        .ok_or("video timestamp overflow")?;
    u64::try_from(n / i128::from(base.denominator())).map_err(|_| "video timestamp overflow".into())
}
fn rotation(stream: &format::stream::Stream<'_>) -> Result<u32> {
    let metadata_degrees = stream
        .metadata()
        .get("rotate")
        .map(|text| text.parse::<f64>()
            .map_err(|error| format!("invalid video rotate metadata {text:?}: {error}")))
        .transpose();
    let degrees = if let Some(data) = stream
        .side_data()
        .find(|d| d.kind() == codec::packet::side_data::Type::DisplayMatrix)
    {
        if data.data().len() != 36 {
            return Err("invalid video display matrix".into());
        }
        let mut matrix = [0i32; 9];
        for (v, bytes) in matrix.iter_mut().zip(data.data().chunks_exact(4)) {
            *v = i32::from_ne_bytes(bytes.try_into().unwrap());
        }
        let determinant = i64::from(matrix[0]) * i64::from(matrix[4])
            - i64::from(matrix[1]) * i64::from(matrix[3]);
        if determinant <= 0 {
            return Err("mirrored video display matrix unsupported".into());
        }
        // FFmpeg's angle is counterclockwise; the renderer uses clockwise.
        -unsafe { av::ffi::av_display_rotation_get(matrix.as_ptr()) }
    } else {
        metadata_degrees?.unwrap_or(0.0)
    };
    if !degrees.is_finite() {
        return Err("unsupported video rotation".into());
    }
    let degrees = degrees.rem_euclid(360.0);
    let quarter = (degrees / 90.0).round();
    if (degrees - quarter * 90.0).abs() > 0.01 {
        return Err("unsupported video rotation".into());
    }
    Ok(((quarter as u32) % 4) * 90)
}
fn audio_tracks(
    input: &format::context::Input,
    selected: Option<u32>,
) -> Result<(Option<u32>, serde_json::Value)> {
    let mut tracks = Vec::new();
    let mut chosen = None;
    for stream in input
        .streams()
        .filter(|s| s.parameters().medium() == media::Type::Audio)
    {
        let decoder = codec::context::Context::from_parameters(stream.parameters())
            .and_then(|c| c.decoder().audio());
        let (rate, channels) = decoder
            .as_ref()
            .map(|d| (d.rate(), d.channels()))
            .unwrap_or((0, 0));
        let supported = (8000..=192000).contains(&rate) && matches!(channels, 1 | 2);
        let index = stream.index() as u32;
        if supported && chosen.is_none() && selected.is_none_or(|n| n == index) {
            chosen = Some(index);
        }
        let decoder_error = decoder.as_ref().err().map(ToString::to_string);
        tracks.push(serde_json::json!({"track":index,"codec":format!("{:?}",stream.parameters().id()),"sample_rate":rate,"channels":channels,"supported":supported,"error":decoder_error}));
    }
    if selected == Some(u32::MAX) {
        chosen = None;
    } else if (!tracks.is_empty() && chosen.is_none())
        || selected.is_some_and(|n| Some(n) != chosen)
    {
        let details: Vec<_> = tracks.iter().filter_map(|track| {
            track["error"].as_str().map(|error| format!("track {}: {error}", track["track"]))
        }).collect();
        let detail = if details.is_empty() { String::new() } else {
            format!(" ({})", details.join("; "))
        };
        return Err(format!(
            "selected audio track unavailable; use with_audio:false to discard audio{detail}"
        ));
    }
    Ok((chosen, serde_json::json!(tracks)))
}

pub fn probe(
    path: &Path,
    selected: Option<u32>,
    audio_selected: Option<u32>,
    check: &dyn Fn() -> Result<()>,
) -> Result<VideoProbe> {
    check()?;
    let mut input = input(path)?;
    if input.nb_streams() == 0 || input.nb_streams() > 32 {
        return Err("invalid media track count".into());
    }
    let stream = video_stream(&input, selected)?;
    let track = stream.index();
    let track_id = stream.id();
    let mov = input.format().name().split(',').any(|name| name == "mov");
    let base = stream.time_base();
    let mime = mime(stream.parameters().id())?.to_string();
    let video = codec::context::Context::from_parameters(stream.parameters())
        .and_then(|c| c.decoder().video())
        .map_err(|e| e.to_string())?;
    let (width, height) = (video.width(), video.height());
    let aspect = video.aspect_ratio();
    if aspect.numerator() > 0 && aspect.numerator() != aspect.denominator() {
        return Err("non-square video pixels unsupported".into());
    }
    if width == 0
        || height == 0
        || width > motion_core::MAX_VIDEO_DIMENSION
        || height > motion_core::MAX_VIDEO_DIMENSION
        || u64::from(width) * u64::from(height) > motion_core::MAX_VIDEO_PIXELS
    {
        return Err("video exceeds source pixel budget".into());
    }
    let rotation = rotation(&stream)?;
    let rate = f64::from(stream.avg_frame_rate());
    let declared = micros(stream.duration().max(0), base)?;
    let (audio_track, tracks) = audio_tracks(&input, audio_selected)?;
    let bytes = path.metadata().map_err(|e| e.to_string())?.len();
    let began = Instant::now();
    let budget = motion_media::timestamp_scan_budget(bytes);
    let mut timestamps = Vec::new();
    let mut last_sample = None;
    loop {
        check()?;
        if began.elapsed() > budget {
            return Err("video timestamp scan timed out".into());
        }
        let mut packet = Packet::empty();
        match packet.read(&mut input) {
            Ok(()) => {}
            Err(Error::Eof) => break,
            Err(e) => return Err(format!("read video index: {e}")),
        }
        if packet.stream() != track {
            continue;
        }
        let pts = packet
            .pts()
            .ok_or("video packet has no presentation timestamp")?;
        if pts < 0 {
            continue;
        }
        let time = micros(pts, base)?;
        timestamps.push(time);
        // Packets are in decode order (B frames), so retain the duration paired
        // with the largest presentation timestamp, not the last scanned packet.
        if last_sample.is_none_or(|(previous, _, _, _)| pts >= previous) {
            last_sample = Some((pts, packet.duration(), packet.position(), packet.size()));
        }
        if timestamps.len() > motion_core::MAX_VIDEO_FRAMES as usize {
            return Err("video timestamp cache budget exceeded".into());
        }
    }
    timestamps.sort_unstable();
    timestamps.dedup();
    let first = *timestamps.first().ok_or("video has no visible frames")?;
    let last = *timestamps.last().unwrap();
    let mut deltas: Vec<_> = timestamps.windows(2).map(|w| w[1] - w[0]).collect();
    deltas.sort_unstable();
    let step = deltas
        .get(deltas.len() / 2)
        .copied()
        .unwrap_or(if rate.is_finite() && rate > 0.0 {
            (1_000_000.0 / rate).round().max(1.0) as u64
        } else {
            33_333
        });
    let variable = deltas.iter().any(|d| d.abs_diff(step) > 2);
    let (pts, duration, position, packet_bytes) =
        last_sample.ok_or("video has no visible samples")?;
    let end = if mov {
        // Older MOV demuxers infer packet.duration from FPS when CTTS exists.
        // Neither a positive packet duration nor a decoded frame proves STTS.
        let timing = motion_media::mp4_sample_duration(
            path,
            u32::try_from(track_id).map_err(|_| "invalid MOV track identity")?,
            u64::try_from(position).map_err(|_| "MOV packet has no source position")?,
            packet_bytes,
            &|| {
                check()?;
                if began.elapsed() > budget {
                    Err("video timestamp scan timed out".into())
                } else {
                    Ok(())
                }
            },
        )?;
        timing.end_us(pts, base.numerator(), base.denominator())?
    } else if duration > 0 {
        micros(
            pts.checked_add(duration)
                .ok_or("video timestamp overflow")?,
            base,
        )?
    } else {
        motion_media::video_end_us(
            last,
            None,
            step,
            motion_media::VideoDuration::Span {
                start_us: first,
                duration_us: declared,
            },
        )?
    };
    let (display_width, display_height) = if rotation % 180 == 0 {
        (width, height)
    } else {
        (height, width)
    };
    let mut asset = VideoAsset {
        id: 1,
        path: "assets/probed-video.mp4".into(),
        bytes,
        mime,
        track: track as u32,
        width,
        height,
        rotation,
        display_width,
        display_height,
        video_start_us: first,
        video_end_us: end,
        duration_us: end,
        frame_count: timestamps.len() as u32,
        variable_frame_rate: variable,
        nominal_frame_rate: motion_media::source_frame_rate(rate, &timestamps, step, variable),
        color_standard: if width >= 1280 || height > 576 { 1 } else { 4 },
        color_range: 2,
        audio_asset: None,
    };
    asset.validate().map_err(|e| e.to_string())?;
    let mut decoder = Decoder::open(path, asset.clone(), timestamps.clone())?;
    let first_frame = decoder.decode_to(first, check)?;
    (asset.color_standard, asset.color_range) = colors(&first_frame)?;
    decoder.asset = asset.clone();
    let first_rgba = match convert(&first_frame, &asset)? {
        VideoPixels::Yuv(p) => p.to_rgba().map_err(|e| e.to_string())?,
        VideoPixels::Rgba(p) => p,
    };
    if timestamps.len() > 1 {
        decoder.frame(last, check)?;
    }
    Ok(VideoProbe {
        asset,
        timestamps,
        audio_track,
        first_rgba,
        tracks,
    })
}

pub struct Decoder {
    input: format::context::Input,
    video: codec::decoder::Video,
    base: Rational,
    asset: VideoAsset,
    timestamps: Vec<u64>,
    draining: bool,
    last_decoded: Option<i64>,
    seeks: u64,
}
impl Decoder {
    pub fn open(path: &Path, asset: VideoAsset, timestamps: Vec<u64>) -> Result<Self> {
        asset.validate().map_err(|e| e.to_string())?;
        if timestamps.len() != asset.frame_count as usize
            || timestamps.first() != Some(&asset.video_start_us)
            || timestamps.windows(2).any(|w| w[0] >= w[1])
            || timestamps.last().is_none_or(|p| *p >= asset.video_end_us)
        {
            return Err("invalid video timestamp index".into());
        }
        let input = input(path)?;
        let stream = video_stream(&input, Some(asset.track))?;
        let base = stream.time_base();
        let video = codec::context::Context::from_parameters(stream.parameters())
            .and_then(|c| c.decoder().video())
            .map_err(|e| e.to_string())?;
        Ok(Self {
            input,
            video,
            base,
            asset,
            timestamps,
            draining: false,
            last_decoded: None,
            seeks: 0,
        })
    }
    fn next(&mut self, check: &dyn Fn() -> Result<()>) -> Result<Option<frame::Video>> {
        loop {
            check()?;
            let mut frame = frame::Video::empty();
            match self.video.receive_frame(&mut frame) {
                Ok(()) => return Ok(Some(frame)),
                Err(Error::Eof) => return Ok(None),
                Err(Error::Other { errno }) if errno == av::util::error::EAGAIN => {}
                Err(e) => return Err(format!("decode video: {e}")),
            }
            if self.draining {
                return Err("video decoder did not drain at EOF".into());
            }
            let mut packet = Packet::empty();
            match packet.read(&mut self.input) {
                Ok(()) if packet.stream() == self.asset.track as usize => self
                    .video
                    .send_packet(&packet)
                    .map_err(|e| format!("submit video: {e}"))?,
                Ok(()) => {}
                Err(Error::Eof) => {
                    self.video.send_eof().map_err(|e| e.to_string())?;
                    self.draining = true;
                }
                Err(e) => return Err(format!("read video: {e}")),
            }
        }
    }
    fn decode_to(&mut self, wanted: u64, check: &dyn Fn() -> Result<()>) -> Result<frame::Video> {
        check()?;
        if video_decode_policy::needs_seek(self.last_decoded, wanted, None) {
            let target = i64::try_from(wanted).map_err(|_| "seek timestamp overflow")?;
            // Input::seek uses AV_TIME_BASE (microseconds) with stream index -1.
            self.input
                .seek(target, ..target)
                .map_err(|e| format!("seek video: {e}"))?;
            self.video.flush();
            self.draining = false;
            self.last_decoded = None;
            self.seeks += 1;
        }
        while let Some(frame) = self.next(check)? {
            let pts = frame
                .timestamp()
                .or(frame.pts())
                .ok_or("decoded frame has no timestamp")?;
            if pts < 0 {
                continue;
            }
            let time = micros(pts, self.base)?;
            if time > wanted {
                return Err("decoded frame does not match presentation index".into());
            }
            self.last_decoded = Some(i64::try_from(time).map_err(|_| "decoded timestamp overflow")?);
            if time == wanted {
                check()?;
                return Ok(frame);
            }
        }
        Err("video ended before requested frame".into())
    }
    pub fn frame(&mut self, target: u64, check: &dyn Fn() -> Result<()>) -> Result<DecodedFrame> {
        if target < self.asset.video_start_us || target >= self.asset.video_end_us {
            return Err("source time outside visible video".into());
        }
        let began = Instant::now();
        let index = self
            .timestamps
            .partition_point(|p| *p <= target)
            .saturating_sub(1);
        let wanted = self.timestamps[index];
        let decoded = self.decode_to(wanted, check)?;
        let codec_us = began.elapsed().as_micros() as u64;
        let packed = Instant::now();
        let pixels = convert(&decoded, &self.asset)?;
        check()?;
        Ok(DecodedFrame {
            pixels,
            pts: wanted,
            end: self
                .timestamps
                .get(index + 1)
                .copied()
                .unwrap_or(self.asset.video_end_us),
            width: self.asset.display_width,
            height: self.asset.display_height,
            decode_us: began.elapsed().as_micros() as u64,
            codec_us,
            transfer_us: 0,
            pack_us: packed.elapsed().as_micros() as u64,
            source_transfer: "libav-yuv",
            decoder_name: "libavcodec".into(),
        })
    }
}
impl VideoDecoder for Decoder {
    fn frame(&mut self, target_us: u64, check: &dyn Fn() -> Result<()>) -> Result<DecodedFrame> {
        Decoder::frame(self, target_us, check)
    }
    fn seeks(&self) -> u64 {
        self.seeks
    }
}
fn colors(frame: &frame::Video) -> Result<(u32, u32)> {
    use color::{Space, TransferCharacteristic as Transfer};
    if matches!(
        frame.color_transfer_characteristic(),
        Transfer::SMPTE2084 | Transfer::ARIB_STD_B67
    ) || matches!(frame.color_space(), Space::BT2020NCL | Space::BT2020CL)
    {
        return Err("HDR/BT.2020 video unsupported".into());
    }
    let standard = match frame.color_space() {
        Space::BT709 => 1,
        Space::BT470BG => 2,
        Space::SMPTE170M => 4,
        Space::Unspecified => {
            if frame.width() >= 1280 || frame.height() > 576 {
                1
            } else {
                4
            }
        }
        _ => return Err("unsupported SDR color matrix".into()),
    };
    let range =
        if frame.color_range() == color::Range::JPEG || frame.format() == format::Pixel::YUVJ420P {
            1
        } else {
            2
        };
    Ok((standard, range))
}
fn convert(frame: &frame::Video, asset: &VideoAsset) -> Result<VideoPixels> {
    if frame.width() != asset.width || frame.height() != asset.height {
        return Err("video changed dimensions".into());
    }
    let (standard, range) = colors(frame)?;
    if (standard, range) != (asset.color_standard, asset.color_range) {
        return Err("video changed color space".into());
    }
    // ffmpeg-next's data() constructs a slice using native stride/height. Check
    // every required plane before calling it, including the full padded span.
    let chroma_width = asset.width.div_ceil(2) as usize;
    let chroma_row_bytes = match frame.format() {
        format::Pixel::YUV420P | format::Pixel::YUVJ420P => chroma_width,
        format::Pixel::NV12 | format::Pixel::NV21 => chroma_width * 2,
        _ => {
            return Err(format!(
                "unsupported video pixels {:?}; requires 8-bit 4:2:0 SDR",
                frame.format()
            ));
        }
    };
    let y_stride = crate::frame_layout::checked_stride(frame, 0, asset.width as usize)?;
    let chroma_stride = crate::frame_layout::checked_stride(frame, 1, chroma_row_bytes)?;
    let v_stride = if matches!(frame.format(), format::Pixel::YUV420P | format::Pixel::YUVJ420P) {
        crate::frame_layout::checked_stride(frame, 2, chroma_row_bytes)?
    } else {
        chroma_stride
    };
    let (u, v, stride, pixel_stride) = match frame.format() {
        format::Pixel::YUV420P | format::Pixel::YUVJ420P if frame.planes() >= 3 => (
            frame.data(1),
            frame.data(2),
            [chroma_stride, v_stride],
            1,
        ),
        format::Pixel::NV12 | format::Pixel::NV21 if frame.planes() >= 2 => {
            let chroma = frame.data(1);
            let other = chroma.get(1..).ok_or("invalid interleaved video plane")?;
            let (u, v) = if frame.format() == format::Pixel::NV12 {
                (chroma, other)
            } else {
                (other, chroma)
            };
            (u, v, [chroma_stride; 2], 2)
        }
        _ => {
            return Err(format!(
                "unsupported video pixels {:?}; requires 8-bit 4:2:0 SDR",
                frame.format()
            ))
        }
    };
    Yuv420Frame::pack(
        asset.width,
        asset.height,
        [0, 0],
        asset.rotation,
        standard,
        range,
        [
            VideoPlane {
                data: frame.data(0),
                row_stride: y_stride,
                pixel_stride: 1,
            },
            VideoPlane {
                data: u,
                row_stride: stride[0],
                pixel_stride,
            },
            VideoPlane {
                data: v,
                row_stride: stride[1],
                pixel_stride,
            },
        ],
    )
    .map(VideoPixels::Yuv)
    .map_err(|e| e.to_string())
}

#[derive(Clone, Copy)]
pub struct EncoderSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}
impl EncoderSettings {
    pub fn bitrate(&self) -> i64 {
        (i64::from(self.width) * i64::from(self.height) * i64::from(self.fps) / 8)
            .clamp(2_000_000, 20_000_000)
    }
}
pub const OUTPUT_SAMPLE_RATE: u32 = 48_000;
pub const OUTPUT_LAYOUT: av::ChannelLayout = av::ChannelLayout::STEREO;

#[cfg(test)]
mod plane_conversion_tests {
    use super::*;

    fn fixture(pixel: format::Pixel, width: u32, height: u32) -> (frame::Video, VideoAsset) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../motion-media/tests/fixtures/video/silent-24fps.mp4");
        let mut asset = probe(&path, None, Some(u32::MAX), &|| Ok(()))
            .unwrap()
            .asset;
        let mut frame = frame::Video::new(pixel, width, height);
        frame.set_color_space(color::Space::SMPTE170M);
        frame.set_color_range(color::Range::MPEG);
        for index in 0..frame.planes() {
            let stride = frame.stride(index);
            let data = frame.data_mut(index);
            data.fill(0xa5);
            for (y, row) in data.chunks_exact_mut(stride).enumerate() {
                if index == 0 {
                    for (x, value) in row[..width as usize].iter_mut().enumerate() {
                        *value = (30 + x + y * 3) as u8;
                    }
                } else if matches!(pixel, format::Pixel::NV12 | format::Pixel::NV21) {
                    let uv = if pixel == format::Pixel::NV12 {
                        [73, 181]
                    } else {
                        [181, 73]
                    };
                    for pair in row[..width.div_ceil(2) as usize * 2].chunks_exact_mut(2) {
                        pair.copy_from_slice(&uv);
                    }
                } else {
                    row[..width.div_ceil(2) as usize].fill(if index == 1 { 73 } else { 181 });
                }
            }
        }
        asset.width = width;
        asset.height = height;
        asset.display_width = width;
        asset.display_height = height;
        (asset.color_standard, asset.color_range) = colors(&frame).unwrap();
        (frame, asset)
    }

    #[test]
    fn planar_and_interleaved_pixels_remain_equal_for_odd_sizes_and_rotations() {
        for (width, height) in [(18, 10), (17, 9)] {
            for rotation in [0, 90, 180, 270] {
                let mut reference = None;
                for pixel in [
                    format::Pixel::YUV420P,
                    format::Pixel::NV12,
                    format::Pixel::NV21,
                ] {
                    let (frame, mut asset) = fixture(pixel, width, height);
                    asset.rotation = rotation;
                    let converted = convert(&frame, &asset).unwrap();
                    let VideoPixels::Yuv(yuv) = converted else {
                        panic!("expected packed YUV")
                    };
                    assert_eq!(yuv.rotation, rotation);
                    let expected_y: Vec<_> = (0..height)
                        .flat_map(|y| (0..width).map(move |x| (30 + x + y * 3) as u8))
                        .collect();
                    assert_eq!(yuv.y, expected_y);
                    let cw = width.div_ceil(2) as usize;
                    let row = match pixel {
                        format::Pixel::YUV420P => [vec![73; cw], vec![181; cw]].concat(),
                        format::Pixel::NV12 => [73, 181].repeat(cw),
                        format::Pixel::NV21 => [181, 73].repeat(cw),
                        _ => unreachable!(),
                    };
                    assert_eq!(yuv.uv, row.repeat(height.div_ceil(2) as usize));
                    let rgba = yuv.to_rgba().unwrap();
                    assert_eq!(rgba.len(), (width * height * 4) as usize);
                    if let Some(expected) = &reference {
                        assert_eq!(&rgba, expected);
                    } else {
                        reference = Some(rgba);
                    }
                }
            }
        }
    }

    #[test]
    fn decoded_plane_errors_are_returned_by_the_conversion_entry_point() {
        let (mut frame, asset) = fixture(format::Pixel::YUV420P, 18, 10);
        for index in 0..3 {
            // SAFETY: Modify metadata on the owned frame and restore it before
            // inspecting the result. convert must reject it before data().
            let stride = frame.stride(index);
            unsafe {
                (*frame.as_mut_ptr()).linesize[index] = -1;
            }
            let result = convert(&frame, &asset);
            unsafe {
                (*frame.as_mut_ptr()).linesize[index] = stride as i32;
            }
            let error = result
                .err()
                .expect("conversion must reject negative stride");
            assert!(error.contains(&format!("video plane {index}:")), "{error}");

            let pointer = unsafe { (*frame.as_ptr()).data[index] };
            unsafe {
                (*frame.as_mut_ptr()).data[index] = std::ptr::null_mut();
            }
            let result = convert(&frame, &asset);
            unsafe {
                (*frame.as_mut_ptr()).data[index] = pointer;
            }
            let error = result.err().expect("conversion must reject null data");
            assert!(error.contains(&format!("video plane {index}:")), "{error}");
        }
        assert!(convert(&frame, &asset).is_ok());
    }
}

#[cfg(test)]
mod timestamp_arithmetic_tests {
    use super::*;
    #[test]
    fn extreme_timestamps_are_rejected_or_rescaled_without_wrapping() {
        assert_eq!(micros(0, Rational(1, 1)).unwrap(), 0);
        assert_eq!(
            micros(i64::MAX, Rational(1, 1_000_000)).unwrap(),
            i64::MAX as u64
        );
        assert_eq!(micros(1, Rational(1, 3)).unwrap(), 333333);
        assert_eq!(micros(2, Rational(1, 3)).unwrap(), 666667);
        assert!(micros(i64::MAX, Rational(i32::MAX, 1)).is_err());
        assert!(micros(-1, Rational(1, 1)).is_err());
        assert!(micros(1, Rational(0, 1)).is_err());
        assert!(micros(1, Rational(1, 0)).is_err());
    }
}

#[cfg(test)]
mod metadata_failure_tests {
    use super::*;
    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../motion-media/tests/fixtures/video").join(name)
    }
    fn set_rotation(input: &mut format::context::Input, index: usize, text: &str) {
        let mut metadata = av::Dictionary::new();
        metadata.set("rotate", text);
        input.stream_mut(index).unwrap().set_metadata(metadata);
    }

    #[test]
    fn malformed_rotation_is_rejected_without_overriding_a_valid_display_matrix() {
        let mut plain = input(&fixture("silent-24fps.mp4")).unwrap();
        let index = video_stream(&plain, None).unwrap().index();
        assert_eq!(rotation(&video_stream(&plain, None).unwrap()).unwrap(), 0);
        set_rotation(&mut plain, index, "broken");
        let error = rotation(&video_stream(&plain, None).unwrap()).unwrap_err();
        assert!(error.contains("invalid video rotate metadata") && error.contains("broken"), "{error}");
        for text in ["NaN", "inf", "45", ""] {
            set_rotation(&mut plain, index, text);
            assert!(rotation(&video_stream(&plain, None).unwrap()).is_err(), "{text}");
        }
        for (text, expected) in [("-90", 270), ("450", 90), ("360", 0), ("270", 270)] {
            set_rotation(&mut plain, index, text);
            assert_eq!(rotation(&video_stream(&plain, None).unwrap()).unwrap(), expected, "{text}");
        }

        let mut matrix = input(&fixture("rotated-90.mp4")).unwrap();
        let index = video_stream(&matrix, None).unwrap().index();
        set_rotation(&mut matrix, index, "broken");
        assert_eq!(rotation(&video_stream(&matrix, None).unwrap()).unwrap(), 90);
    }

    #[test]
    fn unsupported_audio_retains_decoder_cause_in_metadata_and_selection_error() {
        let mut media = input(&fixture("sound-24fps.mp4")).unwrap();
        let index = media.streams().best(media::Type::Audio).unwrap().index();
        let mut parameters = media.stream(index).unwrap().parameters().clone();
        parameters.set_id(codec::Id::None);
        media.stream_mut(index).unwrap().set_parameters(parameters);
        let (_, tracks) = audio_tracks(&media, Some(u32::MAX)).unwrap();
        let error = tracks[0]["error"].as_str().expect("decoder failure cause");
        assert!(!error.is_empty());
        assert_eq!(tracks[0]["supported"], false);
        let selection = audio_tracks(&media, None).unwrap_err();
        assert!(selection.contains(error) && selection.contains(&format!("track {index}")), "{selection}");
    }
}
