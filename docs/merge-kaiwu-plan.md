# kaiwu × rm 合并计划：设计全景与执行纲要

> 把 kaiwu 的知识库质量契约（26 条规范 + check_kb.py）合并进 repository-manage（下称 rm，Synapse KB），
> 使 rm 成为「支持知识库管理、且能自主完成文档内容优化的桌面工具」。
> dev-kb 保持独立项目，后续作为只读检索 Agent（MDSearchAgent）接入。
> 本文是 2026-11-16～17 设计推演的收拢：四轮 17 个决策点（Q1–Q17）+ 一轮补充思考（D1–D9）。
> 状态：**定稿**（Q1–Q17 与 D1–D9 已于 2026-11-17 全部确认）；**执行收官**
> ——批次 0～5 全部完成（见 [merge-progress/README.md](merge-progress/README.md)；
> 挂起尾巴与计划外方向见该文档 §3 与 batch-5.md §续接点）。
> 证据表见第 6 节，全部结论有实测或源码行号背书。

---

## 1. 为什么做这个任务

**三个项目的现状与关系：**

| 项目 | 本质 | 有什么 | 缺什么 |
|---|---|---|---|
| kaiwu | 个人 AI 实验工作区（M5 Pro / MLX 微调 / RAG 评测） | `kb/kaiwu/` 35 篇受 26 条规范约束的知识库 + `check_kb.py` 校验器 + 回归门禁 + 口径纪律 | UI、索引、图谱、改文档的能力 |
| rm（Synapse KB） | Tauri 2 桌面文档管理工具（~12k 行 Rust/TS） | 扫描/SQLite 索引/图谱/编辑/版本历史/校验+修复/多 KB 注册表/双语 | 「内容该不该优化」的判断与执行能力 |
| dev-kb | 只读检索问答 Agent（MDSearchAgent） | 扫描→切块→混合检索→引用式问答 + 只读 MCP + 接入契约 | 写侧（ADR-0030 已明确移出范围） |

**触发点：** 写 kaiwu 知识库时意识到——既然有文档管理工具，为什么还要单独维护一份知识库管理工具？两者在「把 Markdown 当机器可校验的结构化对象」这一层方向一致，能力互补：kaiwu 有质量契约与口径纪律，没有工具；rm 有完整工具链，没有契约与 Agent。

**合并的实质不是「两个工具并成一个」，而是三层拼装：**

```
kaiwu 的质量契约（规范 + 校验器）      →  合并进 rm
rm 的工具运行时（GUI/索引/写路径）     →  宿主，原地演化
写侧 Agent 层（Edit / Reviewer）      →  全新建，Python 独立进程
dev-kb 的接入（MDSearchAgent）        →  第四件事，独立仓库不变
```

**为什么不是另两个方案：** 不新建仓库（Q1.a：rm 完成度最高，扔掉等于重写）；不让 kaiwu 当宿主（它的 kb/ 只是一批内容加一个 500 行脚本）。dev-kb 2026-11-15 刚做过一次方向修正（删掉 123 条写侧规范，理由：对检索器行为约束为 0）——本计划不重蹈覆辙：契约归 rm，dev-kb 只接只读。

---

## 2. 设计共识（Q1–Q17 定案）

### 2.1 仓库分工

| 仓库 | 合并后角色 |
|---|---|
| repository-manage | **宿主，唯一长工具**：GUI + stdio MCP server + 规范正文（docs/）+ Rust validator（最终唯一实现）+ `agents/`（Python Agent，不新建仓库） |
| kaiwu | 内容 + 实验工作区：`kb/kaiwu/` 成为 rm 管理的 KB 实例；`scripts/`、`evals/`、`app/` 照旧；回归门禁过渡期用 check_kb.py，终点换 `synapse validate` |
| dev-kb | 独立项目：只读检索 + 接入契约不变（ADR-0030 维持）；新增「双实现 fixture 一致性 CI」职责（只读，不违背其定位） |

### 2.2 三条硬约束（每条有实测事故背书）

