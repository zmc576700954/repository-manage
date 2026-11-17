# 批次 3 —— stdio MCP server

> 状态：**✅ 已完成**（2026-11-17）。计划见 [../merge-kaiwu-plan.md](../merge-kaiwu-plan.md) §3 批次 3。
> 验收：EditAgent 冒烟——经 MCP 给一篇文档补 frontmatter 键，全程有 dry_run
> 预览、有版本记录（origin=agent）、有 D7 冻结键拒绝；双校验器保持 0E/0W。

## 修改清单

### 3.1 纯函数层下沉（Q11：写路径脱 tauri 化）

- `commands.rs`：模块级 `#![cfg(feature = "tauri")]` 门控下沉到**每个
  `#[tauri::command]` 包装器**（41 处，cfg 必须位于属性宏之前）；`*_impl`
  纯函数层与私有 helper 改为无条件编译。`use tauri::State` 单独门控。
- `load_active_kb` / `clear_active_kb` / `swap_watcher` 只被 KB 切换
  command 使用（依赖 `create_watcher`），一并下沉到 tauri 门控——
  MCP server 绑定单一 KB（`--kb`），不参与注册表切换。
- `write_entry_content` 改 `pub`（MCP `save_content` 以 `SaveOrigin::Agent`
  复用同一条管线：原子写 + 本地写登记 + 版本 hook + rebuild）。
- `apply_fix` command 抽出 `apply_fix_impl(state, entry_id, rule_id, confirm)`
  可复用内核，command 变薄包装。
- `lib.rs`：`pub mod commands` 无条件编译。
- 顺带对齐（D3 收尾）：`update_entry_metadata_impl` 的 status 校验从英文
  四值扩展为「canonical 中文六枚举（剥 `（注记）` 比对，口径同 W7）∪
  英文旧值」——GUI 存量流程与 Agent canonical 值两条路都合法。

### 3.2 SaveOrigin::Agent（D7 出处载体）

- `history.rs`：`SaveOrigin` 增 `Agent` 变体（序列化 `"agent"`）。
  出处由版本历史承载，Agent 不代 bump `最后更新` / `下次 review`。
- 前端：`shared/types.ts` 的 `VersionOrigin` 加 `'agent'`；
  `VersionHistorySection.tsx` 的 ORIGIN_LABEL 加映射；
  `DetailPanel.{zh,en}.json` 加 `versionOriginAgent`（zh「Agent 写入」/
  en「Agent write」——命名空间成对纪律）。

### 3.3 mcp.rs —— stdio MCP server（新模块，~880 行）

- 协议：换行分隔 JSON-RPC 2.0 over stdio，与 dev-kb 只读 MCP 同协议
  （`initialize` 版本协商集合 `2024-11-05/2025-03-26/2025-06-18`、`ping`、
  `tools/list`、`tools/call`、`exit` 断连；通知不回应；单次工具异常不杀进程）。
- 进程模型：与 GUI 是两个进程，各自持**私有** SQLite 索引
  （`indexes/<kb_id>.agent.db`——索引是可重建缓存，不与 GUI 的 DB 争锁）；
  启动序同 GUI（旧运行时产物迁移 → rebuild → 校验缓存），均在
  startup_gate + write_lock 内。
- KB 解析（注册表**只读不写**）：`--kb <path>` 精确/规范化匹配注册表条目；
  未注册则按规范化路径派生稳定 id（`agent-<fnv1a64>`），历史与索引仍落
  app data 运行时目录；缺省用注册表活动 KB。
- **三层工具面（Q14，9 工具）**：
  - read 全开：`list_entries` / `read_entry` / `get_graph`（节点+边+计数）/
    `validate`（CLI 同款 JSON + summary）/ `get_history`（versions + warnings）
  - 结构写自动（**强制 `dryRun` 布尔参数**，缺参即报错）：
    `save_content`（整文件，`origin=agent`）/ `patch_metadata`（dry_run 与
    落盘共用同一渲染函数，字节级一致预览）/ `apply_fix`（dry_run 走 fix
    体系 confirm=false 分支；执行后补记 agent 版本）
  - 语义写走提案：`propose_change` → KB 内 `_proposals/`（提案 id =
    UTC 紧凑时间戳 + 安全化 target；frontmatter 含 proposal/target/
    status: pending/created/origin: agent/rationale；可选 content 用
    `synapse:proposal-content-start/end` 标记包裹，批次 4 按此切分应用）
- **D7 闸门**：`save_content` 逐值比对 frontmatter 的 `最后更新` /
  `下次 review`（行级标量提取），任何改动（含新增/删除键）直接拒绝，
  错误码 `agent-frozen-field`；dry_run 阶段同样拦截。
  `patch_metadata` 的参数结构不含这两个键（结构性保证）。
- 补键建议（batch-2 遗留 #1）：落盘响应带 `hints`，frontmatter 缺
  `文档代号` 时返回 `docCodeMissing: true`（建议值由 Agent 结合
  list_entries 的 dir 字段按 NN-MM 规则自行推导）。
- 跨进程写安全：复用 `*_impl` 的进程内 `write_lock` + `content_io` 原子
  替换；GUI watcher 会把 MCP 写视作 external 变更，但 agent 版本记录
  （毫秒级）先于 watcher 防抖（500ms）落盘，且 `record_external_change`
  按内容哈希去重——不会出现双版本。已知的残余竞态（GUI 用户与 Agent
  同毫秒改同一文件的读-改-写丢失）记录在案，等全部写方接入同一把
  跨进程锁时收口（非本批范围）。
- 新 bin `synapse-mcp`（`src/bin/synapse_mcp.rs` + Cargo.toml 注册）：
  clap `--kb`，current_thread tokio runtime。
