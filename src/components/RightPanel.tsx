import React from 'react';
import { useTranslation } from 'react-i18next';
import type { PanelType } from '@/store/panelStore';

interface Props {
  type: PanelType;
  onClose: () => void;
  children: React.ReactNode;
}

const TITLE_KEY: Record<PanelType, string> = {
  detail: 'titleDetail',
  validation: 'titleValidation',
  settings: 'titleSettings',
  filter: 'titleFilter',
};

export const RightPanel: React.FC<Props> = ({ type, onClose, children }) => {
  const { t } = useTranslation('RightPanel');
  return (
    <div className="flex min-h-[140px] flex-1 flex-col overflow-hidden rounded-md border border-gray-200 bg-white shadow-sm">
      <div className="flex items-center justify-between border-b border-gray-200 px-3 py-2">
        <h3 className="text-sm font-medium text-gray-900">{t(TITLE_KEY[type])}</h3>
        <button
          onClick={onClose}
          className="text-gray-400 hover:text-gray-600"
          aria-label="close"
        >
          ✕
        </button>
      </div>
      <div className="flex-1 overflow-y-auto">{children}</div>
    </div>
  );
};
