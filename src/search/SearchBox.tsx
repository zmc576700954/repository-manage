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
