use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::async_runtime::Mutex as AsyncMutex;
use video_worker::SessionSummary;

use crate::worker_sidecar::WorkerClient;

pub struct VideoSession {
    pub summary: SessionSummary,
    pub worker: AsyncMutex<Option<WorkerClient>>,
    current_frame_index: Mutex<Option<u32>>,
}

#[derive(Default)]
pub struct AppState {
    sessions: Mutex<HashMap<String, Arc<VideoSession>>>,
}

impl AppState {
    pub fn insert_session(&self, summary: SessionSummary, worker: WorkerClient) -> Arc<VideoSession> {
        let session_id = summary.session_id.clone();
        let session = Arc::new(VideoSession {
            summary,
            worker: AsyncMutex::new(Some(worker)),
            current_frame_index: Mutex::new(None),
        });
        self.sessions
            .lock()
            .expect("session lock poisoned")
            .insert(session_id, session.clone());
        session
    }

    pub fn get_session(&self, session_id: &str) -> Option<Arc<VideoSession>> {
        self.sessions
            .lock()
            .expect("session lock poisoned")
            .get(session_id)
            .cloned()
    }

    pub fn remove_session(&self, session_id: &str) -> Option<Arc<VideoSession>> {
        self.sessions
            .lock()
            .expect("session lock poisoned")
            .remove(session_id)
    }
}

impl VideoSession {
    pub fn set_current_frame_index(&self, frame_index: u32) {
        *self
            .current_frame_index
            .lock()
            .expect("current frame lock poisoned") = Some(frame_index);
    }
}
