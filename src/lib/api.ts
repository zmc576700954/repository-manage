import type { Entry, Relation } from '@shared/types';

const BASE_URL = 'http://127.0.0.1:19181';

interface ApiResponse<T> {
  data: T;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${BASE_URL}${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...init?.headers,
    },
  });

  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(error.error || response.statusText);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  const body: ApiResponse<T> = await response.json();
  return body.data;
}

export const api = {
  listEntries: () => request<Entry[]>('/api/entries'),

  getEntry: (id: string) => request<Entry>(`/api/entries/${encodeURIComponent(id)}`),

  createRelation: (relation: Omit<Relation, 'id'>) =>
    request<Relation>('/api/relations', {
      method: 'POST',
      body: JSON.stringify(relation),
    }),

  deleteRelation: (fromId: string, toId: string, type: string) =>
    request<void>(
      `/api/relations/${encodeURIComponent(fromId)}/${encodeURIComponent(toId)}/${encodeURIComponent(type)}`,
      { method: 'DELETE' }
    ),
};

export const isTauriEnvironment = (): boolean => {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
};