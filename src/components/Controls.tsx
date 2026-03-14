import type { SessionInfo, FrameHandle } from "../types/session";
import type { MarkingState } from "../types/measurement";
import { usToMs, formatFrameRate } from "../lib/format";

interface ControlsProps {
  session: SessionInfo | null;
  currentFrame: FrameHandle | null;
  marking: MarkingState;
  onOpenVideo: () => void;
}

export function Controls({
  session,
  currentFrame,
  marking,
  onOpenVideo,
}: ControlsProps) {
  return (
    <div className="controls">
      <button className="btn-primary" onClick={onOpenVideo}>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
          <path d="M14 2v4a2 2 0 0 0 2 2h4" />
          <path d="m10 11 5 3-5 3v-6Z" />
        </svg>
        打开视频
      </button>

      {session && (
        <div className="info-section">
          <span className="section-label">视频信息</span>
          <dl className="info-grid">
            <dt>文件</dt>
            <dd title={session.path}>{session.path.split(/[/\\]/).pop()}</dd>
            <dt>分辨率</dt>
            <dd>
              {session.width}&times;{session.height}
            </dd>
            <dt>编码</dt>
            <dd>{session.codec_name}</dd>
            <dt>帧率</dt>
            <dd>
              {formatFrameRate(
                session.avg_frame_rate_num,
                session.avg_frame_rate_den
              )}{" "}
              fps
            </dd>
            <dt>总帧数</dt>
            <dd>{session.total_frames.toLocaleString()}</dd>
            <dt>时长</dt>
            <dd>{usToMs(session.duration_us)} ms</dd>
            {session.decode_errors > 0 && (
              <>
                <dt>解码错误</dt>
                <dd className="error-count">{session.decode_errors}</dd>
              </>
            )}
          </dl>
        </div>
      )}

      {currentFrame && session && (
        <div className="info-section">
          <span className="section-label">当前帧</span>
          <dl className="info-grid">
            <dt>帧序号</dt>
            <dd>
              {currentFrame.frame_index} / {session.total_frames - 1}
            </dd>
            <dt>时间戳</dt>
            <dd>{usToMs(currentFrame.timestamp_us)} ms</dd>
          </dl>
        </div>
      )}

      {marking.step === "trigger_set" && (
        <div className="marking-callout">
          已标记触发帧 {marking.trigger_frame}，按{" "}
          <kbd>Space</kbd> 标记响应帧
        </div>
      )}

      <div className="hotkey-section">
        <span className="section-label">快捷键</span>
        <ul className="hotkey-list">
          <li>
            <span className="hotkey-keys">
              <kbd>A</kbd>
              <span className="hotkey-sep">/</span>
              <kbd>D</kbd>
            </span>
            &plusmn;1 帧
          </li>
          <li>
            <span className="hotkey-keys">
              <kbd>Z</kbd>
              <span className="hotkey-sep">/</span>
              <kbd>C</kbd>
            </span>
            &plusmn;10 帧
          </li>
          <li>
            <span className="hotkey-keys">
              <kbd>X</kbd>
            </span>
            &minus;100 帧
          </li>
          <li>
            <span className="hotkey-keys">
              <kbd>Space</kbd>
            </span>
            标记触发/响应帧
          </li>
          <li>
            <span className="hotkey-keys">
              <kbd>S</kbd>
            </span>
            删除末行
          </li>
          <li>
            <span className="hotkey-keys">
              <kbd>Q</kbd>
            </span>
            计算均值
          </li>
          <li>
            <span className="hotkey-keys">
              <kbd>Ctrl+C</kbd>
            </span>
            复制表格
          </li>
        </ul>
      </div>
    </div>
  );
}
