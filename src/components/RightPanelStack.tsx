import React from 'react';
import { usePanelStore, PanelType } from '@/store/panelStore';
import { RightPanel } from './RightPanel';
import { ValidationPanel } from './ValidationPanel';
import { Settings } from '@/views/Settings';
import { DetailPanel } from '@/views/DetailPanel';

export const RightPanelStack: React.FC = () => {
  const { openPanels, close, width } = usePanelStore();

  if (openPanels.length === 0) return null;

  return (
    <div
      className="flex h-full flex-col gap-2 border-l border-gray-200 bg-gray-50 p-2"
      style={{ width }}
    >
      {openPanels.map((panelType) => (
        <RightPanel
          key={panelType}
          type={panelType}
          onClose={() => close(panelType)}
        >
          <PanelContent type={panelType} />
        </RightPanel>
      ))}
    </div>
  );
};

const PanelContent: React.FC<{ type: PanelType }> = ({ type }) => {
  switch (type) {
    case 'detail':
      return <DetailPanel />;
    case 'validation':
      return <ValidationPanel />;
    case 'settings':
      return <Settings />;
    case 'filter':
      return <div className="p-4 text-sm text-gray-500">Filter placeholder</div>;
  }
};