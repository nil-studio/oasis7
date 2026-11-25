---
name: optimizing-ci
description: Use when analyzing or optimizing oasis7 CI workflows, including reducing job counts, cutting queue/wall-clock latency, tuning cache/build efficiency, or narrowing full-scope fallbacks without weakening security or gate invariants. Do not use for routine business test failure debugging.
file_names:
    - references/job-count-reduction.md
---
# optimizing-ci

通用规则见 [开发流程规范](../../../doc/engineering/workflow/source-of-truth.md)。CI 分层与门禁主责合同见 [PR 快速门禁与定期全量设计](../../../doc/testing/ci/ci-pr-fast-and-periodic-full-design.md) 与 [CI 分层执行设计](../../../doc/testing/ci/ci-tiered-execution.design.md)；测试入口与命令语义见 [测试手册](../../../testing-manual.md)。

本 Skill 是仓库通用 CI 优化方法的总入口，详细专题按分册放在 `references/` 下按需读取；本入口只定义共享诊断口径、安全底线、通用验证与分册路由，不另立第二套门禁真值，也不新增任务准入或审批条件。

## 1. 按优化目标选择分册

先根据瓶颈现象确认优化维度，再只读取对应分册：

| 优化维度 | 典型信号 | 对应分册 |
| --- | --- | --- |
| **减少 Job 数量与收敛全量触发** | 单次运行启动过多短耗时 job、串行汇总跳数多、小范围或规则改动频繁退化 `scope=full` | [减少 Job 数量分册](references/job-count-reduction.md) |
| **后续扩展方向（按需新增分册）** | 单 job 编译/测试慢、缓存命中率或构建产物膨胀、队列反压与 flaky 治理、镜像预装与冷启动优化等 | 在 `references/` 下新增对应专题分册并在本表登记 |

## 2. 所有 CI 优化共享的四条底线

无论优化 job 数量、编译耗时还是缓存策略，均须同时满足以下约束：

1. **先用真实运行遥测建立可比基线**：通过 `gh run list --workflow Rust` 与 `gh run view <run-id> --json jobs` 区分队列等待时间、关键路径单 job 墙钟、runner 总核时与触发频率。不为压缩单一指标而明显拉长关键路径墙钟或制造隐性重跑。
2. **严守 `policy_sha` 与 `tested_sha` 权威隔离及 fail-closed**：PR 门禁计划器、结果判定器、缓存写权限和信任锚点必须受控于受信基线（`policy_sha` / `ci-authority`）；候选代码（`tested_sha`）不得在 PR 中自授豁免、覆盖门禁脚本或污染共享缓存。无法静态证明等价覆盖时一律 **fail-closed** 回退保守路径。
3. **混合版本窗口平滑切换**：凡改变 job 拓扑、产物契约、结果判定或工作流摘要的优化，必须使用单调递增的能力版本号（`*_version`）双向门控，保证本优化 PR 在旧 `origin/main` `policy_sha` 下首跑通过，同时保留 `periodic-full` / `candidate-full` 等显式全量证明边界不被快速门禁裁剪侵蚀。
4. **设计先行与本地闭环收敛**：调整门禁能力或分层策略前，先同步受影响设计文档（及适用 `O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29` 追踪说明）。由于工作流与核心控制脚本修改通常触发远端 `scope=full`，须先在本地跑通对应控制单测与 PR 边界检查后再成批推送，避免高频零散 push 反复拉起和取消全量流水线。

## 3. 通用验证与交付要求

- **实施 CI 代码/工作流改动时**：按所选分册运行对应控制脚本单测（如 `ci-contract-jobs.py --check`、`ci-workflow.test.py`、`plan-rust-required-scope.test.py` 等）、`./scripts/check-pr-boundaries.sh --slice <Slice>` 与 `python3 scripts/check-local.py --profile quick --base-ref origin/main`；涉及门禁判定、权限或信任边界的修改按 [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md) 完成独立评审。PR 正文写清基线 run ID、优化前后指标对比与混合版本生效说明。
- **扩展或修改本 Skill 分册时**：
  - 保持主入口 `SKILL.md <= 300` 行，具体套路、脚本接缝与踩坑案例下沉到 `references/<topic>.md`；
  - 新增分册须同时登记到 frontmatter `file_names`、上表路由及下方 `## Supporting Files`；
  - 运行 `./scripts/lint-skills.sh`、`./scripts/doc-governance-check.sh`、`./scripts/check-pr-boundaries.sh --slice Docs`、`python3 scripts/check-local.py --profile quick --base-ref origin/main` 和 `git diff --check`。

## Supporting Files

- `references/job-count-reduction.md`：减少 CI Job 数量、归并短合约、零检出门禁直验与收敛 `scope=full` 误触发的实证方法与防坑清单。

## Related methods

- [executing-project-tasks](../executing-project-tasks/SKILL.md)
- [verification-before-completion](../verification-before-completion/SKILL.md)
- [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md)
- [systematic-debugging](../systematic-debugging/SKILL.md)
- [evaluating-development-methods](../evaluating-development-methods/SKILL.md)

## Known Failure Modes

- 未核实关键路径就把多个中重型检查塞入同一 job，表面减少了 job 数，实则拉长单次 PR 等待墙钟。
- 直接删除旧 job 或向 `required-gate` `needs:` 塞入旧版 `scope=full` 下会 `skipped` 的 fallback job，导致优化 PR 自身在旧 `origin/main` `policy_sha` 首跑时判红。
- 只改一个工作流而忽略跨脚本静态结构测试（如 `baseline` 产品文档范围环境变量不覆盖约束、Rust 缓存配对 `if:`、旁路工作流 `paths-ignore` 解析假设），直到远端全量运行才暴露。
