# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

countDelay-rs is a Windows-first desktop app for measuring frame-level video delay (trigger-to-response latency). It is being migrated from a single-file PySide6/OpenCV Python tool (`countDelay.py`) to Tauri v2 + React 19 + Rust + FFmpeg. The core measurement semantics are: open a local video, step through frames, mark trigger/response frames with keyboard hotkeys, compute delay from real PTS timestamps (not `frame/fps`).

**Current status:** Milestones A-C complete. A: real FFmpeg sidecar probe pipeline. B: `open_video` session with full frame index. C: persistent worker session, seek/decode/RGBA pipeline, frame-buffer ownership, adjacent-frame cache warmup, and core-side binary frame reads. Milestone D (frontend parity and measurement UX rebuild) is not started.

## Build & Development Commands

**Prerequisites:** Rust toolchain (MSVC), Node.js + pnpm, LLVM/libclang (for ffmpeg-sys-next bindgen).

```bash
# Validate vendored FFmpeg SDK layout
pnpm verify:ffmpeg-sdk

# Build the video-worker sidecar (also validates SDK first)
pnpm build:sidecar

# Full Tauri dev mode (builds sidecar, starts vite + tauri window)
pnpm tauri dev

# Production build (frontend + sidecar + installer)
pnpm tauri build
```

### Running Tests

```bash
# All Rust tests (Tauri core + video-worker)
cargo test --manifest-path src-tauri/Cargo.toml

# Video-worker tests only
cargo test -p video-worker

# Single test with output
cargo test -p video-worker -- --nocapture test_name

# Frontend tests (not yet implemented, will use vitest)
pnpm test
```

### Cargo Notes

There is no root workspace `Cargo.toml`. The Rust entry point is `src-tauri/Cargo.toml`, which depends on `video-worker` via `path = "video-worker"`. Always use `--manifest-path src-tauri/Cargo.toml` when running cargo commands from the repo root.

## Architecture

```
Frontend (React/TS, src/)
  ──invoke──▶  Tauri Core (src-tauri/src/)
                  ──stdin/stdout JSON-lines──▶  video-worker sidecar (src-tauri/video-worker/)
                                                   ──FFI──▶  FFmpeg libs (ffmpeg-sys-next)
```

### Three-layer separation

1. **Frontend** (`src/`): UI, keyboard shortcuts (A/D/Z/C/X/Space/S/Q), measurement table, results display. Currently still default Tauri template — not yet migrated.
2. **Tauri Core** (`src-tauri/src/`): Tauri commands, app state, sidecar lifecycle, system integration (dialog, clipboard). Exposes `#[tauri::command]` functions called from the frontend.
3. **video-worker** (`src-tauri/video-worker/`): Standalone binary that links FFmpeg directly via `ffmpeg-sys-next`. Communicates with Tauri core over stdin/stdout JSON-lines protocol. Runs as a Tauri sidecar for process isolation — FFmpeg/unsafe crashes don't take down the UI.

### Key source files

| File | Role |
|------|------|
| `src-tauri/src/lib.rs` | Tauri app builder, command registration (`ffmpeg_sdk_report`, worker probe/ping, video session commands) |
| `src-tauri/src/ffmpeg_sdk.rs` | Validates vendored FFmpeg SDK layout (headers, libs, DLLs, version) |
| `src-tauri/src/state.rs` | App-level session registry and current-frame bookkeeping |
| `src-tauri/src/worker_sidecar.rs` | Spawns sidecar, validates handshake, applies request timeouts, and sends/receives JSON-lines protocol messages |
| `src-tauri/video-worker/src/lib.rs` | Worker protocol types (`WorkerHello`, `WorkerRequest`, `WorkerResponse`, `SessionSummary`, `FrameHandle`, `FrameBuffer`) |
| `src-tauri/video-worker/src/worker.rs` | Persistent worker session manager, frame-buffer cache, `GetFrame`/`ReadFrameBuffer`/`StepFrame` routing |
| `src-tauri/video-worker/src/ffmpeg.rs` | Raw FFmpeg FFI for probe, frame indexing, persistent decoder sessions, seek/decode, and RGBA conversion |
| `src-tauri/build.rs` | Build-time: compiles video-worker, stages sidecar binary + FFmpeg DLLs into `src-tauri/binaries/` |

### Sidecar protocol (JSON-lines over stdin/stdout)

- **Requests:** `Ping`, `Probe { path }`, `OpenVideo { path }`, `GetFrame { session_id, frame_index }`, `ReadFrameBuffer { session_id, frame_id }`, `StepFrame { session_id, delta }`, `CloseVideo { session_id }`, `Shutdown`
- **Responses:** `Hello` (handshake with linked FFmpeg versions), `ProbeResult`, `OpenVideoResult`, `GetFrameResult`, `ReadFrameBufferResult`, `StepFrameResult`, `CloseVideoResult`, `RequestError`, `Ack`, `Error`
- Worker modes: `--ping` (one-shot handshake) or stdio (persistent session)

### Shared types

The `video-worker` crate is both a library and a binary. Tauri core imports `video_worker::*` types (`WorkerHello`, `VideoProbe`, etc.) for deserialization. This means protocol types are defined once in `video-worker/src/lib.rs` and shared across both sides.

## FFmpeg Integration

- **Vendored SDK:** `third_party/ffmpeg/windows-x86_64/` (v8.0.1, GPL v3, GCC-built)
- **Environment:** `FFMPEG_DIR` is set in `.cargo/config.toml` (relative path), consumed by `ffmpeg-sys-next` for linking
- **Required DLLs:** avcodec-62, avdevice-62, avfilter-11, avformat-62, avutil-60, swresample-6, swscale-9
- **Build scripts** (`src-tauri/build.rs`, `video-worker/build.rs`) copy DLLs to output dirs so tests and the Tauri bundle can find them
- **libclang** is required for `ffmpeg-sys-next` bindgen — the SDK validation script checks for it

## Key Design Decisions

- **`ffmpeg-sys-next` over `ffmpeg-next`:** Low-level FFI chosen because the high-level crate is in maintenance mode and this project needs precise control over seek, PTS, frame caching, and pixel format conversion.
- **Sidecar over in-process:** Process isolation protects the UI from FFmpeg/unsafe crashes. If IPC latency breaks the ≤60ms single-step target, the protocol and module boundaries are designed to allow collapsing into an in-process thread.
- **Windows-first:** Primary target is `x86_64-pc-windows-msvc`. macOS/Linux support is deferred.
- **No root workspace yet:** `video-worker` lives under `src-tauri/` instead of a top-level `crates/` dir. May be restructured later.

## Migration Plan & Specs

- Implementation plan: `docs/superpowers/plans/2026-03-13-tauri-rust-ffmpeg-migration.md`
- Architecture spec: `docs/superpowers/specs/2026-03-13-tauri-rust-ffmpeg-design.md`
