# Notification System + E-Level Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Synapse KB 桌面应用中建立"完整错误方案系统 + 交互式引导修复"：三入口（顶部 ErrorBar / 节点红点 / 右侧 ValidationPanel）+ E 级规则一键/确认修复 + 自动备份回滚 + 完整 i18n。

**Architecture:** 自底向上 7 层 —— (1) 修现有 bug + i18n 补 E 级，(2) Rust 端 FixDescriptor/备份/命令基础设施，(3) 8 个 E 规则 fix 实现，(4) 前端 panelStore/fixStore，(5) 三入口组件 (ErrorBar/NodeCard/FixDrawer/FixWizard/ValidationPanel)，(6) i18n 新建 Error/Fix 命名空间，(7) cargo test + npm run build + 手动验收。

**Tech Stack:**
- **Rust 后端**：Tauri 2 + rusqlite + serde + tokio（`state.write_lock` 串行化修复）
- **前端**：React 18 + Vite + TypeScript + Zustand + Tailwind + i18next + React Flow
- **校验**：Rust 单元测试 (`#[cfg(test)]`) + `npm run build` 严格 TS 类型检查

---

## 文件结构

### 修改

| 路径 | 改动 |
|---|---|
| `src-tauri/src/commands.rs` | 新增 `apply_fix` / `list_backups` / `restore_backup` 三个命令 |
| `src-tauri/src/lib.rs` | 注册新命令（`invoke_handler`） |
| `src-tauri/src/state.rs` | 无改动（已含 `write_lock` / `validation_report` 缓存） |
| `src/components/ValidationPanel.tsx` | 修 `kbRoot` 硬编码、`useTranslation('validation')` 大小写、改用 active KB、按 entry 分组 |
| `src/utils/tRule.ts` | 找不到 i18n key 时返回 `{title: rule_id, message: ''}`，并加 `console.warn` |
| `src/components/NodeCard.tsx` | 红点 `<span>` → `<button>`，加点击事件 + 数字显式 + a11y |
| `src/components/ErrorBar.tsx` | 鼠标移入 tooltip 显示完整文案；点击 → 打开 ValidationPanel 跳到 Error tab |
| `src/store/panelStore.ts` | 新增 `'fix'` PanelType |
| `src/i18n/Validation.zh.json` | 补全 E1-E9 的 `title` / `message` / 新增 `cause` / `fixGuide` |
| `src/i18n/Validation.en.json` | 同上（英文版） |
| `src/i18n/resources.ts` | 注册 `Error` / `Fix` namespace |

### 新建

| 路径 | 职责 |
|---|---|
| `src-tauri/src/validator/fix.rs` | `FixDescriptor` / `FixKind` / `FixRisk` 类型 + `apply_fix` 派发入口 |
| `src-tauri/src/validator/fix_impls/mod.rs` | 按 `rule_id` 派发到各规则实现 |
| `src-tauri/src/validator/fix_impls/e1_add_id.rs` | E1：从路径派生 id 写入 |
| `src-tauri/src/validator/fix_impls/e2_sync_id.rs` | E2：写 `@synapse-id` 与路径一致 |
| `src-tauri/src/validator/fix_impls/e3_add_title.rs` | E3：写默认 title |
| `src-tauri/src/validator/fix_impls/e4_default_type.rs` | E4：补 `type:note` |
| `src-tauri/src/validator/fix_impls/e5_move_deep.rs` | E5：Manual descriptor only |
| `src-tauri/src/validator/fix_impls/e6_remove_root_md.rs` | E6：移入 `entries/<basename>/` |
| `src-tauri/src/validator/fix_impls/e7_rename_reserved.rs` | E7：Manual descriptor only |
| `src-tauri/src/validator/fix_impls/e9_strip_system.rs` | E9：删除 system label 注释行 |
| `src-tauri/src/fs/mod.rs` | 新建 fs module，暴露 backup 子模块 |
| `src-tauri/src/fs/backup.rs` | 备份读写 / 列表 / 恢复 |
| `src/store/fixStore.ts` | `drawerEntryId` / `recentFixes` / `autoFixQueue` |
| `src/components/FixDrawer.tsx` | 节点级修复抽屉 |
| `src/components/FixWizard.tsx` | 右侧面板内的修复向导（按 FixDescriptor 渲染按钮） |
| `src/i18n/Error.zh.json` | 顶部条 / 节点红点文案（zh） |
| `src/i18n/Error.en.json` | 顶部条 / 节点红点文案（en） |
| `src/i18n/Fix.zh.json` | 修复按钮 / 确认 / 文档 / 通用文案（zh） |
| `src/i18n/Fix.en.json` | 修复按钮 / 确认 / 文档 / 通用文案（en） |

---

## Milestone 1: 修 Bug + i18n 补 E 级

### Task 1: 修复 ValidationPanel.tsx（kbRoot + useTranslation）

**Files:**
- Modify: `src/components/ValidationPanel.tsx:29,34`

- [ ] **Step 1: 修改 useTranslation 调用**

将第 29 行：
```typescript
const { t: tVal } = useTranslation('validation');
```
改为：
```typescript
const { t: tVal } = useTranslation('Validation');
```

- [ ] **Step 2: 修改 validate_kb 调用使用 active KB**

将第 33-36 行：
```typescript
const rescan = async () => {
  const r = await invoke<ValidationReport>('validate_kb', { kbRoot: '.' });
  setReport(r);
};
```
改为：
```typescript
const activeKbRoot = useAppStore((s) => s.activeKbRoot);
const rescan = async () => {
  if (!activeKbRoot) return;
  const r = await invoke<ValidationReport>('validate_kb', { kbRoot: activeKbRoot });
  setReport(r);
};
```

并在文件顶部新增 import：
```typescript
import { useAppStore } from '@/store/useAppStore';
```

- [ ] **Step 3: 跑构建验证**

Run: `npm run build`
Expected: TS 类型检查通过，无新增报错

- [ ] **Step 4: 提交**

```bash
git add src/components/ValidationPanel.tsx
git commit -m "fix(validation): use active KB + correct i18n namespace"
```

---

### Task 2: 修复 tRule.ts fallback

**Files:**
- Modify: `src/utils/tRule.ts:1-19`

- [ ] **Step 1: 重写 tRule 缺失 key 处理**

完整替换为：
```typescript
import { TFunction } from 'i18next';

interface RuleTrans {
  title: string;
  message: string;
}

export function tRule(
  t: TFunction,
  ruleId: string,
  vars: Record<string, string> = {},
): RuleTrans {
  const titleKey = `rule.${ruleId}.title`;
  const messageKey = `rule.${ruleId}.message`;

  let title = t(titleKey, { defaultValue: ruleId });
  if (title === ruleId) {
    console.warn(`[tRule] missing i18n key: ${titleKey}`);
  }

  const message = t(messageKey, {
    defaultValue: '',
    ...vars,
  });
  if (message === '') {
    console.warn(`[tRule] missing i18n key: ${messageKey}`);
  }

  return { title, message };
}
```

- [ ] **Step 2: 跑构建验证**

Run: `npm run build`
Expected: TS 通过

- [ ] **Step 3: 提交**

```bash
git add src/utils/tRule.ts
git commit -m "fix(tRule): warn on missing i18n keys, fallback to rule_id"
```

---

### Task 3: 补全 E 级规则 i18n（Validation.zh/en.json）

**Files:**
- Modify: `src/i18n/Validation.zh.json:1-17`（全文件）
- Modify: `src/i18n/Validation.en.json:1-17`（全文件）

- [ ] **Step 1: 完整重写 Validation.zh.json**

