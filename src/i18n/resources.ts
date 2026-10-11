import AppZh from '../App.zh.json';
import AppEn from '../App.en.json';
import SettingsZh from '../views/Settings.zh.json';
import SettingsEn from '../views/Settings.en.json';

export const NS = ['App', 'Settings'] as const;
export type Namespace = (typeof NS)[number];

export const resources = {
  zh: { App: AppZh, Settings: SettingsZh },
  en: { App: AppEn, Settings: SettingsEn },
} as const;
