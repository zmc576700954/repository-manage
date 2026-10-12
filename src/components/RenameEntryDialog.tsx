import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import type { PrecheckRenameResult } from '@shared/types';

interface Props {
  entryId: string;
  initialName: string;
  initialGroup: string;
  existingGroups: string[];
  onClose: () => void;
  onSuccess: (newId: string) => void;
}

type PrecheckState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'valid'; result: PrecheckRenameResult }
  | { kind: 'conflict'; conflict: string }
  | { kind: 'error'; message: string };

export function RenameEntryDialog({
  entryId,
  initialName,
  initialGroup,
  existingGroups,
  onClose,
  onSuccess,
}: Props) {
  const { t } = useTranslation('RenameEntryDialog');
  const precheckRename = useAppStore((s) => s.precheckRename);
  const renameEntry = useAppStore((s) => s.renameEntry);

  const [newName, setNewName] = useState(initialName);
  const [newGroup, setNewGroup] = useState(initialGroup);
  const [cascade, setCascade] = useState(true);
  const [precheck, setPrecheck] = useState<PrecheckState>({ kind: 'idle' });
  const [submitting, setSubmitting] = useState(false);
  const debounceRef = useRef<number | null>(null);

  // Debounced precheck whenever name or group changes.
  useEffect(() => {
    if (newName === initialName && newGroup === initialGroup) {
      setPrecheck({ kind: 'idle' });
      return;
    }
    if (!newName.trim()) {
      setPrecheck({ kind: 'error', message: t('errorInvalidName') });
      return;
    }
    setPrecheck({ kind: 'checking' });
    if (debounceRef.current) window.clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(async () => {
      try {
        const result = await precheckRename(entryId, newName.trim(), newGroup.trim());
        if (result.conflictWith) {
          setPrecheck({ kind: 'conflict', conflict: result.conflictWith });
        } else if (result.valid) {
          setPrecheck({ kind: 'valid', result });
        } else {
          setPrecheck({ kind: 'error', message: t('errorInvalidName') });
        }
      } catch (e) {
        setPrecheck({ kind: 'error', message: (e as Error).message });
      }
    }, 200);
    return () => {
      if (debounceRef.current) window.clearTimeout(debounceRef.current);
    };
  }, [entryId, newName, newGroup, initialName, initialGroup, precheckRename, t]);

  const canSubmit = precheck.kind === 'valid' && !submitting;
  const showCascade = precheck.kind === 'valid' && precheck.result.incomingCount > 0;

  const handleSubmit = async () => {
    if (precheck.kind !== 'valid') return;
    setSubmitting(true);
    try {
      const r = await renameEntry(entryId, newName.trim(), newGroup.trim(), cascade);
      onSuccess(r.newId);
    } catch (e) {
      setPrecheck({ kind: 'error', message: (e as Error).message });
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">{t('title')}</h2>

        <div className="mt-4 space-y-3">
          <div>
            <label className="block text-sm font-medium text-gray-700">{t('newNameLabel')}</label>
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={t('newNamePlaceholder')}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm font-mono"
              autoFocus
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-gray-700">{t('newGroupLabel')}</label>
            <input
              list="kb-groups-rename"
              value={newGroup}
              onChange={(e) => setNewGroup(e.target.value.trim())}
              placeholder={t('newGroupPlaceholder')}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            />
            <datalist id="kb-groups-rename">
              {existingGroups.map((g) => (
                <option key={g} value={g} />
              ))}
            </datalist>
          </div>
        </div>

        {/* Precheck status */}
        <div className="mt-3 min-h-[40px] text-xs">
          {precheck.kind === 'checking' && (
            <p className="text-gray-500">{t('precheckChecking')}</p>
          )}
          {precheck.kind === 'valid' && precheck.result.incomingCount === 0 && (
            <p className="text-gray-500">{t('precheckNoIncoming')}</p>
          )}
          {precheck.kind === 'valid' && precheck.result.incomingCount === 1 && (
            <p className="text-amber-700">⚠ {t('precheckIncomingOne')}</p>
          )}
          {precheck.kind === 'valid' && precheck.result.incomingCount > 1 && (
            <p className="text-amber-700">
              ⚠ {t('precheckIncomingMany', { count: precheck.result.incomingCount })}
            </p>
          )}
          {precheck.kind === 'conflict' && (
            <p className="text-red-600">⚠ {t('conflictExists', { id: precheck.conflict })}</p>
          )}
          {precheck.kind === 'error' && (
            <p className="text-red-600">⚠ {precheck.message}</p>
          )}
        </div>

        {/* Cascade option (only when there are incoming refs) */}
        {showCascade && (
          <div className="mt-3 space-y-1 text-sm">
            <p className="font-medium text-gray-700">{t('cascadeLabel')}</p>
            <label className="flex items-center gap-2">
              <input
                type="radio"
                checked={cascade}
                onChange={() => setCascade(true)}
              />
              {t('cascadeUpdate')}
            </label>
            <label className="flex items-center gap-2">
              <input
                type="radio"
                checked={!cascade}
                onChange={() => setCascade(false)}
              />
              {t('cascadeLeave')}
            </label>
          </div>
        )}

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            disabled={submitting}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
          >
            {t('cancel')}
          </button>
          <button
            onClick={handleSubmit}
            disabled={!canSubmit}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {submitting ? t('renameBusy') : t('rename')}
          </button>
        </div>
      </div>
    </div>
  );
}