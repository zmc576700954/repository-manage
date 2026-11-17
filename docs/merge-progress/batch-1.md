# 批次 1 —— 写路径改造 + 状态迁出 + 解析健壮化

> 状态：**✅ 代码完成，自动化验收通过**（2026-11-17）。
> 计划见 [../merge-kaiwu-plan.md](../merge-kaiwu-plan.md) §3 批次 1。
> 验收：rm 编辑 kaiwu 任一文档后 `check_kb.py --strict` 仍 0/0；
> 演练「06-微调与训练/ → 99-归档/」移动，指向它的相对链接全部自动改写，check_kb 仍 0/0（D1）。

## 六个子项（全部完成）

1. ✅ frontmatter 保序写入器（fm_writer）+ metadata_updater 改调 + 修复器去注释化（Q6.a）
2. ✅ 运行时状态迁出 KB 目录：layout/history → app data（Q10.a）
3. ✅ 链接解析根因修复：来源 dir 归一化 + 文档代号优先键（Q17.a/Q12.a）
4. ✅ 移动/改名级联重写相对 markdown 链接（D1）
5. ✅ README = KB 首页（D6）：`read_kb_readme` 命令 + 「home」面板 + Sidebar 入口
6. ✅ 状态枚举统一（D3）：canonical 中文枚举 + locale 展示映射

### 1.5 README = KB 首页（补充记录）

- 后端：`read_kb_readme` 命令（README.md/readme.md/Readme.md 三个名字探测，
  2 MiB 上限），README 保持不索引/不入图谱/不参与关系解析（scanner 原有豁免不变）。
- 前端：`KbHomePanel`（@/views/KbHomePanel）+ panelStore 新 `home` 面板类型 +
  RightPanelStack 接线 + Sidebar「📖 KB 首页」按钮；README 内 wikilink 按
  broken-ref 渲染为纯文本（首页只读）。
- i18n：KbHomePanel.{zh,en}.json 新命名空间注册；Sidebar/RightPanel 各加 title/button 键。

### 1.6 状态枚举统一（补充记录）

- `EntryMetadataEditor`：STATUS_OPTIONS 换成 canonical 中文六枚举
  （草案/模板/稳定/需持续监控/过期/已归档，= kaiwu M3a STATUS_BASES）；
  默认值 'stable' → '草案'；旧英文自由文本与括号注记自定义值作为只读兼容
  选项保底展示（不静默改写）。
- 展示映射 `STATUS_DISPLAY`：代码内双语表（zh 显示 canonical 原文，
  en 映射 Draft/Template/Stable/Monitor/Outdated/Archived）。
  **决策记录**：枚举标签是封闭集合的数据表而非 i18n 键——表天然双语言，
  避免 12 个近似重复的 JSON 键；如需外化可后续挪进 i18n。
- 写入路径与 1.1 的方言路由闭环：kaiwu 文件的状态编辑写 `状态` 键
  （M3a 枚举对象），rm 文件写 `status` 键。

## 新增测试（跑绿后删除）

## 修改清单

### 1.1 frontmatter 保序写入器（已完成 2026-11-17）

- 新增 `src-tauri/src/parser/fm_writer.rs`（Q2.a/Q6.a 核心）：
  - `FrontmatterEdit { set, remove }` + `apply_edits(content, edit)`：
    只重写 frontmatter 区块内的目标键行区间，区块外字节原样保留
    （含 CRLF 正文、未知键、键序、区块内注释）。
  - 区块定位与解析器同一宽容口径（第一个 `---` 块）；无区块时在文件顶部
    创建（首行位置 = 文件第 1 行，check_kb M1 安全；**不再插 HTML 注释到
    frontmatter 之前**——F3 事故路径已物理移除）。
  - 值序列化：标量交给 serde_yaml 定引号；**列表写 CSV 标量**
    （kaiwu check_kb M1 要求每行扁平「键: 值」，flow 列表 `[a, b]` 会触发 M1）。
  - `has_frontmatter` / `has_key` 辅助。
