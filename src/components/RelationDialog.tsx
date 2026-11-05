import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import type { RelationType } from '@shared/types';

const RELATION_TYPES: RelationType[] = [
  'reference',
  'derived',
  'contradicts',
  'supersedes',
  'extends',
];

interface RelationDialogProps {
  fromId: string;
  onClose: () => void;
  /** 在写入 relation 前调用；返回 false 时中止（例如当前编辑未保存成功）。 */
  beforeCreate?: () => Promise<boolean>;
}

/**
 * 快捷关联对话框：搜索目标条目、选择关系类型，确认后由后端
 * `create_relation` 在正文末尾追加 `@relation: 类型 [[目标id]]` 并写入
 * 对应 relation。已存在同类型关系的目标会标记并禁止重复创建。
 */
export function RelationDialog({ fromId, onClose, beforeCreate }: RelationDialogProps) {
  const { t } = useTranslation('RelationDialog');
  const entries = useAppStore((s) => s.entries);
  const createRelation = useAppStore((s) => s.createRelation);

  const [query, setQuery] = useState('');
  const [toId, setToId] = useState('');
  const [type, setType] = useState<RelationType>('reference');
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fromEntry = useMemo(() => entries.find((e) => e.id === fromId), [entries, fromId]);

  const hasSameType = (targetId: string, relationType: RelationType): boolean =>
    (fromEntry?.relations ?? []).some(
      (rel) => rel.fromId === fromId && rel.toId === targetId && rel.relationType === relationType
    );

  const candidates = useMemo(() => {
    const q = query.trim().toLowerCase();
    return entries
      .filter((e) => e.id !== fromId)
      .filter((e) => {
        if (!q) return true;
        return (
          e.title.toLowerCase().includes(q) ||
          e.id.toLowerCase().includes(q) ||
          e.tags.some((tag) => tag.toLowerCase().includes(q))
        );
      })
      .slice(0, 50);
  }, [entries, fromId, query]);

  const selectedBusy = toId ? hasSameType(toId, type) : false;

  const handleTypeChange = (next: RelationType) => {
    setType(next);
    if (toId && hasSameType(toId, next)) setToId('');
  };

  const handleCreate = async () => {
    if (!toId || busy || selectedBusy) return;
    setBusy(true);
    setError(null);
    try {
      if (beforeCreate && !(await beforeCreate())) {
        setError(t('pendingSaveFailed'));
        return;
      }
      const ok = await createRelation(fromId, toId, type, note.trim() || undefined);
      if (!ok) {
        setError(t('createFailed'));
        return;
      }
      onClose();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">{t('title')}</h2>
        <p className="mt-1 text-sm text-gray-500">{t('fromLabel', { id: fromId })}</p>

        <div className="mt-4 space-y-3">
          <div>
            <label className="block text-sm font-medium text-gray-700">{t('searchLabel')}</label>
            <input
              type="search"
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t('searchPlaceholder')}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none"
            />
            <div className="mt-1 max-h-44 overflow-y-auto rounded-md border border-gray-200">
              {candidates.length === 0 ? (
                <p className="p-2 text-xs text-gray-500">{t('noMatches')}</p>
              ) : (
                <ul className="divide-y divide-gray-100">
                  {candidates.map((candidate) => {
                    const exists = hasSameType(candidate.id, type);
                    const active = candidate.id === toId;
                    return (
                      <li key={candidate.id}>
                        <button
                          type="button"
                          disabled={exists}
                          onClick={() => setToId(candidate.id)}
                          className={`flex w-full items-start justify-between gap-2 px-2 py-1.5 text-left ${
                            active ? 'bg-blue-50' : 'hover:bg-gray-50'
                          } disabled:cursor-not-allowed disabled:opacity-60`}
                        >
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-sm text-gray-800">
                              {candidate.title}
                            </span>
                            <span className="block truncate font-mono text-xs text-gray-400">
                              {candidate.id}
                            </span>
                          </span>
                          {exists && (
                            <span className="shrink-0 rounded bg-gray-100 px-1.5 py-0.5 text-[10px] text-gray-500">
                              {t('alreadyRelated')}
                            </span>
                          )}
                        </button>
                      </li>
                    );
                  })}
                </ul>
              )}
            </div>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">{t('typeLabel')}</label>
            <select
              value={type}
              onChange={(e) => handleTypeChange(e.target.value as RelationType)}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            >
              {RELATION_TYPES.map((relationType) => (
                <option key={relationType} value={relationType}>
                  {t(`types.${relationType}`)}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">{t('noteLabel')}</label>
            <input
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            />
          </div>

          <p className="rounded-md bg-gray-50 px-2 py-1.5 text-xs text-gray-500">
            {t('insertHint', { type: t(`types.${type}`), id: toId || '[[id]]' })}
          </p>
          {selectedBusy && <p className="text-xs text-amber-700">{t('duplicateHint')}</p>}
          {error && <p className="text-xs text-red-600">{error}</p>}
        </div>

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            disabled={busy}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100 disabled:opacity-50"
          >
            {t('cancel')}
          </button>
          <button
            onClick={handleCreate}
            disabled={!toId || busy || selectedBusy}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {busy ? t('creating') : t('create')}
          </button>
        </div>
      </div>
    </div>
  );
}
