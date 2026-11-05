# Synapse KB 架构说明（代码实证版）

> 基线：`6b9c642` 的修复前快照；第 11 节记录此后落地的改动，冲突处以第 11 节为准。本文以实际代码为准，不以 README 为准；文末单列了文档与代码不一致的地方。
> 结论先行：这是一个**桌面优先的单进程应用**——React 通过 Tauri IPC 调 Rust，Rust 把 KB 目录里的 Markdown 扫描成内存模型、写入每个 KB 独立的 SQLite 缓存，再由前端一次性拉全量数据渲染 React Flow。**没有 axum、没有 HTTP 服务**；基线代码也没有后端事件推送（现已由 `kb-changed` 补齐，见 §11）。基线的结构性风险是**「目录条目格式」与「扁平 .md 格式」两套互不兼容的解析/校验口径**，现已统一（见 §11）。

---

## 0. 整体图景

```text
┌────────────────────────────── WebView（React 18 + Vite）──────────────────────────────┐
│ src/main.tsx                                                                          │
│   ├─ initI18n()（tauri-plugin-store 持久化语言）                                        │
│   └─ App.tsx                                                                          │
│        ├─ Sidebar               KB 列表 / 语言 / reload                                │
│        ├─ MapView               React Flow 画布：nodes/edges 全部由 entries 派生         │
│        │    └─ NodeCard         单条目节点                                                │
│        └─ RightPanelStack       DetailPanel / Settings / RelationDialog / EntryDialog    │
│   Zustand：useAppStore(entries/selected/relationFilter/kbs) · panelStore · fixStore      │
│             langStore                                                                    │
│   后端调用：`invoke('<snake_case 命令>', camelCase 参数)` —— 直接散落在 6 个文件里          │
└───────────────────────────────────┬──────────────────────────────────────────────────────┘
                                    │ Tauri IPC（invoke_handler，26 个命令）
┌───────────────────────────────────▼──────────────────────────────────────────────────────┐
│ Rust crate `synapse_kb_lib`（src-tauri/src）                                               │
│  lib.rs::run()                                                                             │
│    ├─ KbRegistry::load_or_default()  → kbs.json                                           │
│    ├─ Index::open(indexes/<kb_id>.db)  → 建表 + ALTER TABLE 补列，不重建                     │
│    ├─ AppState { index, kbs, current_kb, validation_report, write_lock, watcher, watch_tx }│
│    ├─ setup: spawn(run_startup_rebuild)  // 异步重建 + validate_kb                          │
│    └─ 独立线程 kb-watcher：notify 500ms debounce → 重新 Index::rebuild                      │
│                                                                                            │
│  commands.rs（2189 行，唯一 IPC 适配层）                                                     │
│    └─ 大多数写命令持 write_lock，然后：写文件 / 改 DB → loadEntries() 重新拉全量              │
│                                                                                            │
│  index.rs（1788 行）   SQLite schema / rebuild 两遍 / 关系解析                                 │
│  scanner.rs           walkdir 扫 `*.md`（≤5 层）与含 content.md 的目录                        │
│  entry.rs             read_md_file（扁平）/ read_entry（目录）两套解析器                      │
│  parser/*             @synapse-* 注释、YAML、[[wikilink]]、![[embed]]、[text](url)           │
│  validator/*          E/W/I 规则、fix_impls(E1..E9)、migrator、system_labels                  │
│  fs/backup.rs         .synapse-fix-backup 快照                                                │
│  kb_registry.rs       多 KB 注册表                                                            │
│  layout_store.rs      .synapse-layout.json（**当前无调用方**）                                 │
└─────────────────────────────────────────────────────────────────────────────────────────────┘
持久化分工：KB 目录 `*.md` = 事实源；SQLite = 由文件重建的缓存；`kbs.json` = KB 注册表；layout json = 孤儿数据
```

### 0.1 五条最重要的结论

1. **没有 Web 模式。** `README.md:18` 写有 axum local HTTP server，但 [Cargo.toml](../src-tauri/Cargo.toml) 无 `axum`/`tower`，工作树无 `local_server.rs`；git 历史只留下 `59a5dcb feat(server): axum ...`，后续被移除。前端也没有任何 transport 抽象层，全部直接 `invoke(...)`，所以 `npm run dev` 只能起一个无法工作的纯浏览器页面。
2. **KB 文件格式处于迁移中间态。** 索引走扁平格式 `read_md_file`（`index.rs:576`），校验规则按目录格式设计（`rules.rs`）；两套 id 派生、标签解析、附件解析不同，导致索引与校验互相对不上。
3. **SQLite 不是事实源。** 文件才是；命令先改 `.md` 再 `rebuild`，DB 随时可丢弃。`CLAUDE.md:93` 的「SQLite is the source of truth at runtime」与代码相反。
4. **基线没有事件通道（现已补齐，见 §11）。** 基线后端只写 DB，不 `emit`；前端只在命令返回后手动 `loadEntries()`，外部编辑后界面保持旧数据。当前版本在 watcher 重建完成后推送 `kb-changed`（含 `origin`、内容指纹、单调 `revision`、`renamed`），前端 `kbRefresh` 防抖合并并区分自身回声与外部变更。
5. **图谱不是力导向布局。** `MapView.tsx:111-161` 是按 `group` 分桶的确定性网格；`layout_store` 与拖拽持久化均未接线。

---

## 1. 目录与模块分层

### 1.1 Rust 后端（`src-tauri/src`）

