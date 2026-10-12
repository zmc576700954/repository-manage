# UI 布局重构 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Synapse KB 桌面应用从 4 内容块挤压布局改为 3 内容块 + 右侧单列多面板上下排列 + 右上角错误 badge 的 ClaudeCode APP 风格布局。

**Architecture:** 自底向上分 5 层 —— (1) 面板状态机扩展（FIFO + 'detail' 类型），(2) RightPanel / RightPanelStack 容器渲染新类型，(3) i18n 字符串补全，(4) 调用方集成（Sidebar / MapView / useAppStore），(5) App.tsx 顶层布局整合 + ErrorBar 改 ErrorBadge。每层完成后跑 `npm run build` 校验 TS。

**Tech Stack:**
- **前端**：React 18 + Vite + TypeScript + Zustand + Tailwind + i18next
- **状态**：zustand store（panelStore, useAppStore）
- **验证**：`npm run build`（TS 严格类型检查 + Vite 构建）

---

## 文件结构

### 修改

| 路径 | 改动 |
|---|---|
| `src/store/panelStore.ts` | PanelType 加 `'detail'`；新增 `MAX_PANELS = 3`；`open()` 加 FIFO evict 逻辑；新增 `handleEviction` + `closeAll` |
| `src/components/RightPanelStack.tsx` | `renderPanel` switch 加 `'detail'` case |
| `src/components/RightPanel.tsx` | Props 加 `type: PanelType`；容器加 `min-h-[140px]` + `flex-1`；标题改 i18n；外层加圆角阴影 |
| `src/components/ErrorBar.tsx` | 改名为 `ErrorBadge`（或保留文件名仅改内容）；从 div 横条改为右上角浮动 button |
| `src/views/DetailPanel.tsx` | 移除 `<aside>` 外壳（容器由 RightPanel 提供）；`selectEntry(null)` 同步关闭 detail 面板 |
| `src/views/MapView.tsx` | `onNodeClick` 增加 `openPanel('detail')` |
| `src/components/Sidebar.tsx` | `TreeRow` onSelect 改为 `handleSelect`（selectEntry + openPanel） |
| `src/store/useAppStore.ts` | `setActiveKb` / `removeKb` 同步调用 `usePanelStore.getState().close('detail')` |
| `src/App.tsx` | 移除 `ErrorBar` 改 `ErrorBadge`；移除 `openKbButton` 按钮；移除 `{selectedEntryId && <DetailPanel />}` 条件渲染 |
| `src/i18n/resources.ts` | 注册 `RightPanel` 命名空间 |
| `src/components/ErrorBar.zh.json` | 删除 `errors` / `warnings` / `clickToView`；新增 `tooltip` |
| `src/components/ErrorBar.en.json` | 同上（英文版） |
| `src/App.zh.json` | 删除 `openKbButton` 键 |
| `src/App.en.json` | 同上（英文版） |

### 新建

| 路径 | 职责 |
|---|---|
| `src/components/RightPanel.zh.json` | 面板标题中文 i18n |
| `src/components/RightPanel.en.json` | 面板标题英文 i18n |

---

## Milestone 1: 面板状态机扩展

### Task 1: 扩展 PanelType 与 FIFO open/close

**Files:**
- Modify: `src/store/panelStore.ts:1-35`（全文件）

- [ ] **Step 1: 重写 panelStore.ts**

完整替换为：

```typescript
import { create } from 'zustand';
import { useAppStore } from './useAppStore';

export type PanelType = 'detail' | 'validation' | 'settings' | 'filter';

const MAX_PANELS = 3;

interface PanelState {
  openPanels: PanelType[];
  open: (panel: PanelType) => void;
  close: (panel: PanelType) => void;
  toggle: (panel: PanelType) => void;
  closeAll: () => void;
  width: number;
  setWidth: (w: number) => void;
}

function handleEviction(evicted: PanelType): void {
  if (evicted === 'detail') {
    useAppStore.getState().selectEntry(null);
  }
}

export const usePanelStore = create<PanelState>((set, get) => ({
  openPanels: [],
  width: 360,
  setWidth: (w) => set({ width: Math.max(280, Math.min(800, w)) }),

  open: (panel) => {
    const { openPanels } = get();
    if (openPanels.includes(panel)) return;
    if (openPanels.length >= MAX_PANELS) {
      const evicted = openPanels[0];
      handleEviction(evicted);
      set({ openPanels: [...openPanels.slice(1), panel] });
    } else {
      set({ openPanels: [...openPanels, panel] });
    }
  },

  close: (panel) =>
    set((s) => ({ openPanels: s.openPanels.filter((p) => p !== panel) })),

  toggle: (panel) => {
    const { openPanels, open, close } = get();
    if (openPanels.includes(panel)) close(panel);
    else open(panel);
  },

  closeAll: () => set({ openPanels: [] }),
}));
```

