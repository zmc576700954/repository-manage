# kaiwu × rm 合并执行：任务状态与交接文档

> 计划来源：[../merge-kaiwu-plan.md](../merge-kaiwu-plan.md)（定稿 2026-11-17）。
> **本文档是执行态的唯一权威记录**：完成了什么、怎么验收的、剩下的任务是什么、
> 新会话如何接手。每批的详细修改清单见各批次文档。
>
> **执行状态（2026-11-17）：批次 0、0.5、1、2 已全部完成并提交。**
> **批次 3、4、5 未开始，将在另一个会话中完成**——接手指引见文末 §5。

## 纪律（任务约定，贯穿所有批次）

1. 不跑全量测试套件；每处修改补针对性单元测试，跑绿后删除测试代码，生产代码保留。
2. 每批一个文档记录完整修改内容，中断后可从本目录恢复上下文再继续。
3. 每批独立可回滚（每批一次提交）；上一批验收通过才开下一批。

## §1 已完成的任务

### 批次 0 —— 噪声清除三件套 ✅（rm `07ed59e`）

| 项 | 结果 |
|---|---|
| E4 豁免 | tags 体系声明门控：KB 内任一条目声明（`@synapse-tags` 注释或 frontmatter `tags` 键）才启用 `type:` 前缀检查。kaiwu 35 个 E4 误报清零 |
| I2 口径统一 | 提取 `build_link_lookups` + 开放 `resolve_target`/`is_attachment_url`，I2 入/出度纳入 markdown 链接解析，与索引边、W5 同口径；35 个孤立误报清零 |
| frontmatter 未知键保留 | `parse_yaml_fields_with_extras`：kaiwu 四键（优先级/状态/最后更新/下次 review）保序进旁路表 → SQLite `extra_meta` 列 → DetailPanel 只读展示（i18n 双语） |

验收：`synapse validate kb/kaiwu` 从 **35E/0W/35I → 0/0/0**。详见 [batch-0.md](batch-0.md)。

### 批次 0.5 —— CLI 接入 kaiwu 回归门禁 ✅（kaiwu `75a5b3e`）

- kaiwu 新增 `scripts/check_kb_synapse.py`（synapse --json → check_kb 分组 shape 适配器）；
  `scripts/run_regression.sh` 第 3 步改调适配器，`kb_errors: 0` 阈值口径不变；
  check_kb.py 保留作比对参考。
- 接线级验收通过（适配器三路径 + compare 门禁消费 + check_kb 交叉比对一致）。
- **遗留**：全链路 PASS 待 kaiwu 模型目录恢复（`models/Qwen3-4B-Instruct-2507-mlx-4bit`
  被清理，~3GB 下载后跑一次 `scripts/run_regression.sh` 即可补验）——环境问题，与本批无关。
  详见 [batch-0.5.md](batch-0.5.md)。

### 批次 1 —— 写路径改造 + 状态迁出 + 解析健壮化 ✅（rm `73af741`，六项全完成）

1. **frontmatter 保序写入器**（`parser/fm_writer.rs`，Q2.a/Q6.a）：元数据唯一真源改为
   YAML frontmatter；只动区块内目标键，未知键/键序/正文/CRLF 字节级保留；不再插
   HTML 注释（F3 事故路径物理移除）；metadata_updater + E1/E2/E3/E4/E9 修复器全部改写
   frontmatter；解析侧 frontmatter 优先、`@synapse-*` 注释降为只读回退（含中文键别名
   状态/标题/标签）；列表写 CSV 标量（check_kb M1 扁平要求）；状态键方言路由
   （kaiwu 文件写 `状态`，rm 文件写 `status`）。
2. **运行时状态迁出 KB 目录**（Q10.a）：布局 `layout.json` 与版本历史 `history/` 迁到
   app data `indexes/<kb_id>/`（`AppState.runtime_dir_base`）；启动时幂等迁移旧
   `.synapse-layout.json` / `.synapse/history/`；rename/回滚/watcher 路径全部同步。
3. **链接解析根因修复**（Q17/Q12）：`resolve_target` 新增来源目录归一化（`../` 相对
   链接不再依赖 stem 兜底）+ 文档代号最优先键（frontmatter `文档代号` ∪ 路径派生
   NN-MM）；`entries` 表加 `doc_code` 列，增量与 rebuild 同口径。
