import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Sidebar } from '@/components/Sidebar';
import { RightPanelStack } from '@/components/RightPanelStack';
import { MapView } from '@/views/MapView';
import { DetailPanel } from '@/views/DetailPanel';
import { Settings } from '@/views/Settings';
import { useAppStore } from '@/store/useAppStore';

export default function App() {
  const { t } = useTranslation('App');
  const loadEntries = useAppStore((s) => s.loadEntries);
  const refreshKbs = useAppStore((s) => s.refreshKbs);
  const error = useAppStore((s) => s.error);
  const isLoading = useAppStore((s) => s.isLoading);
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);
  const activeKbId = useAppStore((s) => s.activeKbId);

  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    refreshKbs().then(() => loadEntries());
  }, [refreshKbs, loadEntries]);

  return (
    <div className="flex h-screen w-screen bg-gray-50 text-gray-900">
      <Sidebar onOpenSettings={() => setSettingsOpen(true)} />
      <main className="relative flex-1">
        <button
          onClick={() => setSettingsOpen(true)}
          className="absolute right-4 top-4 z-10 rounded-md bg-white px-3 py-1.5 text-sm shadow-sm hover:bg-gray-50"
        >
          {t('openKbButton')}
        </button>
        {error && (
          <div className="absolute left-4 right-4 top-4 z-10 rounded-md bg-red-50 p-3 text-sm text-red-700 shadow-sm">
            {error}
          </div>
        )}
        {isLoading ? (
          <div className="flex h-full items-center justify-center text-sm text-gray-500">
            {t('loading')}
          </div>
        ) : activeKbId ? (
          <MapView />
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-3 text-sm text-gray-500">
            <p>{t('noActiveKb')}</p>
            <button
              onClick={() => setSettingsOpen(true)}
              className="rounded-md bg-blue-500 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-600"
            >
              {t('openKbAction')}
            </button>
          </div>
        )}
      </main>
      <RightPanelStack />
      {selectedEntryId && <DetailPanel />}
      {settingsOpen && <Settings onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
