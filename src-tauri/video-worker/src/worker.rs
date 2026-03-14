use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::Path,
    sync::{Mutex, OnceLock},
};

use crate::{
    ffmpeg, hello, FrameBuffer, FrameHandle, FrameMeta, SessionSummary, WorkerError,
    WorkerErrorCode, WorkerRequest, WorkerResponse,
};

const DEFAULT_FRAME_CACHE_CAPACITY: usize = 64;
const PREFETCH_RADIUS: u32 = 1;

static GLOBAL_WORKER: OnceLock<Mutex<WorkerServer>> = OnceLock::new();

pub fn handle_request(request: WorkerRequest) -> WorkerResponse {
    let worker = GLOBAL_WORKER.get_or_init(|| Mutex::new(WorkerServer::default()));
    let mut worker = worker.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    worker.handle_request(request)
}

#[derive(Debug)]
pub struct WorkerServer {
    sessions: HashMap<String, VideoSession>,
    frame_cache_capacity: usize,
}

impl Default for WorkerServer {
    fn default() -> Self {
        Self::with_cache_capacity(DEFAULT_FRAME_CACHE_CAPACITY)
    }
}

impl WorkerServer {
    pub fn with_cache_capacity(frame_cache_capacity: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            frame_cache_capacity: frame_cache_capacity.max(1),
        }
    }

    pub fn handle_request(&mut self, request: WorkerRequest) -> WorkerResponse {
        match request {
            WorkerRequest::Ping => WorkerResponse::Hello {
                hello: hello(true),
            },
            WorkerRequest::Probe { path } => match ffmpeg::probe_video(Path::new(&path)) {
                Ok(probe) => WorkerResponse::ProbeResult { probe },
                Err(message) => WorkerResponse::Error { message },
            },
            WorkerRequest::OpenVideo { path } => match self.open_video(Path::new(&path)) {
                Ok(session) => WorkerResponse::OpenVideoResult { session },
                Err(message) => WorkerResponse::Error { message },
            },
            WorkerRequest::GetFrame {
                session_id,
                frame_index,
            } => match self.get_frame(&session_id, frame_index) {
                Ok(frame) => WorkerResponse::GetFrameResult { frame },
                Err(error) => WorkerResponse::RequestError { error },
            },
            WorkerRequest::ReadFrameBuffer {
                session_id,
                frame_id,
            } => match self.read_frame_buffer(&session_id, &frame_id) {
                Ok(buffer) => WorkerResponse::ReadFrameBufferResult { buffer },
                Err(error) => WorkerResponse::RequestError { error },
            },
            WorkerRequest::StepFrame { session_id, delta } => {
                match self.step_frame(&session_id, delta) {
                    Ok(frame) => WorkerResponse::StepFrameResult { frame },
                    Err(error) => WorkerResponse::RequestError { error },
                }
            }
            WorkerRequest::CloseVideo { session_id } => {
                let closed = self.sessions.remove(&session_id).is_some();
                WorkerResponse::CloseVideoResult { closed }
            }
            WorkerRequest::Shutdown => {
                self.sessions.clear();
                WorkerResponse::Ack { accepted: true }
            }
        }
    }

    fn open_video(&mut self, path: &Path) -> Result<SessionSummary, String> {
        let decoder_session = ffmpeg::open_decoder_session(path)?;
        let session = decoder_session.summary().clone();
        let session_id = session.session_id.clone();
        self.sessions.insert(
            session_id,
            VideoSession::new(session.clone(), self.frame_cache_capacity, decoder_session)?,
        );
        Ok(session)
    }

    fn get_frame(&mut self, session_id: &str, frame_index: u32) -> Result<FrameHandle, WorkerError> {
        let session = self.session_mut(session_id)?;
        let meta = session
            .summary
            .frames
            .get(frame_index as usize)
            .cloned()
            .ok_or_else(|| frame_out_of_range(frame_index, session.summary.total_frames))?;

        let frame_id = make_frame_id(session_id, &meta);
        if let Some(buffer) = session.frame_cache.get(&frame_id) {
            session.current_frame_index = Some(frame_index);
            self.prefetch_adjacent_frames(session_id, frame_index);
            return Ok(FrameHandle {
                frame_id,
                frame_index: meta.frame_index,
                width: buffer.width,
                height: buffer.height,
                timestamp_us: frame_timestamp_us(&meta),
                is_precise: frame_timestamp_us(&meta).is_some(),
                rgba_path: buffer.rgba_path,
            });
        }

        let decoded = session
            .decoder
            .decode_frame(frame_index)
            .map_err(|message| WorkerError::new(WorkerErrorCode::DecodeFailed, message))?;

        let rgba_path = write_frame_buffer(&session.temp_dir, &frame_id, &decoded).map_err(|message| {
            WorkerError::new(WorkerErrorCode::DecodeFailed, message)
        })?;
        let buffer = FrameBuffer {
            frame_id: frame_id.clone(),
            width: decoded.width,
            height: decoded.height,
            rgba_path: rgba_path.clone(),
        };

        session.frame_cache.insert(buffer.clone());
        session.current_frame_index = Some(frame_index);
        self.prefetch_adjacent_frames(session_id, frame_index);

        Ok(FrameHandle {
            frame_id,
            frame_index: meta.frame_index,
            width: buffer.width,
            height: buffer.height,
            timestamp_us: frame_timestamp_us(&meta),
            is_precise: frame_timestamp_us(&meta).is_some(),
            rgba_path,
        })
    }

    fn read_frame_buffer(
        &mut self,
        session_id: &str,
        frame_id: &str,
    ) -> Result<FrameBuffer, WorkerError> {
        let session = self.session_mut(session_id)?;
        validate_frame_id(session_id, &session.summary, frame_id)?;

        session
            .frame_cache
            .get(frame_id)
            .ok_or_else(|| WorkerError::new(
                WorkerErrorCode::FrameExpired,
                format!("frame buffer expired for {frame_id}"),
            ))
    }

    fn step_frame(&mut self, session_id: &str, delta: i32) -> Result<FrameHandle, WorkerError> {
        let current_frame_index = {
            let session = self.session_mut(session_id)?;
            session.current_frame_index.ok_or_else(|| {
                WorkerError::new(
                    WorkerErrorCode::NoCurrentFrame,
                    format!("no current frame for session {session_id}"),
                )
            })?
        };

        let target_index = {
            let session = self.session_mut(session_id)?;
            clamp_frame_index(current_frame_index, delta, session.summary.total_frames)
        };

        self.get_frame(session_id, target_index)
    }

    fn session_mut(&mut self, session_id: &str) -> Result<&mut VideoSession, WorkerError> {
        self.sessions.get_mut(session_id).ok_or_else(|| {
            WorkerError::new(
                WorkerErrorCode::SessionNotFound,
                format!("session not found: {session_id}"),
            )
        })
    }

    fn prefetch_adjacent_frames(&mut self, session_id: &str, center_index: u32) {
        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };

        let total_frames = session.summary.total_frames;
        if total_frames == 0 {
            return;
        }

        let upper = center_index
            .saturating_add(PREFETCH_RADIUS)
            .min(total_frames.saturating_sub(1));
        for offset in 1..=PREFETCH_RADIUS {
            let forward = center_index.saturating_add(offset);
            if forward <= upper {
                prefetch_frame(session_id, session, forward);
            }

            if let Some(backward) = center_index.checked_sub(offset) {
                prefetch_frame(session_id, session, backward);
            }
        }
    }
}

