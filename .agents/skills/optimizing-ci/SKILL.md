---
name: optimizing-ci
description: Use when analyzing or optimizing oasis7 CI workflows, including reducing job counts, tightening targeted on-demand scope and fail-fast preflight gates, eliminating two-step trusted-baseline or unregistered-input prerequisite micro-PRs, cutting queue/wall-clock or multi-OS billed latency, tuning cache/build efficiency, or narrowing full-scope fallbacks without weakening security or gate invariants. Do not use for routine business test failure debugging.
file_names:
    - references/job-count-reduction.md
    - references/on-demand-scope-and-preflight.md
    - references/capacity-budget-and-cancel-waste.md
    - references/trusted-baseline-and-input-governance.md
---
# optimizing-ci

通用规则见 [开发流程规范](../../../doc/engineering/workflow/source-of-truth.md)。CI 分层与门禁主责合同见 [PR 快速门禁与定期全量设计](../../../doc/testing/ci/ci-pr-fast-and-periodic-full-design.md) 与 [CI 分层执行设计](../../../doc/testing/ci/ci-tiered-execution.design.md)；测试入口与命令语义见 [测试手册](../../../testing-manual.md)。

本 Skill 是仓库通用 CI 优化方法的总入口，详细专题按分册放在 `references/` 下按需读取；本入口只定义共享诊断口径、安全底线、通用验证与分册路由，不另立第二套门禁真值，也不新增任务准入或审批条件。

## 1. 按优化目标选择分册

先根据瓶颈现象确认优化维度，再只读取对应分册：

| 优化维度 | 典型信号 | 对应分册 |
| --- | --- | --- |
| **减少 Job 数量与收敛全量触发** | 单次运行启动过多短耗时 job、串行汇总跳数多、小范围或规则改动频繁退化 `scope=full` | [减少 Job 数量分册](references/job-count-reduction.md) |
| **收紧按需选测边界、多平台切片与前置快速失败** | `scope=targeted` 普通 PR 仍陪跑无关重编译/前端构建、高单价 macOS/Windows 被连带唤醒、格式等低级错误仍空跑多个重编译 job，或同源检查上下游重复执行 | [按需选测与快速失败分册](references/on-demand-scope-and-preflight.md) |
| **容量预算、取消浪费归因与主干节流** | `cancelled` 占比偏高、`main-fast` 高频合入导致日均核时居高不下，或需对容量优化做合入后 12–24h 遥测复测 | [容量预算与取消浪费分册](references/capacity-budget-and-cancel-waste.md) |
| **受信基线不变量、输入路由治理与消除前置微 PR** | 业务拆迁或工作流迭代频繁被迫先提 1–2 文件的白名单/过渡前置 PR（精确依赖集合拦截、`UnknownInputsError` / CodeQL `unregistered input`、或非演进契约双跑锁死） | [受信基线与输入治理分册](references/trusted-baseline-and-input-governance.md) |
| **后续扩展方向（按需新增分册）** | 单 job 编译/测试慢、缓存命中率或构建产物膨胀、队列反压与 flaky 治理、镜像预装与冷启动优化等 | 在 `references/` 下新增对应专题分册并在本表登记 |

## 2. 所有 CI 优化共享的四条底线

无论优化 job 数量、选测边界、编译耗时还是缓存策略，均须同时满足以下约束：

1. **先用真实运行遥测建立可比基线（含跨平台计费倍率）**：通过 `gh run list --workflow Rust` 与 `gh run view <run-id> --json jobs` 区分队列等待时间、关键路径单 job 墙钟、原始核时、**GitHub Actions 跨平台折算计费分钟（Linux `1×`、Windows `2×`、macOS `10×`）**与触发频率。不为压缩单一指标而明显拉长关键路径墙钟或制造隐性重跑。
2. **严守 `policy_sha` 与 `tested_sha` 权威隔离及 fail-closed**：PR 门禁计划器、结果判定器、缓存写权限和信任锚点必须受控于受信基线（`policy_sha` / `ci-authority`）；候选代码（`tested_sha`）不得在 PR 中自授豁免、覆盖门禁脚本或污染共享缓存。无法静态证明等价覆盖时一律 **fail-closed** 回退保守路径。
3. **混合版本窗口平滑切换（`*_version` 门控与受信红线前置）**：凡改变 job 拓扑、产物契约、结果判定或工作流摘要的优化，优先用单调递增的能力版本号（`*_version`）双向门控；受信基线测试只写单调边界不变量而不锁演进态精确快照，`EVOLVING_POLICY_TESTS`（如 `ci-workflow.test.py`、`plan-rust-required-scope.test.py`、`ci-bin-ownership.test.py`、`ci-full-result.test.py`）在 `policy_sha` 侧仅强制执行 `FIXED_TESTS` 红线子集，常规 DAG/投影/拆迁断言可随实现 **同单 PR 原子更新**，仅当触及 `policy_sha` `FIXED_TESTS` 红线或无全量 `migration` 的非演进契约套件时才拆 `PR-A -> PR-B` 双态前置（且不在 PR-A 提前写入依赖 PR-B 的新断言）。始终保留 `periodic-full` / `candidate-full` 等显式全量证明边界不被快速门禁裁剪侵蚀。
4. **设计先行、同步清理流程惯性、本地闭环与共享控制面单飞**：调整门禁能力或分层策略前，先同步受影响设计文档（及适用 `O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29` 追踪说明），并同批检索刷新 `.agents/skills/`、`AGENTS.md` 与脚本注释中已过时的多步绕行口诀，防止门禁解除后 Agent 仍惯性空转拆单。工作流与核心控制脚本修改通常触发远端 `scope=full`，须先在本地跑通控制单测、**`policy_sha` `FIXED_TESTS` 跨版本仿真**与 PR 边界检查后再成批推送；多子项触碰共享控制面文件（`rust.yml`、`ci-required-scope.json`、`plan-rust-required-scope.py`、`ci-workflow.test.py`）时可在本地预备与预审，但远端同时保持至多 **1 个活跃开放 PR**，避免自我变基冲突与双倍全量算力浪费。

