use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;

const REQUIRED_LIBS: &[&str] = &[
    "avcodec.lib",
    "avdevice.lib",
    "avfilter.lib",
    "avformat.lib",
    "avutil.lib",
    "swresample.lib",
    "swscale.lib",
];

const REQUIRED_DLLS: &[&str] = &[
    "avcodec-62.dll",
    "avdevice-62.dll",
    "avfilter-11.dll",
    "avformat-62.dll",
    "avutil-60.dll",
    "swresample-6.dll",
    "swscale-9.dll",
];

const REQUIRED_HEADERS: &[&str] = &[
    "libavcodec/avcodec.h",
    "libavformat/avformat.h",
    "libavutil/avutil.h",
    "libswscale/swscale.h",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SdkIssue {
    pub severity: IssueSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct BuildInfo {
    pub version_line: Option<String>,
    pub built_with: Option<String>,
    pub configuration_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FfmpegSdkReport {
    pub root: String,
    pub include_dir: String,
    pub lib_dir: String,
    pub bin_dir: String,
    pub build_info: BuildInfo,
    pub libclang_path: Option<String>,
    pub required_headers: Vec<String>,
    pub required_libs: Vec<String>,
    pub required_dlls: Vec<String>,
    pub issues: Vec<SdkIssue>,
}

pub fn default_sdk_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../third_party/ffmpeg/windows-x86_64")
        .components()
        .collect()
}

pub fn inspect_sdk(root: &Path) -> FfmpegSdkReport {
    let include_dir = root.join("include");
    let lib_dir = root.join("lib");
    let bin_dir = root.join("bin");
    let mut issues = inspect_layout(root);

    let mut build_info = BuildInfo::default();
    let ffmpeg_exe = bin_dir.join(binary_name("ffmpeg"));
    if ffmpeg_exe.is_file() {
        match Command::new(&ffmpeg_exe).arg("-version").output() {
            Ok(output) if output.status.success() => {
                let version_text = String::from_utf8_lossy(&output.stdout);
                let (parsed, parsed_issues) = analyze_version_output(&version_text);
                build_info = parsed;
                issues.extend(parsed_issues);
            }
            Ok(output) => {
                issues.push(issue(
                    IssueSeverity::Warning,
                    "ffmpeg-version-command-failed",
                    format!(
                        "{} exited with status {:?} while probing SDK version",
                        ffmpeg_exe.display(),
                        output.status.code()
                    ),
                ));
            }
            Err(error) => {
                issues.push(issue(
                    IssueSeverity::Warning,
                    "ffmpeg-version-command-missing",
                    format!(
                        "could not execute {} while probing SDK version: {error}",
                        ffmpeg_exe.display()
                    ),
                ));
            }
        }
    }

    let libclang_path = detect_libclang(&default_libclang_candidates());
    if libclang_path.is_none() {
        issues.push(issue(
            IssueSeverity::Warning,
            "missing-libclang",
            "libclang.dll was not found in LIBCLANG_PATH or common Windows LLVM locations; ffmpeg-sys-next will not build on this machine until LLVM is available".to_string(),
        ));
    }

    FfmpegSdkReport {
        root: display_path(root),
        include_dir: display_path(&include_dir),
        lib_dir: display_path(&lib_dir),
        bin_dir: display_path(&bin_dir),
        build_info,
        libclang_path: libclang_path.map(|path| display_path(&path)),
        required_headers: REQUIRED_HEADERS.iter().map(|item| (*item).to_string()).collect(),
        required_libs: REQUIRED_LIBS.iter().map(|item| (*item).to_string()).collect(),
        required_dlls: REQUIRED_DLLS.iter().map(|item| (*item).to_string()).collect(),
        issues,
    }
}

pub fn inspect_layout(root: &Path) -> Vec<SdkIssue> {
    let mut issues = Vec::new();
    let include_dir = root.join("include");
    let lib_dir = root.join("lib");
    let bin_dir = root.join("bin");

    for (dir, code) in [
        (&include_dir, "missing-include-dir"),
        (&lib_dir, "missing-lib-dir"),
        (&bin_dir, "missing-bin-dir"),
    ] {
        if !dir.is_dir() {
            issues.push(issue(
                IssueSeverity::Error,
                code,
                format!("required directory is missing: {}", dir.display()),
            ));
        }
    }

    for relative in REQUIRED_HEADERS {
        let path = include_dir.join(relative);
        if !path.is_file() {
            issues.push(issue(
                IssueSeverity::Error,
                "missing-header",
                format!("required header is missing: {}", path.display()),
            ));
        }
    }

    for file_name in REQUIRED_LIBS {
        let path = lib_dir.join(file_name);
        if !path.is_file() {
            issues.push(issue(
                IssueSeverity::Error,
                "missing-import-lib",
                format!("required import library is missing: {}", path.display()),
            ));
        }
    }

    for file_name in REQUIRED_DLLS {
        let path = bin_dir.join(file_name);
        if !path.is_file() {
            issues.push(issue(
                IssueSeverity::Error,
                "missing-runtime-dll",
                format!("required runtime DLL is missing: {}", path.display()),
            ));
        }
    }

    issues
}

pub fn analyze_version_output(text: &str) -> (BuildInfo, Vec<SdkIssue>) {
    let version_line = text.lines().find(|line| !line.trim().is_empty());
    let built_with = text
        .lines()
        .find(|line| line.starts_with("built with "))
        .map(str::to_string);
    let configuration_flags: Vec<String> = text
        .lines()
        .find_map(|line| {
            line.strip_prefix("configuration: ")
                .map(|flags| flags.split_whitespace().map(str::to_string).collect())
        })
        .unwrap_or_default();

    let mut issues = Vec::new();
    if configuration_flags.iter().any(|flag| flag == "--enable-gpl") {
        issues.push(issue(
            IssueSeverity::Warning,
            "gpl-build",
            "FFmpeg bundle was built with --enable-gpl; this is a distribution blocker if the app is meant to stay under LGPL-compatible dynamic linking terms".to_string(),
        ));
    }

    if configuration_flags
        .iter()
        .any(|flag| flag == "--enable-nonfree")
    {
        issues.push(issue(
            IssueSeverity::Warning,
            "nonfree-build",
            "FFmpeg bundle was built with --enable-nonfree; replace it before product distribution".to_string(),
        ));
    }

    if built_with
        .as_deref()
        .map(|line| line.to_ascii_lowercase().contains("gcc"))
        .unwrap_or(false)
    {
        issues.push(issue(
            IssueSeverity::Warning,
            "gcc-built-bundle",
            "FFmpeg bundle reports a GCC build; verify the shipped .lib import libraries link cleanly with the MSVC Rust target before treating this SDK as settled".to_string(),
        ));
    }

    (
        BuildInfo {
            version_line: version_line.map(str::to_string),
            built_with,
            configuration_flags,
        },
        issues,
    )
}

pub fn detect_libclang(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|path| path.is_file()).cloned()
}

fn default_libclang_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(path) = env::var("LIBCLANG_PATH") {
        let libclang_path = PathBuf::from(path);
        if libclang_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("libclang.dll"))
            .unwrap_or(false)
        {
            candidates.push(libclang_path);
        } else {
            candidates.push(libclang_path.join("libclang.dll"));
        }
    }

    if let Ok(program_files) = env::var("ProgramFiles") {
        candidates.push(Path::new(&program_files).join("LLVM/bin/libclang.dll"));
    }

    if let Ok(program_files_x86) = env::var("ProgramFiles(x86)") {
        candidates.push(Path::new(&program_files_x86).join("LLVM/bin/libclang.dll"));
    }

    candidates
}