| 模块 | 职责 | 关键入口 |
|---|---|---|
| `main.rs` | Tauri 可执行入口，仅调 `synapse_kb_lib::run()` | `main()` |
| `lib.rs` | 组装：注册表、索引、AppState、watcher、setup、26 个 handler | `run()`（486 行） |
| `state.rs` | 全局共享状态与锁语义 | `AppState`、`WatcherHandle` |
| `commands.rs` | 所有 `#[tauri::command]`；文件写、DB 改、备份、重命名级联都在这里 | 2189 行，见 §2.2 |
| `index.rs` | SQLite schema、`rebuild`、`get_all_entries`、关系读写、id/路径查询 | `Index::open`、`rebuild`、`get_all_entries` |
| `scanner.rs` | 两种扫描口径：`find_entry_dirs`（目录模式）与 `find_md_files`（扁平模式） | 二者均被使用，见 §4.1 |
| `entry.rs` | `RawEntry` 的两套构造：`read_md_file` / `read_entry` | §4.2 |
| `parser/` | `comments.rs` 解析 `@synapse-*`；`yaml_field.rs`；`links.rs`；`attachments.rs`；`metadata_updater.rs` 原位改写注释 | `check_required` 依赖的 tags 在这里产出 |
| `validator/` | `validate_kb`、`rules.rs`、`system_labels.rs`、`fix_impls/*`、`migrator.rs` | 规则 E1–E9 / W1–W7 / I* |
| `kb_registry.rs` | `kbs.json` 原子写、active KB、每 KB 独立 index DB 路径 | `load_or_default`、`index_db_path_for` |
| `paths.rs` | app data 目录、`SYNAPSE_KB_ROOT` 回退根目录、默认索引路径 | `kb_root`、`app_data_dir`、`index_db_path` |
| `layout_store.rs` | `.synapse-layout.json` 读写 | **只有自身测试引用** |
| `fs/backup.rs` | 修复前快照 | `backup_then_write` |
| `bin/synapse.rs` | 独立 CLI，复用 lib 的 validator/registry，不经 IPC | `synapse` bin |

**分层是否清晰：** 名义上是 `commands → index/entry/parser/validator`，实际 `commands.rs` 同时承担了 UI 适配、业务编排、文件系统写入、事务顺序和错误码包装，是最重的耦合点；`index.rs` 又把「扫描 → 解析 → 校验 → 入库 → 关系解析」全部塞进 `rebuild`，导致任何格式变更都要同时改动三处。

### 1.2 前端（`src`）

| 层 | 文件 | 说明 |
|---|---|---|
| 入口 | `main.tsx`、`App.tsx` | 初始化 i18n；挂 Sidebar + MapView + 右侧面板 |
| 视图 | `views/MapView.tsx`、`views/DetailPanel.tsx`、`views/Settings.tsx` | MapView 负责布局/边/图例；DetailPanel 读 `get_entry_content` |
| 组件 | `components/*` | Sidebar、NodeCard、各 Dialog、ValidationPanel、FixDrawer、ErrorBar、RightPanelStack |
| 状态 | `store/useAppStore.ts`、`store/panelStore.ts`、`store/langStore.ts`、`store/fixStore.ts` | 全量拉取 + 乐观更新 |
| 搜索 | `search/index.ts`、`search/useSearch.ts` | 纯前端排序 + 逐条 `get_entry_content`（带 LRU） |
| i18n | `i18n/*`、`*.zh.json` / `*.en.json` | 一组件一 namespace；错误码另有 `Errors` namespace |
| 类型 | `shared/types.ts` | **手写契约**，与 Rust serde 输出存在漂移（见 §6.3） |

前端没有 `api/` 或 `services/` 层：`useAppStore.ts`、`DetailPanel.tsx`、`Settings.tsx`、`ValidationPanel.tsx`、`FixDrawer.tsx`、`useSearch.ts` 各自直接 `invoke`。命令名是 snake_case，参数名靠 Tauri 的 `rename_all = "camelCase"` 映射。

### 1.3 `shared/` 与 `docs/`

- `shared/types.ts`（112 行）被 `tsconfig.json` 的 `@shared/*` 与 Vite alias 共用；它只是 TS 声明，**不参与 Rust 端生成**，因此无法保证与 `StoredEntry` 一致。
- `docs/standards.md` 开头已自述「已被 KB Standards v2 设计方案替代」；`docs/superpowers/specs|plans|verification` 是历史设计与实施记录，包含已废弃的 axum、目录格式、布局持久化方案。它们不是当前运行时契约。

---

## 2. 启动、IPC 与运行时

### 2.1 启动序列（`lib.rs::run`）

1. `KbRegistry::load_or_default()` 读取 `kbs.json`；解析失败时静默回退默认值（`kb_registry.rs`）。
2. 按 active KB 打开 `Index::open(<app_data>/indexes/<kb_id>.db)`：`CREATE TABLE IF NOT EXISTS` + `migrate_schema` 逐列 `ALTER TABLE`（`index.rs:129-171`）。**启动不做同步重建**，窗口先显示旧 DB 内容。
3. 创建 `AppState`（`state.rs:19-51`）：`index`、`kbs`、`current_kb`、`validation_report`、`write_lock`、`watcher`、`watch_tx`。
4. 启动独立 `kb-watcher` 线程（`lib.rs:91-96`），内部 `new_debouncer(500ms)`（`lib.rs:158-159`），事件经 `mpsc` 交给 `run_watch_loop`，再 `spawn_blocking(rebuild)`。
5. `setup` 里 `tauri::async_runtime::spawn(run_startup_rebuild)`（`lib.rs:109-111`）：持 `write_lock` → `index.rebuild` → `validate_kb` 写入 `validation_report`（`lib.rs:275-302`）。
6. 注册 26 个命令（`lib.rs:114-141`）。
7. 前端 `App.tsx:23-25` 立即 `refreshKbs().then(loadEntries())`。

**启动竞态：** 步骤 5 在后台跑，步骤 7 不做等待；`get_entries` 与 `rebuild` 都只取 `index.read()`，可以并发。`rebuild` 先在无事务状态下 `DELETE FROM entries/relations`（`index.rs:555-560`），再在事务里插入，因此前端可能读到「删除后、插入前」的空表，表现为首屏空白，直到手动刷新。

### 2.2 Tauri 命令面（26 个）

| 分组 | 命令 | 前端是否调用 |
|---|---|---|
| 条目读 | `get_entries`、`get_entry_content` | 是 |
| 条目写 | `create_entry_simple`、`create_entry_from_reference`、`delete_entry` | 是 |
| 关系 | `create_relation`、`delete_relation` | 是 |
| 元信息 | `update_entry_metadata` | 是 |
| 重命名 | `precheck_rename`、`execute_rename` | 是 |
| KB 注册表 | `list_kbs`、`add_kb`、`remove_kb`、`set_active_kb`、`get_active_kb`、`rebuild_active_kb` | 是 |
| 校验/修复 | `validate_kb`、`apply_fix` | 是 |
| **未接线** | `active_kb_entry_count`、`validate_entry`、`get_system_labels`、`run_validation`、`get_kb_config`、`update_kb_config`、`list_backups`、`restore_backup` | **否**（全仓 `invoke` 无匹配） |

