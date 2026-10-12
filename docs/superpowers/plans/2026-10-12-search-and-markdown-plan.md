# 全文搜索 + Markdown 渲染实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Synapse KB 中加入跨字段全文搜索与原生 markdown 渲染，让 `[[wikilink]]` 成为可点击内部链接。

**Architecture:**
- 搜索：纯前端实现（spec §7 决策 1）。React hook 包评分+LRU+并发竞态，SearchBox 嵌 Sidebar 顶部，⌘K 全局唤起。
- 渲染：react-markdown + remark-gfm + 自写 remark 插件解析 wikilink。DetailPanel 内替换 `<pre>`，保留原文切换。

**Tech Stack:** React 18 + TypeScript + Zustand + react-flow + react-markdown 9.x + remark-gfm 4.x + i18next

**Reference:** 详见 `docs/superpowers/specs/2026-10-12-search-and-markdown-design.md`

---

## File Structure

**新建：**
- `src/search/index.ts` — 评分 + LRU 缓存 + snippet
- `src/search/useSearch.ts` — debounce + memoize + abort
- `src/search/highlight.ts` — 匹配词高亮工具
- `src/search/SearchBox.tsx` — 搜索框 UI
- `src/search/SearchBox.zh.json` — i18n
- `src/search/SearchBox.en.json` — i18n
- `src/markdown/remarkWikilink.ts` — remark 插件识别 `[[id]]` / `[[id|caption]]`
- `src/markdown/MarkdownView.tsx` — react-markdown 容器，含错误回退
- `src/markdown/MarkdownView.zh.json` — i18n
- `src/markdown/MarkdownView.en.json` — i18n

**修改：**
- `src/components/Sidebar.tsx` — 顶部嵌入 SearchBox，注册 ⌘K
- `src/views/DetailPanel.tsx` — 替换 `<pre>` 为 MarkdownView，加原文/渲染切换
- `src/i18n/resources.ts` — 注册 Search / MarkdownView 两个 NS
- `package.json` — 新增 react-markdown + remark-gfm 依赖

---

## Task 1: 安装依赖

**Files:**
- Modify: `package.json`

- [ ] **Step 1: 安装 react-markdown 与 remark-gfm**

```bash
cd /Users/zhumingchen/repository-manage && npm install react-markdown@^9 remark-gfm@^4
```

- [ ] **Step 2: 验证 package.json 已写入**

```bash
grep -E '"react-markdown"|"remark-gfm"' /Users/zhumingchen/repository-manage/package.json
```

Expected: 两行均出现，版本号为 ^9.x / ^4.x

- [ ] **Step 3: 类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 编译通过（仅依赖装好，不应有任何业务代码错误）

- [ ] **Step 4: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add package.json package-lock.json && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "chore(deps): add react-markdown 9 + remark-gfm 4"
```

---

## Task 2: 创建 i18n 资源文件

**Files:**
- Create: `src/search/SearchBox.zh.json`
- Create: `src/search/SearchBox.en.json`
- Create: `src/markdown/MarkdownView.zh.json`
- Create: `src/markdown/MarkdownView.en.json`

- [ ] **Step 1: 创建 SearchBox.zh.json**

写入 `/Users/zhumingchen/repository-manage/src/search/SearchBox.zh.json`：

```json
{
  "placeholder": "搜索整个知识库…",
  "shortcutHint": "⌘K",
  "noResults": "无匹配条目",
  "searching": "搜索中…",
  "resultCount": "{count} 条匹配",
  "inGroupBadge": "📁 {group}",
  "openEntry": "打开",
  "createFromRef": "从引用创建",
  "loadFailed": "读取失败，已跳过",
  "matchHighlight": "匹配"
}
```

- [ ] **Step 2: 创建 SearchBox.en.json**

写入 `/Users/zhumingchen/repository-manage/src/search/SearchBox.en.json`：

```json
{
  "placeholder": "Search across the KB…",
  "shortcutHint": "Ctrl+K",
  "noResults": "No matches",
  "searching": "Searching…",
  "resultCount": "{count} matches",
  "inGroupBadge": "📁 {group}",
  "openEntry": "Open",
  "createFromRef": "Create from ref",
  "loadFailed": "Read failed, skipped",
  "matchHighlight": "match"
}
```

- [ ] **Step 3: 创建 MarkdownView.zh.json**

写入 `/Users/zhumingchen/repository-manage/src/markdown/MarkdownView.zh.json`：

```json
{
  "renderError": "渲染失败，已切换到原文",
  "renderErrorTitle": "渲染失败",
  "viewToggleRender": "渲染",
  "viewToggleSource": "原文",
  "wikilinkBroken": "(引用不存在)",
  "wikilinkBrokenHint": "点击创建"
}
```

- [ ] **Step 4: 创建 MarkdownView.en.json**

写入 `/Users/zhumingchen/repository-manage/src/markdown/MarkdownView.en.json`：

```json
{
  "renderError": "Render failed, showing source",
  "renderErrorTitle": "Render failed",
  "viewToggleRender": "Rendered",
  "viewToggleSource": "Source",
  "wikilinkBroken": "(missing target)",
  "wikilinkBrokenHint": "Click to create"
}
```

- [ ] **Step 5: 验证 JSON 合法**

```bash
for f in /Users/zhumingchen/repository-manage/src/search/SearchBox.zh.json \
         /Users/zhumingchen/repository-manage/src/search/SearchBox.en.json \
         /Users/zhumingchen/repository-manage/src/markdown/MarkdownView.zh.json \
         /Users/zhumingchen/repository-manage/src/markdown/MarkdownView.en.json; do
  node -e "JSON.parse(require('fs').readFileSync('$f', 'utf8'))" && echo "OK: $f"
