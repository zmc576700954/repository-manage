import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import type { Entry, KBInfo, RelationType } from '@shared/types';

const RELATION_TYPES: RelationType[] = [
  'reference',
  'derived',
  'extends',
  'supersedes',
  'contradicts',
];

const ROOT_OPTION = '__root__';

/** 触发 EntryDialog 的来源，决定默认行为。 */
export type EntryDialogMode =
  | { kind: 'ungrouped' }                              // 在 KB 根目录新建
  | { kind: 'in_group'; parentGroup: string }          // 在某 group 下新建
  | { kind: 'as_child_of'; parent: Entry }             // 新 entry 是某 entry 的下级
  | { kind: 'as_parent_of'; child: Entry };            // 新 entry 是某 entry 的上级

interface EntryDialogProps {
  activeKb: KBInfo;
  mode: EntryDialogMode;
  existingGroups: string[];
  /** 顶层分组（不带 `/`），用于"已有分组"下拉 — 避免下拉项过长 */
  topLevelGroups?: string[];
  onClose: () => void;
}

export function EntryDialog({
  activeKb,
  mode,
  existingGroups,
  topLevelGroups,
  onClose,
}: EntryDialogProps) {
  const { t } = useTranslation('EntryDialog');
  const createEntry = useAppStore((s) => s.createEntry);

  // 默认 parent group：
  // - ungrouped: 根（可选 ROOT_OPTION）
  // - in_group: 锁定为指定 group
  // - as_child_of: 锁定为 parent.group（或 ROOT_OPTION）
  // - as_parent_of: 锁定为 child.group（或 ROOT_OPTION）
  const defaultGroup = (() => {
    if (mode.kind === 'in_group') return mode.parentGroup;
    if (mode.kind === 'as_child_of') return mode.parent.group ?? '';
    if (mode.kind === 'as_parent_of') return mode.child.group ?? '';
    return existingGroups[0] ?? '';
  })();
  const isGroupLocked = mode.kind !== 'ungrouped';

  const [parentChoice, setParentChoice] = useState<string>(defaultGroup || ROOT_OPTION);
  const [customGroup, setCustomGroup] = useState('');
  const [name, setName] = useState('');
  const [content, setContent] = useState('');
  const [linkSource, setLinkSource] = useState(mode.kind === 'as_child_of' || mode.kind === 'as_parent_of');
  const [linkDirection, setLinkDirection] = useState<'parent' | 'child'>(
    mode.kind === 'as_parent_of' ? 'parent' : 'child'
  );
  const [relationType, setRelationType] = useState<RelationType>('reference');
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const sourceEntry = mode.kind === 'as_child_of' || mode.kind === 'as_parent_of'
    ? (mode.kind === 'as_child_of' ? mode.parent : mode.child)
    : null;

  const parentGroup = (() => {
    if (isGroupLocked) return defaultGroup;
    if (parentChoice === ROOT_OPTION) return customGroup.trim();
    if (parentChoice === '__custom__') return customGroup.trim();
    return parentChoice;
  })();

  // 自动从 source 生成默认 name
  useEffect(() => {
    if (name === '' && sourceEntry) {
      setName(slugFromText(sourceEntry.title || sourceEntry.id));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [parentGroup]);

  const handleSubmit = async () => {
    setError(null);
    if (!name) {
      setError(t('errors.nameRequired'));
      return;
    }
    if (parentChoice === '__custom__' && !customGroup.trim()) {
      setError(t('errors.customGroupRequired'));
      return;
    }
    setBusy(true);

    let linkFromEntry: string | undefined;
    let linkToEntry: string | undefined;
    if (linkSource && sourceEntry) {
      if (linkDirection === 'parent') {
        // 新 entry 作为上级：source 是下级
        // 关系方向 新 entry → source，写入新 .md
        linkToEntry = sourceEntry.id;
      } else {
        // 新 entry 作为下级：source 是上级
        // 关系方向 source → 新 entry，写入 source .md
        linkFromEntry = sourceEntry.id;
      }
    }

    const newId = await createEntry({
      parentGroup,
      name,
      content: content || undefined,
      linkFromEntry,
      linkToEntry,
      relationType: linkSource ? relationType : undefined,
      note: note || undefined,
    });
    setBusy(false);
    if (newId) {
      onClose();
    } else {
      setError(t('errors.createFailed'));
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-lg rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">{t('title')}</h2>
        <p className="mt-1 text-sm text-gray-500">{describeMode(mode, sourceEntry, t)}</p>

        <div className="mt-3 rounded-md border border-blue-200 bg-blue-50/60 p-2.5">
          <div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-wider text-blue-700">
            <span>📁</span>
            <span>{t('targetKbHeader')}</span>
          </div>
          <div className="mt-1 truncate text-sm font-semibold text-gray-900">
            {activeKb.name}
          </div>
          <div className="truncate font-mono text-[11px] text-gray-500" title={activeKb.path}>
            {activeKb.path}
          </div>
        </div>

        <div className="mt-4 space-y-3">
          <div>
            <label className="block text-sm font-medium text-gray-700">
              {t('groupLabel')}{isGroupLocked ? t('groupLockedSuffix') : ''}
            </label>
            {isGroupLocked ? (
              <div className="mt-1 flex items-center gap-2">
                <span className="rounded bg-gray-100 px-2 py-1 font-mono text-xs text-gray-700">
                  {parentGroup || t('groupRootOption')}
                </span>
                <span className="text-xs text-gray-500">{t('groupDetermined')}</span>
              </div>
            ) : (
              <>
                <select
                  value={parentChoice}
                  onChange={(e) => setParentChoice(e.target.value)}
                  className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
                >
                  <option value={ROOT_OPTION}>{t('groupRootOption')}</option>
                  {(topLevelGroups ?? existingGroups.map((g) => g.split('/')[0])) .map((g) => (
                    <option key={g} value={g}>
                      {g}
                    </option>
                  ))}
                  <option value="__custom__">{t('groupCustomOption')}</option>
                </select>
                {parentChoice === '__custom__' && (
                  <input
                    type="text"
                    value={customGroup}
                    onChange={(e) => setCustomGroup(e.target.value)}
                    placeholder={t('groupCustomPlaceholder')}
                    className="mt-2 w-full rounded-md border border-gray-300 px-3 py-2 text-sm font-mono"
                  />
                )}
              </>
            )}
            <p className="mt-1 text-xs text-gray-500">
              {t('savePathHint')}{' '}
              <span className="font-mono">
                {'<kb>/' + (parentGroup || '<root>') + '/' + (name || '<name>') + '.md'}
              </span>
            </p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">{t('nameLabel')}</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value.trim())}
              placeholder={t('namePlaceholder')}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm font-mono"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">
              {t('contentLabel')}
            </label>
            <textarea
              value={content}
              onChange={(e) => setContent(e.target.value)}
              rows={4}
              placeholder={t('contentPlaceholder')}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 font-mono text-xs"
            />
          </div>

          {sourceEntry && (
            <div className="rounded-md border border-gray-200 bg-gray-50 p-3">
              <label className="flex items-center gap-2 text-sm font-medium text-gray-800">
                <input
                  type="checkbox"
                  checked={linkSource}
                  onChange={(e) => setLinkSource(e.target.checked)}
                />
                {t('linkHeader')}
                <span className="ml-1 rounded bg-gray-200 px-1.5 py-0.5 font-mono text-xs text-gray-600">
                  {sourceEntry.id}
                </span>
              </label>
              {linkSource && (
                <div className="mt-3 space-y-3">
                  <div>
                    <label className="block text-xs font-medium text-gray-700">{t('linkDirLabel')}</label>
                    <div className="mt-1 grid grid-cols-2 gap-2">
                      <label
                        className={`flex cursor-pointer items-start gap-2 rounded-md border px-2 py-1.5 text-xs ${
                          linkDirection === 'parent'
                            ? 'border-blue-500 bg-blue-50 text-blue-900'
                            : 'border-gray-200 bg-white text-gray-700'
                        }`}
                      >
                        <input
                          type="radio"
                          name="linkDir"
                          value="parent"
                          checked={linkDirection === 'parent'}
                          onChange={() => setLinkDirection('parent')}
                          className="mt-0.5"
                        />
                        <span>
                          <span className="font-semibold">{t('linkDirParentLabel')}</span>
                          <span className="mt-0.5 block text-[11px] text-gray-600">
                            {t('linkDirParentHint', { title: sourceEntry.title })}
                          </span>
                        </span>
                      </label>
                      <label
                        className={`flex cursor-pointer items-start gap-2 rounded-md border px-2 py-1.5 text-xs ${
                          linkDirection === 'child'
                            ? 'border-blue-500 bg-blue-50 text-blue-900'
                            : 'border-gray-200 bg-white text-gray-700'
                        }`}
                      >
                        <input
                          type="radio"
                          name="linkDir"
                          value="child"
                          checked={linkDirection === 'child'}
                          onChange={() => setLinkDirection('child')}
                          className="mt-0.5"
                        />
                        <span>
                          <span className="font-semibold">{t('linkDirChildLabel')}</span>
                          <span className="mt-0.5 block text-[11px] text-gray-600">
                            {t('linkDirChildHint', { title: sourceEntry.title })}
                          </span>
                        </span>
                      </label>
                    </div>
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="block text-xs font-medium text-gray-700">{t('relationTypeLabel')}</label>
                      <select
                        value={relationType}
                        onChange={(e) => setRelationType(e.target.value as RelationType)}
                        className="mt-1 w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm"
                      >
                        {RELATION_TYPES.map((t) => (
                          <option key={t} value={t}>
                            {t}
                          </option>
                        ))}
                      </select>
                    </div>
                    <div>
                      <label className="block text-xs font-medium text-gray-700">
                        {t('noteLabel')}
                      </label>
                      <input
                        value={note}
                        onChange={(e) => setNote(e.target.value)}
                        placeholder={t('notePlaceholder')}
                        className="mt-1 w-full rounded-md border border-gray-300 px-2 py-1.5 text-sm"
                      />
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}

          {error && (
            <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>
          )}
        </div>

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
          >
            {t('cancel')}
          </button>
          <button
            disabled={busy}
            onClick={handleSubmit}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {busy ? t('submitBusy') : t('submitIdle')}
          </button>
        </div>
      </div>
    </div>
  );
}

function describeMode(
  mode: EntryDialogMode,
  source: Entry | null,
  t: (k: string, opts?: Record<string, unknown>) => string
): string {
  if (mode.kind === 'ungrouped') return t('modeDesc.ungrouped');
  if (mode.kind === 'in_group') return t('modeDesc.inGroup', { group: mode.parentGroup });
  if (mode.kind === 'as_child_of') return t('modeDesc.asChild', { title: source?.title });
  if (mode.kind === 'as_parent_of') return t('modeDesc.asParent', { title: source?.title });
  return '';
}

function slugFromText(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9一-龥]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 40);
}