fn binary_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn issue(severity: IssueSeverity, code: &str, message: String) -> SdkIssue {
    SdkIssue {
        severity,
        code: code.to_string(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::tempdir;

    use super::{analyze_version_output, detect_libclang, inspect_layout, IssueSeverity};

    #[test]
    fn complete_layout_has_no_errors() {
        let temp_dir = tempdir().expect("temp dir");
        let root = temp_dir.path();
        for dir in ["include/libavcodec", "include/libavformat", "include/libavutil", "include/libswscale", "lib", "bin"] {
            fs::create_dir_all(root.join(dir)).expect("create dir");
        }

        for header in [
            "include/libavcodec/avcodec.h",
            "include/libavformat/avformat.h",
            "include/libavutil/avutil.h",
            "include/libswscale/swscale.h",
        ] {
            fs::write(root.join(header), b"header").expect("write header");
        }

        for lib in [
            "lib/avcodec.lib",
            "lib/avdevice.lib",
            "lib/avfilter.lib",
            "lib/avformat.lib",
            "lib/avutil.lib",
            "lib/swresample.lib",
            "lib/swscale.lib",
        ] {
            fs::write(root.join(lib), b"lib").expect("write import lib");
        }

        for dll in [
            "bin/avcodec-62.dll",
            "bin/avdevice-62.dll",
            "bin/avfilter-11.dll",
            "bin/avformat-62.dll",
            "bin/avutil-60.dll",
            "bin/swresample-6.dll",
            "bin/swscale-9.dll",
        ] {
            fs::write(root.join(dll), b"dll").expect("write dll");
        }

        let issues = inspect_layout(root);
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn missing_layout_entries_are_reported_as_errors() {
        let temp_dir = tempdir().expect("temp dir");
        let root = temp_dir.path();
        fs::create_dir_all(root.join("include/libavcodec")).expect("create include dir");

        let issues = inspect_layout(root);
        assert!(issues
            .iter()
            .any(|issue| issue.code == "missing-lib-dir" && issue.severity == IssueSeverity::Error));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "missing-runtime-dll" && issue.severity == IssueSeverity::Error));
    }

    #[test]
    fn version_probe_flags_gpl_and_gcc_builds() {
        let (build_info, issues) = analyze_version_output(
            "ffmpeg version 8.0.1-full_build-www.gyan.dev\nbuilt with gcc 15.2.0\nconfiguration: --enable-gpl --enable-version3 --enable-shared\n",
        );

        assert_eq!(
            build_info.version_line.as_deref(),
            Some("ffmpeg version 8.0.1-full_build-www.gyan.dev")
        );
        assert!(issues.iter().any(|issue| issue.code == "gpl-build"));
        assert!(issues.iter().any(|issue| issue.code == "gcc-built-bundle"));
    }

    #[test]
    fn detect_libclang_accepts_explicit_candidate() {
        let temp_dir = tempdir().expect("temp dir");
        let candidate = temp_dir.path().join("libclang.dll");
        fs::write(&candidate, b"dll").expect("write libclang");

        let found = detect_libclang(&[PathBuf::from(&candidate)]);
        assert_eq!(found.as_deref(), Some(candidate.as_path()));
    }
}
