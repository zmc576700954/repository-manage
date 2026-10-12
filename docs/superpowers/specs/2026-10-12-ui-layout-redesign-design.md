# Synapse KB UI 布局重构设计

**代号**：UI 三栏布局重构
**日期**：2026-10-12
**状态**：待用户审阅
**关联文档**：
- [Synapse-KB 设计文档](./2026-10-09-synapse-kb-design.md)
- [国际化设计文档](./2026-10-11-i18n-design.md)

---

## §0 目标

当前 Synapse KB 桌面应用在用户交互过程中出现 **4 个内容块挤压**的问题：

```
[Sidebar] [Map + ErrorBar] [RightPanelStack] [DetailPanel]
   288px      flex-1          360px            448px
```

当用户依次操作：① 点击地图节点打开详情；② 点击 ErrorBar 红条打开验证面板 → 右侧同时出现两个并列面板，总宽 808px，主图被严重压缩，视觉拥挤。

**本次重构目标**：
1. 应用固定为 **3 个内容块**：`[Sidebar | Main | RightColumn]`
2. 右侧所有面板合并到同一列（参考 ClaudeCode APP 风格），**上下排列**
3. ErrorBar 从主图顶部挪走，改为右上角 badge 按钮
4. 右列面板上限 3 个，超过时按 FIFO 关闭栈底

**产出**：UI 布局变更 + 状态机重构 + i18n 字符串调整。

---

## §1 架构概览

### 1.1 当前实现

`App.tsx` 是顶层水平 flex 容器：

```jsx
<div className="flex h-screen w-screen">
  <Sidebar />                    {/* 固定 288px */}
  <main className="flex-1">       {/* 占满剩余 */}
    <ErrorBar />                 {/* 主图顶部 ~40px 红/黄条 */}
    <MapView />
  </main>
  <RightPanelStack />            {/* 360px，仅 openPanels 非空时显示 */}
  {selectedEntryId && <DetailPanel />}  {/* 448px，独立右侧 */}
</div>
```

`RightPanelStack` 内部已正确实现上下排列：
```jsx
<div className="flex h-full flex-col">  {/* 列容器 */}
  {openPanels.map(panel => <RightPanel>...</RightPanel>)}
</div>
```

### 1.2 目标实现

`App.tsx` 简化为：

```jsx
<div className="flex h-screen w-screen">
  <Sidebar />
  <main className="flex-1 relative">
    <ErrorBadge />          {/* 右上角小按钮 */}
    <MapView />
  </main>
  <RightPanelStack />       {/* 唯一右列，内含 DetailPanel + Validation + Settings + Filter */}
</div>
```

右列结构：
```
┌─────────────────────────┐
│ DetailPanel     ✕      │  ← flex 1，最小高度 140px
├─────────────────────────┤
│ ValidationPanel ✕      │  ← flex 1，最小高度 140px
├─────────────────────────┤
│ SettingsPanel   ✕      │  ← flex 1，最小高度 140px
└─────────────────────────┘
```

---

## §2 面板状态机

### 2.1 PanelType 扩展

当前 `panelStore.ts`：

```typescript
export type PanelType = 'validation' | 'settings' | 'filter';
```

新增 `'detail'` 类型，使 DetailPanel 与其他面板走同一通道：

```typescript
export type PanelType = 'detail' | 'validation' | 'settings' | 'filter';

const MAX_PANELS = 3;
```

### 2.2 open / close / FIFO 收回

`panelStore.ts` 改造：

