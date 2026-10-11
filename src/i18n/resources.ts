import AppZh from '../App.zh.json';
import AppEn from '../App.en.json';
import SettingsZh from '../views/Settings.zh.json';
import SettingsEn from '../views/Settings.en.json';
import SidebarZh from '../components/Sidebar.zh.json';
import SidebarEn from '../components/Sidebar.en.json';
import NodeCardZh from '../components/NodeCard.zh.json';
import NodeCardEn from '../components/NodeCard.en.json';
import RelationDialogZh from '../components/RelationDialog.zh.json';
import RelationDialogEn from '../components/RelationDialog.en.json';
import MapViewZh from '../views/MapView.zh.json';
import MapViewEn from '../views/MapView.en.json';
import EntryDialogZh from '../components/EntryDialog.zh.json';
import EntryDialogEn from '../components/EntryDialog.en.json';
import DetailPanelZh from '../views/DetailPanel.zh.json';
import DetailPanelEn from '../views/DetailPanel.en.json';

export const NS = ['App', 'Settings', 'Sidebar', 'NodeCard', 'RelationDialog', 'MapView', 'EntryDialog', 'DetailPanel'] as const;
export type Namespace = (typeof NS)[number];

export const resources = {
  zh: { App: AppZh, Settings: SettingsZh, Sidebar: SidebarZh, NodeCard: NodeCardZh, RelationDialog: RelationDialogZh, MapView: MapViewZh, EntryDialog: EntryDialogZh, DetailPanel: DetailPanelZh },
  en: { App: AppEn, Settings: SettingsEn, Sidebar: SidebarEn, NodeCard: NodeCardEn, RelationDialog: RelationDialogEn, MapView: MapViewEn, EntryDialog: EntryDialogEn, DetailPanel: DetailPanelEn },
} as const;
