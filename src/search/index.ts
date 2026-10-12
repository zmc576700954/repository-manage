import type { Entry } from '@shared/types';
import { snippetAround, tokenize } from './highlight';

const FIELD_WEIGHTS = {
  title: 3.0,
  tags: 2.0,
  labels: 1.5,
  group: 1.0,
  content: 1.0,
} as const;

const MAX_RESULTS = 200;
const SNIPPET_RADIUS = 20;

/** A single search hit. `score > 0`. */
export interface SearchHit {
  entry: Entry;
  score: number;
  /** Snippet from content (only when content matched). */
  snippet: string | null;
}

/** LRU content cache for `get_entry_content`. */
export interface ContentCache {
  get(id: string): string | null;
  set(id: string, text: string): void;
  invalidate(predicate?: (id: string) => boolean): void;
  size(): number;
}

export function createContentCache(
  maxEntries = 200,
  maxAgeMs = 5 * 60 * 1000,
): ContentCache {
  const map = new Map<string, { text: string; at: number }>();

  return {
    get(id) {
      const v = map.get(id);
      if (!v) return null;
      if (Date.now() - v.at > maxAgeMs) {
        map.delete(id);
        return null;
      }
      // LRU touch
      map.delete(id);
      map.set(id, v);
      return v.text;
    },
    set(id, text) {
      if (map.has(id)) map.delete(id);
      map.set(id, { text, at: Date.now() });
      while (map.size > maxEntries) {
        const firstKey = map.keys().next().value;
        if (firstKey === undefined) break;
        map.delete(firstKey);
      }
    },
    invalidate(predicate) {
      if (!predicate) {
        map.clear();
        return;
      }
      for (const k of Array.from(map.keys())) {
        if (predicate(k)) map.delete(k);
      }
    },
    size() {
      return map.size;
    },
  };
}

/**
 * Compute the field score in [0, 1]: fraction of keywords matched in this field.
 * Returns 0 if any keyword is missing (strict AND).
 */
function fieldScore(fieldValue: string | null | undefined, kws: string[]): number {
  if (!fieldValue) return 0;
  const lower = fieldValue.toLowerCase();
  let hits = 0;
  for (const k of kws) {
    if (lower.includes(k.toLowerCase())) hits += 1;
  }
  if (hits === 0) return 0;
  return hits / kws.length;
}

/** Whether ALL keywords match somewhere in the union of given strings (case-insensitive). */
function allKeywordsPresent(blob: string, kws: string[]): boolean {
  const lower = blob.toLowerCase();
  return kws.every((k) => lower.includes(k.toLowerCase()));
}

export interface RankInputs {
  entries: Entry[];
  contentFor: (id: string) => string | null | Promise<string | null>;
  /** Bump this when entries change to invalidate memoised results. */
  epoch: number;
}

/**
 * Rank entries against `query`. Returns top MAX_RESULTS hits.
 * `contentFor` may return a cached string or fetch fresh. Promises are awaited
 * with a concurrency cap.
 */
export async function rankEntries(
  query: string,
  inputs: RankInputs,
): Promise<SearchHit[]> {
  const kws = tokenize(query);
  if (kws.length === 0) return [];

  const { entries, contentFor } = inputs;

  // Phase 1: in-memory filter on title/tags/labels/group.
  // Compute candidate set: every entry where in-memory fields contain ALL keywords.
  type Candidate = { entry: Entry; inMemoryScore: number };
  const candidates: Candidate[] = [];
  for (const e of entries) {
    const blob = [e.title, e.group ?? '', ...(e.tags ?? []), ...(e.labels ?? [])]
      .filter(Boolean)
      .join('\n');
    if (!allKeywordsPresent(blob, kws)) continue;
    const title = fieldScore(e.title, kws);
    const tags = fieldScore((e.tags ?? []).join(' '), kws);
    const labels = fieldScore((e.labels ?? []).join(' '), kws);
    const group = fieldScore(e.group, kws);
    const inMemoryScore =
      title * FIELD_WEIGHTS.title +
      tags * FIELD_WEIGHTS.tags +
      labels * FIELD_WEIGHTS.labels +
      group * FIELD_WEIGHTS.group;
    candidates.push({ entry: e, inMemoryScore });
  }

  if (candidates.length === 0) return [];

  // Phase 2: content search with bounded concurrency.
  const CONCURRENCY = 4;
  const results: SearchHit[] = [];

  async function process(c: Candidate) {
    const cached = await Promise.resolve(contentFor(c.entry.id));
    if (cached == null) return; // skip unreadable entries
    const fScore = fieldScore(cached, kws);
    if (fScore === 0) return; // content didn't actually match (could happen if kws span content only)
    const total = c.inMemoryScore + fScore * FIELD_WEIGHTS.content;
    results.push({
      entry: c.entry,
      score: total,
      snippet: snippetAround(cached, kws, SNIPPET_RADIUS),
    });
  }

  // Chunked concurrency
  for (let i = 0; i < candidates.length; i += CONCURRENCY) {
    const slice = candidates.slice(i, i + CONCURRENCY);
    await Promise.all(slice.map(process));
  }

  // Sort by score desc, then by title asc for stability
  results.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return a.entry.title.localeCompare(b.entry.title);
  });

  return results.slice(0, MAX_RESULTS);
}
