# 批次 5 —— dev-kb 接入为 MDSearchAgent（三 Agent 同协议互见）

> 状态：**✅ 已完成**（2026-11-17）。计划见 [../merge-kaiwu-plan.md](../merge-kaiwu-plan.md) §3 批次 5。
> 验收：三 Agent 同协议（stdio MCP，2025-06-18）互见；dev-kb 自身 CI
> （契约门禁 / 单元与集成测试 / kb-spec 比对）本地等价执行全绿——不受影响。
> **dev-kb 仓库本批零改动**（ADR-0030 只读边界维持，projects.yaml 批次前已登记）。

## 修改清单（全部在 rm，单仓提交）

### 5.1 mcp_client 泛化（互见的协议底座）

- `agents/mcp_client.py`：拆出通用基类 `McpClient`（任意 server 启动命令
  argv + 可选 cwd/env）——协议层（initialize 协商 / tools/list / tools/call /
  ping / 生命周期）全部在基类；`SynapseMcpClient` 只留 `--kb` 构造与
  KB 便捷方法（find_entry / read_entry）。记录协商结果
  `protocol_version`。dev-kb 服务需要的 `PYTHONPATH=src` 经 env 注入。

### 5.2 `agents/mdsearch.py` —— MDSearchAgent 桥接

- dev-kb 只读检索的客户端封装：`index_status`（快路径）/ `search` /
  `read_source`（ADR-0030 证据复核）。
- **search 带超时降级闸**（默认 120s，可 `--timeout` / Agent 参数覆盖）：
  dev-kb 侧首次检索要加载 bge-reranker-v2-m3（`HF_HUB_OFFLINE=1`，本机
  无 HF 缓存 → 长时间阻塞）；超时即判「检索不可用」，**重建会话**
  （search 的响应可能迟到，连接不可复用）并抛 ToolError，调用方优雅降级。
- `unavailable_reason` 记忆失败原因，重复调用快速失败。

### 5.3 两个 Agent 接 `--mdsearch`

- `edit_agent`：起草前检索任务相关文档（query = `search_query` 或
  instruction）；命中进 `llm_cmd` 上下文（`related` 字段，外挂生成器的
  读侧输入）；确定性骨架模式下命中路径进 stderr 报告与最终输出的
  `relatedDocs`（溯源信息给人，不污染提案内容）。检索不可用 → related
  为空，流程继续。
- `reviewer_agent`：D8-1 的 KB 语料追不到的数字，经 MDSearchAgent 二次
  检索（一次 search + ≤5 次 read_source），命中原文并入溯源语料再判——
  找到补溯源出处，仍找不到才标「未确认」。报告注明出处是否经检索补齐。

### 5.4 宿主登记示例 + README

- `agents/mcp.example.json`：两组 MCP server 的 mcpServers 片段
  （synapse-kb 读写 / dev-kb-mdsearch 只读）+ 形态说明（EditAgent /
  ReviewerAgent 是编排方按闭环调用的 CLI 进程，不是 server）。
- `agents/README.md` 增「三 Agent 同协议互见」节：形态表（谁在协议哪端）、
  用法命令、环境遗留说明；文件表补 mdsearch.py。

## 验收记录

**互见冒烟（kaiwu 副本，13 项断言全 PASS）**：

1. MDSearchAgent 可见：协议协商 `2025-06-18`、serverInfo=dev-kb、
   登记含 kaiwu 与 repository-manage、kaiwu 索引 80 文件 / 923 chunks、
   `read_only=true`（ADR-0030 边界）；
2. synapse-mcp 可见：同协议协商 2025-06-18、10 工具；
3. 降级闸：`--timeout 15` 下 search 给出结构化错误（超时原因明确），
   edit/reviewer 带 `--mdsearch` 仍完成全流程；
4. 协作闭环：EditAgent 出提案（语义改动 → propose_change）→ 提案经 MCP
   可读（机制文件被索引）→ ReviewerAgent verdict=pass（新增数字 80/923
   均溯源）；
5. 副本 `synapse validate` 0E/0W。

**dev-kb 自身 CI 本地等价执行（三 job 全绿，不受影响）**：

| CI job | 本地等价 | 结果 |
|---|---|---|
| 契约门禁 | `DEV_KB_PYTHON=python3 sh scripts/contract_check.sh --config projects.ci.yaml` | ✓ error 0 |
| 单元与集成测试 | `.venv/bin/python -m pytest -q`（等价 CI 装依赖步骤） | ✓ 115 passed |
| kb-spec 双实现一致性 | `fixtures/kb-spec/compare.py --rm-repo ~/repository-manage`（frozen） | ✓ PASS |

**dev-kb 仓库零改动**：`git status` 干净——ADR-0030 只读边界维持，
projects.yaml 在批次前已登记 kaiwu 与 repository-manage。

## 已知边界与遗留

1. **search 功能路径待模型缓存**（环境遗留，与批次 0.5 的 kaiwu 模型目录
   同类）：`bge-reranker-v2-m3` 未缓存 + `HF_HUB_OFFLINE=1` → search
   阻塞，降级闸已验证。补齐模型缓存（或去掉 OFFLINE 让其下载）后检索
   路径自动恢复，代码无需改动。索引本身完好（923 chunks，pending_embedding=0）。
2. MDSearchAgent 的溯源语料只扩到 top-5 命中（一次 search + 5 次
   read_source 的成本边界）；更深的回溯留给 LLM 接入后的智能迭代。
3. merge-kaiwu-plan 的全部五批次至此完成。总验收（DoD 4.1）的 5 条：
   ①能看（批次 0/1，GUI 验收待用户）②能改不破（批次 1/2）③能验
   （批次 2，kaiwu 全链路回归待模型恢复）④能托（批次 3/4）⑤不越界
   （批次 5 本批验证）——两条 GUI/模型类尾巴已列入挂起事项。

## 续接点（无——计划内批次全部完成）

计划外可做的方向（未排期）：
- LLM 接入：edit_agent 的 `llm_cmd` 外挂（kaiwu 模型）+ reviewer 的语义级
  D8-1/D8-2 判断；
- 提案 apply 前的目标漂移检测（batch-4.md 遗留 #2）；
- 跨进程写锁收口（batch-3.md 遗留 #1）；
- GUI 内的 MDSearchAgent 检索入口（当前只经 CLI/宿主）。
