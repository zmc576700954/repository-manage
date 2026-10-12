# 内联元数据编辑 + 重命名/移动 设计

**代号**：内联编辑 & 重命名
**日期**：2026-10-12
**状态**：待用户审阅
**关联文档**：
- [Synapse-KB 设计文档](./2026-10-09-synapse-kb-design.md)
- [KB 规范 v2](./2026-10-12-kb-standards-v2-design.md)
- [全文搜索 + Markdown 渲染设计](./2026-10-12-search-and-markdown-design.md)

---

## §0 目标

为 Synapse KB 增加两类**元数据可写操作**：

1. **内联元数据编辑** — 在 DetailPanel 直接编辑 `title` / `tags` / `labels` / `status`（v2 规范的核心字段），无需打开文件或模态。
2. **重命名/移动** — 改变 entry 的文件路径（name 部分和/或 group 部分），新 id 自动派生；提供 wikilink 级联策略选择。

**v1 不做**（范围边界见 §7）：
- 拖拽移动 / 批量重命名 / 跨 KB 移动
- aliases / source / ignore 字段的 UI 编辑
- YAML frontmatter 编辑（color / pinned / hidden 等）
- 移动到 `_inbox/` 暂存
- 历史快照（交给 Git / OS-level backup）
- 冲突自动解决（Auto-suffix 按钮是 v2）

---

## §1 架构总览

```
src-tauri/
  src/
    commands.rs              # +3 commands（§2）
    parser/
      metadata_updater.rs    # 新建：改写 @synapse-* 注释，保留其他内容
src/
  components/
    EntryMetadataEditor.tsx  # 新建：DetailPanel 内的内联编辑组件
    RenameEntryDialog.tsx    # 新建：重命名模态对话框
    DetailPanel.tsx          # 修改：用 EntryMetadataEditor 替换只读 tags
  store/
    useAppStore.ts           # 修改：新增 3 个 mutations
  i18n/
    EntryMetadataEditor.{zh,en}.json  # 新建
    RenameEntryDialog.{zh,en}.json    # 新建
    resources.ts             # 修改：注册 2 个 NS
```

**唯一写入路径**：
- 内联编辑 → `update_entry_metadata` → `metadata_updater.rs`
- 重命名 → `execute_rename`（cascade + 文件改名 + SQLite 关系迁移在同一写锁内）

---

## §2 Rust commands

### 2.1 命令清单

| Command | 用途 | 调用方 |
|---|---|---|
| `update_entry_metadata(id, changes)` | 内联编辑一个或多个元数据字段 | EntryMetadataEditor on blur |
| `precheck_rename(id, new_name, new_group)` | 验证 + 计数（无副作用） | RenameEntryDialog 用户输入时 |
| `execute_rename(id, new_name, new_group, cascade)` | 原子重命名（写锁内） | RenameEntryDialog 提交时 |

### 2.2 类型签名

```rust
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MetadataChanges {
    pub title: Option<String>,
    pub tags: Option<Vec<String>>,
    pub labels: Option<Vec<String>>,
    pub status: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PrecheckRenameResult {
    pub valid: bool,
    pub new_id: String,
    pub conflict_with: Option<String>,
    pub incoming_count: u32,
    pub incoming_files: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RenameResult {
    pub new_id: String,
    pub updated_files: Vec<String>,
}
```

### 2.3 update_entry_metadata 算法

1. 读 `<kb>/<parent>/<id>.md` 全文
2. 对每个要改的字段（`title` / `tags` / `labels` / `status`）：
   - 正则匹配 `<!-- @synapse-{field}:\s*([^-]+?)\s*-->`
   - 替换为 `<!-- @synapse-{field}: {new_value} -->`（tags / labels 是 csv 形式）
   - 若原文件无该行 → 在第一个 `<!-- @synapse-` 行后插入新行
