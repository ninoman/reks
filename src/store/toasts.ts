/**
 * One toast at a time (a message with an optional action such as Undo).
 * It dismisses itself after `TOAST_MS` unless the pointer or focus is on it
 * (the host pauses that). A new toast replaces the old one.
 */
import { create } from "zustand";

export interface Toast {
  id: number;
  message: string;
  action?: { label: string; run: () => void } | undefined;
}

export const TOAST_MS = 8000;

export interface ToastsState {
  toast: Toast | null;
  show(message: string, action?: Toast["action"]): void;
  dismiss(): void;
}

let nextId = 0;

export const useToasts = create<ToastsState>()((set) => ({
  toast: null,
  show(message, action) {
    nextId += 1;
    set({ toast: { id: nextId, message, action } });
  },
  dismiss() {
    set({ toast: null });
  },
}));