`update_entry_metadata` 是一次「写文件 → 立刻全量 rebuild → 再读全量 entries」的重命令（`commands.rs:1017-1042`）；返回的 `StoredEntry` 会被前端用来覆盖乐观更新（`useAppStore.ts:291-307`）。

### 2.3 事件通道：基线不存在（现由 `kb-changed` 补齐，见 §11）

- 基线后端全仓没有 `emit(...)` / `Event::`；前端没有 `@tauri-apps/api/event` / `listen`。
- 基线 watcher 更新 DB 后不通知前端；`reload` 按钮才会触发 `loadEntries`。
- 当前实现：watcher 重建完成后 `emit("kb-changed", payload)`，负载带 `kbId`、`entryCount`、`added`/`removed`/`renamed`、`origin`（`local-write`/`external`）、内容指纹与单调 `revision`；指纹与上次相同则不推送。前端 `src/events/kbRefresh.ts` 做 300ms 防抖合并，仅在 `local-write` 且指纹与本地已同步内容一致时丢弃回声。

### 2.4 前端刷新模型

- 每个写命令成功后调用 `loadEntries()`，即**全量 `get_entries`**；`get_all_entries` 对每个 entry 再发 3 条查询（tags/attachments/relations），是 O(3N) 的 N+1（`index.rs:277-319`）。
- `setActiveKb` 为了避免「await 期间用户点选被清空」，直接再拉一次 `get_entries` 并做 id 存在性判断（`useAppStore.ts:252-273`）。
- `updateMetadata` 先乐观改本地，失败回滚（`useAppStore.ts:279-315`）。

---

## 3. 核心数据模型与 SQLite 索引

### 3.1 表结构（`index.rs:30-83`）

| 表 | 关键列 | 说明 |
|---|---|---|
| `entries` | `id PK`、`title`、`path`、`group_name`、`has_content_md`、`labels/status/aliases/source/created/updated/ignored_rules`（JSON 文本）、`error_count`、`warning_count` | `labels` 等是 JSON 字符串，读时 `parse_json_vec` |
| `tags` | `(entry_id, tag) PK`，FK CASCADE | |
| `attachments` | `id`、`entry_id`、`path`、`caption`、`attachment_type`，FK CASCADE | |
| `relations` | `(from_id, to_id, relation_type) UNIQUE`，双 FK CASCADE，`idx_relations_from/to` | 关系类型是列值 |
| 无 FTS | — | 搜索完全在前端内存里做 |

**重要事实：没有 `PRAGMA foreign_keys = ON`。** 全仓唯一相关语句是 `PRAGMA defer_foreign_keys = ON`（`index.rs:386`，只在 `rename_entry_id` 事务内），SQLite 默认 FK 关闭，因此上面所有 `ON DELETE CASCADE` 实际不生效：`delete_entry` 只删 `entries` 行（`index.rs:349`），`tags/attachments/relations` 会留下孤儿；孤儿关系虽会被前端 `entryIds.has(...)` 过滤掉，但会持续累积。

### 3.2 内存模型与序列化

- `RawEntry`（`entry.rs`）= 解析结果；`StoredEntry`（`index.rs:85-105`）= IPC 输出。
- `StoredEntry` 字段：`id,title,path,group,tags,attachments,relations,has_content_md,labels,status,aliases,source,created,updated,ignored_rules,error_count,warning_count`；`#[serde(rename_all = "camelCase")]`。
- `StoredRelation` 序列化为 `fromId/toId/relationType/note`；`StoredAttachment` 为 `path/caption/attachmentType`。
- **没有 `contentPath` 字段**，而前端 `Entry` 接口声明了必填 `contentPath`（`shared/types.ts:36`）。

### 3.3 id 派生、分组与冲突

- 扁平模式：`read_md_file` 用相对路径去扩展名、把 `/` 换 `-`：`frontend/react.md` → `frontend-react`；`group` = 父目录字符串（`entry.rs:48-105`）。
- 目录模式：`read_entry` 优先 `@synapse-id`，否则目录名（`entry.rs:110-155`）。
- 同一 id 可由不同文件产生（`compliance-gdpr.md` vs `compliance/gdpr.md`）。`rebuild` 按 id 去重、保留最后一个并只 `eprintln` 提示（`index.rs:579-588`），主键冲突不会报给用户。
- 关系解析（`index.rs:656-703`）：对 `[[id]]` 与内部 `[text](path)` 做 `resolve_target`（直接 id / 文件名 stem / 相对路径），命中即插入 **`reference`**，并且只插入 `reference`。

### 3.4 `rebuild` 的两遍结构

1. 清空 `entries`、`relations`（**无事务**，`index.rs:555-560`）。
2. Pass 1：`find_md_files` 遍历（≤5 层，根目录 `README.md` 跳过）→ `read_md_file` → 按 id 去重 → 对每条跑 `rules::check_all` 写入 `e.validation`（`index.rs:632-640`）→ 单事务批量 upsert（`index.rs:647-654`）。
3. Pass 2：收集 `[[...]]` 与内部 Markdown 链接 → 解析 → 单事务批量插入 `reference`（`index.rs:656-703`）。
4. 返回 DB 实际行数，而不是文件数（测试 `rebuild_returns_db_row_count_not_file_count`）。

**关系类型不可持久化：** 用户通过 `create_relation` 写 `relation_type`，但磁盘上只留下 `[[to_id]]` 文本（`append_reference_line`），没有任何注释记录类型。任何一次 `rebuild`（启动、watcher、`update_entry_metadata`、`create_entry_from_reference`）都会删掉全部关系并只重建 `reference`。所以 `derived/contradicts/supersedes/extends/custom` **会在下一次重建时全部退化为 `reference`**。`commands.rs` 里 `create_entry_from_reference` 的注释也承认 rebuild 会把关系重置为 reference，而 `create_entry_simple` 则刻意走增量路径以避免这一点——同一类操作存在两套语义。

---

## 4. 文件扫描与解析流程

### 4.1 三种扫描口径

| 函数 | 扫描对象 | 使用者 |
|---|---|---|
| `scanner::find_entry_dirs` | 含 `content.md` 的目录（≤8 层） | `validator::scan_entries`（目录格式优先） |
| `scanner::find_md_files` | 所有 `*.md`（≤5 层） | `Index::rebuild`（唯一索引路径） |
| `validator::scan_entries` | 若存在 entry dirs 则用目录，否则回退 `find_md_files` | `validate_kb` / `validate_entry` |