done
```

Expected: 4 行 `OK: ...`

- [ ] **Step 6: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add src/search/SearchBox.zh.json src/search/SearchBox.en.json src/markdown/MarkdownView.zh.json src/markdown/MarkdownView.en.json && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(i18n): add Search + MarkdownView namespaces (zh, en)"
```

---

## Task 3: 注册新 i18n 命名空间

**Files:**
- Modify: `src/i18n/resources.ts:1-38`

- [ ] **Step 1: 添加 import**

在 `/Users/zhumingchen/repository-manage/src/i18n/resources.ts` 顶部 import 之后（约第 30 行 `RightPanelEn` 之前）追加：

```ts
import SearchBoxZh from '../search/SearchBox.zh.json';
import SearchBoxEn from '../search/SearchBox.en.json';
import MarkdownViewZh from '../markdown/MarkdownView.zh.json';
import MarkdownViewEn from '../markdown/MarkdownView.en.json';
```

- [ ] **Step 2: 添加到 NS 数组**

修改 `NS` 常量（约第 32 行）：

```ts
export const NS = ['App', 'Errors', 'Settings', 'Sidebar', 'NodeCard', 'RelationDialog', 'MapView', 'EntryDialog', 'DetailPanel', 'ValidationPanel', 'Validation', 'Error', 'Fix', 'FilterPanel', 'RightPanel', 'SearchBox', 'MarkdownView'] as const;
```

- [ ] **Step 3: 添加到 zh/en resources 对象**

修改 `resources` 对象（约第 35-38 行），在两个对象末尾分别添加：

```ts
  zh: { App: AppZh, Errors: ErrorsZh, Settings: SettingsZh, Sidebar: SidebarZh, NodeCard: NodeCardZh, RelationDialog: RelationDialogZh, MapView: MapViewZh, EntryDialog: EntryDialogZh, DetailPanel: DetailPanelZh, ValidationPanel: ValidationPanelZh, Validation: ValidationZh, Error: ErrorZh, Fix: FixZh, FilterPanel: FilterPanelZh, RightPanel: RightPanelZh, SearchBox: SearchBoxZh, MarkdownView: MarkdownViewZh },
  en: { App: AppEn, Errors: ErrorsEn, Settings: SettingsEn, Sidebar: SidebarEn, NodeCard: NodeCardEn, RelationDialog: RelationDialogEn, MapView: MapViewEn, EntryDialog: EntryDialogEn, DetailPanel: DetailPanelEn, ValidationPanel: ValidationPanelEn, Validation: ValidationEn, Error: ErrorEn, Fix: FixEn, FilterPanel: FilterPanelEn, RightPanel: RightPanelEn, SearchBox: SearchBoxEn, MarkdownView: MarkdownViewEn },
```