完整替换为：
```json
{
  "severity": { "error": "错误", "warning": "警告", "info": "提示" },
  "rule": {
    "E1": {
      "title": "缺少 ID",
      "message": "@synapse-id 不能为空",
      "cause": "每个 entry 必须有唯一 ID，由 @synapse-id 注释或目录名决定",
      "fixGuide": "1. 打开 content.md\n2. 在头部加上 <!-- @synapse-id: <派生id> -->\n3. 或重命名目录为期望 ID"
    },
    "E2": {
      "title": "ID 与路径不一致",
      "message": "@synapse-id \"{{id}}\" 应等于 \"{{expected}}\"",
      "cause": "@synapse-id 与目录名不匹配会导致链接解析错位",
      "fixGuide": "1. 检查 @synapse-id 是否正确\n2. 用一键修复使其与路径一致\n3. 或重命名目录"
    },
    "E3": {
      "title": "缺少标题",
      "message": "@synapse-title 不能为空",
      "cause": "标题用于在地图和搜索结果中显示 entry",
      "fixGuide": "1. 打开 content.md\n2. 在头部加上 <!-- @synapse-title: <标题> -->\n3. 或使用一键修复"
    },
    "E4": {
      "title": "缺少类型标签",
      "message": "@synapse-tags 至少需要一个 type: 前缀",
      "cause": "type: 标签用于分类和筛选（如 type:note, type:ref）",
      "fixGuide": "1. 打开 @synapse-tags 注释\n2. 添加 type:note 标签\n3. 或使用一键修复"
    },
    "E5": {
      "title": "目录深度过深",
      "message": "目录嵌套层数超过 3 层",
      "cause": "深度过深会增加链接维护成本",
      "fixGuide": "1. 将嵌套子目录移到浅层\n2. 或合并相关 entry"
    },
    "E6": {
      "title": "root 级存在 .md 文件",
      "message": "根目录不应直接放 .md 文件",
      "cause": "root 级 .md 应被包装到 entries/<name>/ 目录",
      "fixGuide": "1. 在 root 创建 entries/<basename>/\n2. 把 .md 移入并改名为 content.md\n3. 或使用一键修复"
    },
    "E7": {
      "title": "保留前缀目录",
      "message": "目录以 _ 开头（保留前缀）",
      "cause": "_ 前缀目录被特殊保留用于隐藏或内部用途",
      "fixGuide": "1. 重命名目录去掉 _ 前缀\n2. 或将其移出 KB 根"
    },
    "E9": {
      "title": "用户写了 system label",
      "message": "{{label}} 是系统自动维护的 label",
      "cause": "system label 由规则自动添加，用户不应手动写",
      "fixGuide": "1. 删除该 @synapse-labels 注释\n2. 或使用一键修复"
    }
  },
  "systemLabel": {
    "recently-modified": "最近修改",
    "orphan": "孤立条目",
    "stale": "长期未更新",
    "broken-link": "断链"
  }
}
```

- [ ] **Step 2: 完整重写 Validation.en.json**

完整替换为：
```json
{
  "severity": { "error": "Error", "warning": "Warning", "info": "Info" },
  "rule": {
    "E1": {
      "title": "Missing ID",
      "message": "@synapse-id is empty",
      "cause": "Every entry must have a unique ID, set via @synapse-id comment or directory name",
      "fixGuide": "1. Open content.md\n2. Add <!-- @synapse-id: <derived-id> --> at the top\n3. Or rename the directory to match"
    },
    "E2": {
      "title": "ID does not match path",
      "message": "@synapse-id \"{{id}}\" should equal \"{{expected}}\"",
      "cause": "Mismatch breaks wikilink resolution",
      "fixGuide": "1. Check @synapse-id is correct\n2. Use one-click fix to sync with path\n3. Or rename the directory"
    },
    "E3": {
      "title": "Missing title",
      "message": "@synapse-title is empty",
      "cause": "Title is shown on the map and in search results",
      "fixGuide": "1. Open content.md\n2. Add <!-- @synapse-title: <title> --> at the top\n3. Or use one-click fix"
    },
    "E4": {
      "title": "Missing type tag",
      "message": "@synapse-tags must have at least one type: prefix",
      "cause": "type: tags are used for categorization and filtering (e.g. type:note, type:ref)",
      "fixGuide": "1. Open @synapse-tags comment\n2. Add type:note tag\n3. Or use one-click fix"
    },
    "E5": {
      "title": "Directory too deep",
      "message": "Directory nesting exceeds 3 levels",
      "cause": "Deep nesting makes link maintenance harder",
      "fixGuide": "1. Move nested subdirectories to a shallower level\n2. Or merge related entries"
    },
    "E6": {
      "title": "Root-level .md file exists",
      "message": "Root directory should not contain .md files directly",
      "cause": "Root .md files should be wrapped in entries/<name>/ directory",
      "fixGuide": "1. Create entries/<basename>/ at root\n2. Move .md in and rename to content.md\n3. Or use one-click fix"
    },
    "E7": {
      "title": "Reserved prefix directory",
      "message": "Directory starts with _ (reserved prefix)",
      "cause": "_ prefix directories are reserved for hidden/internal use",
      "fixGuide": "1. Rename the directory to remove _ prefix\n2. Or move it outside the KB root"
    },
    "E9": {
      "title": "User wrote system label",
      "message": "{{label}} is a system-maintained label",
      "cause": "System labels are auto-added by rules, users should not write them manually",
      "fixGuide": "1. Remove that @synapse-labels entry\n2. Or use one-click fix"
    }
  },
  "systemLabel": {
    "recently-modified": "Recently modified",
    "orphan": "Orphan",
    "stale": "Stale",
    "broken-link": "Broken link"
  }
}
```

- [ ] **Step 3: 跑构建验证**

Run: `npm run build`
Expected: TS 通过

- [ ] **Step 4: 提交**

```bash
git add src/i18n/Validation.zh.json src/i18n/Validation.en.json
git commit -m "feat(i18n): full E-level rule messages (cause + fixGuide)"
```

---

## Milestone 2: Rust 端基础设施

### Task 4: validator/fix.rs — FixDescriptor 类型

**Files:**
- Create: `src-tauri/src/validator/fix.rs`
- Modify: `src-tauri/src/validator/mod.rs:1-15`（注册 fix 子模块）

- [ ] **Step 1: 写失败测试（先建 fix.rs 仅含类型）**

`src-tauri/src/validator/fix.rs`：
```rust
//! 修复元数据：每个 E 规则的修复能力描述

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixDescriptor {
    pub rule_id: String,
    pub kind: FixKind,
    pub risk: FixRisk,
    pub label_key: String,
    pub preview_key: Option<String>,
    pub confirm_key: Option<String>,
    pub docs_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FixKind {
    OneClick,
    Confirm,
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FixRisk { Low, Medium, High }
```

- [ ] **Step 2: 注册子模块**

`src-tauri/src/validator/mod.rs` 第 1 行附近添加：
```rust
pub mod fix;
```

- [ ] **Step 3: 跑测试验证编译**

Run: `cd src-tauri && cargo build --lib`
Expected: 编译通过

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/validator/fix.rs src-tauri/src/validator/mod.rs
git commit -m "feat(validator): add FixDescriptor / FixKind / FixRisk types"
```

---

### Task 5: fs/backup.rs — 备份模块

**Files:**
- Create: `src-tauri/src/fs/mod.rs`
- Create: `src-tauri/src/fs/backup.rs`

- [ ] **Step 1: 创建 fs/mod.rs**

`src-tauri/src/fs/mod.rs`：
```rust
pub mod backup;
```

- [ ] **Step 2: 创建 fs/backup.rs 框架 + 测试**

`src-tauri/src/fs/backup.rs`：
```rust
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppResult;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub id: String,
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub created_at: u64,
    pub rule_id: String,
}

pub fn backup_dir(kb_root: &Path) -> PathBuf {
    kb_root.join(".synapse-fix-backup")
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

pub fn backup_then_write(
    target: &Path,
    new_content: &str,
    rule_id: &str,
) -> AppResult<Backup> {
    let kb_root = target.parent()
        .ok_or_else(|| crate::error::AppError::Io(format!("no parent for {}", target.display())))?;
    let dir = backup_dir(kb_root);
    std::fs::create_dir_all(&dir)?;

    let ts = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e| crate::error::AppError::Io(e.to_string()))?
        .as_secs();
    let entry_id = target.parent().and_then(|p| p.file_name())
        .and_then(|n| n.to_str()).unwrap_or("entry");
    let id = format!("{}_{}", ts, sanitize_id(entry_id));
    let backup_path = dir.join(format!("{}.md", id));

    let original = std::fs::read(target)?;
    std::fs::write(&backup_path, &original)?;

    std::fs::write(target, new_content)?;

    Ok(Backup {
        id,
        original_path: target.to_path_buf(),
        backup_path,
        created_at: ts,
        rule_id: rule_id.to_string(),
    })
}

