# 批次 0 —— 噪声清除三件套（只改 rm，不碰任何 KB 数据）

> 状态：**✅ 已完成**（2026-11-17）。计划见 [../merge-kaiwu-plan.md](../merge-kaiwu-plan.md) §3 批次 0。

## 验收结果（Q16.a）

```
synapse validate /Users/zhumingchen/kaiwu/kb/kaiwu --json
→ 改动前: {errors: 35, warnings: 0, info: 35}   by rule: {E4: 35, I2: 35}
→ 改动后: {errors: 0, warnings: 0, info: 0}     by rule: {}
```

- E4 清零：kaiwu 无任何 tags 声明 → 门控关闭，35 个 error 消失。
- I2 清零：全部条目的相对 markdown 链接经 resolve_target 解析出边（stem 兜底），
  无孤立条目。计划预期「07-08、10-01 零入边」的条目也有出边，故不触发 I2。
- I1 也为 0：kaiwu 文件最后修改 >7 天（合理，不算误报）。
- README 边不可见（rm 按设计跳过根 README）——接受不修，批次 1 第 5 项（README 首页）处理。
- **GUI 四键可见性待用户在 tauri:dev 里确认**（自动化已覆盖解析与落库，见测试记录）。

## 修改清单（生产代码，全部保留）

### 0.1 E4 豁免（tags 体系声明门控）

- `src-tauri/src/entry.rs`
  - `RawEntry` 新增 `extra_meta: Vec<ExtraMetaEntry>` 与 `declares_tags_system: bool`。
  - `parse_body`：tags 体系声明 = `@synapse-tags` 注释 **或** frontmatter `tags` 键。
  - 三处 RawEntry 构造（read_md_file / read_entry 两分支）补齐新字段。
- `src-tauri/src/validator/rules.rs`
  - `check_required` / `check_all` 增加 `tags_system_declared: bool` 参数；
    E4 只在门控开启时检查 `type:` 前缀。
  - 决策说明：计划原文写「Flat 布局 + frontmatter 型 KB 跳过 E4」，实现取其括号原则
    「`type:` 要求只适用于声明了 `@synapse-tags` 体系的 KB」——KB 内任一条目声明
    （注释或 YAML tags 键）即视为声明。这比按布局判断更准确：声明了体系的扁平 KB
    仍然被校验，未声明的目录布局 KB 也不再误报。
- `src-tauri/src/validator/mod.rs`：validate_kb 计算 KB 级门控并传入；
  `compute_all` 调用加 kb_root 参数。
- `src-tauri/src/commands.rs`：`validate_entry`（单文件校验）传 `entry.declares_tags_system`。
- `src-tauri/src/index.rs`：rebuild 的 check_all 调用点同样计算门控。
- fix 流程无需改：E4 不再产出 issue，`e4_default_type` 修复器自然不会被触发。

### 0.2 I2 链接口径统一

- `src-tauri/src/index.rs`
  - 提取 `pub(crate) struct LinkLookups` + `pub(crate) fn build_link_lookups(entries, kb_root)`
    （rebuild 原内联构建逻辑原样搬出，语义未变）。
  - `resolve_target`、`is_attachment_url` 改 `pub(crate)` 供 validator 复用。
  - rebuild 内三处 resolve_target 调用改用 `lookups.` 字段。
- `src-tauri/src/validator/system_labels.rs`
  - `compute_all(entries, kb_root)`：I2 入/出度改用 `build_link_lookups` + `resolve_target`，
    `[[wikilink]]` 与内部 markdown 链接（剔除外链/附件）统一计入；
    自链接不计边（旧口径自链会自我解孤，属隐性 bug，一并修正）。
  - I4（断链）仍按 wikilink 口径，与 W4 一致，不在本批范围。

### 0.3 frontmatter 未知键旁路 + 四键展示链路

- `src-tauri/src/parser/yaml_field.rs`
  - 新增 `ExtraMetaEntry { key, value: serde_json::Value }`（serde camelCase）。
  - 新增 `parse_yaml_fields_with_extras`：一次解析 frontmatter，
    已知 7 键（color/shape/pinned/order/icon/preview/hidden）进 `YamlFields`，
    其余键保序进旁路表（serde_yaml 0.9 Mapping 为 IndexMap 背书，F14）。
  - 行为放宽（记录在案）：已知键类型不符时按缺省处理，不再使整个 frontmatter 失效
    （旧口径会丢弃全部字段）。
  - `parse_yaml_fields` 保留未动（metadata_updater 等旧路径仍用）。
