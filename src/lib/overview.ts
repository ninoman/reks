/**
 * Overview waveform drawing helpers. The columns come from Rust
 * (`reks_audio::overview`); this only fits them to a pixel width.
 */

/** `[min, max]` pairs from the `overview_waveform` bytes. */
export function overviewColumns(bytes: ArrayBuffer): Float32Array {
  return new Float32Array(bytes, 0, Math.floor(bytes.byteLength / 8) * 2);
}

/**
 * Min/max per pixel for `width` pixels: each pixel takes the extremes of
 * the columns under it. Returns `[min0, max0, min1, max1, …]`.
 */
export function fitColumns(columns: Float32Array, width: number): Float32Array {
  const n = columns.length / 2;
  const out = new Float32Array(Math.max(0, width) * 2);
  if (n === 0) return out;
  for (let x = 0; x < width; x++) {
    const from = Math.floor((x * n) / width);
    const to = Math.max(from + 1, Math.floor(((x + 1) * n) / width));
    let min = 0;
    let max = 0;
    for (let i = from; i < to && i < n; i++) {
      min = Math.min(min, columns[2 * i] ?? 0);
      max = Math.max(max, columns[2 * i + 1] ?? 0);
    }
    out[2 * x] = min;
    out[2 * x + 1] = max;
  }
  return out;
}