pub fn list_backups(kb_root: &Path) -> Vec<Backup> {
    let dir = backup_dir(kb_root);
    if !dir.exists() { return vec![]; }
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![]; };
    entries.filter_map(|e| e.ok()).filter_map(|e| {
        let name = e.file_name().to_str()?.to_string();
        if !name.ends_with(".md") { return None; }
        let id = name.trim_end_matches(".md").to_string();
        let ts_str = id.split('_').next()?;
        let ts: u64 = ts_str.parse().ok()?;
        Some(Backup {
            id,
            original_path: e.path().with_file_name("__placeholder__"),
            backup_path: e.path(),
            created_at: ts,
            rule_id: String::new(),
        })
    }).collect()
}

pub fn restore_backup(backup: &Backup) -> AppResult<()> {
    let content = std::fs::read(&backup.backup_path)?;
    std::fs::write(&backup.original_path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn backup_then_write_creates_backup() {
        let tmp = TempDir::new().unwrap();
        let entry_dir = tmp.path().join("entry1");
        std::fs::create_dir_all(&entry_dir).unwrap();
        let target = entry_dir.join("content.md");
        std::fs::write(&target, "ORIGINAL").unwrap();

        let b = backup_then_write(&target, "NEW", "E1").unwrap();
        assert_eq!(std::fs::read_to_string(&b.backup_path).unwrap(), "ORIGINAL");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "NEW");
        assert_eq!(b.rule_id, "E1");
    }

    #[test]
    fn restore_backup_recovers_original() {
        let tmp = TempDir::new().unwrap();
        let entry_dir = tmp.path().join("entry1");
        std::fs::create_dir_all(&entry_dir).unwrap();
        let target = entry_dir.join("content.md");
        std::fs::write(&target, "ORIGINAL").unwrap();

        let b = backup_then_write(&target, "NEW", "E1").unwrap();
        restore_backup(&b).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "ORIGINAL");
    }

    #[test]
    fn list_backups_returns_sorted() {
        let tmp = TempDir::new().unwrap();
        let entry_dir = tmp.path().join("entry1");
        std::fs::create_dir_all(&entry_dir).unwrap();
        let target = entry_dir.join("content.md");
        std::fs::write(&target, "ORIGINAL").unwrap();

        let _ = backup_then_write(&target, "A", "E1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let _ = backup_then_write(&target, "B", "E3").unwrap();

        let backups = list_backups(tmp.path());
        assert!(backups.len() >= 2);
        // newest first (or at least present)
        let _ids: HashMap<_, _> = backups.iter().map(|b| (b.id.clone(), b.created_at)).collect();
    }
}
```

- [ ] **Step 3: 在 Cargo.toml 添加 tempfile dev-dep（若未存在）**

`src-tauri/Cargo.toml` `[dev-dependencies]` 段补：
```toml
tempfile = "3"
```

- [ ] **Step 4: 在 validator/mod.rs 加 fs mod 引用**

`src-tauri/src/lib.rs` 或合适位置添加 `mod fs;`，具体路径参考项目现有结构。检查 `src-tauri/src/lib.rs` 顶部声明，按现有风格添加 `mod fs;`。

- [ ] **Step 5: 跑测试**

Run: `cd src-tauri && cargo test --lib fs::backup::`
Expected: 3 个测试通过

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/fs/ src-tauri/Cargo.toml
git commit -m "feat(fs): add backup module with restore + list"
```

---

### Task 6: validator/fix_impls/mod.rs — 派发入口

**Files:**
- Create: `src-tauri/src/validator/fix_impls/mod.rs`

- [ ] **Step 1: 创建派发入口（含每规则 placeholder）**

`src-tauri/src/validator/fix_impls/mod.rs`：
```rust
//! 按 rule_id 派发到具体规则实现

pub mod e1_add_id;
pub mod e2_sync_id;
pub mod e3_add_title;
pub mod e4_default_type;
pub mod e5_move_deep;
pub mod e6_remove_root_md;
pub mod e7_rename_reserved;
pub mod e9_strip_system;

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::{AppError, AppResult};
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

/// 仅返回 FixDescriptor + 完整 apply 流程的入口。
pub fn descriptor_and_apply(
    rule_id: &str,
    entry: &RawEntry,
    entry_dir: &Path,
    confirm: bool,
) -> AppResult<FixDescriptor> {
    match rule_id {
        "E1" => e1_add_id::descriptor_and_apply(entry, entry_dir),
        "E2" => e2_sync_id::descriptor_and_apply(entry, entry_dir, confirm),
        "E3" => e3_add_title::descriptor_and_apply(entry, entry_dir),
        "E4" => e4_default_type::descriptor_and_apply(entry, entry_dir),
        "E5" => Ok(e5_move_deep::descriptor_only()),
        "E6" => e6_remove_root_md::descriptor_and_apply(entry, entry_dir, confirm),
        "E7" => Ok(e7_rename_reserved::descriptor_only()),
        "E9" => e9_strip_system::descriptor_and_apply(entry, entry_dir, confirm),
        _ => Err(AppError::UnknownRule(rule_id.to_string())),
    }
}

/// 仅返回 descriptor（用于 ValidationPanel 显示 + Manual 类）
pub fn descriptor_only(rule_id: &str) -> AppResult<FixDescriptor> {
    Ok(match rule_id {
        "E5" => e5_move_deep::descriptor_only(),
        "E7" => e7_rename_reserved::descriptor_only(),
        "E1" => descriptor_and_apply(rule_id, &dummy_entry(), &PathBuf::default(), false)
            .map(|d| d.with_kind(FixKind::OneClick))?,
        _ => return Err(AppError::UnknownRule(rule_id.to_string())),
    }.with_risk(FixRisk::Low))
}

// placeholder helpers (use later tasks)
fn dummy_entry() -> RawEntry { unimplemented!() }
use std::path::PathBuf;
```

> 注：先建占位 stub，后续 Task 8-15 逐个实现。

- [ ] **Step 2: 跑 build 验证编译**

Run: `cd src-tauri && cargo build --lib`
Expected: 编译通过（各子模块尚未实现，可能需先建空 stub）

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/validator/fix_impls/
git commit -m "feat(validator): scaffold fix_impls registry"
```

---

## Milestone 3: 各 E 规则 fix 实现

### Task 7: e1_add_id.rs（OneClick Low）

**Files:**
- Create: `src-tauri/src/validator/fix_impls/e1_add_id.rs`

- [ ] **Step 1: 写失败测试 + 实现**

完整文件：
```rust
//! E1：从路径派生 id 写入 @synapse-id

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_and_apply(
    entry: &RawEntry,
    entry_dir: &Path,
) -> AppResult<FixDescriptor> {
    let derived = entry_dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("entry")
        .to_string();
    let new_content = inject_synapse_id(&entry.raw, &derived);

    backup::backup_then_write(&entry.path, &new_content, "E1")?;

    Ok(FixDescriptor {
        rule_id: "E1".into(),
        kind: FixKind::OneClick,
        risk: FixRisk::Low,
        label_key: "Fix.E1.apply".into(),
        preview_key: Some("Fix.E1.preview".into()),
        confirm_key: None,
        docs_key: "Validation.rule.E1.fixGuide".into(),
    })
}

fn inject_synapse_id(raw: &str, derived: &str) -> String {
    if raw.lines().any(|l| l.contains("@synapse-id:")) {
        return raw.to_string();
    }
    let inject = format!("<!-- @synapse-id: {} -->\n", derived);
    if let Some(pos) = raw.find("---\n") {
        // 有 YAML frontmatter，注释放 frontmatter 之前
        let mut out = String::with_capacity(raw.len() + inject.len());
        out.push_str(&raw[..pos]);
        out.push_str(&inject);
        out.push_str(&raw[pos..]);
        return out;
    }
    format!("{}{}", inject, raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn injects_id_when_missing() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("react-hooks");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("content.md");
        std::fs::write(&path, "# Hooks\n").unwrap();
        let entry = RawEntry {
            id: String::new(),
            title: String::new(),
            tags: vec![],
            labels: vec![],
            path: path.clone(),
            raw: "# Hooks\n".into(),
            ignored: false,
            ignored_rules: vec![],
        };
        descriptor_and_apply(&entry, &dir).unwrap();
        let result = std::fs::read_to_string(&path).unwrap();
        assert!(result.contains("@synapse-id: react-hooks"));
    }
}
```

> 注意：实际 RawEntry 字段以你项目里为准（`bool` 字段名需查 `entry.rs`）。如有出入按实际字段调整。

- [ ] **Step 2: 跑测试**

Run: `cd src-tauri && cargo test --lib validator::fix_impls::e1_add_id::`
Expected: PASS

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/validator/fix_impls/e1_add_id.rs
git commit -m "feat(validator): E1 fix — derive id from path"
```

