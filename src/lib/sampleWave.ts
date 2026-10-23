/**
 * A made-up one-bar groove for the waveform style previews in Settings:
 * per-pixel band peaks (`[low, mid, high]`, as `bandPeaks` returns) and mono
 * peaks, all 0..1, so `waveBars` can draw each style without a real track.
 */

const BEATS = 4;

export function sampleWave(width: number): { bands: Float32Array; mono: Float32Array } {
  const bands = new Float32Array(3 * width);
  const mono = new Float32Array(width);
  const beat = width / BEATS;
  for (let x = 0; x < width; x++) {
    const kick = Math.exp(-((x % beat) / beat) * 4);
    const low = 0.1 + 0.85 * kick;
    const mid = 0.15 + 0.2 * Math.sin(x * 0.3) ** 2 + 0.15 * kick;
    const high = 0.08 + 0.3 * Math.abs(Math.sin(x * 1.3)) * (1 - 0.6 * kick);
    bands[3 * x] = low;
    bands[3 * x + 1] = mid;
    bands[3 * x + 2] = high;
    mono[x] = 0.95 * Math.max(low, mid, high);
  }
  return { bands, mono };
}