这意味同一个 KB：
- 索引把它当「扁平 `.md` 条目」：`topic-a/content.md` 会变成 id `topic-a-content`；
- 校验把它当「目录条目」：id 为 `topic-a`（或 `@synapse-id`）。
两套 id 不一致，`validation_report` 的 `entry_id` 无法与 DB 的 `entries.id` 对齐，`system_labels`、错误计数、修复入口都会错位。

### 4.2 两套解析器（`entry.rs`）

| 能力 | `read_md_file`（扁平，索引用） | `read_entry`（目录，校验/CLI 用） |
|---|---|---|
| id | 路径派生（去扩展名） | `@synapse-id` 或目录名 |
| title | `@synapse-title`，回退 `# heading`，再回退 stem | `@synapse-title`，回退 id |
| tags | **恒为 `[]`** | 解析 `@synapse-tags` 逗号分隔 |
| labels/status/aliases/source/created/updated/ignored_rules | **全部为空**（`entry.rs:97-103`） | 全量解析 |
| YAML | **`None`**（`entry.rs:95`） | `parse_yaml_field` |
| attachments | **`[]`** | `extract_attachments` + `classify_attachment` |
| `[[links]]` / `[text](url)` | 解析 | 解析 |

结果：**索引路径只保留 title + 链接 + 路径**。`EntryMetadataEditor` 写回 `@synapse-tags` 后立刻 rebuild，rebuild 用扁平解析器读回空 tags，接口返回值再把前端的乐观 tags 覆盖为空——除 title 外的元数据编辑在扁平 KB 上不闭环（`commands.rs:1024-1042` + `useAppStore.ts:291-307`）。

### 4.3 校验规则与格式的系统性冲突

`rules::check_all` 被 `rebuild` 和 `validate_kb` 共用（`index.rs:637`、`validator/mod.rs`），但规则按目录格式写：

- `E2 check_id_matches_path`（`rules.rs:73-111`）直接拿 `file_name`（含 `.md`）拼 id。扁平 `foo.md` 的 id 是 `foo`，比较值是 `foo.md` → **每条扁平条目必报 E2**。
- `E4 check_required`（`rules.rs:60-69`）要求 tags 含 `type:`；扁平解析器 tags 恒空 → **每条必报 E4**。
- `E6 check_root_md_files`（`rules.rs:146-165`）禁止 KB 根下的 `.md` → **扁平根级条目必报 E6**（而扁平格式恰恰把根级 `.md` 视为合法条目，`index.rs:565-566` 还专门只跳过 README）。
- `W4 check_broken_link_targets` 在 `validate_entry` 里拿到的是「只含自己的 id 集合」（`commands.rs:560-566`），因此任何跨条目链接都会被报成 broken；该命令目前前端未使用。

所以按当前代码静态推演，**扁平 KB 的每个节点都会带 1–3 个系统性误报**，ErrorBar 与节点红点长期非零；目录 KB 则索引 id/关系与校验报告对不上。若实际界面未出现，说明现有 KB 数据或 `ignored_rules` 与代码假设不同，建议用 `validate_kb` 实测一次确认。

### 4.4 元数据写入与修复

- `metadata_updater.rs` 按行正则替换/追加 `<!-- @synapse-* -->`，保留正文。
- `fix_impls` 实现 E1–E9（缺 E8）：E6 会把根级 md 包成 `entries/<name>/content.md`（目录格式），进一步加深双格式混杂。
- `fs/backup.rs:38-43` 用 `target.parent().parent()` 推导 `kb_root`：目录格式 `kb/topic/content.md` 正确；扁平 `kb/note.md` 会推导成 **KB 的上一级目录**，备份写到 KB 之外。

---

## 5. Web 模式下 axum 如何复用后端逻辑

**当前不存在这条路径。** 证据：

- `README.md:18` 声称有 axum local HTTP server；`src-tauri/Cargo.toml` 依赖中无 `axum`、无 `tower`/`hyper`；工作树无 `src-tauri/src/local_server.rs`；`lib.rs` 无 serve 调用。
- git 历史：`59a5dcb feat(server): axum local HTTP server` 引入；`bc2708d` 同时删除了 `src-tauri/src/local_server.rs`、`src-tauri/src/bin/server.rs`、`Dockerfile`、部署脚本以及前端仅有的 `src/lib/api.ts` 适配层；`CLAUDE.md:94` 明确写「No Docker / no local HTTP server in current code」。
- 前端所有数据获取都走 `@tauri-apps/api/core` 的 `invoke`，没有 fetch/REST 客户端，也没有可切换的 transport 接口；脱离 Tauri 宿主时 `invoke` 直接失败。

**目前唯一可复用的「非 Tauri 入口」是 CLI**：`src-tauri/src/bin/synapse.rs` 直接依赖 `synapse_kb_lib`，复用 `validator`（`validate_kb`、`Severity`、`migrator`）与 `kb_registry`，不经过 `AppState`/`commands`。它也**没有复用 `Index`**，所以 CLI 与 GUI 的扫描/校验口径仍然可能分叉。

如果未来真要加 axum，最小改造是：
1. 在 Rust 侧把 `commands.rs` 的编排抽成与 `tauri::State` 无关的 service 函数（现有 `*_impl` 已是这个方向的雏形，如 `update_entry_metadata_impl`、`execute_rename_impl`）；
2. 用 `AppState` 作为 axum `State`，为每个命令写一个 thin handler，保持 camelCase JSON；
3. 前端引入唯一 `api` 适配层，按 `window.__TAURI_INTERNALS__` 切换 IPC / HTTP；
4. 事件用 SSE/WebSocket 替代 Tauri event，解决 §2.3 的「只写库不通知」。

在此之前，README 的 axum 描述应删除，或明确标注为「历史方案」。

---

## 6. 图谱视图（React Flow）与数据来源

### 6.1 数据源

- 唯一来源是 `get_entries` 返回的 `Entry[]`，其中每个 entry 内嵌 `relations`（`StoredRelation[]`）。
- `MapView` 不请求图数据接口；nodes/edges 全部在 `useMemo` 里从 `entries` 派生（`MapView.tsx:127-185`）。

### 6.2 布局与交互

