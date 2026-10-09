# Synapse-KB 设计文档

**代号**：Synapse-KB（个人知识库管理器）
**日期**：2026-10-09
**状态**：待用户审阅

---

## 1. 目标与定位

做一个让用户"只需要考虑怎么新增知识库、知识库之间关联关系"的工具，不因为使用本工具而产生新的认知负担。

### 核心价值观
- **轻量**：包体 4-5MB，启动 0.8-1.5s
- **零业务存储**：不存任何业务内容，只存运行时元数据
- **就近规范**：在内容文件里就地规范元数据，不引入新的元数据库
- **可视化优先**：关系胜过信息本身
- **友好门槛**：HTML 注释 + Markdown 双向链接即可上手，无需懂 YAML

### 参考启发
- dsh-synapse 的"投影式"读取 + 独立画布布局存储
- Obsidian / Logseq 的 `[[双向链接]]` 与 `![[附件嵌入]]`
- Tauri 2.0 + React Flow 在性能与开发体验的平衡

---

## 2. 技术栈

| 层 | 选型 | 理由 |
|---|---|---|
| 桌面壳 | Tauri 2.0 | 包小 4-5MB，启动 0.8-1.5s，60% 快于 Electron |
| 后端 | Rust | 高性能文件 IO + 索引，tokio 异步 |
| 前端框架 | React 18 + TypeScript | 生态成熟 |
| 构建工具 | Vite | 快速开发体验 |
| 地图可视化 | React Flow (xyflow) | 500 节点时比 Cytoscape 快 6 倍，DOM 渲染便于交互 |
| 样式 | Tailwind CSS + Radix UI | macOS 原生风落地快 |
| 本地索引 | SQLite via rusqlite | 加速条目与关联查询 |
| 本地服务 | axum (HTTP) | Web 端通过 REST 调用 |
| 状态管理 | Zustand | 轻量 |

---

## 3. 强制规范

### 3.1 目录结构

```
kb-root/                          # 用户在 Settings 里配置的根路径
├── topic-a/                      # 条目文件夹（文件夹名即条目 ID）
│   ├── content.md                # 必含：条目主体
│   ├── attachments/              # 推荐：本地附件
│   │   └── diagram.png
│   ├── notes/                    # 可选：块级补充文件
│   │   └── follow-up.md
│   └── .synapse-layout.json      # 自动生成：本条目节点位置
├── topic-b/
│   ├── content.md
│   └── attachments/
└── _index.yaml                   # 自动生成：知识库摘要（可选）
```

### 3.2 content.md 规范

#### 3.2.1 基础元数据（HTML 注释行）

