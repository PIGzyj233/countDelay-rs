import { useEffect } from "react";

export interface HotkeyActions {
  stepFrame: (delta: number) => void;
  markFrame: () => void;
  deleteLastRow: () => void;
  computeAverage: () => void;
  copyToClipboard: () => void;
}

/**
 * Global hotkey listener matching the legacy countDelay.py keybindings:
 * - D: forward 1 frame
 * - A: backward 1 frame
 * - C: forward 10 frames
 * - Z: backward 10 frames
 * - X: backward 100 frames
 * - Space: mark trigger/response frame
 * - S: delete last measurement
 * - Q: compute average
 * - Ctrl+C: copy table to clipboard
 */
export function useHotkeys(actions: HotkeyActions) {
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      // Skip if user is typing in an input
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      // Ctrl+C: clipboard copy
      if (e.key === "c" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        actions.copyToClipboard();
        return;
      }

      // Skip other combos
      if (e.ctrlKey || e.metaKey || e.altKey) return;

      switch (e.key.toLowerCase()) {
        case "d":
          e.preventDefault();
          actions.stepFrame(1);
          break;
        case "a":
          e.preventDefault();
          actions.stepFrame(-1);
          break;
        case "c":
          e.preventDefault();
          actions.stepFrame(10);
          break;
        case "z":
          e.preventDefault();
          actions.stepFrame(-10);
          break;
        case "x":
          e.preventDefault();
          actions.stepFrame(-100);
          break;
        case " ":
          e.preventDefault();
          actions.markFrame();
          break;
        case "s":
          e.preventDefault();
          actions.deleteLastRow();
          break;
        case "q":
          e.preventDefault();
          actions.computeAverage();
          break;
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [actions]);
}
