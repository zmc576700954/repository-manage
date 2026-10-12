import { create } from 'zustand';

export interface ApplyResult {
  ruleId: string;
  entryId: string;
}

export interface FixTarget {
  entryId: string;
  ruleId: string;
}

interface FixStore {
  drawerEntryId: string | null;
  openDrawer: (id: string) => void;
  closeDrawer: () => void;

  recentFixes: ApplyResult[];
  pushFix: (r: ApplyResult) => void;

  autoFixQueue: FixTarget[];
  startAutoFix: (entryId: string, ruleIds: string[]) => void;
}

export const useFixStore = create<FixStore>((set) => ({
  drawerEntryId: null,
  openDrawer: (id) => set({ drawerEntryId: id }),
  closeDrawer: () => set({ drawerEntryId: null }),

  recentFixes: [],
  pushFix: (r) => set((s) => ({ recentFixes: [r, ...s.recentFixes].slice(0, 20) })),

  autoFixQueue: [],
  startAutoFix: (entryId, ruleIds) =>
    set({ autoFixQueue: ruleIds.map((r) => ({ entryId, ruleId: r })) }),
}));
