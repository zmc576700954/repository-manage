export const LANGUAGES = ['zh', 'en'] as const;
export type Lang = (typeof LANGUAGES)[number];

export function detectBrowserLang(): Lang {
  const raw = typeof navigator !== 'undefined' ? navigator.language : 'en';
  return raw.toLowerCase().startsWith('zh') ? 'zh' : 'en';
}

export function isLang(v: unknown): v is Lang {
  return typeof v === 'string' && (LANGUAGES as readonly string[]).includes(v);
}
