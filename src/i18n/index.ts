import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import { detectBrowserLang, isLang } from './languages';
import { resources } from './resources';
import { getLang, setLang } from '@/store/langStore';

const FALLBACK: 'zh' | 'en' = 'en';

async function resolveInitialLang(): Promise<'zh' | 'en'> {
  const stored = await getLang();
  if (isLang(stored)) return stored;
  return detectBrowserLang();
}

export async function initI18n(): Promise<void> {
  try {
    const lng = await resolveInitialLang();
    await i18n.use(initReactI18next).init({
      resources,
      lng,
      fallbackLng: FALLBACK,
      ns: Object.keys(resources.en),
      defaultNS: 'App',
      interpolation: { escapeValue: false },
      returnNull: false,
    });
  } catch (e) {
    console.warn('[i18n] init failed, falling back to', FALLBACK, e);
    await i18n.use(initReactI18next).init({
      resources,
      lng: FALLBACK,
      fallbackLng: FALLBACK,
      ns: Object.keys(resources.en),
      defaultNS: 'App',
      interpolation: { escapeValue: false },
      returnNull: false,
    });
  }
}

export async function changeLanguage(lang: 'zh' | 'en'): Promise<void> {
  await i18n.changeLanguage(lang);
  try {
    await setLang(lang);
  } catch (e) {
    console.warn('[i18n] failed to persist language', e);
  }
}

export { i18n };
