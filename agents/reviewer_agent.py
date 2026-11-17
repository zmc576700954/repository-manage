"""ReviewerAgent（批次 4）：按 D8 行为宪章审阅语义提案。

三条宪章（源自 kaiwu 口径纪律）机器可查的部分在这里落地：
1. **数字可追溯**：提案正文里的每个数字都要能在 KB 语料（目标条目、
   README、相关文档）或提案 rationale 里找到出处；找不到的标「未确认」，
   不许按印象补写。
2. **被推翻表述显式化**：提案删除了带数字/结论词的行、但新文里没有
   「已推翻 / 作废 / supersede」字样时，标记为疑似静默改写。
3. **依赖矩阵一致性**：经 MCP `validate` 工具跑 L8/L9（rm 侧 W10/W11），
   有 findings 即 needs-attention。

结论经 MCP `review_proposal` 写入 `_proposals/<id>.review.md` 并把提案
status 翻为 reviewed——审阅同样不直接碰盘。

用法：
  python3 agents/reviewer_agent.py --kb <path> [--proposal <id>] [--all]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from mcp_client import SynapseMcpClient  # noqa: E402

CONTENT_START = "<!-- synapse:proposal-content-start -->"
CONTENT_END = "<!-- synapse:proposal-content-end -->"

NUMBER_RE = re.compile(r"\d+(?:\.\d+)?")
# 结论性行的启发式：带数字，或带强断言词。
CONCLUSION_HINTS = ("准确率", "结论", "表明", "达到", "提升", "下降", "最优", "指标", "胜过", "劣于")
SUPERSEDED_MARKERS = ("已推翻", "作废", "supersede", "已被替代", "不再成立")


def extract_proposed(raw: str) -> str | None:
    if CONTENT_START not in raw:
        return None
    body = raw.split(CONTENT_START, 1)[1]
    return body.split(CONTENT_END, 1)[0].lstrip("\n") if CONTENT_END in body else None


def number_traceability(
    proposed_body: str, rationale: str, corpus: dict[str, str]
) -> tuple[list[str], list[str]]:
    """返回 (已溯源说明, 未确认数字)。数字在语料任一文件出现即视为可追溯。"""
    confirmed: list[str] = []
    unconfirmed: list[str] = []
    haystack_all = "\n".join(corpus.values()) + "\n" + rationale
    for num in sorted(set(NUMBER_RE.findall(proposed_body)), key=float):
        # 溯源到具体文件（取第一个命中的，报告里列全量命中太吵）。
        sources = [f"{eid}（现文）" for eid, text in corpus.items() if num in text]
        if sources or num in rationale:
            where = sources[0] if sources else "提案 rationale"
            confirmed.append(f"{num} ← {where}")
        else:
            unconfirmed.append(num)
    return confirmed, unconfirmed


def overturned_expressions(current: str, proposed: str) -> list[str]:
    """被删的结论性行 + 新文无推翻标记 → 疑似静默改写清单。"""
    if any(marker in proposed for marker in SUPERSEDED_MARKERS):
        return []
    removed = set(current.split("\n")) - set(proposed.split("\n"))
    return [
        line.strip()
        for line in removed
        if line.strip()
        and (NUMBER_RE.search(line) or any(h in line for h in CONCLUSION_HINTS))
    ]


def review_proposal(
    client: SynapseMcpClient,
    proposal_id: str,
    mdsearch_agent: "MdSearchAgent | None" = None,
    mdsearch_project: str | None = None,
) -> dict:
    # 提案是机制目录里的被索引文件 → 经 read_entry 读（Agent 只走 MCP）。
    detail = client.call("read_entry", id=proposal_id)
    raw = detail["content"]
    proposed = extract_proposed(raw)
    fm = raw.split(CONTENT_START, 1)[0]
    target = _scalar(fm, "target") or ""
    rationale = _scalar(fm, "rationale") or ""

    entry = client.find_entry(target)
    _, current = client.read_entry(target)

    corpus: dict[str, str] = {entry["id"]: current}
    # README（若有）也是溯源语料：读 KB 里含 README 的条目或退化为空。
    entries = client.call("list_entries")["entries"]
    for e in entries:
        if "readme" in e["id"].lower() and e["id"] not in corpus:
            _, readme = client.read_entry(e["id"])
            corpus[e["id"]] = readme
            break

    report_lines: list[str] = [f"# 审阅报告：{proposal_id}", ""]
    needs_attention = False

    # D8-1 数字可追溯（只查新增行——全文里既有的数字不是本次新引入的断言）
    if proposed is not None:
        body_without_fm = proposed.split("---\n", 2)[-1] if proposed.startswith("---") else proposed
        added_lines = set(body_without_fm.split("\n")) - set(current.split("\n"))
        added_text = "\n".join(sorted(added_lines))
        report_lines.append("## D8-1 数字可追溯（仅新增行）")
        if not added_text.strip():
            report_lines.append("- 无新增行（纯删改提案），数字追溯不适用")
        else:
            confirmed, unconfirmed = number_traceability(added_text, rationale, corpus)
            # 批次 5：MDSearchAgent 二次检索——KB 语料追不到的数字，在
            # dev-kb 索引的全库（含 kaiwu 的 scripts/evals 等工程侧语料）
            # 里再找一次出处；找到则补溯源，仍找不到才标「未确认」。
            if unconfirmed and mdsearch_agent is not None:
                try:
                    hits = mdsearch_agent.search(rationale or added_text[:120], project=mdsearch_project, limit=5)
                    for h in hits:
                        try:
                            src = mdsearch_agent.read_source(h["project"], h["path"])
                            text = src.get("content") or src.get("text") or json.dumps(src, ensure_ascii=False)
                            corpus[f"MDSearch:{h['project']}/{h['path']}"] = str(text)
                        except Exception:
                            continue
                except Exception as exc:
                    report_lines.append(f"- （MDSearchAgent 检索不可用：{exc}）")
                confirmed2, unconfirmed = number_traceability(added_text, rationale, corpus)
                confirmed = confirmed + [
                    c for c in confirmed2 if c not in confirmed
                ]
                if confirmed2 and not unconfirmed:
                    report_lines.append("- （部分出处由 MDSearchAgent 检索补齐）")
            for c in confirmed:
                report_lines.append(f"- ✅ {c}")
            for n in unconfirmed:
                report_lines.append(f"- ⚠️ 未确认：{n}（KB 语料与 rationale 中未找到出处）")
            if unconfirmed:
                needs_attention = True
        report_lines.append("")

        # D8-2 被推翻表述显式化
        suspects = overturned_expressions(current, proposed)
        report_lines.append("## D8-2 被推翻表述显式化")
        if suspects:
            needs_attention = True
            for s in suspects[:10]:
                report_lines.append(f"- ⚠️ 疑似静默删除结论行：「{s[:60]}」——需在文中显式列出已推翻表述或走 supersedes/归档")
        else:
            report_lines.append("- ✅ 无静默删除结论行的迹象")
        report_lines.append("")

    # D8-3 依赖矩阵一致性（validate 的 W10/W11 + 全部 error）
    validation = client.call("validate")
    findings = validation.get("issues", [])
    errors = [i for i in findings if i.get("severity") == "error"]
    matrix = [i for i in findings if i.get("rule_id") in ("W10", "W11", "L8", "L9")]
    report_lines.append("## D8-3 依赖矩阵一致性")
    if errors or matrix:
        needs_attention = True
        for i in (errors + matrix)[:10]:
            report_lines.append(f"- ⚠️ [{i['rule_id']}] {i['message'][:80]}")
    else:
        report_lines.append("- ✅ validate 0 error，W10/W11 静默")

    verdict = "needs-attention" if needs_attention else "pass"
    report_lines.insert(1, f"verdict: {verdict}")
    report = "\n".join(report_lines) + "\n"

    result = client.call(
        "review_proposal",
        proposalId=proposal_id,
        verdict=verdict,
        notes=report,
        dryRun=False,
    )
    return {"proposalId": proposal_id, "verdict": verdict, "reviewPath": result["reviewPath"]}


def _scalar(fm: str, key: str) -> str | None:
    for line in fm.split("\n"):
        if line.startswith(f"{key}:"):
            return line.split(":", 1)[1].strip()
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="ReviewerAgent：按 D8 审阅提案")
    parser.add_argument("--kb", required=True, type=Path)
    parser.add_argument("--proposal", default=None, help="单个提案 id；缺省配合 --all")
    parser.add_argument("--all", action="store_true", help="审阅全部 pending 提案")
    parser.add_argument("--bin", type=Path, default=None)
    parser.add_argument(
        "--mdsearch", action="store_true",
        help="KB 语料追不到的数字，经 MDSearchAgent（dev-kb 只读检索）二次溯源",
    )
    parser.add_argument(
        "--mdsearch-project", default=None,
        help="dev-kb 侧的项目名（如 kaiwu / repository-manage）",
    )
    parser.add_argument("--devkb-repo", type=Path, default=None)
    parser.add_argument(
        "--mdsearch-timeout", type=int, default=None,
        help="search 超时秒数（检索引擎冷启动/模型缺失时的降级闸）",
    )
    args = parser.parse_args()
    if not args.all and not args.proposal:
        parser.error("需要 --proposal <id> 或 --all")

    kwargs = {"bin_path": args.bin} if args.bin else {}
    mdsearch_agent = None
    if args.mdsearch:
        from mdsearch import MdSearchAgent, DEFAULT_DEVKB
        mdsearch_kwargs = {"search_timeout_s": args.mdsearch_timeout} if args.mdsearch_timeout else {}
        mdsearch_agent = MdSearchAgent(args.devkb_repo or DEFAULT_DEVKB, **mdsearch_kwargs)
    try:
        with SynapseMcpClient(args.kb, **kwargs) as client:
            if args.proposal:
                targets = [args.proposal]
            else:
                # pending 提案 = 索引里 _proposals/ 下的条目（机制目录被索引但仍豁免校验）。
                targets = [
                    e["id"]
                    for e in client.call("list_entries")["entries"]
                    if "_proposals/" in e["contentPath"].replace("\\", "/")
                    and _scalar(client.call("read_entry", id=e["id"])["content"], "status") == "pending"
                ]
            if not targets:
                print(json.dumps({"reviewed": [], "note": "没有待审提案"}, ensure_ascii=False))
                return 0
            results = [
                review_proposal(client, tid, mdsearch_agent, args.mdsearch_project)
                for tid in targets
            ]
    finally:
        if mdsearch_agent is not None:
            mdsearch_agent.close()
    print(json.dumps({"reviewed": results}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
