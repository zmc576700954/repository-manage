import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '@/store/useAppStore';
import { tRule } from '@/utils/tRule';

type Severity = 'Error' | 'Warning' | 'Info';
type GroupBy = 'severity' | 'entry';

interface ValidationIssue {
  entry_id: string;
  rule_id: string;
  severity: Record<Severity, null>;
  message: string;
}

interface ValidationReport {
  kb_root: string;
  issues: ValidationIssue[];
}

const severityKey = (s: Record<Severity, null>): Severity => {
  if ('Error' in s) return 'Error';
  if ('Warning' in s) return 'Warning';
  return 'Info';
};

export const ValidationPanel: React.FC = () => {
  const { t } = useTranslation('ValidationPanel');
  const { t: tVal } = useTranslation('Validation');
  const activeKbRoot = useAppStore((s) =>
    s.kbs.find((k) => k.id === s.activeKbId)?.path ?? null
  );
  const [report, setReport] = useState<ValidationReport | null>(null);
  const [tab, setTab] = useState<Severity>('Error');
  const [groupBy, setGroupBy] = useState<GroupBy>('severity');
  const [busyRule, setBusyRule] = useState<string | null>(null);

  const rescan = async () => {
    if (!activeKbRoot) return;
    try {
      const r = await invoke<ValidationReport>('validate_kb', { kbRoot: activeKbRoot });
      setReport(r);
    } catch (e) {
      console.error(e);
    }
  };

  React.useEffect(() => {
    rescan();
  }, [activeKbRoot]);

  const filtered = report?.issues.filter((i) => severityKey(i.severity) === tab) ?? [];

  const applyFix = async (entryId: string, ruleId: string, confirm: boolean) => {
    const key = `${entryId}::${ruleId}`;
    setBusyRule(key);
    try {
      await invoke('apply_fix', { entryId, ruleId, confirm });
      await rescan();
    } catch (e) {
      console.error(e);
    } finally {
      setBusyRule(null);
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex gap-2 text-xs">
        {(['Error', 'Warning', 'Info'] as const).map((s) => (
          <button
            key={s}
            onClick={() => setTab(s)}
            className={`rounded px-2 py-1 ${
              tab === s ? 'bg-blue-500 text-white' : 'bg-gray-100'
            }`}
          >
            {s === 'Error' ? t('errors') : s === 'Warning' ? t('warnings') : t('info')}
          </button>
        ))}
        <button
          onClick={() => setGroupBy(groupBy === 'severity' ? 'entry' : 'severity')}
          className="rounded bg-gray-100 px-2 py-1 hover:bg-gray-200"
        >
          {groupBy === 'severity' ? t('groupByEntry') : t('groupBySeverity')}
        </button>
        <button
          onClick={rescan}
          className="ml-auto rounded bg-gray-100 px-2 py-1 hover:bg-gray-200"
        >
          {t('rescan')}
        </button>
      </div>
      <div className="space-y-1">
        {filtered.length === 0 ? (
          <div className="text-sm text-gray-500">{t('noIssues')}</div>
        ) : groupBy === 'severity' ? (
          filtered.map((issue, i) => {
            const safeMessage = typeof issue.message === 'string' ? issue.message : '';
            const { title, message } = tRule(tVal, issue.rule_id, {
              id: issue.entry_id,
              target: safeMessage.match(/\[\[(.+?)\]\]/)?.[1] || '',
              expected: safeMessage.match(/should equal "(.+?)"/)?.[1] || '',
            });
            const key = `${issue.entry_id}::${issue.rule_id}`;
            return (
              <div
                key={i}
                className="rounded border border-gray-200 bg-white p-2 text-xs"
              >
                <div className="font-medium">{title}: {issue.entry_id}</div>
                <div className="text-gray-600">{message || issue.message}</div>
                <div className="mt-2 flex gap-2">
                  <button
                    disabled={busyRule === key}
                    onClick={() => applyFix(issue.entry_id, issue.rule_id, false)}
                    className="rounded bg-red-500 px-2 py-1 text-[11px] text-white hover:bg-red-600 disabled:opacity-50"
                  >
                    {busyRule === key ? t('applying') : t('oneClickFix')}
                  </button>
                </div>
              </div>
            );
          })
        ) : (
          (() => {
            const byEntry = new Map<string, ValidationIssue[]>();
            for (const issue of filtered) {
              const list = byEntry.get(issue.entry_id) ?? [];
              list.push(issue);
              byEntry.set(issue.entry_id, list);
            }
            return Array.from(byEntry.entries()).map(([entryId, entryIssues]) => (
              <div key={entryId} className="rounded border border-gray-200 bg-white p-2">
                <div className="text-xs font-semibold">{entryId}</div>
                <div className="mt-1 space-y-1">
                  {entryIssues.map((issue, j) => {
                    const safeMessage = typeof issue.message === 'string' ? issue.message : '';
                    const { title, message } = tRule(tVal, issue.rule_id, {
                      id: issue.entry_id,
                      target: safeMessage.match(/\[\[(.+?)\]\]/)?.[1] || '',
                      expected: safeMessage.match(/should equal "(.+?)"/)?.[1] || '',
                    });
                    const key = `${issue.entry_id}::${issue.rule_id}`;
                    return (
                      <div key={j} className="rounded bg-gray-50 p-2 text-xs">
                        <div className="font-medium">{title}</div>
                        <div className="text-gray-600">{message || issue.message}</div>
                        <div className="mt-1 flex gap-2">
                          <button
                            disabled={busyRule === key}
                            onClick={() => applyFix(issue.entry_id, issue.rule_id, false)}
                            className="rounded bg-red-500 px-2 py-0.5 text-[10px] text-white hover:bg-red-600 disabled:opacity-50"
                          >
                            {busyRule === key ? t('applying') : t('oneClickFix')}
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            ));
          })()
        )}
      </div>
    </div>
  );
};