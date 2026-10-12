# 全文搜索 + Markdown 渲染设计

**代号**：搜索 & 渲染
**日期**：2026-10-12
**状态**：待用户审阅
**关联文档**：
- [Synapse-KB 设计文档](./2026-10-09-synapse-kb-design.md)
- [国际化设计文档](./2026-10-11-i18n-design.md)
- [KB 规范 v2](./2026-10-12-kb-standards-v2-design.md)

---

## §0 目标

为 Synapse KB 增加两个互补的核心能力：

1. **全文搜索** — KB 中条目变多（>50）后，单纯靠侧栏树形导航找不到东西。需要按 title / tags / labels / group / content 跨字段加权匹配。
2. **Markdown 渲染** — 当前直接用 `<pre>` 展示 `.md` 原始文本（`src/views/DetailPanel.tsx:314`），用户阅读体验差，且 `[[wikilink]]` 是死文本。需要按 markdown 渲染，并把 `[[id]]` / `[[id|caption]]` 变成可点击的内部链接。

**未做**（留给后续）：
- 全文搜索不引入 SQLite FTS5（KB 规模 < 几千条时不需要；如需要 Rust 端层可独立加，hook 层不用动）
- 不做替换 .md 文件的编辑（属于方向 2）

---

## §1 架构总览

```
src/
  search/
    index.ts              # 内存索引构建（基于 entries + content LRU 缓存）
    useSearch.ts          # hook（150ms debounce + memoized）
    highlight.ts          # 匹配词高亮
    SearchBox.tsx         # 输入框 + 结果下拉
    SearchBox.zh.json
    SearchBox.en.json
  markdown/
    MarkdownView.tsx      # react-markdown 容器，处理 wikilink + i18n 错误回退
    remarkWikilink.ts     # remark 插件：识别 [[id]] / [[id|caption]]
    MarkdownView.zh.json
    MarkdownView.en.json
src/components/Sidebar.tsx           # 顶部嵌入 SearchBox
src/views/DetailPanel.tsx            # 用 MarkdownView 替换 <pre>，加原文/渲染切换
src/i18n/resources.ts                # 注册新 NS：Search, MarkdownView
package.json                         # +react-markdown, +remark-gfm
```

**不动的部分**：
- Rust 后端零改动（方案 A：纯前端搜索；方案 B/C 已评估但 v1 不上）
- 数据库 schema 不变
- 现有 `get_entry_content` 命令复用（搜索 content 时按需拉）

---

## §2 全文搜索

### §2.1 匹配范围与权重

跨以下 5 类字段加权匹配：

| 字段 | 权重 | 数据就位 |
|---|---|---|
| `title` | 3.0 | entries 内存 |
| `labels` | 1.5 | entries 内存 |
| `tags` | 2.0 | entries 内存 |
| `group` | 1.0 | entries 内存 |
| `content` | 1.0 | 按需 `get_entry_content` |

**评分**：`score = Σ field_weight × field_score`，`field_score ∈ [0, 1]`，计算方式 = 关键词命中数 / 查询关键词数（多关键词 AND 关系，每个关键词都必须至少在一处命中）。

示例：query="react hook"
- entry A title="React Hooks 入门" → 两个词都在 title 命中 → 3.0 × 1.0 = 3.0
- entry B content 含 "react" 和 "hook" → 1.0 × 1.0 = 1.0
- entry C content 仅含 "react" → field_score = 0.5, 不是 1.0, **不出现**在结果里（v1 严格 AND）

### §2.2 Snippet 截取

仅对 content 命中的条目生成 snippet：
- 在 content 中找第一个匹配的关键词位置
- 取该位置前后各 20 字符作为 snippet 窗口
- 高亮匹配词
- 跨窗口截断：截掉窗口外的字符，窗口边界补 `..` / `..` 前缀后缀
- 若多个关键词都命中，第一个匹配的 snippet 优先

title/labels/tags/group 不生成 snippet —— 结果项里直接展示原值（高亮匹配词）。

