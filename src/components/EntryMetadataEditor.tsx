import { useEffect, useMemo, useState, useCallback, useRef, KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import type { Entry } from '@shared/types';

interface Props {
  entry: Entry;
}

// D3 状态枚举统一：canonical 用规范中文枚举（与 kaiwu M3a 一致，括号注记
// 允许但编辑器只提供基准值）；旧英文自由文本保留为只读兼容选项，
// 用户选中 canonical 后即被替换。展示标签双内建（zh/en），不入 i18n 键。
const STATUS_OPTIONS = ['草案', '模板', '稳定', '需持续监控', '过期', '已归档'] as const;

const STATUS_DISPLAY: Record<string, { zh: string; en: string }> = {
  草案: { zh: '草案', en: 'Draft' },
  模板: { zh: '模板', en: 'Template' },
  稳定: { zh: '稳定', en: 'Stable' },
  需持续监控: { zh: '需持续监控', en: 'Monitor' },
  过期: { zh: '过期', en: 'Outdated' },
  已归档: { zh: '已归档', en: 'Archived' },
  // 旧自由文本（只读兼容显示）
  stable: { zh: '稳定（旧）', en: 'Stable (legacy)' },
  draft: { zh: '草稿（旧）', en: 'Draft (legacy)' },
  wip: { zh: '撰写中（旧）', en: 'WIP (legacy)' },
  deprecated: { zh: '遗弃（旧）', en: 'Deprecated (legacy)' },
};

export function EntryMetadataEditor({ entry }: Props) {
  const { t, i18n } = useTranslation('EntryMetadataEditor');
  const lang = i18n.language?.startsWith('zh') ? 'zh' : 'en';
  const updateMetadata = useAppStore((s) => s.updateMetadata);

  // Local controlled state mirrors entry, so rapid edits don't fight
  // a stale entry prop. The component remounts when the parent passes a
  // new entry, so we don't need to sync on entry.id change.
  const [title, setTitle] = useState(entry.title);
  const [tags, setTags] = useState<string[]>(entry.tags);
  const [labels, setLabels] = useState<string[]>(entry.labels);
  const [status, setStatus] = useState<string>(entry.status ?? '草案');
  const [tagInput, setTagInput] = useState('');
  const [labelInput, setLabelInput] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const savedAtRef = useRef<number | null>(null);
  // 用户在面板内输入/提交期间的本地草稿不能被外部刷新覆盖；卸载前
  // watcher 事件可能随时到达，所以用 ref 记录焦点与在途提交。
  const rootRef = useRef<HTMLDivElement>(null);
  const pendingCommitsRef = useRef(0);

  // 外部（watcher 事件触发的静默刷新）改了元数据后，把最新值同步到本地
  // 草稿，否则面板会一直展示挂载时的旧值。焦点在面板内或有提交未完成时
  // 跳过，避免打断输入、抢焦点或覆盖用户刚敲的内容。
  const externalSnapshot = useMemo(
    () => JSON.stringify([entry.title, entry.tags, entry.labels, entry.status ?? '草案']),
    [entry.title, entry.tags, entry.labels, entry.status]
  );
  const syncedSnapshotRef = useRef(externalSnapshot);

  useEffect(() => {
    // 内容没变（只是 entries 数组产生新引用）时不重设本地 state，
    // 避免每次静默刷新都让整块面板重绘。
    if (syncedSnapshotRef.current === externalSnapshot) return;
    if (pendingCommitsRef.current > 0) return;
    const active = document.activeElement;
    if (rootRef.current && active && rootRef.current.contains(active)) return;
    syncedSnapshotRef.current = externalSnapshot;
    setTitle(entry.title);
    setTags(entry.tags);
    setLabels(entry.labels);
    setStatus(entry.status ?? '草案');
  }, [externalSnapshot, entry.title, entry.tags, entry.labels, entry.status]);

  const commit = useCallback(
    (changes: { title?: string; tags?: string[]; labels?: string[]; status?: string }) => {
      setError(null);
      pendingCommitsRef.current += 1;
      return updateMetadata(entry.id, changes)
        .then(() => {
          const now = Date.now();
          savedAtRef.current = now;
          setSavedAt(now);
          setTimeout(() => {
            if (savedAtRef.current === now) {
              savedAtRef.current = null;
              setSavedAt(null);
            }
          }, 1500);
        })
        .catch((e) => {
          setError((e as Error).message);
          // Revert local state on error
          setTitle(entry.title);
          setTags(entry.tags);
          setLabels(entry.labels);
          setStatus(entry.status ?? '草案');
        })
        .finally(() => {
          pendingCommitsRef.current -= 1;
        });
    },
    [entry.id, entry.title, entry.tags, entry.labels, entry.status, updateMetadata]
  );

  const addTag = (val: string) => {
    const trimmed = val.trim();
    if (!trimmed) return;
    if (trimmed.includes(',')) {
      setError(t('errorInvalidTag'));
      return;
    }
    if (trimmed.length > 50) {
      setError(t('errorTagTooLong'));
      return;
    }
    if (tags.length >= 15) {
      setError(t('errorTooManyTags'));
      return;
    }
    if (tags.includes(trimmed)) {
      setTagInput('');
      return;
    }
    const next = [...tags, trimmed];
    setTags(next);
    setTagInput('');
    commit({ tags: next });
  };

  const removeTag = (tag: string) => {
    const next = tags.filter((x) => x !== tag);
    setTags(next);
    commit({ tags: next });
  };

  const handleTagKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      addTag(tagInput);
    } else if (e.key === 'Backspace' && tagInput === '' && tags.length > 0) {
      removeTag(tags[tags.length - 1]);
    }
  };

  return (
    <div ref={rootRef} className="border-b border-gray-200 p-4">
      {/* Title (editable inline) */}
      <div className="mb-3">
        <label className="mb-1 block text-xs font-semibold uppercase tracking-wider text-gray-500">
          {t('titleLabel')}
        </label>
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={() => {
            const trimmed = title.trim();
            if (!trimmed) {
              setError(t('errorEmptyTitle'));
              setTitle(entry.title);
              return;
            }
            if (trimmed !== entry.title) commit({ title: trimmed });
          }}
          placeholder={t('titlePlaceholder')}
          className="w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm"
        />
      </div>

      {/* Tags */}
      <div className="mb-3">
        <label className="mb-1 block text-xs font-semibold uppercase tracking-wider text-gray-500">
          {t('tagsLabel')}
        </label>
        <div className="flex flex-wrap items-center gap-1 rounded-md border border-gray-300 px-2 py-1.5">
          {tags.map((tag) => (
            <span
              key={tag}
              className="flex items-center gap-1 rounded-full bg-blue-50 px-2 py-0.5 text-xs text-blue-700"
            >
              #{tag}
              <button
                type="button"
                onClick={() => removeTag(tag)}
                aria-label={t('removeTagAria')}
                className="text-blue-500 hover:text-red-600"
              >
                ×
              </button>
            </span>
          ))}
          <input
            value={tagInput}
            onChange={(e) => setTagInput(e.target.value)}
            onKeyDown={handleTagKey}
            onBlur={() => tagInput && addTag(tagInput)}
            placeholder={tags.length === 0 ? t('tagsPlaceholder') : ''}
            className="flex-1 min-w-[80px] border-none bg-transparent text-sm outline-none"
          />
        </div>
      </div>

      {/* Labels */}
      <div className="mb-3">
        <label className="mb-1 block text-xs font-semibold uppercase tracking-wider text-gray-500">
          {t('labelsLabel')}
        </label>
        <div className="flex flex-wrap items-center gap-1 rounded-md border border-gray-300 px-2 py-1.5">
          {labels.map((label) => (
            <span
              key={label}
              className="flex items-center gap-1 rounded-full bg-amber-50 px-2 py-0.5 text-xs text-amber-700"
            >
              {label}
              <button
                type="button"
                onClick={() => {
                  const next = labels.filter((x) => x !== label);
                  setLabels(next);
                  commit({ labels: next });
                }}
                aria-label={t('removeTagAria')}
                className="text-amber-500 hover:text-red-600"
              >
                ×
              </button>
            </span>
          ))}
          <input
            value={labelInput}
            onChange={(e) => setLabelInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ',') {
                e.preventDefault();
                const trimmed = labelInput.trim();
                if (trimmed && !labels.includes(trimmed)) {
                  const next = [...labels, trimmed];
                  setLabels(next);
                  setLabelInput('');
                  commit({ labels: next });
                }
              }
            }}
            onBlur={() => labelInput && setLabelInput('')}
            placeholder={labels.length === 0 ? t('labelsPlaceholder') : ''}
            className="flex-1 min-w-[80px] border-none bg-transparent text-sm outline-none"
          />
        </div>
      </div>

      {/* Status */}
      <div className="mb-2">
        <label className="mb-1 block text-xs font-semibold uppercase tracking-wider text-gray-500">
          {t('statusLabel')}
        </label>
        <select
          value={status}
          onChange={(e) => {
            setStatus(e.target.value);
            commit({ status: e.target.value });
          }}
          className="rounded-md border border-gray-300 px-2 py-1 text-sm"
        >
          {/* 非规范旧值（含括号注记的自定义值）保底展示，避免静默改写 */}
          {!STATUS_OPTIONS.includes(status as (typeof STATUS_OPTIONS)[number]) &&
            status !== '' && (
              <option key={status} value={status}>
                {STATUS_DISPLAY[status]?.[lang] ?? status}
              </option>
            )}
          {STATUS_OPTIONS.map((s) => (
            <option key={s} value={s}>
              {STATUS_DISPLAY[s]?.[lang] ?? s}
            </option>
          ))}
        </select>
      </div>

      {error && <p className="mt-2 text-xs text-red-600">{error}</p>}
      {savedAt && <p className="mt-2 text-xs text-green-600">{t('savedFlash')}</p>}
    </div>
  );
}
