/** A waveform style drawn over a made-up groove, in the theme's deck colours. */
import { useEffect, useRef } from "react";

import type { DeckWaveform } from "../../../lib/ipc";
import { sampleWave } from "../../../lib/sampleWave";
import { prepare, waveBars, wavePalette } from "../../../lib/waveDraw";
import { useTheme } from "../../../store/settings";

export function WaveformPreview({ style }: { style: DeckWaveform }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const theme = useTheme();
  // biome-ignore lint/correctness/useExhaustiveDependencies: `theme` re-reads the waveform colours.
  useEffect(() => {
    const canvas = ref.current;
    if (canvas === null) return;
    const { clientWidth: width, clientHeight: height } = canvas;
    const drawing = prepare(canvas, width, height);
    if (drawing === null) return;
    const { bands, mono } = sampleWave(width);
    const mid = height / 2;
    for (const bar of waveBars(style, bands, mono, height, wavePalette(drawing.token))) {
      drawing.ctx.fillStyle = bar.colour;
      drawing.ctx.fillRect(bar.x, mid - bar.half, 1, 2 * bar.half);
    }
  }, [style, theme]);
  return (
    <span className="size-full px-2 py-1.5">
      <canvas ref={ref} className="size-full" />
    </span>
  );
}
