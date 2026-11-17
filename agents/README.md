# agents/ —— EditAgent / ReviewerAgent（批次 4）

Python 独立进程，经 stdio MCP（`synapse-mcp`，批次 3）访问 KB。
**全程不直接写盘**：读走 MCP read 工具，写走 MCP 结构写（强制 dryRun）
或语义提案（`propose_change`），审阅走 `review_proposal`。
rm 仓库不引入 LLM SDK（Q4.a）——内容生成走 `llm_cmd` 外挂命令
（kaiwu 的模型脚本可在此接入）。

## 行为宪章

**D7（写侧红线）**：Agent 编辑**不碰** `最后更新` / `下次 review`。
这是人工登记的复核日期，Agent 代改等于替人签到。三层机器化：
1. `edit_agent` 本地预检（`FROZEN_KEYS`，报错更友好）；
2. MCP `save_content` / GUI `apply_proposal` 服务端比对（`agent-frozen-field`）；
3. `patch_metadata` 参数结构里根本没有这两个键。
出处由版本历史 `origin=agent` 记录；复核后由人自行 bump。

**D8（ReviewerAgent 宪章，三条）**：
1. **数字可追溯**——每个数字必须能指到来源文件（KB 语料或提案
   rationale）；追不到的标「未确认」，不许按印象补写。
2. **被推翻表述显式化**——旧结论错误时不静默改写：列出「已推翻的
   表述」+ 替代文档，走 supersedes/归档流程；Agent 检测「删除结论行
   且无推翻标记」并标记。
3. **依赖矩阵一致性**——`依赖 ⊆ 相关文档` + 两列互惠，已由批次 2 的
   L8/L9（rm W10/W11）机器化，审阅时经 `validate` 工具核查。

机器只判形式，不判内容真实性——最终裁决是人（GUI 提案审阅面板）。

## 闭环（Q9.a 自治边界）

```
EditAgent ──结构改动──> save_content(dryRun=false)   # 自动落盘, origin=agent
   │
   └──正文语义改动──> propose_change                  # 落 _proposals/, status=pending
                          │
ReviewerAgent ──review_proposal──> <id>.review.md     # D8 报告, status=reviewed
                          │
人（GUI 📋 语义提案面板）──看 diff──> 批准(apply_proposal)
                          │                或 驳回(dismiss_proposal)
                          └──> write_entry_content(origin=agent) → status=applied
```

## 用法

```bash
# 先构建 MCP server
cd src-tauri && cargo build --bin synapse-mcp

# EditAgent：确定性编辑骨架（无 LLM）
python3 agents/edit_agent.py --kb ~/kaiwu/kb/kaiwu --task task.json

# EditAgent：外挂内容生成器（kaiwu 模型接入点）
# task.json 里加 "llm_cmd": ["python3", "/path/to/generate.py"]

# ReviewerAgent：审阅全部 pending 提案
python3 agents/reviewer_agent.py --kb ~/kaiwu/kb/kaiwu --all
```

task.json 的 `edit` 支持 `append_section` / `replace_text` /
`set_frontmatter_key` 三种确定性操作；正文语义改动自动转提案，
frontmatter/结构改动走 dry_run 预览后自动落盘。

## 文件

| 文件 | 职责 |
|---|---|
| `mcp_client.py` | stdio MCP 客户端（JSON-RPC 2.0 换行分隔，与 dev-kb 同协议；`McpClient` 通用基类可接任意同协议 server） |
| `mdsearch.py` | MDSearchAgent 桥接（批次 5）：dev-kb 只读检索的客户端封装，search 带超时降级闸 |
| `edit_agent.py` | EditAgent：结构写自动 / 语义改动出提案（`--mdsearch` 检索上下文） |
| `reviewer_agent.py` | ReviewerAgent：D8 三条审阅 + `review_proposal` 落报告（`--mdsearch` 二次溯源） |
| `mcp.example.json` | MCP 宿主登记示例（两组 server 的 mcpServers 片段） |

## 三 Agent 同协议互见（批次 5）

| Agent | 形态 | 协议端 |
|---|---|---|
| MDSearchAgent | dev-kb 只读 MCP server（`index_status` / `search` / `read_source`） | 服务端（`agents/mdsearch.py` 是它的客户端桥接） |
| EditAgent | rm CLI 进程 | 客户端（经 `SynapseMcpClient` → synapse-mcp 读写；可选 `--mdsearch` 连 dev-kb 检索） |
| ReviewerAgent | rm CLI 进程 | 客户端（同上 + `--mdsearch` 二次溯源） |

协议版本集三方一致（2024-11-05 / 2025-03-26 / 2025-06-18），协商结果
2025-06-18。宿主登记示例见 `mcp.example.json`。

```bash
# MDSearchAgent 快路径（登记与索引统计，不加载检索引擎）
python3 agents/mdsearch.py --status

# EditAgent 带检索起草（search 不可用时优雅降级：related 为空，流程继续）
python3 agents/edit_agent.py --kb <kb> --task task.json --mdsearch \
  --mdsearch-project kaiwu --mdsearch-timeout 60

# ReviewerAgent 二次溯源（KB 语料追不到的数字，在 dev-kb 索引里再找一次）
python3 agents/reviewer_agent.py --kb <kb> --all --mdsearch --mdsearch-project kaiwu
```

**已知环境遗留**：dev-kb `search` 首次调用需 bge-reranker-v2-m3 模型缓存
（`HF_HUB_OFFLINE=1`，缓存缺失时长时间阻塞）——`--mdsearch-timeout` 是
降级闸（超时即判检索不可用，MdSearchAgent 重建会话并抛 ToolError）；
补齐模型缓存后检索路径自动恢复，无需改代码。