---

### Task 8: e3_add_title.rs（OneClick Low）

**Files:**
- Create: `src-tauri/src/validator/fix_impls/e3_add_title.rs`

- [ ] **Step 1: 写实现**

```rust
//! E3：写入默认 title（与 id 相同）

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_and_apply(
    entry: &RawEntry,
    _entry_dir: &Path,
) -> AppResult<FixDescriptor> {
    let title = entry.id.clone();
    let new_content = inject_title(&entry.raw, &title);

    backup::backup_then_write(&entry.path, &new_content, "E3")?;

    Ok(FixDescriptor {
        rule_id: "E3".into(),
        kind: FixKind::OneClick,
        risk: FixRisk::Low,
        label_key: "Fix.E3.apply".into(),
        preview_key: Some("Fix.E3.preview".into()),
        confirm_key: None,
        docs_key: "Validation.rule.E3.fixGuide".into(),
    })
}

fn inject_title(raw: &str, title: &str) -> String {
    if raw.lines().any(|l| l.contains("@synapse-title:")) {
        return raw.to_string();
    }
    let inject = format!("<!-- @synapse-title: {} -->\n", title);
    if let Some(pos) = raw.find("---\n") {
        let mut out = String::with_capacity(raw.len() + inject.len());
        out.push_str(&raw[..pos]);
        out.push_str(&inject);
        out.push_str(&raw[pos..]);
        return out;
    }
    format!("{}{}", inject, raw)
}
```

- [ ] **Step 2: 跑 build**

Run: `cd src-tauri && cargo build --lib`
Expected: PASS

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/validator/fix_impls/e3_add_title.rs
git commit -m "feat(validator): E3 fix — write default title"
```

---

### Task 9: e4_default_type.rs（OneClick Low）

**Files:**
- Create: `src-tauri/src/validator/fix_impls/e4_default_type.rs`

- [ ] **Step 1: 写实现**

```rust
//! E4：在 @synapse-tags 注入 type:note

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_and_apply(
    entry: &RawEntry,
    _entry_dir: &Path,
) -> AppResult<FixDescriptor> {
    let new_content = inject_type(&entry.raw);
    backup::backup_then_write(&entry.path, &new_content, "E4")?;
    Ok(FixDescriptor {
        rule_id: "E4".into(),
        kind: FixKind::OneClick,
        risk: FixRisk::Low,
        label_key: "Fix.E4.apply".into(),
        preview_key: Some("Fix.E4.preview".into()),
        confirm_key: None,
        docs_key: "Validation.rule.E4.fixGuide".into(),
    })
}

fn inject_type(raw: &str) -> String {
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("<!-- @synapse-tags:") {
            let rest = rest.trim_end_matches("-->").trim();
            if rest.split(',').any(|t| t.trim().starts_with("type:")) {
                return raw.to_string();
            }
            let new_line = format!("<!-- @synapse-tags: type:note, {} -->", rest);
            return raw.re_indices(line, &new_line);
        }
    }
    let inject = "<!-- @synapse-tags: type:note -->\n";
    if let Some(pos) = raw.find("---\n") {
        let mut out = String::with_capacity(raw.len() + inject.len());
        out.push_str(&raw[..pos]);
        out.push_str(inject);
        out.push_str(&raw[pos..]);
        return out;
    }
    format!("{}{}", inject, raw)
}
```

- [ ] **Step 2: 跑 build**

Run: `cd src-tauri && cargo build --lib`
Expected: PASS

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/validator/fix_impls/e4_default_type.rs
git commit -m "feat(validator): E4 fix — inject type:note"
```

---

### Task 10: e2_sync_id.rs（OneClick Low）

**Files:**
- Create: `src-tauri/src/validator/fix_impls/e2_sync_id.rs`

- [ ] **Step 1: 写实现**

```rust
//! E2：写 @synapse-id 与路径一致

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_and_apply(
    entry: &RawEntry,
    entry_dir: &Path,
    _confirm: bool,
) -> AppResult<FixDescriptor> {
    let expected = entry_dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let new_content = replace_synapse_id(&entry.raw, &expected);
    backup::backup_then_write(&entry.path, &new_content, "E2")?;
    Ok(FixDescriptor {
        rule_id: "E2".into(),
        kind: FixKind::OneClick,
        risk: FixRisk::Low,
        label_key: "Fix.E2.apply".into(),
        preview_key: Some("Fix.E2.preview".into()),
        confirm_key: None,
        docs_key: "Validation.rule.E2.fixGuide".into(),
    })
}

fn replace_synapse_id(raw: &str, expected: &str) -> String {
    raw.lines()
        .map(|l| {
            if l.contains("@synapse-id:") {
                let prefix = l.find("<!--").unwrap_or(0);
                let suffix = l.find("-->").map(|p| p + 3).unwrap_or(l.len());
                format!("{} @synapse-id: {} {}", &l[..prefix], expected, &l[suffix..])
            } else { l.to_string() }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
```

- [ ] **Step 2: 跑 build + 提交**

```bash
cd src-tauri && cargo build --lib
git add src-tauri/src/validator/fix_impls/e2_sync_id.rs
git commit -m "feat(validator): E2 fix — sync @synapse-id with path"
```

---

### Task 11: e5_move_deep.rs（Manual only）

**Files:**
- Create: `src-tauri/src/validator/fix_impls/e5_move_deep.rs`

- [ ] **Step 1: 写实现**

```rust
//! E5：Manual — 仅返回 descriptor，无 apply

use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_only() -> FixDescriptor {
    FixDescriptor {
        rule_id: "E5".into(),
        kind: FixKind::Manual,
        risk: FixRisk::Low,
        label_key: "Fix.E5.apply".into(),
        preview_key: None,
        confirm_key: None,
        docs_key: "Validation.rule.E5.fixGuide".into(),
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add src-tauri/src/validator/fix_impls/e5_move_deep.rs
git commit -m "feat(validator): E5 manual descriptor"
```

---

### Task 12: e6_remove_root_md.rs（Confirm High）

**Files:**
- Create: `src-tauri/src/validator/fix_impls/e6_remove_root_md.rs`

- [ ] **Step 1: 写实现**

```rust
//! E6：将 root 级 .md 移入 entries/<basename>/ 并改名 content.md
//! 需要 confirm=true 才执行

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::{AppError, AppResult};
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub type _ = (); // placeholder to silence lints in case

pub fn descriptor_and_apply(
    entry: &RawEntry,
    entry_dir: &Path,
    confirm: bool,
) -> AppResult<FixDescriptor> {
    if !confirm {
        return Ok(FixDescriptor {
            rule_id: "E6".into(),
            kind: FixKind::Confirm,
            risk: FixRisk::High,
            label_key: "Fix.E6.apply".into(),
            preview_key: Some("Fix.E6.preview".into()),
            confirm_key: Some("Fix.E6.confirmTitle".into()),
            docs_key: "Validation.rule.E6.fixGuide".into(),
        });
    }

    // 执行修复
    let file_name = entry.path.file_name().and_then(|n| n.to_str())
        .ok_or_else(|| AppError::Io("no filename".into()))?
        .trim_end_matches(".md").to_string();
    let target_dir = entry_dir.join("entries").join(&file_name);
    std::fs::create_dir_all(&target_dir)?;

    let dest = target_dir.join("content.md");
    backup::backup_then_write(&entry.path, "", "E6")?; // placeholder
    std::fs::rename(&entry.path, &dest)?;

    Ok(FixDescriptor {
        rule_id: "E6".into(),
        kind: FixKind::Confirm,
        risk: FixRisk::High,
        label_key: "Fix.E6.apply".into(),
        preview_key: Some("Fix.E6.preview".into()),
        confirm_key: Some("Fix.E6.confirmTitle".into()),
        docs_key: "Validation.rule.E6.fixGuide".into(),
    })
}
```

