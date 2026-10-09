import { useEffect } from 'react';
import { Sidebar } from '@/components/Sidebar';
import { useAppStore } from '@/store/useAppStore';

export default function App() {
  const loadEntries = useAppStore((s) => s.loadEntries);
  const error = useAppStore((s) => s.error);
  const isLoading = useAppStore((s) => s.isLoading);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  return (
    <div className="flex h-screen w-screen bg-gray-50 text-gray-900">
      <Sidebar />
      <main className="flex-1">
        {error && (
          <div className="m-4 rounded-md bg-red-50 p-3 text-sm text-red-700">
            {error}
          </div>
        )}
        {isLoading && (
          <div className="m-4 text-sm text-gray-500">Loading...</div>
        )}
      </main>
    </div>
  );
}
