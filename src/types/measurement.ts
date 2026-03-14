/** A single delay measurement: trigger frame → response frame. */
export interface Measurement {
  id: number;
  trigger_frame: number;
  trigger_timestamp_us: number | null;
  response_frame: number;
  response_timestamp_us: number | null;
  delay_us: number | null;
}

/** Computed average of a set of measurements. */
export interface AverageResult {
  id: number;
  count: number;
  avg_delay_us: number;
  min_delay_us: number;
  max_delay_us: number;
}

/** Union type for table rows (individual measurement or average summary). */
export type TableRow =
  | { kind: "measurement"; data: Measurement }
  | { kind: "average"; data: AverageResult };

/** State machine for the two-step Space marking flow. */
export type MarkingState =
  | { step: "idle" }
  | { step: "trigger_set"; trigger_frame: number; trigger_timestamp_us: number | null };