```typescript
interface PanelState {
  openPanels: PanelType[];         // 按打开顺序排列：index 0 = 最早（栈底），最后 = 最新（栈顶）
  open: (panel: PanelType) => void;
  close: (panel: PanelType) => void;
  toggle: (panel: PanelType) => void;
  closeAll: () => void;
  width: number;
  setWidth: (w: number) => void;
}

export const usePanelStore = create<PanelState>((set, get) => ({
  openPanels: [],
  width: 360,
  setWidth: (w) => set({ width: Math.max(280, Math.min(800, w)) }),

  open: (panel) => {
    const { openPanels } = get();
    // 已开则什么都不做
    if (openPanels.includes(panel)) return;
    // 未开：若已达上限，关闭栈底（最早打开的）
    if (openPanels.length >= MAX_PANELS) {
      const evicted = openPanels[0];
      const next = [...openPanels.slice(1), panel];
      set({ openPanels: next });
      // 同时清掉 evicted 对应的状态（如 detail 的 selectedEntryId）
      onPanelEvicted?.(evicted);
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

**FIFO 行为**：开第 4 个面板时，关闭 `openPanels[0]`（最早打开），新面板追加到栈尾。

### 2.3 选中状态与 detail 面板联动

DetailPanel 的可见性由 `panelStore.openPanels` 中是否包含 `'detail'` 决定，**不再** 由 `useAppStore.selectedEntryId` 单独控制。

但 `selectedEntryId` 仍需保留（用于侧边栏高亮、MapView 节点高亮）。

调用约定：
- **Sidebar 点击条目** → `selectEntry(id)`（高亮）+ `openPanel('detail')`（开面板）
- **MapView 点击节点** → `selectEntry(id)` + `openPanel('detail')`
- **DetailPanel 内关联跳转** → `selectEntry(newId)` + 已有 detail 面板**复用**（仅更新内容，不重复打开）
- **DetailPanel ✕ 关闭** → `closePanel('detail')` + `selectEntry(null)`

### 2.4 收回联动（FIFO 副作用）

`open()` 调用 `onPanelEvicted?.(evicted)` 回调，需要 store 暴露订阅点，或在 store 内直接处理：

```typescript
open: (panel) => {
  // ... 同上
  if (openPanels.length >= MAX_PANELS) {
    const evicted = openPanels[0];
    if (evicted === 'detail') {
      useAppStore.getState().selectEntry(null);
    }
    // Validation / Settings / Filter 无副作用
    set({ openPanels: [...openPanels.slice(1), panel] });
  }
  // ...
}
```

通过 `useAppStore.getState()` 直接访问避免循环依赖。

---

## §3 RightPanelStack 渲染

### 3.1 新组件结构

```typescript
// src/components/RightPanelStack.tsx
export const RightPanelStack: React.FC = () => {
  const { openPanels, close, width } = usePanelStore();

  if (openPanels.length === 0) return null;

  return (
    <div
      className="flex h-full flex-col gap-2 border-l border-gray-200 bg-gray-50 p-2"
      style={{ width }}
    >
      {openPanels.map((panelType) => (
        <RightPanel key={panelType} type={panelType} onClose={() => close(panelType)}>
          {renderPanel(panelType)}
        </RightPanel>
      ))}
    </div>
  );
};

function renderPanel(type: PanelType): React.ReactNode {
  switch (type) {
    case 'detail':      return <DetailPanel />;
    case 'validation':  return <ValidationPanel />;
    case 'settings':    return <Settings />;
    case 'filter':      return <FilterPanel />;  // 待实现
  }
}
```

### 3.2 RightPanel 容器

`src/components/RightPanel.tsx` 调整为受控高度：

```typescript
interface Props {
  type: PanelType;
  onClose: () => void;
  children: React.ReactNode;
}

export const RightPanel: React.FC<Props> = ({ type, onClose, children }) => {
  return (
    <div className="flex min-h-[140px] flex-1 flex-col overflow-hidden rounded-md border border-gray-200 bg-white shadow-sm">
      <div className="flex items-center justify-between border-b border-gray-200 px-3 py-2">
        <h3 className="text-sm font-medium text-gray-900">{getPanelTitle(type)}</h3>
        <button onClick={onClose} className="text-gray-400 hover:text-gray-600">✕</button>
      </div>
      <div className="flex-1 overflow-y-auto">{children}</div>
    </div>
  );
};
```

**`min-h-[140px]` + `flex-1` + 内部 `overflow-y-auto`** 三件套确保：
- 每个面板最小 140px，可读
- 多面板时按 `flex-1` 平均分配剩余高度
- 内容超出时各自内部滚动，互不干扰

### 3.3 标题映射

```typescript
const PANEL_TITLES: Record<PanelType, string> = {
  detail:     '📄 Detail',
  validation: '⚠ Validation',
  settings:   '⚙ Settings',
  filter:     '🔍 Filter',
};
```

走 i18n 命名空间 `RightPanel`（新建）：

```json
// src/components/RightPanel.zh.json
{
  "titleDetail":     "详情",
  "titleValidation": "校验",
  "titleSettings":   "设置",
  "titleFilter":     "筛选"
}
```

---

## §4 ErrorBar → ErrorBadge

### 4.1 新组件

`src/components/ErrorBar.tsx` 改名为 `ErrorBadge.tsx`，从条状改为右上角浮动按钮：

```typescript
interface Props {
  errorCount: number;
  warningCount: number;
}