- [ ] **Step 2: 运行 build 校验**

Run: `npm run build`
Expected: PASS（TS 类型检查通过；`useAppStore` 已存在并导出 `selectEntry`）

- [ ] **Step 3: 提交**

```bash
git add src/store/panelStore.ts
git commit -m "feat(panelStore): add 'detail' panel type with FIFO eviction (max 3)"
```

---

## Milestone 2: RightPanel 容器与 RightPanelStack 渲染

### Task 2: 创建 RightPanel i18n 命名空间

**Files:**
- Create: `src/components/RightPanel.zh.json`
- Create: `src/components/RightPanel.en.json`
- Modify: `src/i18n/resources.ts`

- [ ] **Step 1: 写 RightPanel.zh.json**

```json
{
  "titleDetail": "详情",
  "titleValidation": "校验",
  "titleSettings": "设置",
  "titleFilter": "筛选"
}
```

- [ ] **Step 2: 写 RightPanel.en.json**

```json
{
  "titleDetail": "Detail",
  "titleValidation": "Validation",
  "titleSettings": "Settings",
  "titleFilter": "Filter"
}
```

- [ ] **Step 3: 在 resources.ts 注册命名空间**

读 `src/i18n/resources.ts` 找到 `NS` 类型和两个资源对象（`zh` / `en`），添加 `RightPanel`：

```typescript
// NS 类型联合中加 RightPanel
export type NS =
  | 'App'
  | 'Sidebar'
  | 'NodeCard'
  | 'RelationDialog'
  | 'MapView'
  | 'EntryDialog'
  | 'DetailPanel'
  | 'Settings'
  | 'ErrorBar'
  | 'ValidationPanel'
  | 'FilterPanel'
  | 'RightPanel';  // 新增

// zh 资源对象中加：
RightPanel: RightPanel_zh,

// en 资源对象中加：
RightPanel: RightPanel_en,
```

import 语句：
```typescript
import RightPanel_zh from '@/components/RightPanel.zh.json';
import RightPanel_en from '@/components/RightPanel.en.json';
```

- [ ] **Step 4: 运行 build 校验**

Run: `npm run build`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add src/components/RightPanel.zh.json src/components/RightPanel.en.json src/i18n/resources.ts
git commit -m "feat(i18n): add RightPanel namespace (panel titles)"
```

---

### Task 3: 重写 RightPanel 容器（受控高度 + i18n 标题）

**Files:**
- Modify: `src/components/RightPanel.tsx:1-24`（全文件）

- [ ] **Step 1: 重写 RightPanel.tsx**

完整替换为：

```typescript
import React from 'react';
import { useTranslation } from 'react-i18next';
import type { PanelType } from '@/store/panelStore';

interface Props {
  type: PanelType;
  onClose: () => void;
  children: React.ReactNode;
}

const TITLE_KEY: Record<PanelType, string> = {
  detail: 'titleDetail',
  validation: 'titleValidation',
  settings: 'titleSettings',
  filter: 'titleFilter',
};

export const RightPanel: React.FC<Props> = ({ type, onClose, children }) => {
  const { t } = useTranslation('RightPanel');
  return (
    <div className="flex min-h-[140px] flex-1 flex-col overflow-hidden rounded-md border border-gray-200 bg-white shadow-sm">
      <div className="flex items-center justify-between border-b border-gray-200 px-3 py-2">
        <h3 className="text-sm font-medium text-gray-900">{t(TITLE_KEY[type])}</h3>
        <button
          onClick={onClose}
          className="text-gray-400 hover:text-gray-600"
          aria-label="Close"
        >
          ✕
        </button>
      </div>
      <div className="flex-1 overflow-y-auto">{children}</div>
    </div>
  );
};
```

- [ ] **Step 2: 运行 build 校验**

Run: `npm run build`
Expected: PASS

- [ ] **Step 3: 提交**

```bash
git add src/components/RightPanel.tsx
git commit -m "feat(RightPanel): controlled min-h + flex-1 + i18n title"
```

---

### Task 4: RightPanelStack 渲染 'detail' 类型

**Files:**
- Modify: `src/components/RightPanelStack.tsx:1-36`（全文件）

- [ ] **Step 1: 重写 RightPanelStack.tsx**

完整替换为：

```typescript
import React from 'react';
import { usePanelStore, PanelType } from '@/store/panelStore';
import { RightPanel } from './RightPanel';
import { ValidationPanel } from './ValidationPanel';
import { Settings } from '@/views/Settings';
import { DetailPanel } from '@/views/DetailPanel';

