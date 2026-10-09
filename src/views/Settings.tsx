import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '@/store/useAppStore';

interface SettingsProps {
  onClose: () => void;
}

export function Settings({ onClose }: SettingsProps) {
  const [path, setPath] = useState('');
  const [isSetting, setIsSetting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const loadEntries = useAppStore((s) => s.loadEntries);

  const handleSetRoot = async () => {
    setIsSetting(true);
    setError(null);
    try {
      await invoke('set_kb_root', { path });
      await loadEntries();
      onClose();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSetting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">Settings</h2>

        <div className="mt-4">
          <label className="block text-sm font-medium text-gray-700">
            Knowledge Base Root
          </label>
          <input
            type="text"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder="/path/to/knowledge-base"
            className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none"
          />
          <p className="mt-1 text-xs text-gray-500">
            The folder containing your entries (each entry is a subfolder with content.md).
          </p>
        </div>

        {error && (
          <div className="mt-3 rounded-md bg-red-50 p-2 text-sm text-red-700">
            {error}
          </div>
        )}

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
          >
            Cancel
          </button>
          <button
            onClick={handleSetRoot}
            disabled={!path || isSetting}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {isSetting ? 'Setting...' : 'Set Root'}
          </button>
        </div>
      </div>
    </div>
  );
}