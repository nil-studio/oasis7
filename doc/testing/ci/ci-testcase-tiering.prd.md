# oasis7: CI 测试分级细化到 Test Case

- 对应设计文档: [CI 分级执行共享设计](ci-tiered-execution.design.md#ci-case-tier-design)；原同名 design 仅保留兼容入口。
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

审计轮次: 4


## ROUND-002 口径归属（2026-03-05）
- 本文档是 `test_tier_required` / `test_tier_full` 标签语义与分配规则的权威入口。
- 其它文档只引用标签语义，不重复描述 case 标签边界；命令级 `commit` / `required` / `full` 触发策略以 `ci-tiered-execution.prd.md` 为准。

## 1. Executive Summary
- Problem Statement: 仅按“整套 `cargo test`”设置门禁粒度会拉长反馈链路，难以在 required 阶段快速定位关键回归。
- Proposed Solution: 将 CI 分层细化到 test case 级别，使用 `test_tier_required`/`test_tier_full` 标签驱动 `required` / `full` 执行路径，并让默认 `commit` baseline 保持命令级轻量门禁。
- Success Criteria:
  - SC-1: `required` 门禁聚焦最小 smoke case 且执行时长下降。
  - SC-2: `full` 继续覆盖重型特性与联测，保持回归深度。
  - SC-3: `scripts/ci-tests.sh` 不再依赖硬编码 `--test` 清单。
  - SC-4: 测试分级策略在文档、脚本、门禁三处保持一致。

<a id="case-tier-requirements"></a>
## 2. User Experience & Functionality
- User Personas:
  - CI 维护者：需要可维护的分级执行模型。
  - 开发者：需要更快获得 required 反馈。
  - 发布负责人：需要 full 回归不缩水。
- User Scenarios & Frequency:
  - 日常本地提交：不执行验证；`commit` baseline 仅可显式调用。
  - PR 门禁：每次执行 required 门禁。
  - 分支合并前回归：高风险变更执行 full。
  - 策略维护：测试新增/迁移时同步更新标签与脚本。
- User Stories:
  - PRD-TESTING-CI-TIER-001: As a CI 维护者, I want case-level tier tagging, so that required/full boundaries are explicit.
  - PRD-TESTING-CI-TIER-002: As a 开发者, I want required checks to run only critical smoke cases, so that feedback is faster.
  - PRD-TESTING-CI-TIER-003: As a 发布负责人, I want full regression paths preserved, so that release risk remains controlled.
- Critical User Flows:
  1. Flow-TIER-001: `为测试打标签 -> required 执行 --tests + feature 过滤 -> 输出门禁结论`
  2. Flow-TIER-002: `触发 full -> 运行扩展回归（wasmtime/libp2p/viewer）-> 汇总结果`
  3. Flow-TIER-003: `新增测试 -> 选择 required/full 标签 -> 更新文档与脚本 -> 验证一致性，并确认是否需要进入默认 commit baseline`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 测试分级标签 | `test_tier_required` / `test_tier_full` | 用 `#[cfg(feature = ...)]` 标记用例 | `unlabeled -> labeled -> validated` | required 为最小闭环，full 追加重型场景 | 测试维护者审核分级 |
| 命令级基线分流 | `commit` / `required` / `full` | 普通 commit 不验证；显式命令可执行 `commit`，PR/CI 执行 `required`，重型回归执行 `full` | `queued -> running -> passed/failed` | commit 与验证解耦，case 标签用于门禁分层 | 开发者/CI 可触发 |
| required 执行路径 | `scripts/ci-tests.sh required` | 执行静态门禁 + `--tests` + required feature | `queued -> running -> passed/failed` | 优先速度与关键覆盖 | 开发者按需触发，CI 自动触发 |
| full 执行路径 | `scripts/ci-tests.sh full` | 在 required 基础上执行扩展回归 | `queued -> running -> passed/failed` | 覆盖优先于耗时 | 发布前必须通过 |
- Acceptance Criteria:
  - AC-1: `required` 由 feature 标签筛选 smoke case，不再硬编码 `--test` 清单。
  - AC-2: `full` 保持 `libp2p`、`wasmtime`、viewer 联测路径。
  - AC-3: 文档明确“`commit` 命令级轻量基线 + `required/full` case 级筛选”策略。
  - AC-4: 回归验证与任务日志完整可追溯。
- Non-Goals:
  - 不做 changed-files 动态选测。
  - 不做 smoke case 自动学习/自动生成。
  - 不修改业务测试断言语义。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本任务为 CI 测试分层治理）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 在既有 `commit` / `required` / `full` 门禁框架内，把较重回归的 case 选择逻辑从脚本硬编码迁移到测试标签，同时让默认 `commit` baseline 保持不依赖 `oasis7 --tests` heavy shard 的命令级轻量路径。
- Integration Points:
  - `scripts/ci-tests.sh`
  - `.github/workflows/rust.yml`
  - `scripts/pre-commit.sh`
  - Rust 测试 feature 标签：`test_tier_required`、`test_tier_full`
- Edge Cases & Error Handling:
  - 标签覆盖不足：required 可能漏测关键路径，需根据缺陷复盘持续扩容。
  - 标签/脚本不同步：标记存在但门禁未执行时，必须同步修正脚本与文档。
  - full 路径误删：若重型回归被意外移除，发布门禁立即阻断。
  - 迁移期混用旧策略：硬编码清单与 feature 过滤并存时，先去重再切换。
- Non-Functional Requirements:
  - NFR-TIER-1: required 门禁反馈时间较旧策略显著下降（趋势可观测）。
  - NFR-TIER-2: full 回归覆盖项不低于迁移前。
  - NFR-TIER-3: 标签与脚本一致性检查可追溯并可复现。
- Security & Privacy: 仅涉及测试执行策略，不引入新数据面。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (T1): 设计与实际 PR 验证记录对齐；Issue/Project 按跟踪需要使用。
  - v1.1 (T2): 脚本改造到 case 级 required 筛选并移除硬编码清单。
  - v2.0 (T3/T4): 回归验证、文档回写、巡检与规则同步。
- Technical Risks:
  - 风险-1: smoke 清单过窄形成覆盖盲区。
  - 风险-2: feature 标签与脚本策略漂移导致门禁失真。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-TESTING-CI-TIER-001 | T1/T2 | `test_tier_required` | 标签接线与脚本参数验证 | CI 分级可维护性 |
| PRD-TESTING-CI-TIER-002 | T2/T3 | `test_tier_required` | required 执行与反馈链路验证 | 开发反馈时延 |
| PRD-TESTING-CI-TIER-003 | T2/T4 | `test_tier_full` | full 回归路径完整性检查 | 发布回归深度 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-TIER-001 | 用 feature 标签筛选 test case | 长期维护硬编码 `--test` 清单 | 标签方案可扩展且维护成本更低。 |
| DEC-TIER-002 | 保留 full 重型回归不变 | 与 required 同步削减 | 发布风险不可接受。 |
| DEC-TIER-003 | 策略变更同步文档/脚本/任务日志 | 只改脚本不回写文档 | 容易产生口径漂移。 |

## 原文约束点映射（内容保真）
- 原“目标（required 更细化 + full 保持）” -> 第 1 章与第 2 章 AC。
- 原“In/Out of Scope” -> 第 2 章 AC 与 Non-Goals。
- 原“接口/数据（入口命令、required/full、标签）” -> 第 4 章 Integration Points。
- 原“里程碑 T1/T2/T3”与巡检 -> 第 5 章 Phased Rollout。
- 原“风险（覆盖盲区、标签不同步）” -> 第 4 章 Edge Cases + 第 5 章 Risks。
