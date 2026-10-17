//! H.264/AAC MP4 encoding of rendered RGBA and the shared 48 kHz stereo mix.
use ffmpeg_next::{
    self as av, codec, color, encoder, format, frame, software, Dictionary, Error, Packet, Rational,
};
use motion_host::Result;
use std::path::Path;

pub fn available() -> bool {
    av::init().is_ok()
        && encoder::find_by_name("aac").is_some()
        && (encoder::find_by_name("libopenh264").is_some()
            || encoder::find_by_name("libx264").is_some()
            || encoder::find(codec::Id::H264).is_some())
}

pub struct Mp4Encoder {
    output: format::context::Output,
    video: encoder::Video,
    audio: encoder::Audio,
    scaler: software::scaling::Context,
    video_base: Rational,
    audio_base: Rational,
    video_stream_base: Rational,
    audio_stream_base: Rational,
    width: u32,
    height: u32,
    pub video_encoder: String,
    video_frames: u64,
    audio_samples: u64,
}

impl Mp4Encoder {
    pub fn new(path: &Path, width: u32, height: u32, fps: u32) -> Result<Self> {
        av::init().map_err(|e| e.to_string())?;
        if width == 0
            || height == 0
            || width > motion_core::MAX_VIDEO_DIMENSION
            || height > motion_core::MAX_VIDEO_DIMENSION
            || width % 2 != 0
            || height % 2 != 0
            || !(1..=240).contains(&fps)
        {
            return Err(format!(
                "H.264 export requires positive even dimensions up to {} and 1–240 fps",
                motion_core::MAX_VIDEO_DIMENSION
            ));
        }
        // LGPL distributions commonly provide OpenH264. Never require GPL x264.
        let video_codec = encoder::find_by_name("libopenh264")
            .or_else(|| encoder::find_by_name("libx264"))
            .or_else(|| encoder::find(codec::Id::H264))
            .ok_or("this FFmpeg build has no H.264 encoder")?;
        let audio_codec =
            encoder::find_by_name("aac").ok_or("this FFmpeg build has no AAC encoder")?;
        let video_encoder = video_codec.name().to_owned();
        let mut output =
            format::output_as(path, "mp4").map_err(|e| format!("open MP4 output: {e}"))?;
        let global_header = output
            .format()
            .flags()
            .contains(format::Flags::GLOBAL_HEADER);
        let video_base = Rational(1, fps as i32);
        let audio_base = Rational(1, 48_000);
        let mut video = codec::context::Context::new_with_codec(video_codec)
            .encoder()
            .video()
            .map_err(|e| e.to_string())?;
        video.set_width(width);
        video.set_height(height);
        video.set_format(format::Pixel::YUV420P);
        video.set_colorspace(color::Space::BT709);
        video.set_color_range(color::Range::MPEG);
        video.set_color_primaries(color::Primaries::BT709);
        video.set_color_transfer_characteristic(color::TransferCharacteristic::IEC61966_2_1);
        video.set_time_base(video_base);
        video.set_frame_rate(Some(Rational(fps as i32, 1)));
        video.set_aspect_ratio(Rational(1, 1));
        video.set_bit_rate(
            (u64::from(width) * u64::from(height) * u64::from(fps) / 8).clamp(2_000_000, 20_000_000)
                as usize,
        );
        video.set_gop(fps * 2);
        video.set_max_b_frames(0);
        if global_header {
            video.set_flags(codec::Flags::GLOBAL_HEADER);
        }
        let video = video
            .open_as(video_codec)
            .map_err(|e| format!("open {video_encoder} encoder: {e}"))?;
        {
            let mut stream = output.add_stream(video_codec).map_err(|e| e.to_string())?;
            stream.set_time_base(video_base);
            stream.set_rate(Rational(fps as i32, 1));
            stream.set_parameters(&video);
        }
        let mut audio = codec::context::Context::new_with_codec(audio_codec)
            .encoder()
            .audio()
            .map_err(|e| e.to_string())?;
        audio.set_rate(48_000);
        audio.set_channel_layout(av::ChannelLayout::STEREO);
        audio.set_format(format::Sample::F32(format::sample::Type::Planar));
        audio.set_time_base(audio_base);
        audio.set_bit_rate(192_000);
        if global_header {
            audio.set_flags(codec::Flags::GLOBAL_HEADER);
        }
        let audio = audio
            .open_as(audio_codec)
            .map_err(|e| format!("open AAC encoder: {e}"))?;
        {
            let mut stream = output.add_stream(audio_codec).map_err(|e| e.to_string())?;
            stream.set_time_base(audio_base);
            stream.set_parameters(&audio);
        }
        let mut options = Dictionary::new();
        options.set("movflags", "+faststart");
        output
            .write_header_with(options)
            .map_err(|e| format!("write MP4 header: {e}"))?;
        let video_stream_base = output
            .stream(0)
            .ok_or("missing output video stream")?
            .time_base();
        let audio_stream_base = output
            .stream(1)
            .ok_or("missing output audio stream")?
            .time_base();
        let mut scaler = software::scaling::Context::get(
            format::Pixel::RGBA,
            width,
            height,
            format::Pixel::YUV420P,
            width,
            height,
            software::scaling::Flags::BILINEAR,
        )
        .map_err(|e| e.to_string())?;
        // swscale defaults to a BT.601 matrix even at HD dimensions. Explicitly
        // select the same BT.709 limited-range matrix signalled in the stream.
        let color_result = unsafe {
            let coefficients = av::ffi::sws_getCoefficients(av::ffi::SWS_CS_ITU709);
            av::ffi::sws_setColorspaceDetails(
                scaler.as_mut_ptr(),
                coefficients,
                1,
                coefficients,
                0,
                0,
                1 << 16,
                1 << 16,
            )
        };
        if color_result < 0 {
            return Err("cannot configure export color conversion".into());
        }
        Ok(Self {
            output,
            video,
            audio,
            scaler,
            video_base,
            audio_base,
            video_stream_base,
            audio_stream_base,
            width,
            height,
            video_encoder,
            video_frames: 0,
            audio_samples: 0,
        })
    }