1. **KB 目录 = 纯文档。** rm 的运行时产物（`.synapse/` 版本历史、`.synapse-layout.json` 节点位置）全部迁到 app data 目录（与 `indexes/<kb_id>.db` 同层）。否则 rm 一编辑，check_kb 就报 S2/M1/H1/L3（实测 6 error / 8 warning）——两个校验器永远无法同时归零。（Q10.a）
2. **元数据只有一个写路径。** YAML frontmatter 是唯一真源；rm 新增 frontmatter 保序写入器（保留未知键、键序、首行位置）；`@synapse-*` HTML 注释降为只读兼容。（Q2.a/Q6.a）否则「E4 一键修复把注释插到 frontmatter 之前 → 35 篇全部打成 M1」的事故重演（已实测复现）。
3. **Agent 不直接碰盘。** 一切写经 rm 的 MCP 写工具：write_lock + 原子写 + 版本历史 + `origin=agent` 标记 + 强制 dry_run 参数。（Q5.a/Q14.a）rm 现有写路径是 Tauri IPC（34 个 command），外部 Python 进程物理不可达——stdio MCP 面是新增的。

### 2.3 决策一览（Q1–Q17）

| # | 决策 | 结果 |
|---|---|---|
| Q1 | 合并宿主 | (a) rm 原地演化，kb/kaiwu 成为其 KB 实例 |
| Q2 | 元数据真源 | (a) frontmatter 唯一真源，`@synapse-*` 只读兼容 |
| Q3 | 规则集收敛 | (a) 一份规则正文 + 短期双实现 + ID 映射，终点 Rust 单实现 |
| Q4 | Agent 运行时 | (a) Python 独立进程，stdio MCP，rm 不引入 LLM SDK |
| Q5 | Agent 写回 | (a) 走 rm 写路径（history/backup/atomic/lock） |
| Q6 | frontmatter 写入 | (a) 改写路径为 frontmatter，保留未知键/键序/首行 |
| Q7 | 规则编号 | (a) S/M/H/L/T 主编号，E/W/I 为运行时增量 |
| Q8 | 一致性 CI 宿主 | (a) dev-kb（只读比对，不接管写侧） |
| Q9 | Agent 自治边界 | (b) 结构/元数据自动落盘，正文语义改动出提案等确认 |
| Q10 | 运行时状态位置 | (a) 全部移出 KB 目录到 app data |
| Q11 | Agent 触达写路径 | (a) rm 抽纯函数层 + stdio MCP server（Rust，~300–500 行） |
| Q12 | NN-MM 代号 | (a) frontmatter 加 `文档代号` 键（第 5 键），作为稳定引用键 |
| Q13 | 迁移批次 | (a) 先只读接入「能看」，验收后才开写 |
| Q14 | MCP 工具面 | (a) 三层：read 全开 / 结构写自动（dry_run）/ 语义写走提案（`_proposals/`） |
| Q15 | fixture 断言 | (a) 双黄金基线，起步形态为 Python 基线 + 映射表 |
| Q16 | 第 0 批交付 | (a) 噪声清除三件套 + 只读验收，不碰数据字节 |
| Q17 | 链接解析健壮性 | (c) rm 修根因（文件相对解析）+ 规范加 stem 唯一警告 |

---

## 3. 怎么开展任务（批次 0–5）

每批独立可回滚；上一批验收通过才开下一批。

### 批次 0 —— 噪声清除三件套（只改 rm，不碰任何 KB 数据）

| 动作 | 文件 | 内容 |
|---|---|---|
| E4 布局豁免 | `src-tauri/src/validator/rules.rs` | Flat 布局 + frontmatter 型 KB 跳过 E4（`type:` 标签要求只适用于声明了 `@synapse-tags` 体系的 KB） |
| I2 链接口径 | `src-tauri/src/validator/system_labels.rs` | 入度/出度统计纳入 `markdown_links` 经 `resolve_target` 的解析结果（复用 index.rs 的 lookup 构建），消除与 W5 的口径分裂 |
| YamlFields 保留未知键 | `src-tauri/src/parser/yaml_field.rs`、`entry.rs` | serde_yaml + indexmap 保序已知可行；未知键存旁路 map，`StoredEntry` 增加元数据摘要字段供 UI 展示 kaiwu 四键 |

