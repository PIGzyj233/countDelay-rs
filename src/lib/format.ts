/** Convert microseconds to milliseconds string with 2 decimal places. */
export function usToMs(us: number | null): string {
  if (us == null) return "—";
  return (us / 1000).toFixed(2);
}

/** Convert microseconds to seconds string with 3 decimal places. */
export function usToSec(us: number | null): string {
  if (us == null) return "—";
  return (us / 1_000_000).toFixed(3);
}

/** Format frame rate from numerator/denominator. */
export function formatFrameRate(num: number, den: number): string {
  if (den === 0) return "—";
  return (num / den).toFixed(2);
}

/** Format a table row for clipboard export. */
export function formatMeasurementRow(
  index: number,
  triggerFrame: number,
  responseFrame: number,
  delayUs: number | null
): string {
  const delayMs = delayUs != null ? (delayUs / 1000).toFixed(2) : "—";
  return `${index}\t${triggerFrame}\t${responseFrame}\t${delayMs}`;
}

/** Format average row for clipboard export. */
export function formatAverageRow(
  count: number,
  avgUs: number,
  minUs: number,
  maxUs: number
): string {
  return `AVG(${count})\t${(avgUs / 1000).toFixed(2)}\t${(minUs / 1000).toFixed(2)}\t${(maxUs / 1000).toFixed(2)}`;
}
