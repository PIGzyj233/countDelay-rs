use std::path::{Path, PathBuf};

use video_worker::{
    WorkerErrorCode, WorkerRequest, WorkerResponse, WorkerServer,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/videos")
        .join(name)
        .components()
        .collect()
}

fn open_session(worker: &mut WorkerServer, fixture_name: &str) -> video_worker::SessionSummary {
    let path = fixture_path(fixture_name);
    assert!(path.is_file(), "fixture missing: {}", path.display());

    match worker.handle_request(WorkerRequest::OpenVideo {
        path: path.to_string_lossy().into_owned(),
    }) {
        WorkerResponse::OpenVideoResult { session } => session,
        other => panic!("unexpected response: {other:?}"),
    }
}

fn frame_id_for(session_id: &str, frame: &video_worker::FrameMeta) -> String {
    let timestamp = frame
        .best_effort_timestamp_us
        .or(frame.timestamp_us)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!("{session_id}:{}:{timestamp}", frame.frame_index)
}

#[test]
fn get_frame_returns_exact_index_timestamp_and_rgba_buffer() {
    let mut worker = WorkerServer::with_cache_capacity(4);
    let session = open_session(&mut worker, "cfr_30fps_10frames.mp4");

    let frame = match worker.handle_request(WorkerRequest::GetFrame {
        session_id: session.session_id.clone(),
        frame_index: 4,
    }) {
        WorkerResponse::GetFrameResult { frame } => frame,
        other => panic!("unexpected response: {other:?}"),
    };

    assert_eq!(frame.frame_index, 4);
    assert!(!frame.frame_id.is_empty());
    assert_eq!(frame.timestamp_us, session.frames[4].timestamp_us);
    assert!(Path::new(&frame.rgba_path).is_file());

    let buffer = match worker.handle_request(WorkerRequest::ReadFrameBuffer {
        session_id: session.session_id.clone(),
        frame_id: frame.frame_id.clone(),
    }) {
        WorkerResponse::ReadFrameBufferResult { buffer } => buffer,
        other => panic!("unexpected response: {other:?}"),
    };

    assert_eq!(buffer.frame_id, frame.frame_id);
    assert_eq!(buffer.width, session.width);
    assert_eq!(buffer.height, session.height);
    assert_eq!(buffer.rgba_path, frame.rgba_path);
    assert!(Path::new(&buffer.rgba_path).is_file());
    let rgba_size = std::fs::metadata(&buffer.rgba_path).unwrap().len();
    assert_eq!(rgba_size, (session.width * session.height * 4) as u64);
}

#[test]
fn get_frame_prefetches_adjacent_buffers() {
    let mut worker = WorkerServer::with_cache_capacity(8);
    let session = open_session(&mut worker, "cfr_25fps_60frames.mp4");

    match worker.handle_request(WorkerRequest::GetFrame {
        session_id: session.session_id.clone(),
        frame_index: 10,
    }) {
        WorkerResponse::GetFrameResult { frame } => {
            assert_eq!(frame.frame_index, 10);
        }
        other => panic!("unexpected response: {other:?}"),
    }

    for index in [9_usize, 11_usize] {
        let frame_id = frame_id_for(&session.session_id, &session.frames[index]);
        match worker.handle_request(WorkerRequest::ReadFrameBuffer {
            session_id: session.session_id.clone(),
            frame_id,
        }) {
            WorkerResponse::ReadFrameBufferResult { buffer } => {
                assert_eq!(buffer.width, session.width);
                assert_eq!(buffer.height, session.height);
            }
            other => panic!("adjacent frame {index} should be prefetched, got {other:?}"),
        }
    }
}

