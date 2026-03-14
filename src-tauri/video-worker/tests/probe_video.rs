use std::{path::{Path, PathBuf}, process::Command};

use tempfile::tempdir;
use video_worker::ffmpeg::{linked_versions, probe_video};

#[test]
fn linked_versions_are_available_from_runtime() {
    let versions = linked_versions();

    assert!(versions.avcodec > 0);
    assert!(versions.avformat > 0);
    assert!(versions.avutil > 0);
    assert!(versions.swscale > 0);
}

#[test]
fn probe_video_reads_stream_metadata_from_generated_fixture() {
    let temp_dir = tempdir().expect("temp dir");
    let video_path = temp_dir.path().join("probe-fixture.mp4");
    generate_fixture(&video_path);

    let probe = probe_video(&video_path).expect("probe video");

    assert_eq!(probe.width, 64);
    assert_eq!(probe.height, 48);
    assert_eq!(probe.video_stream_index, 0);
    assert!(probe.duration_us.is_some());
    assert!(!probe.codec_name.is_empty());
}

fn generate_fixture(output_path: &Path) {
    let ffmpeg_exe = sdk_root().join("bin/ffmpeg.exe");
    assert!(ffmpeg_exe.is_file(), "ffmpeg.exe missing at {}", ffmpeg_exe.display());

    let status = Command::new(ffmpeg_exe)
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc=size=64x48:rate=5:duration=1")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-c:v")
        .arg("mpeg4")
        .arg("-y")
        .arg(output_path)
        .status()
        .expect("run ffmpeg");

    assert!(status.success(), "fixture generation failed with status {:?}", status.code());
}

fn sdk_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/ffmpeg/windows-x86_64")
        .components()
        .collect()
}
