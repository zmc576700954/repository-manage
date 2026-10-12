# KB Standards v2 Verification

> 本文档记录 kb-standards-v2 实施后的全量手工验收清单。
> 由项目维护者逐项测试并勾选。

## 验收记录（2026-10-12）

### CLI 层（已完成，18/48）

| 类型 | 已验收 | 范围 |
|------|-------|------|
| 解析器/字段 | #1 #2 #3 #4 #5 #6 #7 #8 | 8/8 |
| 迁移 | #19 #20 #21 #22 | 4/4 |
| 多 KB | #25 | 1/1 |
| 边界 | #33 #34 #35 | 3/3 |
| 回归 | R8 | 1/1 |

修复的 8 个 issue（按 commit 顺序）：
- `count_by_severity` 用 `matches!` 绑定模式，恒为 true（61a7026）
- `@synapse-id` 永远被路径覆盖（5941663）
- 扫描器 max_depth(3) 阻止 E5 检测深目录（2d2ef0f）
- 扫描器扫到迁移备份目录（fb95aab）
- W6 规则（legacy @synapse-group）缺失（908dc5d）
- validate-all 只是占位（6b1f8f3）
- 迁移预览输出 `migrated` 应为 `would_modify`（0845661）
- E2 对 `_inbox` 等保留前缀应剥离（a73e4e8）

### UI 层（待人工验收，30/48）

以下项需要启动 `npm run tauri:dev` 后由维护者人工验证：

- 校验 UI：#9 #10 #11 #12 #13 #14
- 标签筛选：#15 #16 #17 #18
- 多 KB：#23 #24
- i18n：#26 #27 #28
- 性能：#29 #30 #31 #32
- 边界：#36 #37 #38 #39 #40
- 回归：R1 R2 R3 R4 R5 R6 R7

自动化层验证已通过 179 个 Rust 单元测试 + Vite 编译（`npm run build`）。

---

## §15.2 手工验收（32 条）

### 解析器与字段
- [x] #1 新建 entry 模板，跑 synapse validate → 全绿
- [x] #2 删 @synapse-title → E3 报错（注：需同时清空 @synapse-id，id→title fallback 是设计）
- [x] #3 改 @synapse-id 与路径不一致 → E2 报错
- [x] #4 删 @synapse-tags → E4 报错
- [x] #5 写 @synapse-labels: important, wip → 解析成功
- [x] #6 写 @synapse-labels: recently-modified → E9 报错
- [x] #7 写 @relation: derived [[x]] → 解析为 derived 关系
- [x] #8 写老 @synapse-group: foo → 静默忽略 + W6 警告

### 校验 UI
- [ ] #9 故意写 E4 错误 entry → 红边 + 感叹号
- [ ] #10 悬停感叹号 → tooltip 显示规则详情
- [ ] #11 顶部红条「X 个错误」 → 点击打开 Validation 面板
- [ ] #12 Validation 面板「跳到 entry」 → 地图聚焦
- [ ] #13 故意 W4 → 黄边 + Sidebar 黄点
- [ ] #14 Settings → Validation 改 ignore → 重校验后警告消失

### 标签筛选
- [ ] #15 勾 important → 只显示带此 label
- [ ] #16 多选 important + wip → OR
- [ ] #17 勾 recently-modified → 显示 mtime < 7 天
- [ ] #18 清空筛选 → 全部显示

### 迁移
- [x] #19 synapse migrate labels --dry-run → 显示 diff
- [x] #20 synapse migrate labels → 备份 + 改写 + log
- [x] #21 迁移后 validate → 无 W6
- [x] #22 group=version/1.0 → 启发式保留为 W6

### 多 KB
- [ ] #23 添加新 KB → 切换器显示
- [ ] #24 切到有错误 KB → 红条更新
- [x] #25 synapse validate-all → 列出所有 KB

### i18n
- [ ] #26 切英文 → 文案变英文
- [ ] #27 切中文 → 变中文
- [ ] #28 删 Validation.zh.json 一个 key → fallback 英文

### 性能
- [ ] #29 100 entry → 启动 <500ms
- [ ] #30 1000 entry → 启动 <2s
- [ ] #31 编辑一个 entry → 增量 <200ms
- [ ] #32 跑全量校验 → UI 不卡

## §15.3 边界（8 条）
- [x] #33 KB 根 random.md → E6
- [x] #34 4 层目录 → E5
- [x] #35 _inbox/foo/ → 通过
- [ ] #36 Finder 改名 → 检测 + 提示
- [ ] #37 同时编辑两个 → 锁串行
- [ ] #38 删 content.md → 灰显
- [ ] #39 老 KB 打开 → 不报错
- [ ] #40 config YAML 损坏 → 提示不覆盖

## §15.5 回归（8 条）
- [ ] R1 老 KB → 正常
- [ ] R2 创建 entry UI → 正常
- [ ] R3 拖拽节点 → 不破坏
- [ ] R4 现有关系 → 正常
- [ ] R5 KB 切换 → 正常
- [ ] R6 i18n 切换 → 不丢失
- [ ] R7 npm run tauri:build → 成功
- [x] R8 cargo test → 全绿（179 passed）