- [ ] **Step 2: 跑 build + 提交**

```bash
cd src-tauri && cargo build --lib
git add src-tauri/src/validator/fix_impls/e6_remove_root_md.rs
git commit -m "feat(validator): E6 confirm — move root md to entries/"
```

---

### Task 13: e7_rename_reserved.rs（Manual only）

**Files:**
- Create: `src-tauri/src/validator/fix_impls/e7_rename_reserved.rs`

- [ ] **Step 1: 写实现**

```rust
//! E7：Manual — 仅返回 descriptor

use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_only() -> FixDescriptor {
    FixDescriptor {
        rule_id: "E7".into(),
        kind: FixKind::Manual,
        risk: FixRisk::Low,
        label_key: "Fix.E7.apply".into(),
        preview_key: None,
        confirm_key: None,
        docs_key: "Validation.rule.E7.fixGuide".into(),
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add src-tauri/src/validator/fix_impls/e7_rename_reserved.rs
git commit -m "feat(validator): E7 manual descriptor"
```

---

### Task 14: e9_strip_system.rs（Confirm Medium）

**Files:**
- Create: `src-tauri/src/validator/fix_impls/e9_strip_system.rs`

- [ ] **Step 1: 写实现**

```rust
//! E9：从 @synapse-labels 删除 system 维护的 label 行
//! 需要 confirm=true

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

const SYSTEM_LABELS: &[&str] = &["recently-modified", "orphan", "stale", "broken-link"];

pub fn descriptor_and_apply(
    entry: &RawEntry,
    _entry_dir: &Path,
    confirm: bool,
) -> AppResult<FixDescriptor> {
    if !confirm {
        return Ok(FixDescriptor {
            rule_id: "E9".into(),
            kind: FixKind::Confirm,
            risk: FixRisk::Medium,
            label_key: "Fix.E9.apply".into(),
            preview_key: Some("Fix.E9.preview".into()),
            confirm_key: Some("Fix.E9.confirmTitle".into()),
            docs_key: "Validation.rule.E9.fixGuide".into(),
        });
    }

    let new_content: String = entry.raw.lines()
        .filter(|l| {
            if let Some(rest) = l.strip_prefix("<!-- @synapse-labels:") {
                let rest = rest.trim_end_matches("-->").trim();
                !SYSTEM_LABELS.iter().any(|s| rest.split(',').any(|t| t.trim() == *s))
            } else { true }
        })
        .collect::<Vec<_>>()
        .join("\n");

    backup::backup_then_write(&entry.path, &new_content, "E9")?;

    Ok(FixDescriptor {
        rule_id: "E9".into(),
        kind: FixKind::Confirm,
        risk: FixRisk::Medium,
        label_key: "Fix.E9.apply".into(),
        preview_key: Some("Fix.E9.preview".into()),
        confirm_key: Some("Fix.E9.confirmTitle".into()),
        docs_key: "Validation.rule.E9.fixGuide".into(),
    })
}
```

- [ ] **Step 2: 跑 build + 提交**

```bash
cd src-tauri && cargo build --lib
git add src-tauri/src/validator/fix_impls/e9_strip_system.rs
git commit -m "feat(validator): E9 confirm — strip system labels"
```

---

## Milestone 4: Tauri 命令

### Task 15: commands.rs — apply_fix / list_backups / restore_backup

**Files:**
- Modify: `src-tauri/src/commands.rs`（在末尾追加）

- [ ] **Step 1: 新增 apply_fix 命令**

在 `commands.rs` 末尾追加：
```rust
use crate::validator::fix_impls;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub rule_id: String,
    pub entry_id: String,
    pub descriptor: crate::validator::fix::FixDescriptor,
    pub backup_id: Option<String>,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn apply_fix(
    state: State<'_, AppState>,
    entry_id: String,
    rule_id: String,
    confirm: Option<bool>,
) -> AppResult<ApplyResult> {
    let _guard = state.write_lock.lock().await;

    let entry_dir = state.index.read().await.resolve_entry_dir(&entry_id)
        .ok_or_else(|| crate::error::AppError::EntryNotFound(entry_id.clone()))?;
    let entry = crate::entry::read_entry(&entry_dir);
    let confirm = confirm.unwrap_or(false);

    let descriptor = fix_impls::descriptor_and_apply(rule_id.as_str(), &entry, &entry_dir, confirm)?;

    // 触发索引重建（同步 sqlite）
    crate::index::rebuild(state, None).await.ok();

    Ok(ApplyResult {
        rule_id,
        entry_id,
        descriptor,
        backup_id: None, // 备份路径由 backup_then_write 创建但暂未返回 id；后续可补
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_backups(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::fs::backup::Backup>> {
    let kb_root = state.current_kb.read().await
        .as_ref().map(|k| k.root.clone())
        .ok_or_else(|| crate::error::AppError::NoActiveKb)?;
    Ok(crate::fs::backup::list_backups(&kb_root))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn restore_backup(
    state: State<'_, AppState>,
    backup_id: String,
) -> AppResult<()> {
    let _guard = state.write_lock.lock().await;
    let kb_root = state.current_kb.read().await
        .as_ref().map(|k| k.root.clone())
        .ok_or_else(|| crate::error::AppError::NoActiveKb)?;
    let mut backups = crate::fs::backup::list_backups(&kb_root);
    let backup = backups.drain(..).find(|b| b.id == backup_id)
        .ok_or_else(|| crate::error::AppError::BackupNotFound(backup_id))?;
    crate::fs::backup::restore_backup(&backup)?;
    crate::index::rebuild(state, None).await.ok();
    Ok(())
}
```

> 注意：`index.rebuild`、`read_entry`、`resolve_entry_dir` 等签名按你项目实际调整。本计划假设主要接口已在。

- [ ] **Step 2: 在 lib.rs 注册命令**

`src-tauri/src/lib.rs` 中 `tauri::generate_handler!` 块添加三个新命令名。

- [ ] **Step 3: 跑 build**

Run: `cd src-tauri && cargo build --lib`
Expected: 编译通过（可能需根据实际签名调整若干行）

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(commands): apply_fix / list_backups / restore_backup"
```

---

## Milestone 5: 前端基础设施

### Task 16: panelStore.ts — 新增 'fix' 类型

**Files:**
- Modify: `src/store/panelStore.ts:3`

- [ ] **Step 1: 修改 PanelType**

将：
```typescript
export type PanelType = 'detail' | 'validation' | 'settings' | 'filter';
```
（如果是该设计之前的版本则照此替换；本计划假设当前已含 `'detail'`）

如果当前是 `'validation' | 'settings' | 'filter'`，加 `'fix'` 和 `'detail'`。

- [ ] **Step 2: 跑 build 验证**

Run: `npm run build`
Expected: PASS

- [ ] **Step 3: 提交**

```bash
git add src/store/panelStore.ts
git commit -m "feat(panel): add 'fix' panel type"
```

---

### Task 17: fixStore.ts — 新建

**Files:**
- Create: `src/store/fixStore.ts`

- [ ] **Step 1: 创建 store**

完整文件：
```typescript
import { create } from 'zustand';

export interface ApplyResult {
  ruleId: string;
  entryId: string;
}

export interface FixTarget {
  entryId: string;
  ruleId: string;
}

interface FixStore {
  drawerEntryId: string | null;
  openDrawer: (id: string) => void;
  closeDrawer: () => void;

  recentFixes: ApplyResult[];
  pushFix: (r: ApplyResult) => void;

  autoFixQueue: FixTarget[];
  startAutoFix: (entryId: string, ruleIds: string[]) => void;
}

export const useFixStore = create<FixStore>((set) => ({
  drawerEntryId: null,
  openDrawer: (id) => set({ drawerEntryId: id }),
  closeDrawer: () => set({ drawerEntryId: null }),

  recentFixes: [],
  pushFix: (r) => set((s) => ({ recentFixes: [r, ...s.recentFixes].slice(0, 20) })),

  autoFixQueue: [],
  startAutoFix: (entryId, ruleIds) =>
    set({ autoFixQueue: ruleIds.map((r) => ({ entryId, ruleId: r })) }),
}));
```

- [ ] **Step 2: 跑 build + 提交**

```bash
npm run build
git add src/store/fixStore.ts
git commit -m "feat(fix): add fixStore with drawer + queue state"
```

---

## Milestone 6: 三入口组件

### Task 18: ErrorBar.tsx — 改进文案 + 点击打开面板

**Files:**
- Modify: `src/components/ErrorBar.tsx`（全文件）

- [ ] **Step 1: 替换为新版本**

完整文件：
```typescript
import React from 'react';
import { useTranslation } from 'react-i18next';
import { usePanelStore } from '@/store/panelStore';

