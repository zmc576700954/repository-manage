import AppZh from '../App.zh.json';
import AppEn from '../App.en.json';

export const NS = ['App'] as const;
export type Namespace = (typeof NS)[number];

export const resources = {
  zh: { App: AppZh },
  en: { App: AppEn },
} as const;
