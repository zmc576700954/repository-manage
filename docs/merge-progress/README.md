# kaiwu × rm 合并执行进度

> 计划来源：[../merge-kaiwu-plan.md](../merge-kaiwu-plan.md)（定稿 2026-11-17）。
> 本目录是执行态记录：**一个批次一个文档**，避免单文档过大。
> 每批文档固定结构：目标 / 修改清单 / 新增测试（测后已删）/ 验收 / 遗留与续接点。

## 纪律（来自任务约定）

1. 不跑全量测试套件；每处修改补针对性单元测试，跑绿后删除测试代码，生产代码保留。
2. 每批记录完整修改内容，中断后可从本目录恢复上下文再继续。
3. 每批独立可回滚；上一批验收通过才开下一批。

## 批次状态

| 批次 | 内容 | 状态 | 文档 |
|---|---|---|---|
| 0 | 噪声清除三件套（E4 豁免 / I2 口径 / YAML 未知键保留） | ✅ 完成 2026-11-17 | [batch-0.md](batch-0.md) |
| 0.5 | CLI 接入 kaiwu 回归门禁（适配脚本 + run_regression.sh） | ✅ 完成（接线级；全链路待模型目录恢复） | [batch-0.5.md](batch-0.5.md) |
| 1 | 写路径改造 + 状态迁出 + 解析健壮化 + 级联重写 + README 首页 + 状态枚举 | 未开始 | batch-1.md |
| 2 | 规范正文归位 + 双实现一致性 CI + fixtures | 未开始 | batch-2.md |
| 3 | stdio MCP server（三层工具面） | 未开始 | batch-3.md |
| 4 | EditAgent / ReviewerAgent（Python 独立进程） | 未开始 | batch-4.md |
| 5 | dev-kb 接入为 MDSearchAgent | 未开始 | batch-5.md |

## 环境事实（跨批次有用）

- kaiwu 仓库：`/Users/zhumingchen/kaiwu`（KB 在 `kb/kaiwu/`，35 篇 + 根 README）。
- rm CLI：`cd src-tauri && cargo build --bin synapse` → `./target/debug/synapse validate <kb-path> [--json]`。
- kaiwu 文档格式：YAML frontmatter 四键（优先级 / 状态 / 最后更新 / 下次 review），无 `@synapse-*` 注释、无 `tags`。
- NN-MM 代号可从路径机械派生（目录序号-文件序号），批次 1 实现代号索引。
