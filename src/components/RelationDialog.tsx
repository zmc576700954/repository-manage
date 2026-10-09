import { useState } from 'react';
import { useAppStore } from '@/store/useAppStore';
import type { RelationType } from '@shared/types';

const RELATION_TYPES: RelationType[] = [
  'reference',
  'derived',
  'contradicts',
  'supersedes',
  'extends',
];

interface RelationDialogProps {
  fromId: string;
  onClose: () => void;
}

export function RelationDialog({ fromId, onClose }: RelationDialogProps) {
  const entries = useAppStore((s) => s.entries);
  const createRelation = useAppStore((s) => s.createRelation);

  const [toId, setToId] = useState('');
  const [type, setType] = useState<RelationType>('reference');
  const [note, setNote] = useState('');

  const otherEntries = entries.filter((e) => e.id !== fromId);

  const handleCreate = async () => {
    if (!toId) return;
    await createRelation(fromId, toId, type, note || undefined);
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">Create Relation</h2>
        <p className="mt-1 text-sm text-gray-500">From: {fromId}</p>

        <div className="mt-4 space-y-3">
          <div>
            <label className="block text-sm font-medium text-gray-700">To Entry</label>
            <select
              value={toId}
              onChange={(e) => setToId(e.target.value)}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            >
              <option value="">Select...</option>
              {otherEntries.map((e) => (
                <option key={e.id} value={e.id}>
                  {e.title}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Type</label>
            <select
              value={type}
              onChange={(e) => setType(e.target.value as RelationType)}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            >
              {RELATION_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Note (optional)</label>
            <input
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            />
          </div>
        </div>

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
          >
            Cancel
          </button>
          <button
            onClick={handleCreate}
            disabled={!toId}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}
