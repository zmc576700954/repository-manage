import React from 'react';
import { useTranslation } from 'react-i18next';

interface FixDescriptor {
  rule_id: string;
  kind: { type: string };
  risk: string;
  label_key: string;
  docs_key: string;
}

interface Props {
  descriptor: FixDescriptor;
  onApply: (confirm: boolean) => void;
  busy: boolean;
}

export const FixWizard: React.FC<Props> = ({ descriptor, onApply, busy }) => {
  const { t } = useTranslation('Fix');
  const label = t(descriptor.label_key, { defaultValue: t('common.apply') });
  const docs = t(descriptor.docs_key, { defaultValue: '' });

  return (
    <div className="space-y-2 rounded border border-gray-200 bg-white p-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-gray-700">{descriptor.rule_id}</span>
        <span className={`rounded px-1 text-[10px] ${
          descriptor.risk === 'low' ? 'bg-green-100 text-green-700' :
          descriptor.risk === 'medium' ? 'bg-yellow-100 text-yellow-700' :
          'bg-red-100 text-red-700'
        }`}>{t(`risk.${descriptor.risk}`)}</span>
      </div>
      {docs && <p className="whitespace-pre-wrap text-xs text-gray-600">{docs}</p>}
      <div className="flex gap-2">
        {descriptor.kind.type === 'oneClick' && (
          <button
            disabled={busy}
            onClick={() => onApply(false)}
            className="rounded bg-blue-500 px-3 py-1 text-xs text-white hover:bg-blue-600 disabled:opacity-50"
          >{busy ? t('applying') : label}</button>
        )}
        {descriptor.kind.type === 'confirm' && (
          <button
            disabled={busy}
            onClick={() => { if (confirm(label)) onApply(true); }}
            className="rounded bg-red-500 px-3 py-1 text-xs text-white hover:bg-red-600 disabled:opacity-50"
          >{busy ? t('applying') : label}</button>
        )}
        {descriptor.kind.type === 'manual' && (
          <span className="text-xs text-gray-500">{t('kind.manual')}</span>
        )}
      </div>
    </div>
  );
};