#[derive(Debug)]
struct VideoSession {
    summary: SessionSummary,
    current_frame_index: Option<u32>,
    frame_cache: FrameBufferCache,
    temp_dir: std::path::PathBuf,
    decoder: ffmpeg::DecoderSession,
}

impl VideoSession {
    fn new(
        summary: SessionSummary,
        frame_cache_capacity: usize,
        decoder: ffmpeg::DecoderSession,
    ) -> Result<Self, String> {
        let temp_dir = std::env::temp_dir()
            .join("countdelay-rs")
            .join(&summary.session_id);
        fs::create_dir_all(&temp_dir)
            .map_err(|error| format!("failed to create frame cache dir {}: {error}", temp_dir.display()))?;

        Ok(Self {
            summary,
            current_frame_index: None,
            frame_cache: FrameBufferCache::new(frame_cache_capacity),
            temp_dir,
            decoder,
        })
    }
}

impl Drop for VideoSession {
    fn drop(&mut self) {
        self.frame_cache.clear();
        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

#[derive(Debug)]
struct FrameBufferCache {
    capacity: usize,
    order: VecDeque<String>,
    entries: HashMap<String, FrameBuffer>,
}

impl FrameBufferCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            order: VecDeque::new(),
            entries: HashMap::new(),
        }
    }

    fn insert(&mut self, buffer: FrameBuffer) {
        self.touch(&buffer.frame_id);
        self.entries.insert(buffer.frame_id.clone(), buffer);

        while self.order.len() > self.capacity {
            if let Some(expired_frame_id) = self.order.pop_front() {
                if let Some(expired) = self.entries.remove(&expired_frame_id) {
                    let _ = fs::remove_file(expired.rgba_path);
                }
            }
        }
    }

    fn get(&mut self, frame_id: &str) -> Option<FrameBuffer> {
        let buffer = self.entries.get(frame_id).cloned()?;
        self.touch(frame_id);
        Some(buffer)
    }

    fn clear(&mut self) {
        for buffer in self.entries.drain().map(|(_, buffer)| buffer) {
            let _ = fs::remove_file(buffer.rgba_path);
        }
        self.order.clear();
    }

    fn contains(&self, frame_id: &str) -> bool {
        self.entries.contains_key(frame_id)
    }

    fn touch(&mut self, frame_id: &str) {
        if let Some(existing_index) = self.order.iter().position(|existing| existing == frame_id) {
            self.order.remove(existing_index);
        }
        self.order.push_back(frame_id.to_string());
    }
}

