/**
 * Grid mode on the zoomed waveform: a press on a section marker drags the
 * marker (stored on release; Alt: no snap); anywhere else the deck scrubs
 * as in play mode (`useDeckScrub`). While dragged, the marker is drawn where
 * the pointer is.
 */
import { type PointerEvent, useRef, useState } from "react";

import { MARKER_GRAB_PX } from "../../lib/gridMode";
import type { GridMarks } from "./deckCanvas";
import type { GridOverlay } from "./useDeckGridModes";
import type { useDeckScrub } from "./useDeckScrub";

interface Drag {
  pointerId: number;
  section: number;
  startX: number;
  fromMs: number;
  toMs: number;
  msPerPx: number;
}

export function useMarkerDrag(
  overlay: GridOverlay | null,
  positionMs: number,
  viewS: number,
  scrub: ReturnType<typeof useDeckScrub>,
): { handlers: ReturnType<typeof useDeckScrub>; marks: GridMarks | null } {
  // A ref, so the release and the lost capture after it see the same drag.
  const dragRef = useRef<Drag | null>(null);
  const [drag, setDragState] = useState<Drag | null>(null);
  const setDrag = (d: Drag | null) => {
    dragRef.current = d;
    setDragState(d);
  };

  function onPointerDown(e: PointerEvent<HTMLElement>) {
    const canvas = (e.target as Element).closest("canvas");
    if (overlay === null || e.button !== 0 || canvas === null || canvas.hasAttribute("role")) {
      scrub.onPointerDown(e);
      return;
    }
    const rect = canvas.getBoundingClientRect();
    const msPerPx = (viewS * 1000) / rect.width;
    const ms = positionMs + (e.clientX - rect.left - rect.width / 2) * msPerPx;
    let section = -1;
    let best = MARKER_GRAB_PX * msPerPx;
    overlay.markers.forEach((m, i) => {
      const d = Math.abs(m.ms - ms);
      if (d <= best) {
        best = d;
        section = i;
      }
    });
    const from = overlay.markers[section]?.ms;
    if (from === undefined) {
      scrub.onPointerDown(e);
      return;
    }
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setDrag({
      pointerId: e.pointerId,
      section,
      startX: e.clientX,
      fromMs: from,
      toMs: from,
      msPerPx,
    });
  }

  function onPointerMove(e: PointerEvent<HTMLElement>) {
    const drag = dragRef.current;
    if (drag === null) return scrub.onPointerMove(e);
    if (e.pointerId !== drag.pointerId) return;
    setDrag({ ...drag, toMs: drag.fromMs + (e.clientX - drag.startX) * drag.msPerPx });
  }

  function end(e: PointerEvent<HTMLElement>, commit: boolean) {
    const drag = dragRef.current;
    if (drag === null) return scrub.onPointerUp(e);
    if (e.pointerId !== drag.pointerId) return;
    setDrag(null);
    if (commit && Math.round(drag.toMs) !== drag.fromMs) {
      overlay?.onMoveMarker(drag.section, drag.toMs, e.altKey);
    }
  }

  const marks =
    overlay === null
      ? null
      : {
          pivotMs: overlay.pivotMs,
          markers:
            drag === null
              ? overlay.markers
              : overlay.markers.map((m, i) => (i === drag.section ? { ...m, ms: drag.toMs } : m)),
        };

  return {
    handlers: {
      ...scrub,
      onPointerDown,
      onPointerMove,
      onPointerUp: (e) => end(e, true),
      onPointerCancel: (e) => end(e, false),
      onLostPointerCapture: (e) => end(e, true),
    },
    marks,
  };
}
