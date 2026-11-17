# kaiwu × rm 合并执行：任务状态与交接文档

> 计划来源：[../merge-kaiwu-plan.md](../merge-kaiwu-plan.md)（定稿 2026-11-17）。
> **本文档是执行态的唯一权威记录**：完成了什么、怎么验收的、剩下的任务是什么、
> 新会话如何接手。每批的详细修改清单见各批次文档。
>
> **执行状态（2026-11-17）：批次 0～5 全部完成（计划收官）。**
> 收官状态与计划外方向见文末 §5 与 [batch-5.md](batch-5.md) §续接点。

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

### 批次 3 —— stdio MCP server ✅（rm 本批提交）

1. **纯函数层下沉**：commands.rs 的 tauri 门控下沉到每个 command 包装器，
   `*_impl` 层 tauri-free（Q11）；`apply_fix` 抽 `apply_fix_impl` 内核；
   status 写路径校验对齐 D3 canonical 枚举。
2. **`SaveOrigin::Agent`**（D7）：出处由版本历史 `origin=agent` 记录；
   GUI 版本列表双语标签同步。
3. **`mcp.rs` + `synapse-mcp` bin**：换行分隔 JSON-RPC 2.0（与 dev-kb
   同协议）；三层工具面九工具（Q14）——read 全开 / 结构写强制 `dryRun` /
   语义写 `propose_change` 落 `_proposals/`；私有索引
   `indexes/<kb_id>.agent.db`；注册表只读（`--kb` 未注册时派生稳定 id）。
4. **D7 闸门**：`save_content` 比对 `最后更新` / `下次 review`，任何改动
   拒绝（`agent-frozen-field`）；`patch_metadata` 结构上不含这两键。
5. **`_` 机制目录豁免**（rm validator + kaiwu check_kb + kb-spec 豁免节 +
   dev-kb proj-mechanism fixture + 基线重冻结）：豁免校验、仍索引展示、
   点名照查。
6. 补键建议（batch-2 遗留 #1）：落盘响应 `hints.docCodeMissing`。

验收：EditAgent 冒烟 24 项断言全 PASS（dry_run 预览 → 落盘 + origin=agent
版本 → D7 拒绝 bump → 提案落盘 → 双校验器 0E/0W）；套件 312 绿、TS 构建绿。
详见 [batch-3.md](batch-3.md)。

### 批次 4 —— EditAgent / ReviewerAgent + GUI 提案审阅闭环 ✅（rm 本批提交）

1. **rm `agents/`**（Python 独立进程，纯标准库，无 LLM SDK）：`mcp_client.py`
   （stdio MCP 客户端）、`edit_agent.py`（结构写自动 dry_run 落盘 /
   正文语义改动出提案；确定性 edit ops 或 `llm_cmd` 外挂生成器）、
   `reviewer_agent.py`（D8 三条：数字可追溯【仅新增行】、被推翻表述
   显式化、依赖矩阵一致性【经 validate】；审阅经 MCP `review_proposal`
   落报告）、`README.md`（行为宪章 + 闭环图 + 用法）。
2. **提案命令层**：新模块 `proposals.rs`（解析/列举/状态/审阅，GUI 与
   MCP 共用）+ 四个 Tauri 命令（list/read/apply/dismiss）；
   `apply_proposal` 带 D7 闸门（Agent 起草全文 bump 复审日期连人批准也拦）
   并以 origin=agent 记版本；mcp.rs 增第 10 个工具 `review_proposal`。
3. **GUI 提案审阅面板**：`📋 语义提案`（列表 status 徽标 → 审阅报告 +
   行级 LCS diff → 批准应用 / 驳回），i18n 双语成对。

验收：kaiwu 副本闭环跑通——EditAgent 出提案 → ReviewerAgent verdict=pass
（反向：凭印象的 99.9 → needs-attention；D7 反向：本地预检拒绝）→
apply 落盘 origin=agent（Rust 测试路径）→ 双校验器 0E/0W；套件 312 绿、
TS 构建绿。GUI 人审步骤在 [batch-4.md](batch-4.md) §GUI，待用户执行。

