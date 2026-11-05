import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { Entry, FormatPreview, HistoryWarnings, KBInfo, MetadataChanges, PrecheckRenameResult, RelationType, RenameResult, SaveEntryResult, StoredEntry, TrimOutcome, TrimPreview, VersionMeta } from '@shared/types';
import { localizeError } from '@/i18n/translateError';
import { usePanelStore } from './panelStore';
import { setSyncedFingerprint, type KbChangedPayload } from '@/events/kbRefresh';

interface EntriesSnapshot {
  entries: Entry[];
  fingerprint: string;
}

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
  /// 每次 entries 被刷新时递增。详情面板据此判断“列表刷新了”，
  /// 在不改变选中项的前提下重新核对正文内容。
  entriesRevision: number;

  /// Entry id that should have its rename dialog opened. DetailPanel
  /// watches this and shows RenameEntryDialog when it changes (then
  /// clears it back to null via `clearRenamingEntryId`).
  renamingEntryId: string | null;
  /// 节点是否正在被拖拽（MapView 维护）。watcher 事件在此期间延后刷新，
  /// 避免刷新 nodes 打断拖拽或让节点回弹。
  isDraggingNode: boolean;
  /// 是否有模态交互正在进行（重命名对话框等，由详情面板维护）。
  /// watcher 事件在此期间延后刷新，避免抢焦点或重置面板。
  interactionBusy: boolean;

  loadEntries: () => Promise<void>;
  /// 由 `kb-changed` 事件触发的静默刷新：不置 isLoading（不遮整屏）、
  /// 不改 selectedEntryId（不抢焦点/不重置面板），失败也不弹错误。
  refreshEntriesQuiet: () => Promise<void>;
  /// watcher 事件的统一入口：先按 renamed/removed 增量修正选中项，
  /// 再静默刷新列表。
  applyKbChanged: (payload: KbChangedPayload) => Promise<void>;
  setDraggingNode: (dragging: boolean) => void;
  setInteractionBusy: (busy: boolean) => void;
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
  deleteEntry: (id: string) => Promise<void>;

  refreshKbs: () => Promise<void>;
  addKb: (name: string, path: string) => Promise<void>;
  removeKb: (id: string) => Promise<void>;
  setActiveKb: (id: string) => Promise<void>;

  /// Cycle 3 polish B: request that the rename dialog open for `id`.
  /// DetailPanel observes this and renders RenameEntryDialog.
  setRenamingEntryId: (id: string | null) => void;

  // ---------- Inline metadata editing + rename (Cycle 2) ----------
  updateMetadata: (id: string, changes: MetadataChanges) => Promise<StoredEntry>;
  precheckRename: (id: string, newName: string, newGroup: string) => Promise<PrecheckRenameResult>;
  renameEntry: (id: string, newName: string, newGroup: string, cascade: boolean) => Promise<RenameResult>;

  // ---------- 正文编辑 / 无损排版 / 同 KB 迁移 ----------
  /// 显式保存正文。成功后静默重取快照，让前端指纹与后端一致（避免把
  /// 自己这次写入当成外部变更）。错误向上抛，由编辑器就地展示。
  saveEntryContent: (id: string, content: string) => Promise<SaveEntryResult>;
  /// 自动保存：写盘，版本按 5 分钟节流。
  autosaveEntryContent: (id: string, content: string) => Promise<SaveEntryResult>;
  /// 只读预览排版差异，不写盘。
  previewFormatEntry: (id: string) => Promise<FormatPreview>;
  /// 用户确认后写回排版结果。
  applyFormatEntry: (id: string) => Promise<SaveEntryResult>;
  /// 同一 KB 内移动文档到目标目录（复用 rename 的迁移管线）。
  moveEntry: (id: string, targetDir: string) => Promise<RenameResult>;

  // ---------- 版本历史 ----------
  listEntryVersions: (id: string) => Promise<VersionMeta[]>;
  getEntryVersionContent: (id: string, versionId: string) => Promise<string | null>;
  getHistoryWarnings: (id: string) => Promise<HistoryWarnings>;
  previewTrimHistory: (id: string, keepVersionId: string) => Promise<TrimPreview>;
  trimHistory: (id: string, keepVersionId: string) => Promise<TrimOutcome>;
  restoreEntryVersion: (id: string, versionId: string) => Promise<SaveEntryResult>;
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
  entriesRevision: 0,
  renamingEntryId: null,
  isDraggingNode: false,
  interactionBusy: false,

  loadEntries: async () => {
    set({ isLoading: true, error: null });
    try {
      // 条目与指纹必须来自同一次读取：分两次取会让中间的重建把两者
      // 拆开，导致回声判定误吞外部变更。
      const snapshot = await invoke<EntriesSnapshot>('get_entries_snapshot');
      set((s) => ({
        entries: snapshot.entries,
        isLoading: false,
        activeKbEntryCount: snapshot.entries.length,
        entriesRevision: s.entriesRevision + 1,
      }));
      setSyncedFingerprint(snapshot.fingerprint);
    } catch (e) {
      set({ error: errMsg(e), isLoading: false });
    }
  },

  refreshEntriesQuiet: async () => {
    try {
      const snapshot = await invoke<EntriesSnapshot>('get_entries_snapshot');
      set((s) => ({
        entries: snapshot.entries,
        activeKbEntryCount: snapshot.entries.length,
        entriesRevision: s.entriesRevision + 1,
      }));
      setSyncedFingerprint(snapshot.fingerprint);
    } catch {
      // 外部变更触发的静默刷新失败时保留旧数据，等待下一次事件；
      // 不覆盖 store.error，避免自动刷新把用户正在看的错误顶掉。
    }
  },

  applyKbChanged: async (payload) => {
    const selected = get().selectedEntryId;
    if (selected) {
      const renamed = payload.renamed?.find((r) => r.fromId === selected);
      if (renamed) {
        // 选中项被重命名：直接跟到新 id，面板不闪、不丢失。
        set({ selectedEntryId: renamed.toId });
      } else if (payload.removed.includes(selected)) {
        // 选中项被外部删除：清空选择并关闭详情面板。
        set({ selectedEntryId: null });
        usePanelStore.getState().close('detail');
      }
    }
    await get().refreshEntriesQuiet();
  },

  setDraggingNode: (dragging) => set({ isDraggingNode: dragging }),
  setInteractionBusy: (busy) => set({ interactionBusy: busy }),

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

  /// Cycle 3 polish B: delete an entry from the disk + DB. If the deleted
  /// entry is currently selected, clear the selection + close the panel.
  deleteEntry: async (id) => {
    try {
      await invoke('delete_entry', { id });
      const wasSelected = get().selectedEntryId === id;
      if (wasSelected) {
        set({ selectedEntryId: null });
        usePanelStore.getState().close('detail');
      }
      await get().loadEntries();
    } catch (e) {
      set({ error: errMsg(e) });
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
      set({ selectedEntryId: null });
      usePanelStore.getState().close('detail');  // 同步关闭 detail 面板
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
      const snapshot = await invoke<EntriesSnapshot>('get_entries_snapshot');
      const newEntries = snapshot.entries;
      const newIds = new Set(newEntries.map((e) => e.id));
      const stillExists = prevSelection !== null && newIds.has(prevSelection);
      set({
        entries: newEntries,
        activeKbEntryCount: newEntries.length,
        entriesRevision: get().entriesRevision + 1,
        selectedEntryId: stillExists ? prevSelection : null,
      });
      setSyncedFingerprint(snapshot.fingerprint);
      if (!stillExists) {
        usePanelStore.getState().close('detail');
      }
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  // ---------- Inline metadata editing + rename (Cycle 2) ----------

  /// Optimistic update: mutate the local entries array so the UI
  /// reflects the new value immediately. On failure we revert.
  updateMetadata: async (id, changes) => {
    const prev = get().entries.find((e) => e.id === id);
    if (!prev) throw new Error(`entry not found: ${id}`);
    const optimistic: Entry = {
      ...prev,
      title: changes.title ?? prev.title,
      tags: changes.tags ?? prev.tags,
      labels: changes.labels ?? prev.labels,
      status: changes.status !== undefined && changes.status !== null ? changes.status : prev.status,
    };
    set((s) => ({ entries: s.entries.map((e) => (e.id === id ? optimistic : e)) }));

    try {
      const updated = await invoke<StoredEntry>('update_entry_metadata', { id, changes });
      // Reconcile with backend (DB might compute derived fields).
      set((s) => ({
        entries: s.entries.map((e) =>
          e.id === id
            ? {
                ...e,
                id: updated.id,
                title: updated.title,
                tags: updated.tags,
                labels: updated.labels,
                status: updated.status ?? undefined,
              }
            : e
        ),
      }));
      // 静默重取一次列表：metadata 写入可能落在外部的同秒变更之前/之后，
      // 用一次原子快照确保 entries 与自己的指纹一致，避免后续把真实外部
      // 变更当成回声丢掉。refreshEntriesQuiet 内部会同步指纹。
      await get().refreshEntriesQuiet();
      return updated;
    } catch (e) {
      // Revert
      set((s) => ({ entries: s.entries.map((ent) => (ent.id === id ? prev : ent)) }));
      set({ error: errMsg(e) });
      throw e;
    }
  },

  precheckRename: async (id, newName, newGroup) => {
    return invoke<PrecheckRenameResult>('precheck_rename', { id, newName, newGroup });
  },

  renameEntry: async (id, newName, newGroup, cascade) => {
    const result = await invoke<RenameResult>('execute_rename', { id, newName, newGroup, cascade });

    // Reload all entries (the renamed file's id is now result.newId,
    // and other entries' content may have changed if cascade=true).
    await get().loadEntries();

    // If the renamed entry was selected, update selection to new id.
    if (get().selectedEntryId === id) {
      set({ selectedEntryId: result.newId });
    }
    return result;
  },

  // ---------- 正文编辑 / 无损排版 / 同 KB 迁移 ----------

  saveEntryContent: async (id, content) => {
    const result = await invoke<SaveEntryResult>('save_entry_content', { id, content });
    await get().refreshEntriesQuiet();
    return result;
  },

  autosaveEntryContent: async (id, content) => {
    const result = await invoke<SaveEntryResult>('autosave_entry_content', { id, content });
    await get().refreshEntriesQuiet();
    return result;
  },

  previewFormatEntry: async (id) => {
    return invoke<FormatPreview>('preview_format_entry', { id });
  },

  applyFormatEntry: async (id) => {
    const result = await invoke<SaveEntryResult>('apply_format_entry', { id });
    await get().refreshEntriesQuiet();
    return result;
  },

  moveEntry: async (id, targetDir) => {
    const result = await invoke<RenameResult>('move_entry', { id, targetDir });
    await get().loadEntries();
    if (get().selectedEntryId === id) {
      set({ selectedEntryId: result.newId });
    }
    return result;
  },

  listEntryVersions: (id) => invoke<VersionMeta[]>('list_entry_versions', { id }),

  getEntryVersionContent: (id, versionId) =>
    invoke<string | null>('get_entry_version_content', { id, versionId }),

  getHistoryWarnings: (id) => invoke<HistoryWarnings>('get_history_warnings', { id }),

  previewTrimHistory: (id, keepVersionId) =>
    invoke<TrimPreview>('preview_trim_entry_history', { id, keepVersionId }),

  trimHistory: async (id, keepVersionId) => {
    const result = await invoke<TrimOutcome>('trim_entry_history', { id, keepVersionId });
    await get().loadEntries();
    return result;
  },

  restoreEntryVersion: async (id, versionId) => {
    const result = await invoke<SaveEntryResult>('restore_entry_version', { id, versionId });
    await get().refreshEntriesQuiet();
    return result;
  },

  /// Cycle 3 polish B: signal that the rename dialog should open for `id`.
  /// DetailPanel observes this value and renders RenameEntryDialog when
  /// it changes. The component clears the value back to null on close.
  setRenamingEntryId: (id) => set({ renamingEntryId: id }),
}));