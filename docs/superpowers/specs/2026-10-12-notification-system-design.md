# Synapse KB 通知系统重构设计

**代号**：通知三入口 + E 级修复系统
**日期**：2026-10-12
**状态**：待用户审阅
**关联文档**：
- [Synapse-KB 设计文档](./2026-10-09-synapse-kb-design.md)
- [国际化设计文档](./2026-10-11-i18n-design.md)
- [UI 布局重构](./2026-10-12-ui-layout-redesign-design.md)
- [KB Standards v2](./2026-10-12-kb-standards-v2-design.md)

---

## §0 目标与背景

当前 Synapse KB 桌面应用的通知系统存在以下问题：

**Bug**：
1. `ValidationPanel.tsx:34` — `kbRoot: '.'` 硬编码，校验结果永远是空（用的是进程当前目录）
2. `ValidationPanel.tsx:29` — `useTranslation('validation')` 大小写与注册名 `'Validation'` 不一致
3. `NodeCard.tsx` 红点仅 `title` tooltip，无点击交互
4. `tRule.ts` — 找不到 i18n key 时返回空 message，"看不到任何信息"的根因
5. 校验规则 i18n 覆盖不全：缺 W1/W2/W3 等多规则消息

**用户视角的痛点**：
- 红点/红条不知道能不能点，点了看不到信息
- 看到错误不知道"怎么修"
- 修复路径不透明，可能改坏文件

**本次重构目标**：
1. **三入口联动**：顶部 ErrorBar 汇总 + 节点红点气泡 + 右侧 ValidationPanel 修复面板
2. **完整错误方案系统**：每个 E 级规则都有结构化"原因 + 步骤 + 预览"文档
3. **多级修复能力**：OneClick / Confirm / Manual 三种类型，按风险分层
4. **安全可回滚**：每次自动修复前自动原文件，可恢复
5. **i18n 全量补齐 E 级规则消息**（W/I 维持现状，不在本期范围）

**不在范围内**：
- W/I 级规则的交互修复（仅显示 + 文档）
- 备份自动清理（30 天过期）
- 修复审计日志
- 撤销栈（多次修复一键还原）

---

## §1 架构概览

### 1.1 数据流

```
       ┌─────────────────────────────────────────┐
       │ Rust backend (src-tauri/)               │
       │                                         │
       │  validator/rules.rs                      │
       │    每个 E 规则同时声明:                   │
       │      - detect(&entry) -> Vec<Issue>      │
       │      - fix_meta(&issue) -> FixDescriptor │
       │                                         │
       │  validator/fix.rs (新)                   │
       │    FixDescriptor {                       │
       │      kind: OneClick | Confirm | Manual   │
       │      risk:  Low | Medium | High          │
       │      label_key: "Fix.E1.apply"          │
       │      preview_key:  "Fix.E1.preview"     │
       │      docs_key:     "Fix.E1.docsBody"    │
       │      apply: fn(entry) -> Result<()>      │
       │    }                                     │
       │                                         │
       │  fs/backup.rs (新)                       │
       │    backup_then_write(path, content)       │
       │    list_backups(kb_root) -> Vec<Backup>   │
       │    restore_backup(backup_id)             │
       │                                         │
       │  commands/                               │
       │    validate_kb(kb_root) -> Report        │
       │    apply_fix(entry_id, rule_id, confirm) │
       │    list_backups()                        │
       │    restore_backup(backup_id)             │
       └─────────────────────────────────────────┘
                          │ Tauri invoke
                          ▼
       ┌─────────────────────────────────────────┐
       │ React frontend                          │
       │                                         │
       │  App.tsx                                │
       │    ├─ <ErrorBar/>   (顶部条)            │
       │    └─ 右/侧两层:                          │
       │        <FixDrawer/>  (节点红点→打开)    │
       │        <RightPanel>  (右侧面板)          │
       │          ├─ <ValidationPanel/>           │
       │          └─ <FixWizard/>                 │
       │                                         │
       │  useFixStore (新)                       │
       │    active entry_id, pending fix         │
       └─────────────────────────────────────────┘
```