- **布局是确定性的**：按 `group` 首次出现顺序分桶，每桶固定列宽 `GROUP_COL_WIDTH`，逐行堆叠；颜色由 group 名 hash 取 `GROUP_PALETTE`（`MapView.tsx:97-161`）。没有 d3-force、dagre、elk。
- `onNodesChange` 直接 `return changes`，不写回 state（`MapView.tsx:187-192`）；`nodes` 每次由 `entries` 重算，所以拖拽不会持久，松手后位置由布局函数决定。
- `layout_store.rs` 的 `load_layout/save_layout/update_position` 没有任何 Tauri 命令注册，前端也无 `invoke`；`.synapse-layout.json` 除 `rename_entry_id` 可能改写外，不参与渲染。**该模块当前是死代码**。

### 6.3 前后端类型契约漂移（图谱直接受害）

| 位置 | 后端实际 JSON | 前端声明/使用 | 后果 |
|---|---|---|---|
| 关系类型 | `relationType`（`index.rs:115-121`） | `Relation.type`（`shared/types.ts:19`），`MapView.tsx:168/174/176/180` 用 `rel.type` | `rel.type === undefined`；`relationFilter.has(undefined)` 恒 false → **边被全部过滤** |
| 关系 id | 后端无 `id` 字段 | `Relation.id` 必填（`shared/types.ts:16`） | 运行时 `undefined`；MapView 自造 edge id 暂未爆炸，其他消费者有隐患 |
| 附件类型 | `attachmentType` | `Attachment.type` | 当前 UI 未消费，暂不显现 |
| 内容路径 | 后端无此字段 | `Entry.contentPath` 必填 | 运行时 `undefined`，TypeScript 不报错 |
| 关系方向 | 每条关系会出现在 `from` 与 `to` 两个 entry 的 `relations` 里（`get_relations` 用 `from_id = ? OR to_id = ?`，`index.rs:530-543`） | `MapView` 对每个 entry 都按「出边」渲染：`source = entry.id, target = rel.toId` | 修正 `type` 后，入边会变成 `B → B` 自环，且同一条关系被画两次 |
| 默认过滤 | — | `relationFilter` 初始集合不含 `custom`（`useAppStore.ts:74`） | 即使类型修正，`custom` 关系默认不可见 |

`DetailPanel` 对入/出边的拆分是对的（按 `fromId` 判断），但同样读 `rel.type`，会显示空类型。这个漂移之所以能通过 `npm run build`，是因为 `shared/types.ts` 是手写的，TS 只校验「声明内部自洽」，不校验与 Rust serde 的一致性。

---

## 7. 构建、运行与测试脚本

| 项 | 内容 |
|---|---|
| 前端 | `package.json`：`dev` / `build` / `preview` / `test:unit`（`test` 同义）/ `tauri` / `tauri:dev` / `tauri:build`；无 lint 脚本 |
| 构建 | `build` = `tsc && vite build`；Tauri `beforeDevCommand: npm run dev`、`beforeBuildCommand: npm run build`、`frontendDist: ../dist`、`devUrl: http://localhost:1420`（`tauri.conf.json:6-11`） |
| Rust | `[[bin]] synapse`（CLI）与 `synapse-app`（GUI）；`default-run = "synapse-app"` |
| feature | `default = ["tauri"]`；`tauri` feature 控制 `tauri`/`tauri-build` 可选依赖。**`cargo build --bin synapse` 默认仍会编译 Tauri**，需显式 `--no-default-features` |
| 测试 | `cd src-tauri && cargo test`；测试内嵌在 `#[cfg(test)]`，覆盖 parser/scanner/index/paths/layout_store/error/commands/watcher。前端 `npm run test:unit`（`scripts/run-unit-tests.mjs`，esbuild + `node --test`） |
| 安全 | `capabilities/default.json` 只给 `dialog:default`、`store:default`；`tauri.conf.json:23` `csp: null` |
| 平台 | 桌面 Tauri 2（macOS 当前环境）；无 Docker、无 HTTP 服务 |

---

## 8. 结构性风险清单（按影响排序）

| # | 风险 | 证据 | 影响 | 建议 |
|---|---|---|---|---|
| R1 | 关系类型不落盘，任何 rebuild 退化为 `reference` | `commands.rs` 的 `append_reference_line`；`index.rs:656-703` 只插 `reference` | 用户标注的 `derived/contradicts/...` 会静默丢失 | 在 Markdown 里持久化类型（如 `[[id|derived]]` 或 `@synapse-relations`），rebuild 时读回 |
| R2 | 关系字段名漂移 + 入边重复渲染 | `index.rs` `relationType` vs `shared/types.ts:19` `type`；`MapView.tsx:168-181`；`get_relations` 双向 | 图谱边可能完全不显示；修正后出现自环/重边 | 统一契约；MapView 只渲染 `rel.fromId === entry.id` 的边，或按 `(from,to,type)` 去重 |
| R3 | 索引/校验双格式、双解析器 | `index.rs:576` `read_md_file`；`validator/mod.rs` `scan_entries`；`entry.rs:48-105/110-155` | id 不一致；tags/labels/YAML/attachments 不入索引；元数据编辑被回读覆盖 | 选定一种格式；让索引与校验共用同一 `scan + parse` 函数 |
| R4 | 扁平格式下 E2/E4（根级 E6）系统性误报 | `rules.rs:60-69,73-111,146-165` | 节点/错误栏长期报错，修复流被噪声淹没 | 规则按实际格式派生 id、跳过 `.md` 后缀、扁平模式不查 E6 |
| R5 | 无事件推送，watcher 更新后 UI 陈旧 | 后端无 `emit`；前端无 `listen`；`lib.rs` watcher 只 rebuild | 外部编辑/批量导入后必须手动刷新 | 增加 `index-updated` 事件或单调递增 version + 前端轮询/监听 |
| R6 | 启动 rebuild 与首屏 `get_entries` 竞态；rebuild 清表在事务外 | `lib.rs:109-111` + `App.tsx:23-25`；`index.rs:555-560` | 首屏空白/数据闪烁 | 把 DELETE 放进同一事务；或 `get_entries` 等首次 rebuild 完成（ready flag） |
| R7 | SQLite FK 未开启，CASCADE 失效 | `Index::open` 无 `PRAGMA foreign_keys = ON`；`index.rs:348-350` | tags/attachments/relations 孤儿累积 | 打开 FK；或显式删除子表 |
| R8 | `execute_rename` 未校验 `new_name/new_group` | `commands.rs:1155-1164`；对比 `create_entry_simple` 使用的 `validate_filename/validate_parent_group`（`commands.rs:15,76`） | 路径穿越，可把文件写到 KB 之外 | 复用同一组校验，并 canonicalize 后确认仍在 `kb_root` 下 |
| R9 | 备份根目录推导只适配目录格式 | `fs/backup.rs:38-43` | 扁平格式备份写到 KB 上级目录 | 由调用方显式传入 `kb_root` |
| R10 | 全量 rebuild + N+1 读 + 同步 SQLite 阻塞 async worker | `index.rs:277-319`；`commands.rs:1029-1031` | 大 KB 每次编辑卡顿；rebuild 期间命令排队 | 增量更新；批量 join 查询；`spawn_blocking` 包 DB 操作 |
| R11 | 26 个命令中 8 个未接线，`validate_entry` 语义错误 | `lib.rs:114-141` 对比前端 `invoke` 列表；`commands.rs:560-566` | API 面持续漂移，测试/文档成本上升 | 删除或接线；修正 `validate_entry` 的 `all_ids` |
| R12 | layout_store 死代码 + 拖拽不落库 | `layout_store.rs`；`MapView.tsx:187-192`；handler 无 layout 命令 | 文档承诺的可拖拽布局不存在 | 接 `onNodesChange` + 持久化命令，或删除模块并更新文档 |
| R13 | `read_md_file` 与 `read_entry` 的字段能力差 6 项 | `entry.rs:85-105` vs `143-217` | 元数据编辑器/标签筛选在扁平 KB 上失效 | 合并为单一解析器，按路径形态决定 content 文件 |
| R14 | CSP 为 `null`，Markdown 外链/本地 HTML 无策略 | `tauri.conf.json:23` | WebView 安全面过大 | 设定最小 CSP；外链走系统浏览器 |

