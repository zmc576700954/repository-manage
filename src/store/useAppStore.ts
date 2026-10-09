import { create } from 'zustand';
import type { Entry, Relation, RelationType } from '@shared/types';
import { api } from '@/lib/api';

interface AppState {
  entries: Entry[];
  selectedEntryId: string | null;
  relationFilter: Set<RelationType>;
  isLoading: boolean;
  error: string | null;

  loadEntries: () => Promise<void>;
  selectEntry: (id: string | null) => void;
  toggleRelationType: (type: RelationType) => void;
  createRelation: (fromId: string, toId: string, type: RelationType, note?: string) => Promise<void>;
  deleteRelation: (fromId: string, toId: string, type: RelationType) => Promise<void>;
}

export const useAppStore = create<AppState>((set, get) => ({
  entries: [],
  selectedEntryId: null,
  relationFilter: new Set<RelationType>(['reference', 'derived', 'contradicts', 'supersedes', 'extends']),
  isLoading: false,
  error: null,

  loadEntries: async () => {
    set({ isLoading: true, error: null });
    try {
      const entries = await api.listEntries();
      set({ entries, isLoading: false });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  selectEntry: (id) => set({ selectedEntryId: id }),

  toggleRelationType: (type) => {
    const filter = new Set(get().relationFilter);
    if (filter.has(type)) {
      filter.delete(type);
    } else {
      filter.add(type);
    }
    set({ relationFilter: filter });
  },

  createRelation: async (fromId, toId, type, note) => {
    try {
      await api.createRelation({ fromId, toId, type, note });
      await get().loadEntries();
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  deleteRelation: async (fromId, toId, type) => {
    try {
      await api.deleteRelation(fromId, toId, type);
      await get().loadEntries();
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },
}));
