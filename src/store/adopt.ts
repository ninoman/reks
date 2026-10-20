/**
 * Adopt a foreign stick (phase 2.5 task 12, D27): which stick the adopt
 * dialog is open for. The dialog itself runs the dry run and the apply.
 */
import { create } from "zustand";

import type { DeviceInfo } from "../lib/ipc";

export interface AdoptTarget {
  /** The stick's profile; its name is the folder under `From stick`. */
  deviceId: number;
  deviceName: string;
  info: DeviceInfo;
}

export interface AdoptState {
  target: AdoptTarget | null;
  open(target: AdoptTarget): void;
  close(): void;
}

export const useAdopt = create<AdoptState>()((set) => ({
  target: null,
  open(target) {
    set({ target });
  },
  close() {
    set({ target: null });
  },
}));
