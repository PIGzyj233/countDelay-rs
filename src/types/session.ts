/** Mirrors Rust `commands::SessionInfo` — returned by `open_video`. */
export interface SessionInfo {
  session_id: string;
  path: string;
  width: number;
  height: number;
  duration_us: number | null;
  codec_name: string;
  total_frames: number;
  avg_frame_rate_num: number;
  avg_frame_rate_den: number;
  decode_errors: number;
}

/** Mirrors Rust `video_worker::FrameMeta`. */
export interface FrameMeta {
  frame_index: number;
  pts: number | null;
  timestamp_us: number | null;
  best_effort_timestamp_us: number | null;
  is_keyframe: boolean;
}

/** Mirrors Rust `video_worker::FrameHandle` — returned by `get_frame`/`step_frame`. */
export interface FrameHandle {
  frame_id: string;
  frame_index: number;
  width: number;
  height: number;
  timestamp_us: number | null;
  is_precise: boolean;
  rgba_path: string;
}
