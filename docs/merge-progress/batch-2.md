# 批次 2 —— 规范正文归位 + 双实现一致性 CI

> 状态：**✅ 已完成**（2026-11-17）。计划见 [../merge-kaiwu-plan.md](../merge-kaiwu-plan.md) §3 批次 2。
> 验收：dev-kb CI 双实现 fixture 比对绿（30 份 fixture 上判据等价）；基线更新必须附理由。

## 修改清单

### 2.1 规范正文归位（F13 → 版本控制）

- 新增 rm `docs/kb-spec.md`：原文自 `~/Documents/Trae-CN/temp_20261103_01/kb-general-spec.md`
  （236 行，无版本控制）迁入，追加：
  - `文档代号` 第 5 键（Q12，M5 条件规则：有键查格式 NN-MM，缺键沉默）；
  - L-7 stem 唯一警告（Q17）、L-8/L-9 依赖矩阵一致性（D8-3，条件规则）；
  - W6 复审过期 + I3 让位（D4）、W7 状态枚举（D3 的校验器部分，批次 1 只做了 UI）；
  - **附录 A** 双实现映射表（规范 S/M/H/L/T ↔ kaiwu 30 条 ↔ rm E/W/I，判据等价
    与级别语义分开记录）；**附录 B** 盲区声明汇编（D9）+ rm 死规则清理声明。
- 合计规则：原 26 条 + 批次 2 新增 4 条（M5/L7/L8/L9）= **30 条**。

### 2.2 双实现同步新规则 + rm 死规则清理

**kaiwu `scripts/check_kb.py`**（+4 条，26→30）：
- `M5`（错误级，条件）：frontmatter `文档代号` 存在时必须匹配 `^\d{1,4}-\d{1,4}$`；
- `L7`（警告）：同项目 stem 重复（README 豁免）；
- `L8`（警告，条件）：README `## 依赖矩阵` 存在且表头合法时，「依赖」列 ⊆ 该文档
  `## 相关文档` 链接目标的代号（新 helper `doc_code_of`：frontmatter 优先、路径派生兜底）；
- `L9`（警告，条件）：依赖/被依赖互惠。
- kaiwu 30 条规则下保持 0 findings（全部条件规则在 kaiwu 数据上静默）✓。

**rm Rust validator**（+6 条新 ID + 死规则删除）：
- `rules.rs` 新增 `W6`（复审过期，ISO 日期字符串比较 + civil 算法取今天，
  `SYNAPSE_TODAY` 环境变量可钉日期——fixture 基线确定性所需）、
  `W7`（状态枚举，括号注记剥离后比对 canonical 六枚举）、
  `W8`（文档代号格式）；三者都是条件规则（键不存在即沉默）。
- 新模块 `validator/kb_rules.rs`：`W9`（stem 唯一）、`W10/W11`（依赖矩阵 ⊆ / 互惠，
  含围栏剥除、小节定位、表头列解析、相关文档目标代号解析——与 kaiwu 同构）。
  `validate_kb` 挂载 KB 级检查。
- `system_labels.rs`：I3 在条目声明 `下次 review` 时让位（D4）。
- **死规则删除（D9）**：`check_yaml_parse`（空转）、`check_unused_attachments`（桩）
  移除；E8 编号缺位在附录 B 声明（编号不回收）。

### 2.3 fixtures + 双黄金基线（Q15.a）

- dev-kb `fixtures/kb-spec/`：
  - `kaiwu-snapshot/`：kaiwu KB 36 文件冻结快照（真实数据回归面）；
  - `synthetic/`：6 个项目 27 文件，覆盖 ~20 个边界样例——M3a/M4/M5 触发与静默
    （过期/未来/按需、枚举合法带注记/非法/缺键、代号合法/非法/缺键）、stem 重复/唯一、
    依赖矩阵一致/越界+不互惠/无小节/坏表头、legacy 注释元数据、wiki 语法、E4 门控、孤儿；
  - `frozen_check_kb.py`：kaiwu 校验器的冻结副本（kaiwu 仓库无 git 远端（F12），
    CI 拉不到；副本与快照同进退，更新需附理由）；
  - `mapping.json`：判据等价对（M3a↔W7、M4↔W6、M5↔W8、L7↔W9、L8↔W10、L9↔W11）
    + 钉住的日期 `2026-11-17`；
  - `compare.py`：比对器——逐项目跑双实现（check_kb 的「今天」被 monkeypatch 钉住、
    synapse 走 `SYNAPSE_TODAY`），断言 shared_rules 两侧文件集合一致；与双黄金基线
    （`baselines/{kaiwu-snapshot,synthetic}.json`）比对；`--update-baseline` 必须
    `--reason`（基线冻结纪律）。支持 `--kaiwu-repo` 实时模式（本地用）。
- 首跑结果：**0 等价违反**；快照 30 条规则下双侧 0 findings；合成样例六对规则逐文件一致。

### 2.4 dev-kb CI（Q8）

- `.github/workflows/ci.yml` 新增 `kb-spec-consistency` job：checkout dev-kb + rm
  （kaiwu 无远端，用冻结副本），setup-python + rust stable，`cargo build --bin synapse`，
  跑 compare.py。原 `contract` / `tests` job 不动（dev-kb 自身契约门禁不受影响，验收 4.1-5）。

## 新增测试（跑绿后删除）

| 侧 | 覆盖 | 结果 |
|---|---|---|
| Rust rules ×6 | W6 过期/未来/按需/缺键；W7 注记剥离/非法/缺键；W8 合法/非法/缺键 | ✅ 已删 |
| Rust kb_rules ×5 | 围栏剥除与小节定位、W9 重名、W10/W11 一致静默+破坏触发、无小节/坏表头静默 | ✅ 已删 |
| Python（脚本断言） | M5/L7/L8/L9 全部触发/静默路径 + kaiwu 0 findings | ✅（一次性脚本） |
| fixture 等价 | 六对规则 × 全部样例 + 快照 | ✅ 常驻（compare.py + 基线即测试） |

删除后 rm 套件 312 passed / 0 failed；`synapse validate kb/kaiwu`（含新规则）保持 0/0。

## 验收记录

- **dev-kb CI 比对（本机等价执行）**：`compare.py` frozen 模式与 live 模式均
  `PASS：双实现判据等价，输出与黄金基线一致`（rc=0）。30 份 fixture（36 快照文件 +
  27 合成文件）上两实现给出映射等价结论。
- 基线带 `_reason`（「批次 2 初始基线：双实现首跑冻结（pinned 2026-11-17）」+
  修正快照定位后的重冻结理由），符合基线冻结纪律。
- GitHub Actions 实际运行待 push 后观察（job 依赖 rm 仓库 `main` 上的 synapse 构建）。

## 遗留与续接点

1. kaiwu 35 篇尚无 `文档代号` 键（M5 缺键静默）；批次 3 的 Agent 行为规范可定义
   「编辑时建议补键」。
2. `frozen_check_kb.py` 与 kaiwu `scripts/check_kb.py` 的同步是人工动作：
   kaiwu 校验器变更时复制 + `--update-baseline --reason` 重跑。
3. CI 的 rm 侧 checkout 用 `zmc576700954/repository-manage@main`——rm 的批次改动
   必须先合入 main，否则 CI 测的是旧校验器。
4. 批次 3 入口：stdio MCP server（三层工具面，Q14；D7 行为规范写进 Agent 宪章）。
