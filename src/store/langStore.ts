import { Store } from '@tauri-apps/plugin-store';

const STORE_FILE = 'preferences.json';
const LANG_KEY = 'language';
const VALID = new Set(['zh', 'en']);

let cache: Store | null = null;

async function getStore(): Promise<Store> {
  if (!cache) cache = await Store.load(STORE_FILE);
  return cache;
}

export async function getLang(): Promise<'zh' | 'en' | null> {
  try {
    const s = await getStore();
    const v = await s.get<string>(LANG_KEY);
    if (v && (VALID as Set<string>).has(v)) return v as 'zh' | 'en';
    return null;
  } catch {
    return null;
  }
}

export async function setLang(lang: 'zh' | 'en'): Promise<void> {
  const s = await getStore();
  await s.set(LANG_KEY, lang);
  await s.save();
}
