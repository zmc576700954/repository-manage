import React from 'react';
import { useTranslation } from 'react-i18next';
import { usePanelStore } from '@/store/panelStore';

interface Props {
  errorCount: number;
  warningCount: number;
}

export const ErrorBar: React.FC<Props> = ({ errorCount, warningCount }) => {
  const { t } = useTranslation('ErrorBar');
  const open = usePanelStore((s) => s.open);

  if (errorCount === 0 && warningCount === 0) return null;

  const isError = errorCount > 0;
  const colorClass = isError
    ? 'bg-red-100 text-red-800 hover:bg-red-200'
    : 'bg-yellow-100 text-yellow-800 hover:bg-yellow-200';

  return (
    <button
      onClick={() => open('validation')}
      className={`absolute right-4 top-4 z-10 rounded-full px-3 py-1 text-xs shadow-sm transition-colors ${colorClass}`}
      title={t('tooltip', { errors: errorCount, warnings: warningCount })}
    >
      {isError && `⚠ ${errorCount}`}
      {isError && warningCount > 0 && ' · '}
      {!isError && warningCount > 0 && `${warningCount}`}
    </button>
  );
};