### 批次 5 —— dev-kb 接入为 MDSearchAgent ✅（rm 本批提交；dev-kb 零改动）

1. **mcp_client 泛化**：通用 `McpClient` 基类（任意 server 命令 + env 注入），
   协议层全部上移；`SynapseMcpClient` 只留 KB 便捷方法。
2. **`agents/mdsearch.py`**：dev-kb 只读检索桥接——index_status 快路径 /
   search（**带超时降级闸**，rerank 模型缺失时不拖死流程）/ read_source。
3. **双 Agent 接 `--mdsearch`**：edit_agent 检索上下文进 `llm_cmd`（related）
   与报告（relatedDocs）；reviewer_agent 对 KB 语料追不到的数字二次溯源
   （一次 search + ≤5 次 read_source）。
4. **宿主登记示例** `agents/mcp.example.json` + README 互见节。

验收：互见冒烟 13 项全 PASS（双 server 同协议 2025-06-18、kaiwu 923
chunks、降级闸、闭环提案→审阅 pass、validate 0E/0W）；dev-kb CI 三 job
本地等价执行全绿（契约 ✓ / pytest 115 ✓ / compare frozen ✓）；
**dev-kb 仓库零改动**（ADR-0030 维持）。遗留：search 功能路径待
bge-reranker 模型缓存（环境遗留，降级闸已验证）。详见 [batch-5.md](batch-5.md)。

## §2 提交索引（均未 push）

| 仓库 | 提交 | 内容 |
|---|---|---|
| rm | `dd2fd57` | 合并计划 + 进度目录 |
| rm | `07ed59e` | 批次 0（validator 三件套） |
| rm | `73af741` | 批次 1（六项） |
| rm | `40e18b5` | 批次 2（kb-spec + validator 规则） |
| rm | 本批 HEAD（文档在本仓内，自引用哈希见 `git log`） | 批次 3（MCP server + 机制目录豁免） |
| rm | 本批 HEAD | 批次 4（agents/ 双 Agent + 提案命令层 + GUI 审阅面板） |
| rm | 本批 HEAD | 批次 5（MDSearchAgent 桥接 + 双 Agent --mdsearch + 宿主登记示例） |
| kaiwu | `75a5b3e` | 批次 0.5（门禁适配器） |
| kaiwu | `af7b8c1` | 批次 2（check_kb 新规则） |
| kaiwu | `83304df` | 批次 3（check_kb `_` 机制目录豁免） |
| dev-kb | `e036620` | 批次 2（fixtures + CI job） |
| dev-kb | `a9ae4d0` | 批次 3（frozen 副本同步 + proj-mechanism + 基线） |

## §3 未完成的任务（计划内批次已收官；以下是挂起事项，随需处理）

> **批次 0～5 全部完成**。本节不再有批次任务，只剩挂起事项（均不阻塞）。

### 挂起事项（随批处理，不阻塞）

- ~~kaiwu 全链路回归 PASS 待模型目录恢复~~ 已终止（2026-11-17 实验区归档删除，
  门禁随归档退役；口径由 fixture 比对 + synapse validate 承接）。
- dev-kb `search` 功能路径待 bge-reranker/embedding 模型缓存（批次 5 遗留，
  环境问题；降级闸已验证）。另：kaiwu 项目索引待重建（projects.yaml 已改指
  `~/Documents/kaiwu-kb`，tuning 意图映射已重写，契约校验 pass）。
- 批次 1 的 GUI 验收（编辑 + 归档演练 + check_kb 0/0）待用户在 tauri:dev 执行。
- 批次 4 的 GUI 人审步骤（提案面板看 diff → 批准/驳回）待用户在 tauri:dev 执行
  （[batch-4.md](batch-4.md) §GUI 人审步骤）。
- dev-kb CI 首跑已观察（2026-11-17）：`kb-spec 双实现一致性` job 修复后 **绿**
  （两处修复：job 的 cargo build 加 `--no-default-features`，dev-kb `762477a`；
  rm 侧 tauri-plugin-dialog/store 改 optional——此前 tauri-free 构建隐性依赖
  glib，rm `5605e96`）。`单元与集成测试` job 为 dev-kb 既有失败（4 个 hybrid
  检索用例 ubuntu 挂 / 本地 venv 绿，平台相关，与合并无关）——处置待定。
  另：dev-kb 仓库级 git 代理配置（127.0.0.1:7897）指向的代理未运行，push 需
  `-c http.proxy=` 绕过或清理该配置。