- [ ] **Step 4: 类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过

- [ ] **Step 5: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add src/i18n/resources.ts && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(i18n): register SearchBox + MarkdownView namespaces"
```

---

## Task 4: 创建 highlight 工具模块

**Files:**
- Create: `src/search/highlight.ts`

- [ ] **Step 1: 实现 highlight.ts**

写入 `/Users/zhumingchen/repository-manage/src/search/highlight.ts`：

```ts
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
```

- [ ] **Step 2: 类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过

- [ ] **Step 3: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add src/search/highlight.ts && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(search): add keyword highlighting + snippet + tokenize utils"
```

---

## Task 5: 创建 search index 模块（评分 + LRU）

**Files:**
- Create: `src/search/index.ts`

- [ ] **Step 1: 实现 index.ts**

写入 `/Users/zhumingchen/repository-manage/src/search/index.ts`：

```ts
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
```

- [ ] **Step 2: 类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过

- [ ] **Step 3: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add src/search/index.ts && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(search): weighted ranking + LRU content cache"
```

---

## Task 6: 创建 useSearch hook（debounce + memoize + abort）

**Files:**
- Create: `src/search/useSearch.ts`

- [ ] **Step 1: 实现 useSearch.ts**

写入 `/Users/zhumingchen/repository-manage/src/search/useSearch.ts`：

```ts
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

  // The unused `entries` is for re-running on store updates; keep reference.
  void entries;

  return useMemo(
    () => ({ query, setQuery, hits, loading }),
    [query, hits, loading],
  );
}

/** Re-export Entry for callers that need it. */
export type { Entry };
```

- [ ] **Step 2: 类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过（store 中 `entries` 已是 `Entry[]` 类型）

- [ ] **Step 3: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add src/search/useSearch.ts && git -c user.email="zhumingchen@local" -c user.name="ZBuming" commit -m "feat(search): useSearch hook with 150ms debounce + abort + KB-scoped cache"
```

---

## Task 7: 创建 SearchBox 组件

**Files:**
- Create: `src/search/SearchBox.tsx`

- [ ] **Step 1: 实现 SearchBox.tsx**

写入 `/Users/zhumingchen/repository-manage/src/search/SearchBox.tsx`：

