"""EditAgent（批次 4）：EditAgent 的执行体——Python 独立进程，经 stdio MCP
访问 KB，全程不直接写盘（rm 硬约束 3）。

自治边界（Q9.a）：frontmatter/结构改动自动落盘（dry_run 预览后 save_content）；
正文语义改动一律 `propose_change` 出提案，等人审（GUI 提案审阅面板）。

用法：
  python3 agents/edit_agent.py --kb ~/kaiwu/kb/kaiwu --task task.json

task.json:
  {
    "target": "<条目 id 或相对路径>",
    "instruction": "给 EditAgent 的自然语言任务（进提案 rationale）",
    "edit": {                       // 确定性编辑（无 LLM 时的骨架模式）
      "op": "append_section",       // append_section | replace_text | set_frontmatter_key
      "heading": "## 新小节",
      "body": "……"
    },
    "llm_cmd": ["python", "gen.py"] // 可选：外挂内容生成器，stdin=上下文 JSON，
  }                                // stdout=新全文；rm 不引入 LLM SDK（Q4.a）
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from mcp_client import SynapseMcpClient, ToolError  # noqa: E402

# D7（与 rm mcp::AGENT_FROZEN_KEYS 同源）：Agent 不许代 bump 人工复审日期。
FROZEN_KEYS = ("最后更新", "下次 review")


def frontmatter_block(content: str) -> tuple[str, str, str]:
    """切 (frontmatter 含围栏, 正文, 错误兜底)。无 frontmatter 时 fm 为空。"""
    lines = content.split("\n")
    if not lines or lines[0].strip() != "---":
        return "", content, ""
    try:
        end = next(i for i in range(1, len(lines)) if lines[i].strip() == "---")
    except StopIteration:
        return "", content, ""
    fm = "\n".join(lines[: end + 1]) + "\n"
    body = "\n".join(lines[end + 1 :])
    return fm, body, "---"


def apply_edit(content: str, edit: dict) -> str:
    op = edit.get("op")
    if op == "append_section":
        heading = edit["heading"].strip()
        body = edit["body"].rstrip()
        return content.rstrip("\n") + f"\n\n{heading}\n\n{body}\n"
    if op == "replace_text":
        find, replace = edit["find"], edit["replace"]
        if find not in content:
            raise SystemExit(f"replace_text: 找不到目标文本: {find[:60]!r}")
        return content.replace(find, replace, 1)
    if op == "set_frontmatter_key":
        key, value = edit["key"], edit["value"]
        fm, body, _ = frontmatter_block(content)
        lines = fm.split("\n") if fm else ["---"]
        # 已有键 → 就地替换；否则追加到关闭 --- 之前。
        out, inserted, closed = [], False, False
        for line in lines:
            if line.strip() == "---" and out and inserted is False and any(
                l.startswith(f"{key}:") for l in out
            ) is False:
                if closed is False:
                    out.append(f"{key}: {value}")
                    inserted = True
                closed = True
            if line.startswith(f"{key}:"):
                out.append(f"{key}: {value}")
                inserted = True
                continue
            out.append(line)
        return "\n".join(out) + body
    raise SystemExit(f"未知编辑 op: {op}")


def generate_content(task: dict, entry: dict, current: str) -> str:
    if "llm_cmd" in task:
        context = {
            "instruction": task.get("instruction", ""),
            "entry": {"id": entry["id"], "title": entry.get("title"), "path": entry.get("path")},
            "current_content": current,
            "charter": "数字必须可追溯；被推翻表述显式化；不改动 最后更新/下次 review。",
        }
        proc = subprocess.run(
            task["llm_cmd"], input=json.dumps(context, ensure_ascii=False),
            capture_output=True, text=True, check=True,
        )
        return proc.stdout
    if "edit" in task:
        return apply_edit(current, task["edit"])
    raise SystemExit("task.json 需要 edit（确定性编辑）或 llm_cmd（外挂生成器）")


def split_frontmatter(content: str) -> tuple[str, str]:
    lines = content.split("\n")
    if not lines or lines[0].strip() != "---":
        return "", content
    for i in range(1, len(lines)):
        if lines[i].strip() == "---":
            return "\n".join(lines[: i + 1]), "\n".join(lines[i + 1 :])
    return "", content


def main() -> int:
    parser = argparse.ArgumentParser(description="EditAgent：经 MCP 编辑 KB")
    parser.add_argument("--kb", required=True, type=Path)
    parser.add_argument("--task", required=True, type=Path)
    parser.add_argument(
        "--bin", type=Path, default=None, help="synapse-mcp 路径（缺省=构建目录）"
    )
    args = parser.parse_args()

    task = json.loads(args.task.read_text(encoding="utf-8"))
    kwargs = {"bin_path": args.bin} if args.bin else {}

    with SynapseMcpClient(args.kb, **kwargs) as client:
        entry, current = client.read_entry(task["target"])
        proposed = generate_content(task, entry, current)

        # D7 预检：冻结键改动直接拒绝（服务端也会拦，这里给出更友好的报错）。
        old_fm, _ = split_frontmatter(current)
        new_fm, _ = split_frontmatter(proposed)
        for key in FROZEN_KEYS:
            old_v = _scalar(old_fm, key)
            new_v = _scalar(new_fm, key)
            if old_v != new_v:
                print(f"[D7] 拒绝：{key} 是人工复审日期，Agent 不得改动", file=sys.stderr)
                return 2

        fm_changed = old_fm != new_fm
        body_changed = split_frontmatter(current)[1] != split_frontmatter(proposed)[1]
        instruction = task.get("instruction", "(无说明)")

        if body_changed:
            # 正文语义改动 → 提案等人审（Q9.a）。
            result = client.call(
                "propose_change",
                target=entry["id"],
                rationale=instruction,
                content=proposed,
            )
            print(json.dumps({
                "mode": "proposal",
                "proposalId": result["proposalId"],
                "path": result["path"],
                "next": "GUI 提案审阅面板（📋 语义提案）里看 diff 后批准/驳回",
            }, ensure_ascii=False))
        elif fm_changed:
            # 结构/元数据改动 → dry_run 预览后自动落盘（origin=agent）。
            preview = client.call(
                "save_content", id=entry["id"], content=proposed, dryRun=True
            )
            if preview["changed"]:
                applied = client.call(
                    "save_content", id=entry["id"], content=proposed, dryRun=False
                )
                print(json.dumps({
                    "mode": "structural",
                    "changed": True,
                    "versionOrigin": (applied.get("version") or {}).get("origin"),
                    "hints": applied.get("hints", {}),
                }, ensure_ascii=False))
            else:
                print(json.dumps({"mode": "structural", "changed": False}, ensure_ascii=False))
        else:
            print(json.dumps({"mode": "noop", "changed": False}, ensure_ascii=False))
    return 0


def _scalar(fm: str, key: str) -> str | None:
    for line in fm.split("\n"):
        if line.startswith(f"{key}:"):
            return line.split(":", 1)[1].strip()
    return None


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ToolError as exc:
        print(f"[tool-error] {exc.message}", file=sys.stderr)
        raise SystemExit(1)
