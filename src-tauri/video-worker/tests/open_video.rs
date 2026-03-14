use std::path::PathBuf;

use video_worker::ffmpeg::{open_decoder_session, open_video_session};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/videos")
        .join(name)
        .components()
        .collect()
}

#[test]
fn open_cfr_fixture_builds_frame_index() {
    let path = fixture_path("cfr_30fps_10frames.mp4");
    assert!(path.is_file(), "fixture missing: {}", path.display());

    let session = open_video_session(&path).expect("open_video_session");

    assert_eq!(session.width, 64);
    assert_eq!(session.height, 48);
    assert!(session.total_frames >= 9, "expected at least 9 frames, got {}", session.total_frames);
    assert!(session.total_frames <= 11, "expected at most 11 frames, got {}", session.total_frames);
    assert!(!session.session_id.is_empty());
    assert!(!session.codec_name.is_empty());
    assert_eq!(session.decode_errors, 0);

    // Verify frame index properties
    assert_eq!(session.frames.len(), session.total_frames as usize);

    // Frame indices should be sequential 0..N
    for (i, frame) in session.frames.iter().enumerate() {
        assert_eq!(frame.frame_index, i as u32, "frame_index mismatch at {i}");
    }

    // All frames from a well-formed MP4 should have valid timestamps
    for frame in &session.frames {
        assert!(
            frame.pts.is_some(),
            "frame {} should have a valid PTS in well-formed MP4",
            frame.frame_index,
        );
        assert!(
            frame.timestamp_us.is_some(),
            "frame {} should have a valid timestamp_us in well-formed MP4",
            frame.frame_index,
        );
    }

    // Timestamps should be monotonically non-decreasing (presentation order)
    let timestamps: Vec<i64> = session.frames.iter()
        .filter_map(|f| f.timestamp_us)
        .collect();
    for window in timestamps.windows(2) {
        assert!(
            window[1] >= window[0],
            "timestamps not monotonic: {}us then {}us",
            window[0], window[1],
        );
    }

    // First frame should be a keyframe
    assert!(session.frames[0].is_keyframe, "first frame should be a keyframe");

    // First frame timestamp should be 0 or very close
    let first_ts = session.frames[0].timestamp_us.unwrap();
    assert!(
        first_ts.abs() < 1000,
        "first frame timestamp should be near 0, got {first_ts}",
    );
}

#[test]
fn open_real_video_fixture_builds_frame_index() {
    let path = fixture_path("video.mp4");
    if !path.is_file() {
        eprintln!("skipping: video.mp4 fixture not present");
        return;
    }

    let session = open_video_session(&path).expect("open_video_session");

    assert_eq!(session.width, 720);
    assert_eq!(session.height, 1570);
    assert_eq!(session.codec_name, "hevc");
    assert!(session.total_frames > 0, "should have decoded at least one frame");
    assert!(session.duration_us.is_some(), "should have duration");
    assert_eq!(session.decode_errors, 0, "well-formed video should have no decode errors");

    // Frame indices should be sequential
    for (i, frame) in session.frames.iter().enumerate() {
        assert_eq!(frame.frame_index, i as u32);
    }

    // All frames should have valid timestamps
    let all_have_ts = session.frames.iter().all(|f| f.timestamp_us.is_some());
    assert!(all_have_ts, "all frames in a well-formed MP4 should have timestamps");

    // Timestamps should be monotonically non-decreasing
    let timestamps: Vec<i64> = session.frames.iter()
        .filter_map(|f| f.timestamp_us)
        .collect();
    for window in timestamps.windows(2) {
        assert!(
            window[1] >= window[0],
            "timestamps not monotonic: {}us then {}us",
            window[0], window[1],
        );
    }

    let frames_without_pts = session.frames.iter().filter(|f| f.pts.is_none()).count();
    eprintln!(
        "video.mp4: {} frames, duration={}us, decode_errors={}, frames_without_pts={}, \
         first_ts={}us, last_ts={}us",
        session.total_frames,
        session.duration_us.unwrap_or(-1),
        session.decode_errors,
        frames_without_pts,
        session.frames.first().and_then(|f| f.timestamp_us).unwrap_or(-1),
        session.frames.last().and_then(|f| f.timestamp_us).unwrap_or(-1),
    );
}

#[test]
fn open_video_protocol_roundtrip() {
    let path = fixture_path("cfr_30fps_10frames.mp4");
    assert!(path.is_file(), "fixture missing: {}", path.display());

    let request = video_worker::WorkerRequest::OpenVideo {
        path: path.to_string_lossy().into_owned(),
    };
    let response = video_worker::handle_request(request);

    match response {
        video_worker::WorkerResponse::OpenVideoResult { session } => {
            assert!(session.total_frames >= 9);
            assert_eq!(session.width, 64);
            assert_eq!(session.decode_errors, 0);
        }
        video_worker::WorkerResponse::Error { message } => {
            panic!("unexpected error: {message}");
        }
        other => panic!("unexpected response type: {other:?}"),
    }
}

#[test]
fn frame_meta_has_best_effort_timestamp() {
    let path = fixture_path("cfr_30fps_10frames.mp4");
    assert!(path.is_file(), "fixture missing: {}", path.display());

    let session = open_video_session(&path).expect("open_video_session");

    for frame in &session.frames {
        assert!(
            frame.best_effort_timestamp_us.is_some(),
            "frame {} should have best_effort_timestamp_us",
            frame.frame_index,
        );

        if let (Some(ts), Some(best_effort)) = (frame.timestamp_us, frame.best_effort_timestamp_us)
        {
            assert_eq!(
                ts, best_effort,
                "frame {} timestamps should match in well-formed video",
                frame.frame_index,
            );
        }
    }
}

#[test]
fn decoder_session_decodes_forward_and_backward_frames() {
    let path = fixture_path("cfr_30fps_10frames.mp4");
    assert!(path.is_file(), "fixture missing: {}", path.display());

    let mut session = open_decoder_session(&path).expect("open_decoder_session");
    assert!(session.summary().total_frames >= 9);

    let frame5 = session.decode_frame(5).expect("decode frame 5");
    assert_eq!(frame5.width, 64);
    assert_eq!(frame5.height, 48);
    assert_eq!(frame5.rgba.len(), (64 * 48 * 4) as usize);

    let frame2 = session.decode_frame(2).expect("decode frame 2");
    assert_eq!(frame2.width, 64);
    assert_eq!(frame2.height, 48);
    assert_eq!(frame2.rgba.len(), (64 * 48 * 4) as usize);
}