- `metadata_updater` 抽 `preview_metadata_changes`（纯读预览）+ 私有
  `render_metadata_edits` 共用变换；`history::diff_summary` / 
  `index::derive_doc_code` 转 pub 供 MCP 复用；`error.rs` 增
  `AgentFrozenField`。

### 3.4 `_` 机制目录豁免（Q14，双实现 + 规范 + fixtures 同步）

- rm：`scanner::is_mechanism_path`（KB 根以下任一路径段 `_` 前缀）；
  `validator::validate_kb` 跳过机制条目的全部规则（但保留在 id 集合里
  参与链接解析——指向提案不算断链）；`index::rebuild` 对机制条目
  置空 `validation`（图谱可见、不出错误徽标）。
- kaiwu `scripts/check_kb.py`：`load_project` 加 `_is_mechanism` 过滤
  （docs 与 S 组的 files/dirs 收集面双过滤），与 rm 同口径。
- 规范 `docs/kb-spec.md`「豁免」节明确机制目录契约：豁免 S/M/H/L、
  仍索引展示、点名照查；变更记录补一行。
- dev-kb fixtures：新增 `synthetic/proj-mechanism`（提案文件故意不合规，
  双侧 0 findings、等价静默）；`frozen_check_kb.py` 同步 kaiwu 批次 3
  变更；双基线 `--update-baseline --reason` 重冻结（基线冻结纪律）。

## 新增测试（跑绿后删除）

| 覆盖 | 结果 |
|---|---|
| D7 冻结键：值变更/新增键拦截、dry_run 同拦、行级提取不误读嵌套键 | ✅ 已删 |
| save_content：缺 dryRun 报错、dry_run 磁盘不变、落盘 + origin=agent 版本 | ✅ 已删 |
| patch_metadata：dry_run 预览状态键方言路由、落盘 + agent 版本 + 四键原样 | ✅ 已删 |
| propose_change：文件形状、路径安全化（遍历/隐藏名/双扩展名）、提案豁免校验 | ✅ 已删 |
| 机制目录：索引含提案但 validate_kb 零 findings；scanner 对 `_`/`.` 前缀的不同处理 | ✅ 已删 |
| 协议层：initialize 版本协商回落、tools/list 9 工具且写工具强制 dryRun、未知方法 -32601、通知不回应 | ✅ 已删 |
| read 层形状 + entry-not-found | ✅ 已删 |
| UTC 时间戳 / 提案 stem 清洗 | ✅ 已删 |

删除后 rm 套件 **312 passed / 0 failed**（与基线一致）；`cargo build` 与
`cargo build --no-default-features --bin synapse-mcp` 均绿（后者是
tauri-free 构建，dead-code 警告为预期，见下）；`npm run build` 绿。

## 验收记录

**EditAgent 冒烟（通过 MCP 给一篇文档补 frontmatter 键，KB 用副本，
真实 kaiwu KB 未动）：** 驱动脚本模拟 MCP 客户端经 stdio 全流程，24 项断言
全部 PASS：

1. 握手（协议版本、serverInfo）+ tools/list 三层九工具、写工具强制 dryRun；
2. read 层：list_entries 35 条目、read_entry 正文、get_graph 35 节点 /100+ 边；
3. `save_content` 补 `文档代号: 01-01`（batch-2 遗留 #1 的建议动作）：
   dryRun 预览 changed + addedLines≥1 且磁盘未动 → 落盘 changed →
   版本记录 origin=agent → 磁盘字节确已更新；
4. **D7 反向**：bump `最后更新` → `agent-frozen-field` 拒绝；
5. `patch_metadata` 状态改 `需持续监控`：dryRun 预览路由到中文 `状态` 键 →
   落盘 + agent 版本，`最后更新`/`下次 review` 字节原样；
6. `get_history`：两条 agent 记录（+ 既有 editor/external 历史）；
7. `propose_change`：`_proposals/20261117-HHMMSS-…md`，frontmatter 完整；
8. `synapse validate` 副本 0 error / 0 warning、提案文件零 findings；
   `check_kb.py --strict` 同一副本 0 错误 0 警告（`_` 豁免双侧一致）。

dev-kb 比对：frozen 与 live 模式均 `PASS：双实现判据等价，输出与黄金基线
一致`（rc=0），新 fixture 纳入基线。

## 已知边界与遗留

1. **跨进程锁未收口**：MCP 进程内 `write_lock` + 原子替换已保单文件安全；
   GUI 用户与 Agent 并发改同一文件的读-改-写仍靠「后写覆盖」（罕见且
   不产生半文件）。全部写方（GUI command 也）接同一把文件锁才是完整解，
   改动面大（ touches 全部 GUI 写命令），留待需要时单独一批。
2. **`--no-default-features` 构建有 dead-code 警告**（command 包装器被
   cfg 剥离后，部分仅被 GUI 使用的 impl/helper 不可达）。生产路径
   （默认特性）无新增警告。MCP bin 发布构建走默认特性即可。
3. kaiwu 全链路回归 PASS 仍待模型目录恢复（批次 0.5 遗留，环境问题）；
   本批改动不影响其 `kb_errors: 0` 口径（check_kb 在 kaiwu KB 上 0/0）。
4. GUI 内的提案审阅视图（diff + 批准/驳回）归批次 4。
5. 批次 1 的 GUI 验收（编辑 + 归档演练）仍待用户执行（批次 1 遗留）。

## 续接点（批次 4）

- EditAgent / ReviewerAgent（Python 独立进程，rm `agents/`），行为宪章 D7/D8；
  提案文件格式已由本批定死（frontmatter + content 标记），GUI 审阅视图
  按 `synapse:proposal-content-start/end` 切分展示 diff。
- MCP server 以子进程方式被 Agent 宿主拉起：`synapse-mcp --kb <path>`。
