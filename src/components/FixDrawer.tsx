import React, { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { useFixStore } from '@/store/fixStore';

interface ValidationIssue {
  entry_id: string;
  rule_id: string;
  // Rust `Severity` enum serializes as a bare variant string
  // (e.g. "Error"), not `{ "Error": null }`.
  severity: 'Error' | 'Warning' | 'Info';
  message: string;
}

interface ValidationReport {
  kb_root: string;
  issues: ValidationIssue[];
}

interface Props {
  activeKbRoot: string | null;
}

export const FixDrawer: React.FC<Props> = ({ activeKbRoot }) => {
  const { t } = useTranslation('Fix');
  const drawerEntryId = useFixStore((s) => s.drawerEntryId);
  const closeDrawer = useFixStore((s) => s.closeDrawer);
  const pushFix = useFixStore((s) => s.pushFix);

  const [issues, setIssues] = useState<ValidationIssue[]>([]);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    if (!drawerEntryId || !activeKbRoot) return;
    invoke<ValidationReport>('validate_kb', { kbRoot: activeKbRoot })
      .then((r) => setIssues(r.issues.filter((i) => i.entry_id === drawerEntryId)))
      .catch(console.error);
  }, [drawerEntryId, activeKbRoot]);

  useEffect(() => {
    const h = (e: Event) => {
      const ce = e as CustomEvent<{ entryId: string }>;
      useFixStore.getState().openDrawer(ce.detail.entryId);
    };
    window.addEventListener('open-fix-drawer', h);
    return () => window.removeEventListener('open-fix-drawer', h);
  }, []);

  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape') closeDrawer(); };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [closeDrawer]);

  if (!drawerEntryId) return null;

  const errors = Array.isArray(issues)
    ? issues.filter((i) => i && i.severity === 'Error')
    : [];
  const loading = !activeKbRoot || issues.length === 0;

  const applyFix = async (ruleId: string, confirm: boolean) => {
    setBusy(ruleId);
    try {
      await invoke('apply_fix', { entryId: drawerEntryId, ruleId, confirm });
      pushFix({ ruleId, entryId: drawerEntryId });
      setIssues((prev) => prev.filter((i) => i.rule_id !== ruleId));
    } catch (e) {
      console.error(e);
      alert(t('common.failed', { reason: String(e) }));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div
      role="dialog"
      aria-label={t('drawerTitle', { entry: drawerEntryId ?? '' })}
      className="fixed right-0 top-12 z-50 flex w-96 max-w-[90vw] flex-col rounded-l-lg border border-gray-200 bg-white shadow-xl"
    >
      <div className="flex items-center justify-between border-b px-4 py-2">
        <h3 className="truncate text-sm font-semibold">
          {t('drawerTitle', { entry: drawerEntryId ?? '' })}
        </h3>
        <button
          onClick={closeDrawer}
          aria-label="close"
          className="rounded px-2 py-1 text-sm text-gray-500 hover:bg-gray-100 hover:text-gray-800"
        >
          ✕
        </button>
      </div>
      <div className="max-h-[70vh] flex-1 space-y-3 overflow-y-auto p-4">
        {loading && (
          <div className="text-sm text-gray-500">{t('noErrors')}</div>
        )}
        {errors.map((issue) => (
          <div key={issue.rule_id} className="rounded border border-red-100 bg-red-50 p-2">
            <div className="text-xs font-medium text-red-700">
              {issue.rule_id}: {issue.message}
            </div>
            <div className="mt-2 flex gap-2">
              <button
                disabled={busy === issue.rule_id}
                onClick={() => applyFix(issue.rule_id, false)}
                className="rounded bg-red-500 px-2 py-1 text-xs text-white hover:bg-red-600 disabled:opacity-50"
              >
                {busy === issue.rule_id ? t('applying') : t(`${issue.rule_id}.apply`, { defaultValue: t('common.apply') })}
              </button>
              <button className="rounded bg-gray-100 px-2 py-1 text-xs hover:bg-gray-200">
                {t('common.openDocs')}
              </button>
            </div>
          </div>
        ))}
      </div>
      <div className="border-t px-4 py-2">
        <button
          onClick={closeDrawer}
          className="w-full rounded-md bg-gray-100 px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-200"
        >
          {t('backToMap', { defaultValue: '返回地图' })}
        </button>
      </div>
    </div>
  );
};