export const ErrorBadge: React.FC<Props> = ({ errorCount, warningCount }) => {
  const { open } = usePanelStore();

  if (errorCount === 0 && warningCount === 0) return null;

  const color = errorCount > 0 ? 'red' : 'yellow';

  return (
    <button
      onClick={() => open('validation')}
      className={`absolute right-4 top-4 z-10 rounded-full px-3 py-1 text-xs shadow-sm ${
        color === 'red'
          ? 'bg-red-100 text-red-800 hover:bg-red-200'
          : 'bg-yellow-100 text-yellow-800 hover:bg-yellow-200'
      }`}
      title={t('tooltip', { errors: errorCount, warnings: warningCount })}
    >
      {errorCount > 0 && `⚠ ${errorCount}`}
      {errorCount > 0 && warningCount > 0 && ' · '}
      {warningCount > 0 && `${warningCount}`}
    </button>
  );
};
```

**变化点**：
- `<div>` → `<button>`（可点击性符合语义）
- 高度从 ~40px 横条 → ~24px badge
- 颜色更浅（hover 高亮），不刺眼
- 仅显示数字（点击 tooltip 给出完整文本）

### 4.2 App.tsx 集成

```typescript
// App.tsx
<main className="relative flex-1">
  <ErrorBadge errorCount={errorCount} warningCount={warningCount} />
  {error && <ErrorBanner>{error}</ErrorBanner>}
  {isLoading ? <Loading /> : activeKbId ? <MapView /> : <NoActiveKb />}
</main>
```

### 4.3 i18n 字符串

```json
// src/components/ErrorBar.zh.json (改名为 ErrorBadge，或保留 ErrorBar 文件名仅改内容)
{
  "tooltip": "查看 {{errors}} 个错误和 {{warnings}} 个警告"
}
```

保留文件路径 `ErrorBar.tsx` 以减小 diff（实际语义已变）或直接改名 `ErrorBadge.tsx`。**决定**：改名以保持命名清晰。

---

## §5 DetailPanel 适配

### 5.1 容器变化

原 `DetailPanel.tsx` 直接渲染 `<aside className="...w-[28rem]">`。改造后它**作为 RightPanel 的子组件**渲染，不再自带宽度与边框。

需要删除/修改的样式：
```diff
- <aside className="flex h-full w-[28rem] max-w-[40vw] flex-col border-l border-gray-200 bg-white">
+ <div className="flex h-full flex-col">  {/* 容器由 RightPanel 提供 */}
```

### 5.2 关闭行为

`DetailPanel` 内的 ✕ 按钮和 "close footer"：

```diff
- onClick={() => selectEntry(null)}
+ onClick={() => {
+   selectEntry(null);
+   usePanelStore.getState().close('detail');
+ }}
```

关联跳转（点击 outgoing/incoming 关联的标题）：
```typescript
const handleSelectRelated = (id: string) => {
  selectEntry(id);  // 高亮新 entry
  // 不调用 open('detail') —— 面板已开，避免重渲染
};
```

### 5.3 关联 entry 被删除

`useAppStore.setActiveKb` 已处理：若新 KB 不包含旧 selection 则 `selectEntry(null)`。补充：在 `useAppStore.removeKb` 后增加：

```typescript
removeKb: async (id) => {
  try {
    await invoke('remove_kb', { id });
    set({ selectedEntryId: null });
    usePanelStore.getState().close('detail');  // 同步关闭 detail 面板
    await Promise.all([get().refreshKbs(), get().loadEntries()]);
  } catch (e) { ... }
}
```

---

## §6 调用方改造

### 6.1 Sidebar

```typescript
// src/components/Sidebar.tsx
const selectEntry = useAppStore((s) => s.selectEntry);
const { open: openPanel } = usePanelStore();

