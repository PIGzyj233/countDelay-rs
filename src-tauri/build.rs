use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
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
    stage_sidecar().expect("failed to stage video-worker sidecar");
    tauri_build::build()
}

fn stage_sidecar() -> Result<(), String> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|error| error.to_string())?);
    let repo_root = manifest_dir
        .parent()
        .ok_or_else(|| format!("failed to resolve repo root from {}", manifest_dir.display()))?;
    let target = env::var("TARGET").map_err(|error| error.to_string())?;
    let profile = env::var("PROFILE").map_err(|error| error.to_string())?;
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let worker_manifest = manifest_dir.join("video-worker/Cargo.toml");
    let binaries_dir = manifest_dir.join("binaries");
    let ffmpeg_bin_dir = repo_root.join("third_party/ffmpeg/windows-x86_64/bin");
    let worker_profile = if profile == "release" { "release" } else { "debug" };
    let exe_suffix = if target.contains("windows") { ".exe" } else { "" };

    println!("cargo:rerun-if-changed={}", worker_manifest.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("video-worker/src").display()
    );
    println!("cargo:rerun-if-changed={}", ffmpeg_bin_dir.display());

    fs::create_dir_all(&binaries_dir).map_err(|error| {
        format!(
            "failed to create binaries directory {}: {error}",
            binaries_dir.display()
        )
    })?;

    let status = Command::new(cargo)
        .current_dir(&manifest_dir)
        .arg("build")
        .arg("--manifest-path")
        .arg(&worker_manifest)
        .arg("--target")
        .arg(&target)
        .args(if worker_profile == "release" {
            vec!["--release"]
        } else {
            Vec::new()
        })
        .status()
        .map_err(|error| format!("failed to launch cargo for video-worker: {error}"))?;

    if !status.success() {
        return Err(format!(
            "video-worker cargo build failed with status {:?}",
            status.code()
        ));
    }

    let built_sidecar = manifest_dir.join(format!(
        "video-worker/target/{target}/{worker_profile}/video-worker{exe_suffix}"
    ));
    let bundled_sidecar = binaries_dir.join(format!("video-worker-{target}{exe_suffix}"));
    copy_file(&built_sidecar, &bundled_sidecar)?;

    for dll in FFMPEG_DLLS {
        let source = ffmpeg_bin_dir.join(dll);
        if source.is_file() {
            let destination = binaries_dir.join(dll);
            copy_file(&source, &destination)?;
        }
    }

    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!("expected file does not exist: {}", source.display()));
    }

    fs::copy(source, destination).map_err(|error| {
        format!(
            "failed to copy {} -> {}: {error}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}