- `metadata_updater.rs`：`apply_metadata_changes` 改走 fm_writer
  （title/tags/labels/status → frontmatter 键，CSV 列表）。
  **方言路由（D3）**：文件 frontmatter 已有中文 `状态` 键 → status 写 `状态`
  （M3a 枚举对象），否则写 ASCII `status`；不再产生并行双键。
- `entry.rs` 解析优先级反转（Q2.a）：
  - `ParsedBody.effective(key)`：frontmatter 优先，`@synapse-*` 注释只读兼容回退。
  - 读别名：`状态→status`、`标题→title`、`标签→tags`（别名不覆盖原生键）。
  - `RawEntry` 新增 `doc_code: Option<String>`（frontmatter `文档代号`，Q12）。
- 修复器去注释化：e1（id）/e2（id）/e3（title）/e4（tags）全部改为 fm_writer
  写 frontmatter；e9 有 frontmatter 时写净化后的 labels 键（空则删键），
  无 frontmatter 的旧文件维持注释行过滤（保守策略不变）。
- `commands.rs::update_synapse_id_in_file`（rename 第 6 步）：frontmatter id 写入。
- 行为决策（记录）：修复器/metadata 写入对旧注释不清除、不改写（只读兼容），
  解析侧 frontmatter 优先保证语义单一。

### 1.1 既有测试更新（行为变更跟随，非临时测试）

- `metadata_updater.rs` 测试 ×6 重写为 frontmatter 断言（含 kaiwu 四键保留、
  无 frontmatter 老文件顶部创建区块、方言路由）。
- `commands.rs` 两个 update_metadata 测试 + rename cascade 测试断言改 frontmatter。
- `fix_impls` e1/e2/e3/e4/e9 测试断言全部改为 frontmatter 行为
  （e9 保留旧注释路径测试，新增 frontmatter 路径测试）。

### 1.2 运行时状态迁出（已完成）

- `paths.rs` 去掉 `.synapse` 硬编码后不再需要 kb_runtime_dir（改为显式 base）。
- `AppState` 新增 `runtime_dir_base: PathBuf`（生产 = `indexes_dir()`，测试注入
  tempdir，杜绝测试污染真实 app data）；`runtime_dir_or_err` = base/kb_id。
- `layout_store.rs`：文件名 `.synapse-layout.json` → `layout.json`，
  读写入参语义改为 runtime 目录；`save_layout` 先建目录（新 KB 首次保存）。
- `history.rs`：`history_dir` = `<runtime_dir>/history/<entry-key>/`；
  `ContentSavedEvent.kb_root` 更名 `history_root`；保存/回退/watcher 外部变更
  三条版本痕路径全部改走 runtime 目录。
- `state.rs::migrate_legacy_runtime_state`：`.synapse-layout.json` →
  `runtime/layout.json`、`.synapse/history/` → `runtime/history/`；幂等
  （新旧并存以新为准、旧产物删除）；`.synapse` 空壳清理。
  `run_startup_rebuild` 在 rebuild 前对当前 KB 执行迁移。
- `execute_rename_impl` 的布局/历史迁移与回滚路径同步走 runtime 目录。

### 1.3 链接解析根因修复（已完成）

- `resolve_target` 增加「来源文档 dir 归一化成根相对路径再查 by_rel_path」步骤
  （kaiwu 的 `../06-x/03-y.md` 不再依赖 stem 兜底）。
- 文档代号索引：frontmatter `文档代号` ∪ 路径机械派生（目录序号-文件序号 NN-MM，
  `doc_code_from_parts`），作为 resolve_target 最优先键（Q12/D2）；
  `entries` 表加 `doc_code` 列（含 ALTER 迁移），增量和 rebuild 两条解析路径
  同口径（都构建代号索引 + 来源归一化）。
- `resolve_target` 签名收敛为 `(target, source_dir, &LinkLookups)`；
  `LinkLookups` 增加 `by_doc_code`。
