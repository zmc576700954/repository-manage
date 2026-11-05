import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { Sidebar } from '@/components/Sidebar';
import { ErrorBar } from '@/components/ErrorBar';
import { FixDrawer } from '@/components/FixDrawer';
import { MapView } from '@/views/MapView';
import { useAppStore } from '@/store/useAppStore';
import { usePanelStore } from '@/store/panelStore';
import { RightPanelStack } from '@/components/RightPanelStack';
import {
  createKbRefreshCoordinator,
  type KbChangedPayload,
} from '@/events/kbRefresh';

export default function App() {
  const { t } = useTranslation('App');
  const loadEntries = useAppStore((s) => s.loadEntries);
  const refreshKbs = useAppStore((s) => s.refreshKbs);
  const error = useAppStore((s) => s.error);
  const isLoading = useAppStore((s) => s.isLoading);
  const activeKbId = useAppStore((s) => s.activeKbId);
  const entries = useAppStore((s) => s.entries);
  const activeKbRoot = useAppStore((s) =>
    s.kbs.find((k) => k.id === s.activeKbId)?.path ?? null
  );

  useEffect(() => {
    refreshKbs().then(() => loadEntries());
  }, [refreshKbs, loadEntries]);

  // 外部（watcher）重建完成后自动刷新图谱：防抖合并、避开自身写操作回声，
  // 重命名/拖拽进行中延迟刷新。事件的产生时机见 lib.rs 的 emit 逻辑。
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    const coordinator = createKbRefreshCoordinator({
      getActiveKbId: () => useAppStore.getState().activeKbId,
      isBusy: () => {
        const state = useAppStore.getState();
        return (
          state.renamingEntryId !== null ||
          state.interactionBusy ||
          state.isDraggingNode
        );
      },
      apply: (payload) => {
        void useAppStore.getState().applyKbChanged(payload);
      },
    });

    void listen<KbChangedPayload>('kb-changed', (event) => {
      coordinator.handle(event.payload);
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // 非 Tauri 环境（例如纯浏览器预览）没有事件系统，忽略即可。
      });

    return () => {
      disposed = true;
      coordinator.dispose();
      unlisten?.();
    };
  }, []);

  const errorCount = entries.reduce((sum, e) => sum + (e.errorCount ?? 0), 0);
  const warningCount = entries.reduce((sum, e) => sum + (e.warningCount ?? 0), 0);

  const openSettings = () => usePanelStore.getState().open('settings');

  return (
    <div className="flex h-screen w-screen bg-gray-50 text-gray-900">
      <Sidebar />
      <main className="relative flex-1">
        <ErrorBar errorCount={errorCount} warningCount={warningCount} />
        {error && (
          <div className="absolute left-4 right-4 top-14 z-10 rounded-md bg-red-50 p-3 text-sm text-red-700 shadow-sm">
            {error}
          </div>
        )}
        {isLoading ? (
          <div className="flex h-full items-center justify-center text-sm text-gray-500">
            {t('loading')}
          </div>
        ) : activeKbId ? (
          <MapView />
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-3 text-sm text-gray-500">
            <p>{t('noActiveKb')}</p>
            <button
              onClick={openSettings}
              className="rounded-md bg-blue-500 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-600"
            >
              {t('openKbAction')}
            </button>
          </div>
        )}
      </main>
      <RightPanelStack />
      <FixDrawer activeKbRoot={activeKbRoot} />
    </div>
  );
}
