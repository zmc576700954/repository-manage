# 批次 4 —— EditAgent / ReviewerAgent + GUI 提案审阅闭环

> 状态：**✅ 已完成**（2026-11-17）。计划见 [../merge-kaiwu-plan.md](../merge-kaiwu-plan.md) §3 批次 4。
> 验收：语义提案 → ReviewerAgent 出 D8 报告 → 人审（GUI diff）→ 应用，
> 闭环跑通；EditAgent 全程不直接写盘（一切访问经 stdio MCP）。

## 修改清单

### 4.1 rm `agents/` —— Python 独立进程（Q4.a：不引入 LLM SDK）

- `agents/mcp_client.py`：stdio MCP 客户端（换行分隔 JSON-RPC 2.0，与
  dev-kb 同协议），拉起 `synapse-mcp --kb <path>` 子进程；工具级错误走
  `ToolError`（进程不死），协议错误抛 `McpError`；`find_entry` 按 id 或
  路径后缀双方式解析。纯标准库。
- `agents/edit_agent.py`（EditAgent 执行体）：
  - 自治边界（Q9.a）：frontmatter/结构改动 → `save_content` dry_run 预览
    后自动落盘（origin=agent）；正文语义改动 → `propose_change` 出提案
    等人审；
  - 内容生成两条路：确定性编辑 ops（`append_section` / `replace_text` /
    `set_frontmatter_key`，骨架模式）或 `llm_cmd` 外挂生成器（stdin=上下文
    JSON，stdout=新全文；kaiwu 模型脚本的接入点）；
  - D7 本地预检（`FROZEN_KEYS`，exit 2）——服务端闸门是第二层。
- `agents/reviewer_agent.py`（ReviewerAgent，D8 三条的机器可查部分）：
  1. 数字可追溯——只对提案**新增行**提取数字，在 KB 语料（目标条目现文
     + README）与 rationale 中找出处，找不到标「未确认」；
  2. 被推翻表述显式化——删除含数字/结论词的行且新文无
     「已推翻/作废/supersede」标记 → 标疑似静默改写；
  3. 依赖矩阵一致性——经 MCP `validate` 工具核查（L8/L9 已机器化）。
  结论经 MCP `review_proposal` 落报告并翻提案 status（审阅也不直接碰盘）。
  提案本身是被索引的机制文件 → 经 `read_entry` 读取（Agent 只走 MCP）。
- `agents/README.md`：行为宪章（D7/D8 原文 + 三层机器化说明）、闭环图、用法。

### 4.2 rm 提案命令层 + MCP 第 10 个工具

- 新模块 `proposals.rs`（tauri-free，GUI 与 MCP 共用）：提案 meta 解析
  （fm_writer::scalar_value）、`list_proposals`（跳过 `.review.md`，新在前）、
  `read_proposal`（meta + 内容块 + 审阅报告）、`set_status`（fm_writer
  保序改写只动 status 键行）、`write_review`（写 `<id>.review.md` +
  翻 reviewed）；提案 id 白名单校验（杜绝路径逃逸）。
- `commands.rs` 四命令（impl 内核 + tauri 包装，lib.rs 注册）：
  `list_proposals` / `read_proposal`（附 target 解析结果与目标现文，供
  GUI 渲染 diff）/ `apply_proposal` / `dismiss_proposal`。
- **`apply_proposal_impl`**（闭环的「应用」步）：target 按 id → 路径后缀
  解析（多义拒绝）→ **D7 闸门**（`mcp::frozen_field_violations`，Agent
  起草的全文 bump 复审日期连人批准也拦——复审日期必须人在编辑器里自己
  bump）→ `write_entry_content(origin=Agent)`（原子替换 + 版本记录 +
  rebuild）→ 翻提案 status=applied。
- `mcp.rs`：`review_proposal` 工具（verdict 只接受 pass/needs-attention，
  强制 dryRun；dry_run 返回 currentStatus 预览）——「Agent 不直接碰盘」
  对审阅同样成立。`frozen_field_violations` / `AGENT_FROZEN_KEYS` 转 pub
  供 GUI 复用；`fm_writer` 增 `scalar_value`（mcp 私有实现改委托）。

### 4.3 GUI 提案审阅面板

- `panelStore` 增 `'proposals'` 面板型；`RightPanelStack` / `RightPanel`
  （`titleProposals`）接线；Sidebar 加「📋 语义提案」入口按钮。
- 新组件 `src/components/ProposalsPanel.tsx`：提案列表（status 徽标 +
  verdict、target、rationale）→ 详情（审阅报告 + 行级 diff + 批准/驳回）。
  应用后刷新列表与图谱（`loadEntries`）。