    pub fn audio_frame_size(&self) -> usize {
        self.audio.frame_size() as usize
    }

    pub fn video_frame(&mut self, rgba: &[u8], index: u32) -> Result<()> {
        if rgba.len() != self.width as usize * self.height as usize * 4
            || u64::from(index) != self.video_frames
        {
            return Err("invalid RGBA frame or nonsequential export frame".into());
        }
        let mut source = frame::Video::new(format::Pixel::RGBA, self.width, self.height);
        write_rgba(&mut source, rgba)?;
        let mut yuv = frame::Video::empty();
        self.scaler
            .run(&source, &mut yuv)
            .map_err(|e| format!("convert export pixels: {e}"))?;
        yuv.set_pts(Some(i64::from(index)));
        yuv.set_color_space(color::Space::BT709);
        yuv.set_color_range(color::Range::MPEG);
        yuv.set_color_primaries(color::Primaries::BT709);
        yuv.set_color_transfer_characteristic(color::TransferCharacteristic::IEC61966_2_1);
        self.video
            .send_frame(&yuv)
            .map_err(|e| format!("encode H.264 frame: {e}"))?;
        drain(
            &mut self.video,
            &mut self.output,
            0,
            self.video_base,
            self.video_stream_base,
        )?;
        self.video_frames += 1;
        Ok(())
    }

    pub fn audio_frame(&mut self, stereo: &[f32], start_sample: u64) -> Result<()> {
        let samples = stereo.len() / 2;
        if stereo.len() % 2 != 0
            || samples == 0
            || samples > self.audio_frame_size()
            || start_sample != self.audio_samples
            || stereo.iter().any(|s| !s.is_finite())
        {
            return Err("invalid AAC source block or nonsequential audio samples".into());
        }
        let mut pcm = frame::Audio::new(self.audio.format(), samples, av::ChannelLayout::STEREO);
        pcm.set_rate(48_000);
        pcm.set_pts(Some(start_sample as i64));
        for channel in 0..2 {
            for (target, source) in pcm
                .plane_mut::<f32>(channel)
                .iter_mut()
                .zip(stereo.chunks_exact(2))
            {
                *target = source[channel];
            }
        }
        self.audio
            .send_frame(&pcm)
            .map_err(|e| format!("encode AAC block: {e}"))?;
        drain(
            &mut self.audio,
            &mut self.output,
            1,
            self.audio_base,
            self.audio_stream_base,
        )?;
        self.audio_samples += samples as u64;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        if self.video_frames == 0 || self.audio_samples == 0 {
            return Err("cannot finish empty movie".into());
        }
        self.video.send_eof().map_err(|e| e.to_string())?;
        drain(
            &mut self.video,
            &mut self.output,
            0,
            self.video_base,
            self.video_stream_base,
        )?;
        self.audio.send_eof().map_err(|e| e.to_string())?;
        drain(
            &mut self.audio,
            &mut self.output,
            1,
            self.audio_base,
            self.audio_stream_base,
        )?;
        self.output
            .write_trailer()
            .map_err(|e| format!("finalize MP4: {e}"))?;
        Ok(())
    }
}

