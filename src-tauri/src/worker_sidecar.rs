use std::time::Duration;

use tauri::AppHandle;
use tauri::async_runtime::Receiver;
use tauri_plugin_shell::{process::CommandEvent, ShellExt};
use video_worker::{
    FrameBuffer, FrameHandle, SessionSummary, VideoProbe, WorkerHello,
    WorkerRequest, WorkerResponse, PROTOCOL_VERSION,
};

const WORKER_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct WorkerClient {
    receiver: Receiver<CommandEvent>,
    child: Option<tauri_plugin_shell::process::CommandChild>,
}

impl WorkerClient {
    pub async fn request(&mut self, request: &WorkerRequest) -> Result<WorkerResponse, String> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| "video-worker process already closed".to_string())?;
        write_request(child, request)?;
        let payload = match tokio::time::timeout(WORKER_REQUEST_TIMEOUT, self.read_stdout_line()).await
        {
            Ok(result) => result?,
            Err(_) => {
                if let Some(child) = self.child.take() {
                    let _ = child.kill();
                }
                return Err(format!(
                    "video-worker request timed out after {}s",
                    WORKER_REQUEST_TIMEOUT.as_secs()
                ));
            }
        };
        parse_worker_response(payload.as_bytes())
    }

    pub fn kill(self) -> Result<(), String> {
        match self.child {
            Some(child) => child
                .kill()
                .map_err(|error| format!("failed to kill video-worker: {error}")),
            None => Ok(()),
        }
    }

    async fn read_stdout_line(&mut self) -> Result<String, String> {
        let mut stderr_lines = Vec::new();

        loop {
            match self.receiver.recv().await {
                Some(CommandEvent::Stdout(line)) => {
                    let line = String::from_utf8_lossy(&line).trim().to_string();
                    if !line.is_empty() {
                        return Ok(line);
                    }
                }
                Some(CommandEvent::Stderr(line)) => {
                    let line = String::from_utf8_lossy(&line).trim().to_string();
                    if !line.is_empty() {
                        stderr_lines.push(line);
                    }
                }
                Some(CommandEvent::Error(message)) => return Err(message),
                Some(CommandEvent::Terminated(payload)) => {
                    let stderr = stderr_lines.join("\n");
                    return Err(if !stderr.is_empty() {
                        stderr
                    } else {
                        format!("video-worker terminated with status {:?}", payload.code)
                    });
                }
                Some(_) => {}
                None => {
                    let stderr = stderr_lines.join("\n");
                    return Err(if !stderr.is_empty() {
                        stderr
                    } else {
                        "video-worker event stream closed".to_string()
                    });
                }
            }
        }
    }
}

pub async fn ping_video_worker(app: &AppHandle) -> Result<WorkerHello, String> {
    let output = app
        .shell()
        .sidecar("video-worker")
        .map_err(|error| format!("failed to resolve sidecar command: {error}"))?
        .arg("--ping")
        .output()
        .await
        .map_err(|error| format!("failed to run video-worker --ping: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "video-worker exited without output".to_string()
        };
        return Err(message);
    }

    parse_hello_payload(&output.stdout)
}

pub async fn probe_video_with_worker(app: &AppHandle, path: &str) -> Result<VideoProbe, String> {
    let response = send_one_shot_request(
        app,
        WorkerRequest::Probe {
            path: path.to_string(),
        },
    )
    .await?;

    parse_probe_response(response)
}

pub async fn open_video_with_worker(
    app: &AppHandle,
    path: &str,
) -> Result<(WorkerClient, SessionSummary), String> {
    let mut worker = spawn_worker(app).await?;
    let response = worker
        .request(&WorkerRequest::OpenVideo {
            path: path.to_string(),
        })
        .await?;
    let session = parse_open_video_response(response)?;
    Ok((worker, session))
}

pub async fn get_frame_with_worker(
    worker: &mut WorkerClient,
    session_id: &str,
    frame_index: u32,
) -> Result<FrameHandle, String> {
    let response = worker
        .request(&WorkerRequest::GetFrame {
            session_id: session_id.to_string(),
            frame_index,
        })
        .await?;

    parse_get_frame_response(response)
}

