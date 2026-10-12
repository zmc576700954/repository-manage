import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '@/store/useAppStore';
import type { Entry } from '@shared/types';
import { createContentCache, rankEntries, type ContentCache, type SearchHit } from './index';

const DEBOUNCE_MS = 150;

interface UseSearchResult {
  query: string;
  setQuery: (q: string) => void;
  hits: SearchHit[];
  loading: boolean;
}

/**
 * Hook driving SearchBox. Holds the debounced query, runs `rankEntries`
 * against current KB entries + cached content, and cancels stale runs.
 *
 * Cache is bound to `activeKbId` — switching KBs clears it.
 */
export function useSearch(): UseSearchResult {
  const entries = useAppStore((s) => s.entries);
  const activeKbId = useAppStore((s) => s.activeKbId);
  const [query, setQuery] = useState('');
  const [debounced, setDebounced] = useState('');
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [loading, setLoading] = useState(false);

  // Per-KB cache; recreated on KB switch so old content goes away.
  const cacheRef = useRef<ContentCache>(createContentCache());
  useEffect(() => {
    cacheRef.current = createContentCache();
    setHits([]);
    setDebounced('');
    setQuery('');
  }, [activeKbId]);

  // Debounce query -> debounced
  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(query), DEBOUNCE_MS);
    return () => window.clearTimeout(id);
  }, [query]);

  // Run ranking when debounced query / entries change. Cancel stale runs.
  const epochRef = useRef(0);
  useEffect(() => {
    if (!debounced.trim()) {
      setHits([]);
      setLoading(false);
      return;
    }
    const epoch = ++epochRef.current;
    setLoading(true);
    const cache = cacheRef.current;

    const contentFor = async (id: string): Promise<string | null> => {
      const hit = cache.get(id);
      if (hit !== null) return hit;
      try {
        const text = await invoke<string | null>('get_entry_content', { id });
        if (text != null) cache.set(id, text);
        return text;
      } catch {
        return null;
      }
    };

    rankEntries(debounced, { entries, contentFor, epoch: epochRef.current })
      .then((r) => {
        // Stale check: only apply if this is still the latest run.
        if (epoch !== epochRef.current) return;
        setHits(r);
        setLoading(false);
      })
      .catch(() => {
        if (epoch !== epochRef.current) return;
        setHits([]);
        setLoading(false);
      });
  }, [debounced, entries]);

  return useMemo(
    () => ({ query, setQuery, hits, loading }),
    [query, hits, loading],
  );
}

/** Re-export Entry for callers that need it. */
export type { Entry };