fn clamp_frame_index(current_frame_index: u32, delta: i32, total_frames: u32) -> u32 {
    let max_index = total_frames.saturating_sub(1) as i64;
    (i64::from(current_frame_index) + i64::from(delta)).clamp(0, max_index) as u32
}

fn frame_out_of_range(frame_index: u32, total_frames: u32) -> WorkerError {
    WorkerError::new(
        WorkerErrorCode::FrameOutOfRange,
        format!("frame {frame_index} out of range for total_frames={total_frames}"),
    )
}

fn make_frame_id(session_id: &str, meta: &FrameMeta) -> String {
    let timestamp = frame_timestamp_us(meta)
        .map(|timestamp| timestamp.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!("{session_id}:{}:{timestamp}", meta.frame_index)
}

fn frame_timestamp_us(meta: &FrameMeta) -> Option<i64> {
    meta.best_effort_timestamp_us.or(meta.timestamp_us)
}

fn write_frame_buffer(
    temp_dir: &Path,
    frame_id: &str,
    decoded: &ffmpeg::DecodedFrame,
) -> Result<String, String> {
    let file_name = format!("{}.rgba", frame_id.replace(':', "_"));
    let path = temp_dir.join(file_name);
    fs::write(&path, &decoded.rgba)
        .map_err(|error| format!("failed to write frame buffer {}: {error}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

fn prefetch_frame(session_id: &str, session: &mut VideoSession, frame_index: u32) {
    let Some(meta) = session.summary.frames.get(frame_index as usize).cloned() else {
        return;
    };
    let frame_id = make_frame_id(session_id, &meta);
    if session.frame_cache.contains(&frame_id) {
        return;
    }

    let decoded = match session.decoder.decode_frame(frame_index) {
        Ok(decoded) => decoded,
        Err(error) => {
            eprintln!(
                "video-worker: failed to prefetch frame {} for session {}: {}",
                frame_index, session_id, error
            );
            return;
        }
    };

    let rgba_path = match write_frame_buffer(&session.temp_dir, &frame_id, &decoded) {
        Ok(path) => path,
        Err(error) => {
            eprintln!(
                "video-worker: failed to write prefetched frame {} for session {}: {}",
                frame_index, session_id, error
            );
            return;
        }
    };

    session.frame_cache.insert(FrameBuffer {
        frame_id,
        width: decoded.width,
        height: decoded.height,
        rgba_path,
    });
}

fn validate_frame_id(
    session_id: &str,
    session: &SessionSummary,
    frame_id: &str,
) -> Result<(), WorkerError> {
    let mut parts = frame_id.splitn(3, ':');
    let actual_session_id = parts.next();
    let frame_index = parts.next();
    let timestamp = parts.next();

    match (actual_session_id, frame_index, timestamp) {
        (Some(actual_session_id), Some(frame_index), Some(timestamp))
            if actual_session_id == session_id =>
        {
            let frame_index = frame_index.parse::<u32>().map_err(|_| {
                WorkerError::new(
                    WorkerErrorCode::InvalidFrameId,
                    format!("invalid frame id: {frame_id}"),
                )
            })?;

            let meta = session.frames.get(frame_index as usize).ok_or_else(|| {
                WorkerError::new(
                    WorkerErrorCode::InvalidFrameId,
                    format!("invalid frame id: {frame_id}"),
                )
            })?;

            let expected_timestamp = meta
                .best_effort_timestamp_us
                .or(meta.timestamp_us)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string());
            if expected_timestamp == timestamp {
                Ok(())
            } else {
                Err(WorkerError::new(
                    WorkerErrorCode::InvalidFrameId,
                    format!("invalid frame id: {frame_id}"),
                ))
            }
        }
        _ => Err(WorkerError::new(
            WorkerErrorCode::InvalidFrameId,
            format!("invalid frame id: {frame_id}"),
        )),
    }
}