### 1.2 三入口职责

| 入口 | 触发 | 展示 | 操作 |
|---|---|---|---|
| 顶部 ErrorBar | 自动（任意 issue） | 全局汇总："3 errors · 5 warnings" | 点击 → 打开右侧 ValidationPanel 跳到对应 tab |
| 节点红点 | 红点 badge | 该 entry 的 issue 列表 + 修复按钮 | 点击按钮 → OneClick / Confirm / Manual |
| 右侧 ValidationPanel | 手动按钮 / 顶部条 | 按 entry 分组的全量 issue + 详细方案 + 跳转 | 一键修复 + 跳转节点 |

### 1.3 事务模型

```
apply_fix 流程：
  1. 取 write_lock(state.write_lock)
  2. 读原 entry → 验证 hash 未变
  3. 读 report 中该 entry 的当前 issue（确认 fix_id 仍存在）
  4. 生成新 content
  5. backup_then_write(entry.path, new_content)
       备份路径: .synapse-fix-backup/{ts}_{entry_id}.md
  6. rebuild_index(current_kb)  // 同步 sqlite
  7. 释放锁
失败任意步 → 回滚到备份，返回错误给前端
```

---

## §2 Rust 后端模块

### 2.1 新模块布局

```
src-tauri/src/
├── validator/
│   ├── mod.rs              # ValidationReport, Severity (现有)
│   ├── rules.rs            # E/W/I 检测 (现有)
│   ├── fix.rs              # 新：FixDescriptor, FixKind, FixRisk
│   └── fix_impls/          # 新：每个 E 规则的具体修复实现
│       ├── mod.rs           # 暴露 fix_for(rule_id, issue, entry)
│       ├── e1_add_id.rs
│       ├── e2_sync_id.rs
│       ├── e3_add_title.rs
│       ├── e4_default_type.rs
│       ├── e5_move_deep.rs    # Manual 类，仅文档指引
│       ├── e6_remove_root_md.rs
│       ├── e7_rename_reserved.rs  # Manual 类
│       └── e9_strip_system.rs
├── fs/
│   └── backup.rs           # 新：备份/恢复
└── commands.rs             # 新增 apply_fix / list_backups / restore_backup
```

### 2.2 FixDescriptor 数据结构

```rust
// src-tauri/src/validator/fix.rs

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixDescriptor {
    pub rule_id: String,                  // "E1"
    pub kind: FixKind,
    pub risk: FixRisk,
    pub label_key: String,                // 前端 i18n: "Fix.E1.apply"
    pub preview_key: Option<String>,      // OneClick 才有
    pub confirm_key: Option<String>,      // Confirm 才有
    pub docs_key: String,                 // 详细方案: "Fix.E1.docsBody"
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FixKind {
    /// 一键直接修，无参数。例：补默认 type 标签
    OneClick,
    /// 一键但需用户显式确认。例：删除 root 级 md
    Confirm,
    /// 不可自动修，打开编辑器 + 文档
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FixRisk { Low, Medium, High }
```

### 2.3 E 级修复能力矩阵（首期）

| 规则 | 标题 | kind | risk | 一键做什么 |
|---|---|---|---|---|
| E1 | 缺少 ID | OneClick | Low | 从路径推导 id，写入 `@synapse-id: <derived>` |
| E2 | id 与路径不一致 | OneClick | Low | 改 `@synapse-id` 与路径一致 |
| E3 | 缺少 title | OneClick | Low | 写入 `@synapse-title: <id>` |
| E4 | 缺 type 标签 | OneClick | Low | 追加 `type:note` |
| E5 | 目录深度过深 | Manual | — | 文档：移动到浅层 |
| E6 | root 级 .md | Confirm | High | 移入 `entries/<basename>/` 并包裹 content.md |
| E7 | 保留前缀目录 | Manual | — | 文档：移除 `_xxx` 前缀 |
| E9 | 用户写了 system label | Confirm | Medium | 删除该注释行 |

