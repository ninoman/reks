/**
 * Transport bar under the library: play/pause, the loaded track, elapsed and
 * remaining time, the overview waveform as the position slider, volume and
 * output device. Everything it shows comes from `usePlayer`.
 */
import { type KeyboardEvent, type PointerEvent, useEffect, useRef, useState } from "react";

import { formatDuration } from "../lib/format";
import { fitColumns } from "../lib/overview";
import { usePlayer } from "../store/player";

/** Arrow-key step on the position slider. */
const SEEK_STEP_MS = 5_000;

export function TransportBar() {
  const status = usePlayer((s) => s.status);
  const track = usePlayer((s) => s.track);
  const loading = usePlayer((s) => s.loading);
  const error = usePlayer((s) => s.error);
  const devices = usePlayer((s) => s.devices);
  const overview = usePlayer((s) =>
    s.overview !== null && s.overview.trackId === s.status.trackId ? s.overview.columns : null,
  );
  const { toggle, seek, setVolume, setDevice, refreshDevices } = usePlayer.getState();

  useEffect(() => {
    void refreshDevices();
  }, [refreshDevices]);

  const remaining = Math.max(0, status.durationMs - status.positionMs);
  const title = track === null ? "" : (track.title ?? "Untitled");

  return (
    <section className="transport" aria-label="Player">
      <button
        type="button"
        className="transport-play"
        disabled={!status.loaded}
        aria-label={status.playing ? "Pause" : "Play"}
        onClick={() => void toggle()}
      >
        {status.playing ? "❚❚" : "▶"}
      </button>
      <div className="transport-track" title={title}>
        {loading !== null ? (
          <span className="muted">Loading…</span>
        ) : track === null ? (
          <span className="muted">Double-click a track to play it</span>
        ) : (
          <>
            <span className="transport-title">{title}</span>
            {track.artist !== null && <span className="muted"> — {track.artist}</span>}
          </>
        )}
      </div>
      <span className="transport-time">{formatDuration(status.positionMs)}</span>
      <OverviewSlider
        columns={overview}
        positionMs={status.positionMs}
        durationMs={status.durationMs}
        disabled={!status.loaded}
        onSeek={(ms) => void seek(ms)}
      />
      <span className="transport-time">-{formatDuration(remaining)}</span>
      <input
        className="transport-volume"
        type="range"
        min={0}
        max={1}
        step={0.01}
        value={status.volume}
        aria-label="Volume"
        onChange={(e) => void setVolume(Number(e.target.value))}
      />
      <select
        className="transport-device"
        aria-label="Output device"
        value={status.device ?? ""}
        onFocus={() => void refreshDevices()}
        onChange={(e) => void setDevice(e.target.value === "" ? null : e.target.value)}
      >
        <option value="">System default</option>
        {devices.map((d) => (
          <option key={d} value={d}>
            {d}
          </option>
        ))}
      </select>
      {(error !== null || status.deviceLost) && (
        <span className="transport-error" role="alert">
          {status.deviceLost ? "Audio device disconnected" : error}
        </span>
      )}
    </section>
  );
}

interface OverviewSliderProps {
  columns: Float32Array | null;
  positionMs: number;
  durationMs: number;
  disabled: boolean;
  onSeek: (ms: number) => void;
}

/**
 * The overview waveform, drawn on a canvas. Seeks the moment the pointer goes
 * down and keeps seeking while it drags (DJ-style scrub, no wait for release).
 */
function OverviewSlider({
  columns,
  positionMs,
  durationMs,
  disabled,
  onSeek,
}: OverviewSliderProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  /** Position (0..1) under the pointer while dragging; every move seeks. */
  const [drag, setDrag] = useState<number | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas === null) return;
    const observer = new ResizeObserver(([entry]) =>
      setWidth(Math.floor(entry?.contentRect.width ?? 0)),
    );
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  const played = drag ?? (durationMs > 0 ? Math.min(1, positionMs / durationMs) : 0);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (canvas == null || ctx == null || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    const height = canvas.clientHeight;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const style = getComputedStyle(canvas);
    const accent = style.getPropertyValue("--accent").trim() || "#2f6fde";
    const muted = style.getPropertyValue("--muted").trim() || "#6e6e73";
    const mid = height / 2;
    const playhead = Math.round(played * width);
    if (columns !== null) {
      const peaks = fitColumns(columns, width);
      for (let x = 0; x < width; x++) {
        const top = mid - (peaks[2 * x + 1] ?? 0) * mid;
        const bottom = mid - (peaks[2 * x] ?? 0) * mid;
        ctx.fillStyle = x < playhead ? accent : muted;
        ctx.fillRect(x, top, 1, Math.max(1, bottom - top));
      }
    } else {
      ctx.fillStyle = muted;
      ctx.fillRect(0, mid, width, 1);
    }
    if (durationMs > 0) {
      ctx.fillStyle = style.getPropertyValue("--fg").trim() || "#1d1d1f";
      ctx.fillRect(Math.min(playhead, width - 1), 0, 1, height);
    }
  }, [columns, played, width, durationMs]);

  function ratioAt(e: PointerEvent<HTMLCanvasElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
  }

  function onKeyDown(e: KeyboardEvent<HTMLCanvasElement>) {
    const step = { ArrowLeft: -SEEK_STEP_MS, ArrowRight: SEEK_STEP_MS }[e.key];
    if (step !== undefined) {
      e.preventDefault();
      onSeek(Math.min(durationMs, Math.max(0, positionMs + step)));
    } else if (e.key === "Home") {
      e.preventDefault();
      onSeek(0);
    }
  }

  return (
    <canvas
      ref={canvasRef}
      className="transport-overview"
      role="slider"
      tabIndex={disabled ? -1 : 0}
      aria-label="Position"
      aria-disabled={disabled}
      aria-valuemin={0}
      aria-valuemax={Math.round(durationMs / 1000)}
      aria-valuenow={Math.round(positionMs / 1000)}
      aria-valuetext={`${formatDuration(positionMs)} of ${formatDuration(durationMs)}`}
      onPointerDown={(e) => {
        if (disabled) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        const ratio = ratioAt(e);
        setDrag(ratio);
        onSeek(ratio * durationMs);
      }}
      onPointerMove={(e) => {
        if (drag === null) return;
        const ratio = ratioAt(e);
        if (ratio === drag) return;
        setDrag(ratio);
        onSeek(ratio * durationMs);
      }}
      onPointerUp={() => setDrag(null)}
      onPointerCancel={() => setDrag(null)}
      onKeyDown={disabled ? undefined : onKeyDown}
    />
  );
}