pub async fn read_frame_buffer_with_worker(
    worker: &mut WorkerClient,
    session_id: &str,
    frame_id: &str,
) -> Result<FrameBuffer, String> {
    let response = worker
        .request(&WorkerRequest::ReadFrameBuffer {
            session_id: session_id.to_string(),
            frame_id: frame_id.to_string(),
        })
        .await?;

    parse_read_frame_buffer_response(response)
}

pub async fn step_frame_with_worker(
    worker: &mut WorkerClient,
    session_id: &str,
    delta: i32,
) -> Result<FrameHandle, String> {
    let response = worker
        .request(&WorkerRequest::StepFrame {
            session_id: session_id.to_string(),
            delta,
        })
        .await?;

    parse_step_frame_response(response)
}

pub async fn close_video_with_worker(
    worker: &mut WorkerClient,
    session_id: &str,
) -> Result<bool, String> {
    let response = worker
        .request(&WorkerRequest::CloseVideo {
            session_id: session_id.to_string(),
        })
        .await?;

    parse_close_video_response(response)
}

pub async fn shutdown_worker(mut worker: WorkerClient) -> Result<(), String> {
    let response = match worker.request(&WorkerRequest::Shutdown).await {
        Ok(response) => response,
        Err(error) => {
            let kill_result = worker.kill();
            return match kill_result {
                Ok(()) => Err(error),
                Err(kill_error) => Err(format!("{error}; {kill_error}")),
            };
        }
    };
    match response {
        WorkerResponse::Ack { accepted } if accepted => Ok(()),
        WorkerResponse::Ack { accepted: false } => {
            Err("video-worker rejected shutdown request".to_string())
        }
        WorkerResponse::RequestError { error } => Err(worker_error_message(&error)),
        WorkerResponse::Error { message } => Err(message),
        other => Err(format!("unexpected response for shutdown payload: {other:?}")),
    }
}

async fn spawn_worker(app: &AppHandle) -> Result<WorkerClient, String> {
    let (receiver, child) = app
        .shell()
        .sidecar("video-worker")
        .map_err(|error| format!("failed to resolve sidecar command: {error}"))?
        .spawn()
        .map_err(|error| format!("failed to spawn video-worker: {error}"))?;

    let mut worker = WorkerClient {
        receiver,
        child: Some(child),
    };
    let hello = worker.request(&WorkerRequest::Ping).await?;
    validate_worker_hello(hello)?;
    Ok(worker)
}

async fn send_one_shot_request(app: &AppHandle, request: WorkerRequest) -> Result<WorkerResponse, String> {
    let mut worker = spawn_worker(app).await?;
    let response = worker.request(&request).await?;
    shutdown_worker(worker).await?;
    Ok(response)
}

fn write_request(child: &mut tauri_plugin_shell::process::CommandChild, request: &WorkerRequest) -> Result<(), String> {
    let payload = serde_json::to_vec(request)
        .map_err(|error| format!("failed to serialize worker request: {error}"))?;
    child
        .write(&payload)
        .map_err(|error| format!("failed to write worker request: {error}"))?;
    child
        .write(b"\n")
        .map_err(|error| format!("failed to terminate worker request line: {error}"))
}

fn parse_worker_response(payload: &[u8]) -> Result<WorkerResponse, String> {
    serde_json::from_slice(payload)
        .map_err(|error| format!("failed to parse worker response JSON: {error}"))
}

fn parse_hello_payload(payload: &[u8]) -> Result<WorkerHello, String> {
    serde_json::from_slice(payload)
        .map_err(|error| format!("failed to parse worker handshake JSON: {error}"))
}

#[cfg(test)]
fn parse_probe_payload(payload: &[u8]) -> Result<VideoProbe, String> {
    parse_probe_response(parse_worker_response(payload)?)
}

fn parse_probe_response(response: WorkerResponse) -> Result<VideoProbe, String> {
    match response {
        WorkerResponse::ProbeResult { probe } => Ok(probe),
        WorkerResponse::RequestError { error } => Err(worker_error_message(&error)),
        WorkerResponse::Error { message } => Err(message),
        other => Err(format!("unexpected response for probe payload: {other:?}")),
    }
}