### 2.4 修复注册入口

```rust
// src-tauri/src/validator/fix_impls/mod.rs

pub fn fix_for(
    rule_id: &str,
    entry: &RawEntry,
    entry_dir: &Path,
) -> AppResult<FixDescriptor> {
    match rule_id {
        "E1" => e1_add_id::descriptor_and_apply(entry, entry_dir),
        "E2" => e2_sync_id::descriptor_and_apply(entry, entry_dir),
        "E3" => e3_add_title::descriptor_and_apply(entry, entry_dir),
        "E4" => e4_default_type::descriptor_and_apply(entry, entry_dir),
        "E5" => Ok(e5_move_deep::descriptor_only()),
        "E6" => e6_remove_root_md::descriptor_and_apply(entry, entry_dir),
        "E7" => Ok(e7_rename_reserved::descriptor_only()),
        "E9" => e9_strip_system::descriptor_and_apply(entry, entry_dir),
        _ => Err(AppError::UnknownRule(rule_id.into())),
    }
}
```

### 2.5 备份模块

```rust
// src-tauri/src/fs/backup.rs

pub struct Backup {
    pub id: String,            // "{ts}_{entry_id}"
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub created_at: u64,
    pub rule_id: String,
}

pub fn backup_dir(kb_root: &Path) -> PathBuf {
    kb_root.join(".synapse-fix-backup")
}

pub fn backup_then_write(
    target: &Path,
    new_content: &str,
    rule_id: &str,
) -> AppResult<Backup>;

pub fn list_backups(kb_root: &Path) -> Vec<Backup>;

pub fn restore_backup(backup: &Backup) -> AppResult<()>;
```

**关键约束**：
- 备份目录在 KB 根下 `.synapse-fix-backup/`，不进 git
- 备份保留 30 天（或可配置），过期扫描清理 — **本期不做**
- 备份文件命名：`{unix_ts}_{entry_id_safe}.md`

### 2.6 commands.rs 新增

```rust
#[tauri::command(rename_all = "camelCase")]
pub async fn apply_fix(
    state: State<'_, AppState>,
    entry_id: String,
    rule_id: String,
    confirm: Option<bool>,        // Confirm 类需要显式 true
) -> AppResult<ApplyResult>;

#[tauri::command(rename_all = "camelCase")]
pub async fn list_backups(state: State<'_, AppState>) -> Vec<Backup>;

#[tauri::command(rename_all = "camelCase")]
pub async fn restore_backup(state: State<'_, AppState>, backup_id: String) -> AppResult<()>;
```

---

## §3 前端组件设计

### 3.1 新/改文件清单

```
src/
├── components/
│   ├── ErrorBar.tsx          # 改：汇总+点击打开面板，跳转 tab
│   ├── NodeCard.tsx          # 改：红点变 button，可点击；打开 FixDrawer
│   ├── FixDrawer.tsx         # 新：节点级修复抽屉
│   ├── FixWizard.tsx         # 新：右侧面板内的修复向导
│   └── ValidationPanel.tsx   # 改：用 active KB，分组/筛选/跳转
├── store/
│   ├── panelStore.ts         # 改：增加 fix panel 类型
│   └── fixStore.ts           # 新：active entry, pending fix, recent fixes
└── i18n/
    ├── Errors.zh.json        # 扩展：规则全文+修复按钮文案
    ├── Errors.en.json
    ├── Fix.zh.json           # 新：fix.* 文案
    ├── Fix.en.json
    └── resources.ts          # 注册新 namespace
```

### 3.2 NodeCard 红点改造