3. 字段校验：
   - `title` 非空、长度 ≤ 200
   - `tags` 每项不含 `,`、长度 ≤ 50、总数 ≤ 15
   - `labels` 每项不含 `,`、长度 ≤ 50
   - `status` ∈ `{draft, stable, wip, deprecated}`
4. 写回文件（保留所有非 `<!-- @synapse-` 行不动）
5. `idx.rebuild(kb_root)` 增量刷新
6. 返回新的 `StoredEntry`

### 2.4 precheck_rename 算法

1. 读 entry 当前路径 → `old_path` / `old_id`
2. 拼 `new_path = kb_root/new_group/new_name.md`
3. `new_id = derive_id_from_path(new_path)`（与 `entry::read_md_file` 一致：`rel.with_extension("").replace(['\\','/'], "-")`）
4. 冲突检查：
   - `new_id == old_id` → `valid=true`（no-op rename，但允许）
   - `new_id != old_id && entries.contains(new_id)` → `valid=false`, `conflict_with=new_id`
5. 扫所有 `.md`（除 old_path 自身）→ 正则 `\[\[<old_id>(\|[^\]]+)?\]\]` 计数 + 列出文件路径
6. 返回 `PrecheckRenameResult`

### 2.5 execute_rename 算法（写锁内，原子）

```
1. write_lock.acquire()
2. 计算 new_path / new_id（同 precheck）
3. 若 cascade=true：
   a. 对每个 .md（除 old_path）：
      - 跳过 code block 内的 [[old_id]]（line-by-line 扫描）
      - regex replace \[\[<old_id>(\|[^\]]+)?\]\] → \[\[<new_id>\1\]\]
      - 缓存 pre-cascade 内容用于失败回滚
4. fs::create_dir_all(new_path.parent())
5. fs::rename(old_path, new_path)（若 new_path 已存在 → 报错回滚）
6. 改新文件里的 <!-- @synapse-id: old_id --> → <!-- @synapse-id: new_id -->
7. SQLite 事务：
   a. UPDATE entries SET id=new_id WHERE id=old_id
   b. UPDATE relations SET from_id=new_id WHERE from_id=old_id
   c. UPDATE relations SET to_id=new_id WHERE to_id=old_id
   d. tx.commit()  // 任何步骤失败 → tx.rollback()
8. layout_store.rename_key(old_id, new_id)（位置跟随）
9. idx.rebuild(kb_root)
10. write_lock.release()
11. 返回 RenameResult { new_id, updated_files }
```

**回滚策略**：
- 步骤 3-5 任一失败 → 撤销前面所有步骤
- cascade 撤销：写回 pre-cascade 内容
- 文件改名撤销：`fs::rename(new_path, old_path)`
- SQLite 撤销：事务内 rollback
- 步骤 6-7 任一失败 → 撤销 3-5 + 6-7

---

## §3 前端数据流

### 3.1 useAppStore mutations

```ts
interface UseAppStore {
  // 现有字段省略
  
  // 内联编辑：optimistic update + IPC + 失败回滚
  updateMetadata: (id: string, changes: MetadataChanges) => Promise<StoredEntry>;
  // 1) 立即更新本地 entry（optimistic）
  // 2) invoke('update_entry_metadata', { id, changes })
  // 3) 成功：用后端返回的 StoredEntry 覆盖本地
  // 4) 失败：revert + setError(toast)
  
  // 重命名
  precheckRename: (id: string, newName: string, newGroup: string) => Promise<PrecheckRenameResult>;
  renameEntry: (id: string, newName: string, newGroup: string, cascade: boolean) => Promise<RenameResult>;
  // renameEntry 成功时：
  //   - entries 数组中 id: old_id → id: new_id（连带 relations）
  //   - 若 selectedEntryId === old_id，更新为 new_id
  //   - layoutStore 同步更新（key 转移由后端完成，前端无感）
}
```

### 3.2 DetailPanel 渲染

