"""stdio MCP 客户端（批次 4；批次 5 泛化为任意同协议 server）。

按换行分隔 JSON-RPC 2.0 与 MCP server 子进程通信。本模块只依赖标准库
——rm 仓库不引入 LLM SDK（Q4.a），内容生成走外挂命令。

三 Agent 同协议互见（批次 5）的载体：[`McpClient`] 接任意 server 启动
命令——rm 的 `synapse-mcp`（读写）与 dev-kb 只读检索（`mdsearch.py`）
共用本协议层；协议版本集与 dev-kb 侧一致
（2024-11-05 / 2025-03-26 / 2025-06-18）。

契约要点（与 rm 硬约束 3 对齐）：
- Agent 的**一切** KB 访问都经本客户端（子进程 = MCP server）；
- 结构写工具必须显式传 dryRun（服务端强制，缺参即错）；
- 工具级错误走 result.isError（进程不退出）；协议错误抛 McpError。
"""

from __future__ import annotations

import json
import subprocess
import threading
from pathlib import Path

# agents/mcp_client.py → parents[1] = rm 仓库根 → src-tauri/target/debug。
DEFAULT_BIN = Path(__file__).resolve().parents[1] / "src-tauri" / "target" / "debug" / "synapse-mcp"

SUPPORTED_PROTOCOL_VERSIONS = {"2024-11-05", "2025-03-26", "2025-06-18"}
REQUESTED_PROTOCOL_VERSION = "2025-06-18"


class McpError(RuntimeError):
    """协议层错误（未知方法 / 进程退出等）。"""


class ToolError(RuntimeError):
    """工具级错误（服务端 isError=true，如 D7 冻结键拒绝、参数缺失）。"""

    def __init__(self, message: str) -> None:
        super().__init__(message)
        self.message = message


class McpClient:
    """通用 stdio MCP 客户端：`cmd` 是 server 的启动命令（argv）。
    一次会话一个实例；`close()` 或上下文退出后子进程结束。"""

    def __init__(
        self,
        cmd: list[str],
        cwd: Path | None = None,
        env: dict[str, str] | None = None,
    ) -> None:
        self.cmd = cmd
        import os

        child_env = None
        if env:
            child_env = dict(os.environ)
            child_env.update(env)
        self._proc = subprocess.Popen(
            [str(c) for c in cmd],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            encoding="utf-8",
            cwd=str(cwd) if cwd else None,
            env=child_env,
        )
        self._next_id = 0
        self._lock = threading.Lock()
        self.server_info: dict = {}
        self.protocol_version: str | None = None
        self.initialize()

    # -- 协议层 ---------------------------------------------------------------

    def _request(self, method: str, params: dict | None = None) -> dict:
        with self._lock:
            self._next_id += 1
            req: dict = {"jsonrpc": "2.0", "id": self._next_id, "method": method}
            if params is not None:
                req["params"] = params
            assert self._proc.stdin is not None and self._proc.stdout is not None
            self._proc.stdin.write(json.dumps(req, ensure_ascii=False) + "\n")
            self._proc.stdin.flush()
            line = self._proc.stdout.readline()
        if not line.strip():
            raise McpError("server closed the connection")
        resp = json.loads(line)
        if "error" in resp:
            raise McpError(f"{method}: {resp['error'].get('message', resp['error'])}")
        return resp.get("result", {})

    def initialize(self) -> dict:
        result = self._request(
            "initialize", {"protocolVersion": REQUESTED_PROTOCOL_VERSION}
        )
        if result.get("protocolVersion") not in SUPPORTED_PROTOCOL_VERSIONS:
            raise McpError(f"unsupported protocol: {result.get('protocolVersion')}")
        self.server_info = result.get("serverInfo", {})
        self.protocol_version = result.get("protocolVersion")
        return result

    def ping(self) -> bool:
        return self._request("ping") == {}

    # -- 工具层 ---------------------------------------------------------------

    def list_tools(self) -> list[dict]:
        return self._request("tools/list")["tools"]

    def call(self, name: str, **arguments) -> dict:
        """调用工具，返回 structuredContent；isError 时抛 ToolError。"""
        result = self._request(
            "tools/call", {"name": name, "arguments": arguments}
        )
        if result.get("isError"):
            text = "".join(
                c.get("text", "") for c in result.get("content", []) if c.get("type") == "text"
            )
            raise ToolError(text or "tool failed")
        return result.get("structuredContent", {})

    # -- 生命周期 -------------------------------------------------------------

    def close(self) -> None:
        try:
            assert self._proc.stdin is not None
            self._proc.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "exit"}) + "\n")
            self._proc.stdin.flush()
        except Exception:
            pass
        try:
            self._proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self._proc.kill()

    def __enter__(self) -> "McpClient":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


class SynapseMcpClient(McpClient):
    """rm 的 synapse-mcp（批次 3，读写）：`--kb` 绑定单一 KB。"""

    def __init__(self, kb_path: Path, bin_path: Path = DEFAULT_BIN) -> None:
        self.kb_path = kb_path
        super().__init__([str(bin_path), "--kb", str(kb_path)])

    # -- KB 便捷封装（EditAgent / ReviewerAgent 的全部 KB 访问都走这里） ------

    def find_entry(self, target: str) -> dict:
        """按 id 或相对路径后缀定位条目（list_entries 是全量元数据，很小）。"""
        entries = self.call("list_entries")["entries"]
        for entry in entries:
            if entry["id"] == target:
                return entry
        norm = target.replace("\\", "/")
        hits = [e for e in entries if e["contentPath"].replace("\\", "/").endswith(norm)]
        if len(hits) == 1:
            return hits[0]
        raise ToolError(f"target 不唯一或不存在: {target}")

    def read_entry(self, target: str) -> tuple[dict, str]:
        entry = self.find_entry(target)
        detail = self.call("read_entry", id=entry["id"])
        return entry, detail["content"]
