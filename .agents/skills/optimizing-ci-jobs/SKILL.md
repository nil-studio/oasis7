---
name: optimizing-ci-jobs
description: Use when reducing oasis7 GitHub Actions job count, runner queue pressure, or full-scope CI fallback frequency without regressing critical-path wall clock or security invariants. Do not use for routine business test failure debugging.
file_names:
    - references/consolidation-and-scoping-patterns.md
---
# optimizing-ci-jobs

通用规则见 [开发流程规范](../../../doc/engineering/workflow/source-of-truth.md)。CI 分层与门禁主责合同见 [PR 快速门禁与定期全量设计](../../../doc/testing/ci/ci-pr-fast-and-periodic-full-design.md) 与 [CI 分层执行设计](../../../doc/testing/ci/ci-tiered-execution.design.md)；命令入口见 [测试手册](../../../testing-manual.md)。本方法总结如何在不削弱门禁证据、不拉长单 job 关键路径墙钟的前提下持续缩减 CI job 数量与队列开销，不新增任务准入或审批条件。

## 1. 先用真实运行遥测建立基线

动手前先通过 `gh run list --workflow Rust` 与 `gh run view <run-id> --json jobs` 采集近期 `pr-fast`、`main-fast` 和 `scope=full` 的真实执行数据，区分三件事：

- **关键路径（Critical Path）**：找出决定整条流水线总墙钟的最慢链路（如 `oasis7-test-build-*` → `oasis7-required` 分片、`custody-native`/`custody-wasm-production` → `custody-node`、`viewer-js-required`）。任何合并都不得让目标宿主 job 逼近或超过当前关键路径。
- **高冷启动短 job**：识别本身执行仅数秒至一分钟，却独占容器拉取、`actions/checkout`、工具链安装或产物上传下载的独立 job。
- **过度扇出与全量回退（`scope=full`）**：检查日常小改动或规则注册 PR 为何退化为全量 25+ job，或为何唤醒与受测代码无真实依赖的构建/跨平台矩阵。

## 2. 按四类手段排定收益与边界

优先使用不损失覆盖面且可静态证明等价性的手段；详细案例与代码接缝见 [实证模式参考](references/consolidation-and-scoping-patterns.md)。

| 优化手段 | 适用信号 | 核心做法 | 硬边界（不可突破） |
| --- | --- | --- | --- |
| **精准切片，收敛 `scope=full`** | 小范围控制脚本、条件依赖或 `rules` 登记触发全量 | 在 `scripts/plan-rust-required-scope.py` 与 `scripts/ci-required-scope.json` 按真实消费闭包子集选组（如目标平台 `cfg` 依赖、受控字段切片、保序且删零匹配规则的 `rules` 增删） | 无法证明安全不变量时必须 **fail-closed 回退 `scope=full`**；禁止白名单硬编码跳过未验证影响面 |
| **短合约并入同栈宿主 job** | 10s–60s 的契约/静态检查独占 runner | 并入已具备相同镜像、工具链或浏览器的常驻/同域 job（如 `baseline`、`rust-baseline`、`workflow-governance`、`viewer-js-required`、`fleet-health`）；仅命中轻量组时按 `if:` 跳过宿主的重型编译步骤 | 不跨权限等级（候选只读 vs 受信写缓存）混跑；不把重型编译塞进快基线导致单 job 墙钟膨胀 |
| **消除纯搬运/汇总中转 job** | 独立 job 仅下载上游分片产物做 `validate` 或二次打包，或主干连续合入重复重跑 | 在 `select` 已检出受测提交时导出不可变清单至 `ci-authority`，由最终 `required-gate` 零检出直验（`--context-file`）；合并同栈串行构建产物；主干启用 `main-fast` 准入跳过与并发合并（coalesce） | 零检出门禁必须使用 `policy_sha` 侧校验器核验 `tested_sha` 的完整预期清单，防止分片静默漏跑 |
| **预检门控与按需矩阵** | 昂贵跨平台或重型构建在廉价预检失败时仍全量并发消耗配额 | 用快速预检门控后续重型宿主机 job，或按实际受影响平台动态裁剪 OS 矩阵 | `periodic-full` / `candidate-full` 等显式全量收据契约保持完整覆盖，不把快速门禁裁剪混入全量发布证明 |

## 3. 混合版本切换与安全铁律

CI 控制面改动在合入 `main` 前后天然经历“旧 `policy_sha`（`origin/main`）评判新工作流”的窗口，实施时必须遵守以下规则：

