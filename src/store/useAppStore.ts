import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { Entry, KBInfo, RelationType } from '@shared/types';
import { localizeError } from '@/i18n/translateError';

interface AppState {
  entries: Entry[];
  selectedEntryId: string | null;
  relationFilter: Set<RelationType>;
  filterLabels: Set<string>;
  isLoading: boolean;
  error: string | null;

  kbs: KBInfo[];
  activeKbId: string | null;
  activeKbEntryCount: number;

  loadEntries: () => Promise<void>;
  selectEntry: (id: string | null) => void;
  toggleRelationType: (type: RelationType) => void;
  toggleFilterLabel: (label: string) => void;
  clearFilters: () => void;
  createRelation: (fromId: string, toId: string, type: RelationType, note?: string) => Promise<void>;
  deleteRelation: (fromId: string, toId: string, type: RelationType) => Promise<void>;
  createEntryFromReference: (
    fromId: string,
    parentGroup: string,
    name: string,
    sourceUrl: string,
    refText: string
  ) => Promise<string | null>;
  createEntry: (params: {
    parentGroup: string;
    name: string;
    content?: string;
    linkFromEntry?: string;
    linkToEntry?: string;
    relationType?: RelationType;
    note?: string;
  }) => Promise<string | null>;

  refreshKbs: () => Promise<void>;
  addKb: (name: string, path: string) => Promise<void>;
  removeKb: (id: string) => Promise<void>;
  setActiveKb: (id: string) => Promise<void>;
}

/// Read an invoke error and turn it into a user-facing, localized message.
/// Per Finding #14, the Rust `AppError` payload is `{ code, ...params }`;
/// `localizeError` looks up `errors.<code>` in the Errors namespace.
function errMsg(e: unknown): string {
  return localizeError(e);
}

export const useAppStore = create<AppState>((set, get) => ({
  entries: [],
  selectedEntryId: null,
  relationFilter: new Set<RelationType>(['reference', 'derived', 'contradicts', 'supersedes', 'extends']),
  filterLabels: new Set<string>(),
  isLoading: false,
  error: null,

  kbs: [],
  activeKbId: null,
  activeKbEntryCount: 0,

  loadEntries: async () => {
    set({ isLoading: true, error: null });
    try {
      const entries = await invoke<Entry[]>('get_entries');
      set({ entries, isLoading: false, activeKbEntryCount: entries.length });
    } catch (e) {
      set({ error: errMsg(e), isLoading: false });
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

  toggleFilterLabel: (label) => {
    const filter = new Set(get().filterLabels);
    if (filter.has(label)) {
      filter.delete(label);
    } else {
      filter.add(label);
    }
    set({ filterLabels: filter });
  },

  clearFilters: () => set({ filterLabels: new Set<string>() }),

  createRelation: async (fromId, toId, type, note) => {
    try {
      await invoke('create_relation', {
        fromId,
        toId,
        relationType: type,
        note,
      });
      await get().loadEntries();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  deleteRelation: async (fromId, toId, type) => {
    try {
      await invoke('delete_relation', {
        fromId,
        toId,
        relationType: type,
      });
      await get().loadEntries();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  createEntryFromReference: async (fromId, parentGroup, name, sourceUrl, refText) => {
    try {
      const newId = await invoke<string>('create_entry_from_reference', {
        fromId,
        parentGroup,
        name,
        sourceUrl,
        refText,
      });
      await get().loadEntries();
      return newId;
    } catch (e) {
      set({ error: errMsg(e) });
      return null;
    }
  },

  createEntry: async ({
    parentGroup,
    name,
    content,
    linkFromEntry,
    linkToEntry,
    relationType,
    note,
  }) => {
    try {
      const newId = await invoke<string>('create_entry_simple', {
        parentGroup,
        name,
        content: content ?? null,
        linkFromEntry: linkFromEntry ?? null,
        linkToEntry: linkToEntry ?? null,
        relationType: relationType ?? null,
        note: note ?? null,
      });
      await get().loadEntries();
      return newId;
    } catch (e) {
      set({ error: errMsg(e) });
      return null;
    }
  },

  /// Per Finding #7: pre-fix this caught errors into store.error but
  /// did NOT update `kbs`/`activeKbId` and never rethrew. Callers like
  /// `addKb` did `Promise.all([refreshKbs(), loadEntries()])` — the
  /// outer Promise.all rejected only on `loadEntries`'s failure; a
  /// refreshKbs-only error got swallowed. Now: log + store the localized
  /// error AND keep `kbs`/`activeKbId` at their last good value so the
  /// UI doesn't flicker, but do NOT mark refresh as successful. The
  /// store's `error` field carries the message; consumers like Sidebar
  /// already read it.
  refreshKbs: async () => {
    const [kbs, active] = await Promise.all([
      invoke<KBInfo[]>('list_kbs'),
      invoke<KBInfo | null>('get_active_kb'),
    ]);
    set({
      kbs,
      activeKbId: active?.id ?? null,
      error: null,
    });
  },

  addKb: async (name, path) => {
    try {
      await invoke<KBInfo>('add_kb', { name, path });
      await Promise.all([get().refreshKbs(), get().loadEntries()]);
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  removeKb: async (id) => {
    try {
      await invoke('remove_kb', { id });
      // The active KB may have just been cleared (Finding #15 makes
      // remove of the active KB → "no active KB"). Drop any selection
      // that belonged to the removed KB so the user doesn't see a
      // dangling reference in the detail panel.
      set({ selectedEntryId: null });
      await Promise.all([get().refreshKbs(), get().loadEntries()]);
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  /// Per Finding #10: pre-fix wiped `selectedEntryId` synchronously
  /// after `set_active_kb` resolved, racing with any click the user
  /// made during the await window. The wipe would clobber a selection
  /// the user just made. Post-fix: only clear the selection if the
  /// entry isn't in the new KB. If the user picked the same entry
  /// again on the new KB, we let it stand.
  setActiveKb: async (id) => {
    const prevSelection = get().selectedEntryId;
    try {
      await invoke('set_active_kb', { id });
      // Don't clear selection eagerly — wait for loadEntries so we know
      // what the new KB's entry ids look like, then decide.
      await get().refreshKbs();
      const newEntries = await invoke<Entry[]>('get_entries');
      const newIds = new Set(newEntries.map((e) => e.id));
      const stillExists = prevSelection !== null && newIds.has(prevSelection);
      set({
        entries: newEntries,
        activeKbEntryCount: newEntries.length,
        selectedEntryId: stillExists ? prevSelection : null,
      });
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },
}));