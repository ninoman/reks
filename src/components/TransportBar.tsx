/**
 * Transport bar under the library: play/pause, the loaded track, elapsed and
 * remaining time, the overview waveform as the position slider, volume and
 * output device. Everything it shows comes from `usePlayer`.
 */
import { type KeyboardEvent, type PointerEvent, useEffect, useRef, useState } from "react";

import { formatDuration } from "../lib/format";
import { fitColumns } from "../lib/overview";
import { usePlayer } from "../store/player";
import { PauseIcon, PlayIcon } from "./icons";
import { focusRing } from "./ui/control";
import { IconButton } from "./ui/IconButton";
import { Select } from "./ui/Select";
import { Slider } from "./ui/Slider";

const time = "min-w-11 flex-none text-center font-mono text-xs text-fg-muted tabular-nums";

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
    <section
      className="flex h-(--transport-h) flex-none items-center gap-2.5 border-t border-border bg-bg-subtle px-3"
      aria-label="Player"
    >
      <IconButton
        size="lg"
        shape="round"
        variant="secondary"
        disabled={!status.loaded}
        aria-label={status.playing ? "Pause" : "Play"}
        onClick={() => void toggle()}
      >
        {status.playing ? <PauseIcon /> : <PlayIcon />}
      </IconButton>
      <div className="w-55 flex-none truncate" title={title}>
        {loading !== null ? (
          <span className="text-fg-muted">Loading…</span>
        ) : track === null ? (
          <span className="text-fg-muted">Double-click a track to play it</span>
        ) : (
          <>
            <span className="font-semibold">{title}</span>
            {track.artist !== null && <span className="text-fg-muted"> — {track.artist}</span>}
          </>
        )}
      </div>
      <span className={time}>{formatDuration(status.positionMs)}</span>
      <OverviewSlider
        columns={overview}
        positionMs={status.positionMs}
        durationMs={status.durationMs}
        disabled={!status.loaded}
        onSeek={(ms) => void seek(ms)}
      />
      <span className={time}>-{formatDuration(remaining)}</span>
      <Slider
        className="w-22.5 flex-none"
        min={0}
        max={1}
        step={0.01}
        value={status.volume}
        aria-label="Volume"
        onValueChange={(v) => void setVolume(v)}
      />
      <Select
        size="sm"
        className="max-w-40 flex-none"
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
      </Select>
      {(error !== null || status.deviceLost) && (
        <span className="flex-none text-danger" role="alert">
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
    // Colours from tokens.css (the canvas can't use classes).
    const playedColour = style.getPropertyValue("--color-waveform-played").trim();
    const restColour = style.getPropertyValue("--color-waveform-rest").trim();
    const mid = height / 2;
    const playhead = Math.round(played * width);
    if (columns !== null) {
      const peaks = fitColumns(columns, width);
      for (let x = 0; x < width; x++) {
        const top = mid - (peaks[2 * x + 1] ?? 0) * mid;
        const bottom = mid - (peaks[2 * x] ?? 0) * mid;
        ctx.fillStyle = x < playhead ? playedColour : restColour;
        ctx.fillRect(x, top, 1, Math.max(1, bottom - top));
      }
    } else {
      ctx.fillStyle = restColour;
      ctx.fillRect(0, mid, width, 1);
    }
    if (durationMs > 0) {
      ctx.fillStyle = style.getPropertyValue("--color-waveform-playhead").trim();
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
      className={`h-9 min-w-0 flex-1 cursor-pointer aria-disabled:cursor-default ${focusRing}`}
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
