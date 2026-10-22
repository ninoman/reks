/**
 * A canvas's size in whole device pixels, from its wrapper's CSS box, so the
 * canvas can be sized 1:1 with the screen's pixels.
 */
import { useEffect, useState } from "react";

/** A canvas's size in device pixels, from its wrapper's CSS box. */
export interface CanvasSize {
  width: number;
  height: number;
  dpr: number;
}

export function useCanvasSize(el: HTMLElement | null): CanvasSize | null {
  const [size, setSize] = useState<CanvasSize | null>(null);
  useEffect(() => {
    if (el === null) return;
    const observer = new ResizeObserver(([entry]) => {
      const dpr = window.devicePixelRatio || 1;
      const box = entry?.contentRect;
      // Whole device pixels, and the canvas sized to exactly that: a canvas
      // stretched by a fraction of a pixel is resampled, which blurs it.
      const width = Math.floor((box?.width ?? 0) * dpr);
      const height = Math.floor((box?.height ?? 0) * dpr);
      setSize((s) =>
        s?.width === width && s.height === height && s.dpr === dpr ? s : { width, height, dpr },
      );
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [el]);
  return size;
}