fn parse_open_video_response(response: WorkerResponse) -> Result<SessionSummary, String> {
    match response {
        WorkerResponse::OpenVideoResult { session } => Ok(session),
        WorkerResponse::RequestError { error } => Err(worker_error_message(&error)),
        WorkerResponse::Error { message } => Err(message),
        other => Err(format!(
            "unexpected response for open_video payload: {other:?}"
        )),
    }
}

#[cfg(test)]
fn parse_get_frame_payload(payload: &[u8]) -> Result<FrameHandle, String> {
    parse_get_frame_response(parse_worker_response(payload)?)
}

fn parse_get_frame_response(response: WorkerResponse) -> Result<FrameHandle, String> {
    match response {
        WorkerResponse::GetFrameResult { frame } => Ok(frame),
        WorkerResponse::RequestError { error } => Err(worker_error_message(&error)),
        WorkerResponse::Error { message } => Err(message),
        other => Err(format!("unexpected response for get_frame payload: {other:?}")),
    }
}

#[cfg(test)]
fn parse_read_frame_buffer_payload(payload: &[u8]) -> Result<FrameBuffer, String> {
    parse_read_frame_buffer_response(parse_worker_response(payload)?)
}

fn parse_read_frame_buffer_response(response: WorkerResponse) -> Result<FrameBuffer, String> {
    match response {
        WorkerResponse::ReadFrameBufferResult { buffer } => Ok(buffer),
        WorkerResponse::RequestError { error } => Err(worker_error_message(&error)),
        WorkerResponse::Error { message } => Err(message),
        other => Err(format!(
            "unexpected response for read_frame_buffer payload: {other:?}"
        )),
    }
}

fn parse_step_frame_response(response: WorkerResponse) -> Result<FrameHandle, String> {
    match response {
        WorkerResponse::StepFrameResult { frame } => Ok(frame),
        WorkerResponse::RequestError { error } => Err(worker_error_message(&error)),
        WorkerResponse::Error { message } => Err(message),
        other => Err(format!("unexpected response for step_frame payload: {other:?}")),
    }
}

fn parse_close_video_response(response: WorkerResponse) -> Result<bool, String> {
    match response {
        WorkerResponse::CloseVideoResult { closed } => Ok(closed),
        WorkerResponse::RequestError { error } => Err(worker_error_message(&error)),
        WorkerResponse::Error { message } => Err(message),
        other => Err(format!("unexpected response for close_video payload: {other:?}")),
    }
}

fn validate_worker_hello(response: WorkerResponse) -> Result<WorkerHello, String> {
    match response {
        WorkerResponse::Hello { hello } => {
            if hello.protocol_version != PROTOCOL_VERSION {
                return Err(format!(
                    "video-worker protocol mismatch: expected {}, got {}",
                    PROTOCOL_VERSION, hello.protocol_version
                ));
            }
            Ok(hello)
        }
        WorkerResponse::RequestError { error } => Err(worker_error_message(&error)),
        WorkerResponse::Error { message } => Err(message),
        other => Err(format!("unexpected response for worker handshake: {other:?}")),
    }
}

fn worker_error_message(error: &video_worker::WorkerError) -> String {
    format!("{}: {}", worker_error_code(error.code), error.message)
}

