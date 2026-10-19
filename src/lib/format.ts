/** Display helpers for table cells. Presentation only. */

export function formatBpm(bpmX100: number | null): string {
  return bpmX100 === null ? "" : (bpmX100 / 100).toFixed(2);
}

export function formatDuration(ms: number | null): string {
  if (ms === null) return "";
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export function formatRating(rating: number): string {
  return "★".repeat(rating);
}
