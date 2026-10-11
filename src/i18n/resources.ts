import AppZh from '../App.zh.json';
import AppEn from '../App.en.json';
import SettingsZh from '../views/Settings.zh.json';
import SettingsEn from '../views/Settings.en.json';
import SidebarZh from '../components/Sidebar.zh.json';
import SidebarEn from '../components/Sidebar.en.json';
import NodeCardZh from '../components/NodeCard.zh.json';
import NodeCardEn from '../components/NodeCard.en.json';

export const NS = ['App', 'Settings', 'Sidebar', 'NodeCard'] as const;
export type Namespace = (typeof NS)[number];

export const resources = {
  zh: { App: AppZh, Settings: SettingsZh, Sidebar: SidebarZh, NodeCard: NodeCardZh },
  en: { App: AppEn, Settings: SettingsEn, Sidebar: SidebarEn, NodeCard: NodeCardEn },
} as const;