### §2.3 客户端缓存策略

`get_entry_content` 是 per-entry IPC 调用；搜索时可能触发 N 次（N = 当前 KB entry 数）。为此加 LRU：

```ts
// 伪代码
type ContentCache = Map<string, { text: string; at: number }>;
const MAX_CACHE_ENTRIES = 200;
const MAX_CACHE_AGE_MS = 5 * 60 * 1000; // 5 分钟
```

策略 A 维护边界
- KB 切换时清空（按 `activeKbId` 失效）
- entry 数量超过 MAX 时 LRU 淘汰
- 失效通过 KB 切换 + 时间过期 + 手动 reload 按钮

### §2.4 debounce 与并发竞态

- 输入触发 debounce 150ms（用户确认）
- 150ms 内多次输入只触发最后一次的查询
- 上一次查询若还在 in-flight，后续输入触发的新查询开始时取消前一次（用 `AbortController` 或自增 query id 哨兵）
- 输入框失焦 / Escape / 点击外部 → 关闭下拉

### §2.5 边界与降级

- 空 KB（0 entry）→ 不展示搜索框
- 空 query → 不展示结果下拉
- IPC 失败（content 读不出）→ 该 entry 从结果里跳过，不阻塞其他
- 查询无匹配 → 下拉展示 "No results" 占位文案

### §2.6 全局快捷键

- `⌘K`（macOS）/ `Ctrl+K`（Win/Linux）→ 聚焦 SearchBox 输入框
- 已聚焦时按 `Esc` → 关闭结果下拉并失焦
- `↑` / `↓` 在结果项之间移动焦点
- `Enter` → 跳转到当前焦点项

注册位置：根级 `useEffect`（在 `Sidebar` 或 `App`），监听 `document` keydown。

---

## §3 Markdown 渲染

### §3.1 库选型

`react-markdown` + `remark-gfm`：
- 工业级组件，TypeScript 类型完整
- `remark-gfm` 提供 GFM 表格、删除线、自动链接、task list
- 通过自定义 `components` 覆盖 + `remark` 插件支持 wikilink

`package.json` 新增依赖：
```
"react-markdown": "^9.0.1",
"remark-gfm": "^4.0.0",
```

### §3.2 wikilink 解析规则

**支持的两种语法**：

```markdown
See [[react-hooks]] for details.            # id-only
Compare with [[state-management|状态管理]].  # id|caption
```

**匹配正则**（remark 文本节点扫描时使用）：
```
\[\[([^\[\]\|]+)(?:\|([^\]]+))?\]\]
```

- `[^\[\]\|]+` 限定 id 不能含 `--` `--` `[` `|` 自身（保留 wikilink 边界）；其余 Unicode 字符均允许
- 与 `entry::read_md_file` 的 id 生成规则对齐：`rel.with_extension("").replace(['\\','/'], "-")`，理论上能含中文、空格、点号
- `(?:\|([^\]]+))?` 可选的 `|caption` 部分，`[^\]]+` 表示任意非 `]` 字符
- 不支持嵌套、跨行、转义 `\]`（v1 范围）

**在 `code block` / `inline code` 内的 `[[..]]` 不识别** —— remark 标准行为：text 节点不会进入 code 容器。

### §3.3 wikilink 渲染输出

解析后的 wikilink 渲染为：

```html
<a class="wikilink" data-wikilink data-target="<id>"><caption or id></a>
```

点击行为（在 MarkdownView 上方）：
- 若 `target` 在当前 KB 存在 → `useAppStore.selectEntry(id)` + `usePanelStore.open('detail')`
- 若不存在 → 弹 CreateEntryDialog（参考现有 `DetailPanel.tsx:524` 的 "create entry from external reference" 路径，但简化为：只需输入 parentGroup + name，自动从当前 entry 复制 refText/sourceUrl 占位）

实现方式：`MarkdownView` 包一个 onClick 监听，事件源 `e.target.closest('[data-wikilink]')`，target 的 `data-target` 即 id。

