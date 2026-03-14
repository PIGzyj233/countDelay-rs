import { useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { FrameCanvas } from "./components/FrameCanvas";
import { Controls } from "./components/Controls";
import { MeasurementTable } from "./components/MeasurementTable";
import { useSession } from "./hooks/useSession";
import { useHotkeys } from "./hooks/useHotkeys";
import "./App.css";

function App() {
  const s = useSession();

  const handleOpenVideo = useCallback(async () => {
    const path = await open({
      title: "打开视频",
      filters: [
        {
          name: "视频文件",
          extensions: ["mp4", "mkv", "avi", "mov", "webm", "ts", "flv"],
        },
      ],
    });
    if (path) {
      await s.openVideo(path);
    }
  }, [s]);

  const handleCopyToClipboard = useCallback(async () => {
    const text = s.getClipboardText();
    try {
      await writeText(text);
    } catch {
      // Silently fail if clipboard not available
    }
  }, [s]);

  useHotkeys({
    stepFrame: s.stepFrame,
    markFrame: s.markFrame,
    deleteLastRow: s.deleteLastRow,
    computeAverage: s.computeAverage,
    copyToClipboard: handleCopyToClipboard,
  });

  return (
    <main className="app">
      <div className="viewer-panel">
        <FrameCanvas
          rgbaBuffer={s.rgbaBuffer}
          width={s.currentFrame?.width ?? 0}
          height={s.currentFrame?.height ?? 0}
        />
        {s.error && <div className="error-bar">{s.error}</div>}
        {s.loading && <div className="loading-bar">加载中...</div>}
      </div>

      <div className="side-panel">
        <Controls
          session={s.session}
          currentFrame={s.currentFrame}
          marking={s.marking}
          onOpenVideo={handleOpenVideo}
        />
        <MeasurementTable rows={s.tableRows} />
      </div>
    </main>
  );
}

export default App;