4. **D1 级联重写**：移动/改名重写相对 markdown 链接（按引用方位置算新相对路径、
   锚点保留、frontmatter 代号是稳定锚不改写、路径派生代号随归档重编）。
5. **README = KB 首页**（D6）：`read_kb_readme` 命令 + home 面板 + 侧栏入口。
6. **状态枚举统一**（D3）：canonical 中文六枚举 + 双语展示映射，旧英文自由文本只读兼容。

验收：套件 312 绿、TS 构建绿、kaiwu validate 保持 0/0。
**GUI 验收步骤（编辑 + 归档演练）在 [batch-1.md](batch-1.md) 末尾，尚未由用户执行。**

### 批次 2 —— 规范正文归位 + 双实现一致性 CI ✅（rm `40e18b5`、kaiwu `af7b8c1`、dev-kb `e036620`）

1. **规范正文**：`docs/kb-spec.md` 从无版本控制的临时目录迁入（F13），30 条规则
   （原 26 + M5/L7/L8/L9）；附录 A 双实现映射表（判据等价 vs 级别语义分开记录）；
   附录 B 盲区声明汇编（D9）。
2. **双实现同步**：kaiwu check_kb.py +M5/L7/L8/L9（26→30 条）；rm +W6 复审过期
   （`SYNAPSE_TODAY` 可钉日期）/W7 状态枚举/W8 文档代号格式/W9 stem 唯一/
   W10+W11 依赖矩阵（新模块 `validator/kb_rules.rs`）；I3 让位（D4）；
   死规则删除（check_yaml_parse、unused_attachments，E8 缺位声明）。
3. **fixtures + 双黄金基线**（Q15.a）：dev-kb `fixtures/kb-spec/`——kaiwu 36 文件
   冻结快照 + 6 项目 27 文件合成样例 + `frozen_check_kb.py` 冻结副本（kaiwu 无远端）
   + `compare.py` 比对器（两侧"今天"钉在 2026-11-17）+ 双基线（`--update-baseline`
   必须 `--reason`）。
4. **dev-kb CI**（Q8）：`kb-spec-consistency` job（checkout rm 构建 synapse CLI 跑比对）；
   原 contract/tests job 不动。

验收：比对器 frozen/live 双模式 PASS，**六对规则（M3a↔W7、M4↔W6、M5↔W8、
L7↔W9、L8↔W10、L9↔W11）在全部样例上逐文件等价，0 违反**；快照 30 条规则双侧
0 findings。详见 [batch-2.md](batch-2.md)。

## §2 提交索引（均未 push）

| 仓库 | 提交 | 内容 |
|---|---|---|
| rm | `dd2fd57` | 合并计划 + 进度目录 |
| rm | `07ed59e` | 批次 0（validator 三件套） |
| rm | `73af741` | 批次 1（六项） |
| rm | `40e18b5` | 批次 2（kb-spec + validator 规则） |
| kaiwu | `75a5b3e` | 批次 0.5（门禁适配器） |
| kaiwu | `af7b8c1` | 批次 2（check_kb 新规则） |
| dev-kb | `e036620` | 批次 2（fixtures + CI job） |

## §3 未完成的任务（将在另一个会话中完成）

> **批次 3、4、5 尚未开始。后续任务放到另一个会话执行**，接手指引见 §5。
> 每批的完整定义见 [../merge-kaiwu-plan.md](../merge-kaiwu-plan.md) §3 批次 3–5 与 §5（D7/D8/D9）。

### 批次 3 —— stdio MCP server（新 bin，复用 `*_impl` 纯函数层）

- 三层工具面（Q14）：read 全开（list_entries / read_entry / get_graph / validate /
  get_history）；结构写自动（save_content `origin=agent` / patch_metadata / apply_fix，
  全部强制 `dry_run`）；语义写走提案（写 KB 内 `_proposals/`，`_` 前缀 = 机制目录）。
- 行为规范（D7）：Agent 编辑**不碰** `最后更新` / `下次 review`；出处由 history 的
  `origin=agent` 记录。
