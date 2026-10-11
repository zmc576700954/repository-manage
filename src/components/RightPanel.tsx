import React from 'react';

interface Props {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
}

export const RightPanel: React.FC<Props> = ({ title, onClose, children }) => {
  return (
    <div className="flex h-full flex-col border-b border-gray-200 bg-white">
      <div className="flex items-center justify-between border-b border-gray-200 px-4 py-2">
        <h3 className="text-sm font-medium text-gray-900">{title}</h3>
        <button
          onClick={onClose}
          className="text-gray-400 hover:text-gray-600"
          aria-label="Close"
        >
          ✕
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-4">{children}</div>
    </div>
  );
};