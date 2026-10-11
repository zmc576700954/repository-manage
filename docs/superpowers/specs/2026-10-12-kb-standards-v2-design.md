# Synapse KB 规范 v2 设计方案

**代号**：KB 规范 v2（AI 生成 + 软件识别 + 用户维护三方闭环）
**日期**：2026-10-12
**状态**：待用户审阅
**关联文档**：
- [Synapse-KB 设计文档](./2026-10-09-synapse-kb-design.md)
- [国际化设计文档](./2026-10-11-i18n-design.md)
- [现有 standards.md](../../standards.md)（将被本规范替代/扩展）

---

## §0 目标

当前知识库由 AI 直接创建，存在四个痛点：
1. **命名/组织混乱**：分组名不一致（前端/frontend/前端混用）
2. **内容结构不统一**：格式五花八门，难以统一检索/阅读
3. **关联/关系缺失**：meta 不全、关系没建好，图表混乱
4. **统一目录结构能减少错误关联**

本规范实现**三方闭环**：
- **用户侧**：只需维护目录结构统一
- **AI 侧**：生成时遵守字段/标签/位置规则
- **软件侧**：创建文档时遵守同一规则 + 校验闭环

**产出**：规范文档 + 解析器代码修正 + 验证机制。

---

## §1 目录与 ID 契约

### 目录层级

```
kb-root/
├── {domain}/                          # 一级：领域（必填，单数）
│   ├── {subdomain}/                   # 二级：子领域（可选）
│   │   ├── {entry-id}/                # 三级：条目（可选）
│   │   │   ├── content.md             # 必须
│   │   │   ├── attachments/           # 推荐（本地附件）
│   │   │   └── notes/                 # 可选（块级补充）
│   │   └── ...
│   └── ...
└── .synapse-layout.json
```

**强制规则**：
- KB 根下直接放 `.md` 文件或一级目录之外的内容 = **违规**
- 任何条目必须在 1-3 层目录下；3 层以下禁止（不放 `level4/`）
- 路径分隔符强制 `/`；`\` 视为违规
- 命名完全放开（Unicode 允许），用户能识别即可；命名规范的执行由用户决定
- **`_inbox/` 例外**：未归类的条目可放在 KB 根下的 `_inbox/{entry-id}/`（下划线前缀）；不受 1-3 层目录约束，但仅一层

### ID 命名

- ID = 该条目目录的**完整相对路径**（用 `-` 拼接），与路径一一对应
  - 例：`frontend/react/hooks-basics` ←→ `frontend/react/hooks-basics/content.md`
- ID 全 KB 唯一；重名 = **违规**
- ID 不允许以下前缀：`attachments`、`notes`、`content`（保留名冲突）
- ID 长度上限 80 字符

### 文件名

- 条目目录必须叫 `{entry-id}/`，**不**允许 `react-hooks 副本/` 这类名字
- 唯一允许的非目录文件：`content.md`、`README.md`（KB 根下，仅说明性）
- `attachments/` 和 `notes/` 是保留子目录名，用户不应另作他用

---

## §2 标签约定

### `@synapse-tags`（必填）

**强制使用前缀分类**，值以逗号分隔：

```
<!-- @synapse-tags: type:concept, status:draft, topic:react, custom:hook-pattern -->
```

**4 个保留前缀**（机器可识别）：

| 前缀 | 取值范围 | 含义 |
|---|---|---|
| `type:` | `concept` / `tutorial` / `reference` / `note` / `project` / `question` | 条目类型（决定搜索/筛选权重） |
| `status:` | `draft` / `stable` / `deprecated` / `wip` | 成熟度（决定是否默认隐藏 deprecated） |
| `topic:` | 自由（推荐 kebab-case） | 主题域（与目录同语义但更灵活） |
| `custom:` | 自由 | 用户自定义 |

**规则**：
- 每个 entry **至少** 1 个 `type:` 标签（无 = 违规）
- `status:` 默认值 `stable`（不写时软件视为 stable）
- 单条 entry 标签总数 ≤ 15

### `@synapse-labels`（可选）—— 工作流标注

**与 `topic:` 区别**：
- `topic:` 是内容性（"react"、"hooks"）→ 搜索召回
- `labels` 是工作流性（"important"、"wip"）→ 快速筛选

**用户标签**（自由文本）：

| 值示例 | 用途 |
|---|---|
| `important` / `重要` | 重点关注 |
| `wip` / `待完善` | 撰写中 |
| `deprecated` / `遗弃` | 不再维护 |
| `done` / `已完成` | 收尾归档 |
| 自定义 | 用户自由发挥 |

**系统计算标签**（保留命名空间，软件自动维护）：

| 标签 | 触发条件 | 视觉 |
|---|---|---|
| `recently-modified` | mtime < 7 天 | 蓝色边 |
| `orphan` | 无任何 link 边 | 灰色边 |
| `stale` | mtime > 180 天 | 橙色边 |
| `broken-link` | 含指向不存在 id 的 `[[x]]` | 红色感叹号 |

用户**不能**手填系统标签（写也无效，软件覆盖）。

### 视觉映射（`.synapse-config.yaml`）

```yaml
# kb-root/.synapse-config.yaml
labels:
  important: "#FF6B6B"
  wip: "#FFB347"
  deprecated: "#888888"
  done: "#4CAF50"
  recently-modified: "#5B8DEF"  # 只读
  orphan: "#CCCCCC"             # 只读
  stale: "#FFA500"              # 只读
  broken-link: "#DC143C"        # 只读