```tsx
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import { usePanelStore } from '@/store/panelStore';
import { useSearch } from './useSearch';
import { highlightMatches, tokenize } from './highlight';
import type { SearchHit } from './index';

export function SearchBox() {
  const { t } = useTranslation('SearchBox');
  const activeKbId = useAppStore((s) => s.activeKbId);
  const selectEntry = useAppStore((s) => s.selectEntry);
  const openPanel = usePanelStore((s) => s.open);
  const { query, setQuery, hits, loading } = useSearch();

  const [open, setOpen] = useState(false);
  const [focusIdx, setFocusIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // ⌘K / Ctrl+K to focus
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const isMac = navigator.platform.toUpperCase().includes('MAC');
      const meta = isMac ? e.metaKey : e.ctrlKey;
      if (meta && (e.key === 'k' || e.key === 'K')) {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, []);

  // Click-outside closes dropdown
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (!containerRef.current?.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, []);

  // Reset focus when hits length changes
  useEffect(() => {
    setFocusIdx(0);
  }, [hits.length, query]);

  const visible = activeKbId && (open || query.length > 0);
  const showDropdown = visible && query.trim().length > 0;

  const handleSelect = (hit: SearchHit) => {
    selectEntry(hit.entry.id);
    openPanel('detail');
    setOpen(false);
    inputRef.current?.blur();
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (!showDropdown || hits.length === 0) {
      if (e.key === 'Escape') inputRef.current?.blur();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setFocusIdx((i) => Math.min(i + 1, hits.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setFocusIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const hit = hits[focusIdx];
      if (hit) handleSelect(hit);
    } else if (e.key === 'Escape') {
      setOpen(false);
      inputRef.current?.blur();
    }
  };

  if (!activeKbId) return null;

  const kws = tokenize(query);

  return (
    <div ref={containerRef} className="relative mt-2">
      <div className="relative">
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={handleKeyDown}
          placeholder={t('placeholder')}
          aria-label={t('placeholder')}
          className="w-full rounded-md border border-gray-300 bg-white px-2 py-1 pr-12 text-sm placeholder-gray-400 focus:border-blue-400 focus:outline-none"
        />
        <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 rounded border border-gray-200 bg-gray-50 px-1 font-mono text-[10px] text-gray-500">
          {t('shortcutHint')}
        </span>
      </div>

      {showDropdown && (
        <div className="absolute left-0 right-0 z-30 mt-1 max-h-80 overflow-y-auto rounded-md border border-gray-200 bg-white shadow-lg">
          {loading && hits.length === 0 ? (
            <div className="px-3 py-2 text-xs text-gray-500">{t('searching')}</div>
          ) : hits.length === 0 ? (
            <div className="px-3 py-2 text-xs text-gray-500">{t('noResults')}</div>
          ) : (
            <>
              <div className="border-b border-gray-100 px-3 py-1 text-[11px] text-gray-500">
                {t('resultCount', { count: hits.length })}
              </div>
              <ul>
                {hits.map((hit, idx) => (
                  <li
                    key={hit.entry.id}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => handleSelect(hit)}
                    className={`cursor-pointer border-b border-gray-50 px-3 py-1.5 text-xs last:border-b-0 ${
                      idx === focusIdx ? 'bg-blue-50' : 'hover:bg-gray-50'
                    }`}
                  >
                    <div className="truncate font-medium text-gray-900">
                      {highlightMatches(hit.entry.title, kws).map((seg, i) => (
                        <span
                          key={i}
                          className={seg.matched ? 'bg-yellow-200 font-semibold' : ''}
                        >
                          {seg.text}
                        </span>
                      ))}
                    </div>
                    {hit.entry.group && (
                      <div className="mt-0.5 truncate text-[11px] text-gray-500">
                        {t('inGroupBadge', { group: hit.entry.group })}
                      </div>
                    )}
                    {hit.snippet && (
                      <div className="mt-0.5 truncate font-mono text-[11px] text-gray-500">
                        {highlightMatches(hit.snippet, kws).map((seg, i) => (
                          <span
                            key={i}
                            className={seg.matched ? 'bg-yellow-200' : ''}
                          >
                            {seg.text}
                          </span>
                        ))}
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: 类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过

- [ ] **Step 3: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add src/search/SearchBox.tsx && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(search): SearchBox component with keyboard nav + ⌘K"
```

---

## Task 8: 在 Sidebar 嵌入 SearchBox

**Files:**
- Modify: `src/components/Sidebar.tsx:77-120`

- [ ] **Step 1: 添加 import**

在 `/Users/zhumingchen/repository-manage/src/components/Sidebar.tsx` 顶部（约第 1-7 行 import 之后）追加：

```ts
import { SearchBox } from '@/search/SearchBox';
```

- [ ] **Step 2: 嵌入 SearchBox**

在 `<div className="border-b border-gray-200 p-3">` 块的结尾（即 `<button onClick={() => loadEntries()}` reload 按钮之后，`</div>` 闭合之前）插入：

```tsx
        <SearchBox />
```

具体位置：第 119 行 `        ) : null}` 之后，第 120 行 `      </div>` 之前，缩进 8 个空格。

- [ ] **Step 3: 类型检查 + 视觉验证**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过

启动 dev server 验证：基本仅查看布局

```bash
cd /Users/zhumingchen/repository-manage && npm run dev &
sleep 5
```

打开 http://localhost:1420 ，确认搜索框位于 KB 名称和 reload 按钮之间的位置。

Ctrl+C 关闭。

- [ ] **Step 4: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add src/components/Sidebar.tsx && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(search): mount SearchBox in Sidebar"
```

---

## Task 9: 创建 remarkWikilink 插件

**Files:**
- Create: `src/markdown/remarkWikilink.ts`

- [ ] **Step 1: 实现 remarkWikilink.ts**

写入 `/Users/zhumingchen/repository-manage/src/markdown/remarkWikilink.ts`：

```ts
import { visit, SKIP } from 'unist-util-visit';

/**
 * remark 插件：扫描文本节点中的 [[id]] 或 [[id|caption]]，
 * 替换为自定义 wikilink 节点。
 *
 * 节点结构：
 *   { type: 'wikilink', data: { id: string, caption: string } }
 *
 * `id` 为 entry id（不含 `|caption` 后缀）。
 *   caption 为可选的别名；未指定时与 id 相同。
 *
 * 不处理 code/inlineCode 容器内的文本（不递归）。
 */
