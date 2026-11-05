/**
 * 编辑器自动保存草稿（localStorage，按 KB + 条目隔离）。
 *
 * 草稿只存在前端本地，不写进 KB 目录，因此：
 * - 不会触发文件 watcher 的 `kb-changed` 刷新回环；
 * - 也不会进入后续的版本历史管线（历史只跟“已落盘的保存”有关）。
 */

export interface EntryDraft {
  content: string;
  updatedAt: number;
}

export type DraftStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

const PREFIX = 'synapse:draft';

export function draftKey(kbId: string | null, entryId: string): string {
  return `${PREFIX}:${kbId ?? '_'}:${entryId}`;
}

function defaultStorage(): DraftStorage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

export function readDraft(
  kbId: string | null,
  entryId: string,
  storage: DraftStorage | null = defaultStorage()
): EntryDraft | null {
  if (!storage) return null;
  try {
    const raw = storage.getItem(draftKey(kbId, entryId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<EntryDraft> | null;
    if (
      !parsed ||
      typeof parsed.content !== 'string' ||
      typeof parsed.updatedAt !== 'number'
    ) {
      return null;
    }
    return { content: parsed.content, updatedAt: parsed.updatedAt };
  } catch {
    return null;
  }
}

/// 写入草稿；storage 满/被禁用时静默失败——草稿是尽力而为，不能影响编辑。
export function writeDraft(
  kbId: string | null,
  entryId: string,
  content: string,
  storage: DraftStorage | null = defaultStorage(),
  now: number = Date.now()
): void {
  if (!storage) return;
  try {
    storage.setItem(draftKey(kbId, entryId), JSON.stringify({ content, updatedAt: now }));
  } catch {
    // ignore quota / private-mode errors
  }
}

export function clearDraft(
  kbId: string | null,
  entryId: string,
  storage: DraftStorage | null = defaultStorage()
): void {
  if (!storage) return;
  try {
    storage.removeItem(draftKey(kbId, entryId));
  } catch {
    // ignore
  }
}

/// 只有草稿内容与磁盘正文不同才值得提示恢复；相同内容说明上次保存后
/// 没有未落盘的修改。
export function pendingDraft(
  kbId: string | null,
  entryId: string,
  savedContent: string,
  storage: DraftStorage | null = defaultStorage()
): EntryDraft | null {
  const draft = readDraft(kbId, entryId, storage);
  if (!draft || draft.content === savedContent) return null;
  return draft;
}