```

未在此配置的标签 → 软件分配 hash 颜色（保证视觉稳定）。

### 筛选 UI（AND / OR 语义）

参考 Notion / Logseq：

- 标签内多选 = **OR**（`important` 或 `wip`）
- 不同规则间 = **AND**（标签 AND `recently-modified`）

---

## §3 字段表

### `@synapse-*` HTML 注释字段

| 字段 | 必填 | 写入方 | 默认值 | 语义 |
|---|---|---|---|---|
| `@synapse-id` | ✅ | AI/作者 | — | 条目 ID；必须等于路径派生 ID |
| `@synapse-title` | ✅ | AI/作者 | — | 人类可读标题 |
| `@synapse-tags` | ✅ | AI/作者 | `type:note` | 标签 csv |
| `@synapse-labels` | ❌ | AI/作者 | — | 工作流标注 |
| `@synapse-status` | ❌ | AI/作者 | `stable` | 覆盖 status: 标签 |
| `@synapse-created` | ❌ | 软件 | 文件 mtime | YYYY-MM-DD |
| `@synapse-updated` | ❌ | 软件 | 文件 mtime | 软件每次写入时刷新 |
| `@synapse-aliases` | ❌ | AI/作者 | — | 别名列表（兼容老链接） |
| `@synapse-source` | ❌ | AI/作者 | — | 来源标识（URL/书名） |
| `@synapse-ignore` | ❌ | 用户 | — | 忽略规则列表（如 `W5,W7`） |

**优先级规则**：
- `@synapse-status` 比 `status:` 标签优先；冲突时以注释为准
- `@synapse-id` 必须等于路径派生 ID；不一致 = 警告（W6 衍生）

### YAML frontmatter 字段

| 字段 | 类型 | 默认值 | 语义 |
|---|---|---|---|
| `color` | hex string | 派生自 `type:` | 节点颜色 |
| `shape` | enum | `rect` | `rect` / `circle` / `diamond` / `hexagon` |
| `pinned` | bool | `false` | 是否钉在画布固定位置 |
| `order` | int | 0 | 同 group 内排序 |
| `icon` | string | — | emoji 或单字符（用于节点角标） |
| `preview` | enum | `auto` | `auto` / `full` / `hidden` — DetailPanel 渲染策略 |
| `hidden` | bool | `false` | 是否从默认视图隐藏（deprecated 等） |

### 链接约定

| 形式 | 用途 | 解析结果 |
|---|---|---|
| `[[entry-id]]` | KB 内引用 | 创建 `reference` 边 |
| `[[entry-id#block-id]]` | 块级引用 | 创建 `reference` 边 + DetailPanel 定位 |
| `[[entry-id\|别名]]` | 带别名引用 | 创建 `reference` 边，渲染用别名 |

**显式关系声明**（行级，覆盖默认 `reference`）：

```markdown
@relation: derived [[source-entry]]
@relation: contradicts [[old-entry]]
```

**`@synapse-aliases`**：老链接 `[旧标题]` 或 `[[旧-id]]` 仍可解析；解析器先查 alias 表再查 ID。

---

## §4 AI 生成指令

### 4.1 顶层规则

```
你正在为一个 Synapse 知识库生成 entry。每个 entry 必须遵循以下规则：

1. 路径决定身份（软件不覆盖）
   - 你的 entry 的物理位置 = 它的身份
   - @synapse-id 必须等于路径派生 ID（即路径的 / 替换为 -）
   - 软件会校验二者一致

2. 必填字段
   - @synapse-id：你直接写，等于路径派生 ID
   - @synapse-title：人类可读标题，必填
   - @synapse-tags：至少包含 1 个 type: 前缀标签

3. 标签前缀使用规范
   - type: 决定条目性质（concept/tutorial/reference/note/project/question）
   - status: 成熟度（draft/stable/deprecated/wip），默认 stable
   - topic: 主题域（与目录语义重叠但更灵活）
   - custom: 用户自定义
   - 标签总数 ≤ 15

4. labels（可选）
   - @synapse-labels：用于工作流标注（important/wip/done/deprecated 等）
   - 不要写系统保留标签（recently-modified/orphan/stale/broken-link）
```

### 4.2 内容生成规则

```
5. 链接语义
   - 同 KB 内引用用 [[entry-id]]，自动建立 reference 边
   - 跨 KB 或外部资源用 ![caption](./attachments/file.png)
   - 不要用 [text](entry-id.md) 这种相对路径链接同 KB 条目
   - 派生/反驳/替代关系用行级声明：@relation: derived [[source]]

6. 字段填写要点
   - title：简洁、动名词或名词短语，避免问号
   - tags：3-7 个最佳，过多反而难筛
   - labels：只在确实需要标注时写，不要为每个 entry 都打满

7. 不知道放哪时
   - 暂时无法归类 → 放在 kb-root/_inbox/{entry-id}/（下划线前缀表示未分类）
   - 不要为单个 entry 单独创建新领域
```

### 4.3 禁止行为

```
- 不要写系统保留 labels（recently-modified 等）
- 不要创建超过 3 层的目录
- 不要使用 attachments/notes/以外名称保留目录
- 不要省略 @synapse-title 或 @synapse-tags
- 不要写 .md 文件到 KB 根（除了 content.md 之外）
```

### 4.4 示例（few-shot）

**正确示例**：

```
# frontend/react/hooks-basics/content.md

<!-- @synapse-id: frontend-react-hooks-basics -->
<!-- @synapse-title: React Hooks 基础模式 -->
<!-- @synapse-tags: type:tutorial, topic:react, topic:hooks -->
<!-- @synapse-labels: important -->

# Hooks 基础模式

useState 是 React 最常用的 hook...

参见 [[frontend-react-state-management]] 和 [[frontend-react-effect-lifecycle]]。

## 常见问题

- 闭包陷阱
- 依赖数组
```

**错误示例**：

```
# 错误：放到根目录
kb-root/random-notes.md  ← 违规

# 错误：缺少必填字段
<!-- @synapse-title: Foo -->  ← 缺 tags，违规
```

---

## §5 校验规则

### 5.1 校验级别

| 级别 | 含义 | 行为 |
|---|---|---|
| `error` | 必填缺失 / 路径与 ID 不一致 / 目录层级超限 | 校验失败，CLI 退出非 0，UI 红条提示 |
| `warning` | 软建议（如 `type:` 不规范、孤立 entry） | 列出但不阻断 |
| `info` | 系统计算项（如最近修改、孤立节点） | 仅展示 |

### 5.2 错误级（阻断）

| 规则 | 检查内容 |
|---|---|
| E1 | `@synapse-id` 必填且非空 |
| E2 | `@synapse-id` 与路径派生 ID 完全一致 |
| E3 | `@synapse-title` 必填且非空 |
| E4 | `@synapse-tags` 必填且至少 1 个 `type:` 前缀 |
| E5 | 目录层级 ≤ 3（除 `_inbox` 外） |
| E6 | KB 根下不允许有 `.md` 文件（`content.md` / `README.md` 例外） |
| E7 | 保留子目录名（`attachments`、`notes`）只能用于约定用途 |
| E8 | ID 全 KB 唯一（不能两个目录的派生 ID 相同） |
| E9 | 系统保留 labels（`recently-modified` / `orphan` / `stale` / `broken-link`）不应被用户写入 |

### 5.3 警告级（不阻断）

| 规则 | 检查内容 |
|---|---|
| W1 | YAML 解析失败（frontmatter 损坏） |
| W2 | 标签总数 > 15 |
| W3 | 标签值含非法字符（除 `custom:` 外，规范仅 `[a-z0-9-]`） |
| W4 | `[[broken-id]]` 指向不存在的 ID |
| W5 | entry 无任何 `[[link]]`（孤立节点候选） |
| W6 | `@synapse-group` 老字段被忽略（迁移提示） |
| W7 | `attachments/` 目录存在但 content.md 无 `![[./attachments/...]]` 引用（孤儿附件） |

### 5.4 信息级（展示）

| 规则 | 内容 |
|---|---|
| I1 | `recently-modified`：mtime < 7 天 |
| I2 | `orphan`：无任何 link 边 |
| I3 | `stale`：mtime > 180 天 |
| I4 | `broken-link`：含未解析 `[[id]]` |

### 5.5 输出格式（CLI）

```
$ synapse validate [KB_PATH]

Scanning kb-root/...
[ERROR] E2 frontend/react/hooks-basics/content.md: @synapse-id "hooks-basics" ≠ path-derived "frontend-react-hooks-basics"
[ERROR] E4 backend/auth/oauth/content.md: missing @synapse-tags
[ERROR] E8 ID collision: "react-intro" appears in both frontend/react/intro/ and learning/react-intro/
[WARN] W4 backend/auth/oauth/content.md: [[legacy-flow]] target not found
[WARN] W5 frontend/react/hooks-basics/content.md: no outgoing links (orphan candidate)
[INFO] I1 12 entries recently modified
[INFO] I2 3 orphan entries

Summary: 3 errors, 2 warnings, 2 info
Exit code: 1
```

### 5.6 UI 校验展示（IDE 风格）

#### 两层高亮

**L1：画布节点**

| 状态 | 视觉 |
|---|---|
| 无错误 | 默认节点样式 |
| 有 warning | 黄色边框 + 右上角小三角图标 |
| 有 error | 红色边框 + 右上角感叹号；节点轻微脉冲动画 |
| 全部忽略 | 灰色边框 + 对勾图标 |

鼠标悬停图标 → tooltip 列出具体问题。

**L2：侧边栏条目列表**

每条 entry 行末显示小圆点：
- 无 = 干净
- 黄点 = 有 warning
- 红点 = 有 error

点击红点 → Validation 面板聚焦该 entry 错误。

**L3（可选）**：DetailPanel 内联高亮——本期**不做**，由右侧 Validation 面板承担（§9.5）。

#### 强制确认机制

**Error 级**：阻断操作（保存新版本、创建关系边、拖动节点）
- 弹窗：「此 entry 有 X 个错误，必须修复后才能继续」
- 仅「查看错误详情」和「取消」；无强制忽略入口

**Warning 级**：不阻断，提供三个动作：
- 「修复」（跳到错误位置）
- 「忽略」（写入 `@synapse-ignore`，本条目不再警告）
- 「忽略本次」（仅本会话）

#### 忽略持久化

```markdown
<!-- @synapse-id: foo -->
<!-- @synapse-ignore: W5, W7 -->
```

KB 级忽略写在 `.synapse-config.yaml`：

```yaml
validation:
  ignored_rules:
    - W5
```

### 5.7 工具形态

- **CLI**：`synapse validate [path]` — 给 CI / 脚本 / 用户手动跑
- **UI 内置**：应用启动时静默跑 error 级；error > 0 时顶部红条

---

## §6 解析器改动（Rust 端）

### 6.1 新增 / 修改的类型

```rust
// src-tauri/src/entry.rs

pub struct RawEntry {
    // 现有字段保留
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub path: PathBuf,
    pub content_path: PathBuf,
    pub attachments: Vec<RawAttachment>,
    pub linked_targets: Vec<String>,
    pub markdown_links: Vec<MarkdownLink>,
    pub yaml: Option<YamlFields>,
    pub has_content_md: bool,

    // 新增
    pub labels: Vec<String>,           // 替代原 group
    pub status: Option<String>,        // @synapse-status
    pub aliases: Vec<String>,          // @synapse-aliases
    pub source: Option<String>,        // @synapse-source
    pub created: Option<String>,       // @synapse-created 或 mtime
    pub updated: Option<String>,       // @synapse-updated 或 mtime
    pub ignored_rules: Vec<String>,    // @synapse-ignore
    pub validation: Vec<ValidationIssue>,
}
```

### 6.2 新增 / 修改的解析函数

| 函数 | 路径 | 说明 |
|---|---|---|
| `parse_metadata_comments` | `parser/comments.rs` | 扩展识别新键 |
| `extract_labels(content)` | `parser/comments.rs` | 新增；解析 `@synapse-labels` csv |
| `extract_relations(content)` | `parser/links.rs` | 新增；解析 `@relation:` 行级声明 |
| `validate_entry(entry)` | `validator/mod.rs` | 新增；返回 `Vec<ValidationIssue>` |
| `compute_system_labels(entries)` | `validator/system_labels.rs` | 新增 |

### 6.3 校验引擎骨架

```rust
// src-tauri/src/validator/mod.rs

pub enum Severity { Error, Warning, Info }

pub struct ValidationIssue {
    pub entry_id: String,
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub file_line: Option<u32>,
    pub ignored: bool,
}

pub fn validate_entry(
    entry: &RawEntry,
    kb_root: &Path,
    all_entries: &[RawEntry],
) -> Vec<ValidationIssue> {
    let mut issues = vec![];
    issues.extend(check_e1_e9_required(entry, kb_root));
    issues.extend(check_w1_w7_warnings(entry, all_entries));
    issues
}
```

### 6.4 迁移策略（`@synapse-group` → `@synapse-labels`）

**分级启发式迁移**：

1. 值匹配 `^[a-z0-9-]+$` → 转 label
2. 值含 `/` → 保留原 `@synapse-group`，标 W6
3. 值含 Unicode（非 ASCII 字母数字） → 转 label
4. 值匹配 `^\d+(\.\d+)*$`（纯版本号） → 保留 + W6
5. 其他 → 保留 + W6

**安全机制**：
- `--dry-run` 先打 diff
- 备份到 `.synapse-migrate-backup-{timestamp}/`
- 失败任意一条 → 整体回滚
- 写 `.synapse-migrate.log`

**降级路径**：若启发式覆盖率 < 80%，不做自动迁移，走静默忽略 + W6 + 手动脚本。

### 6.5 新增 Tauri 命令

```rust
#[tauri::command]
pub fn validate_kb(kb_root: PathBuf) -> Result<ValidationReport, AppError>;

#[tauri::command]
pub fn validate_entry(entry_dir: PathBuf) -> Result<Vec<ValidationIssue>, AppError>;

#[tauri::command]
pub fn get_system_labels(kb_root: PathBuf) -> Result<Vec<SystemLabel>, AppError>;

#[tauri::command]
pub fn run_validation(kb_root: PathBuf) -> Result<ValidationReport, AppError>;

#[tauri::command]
pub fn get_kb_config(kb_root: PathBuf) -> Result<KbConfig, AppError>;

#[tauri::command]
pub fn update_kb_config(kb_root: PathBuf, config: KbConfig) -> Result<(), AppError>;
```

### 6.6 文件改动清单

| 文件 | 改动 |
|---|---|
| `src-tauri/src/parser/comments.rs` | 扩展 + 新增 `extract_labels` |
| `src-tauri/src/parser/links.rs` | 新增 `extract_relations` |
| `src-tauri/src/parser/yaml_field.rs` | `YamlFields` 新增 `icon` / `preview` / `hidden` |
| `src-tauri/src/entry.rs` | `RawEntry` 结构扩展 |
| `src-tauri/src/validator/mod.rs` | **新建** |
| `src-tauri/src/validator/system_labels.rs` | **新建** |
| `src-tauri/src/validator/rules.rs` | **新建** |
| `src-tauri/src/index.rs` | SQLite schema 加 `labels` 列 |
| `src-tauri/src/commands.rs` | 新增 6 个 `#[tauri::command]`（§6.5） |
| `src-tauri/src/lib.rs` | 注册新命令 |

### 6.7 Rust 单元测试覆盖

- `comments::parses_new_fields`
- `comments::parses_labels_csv`
- `links::extract_relations_parses_line_declarations`
- `validator/rules::e2_id_matches_path`
- `validator/rules::e4_tags_has_type_prefix`
- `validator/rules::e8_unique_ids`
- `validator/rules::w4_broken_link_targets`
- `validator/system_labels::recently_modified_within_seven_days`
- `validator/system_labels::orphan_no_links`
- `migrator::labels_dry_run_does_not_modify_files`
- `migrator::labels_heuristics_branches`

---

## §7 前端改动

### 7.1 Store 扩展

```ts
// src/store/useAppStore.ts 扩展
interface ValidationState {
  report: ValidationReport | null;
  panelOpen: boolean;
  filterLabels: Set<string>;
  filterMode: 'OR' | 'AND';
}
```

新增 `panelStore`（Zustand）管理右侧多面板开关。

### 7.2 组件改动

| 组件 | 改动 |
|---|---|
| `NodeCard.tsx` | 红/黄边样式 + 感叹号 / 三角图标 |
| `Sidebar.tsx` | 条目行末红/黄点 + 筛选面板 trigger |
| `MapView.tsx` | 节点过滤逻辑（按 filterLabels） |
| `Settings.tsx` | 改为右侧面板（§9.5） |
| `RightPanelStack.tsx` | **新建**：多面板垂直堆叠容器 |
| `RightPanel.tsx` | **新建**：单个面板壳 |
| `ValidationPanel.tsx` | **新建**：校验报告查看器 |
| `FilterPanel.tsx` | **新建**：标签筛选面板 |

### 7.3 Tauri 事件订阅

```ts
import { listen } from '@tauri-apps/api/event';

useEffect(() => {
  const unlisten = listen('validation:updated', (event) => {
    setValidationReport(event.payload);
  });
  return unlisten;
}, []);
```

---

## §8 国际化整合

### 8.1 新增命名空间

| 命名空间 | 内容 | 触发位置 |
|---|---|---|
| `Validation` | 规则消息（E1-E9 / W1-W7）、错误描述、严重度名 | 校验面板、错误明细 |
| `Filter` | 标签多选筛选 UI | 顶部筛选工具栏 |

### 8.2 现有命名空间扩展

| 命名空间 | 新增 key |
|---|---|
| `Settings` | `labels.*` / `validation.*` / `migration.*` |
| `Sidebar` | `entry.errorCount` / `entry.warningCount` / `filter.openPanel` |
| `NodeCard` | `node.errorBadge` / `node.warningBadge` / `node.ignoreTooltip` |

### 8.3 资源结构示例

```jsonc
// Validation.zh.json
{
  "severity": { "error": "错误", "warning": "警告", "info": "提示" },
  "rule": {
    "E1": { "title": "缺少 ID", "message": "@synapse-id 不能为空" },
    "E2": { "title": "ID 与路径不一致", "message": "@synapse-id \"{{id}}\" 应等于路径派生 \"{{expected}}\"" },
    "E4": { "title": "缺少类型标签", "message": "@synapse-tags 至少需要一个 type: 前缀" },
    "W4": { "title": "悬空链接", "message": "[[{{target}}]] 目标不存在" }
  },
  "action": {
    "viewDetails": "查看错误详情",
    "fix": "修复",
    "ignore": "忽略",
    "ignoreOnce": "本次忽略",
    "ignoreAlways": "永久忽略"
  },
  "systemLabel": {
    "recently-modified": { "display": "最近修改" },
    "orphan": { "display": "孤立条目" },
    "stale": { "display": "长期未更新" },
    "broken-link": { "display": "断链" }
  }
}
```

英文版结构相同，文案换成英文。

### 8.4 规则 ID 呈现策略

**关键决策**：规则 ID（E1、W4 等）**保持不翻译**；面向用户的标题/消息翻译。

```
┌────────────────────────────────────────┐
│ ⚠ W4: 悬空链接                         │ ← ID 不翻译，标题翻译
│ [[legacy-flow]] 目标不存在              │ ← 消息翻译
│                              [修复] [忽略] │
└────────────────────────────────────────┘
```

理由：用户反馈 bug 时引用 "W4" 比引用中文标题更稳定。

### 8.5 系统标签的本地化

存储 key 不变（机器用），UI 显示翻译（§8.3 `systemLabel` 段）。

### 8.6 翻译维护流程

- 改校验规则 → 同时改 `Validation.zh.json` + `Validation.en.json`（**双文件必同步**）
- CI 校验：两个文件 key 集合必须一致；缺失 → 报错
- 工具函数 `tRule(issue)` 自动根据 `issue.rule_id` + 当前语言返回 `{ title, message }`

### 8.7 不翻译的内容

| 内容 | 不翻译原因 |
|---|---|
| 字段名（`@synapse-id` 等） | 文件格式标识，全 KB 一致 |
| 标签前缀（`type:` / `status:` 等） | 同上 |
| 目录名、ID | 用户自填内容 |
| content.md 正文 | 用户自有内容 |
| 规则 ID（E1 / W4 等） | bug 反馈稳定引用 |

---

## §9 Settings 面板扩展

### 9.1 右侧多面板架构

```
┌─Sidebar─┬──────Main──────┬────RightPanelStack────┐
│  KB     │                │  ┌──────────────────┐ │
│  list   │   MapView /    │  │ Panel A          │ │
│         │   DetailPanel  │  ├──────────────────┤ │
│         │                │  │ Panel B          │ │
│         │                │  └──────────────────┘ │
└─────────┴────────────────┴────────────────────────┘
```

- 多个面板上下堆叠，最新在最上
- 总宽固定，可拖拽分隔条调宽
- 每面板右上角 ✕ 关闭
- 状态写入 `panelStore`（不持久化，重启默认全部关闭）

### 9.2 Settings 改为右侧面板

**替代原 modal**，与其他面板可同时打开。

### 9.3 Labels 分区

```
┌─ Labels ─────────────────────────┐
│ 自定义标签（点击色块改色）        │
│                                    │
│  ● important   [#FF6B6B ▾]         │
│  ● wip         [#FFB347 ▾]         │
│  ● deprecated  [#888888 ▾]         │
│  ● done        [#4CAF50 ▾]         │
│  ＋ 添加新标签                     │
│                                    │
│ ────────────                       │
│ 系统标签（只读）                   │
│                                    │
│  ● recently-modified  #5B8DEF      │
│  ● orphan             #CCCCCC      │
│  ● stale              #FFA500      │
│  ● broken-link        #DC143C      │
└────────────────────────────────────┘
```

数据流：标签列表从 KB 扫描得出 → 用户改色写入 `.synapse-config.yaml`。

### 9.4 Validation 分区

```
┌─ Validation ──────────────────────┐
│ 当前 KB 状态                       │
│   ● 12 errors   ● 5 warnings       │
│   [打开报告面板]  [重新校验]       │
│                                     │
│ 校验规则开关（全局）               │
│   ☑ 警告孤立节点 (W5)              │
│   ☑ 警告未用附件 (W7)              │
│   ☑ 警告断链 (W4)                  │
│                                     │
│ 忽略规则（per-entry）              │
│   共 3 个条目设置了忽略规则         │
│   [查看详情]                       │
└─────────────────────────────────────┘
```

### 9.5 Migration 分区（仅老 KB 显示）

**触发条件**：检测到 KB 内任何 `content.md` 含 `@synapse-group` 时显示。

```
┌─ Migration ──────────────────────┐
│ ⚠ 检测到老的 @synapse-group 字段 │
│                                    │
│ 影响范围：                         │
│   5 个条目使用了 @synapse-group   │
│                                    │
│ [查看详情] [预览迁移] [执行迁移]  │
└────────────────────────────────────┘
```

执行流程：预览 diff → 用户确认 → 备份 + 改写 + log。

### 9.6 Validation 报告查看器（右侧面板）

```
┌─Validation ──────────────[✕]─┐
│ [Errors 12] [Warnings 5]      │
│ [Info 23]    [Rescan]         │
├──────────────────────────────┤
│ frontend/react/hooks-basics  │
│   E2 @synapse-id 不一致       │
│   W4 [[legacy-flow]] 目标不存在│
│                              │
│ backend/auth/oauth           │
│   E4 缺少 @synapse-tags       │
│   E8 ID 冲突                  │
├──────────────────────────────┤
│ [导出 JSON] [跳到 entry]     │
└──────────────────────────────┘
```

### 9.7 Tauri 命令配套

```rust
#[tauri::command]
pub fn get_kb_config(kb_root: PathBuf) -> Result<KbConfig, AppError>;

#[tauri::command]
pub fn update_kb_config(kb_root: PathBuf, config: KbConfig) -> Result<(), AppError>;

#[tauri::command]
pub fn run_validation(kb_root: PathBuf) -> Result<ValidationReport, AppError>;

#[tauri::command]
pub fn get_validation_report(kb_root: PathBuf) -> Result<ValidationReport, AppError>;
```

### 9.8 边界条件

| 场景 | 行为 |
|---|---|
| KB config 文件不存在 | 自动创建默认（仅 system labels） |
| 用户写非法 YAML | 解析失败 → 显示原文件 + 错误提示，不覆盖 |
| 校验运行超时（>5s） | 显示进度；允许取消 |
| 多 KB 切换 | 当前 KB config 跟着切；不污染其他 KB |

---

## §10 CLI 工具形态

### 10.1 选型

**推荐**：独立 Rust 二进制 + 与 app 共享 lib（synapse_lib）。两者兼顾 CI 友好与代码复用。

### 10.2 工程结构

```
src-tauri/
├── src/
│   ├── bin/
│   │   └── synapse.rs          # CLI 入口（独立二进制）
│   ├── lib.rs                  # 共享库（被 Tauri app 与 CLI 共同使用）
│   ├── ...
│   └── (parser / validator / index / kb_registry / entry 共用)
├── Cargo.toml                  # [[bin]] 段配置
```

`Cargo.toml`：

```toml
[[bin]]
name = "synapse"
path = "src/bin/synapse.rs"

[[bin]]
name = "synapse-app"
path = "src/main.rs"
```

### 10.3 CLI 命令集

```bash
synapse validate <KB_PATH>              # 校验，返回报告
synapse validate <KB_PATH> --json       # 输出 JSON（CI / 脚本）
synapse validate <KB_PATH> --severity error   # 只输出 error 级

synapse migrate labels <KB_PATH>        # 执行迁移
synapse migrate labels <KB_PATH> --dry-run   # 预览
synapse migrate labels <KB_PATH> --report    # 列出所有含 @synapse-group 的文件

synapse validate-all                     # 扫所有 KB

synapse scan <KB_PATH>                   # 列出所有 entry（调试）
synapse entry show <KB_PATH> <ID>        # 显示单个 entry 元数据

synapse config get <KB_PATH>             # 显示 .synapse-config.yaml
synapse config set <KB_PATH> ...         # 修改 config
```

### 10.4 实现要点

```rust
// src/bin/synapse.rs
use synapse_lib::{parser, validator, index, kb_registry};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Validate { path, json, severity } => {
            let report = validator::validate_kb(&path)?;
            print_report(report, json, severity)?;
            if report.has_errors() { std::process::exit(1); }
        }
        Commands::MigrateLabels { path, dry_run, report } => {
            migrator::migrate_labels(&path, dry_run, report)?;
        }
    }
    Ok(())
}
```

**关键约束**：CLI 与 Tauri 共用同一份 `synapse_lib::validator`，避免规则漂移。

### 10.5 输出格式

**默认（人类可读）**：见 §5.5。

**`--json`**（机器可读）：

```json
{
  "kb_root": "/Users/foo/kb",
  "summary": { "errors": 3, "warnings": 2, "info": 2 },
  "issues": [
    {
      "entry_id": "frontend-react-hooks-basics",
      "rule_id": "E2",
      "severity": "error",
      "message": "...",
      "file": "frontend/react/hooks-basics/content.md",
      "line": 2
    }
  ]
}
```

**Exit code**：
- 0 = 无 error
- 1 = 有 error
- 2 = 内部错误

### 10.6 CI 集成示例

```yaml
# .github/workflows/kb-lint.yml
name: KB Lint
on: [push, pull_request]
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install --path src-tauri --bin synapse --locked
      - run: synapse validate ./kb --severity error
```

---

## §11 校验引擎性能

### 11.1 性能目标

| KB 规模 | 入口数 | 启动校验耗时 | 增量校验耗时 |
|---|---|---|---|
| 小 | <50 | <100ms | <20ms |
| 中 | 50-500 | <500ms | <50ms |
| 大 | 500-5000 | <2s | <200ms |
| 超大 | >5000 | <5s | <500ms |

**总原则**：启动不卡，编辑实时反馈。

### 11.2 校验触发时机（5 个钩子点）

| 钩子 | 触发 | 范围 | 频率 |
|---|---|---|---|
| H1: 启动 | `index.rebuild()` 之后 | 全 KB | 每次启动一次 |
| H2: 文件变更 | fsevents / notify watcher | 单 entry | 编辑即跑 |
| H3: AI 生成 | Tauri 命令 `create_entry` | 单 entry | 写入即跑 |
| H4: 手动触发 | 用户点「重新校验」 | 全 KB | 按需 |
| H5: 关系建立 | `add_relation` 命令 | 两端 entry | 边写入即跑 |

### 11.3 增量校验算法

**关键**：H2/H3/H5 不重跑全量，只校验受影响 entry。

```rust
fn validate_single_entry(
    entry: &RawEntry,
    kb_root: &Path,
    all_ids: &HashSet<String>,
    ignore_rules: &HashMap<String, Vec<String>>,
) -> Vec<ValidationIssue>;
```

**共享上下文**（启动时构建一次）：

| 数据 | 大小估计（5000 entry） |
|---|---|
| `all_ids: HashSet<String>` | ~100KB |
| `id_to_path: HashMap<String, PathBuf>` | ~500KB |
| `inverse_links: HashMap<String, Vec<String>>` | ~200KB |
| `mtime_index: HashMap<String, SystemTime>` | ~500KB |

### 11.4 系统标签计算

需要全图视角，单独批处理：

```rust
fn compute_system_labels(entries: &[RawEntry]) -> Vec<(String, SystemLabel)> {
    // recently-modified: mtime < 7 天
    // orphan: linked_targets.is_empty() && incoming_links.is_empty()
    // stale: mtime > 180 天
    // broken-link: linked_targets 中含 all_ids 不存在的 id
}
```

### 11.5 文件监听

```rust
use notify::{Watcher, RecursiveMode};

let (tx, rx) = std::sync::mpsc::channel();
let mut watcher = notify::recommended_watcher(tx)?;
watcher.watch(kb_root, RecursiveMode::Recursive)?;

// 后台线程：debounce 500ms → 触发增量校验
```

### 11.6 与 `index.rebuild` 集成

```rust
async fn startup(state: State<'_, AppState>) -> Result<(), AppError> {
    let kb_root = state.kb_registry.active_kb_path()?;
    index::rebuild(&kb_root, &state.index)?;
    let report = validator::validate_kb(&kb_root)?;
    *state.validation_report.write().await = Some(report);
    Ok(())
}
```

### 11.7 大 KB 优化（>5000 entry 启用，本期不做）

- 并行解析（rayon）：启动 -50%
- 共享上下文 mmap：内存 -30%
- 校验结果持久化到 SQLite：重启 -90%

### 11.8 边界

| 场景 | 行为 |
|---|---|
| KB 巨大（10000 entry）启动 | 进度条「校验中... X/Y」 |
| watcher 事件风暴 | debounce + 队列上限（最近 100 个） |
| 校验跑一半用户关闭 | 完成部分保留；下次启动补全 |
| 校验超时（>5s） | 允许取消；保留旧报告 |

---

## §12 多 KB 场景

### 12.1 KB 隔离原则

**核心规则**：每个 KB 完全独立校验，不跨 KB 检查。

| 项 | 范围 |
|---|---|
| ID 唯一性（E8） | per-KB；不同 KB 可有同名 ID |
| ID 与路径匹配（E2） | per-KB |
| 标签前缀 | per-KB；标签配置在 KB config |
| 系统标签 | per-KB |
| `_inbox/` | per-KB；不共享 |
| 校验忽略规则 | per-KB |

### 12.2 KB 切换时的校验

```rust
#[tauri::command]
pub async fn switch_kb(state: State<'_, AppState>, kb_id: String)
    -> Result<SwitchKbResult, AppError>
{
    state.kb_registry.set_active(&kb_id)?;
    let kb_root = state.kb_registry.active_kb_path()?;
    let entries = index::rebuild(&kb_root, &state.index)?;
    let report = validator::validate_kb(&kb_root)?;
    *state.validation_report.write().await = Some(report.clone());
    Ok(SwitchKbResult { entries: ..., validation_report: report })
}
```

### 12.3 跨 KB 链接（本期不实现）

**`[[entry-id]]` 仅解析当前 KB**。本期规范只规定 `@synapse-cross-kb` 字段，解析器静默忽略。

### 12.4 KB 级 vs 全局配置

| 配置 | 存储位置 |
|---|---|
| 标签颜色 | per-KB |
| 校验规则开关 | per-KB |
| 忽略规则（per-entry） | per-entry 注释 |
| 忽略规则（per-KB） | per-KB config |
| i18n 语言偏好 | 全局 |
| AI 生成规则片段 | 全局（`~/.synapse/prompts/kb-gen.md`） |

### 12.5 KB 注册表的校验元数据

```rust
pub struct KbSummary {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub entry_count: usize,
    pub last_validated: Option<SystemTime>,
    pub error_count: usize,
    pub warning_count: usize,
}
```

**用途**：Sidebar 顶部 KB 切换器显示每 KB 健康状态。

### 12.6 边界

| 场景 | 行为 |
|---|---|
| 用户添加新 KB | 触发首次校验 H1 |
| KB 目录被删除 | 注册表标「不可访问」；UI 灰显 |
| 同一 KB 多个实例打开 | 文件锁 / 提示冲突；只读模式 |
| KB 路径含符号链接 | canonicalize 规范化 |

---

## §13 冲突解决

### 13.1 字段所有权矩阵

| 字段 | 归属 | 谁写 | 软件能改吗 |
|---|---|---|---|
| `@synapse-id` | 用户/AI | AI/作者 | 仅校验不覆盖 |
| `@synapse-title` | 用户/AI | AI/作者 | 否 |
| `@synapse-tags` | 用户/AI | AI/作者 | 否 |
| `@synapse-labels` | 用户/AI | AI/作者 | 否 |
| `@synapse-status` | 用户/AI | AI/作者 | 否 |
| `@synapse-aliases` | 用户/AI | AI/作者 | 否 |
| `@synapse-source` | 用户/AI | AI/作者 | 否 |
| `@synapse-ignore` | 用户 | 用户 | 否 |
| `@synapse-created` | 软件 | 软件 | 仅当文件 mtime 早于 created 时 |
| `@synapse-updated` | 软件 | 软件 | 每次写入刷新 |
| 文件 mtime | OS | OS/任何编辑器 | — |

### 13.2 外部编辑检测

```rust
struct FileSnapshot {
    mtime: SystemTime,
    content_hash: u64,  // xxhash
}
```

**机制**：watcher + mtime + content hash 对比。

### 13.3 编辑器 vs 软件写入

软件只动 `@synapse-updated`（+ 仅在缺时补 `@synapse-created`）；绝不修改用户/AI 字段。

### 13.4 AI 生成的并发

```rust
// 全局 KB 级锁
let kb_lock = state.kb_locks.entry(kb_id).or_insert_with(|| Mutex::new(()));
let _guard = kb_lock.lock().await;
```

不同 KB 可并行；同一 KB 串行。

### 13.5 目录重命名

**UI 内拖拽**：自动同步 ID 与路径。
**外部改名（Finder / mv）**：仅提示，不自动改 ID：

```
⚠ 检测到路径变化
  旧路径: frontend/react/hooks-basics/
  新路径: frontend/react/hooks-basics-v2/
  @synapse-id "react-hooks" 与新路径 "frontend-react-hooks-basics-v2" 不一致
  [更新 ID 以匹配新路径]  [回滚路径]  [忽略]
```

### 13.6 ID 冲突（E8）

- 校验跑 E8 → 报错
- UI：两个节点都标红边
- 用户手动改名 / 「自动加后缀」按钮（`-2` / `-3` ...）

### 13.7 用户删文件

- watcher → DB 对账 → 标记孤儿
- entry 节点变灰、半透明
- Sidebar 条目保留但标 ⚠「文件已删除」
- 30 天后仍孤儿 → 提供「清理」按钮

### 13.8 多端冲突（本期不实现）

仅单端单进程。spec 留位文件锁 / CRDT 等未来扩展。

---

## §14 YAGNI

### 14.1 不实现的解析/校验功能

| 项 | 不做的理由 |
|---|---|
| 跨 KB 链接解析 | KB 完全独立 |
| 自动派生 `title` from H1 | 用户/AI 写更可靠 |
| 标签自动补全/推荐 | AI 行为 |
| YAML schema 严格校验 | parser 已容错 |
| 内容质量打分 | 主观 |
| 自动修复 broken-link | 风险高 |

### 14.2 不做的 UI 功能

- L3 内联高亮（Validation 面板已独立承担，§9.5）
- Validation 面板内嵌编辑器
- 校验历史趋势图
- 自动播放校验进度动画

### 14.3 不做的高级校验

- 内容查重
- HTTP 死链探测
- AI 写作品质评估
- 拼写/语法检查
- 命名规范实时建议

### 14.4 不做的多端/协作

- 多设备同步
- 文件锁
- Git 自动 commit
- 协作编辑（CRDT/OT）

### 14.5 不做的元功能

- 规则自定义（用户加新规则）
- 校验引擎插件系统
- 远程校验
- 历史快照 / 回滚（交给 Git）

### 14.6 不做的全局开关

启动自动校验、watcher、系统标签计算、ID 路径一致性校验——**全部强制开启**，不留开关。

---

## §15 验收清单

### 15.1 Rust 单元测试

见 §6.7 列表（11 个核心测试）。实施时可按需拆分 helper 与边界用例，但最低覆盖范围以此为准。

### 15.2 手工验收清单（32 条）

#### 解析器与字段（#1-8）

| # | 操作 | 期望 |
|---|---|---|
| 1 | 新建 entry 模板，跑 `synapse validate` | 全绿 |
| 2 | 删 `@synapse-title` | E3 报错 |
| 3 | 改 `@synapse-id` 与路径不一致 | E2 报错 |
| 4 | 删 `@synapse-tags` | E4 报错 |
| 5 | 写 `@synapse-labels: important, wip` | 解析成功 |
| 6 | 写 `@synapse-labels: recently-modified` | E9 报错 |
| 7 | 写 `@relation: derived [[x]]` | 解析为 `derived` 关系 |
| 8 | 写老 `@synapse-group: foo` | 静默忽略 + W6 警告 |

#### 校验 UI（#9-14）

| # | 操作 | 期望 |
|---|---|---|
| 9 | 故意写一个 E4 错误的 entry | 该节点红边 + 感叹号 |
| 10 | 悬停感叹号 | tooltip 显示规则详情 |
| 11 | 顶部红条显示「X 个错误」 | 点击打开 Validation 面板 |
| 12 | Validation 面板点「跳到 entry」 | 地图聚焦该节点 |
| 13 | 故意制造 W4 警告 | 该节点黄边 + Sidebar 黄点 |
| 14 | Settings → Validation 改 ignore 列表 | 重新校验后对应警告消失 |

#### 标签筛选（#15-18）

| # | 操作 | 期望 |
|---|---|---|
| 15 | 筛选器勾 `important` | 只显示带此 label 的节点 |
| 16 | 多选 `important` + `wip` | OR 语义，两者皆显示 |
| 17 | 勾 `recently-modified` | 显示 mtime < 7 天的节点 |
| 18 | 清空筛选 | 全部显示 |

#### 迁移（#19-22）

| # | 操作 | 期望 |
|---|---|---|
| 19 | `synapse migrate labels ./kb --dry-run` | 显示 diff 不改文件 |
| 20 | `synapse migrate labels ./kb` | 备份目录生成 + 文件改写 + log 写 |
| 21 | 迁移后跑 `synapse validate ./kb` | 无 W6 警告 |
| 22 | 故意把 `group` 写成 `version/1.0` | 启发式保留为 W6，不自动转 |

#### 多 KB（#23-25）

| # | 操作 | 期望 |
|---|---|---|
| 23 | 添加新 KB | 切换器显示该 KB + entry 数 |
| 24 | 切到有错误的 KB | 顶部红条更新 |
| 25 | `synapse validate-all` | 列出所有 KB 的错误数 |

#### i18n（#26-28）

| # | 操作 | 期望 |
|---|---|---|
| 26 | 切到英文 | 所有新文案变英文 |
| 27 | 切到中文 | 变中文 |
| 28 | 删 `Validation.zh.json` 一个 key | fallback 英文，不崩 |

#### 性能（#29-32）

| # | 操作 | 期望 |
|---|---|---|
| 29 | 生成 100 个 entry | 启动校验 <500ms |
| 30 | 生成 1000 个 entry | 启动校验 <2s |
| 31 | 编辑一个 entry | 增量校验 <200ms |
| 32 | 跑全量校验 | UI 不卡顿，可取消 |

### 15.3 边界场景（8 条）

| # | 场景 | 期望 |
|---|---|---|
| 33 | KB 根下放 `random.md`（违规） | E6 报错 |
| 34 | 4 层目录 | E5 报错 |
| 35 | `_inbox/foo/` 收容 | 通过（白名单） |
| 36 | 目录重命名（Finder） | 检测 + E2 报错 + UI 提示 |
| 37 | 同时编辑两个 entry | KB 锁串行；不冲突 |
| 38 | 删除 `content.md` | 节点灰显 + Sidebar 标记孤儿 |
| 39 | 老 KB 打开 | 不报错；W6 警告 |
| 40 | KB config YAML 损坏 | 提示原文件内容 + 错误；不覆盖 |

### 15.4 跨平台

| 平台 | 必跑 |
|---|---|
| macOS | §15.2 + §15.3 全部 |
| Linux | §15.2 #1-32 + §15.3 |
| Windows | §15.2 #1-32 + §15.3 |

### 15.5 回归（8 条）

| # | 操作 | 期望 |
|---|---|---|
| R1 | 打开一个老 KB（无新字段） | 正常显示，W 警告非阻断 |
| R2 | 创建 entry UI 流程 | 正常写文件 |
| R3 | 拖拽节点 | 不破坏 entry |
| R4 | 现有关系 | 正常显示 |
| R5 | Sidebar KB 切换 | 切 KB 正常 |
| R6 | i18n zh/en 切换 | 原有文案不丢失 |
| R7 | `npm run tauri:build` | 构建成功 |
| R8 | `cargo test` | 全绿 |

### 15.6 退出标准

**全部以下条件满足，spec 实现视为完成**：

- [ ] §15.1 §6.7 全部 Rust 单元测试通过
- [ ] §15.2 全部 32 条手工验证通过
- [ ] §15.3 全部 8 条边界场景通过
- [ ] §15.4 至少 macOS + Linux 通过
- [ ] §15.5 全部 8 条回归测试通过
- [ ] 无 P0 / P1 bug 残留

---

## §16 实施顺序与里程碑

### 16.1 总体策略

**自底向上**：解析器 → 校验引擎 → CLI → Tauri 集成 → UI → i18n → 验收。
每层完成且测试通过后再启下一层。

### 16.2 7 个里程碑

| 里程碑 | 工作日 | 验证子集 |
|---|---|---|
| **M1 解析器扩展** | 4 | §15.2 #1-8 + R1-R3 |
| **M2 校验引擎** | 6 | §15.2 #2-6 + §15.3 #33-36 部分 |
| **M3 CLI + 迁移** | 6.5 | §15.2 #19-22 + #25 |
| **M4 Tauri 集成** | 4 | §15.2 #23-25 + R5 |
| **M5 UI 右侧面板** | 7 | §15.2 #9-14 + §15.3 #36-38 |
| **M6 i18n** | 4 | §15.2 #26-28 + R6 |
| **M7 筛选 + 验收** | 7.5 | §15.2 #15-18 + #29-32 + §15.3-15.5 全量 |
| **合计** | **39 工作日** | — |

### 16.3 M1：解析器扩展

- 扩展 `parse_metadata_comments` 支持新键
- 新增 `extract_labels` / `extract_relations`
- `YamlFields` 加 `icon` / `preview` / `hidden`
- `RawEntry` 结构扩展（8 字段）
- SQLite schema 迁移（新增 `labels` 列）
- 单元测试 §15.1 解析器部分

### 16.4 M2：校验引擎

- `validator/mod.rs` 框架
- `validator/rules.rs` E1-E9 实现
- `validator/rules.rs` W1-W7 实现
- `validator/system_labels.rs` I1-I4
- `ValidationIssue` / `ValidationReport` 类型
- 共享上下文（all_ids / inverse_links）
- 单元测试 §15.1 校验部分

### 16.5 M3：CLI + 迁移

- Cargo.toml `[[bin]]` 段
- `src/bin/synapse.rs` clap 骨架
- 复用 `synapse_lib::validator`
- `migrator::labels_dry_run` / `labels_execute` + 备份 + 回滚
- `--json` 输出
- `synapse validate-all`
- 集成测试 §15.1.2

### 16.6 M4：Tauri 集成

- `AppState` 增加 `validation_report`
- 新增 6 个 `#[tauri::command]`（§6.5）
- `lib.rs` 启动钩子跑 H1 校验
- `kb_registry.set_active` 后重跑校验
- `KbSummary` 缓存
- 集成测试（启动 + 切 KB）

### 16.7 M5：UI 右侧面板系统

- `panelStore`（Zustand）
- `RightPanelStack` / `RightPanel` 组件
- 分隔条拖拽
- Settings 改为右侧面板
- Validation 面板组件
- NodeCard 红/黄边样式
- Sidebar 红/黄点
- 顶部红条 trigger

### 16.8 M6：i18n

- 新建 `Validation.zh.json` / `.en.json`
- 新建 `Filter.zh.json` / `.en.json`
- 扩展 `Settings` / `Sidebar` / `NodeCard` 双语
- 工具函数 `tRule()` + 系统标签本地化
- CI key 一致性校验脚本

### 16.9 M7：筛选 + 验收

- 标签筛选面板 UI
- AND/OR 语义
- Settings 标签颜色编辑
- Settings 校验开关
- Settings 迁移入口
- 全量手工验收 §15.2 + §15.3
- 回归测试 §15.5
- 跨平台验证（macOS + Linux）

### 16.10 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| M2 校验规则覆盖不全 | 后期返工 | M1 完成后立即启动 M2；规则列表先冻结再实现 |
| M3 迁移数据损坏 | 用户信任崩塌 | M3 强制 dry-run + 备份 + 回滚测试；beta |
| M5 右侧面板与现有布局冲突 | UI 改动大 | 先做无面板 mock 验证布局 |
| M6 i18n key 漂移 | 翻译遗漏 | CI 强制 zh/en key 同步 |

### 16.11 关键依赖

```
M1 → M2 → M3 → M4 → M5 → M6 → M7
              ↓
            CLI 可用
                    ↓
                  UI 可用
```

**并行机会**：
- M3（CLI）与 M4 部分并行（CLI 与 Tauri 集成解耦）
- M6（i18n 文案）与 M5（UI 实现）并行（依赖 M5 组件名确定）

### 16.12 可演示节点

- **M3 完成**：跑 `synapse validate` / `synapse migrate labels --dry-run` 看报告
- **M5 完成**：启动应用 → 红边节点 → Validation 面板 → 修复 → 重校验

---

## §17 实施完成判定

**全部以下条件满足，spec 实施完成**：

- [ ] M1-M7 全部里程碑通过其验证子集
- [ ] §15.6 退出标准全部勾选
- [ ] spec 文档本身已 commit
- [ ] `docs/standards.md` 更新引用新规范（v2）
- [ ] README 更新引用新规范

---

**审阅请关注**：
1. §1 目录契约层级是否合理
2. §2 标签与 labels 区分是否清晰
3. §3 字段表是否完整
4. §5.6 UI 校验展示是否够用
5. §9 右侧多面板架构接受度
6. §15 验收清单覆盖度
7. §16 里程碑与时间估计是否合理
