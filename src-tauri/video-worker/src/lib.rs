pub mod ffmpeg;
mod worker;

pub use ffmpeg::{LinkedFfmpegVersions, VideoProbe};
pub use worker::WorkerServer;

use ffmpeg::linked_versions;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerHello {
    pub worker_name: String,
    pub protocol_version: u32,
    pub ffmpeg_linked: bool,
    pub transport: String,
    pub linked_versions: LinkedFfmpegVersions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrameMeta {
    pub frame_index: u32,
    pub pts: Option<i64>,
    pub timestamp_us: Option<i64>,
    pub best_effort_timestamp_us: Option<i64>,
    pub is_keyframe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrameHandle {
    pub frame_id: String,
    pub frame_index: u32,
    pub width: u32,
    pub height: u32,
    pub timestamp_us: Option<i64>,
    pub is_precise: bool,
    pub rgba_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrameBuffer {
    pub frame_id: String,
    pub width: u32,
    pub height: u32,
    pub rgba_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSummary {
    pub session_id: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub duration_us: Option<i64>,
    pub codec_name: String,
    pub total_frames: u32,
    pub avg_frame_rate_num: i32,
    pub avg_frame_rate_den: i32,
    pub decode_errors: u32,
    pub frames: Vec<FrameMeta>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerErrorCode {
    DecodeFailed,
    FrameExpired,
    FrameOutOfRange,
    InvalidFrameId,
    NoCurrentFrame,
    SessionNotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerError {
    pub code: WorkerErrorCode,
    pub message: String,
}

impl WorkerError {
    pub fn new(code: WorkerErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerRequest {
    Ping,
    Probe { path: String },
    OpenVideo { path: String },
    GetFrame { session_id: String, frame_index: u32 },
    ReadFrameBuffer { session_id: String, frame_id: String },
    StepFrame { session_id: String, delta: i32 },
    CloseVideo { session_id: String },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerResponse {
    Hello { hello: WorkerHello },
    ProbeResult { probe: VideoProbe },
    OpenVideoResult { session: SessionSummary },
    GetFrameResult { frame: FrameHandle },
    ReadFrameBufferResult { buffer: FrameBuffer },
    StepFrameResult { frame: FrameHandle },
    CloseVideoResult { closed: bool },
    RequestError { error: WorkerError },
    Ack { accepted: bool },
    Error { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    Ping,
    Stdio,
}

pub fn hello(ffmpeg_linked: bool) -> WorkerHello {
    WorkerHello {
        worker_name: "video-worker".to_string(),
        protocol_version: PROTOCOL_VERSION,
        ffmpeg_linked,
        transport: "stdio-jsonl".to_string(),
        linked_versions: linked_versions(),
    }
}

pub fn handle_request(request: WorkerRequest) -> WorkerResponse {
    worker::handle_request(request)
}

pub fn parse_mode(args: &[String]) -> WorkerMode {
    if args.iter().any(|arg| arg == "--ping") {
        WorkerMode::Ping
    } else {
        WorkerMode::Stdio
    }
}

#[cfg(test)]
mod tests {
    use super::{handle_request, parse_mode, WorkerMode, WorkerRequest, WorkerResponse};

    #[test]
    fn parse_mode_defaults_to_stdio() {
        let args = vec!["video-worker".to_string()];
        assert_eq!(parse_mode(&args), WorkerMode::Stdio);
    }

    #[test]
    fn parse_mode_detects_ping_flag() {
        let args = vec!["video-worker".to_string(), "--ping".to_string()];
        assert_eq!(parse_mode(&args), WorkerMode::Ping);
    }

    #[test]
    fn ping_request_returns_handshake() {
        let response = handle_request(WorkerRequest::Ping);

        match response {
            WorkerResponse::Hello { hello } => {
                assert_eq!(hello.worker_name, "video-worker");
                assert_eq!(hello.protocol_version, 1);
                assert!(hello.ffmpeg_linked);
                assert!(hello.linked_versions.avcodec > 0);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn shutdown_request_returns_ack() {
        let response = handle_request(WorkerRequest::Shutdown);
        assert_eq!(response, WorkerResponse::Ack { accepted: true });
    }
}