**验收（Q16.a）：** `synapse validate kb/kaiwu` → 0 error / 0 warning（I 类仅剩合理项）；rm GUI 打开 kb/kaiwu：35 节点 + ~177 条条目间边可见、四键元数据可见、0 系统性误报。已知残留（接受不修）：README 的边不可见（rm 按设计跳过根 README），07-08、10-01 两节点零入边。

### 批次 0.5 —— CLI 接入 kaiwu 回归门禁

| 动作 | 文件 |
|---|---|
| 写适配脚本：`synapse validate --json` → check_kb JSON shape | kaiwu `scripts/`（新小脚本，不动 gate 逻辑） |
| `run_regression.sh:117` 改调适配后的 synapse CLI（check_kb.py 保留一版作比对参考） | kaiwu `scripts/run_regression.sh` |

**验收：** kaiwu 全链路 `run_regression.sh` PASS，`kb_errors: 0` 阈值口径不变。

### 批次 1 —— 写路径改造 + 状态迁出 + 解析健壮化（改 rm）

1. **frontmatter 保序写入器**（新模块，建议 `fm_writer.rs`）：读→改→写，保留未知键/键序/注释/行尾；`metadata_updater.rs` 改为调它；E1–E9 修复器同步改造（不再插 HTML 注释）。（Q6.a）
2. **运行时状态迁出 KB 目录**：`layout_store.rs`（`.synapse-layout.json`）与 `history.rs::history_dir`（`.synapse/history/`）迁到 app data `indexes/<kb_id>/` 旁；含旧目录搬迁逻辑；`execute_rename` 的布局/历史迁移路径同步更新。（Q10.a）
3. **链接解析根因修复**：`resolve_target` 在 stem 兜底之前增加「按来源文档 dir 归一化成根相对路径再查 by_rel_path」步骤；`文档代号`（NN-MM）索引作为最优先解析键。（Q17.a/Q12.a）
4. **移动/改名级联重写相对 markdown 链接**（D1，见 §5）：现级联只重写 `[[wikilink]]`，必须扩展到相对链接（按每个引用文档的位置计算新旧相对路径）。
5. **README = KB 首页**（D6）：根 README 作为特殊「首页」入口展示（可读、不入图谱、不索引为条目）。
6. **状态枚举统一**（D3）：canonical = 草案/模板/稳定/需持续监控/过期/已归档（+括号注记），rm UI 按语言映射。

**验收：** rm GUI 编辑 kaiwu 任一文档后，`check_kb.py --strict` 仍 0/0；演练「把一篇文档从 06-微调与训练/ 归档到 99-归档/」，所有指向它的相对链接自动改写，check_kb 仍 0/0（D1 场景）。

### 批次 2 —— 规范正文归位 + 双实现一致性 CI

1. `kb-general-spec.md` 从临时目录（`~/Documents/Trae-CN/temp_20261103_01/`，无版本控制）迁入 rm `docs/kb-spec.md`；补：26 条 ↔ E/W/I 映射表（S/M/H/L/T 主编号）、每条规则的盲区声明（D9）、`文档代号` 第 5 键、stem 唯一警告（Q17）、复审过期规则（D4）、依赖矩阵一致性规则（D8）。
2. `check_kb.py` 与 Rust validator 同步实现新规则（保持双实现同构）；删除或声明 rm 死规则（E8 缺位、no-op 规则）。
3. fixtures：kaiwu 35 篇快照 + ~20 个合成边界样例（frontmatter 前置注释、重名 stem、`.synapse` 产物、E4 修复后状态等实测冲突场景）+ 双黄金基线（Q15.a）。
4. dev-kb CI（`.github/workflows/ci.yml`）新增 fixture 比对 job：跑两实现、按映射表断言语义等价。