---

## 9. README / CLAUDE.md 与代码的不一致

| 文档说法 | 代码事实 | 证据 |
|---|---|---|
| README「axum local HTTP server (for Web build)」 | 无 axum 依赖、无服务代码、无 Web 入口 | `README.md:18`；`Cargo.toml`；git `bc2708d` 移除 |
| CLAUDE「folders of `content.md` files」 | 索引扫描的是任意 `*.md`，目录只是一个分组 | `CLAUDE.md:7`；`index.rs:564-577`；`scanner.rs:33-55` |
| CLAUDE「force-directed map」 | 确定性 group 网格布局 | `CLAUDE.md:7`；`MapView.tsx:111-161` |
| CLAUDE「startup force-rebuilt」 | 异步 `spawn`，且首屏不等待 | `CLAUDE.md:70`；`lib.rs:109-111` |
| CLAUDE「SQLite is the source of truth at runtime」 | `.md` 是事实源，DB 由文件重建；写命令先写文件 | `CLAUDE.md:93`；`commands.rs` 各写路径；`index.rs:551` |
| README「docs/standards.md 是格式规范」 | 该文档已自述被 v2 方案替代，且 v2 仍是目录格式，与索引扁平解析冲突 | `README.md:22`；`docs/standards.md:3-4`；`index.rs:576` |
| README「npm run build 能抓类型错误」 | 能抓 TS 内部错误，抓不到手写类型与 Rust serde 的漂移 | `README.md:37`；`shared/types.ts` |
| CLAUDE 列 `layout_store.rs` 为位置持久化 | 无命令、无前端调用 | `CLAUDE.md:66`；`lib.rs:114-141` |

---

## 10. 若要继续演进，建议的收敛顺序

1. **先统一「一种 KB 格式 + 一个解析器」**（R3/R13）：`scan → parse → validate → index` 共用同一 `RawEntry` 生成函数；直接在 `rebuild` 里按文件路径形态选择 `content.md` 或扁平文件，删除另一条扫描分支。
2. **修契约与图渲染**（R2）：让 `relationType` 与前端 `type` 对齐；入边只在 `fromId === entry.id` 时渲染；补 `custom` 默认过滤与自环去重。
3. **让关系类型可重建**（R1）：把类型写进 Markdown，rebuild 读回；否则任何 watcher 触发都是数据损失。
4. **补运行时一致性**（R5/R6/R7）：首次 rebuild 完成后置 ready；DELETE+INSERT 同事务；开启 `PRAGMA foreign_keys`；增加 `index-updated` 事件。
5. **收敛命令面与安全边界**（R8/R11/R14）：删除未接线命令；重命名统一走 `commands.rs` 的 `validate_filename/validate_parent_group`；设定最小 CSP。
6. **最后再谈 Web/axum**：先把 `*_impl` service 层抽出来，再决定是否值得维护第二条传输通道；在完成 1–4 之前加 axum 只会把双格式、双契约问题复制到第二套入口。

---

### 附：关键证据文件

- 启动/状态：`src-tauri/src/lib.rs:91-141,275-302`；`src-tauri/src/state.rs:19-51`
- 命令编排：`src-tauri/src/commands.rs:180,362,554,813,953-1054,1142-1192`
- 索引/关系：`src-tauri/src/index.rs:30-83,129-171,277-320,551-703`
- 解析双轨：`src-tauri/src/entry.rs:48-105,110-217`；`src-tauri/src/scanner.rs:10-55`
- 规则冲突：`src-tauri/src/validator/rules.rs:34-71,73-111,146-165`；`src-tauri/src/validator/mod.rs` `scan_entries`
- 前端图谱：`src/views/MapView.tsx:97-192`；`src/store/useAppStore.ts:74,84-92,279-315`
- 契约漂移：`shared/types.ts:15-21,29-43,50-68`
- 文档不一致：`README.md:12-22`；`CLAUDE.md:7,70,93-94`；`docs/standards.md:3-4`

---

## 11. 修复落地状态（2026-11-04）

> 本节记录按第 10 节收敛顺序实际落地的改动；上文 1–9 节保留为修复前的快照，其中
> 与下表冲突的描述以本节为准。