```tsx
{badge && (
  <button
    className={`absolute -right-1 -top-1 flex h-4 w-4 items-center justify-center rounded-full text-[10px] font-bold text-white ${badge.color} cursor-pointer transition hover:scale-110`}
    title={badge.title}
    aria-label={badge.title}
    onClick={(e) => {
      e.stopPropagation();
      openFixDrawer(data.entryId);
    }}
  >
    {badge.count}
  </button>
)}
```

**变化点**：
- `<span>` → `<button>`
- 数字显式（不再是 `!`）
- hover 缩放反馈
- aria-label 用 i18n

### 3.3 FixDrawer（节点级）

**打开时机**：点节点红点 / 点 ValidationPanel 中某 entry 链接
**位置**：居中右侧滑出（不占全屏）
**内容结构**：

```
┌─────────────────────────────────────┐
│  修复: <entry-title>            [×] │
├─────────────────────────────────────┤
│  ❌ E1 缺少 ID                       │
│    @synapse-id 不能为空              │
│    预览: @synapse-id: react-hooks    │
│    [一键修复]    [查看文档]           │
│                                     │
│  ⚠️ W4 悬空链接                     │
│    [[old-target]] 目标不存在         │
│    [查看文档]                        │
├─────────────────────────────────────┤
│  [全部自动修复 (1)]                  │
└─────────────────────────────────────┘
```

- OneClick 按钮直接调 `apply_fix`，成功 → toast → 自动重新校验
- Manual 按钮 → 展开文档（折叠区）
- Confirm 按钮 → 弹二级确认框，再调 `apply_fix(confirm=true)`
- Esc 关闭抽屉

### 3.4 ValidationPanel（右侧面板）

**核心修改**：用 active KB、修大小写、分组渲染。

```tsx
const activeKbRoot = useAppStore((s) => s.activeKbRoot);
const [report, setReport] = useState<Report | null>(null);
const [tab, setTab] = useState<Severity>('Error');
const [groupBy, setGroupBy] = useState<'severity' | 'entry'>('severity');

useEffect(() => {
  if (!activeKbRoot) return;
  invoke('validate_kb', { kbRoot: activeKbRoot }).then(setReport);
}, [activeKbRoot]);

const filtered = useMemo(() => filterIssues(report?.issues, tab), [report, tab]);
const grouped = useMemo(() => groupIssues(filtered, groupBy), [filtered, groupBy]);
```

**分组渲染**：
- `groupBy=severity`：按 tab 全部平铺
- `groupBy=entry`：按 entry_id 折叠，每组显示 entry 名 + 错误数

**每条目**：
```
[E1] @synapse-id 不能为空
entry: react-hooks
  [一键修复]  [跳到节点]  [查看文档]
```

**顶部按钮**：重新校验 / 导出 JSON / 切换分组 / 全部一键修复（E 级可自动的）

### 3.5 useFixStore

```ts
interface FixStore {
  // 节点红点抽屉
  drawerEntryId: string | null;
  openDrawer: (id: string) => void;
  closeDrawer: () => void;

  //全局最近修复
  recentFixes: ApplyResult[];
  pushFix: (r: ApplyResult) => void;

  // 一键全部修复队列
  autoFixQueue: FixTarget[];
  startAutoFix: (entryId: string) => void;
}
```

### 3.6 错误处理 UI

| 场景 | 行为 |
|---|---|
| 修复成功 | Toast "已修复 E1"，更新 report，节点红点减 1 |
| 修复失败（规则变更） | Toast "修复失败：<reason>"，提供 "查看错误" 链接 |
| 备份失败 | 拒绝修复，Toast "备份失败，拒绝继续" |
| 并发冲突（文件已变更） | 检测文件 hash，弹框 "文件已变更，请重新校验" |
| active KB 不存在 | ValidationPanel 显示空态，ErrorBar 不显示 |
| 规则已移除 | Toast "该规则已被移除"，重新校验 |

### 3.7 a11y / 键盘

- ErrorBar / 红点 / FixDrawer 都有 `role` + `aria-label`
- `Esc` 关闭 FixDrawer
- `Tab` 顺序：标题 → 主操作 → 次要操作
- 焦点陷阱在抽屉打开时启用