**验收：** dev-kb CI 绿：两实现对 55 份 fixture 给出映射等价结论；基线更新必须附理由（kaiwu 的基线冻结纪律）。

### 批次 3 —— stdio MCP server

新 bin（`src-tauri/src/bin/` 或独立 crate），复用 `*_impl` 纯函数层；工具面按 Q14 三层：
- read 全开：list_entries / read_entry / get_graph / validate / get_history
- 结构写自动：save_content（整文件，`origin=agent`）/ patch_metadata / apply_fix —— 全部强制 `dry_run` 参数
- 语义写：propose → 写入 KB 内 `_proposals/`（`_` 前缀 = 机制目录，双工具统一豁免规范校验但仍索引展示）

**验收：** EditAgent 冒烟：通过 MCP 给一篇文档补一个 frontmatter 键，全程有 dry_run 预览、有版本记录、有 origin 标记。

### 批次 4 —— EditAgent / ReviewerAgent

Python 独立进程，放 rm `agents/` 目录。行为宪章见 D7/D8（§5）。

**验收：** 语义提案 → 人审（rm GUI 内看 diff）→ 应用，闭环跑通；EditAgent 全程不直接写盘。

### 批次 5 —— dev-kb 接入为 MDSearchAgent

独立仓库不变；`projects.yaml` 已登记 kaiwu 与 rm。接入方式与三 Agent 同协议（stdio MCP）。

**验收：** 三 Agent 同协议互见；dev-kb 自身 CI（契约门禁）不受影响。

---

## 4. 验收标准

### 4.1 总验收（Definition of Done）

1. **能看**：rm GUI 打开 `kb/kaiwu`——35 节点、~177 条边、四键元数据 + 文档代号 + 依赖矩阵可见、0 系统性误报。
2. **能改不破**：rm 内任何编辑（含移动/归档）之后，`check_kb.py --strict` 与 `synapse validate` 按映射表结论一致，kaiwu 侧保持 0/0。
3. **能验**：dev-kb CI 双实现 fixture 比对绿；kaiwu `run_regression.sh` 全链路 PASS（换 synapse CLI 后口径不变）。
4. **能托**：EditAgent 结构写全程走 MCP（dry_run + history + origin），语义改动走提案闭环；Agent 从不直接写盘。
5. **不越界**：dev-kb 保持独立、只读；kaiwu 实验区（scripts/evals/app）行为不变。

### 4.2 每批验收

见 §3 各批次末行。原则：每批有可执行的单条验收命令或可复现的 GUI 检查步骤；不满足即回滚本批，不带病进下一批。

---

## 5. 补充思考轮：kaiwu 未提取的思想与合并冲突分析（D1–D9）

> 两者在契约层相似，但 kaiwu 真正的积累在「口径纪律」——这层思想 Q1–Q17 只吸收了一部分。
> 本轮逐条提取剩余思想，分析移植到 rm 时会出什么问题，给出处置。
> **以下处置已于 2026-11-17 全部确认，并入对应批次。**

### D1 移动/归档级联必须重写相对 markdown 链接 ⚠️ 硬冲突，最高优先

**kaiwu 思想：** 归档是受管生命周期——移入 `99-归档/`、重编序号、状态改 `已归档`、正文留迁移去向、README 归档索引保持 L3 可达。归档 = 文件移动 + 改名，是常规操作。

**冲突：** rm 的 `execute_rename` 级联**只重写 `[[old_id]]` wikilink**（architecture.md §12.7 第 4 步）。而 kaiwu 全用相对链接（`../06-微调与训练/03-x.md`），且扁平布局的 id 由路径派生——移动即换 id。在 rm 里归档一篇 kaiwu 文档 = 所有指向它的相对链接静默断掉，check_kb 的 L2 立即报错。

**处置：** 批次 1 第 4 项。级联按每个引用文档的位置计算新旧相对路径并重写；`文档代号` 作为稳定键降低对路径 id 的依赖（D2）。**这是 kaiwu 生命周期模型能在 rm 里存活的前提条件。**