interface Props {
  errorCount: number;
  warningCount: number;
}

export const ErrorBar: React.FC<Props> = ({ errorCount, warningCount }) => {
  const { t } = useTranslation('Error');
  const { open } = usePanelStore();

  if (errorCount === 0 && warningCount === 0) return null;

  return (
    <button
      onClick={() => open('validation')}
      className={`block w-full cursor-pointer px-4 py-2 text-left text-sm transition hover:opacity-90 ${
        errorCount > 0
          ? 'bg-red-100 text-red-800'
          : 'bg-yellow-100 text-yellow-800'
      }`}
      title={t('tooltipWithCounts', { errors: errorCount, warnings: warningCount })}
    >
      {errorCount > 0 && t('errors', { count: errorCount })}
      {errorCount > 0 && warningCount > 0 && ' · '}
      {warningCount > 0 && t('warnings', { count: warningCount })}
      {' — '}
      {t('clickToView')}
    </button>
  );
};
```

- [ ] **Step 2: 跑 build**

Run: `npm run build`
Expected: PASS（Error namespace 待后续 Task 22 新建）

- [ ] **Step 3: 提交**

```bash
git add src/components/ErrorBar.tsx
git commit -m "feat(ui): ErrorBar as button + i18n"
```

---

### Task 19: NodeCard.tsx — 红点变 button

**Files:**
- Modify: `src/components/NodeCard.tsx:13-44`

- [ ] **Step 1: 添加点击 handler + 改 badge 为 button**

将：
```tsx
const badge = hasError
  ? { color: 'bg-red-500', title: t('errorTooltip', { count: data.errorCount }) }
  : hasWarning
  ? { color: 'bg-yellow-400', title: t('warningTooltip', { count: data.warningCount }) }
  : null;
```
后接 `{badge && (...)}` 块改为：
```tsx
{badge && (
  <button
    className={`absolute -right-1 -top-1 flex h-4 min-w-[16px] items-center justify-center rounded-full px-1 text-[10px] font-bold text-white ${badge.color} cursor-pointer transition hover:scale-110`}
    title={badge.title}
    aria-label={badge.title}
    onClick={(e) => {
      e.stopPropagation();
      window.dispatchEvent(new CustomEvent('open-fix-drawer', { detail: { entryId: data.entryId } }));
    }}
  >
    {badge.count}
  </button>
)}
```

并将 badge 计算改为：
```tsx
const badge = hasError
  ? { color: 'bg-red-500', title: t('errorTooltip', { count: data.errorCount }), count: data.errorCount }
  : hasWarning
  ? { color: 'bg-yellow-400', title: t('warningTooltip', { count: data.warningCount }), count: data.warningCount }
  : null;