```
DetailPanel
├── Header
│   ├── id (readonly), title (editable inline)
│   ├── [✎ Rename] button → opens RenameEntryDialog
│   └── close [✕]
├── EntryMetadataEditor (new)
│   ├── Tags: chip list (each with × remove) + "+ add" input
│   ├── Labels: chip list
│   └── Status: select (stable/draft/wip/deprecated)
│       └── each field on blur → 立即触发 updateMetadata（无 debounce；快速连改则多 IPC）
├── Markdown content (rendered/source toggle, already exists)
├── Relations (already exists)
└── External references (already exists)
```

### 3.3 RenameEntryDialog 状态机

```
state: idle → checking → (valid | conflict | error) → submitting → (success | error)
                                                       └→ close on success

user types in name/group → debounce 200ms → precheckRename
├── valid + incoming_count > 0: show "⚠ N incoming refs" + cascade radio
├── valid + incoming_count == 0: show "No incoming refs" (no cascade option)
├── conflict: show "ID already exists: {id}" + disable Rename button
└── error: show "Rename check failed: {reason}"

on Submit (valid + cascade chosen):
├── invoke('execute_rename', { id, newName, newGroup, cascade })
├── on success: close dialog, DetailPanel re-renders with new id
└── on error: show error toast, keep dialog open for retry
```

---

## §4 错误处理

### 4.1 前端可恢复错误

| 场景 | 行为 |
|---|---|
| `update_metadata` 写文件失败 | 回滚本地 entry 到改之前 + toast |
| 字段校验失败（非法 tag/status） | 不发 IPC，inline 字段标红 + 提示 |
| `execute_rename` 文件移动失败 | 整操作回滚（已 cascade 的文件也回滚） + toast |
| `execute_rename` 冲突（执行时检测到与 precheck 间新冲突） | 不动文件 + toast |

### 4.2 Rust 端错误码（扩展 `AppError`）

```rust
pub enum AppError {
    // 现有变体省略
    
    MetadataInvalidField { field: String, reason: String },
    RenameConflict { new_id: String },
    RenameFileExists { path: String },
    ExternalEditDetected { path: String, mtime: SystemTime },
    CascadeFailed { failed_files: Vec<String>, reason: String },
}
```

### 4.3 Rust 端回滚

`execute_rename` 的步骤顺序保证可逆：
- cascade 在文件改名之前 → cascade 失败只需恢复文件内容
- 文件改名 + SQLite 事务原子 → 失败时 SQLite rollback + 文件回滚
- @synapse-id 更新在文件改名之后 + SQLite 之前 → 失败时 SQLite rollback + 文件内容修改

---

## §5 i18n

### 5.1 新增 NS

**`EntryMetadataEditor.{zh,en}.json`**（15 keys）：

```jsonc
{
  "titleLabel": "标题",
  "titlePlaceholder": "输入标题...",
  "tagsLabel": "标签",
  "tagsPlaceholder": "添加标签...",
  "labelsLabel": "工作流标注",
  "labelsPlaceholder": "添加标注...",
  "statusLabel": "状态",
  "statusStable": "稳定",
  "statusDraft": "草稿",
  "statusWip": "撰写中",
  "statusDeprecated": "遗弃",
  "removeTagAria": "移除",
  "errorEmptyTitle": "标题不能为空",
  "errorInvalidTag": "标签不能包含逗号",
  "savedFlash": "已保存"
}
```

**`RenameEntryDialog.{zh,en}.json`**（17 keys）：

```jsonc
{
  "title": "重命名条目",
  "newNameLabel": "新名称",
  "newNamePlaceholder": "new-name",
  "newGroupLabel": "新分组",
  "newGroupPlaceholder": "frontend/subdomain",
  "precheckChecking": "检查中...",
  "precheckNoIncoming": "无传入引用",
  "precheckIncomingOne": "1 个文件引用此条目",
  "precheckIncomingMany": "{{count}} 个文件引用此条目",
  "conflictExists": "ID 已存在：{{id}}",
  "errorInvalidName": "名称不合法",
  "cascadeLabel": "如何处理传入引用？",
  "cascadeUpdate": "更新引用（推荐）",
  "cascadeLeave": "保留为断链",
  "cancel": "取消",
  "rename": "重命名",
  "renameBusy": "重命名中..."
}
```