### D2 NN-MM 文档代号作为关系解析的优先键

**kaiwu 思想：** `NN-MM` 代号是文档的稳定身份（35/35 可从路径机械派生，实测），依赖矩阵全靠它做交叉引用。

**冲突：** rm 的 id 随路径变（D1），关系解析四步兜底最终依赖 stem 唯一性——路径一变、stem 一撞，关系就错。

**处置：** 批次 1 第 3 项。`resolve_target` 先查代号索引（frontmatter `文档代号` 键），再走现有四步。代号成为不随移动改变的锚。

### D3 状态枚举统一

**冲突：** kaiwu 状态是中文枚举 + 括号注记（`稳定（首轮结论）`）；rm 的 metadata 编辑器默认值是英文 `'stable'` 自由文本。两套词汇在同一个字段上碰撞。

**处置：** 批次 1 第 6 项。canonical 用规范枚举（草案/模板/稳定/需持续监控/过期/已归档），rm UI 按 locale 映射显示；M3 的枚举校验进 Rust validator。

### D4 新鲜度契约：M4 进 Rust 校验器，I3 让位

**kaiwu 思想：** `最后更新` 是人工登记的复核日期（刻意不取 mtime），`下次 review` 过期报 warning——「知识会过期」被变成可见信号。

**冲突：** rm 的 I3 用文件 mtime 判 180 天陈旧。同一篇文档：人工刚复核但一年没改 → kaiwu 口径新鲜、rm 口径陈旧。两个新鲜度定义打架。

**处置：** 批次 2。Rust 实现等价 M4（有 `下次 review` 的条目以其为准）；I3 在复审日期存在时让位（不再按 mtime 报）。**EditAgent 不许代 bump 这些日期**（见 D7）。

### D5 文档类型词汇统一

**kaiwu 思想：** 五类型分类学（结论/方法/决策记录/数据与口径/归档），10-治理文档维护全库映射。

**巧合：** rm 的 E4 本来就要求 `type:` 前缀标签，但从没定义过词汇表。

**处置：** 批次 2 文档化：五类型作为规范 KB 的推荐词汇，`type:` 标签语义与之对齐；非规范 KB 不受约束（Q16 的 E4 豁免维持）。

### D6 README = KB 首页

**kaiwu 思想：** README 是唯一入口，承载阅读顺序、优先级视图（**优先级≠阅读顺序**——优先级回答「做决策该不该读」）、分类索引、维护约定。这是 kaiwu 编辑层最有价值的设计之一。

**冲突：** rm 按设计跳过根 README（不索引、不可见）——kaiwu KB 最重要的一页在 rm 里根本看不到。

**处置：** 批次 1 第 5 项。根 README 作为特殊「首页」入口展示：可读、不入图谱、不索引为条目、不参与关系解析。

### D7 Agent 编辑不碰复审元数据

**kaiwu 思想：** `最后更新` = 人工复核登记日期，刻意人工 bump。「Agent 自动更新日期」等于替人签到，日期变成谎言。

**处置：** 批次 3 行为规范。EditAgent 结构性编辑**不**自动改 `最后更新`/`下次 review`；Agent 出处由 rm history 的 `origin=agent` 记录，需要时人工复核后自行 bump。

### D8 ReviewerAgent 宪章（三条，全部源自 kaiwu 口径纪律）

1. **数字可追溯**：每个数字必须能指到来源文件；追不到的标「未确认」，不许按印象补写。（kaiwu README 维护约定）
2. **被推翻表述显式化**：旧结论错误时不静默改写——列出「已推翻的表述」+ 替代文档，走 supersedes/归档流程。（kaiwu 07-08 决策日志的修订规则）
3. **依赖矩阵一致性**：`依赖 ⊆ 相关文档`（实测样点 06-02 满足）+ 依赖/被依赖两列互惠——两条都可机器判定，进批次 2 的规则集。

### D9 盲区声明纪律 + 回归门禁文化

**kaiwu 思想：** 每条机器不查的规则必须写明「为什么不查」，避免后人误以为是遗漏；基线冻结、一次性消耗、跨口径不可相减。

