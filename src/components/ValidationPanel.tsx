import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '@/store/useAppStore';
import { tRule } from '@/utils/tRule';

// Severity serializes from Rust as a unit-variant object (e.g. {"Error": null}).
type Severity = 'Error' | 'Warning' | 'Info';

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

  const rescan = async () => {
    if (!activeKbRoot) return;
    const r = await invoke<ValidationReport>('validate_kb', { kbRoot: activeKbRoot });
    setReport(r);
  };

  React.useEffect(() => {
    rescan();
  }, [activeKbRoot]);

  const filtered = report?.issues.filter((i) => severityKey(i.severity) === tab) ?? [];

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
          onClick={rescan}
          className="ml-auto rounded bg-gray-100 px-2 py-1 hover:bg-gray-200"
        >
          {t('rescan')}
        </button>
      </div>
      <div className="space-y-1">
        {filtered.length === 0 ? (
          <div className="text-sm text-gray-500">{t('noIssues')}</div>
        ) : (
          filtered.map((issue, i) => {
            const { title, message } = tRule(tVal, issue.rule_id, {
              id: issue.entry_id,
              target: issue.message.match(/\[\[(.+?)\]\]/)?.[1] || '',
              expected: issue.message.match(/should equal "(.+?)"/)?.[1] || '',
            });
            return (
              <div
                key={i}
                className="rounded border border-gray-200 bg-white p-2 text-xs"
              >
                <div className="font-medium">{title}: {issue.entry_id}</div>
                <div className="text-gray-600">{message || issue.message}</div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};