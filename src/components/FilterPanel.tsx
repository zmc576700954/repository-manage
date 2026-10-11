import React from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';

export const FilterPanel: React.FC = () => {
  const { t } = useTranslation('FilterPanel');
  const { entries, filterLabels, toggleFilterLabel, clearFilters } = useAppStore();

  const allLabels = Array.from(
    new Set(entries.flatMap((e) => e.labels || [])),
  );

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium">{t('labels')}</h4>
        <button onClick={clearFilters} className="text-xs text-blue-500">
          {t('clearAll')}
        </button>
      </div>
      <div className="space-y-1">
        {allLabels.map((label) => (
          <label key={label} className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={filterLabels.has(label)}
              onChange={() => toggleFilterLabel(label)}
            />
            <span>{label}</span>
          </label>
        ))}
      </div>
    </div>
  );
};
