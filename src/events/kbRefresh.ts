/**
 * watcher 事件（`kb-changed`）的前端协调器。
 *
 * 后端在每次 watcher 重建完成后推送一次事件；一次批量写入（例如 rsync、
 * 编辑器保存多个文件）可能触发多次重建，因此这里做防抖 + 合并：quiet 窗口
 * 内只保留最后一次负载，并把 added/removed/renamed 求并集，最终只刷新一次。
 *
 * 自身写操作回声的抑制不再依赖固定时间窗，而是“来源标记 + 内容指纹”：
 * 1. 后端内容指纹去重：重建后索引与上次推送完全相同则不 emit。
 * 2. `origin: "local-write"` 只是提示“这批重建大概率由本进程写命令触发”。
 *    前端只有在事件指纹等于自己刚同步到的指纹（`syncedFingerprint`）时才
 *    丢弃，说明这份数据自己已经拿到，刷新是纯抖动。
 * 3. 若同一批重建里还合并了外部变更，指纹会与本地已同步的不同，事件照常
 *    应用——外部真实变更不会被误吞，也不再受 1.5s 时间窗/重建耗时影响。
 * 4. 忙碌保护：重命名对话框打开、节点拖拽中不刷新，事件保留到交互结束。
 */
export type RebuildOrigin = 'local-write' | 'external';

/** 命令侧产生的重命名增量：旧 id → 新 id。 */
export interface KbRenamedEntry {
  fromId: string;
  toId: string;
}

export interface KbChangedPayload {
  kbId: string;
  entryCount: number;
  added: string[];
  removed: string[];
  renamed: KbRenamedEntry[];
  origin: RebuildOrigin;
  fingerprint: string;
  revision: number;
}

/**
 * 前端本地已经同步到的内容指纹（通常来自写命令之后的 `get_entries` 刷新）。
 * 仅用于判定 `local-write` 事件是否为“自己数据的回声”。
 */
let syncedFingerprint: string | null = null;

export function setSyncedFingerprint(fingerprint: string | null): void {
  syncedFingerprint = fingerprint;
}

export function getSyncedFingerprint(): string | null {
  return syncedFingerprint;
}

export interface CoordinatorOptions {
  /** 当前活动 KB id；事件属于别的 KB 时忽略。 */
  getActiveKbId: () => string | null;
  /** 重命名/拖拽等交互进行中返回 true，事件延迟处理。 */
  isBusy: () => boolean;
  /** 真正执行刷新（通常是一次静默的 entries 重新拉取）。 */
  apply: (payload: KbChangedPayload) => void;
  /** 读取前端已同步的内容指纹；默认用模块级变量（由 store 写入）。 */
  getSyncedFingerprint?: () => string | null;
  schedule?: (fn: () => void, ms: number) => unknown;
  cancel?: (handle: unknown) => void;
  /** 防抖窗口，默认 300ms。 */
  debounceMs?: number;
}

export interface KbRefreshCoordinator {
  handle(payload: KbChangedPayload): void;
  dispose(): void;
  /** 测试/调试用：是否还有未应用的事件。 */
  hasPending(): boolean;
}

function mergeIds(a: string[], b: string[]): string[] {
  const merged = [...a];
  for (const id of b) {
    if (!merged.includes(id)) merged.push(id);
  }
  return merged;
}

function mergeRenamed(a: KbRenamedEntry[], b: KbRenamedEntry[]): KbRenamedEntry[] {
  const merged = [...a];
  for (const entry of b) {
    if (!merged.some((r) => r.fromId === entry.fromId && r.toId === entry.toId)) {
      merged.push(entry);
    }
  }
  return merged;
}

export function createKbRefreshCoordinator(
  options: CoordinatorOptions,
): KbRefreshCoordinator {
  const debounceMs = options.debounceMs ?? 300;
  const schedule =
    options.schedule ??
    ((fn: () => void, ms: number) => setTimeout(fn, ms));
  const cancel =
    options.cancel ??
    ((handle: unknown) => clearTimeout(handle as ReturnType<typeof setTimeout>));
  const getSynced = options.getSyncedFingerprint ?? getSyncedFingerprint;

  let timer: unknown = null;
  let pending: KbChangedPayload | null = null;
  let pendingKbId: string | null = null;
  let lastFingerprint: string | null = null;

  function clearTimer(): void {
    if (timer !== null) {
      cancel(timer);
      timer = null;
    }
  }

  function flush(): void {
    timer = null;
    if (!pending || !pendingKbId) return;
    // 交互中：保留事件并稍后重试，不抢焦点、不重置面板。
    if (options.isBusy()) {
      timer = schedule(flush, debounceMs);
      return;
    }
    // KB 已切换：丢弃旧 KB 的待办事件，避免把上一个 KB 的数据刷进新 KB。
    if (options.getActiveKbId() !== pendingKbId) {
      pending = null;
      pendingKbId = null;
      return;
    }
    const payload = pending;
    const kbId = pendingKbId;
    pending = null;
    pendingKbId = null;
    if (kbId) lastFingerprint = payload.fingerprint;
    options.apply(payload);
  }

  return {
    handle(payload: KbChangedPayload): void {
      if (options.getActiveKbId() !== payload.kbId) return;
      if (payload.fingerprint === lastFingerprint) return;
      if (
        payload.origin === 'local-write' &&
        payload.fingerprint === getSynced()
      ) {
        // 自身写操作的回声：这份内容前端已经通过命令后的刷新拿到了。
        // 记下指纹，避免同一内容再次到达时重复刷新。
        lastFingerprint = payload.fingerprint;
        return;
      }
      if (pending && pendingKbId === payload.kbId) {
        pending = {
          ...payload,
          added: mergeIds(pending.added, payload.added),
          removed: mergeIds(pending.removed, payload.removed),
          renamed: mergeRenamed(pending.renamed ?? [], payload.renamed ?? []),
          // 只要批次里混入了外部变更，就以 external 呈现（仅用于诊断）。
          origin:
            pending.origin === 'external' || payload.origin === 'external'
              ? 'external'
              : 'local-write',
        };
      } else {
        pending = payload;
        pendingKbId = payload.kbId;
      }
      clearTimer();
      timer = schedule(flush, debounceMs);
    },
    dispose(): void {
      clearTimer();
      pending = null;
      pendingKbId = null;
    },
    hasPending(): boolean {
      return pending !== null;
    },
  };
}
