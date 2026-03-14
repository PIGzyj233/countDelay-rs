import { useRef, useEffect } from "react";

interface FrameCanvasProps {
  rgbaBuffer: ArrayBuffer | null;
  width: number;
  height: number;
}

export function FrameCanvas({ rgbaBuffer, width, height }: FrameCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !rgbaBuffer || width === 0 || height === 0) return;

    canvas.width = width;
    canvas.height = height;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const imageData = new ImageData(
      new Uint8ClampedArray(rgbaBuffer),
      width,
      height
    );
    ctx.putImageData(imageData, 0, 0);
  }, [rgbaBuffer, width, height]);

  if (!rgbaBuffer) {
    return (
      <div className="frame-canvas-placeholder">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5" />
          <rect x="2" y="6" width="14" height="12" rx="2" />
        </svg>
        <span>打开视频以开始</span>
      </div>
    );
  }

  return (
    <canvas
      ref={canvasRef}
      className="frame-canvas"
      style={{ maxWidth: "100%", height: "auto" }}
    />
  );
}