### §3.4 样式与错误回退

**样式**：用 Tailwind 的 prose 类（`prose prose-sm`），自定义 `.wikilink` 颜色（蓝色 + 下划线，hover 时加深）。

**错误回退**：若 react-markdown 抛错 → 捕获并降级渲染 `<pre>{markdown}</pre>`（与旧实现一致）。失败时不破坏 UI，仅在该条 entry 顶部显示一个"渲染失败，已切换到原文"的 toast（可选 v2）。

### §3.5 原文 / 渲染切换

在 DetailPanel 顶部 markdown 区加 segmented control：

```
[ 渲染 ] [ 原文 ]
```

- 切换状态保存到本地组件 state（不需要全局 store）
- 切换时重新渲染对应视图
- 原文模式：保留现有的 `<pre>` 展示

### §3.6 wikilink 跳转的边界

| 场景 | 行为 |
|---|---|
| `id` 在当前 KB 存在 | selectEntry + open detail panel |
| `id` 不在当前 KB | 弹 CreateEntryDialog，parentGroup 默认 `kb.root` |
| `id` 含有非法字符 | markdown 渲染时跳过（不输出 `<a>`），保留原文文本 |
| 多重嵌套 `[[a]] [[b]]` | 各自独立识别 |

---

## §4 数据流

### §4.1 搜索数据流

1. 用户在 SearchBox 输入 "react"
2. SearchBox 本地 state 更新 → useEffect 触发 debounce 150ms
3. debounce 到点 → 触发 `useSearch(query)` 内的搜索逻辑
4. useSearch 调用 `searchIndex(query, entries, contentCache)`
5. searchIndex 评分：title/tags/labels/group 全在内存中命中
6. 对所有命中的 entry 异步批量 fetch content（并发上限 4）
7. 计算最终 score，排序，取前 10
8. 下拉渲染前 10 条 + 高亮 + snippet
9. 用户点击某条 → `selectEntry(id)` + `openPanel('detail')`

### §4.2 Markdown 渲染数据流

1. DetailPanel 已加载 `content.markdown`（通过 `get_entry_content`）
2. MarkdownView 接收 markdown 字符串
3. react-markdown 解析 → remark 文本节点扫描 → remarkWikilink 替换 `[[..]]` 为虚拟节点
4. remarkWikilink 输出自定义 node `wikilink` 类型 { id, caption }
6. MarkdownView 的 `components` 映射 `wikilink` → 渲染为 `<a>` 并附加 data-target
7. 用户点击 `<a>` → MarkdownView 的 onClick 拦截 → 处理跳转/创建

---

## §5 i18n 键约定

新增两个 NS：

### `Search.{zh,en}.json`

```json
{
  "placeholder": "搜索整个知识库…",
  "shortcutHint": "⌘K",
  "noResults": "无匹配",
  "openInKb": "在此 KB 搜索",
  "searching": "搜索中…",
  "fieldsLabel": "范围：{fields}",
  "resultCount": "{count} 条匹配",
  "inGroupBadge": "{group}",
  "openEntry": "打开",
  "createFromRef": "从引用创建"
}
```

### `MarkdownView.{zh,en}.json`

```json
{
  "renderError": "渲染失败，已切换到原文",
  "renderErrorTitle": "此 .md 渲染失败",
  "viewToggleRender": "渲染",
  "viewToggleSource": "原文",
  "wikilinkBroken": "(引用不存在: {id})",
  "wikilinkBrokenHint": "点击创建"
}
```

注册到 `src/i18n/resources.ts` 的 `NS` 和 `resources` 对象。

---

## §6 验收标准

### §6.1 搜索功能