/// Initialize the complete padded native plane before passing it to swscale.
fn write_rgba(source: &mut frame::Video, rgba: &[u8]) -> Result<()> {
    let row_bytes = (source.width() as usize)
        .checked_mul(4)
        .ok_or("export RGBA row size overflow")?;
    let bytes = row_bytes
        .checked_mul(source.height() as usize)
        .ok_or("export RGBA frame size overflow")?;
    if source.format() != format::Pixel::RGBA || rgba.len() != bytes {
        return Err("invalid RGBA export source".into());
    }
    let stride = crate::frame_layout::checked_stride(source, 0, row_bytes)?;
    let data = source.data_mut(0);
    data.fill(0);
    for (row, input) in data
        .chunks_exact_mut(stride)
        .zip(rgba.chunks_exact(row_bytes))
    {
        row[..row_bytes].copy_from_slice(input);
        // MP4 has no alpha: composite straight-alpha capture pixels onto black.
        for pixel in row[..row_bytes].chunks_exact_mut(4) {
            if pixel[3] != 255 {
                let alpha = f32::from(pixel[3]) / 255.0;
                for color in &mut pixel[..3] {
                    let s = f32::from(*color) / 255.0;
                    let l = if s <= 0.04045 {
                        s / 12.92
                    } else {
                        ((s + 0.055) / 1.055).powf(2.4)
                    } * alpha;
                    let s = if l <= 0.0031308 {
                        l * 12.92
                    } else {
                        1.055 * l.powf(1.0 / 2.4) - 0.055
                    };
                    *color = (s * 255.0).round() as u8;
                }
                pixel[3] = 255;
            }
        }
    }
    Ok(())
}

fn drain(
    encoder: &mut codec::encoder::Encoder,
    output: &mut format::context::Output,
    stream: usize,
    source_base: Rational,
    target_base: Rational,
) -> Result<()> {
    loop {
        let mut packet = Packet::empty();
        match encoder.receive_packet(&mut packet) {
            Ok(()) => {
                packet.set_stream(stream);
                // This encoder emits CFR video at one frame per source tick.
                // Some older codecs leave duration unset even on flush; the
                // muxer cannot infer the final sample from a following DTS.
                if encoder.medium() == av::media::Type::Video && packet.duration() <= 0 {
                    packet.set_duration(1);
                }
                packet.rescale_ts(source_base, target_base);
                packet
                    .write_interleaved(output)
                    .map_err(|e| format!("mux MP4 packet: {e}"))?;
            }
            Err(Error::Eof) => return Ok(()),
            Err(Error::Other { errno }) if errno == av::util::error::EAGAIN => return Ok(()),
            Err(e) => return Err(format!("read encoded packet: {e}")),
        }
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn rgba_export_initializes_row_padding_and_preserves_opaque_pixels() {
        let mut frame = frame::Video::new(format::Pixel::RGBA, 18, 4);
        let stride = frame.stride(0);
        assert!(stride > 18 * 4, "fixture must include native row padding");
        frame.data_mut(0).fill(0xa5);
        let rgba: Vec<_> = (0..18 * 4).flat_map(|i| [i as u8, 80, 160, 255]).collect();
        write_rgba(&mut frame, &rgba).unwrap();
        for (row, input) in frame
            .data(0)
            .chunks_exact(stride)
            .zip(rgba.chunks_exact(72))
        {
            assert_eq!(&row[..72], input);
            assert!(row[72..].iter().all(|byte| *byte == 0));
        }
    }

    #[test]
    fn rgba_export_keeps_linear_alpha_compositing_and_clears_padding_each_time() {
        let mut frame = frame::Video::new(format::Pixel::RGBA, 2, 2);
        let stride = frame.stride(0);
        let rgba = [
            255, 0, 0, 128, 50, 100, 150, 0, 10, 20, 30, 255, 0, 0, 0, 255,
        ];
        for _ in 0..2 {
            frame.data_mut(0).fill(0xa5);
            write_rgba(&mut frame, &rgba).unwrap();
            assert_eq!(&frame.data(0)[..8], &[188, 0, 0, 255, 0, 0, 0, 255]);
            assert_eq!(
                &frame.data(0)[stride..stride + 8],
                &[10, 20, 30, 255, 0, 0, 0, 255]
            );
            assert!(frame
                .data(0)
                .chunks_exact(stride)
                .all(|row| row[8..].iter().all(|b| *b == 0)));
        }
    }

    #[test]
    fn invalid_rgba_input_or_native_layout_returns_error_without_writes() {
        let mut frame = frame::Video::new(format::Pixel::RGBA, 18, 4);
        frame.data_mut(0).fill(0xa5);
        let before = frame.data(0).to_vec();
        assert!(write_rgba(&mut frame, &[0; 4]).is_err());
        assert_eq!(frame.data(0), before);
        let stride = frame.stride(0);
        // SAFETY: Restore the owned metadata before reading the plane again.
        unsafe {
            (*frame.as_mut_ptr()).linesize[0] = -1;
        }
        let result = write_rgba(&mut frame, &[0; 18 * 4 * 4]);
        unsafe {
            (*frame.as_mut_ptr()).linesize[0] = stride as i32;
        }
        assert!(result
            .unwrap_err()
            .contains("video plane 0: invalid row stride"));
        assert_eq!(frame.data(0), before);
    }
}
