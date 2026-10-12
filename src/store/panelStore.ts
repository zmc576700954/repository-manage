import { create } from 'zustand';
import { useAppStore } from './useAppStore';

export type PanelType = 'detail' | 'validation' | 'settings' | 'filter';

const MAX_PANELS = 3;

interface PanelState {
  openPanels: PanelType[];
  open: (panel: PanelType) => void;
  close: (panel: PanelType) => void;
  toggle: (panel: PanelType) => void;
  closeAll: () => void;
  width: number;
  setWidth: (w: number) => void;
}

function handleEviction(evicted: PanelType): void {
  if (evicted === 'detail') {
    useAppStore.getState().selectEntry(null);
  }
}

export const usePanelStore = create<PanelState>((set, get) => ({
  openPanels: [],
  width: 360,
  setWidth: (w) => set({ width: Math.max(280, Math.min(800, w)) }),

  open: (panel) => {
    const { openPanels } = get();
    if (openPanels.includes(panel)) return;
    if (openPanels.length >= MAX_PANELS) {
      const evicted = openPanels[0];
      handleEviction(evicted);
      set({ openPanels: [...openPanels.slice(1), panel] });
    } else {
      set({ openPanels: [...openPanels, panel] });
    }
  },

  close: (panel) =>
    set((s) => ({ openPanels: s.openPanels.filter((p) => p !== panel) })),

  toggle: (panel) => {
    const { openPanels, open, close } = get();
    if (openPanels.includes(panel)) close(panel);
    else open(panel);
  },

  closeAll: () => set({ openPanels: [] }),
}));