1. ✅ 在 Sidebar 顶部搜索框输入 "react" → 显示 title/tags/labels/group/content 任一含 "react" 的前 10 条结果
2. ✅ 结果按 score 排序（title 命中排前）
3. ✅ 多关键词 AND（"react hook" 必须两个词都存在）
4. ✅ snippet 在 content 命中条目下展示，前后 ±20 字符 + 省略号
5. ✅ 匹配词高亮（黄色背景 + 加粗）
6. ✅ 点击结果 → 跳转到对应 entry 并打开 detail panel
7. ✅ `⌘K` / `Ctrl+K` 唤起搜索框
9. ✅ KB 切换时清空缓存
10. ✅ 单条 content IPC 失败不影响整体搜索

### §6.2 Markdown 渲染

1. ✅ heading / list / code block / bold / italic / link 全部正确
2. ✅ GFM 表格、task list 支持
3. ✅ `[[react-hooks]]` 渲染为蓝色下划线链接
4. ✅ `[[state-management|状态管理]]` 渲染为显示 "状态管理" 的链接
5. ✅ 点击 wikilink → 跳转 / 弹创建
6. ✅ code block 内的 `[[..]]` 不被识别（保留原文）
7. ✅ 原文/渲染切换双向工作
8. ✅ 渲染失败 → 降级到原文，不破坏 layout

### §6.3 测试

- TS: `npm run build` 通过（strict typecheck）
- Rust: 无新代码（无需）
- 手工 e2e: 7 条搜索 + 10 条渲染场景，按 §6.1 / §6.2 逐项验收

---

## §7 风险与决策记录

### 决策 1：纯前端 vs SQLite FTS5

**结论**：v1 纯前端（方案 A）。

**理由**：
- KB 规模定位为个人/小团队，< 几千条
- 全文搜索响应瓶颈是 IPC N 次，但加 LRU + 并发上限后实测可接受
- 不动 Rust 端减少 PR 范围

**未来触发条件**：若实测 KB > 5000 条时搜索明显卡顿，考虑切到方案 B（SQLite FTS5 + 新建 `search_entries` 命令）。架构上 `useSearch` 已隔离搜索逻辑，替换实现层不动 hook 接口。

### 决策 2：react-markdown vs 自实现

**结论**：react-markdown + remark-gfm。

**理由**：
- 自实现要处理 30+ 边界 case（嵌套列表、code block 内 `[ ]`、HTML 内嵌、转义），收益不抵成本
- react-markdown 9.x 是 tree-shakable，bundle 增量 < 30KB
- 类型完整，扩展点清晰

### 决策 3：搜索同时支持标签与标签

**结论**：支持 `tags` 与 `labels` 两类字段都参与搜索。

**理由**：
- kb-standards-v2 已区分 `tags`（内容性，搜索召回）与 `labels`（工作流性，快速筛选）
- 用户搜索时两类都要找

### 决策 4：debounce 时长

**结论**：150ms。

**理由**：
- 低于 100ms 体感跳，太短；200ms+ 在慢机器上感觉卡
- 150ms 在中等规模 KB 上既不重复请求也不卡输入

### 决策 5：wikilink 别名语法

**结论**：v1 同时支持 `[[id]]` 与 `[[id|caption]]`。

**理由**：
- 用户已确认要支持
- 实现简单（一行正则多一个 capture group）
- 现有 .md 文件不存在兼容性破坏（兼容 id-only）

### 决策 6：wikilink 跨 KB 处理

**未做**：v1 不支持跨 KB wikilink（只能跳到当前 KB 的 entry）。若 `id` 在其他 KB 存在但当前 KB 不存在 → 提示"跨 KB 跳转 v2 支持"。

**理由**：跨 KB 跳转语义复杂（要不要切换 KB？弹 toast？），留给 v2。

---

## §8 未来扩展（不在 v1 范围）

- 跨 KB wikilink 跳转
- 搜索结果点击后高亮匹配词在 entry 中位置（DetailPanel 内的二次跳转）
- 搜索范围按 severity / status / custom 标签过滤
- SQLite FTS5（若 KB 规模超过 5000 条触发）
- Markdown 渲染失败时弹 toast 通知（v1 静默降级）