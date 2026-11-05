import { create } from 'zustand';

/** 阅读排版参数。所有值都限制在合理区间，并可通过 `reset` 恢复默认。 */
export interface ReadingPrefs {
  fontSize: number;
  lineHeight: number;
  /** 段间距（em）。 */
  paragraphGap: number;
  /** 最大行宽（ch）；0 表示不限制。 */
  maxWidth: number;
  codeFontSize: number;
  tableStyle: 'bordered' | 'striped';
}

export const DEFAULT_READING_PREFS: ReadingPrefs = {
  fontSize: 15,
  lineHeight: 1.7,
  paragraphGap: 0.7,
  maxWidth: 78,
  codeFontSize: 12.5,
  tableStyle: 'bordered',
};

const STORAGE_KEY = 'synapse.readingPrefs';

function clampNumber(value: unknown, min: number, max: number, fallback: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, value));
}

/** 把任意来源（localStorage / 调用方）的值收敛成合法参数。 */
export function normalizeReadingPrefs(input: Partial<ReadingPrefs> | null | undefined): ReadingPrefs {
  const src = input ?? {};
  return {
    fontSize: clampNumber(src.fontSize, 13, 20, DEFAULT_READING_PREFS.fontSize),
    lineHeight: clampNumber(src.lineHeight, 1.4, 2.2, DEFAULT_READING_PREFS.lineHeight),
    paragraphGap: clampNumber(src.paragraphGap, 0.2, 1.4, DEFAULT_READING_PREFS.paragraphGap),
    maxWidth:
      src.maxWidth === 0
        ? 0
        : clampNumber(src.maxWidth, 50, 110, DEFAULT_READING_PREFS.maxWidth),
    codeFontSize: clampNumber(src.codeFontSize, 11, 16, DEFAULT_READING_PREFS.codeFontSize),
    tableStyle: src.tableStyle === 'striped' ? 'striped' : 'bordered',
  };
}

function loadPrefs(): ReadingPrefs {
  if (typeof localStorage === 'undefined') return DEFAULT_READING_PREFS;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_READING_PREFS;
    return normalizeReadingPrefs(JSON.parse(raw) as Partial<ReadingPrefs>);
  } catch {
    return DEFAULT_READING_PREFS;
  }
}

function persistPrefs(prefs: ReadingPrefs): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // localStorage 不可写时仅本次会话生效。
  }
}

interface ReadingPrefsState {
  prefs: ReadingPrefs;
  update: (patch: Partial<ReadingPrefs>) => void;
  reset: () => void;
}

export const useReadingPrefs = create<ReadingPrefsState>((set) => ({
  prefs: loadPrefs(),
  update: (patch) =>
    set((state) => {
      const next = normalizeReadingPrefs({ ...state.prefs, ...patch });
      persistPrefs(next);
      return { prefs: next };
    }),
  reset: () =>
    set(() => {
      persistPrefs(DEFAULT_READING_PREFS);
      return { prefs: DEFAULT_READING_PREFS };
    }),
}));

export const READING_PREF_STORAGE_KEY = STORAGE_KEY;
