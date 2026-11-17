"""MDSearchAgent 桥接（批次 5）：dev-kb 只读检索的客户端视图。

dev-kb 的 MCP stdio 服务（`agent_app/mcp_server.py`）与 rm 的
`synapse-mcp` 同协议（换行分隔 JSON-RPC 2.0，协议版本集一致）——
三 Agent 互见的「只读检索」一侧。本模块只加一个薄封装：

- `index_status()`：项目登记与索引统计（不加载检索引擎，快路径）；
- `search(query, project, limit)`：混合检索。**带超时**——dev-kb 侧
  首次检索要加载 bge-reranker/embedding 模型，模型缺失时会长时间阻塞；
  超时即判「检索不可用」，重建会话并抛 ToolError，调用方优雅降级；
- `read_source(project, path)`：按行回读原文（ADR-0030 的证据复核契约）。

dev-kb 仓库零改动（ADR-0030 只读边界维持）；启动命令可经 `--devkb-repo`
指定，缺省找 `~/dev-kb`。

用法：
    python3 agents/mdsearch.py --status                # 快路径
    python3 agents/mdsearch.py --query "文档代号" --project kaiwu
"""

from __future__ import annotations

import argparse
import json
import sys
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from mcp_client import McpClient, McpError, ToolError  # noqa: E402

DEFAULT_DEVKB = Path.home() / "dev-kb"
DEFAULT_SEARCH_TIMEOUT_S = 120


class MdSearchAgent:
    """连 dev-kb 只读 MCP 服务的会话。上下文退出后子进程结束。"""

    def __init__(
        self,
        devkb_repo: Path = DEFAULT_DEVKB,
        search_timeout_s: int = DEFAULT_SEARCH_TIMEOUT_S,
    ) -> None:
        self.repo = devkb_repo
        self.search_timeout_s = search_timeout_s
        self.unavailable_reason: str | None = None
        self._client = self._connect()

    def _connect(self) -> McpClient:
        venv_python = self.repo / ".venv" / "bin" / "python"
        python = str(venv_python) if venv_python.exists() else sys.executable
        return McpClient(
            [python, "-m", "agent_app.mcp_server"],
            cwd=self.repo,
            # dev-kb 的 agent_app 包在 <repo>/src 下：服务端约定
            # PYTHONPATH=src 启动（见 agent_app/mcp_server.py 模块注释）。
            env={"PYTHONPATH": "src"},
        )

    @property
    def protocol_version(self) -> str | None:
        return self._client.protocol_version

    @property
    def server_info(self) -> dict:
        return self._client.server_info

    def index_status(self) -> dict:
        """项目登记与索引统计（快路径，不加载检索引擎）。"""
        return self._client.call("index_status")

    def search(self, query: str, project: str | None = None, limit: int = 5) -> list[dict]:
        """检索；带超时保护。超时后连接已失同步（响应可能迟到），重建会话。

        Raises:
            ToolError: 检索超时（引擎冷启动 / rerank 模型缺失）或 dev-kb 侧
                返回非 ok 状态。调用方应捕获并降级。
        """
        if self.unavailable_reason:
            raise ToolError(f"search 不可用（先前已失败）: {self.unavailable_reason}")
        holder: dict = {}

        def run() -> None:
            try:
                holder["result"] = self._client.call(
                    "search", query=query, project=project, limit=limit
                )
            except Exception as exc:  # noqa: BLE001 - 线程内统一回传
                holder["error"] = exc

        worker = threading.Thread(target=run, daemon=True)
        worker.start()
        worker.join(self.search_timeout_s)
        if worker.is_alive():
            # 超时：search 的响应可能仍在路上，连接不可复用——重建。
            self.unavailable_reason = (
                f"search 超时（>{self.search_timeout_s}s）：检索引擎冷启动或"
                "rerank/embedding 模型未缓存"
            )
            try:
                self._client.close()
            except Exception:
                pass
            self._client = self._connect()
            raise ToolError(self.unavailable_reason)
        if "error" in holder:
            raise holder["error"]
        result = holder["result"]
        if result.get("status") != "ok":
            raise ToolError(f"search: {result}")
        return result.get("results", [])

    def read_source(self, project: str, path: str, start_line: int = 1, end_line: int | None = None) -> dict:
        """按行回读项目原文件（证据复核，ADR-0030）。"""
        kwargs: dict = {"project": project, "path": path, "start_line": start_line}
        if end_line is not None:
            kwargs["end_line"] = end_line
        return self._client.call("read_source", **kwargs)

    def close(self) -> None:
        self._client.close()

    def __enter__(self) -> "MdSearchAgent":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


def main() -> int:
    parser = argparse.ArgumentParser(description="MDSearchAgent 只读检索 CLI")
    parser.add_argument("--devkb-repo", type=Path, default=DEFAULT_DEVKB)
    parser.add_argument("--query", default=None)
    parser.add_argument("--project", default=None)
    parser.add_argument("--limit", type=int, default=5)
    parser.add_argument("--timeout", type=int, default=DEFAULT_SEARCH_TIMEOUT_S)
    parser.add_argument("--status", action="store_true", help="只看登记与索引统计")
    args = parser.parse_args()

    with MdSearchAgent(args.devkb_repo, search_timeout_s=args.timeout) as agent:
        print(json.dumps({
            "protocol": agent.protocol_version,
            "server": agent.server_info,
        }, ensure_ascii=False))
        if args.status:
            print(json.dumps(agent.index_status(), ensure_ascii=False))
            return 0
        if not args.query:
            parser.error("需要 --query 或 --status")
        try:
            hits = agent.search(args.query, project=args.project, limit=args.limit)
        except (ToolError, McpError) as exc:
            print(json.dumps({"search_error": str(exc)}, ensure_ascii=False))
            return 1
        for hit in hits:
            print(json.dumps({
                "path": hit["path"], "project": hit["project"],
                "score": hit.get("score"), "matched_by": hit.get("matched_by"),
            }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
