import React from 'react';
import { usePanelStore, PanelType } from '@/store/panelStore';
import { RightPanel } from './RightPanel';
import { ValidationPanel } from './ValidationPanel';
import { Settings } from '@/views/Settings';

const panelTitles: Record<PanelType, string> = {
  validation: 'Validation',
  settings: 'Settings',
  filter: 'Filter',
  detail: 'Detail',
};

export const RightPanelStack: React.FC = () => {
  const { openPanels, close, width } = usePanelStore();

  if (openPanels.length === 0) return null;

  return (
    <div
      className="flex h-full flex-col border-l border-gray-200 bg-gray-50"
      style={{ width }}
    >
      {openPanels.map((panelType) => (
        <RightPanel
          key={panelType}
          title={panelTitles[panelType]}
          onClose={() => close(panelType)}
        >
          {panelType === 'validation' && <ValidationPanel />}
          {panelType === 'settings' && <Settings />}
          {panelType === 'filter' && <div>Filter placeholder</div>}
        </RightPanel>
      ))}
    </div>
  );
};
