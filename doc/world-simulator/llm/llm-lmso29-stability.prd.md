# oasis7 Simulator：LMSO29 可用性与稳定性收敛（设计文档）

历史运行边界：2026-02-11 的已实施、回归观察和 LMSO29.1 下一动作均为原窗口记录；原 Task 与 `.tmp` 原始输出的长期可取回性未核实，故保留原文，不当作当前排期、今日 PASS 或自动复跑授权。预算、协议兼容、测试方法与 builtin 输入保护合同继续保留。

- 对应设计文档: `doc/world-simulator/llm/llm-lmso29-stability.design.md`
- 专题入口与权威边界: `doc/world-simulator/llm/README.md`

本专题维护预算调优与稳定性要求。当前必需观测、目标、最新查询结果的保护和
不可容纳时的停止行为由 [builtin 输入合同](decision-provider-contract.prd.md#builtin-input-integrity)
及其 [请求设计](decision-provider-contract.design.md#builtin-request-contract) 定义；
阈值放宽与摘要压缩必须遵守该合同，历史稳定性样本不证明当前请求已保留这些输入。

审计轮次: 5

## 1. Executive Summary
- 在保持链路可用（`llm_errors=0`、`parse_errors=0`）前提下，回收 LMSO28 后的策略稳定性波动。
- 允许适度放宽 token 预算限制，减少过度压缩导致的动作决策信息不足。
- 保持多轮决策链路（plan/module_call/decision/execute_until）兼容，不引入协议破坏性变更。

## 2. User Experience & Functionality
- In Scope：
  - 放宽 Prompt 预算收敛阈值（soft/hard reserve 与 soft section cap）。
  - 放宽 Prompt 内容压缩上限（module history args/result、memory digest、observation 注入规模）。
  - 基于 `llm_bootstrap` 场景执行 30 tick 回归，验证链路可用性与稳定性指标。
- Out of Scope：
  - 模型与 provider 切换。
  - 决策协议字段新增/删减（JSON schema 不变）。
  - world kernel 业务规则改造。

## 3. AI System Requirements (If Applicable)
- N/A: 本专题不新增 AI 专属要求。

## 4. Technical Specifications
- 主要改动接口：
  - `LlmPromptProfile::prompt_budget()`：调整输入预算有效空间。
  - `PromptAssembler::peak_targets_tokens()`：调整 peak soft/hard 目标。
  - `PromptAssembler::apply_peak_budget()`：调整软段压缩上限。
  - Prompt 素材压缩常量：`PROMPT_MODULE_*`、`PROMPT_MEMORY_*`、`PROMPT_OBSERVATION_*`。
- 验证数据：
  - `.tmp/<run>/report.json`（核心指标对比）。
  - `.tmp/<run>/run.log`（逐 tick 现象与异常定位）。

## 5. Risks & Roadmap
- M1：完成预算/压缩阈值放宽改造与单元测试。
- M2：完成 30 tick 回归并对比 LMSO28 基线，确认链路稳定性不退化。
- M3：更新项目文档、devlog 并提交。

### Technical Risks
- **放宽过度风险**：输入体积反弹导致峰值再次失控；通过 30 tick 指标阈值与对比回归控制。
- **样本波动风险**：单次 run 的动作成功率存在随机波动；通过固定场景同口径对比，先验证链路稳定再逐步扩样。
- **兼容风险**：放宽预算后 prompt 结构变化可能触发边缘解析分支；通过现有 parser/repair 单测回归兜底。

## 已实施与验证（2026-02-11）
- 已实施：
  - 适度放宽 `PromptBudget` 有效输入空间（降低 safety margin；保留输出预留不变）。
  - 新增并校验 peak/soft cap 相关单测，确保预算逻辑可回归。
  - 在 prompt 约束中增加 `move_agent.to` 不得指向当前位置的显式规则，降低无效移动概率。
- 已验证（命令）：
  - `env -u RUSTC_WRAPPER cargo test -p oasis7 prompt_budget_peak_targets_use_relaxed_reserve_values -- --nocapture`
  - `env -u RUSTC_WRAPPER cargo test -p oasis7 llm_prompt_profile_uses_relaxed_token_budget_for_stability -- --nocapture`
  - `env -u RUSTC_WRAPPER cargo test -p oasis7 llm_agent_prompt_contains_execute_until_and_exploration_guidance -- --nocapture`
  - `env -u RUSTC_WRAPPER cargo check -p oasis7`
  - `env -u RUSTC_WRAPPER cargo run -p oasis7_provider_tools --bin oasis7_llm_agent_demo -- llm_bootstrap --ticks 30 --report-json .tmp/lmso29_final_30_promptfix/report.json`

## 回归观察
- 对比 LMSO28 基线（`.tmp/lmso28_peak_30_final/report.json`）：
  - 稳定性主链保持：`llm_errors=0`。
  - 存在轻度波动：`parse_errors` 与 `repair_rounds_total` 偶发非零。
  - `action_success` 暂未回收至 LMSO28 水位，且输入体积指标有回升，说明“放宽 token + 当前策略”仍需二次收敛。

## 后续优化点（LMSO29.1）
- 先收敛动作失败（特别是 move 失败模式）再扩大 token 放宽范围。
- 引入“多次 30 tick + 固定统计口径”的验收方式，降低单次采样偏差影响。
- 在保持 `llm_errors=0` 的前提下，逐步回收 `llm_input_chars_max` 与 `module_call` 波动。

<a name="6-validation-decision-record"></a>

## 6. Validation & Decision Record

- 本文保留 prompt-budget、协议兼容和稳定性约束；历史任务状态、运行
  产物与执行证据由 PR、实际验证记录和 Git history 管理。
- 当前输入完整性按 [builtin 确定性验收](decision-provider-contract.prd.md#ac-builtin-input-integrity)
  检查最终 Responses 请求及配置不足时的调用阻断；本专题的 peak/soft-cap 单测和
  稳定性统计分别保留其证明范围，不替代该验收。
- 扩大上下文预算只是一项受测的稳定性调整，不证明模型/provider 切换、
  长时成本、无漂移行为或 release readiness。