type TextNode = { type: 'text'; value: string };
type ParentNode = { type: string; children: any[] };

const WIKILINK_RE = /\[\[([^\[\]\|]+)(?:\|([^\]]+))?\]\]/g;

function isTextNode(n: any): n is TextNode {
  return n && n.type === 'text' && typeof n.value === 'string';
}

function isCodeLike(type: string): boolean {
  return type === 'code' || type === 'inlineCode' || type === 'codeBlock';
}

export function remarkWikilink() {
  return (tree: any) => {
    visit(tree, (node: any, index, parent: ParentNode | undefined) => {
      if (!isTextNode(node)) return;
      // Skip if inside code-like containers
      if (parent && isCodeLike(parent.type)) return;

      const value = node.value;
      WIKILINK_RE.lastIndex = 0;
      const matches: { start: number; end: number; id: string; caption: string }[] = [];
      let m: RegExpExecArray | null;
      while ((m = WIKILINK_RE.exec(value))) {
        const id = m[1];
        const caption = m[2] ?? id;
        matches.push({ start: m.index, end: m.index + m[0].length, id, caption });
      }
      if (matches.length === 0) return;

      const out: any[] = [];
      let cursor = 0;
      for (const m of matches) {
        if (m.start > cursor) {
          out.push({ type: 'text', value: value.slice(cursor, m.start) });
        }
        out.push({
          type: 'wikilink',
          data: { id: m.id, caption: m.caption },
          children: [{ type: 'text', value: m.caption }],
        });
        cursor = m.end;
      }
      if (cursor < value.length) {
        out.push({ type: 'text', value: value.slice(cursor) });
      }

      if (parent && typeof index === 'number') {
        parent.children.splice(index, 1, ...out);
        return [SKIP, index + out.length];
      }
    });
  };
}
```

- [ ] **Step 2: 添加 unist-util-visit 依赖**

```bash
cd /Users/zhumingchen/repository-manage && npm install unist-util-visit@^5
```

- [ ] **Step 3: 类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过

- [ ] **Step 4: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add src/markdown/remarkWikilink.ts package.json package-lock.json && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(markdown): remarkWikilink plugin"
```

---

## Task 10: 创建 MarkdownView 组件

**Files:**
- Create: `src/markdown/MarkdownView.tsx`

- [ ] **Step 1: 实现 MarkdownView.tsx**

写入 `/Users/zhumingchen/repository-manage/src/markdown/MarkdownView.tsx`：

```tsx
import { useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { remarkWikilink } from './remarkWikilink';
import type { Components } from 'react-markdown';

export interface MarkdownViewProps {
  /** The raw markdown source. */
  source: string;
  /**
   * Resolve a wikilink target id. Returns target info if it exists
   * in the current KB; otherwise null to indicate a broken reference.
   */
  resolveTarget: (id: string) => { id: string; title: string } | null;
  /**
   * Click handler for an existing target: navigate to it.
   */
  onNavigate: (id: string) => void;
  /**
   * Click handler for a broken target: open "create from ref" dialog.
   */
  onCreateFromRef: (id: string, caption: string) => void;
}

/** Render markdown into React nodes, with [[wikilink]] handling. */
export function MarkdownView({
  source,
  resolveTarget,
  onNavigate,
  onCreateFromRef,
}: MarkdownViewProps) {
  const { t } = useTranslation('MarkdownView');
  const [renderError, setRenderError] = useState(false);

  // Reset error state when source changes
  useEffect(() => {
    setRenderError(false);
  }, [source]);

  const components: Components = useMemo(() => {
    return {
      wikilink(props: { node?: any; data?: { id: string; caption: string }; children?: React.ReactNode }) {
        const id = props.data?.id ?? '';
        const caption = props.data?.caption ?? id;
        if (!id) return <>{caption}</>;
        const target = resolveTarget(id);
        const handleClick = (e: React.MouseEvent) => {
          e.preventDefault();
          if (target) onNavigate(id);
          else onCreateFromRef(id, caption);
        };
        if (!target) {
          return (
            <a
              href="#"
              onClick={handleClick}
              className="cursor-pointer text-red-700 decoration-dotted"
              title={t('wikilinkBrokenHint')}
            >
              {caption}
              <span className="ml-1 text-[10px] text-red-500">
                {t('wikilinkBroken')}
              </span>
            </a>
          );
        }
        return (
          <a
            href="#"
            onClick={handleClick}
            className="cursor-pointer text-blue-600 underline decoration-blue-300 hover:decoration-blue-600"
          >
            {caption}
          </a>
        );
      },
      // Allow raw HTML in markdown (default react-markdown strips it; we keep GFM only)
    } satisfies Components;
  }, [resolveTarget, onNavigate, onCreateFromRef, t]);

  const handleError = useCallback((e: { error: Error }) => {
    console.error('MarkdownView render error:', e.error);
    setRenderError(true);
  }, []);

  if (renderError) {
    return (
      <pre className="max-h-[60vh] overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800">
        {source}
      </pre>
    );
  }

  try {
    return (
      <div className="prose prose-sm max-w-none break-words text-gray-800">
        <ReactMarkdown
          remarkPlugins={[remarkGfm, remarkWikilink]}
          components={components}
        >
          {source}
        </ReactMarkdown>
      </div>
    );
  } catch (e) {
    // Should be caught by ErrorBoundary in react-markdown 9.x, but just in case.
    return (
      <pre className="max-h-[60vh] overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800">
        {source}
      </pre>
    );
  }
}
```