**冲突：** rm validator 有死规则（E8 缺位、check_yaml_parse 空转、unused_attachments 桩）且无声明；规则集变更无门禁。

**处置：** 批次 2。合并规范逐条附盲区声明；死规则删除或补声明；fixture 基线更新必须附理由（沿用 kaiwu 纪律）。

---

## 6. 实测事实与证据表

| # | 事实 | 证据 |
|---|---|---|
| F1 | rm 加载 kaiwu KB：35 error / 0 warning / 35 info，error 全为 E4 | 实测：`synapse validate kb/kaiwu --json`（2026-11-16） |
| F2 | rm 运行时产物使 check_kb 报 6 error / 8 warning（S2/M1/H1/L3/S4） | 实测：构造含 `.synapse-layout.json`、`.synapse/history/` 的 KB 跑 check_kb |
| F3 | E4 一键修复把注释插到 frontmatter 之前 → M1 立即失败 | 实测复现；源码 `e4_default_type.rs:49-55`、`metadata_updater.rs:60` |
| F4 | I2 与 W5 对同一文件给出相反的孤儿判断 | 实测 F1 + 源码：`system_labels.rs:32-39,71` 只认双链；`rules.rs` W5 认 markdown 链接 |
| F5 | kaiwu 全部链接靠 stem 兜底解析；35 stem 恰好唯一（无人强制） | 源码 `index.rs:783` resolve_target 四步 + `by_rel_path` 只存根相对路径（:657-680）；实测 dup_stems=[] |
| F6 | 条目间链接 ~177 实例 / ~165 对；README 另有 ~34 条不可见 | 模拟脚本（2026-11-17，修正 off-by-one 后口径）；与迁移报告 199 边吻合 |
| F7 | NN-MM 代号 35/35 可从路径机械派生 | 实测脚本（2026-11-17） |
| F8 | 依赖矩阵「依赖」列 ⊆ 该文档「相关文档」（样点 06-02），两者是不同关系 | 实测抽样 |
| F9 | rm 写路径 = 34 个 Tauri command，外部进程不可达 | `lib.rs:254` generate_handler |
| F10 | rm 从不写条目级 YAML；YamlFields 仅 7 个展示字段，kaiwu 四键被静默丢弃 | grep 全仓；`yaml_field.rs:5-20` |
| F11 | rename 级联只重写 wikilink，不重写相对 markdown 链接 | rm `docs/architecture.md` §12.7 第 4 步 |
| F12 | kaiwu 的 26 条规则已进回归门禁（`kb_errors: 0`），但仓库无 CI | `run_regression.sh:117`、`compare_regression.py:131`、`regression_config.json:35` |
| F13 | 规范正文在无版本控制的临时目录 | `~/Documents/Trae-CN/temp_20261103_01/kb-general-spec.md` |
| F14 | serde_yaml 0.9 依赖 indexmap（键序可保，保序写入器可行） | `Cargo.lock` serde_yaml 0.9.34 依赖表 |

---

## 7. 明确不做

- 不新建仓库（Q1.a）；`agents/` 放 rm 仓库内。
- dev-kb 不接管写侧（其 ADR-0030 维持）；不动它的契约层与检索器。
- 不做一次性全量迁移（Q13.a）；每批独立可回滚。
- 不动 rm 的确定性网格布局等 UI 架构；不引入力导向图。
- 机器只判形式，不查内容真实性（语义交 ReviewerAgent + 人）——kaiwu 校验器哲学原样保留。
- Agent 不自动 bump 复审日期（D7）；不静默改写被推翻的结论（D8-2）。

## 8. 变更记录

| 日期 | 变更 |
|---|---|
| 2026-11-17 | 初版：Q1–Q17 共识收拢 + 补充思考轮 D1–D9 + 批次 0–5 执行纲要。D1–D9 待确认。 |
| 2026-11-17 | D1–D9 全部按推荐确认，状态改为定稿；明确执行未开始，等待启动批次 0。 |
