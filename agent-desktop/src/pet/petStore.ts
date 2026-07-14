import { create } from "zustand";
import type { PetStats, PetVisualState } from "./types";

interface PetStore {
  /** 当前动画状态 */
  state: PetVisualState;
  /** 数值 */
  stats: PetStats;
  /** 气泡文案（null 不显示） */
  bubble: string | null;
  setState: (s: PetVisualState, bubble?: string | null) => void;
  setStats: (s: PetStats) => void;
  setBubble: (b: string | null) => void;
}

export const usePetStore = create<PetStore>((set) => ({
  state: "idle",
  stats: { mood: 70, friendship: 50, energy: 80, daysTogether: 1 },
  bubble: null,
  setState: (state, bubble) =>
    set({ state, bubble: bubble === undefined ? null : bubble }),
  setStats: (stats) => set({ stats }),
  setBubble: (bubble) => set({ bubble }),
}));