| 问题 | 状态 | 落地方式 |
|---|---|---|
| R3/R13 双格式、双解析器 | 已修 | `scanner::scan_entry_paths` 返回 `(paths, KbLayout)`；`entry::read_entry_path` 统一分派 `read_md_file` / `read_entry`；`parse_body` 统一解析 tags/labels/YAML/附件/typed relations。`index::rebuild`、`validator::validate_kb` 都走这一套 |
| R4 扁平格式 E2/E4/E6 误报 | 已修 | E2 扁平条目用 `flat_id_from_rel`（去 `.md` 后缀）；E6 仅在 `KbLayout::Directory` 生效；W6 跳过文件型条目 |
| R2 `relationType` vs `type` 契约漂移 | 已修 | `shared/types.ts` 的 `Relation.type` 改为 `relationType`；`Attachment.type` 改为 `attachmentType`；MapView 只渲染出边并按 `(from,to,type)` 去重 |
| R1 rebuild 关系类型退化 | 已修 | `@relation: <type> [[id]]` 解析进 `entry.typed_relations`；rebuild 先写显式类型，并对同 `(from,to)` 的隐式 reference 去重 |
| R7 SQLite FK 未开启 | 已修 | `Index::open` 执行 `PRAGMA foreign_keys = ON`；`upsert_entry_in_tx` 由 `INSERT OR REPLACE` 改为 `ON CONFLICT(id) DO UPDATE`，避免 REPLACE 级联误删子行 |
| R6 启动竞态 | 已修 | 启动 rebuild 持有 `startup_gate` 写锁，`get_entries` 持读锁；rebuild 的 DELETE + INSERT 放在同一事务内 |
| R12 layout 死代码 | 已修 | 新增 `get_layout` / `save_node_position` 命令；`MapView` 加载布局、拖拽落库，`execute_rename` 迁移 key；`layout_store` 内部串行化读-改-写，并用临时文件 + rename 原子落盘 |
| R8 rename 未校验 | 已修 | `execute_rename` 复用 `validate_filename` / `validate_parent_group`，并拒绝带 `.md` 后缀的新名，避免写出越界或错位路径 |
| group / 物理路径耦合 | 已修 | 新增 `dir`（真实相对子路径）与 `group`（顶层展示分类）双字段：扁平布局 `group` = `dir` 的首个非保留段，`dir` 保留完整层级；Sidebar 树 / MapView 空间聚类 / 新建 / 重命名全部改用 `dir`，`group` 仅用于展示；目录布局 `dir` 取 `@synapse-group`，缺失时回退物理父目录 |
| R5 无事件推送 | 已修 | watcher 重建完成后 `emit("kb-changed", payload)`：带 `origin`（写命令成功后按路径登记并校验期望状态，见 `State::all_paths_locally_written`）、重建内容指纹、单调 `revision` 与 `renamed` 增量；指纹与上次相同则不推送。前端 `src/events/kbRefresh.ts` 防抖合并，`local-write` 且指纹已同步时丢弃回声 |
| README/CLAUDE 反向描述 | 已修 | 删除 axum/Web 模式；改为「确定性分组网格 + 位置持久化」「Markdown 是事实源，SQLite 是可重建缓存」 |
| 前端无单测 / 固定 `/tmp` 残留 | 已修 | `npm run test:unit` 走 `scripts/run-unit-tests.mjs`：esbuild + `node --test`，产物在 `mkdtemp` 目录并在 `finally` 清理；启动时 `cleanup-temp.mjs` 只清理 `synapse-unit-*` 且 mtime 超过 24h 的目录 |

仍未处理：R9（`fs/backup.rs` 根目录推导）、R10（全量 rebuild / N+1 查询）、R14（CSP 为 `null`）。

## 12. 三批功能落地（2026-11-05）

本节补充批次 1–3 新增的模块与流程；上文 1–11 节仍然有效，冲突处以本节为准。

### 12.1 版本历史、回退与裁剪（批次 1）

- 存储位置：`<kb>/.synapse/history/<entry_key>/<version_id>.json|.md`。`entry_key` 由条目 id 的安全化字符串加 `fnv1a64` 的 8 位十六进制后缀组成，避免不同 id 安全化后重名（`history::entry_key`）。
- 版本元数据 `VersionMeta` 记录 `origin`（manual / autosave / external / restore / trim）、增删行摘要、内容哈希与 `content_stored`。正文超过 `MAX_STORED_CONTENT_BYTES`（2 MiB）时只存元数据并把 `content_stored` 置为 false。
- 去重与节流：普通保存若内容哈希与最新版本相同则不追加；`autosave` 距上一版本不足 `AUTOSAVE_MIN_INTERVAL_MS`（5 分钟）时跳过；回退总是追加新版本并带 `restored_from`，保证“回退也留痕”。
- 裁剪 `trim_history` 语义：保留 `keep_version_id` 及其之后的全部版本，删除更早的 `.json` + `.md`，随后追加一条 `origin=trim` 的审计元数据（`content_stored=false`，note 记录删除区间与释放字节）。`trim_preview` 先给出删除数量与可释放空间，UI 必须预览并二次确认。裁剪命令持有 `write_lock`，且不可恢复。
- 告警 `warnings`：单条目非 trim 版本超过 `PER_ENTRY_WARN_COUNT`（200）或历史目录总大小超过 `TOTAL_WARN_BYTES`（5 GiB）时提示。
- 回退 `restore_entry_version` 读取历史正文后走 `write_entry_content(origin=restore, restored_from=version_id)`，因此当前文件被覆盖前同样会先记录一条版本。
- 前端入口集中在 `src/components/VersionHistorySection.tsx` 与 `src/utils/lineDiff.ts`；命令层为 `list_entry_versions` / `get_entry_version_content` / `get_history_warnings` / `preview_trim_entry_history` / `trim_entry_history` / `restore_entry_version`。

### 12.2 编辑器草稿、自动保存与外部变更（批次 1）

- 原子写入统一走 `content_io::write_atomic`：同目录临时文件写入后 rename，失败时原文件保持完整。
- 草稿按 `kb id + entry id` 隔离存放在 localStorage（`src/utils/editorDraft.ts` 的 `draftKey` / `readDraft` / `writeDraft` / `clearDraft` / `pendingDraft`）。编辑器每 `DRAFT_AUTOSAVE_MS`（30 秒）写一次草稿；恢复草稿使用 `pendingDraft`，避免把其它条目的草稿串到当前条目。
- 外部变更：watcher 推送后 `DetailPanel` 对比磁盘内容与编辑器基准。无未保存修改时替换编辑器内容并清理草稿；有未保存修改时保留编辑内容，提示用户选择“重新加载磁盘版本”或“保留我的编辑”。草稿归属用 ref 校验，避免切换条目后误写。
- 保存结果通过 `SaveEntryResult` 返回是否真的写盘及变更行摘要，供 UI 显示“已保存/无变化”。

