import { invoke } from "@tauri-apps/api/core";
import type { SessionInfo, FrameHandle, FrameMeta } from "../types/session";

export async function openVideo(path: string): Promise<SessionInfo> {
  return invoke<SessionInfo>("open_video", { path });
}

export async function closeVideo(sessionId: string): Promise<boolean> {
  return invoke<boolean>("close_video", { sessionId });
}

export async function getFrame(
  sessionId: string,
  frameIndex: number
): Promise<FrameHandle> {
  return invoke<FrameHandle>("get_frame", { sessionId, frameIndex });
}

export async function stepFrame(
  sessionId: string,
  delta: number
): Promise<FrameHandle> {
  return invoke<FrameHandle>("step_frame", { sessionId, delta });
}

export async function readFrameBuffer(
  sessionId: string,
  frameId: string
): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("read_frame_buffer", { sessionId, frameId });
}

export async function getFrameMeta(
  sessionId: string,
  frameIndex: number
): Promise<FrameMeta> {
  return invoke<FrameMeta>("get_frame_meta", { sessionId, frameIndex });
}
