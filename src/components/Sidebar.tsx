import { useAppStore } from '@/store/useAppStore';

export function Sidebar() {
  const entries = useAppStore((s) => s.entries);
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);
  const selectEntry = useAppStore((s) => s.selectEntry);
  const loadEntries = useAppStore((s) => s.loadEntries);

  return (
    <aside className="flex w-64 flex-col border-r border-gray-200 bg-white">
      <div className="border-b border-gray-200 p-4">
        <button
          onClick={() => loadEntries()}
          className="w-full rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600"
        >
          Reload
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-2">
        <ul className="space-y-1">
          {entries.map((entry) => (
            <li key={entry.id}>
              <button
                onClick={() => selectEntry(entry.id)}
                className={`w-full rounded-md px-3 py-2 text-left text-sm transition ${
                  selectedEntryId === entry.id
                    ? 'bg-blue-50 font-medium text-blue-700'
                    : 'text-gray-700 hover:bg-gray-100'
                }`}
              >
                {entry.title}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </aside>
  );
}
