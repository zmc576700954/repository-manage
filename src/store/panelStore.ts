import { create } from 'zustand';

export type PanelType = 'validation' | 'settings' | 'filter';

interface PanelState {
  openPanels: PanelType[];
  open: (panel: PanelType) => void;
  close: (panel: PanelType) => void;
  toggle: (panel: PanelType) => void;
  width: number;
  setWidth: (w: number) => void;
}

export const usePanelStore = create<PanelState>((set) => ({
  openPanels: [],
  open: (panel) =>
    set((s) => ({
      openPanels: s.openPanels.includes(panel)
        ? s.openPanels
        : [...s.openPanels, panel],
    })),
  close: (panel) =>
    set((s) => ({
      openPanels: s.openPanels.filter((p) => p !== panel),
    })),
  toggle: (panel) =>
    set((s) => ({
      openPanels: s.openPanels.includes(panel)
        ? s.openPanels.filter((p) => p !== panel)
        : [...s.openPanels, panel],
    })),
  width: 360,
  setWidth: (w) => set({ width: Math.max(280, Math.min(800, w)) }),
}));
