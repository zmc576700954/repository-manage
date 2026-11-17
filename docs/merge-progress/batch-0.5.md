# 批次 0.5 —— CLI 接入 kaiwu 回归门禁

> 状态：**✅ 完成（接线级验收通过；全链路 PASS 待模型目录恢复）**（2026-11-17）。
> 计划见 [../merge-kaiwu-plan.md](../merge-kaiwu-plan.md) §3 批次 0.5。
> 验收：kaiwu 全链路 `run_regression.sh` PASS，`kb_errors: 0` 阈值口径不变。

## 修改清单（全部在 kaiwu 仓库 `/Users/zhumingchen/kaiwu`）

1. **新增 `scripts/check_kb_synapse.py`**（适配脚本，不动 gate 逻辑）
   - 输入：`synapse validate <kb-root> --json`（rm 的 CLI，退出码 0/1/其他）。
   - 输出：check_kb.py `--format json` 的分组形状
     `[{"project", "findings": [{"rule_id", "level", "path", "message"}]}]`，
     供 `compare_regression.py` 原样消费。
   - 映射规则：severity error/warning 原样映射为 level；synapse 的 info 不进
     findings（check_kb 模型里没有 info）；`ignored` 的 issue 丢弃；空分组不输出。
   - `--strict`：errors > 0 时退出码 1（镜像 check_kb --strict）。
   - 二进制解析：`--synapse-bin` 参数 → `SYNAPSE_BIN` 环境变量 → PATH 上的 `synapse`。
   - 调用失败（二进制缺失/异常退出码）退出码 2，stderr 给出构建指引。
2. **`scripts/run_regression.sh` 第 3 步改调适配器**
   - `SYNAPSE_BIN` 默认 `$HOME/repository-manage/src-tauri/target/debug/synapse`，
     可环境变量覆盖；不存在/不可执行时报错退出 2（附 cargo build 指引）。
   - `check_kb.py` 保留一版作比对参考（计划要求），门禁不再调用。
   - `kb_errors: 0` / `kb_warnings: 0` 阈值仍由 `evals/regression_config.json` gates 决定，未动。

## 独立验证（已通过）

| 用例 | 结果 |
|---|---|
| happy path：kaiwu KB，0 findings → 输出 `[]`，rc=0 | ✅ |
| strict：合成含 `@synapse-tags: note`（无 type:）的临时 KB → E4 error 形状正确，rc=1 | ✅ |
| 二进制缺失：`/nonexistent/synapse` → 报错信息清晰，rc=2 | ✅ |
| 交叉比对：`check_kb.py --project kaiwu --strict --format json` 同样输出 `[]` rc=0 | ✅ 两校验器结论一致 |

## 验收记录

### 接线级验收（✅ 通过，2026-11-17）

`run_regression.sh` 实跑时在第 1 步失败：**默认抽取模型目录
`models/Qwen3-4B-Instruct-2507-mlx-4bit` 已不存在**（models/ 只剩 adapters/，
2026-11-05 上次全链路之后被清理）——环境问题，与本批改动无关。

因此改为接线级验收（真实跑第 3/4 步 + compare 门禁判定路径）：

1. 第 3 步（适配器 → synapse CLI）：rc=0，输出 `[]`。
2. 第 4 步（泄漏检查，真实跑）：`train=302 verdict=PASS（精确重合 0，文本疑似泄漏 0）`。
3. `compare_regression.py`（用 11-05 存档 summary 的抽取/检索指标合成输入，
   仅补齐第 1/2 步缺失的环境依赖；kb 输入是本批适配器的真实输出）：
   - `kb.errors: 0 (阈值 0)` / `kb.warnings: 0 (阈值 0)` —— **阈值口径不变，判定路径正确消费适配器形状**。
   - verdict **PASS（无指标回退，门禁通过）**，rc=0。

### 全链路 PASS（⏳ 待办，环境恢复后一条命令补验）

```bash
# 恢复模型（~3GB，mlx-community/Qwen3-4B-Instruct-2507-4bit → models/Qwen3-4B-Instruct-2507-mlx-4bit，
# 或按 kaiwu 的 model_registry 下载方式），然后：
cd /Users/zhumingchen/kaiwu && scripts/run_regression.sh
```

预期：第 1/2 步指标与基线持平（模型与基线未变），第 3 步走本批适配器，总判定 PASS。

## 遗留与续接点

1. 全链路 PASS 待模型目录恢复（见上）。
2. rm 侧更新后需 `cargo build --bin synapse` 刷新门禁用的二进制（脚本已写明指引）。
3. 批次 1 入口：见 [batch-1.md](batch-1.md)（写路径改造 + 状态迁出 + 解析健壮化等 6 项）。