fn worker_error_code(code: video_worker::WorkerErrorCode) -> &'static str {
    match code {
        video_worker::WorkerErrorCode::DecodeFailed => "decode_failed",
        video_worker::WorkerErrorCode::FrameExpired => "frame_expired",
        video_worker::WorkerErrorCode::FrameOutOfRange => "frame_out_of_range",
        video_worker::WorkerErrorCode::InvalidFrameId => "invalid_frame_id",
        video_worker::WorkerErrorCode::NoCurrentFrame => "no_current_frame",
        video_worker::WorkerErrorCode::SessionNotFound => "session_not_found",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_get_frame_payload, parse_hello_payload, parse_probe_payload,
        parse_read_frame_buffer_payload, validate_worker_hello,
    };
    use video_worker::{LinkedFfmpegVersions, WorkerHello, WorkerResponse, PROTOCOL_VERSION};

    #[test]
    fn parse_hello_payload_decodes_worker_handshake() {
        let hello = parse_hello_payload(
            br#"{"worker_name":"video-worker","protocol_version":1,"ffmpeg_linked":true,"transport":"stdio-jsonl","linked_versions":{"avcodec":1,"avformat":2,"avutil":3,"swscale":4}}"#,
        )
        .expect("parse hello");

        assert_eq!(hello.worker_name, "video-worker");
        assert!(hello.ffmpeg_linked);
        assert_eq!(hello.linked_versions.avformat, 2);
    }

    #[test]
    fn parse_probe_payload_decodes_worker_probe_response() {
        let probe = parse_probe_payload(
            br#"{"type":"probe_result","probe":{"path":"sample.mp4","video_stream_index":0,"width":64,"height":48,"duration_us":1000000,"codec_name":"mpeg4","bit_rate":1234,"avg_frame_rate_num":5,"avg_frame_rate_den":1}}"#,
        )
        .expect("parse probe");

        assert_eq!(probe.path, "sample.mp4");
        assert_eq!(probe.width, 64);
        assert_eq!(probe.avg_frame_rate_num, 5);
    }

    #[test]
    fn parse_get_frame_payload_decodes_frame_handle() {
        let frame = parse_get_frame_payload(
            br#"{"type":"get_frame_result","frame":{"frame_id":"session-1:4:133333","frame_index":4,"width":64,"height":48,"timestamp_us":133333,"is_precise":true,"rgba_path":"C:/tmp/session-1/frame-4.rgba"}}"#,
        )
        .expect("parse get_frame");

        assert_eq!(frame.frame_id, "session-1:4:133333");
        assert_eq!(frame.frame_index, 4);
        assert_eq!(frame.timestamp_us, Some(133333));
        assert_eq!(frame.rgba_path, "C:/tmp/session-1/frame-4.rgba");
    }

    #[test]
    fn parse_get_frame_payload_surfaces_request_error() {
        let error = parse_get_frame_payload(
            br#"{"type":"request_error","error":{"code":"frame_expired","message":"frame buffer expired for session-1:4:133333"}}"#,
        )
        .expect_err("request error should fail");

        assert!(error.contains("frame buffer expired"));
        assert!(error.contains("frame_expired"));
    }

    #[test]
    fn parse_read_frame_buffer_payload_decodes_rgba_path() {
        let buffer = parse_read_frame_buffer_payload(
            br#"{"type":"read_frame_buffer_result","buffer":{"frame_id":"session-1:4:133333","width":2,"height":1,"rgba_path":"C:/tmp/session-1/frame-4.rgba"}}"#,
        )
        .expect("parse frame buffer");

        assert_eq!(buffer.frame_id, "session-1:4:133333");
        assert_eq!(buffer.width, 2);
        assert_eq!(buffer.height, 1);
        assert_eq!(buffer.rgba_path, "C:/tmp/session-1/frame-4.rgba");
    }

    #[test]
    fn validate_worker_hello_accepts_matching_protocol() {
        let hello = validate_worker_hello(WorkerResponse::Hello {
            hello: WorkerHello {
                worker_name: "video-worker".to_string(),
                protocol_version: PROTOCOL_VERSION,
                ffmpeg_linked: true,
                transport: "stdio-jsonl".to_string(),
                linked_versions: LinkedFfmpegVersions {
                    avcodec: 1,
                    avformat: 2,
                    avutil: 3,
                    swscale: 4,
                },
            },
        })
        .expect("matching protocol should validate");

        assert_eq!(hello.protocol_version, PROTOCOL_VERSION);
    }

    #[test]
    fn validate_worker_hello_rejects_protocol_mismatch() {
        let error = validate_worker_hello(WorkerResponse::Hello {
            hello: WorkerHello {
                worker_name: "video-worker".to_string(),
                protocol_version: PROTOCOL_VERSION + 1,
                ffmpeg_linked: true,
                transport: "stdio-jsonl".to_string(),
                linked_versions: LinkedFfmpegVersions {
                    avcodec: 1,
                    avformat: 2,
                    avutil: 3,
                    swscale: 4,
                },
            },
        })
        .expect_err("protocol mismatch should fail");

        assert!(error.contains("protocol"));
    }
}
