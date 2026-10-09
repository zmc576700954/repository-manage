import { useAppStore } from '@/store/useAppStore';

export function DetailPanel() {
  const selectedEntryId = useAppStore((s) => s.selectedEntryId);
  const entry = useAppStore((s) =>
    s.entries.find((e) => e.id === selectedEntryId)
  );

  if (!entry) {
    return (
      <div className="flex h-full w-96 items-center justify-center border-l border-gray-200 bg-white text-sm text-gray-500">
        Select an entry to see details
      </div>
    );
  }

  return (
    <aside className="flex h-full w-96 flex-col overflow-y-auto border-l border-gray-200 bg-white">
      <div className="border-b border-gray-200 p-4">
        <h2 className="text-lg font-semibold text-gray-900">{entry.title}</h2>
        <div className="mt-2 flex flex-wrap gap-1">
          {entry.tags.map((tag) => (
            <span
              key={tag}
              className="rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-700"
            >
              {tag}
            </span>
          ))}
        </div>
        {entry.group && (
          <div className="mt-2 text-xs text-gray-500">
            Group: <span className="font-medium">{entry.group}</span>
          </div>
        )}
      </div>

      {entry.attachments.length > 0 && (
        <div className="border-b border-gray-200 p-4">
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
            Attachments ({entry.attachments.length})
          </h3>
          <ul className="space-y-1">
            {entry.attachments.map((att, idx) => (
              <li
                key={idx}
                className="flex items-center gap-2 rounded-md px-2 py-1 text-sm hover:bg-gray-50"
              >
                <span className="text-gray-400">{att.type === 'image' ? '🖼' : '📎'}</span>
                <span className="text-gray-700">{att.caption || att.path}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="border-b border-gray-200 p-4">
        <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
          Relations ({entry.relations.length})
        </h3>
        {entry.relations.length === 0 ? (
          <p className="text-sm text-gray-500">No relations</p>
        ) : (
          <ul className="space-y-1">
            {entry.relations.map((rel, idx) => (
              <li
                key={idx}
                className="flex items-center justify-between rounded-md px-2 py-1 text-sm hover:bg-gray-50"
              >
                <span className="text-gray-700">
                  → {rel.toId}{' '}
                  <span className="ml-1 text-xs text-gray-500">({rel.type})</span>
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="p-4 text-xs text-gray-400">
        <div>ID: {entry.id}</div>
        <div className="mt-1 truncate">Path: {entry.path}</div>
      </div>
    </aside>
  );
}
