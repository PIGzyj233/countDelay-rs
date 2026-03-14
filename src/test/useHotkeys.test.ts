import { describe, it, expect, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { useHotkeys, type HotkeyActions } from "../hooks/useHotkeys";

function createMockActions(): HotkeyActions {
  return {
    stepFrame: vi.fn(),
    markFrame: vi.fn(),
    deleteLastRow: vi.fn(),
    computeAverage: vi.fn(),
    copyToClipboard: vi.fn(),
  };
}

function fireKey(key: string, opts?: Partial<KeyboardEvent>) {
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key, bubbles: true, ...opts })
  );
}

describe("useHotkeys", () => {
  it("D steps forward 1 frame", () => {
    const actions = createMockActions();
    renderHook(() => useHotkeys(actions));
    fireKey("d");
    expect(actions.stepFrame).toHaveBeenCalledWith(1);
  });

  it("A steps backward 1 frame", () => {
    const actions = createMockActions();
    renderHook(() => useHotkeys(actions));
    fireKey("a");
    expect(actions.stepFrame).toHaveBeenCalledWith(-1);
  });

  it("C steps forward 10 frames", () => {
    const actions = createMockActions();
    renderHook(() => useHotkeys(actions));
    fireKey("c");
    expect(actions.stepFrame).toHaveBeenCalledWith(10);
  });

  it("Z steps backward 10 frames", () => {
    const actions = createMockActions();
    renderHook(() => useHotkeys(actions));
    fireKey("z");
    expect(actions.stepFrame).toHaveBeenCalledWith(-10);
  });

  it("X steps backward 100 frames", () => {
    const actions = createMockActions();
    renderHook(() => useHotkeys(actions));
    fireKey("x");
    expect(actions.stepFrame).toHaveBeenCalledWith(-100);
  });

  it("Space marks a frame", () => {
    const actions = createMockActions();
    renderHook(() => useHotkeys(actions));
    fireKey(" ");
    expect(actions.markFrame).toHaveBeenCalled();
  });

  it("S deletes last row", () => {
    const actions = createMockActions();
    renderHook(() => useHotkeys(actions));
    fireKey("s");
    expect(actions.deleteLastRow).toHaveBeenCalled();
  });

  it("Q computes average", () => {
    const actions = createMockActions();
    renderHook(() => useHotkeys(actions));
    fireKey("q");
    expect(actions.computeAverage).toHaveBeenCalled();
  });

  it("Ctrl+C copies to clipboard", () => {
    const actions = createMockActions();
    renderHook(() => useHotkeys(actions));
    fireKey("c", { ctrlKey: true });
    expect(actions.copyToClipboard).toHaveBeenCalled();
  });

  it("Ctrl+C does NOT step forward", () => {
    const actions = createMockActions();
    renderHook(() => useHotkeys(actions));
    fireKey("c", { ctrlKey: true });
    expect(actions.stepFrame).not.toHaveBeenCalled();
  });
});
