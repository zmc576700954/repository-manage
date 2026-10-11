# KB Standards v2 Verification

> 本文档记录 kb-standards-v2 实施后的全量手工验收清单。
> 由项目维护者逐项测试并勾选。

---

## §15.2 手工验收（32 条）

### 解析器与字段
- [ ] #1 新建 entry 模板，跑 synapse validate → 全绿
- [ ] #2 删 @synapse-title → E3 报错
- [ ] #3 改 @synapse-id 与路径不一致 → E2 报错
- [ ] #4 删 @synapse-tags → E4 报错
- [ ] #5 写 @synapse-labels: important, wip → 解析成功
- [ ] #6 写 @synapse-labels: recently-modified → E9 报错
- [ ] #7 写 @relation: derived [[x]] → 解析为 derived 关系
- [ ] #8 写老 @synapse-group: foo → 静默忽略 + W6 警告

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
- [ ] #19 synapse migrate labels --dry-run → 显示 diff
- [ ] #20 synapse migrate labels → 备份 + 改写 + log
- [ ] #21 迁移后 validate → 无 W6
- [ ] #22 group=version/1.0 → 启发式保留为 W6

### 多 KB
- [ ] #23 添加新 KB → 切换器显示
- [ ] #24 切到有错误 KB → 红条更新
- [ ] #25 synapse validate-all → 列出所有 KB

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
- [ ] #33 KB 根 random.md → E6
- [ ] #34 4 层目录 → E5
- [ ] #35 _inbox/foo/ → 通过
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
- [ ] R8 cargo test → 全绿