### 5.2 注册

在 `src/i18n/resources.ts` 的 `NS` 数组添加两个 namespace，并在 `zh` / `en` 资源对象中引用。

### 5.3 一致性校验

两个文件的 key 集合必须严格一致。Cycle 1 已建立的 i18n 校验脚本可复用。

---

## §6 数据存储

### 6.1 文件格式

`@synapse-title` / `@synapse-tags` / `@synapse-labels` / `@synapse-status` 全部以 HTML 注释形式存储于 content.md 顶部。`metadata_updater.rs` 用正则匹配并改写这些行，**保留**：
- 所有非 `<!-- @synapse-` 行（markdown body、wikilinks、YAML frontmatter）
- 行末换行符
- 代码块内嵌的 `[[id]]`（cascade 不替换）

### 6.2 SQLite

`entries` 表已有 `id` 主键、`tags` JSON 字段。新增需求：
- `rename_entry_id` 操作：跨表更新（entries / relations 的 from_id / to_id）
- 用事务保证原子性

`layout_store`（`src-tauri/src/layout_store.rs`）需新增 `rename_key(old, new)` 操作，将旧 id 的位置数据迁移到新 id（位置跟随）。

---

## §7 范围边界（v1 不做）

| 不做 | 理由 |
|---|---|
| 拖拽移动 | 涉及 React Flow 与文件系统的复杂集成，v2 |
| 批量重命名 | v1 一次性操作一个 entry 即可 |
| 跨 KB 移动 | v2 规范明确 KB 独立 |
| aliases / source / ignore UI 编辑 | 这些字段用户极少改，保留文件级编辑 |
| YAML frontmatter 编辑 | color / pinned 等是 styling，v2 |
| 移动到 `_inbox/` 暂存 | v2 规范的暂存概念 |
| 历史快照 | 交给 Git / OS-level backup；提供 `backup_then_write` 工具但不在 UI 暴露 |
| 冲突自动解决（Auto-suffix） | v2 |
| rename 时自动新建 `attachments/` 或 `notes/` 子目录 | 违反"路径由用户决定"原则 |

---

## §8 验收清单

### 8.1 手工 e2e（13 条）

**内联编辑**：
1. 打开 entry → 改 title → blur → 切到源码模式看到 `@synapse-title` 已更新
2. 改 tags（加 1 个、删 1 个）→ blur → SQLite 反映新 tags
3. 改 status → select 改变 → DB 反映
4. 连续改 title + tags + status（5 秒内）→ 全部落盘，无中间状态
5. 改 title 为空 → inline 报错，不发 IPC
6. 改 tag 含逗号 → inline 报错

**重命名**：
7. ✎ Rename 打开对话框 → 改 name → precheck 显示 incoming count
8. cascade=true → 提交 → 其他文件 `[[old-id]]` 全部变 `[[new-id]]`
9. cascade=false → 提交 → 那些文件显示 `(引用不存在: old-id)`
10. rename 到已存在的 id → dialog 标红 "ID conflict"，禁用 Rename 按钮
11. rename 跨 group（同一 KB）→ 文件移到新目录，`@synapse-id` 同步更新
12. rename 后看 map → 节点位置保留（layout 跟随 id 转移）
13. rename 期间外部修改文件 → 回滚 + 报错

### 8.2 Rust 单元测试（≥ 7 个）

| 测试 | 验证 |
|---|---|
| `update_metadata_writes_title_comment` | title 改后 `@synapse-title:` 行更新 |
| `update_metadata_preserves_unrelated_content` | markdown body 不动 |
| `update_metadata_validates_tag_format` | 非法 tag 拒绝 |
| `precheck_rename_detects_id_collision` | 目标 id 已存在返回 conflict |
| `execute_rename_cascades_incoming_wikilinks` | `[[old]]` → `[[new]]` 替换 |
| `execute_rename_skips_code_blocks` | code block 内的 `[[old]]` 保留 |
| `count_incoming_refs_matches_execute_cascade` | 计数与实际替换数一致 |