- 验收：EditAgent 冒烟——经 MCP 给一篇文档补 frontmatter 键，全程 dry_run 预览 +
  版本记录 + origin 标记。
- 入口提示：rm 写路径已是 `*_impl` 纯函数（`commands.rs`），`write_lock` 语义要跨
  进程保持；MCP server 建议 `src-tauri/src/bin/` 新 bin 或独立 crate（计划 §3 批次 3）。

### 批次 4 —— EditAgent / ReviewerAgent（Python 独立进程，rm `agents/`）

- 行为宪章（D8 三条）：数字可追溯；被推翻表述显式化（走 supersedes/归档流程）；
  依赖矩阵一致性（批次 2 的 L8/L9 已机器化）。
- 验收：语义提案 → 人审（rm GUI 内看 diff）→ 应用，闭环跑通；全程不直接写盘。

### 批次 5 —— dev-kb 接入为 MDSearchAgent

- 独立仓库不变；`projects.yaml` 已登记；三 Agent 同协议（stdio MCP）互见。
- 验收：三 Agent 同协议互见；dev-kb 自身 CI（契约门禁）不受影响。

### 挂起事项（随批处理，不阻塞）

- kaiwu 全链路回归 PASS 待模型目录恢复（见批次 0.5 遗留）。
- 批次 1 的 GUI 验收（编辑 + 归档演练 + check_kb 0/0）待用户在 tauri:dev 执行。
- 三仓 push 后观察 dev-kb CI 首跑（`kb-spec-consistency` job 依赖 rm `main` 的 synapse）。
- kaiwu 35 篇无 `文档代号` 键（M5 缺键静默）；批次 3 Agent 规范可定义补键建议。

## §4 环境事实（跨会话有用）

- 仓库路径：rm `/Users/zhumingchen/repository-manage`、kaiwu `/Users/zhumingchen/kaiwu`
  （**无 git 远端**，纯本地）、dev-kb `/Users/zhumingchen/dev-kb`（远端
  `zmc576700954/dev-kb`；rm 远端 `zmc576700954/repository-manage`）。
- rm CLI：`cd src-tauri && cargo build --bin synapse` →
  `./target/debug/synapse validate <kb> [--json]`；`SYNAPSE_TODAY` 可钉日期。
- fixture 比对：`cd ~/dev-kb && python3 fixtures/kb-spec/compare.py
  --rm-repo ~/repository-manage`（frozen 模式 = CI 同款）；实时比对加
  `--kaiwu-repo ~/kaiwu`；更新基线必须 `--update-baseline --reason "..."`。
- kaiwu 门禁：`scripts/run_regression.sh`（第 3 步已走 synapse CLI，
  `SYNAPSE_BIN` 默认指向 rm 的 target/debug/synapse）。
- kaiwu 文档格式：YAML frontmatter 四键（优先级/状态/最后更新/下次 review），
  无 `@synapse-*` 注释、无 tags；NN-MM 代号可从路径机械派生。
- 测试纪律工具链：`cargo test`（312 passed 基线）、`npm run build`（TS 严格检查）。

## §5 新会话接手指引

1. **读这三份**：本文档（全局状态）→ [../merge-kaiwu-plan.md](../merge-kaiwu-plan.md)
   §3 批次 3（任务定义）与 §5 的 D7/D8（行为宪章）→ [batch-2.md](batch-2.md) 末尾
   的「遗留与续接点」。
2. **验证现场**：三仓 `git log -1` 应为 §2 表末行提交；`cargo test` 312 绿；
   `SYNAPSE_TODAY=2026-11-17 ./target/debug/synapse validate ~/kaiwu/kb/kaiwu --json`
   应 0/0/0；dev-kb `compare.py` frozen 模式应 PASS。
3. **批次 3 开工顺序**：计划 §3 批次 3 表 → Q14 三层工具面 → D7 行为规范 →
   实现（新 bin）→ 冒烟验收 → 更新 `merge-progress/batch-3.md`（新建）→
   rm 单仓提交（每批一次提交的纪律不变）。
4. **纪律不变**：批次间独立可回滚；针对性单元测试跑绿后删除；每批验收通过才开下一批。