---

## §4 i18n 字符串体系

### 4.1 Namespace 划分

| Namespace | 内容 | 现状 |
|---|---|---|
| `Validation.zh/en.json` | 规则标题/描述（每个 E/W/I 规则的 title/message） | 已存在但不全，扩展 |
| `Error.zh/en.json`（新建） | 顶部条文案（汇总、点击查看等） | 新建 |
| `Fix.zh/en.json`（新建） | 修复按钮文案、确认文案、文档提示 | 新建 |
| `ValidationPanel.zh/en.json` | 面板按钮（重扫、导出、切换分组等） | 已存在，扩展 |

### 4.2 key 命名约定

```
Validation.rule.{RULE_ID}.title      — "缺少 ID"
Validation.rule.{RULE_ID}.message     — "@synapse-id 不能为空"
Validation.rule.{RULE_ID}.cause       — 新：为什么会出现（仅 E 级）
Validation.rule.{RULE_ID}.fixGuide    — 新：分步修复指引（仅 E 级）

Fix.{RULE_ID}.apply                   — "从目录名派生 ID"（仅 E 级）
Fix.{RULE_ID}.preview                 — "将写入 @synapse-id: {{derived}}"
Fix.{RULE_ID}.confirmTitle            — "确认删除 root 级文件？"
Fix.{RULE_ID}.confirmBody             — "该操作不可逆，但可从备份恢复"
Fix.{RULE_ID}.docsTitle               — "为什么需要 ID"
Fix.{RULE_ID}.docsBody                — 长文档（Markdown）

Fix.common.success                    — "已修复"
Fix.common.failed                     — "修复失败：{{reason}}"
Fix.common.backupOk                   — "已备份至 {{path}}"
Fix.common.restoreOk                  — "已从备份恢复"

Error.summary                         — "{{errors}} 个错误 · {{warnings}} 个警告"
Error.tooltipWithCounts               — "{{errors}} 错误 / {{warnings}} 警告"
Error.nodeBadge                       — "{{count}}"
Error.empty                           — "无问题"
```

### 4.3 E1 在 Validation.zh.json 中的完整定义

```json
{
  "rule": {
    "E1": {
      "title": "缺少 ID",
      "message": "@synapse-id 不能为空",
      "cause": "每个 entry 必须有唯一 ID，由 @synapse-id 注释或目录名决定",
      "fixGuide": "1. 打开 content.md\n2. 在头部加上 `<!-- @synapse-id: <派生id> -->`\n3. 或重命名目录为期望 ID"
    }
  }
}
```

### 4.4 加载策略

- 启动时 `initI18n()` 加载全部 namespace（包括新建的 `Fix` / `Error`）
- 缺失 key 走 `defaultValue` + `console.warn`（不让 UI 卡住）
- CI 校验：扫描 Rust 规则 ID 列表 vs JSON keys，缺失则 fail

### 4.5 一致性保证

- Rust 端 `FixDescriptor.label_key` 必须以 `Fix.` 开头
- 前端 `useFixI18n(ruleId)` 工具函数统一取 4 个 key，避免散落
- `tRule` 改造：找不到时返回 `{ title: rule_id, message: '' }` 而不是空对象，便于前端容错

---

## §5 错误处理 / 测试 / 风险

### 5.1 错误处理矩阵