const handleSelect = (id: string) => {
  selectEntry(id);
  openPanel('detail');
};

// TreeRow onSelect 改用 handleSelect
<TreeRow ... onSelect={handleSelect} />
```

### 6.2 MapView

```typescript
// src/views/MapView.tsx
const onNodeClick = useCallback(
  (_, node) => {
    selectEntry(node.id);
    usePanelStore.getState().open('detail');
  },
  [selectEntry]
);
```

### 6.3 ErrorBar / ErrorBadge

```typescript
const onClick = () => usePanelStore.getState().open('validation');
```

### 6.4 Settings 入口

Sidebar 右上角 ⚙ 按钮：
```typescript
onClick={() => usePanelStore.getState().open('settings')}
```

App.tsx 顶部 "打开知识库" 按钮（如果保留）同样改为开 settings 面板。

---

## §7 i18n 变更汇总

| 文件 | 变更 |
|------|------|
| `App.zh/en.json` | `openKbButton` 可移除（改用 Sidebar 内 ⚙ 按钮）或保留为面板标题 |
| `ErrorBar.zh/en.json` | 移除 `clickToView`、`errors`、`warnings`；新增 `tooltip` |
| `RightPanel.zh/en.json` | 新建：`titleDetail / titleValidation / titleSettings / titleFilter` |
| `Sidebar.zh/en.json` | `manageKbTitle` 文案微调（仍为 "管理知识库"） |
| `DetailPanel.zh/en.json` | 无内容变更，仅引用方式变化 |

`src/i18n/resources.ts` 需注册新命名空间 `RightPanel`。

---

## §8 边界情况

1. **DetailPanel 加载失败**：`content.error` 显示错误信息，面板保留，不关闭
2. **当前 entry 被删除**：自动 `selectEntry(null)` + `close('detail')`
3. **KB 切换**：setActiveKb 完成后，若 selectedEntryId 不在新 KB 中则清除，并关闭 detail 面板
4. **重复点击同一节点**：`open()` 内置 `includes` 检查，不会重复添加
5. **panelStore 状态丢失**：当前 panelStore 是 zustand 普通 store（非 persist）。刷新页面后面板全部关闭，符合预期；如需保留则后续单独设计
6. **FilterPanel 暂未实现**：保留 `'filter'` 类型占位，UI 显示 "Filter placeholder"

---

## §9 测试要点

### 9.1 手工验证

- [ ] 主图无面板打开时：3 内容块，右侧无面板
- [ ] 点击节点：DetailPanel 出现在右列
- [ ] 点击右上角 ⚠ badge：ValidationPanel 出现在 Detail 下方
- [ ] 依次点击 Settings、Filter：四个面板时，Filter 进入底部，Validation 自动关闭（栈底）
- [ ] 关闭某个面板：剩余面板保持高度
- [ ] 主图宽度足够大时（>1200px），地图呼吸顺畅

### 9.2 类型与构建

- `npm run build` 通过（严格 TS）
- `panelStore.open/close` 类型签名正确
- 不引入新的 `any`

### 9.3 i18n

- 所有新文案 zh/en 双语齐全
- resources.ts 已注册 RightPanel
- ErrorBar tooltip 双语显示

---

## §10 不在范围内

- 面板顺序拖拽调整（仅固定 FIFO）
- 面板大小的手动拖拽（仅自动 flex-1 + min-height）
- panelStore 状态持久化（刷新即丢失）
- FilterPanel 实际功能实现（仅占位）
- DetailPanel 内部结构调整（仅外部容器变化）
