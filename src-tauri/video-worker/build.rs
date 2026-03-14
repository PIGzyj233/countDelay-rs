use std::{
    env, fs,
    path::{Path, PathBuf},
};

const FFMPEG_DLLS: &[&str] = &[
    "avcodec-62.dll",
    "avdevice-62.dll",
    "avfilter-11.dll",
    "avformat-62.dll",
    "avutil-60.dll",
    "swresample-6.dll",
    "swscale-9.dll",
];

fn main() {
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR"));
    let sdk_root = env::var("FFMPEG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest_dir.join("../../third_party/ffmpeg/windows-x86_64"));
    let bin_dir = sdk_root.join("bin");
    println!("cargo:rerun-if-changed={}", bin_dir.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("missing OUT_DIR"));
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("failed to resolve cargo profile directory");
    let deps_dir = profile_dir.join("deps");

    copy_dlls(&bin_dir, profile_dir);
    if deps_dir.is_dir() {
        copy_dlls(&bin_dir, &deps_dir);
    }
}

fn copy_dlls(bin_dir: &Path, destination_dir: &Path) {
    fs::create_dir_all(destination_dir).expect("failed to create destination directory for FFmpeg DLLs");

    for dll in FFMPEG_DLLS {
        let source = bin_dir.join(dll);
        if !source.is_file() {
            panic!("required FFmpeg DLL is missing: {}", source.display());
        }
        let destination = destination_dir.join(dll);
        fs::copy(&source, &destination).unwrap_or_else(|error| {
            panic!(
                "failed to copy FFmpeg DLL {} to {}: {error}",
                source.display(),
                destination.display()
            )
        });
    }
}