| 失败场景 | 检测位置 | UI 行为 | 恢复策略 |
|---|---|---|---|
| 备份写失败 | `backup_then_write` | Toast: "备份失败，已中止修复" | 不写新文件 |
| entry 文件被外部修改 | 修复前算 hash，与 report 时 hash 比对 | 弹框 "文件已变更，请重新校验" | 重新校验后再试 |
| 规则已移除 | `apply_fix` 返回 unknown rule | Toast "该规则已被移除" | 重新校验 |
| 并发修复冲突 | `state.write_lock` | 后到者排队 | 用户看到进度提示 |
| KB 未激活 | ErrorBar/ValidationPanel 渲染前 | 不显示 | 等 `refreshKbs` 完成 |
| 修复后 issue 仍存在 | 重新校验发现 | 节点红点不消失 + Toast "修复未生效，请查看文档" | 用户查看文档 |
| 备份目录满了 | 启动时 `list_backups` 超阈值（>1GB） | 提示清理旧备份 | 用户手动清理 |
| 备份文件损坏 | 校验 backup 时读失败 | 不阻止新修复 | 旧备份不参与回滚列表 |

### 5.2 测试策略

**Rust 单元测试**（`#[cfg(test)]`）：
```
src-tauri/src/validator/fix_impls/e1_add_id.rs
  - 正常：缺 id，从路径派生，写回
  - 边界：路径含中文/特殊字符
  - 边界：备份已存在不同 entry_id（不覆盖）

src-tauri/src/fs/backup.rs
  - 备份-还原一致性
  - 备份列表按 ts 排序
  - restore_backup 把文件恢复到原位

src-tauri/src/commands.rs::apply_fix
  - OneClick 类不传 confirm 也成功
  - Confirm 类不传 confirm 返回 Err
  - 写入失败时回滚到备份
```

**前端类型与构建**：
- `npm run build` 通过（严格 TS）
- 无新 `any`

**集成验证**（按 docs/superpowers/verification/）：
- 每个 E 规则至少 1 个 fixture 验证修复链路
- 文档：`docs/superpowers/verification/2026-10-12-notification-verify.md`

### 5.3 风险与缓解

| 风险 | 缓解 |
|---|---|
| 自动修复改坏文件 | 备份 + 事务回滚；首期仅 Low/Medium risk 自动；High 必须 confirm |
| `apply_fix` 与 `rebuild_index` 不一致 | 复用 `state.write_lock`；rebuild 在锁内 |
| 前端错把 "打开文件" 按钮当成修复 | kind 字段强类型，前端按 kind 渲染不同按钮 |
| E6/E9 等 Confirm 类被绕过 | 后端强制要求 `confirm=true` 参数 |
| 备份膨胀 | 后续实现 GC；本期仅记录 README 提示 |
| 多语言文案漂移 | CI 校验 Rust rule IDs 与 i18n keys 对齐 |

---

## §6 文档产出

- `docs/superpowers/specs/2026-10-12-notification-system-design.md`（本文件）
- `docs/superpowers/verification/2026-10-12-notification-verify.md`（验收清单）
- `docs/standards.md` 增加 "E 级修复" 章节，说明触发与回滚
- `README.md`（如有）增加 "修复工作流" 小节

---

## §7 边界情况

1. **节点红点点击与节点选中冲突**：`stopPropagation` 阻止触发节点 onClick
2. **ValidationPanel 初始 report 为空**：显示 loading 状态，ErrorBar 仍可见
3. **修复过程中文件被外部修改**：hash 不匹配，弹框要求重新校验
4. **同一 entry 多规则同时修复**：前端按顺序串行调用 `apply_fix`，后端 `write_lock` 串行化
5. **修复后 issue 仍存在**：红点不消失 + Toast，提示用户查看文档
6. **active KB 切换时 report 失效**：`useEffect` 依赖 `activeKbRoot`，切换后重新校验
7. **修复按钮文案找不到 i18n key**：fallback 到 rule_id，不阻塞 UI

---

## §8 不在范围内（明确排除）

- ❌ W/I 级规则的交互修复（仅显示 + 文档）
- ❌ 备份自动清理（30 天过期）
- ❌ 修复审计日志（记录到 KB 的 `.synapse-fixes.log`）
- ❌ 撤销栈（多次修复一键回到原状）
- ❌ 多 KB 并行修复
- ❌ FilterPanel 实际功能实现
- ❌ 修复按钮的 AI 建议（仅人工文档指引）