### 12.3 无损排版（批次 1）

- `formatter::format_content` 只调整版面，不改写语义。受保护区域包括 YAML front matter、围栏代码块、表格、`@synapse` 元数据注释、wikilink、行内代码与链接目标，均原样保留；这些区域的识别用逐字符扫描而非正则替换。
- 规范化内容：标题 `#` 后空格、列表标记与缩进、CJK 与拉丁字符之间补空格、行尾空白；不压缩连续空行，保留 CRLF / 混合换行，输出幂等。
- 命令 `preview_format_entry` 返回 `FormatOutcome`（成品、是否变化、变更行数），`apply_format_entry` 在写锁内写回；前端用 `lineDiff.ts` 的 `changedLinePairs` 在预览对话框里高亮差异。

### 12.4 同 KB 移动（批次 1）

- `move_entry_impl` 从旧路径推导名称（目录取目录名，文件取去 `.md` 的主干名），随后复用重命名管线 `execute_rename_impl(id, name, target_dir, cascade=true)`，因此移动与改名共享校验、级联、历史与布局迁移及回滚。
- UI 的移动对话框（`DetailPanel`）展示目标目录、迁移后的 id、附件是否随目录迁移与失败回滚说明；成功后按新 id 重新选中。

### 12.5 模版字头与冲突处理（批次 2）

- KB 配置经 `get_kb_config` / `update_kb_config` 读取和写回 `.synapse-config.yaml`；`useAppStore.loadKbConfig` 在切换 KB 后加载，`saveTemplateHeader` 做读-改-写，只更新 `templateHeader`，保留配置里的其它键。
- `src/utils/templateHeader.ts` 是纯逻辑：`findFrontMatter` / `bodyStartLine` 定位正文起点（跳过 front matter、开头的空行与元数据注释），`insertTemplateHeader` 返回 `inserted` / `already-present` / `conflict` / `empty`。
- 冲突规则：字头本身是 YAML front matter 而文档已有 front matter 时返回 `conflict`，绝不静默替换；正文已含相同字头时返回 `already-present`。插入成功后仍走普通保存流程记录版本。Settings 提供字头编辑区，DetailPanel 提供插入按钮与状态提示。

### 12.6 快捷关联与 `@relation` 写入（批次 2）

- `RelationDialog` 支持按标题/id/标签搜索目标，并用 `hasSameType` 阻止同类型关系重复创建；`beforeCreate` 钩子先保存脏编辑器，避免后端追加关系时丢弃未保存正文。
- `create_relation` 在来源正文末尾追加一行 `@relation: <type> [[<target id>]]`，落盘成功后登记本地写入并写入索引关系表；store 层把命令成功/异常折算为布尔值，供对话框显示失败原因。

### 12.7 目录迁移管线（批次 3）

`precheck_rename` 先返回 `valid`、`newId`、`conflictWith`、`incomingCount` / `incomingFiles`（会被级联改写的引用），供 UI 预览；`execute_rename_impl` 在 `write_lock` 内按序执行：

1. 校验新名与新目录（复用新建条目的 `validate_filename` / `validate_parent_group`，拒绝 `.md` 后缀）。
2. 解析旧路径，按目录/扁平格式计算 `new_path` 与 `new_id`。
3. 冲突检查：先查 id 冲突，再查路径已存在。
4. 级联把其它文档中的 `[[old_id]]` 替换为 `[[new_id]]`（跳过代码块），并缓存原文用于回滚。
5. 移动文件/目录（目录条目连同附件一起移动）。
6. 更新条目的 `@synapse-id` 与 id 文件、改写 SQLite 索引行。
7. 迁移历史目录（`history::migrate_entry_history`，目标已存在则报错而不覆盖）。
8. 迁移 `.synapse-layout.json` 的键。
9. rebuild 索引刷新派生字段。

任一步失败调用 `rollback_rename_state`：恢复级联文件、DB 行、id 文件与文件移动；历史迁移失败返回 `HistoryMigrationFailed`，布局迁移失败先把历史搬回旧 id 再回滚，返回 `LayoutMigrationFailed`。若回滚本身有残留，错误信息附带 `rollback incomplete: ...`，UI 明确提示而不是假装成功。

### 12.8 阅读排版的 CSS 变量与默认/恢复默认（批次 3）

- `src/store/readingPrefsStore.ts` 持久化到 localStorage `synapse.readingPrefs`，`normalizeReadingPrefs` 把外部值收敛到合法区间：字号 13–20、行高 1.4–2.2、段间距 0.2–1.4em、最大宽度 50–110ch（0 表示不限制）、代码字号 11–16、表格样式 bordered/striped。
- 默认值：字号 15px、行高 1.7、段间距 0.7em、最大宽度 78ch、代码字号 12.5px、bordered。
- `MarkdownView` 把偏好写成 CSS 变量 `--md-font-size` / `--md-line-height` / `--md-paragraph-gap` / `--md-max-width` / `--md-code-size`，并以 `data-table-style` 切换表格样式；`styles.css` 的 Markdown 规则读取这些变量。
- Settings 提供滑块与“恢复默认”（`reset`），恢复默认同时把持久化值写回默认值。

### 12.9 面板过渡与 `prefers-reduced-motion`（批次 3）

- `.panel-enter` 使用 180ms `cubic-bezier(0.2, 0.8, 0.2, 1)` 淡入上移；`.list-enter` 使用 160ms ease-out。分别用于右面板、侧栏列表与详情面板的关系/标签列表。
- `@media (prefers-reduced-motion: reduce)` 下两类动画均禁用，避免对动效敏感的用户产生干扰。

### 12.10 指纹缓存与测试隔离（批次 1）

- `fingerprint.rs` 生产路径使用进程级 `Mutex` 缓存；`cfg(test)` 下改用 `thread_local` 缓存，避免并行测试共享缓存造成 flake。
- `commands.rs` 与 `lib.rs` 的测试临时 DB/registry 路径在时间戳后追加进程内 `AtomicU64` 自增序号，消除同一毫秒内创建同名路径的碰撞。
