import { useEffect } from 'react';
import { Sidebar } from '@/components/Sidebar';
import { MapView } from '@/views/MapView';
import { DetailPanel } from '@/views/DetailPanel';
import { useAppStore } from '@/store/useAppStore';

export default function App() {
  const loadEntries = useAppStore((s) => s.loadEntries);
  const error = useAppStore((s) => s.error);
  const isLoading = useAppStore((s) => s.isLoading);
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  return (
    <div className="flex h-screen w-screen bg-gray-50 text-gray-900">
      <Sidebar />
      <main className="relative flex-1">
        {error && (
          <div className="absolute left-4 right-4 top-4 z-10 rounded-md bg-red-50 p-3 text-sm text-red-700 shadow-sm">
            {error}
          </div>
        )}
        {isLoading ? (
          <div className="flex h-full items-center justify-center text-sm text-gray-500">
            Loading...
          </div>
        ) : (
          <MapView />
        )}
      </main>
      {selectedEntryId && <DetailPanel />}
    </div>
  );
}