- 跨进程写锁未收口：GUI 与 MCP 并发改同一文件的读-改-写靠后写覆盖，
  完整解（全部写方接同一把文件锁）待需要时单独一批（batch-3.md §已知边界）。
- 提案 apply 前的目标漂移检测（目标条目在审阅期间被改时提示）——
  加固项（batch-4.md §已知边界 #2）。

### 计划外可做的方向（未排期，见 batch-5.md §续接点）

- LLM 接入：edit_agent 的 `llm_cmd` 外挂（kaiwu 模型）+ reviewer 语义级 D8 判断。
- GUI 内的 MDSearchAgent 检索入口（当前只经 CLI/宿主）。

## §4 环境事实（跨会话有用）

- 仓库路径：rm `/Users/zhumingchen/repository-manage`（远端
  `zmc576700954/repository-manage`）、dev-kb `/Users/zhumingchen/dev-kb`
  （远端 `zmc576700954/dev-kb`）。
  **kaiwu 仓库已于 2026-11-17 归档删除**（全量备份
  `~/Archive/kaiwu-full-20261117.tar.gz` 1.5G，含 LoRA adapters/训练集/门禁脚本）；
  知识库本体迁至 **`~/Documents/kaiwu-kb`**（rm 是唯一管理入口，GUI ⚙ 添加该路径即可）。
- rm CLI：`cd src-tauri && cargo build --bin synapse` →
  `./target/debug/synapse validate <kb> [--json]`；`SYNAPSE_TODAY` 可钉日期。
- rm MCP server：`cd src-tauri && cargo build --bin synapse-mcp` →
  `./target/debug/synapse-mcp --kb <path>`（换行分隔 JSON-RPC 2.0 over stdio，
  与 dev-kb 同协议；注册表只读，`--kb` 未注册时派生 `agent-<hash>` 稳定 id）。
- fixture 比对：`cd ~/dev-kb && python3 fixtures/kb-spec/compare.py
  --rm-repo ~/repository-manage`（frozen 模式 = CI 同款）；实时比对加
  `--kaiwu-repo <已删>`（kaiwu 仓库归档后实时比对不可用；frozen 模式为唯一口径）；更新基线必须 `--update-baseline --reason "..."`。
- kaiwu 门禁已随实验区归档退役（`run_regression.sh` 在归档内）；`kb_errors: 0`
  口径由 dev-kb 的 fixture 双实现比对与 `synapse validate` 承接。
- kaiwu 文档格式（KB 现：`~/Documents/kaiwu-kb`）：YAML frontmatter 四键（优先级/状态/最后更新/下次 review），
  无 `@synapse-*` 注释、无 tags；NN-MM 代号可从路径机械派生。
- `_` 前缀路径段 = 机制目录（`_proposals/` 首例）：双侧校验器豁免、
  仍索引展示（批次 3 定稿，规范「豁免」节有契约）。
- 测试纪律工具链：`cargo test`（312 passed 基线）、`npm run build`（TS 严格检查）。

## §5 新会话接手指引（计划已收官，按需续接）

1. **读这两份**：本文档（全局状态）→ [batch-5.md](batch-5.md) 的
   「已知边界与遗留」与「续接点」（计划外方向清单）。
2. **验证现场**：rm/kaiwu/dev-kb 三仓 `git log -1` 应为 §2 表末行提交；
   `cargo test` 312 绿；
   `SYNAPSE_TODAY=2026-11-17 ./target/debug/synapse validate ~/Documents/kaiwu-kb --json`
   应 0/0/0；dev-kb `compare.py` frozen 模式应 PASS；
   `synapse-mcp --help` 可运行；`python3 agents/mdsearch.py --status`
   可见六项目登记。
3. **新任务开工**：从 §3 挂起事项或 batch-5.md §续接点挑方向；仍守纪律
   ——针对性单元测试跑绿后删除；每批（或每项）独立提交可回滚。
