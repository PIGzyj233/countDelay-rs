mod commands;
mod ffmpeg_sdk;
mod state;
mod worker_sidecar;

use ffmpeg_sdk::{default_sdk_root, inspect_sdk, FfmpegSdkReport};
use state::AppState;
use video_worker::{VideoProbe, WorkerHello};

#[tauri::command]
fn ffmpeg_sdk_report() -> FfmpegSdkReport {
    inspect_sdk(&default_sdk_root())
}

#[tauri::command]
async fn ping_video_worker(app: tauri::AppHandle) -> Result<WorkerHello, String> {
    worker_sidecar::ping_video_worker(&app).await
}

#[tauri::command]
async fn probe_video_with_worker(
    app: tauri::AppHandle,
    path: String,
) -> Result<VideoProbe, String> {
    worker_sidecar::probe_video_with_worker(&app, &path).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            ffmpeg_sdk_report,
            ping_video_worker,
            probe_video_with_worker,
            commands::open_video,
            commands::close_video,
            commands::get_frame_meta,
            commands::get_frame,
            commands::read_frame_buffer,
            commands::step_frame,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
