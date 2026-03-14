import type { TableRow } from "../types/measurement";

interface MeasurementTableProps {
  rows: TableRow[];
}

export function MeasurementTable({ rows }: MeasurementTableProps) {
  if (rows.length === 0) {
    return (
      <div className="measurement-section">
        <div className="measurement-empty">
          暂无测量数据
          <span className="measurement-empty-hint">
            按 <kbd>Space</kbd> 标记触发帧和响应帧
          </span>
        </div>
      </div>
    );
  }

  return (
    <div className="measurement-section">
      <table className="measurement-table">
        <thead>
          <tr>
            <th>#</th>
            <th>触发帧</th>
            <th>响应帧</th>
            <th>延迟 (ms)</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row, i) => {
            if (row.kind === "measurement") {
              const m = row.data;
              const delay =
                m.delay_us != null ? (m.delay_us / 1000).toFixed(2) : "\u2014";
              return (
                <tr key={`m-${i}`}>
                  <td>{m.id}</td>
                  <td>{m.trigger_frame}</td>
                  <td>{m.response_frame}</td>
                  <td>{delay}</td>
                </tr>
              );
            } else {
              const a = row.data;
              return (
                <tr key={`a-${i}`} className="average-row">
                  <td colSpan={3}>均值 ({a.count} 次)</td>
                  <td>{(a.avg_delay_us / 1000).toFixed(2)}</td>
                </tr>
              );
            }
          })}
        </tbody>
      </table>
    </div>
  );
}