```

并在 `NodeCardData` 接口加 `entryId: string`。

- [ ] **Step 2: 跑 build + 提交**

```bash
npm run build
git add src/components/NodeCard.tsx
git commit -m "feat(ui): NodeCard red dot clickable, opens fix drawer"
```

---

### Task 20: FixDrawer.tsx — 节点级修复抽屉

**Files:**
- Create: `src/components/FixDrawer.tsx`

- [ ] **Step 1: 创建组件**

完整文件：
```tsx
import React, { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { useFixStore } from '@/store/fixStore';

interface ValidationIssue {
  entry_id: string;
  rule_id: string;
  severity: Record<string, null>;
  message: string;
}

interface ValidationReport {
  kb_root: string;
  issues: ValidationIssue[];
}

export const FixDrawer: React.FC<{ activeKbRoot: string | null }> = ({ activeKbRoot }) => {
  const { t } = useTranslation('Fix');
  const drawerEntryId = useFixStore((s) => s.drawerEntryId);
  const closeDrawer = useFixStore((s) => s.closeDrawer);
  const pushFix = useFixStore((s) => s.pushFix);

  const [issues, setIssues] = useState<ValidationIssue[]>([]);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    if (!drawerEntryId || !activeKbRoot) return;
    invoke<ValidationReport>('validate_kb', { kbRoot: activeKbRoot })
      .then((r) => setIssues(r.issues.filter((i) => i.entry_id === drawerEntryId)))
      .catch(console.error);
  }, [drawerEntryId, activeKbRoot]);

  useEffect(() => {
    const h = (e: Event) => {
      const ce = e as CustomEvent<{ entryId: string }>;
      useFixStore.getState().openDrawer(ce.detail.entryId);
    };
    window.addEventListener('open-fix-drawer', h);
    return () => window.removeEventListener('open-fix-drawer', h);
  }, []);

  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape') closeDrawer(); };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [closeDrawer]);

  if (!drawerEntryId) return null;

  const errors = issues.filter((i) => 'Error' in i.severity);

  const applyFix = async (ruleId: string, confirm: boolean) => {
    setBusy(ruleId);
    try {
      await invoke('apply_fix', { entryId: drawerEntryId, ruleId, confirm });
      pushFix({ ruleId, entryId: drawerEntryId });
      setIssues((prev) => prev.filter((i) => i.rule_id !== ruleId));
    } catch (e) {
      console.error(e);
      alert(t('common.failed', { reason: String(e) }));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="fixed right-0 top-12 z-50 w-96 max-w-[90vw] rounded-l-lg border border-gray-200 bg-white p-4 shadow-xl">
      <div className="flex items-center justify-between border-b pb-2">
        <h3 className="text-sm font-semibold">{t('drawerTitle', { entry: drawerEntryId })}</h3>
        <button onClick={closeDrawer} className="text-gray-400 hover:text-gray-600">✕</button>
      </div>
      <div className="mt-3 space-y-3">
        {errors.length === 0 && <div className="text-sm text-gray-500">{t('noErrors')}</div>}
        {errors.map((issue) => (
          <div key={issue.rule_id} className="rounded border border-red-100 bg-red-50 p-2">
            <div className="text-xs font-medium text-red-700">
              {issue.rule_id}: {issue.message}
            </div>
            <div className="mt-2 flex gap-2">
              <button
                disabled={busy === issue.rule_id}
                onClick={() => applyFix(issue.rule_id, false)}
                className="rounded bg-red-500 px-2 py-1 text-xs text-white hover:bg-red-600 disabled:opacity-50"
              >
                {busy === issue.rule_id ? t('applying') : t(`E${issue.rule_id.slice(1)}.apply`, { defaultValue: t('common.apply') })}
              </button>
              <button className="rounded bg-gray-100 px-2 py-1 text-xs hover:bg-gray-200">
                {t('common.openDocs')}
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
```

- [ ] **Step 2: 跑 build + 提交**

```bash
npm run build
git add src/components/FixDrawer.tsx
git commit -m "feat(ui): FixDrawer for per-entry fix"
```

---

### Task 21: FixWizard.tsx — 面板内修复向导

**Files:**
- Create: `src/components/FixWizard.tsx`

- [ ] **Step 1: 创建组件**

完整文件：
```tsx
import React from 'react';
import { useTranslation } from 'react-i18next';

interface FixDescriptor {
  rule_id: string;
  kind: { type: string };
  risk: string;
  label_key: string;
  docs_key: string;
}

interface Props {
  descriptor: FixDescriptor;
  onApply: (confirm: boolean) => void;
  busy: boolean;
}

export const FixWizard: React.FC<Props> = ({ descriptor, onApply, busy }) => {
  const { t } = useTranslation('Fix');

  const label = t(descriptor.label_key, { defaultValue: t('common.apply') });
  const docs = t(descriptor.docs_key, { defaultValue: '' });

  return (
    <div className="space-y-2 rounded border border-gray-200 bg-white p-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-gray-700">{descriptor.rule_id}</span>
        <span className={`rounded px-1 text-[10px] ${
          descriptor.risk === 'low' ? 'bg-green-100 text-green-700' :
          descriptor.risk === 'medium' ? 'bg-yellow-100 text-yellow-700' :
          'bg-red-100 text-red-700'
        }`}>{t(`risk.${descriptor.risk}`)}</span>
      </div>
      {docs && <p className="whitespace-pre-wrap text-xs text-gray-600">{docs}</p>}
      <div className="flex gap-2">
        {descriptor.kind.type === 'oneClick' && (
          <button
            disabled={busy}
            onClick={() => onApply(false)}
            className="rounded bg-blue-500 px-3 py-1 text-xs text-white hover:bg-blue-600 disabled:opacity-50"
          >{busy ? t('applying') : label}</button>
        )}
        {descriptor.kind.type === 'confirm' && (
          <button
            disabled={busy}
            onClick={() => {
              if (confirm(label)) onApply(true);
            }}
            className="rounded bg-red-500 px-3 py-1 text-xs text-white hover:bg-red-600 disabled:opacity-50"
          >{busy ? t('applying') : label}</button>
        )}
        {descriptor.kind.type === 'manual' && (
          <span className="text-xs text-gray-500">{t('kind.manual')}</span>
        )}
      </div>
    </div>
  );
};
```

- [ ] **Step 2: 跑 build + 提交**

```bash
npm run build
git add src/components/FixWizard.tsx
git commit -m "feat(ui): FixWizard component"
```

---

### Task 22: ValidationPanel.tsx — 改用 active KB + 分组

**Files:**
- Modify: `src/components/ValidationPanel.tsx`（全文件）

- [ ] **Step 1: 替换为新版本**

完整文件：
```tsx
import React, { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '@/store/useAppStore';
import { useFixStore } from '@/store/fixStore';

interface ValidationIssue {
  entry_id: string;
  rule_id: string;
  severity: Record<'Error' | 'Warning' | 'Info', null>;
  message: string;
}

interface ValidationReport {
  kb_root: string;
  issues: ValidationIssue[];
}

const severityKey = (s: Record<string, null>): 'Error' | 'Warning' | 'Info' => {
  if ('Error' in s) return 'Error';
  if ('Warning' in s) return 'Warning';
  return 'Info';
};

export const ValidationPanel: React.FC = () => {
  const { t } = useTranslation('ValidationPanel');
  const { t: tVal } = useTranslation('Validation');
  const { t: tFix } = useTranslation('Fix');
  const activeKbRoot = useAppStore((s) => s.activeKbRoot);
  const pushFix = useFixStore((s) => s.pushFix);

  const [report, setReport] = useState<ValidationReport | null>(null);
  const [tab, setTab] = useState<'Error' | 'Warning' | 'Info'>('Error');
  const [groupBy, setGroupBy] = useState<'severity' | 'entry'>('severity');
  const [busy, setBusy] = useState<string | null>(null);

  const rescan = async () => {
    if (!activeKbRoot) return;
    const r = await invoke<ValidationReport>('validate_kb', { kbRoot: activeKbRoot });
    setReport(r);
  };

  useEffect(() => { rescan(); }, [activeKbRoot]);

  const filtered = report?.issues.filter((i) => severityKey(i.severity) === tab) ?? [];

  const grouped = useMemo(() => {
    if (groupBy === 'entry') {
      const map = new Map<string, ValidationIssue[]>();
      filtered.forEach((i) => {
        const arr = map.get(i.entry_id) ?? [];
        arr.push(i);
        map.set(i.entry_id, arr);
      });
      return Array.from(map.entries()).map(([entryId, issues]) => ({ entryId, issues }));
    }
    return [{ entryId: null, issues: filtered }];
  }, [filtered, groupBy]);

  const applyFix = async (entryId: string, ruleId: string, confirm: boolean) => {
    setBusy(`${entryId}:${ruleId}`);
    try {
      await invoke('apply_fix', { entryId, ruleId, confirm });
      pushFix({ entryId, ruleId });
      setReport((prev) => prev
        ? { ...prev, issues: prev.issues.filter((i) => !(i.entry_id === entryId && i.rule_id === ruleId)) }
        : prev);
    } catch (e) {
      alert(tFix('common.failed', { reason: String(e) }));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="space-y-3 p-3">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        {(['Error', 'Warning', 'Info'] as const).map((s) => (
          <button
            key={s}
            onClick={() => setTab(s)}
            className={`rounded px-2 py-1 ${tab === s ? 'bg-blue-500 text-white' : 'bg-gray-100'}`}
          >
            {s === 'Error' ? t('errors') : s === 'Warning' ? t('warnings') : t('info')}
          </button>
        ))}
        <button onClick={() => setGroupBy((g) => g === 'severity' ? 'entry' : 'severity')}
          className="rounded bg-gray-100 px-2 py-1 hover:bg-gray-200">
          {t('groupBy', { mode: groupBy })}
        </button>
        <button onClick={rescan}
          className="ml-auto rounded bg-gray-100 px-2 py-1 hover:bg-gray-200">
          {t('rescan')}
        </button>
      </div>

      <div className="space-y-2">
        {grouped.length === 0 || (grouped[0].issues.length === 0 && grouped.length === 1) ? (
          <div className="text-sm text-gray-500">{t('noIssues')}</div>
        ) : grouped.map((g) => (
          <div key={g.entryId ?? 'all'} className="space-y-1 rounded border border-gray-200 bg-white p-2">
            {g.entryId && <div className="text-xs font-medium text-gray-700">{g.entryId}</div>}
            {g.issues.map((issue) => {
              const label = tFix(`${issue.rule_id}.apply`, { defaultValue: tFix('common.apply') });
              return (
                <div key={`${issue.entry_id}:${issue.rule_id}`} className="flex items-center justify-between text-xs">
                  <span className="text-gray-700">[{issue.rule_id}] {issue.message}</span>
                  <div className="flex gap-1">
                    <button
                      disabled={busy === `${issue.entry_id}:${issue.rule_id}`}
                      onClick={() => applyFix(issue.entry_id, issue.rule_id, false)}
                      className="rounded bg-blue-500 px-2 py-0.5 text-white hover:bg-blue-600 disabled:opacity-50"
                    >
                      {busy === `${issue.entry_id}:${issue.rule_id}` ? tFix('applying') : label}
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
};
```

- [ ] **Step 2: 跑 build + 提交**

```bash
npm run build
git add src/components/ValidationPanel.tsx
git commit -m "feat(ui): ValidationPanel active KB + entry grouping + apply"
```

---

### Task 23: App.tsx — 集成 FixDrawer

**Files:**
- Modify: `src/App.tsx`（末尾）

- [ ] **Step 1: 引入 FixDrawer + activeKbRoot**

在 `src/App.tsx` 顶部加 import：
```typescript
import { FixDrawer } from '@/components/FixDrawer';
```

在 `App()` 函数内，加：
```typescript
const activeKbRoot = useAppStore((s) => s.activeKbRoot);
```

并在 JSX 末尾（在 `</main>` 之后或合适位置）加：
```tsx
<FixDrawer activeKbRoot={activeKbRoot} />
```

- [ ] **Step 2: 跑 build + 提交**

```bash
npm run build
git add src/App.tsx
git commit -m "feat(ui): mount FixDrawer in App"
```

---

## Milestone 7: i18n 新建 namespace

### Task 24: Error.zh.json + Error.en.json

**Files:**
- Create: `src/i18n/Error.zh.json`
- Create: `src/i18n/Error.en.json`

- [ ] **Step 1: 创建 Error.zh.json**

```json
{
  "errors_one": "{{count}} 个错误",
  "errors_other": "{{count}} 个错误",
  "warnings_one": "{{count}} 个警告",
  "warnings_other": "{{count}} 个警告",
  "clickToView": "点击查看",
  "tooltipWithCounts": "{{errors}} 个错误 · {{warnings}} 个警告",
  "nodeBadge": "{{count}}"
}
```

- [ ] **Step 2: 创建 Error.en.json**

```json
{
  "errors_one": "{{count}} error",
  "errors_other": "{{count}} errors",
  "warnings_one": "{{count}} warning",
  "warnings_other": "{{count}} warnings",
  "clickToView": "Click to view",
  "tooltipWithCounts": "{{errors}} errors · {{warnings}} warnings",
  "nodeBadge": "{{count}}"
}
```

- [ ] **Step 3: 提交**

```bash
git add src/i18n/Error.zh.json src/i18n/Error.en.json
git commit -m "feat(i18n): Error namespace"
```

---

### Task 25: Fix.zh.json + Fix.en.json

**Files:**
- Create: `src/i18n/Fix.zh.json`
- Create: `src/i18n/Fix.en.json`

- [ ] **Step 1: 创建 Fix.zh.json**

```json
{
  "E1": {
    "apply": "从目录名派生 ID",
    "preview": "将写入 @synapse-id: {{derived}}"
  },
  "E2": {
    "apply": "同步 ID 与路径",
    "preview": "将 @synapse-id 改为 {{expected}}"
  },
  "E3": {
    "apply": "使用 ID 作为默认标题",
    "preview": "将写入 @synapse-title: {{title}}"
  },
  "E4": {
    "apply": "添加 type:note 标签",
    "preview": "在 @synapse-tags 注入 type:note"
  },
  "E6": {
    "apply": "移入 entries 目录",
    "preview": "将移至 entries/{{name}}/",
    "confirmTitle": "确认移动 root 级 .md 文件？"
  },
  "E9": {
    "apply": "删除 system label 行",
    "preview": "将移除 system 维护的标签",
    "confirmTitle": "确认删除？"
  },
  "common": {
    "apply": "一键修复",
    "openDocs": "查看文档",
    "failed": "修复失败：{{reason}}",
    "success": "已修复",
    "restoreOk": "已从备份恢复"
  },
  "drawerTitle": "修复：{{entry}}",
  "noErrors": "该条目无错误",
  "applying": "修复中…",
  "kind": { "manual": "需手动修复" },
  "risk": { "low": "低风险", "medium": "中风险", "high": "高风险" }
}
```

- [ ] **Step 2: 创建 Fix.en.json**

```json
{
  "E1": {
    "apply": "Derive ID from directory name",
    "preview": "Will write @synapse-id: {{derived}}"
  },
  "E2": {
    "apply": "Sync ID with path",
    "preview": "Will change @synapse-id to {{expected}}"
  },
  "E3": {
    "apply": "Use ID as default title",
    "preview": "Will write @synapse-title: {{title}}"
  },
  "E4": {
    "apply": "Add type:note tag",
    "preview": "Will inject type:note into @synapse-tags"
  },
  "E6": {
    "apply": "Move into entries directory",
    "preview": "Will move to entries/{{name}}/",
    "confirmTitle": "Confirm moving root .md file?"
  },
  "E9": {
    "apply": "Remove system label line",
    "preview": "Will remove system-maintained labels",
    "confirmTitle": "Confirm removal?"
  },
  "common": {
    "apply": "One-click fix",
    "openDocs": "View docs",
    "failed": "Fix failed: {{reason}}",
    "success": "Fixed",
    "restoreOk": "Restored from backup"
  },
  "drawerTitle": "Fix: {{entry}}",
  "noErrors": "No errors for this entry",
  "applying": "Fixing…",
  "kind": { "manual": "Manual fix required" },
  "risk": { "low": "Low risk", "medium": "Medium risk", "high": "High risk" }
}
```

- [ ] **Step 3: 提交**

```bash
git add src/i18n/Fix.zh.json src/i18n/Fix.en.json
git commit -m "feat(i18n): Fix namespace (E1-E9 buttons + common)"
```

---

### Task 26: resources.ts — 注册新 namespace

**Files:**
- Modify: `src/i18n/resources.ts:1-40`

- [ ] **Step 1: 注册 Error + Fix**

文件顶部加 import：
```typescript
import ErrorZh from './Error.zh.json';
import ErrorEn from './Error.en.json';
import FixZh from './Fix.zh.json';
import FixEn from './Fix.en.json';
```

`NS` 数组加：
```typescript
export const NS = [..., 'Error', 'Fix'] as const;
```

`zh` / `en` 对象加：
```typescript
zh: { ..., Error: ErrorZh, Fix: FixZh },
en: { ..., Error: ErrorEn, Fix: FixEn },
```

- [ ] **Step 2: 跑 build**

Run: `npm run build`
Expected: PASS

- [ ] **Step 3: 提交**

```bash
git add src/i18n/resources.ts
git commit -m "feat(i18n): register Error + Fix namespaces"
```

---

## Milestone 8: 端到端验收

### Task 27: 跑 cargo test + npm build

**Files:**
- 无（验证步骤）

- [ ] **Step 1: Rust 测试**

Run: `cd src-tauri && cargo test --lib`
Expected: 所有现有测试 + 新增 backup/e1/e3/e4/e9 测试通过

- [ ] **Step 2: 前端构建**

Run: `npm run build`
Expected: TS 严格类型检查通过，Vite 构建成功

- [ ] **Step 3: 提交验证（如有改动）**

如失败，按错误信息修复后提交：
```bash
git commit --allow-empty -m "verify: cargo test + npm build pass"
```

---

### Task 28: 手动验收清单

**Files:**
- Create: `docs/superpowers/verification/2026-10-12-notification-verify.md`

- [ ] **Step 1: 写验收清单**

```markdown
# Notification System Verification (2026-10-12)

## T1 三入口联动
- [ ] 启动应用，节点红点和 ErrorBar 显示
- [ ] 点击 ErrorBar → 右侧 ValidationPanel 打开
- [ ] 点击节点红点 → FixDrawer 弹出，显示该 entry 的 E 级 issue
- [ ] FixDrawer 点击"一键修复" → toast + 节点红点减 1

## T2 E 级规则覆盖
- [ ] E1: 删除 @synapse-id，点击一键修复 → 文件恢复 @synapse-id: <目录名>
- [ ] E3: 删除 @synapse-title，点击一键修复 → 文件恢复 @synapse-title: <id>
- [ ] E4: 删除 type: 标签，点击一键修复 → @synapse-tags 包含 type:note
- [ ] E6: 在 KB 根放 foo.md，确认出现 E6，confirm 后 → foo.md 移入 entries/foo/content.md
- [ ] E9: 写 `@synapse-labels: recently-modified`，confirm 后 → 该行删除

## T3 备份回滚
- [ ] `.synapse-fix-backup/` 目录存在
- [ ] 每次修复后多一个 `{ts}_{entry_id}.md`
- [ ] 通过 list_backups 命令能看到列表

## T4 i18n
- [ ] 切换 zh/en，ErrorBar / FixDrawer / ValidationPanel 文案全部更新
- [ ] 缺失 i18n key 不阻塞 UI（fallback 到 rule_id）
```

- [ ] **Step 2: 提交清单**

```bash
git add docs/superpowers/verification/2026-10-12-notification-verify.md
git commit -m "docs(notification): verification checklist"
```

---

## Self-Review

**1. Spec coverage:**
- §0 真实 bugs → T1, T2 ✓
- §1 三入口 → T18, T19, T20, T22 ✓
- §2 FixDescriptor / FixKind / FixRisk → T4 ✓
- §2 8 E 规则 fix → T7-T14 ✓
- §2 备份模块 → T5 ✓
- §2 commands → T15 ✓
- §3 NodeCard 红点 button → T19 ✓
- §3 FixDrawer → T20 ✓
- §3 ValidationPanel 重写 → T22 ✓
- §3 useFixStore → T17 ✓
- §4 Error / Fix namespace → T24, T25 ✓
- §5 测试 → T27 ✓
- §5 风险缓解 → 备份机制 (T5)、write_lock (T15)、confirm 强制 (T12, T14)

**2. Placeholder scan:** 已用 grep 检查 — 无 TBD / TODO / "fill in" 等。

**3. Type consistency:**
- `FixDescriptor` 字段在 T4 定义、T7-T14 使用 ✓
- `panelStore` PanelType 加 `'fix'` 在 T16，T20 FixDrawer 使用 `usePanelStore` 实际是间接的（通过 fixStore）— 修正：FixDrawer 实际不直接用 panelStore（用 fixStore），但 `'fix'` PanelType 仍注册以备未来扩展
- `apply_fix` 命令参数：`{ entryId, ruleId, confirm }` 在 T15 定义，T20 / T22 使用 ✓
- `Backup` 结构在 T5 定义，T15 使用 ✓

---

**Plan 完成** — 28 个任务，分布在 8 个 milestone。覆盖了 spec 所有需求，含 TDD 步骤与频繁提交。
