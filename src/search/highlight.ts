/**
 * Split a string into segments preserving original casing, marking
 * which segments match any of the given keywords (case-insensitive).
 *
 * Used to render highlighted matches in the search dropdown.
 */
export type HighlightSegment = { text: string; matched: boolean };

export function highlightMatches(text: string, keywords: string[]): HighlightSegment[] {
  if (!keywords.length) return [{ text, matched: false }];
  // Build a single regex with all keywords, longest-first to avoid partial overlap.
  const sorted = [...keywords]
    .filter((k) => k.length > 0)
    .sort((a, b) => b.length - a.length);
  if (sorted.length === 0) return [{ text, matched: false }];

  const escaped = sorted.map((k) => k.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  const re = new RegExp(`(${escaped.join('|')})`, 'gi');

  const out: HighlightSegment[] = [];
  let lastIdx = 0;
  for (const m of text.matchAll(re)) {
    const idx = m.index ?? 0;
    if (idx > lastIdx) {
      out.push({ text: text.slice(lastIdx, idx), matched: false });
    }
    out.push({ text: m[0], matched: true });
    lastIdx = idx + m[0].length;
  }
  if (lastIdx < text.length) {
    out.push({ text: text.slice(lastIdx), matched: false });
  }
  return out;
}

/**
 * Extract a snippet around the first occurrence of any keyword.
 * Window is `radius` chars before / after; truncates with `..` prefix/suffix.
 */
export function snippetAround(
  text: string,
  keywords: string[],
  radius = 20,
): string {
  if (!text || !keywords.length) return text.slice(0, radius * 2);
  const lower = text.toLowerCase();
  let firstIdx = -1;
  for (const k of keywords) {
    if (!k) continue;
    const idx = lower.indexOf(k.toLowerCase());
    if (idx >= 0 && (firstIdx < 0 || idx < firstIdx)) {
      firstIdx = idx;
    }
  }
  if (firstIdx < 0) return text.slice(0, radius * 2);

  const start = Math.max(0, firstIdx - radius);
  const end = Math.min(text.length, firstIdx + radius * 2);
  const prefix = start > 0 ? '..' : '';
  const suffix = end < text.length ? '..' : '';
  return prefix + text.slice(start, end) + suffix;
}

/**
 * Split a free-text query into individual keywords (whitespace-separated).
 */
export function tokenize(query: string): string[] {
  return query
    .trim()
    .split(/\s+/)
    .filter((s) => s.length > 0);
}