#[test]
fn step_frame_moves_relative_to_current_frame_without_off_by_one() {
    let mut worker = WorkerServer::with_cache_capacity(4);
    let session = open_session(&mut worker, "cfr_30fps_10frames.mp4");

    let initial = match worker.handle_request(WorkerRequest::GetFrame {
        session_id: session.session_id.clone(),
        frame_index: 3,
    }) {
        WorkerResponse::GetFrameResult { frame } => frame,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(initial.frame_index, 3);

    let forward = match worker.handle_request(WorkerRequest::StepFrame {
        session_id: session.session_id.clone(),
        delta: 1,
    }) {
        WorkerResponse::StepFrameResult { frame } => frame,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(forward.frame_index, 4);
    assert_eq!(forward.timestamp_us, session.frames[4].timestamp_us);

    let backward = match worker.handle_request(WorkerRequest::StepFrame {
        session_id: session.session_id.clone(),
        delta: -2,
    }) {
        WorkerResponse::StepFrameResult { frame } => frame,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(backward.frame_index, 2);
    assert_eq!(backward.timestamp_us, session.frames[2].timestamp_us);
}

#[test]
fn evicted_frame_buffer_returns_frame_expired_error() {
    let mut worker = WorkerServer::with_cache_capacity(2);
    let session = open_session(&mut worker, "cfr_30fps_10frames.mp4");

    let frame0 = match worker.handle_request(WorkerRequest::GetFrame {
        session_id: session.session_id.clone(),
        frame_index: 0,
    }) {
        WorkerResponse::GetFrameResult { frame } => frame,
        other => panic!("unexpected response: {other:?}"),
    };

    for index in 1..=2 {
        match worker.handle_request(WorkerRequest::GetFrame {
            session_id: session.session_id.clone(),
            frame_index: index,
        }) {
            WorkerResponse::GetFrameResult { .. } => {}
            other => panic!("unexpected response: {other:?}"),
        }
    }

    match worker.handle_request(WorkerRequest::ReadFrameBuffer {
        session_id: session.session_id.clone(),
        frame_id: frame0.frame_id.clone(),
    }) {
        WorkerResponse::RequestError { error } => {
            assert_eq!(error.code, WorkerErrorCode::FrameExpired);
            assert!(error.message.contains(&frame0.frame_id));
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn tampered_frame_id_returns_invalid_frame_id_error() {
    let mut worker = WorkerServer::with_cache_capacity(2);
    let session = open_session(&mut worker, "cfr_30fps_10frames.mp4");

    let frame = match worker.handle_request(WorkerRequest::GetFrame {
        session_id: session.session_id.clone(),
        frame_index: 1,
    }) {
        WorkerResponse::GetFrameResult { frame } => frame,
        other => panic!("unexpected response: {other:?}"),
    };

    let tampered_frame_id = frame.frame_id.replacen(":1:", ":9:", 1);
    match worker.handle_request(WorkerRequest::ReadFrameBuffer {
        session_id: session.session_id.clone(),
        frame_id: tampered_frame_id,
    }) {
        WorkerResponse::RequestError { error } => {
            assert_eq!(error.code, WorkerErrorCode::InvalidFrameId);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn bframe_navigation_keeps_visible_ordinals_and_timestamps_aligned() {
    let mut worker = WorkerServer::with_cache_capacity(4);
    let session = open_session(&mut worker, "bframes_30fps.mp4");

    for target_index in [0_u32, 1, 5, 11, 17, 23, 29] {
        let frame = match worker.handle_request(WorkerRequest::GetFrame {
            session_id: session.session_id.clone(),
            frame_index: target_index,
        }) {
            WorkerResponse::GetFrameResult { frame } => frame,
            other => panic!("unexpected response: {other:?}"),
        };

        assert_eq!(frame.frame_index, target_index);
        assert_eq!(
            frame.timestamp_us,
            session.frames[target_index as usize].timestamp_us,
            "timestamp mismatch at visible frame {target_index}",
        );
    }

    let stepped = match worker.handle_request(WorkerRequest::StepFrame {
        session_id: session.session_id.clone(),
        delta: -3,
    }) {
        WorkerResponse::StepFrameResult { frame } => frame,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(stepped.frame_index, 26);
    assert_eq!(stepped.timestamp_us, session.frames[26].timestamp_us);
}

#[test]
fn step_frame_without_current_frame_returns_explicit_error() {
    let mut worker = WorkerServer::with_cache_capacity(4);
    let session = open_session(&mut worker, "cfr_30fps_10frames.mp4");

    match worker.handle_request(WorkerRequest::StepFrame {
        session_id: session.session_id.clone(),
        delta: 1,
    }) {
        WorkerResponse::RequestError { error } => {
            assert_eq!(error.code, WorkerErrorCode::NoCurrentFrame);
            assert!(error.message.contains(&session.session_id));
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn close_video_releases_session_and_rejects_follow_up_requests() {
    let mut worker = WorkerServer::with_cache_capacity(4);
    let session = open_session(&mut worker, "cfr_30fps_10frames.mp4");

    let frame = match worker.handle_request(WorkerRequest::GetFrame {
        session_id: session.session_id.clone(),
        frame_index: 2,
    }) {
        WorkerResponse::GetFrameResult { frame } => frame,
        other => panic!("unexpected response: {other:?}"),
    };
    assert!(Path::new(&frame.rgba_path).is_file());

    match worker.handle_request(WorkerRequest::CloseVideo {
        session_id: session.session_id.clone(),
    }) {
        WorkerResponse::CloseVideoResult { closed } => assert!(closed),
        other => panic!("unexpected response: {other:?}"),
    }
    assert!(!Path::new(&frame.rgba_path).exists());

    match worker.handle_request(WorkerRequest::GetFrame {
        session_id: session.session_id.clone(),
        frame_index: 2,
    }) {
        WorkerResponse::RequestError { error } => {
            assert_eq!(error.code, WorkerErrorCode::SessionNotFound);
        }
        other => panic!("unexpected response: {other:?}"),
    }

    match worker.handle_request(WorkerRequest::ReadFrameBuffer {
        session_id: session.session_id.clone(),
        frame_id: frame.frame_id,
    }) {
        WorkerResponse::RequestError { error } => {
            assert_eq!(error.code, WorkerErrorCode::SessionNotFound);
        }
        other => panic!("unexpected response: {other:?}"),
    }

    match worker.handle_request(WorkerRequest::CloseVideo {
        session_id: session.session_id.clone(),
    }) {
        WorkerResponse::CloseVideoResult { closed } => assert!(!closed),
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn sequential_step_latency_benchmark() {
    let mut worker = WorkerServer::with_cache_capacity(16);
    let session = open_session(&mut worker, "cfr_25fps_60frames.mp4");

    match worker.handle_request(WorkerRequest::GetFrame {
        session_id: session.session_id.clone(),
        frame_index: 0,
    }) {
        WorkerResponse::GetFrameResult { .. } => {}
        other => panic!("unexpected response: {other:?}"),
    }

    let total_steps = 20_u32.min(session.total_frames.saturating_sub(1));
    let start = std::time::Instant::now();
    for _ in 0..total_steps {
        match worker.handle_request(WorkerRequest::StepFrame {
            session_id: session.session_id.clone(),
            delta: 1,
        }) {
            WorkerResponse::StepFrameResult { .. } => {}
            other => panic!("unexpected response: {other:?}"),
        }
    }
    let elapsed = start.elapsed();
    let per_step_ms = elapsed.as_secs_f64() * 1000.0 / f64::from(total_steps.max(1));
    eprintln!(
        "Sequential step latency: {:.2} ms/step over {} steps",
        per_step_ms, total_steps
    );
}