注意：上面引入了 `useEffect` 和 `useState`，需要在文件顶部加：

```tsx
import { useEffect, useMemo, useState, useCallback } from 'react';
```

替换原 import 行：

```tsx
import { useMemo, useCallback } from 'react';
```

替换为：

```tsx
import { useEffect, useMemo, useState, useCallback } from 'react';
```

- [ ] **Step 2: 类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过（react-markdown 类型定义覆盖 components prop）

- [ ] **Step 3: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add src/markdown/MarkdownView.tsx && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(markdown): MarkdownView with wikilink handler + error fallback"
```

---

## Task 11: DetailPanel 集成 MarkdownView + 原文切换

**Files:**
- Modify: `src/views/DetailPanel.tsx`

- [ ] **Step 1: 添加 import**

在 `/Users/zhumingchen/repository-manage/src/views/DetailPanel.tsx` 顶部（约第 1-7 行 import 区域）追加：

```tsx
import { MarkdownView } from '@/markdown/MarkdownView';
```

- [ ] **Step 2: 添加 view 状态**

在 DetailPanel 函数体内（约第 117 行 `useState<{ markdown: string | null; error: string | null }>(...)` 之后）插入：

```tsx
  const [viewMode, setViewMode] = useState<'rendered' | 'source'>('rendered');
```

注意：viewMode 在 `selectedEntryId` 变化时重置。在 `useEffect` 内（约第 149-152 行）的 `if (!selectedEntryId)` 分支里，添加：

```tsx
        setViewMode('rendered');
