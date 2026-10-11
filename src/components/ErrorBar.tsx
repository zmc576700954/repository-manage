import React from 'react';
import { useTranslation } from 'react-i18next';
import { usePanelStore } from '@/store/panelStore';

interface Props {
  errorCount: number;
  warningCount: number;
}

export const ErrorBar: React.FC<Props> = ({ errorCount, warningCount }) => {
  const { t } = useTranslation('ErrorBar');
  const { open } = usePanelStore();

  if (errorCount === 0 && warningCount === 0) return null;

  return (
    <div
      className={`cursor-pointer px-4 py-2 text-sm ${
        errorCount > 0
          ? 'bg-red-100 text-red-800'
          : 'bg-yellow-100 text-yellow-800'
      }`}
      onClick={() => open('validation')}
    >
      {errorCount > 0 && t('errors', { count: errorCount })}
      {errorCount > 0 && warningCount > 0 && ' · '}
      {warningCount > 0 && t('warnings', { count: warningCount })}
      {' — '}
      {t('clickToView')}
    </div>
  );
};