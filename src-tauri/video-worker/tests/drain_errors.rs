use ffmpeg_sys_next as ffi;
use video_worker::ffmpeg::{pts_to_us, AVERROR_EAGAIN, AVERROR_EOF};

#[test]
fn averror_constants_match_ffmpeg_conventions() {
    assert_eq!(AVERROR_EAGAIN, -11);
    assert!(AVERROR_EOF < 0);
    assert_ne!(AVERROR_EAGAIN, AVERROR_EOF);
}

#[test]
fn pts_to_us_handles_extreme_values() {
    let tb = ffi::AVRational { num: 1, den: 1 };
    assert_eq!(pts_to_us(1, tb), 1_000_000);

    let result = pts_to_us(i64::MAX, tb);
    assert_eq!(result, i64::MAX);

    let zero_tb = ffi::AVRational { num: 1, den: 0 };
    assert_eq!(pts_to_us(100, zero_tb), 0);
}