```

完整修改后该 useEffect 应为：

```tsx
  useEffect(() => {
    if (!selectedEntryId) {
      setContent({ markdown: null, error: null });
      setExtLinks([]);
      setViewMode('rendered');
      return;
    }
    let cancelled = false;
    setContent({ markdown: null, error: null });
    setExtLinks([]);
    setLinkMsg({});
    setViewMode('rendered');
    // ...
```

- [ ] **Step 3: 实现 wikilink 跳转 / 创建回调**

在 DetailPanel 函数体内（约第 122-130 行的 state 声明区域附近）追加：

```tsx
  const handleWikilinkNavigate = (targetId: string) => {
    selectEntry(targetId);
  };

  const handleWikilinkCreate = (targetId: string, caption: string) => {
    if (!entry) return;
    setCreateDialog({
      open: true,
      linkIndex: -1,
      refText: caption || targetId,
      sourceUrl: targetId,
    });
  };

  const resolveWikilinkTarget = (target: string) => {
    if (!entry) return null;
    const found = resolveInternalLink(target, entry, entries, kbRoot);
    return found ? { id: found.id, title: found.title } : null;
  };
```

注：`handleWikilinkCreate` 复用了已有的 `createDialog` 流程；该 dialog 接受 `linkIndex/refText/sourceUrl`，所以 `linkIndex: -1` 表示非 ext link 路径。检查 `CreateEntryDialog` 是否能处理 `linkIndex=-1` —— 若是 `<View Label="new path">`，需要扩展以不依赖 linkIndex。当前实现 (src/views/DetailPanel.tsx:215-249) 中 `handleCreateEntry` 调用 `createEntryFromReference(entry.id, parentGroup, name, createDialog.sourceUrl, createDialog.refText)`，不依赖 `linkIndex` 用于行为分支。OK。

- [ ] **Step 4: 替换 markdown 内容渲染**

找到 src/views/DetailPanel.tsx 第 309-321 行（即 `Markdown content` section）：

当前：
```tsx
          {/* Markdown content */}
          <div className="border-b border-gray-200 p-4">
            <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-gray-500">
              {t('contentHeader')}
            </h3>
            {content.markdown != null ? (
              <pre className="max-h-[60vh] overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800">
                {content.markdown}
              </pre>
            ) : (
              <p className="text-sm text-gray-400">{content.error ?? t('loadingContent')}</p>
            )}
          </div>
```

替换为：

```tsx
          {/* Markdown content */}
          <div className="border-b border-gray-200 p-4">
            <div className="mb-2 flex items-center justify-between">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-gray-500">
                {t('contentHeader')}
              </h3>
              {content.markdown != null && (
                <div className="flex gap-1 rounded-md border border-gray-200 bg-white p-0.5 text-[11px]">
                  <button
                    onClick={() => setViewMode('rendered')}
                    className={`rounded px-2 py-0.5 ${
                      viewMode === 'rendered'
                        ? 'bg-blue-100 text-blue-700'
                        : 'text-gray-500 hover:bg-gray-50'
                    }`}
                  >
                    {t('viewToggleRender')}
                  </button>
                  <button
                    onClick={() => setViewMode('source')}
                    className={`rounded px-2 py-0.5 ${
                      viewMode === 'source'
                        ? 'bg-blue-100 text-blue-700'
                        : 'text-gray-500 hover:bg-gray-50'
                    }`}
                  >
                    {t('viewToggleSource')}
                  </button>
                </div>
              )}
            </div>
            {content.markdown != null ? (
              viewMode === 'rendered' ? (
                <MarkdownView
                  source={content.markdown}
                  resolveTarget={resolveWikilinkTarget}
                  onNavigate={handleWikilinkNavigate}
                  onCreateFromRef={handleWikilinkCreate}
                />
              ) : (
                <pre className="max-h-[60vh] overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800">
                  {content.markdown}
                </pre>
              )
            ) : (
              <p className="text-sm text-gray-400">{content.error ?? t('loadingContent')}</p>
            )}
          </div>
```

- [ ] **Step 5: 类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过（特别注意 `MarkdownView` 和 `DetailPanel` 的 props 类型一致；`entry!` 非空断言是因为 useEffect 已在上方用 entry 兜底，但实际 useEffect 在 entry 之前引用——若 TS 报 entry 可能为 null，请把 resolveWikilinkTarget 移到 entry 解析之后的位置）

- [ ] **Step 6: Commit**

```bash
cd /Users/zhumingchen/repository-manage && git add src/views/DetailPanel.tsx && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(markdown): DetailPanel renders markdown with view toggle + wikilink navigation"
```

---

## Task 12: 全量验收

**Files:** 无修改

- [ ] **Step 1: 完整构建**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过，无任何 TS 报错

- [ ] **Step 2: 启动 Tauri dev 模式**

```bash
cd /Users/zhumingchen/repository-manage && npm run tauri:dev
```

等待 dev server 启动（首次会编译 Rust 后端，约 30s+）。

- [ ] **Step 3: 验收 - 搜索功能**

按 spec §6.1 的 10 项逐项手动验证：
1. 在 Sidebar 顶部搜索框输入 "react" → 显示前 10 条结果
2. 结果按 score 排序（title 命中靠前）
3. 多关键词 AND 验证：输入 "react hook" → 必须两个词都存在的条目才显示
4. snippet 验证：在含较长 content 的 KB 中搜索，结果应展示前后省略号 + 匹配词高亮（黄底）
5. 点击结果项 → 跳转 + 打开 detail panel
6. `⌘K` / `Ctrl+K` 唤起搜索框
7. KB 切换后旧 KB 的搜索结果清空
8. 单个 entry content 不可读（人为 chmod 000 一个 .md）→ 不阻塞其他搜索结果

- [ ] **Step 4: 验收 - Markdown 渲染**

按 spec §6.2 的 8 项逐项手动验证：
1. heading / list / code / bold / italic / link 都正确渲染
2. GFM 表格与 task list 支持
3. `[[react-hooks]]` 渲染为蓝色下划线链接，点击 → 跳转
4. `[[state-management|状态管理]]` 渲染为显示 "状态管理" 的链接，点击 → 跳转
5. 点击不存在的 wikilink → 弹 CreateEntryDialog
6. code block 内的 `[[xxx]]` 不被识别（保留原文）
7. 原文/渲染切换双向工作
8. 制造一个会触发渲染失败的内容（比如在 wikilink 节点手动注入畸形 md）→ 降级到 `<pre>`，UI 不破

- [ ] **Step 5: i18n 完整性验证**

切换语言：zh ↔ en。所有新增的 UI 字符串（placeholder, 切换按钮, search 结果计数, noResults 等）必须双语齐全，无 key 直接暴露。

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

- [ ] **Step 6: 关闭 dev server**

在启动 dev server 的终端按 Ctrl+C。

- [ ] **Step 7: 最终 commit**

若有未改的改动（验收过程中临时调试产生），先 `git status` 检查，确认无遗留调试代码后再 commit。若一切干净则无需 commit。

```bash
cd /Users/zhumingchen/repository-manage && git status
```

Expected: 干净工作区（仅 spec / plan 文档、已 commit 的源代码）。若有任何未提交改动：

```bash
cd /Users/zhumingchen/repository-manage && git add -A && git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "chore: cleanup after verification"
```

---

## Self-Review Checklist

### Spec 覆盖
- ✅ §2.1 5 字段加权 — Task 5（FIELD_WEIGHTS）
- ✅ §2.2 Snippet 截取 — Task 4（snippetAround）
- ✅ §2.3 LRU cache — Task 5（createContentCache）
- ✅ §2.4 debounce + abort — Task 6（useSearch）
- ✅ §2.5 边界与降级 — Task 5（fieldScore = 0 跳过） + Task 6（catch）
- ✅ §2.6 ⌘K 全局快捷键 — Task 7（document keydown listener）
- ✅ §3.1 react-markdown + remark-gfm — Task 1 + Task 10
- ✅ §3.2 wikilink 解析规则 — Task 9（remarkWikilink 插件）
- ✅ §3.3 wikilink 渲染输出 — Task 10（components.wikilink）
- ✅ §3.4 错误回退 — Task 10（renderError state + try/catch）
- ✅ §3.5 原文 / 渲染切换 — Task 11（viewMode state + segmented control）
- ✅ §3.6 wikilink 跳转边界 — Task 11（onCreateFromRef 处理 missing target）
- ✅ §4 数据流 — 由 Task 5/6/10/11 共同实现
- ✅ §5 i18n 键约定 — Task 2（4 个 JSON 文件）+ Task 3（resources.ts 注册）
- ✅ §6 验收标准 — Task 12
- ✅ §7 决策 — 已在 spec 中记录，不需 plan 期间再决策

### Placeholder scan
- 无 TODO / TBD / "类似 Task N" 描述

### Type 一致性
- `SearchHit`（Task 5）→ `useSearch`（Task 6）→ `SearchBox`（Task 7）— 三处定义一致
- `ContentCache`（Task 5）→ `useSearch`（Task 6）— 一致
- `MarkdownView` props（Task 10）→ DetailPanel 调用（Task 11）— props 类型、字段名一致
- `wikilink` 节点结构 `{ type, data: { id, caption }, children: [{ type: 'text', value: caption }] }` — Task 9 输出与 Task 10 输入一致

### 文件边界
- `src/search/*` 5 个文件 — 单一职责（index 算法 / useSearch hook / highlight 工具 / SearchBox UI / i18n）
- `src/markdown/*` 3 个文件 — 单一职责（插件 / 容器 / i18n）
- 集成改动只在 Sidebar + DetailPanel