- `relative_path_between(from_dir, to_rel)`：D1 级联重写的路径计算。

### 1.4 移动/改名级联重写相对链接（已完成）

- `execute_rename_impl` 第 4 步（move_entry 复用同管线）在 wikilink 级联之外
  新增相对 markdown 链接级联：
  - 对每个引用文件，用统一解析口径（含 1.3 归一化 + 代号）解析其 markdown 链接；
  - 命中被移动条目（旧 id）才重写；新 URL = 从引用方 dir 到新路径的相对形式；
  - 锚点/查询后缀原样保留；frontmatter `文档代号` 命中的链接是稳定锚不改写；
  - 路径派生代号命中 → 重写为新路径派生代号（kaiwu 归档重编序号场景）；
  - 与 wikilink 级联共用回滚缓存，失败整体回滚。
- D1 归档演练测试（06-训练 → 99-归档，同目录/跨目录/锚点/代号/稳定锚五场景）
  已跑绿并按纪律删除；验收阶段会在 kaiwu KB 副本上做全量演练。

## 新增测试（跑绿后删除）

- fm_writer ×9：已跑绿并删除（保序保值、追加位置、多行值整体替换、
  CRLF 正文保留、kaiwu 四键 + rm 三键共存、CSV 列表 round-trip）。
- 1.3 ×5（已删）：normalize_source_relative（../ 与越根）、来源归一化命中、
  代号优先（frontmatter + 路径派生）、doc_code 落库、增量与 rebuild 同口径。
- 1.4 ×1（已删）：D1 归档演练五场景 + 改写后引用仍解析成边。
- 1.2 ×3（已删）：迁移搬家 + 幂等（新位置优先）+ 无旧产物 no-op。

## 验收记录

### 自动化验收（✅ 2026-11-17）

- `cargo test`：312 passed / 0 failed（含 1.1-1.4 行为变更后的既有测试更新）。
- `npm run build`（tsc 严格检查）：绿。
- `cargo build --bin synapse`：0 error。
- `synapse validate kb/kaiwu`：**0 error / 0 warning / 0 info**（批次 1 全部改动后仍 0/0）。

### GUI 验收（⏳ 待用户在 tauri:dev 里执行，步骤如下）

1. **编辑不破**：打开 kaiwu KB，任选一篇文档，在详情面板改状态（如 稳定 → 需持续监控）
   或标题，保存后到 kaiwu 仓库跑 `python scripts/check_kb.py --project kaiwu --strict --format json`
   → 期望 `[]`（0/0）。四键保持 frontmatter 首行、键序不变。
2. **归档演练（D1）**：右键 06-微调与训练/ 下任一被引用的文档 → 改名/移动到 99-归档/，
   级联开启。期望：所有指向它的相对链接（含 ../ 相对与锚点后缀）自动改写为
   `../99-归档/<名>.md` 形式；`check_kb.py --strict` 仍 `[]`。
3. **首页**：侧栏「📖 KB 首页」→ README 内容可读；README 不出现在图谱/条目里。
4. **状态枚举**：详情面板状态下拉显示 草案/模板/稳定/需持续监控/过期/已归档
   （英文界面显示 Draft/Template/Stable/Monitor/Outdated/Archived）。

### 已知残留（接受不修，记录在案）

- 旧 `@synapse-*` 注释在写入后保留为 stale（只读兼容，解析优先级 frontmatter），
  不做清除迁移（避免无谓的 KB 数据 churn；可在批次 3 的 Agent 行为规范里定义
  「可选清理」操作）。
- frontmatter 区块内的行尾统一为 \n（区块外字节原样保留）；kaiwu 文件均为 LF，无实际影响。
- 状态括号注记（如 稳定（首轮结论））编辑器只提供基准值，注记需手改（M3a 容忍注记）。
- validate 用的是 1.3 后的 CLI 二进制；1.2/1.5/1.6 不触碰校验路径。
