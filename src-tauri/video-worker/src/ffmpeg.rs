use std::{
    ffi::{CStr, CString},
    hash::{Hash, Hasher},
    mem,
    os::raw::{c_char, c_int},
    path::Path,
    ptr,
};

use ffmpeg_sys_next as ffi;
use serde::{Deserialize, Serialize};

use crate::{FrameMeta, SessionSummary};

/// FFmpeg AVERROR(e) = -e on POSIX. On Windows with MSVC, EAGAIN = 11.
pub const AVERROR_EAGAIN: c_int = -11;

/// AVERROR_EOF = FFERRTAG('E','O','F',' ') with sign flip.
pub const AVERROR_EOF: c_int = -(0x45 | (0x4F << 8) | (0x46 << 16) | (0x20 << 24));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkedFfmpegVersions {
    pub avcodec: u32,
    pub avformat: u32,
    pub avutil: u32,
    pub swscale: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VideoProbe {
    pub path: String,
    pub video_stream_index: usize,
    pub width: u32,
    pub height: u32,
    pub duration_us: Option<i64>,
    pub codec_name: String,
    pub bit_rate: Option<i64>,
    pub avg_frame_rate_num: i32,
    pub avg_frame_rate_den: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
pub struct DecoderSession {
    summary: SessionSummary,
    decoder: VideoDecoder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SeekAnchor {
    index: u32,
    pts: Option<i64>,
}

pub fn linked_versions() -> LinkedFfmpegVersions {
    LinkedFfmpegVersions {
        avcodec: unsafe { ffi::avcodec_version() },
        avformat: unsafe { ffi::avformat_version() },
        avutil: unsafe { ffi::avutil_version() },
        swscale: unsafe { ffi::swscale_version() },
    }
}

pub fn probe_video(path: &Path) -> Result<VideoProbe, String> {
    let path_string = path.to_string_lossy().into_owned();
    let c_path = CString::new(path_string.clone())
        .map_err(|_| format!("video path contains interior NUL byte: {}", path.display()))?;
    let mut format_context = ptr::null_mut::<ffi::AVFormatContext>();

    let result = unsafe {
        let open_result = ffi::avformat_open_input(
            &mut format_context,
            c_path.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
        if open_result < 0 {
            return Err(ffmpeg_error("avformat_open_input", open_result));
        }

        let stream_info_result = ffi::avformat_find_stream_info(format_context, ptr::null_mut());
        if stream_info_result < 0 {
            return Err(ffmpeg_error("avformat_find_stream_info", stream_info_result));
        }

        let stream_count = (*format_context).nb_streams as usize;
        let streams = std::slice::from_raw_parts((*format_context).streams, stream_count);

        let (video_stream_index, stream) = streams
            .iter()
            .enumerate()
            .find_map(|(index, &stream)| {
                if stream.is_null() {
                    return None;
                }
                let codecpar = (*stream).codecpar;
                if codecpar.is_null() {
                    return None;
                }
                if (*codecpar).codec_type == ffi::AVMediaType::AVMEDIA_TYPE_VIDEO {
                    Some((index, stream))
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("no video stream found in {}", path.display()))?;

        let codecpar = (*stream).codecpar;
        let codec_name = c_string_from_ptr(ffi::avcodec_get_name((*codecpar).codec_id))
            .unwrap_or_else(|| "unknown".to_string());
        let duration_us =
            context_duration_us(format_context).or_else(|| stream_duration_us(stream));
        let avg_frame_rate = (*stream).avg_frame_rate;
        let bit_rate = positive_i64((*codecpar).bit_rate);

        Ok(VideoProbe {
            path: path_string,
            video_stream_index,
            width: (*codecpar).width.max(0) as u32,
            height: (*codecpar).height.max(0) as u32,
            duration_us,
            codec_name,
            bit_rate,
            avg_frame_rate_num: avg_frame_rate.num,
            avg_frame_rate_den: avg_frame_rate.den,
        })
    };

    unsafe {
        if !format_context.is_null() {
            ffi::avformat_close_input(&mut format_context);
        }
    }

    result
}

pub fn open_video_session(path: &Path) -> Result<SessionSummary, String> {
    Ok(open_decoder_session(path)?.into_summary())
}

pub fn open_decoder_session(path: &Path) -> Result<DecoderSession, String> {
    let path_string = path.to_string_lossy().into_owned();
    let mut decoder = VideoDecoder::open(path)?;
    let summary = decoder.build_frame_index(&path_string)?;
    decoder.seek_to_start()?;
    Ok(DecoderSession { summary, decoder })
}

pub fn decode_video_frame(
    path: &Path,
    frames: &[FrameMeta],
    target_index: u32,
) -> Result<DecodedFrame, String> {
    if target_index as usize >= frames.len() {
        return Err(format!(
            "frame {target_index} out of range for {} indexed frames",
            frames.len()
        ));
    }

    let anchor = find_seek_anchor(frames, target_index);
    if anchor.index > 0 {
        let mut decoder = VideoDecoder::open(path)?;
        if let Ok(decoded) = decoder.decode_from_anchor(target_index, anchor) {
            return Ok(decoded);
        }
    }

    let mut decoder = VideoDecoder::open(path)?;
    decoder.decode_from_anchor(
        target_index,
        SeekAnchor {
            index: 0,
            pts: None,
        },
    )
}

impl DecoderSession {
    pub fn summary(&self) -> &SessionSummary {
        &self.summary
    }

    pub fn into_summary(self) -> SessionSummary {
        self.summary
    }

    pub fn decode_frame(&mut self, target_index: u32) -> Result<DecodedFrame, String> {
        if target_index as usize >= self.summary.frames.len() {
            return Err(format!(
                "frame {target_index} out of range for {} indexed frames",
                self.summary.frames.len()
            ));
        }

        let anchor = find_seek_anchor(&self.summary.frames, target_index);
        self.decoder.decode_from_anchor(target_index, anchor)
    }
}

fn find_seek_anchor(frames: &[FrameMeta], target_index: u32) -> SeekAnchor {
    frames
        .iter()
        .take(target_index as usize + 1)
        .enumerate()
        .rev()
        .find_map(|(index, frame)| {
            if frame.is_keyframe && frame.pts.is_some() {
                Some(SeekAnchor {
                    index: index as u32,
                    pts: frame.pts,
                })
            } else {
                None
            }
        })
        .unwrap_or(SeekAnchor {
            index: 0,
            pts: None,
        })
}

#[derive(Debug)]
struct VideoDecoder {
    format_ctx: *mut ffi::AVFormatContext,
    codec_ctx: *mut ffi::AVCodecContext,
    packet: *mut ffi::AVPacket,
    frame: *mut ffi::AVFrame,
    stream: *mut ffi::AVStream,
    stream_index: usize,
}

// SAFETY: The decoder owns FFmpeg pointers and is only accessed through mutable references.
// In the worker it is additionally protected by the global WorkerServer mutex, so it is never
// used concurrently across threads.
unsafe impl Send for VideoDecoder {}

impl VideoDecoder {
    fn open(path: &Path) -> Result<Self, String> {
        let path_string = path.to_string_lossy().into_owned();
        let c_path = CString::new(path_string)
            .map_err(|_| format!("video path contains interior NUL byte: {}", path.display()))?;

        unsafe {
            let mut format_ctx = ptr::null_mut::<ffi::AVFormatContext>();
            let open_result = ffi::avformat_open_input(
                &mut format_ctx,
                c_path.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            if open_result < 0 {
                return Err(ffmpeg_error("avformat_open_input", open_result));
            }

            let stream_info_result = ffi::avformat_find_stream_info(format_ctx, ptr::null_mut());
            if stream_info_result < 0 {
                ffi::avformat_close_input(&mut format_ctx);
                return Err(ffmpeg_error("avformat_find_stream_info", stream_info_result));
            }

            let stream_count = (*format_ctx).nb_streams as usize;
            let streams = std::slice::from_raw_parts((*format_ctx).streams, stream_count);
            let (stream_index, stream) = streams
                .iter()
                .enumerate()
                .find_map(|(index, &stream)| {
                    if stream.is_null() {
                        return None;
                    }
                    let codecpar = (*stream).codecpar;
                    if codecpar.is_null() {
                        return None;
                    }
                    if (*codecpar).codec_type == ffi::AVMediaType::AVMEDIA_TYPE_VIDEO {
                        Some((index, stream))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| format!("no video stream found in {}", path.display()))?;

            let codecpar = (*stream).codecpar;
            let decoder = ffi::avcodec_find_decoder((*codecpar).codec_id);
            if decoder.is_null() {
                ffi::avformat_close_input(&mut format_ctx);
                return Err(format!(
                    "no decoder found for codec {}",
                    c_string_from_ptr(ffi::avcodec_get_name((*codecpar).codec_id))
                        .unwrap_or_else(|| "unknown".to_string())
                ));
            }

            let mut codec_ctx = ffi::avcodec_alloc_context3(decoder);
            if codec_ctx.is_null() {
                ffi::avformat_close_input(&mut format_ctx);
                return Err("failed to allocate codec context".to_string());
            }

            let params_result = ffi::avcodec_parameters_to_context(codec_ctx, codecpar);
            if params_result < 0 {
                ffi::avcodec_free_context(&mut codec_ctx);
                ffi::avformat_close_input(&mut format_ctx);
                return Err(ffmpeg_error("avcodec_parameters_to_context", params_result));
            }

            let open_decoder_result = ffi::avcodec_open2(codec_ctx, decoder, ptr::null_mut());
            if open_decoder_result < 0 {
                ffi::avcodec_free_context(&mut codec_ctx);
                ffi::avformat_close_input(&mut format_ctx);
                return Err(ffmpeg_error("avcodec_open2", open_decoder_result));
            }

            let mut packet = ffi::av_packet_alloc();
            let mut frame = ffi::av_frame_alloc();
            if packet.is_null() || frame.is_null() {
                ffi::av_frame_free(&mut frame);
                ffi::av_packet_free(&mut packet);
                ffi::avcodec_free_context(&mut codec_ctx);
                ffi::avformat_close_input(&mut format_ctx);
                return Err("failed to allocate packet/frame".to_string());
            }

            Ok(Self {
                format_ctx,
                codec_ctx,
                packet,
                frame,
                stream,
                stream_index,
            })
        }
    }

    fn build_frame_index(&mut self, path: &str) -> Result<SessionSummary, String> {
        let codecpar = self.codecpar();
        let time_base = unsafe { (*self.stream).time_base };

        let mut frames = Vec::new();
        let mut frame_ordinal: u32 = 0;
        let mut decode_errors: u32 = 0;

        loop {
            let read_result = unsafe { ffi::av_read_frame(self.format_ctx, self.packet) };
            if read_result < 0 {
                unsafe {
                    ffi::avcodec_send_packet(self.codec_ctx, ptr::null());
                }
                decode_errors += self.drain_indexed_frames(&mut frames, &mut frame_ordinal, time_base);
                break;
            }

            if unsafe { (*self.packet).stream_index as usize } != self.stream_index {
                unsafe {
                    ffi::av_packet_unref(self.packet);
                }
                continue;
            }

            let send_result = unsafe { ffi::avcodec_send_packet(self.codec_ctx, self.packet) };
            unsafe {
                ffi::av_packet_unref(self.packet);
            }
            if send_result < 0 {
                decode_errors += 1;
                eprintln!(
                    "video-worker: avcodec_send_packet failed on packet {}: {}",
                    frame_ordinal + decode_errors,
                    ffmpeg_error_message(send_result),
                );
                continue;
            }

            decode_errors +=
                self.drain_indexed_frames(&mut frames, &mut frame_ordinal, time_base);
        }

        if frames.is_empty() {
            return Err(format!(
                "no frames decoded from {path} ({decode_errors} decode error(s))"
            ));
        }

        let avg_frame_rate = unsafe { (*self.stream).avg_frame_rate };
        Ok(SessionSummary {
            session_id: make_session_id(path),
            path: path.to_string(),
            width: unsafe { (*codecpar).width.max(0) as u32 },
            height: unsafe { (*codecpar).height.max(0) as u32 },
            duration_us: context_duration_us(self.format_ctx).or_else(|| stream_duration_us(self.stream)),
            codec_name: c_string_from_ptr(unsafe { ffi::avcodec_get_name((*codecpar).codec_id) })
                .unwrap_or_else(|| "unknown".to_string()),
            total_frames: frames.len() as u32,
            avg_frame_rate_num: avg_frame_rate.num,
            avg_frame_rate_den: avg_frame_rate.den,
            decode_errors,
            frames,
        })
    }

    fn decode_from_anchor(
        &mut self,
        target_index: u32,
        anchor: SeekAnchor,
    ) -> Result<DecodedFrame, String> {
        if let Some(pts) = anchor.pts {
            self.seek_to_pts(pts)?;
        } else {
            self.seek_to_start()?;
        }

        let mut frame_ordinal = anchor.index;
        let mut expected_anchor_pts = if anchor.index > 0 { anchor.pts } else { None };
        self.decode_until_target(target_index, &mut frame_ordinal, &mut expected_anchor_pts)
    }

    fn seek_to_start(&mut self) -> Result<(), String> {
        let seek_result = unsafe {
            ffi::av_seek_frame(
                self.format_ctx,
                self.stream_index as c_int,
                0,
                ffi::AVSEEK_FLAG_BACKWARD,
            )
        };
        if seek_result < 0 {
            return Err(ffmpeg_error("av_seek_frame", seek_result));
        }

        unsafe {
            ffi::avcodec_flush_buffers(self.codec_ctx);
        }
        Ok(())
    }

    fn seek_to_pts(&mut self, pts: i64) -> Result<(), String> {
        let seek_result = unsafe {
            ffi::av_seek_frame(
                self.format_ctx,
                self.stream_index as c_int,
                pts,
                ffi::AVSEEK_FLAG_BACKWARD,
            )
        };
        if seek_result < 0 {
            return Err(ffmpeg_error("av_seek_frame", seek_result));
        }

        unsafe {
            ffi::avcodec_flush_buffers(self.codec_ctx);
        }
        Ok(())
    }

    fn decode_until_target(
        &mut self,
        target_index: u32,
        frame_ordinal: &mut u32,
        expected_anchor_pts: &mut Option<i64>,
    ) -> Result<DecodedFrame, String> {
        loop {
            let read_result = unsafe { ffi::av_read_frame(self.format_ctx, self.packet) };
            if read_result < 0 {
                let flush_result = unsafe { ffi::avcodec_send_packet(self.codec_ctx, ptr::null()) };
                if flush_result < 0 {
                    return Err(ffmpeg_error("avcodec_send_packet", flush_result));
                }

                if let Some(decoded) =
                    self.drain_until_target(target_index, frame_ordinal, expected_anchor_pts)?
                {
                    return Ok(decoded);
                }
                break;
            }

            if unsafe { (*self.packet).stream_index as usize } != self.stream_index {
                unsafe {
                    ffi::av_packet_unref(self.packet);
                }
                continue;
            }

            let send_result = unsafe { ffi::avcodec_send_packet(self.codec_ctx, self.packet) };
            unsafe {
                ffi::av_packet_unref(self.packet);
            }
            if send_result < 0 {
                return Err(ffmpeg_error("avcodec_send_packet", send_result));
            }

            if let Some(decoded) =
                self.drain_until_target(target_index, frame_ordinal, expected_anchor_pts)?
            {
                return Ok(decoded);
            }
        }

        Err(format!("failed to decode frame {target_index}"))
    }

    fn drain_until_target(
        &mut self,
        target_index: u32,
        frame_ordinal: &mut u32,
        expected_anchor_pts: &mut Option<i64>,
    ) -> Result<Option<DecodedFrame>, String> {
        loop {
            let receive_result = unsafe { ffi::avcodec_receive_frame(self.codec_ctx, self.frame) };
            if receive_result == AVERROR_EAGAIN || receive_result == AVERROR_EOF {
                return Ok(None);
            }
            if receive_result < 0 {
                return Err(ffmpeg_error("avcodec_receive_frame", receive_result));
            }

            let decoded_pts = frame_pts(self.frame);
            if let Some(expected_pts) = *expected_anchor_pts {
                if decoded_pts != Some(expected_pts) {
                    unsafe {
                        ffi::av_frame_unref(self.frame);
                    }
                    return Err(format!(
                        "seek landed on unexpected frame: expected pts {expected_pts}, got {:?}",
                        decoded_pts
                    ));
                }
                *expected_anchor_pts = None;
            }

            if *frame_ordinal == target_index {
                let converted = unsafe { convert_frame_to_rgba(self.frame) };
                unsafe {
                    ffi::av_frame_unref(self.frame);
                }
                return converted.map(Some);
            }

            *frame_ordinal += 1;
            unsafe {
                ffi::av_frame_unref(self.frame);
            }
        }
    }

    fn drain_indexed_frames(
        &mut self,
        frames: &mut Vec<FrameMeta>,
        frame_ordinal: &mut u32,
        time_base: ffi::AVRational,
    ) -> u32 {
        let mut errors = 0;

        loop {
            let receive_result = unsafe { ffi::avcodec_receive_frame(self.codec_ctx, self.frame) };
            if receive_result == AVERROR_EAGAIN || receive_result == AVERROR_EOF {
                break;
            }
            if receive_result < 0 {
                errors += 1;
                eprintln!(
                    "video-worker: avcodec_receive_frame error at frame {}: {}",
                    *frame_ordinal + errors,
                    ffmpeg_error_message(receive_result),
                );
                continue;
            }

            let pts = frame_pts(self.frame);
            let timestamp_us = pts.map(|pts| pts_to_us(pts, time_base));
            let best_effort_timestamp_us =
                frame_best_effort_timestamp(self.frame).map(|pts| pts_to_us(pts, time_base));
            let is_keyframe = unsafe { ((*self.frame).flags & ffi::AV_FRAME_FLAG_KEY) != 0 };

            frames.push(FrameMeta {
                frame_index: *frame_ordinal,
                pts,
                timestamp_us,
                best_effort_timestamp_us,
                is_keyframe,
            });

            *frame_ordinal += 1;
            unsafe {
                ffi::av_frame_unref(self.frame);
            }
        }

        errors
    }

    fn codecpar(&self) -> *mut ffi::AVCodecParameters {
        unsafe { (*self.stream).codecpar }
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        unsafe {
            if !self.frame.is_null() {
                ffi::av_frame_free(&mut self.frame);
            }
            if !self.packet.is_null() {
                ffi::av_packet_free(&mut self.packet);
            }
            if !self.codec_ctx.is_null() {
                ffi::avcodec_free_context(&mut self.codec_ctx);
            }
            if !self.format_ctx.is_null() {
                ffi::avformat_close_input(&mut self.format_ctx);
            }
        }
    }
}

unsafe fn convert_frame_to_rgba(frame: *mut ffi::AVFrame) -> Result<DecodedFrame, String> {
    let width = (*frame).width.max(0);
    let height = (*frame).height.max(0);
    let source_format = av_pixel_format_from_raw((*frame).format)?;

    let sws_context = ffi::sws_getContext(
        width,
        height,
        source_format,
        width,
        height,
        ffi::AVPixelFormat::AV_PIX_FMT_RGBA,
        ffi::SwsFlags::SWS_BILINEAR as c_int,
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null(),
    );
    if sws_context.is_null() {
        return Err("failed to allocate swscale context".to_string());
    }

    let mut rgba = vec![0_u8; (width as usize) * (height as usize) * 4];
    let mut dst_data = [ptr::null_mut(); 4];
    let mut dst_linesize = [0; 4];
    dst_data[0] = rgba.as_mut_ptr();
    dst_linesize[0] = width * 4;

    let scale_result = ffi::sws_scale(
        sws_context,
        (*frame).data.as_ptr() as *const *const u8,
        (*frame).linesize.as_ptr(),
        0,
        height,
        dst_data.as_mut_ptr(),
        dst_linesize.as_mut_ptr(),
    );
    ffi::sws_freeContext(sws_context);

    if scale_result < height {
        return Err(format!(
            "sws_scale produced {scale_result} rows for {height}-row frame"
        ));
    }

    Ok(DecodedFrame {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

fn av_pixel_format_from_raw(format: c_int) -> Result<ffi::AVPixelFormat, String> {
    if format < 0 {
        return Err("decoded frame missing pixel format".to_string());
    }
    Ok(unsafe { mem::transmute::<c_int, ffi::AVPixelFormat>(format) })
}

fn frame_pts(frame: *mut ffi::AVFrame) -> Option<i64> {
    let raw_pts = unsafe { (*frame).pts };
    if raw_pts == ffi::AV_NOPTS_VALUE {
        None
    } else {
        Some(raw_pts)
    }
}

fn frame_best_effort_timestamp(frame: *mut ffi::AVFrame) -> Option<i64> {
    let raw_pts = unsafe { (*frame).best_effort_timestamp };
    if raw_pts == ffi::AV_NOPTS_VALUE {
        None
    } else {
        Some(raw_pts)
    }
}

pub fn pts_to_us(pts: i64, time_base: ffi::AVRational) -> i64 {
    if time_base.den == 0 {
        return 0;
    }
    let num = time_base.num as i128;
    let den = time_base.den as i128;
    let result = (pts as i128) * num * 1_000_000 / den;
    result
        .try_into()
        .unwrap_or(if result > 0 { i64::MAX } else { i64::MIN })
}

fn make_session_id(path: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);
    format!("session-{:016x}", hasher.finish())
}

fn context_duration_us(context: *mut ffi::AVFormatContext) -> Option<i64> {
    let duration = unsafe { (*context).duration };
    if duration == ffi::AV_NOPTS_VALUE || duration < 0 {
        None
    } else {
        Some(duration.saturating_mul(1_000_000) / i64::from(ffi::AV_TIME_BASE))
    }
}

fn stream_duration_us(stream: *mut ffi::AVStream) -> Option<i64> {
    let duration = unsafe { (*stream).duration };
    if duration == ffi::AV_NOPTS_VALUE || duration < 0 {
        return None;
    }

    let time_base = unsafe { (*stream).time_base };
    if time_base.den == 0 {
        None
    } else {
        Some(
            duration.saturating_mul(i64::from(time_base.num) * 1_000_000)
                / i64::from(time_base.den),
        )
    }
}

fn positive_i64(value: i64) -> Option<i64> {
    if value > 0 {
        Some(value)
    } else {
        None
    }
}

fn ffmpeg_error(operation: &str, code: c_int) -> String {
    format!("{operation} failed: {}", ffmpeg_error_message(code))
}

fn ffmpeg_error_message(code: c_int) -> String {
    let mut buffer = [0 as c_char; 256];
    unsafe {
        ffi::av_strerror(code, buffer.as_mut_ptr(), buffer.len());
    }
    c_string_from_ptr(buffer.as_ptr()).unwrap_or_else(|| format!("error code {code}"))
}

fn c_string_from_ptr(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned())
    }
}