1. **保持 `policy_sha` 与 `tested_sha` 职责隔离**：计划器、结果判定器和门禁规则来自受信基线（`policy_sha` / `ci-authority`）；候选提交不能在 PR 中自授豁免或覆盖门禁脚本。
2. **用显式单调版本号门控结构切换**：凡是改变 job 归属、退休汇总 job 或调整结果判定的优化，在 `scripts/ci-required-scope.json`、`scripts/plan-rust-required-scope.py`、`scripts/ci-required-result.py` 和 `scripts/ci-full-result.py` 中以单调递增的 `*_version` 双向门控（`tested_source_supports_capability`），保留旧版本回退分支（旧独立 job 以 `!= <new_ver>` 守护跳过而非直接硬删）。
3. **严守 `required-gate` 的 `needs:` 兼容约束**：旧版 `origin/main` 的 `scripts/ci-required-result.py` 要求 `required-gate` `needs:` 中出现的非预期额外 job 在本 PR 的 `scope=full` 首跑中必须为 `success`（不能为 `skipped`）。向 `required-gate` 新增 `needs:` 时，只加入在旧版 `scope=full` 下同样执行并成功的生产者/分片 job，绝不加入在 `scope=full` 下会被 `skipped` 的 legacy fallback job。
4. **先补设计再改实现**：新增或升级能力版本前，先同步更新 [PR 快速门禁设计](../../../doc/testing/ci/ci-pr-fast-and-periodic-full-design.md) 与 [CI 分层执行设计](../../../doc/testing/ci/ci-tiered-execution.design.md) 中的版本能力说明及 `O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29` 追踪说明。

## 4. 本地闭环验证与交付

由于修改 `.github/workflows/rust.yml`、`scripts/ci-required-scope.json` 或核心计划脚本会在远端触发 `scope=full`，推送前务必在本地一次跑通受影响验证，避免反复重跑全量流水线：

```bash
# 1. 若修改了托管合约组映射或合并版本，先同步生成块并校验无漂移
python3 scripts/ci-contract-jobs.py --write
python3 scripts/ci-contract-jobs.py --check
python3 scripts/ci-contract-jobs.test.py

# 2. 核心门禁结果与 custody/产品文档范围不变量检查
python3 scripts/ci-required-result.test.py
python3 scripts/ci-full-result.test.py
python3 scripts/ci-custody-platform.test.py
python3 scripts/resolve-product-doc-ci-range.test.py RangeTests.test_workflow_wiring_is_baseline_only_and_cannot_override_exports

# 3. 完整工作流图与范围规划回归（覆盖旧版兼容与新版门控）
TMPDIR=/dev/shm python3 scripts/ci-workflow.test.py
TMPDIR=/dev/shm RUSTUP_TOOLCHAIN=1.96.0 python3 scripts/plan-rust-required-scope.test.py

# 4. 本地固定快速基线与格式检查
python3 scripts/check-local.py --profile quick --base-ref origin/main
```

涉及 CI 控制面、权限或门禁判定的改动，按规范完成独立评审（[requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md)）。PR 正文写明实测基线 run ID、减少的 job 数、宿主单 job 墙钟影响以及混合版本首跑说明（本 PR 在远端跑旧 `policy_sha` 的 `scope=full`，合入 `main` 后对后续 PR 正式生效）。

## Supporting Files

- `references/consolidation-and-scoping-patterns.md`：第 1–7 轮 CI 缩减的实证模式、关键文件职责表与高频踩坑清单。

## Verification

修改本 Skill 后运行 `./scripts/lint-skills.sh`、`./scripts/doc-governance-check.sh`、`python3 scripts/check-local.py --profile quick --base-ref origin/main` 和 `git diff --check`。

## Related methods

- [executing-project-tasks](../executing-project-tasks/SKILL.md)
- [verification-before-completion](../verification-before-completion/SKILL.md)
- [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md)
- [systematic-debugging](../systematic-debugging/SKILL.md)

## Known Failure Modes

- 直接删除旧合约/汇总 job 而不加版本化守卫，会导致本 PR 在旧 `origin/main` `policy_sha` 下因缺少预期 job 直接判红，或阻塞进行中的旧基线分支。
- 在宿主 job（如 `baseline`）步骤上额外覆盖 `OASIS7_PRODUCT_DOC_BASE` / `OASIS7_PRODUCT_DOC_HEAD`，或只给 `Swatinem/rust-cache` 加 `if:` 而不给配对的 `Stabilize Rust cache toolchains` 加完全一致的 `if:`，会被工作流静态不变量测试拦截。
- 未检索跨工作流静态测试（如 `scripts/security/codeql-rust-coverage.test.py` 对 `paths-ignore:` 的解析假设）就盲目改动旁路工作流触发器，会造成隐性合约失败。
