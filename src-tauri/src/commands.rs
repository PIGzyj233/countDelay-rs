use serde::Serialize;
use tauri::State;
use video_worker::{FrameHandle, SessionSummary};

use crate::state::AppState;
use crate::worker_sidecar;

/// Session info returned to the frontend (without the full frame index).
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
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
}

impl From<&SessionSummary> for SessionInfo {
    fn from(s: &SessionSummary) -> Self {
        SessionInfo {
            session_id: s.session_id.clone(),
            path: s.path.clone(),
            width: s.width,
            height: s.height,
            duration_us: s.duration_us,
            codec_name: s.codec_name.clone(),
            total_frames: s.total_frames,
            avg_frame_rate_num: s.avg_frame_rate_num,
            avg_frame_rate_den: s.avg_frame_rate_den,
            decode_errors: s.decode_errors,
        }
    }
}

#[tauri::command]
pub async fn open_video(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<SessionInfo, String> {
    let (worker, summary) = worker_sidecar::open_video_with_worker(&app, &path).await?;
    let info = SessionInfo::from(&summary);
    state.insert_session(summary, worker);
    Ok(info)
}

#[tauri::command]
pub async fn close_video(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<bool, String> {
    let Some(session) = state.get_session(&session_id) else {
        return Ok(false);
    };

    let worker = {
        let mut worker = session.worker.lock().await;
        worker.take()
    };

    if let Some(mut worker) = worker {
        let close_result = worker_sidecar::close_video_with_worker(&mut worker, &session_id).await;
        let shutdown_result = worker_sidecar::shutdown_worker(worker).await;
        let cleanup_error = close_result
            .err()
            .or_else(|| shutdown_result.err());
        state.remove_session(&session_id);
        if let Some(error) = cleanup_error {
            return Err(error);
        }
        return Ok(true);
    }

    state.remove_session(&session_id);
    Ok(true)
}

/// Get frame metadata for a specific frame index within a session.
#[tauri::command]
pub async fn get_frame_meta(
    state: State<'_, AppState>,
    session_id: String,
    frame_index: u32,
) -> Result<video_worker::FrameMeta, String> {
    let session = state
        .get_session(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;
    session.summary
        .frames
        .get(frame_index as usize)
        .cloned()
        .ok_or_else(|| {
            format!(
                "frame index {frame_index} out of range (total: {})",
                session.summary.total_frames
            )
        })
}

#[tauri::command]
pub async fn get_frame(
    state: State<'_, AppState>,
    session_id: String,
    frame_index: u32,
) -> Result<FrameHandle, String> {
    let session = state
        .get_session(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;

    let mut worker = session.worker.lock().await;
    let worker = worker
        .as_mut()
        .ok_or_else(|| format!("worker closed for session: {session_id}"))?;

    let frame = worker_sidecar::get_frame_with_worker(worker, &session_id, frame_index).await?;
    session.set_current_frame_index(frame.frame_index);
    Ok(frame)
}

#[tauri::command]
pub async fn read_frame_buffer(
    state: State<'_, AppState>,
    session_id: String,
    frame_id: String,
) -> Result<tauri::ipc::Response, String> {
    let session = state
        .get_session(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;

    let mut worker = session.worker.lock().await;
    let worker = worker
        .as_mut()
        .ok_or_else(|| format!("worker closed for session: {session_id}"))?;

    let buffer = worker_sidecar::read_frame_buffer_with_worker(worker, &session_id, &frame_id).await?;
    let rgba = std::fs::read(&buffer.rgba_path)
        .map_err(|error| format!("failed to read frame buffer {}: {error}", buffer.rgba_path))?;
    Ok(tauri::ipc::Response::new(rgba))
}

#[tauri::command]
pub async fn step_frame(
    state: State<'_, AppState>,
    session_id: String,
    delta: i32,
) -> Result<FrameHandle, String> {
    let session = state
        .get_session(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;

    let mut worker = session.worker.lock().await;
    let worker = worker
        .as_mut()
        .ok_or_else(|| format!("worker closed for session: {session_id}"))?;

    let frame = worker_sidecar::step_frame_with_worker(worker, &session_id, delta).await?;
    session.set_current_frame_index(frame.frame_index);
    Ok(frame)
}
