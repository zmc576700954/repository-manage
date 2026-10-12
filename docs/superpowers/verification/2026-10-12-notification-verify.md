# Notification System Verification (2026-10-12)

## Build verification

- [x] `cd src-tauri && cargo test --lib` — 212 passed, 0 failed, 1 ignored
- [x] `npm run build` — tsc + vite build success (279 modules, 406KB)

## T1 三入口联动

- [ ] 启动应用，节点红点和 ErrorBar 显示
- [ ] 点击 ErrorBar → 右侧 ValidationPanel 打开
- [ ] 点击节点红点 → FixDrawer 弹出，显示该 entry 的 E 级 issue
- [ ] FixDrawer 点击"一键修复" → toast + 节点红点减 1
- [ ] ValidationPanel 切换 Error/Warning/Info tab
- [ ] ValidationPanel 切换 groupBy severity/entry

## T2 E 级规则覆盖

- [ ] E1: 删除 `@synapse-id`，点击一键修复 → 文件恢复 `@synapse-id: <目录名>`
- [ ] E2: 修改 `@synapse-id` 与目录不一致，点击一键修复 → 文件恢复一致
- [ ] E3: 删除 `@synapse-title`，点击一键修复 → 文件恢复 `@synapse-title: <id>`
- [ ] E4: 删除 `type:` 标签，点击一键修复 → `@synapse-tags` 包含 `type:note`
- [ ] E5: 目录嵌套 > 3 层，ValidationPanel 显示 E5 但按钮为 "Manual"，点击不执行修复
- [ ] E6: 在 KB 根放 `foo.md`，确认出现 E6，confirm 后 → `foo.md` 移入 `entries/foo/content.md`
- [ ] E7: 创建 `_reserved` 目录，确认出现 E7，ValidationPanel 显示 "Manual"
- [ ] E9: 写 `@synapse-labels: recently-modified`，confirm 后 → 该行删除

## T3 备份回滚

- [ ] `.synapse-fix-backup/` 目录存在
- [ ] 每次修复后多一个 `{ts}_{entry_id}.md` + `.meta`
- [ ] 通过 `list_backups` 命令能看到列表
- [ ] `restore_backup` 可恢复（手动测试 Tauri 控制台）

## T4 i18n

- [ ] 切换 zh/en，ErrorBar / FixDrawer / ValidationPanel 文案全部更新
- [ ] 缺失 i18n key 时显示 rule_id fallback（不空白）

## 文件状态

| 文件 | 状态 |
|---|---|
| `src/components/ValidationPanel.tsx` | ✅ 修 kbRoot + i18n |
| `src/utils/tRule.ts` | ✅ warn + fallback |
| `src/i18n/Validation.{zh,en}.json` | ✅ 8 E 规则全量 |
| `src-tauri/src/validator/fix.rs` | ✅ FixDescriptor / FixKind / FixRisk |
| `src-tauri/src/fs/backup.rs` | ✅ backup + restore + sidecar .meta |
| `src-tauri/src/validator/fix_impls/` | ✅ E1-E9 全部实现 |
| `src-tauri/src/commands.rs` | ✅ apply_fix / list_backups / restore_backup |
| `src/store/panelStore.ts` | ✅ 'fix' PanelType |
| `src/store/fixStore.ts` | ✅ 新建 |
| `src/components/ErrorBar.tsx` | ✅ button + i18n |
| `src/components/NodeCard.tsx` | ✅ 红点 button + 数字 |
| `src/components/FixDrawer.tsx` | ✅ 新建 |
| `src/components/FixWizard.tsx` | ✅ 新建 |
| `src/components/ValidationPanel.tsx` | ✅ 重写 |
| `src/App.tsx` | ✅ 集成 FixDrawer |
| `src/components/Error.{zh,en}.json` | ✅ 新建 |
| `src/components/Fix.{zh,en}.json` | ✅ 新建 |
| `src/i18n/resources.ts` | ✅ 注册 Error + Fix |