```
<!-- @synapse-id: react-hooks -->
<!-- @synapse-title: React Hooks 深入理解 -->
<!-- @synapse-tags: react, frontend -->
<!-- @synapse-group: frontend -->

# 内容主体

正文里可以使用：
- `[[state-management]]` 自动识别为关联
- `![[./attachments/diagram.png|流程图]]` 自动识别为附件
- `[[render-flow#init-block]]` 自动识别为块引用
```

#### 3.2.2 字段定义

| 字段 | 必填 | 说明 |
|---|---|---|
| `@synapse-id` | 是 | 唯一 ID，默认用文件夹名 |
| `@synapse-title` | 是 | 显示标题 |
| `@synapse-tags` | 否 | 逗号分隔的标签 |
| `@synapse-group` | 否 | 分组（地图上聚类用） |

#### 3.2.3 可选 YAML 高级字段

```yaml
---
color: "#5B8DEF"      # 节点自定义颜色
shape: "rect"          # 节点形状
pinned: false          # 是否固定
order: 1               # 在分组内的排序
---
```

### 3.3 关联类型（5 类）

| type | 颜色 | 含义 | 线型 |
|---|---|---|---|
| `reference` | 蓝色 | 参考引用 | 实线 |
| `derived` | 绿色 | 派生/总结自 | 实线带箭头 |
| `contradicts` | 红色 | 与之矛盾 | 虚线 |
| `supersedes` | 橙色 | 取代旧版 | 双实线 |
| `extends` | 紫色 | 补充/扩展 | 曲线 |

自定义类型（无预设颜色）：灰色虚线。

### 3.4 关联解析优先级

1. `<!-- @synapse-related: path:type -->` 注释（明确语义）
2. YAML `related:` 字段（高级）
3. 正文里的 `[[path]]` 自动提取（默认无类型，灰色虚线，可后续手动指定类型）

---

## 4. 模块拆分

### 4.1 Rust 后端 (`src-tauri/`)

| 模块 | 职责 |
|---|---|
| `scanner/` | 递归扫描根目录，识别条目文件夹 |
| `parser/` | 解析 content.md 的 `@synapse-*` 注释、YAML、Markdown 中的双向链接和附件 |
| `index/` | SQLite 索引（条目元数据、关联关系、标签、分组），存于热路径 `~/.local/share/synapse-kb/index.db` |
| `layout_store/` | `.synapse-layout.json` 读写（节点位置） |
| `watcher/` | 文件系统 watcher（notify crate），增量更新索引 |
| `local_server/` | Tauri 内置 axum HTTP 服务，端口 19181，暴露 REST API |

### 4.2 前端 (`src/`)

| 模块 | 职责 |
|---|---|
| `views/MapView/` | 画布视图，React Flow 实现 |
| `views/DetailPanel/` | 右侧详情面板 |
| `views/Settings/` | 路径配置、关联类型颜色、主题 |
| `components/NodeCard/` | 自定义节点组件 |
| `lib/api/` | IPC 客户端（桌面）/ fetch（Web） |
| `lib/markdown/` | 渲染、解析 |

### 4.3 共享类型 (`shared/`)

| 文件 | 内容 |
|---|---|
| `types.ts` | Entry、Relation、Attachment、Layout 等类型定义 |

---

## 5. 数据流

### 5.1 启动流程

```
用户启动应用
  ↓
Rust 后端扫描根目录
  ↓
构建 SQLite 索引（首次）/ 增量更新（后续）
  ↓
启动 axum HTTP 服务（端口 19181）
  ↓
前端加载 → 调用 /api/entries 获取全部条目元数据
  ↓
渲染地图
```

### 5.2 编辑流程

```
用户在 UI 点击"创建关联"
  ↓
前端调用 IPC: createRelation(from, to, type)
  ↓
Rust 后端：
  1. 读取 from.content.md
  2. 插入 <!-- @synapse-related: to:type --> 注释
  3. 写回文件
  4. 更新 SQLite 索引
  5. 通知前端更新
  ↓
地图实时更新
```

### 5.3 文件变更流程

```
外部编辑器修改 content.md
  ↓
Rust watcher 监听到变更
  ↓
重新解析该条目
  ↓
更新 SQLite 索引
  ↓
推送 SSE 事件给前端
  ↓
地图相应节点更新
```

---

## 6. 性能保证

| 挑战 | 策略 |
|---|---|
| 大量条目（1000+） | Rust 后端 SQLite 索引 + 前端 `onlyRenderVisibleElements` 虚拟化 |
| 大文件加载 | 仅读 YAML/注释头 + Markdown 前 200 字符预览，详情面板才读全文 |
| 启动速度 | Rust 后端并行扫描，前端懒加载 |
| 缩放卡顿 | 节点组件 React.memo，缩放仅触发 transform |
| 关联图更新 | 文件 watcher 增量更新，不重建全图 |

---

## 7. 双形态交付

### 7.1 Tauri 桌面

```
macOS/Win/Linux 原生应用，启动后内置 axum HTTP 服务
```

### 7.2 静态 Web

```
同一套前端代码 → vite build → 静态文件

Web 端访问本地文件的方式：
- 首选：用户在桌面启动 Tauri 应用，内置 axum 服务，Web 通过 CORS 调用
- 降级（计划中）：独立 synapse-server CLI（Rust 二进制），提供相同 REST API
```

> 当前 MVP 阶段仅实现首选方式，synapse-server CLI 在后续版本提供。

### 7.3 Web 端数据访问

- **首选**：用户启动 Tauri，提供 axum HTTP 服务（同源策略通过 CORS 解决）
- **降级**：浏览器 File System Access API（Chrome/Edge 支持）
- **不支持**：Safari/Firefox 完整读写

---

## 8. MVP 功能范围

### 8.1 包含

- ✅ 地图可视化（React Flow）
- ✅ 节点详情面板（只读 + 元数据编辑）
- ✅ 关联创建（从 UI 选择类型与目标）
- ✅ 关联删除
- ✅ 元数据编辑（标题、标签、分组）
- ✅ 本地路径配置
- ✅ 关联类型颜色映射
- ✅ 节点位置自动保存

### 8.2 不包含（MVP 后）

- ❌ Markdown WYSIWYG 编辑器
- ❌ 附件上传 UI（手动放入文件夹即可）
- ❌ 全文本搜索（仅按标题/标签搜索）
- ❌ 多知识库切换
- ❌ 云同步
- ❌ 移动端适配

---

## 9. 错误处理

| 场景 | 行为 |
|---|---|
| 根目录不存在 | 提示明确错误，引导重新选择 |
| content.md 缺失 | 条目节点显示警告图标，可手动创建 |
| 注释行解析失败 | 跳过该条目，记录错误日志，不阻塞其他条目 |
| 关联目标不存在 | 节点显示虚线，悬停提示"目标不存在" |
| 关联类型无效 | 降级为自定义类型（灰色虚线） |
| 附件路径失效 | 详情面板显示占位符 |

---

## 10. 测试策略

| 层级 | 工具 | 范围 |
|---|---|---|
| 单元测试 | Rust 内置 + Vitest | parser、scanner、layout_store |
| 集成测试 | Tauri 测试 + Playwright | 完整启动流程 |
| E2E 测试 | Playwright | 地图渲染、关联创建 |

---

## 11. 风险与开放问题

| 风险 | 缓解 |
|---|---|
| Rust 学习曲线 | 后端逻辑以文件 IO + 解析为主，不涉及复杂算法 |
| Tauri 2.0 移动端稳定性 | MVP 仅桌面，移动端后续 |
| React Flow 大规模节点 | 虚拟化 + memo，已在 Kibana 案例验证 |
| 文件 watcher 跨平台差异 | notify crate 已处理 |

### 开放问题（待实现期决定）

- 是否需要支持配置文件（用户偏好）？建议：通过 Zustand persist 自动写入热路径 JSON
- 是否需要暗黑模式？建议：MVP 提供浅色 + 深色切换
- 节点形状是否可定制？建议：MVP 仅 rect + circle