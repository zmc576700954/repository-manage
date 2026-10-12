import { useState, useCallback, useRef, KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import type { Entry } from '@shared/types';

interface Props {
  entry: Entry;
}

const STATUS_OPTIONS = ['stable', 'draft', 'wip', 'deprecated'] as const;

export function EntryMetadataEditor({ entry }: Props) {
  const { t } = useTranslation('EntryMetadataEditor');
  const updateMetadata = useAppStore((s) => s.updateMetadata);

  // Local controlled state mirrors entry, so rapid edits don't fight
  // a stale entry prop. The component remounts when the parent passes a
  // new entry, so we don't need to sync on entry.id change.
  const [title, setTitle] = useState(entry.title);
  const [tags, setTags] = useState<string[]>(entry.tags);
  const [labels, setLabels] = useState<string[]>(entry.labels);
  const [status, setStatus] = useState<string>(entry.status ?? 'stable');
  const [tagInput, setTagInput] = useState('');
  const [labelInput, setLabelInput] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const savedAtRef = useRef<number | null>(null);

  const commit = useCallback(
    (changes: { title?: string; tags?: string[]; labels?: string[]; status?: string }) => {
      setError(null);
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
          setStatus(entry.status ?? 'stable');
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
    <div className="border-b border-gray-200 p-4">
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
          {STATUS_OPTIONS.map((s) => (
            <option key={s} value={s}>
              {t(`status${s.charAt(0).toUpperCase() + s.slice(1)}`)}
            </option>
          ))}
        </select>
      </div>

      {error && <p className="mt-2 text-xs text-red-600">{error}</p>}
      {savedAt && <p className="mt-2 text-xs text-green-600">{t('savedFlash')}</p>}
    </div>
  );
}
