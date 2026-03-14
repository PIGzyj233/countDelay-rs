import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSession } from "../hooks/useSession";

const { invoke } = vi.mocked(await import("@tauri-apps/api/core"));

function makeSessionInfo(sessionId: string) {
  return {
    session_id: sessionId,
    path: `/test/${sessionId}.mp4`,
    width: 1920,
    height: 1080,
    duration_us: 5_000_000,
    codec_name: "h264",
    total_frames: 150,
    avg_frame_rate_num: 30,
    avg_frame_rate_den: 1,
    decode_errors: 0,
  };
}

function makeFrame(sessionId: string, frameIndex: number, timestampUs: number) {
  return {
    frame_id: `${sessionId}:${frameIndex}`,
    frame_index: frameIndex,
    width: 1920,
    height: 1080,
    timestamp_us: timestampUs,
    is_precise: true,
    rgba_path: `/tmp/${sessionId}-${frameIndex}.rgba`,
  };
}

describe("useSession", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("starts with no session", () => {
    const { result } = renderHook(() => useSession());
    expect(result.current.session).toBeNull();
    expect(result.current.currentFrame).toBeNull();
    expect(result.current.rgbaBuffer).toBeNull();
    expect(result.current.tableRows).toEqual([]);
    expect(result.current.marking).toEqual({ step: "idle" });
  });

  it("opens a video and loads first frame", async () => {
    const sessionInfo = makeSessionInfo("test-session");
    const frameHandle = makeFrame("test-session", 0, 0);
    const rgbaBuffer = new ArrayBuffer(1920 * 1080 * 4);

    invoke
      .mockResolvedValueOnce(sessionInfo)
      .mockResolvedValueOnce(frameHandle)
      .mockResolvedValueOnce(rgbaBuffer);

    const { result } = renderHook(() => useSession());

    await act(async () => {
      await result.current.openVideo("/test/video.mp4");
    });

    expect(result.current.session).toEqual(sessionInfo);
    expect(result.current.currentFrame).toEqual(frameHandle);
    expect(result.current.rgbaBuffer).toBe(rgbaBuffer);
  });

  it("two-step Space marking creates a measurement", async () => {
    const sessionInfo = makeSessionInfo("mark-session");
    const firstFrame = makeFrame("mark-session", 10, 1_000);
    const secondFrame = makeFrame("mark-session", 12, 5_000);

    invoke
      .mockResolvedValueOnce(sessionInfo)
      .mockResolvedValueOnce(firstFrame)
      .mockResolvedValueOnce(new ArrayBuffer(16))
      .mockResolvedValueOnce(secondFrame)
      .mockResolvedValueOnce(new ArrayBuffer(16));

    const { result } = renderHook(() => useSession());

    await act(async () => {
      await result.current.openVideo("/test/video.mp4");
    });

    act(() => {
      result.current.markFrame();
    });

    expect(result.current.marking).toEqual({
      step: "trigger_set",
      trigger_frame: 10,
      trigger_timestamp_us: 1_000,
    });

    await act(async () => {
      await result.current.stepFrame(2);
    });

    act(() => {
      result.current.markFrame();
    });

    expect(result.current.marking).toEqual({ step: "idle" });
    expect(result.current.tableRows).toEqual([
      {
        kind: "measurement",
        data: {
          id: 1,
          trigger_frame: 10,
          trigger_timestamp_us: 1_000,
          response_frame: 12,
          response_timestamp_us: 5_000,
          delay_us: 4_000,
        },
      },
    ]);
  });

  it("reopening a video closes the previous session before replacing it", async () => {
    const sessionOne = makeSessionInfo("session-1");
    const sessionTwo = makeSessionInfo("session-2");
    const frameOne = makeFrame("session-1", 0, 0);
    const frameTwo = makeFrame("session-2", 0, 2_000);
    const bufferOne = new ArrayBuffer(16);
    const bufferTwo = new ArrayBuffer(16);

    invoke
      .mockResolvedValueOnce(sessionOne)
      .mockResolvedValueOnce(frameOne)
      .mockResolvedValueOnce(bufferOne)
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(sessionTwo)
      .mockResolvedValueOnce(frameTwo)
      .mockResolvedValueOnce(bufferTwo);

    const { result } = renderHook(() => useSession());

    await act(async () => {
      await result.current.openVideo("/test/one.mp4");
    });

    await act(async () => {
      await result.current.openVideo("/test/two.mp4");
    });

    expect(invoke).toHaveBeenNthCalledWith(4, "close_video", {
      sessionId: "session-1",
    });
    expect(result.current.session).toEqual(sessionTwo);
    expect(result.current.currentFrame).toEqual(frameTwo);
    expect(result.current.rgbaBuffer).toBe(bufferTwo);
  });

  it("failed reopen clears stale frame state and closes the partial session", async () => {
    const sessionOne = makeSessionInfo("session-1");
    const sessionTwo = makeSessionInfo("session-2");
    const frameOne = makeFrame("session-1", 0, 0);
    const bufferOne = new ArrayBuffer(16);

    invoke
      .mockResolvedValueOnce(sessionOne)
      .mockResolvedValueOnce(frameOne)
      .mockResolvedValueOnce(bufferOne)
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(sessionTwo)
      .mockRejectedValueOnce(new Error("decode failed"))
      .mockResolvedValueOnce(true);

    const { result } = renderHook(() => useSession());

    await act(async () => {
      await result.current.openVideo("/test/one.mp4");
    });

    await act(async () => {
      await result.current.openVideo("/test/two.mp4");
    });

    expect(invoke).toHaveBeenNthCalledWith(7, "close_video", {
      sessionId: "session-2",
    });
    expect(result.current.session).toBeNull();
    expect(result.current.currentFrame).toBeNull();
    expect(result.current.rgbaBuffer).toBeNull();
    expect(result.current.error).toContain("decode failed");
  });

  it("deleteLastRow removes the last table entry", () => {
    const { result } = renderHook(() => useSession());

    expect(result.current.tableRows.length).toBe(0);

    act(() => result.current.deleteLastRow());

    expect(result.current.tableRows.length).toBe(0);
  });

  it("getClipboardText returns header when no rows", () => {
    const { result } = renderHook(() => useSession());
    const text = result.current.getClipboardText();
    expect(text).toBe("#\tTrigger\tResponse\tDelay(ms)");
  });
});