- `src-tauri/src/index.rs`
  - schema：entries 表加 `extra_meta TEXT`；`migrate_schema` 加对应 ALTER（老库自动补列）。
  - `StoredEntry` 加 `extra_meta: Vec<ExtraMetaEntry>`；upsert / get_all_entries 同步。
- `src-tauri/src/commands.rs` / `src-tauri/src/lib.rs` / `src-tauri/src/fingerprint.rs`：
  结构体字面量补新字段（无逻辑变化）。
- `shared/types.ts`：`ExtraMetaEntry` 接口；`Entry.extraMeta?` / `StoredEntry.extraMeta`。
- `src/views/DetailPanel.tsx`：EntryMetadataEditor 下方新增只读键值段
  （`extraMeta` 非空才渲染），值非字符串时 JSON 序列化展示。
- i18n：`DetailPanel.zh.json` / `.en.json` 新增 `extraMetaHeader`
  （「其他元数据（frontmatter）」/「Other metadata (frontmatter)」），键序对齐。

### 进入本批时工作树已有的未提交改动（前置噪声清除，归入本批）

1. `src-tauri/src/scanner.rs`：`is_artifact` 按 KB 根相对路径段判断，任何 `.` 开头
   路径段（`.synapse-fix-backup/`、`.obsidian/` 等）不进扫描。带测试（保留）。
2. `src-tauri/src/validator/rules.rs`：`is_internal_md_link` + W5 认内部 markdown 链接。
   带 3 个测试（保留）。**此改动是 0.2 的前提**（W5 与 I2 同口径）。

### 附带观察（不在本批修，批次 1 Q17 处理）

- 增量索引路径（`insert_entry_relations`，源 DB 构建）在 by_rel_path 里额外存
  「去 `.md` 扩展名」变体；rebuild 的 builder 不存。两条路径对无扩展名引用的解析
  可能差一步（都最终靠 stem 兜底，正常情况下结果一致）。批次 1 重做解析时统一。
- `resolve_target` 不剥 `../` 前缀，kaiwu 相对链接全靠 stem 兜底命中（F5）；
  stem 撞名即错边。批次 1 第 3 项（来源目录归一化 + 文档代号索引）修根因。

## 新增测试（已按纪律删除，生产代码保留）

| 位置 | 覆盖点 | 结果 |
|---|---|---|
| yaml_field.rs ×4 | kaiwu 四键保序保值；已知键抽取+旁路保位；YAML tags 键进旁路；非映射 frontmatter 报错 | ✅ |
| entry.rs ×3 | kaiwu 式扁平文档四键进 extra_meta 且不声明 tags 体系；@synapse-tags 声明；frontmatter tags 键声明 | ✅ |
| rules.rs ×2 | 门控关→无 E4；门控开→E4 照报 | ✅ |
| system_labels.rs ×3 | 相对 md 链接成边→无孤立；纯外链→孤立照报；入向度数生效 | ✅ |
| index.rs ×1 | rebuild 后四键落库可读 + 相对链接解析成 reference 边（kaiwu 式目录结构） | ✅ |

测试期间临时把 `rules.rs::make_entry` / `system_labels.rs::entry_with_path` 改为
`pub(super)` 供兄弟测试模块复用，测试删除后已还原为私有。删除后全套件 310 passed /
0 failed（与改动前一致）。

## 遗留与续接点

1. GUI 四键可见性：启动 `npm run tauri:dev`，打开 kaiwu KB，任选一篇文档看详情面板
   「其他元数据（frontmatter）」段（应见 优先级/状态/最后更新/下次 review）。
2. 批次 0.5 入口：kaiwu 侧 `scripts/` 写适配脚本（synapse --json → check_kb JSON shape），
   `scripts/run_regression.sh:117` 改调适配后的 synapse CLI；check_kb.py 保留一版作比对。
3. 未提交：本批所有改动在工作树未提交。建议每批一个 commit 便于回滚（见批次 README 纪律）。
