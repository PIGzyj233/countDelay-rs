import { useState, useCallback, useRef } from "react";
import type { SessionInfo, FrameHandle } from "../types/session";
import type {
  Measurement,
  AverageResult,
  TableRow,
  MarkingState,
} from "../types/measurement";
import * as api from "../lib/tauri";

export interface SessionState {
  /** Current open session, or null if no video loaded. */
  session: SessionInfo | null;
  /** Current frame handle from the worker. */
  currentFrame: FrameHandle | null;
  /** RGBA pixel buffer for the current frame. */
  rgbaBuffer: ArrayBuffer | null;
  /** Table rows: measurements + averages in insertion order. */
  tableRows: TableRow[];
  /** Current marking state machine. */
  marking: MarkingState;
  /** Whether an async operation is in progress. */
  loading: boolean;
  /** Last error message. */
  error: string | null;

  openVideo: (path: string) => Promise<void>;
  closeVideo: () => Promise<void>;
  goToFrame: (frameIndex: number) => Promise<void>;
  stepFrame: (delta: number) => Promise<void>;
  markFrame: () => void;
  deleteLastRow: () => void;
  computeAverage: () => void;
  getClipboardText: (selectedIndices?: number[]) => string;
}

let nextMeasurementId = 1;
let nextAverageId = 1;

export function useSession(): SessionState {
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [currentFrame, setCurrentFrame] = useState<FrameHandle | null>(null);
  const [rgbaBuffer, setRgbaBuffer] = useState<ArrayBuffer | null>(null);
  const [tableRows, setTableRows] = useState<TableRow[]>([]);
  const [marking, setMarking] = useState<MarkingState>({ step: "idle" });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Prevent concurrent frame requests from racing
  const pendingRef = useRef(false);

  const resetSessionState = useCallback(() => {
    pendingRef.current = false;
    setSession(null);
    setCurrentFrame(null);
    setRgbaBuffer(null);
    setTableRows([]);
    setMarking({ step: "idle" });
    nextMeasurementId = 1;
    nextAverageId = 1;
  }, []);

  const loadFrameBuffer = useCallback(
    async (sessionId: string, frame: FrameHandle) => {
      const buffer = await api.readFrameBuffer(sessionId, frame.frame_id);
      setRgbaBuffer(buffer);
    },
    []
  );

  const openVideo = useCallback(
    async (path: string) => {
      const previousSessionId = session?.session_id ?? null;
      let openedSessionId: string | null = null;

      setLoading(true);
      setError(null);
      resetSessionState();

      try {
        if (previousSessionId) {
          try {
            await api.closeVideo(previousSessionId);
          } catch {
            // Best-effort cleanup before opening a replacement session.
          }
        }

        const info = await api.openVideo(path);
        openedSessionId = info.session_id;

        const frame = await api.getFrame(info.session_id, 0);
        const buffer = await api.readFrameBuffer(info.session_id, frame.frame_id);

        setSession(info);
        setCurrentFrame(frame);
        setRgbaBuffer(buffer);
      } catch (e) {
        if (openedSessionId) {
          try {
            await api.closeVideo(openedSessionId);
          } catch {
            // Ignore cleanup failures after a partially opened session.
          }
        }
        resetSessionState();
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [resetSessionState, session]
  );

  const closeVideo = useCallback(async () => {
    const sessionId = session?.session_id ?? null;
    resetSessionState();
    if (!sessionId) return;
    try {
      await api.closeVideo(sessionId);
    } catch {
      // Ignore close errors
    }
  }, [resetSessionState, session]);

  const goToFrame = useCallback(
    async (frameIndex: number) => {
      if (!session || pendingRef.current) return;
      pendingRef.current = true;
      setError(null);
      try {
        const frame = await api.getFrame(session.session_id, frameIndex);
        setCurrentFrame(frame);
        await loadFrameBuffer(session.session_id, frame);
      } catch (e) {
        setError(String(e));
      } finally {
        pendingRef.current = false;
      }
    },
    [session, loadFrameBuffer]
  );

  const stepFrame = useCallback(
    async (delta: number) => {
      if (!session || pendingRef.current) return;
      pendingRef.current = true;
      setError(null);
      try {
        const frame = await api.stepFrame(session.session_id, delta);
        setCurrentFrame(frame);
        await loadFrameBuffer(session.session_id, frame);
      } catch (e) {
        setError(String(e));
      } finally {
        pendingRef.current = false;
      }
    },
    [session, loadFrameBuffer]
  );

  const markFrame = useCallback(() => {
    if (!currentFrame) return;

    if (marking.step === "idle") {
      // First Space: set trigger
      setMarking({
        step: "trigger_set",
        trigger_frame: currentFrame.frame_index,
        trigger_timestamp_us: currentFrame.timestamp_us,
      });
    } else if (marking.step === "trigger_set") {
      // Second Space: set response, compute delay
      const measurement: Measurement = {
        id: nextMeasurementId++,
        trigger_frame: marking.trigger_frame,
        trigger_timestamp_us: marking.trigger_timestamp_us,
        response_frame: currentFrame.frame_index,
        response_timestamp_us: currentFrame.timestamp_us,
        delay_us:
          currentFrame.timestamp_us != null && marking.trigger_timestamp_us != null
            ? currentFrame.timestamp_us - marking.trigger_timestamp_us
            : null,
      };
      setTableRows((prev) => [
        ...prev,
        { kind: "measurement", data: measurement },
      ]);
      setMarking({ step: "idle" });
    }
  }, [currentFrame, marking]);

  const deleteLastRow = useCallback(() => {
    setTableRows((prev) => {
      if (prev.length === 0) return prev;
      return prev.slice(0, -1);
    });
  }, []);

  const computeAverage = useCallback(() => {
    const measurements = tableRows
      .filter((r): r is { kind: "measurement"; data: Measurement } => r.kind === "measurement")
      .map((r) => r.data)
      .filter((m) => m.delay_us != null);

    if (measurements.length === 0) return;

    const delays = measurements.map((m) => m.delay_us!);
    const avg: AverageResult = {
      id: nextAverageId++,
      count: delays.length,
      avg_delay_us: delays.reduce((a, b) => a + b, 0) / delays.length,
      min_delay_us: Math.min(...delays),
      max_delay_us: Math.max(...delays),
    };
    setTableRows((prev) => [...prev, { kind: "average", data: avg }]);
  }, [tableRows]);

  const getClipboardText = useCallback(
    (selectedIndices?: number[]) => {
      const rows =
        selectedIndices && selectedIndices.length > 0
          ? selectedIndices.map((i) => tableRows[i]).filter(Boolean)
          : tableRows;

      const header = "#\tTrigger\tResponse\tDelay(ms)";
      const lines = rows.map((row) => {
        if (row.kind === "measurement") {
          const m = row.data;
          const delay = m.delay_us != null ? (m.delay_us / 1000).toFixed(2) : "—";
          return `${m.id}\t${m.trigger_frame}\t${m.response_frame}\t${delay}`;
        } else {
          const a = row.data;
          return `AVG(${a.count})\t—\t—\t${(a.avg_delay_us / 1000).toFixed(2)}`;
        }
      });
      return [header, ...lines].join("\n");
    },
    [tableRows]
  );

  return {
    session,
    currentFrame,
    rgbaBuffer,
    tableRows,
    marking,
    loading,
    error,
    openVideo,
    closeVideo,
    goToFrame,
    stepFrame,
    markFrame,
    deleteLastRow,
    computeAverage,
    getClipboardText,
  };
}