- `src/utils/lineDiff.ts` 追加 `lineDiff`（LCS 行级 diff，超长文件退化
  为前后缀对齐）；既有的 `changedLinePairs`（排版预览用）原样保留。
- i18n：新命名空间 `ProposalsPanel.{zh,en}.json`（resources.ts 注册）；
  `RightPanel` / `Sidebar` 命名空间对补键——命名空间成对纪律。
- `shared/types.ts`：`ProposalMeta` / `ProposalDetail` / `ApplyProposalResult`
  （与 Rust camelCase 序列化对齐）。

## 新增测试（跑绿后删除）

| 覆盖 | 结果 |
|---|---|
| proposals.rs：split_content 提取/未闭合、list/read/status 往返（.review.md 不混入列表）、非法 id 拒绝（遍历/斜杠/.review/不存在）、对不存在提案写审阅失败 | ✅ 已删 |
| apply_proposal_impl ×3：应用后全文落盘 + origin=agent 版本 + status 翻 applied + 四键原样；target 用条目 id 也可解析 + D7 拒绝后提案保持 pending；无内容块提案报错 | ✅ 已删 |
| mcp review_proposal 工具：非法 verdict 拒、缺 dryRun 拒、dryRun 只预览、落盘翻 reviewed、tools/list 计 10 | ✅ 已删 |

删除后 rm 套件 **312 passed / 0 failed**（与基线一致）；`cargo build` 与
`--no-default-features --lib` 均绿；`npm run build` 绿。

## 验收记录（kaiwu 副本，真实 KB 未动）

**正向闭环**（`edit_agent` → `reviewer_agent` → [apply 由 Rust 测试验证 +
GUI 步骤见下]）：

1. EditAgent 任务「补写验收口径小节（35 篇文档）」→ 正文改动自动转提案
   `20261117-HHMMSS-01-项目概览-01-项目目标与路线`，落 `_proposals/`
   （经 MCP propose_change，全程未直接写盘）；
2. ReviewerAgent `--all` → 经 MCP `review_proposal` 写出审阅报告：
   D8-1 仅新增行数字（10、35）✅ 可溯源；D8-2 无静默删除 ✅；
   D8-3 validate 0 error / W10/W11 静默 ✅；verdict=pass；提案 status
   → reviewed；
3. apply_proposal（Rust 测试路径）：提案全文经 write_entry_content 落盘、
   origin=agent 版本入历史、提案 status → applied。

**反向场景**：凭印象写「准确率 99.9%」→ D8-1「⚠️ 未确认：99.9」→
verdict=needs-attention（kaiwu「追不到的标未确认，不许按印象补写」落地）。
**D7 反向**：`set_frontmatter_key 最后更新=今天` → edit_agent 本地预检
拒绝（exit 2）。

**双校验器**：改动后副本 `synapse validate` 0E/0W（35 info 为副本 mtime
触发的 I1，与提案无关）、提案文件零 findings；`check_kb.py --strict`
0 错误 0 警告（`_proposals/` 豁免，批次 3 定的契约）。

## GUI 人审步骤（待用户在 tauri:dev 执行，批次 1 验收可顺路做）

1. `npm run tauri:dev`，打开目标 KB；
2. 侧栏「📋 语义提案」→ 列表点开 status=reviewed 的提案；
3. 看 diff（绿=新增 / 红=删除）与 ReviewerAgent 报告；
4. 「批准并应用」→ 条目更新、版本历史出现 origin=agent 记录、提案徽标
   变「已应用」；或「驳回」→ 徽标变「已驳回」（文件保留作审计痕迹）。

## 已知边界与遗留

1. ReviewerAgent 的 D8-1/D8-2 是**启发式**（数字字符串匹配 + 结论词表），
   语义级判断仍归人与未来的 LLM 接入（`llm_cmd` 外挂）；D8-3 是机器精确的。
2. 提案 content 块是**全文快照**：若审阅期间目标条目被人改过，apply 时
   会以提案全文覆盖（D7 冻结键仍拦得住复审日期）；「apply 前检测目标
   mtime/哈希变化并提示」留给批次 5 前的加固项。
3. GUI 面板的交互验收（第 4.4 节步骤）待用户执行。
4. 批次 1 的 GUI 验收（编辑 + 归档演练）仍待用户执行。

## 续接点（批次 5）

- dev-kb 接入为 MDSearchAgent：三 Agent 同协议（stdio MCP）互见——rm 的
  `synapse-mcp` 与 dev-kb 只读 MCP 协议一致（批次 3 对齐）；EditAgent/
  ReviewerAgent 的宿主登记配置（mcpServers 片段）随本批 `agents/README.md`
  给出用法，批次 5 落 dev-kb 侧的编排。