### 8.3 TypeScript

`npm run build` 通过（strict typecheck）。

### 8.4 回归

- 已有 DetailPanel 行为（content / relations / ext links）不变
- Cycle 1 搜索 + Markdown 渲染功能正常
- 切换 KB / reload entries 不破

---

## §9 风险与决策记录

### 决策 1：v1 不做拖拽移动

**理由**：React Flow 节点拖拽 + 文件系统 mv 的集成复杂度高，需要：
- 拖拽事件与现有 layout_store 的位置写入协调
- 拖拽过程中实时反映 group 改变
- 跨 group 时的视觉反馈

v1 走显式 dialog 路径更安全，UI 也更明确（用户主动确认）。

### 决策 2：cascade 总是询问（不默认自动）

**理由**：rename 是破坏性操作。静默修改 12 个文件可能导致：
- 用户没意识到连锁修改
- 误 rename 后无 undo 路径
- 用户的 alias 习惯被破坏

弹出对话框给用户一次明确选择，是安全性 vs 便利性的合理折中。

### 决策 3：layout 位置跟随 id 转移

**理由**：用户 rename 通常是想"保留位置但换个名字/分组"。如果位置丢失，需要重新布局，破坏 flow。
- 实现成本低：`layout_store.rename_key(old, new)`
- 用户体验好：rename 后节点仍在原位

### 决策 4：内联编辑不暴露 aliases / source / ignore

**理由**：
- `aliases` 是兼容老链接的逃生通道，AI/老 KB 偶尔用，普通用户极少编辑
- `source` 通常是 entry 创建时一次填好
- `ignore` 是元元数据，UI 暴露会增加干扰
- 保留文件级编辑（用任何 markdown 编辑器）足够

### 决策 5：status 用 select 不是 input

**理由**：v2 规范定义 4 个枚举值（draft / stable / wip / deprecated）。select 强制合法值，inline 校验更简单，避免用户拼错。

### 决策 6：title / name 关系

`title`（UI 显示名）和 `name`（文件名，id 的一部分）解耦：
- 内联编辑改 `title`，不动文件 name
- Rename 改 name（和 group），不动 title
- 一个 entry 的 title 可以是中文，name 必须是 slug 化的

---

## §10 未来扩展（v2+）

- 拖拽移动
- 批量重命名
- aliases / source / ignore UI 编辑
- YAML frontmatter 编辑
- 跨 KB 移动
- Auto-suffix 冲突解决
- 移动到 `_inbox/` 暂存
- rename 历史快照（per-entry backup dir）

---

## §11 实施顺序

预计 ~12 个 task：

1. Rust `metadata_updater.rs` + 单元测试
2. Rust `update_entry_metadata` command + 单元测试
3. Rust `precheck_rename` + `count_incoming_refs`（内部） + 单元测试
4. Rust `execute_rename` + SQLite 关系迁移 + 单元测试
5. Rust `layout_store.rename_key` + 测试
6. i18n 资源文件（2 NS × 2 语言 = 4 文件）
7. i18n registration in `resources.ts`
8. 前端 `useAppStore` mutations
9. 前端 `EntryMetadataEditor` 组件
10. 前端 `RenameEntryDialog` 组件
11. `DetailPanel` 集成 + 头部 Rename 按钮
12. 全量验收（build + 12 条 e2e + Cycle 1 回归）

---

**审阅请关注**：
1. §2 命令清单是否完整（是否需要拆出 `count_incoming_refs` 为独立 command？）
2. §2.5 execute_rename 步骤顺序是否合理
3. §3.1 optimistic update + 回滚模型是否清晰
4. §5 i18n keys 是否覆盖
5. §7 不做清单是否合理
6. §11 任务拆分粒度