export const RightPanelStack: React.FC = () => {
  const { openPanels, close, width } = usePanelStore();

  if (openPanels.length === 0) return null;

  return (
    <div
      className="flex h-full flex-col gap-2 border-l border-gray-200 bg-gray-50 p-2"
      style={{ width }}
    >
      {openPanels.map((panelType) => (
        <RightPanel
          key={panelType}
          type={panelType}
          onClose={() => close(panelType)}
        >
          <PanelContent type={panelType} />
        </RightPanel>
      ))}
    </div>
  );
};

const PanelContent: React.FC<{ type: PanelType }> = ({ type }) => {
  switch (type) {
    case 'detail':
      return <DetailPanel />;
    case 'validation':
      return <ValidationPanel />;
    case 'settings':
      return <Settings />;
    case 'filter':
      return <div className="p-4 text-sm text-gray-500">Filter placeholder</div>;
  }
};
```

- [ ] **Step 2: 运行 build 校验**

Run: `npm run build`
Expected: 可能 FAIL —— `DetailPanel` 仍带 `<aside>` 外壳，下一 Task 移除

如失败，记录警告但继续（不阻塞）；下一个 Task 修。

- [ ] **Step 3: 提交（仅当 build 通过；否则等下一 Task 后一起提交）**

```bash
git add src/components/RightPanelStack.tsx
git commit -m "feat(RightPanelStack): render 'detail' case via PanelContent switch"
```

---

## Milestone 3: DetailPanel 适配新容器

### Task 5: DetailPanel 移除 aside 外壳 + 关闭联动

**Files:**
- Modify: `src/views/DetailPanel.tsx:249-281`（外层 aside + 关闭按钮）

- [ ] **Step 1: 替换 aside 外壳**

读 `src/views/DetailPanel.tsx` 找到外层：

```typescript
return (
  <>
    <aside className="flex h-full w-[28rem] max-w-[40vw] flex-col border-l border-gray-200 bg-white">
```

替换为：

```typescript
import { usePanelStore } from '@/store/panelStore';

export function DetailPanel() {
  // ... 现有 imports / hooks ...
  const closePanel = usePanelStore((s) => s.close);
  // ... 在 entry 存在逻辑之后 ...

  return (
    <div className="flex h-full flex-col">
```

把 `aside` 改为 `div`，并删除 `w-[28rem] max-w-[40vw] border-l border-gray-200 bg-white` 这些由 RightPanel 提供的样式。

- [ ] **Step 2: 修改 ✕ 关闭按钮**

找到：
```typescript
<button
  onClick={() => selectEntry(null)}
  className="shrink-0 rounded-md px-2 py-1 text-sm text-gray-500 hover:bg-gray-100"
  aria-label={t('closeAria')}
>
  ✕
</button>
```

替换为：
```typescript
<button
  onClick={() => {
    selectEntry(null);
    closePanel('detail');
  }}
  className="shrink-0 rounded-md px-2 py-1 text-sm text-gray-500 hover:bg-gray-100"
  aria-label={t('closeAria')}
>
  ✕
</button>
```

对文件底部 "close footer" 链接同样修改：
```typescript
<button
  onClick={() => {
    selectEntry(null);
    closePanel('detail');
  }}
  className="mt-3 text-blue-600 hover:underline"
>
  {t('closeFooter')}
</button>
```

- [ ] **Step 3: 运行 build 校验**

Run: `npm run build`
Expected: PASS

- [ ] **Step 4: 提交**

```bash
git add src/views/DetailPanel.tsx
git commit -m "refactor(DetailPanel): drop <aside> wrapper, sync close to panelStore"
```

---

## Milestone 4: 调用方集成

### Task 6: MapView 点击节点打开 detail 面板

**Files:**
- Modify: `src/views/MapView.tsx:161-173`（onNodesChange + onNodeClick）

- [ ] **Step 1: 修改 onNodeClick**

找到：
```typescript
const onNodeClick = useCallback(
  (_: React.MouseEvent, node: Node) => {
    selectEntry(node.id);
  },
  [selectEntry]
);
```

替换为：
```typescript
import { usePanelStore } from '@/store/panelStore';

// 在 MapView 函数体内：
const openPanel = usePanelStore((s) => s.open);

const onNodeClick = useCallback(
  (_: React.MouseEvent, node: Node) => {
    selectEntry(node.id);
    openPanel('detail');
  },
  [selectEntry, openPanel]
);
```

- [ ] **Step 2: 运行 build 校验**

Run: `npm run build`
Expected: PASS

- [ ] **Step 3: 提交**

```bash
git add src/views/MapView.tsx
git commit -m "feat(MapView): open detail panel on node click"
```

---

### Task 7: Sidebar 点击条目打开 detail 面板

**Files:**
- Modify: `src/components/Sidebar.tsx:30-138`（Sidebar + TreeRow）

- [ ] **Step 1: 修改 Sidebar 顶层 onSelect**

读 Sidebar.tsx 找到：
```typescript
const selectEntry = useAppStore((s) => s.selectEntry);
const { open } = usePanelStore();
```

替换为：
```typescript
const selectEntry = useAppStore((s) => s.selectEntry);
const openPanel = usePanelStore((s) => s.open);

const handleSelect = (id: string) => {
  selectEntry(id);
  openPanel('detail');
};
```

- [ ] **Step 2: 修改 TreeRow 调用处**

找到：
```typescript
<TreeRow
  ...
  onSelect={selectEntry}
  ...
/>
```

替换为：
```typescript
<TreeRow
  ...
  onSelect={handleSelect}
  ...
/>
```

- [ ] **Step 3: 运行 build 校验**

Run: `npm run build`
Expected: PASS

- [ ] **Step 4: 提交**

```bash
git add src/components/Sidebar.tsx
git commit -m "feat(Sidebar): open detail panel on tree entry click"
```

---

### Task 8: useAppStore KB 切换/删除同步关闭 detail

**Files:**
- Modify: `src/store/useAppStore.ts:202-240`（removeKb / setActiveKb）

- [ ] **Step 1: 在文件顶部加 import**

找到：
```typescript
import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { Entry, KBInfo, RelationType } from '@shared/types';
import { localizeError } from '@/i18n/translateError';
```

在末尾加：
```typescript
import { usePanelStore } from './panelStore';
```

- [ ] **Step 2: 修改 removeKb**

找到：
```typescript
removeKb: async (id) => {
  try {
    await invoke('remove_kb', { id });
    // The active KB may have just been cleared (Finding #15 makes
    // remove of the active KB → "no active KB"). Drop any selection
    // that belonged to the removed KB so the user doesn't see a
    // dangling reference in the detail panel.
    set({ selectedEntryId: null });
    await Promise.all([get().refreshKbs(), get().loadEntries()]);
  } catch (e) {
    set({ error: errMsg(e) });
  }
},
```

替换为：
```typescript
removeKb: async (id) => {
  try {
    await invoke('remove_kb', { id });
    set({ selectedEntryId: null });
    usePanelStore.getState().close('detail');  // 同步关闭 detail 面板
    await Promise.all([get().refreshKbs(), get().loadEntries()]);
  } catch (e) {
    set({ error: errMsg(e) });
  }
},
```

- [ ] **Step 3: 修改 setActiveKb**

找到 setActiveKb 函数末尾 `set({ ... })` 之后：

```typescript
set({
  entries: newEntries,
  activeKbEntryCount: newEntries.length,
  selectedEntryId: stillExists ? prevSelection : null,
});
```

替换为：
```typescript
set({
  entries: newEntries,
  activeKbEntryCount: newEntries.length,
  selectedEntryId: stillExists ? prevSelection : null,
});
if (!stillExists) {
  usePanelStore.getState().close('detail');
}
```

- [ ] **Step 4: 运行 build 校验**

Run: `npm run build`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add src/store/useAppStore.ts
git commit -m "feat(useAppStore): sync-close detail panel on KB switch/remove"
```

---

## Milestone 5: 顶层布局 + ErrorBar → ErrorBadge

### Task 9: ErrorBar i18n 字符串调整

**Files:**
- Modify: `src/components/ErrorBar.zh.json`
- Modify: `src/components/ErrorBar.en.json`

- [ ] **Step 1: 重写 ErrorBar.zh.json**

完整替换为：

```json
{
  "tooltip": "查看 {{errors}} 个错误和 {{warnings}} 个警告"
}
```

- [ ] **Step 2: 重写 ErrorBar.en.json**

完整替换为：

```json
{
  "tooltip": "View {{errors}} errors and {{warnings}} warnings"
}
```

- [ ] **Step 3: 暂不提交**

i18n 字符串更新需要配合下一步组件重写。一起提交。

---

### Task 10: 重写 ErrorBar.tsx 为 ErrorBadge

**Files:**
- Modify: `src/components/ErrorBar.tsx:1-32`（全文件）

- [ ] **Step 1: 重写 ErrorBar.tsx**

完整替换为（保留文件名 `ErrorBar.tsx` 以减小 diff；如要重命名为 `ErrorBadge.tsx` 同时改 App.tsx import）：

```typescript
import React from 'react';
import { useTranslation } from 'react-i18next';
import { usePanelStore } from '@/store/panelStore';

interface Props {
  errorCount: number;
  warningCount: number;
}

export const ErrorBar: React.FC<Props> = ({ errorCount, warningCount }) => {
  const { t } = useTranslation('ErrorBar');
  const open = usePanelStore((s) => s.open);

  if (errorCount === 0 && warningCount === 0) return null;

  const isError = errorCount > 0;
  const colorClass = isError
    ? 'bg-red-100 text-red-800 hover:bg-red-200'
    : 'bg-yellow-100 text-yellow-800 hover:bg-yellow-200';

  return (
    <button
      onClick={() => open('validation')}
      className={`absolute right-4 top-4 z-10 rounded-full px-3 py-1 text-xs shadow-sm transition-colors ${colorClass}`}
      title={t('tooltip', { errors: errorCount, warnings: warningCount })}
    >
      {isError && `⚠ ${errorCount}`}
      {isError && warningCount > 0 && ' · '}
      {!isError && warningCount > 0 && `${warningCount}`}
    </button>
  );
};
```

- [ ] **Step 2: 运行 build 校验**

Run: `npm run build`
Expected: PASS

- [ ] **Step 3: 提交（与上一 Task 合并）**

```bash
git add src/components/ErrorBar.tsx src/components/ErrorBar.zh.json src/components/ErrorBar.en.json
git commit -m "feat(ErrorBar): convert strip to top-right floating badge button"
```

---

### Task 11: App.tsx 整合（移除旧元素）

**Files:**
- Modify: `src/App.tsx:1-69`（全文件）
- Modify: `src/App.zh.json`
- Modify: `src/App.en.json`

- [ ] **Step 1: 重写 App.tsx**

完整替换为：

```typescript
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Sidebar } from '@/components/Sidebar';
import { ErrorBar } from '@/components/ErrorBar';
import { MapView } from '@/views/MapView';
import { useAppStore } from '@/store/useAppStore';
import { usePanelStore } from '@/store/panelStore';
import { RightPanelStack } from '@/components/RightPanelStack';

export default function App() {
  const { t } = useTranslation('App');
  const loadEntries = useAppStore((s) => s.loadEntries);
  const refreshKbs = useAppStore((s) => s.refreshKbs);
  const error = useAppStore((s) => s.error);
  const isLoading = useAppStore((s) => s.isLoading);
  const activeKbId = useAppStore((s) => s.activeKbId);
  const entries = useAppStore((s) => s.entries);

  useEffect(() => {
    refreshKbs().then(() => loadEntries());
  }, [refreshKbs, loadEntries]);

  const errorCount = entries.reduce((sum, e) => sum + (e.errorCount ?? 0), 0);
  const warningCount = entries.reduce((sum, e) => sum + (e.warningCount ?? 0), 0);

  const openSettings = () => usePanelStore.getState().open('settings');

  return (
    <div className="flex h-screen w-screen bg-gray-50 text-gray-900">
      <Sidebar />
      <main className="relative flex-1">
        <ErrorBar errorCount={errorCount} warningCount={warningCount} />
        {error && (
          <div className="absolute left-4 right-4 top-4 z-10 rounded-md bg-red-50 p-3 text-sm text-red-700 shadow-sm">
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
    </div>
  );
}
```

主要变更：
- 删除 `import { DetailPanel }` 与 `{selectedEntryId && <DetailPanel />}` 条件渲染
- 删除 `import { open } from usePanelStore`（改为内联调用）
- 删除 `openSettings = () => open('settings')` 改用 `usePanelStore.getState().open('settings')`
- 删除 `import { RightPanelStack }` 移到上方（已在底部 import；保持现顺序）
- 删除 `openKbButton` 按钮
- ErrorBar 改用新 badge（已转换）

- [ ] **Step 2: 删除 openKbButton 键**

读 `src/App.zh.json` 和 `src/App.en.json`，删除 `openKbButton` 行：

`zh`:
```json
{
  "loading": "加载中…",
  "noActiveKb": "当前没有激活的知识库。",
  "openKbAction": "打开知识库"
}
```

`en`:
```json
{
  "loading": "Loading…",
  "noActiveKb": "No active knowledge base.",
  "openKbAction": "Open knowledge base"
}
```

- [ ] **Step 3: 运行 build 校验**

Run: `npm run build`
Expected: PASS

- [ ] **Step 4: 提交**

```bash
git add src/App.tsx src/App.zh.json src/App.en.json
git commit -m "refactor(App): 3-column layout with badge + merged right column"
```

---

## Milestone 6: 端到端验收

### Task 12: 全链路 build + 手工验收清单

**Files:**
- 无代码变更

- [ ] **Step 1: 最终 build**

Run: `npm run build`
Expected: PASS（无 TS 错误，无 lint 错误）

- [ ] **Step 2: 启动应用**

Run: `npm run tauri:dev`
Expected: 应用启动，Tauri 窗口显示主界面

- [ ] **Step 3: 手工验证清单**

逐项核对（每项 OK 才算通过）：

- [ ] 主界面：左 Sidebar / 中 Map / 右无面板（仅 3 内容块）
- [ ] 点击地图节点 → DetailPanel 出现在右列顶部
- [ ] 点击右上角 ⚠ badge → ValidationPanel 出现在 Detail 下方
- [ ] 同时打开 Settings → 3 个面板共存，按 flex-1 分高度
- [ ] 点击 Filter（占位） → 第 4 个面板打开时，栈底 Validation 自动关闭
- [ ] 点击 ✕ 关闭某个面板 → 剩余面板保持当前高度
- [ ] 切换 KB → detail 面板自动关闭
- [ ] 删除当前 KB → detail 面板自动关闭
- [ ] 主图宽度足够（>1200px）时地图呼吸顺畅

- [ ] **Step 4: 提交验收记录（如有 README 更新）**

如有需要，把验收结果写入 `docs/verification/`：

```bash
# 仅当 docs/verification/ 目录存在且需要更新时执行
ls docs/verification/
# 若有 ui-layout-verification.md 则追加本次结果，否则跳过
```

通常此步骤无需新 commit。如需更新验证记录：

```bash
git add docs/verification/
git commit -m "docs(verification): UI 三栏布局手工验收通过"
```

---

## Self-Review Checklist

实现完成后，确认每条 spec 需求都有对应 task：

- [x] §1 3 内容块布局 → Task 11（App.tsx 整合）
- [x] §2.1 PanelType 加 'detail' → Task 1
- [x] §2.2 FIFO 收回 + MAX_PANELS=3 → Task 1
- [x] §2.4 handleEviction 联动 selectEntry → Task 1
- [x] §3.1 RightPanelStack 渲染 'detail' → Task 4
- [x] §3.2 RightPanel 受控高度 + i18n 标题 → Task 2 + Task 3
- [x] §4 ErrorBar → ErrorBadge → Task 9 + Task 10
- [x] §5.1 DetailPanel 移除 aside → Task 5
- [x] §5.2 DetailPanel 关闭联动 → Task 5
- [x] §5.3 KB 切换/删除关闭 detail → Task 8
- [x] §6.1 Sidebar 调用 openPanel → Task 7
- [x] §6.2 MapView 调用 openPanel → Task 6
- [x] §6.3 ErrorBar 调用 openPanel → Task 10
- [x] §6.4 App.tsx 删除 openKbButton → Task 11
- [x] §7 i18n 调整 → Task 2 + Task 9 + Task 11
- [x] §8 边界情况 → Task 8（KB 切换/删除关闭）+ Task 1（重复点击防抖）+ Task 5（加载失败保留）

所有 spec 章节都有覆盖。

---

## 执行方式

完成后请用 `superpowers:subagent-driven-development`（推荐）或 `superpowers:executing-plans` 技能逐步执行。