## 3. 通用验证与交付要求

- **实施 CI 代码/工作流改动时**：按所选分册运行对应控制脚本单测（如 `ci-contract-jobs.py --check`、`ci-workflow.test.py`、`plan-rust-required-scope.test.py`、`ci-full-result.test.py` 等）、`./scripts/check-pr-boundaries.sh --slice <Slice>` 与 `python3 scripts/check-local.py --profile quick --base-ref origin/main`；涉及门禁判定、权限或信任边界的修改按 [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md) 完成独立评审。PR 标题或正文显式声明 `Slice-Type: <Type>`（或 `[Slice: <Type>]`），写清基线 run ID、优化前后指标对比与混合版本生效说明；容量/频率类优化合入后按 [容量预算与取消浪费分册 §3](references/capacity-budget-and-cancel-waste.md#3-合入后-1224-小时生产遥测验收闭环) 复测 12–24h 真实遥测。
- **扩展或修改本 Skill 分册时**：
  - 保持主入口 `SKILL.md <= 300` 行，具体套路、脚本接缝与踩坑案例下沉到 `references/<topic>.md`；
  - 新增分册须同时登记到 frontmatter `file_names`、上表路由及下方 `## Supporting Files`；
  - 运行 `./scripts/lint-skills.sh`、`./scripts/doc-governance-check.sh`、`./scripts/check-pr-boundaries.sh --slice Docs`、`python3 scripts/check-local.py --profile quick --base-ref origin/main` 和 `git diff --check`。

## Supporting Files

- `references/job-count-reduction.md`：减少 CI Job 数量、归并短合约、零检出门禁直验与收敛 `scope=full` 误触发的实证方法与防坑清单。
- `references/on-demand-scope-and-preflight.md`：清理陈旧包映射与冗余构建准备、解绑连带触发与多平台子契约二维切片、`fast-preflight` 全覆盖与同源检查安全去重的实证方法与防坑清单。
- `references/capacity-budget-and-cancel-waste.md`：取消浪费三分法归因、`main-fast-admission` 主干冷却预算与延后标记回溯隔离、以及合入后 12–24h 生产遥测复测闭环。
- `references/trusted-baseline-and-input-governance.md`：受信基线单调边界不变量设计、受管目录与结构化排他输入路由、`FIXED_TESTS` / `EVOLVING_POLICY_TESTS` 分层演进判据与流程惯性清理。

## Related methods

- [executing-project-tasks](../executing-project-tasks/SKILL.md)
- [verification-before-completion](../verification-before-completion/SKILL.md)
- [requesting-repo-owned-review](../requesting-repo-owned-review/SKILL.md)
- [systematic-debugging](../systematic-debugging/SKILL.md)
- [evaluating-development-methods](../evaluating-development-methods/SKILL.md)

## Known Failure Modes

- 未核实关键路径就把多个中重型检查塞入同一 job，或忽视 macOS `10×` / Windows `2×` 计费倍率，导致单次等待墙钟拉长或跨平台折算计费失真。
- 直接删除旧 job、向 `required-gate` `needs:` 塞入旧版 `scope=full` 下会 `skipped` 的 fallback job，或单 PR 强改被 `policy_sha` `FIXED_TESTS` / 非演进契约锁死的不变量（以及把本可随 `EVOLVING_POLICY_TESTS` 单 PR 原子演进的普通拓扑改动无必要地拆成微 PR），导致首跑判红或冗余全量重跑。
- 在受信基线测试中把演进中模块的依赖/路径写成精确集合快照、对同类新增文件反复叠加单路径硬编码补丁，或在松绑门禁后漏刷新 `.agents/skills/` 旧绕行口诀，导致前置微 PR 持续再生。
- 只改一个工作流而忽略双处投影列表同步（`workflow-plan` 与 `&validate-complete-selection`）或跨脚本静态结构测试（如 `baseline` 产品文档范围环境变量不覆盖约束、Rust 缓存配对 `if:`、多行 `if: >-` 子串断言、旁路工作流 `paths-ignore` 解析假设），直到远端全量运行才暴露。
