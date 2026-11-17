import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import { localizeError } from '@/i18n/translateError';
import type { HistoryWarnings, TrimPreview, VersionMeta, VersionOrigin } from '@shared/types';

const ORIGIN_LABEL: Record<VersionOrigin, string> = {
  editor: 'versionOriginEditor',
  autosave: 'versionOriginAutosave',
  format: 'versionOriginFormat',
  restore: 'versionOriginRestore',
  external: 'versionOriginExternal',
  trim: 'versionOriginTrim',
  agent: 'versionOriginAgent',
};

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function formatTime(ms: number): string {
  return new Date(ms).toLocaleString();
}

interface VersionHistorySectionProps {
  entryId: string;
  /// 列表刷新时递增：保存、排版、回退、裁剪、外部变更都会触发，
  /// 用它静默重拉版本与警告。
  refreshKey: number;
  /// 编辑区有未保存修改时禁止回退，避免回退覆盖用户正在写的正文。
  editingDirty: boolean;
  /// 回退成功后把正文回填到详情面板（同时清掉草稿等本地状态）。
  onRestored: (content: string) => void;
}

interface Notice {
  kind: 'ok' | 'error';
  text: string;
}

export function VersionHistorySection({
  entryId,
  refreshKey,
  editingDirty,
  onRestored,
}: VersionHistorySectionProps) {
  const { t } = useTranslation('DetailPanel');
  const listEntryVersions = useAppStore((s) => s.listEntryVersions);
  const getEntryVersionContent = useAppStore((s) => s.getEntryVersionContent);
  const getHistoryWarnings = useAppStore((s) => s.getHistoryWarnings);
  const previewTrimHistory = useAppStore((s) => s.previewTrimHistory);
  const trimHistory = useAppStore((s) => s.trimHistory);
  const restoreEntryVersion = useAppStore((s) => s.restoreEntryVersion);

  const [versions, setVersions] = useState<VersionMeta[]>([]);
  const [warnings, setWarnings] = useState<HistoryWarnings | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);

  const [previewId, setPreviewId] = useState<string | null>(null);
  const [previewContent, setPreviewContent] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);

  const [restoreTarget, setRestoreTarget] = useState<VersionMeta | null>(null);
  const [trimTarget, setTrimTarget] = useState<{ version: VersionMeta; preview: TrimPreview } | null>(
    null
  );
  const [trimAck, setTrimAck] = useState(false);
  const [busy, setBusy] = useState(false);
  const loadedEntryRef = useRef<string | null>(null);

  // 切条目时重置全部局部状态；同一文档的刷新走静默更新，不清空已加载
  // 的列表，避免自动保存触发的刷新让版本区闪烁。
  useEffect(() => {
    let cancelled = false;
    const entryChanged = loadedEntryRef.current !== entryId;
    loadedEntryRef.current = entryId;
    if (entryChanged) {
      setVersions([]);
      setWarnings(null);
      setLoading(true);
      setError(null);
      setNotice(null);
      setPreviewId(null);
      setPreviewContent(null);
      setRestoreTarget(null);
      setTrimTarget(null);
      setTrimAck(false);
    }
    Promise.all([listEntryVersions(entryId), getHistoryWarnings(entryId)])
      .then(([list, warn]) => {
        if (cancelled) return;
        setVersions(list);
        setWarnings(warn);
        setError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(t('versionHistoryError', { msg: localizeError(e) }));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [entryId, refreshKey, listEntryVersions, getHistoryWarnings, t]);

  // 最近版本在前，便于直接看到最新记录；裁剪是否可用按时间升序原索引判断。
  const ordered = useMemo(() => versions.slice().reverse(), [versions]);
  const indexOf = useMemo(() => {
    const map = new Map<string, number>();
    versions.forEach((v, i) => map.set(v.versionId, i));
    return map;
  }, [versions]);

  const togglePreview = async (version: VersionMeta) => {
    if (previewId === version.versionId) {
      setPreviewId(null);
      setPreviewContent(null);
      return;
    }
    setPreviewId(version.versionId);
    setPreviewContent(null);
    setPreviewLoading(true);
    setNotice(null);
    try {
      const text = await getEntryVersionContent(entryId, version.versionId);
      setPreviewContent(text);
      if (text == null) {
        setNotice({ kind: 'error', text: t('versionNotStored') });
      }
    } catch (e) {
      setNotice({
        kind: 'error',
        text: t('versionPreviewError', { msg: localizeError(e) }),
      });
    } finally {
      setPreviewLoading(false);
    }
  };

  const handleRestore = async () => {
    if (!restoreTarget) return;
    setBusy(true);
    setNotice(null);
    try {
      const result = await restoreEntryVersion(entryId, restoreTarget.versionId);
      onRestored(result.content);
      setRestoreTarget(null);
      setNotice({ kind: 'ok', text: t('versionRestored') });
    } catch (e) {
      setNotice({
        kind: 'error',
        text: t('versionRestoreError', { msg: localizeError(e) }),
      });
    } finally {
      setBusy(false);
    }
  };

  const handleTrimStart = async (version: VersionMeta) => {
    setNotice(null);
    setTrimAck(false);
    try {
      const preview = await previewTrimHistory(entryId, version.versionId);
      if (preview.deleteCount === 0) {
        setNotice({ kind: 'error', text: t('versionTrimDisabled') });
        return;
      }
      setTrimTarget({ version, preview });
    } catch (e) {
      setNotice({
        kind: 'error',
        text: t('versionTrimError', { msg: localizeError(e) }),
      });
    }
  };

  const handleTrimConfirm = async () => {
    if (!trimTarget) return;
    setBusy(true);
    setNotice(null);
    try {
      const outcome = await trimHistory(entryId, trimTarget.version.versionId);
      setTrimTarget(null);
      setTrimAck(false);
      setNotice({
        kind: 'ok',
        text: t('versionTrimDone', {
          count: outcome.deletedCount,
          size: formatBytes(outcome.bytesFreed),
        }),
      });
    } catch (e) {
      setNotice({
        kind: 'error',
        text: t('versionTrimError', { msg: localizeError(e) }),
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="border-b border-gray-200 p-4">
      <div className="mb-2 flex items-center justify-between gap-2">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-gray-500">
          {t('versionHistoryHeader')}
        </h3>
        <span className="text-xs text-gray-400">
          {t('versionHistoryCount', { count: versions.length })}
        </span>
      </div>

      {warnings?.perEntryExceeded && (
        <p className="mb-1 rounded-md border border-amber-200 bg-amber-50 px-2 py-1 text-xs text-amber-800">
          {t('versionWarningPerEntry', {
            count: warnings.versionCount,
            limit: warnings.perEntryLimit,
          })}
        </p>
      )}
      {warnings?.totalExceeded && (
        <p className="mb-1 rounded-md border border-amber-200 bg-amber-50 px-2 py-1 text-xs text-amber-800">
          {t('versionWarningTotal', {
            size: formatBytes(warnings.totalBytes),
            limit: formatBytes(warnings.totalLimitBytes),
          })}
        </p>
      )}

      {notice && (
        <p
          className={`mb-2 rounded-md px-2 py-1 text-xs ${
            notice.kind === 'ok'
              ? 'bg-green-50 text-green-700'
              : 'bg-red-50 text-red-700'
          }`}
        >
          {notice.text}
        </p>
      )}

      {error ? (
        <p className="text-xs text-red-600">{error}</p>
      ) : loading && versions.length === 0 ? (
        <p className="text-xs text-gray-400">{t('versionHistoryLoading')}</p>
      ) : versions.length === 0 ? (
        <p className="text-xs text-gray-400">{t('versionHistoryEmpty')}</p>
      ) : (
        <ul className="list-enter space-y-1.5">
          {ordered.map((version) => {
            const originalIndex = indexOf.get(version.versionId) ?? 0;
            const canUseContent = version.contentStored;
            const previewOpen = previewId === version.versionId;
            return (
              <li
                key={version.versionId}
                className="rounded-md border border-gray-200 px-2 py-1.5 text-xs"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                      <span className="font-medium text-gray-800">
                        {formatTime(version.createdAtMs)}
                      </span>
                      <span className="rounded bg-gray-100 px-1.5 py-0.5 text-gray-600">
                        {t(ORIGIN_LABEL[version.origin])}
                      </span>
                      {version.restoredFrom && (
                        <span className="text-gray-500">
                          {t('versionRestoredFrom', { version: version.restoredFrom })}
                        </span>
                      )}
                    </div>
                    <div className="mt-0.5 text-gray-500">
                      {t('versionAuthor', { author: version.author })} ·{' '}
                      {t('versionSummary', {
                        added: version.summary.addedLines,
                        removed: version.summary.removedLines,
                      })}{' '}
                      · {t('versionSize', { size: formatBytes(version.contentSize) })}
                    </div>
                    {version.origin === 'trim' && version.note && (
                      <p className="mt-0.5 break-all text-gray-500">
                        {t('versionTrimAuditNote')}: {version.note}
                      </p>
                    )}
                    {!canUseContent && version.origin !== 'trim' && (
                      <p className="mt-0.5 text-amber-700">{t('versionNotStored')}</p>
                    )}
                  </div>
                  <div className="flex shrink-0 flex-wrap justify-end gap-1">
                    <button
                      onClick={() => void togglePreview(version)}
                      disabled={!canUseContent || previewLoading}
                      className="rounded border border-gray-300 bg-white px-1.5 py-0.5 text-gray-700 hover:bg-gray-100 disabled:opacity-50"
                    >
                      {previewOpen ? t('versionHidePreview') : t('versionPreview')}
                    </button>
                    <button
                      onClick={() => setRestoreTarget(version)}
                      disabled={!canUseContent || editingDirty}
                      title={editingDirty ? t('versionRestoreDisabledDirty') : undefined}
                      className="rounded border border-gray-300 bg-white px-1.5 py-0.5 text-gray-700 hover:bg-gray-100 disabled:opacity-50"
                    >
                      {t('versionRestore')}
                    </button>
                    <button
                      onClick={() => void handleTrimStart(version)}
                      disabled={originalIndex === 0 || editingDirty}
                      title={
                        editingDirty
                          ? t('versionRestoreDisabledDirty')
                          : originalIndex === 0
                            ? t('versionTrimDisabled')
                            : undefined
                      }
                      className="rounded border border-gray-300 bg-white px-1.5 py-0.5 text-gray-700 hover:bg-gray-100 disabled:opacity-50"
                    >
                      {t('versionTrim')}
                    </button>
                  </div>
                </div>
                {previewOpen && (
                  <div className="mt-1.5 rounded bg-gray-50 p-2">
                    {previewLoading ? (
                      <p className="text-gray-400">{t('versionPreviewLoading')}</p>
                    ) : (
                      <pre className="max-h-60 overflow-y-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-gray-800">
                        {previewContent ?? ''}
                      </pre>
                    )}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}

      {restoreTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
          <div className="w-full max-w-md rounded-lg bg-white p-5 shadow-xl">
            <h2 className="text-base font-semibold text-gray-900">
              {t('versionRestoreTitle')}
            </h2>
            <p className="mt-1 text-sm text-gray-600">{t('versionRestoreHint')}</p>
            <p className="mt-2 text-xs text-gray-500">
              {formatTime(restoreTarget.createdAtMs)} · {t(ORIGIN_LABEL[restoreTarget.origin])}
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <button
                onClick={() => setRestoreTarget(null)}
                disabled={busy}
                className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-100 disabled:opacity-50"
              >
                {t('versionCancel')}
              </button>
              <button
                onClick={() => void handleRestore()}
                disabled={busy}
                className="rounded-md bg-blue-500 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
              >
                {busy ? t('versionBusy') : t('versionRestoreConfirm')}
              </button>
            </div>
          </div>
        </div>
      )}

      {trimTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
          <div className="w-full max-w-md rounded-lg bg-white p-5 shadow-xl">
            <h2 className="text-base font-semibold text-gray-900">
              {t('versionTrimTitle')}
            </h2>
            <p className="mt-1 text-sm text-gray-600">{t('versionTrimHint')}</p>
            <div className="mt-3 rounded-md bg-gray-50 p-2 text-xs text-gray-600">
              <p>
                {t('versionTrimKept', {
                  version: formatTime(trimTarget.version.createdAtMs),
                  count: trimTarget.preview.keptCount,
                })}
              </p>
              <p className="mt-1">
                {t('versionTrimConfirm', { count: trimTarget.preview.deleteCount })} ·{' '}
                {t('versionTrimFreed', {
                  size: formatBytes(trimTarget.preview.bytesToFree),
                })}
              </p>
            </div>
            <p className="mt-2 text-xs font-medium text-red-700">{t('versionTrimWarn')}</p>
            <label className="mt-3 flex items-start gap-2 text-xs text-gray-700">
              <input
                type="checkbox"
                checked={trimAck}
                onChange={(e) => setTrimAck(e.target.checked)}
                className="mt-0.5"
              />
              <span>{t('versionTrimAcknowledge')}</span>
            </label>
            <div className="mt-4 flex justify-end gap-2">
              <button
                onClick={() => {
                  setTrimTarget(null);
                  setTrimAck(false);
                }}
                disabled={busy}
                className="rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-100 disabled:opacity-50"
              >
                {t('versionCancel')}
              </button>
              <button
                onClick={() => void handleTrimConfirm()}
                disabled={busy || !trimAck}
                className="rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
              >
                {busy
                  ? t('versionBusy')
                  : t('versionTrimConfirm', { count: trimTarget.preview.deleteCount })}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
