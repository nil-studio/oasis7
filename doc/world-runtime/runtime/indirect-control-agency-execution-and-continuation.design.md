# Indirect Control Agency Execution and Continuation — Runtime System Design

- 设计 ID：`DES-WR-IA`
- 状态：`active`
- Owner role：`runtime_engineer`
- 专业共同审读：`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`、`qa_engineer`
- 对应专业需求：[`PRD-GAME-014`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#sc-7)
- 上游产品要求：[`REQ-WR-IA-001`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)、[`REQ-WR-IA-002`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002)、[`REQ-WR-IA-003`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003)
- 补充产品 authority：[`Agent 自治、委托与责任连续性`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#agent-delegation-boundary)；本设计只承接其技术边界，不取代该 PRD、资产经济 authority 或玩法/Viewer 专业合同。
- 审读基线：本文只定义长期系统设计与验证方法；实现现状、候选提交、测试结果和任务状态必须从关联 PR、实际 CI 与评审记录读取。
- Last reviewed：2026-10-09

本文把玩家意图、Agent 解释、runtime 权威执行、Viewer/API 投影和 QA 证据组织成一条可追踪的间接控制因果链。产品 PRD 拥有玩家承诺与 REQ/AC，`PRD-GAME-014` 拥有玩法 guarantee、字段语义与失败签名，本文拥有跨组件技术边界、状态映射、恢复合同和验证设计；不复制产品要求、任务台账或当前完成度。

补充的 Agent authority 承接范围是授权来源与范围复核、有效期内累计额度、控制权变化后对未生效行动的重新评估，以及异议/override 到权威结果的因果归因。runtime 需要持久化这些事实的最小身份、版本和引用以支持幂等提交与恢复；玩家/组织授权语义、组织治理规则、经济条件和责任含义仍由产品与玩法 authority 冻结。玩家/组织授权不等同 WASM capability grant；本文定义共享因果链如何关联现有 intent 与领域结算 receipt，不取代领域 receipt，也不建立并行的 player-facing receipt schema、状态机或授权来源。

## 1. 问题、目标与非目标

### 1.1 问题与目标

间接控制同时跨越产品、gameplay、runtime、Agent、Viewer/API 与 QA。若缺少系统设计层，产品要求只能直接跳到实现或任务，容易出现 `accepted` 被误写成 `applied`、各入口自行推导状态、回流只恢复日志、记忆纠正无法关联后续结果等漂移。

本设计目标是：

- 用稳定的 `DES-WR-IA-*` 条款承接产品 REQ/AC 与 `PRD-GAME-014`；
- 让 Agent 自治、授权、转让后重配置、异议/override 与责任连续性产品条款拥有明确的 runtime 设计与验证映射；
- 明确每个事实的 producer、authority、consumer、持久化与恢复边界；
- 让 Viewer 与 pure API 从同一权威事实投影四类 invariant：accepted intent、execution status、primary reason、next step；
- 将要求、设计条款、验证方法和实际 PR 验证记录串成可查询链路。

### 1.2 非目标

- 不改变产品的间接控制方向，不新增第一人称逐帧控制承诺。
- 不在本文冻结具体 UI 布局、API payload、runtime enum、Agent prompt/model/provider 或记忆算法。
- 不把 Agent 输出、请求接受、队列入列、界面计数或 world tick 单独当作权威世界效果。
- 不记录排期、分支、HEAD、CI 结果、当前 verdict 或发布状态；这些属于 Git、PR 与 QA 验证记录。
- 不以文档建档证明实现完成、跨入口 parity 或 release readiness。

### 1.3 裁剪说明

本主题跨 runtime、Agent、Viewer/API、持久化、恢复与验证边界，适用完整十二段系统设计视图。安全与容量不新增数值目标，但保留权限、隐私、资源与未验证边界。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / acceptance | 具体 obligation | 本设计条款 | 外部 owner / dependency | 明确排除 |
| --- | --- | --- | --- | --- |
| [`REQ-WR-IA-001`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | 接受与权威世界生效保持可区分 | [DES-WR-IA-001](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-001) | gameplay / runtime / Agent | 不规定具体 schema 或 UI |
| [`AC-WR-IA-001`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#ac-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | 接受、推进、后果和下一步保持可区分 | [DES-WR-IA-001](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-001) | runtime / Viewer / QA | 不把 ack、接受或 Agent 推断写成世界效果 |
| [`REQ-WR-IA-002`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002) Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | 阻塞事实、复查触发与 fallback 分类由权威状态产生 | [DES-WR-IA-003](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-003) | runtime / Agent / Viewer | 不新增 fallback 选项或阈值 |
| [`AC-WR-IA-002`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#ac-wr-ia-002) Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | 阻塞、回流与重新决策保留有效恢复动作 | [DES-WR-IA-006](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-006) | runtime / Agent / Viewer | 不承诺失效意图继续恢复 |
| [`REQ-WR-IA-003`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003) Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | 关键决策的理由、stakes、替代、纠正和结果处于同一因果链 | [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | Agent / runtime / Viewer / QA | 不暴露私有 prompt 或内部 trace |
| [`AC-WR-IA-003`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#ac-wr-ia-003) Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | 关键决策的原因与纠正结果可追溯 | [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | Agent / runtime / Viewer / QA | 不暴露私有 prompt 或内部 trace |
| [`PRD-GAME-014 AC-3B`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#ac-3b) Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | fallback 选项比较成本、进度、机会成本与推荐理由；safe-wait quote 的触发失效时重新分类 | [DES-WR-IA-003](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-003) | gameplay / runtime / Viewer / QA | 不改变 PRD 现有 option 与时间窗口 |
| [`PRD-GAME-014 AC-3C`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#ac-3c) Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | fallback 选项比较成本、进度、机会成本与推荐理由；safe-wait quote 的触发失效时重新分类 | [DES-WR-IA-003](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-003) | gameplay / runtime / Viewer / QA | 不改变 PRD 现有 option 与时间窗口 |
| [`PRD-GAME-014 AC-13`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#ac-13) Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | 单一语义因果链含 accepted intent、reason/evidence、stakes/expected consequence、alternative、correction/earliest effect 与结果/reason | [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | gameplay / Agent / runtime / Viewer / QA | 复用领域结算 receipt；不新增第二套 receipt schema |
| [`PRD-GAME-014 receipt scope`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#21-reusable-causal-decision-receipt) Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | receipt 覆盖通用 action intent/reason/stakes/alternative/correction/result 语义 | [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | gameplay / Agent / runtime / QA | 不重新拥有 gameplay 字段裁决 |
| [`PRD-GAME-014 SC-7`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#sc-7) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185; O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | 工业 gather/refine/move/schedule 改道、裁剪、等待或拒绝时保留原 intent、替换/阻断原因、expected/actual consequence、成本/进度与恢复面 | [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | gameplay / runtime / Agent / Viewer / QA | debug/probe 不构成玩家后果或正式样本 |
| [`REQ-AGENT-AUTH-001`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | Runtime 唯一裁决本地 signed delegation，旧 nonce 只读匹配且不扩大 profile | [owner delegation result/replay](indirect-control-agency-execution-and-continuation.design.md#owner-delegation-result-replay-contract) | runtime / Viewer host authority | 不新增 canonical transport、组织授权或动作支持 |
| [`REQ-AGENT-AUTH-001`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | 高后果授权明确 issuer/source、范围、期限与累计余额复核 | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | producer / gameplay / Agent / runtime / QA | 不新增组织治理规则或跨 grant 总额 |
| [`AC-AGENT-AUTH-001`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | 同一 grant/source/object/period 的累计额度不因拆分、并发、重试、重连或切换复制 | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | producer / gameplay / runtime / QA | 不从 Agent/WASM permission 推导授权 |
| authority PRD §6.2 [`AC-1`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-1) | 高自治与有界授权遵循同一世界、资源、权限和治理边界 | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | producer / Agent / runtime | 不新增系统权限 |
| authority PRD §6.2 [`AC-2`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-2) | 超范围、失效授权与 hard block 不产生高后果效果 | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | producer / runtime / QA | 组织角色资格沿既有治理授权 |
| authority PRD §6.2 [`AC-9`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-9) | 高后果额度在授权有效期内累计、提交扣账至多一次 | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | runtime / gameplay / QA | 不在独立 grants 间全局聚合 |
| authority PRD §6.2 [`AC-3`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-3) | 团队扩张消费现有 asset economy，不静默复制授权或责任 | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | gameplay / Agent / runtime | 不定义团队经济、容量或平衡 |
| authority PRD §6.2 [`AC-7`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-7) | 工业供给或治理 quota 不静默扩大授权与责任边界 | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | gameplay / runtime / Agent | 不定义工业经济或 quota 公式 |
| [`AC-WR-AOS-004`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004) Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 正式工业供给受玩法资源、产能与交付合同约束，不由 debug 注入替代 | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | gameplay / runtime / QA | 本文不拥有工业经济或产能规则 |
| [`REQ-AGENT-AUTH-002`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002) Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | 转让、重配置和处置保留身份、来源、审计历史与已生效结果 | [DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | producer / gameplay / Agent / runtime / Viewer / QA | 不定义市场或经济条件 |
| [`AC-AGENT-AUTH-002`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-agent-auth-002) Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | 新策略从转让生效点起作用，不改写既有因果与责任 | [DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | gameplay / Agent / runtime / Viewer / QA | 不把旧 owner grant 复制给新 owner |
| authority PRD §6.2 [`AC-4`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-4) | 转让后身份、来源、配置与决策历史可追溯 | [DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | gameplay / Agent / runtime / QA | 不定义转让经济 |
| authority PRD §6.2 [`AC-8`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-8) | 撤销、到期、转让或新硬边界后 pending action 重评或终止，历史不变 | [DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | gameplay / Agent / runtime / QA | 不静默沿用旧授权或重放 |
| [`AC-WR-AOS-005`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-005) Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | 经济转让通过资格、容量、资源、治理、反滥用五门槛且无部分效果 | [DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | gameplay / runtime / QA | 转让本身不授予额外自治 |
| authority PRD §6.2 [`AC-5`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-5) | 区分 Agent objection、有效 owner override 与 world/safety hard block | [DES-WR-IA-012](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-012) | producer / Agent / runtime / QA | override 不越过硬边界 |
| authority PRD §6.2 [`AC-6`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-6) | 归因适用 grant source、owner/organization、Agent、实际载体和结果 | [DES-WR-IA-012](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-012) | producer / Agent / runtime / Viewer / QA | 产品责任语义不等于法律责任制度 |
| [`REQ-AGENT-AUTH-001`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | 已采纳 native 与合同 mandate 分别消费真实来源；原累计账、当前读面和控制变化保持同一语义 | [已采纳授权 profile](indirect-control-agency-execution-and-continuation.design.md#agent-authority-adopted-profiles) | 原 owner control、普通组织签约政策、各合同 family、Agent / Viewer / API | 不把合同签约附件扩大为未声明动作权限，不新增授权账 |
| [`REQ-AGENT-AUTH-002`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002) Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | 处置生效后当前控制与旧配置、记忆 provenance、责任历史及当前读取权限明确分开 | [处置控制与历史消费](indirect-control-agency-execution-and-continuation.design.md#agent-disposition-control-history-consumer) | Agent 经济处置 producer、task/Prompt/identity 与私有内容 authority、Web / native / API | 不把旧私有内容、凭据或账户自动交给新 owner，不删已生效结果 |
| [`REQ-WR-AOS-005`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005) Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | 仅消费五门槛全部通过后的唯一处置结果；新控制、旧 pending、费用/责任与历史引用同根 | [处置控制与历史消费](indirect-control-agency-execution-and-continuation.design.md#agent-disposition-control-history-consumer) | 原经济转让/处置事务与 G1、当前 owner resolver | 不重新定义价格、五门槛或退休/报废经济规则 |

角色边界：`producer_system_designer` 维护跨域取舍与产品授权/责任语义；`gameplay_designer` 维护 guarantee、资产经济和失败签名；`runtime_engineer` 维护 canonical 状态、校验、提交、receipt、回放和恢复；`agent_engineer` 维护解释、计划与记忆处理；`viewer_engineer` 维护玩家表面投影；`qa_engineer` 维护实际候选的验证与阻断判断。组织代表权和 Agent 授权的具体凭据/规则由其专业 owner 决定，不从 WASM module grant 推导。

## 3. 当前状态、目标状态与差距

| 对象/能力 | 当前状态（基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| 产品/玩法要求 | 已有产品 REQ/AC 与 `PRD-GAME-014` guarantee、字段和失败签名及本设计映射 | 稳定映射到设计条款 | 映射不代替各要求的实际组合验收 | product PRD / gameplay PRD |
| runtime authority | 已有 Agent cognition 生命周期设计，声明 runtime 拥有 canonical transition，Agent 输出是 data | 所有 agency 投影都引用权威状态与 receipt | 各具体玩家意图路径的接线需逐候选验证 | [`agent-cognition-lifecycle.design.md`](../../world-runtime/runtime/agent-cognition-lifecycle.design.md#1-boundary-map) |
| Agent 解释与记忆 | 已有 owner-signed memory correction、earliest decision、matching committed receipt 与 lineage reload 接线 | 理由、记忆使用、纠正与 earliest effect 接入统一 receipt | 真实 provider、全部意图类型和跨版本恢复仍需组合验证 | `llm_sidecar_memory_correction.rs`; `agent_memory_correction_control.rs` |
| Viewer / pure API | owner-filtered agency read model 已进入 player snapshot、signed Inspect 与 pure API agency-control；Web Control Proof 消费该投影 | 两入口表达同一四类 invariant | 共享 DTO 不替代真实 transport/browser 组合验证 | `player_agency_projection.rs`; `control_proof_panel.jsx` |
| Agent authority 产品追踪 | native lane 已有 starter owner grant、累计电力额度、effect 前复核与 one-shot owner override | DES-WR-IA-010..012 将产品 authority 条款连到 runtime 校验、待决行动复核和可审计因果结果 | 组织 issuer、其他资源/权利/风险额度与完整经济转让仍是目标；不从局部实现推断完成 | `agent_delegation.rs`; `agent_delegation_controls.rs` |
| 验证 | PRD 已定义 required/full 方向 | 每个 DES 条款有稳定验证方法，实际结果绑定 PR 版本 | 同候选组合证据 pending | qa_engineer / PR 验证记录 |

“目标状态”是设计合同，不等于当前实现。只有固定候选、环境与实际 evidence 同时存在时，某条能力才可被报告为 proven。

### 3.1 已核对的当前 runtime 事实

- 当前 canonical intent 是 [`AgentIntentV2`](../../../crates/oasis7_world_state/src/agent_cell.rs)，由 [`DomainEvent`](../../../crates/oasis7_world_state/src/events/domain_event.rs) 驱动 proposal、submission、acceptance、transition 和 replacement；具体状态与校验仍以源码和 runtime authority 为准。
- retry identity、request digest 与 replacement 校验见 [`agent_intent.rs`](../../../crates/oasis7_runtime/src/world/agent_intent.rs)：身份绑定玩家、Agent、世界、reorg、authority 和 replacement 上下文；相同 durable digest 可重放，冲突 fail closed，新意图替换 active intent 时必须精确引用被替换 intent。
- completion 校验见 [`agent_intent_terminal.rs`](../../../crates/oasis7_runtime/src/world/agent_intent_terminal.rs)：需要 accepted intent、完整 authority tuple、绑定 effect intent 以及先前已提交的匹配 receipt；provider/UI ack 不能产生 completion。
- 当前 Viewer 投影见 [`gameplay_snapshot.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot.rs)：`accepted_intent_id` 可能来自 player-facing feedback action，不能与 canonical `AgentIntentV2.intent_id` 或 receipt identity 互换；任何映射都必须显式、版本化并验证。
- Viewer 与 pure API 的共同投影骨架见 [`PlayerGameplaySnapshot`](../../../crates/oasis7/src/simulator/persist.rs) 与 [`oasis7_pure_api_client.rs`](../../../crates/oasis7_client_cli/src/bin/oasis7_pure_api_client.rs)。Web 本地 queued/error/gate feedback 只能作为 provisional overlay，不能覆盖 runtime snapshot 的 accepted/applied/world-effect 真值；`logicalTime`、`eventSeq` 也只能由 runtime snapshot/event 推进。
- [`control_blocking.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_blocking.rs) 的 completion tuple 只证明 canonical intent 与 matching effect receipt；当前完整解释投影来自 [`player_agency_projection.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/player_agency_projection.rs) 的 `AgentCausalReceiptV1` 引用读模型，包含 stakes、alternative、expected/actual consequence、correction refs、authorization 和 hard boundary。它引用已有 domain events，不替代领域结算 authority。[`Control Proof`](../../../crates/oasis7_viewer/software_safe_src/control_proof_panel.jsx) 将 Agent 预测标为未验证解释，并分开呈现 accepted correction 与 committed decision result；字段存在不证明所有意图或真实 provider/browser 闭环已验收。
- 可复用的 cognition commit/recovery 接缝是 [`WorldCommitRecordV1`](../../../crates/oasis7_world_state/src/cognition_recovery.rs) 与 [`cognition_persistence.rs`](../../../crates/oasis7_runtime/src/world/cognition_persistence.rs)：它们绑定 cognition envelope、staged root 与 receipt 并恢复已提交前缀；这不自动证明所有工业/治理 domain settlement 都已接入该路径。共享 receipt 是引用各领域权威结算证据的语义读模型。

### 3.2 当前 native lane 与目标合同的边界

- 当前 owner source 是 `starter_oc_claims`；issuer 必须等于当前 owner，并精确绑定 claim source。组织成员关系不等于组织代表授权；现有 native lane 缺少组织 issuer resolver，返回 `organization_source_unsupported`，不能凭组织 ID 或 WASM capability 开通。
- 当前额度只使用 native electricity accounting，支持 `MoveAgent`（零电力报价）、`TransferMaterial`（native tariff）、`ScheduleRecipe`（accepted native plan power）与受限的 `BuildFactory`（可信 construction power profile 的建设电力义务）。其他 action（包括尚未接入此 lane 的 module-backed action）、resource/right source 与 risk accounting 仍保持拒绝；扩大范围必须接入对应领域 authority 与原子 effect/debit/receipt，而不是仅放宽 allowlist。
- `BuildFactory` 要求有效且绑定目标工厂的 construction power profile、按其 kind 重算并精确匹配的 canonical factory spec，以及匹配的 factory profile；仍须通过 grant 的 action/object scope、有效期与累计 electricity budget 复核和建设领域准入。已提交的 `FactoryBuildStarted` 是开工及扣账依据，不等于工厂建成。正式客户端的内置 Smelter/Assembler、已授权场地与位置、报价和原请求恢复边界见 [oasis7-play Skill](../../../skills/oasis7-play/SKILL.md)及[建厂到首批生产切片](../../world-simulator/llm/external-agent-game-api.design.md#当前实施候选建厂到首批生产)；该切片不证明冷启动、Viewer 或全部入口的完整首局。
- Grant 的累计 key 是 grant/source/object/period，revocation、expiry、owner source 与 revision 在 cognition effect commit 和 native action admission 再校验。已有 owner-change fixture 证明旧授权失效，不证明产品要求的完整五门槛经济转让、pending revision event model 或组织责任归因已实现。
- 在有效授权和世界前置条件内，Agent dissent 只是 receipt evidence，不要求玩家额外提交 override 才能执行。显式 override 仍必须来自已认证当前 owner、绑定 exact intent/grant revision 并至多消费一次；若显式声明 override，却缺少或伪造其 evidence，则拒绝效果；授权失效和 hard boundary 仍阻止效果。

## 4. 边界与结构

```text
owner / organization authority
  -> runtime validation + canonical transition (提供授权依据，不直接提交世界)
player intent
  -> Viewer / pure API request surface        (提交与投影，不拥有世界真值)
  -> Agent interpretation / plan              (advisory data，不拥有世界提交)
  -> runtime validation + canonical transition (唯一权威接受、拒绝与应用边界)
  -> world state / journal / receipt           (可回放、可恢复的事实)
  -> Viewer / pure API agency projection       (同源表达)
  -> player interrupt / correction / next decision
```

<a id="des-wr-ia-001"></a>
### DES-WR-IA-001：accepted intent 身份与接受/生效边界

每个受支持的玩家意图必须有可关联的身份。入口可以确认请求已收到或已进入权威处理，但只有 runtime authority 能证明校验、提交或拒绝；`accepted`、`applied`、`persisted`、`published` 必须分别由对应 receipt/state 证明，不能互相推导。Agent 的解释与建议是输入数据，不得直接成为 world authority。

高后果授权路径的身份至少关联 authenticated actor、目标 Agent、canonical intent/request digest、world/reorg/authority context、被替换意图、effect intent 和授权来源/revision。可复用 `AgentIntentV2` 已有 `actor_id`、`agent_id`、`intent_id`、`request_digest`、world/time/scope、`replaces_intent_id` 与 `effect_intent_id`；grant 引用要经 durable digest 或经过验证的 authority-decision reference 绑定，不能从 caller 文本或日志重建。缺少专业 authority 规定的必要身份时保持 unavailable/unsupported 并拒绝该高后果路径；legacy snapshot 可读，但不自动证明 delegation。当前实现中的 canonical intent identity、player-facing feedback action 与投影字段可能是不同身份；消费者必须保留显式映射，不得因为字段名包含 `intent` 就视为同一主键。

<a id="des-wr-ia-002"></a>
### DES-WR-IA-002：execution status、主因果与世界效果

agency surface 必须把玩法语义映射到当前 runtime authority，而不是在本文发明新枚举。`overridden`、`completed_no_progress`、`completed_with_progress` 等 gameplay 分类可以由权威 disposition、世界差异与 receipt 组合推导，但映射规则必须版本化、可测试，并保留原始 authoritative disposition。无可验证世界差异时不得报告 progress。

<a id="des-wr-ia-003"></a>
### DES-WR-IA-003：bounded response、阻塞与 fallback

阻塞事实、复查触发与恢复资格由权威组件产生。表面在 bounded-response 窗口后必须从弱等待升级到 `stalled`、`blocked`、`reprioritize_recommended` 或 `fallback_ready` 等玩法分类。进入 `fallback_ready` 时，现有 wait/repair/reroute 选项要带可比较的成本、保留进度、机会成本和推荐理由；`safe_wait` 必须带完整 `wait_resolution_quote`（resolution trigger、expected wait class、next recheck tick/event、expected state change、unresolved risk、alternative unlock condition）。触发到期而未见预期变化，或前置条件改变时，权威投影重新分类为既有 repair/reroute/no-safe-fallback 类别，不静默续等；没有安全 fallback 时必须返回新的决策面，不能形成静默死端。该合同只映射 `PRD-GAME-014` AC-3B/3C，不新增玩法选项或阈值。

<a id="des-wr-ia-004"></a>
### DES-WR-IA-004：interrupt、reprioritize 与意图交接

新意图替换、裁剪或改道旧意图时，交接必须保留旧意图引用、请求者、原因、接受结果和 earliest effective point。提交 interrupt/reprioritize 只证明请求已进入处理；只有权威结果能证明旧意图已停止或新意图已生效。并发冲突按 runtime authority 拒绝、排序或 supersede，入口不得私自选择赢家。

<a id="des-wr-ia-005"></a>
### DES-WR-IA-005：有界后果可读性

投影层必须能从权威事实解释每个关键结果的 `cost`、`progress/world change`、`primary reason` 与 `next step`。这是足以支持下一决策的摘要，不是完整预测器；原始日志、调试字段和次级事件不能取代主因果，也不能遮蔽无进展。

<a id="des-wr-ia-006"></a>
### DES-WR-IA-006：resume anchor 与回流恢复

回流恢复以可验证 snapshot、journal、receipt 或 continuation identity 为输入，重建最近有效意图、主阻塞、最近后果和下一步。旧意图失效时必须带原因进入重新决策；raw history 只能辅助审计，不能成为恢复 agency 的唯一入口。恢复不得把未提交的本地/UI 状态升级为 canonical truth。

<a id="des-wr-ia-007"></a>
### DES-WR-IA-007：causal-decision receipt 与记忆纠正

记忆驱动、社交、治理、冲突以及工业 gather/refine/move/schedule 恢复决策共用一条语义因果链。对 `PRD-GAME-014 AC-13`，链上至少要关联：canonical accepted intent；Agent reason 与可读 evidence；stakes 和 expected consequence；alternative；interrupt/correction；最早可能生效的权威点；以及纠正后的实际结果或尚未生效、stale/ignored 的可读原因。状态区分 request received/accepted、processing/blocked 与 authoritative applied/not-applied；没有对应领域结算 receipt 时，不得从 accepted、Agent 解释或 projection 推导应用。

这条链复用现有 `AgentIntentV2` identity 与领域结算 receipt 的 durable reference，不另造一套并行世界效果 receipt。实现要能稳定关联 authenticated actor、Agent、intent/request digest、world/reorg/authority context、替换关系、作用域版本、具体 effect intent 和领域 receipt ID/digest；具体 wire schema 可由实现增补，但同一因果主键和校验关系不得分叉。工业 gather/refine/move/schedule 若因资源、地点、世界约束或 guardrail 改道、裁剪、等待或拒绝，保留原 intent、替换动作及原因、成本/进度、expected consequence 与实际结果、primary blocker 和现有 wait/repair/reroute/reprioritize 恢复面。debug/probe 资源注入只能作为测试夹具，不产生玩家奖励、结算结果或正式样本。

Agent authority 决定记忆如何选择、纠正和解释；每个认知 turn 使用带 revision、scope 与 digest 的不可变 `MemoryContextSnapshotV1`，空 memory 也必须是显式快照。`MemoryWriteIntent` 只是 advisory intent，只有与同一 decision identity 和已提交 runtime/domain receipt 匹配时才允许提交 authoritative memory；timeout、stale、rejected、blocked 或 wait 不得提交。纠正 outcome 必须关联 correction identity、接受/拒绝、影响的 memory/decision scope、earliest effective action、实际下一条 memory-driven action 结果，或明确 stale/ignored reason。纠正被接受不保证下一世界动作一定改变；若动作未改变，仍要说明结果和原因。

<a id="des-wr-ia-008"></a>
### DES-WR-IA-008：Viewer / pure API parity、权限与隐私

Viewer 与 pure API 可以使用不同布局和 payload，但对同一候选、身份与时间点，必须从相同 `WorldSnapshot.player_gameplay` 权威投影表达 accepted intent、execution status、primary reason、next step。Web 本地 queued/error/gate 状态只能是带来源和 freshness 的临时 overlay；ack arrival、reload、timer、空 mailbox 或 local snapshot 不得推进 `logicalTime`/`eventSeq`，也不得推导 applied/completed。入口只显示调用者有权访问的脱敏意图与记忆摘要，不暴露其他玩家私有信息、prompt 全文或内部 chain trace。任何入口不得以本地推断覆盖 canonical status。

<a id="des-wr-ia-009"></a>
### DES-WR-IA-009：稳定追踪与证据绑定

长期设计只保存各产品 `REQ/AC -> 对应专业 PRD/authority（适用时包含 PRD-GAME-014 guarantee）-> DES-WR-IA-* -> 验证方法`。Agent authority 与 Agent asset economy 可直接映射到其专业 authority，不经由 PRD-GAME-014 代行。每次实现由 PR、Git、实际 CI 与独立评审记录绑定 source/integration/tested tree 和验证产物；任务状态、动态 HEAD 和 verdict 不回写本文。Issue 按跟踪需要使用，`TASK-GAME-071~075` 是玩法工作包标签，不构成开发准入。

<a id="des-wr-ia-010"></a>
### DES-WR-IA-010：当前授权范围与高后果行动

对 Agent authority `REQ/AC-AGENT-AUTH-001` 与 §6.2 AC-1/2/9，runtime 在 intent submission 与权威接受/效果提交前读取当前有效 grant view。有效 issuer 是对目标 Agent 当前具有有效控制权的玩家/owner，或其现行治理授权覆盖该主体与范围的组织；必须保留该 grant 的来源身份、来源权限作用域和 revision。Grant 至少约束目标 Agent、action/target/object scope、适用资源或经济权利的 source 与 object subject、有效期内累计 budget/quantity/risk 类别、到期与撤销效力。Agent 建议、WASM/module capability、一次 receipt 和旧 owner grant 都不能作为新的 player delegation。

累计使用以同一 grant identity + resource/right source + object scope + effective period 为唯一计量域；不跨独立 grants 新设全局账户或聚合额度。拆分、并发、重试、重连、Agent/owner 切换共享这一个 durable usage ledger。提交检查与扣账必须在 world effect、domain settlement receipt、usage update 和 intent terminal outcome 的同一序列化提交边界内完成：在边界读取最新授权 revision 与余额，精确验证 source/object/target/action，写入一次 usage delta 和一次 effect receipt。相同 durable request/effect identity 的重试只返回原结果；冲突 identity fail closed。仅 intent 被接受或进入队列不扣实际额度，也不代表效果已发生；若有界内 reservation 被现有执行路径需要，它必须是该 usage ledger 上可恢复、可见、可释放且不代表资源/权利已对外暴露的暂态。

组织 grant 的主体资格、有效期和撤销从既有治理授权读取，runtime 不另建组织角色或授权规则。高自治仍可在 grant 内自动推进，无须逐动作确认；缺少来源、范围、revision、余额或适用权限证据时，不产生高后果世界效果，并返回实际 blocker 与既有升级、等待、改道、取消或重新确认路径。Agent asset team expansion、工业供给及经济额度继续消费 gameplay authority，不扩大 delegation。

具体已采纳来源与独立验证方法按[授权 profile 承接](#agent-authority-adopted-profiles)消费；通用 grant view 不把不同入口的支持范围取并集。

<a id="des-wr-ia-011"></a>
### DES-WR-IA-011：授权变化与转让边界上的待决行动

对 Agent authority `AC-AGENT-AUTH-002`、§6.2 AC-4/8 与 asset economy `AC-WR-AOS-005`，撤销/到期生效、转让通过现有五项门槛并到达 gameplay 定义的生效点、或新世界/安全硬边界出现时，runtime 在任何后续 world effect 前使受影响的 pending intent 按新 authority revision 重评。处理必须读取新 owner/control identity、现行 grant 与完整前置条件；该请求只能按既有合同继续、明确拒绝/取消/过期，或在新 authority 下由有权主体重新确认。重新确认是新 request/cause edge，不把旧 owner 的批准身份复制给新 owner。

授权 revision 与每个 pending intent 的“最近完成重评 revision”必须可从 canonical event/state 确定。授权变化进入同一 event order 后，之前 accepted/submitted 但无权威效果的请求先变为需重评；下一次提交和最终效果提交若发现 revision 不匹配，必须同步重评或终止，不能先应用效果后补审计。重评无效果，且不得改变旧的 intent/receipt；已提交结果、旧配置 provenance 与责任归因保持只追加可追溯。经济转让须消费 AOS-005 的资格、容量、资源、治理、反滥用前置条件且不能部分生效；转让本身不授予额外自治权。工业供给消费 AOS-004，不得以治理 quota 或 debug/probe 路径代替正式供给。

经济转让及既有 Agent 退休/报废的真实来源由[处置生效合同](agent-economic-transfer-and-disposition.design.md#ad-effective-owner-generation)产生；本条按[控制与历史消费者](#agent-disposition-control-history-consumer)连接实际 owner、task、配置和读取边界。处置预览、claim 终态、单次 identity 安装或本地重配置 ACK 都不能替代该领域结果。

<a id="des-wr-ia-012"></a>
### DES-WR-IA-012：异议、owner override 与因果归因

对 Agent authority §6.2 AC-5/6，权威结果及其 receipt reference 必须分别关联适用的 grant issuer/source、owner confirmation/override、组织 policy/instruction/实质受益、Agent 建议/执行、实际设施/执行载体和 domain outcome。Agent preference/objection 是 evidence，不单独构成世界拒绝。Owner override 必须绑定当前 owner/组织代表的有效授权及当前 grant revision；它可覆盖 Agent objection，但仍重新经过世界规则、资源、治理、竞争和安全 hard-block 校验。硬阻断优先于 override，任何授权/来源失效或 hard-block 均无世界效果。责任投影遵循产品责任语义：有效 owner override 的 owner 承担主要责任；组织在授权、强制或实质受益时承担相应共同责任；Agent 不作为 owner/组织的替罪对象。该归因是产品承诺的可审计结果，不制定法律责任制度。投影要区分异议、授权不足/无效、世界/安全硬阻断、有效 override 与实际执行结果，保留阻断后的可恢复动作；不得把多种原因合并为“Agent refused”。复用 DES-WR-IA-007/008 的提交、隐私和同源投影边界，不暴露私有 prompt 或内部 trace。

## 5. 关键运行流程

### 5.1 接受、执行与结果

1. 玩家通过受支持入口提交带调用者和请求身份的意图。
2. 入口返回 request/validation outcome；尚无权威 receipt 时不得显示世界已应用。
3. Agent 可以产生解释或计划；runtime 依据当前世界、权限和规则接受、拒绝或阻塞。
4. runtime 在提交点产生权威状态、世界差异和可关联 receipt。
5. Viewer/API 投影相同的主意图、状态、主因果与下一步；无 progress 时明确表达。

### 5.2 阻塞、重排与恢复

1. 权威状态报告 blocker 或 bounded window 内无可验证推进。
2. projection 提供 wait/repair/reroute/reprioritize 或返回目标选择的适用集合及 tradeoff。
3. 玩家提交 interrupt/correction/new intent；系统保留旧新身份和 handoff reason。
4. runtime 决定何时生效；失败时保留原状态并返回可执行下一步。
5. 重连从 snapshot/journal/receipt 恢复同一因果链；身份不匹配或证据缺失时降级为重新决策，不伪造 continuation。

### 5.3 重试、并发与取消

重复请求必须依据明确 identity/idempotency 规则去重或成为新意图；当前 runtime 使用确定性 intent identity 与 request digest，未来变更必须保持冲突 fail closed。超时只证明当前观察窗口无结果。并发修改由 authority 决定顺序、拒绝或 supersede；取消只有收到权威结果后才可呈现为已停止。

### 5.4 Agent authority 变化与高后果结果

1. Authenticated submission resolves the current owner/organization authority source and grant revision, then checks Agent, action, target/object, resource/right source, cumulative budget, risk and validity. The request keeps its canonical intent/request identity and the resolved authority reference; Agent output and module capability are never grant sources.
2. At authoritative acceptance and again at the effect commit boundary, runtime re-reads current grant, balance and applicable world/governance/safety preconditions. Budget check and debit serialize on the same grant/source/object/period key as the domain effect and receipt. A stale revision, insufficient balance or failed hard boundary yields a durable not-applied outcome and recovery surface, with no partial effect or hidden debt.
3. A revocation/expiry, transfer effective event or new hard boundary advances the canonical authority revision. Pending requests with no authoritative world effect become recheck-required in event order. Before any later effect, the executor either rechecks against the new owner/grant, explicitly terminates the request, or requires an authorized new request. A changed revision cannot be bypassed by retry, reconnect, actor switch or an accepted status.
4. The committed receipt/reference binds original intent, grant source/revision, applicable owner confirmation or override, organization instruction/benefit, Agent advice/execution, actual carrier and domain result. Agent objection remains evidence; a valid override can supersede that objection but cannot bypass world/safety/resource/governance hard blocks.
5. Replay returns the original decision/debit/receipt for the same durable identity. It never re-runs the effect, charges usage twice, recreates an Agent memory write, or rewrites a historical result. Missing or conflicting authority/receipt evidence restores as pending/unknown or replan-required and grants no execution authority.

This contract consumes current governance grant and ownership-transfer sources; it adds no organization role model or transfer/economy rule. Economic transfer must meet the five AOS-005 prerequisites without partial effect, while AOS-004 industrial supply remains the only source of its gameplay-defined supply result. A governance quota, debug/probe injection or module permission cannot substitute for those domain outcomes.

## 6. 接口与数据合同

| 接口/条款 | producer -> consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| intent submission / DES-WR-IA-001 | Viewer/API -> Agent/runtime | caller + request/intent identity；具体 schema 由专业 authority 定义 | 重试规则必须显式 | accepted/rejected/pending 与 applied 分离 | 新字段不得改变旧状态含义 |
| interpretation / DES-WR-IA-001, DES-WR-IA-007 | Agent -> runtime | 关联 intent 与 Agent contract version | advisory；不得自行提交世界 | plan/reason 或结构化失败 | 旧 Agent 输出不能绕过 validation |
| authoritative result / DES-WR-IA-002 | runtime -> journal/receipt | world/candidate + intent/action identity | canonical order；可重放 | canonical `committed/rejected/failed/pending`；blocker 由 reason/disposition 表达 | gameplay 分类保留原 disposition，不新增 runtime enum |
| agency projection / DES-WR-IA-005, DES-WR-IA-008 | `WorldSnapshot.player_gameplay` / receipt -> Viewer/API | 同候选、调用者、时间/版本边界 | 只读投影；local feedback 仅 provisional overlay | 四类 invariant 或结构化 unavailable | 两入口语义等价，不要求 payload 相同 |
| resume / DES-WR-IA-006 | snapshot/journal/receipt -> continuation surface | world/session + continuation identity | 恢复必须幂等或显式冲突 | resumed/stale/replan-required | 旧快照缺字段时不得伪造当前值 |
| grant view / DES-WR-IA-010 | current owner/player or already-authorized organization source -> runtime | issuer/authority-source identity and revision; Agent + action/target/object scope; resource/right source and object scope; cumulative limit/usage and period; risk and effective/revoked interval | Same grant + source + object + period is the only budget key; read on submission, acceptance and effect commit | valid/insufficient/expired/revoked/out-of-scope/unavailable | Organization eligibility comes from current governance authority; no WASM/module grant substitution or cross-grant global aggregation |
| pending-action recheck / DES-WR-IA-011 | authority change -> every pending intent without an authoritative world effect | existing intent/request identity + prior/current grant revision + effective event boundary | recheck before any effect; serialized event order; exact retry returns original decision | continue under current authority, terminate explicitly, or require new authorized request | submitted/accepted cannot bypass; transfer/economy semantics stay with product/gameplay authority |
| effect commit / DES-WR-IA-010/011 | runtime authority -> domain settlement + durable ledger | effect identity, validated authority revision, source/object/period budget key and domain receipt ref | one serialized commit binds world effect, one usage delta, intent outcome and receipt; duplicate identity returns original | committed once or durable not-applied/blocker; no partial world effect/debit | technical fields may be added to existing records; no second receipt/state machine |
| attribution / DES-WR-IA-012 | Agent reason + owner/organization decision + runtime result -> receipt/projection | 既有意图、authority 来源与权威结果 | 与结果保持同一因果关联 | 区分异议、override、硬阻断和执行结果 | 遵守 DES-WR-IA-007/008 隐私边界 |


<a id="owner-delegation-result-replay-contract"></a>
### 6.0 本地 owner delegation 的结果、重试与 profile

Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad；承接 [REQ-AGENT-AUTH-001](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) 及 DES-WR-IA-010 的同一授权/预算责任，不新增玩家权限或产品动作。本节只收敛 install/revoke 用例；memory correction、task fence、signer delegation、interrupt/override 的既有接口及 effect-time 重验不借此重构。

**现状与 owner。** [`owner_control_admission.rs`](../../../crates/oasis7_runtime/src/world/owner_control_admission.rs) 负责完整 signed request 与当前 host/owner 认证；[`owner_delegation_control.rs`](../../../crates/oasis7_runtime/src/world/owner_delegation_control.rs) 实现下述 typed result/read 接缝，并唯一拥有 command→grant、同 ID equality、scope reservation、revision transition 和 exact-replay matching。[`agent_memory_correction.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/agent_memory_correction.rs) 消费该结果，只拥有入口 profile、transport、session/nonce/write-fence 及既有 JSON/error 映射。列表读取可用于显示，不得用于 Viewer 再裁决 mutation。无需新增 crate、服务、账本或通用 capability registry。

| 目标进程内接口（名称示意） | 输入/返回 | 行为与权限 |
| --- | --- | --- |
| `apply_signed_delegation_control` | `&mut World`、完整 `AgencyControlRequest`、借用的 `OwnerControlHostAuthority`；`Result<DelegationControlOutcome, DelegationControlError>` | 仅新鲜 nonce 经宿主放行后调用；Runtime 重验签名与当前 owner/host authority，再作唯一业务裁决 |
| `signed_delegation_control_replay` | `&World`、同一完整 signed request 与当前 host authority；`Result<DelegationReplayMatch, DelegationControlError>` | 只读；能返回 `ExactReplay(authorization)`、`IdentityConflict` 或 `NoMatch`，绝不安装/撤销、不落盘、不消费 nonce |
| `DelegationControlOutcome` | operation=`Install/Revoke`，disposition=`Applied/ExactReplay`，本次对应的现有 authorization（含 grant ID/revision） | 是调用结果，不是授权 token；`Applied` 只指控制变更，不代表后续 Agent 行动完成或新增 finality |
| `DelegationControlError` | 保留失败阶段（认证、读取、scope/identity/revision、应用/持久化）及原原因 | adapter 可保持原 error code/message；不得靠解析 Debug 字符串或重读历史重建领域裁决 |

两条 Runtime 接口共享 grant 构造与历史匹配，不共享“旧 nonce 可 mutation”的入口。完整请求含签名覆盖的 request/player/key/Agent/world/reorg/command；host callback 必须读取真实当前注册 session key 与 Agent binding，不接受请求选择的 authority。每次调用仍核对当前 owner source，不能缓存上次成功为可复用权限。Runtime 验证不替代 Viewer 的当前 world/reorg/write-fence；effect commit 仍重验 grant/source/revision/时间/范围/预算。

**同一领域判定与 nonce 分支。** Install 用当前 owner claim source 构造既有 grant：issuer/owner 为当前 owner、organization=None、revision=1、resource=electricity、revoked=false，其余条款来自 signed command。exact equality 包含全部既有字段和 action 顺序，不把 action 列表排序/集合化。Revoke 保留 `expected_revision.saturating_add(1)` 的既有边界，不在接口迁移中修改溢出语义。

| 当前历史与命令（已通过入口身份/当前 authority 检查） | 新鲜 nonce mutation 结果 | 已消费/不新鲜 nonce 的只读匹配与既有响应 |
| --- | --- | --- |
| Install，无同 ID、无 reservation，底层 grant 校验及持久化成功 | `Applied`，既有安装一次 | `NoMatch` → `auth_nonce_replay`；不能借此新建 grant |
| Install，同 ID 全字段相等 | `ExactReplay`，不重写 | `ExactReplay`；只返回该 authorization |
| Install，同 ID 不同 terms（包括已撤销或 source 改变） | identity conflict → `grant_identity_conflict` | `IdentityConflict` → `grant_identity_conflict`，不是统一改成 nonce 错误 |
| Install，无同 ID，但同 source、未撤销、同 object、action 有交集 | scope reservation → `grant_scope_already_reserved` | `NoMatch` → `auth_nonce_replay`，不新增 scope 查询裁决 |
| Revoke，当前未撤销且 revision=expected | `Applied`，revision 按既有规则递增并 revoked=true | `NoMatch` → `auth_nonce_replay`；不能撤销 |
| Revoke，已 revoked 且 revision=expected.saturating_add(1) | `ExactReplay` | `ExactReplay` |
| Revoke，缺 ID / revision 不符 / 其他 revoked 状态 | 分别 `grant_not_found` / `grant_revision_stale` | `NoMatch` → `auth_nonce_replay` |
| 授权列表读取失败 | `runtime_agency_read_failed` | 保留原分支的无匹配→`auth_nonce_replay`，内部可保留读取诊断；不得用错误结果放行 |
| 新应用在底层 grant 校验或持久化失败 | `grant_install_rejected` / `grant_revoke_rejected`，不发布 candidate、不消耗 nonce | 不调用应用/持久化路径 |

**损坏 ledger 的有意收紧。** `Applied` 必须携带可表示的 authorization，因此对应投影在 candidate 持久化与发布前完成。若 ledger 缺少该 grant，却已有同一 budget key 的 orphan `spent`，且超过新 grant 的 limit，则返回应用阶段的原 `budget_corrupt` 原因（Viewer 为 `grant_install_rejected`）；不发布 World、不持久化 candidate、不消费 nonce，不能伪造 remaining 或省略 authorization。旧 unit mutation 在这种不完整状态下可能先成功，而后列表显示读取失败并 fallback 空列表；这项损坏状态的收紧不宣称全状态等价。budget key 包含 grant ID，合法历史及撤销后使用新 ID 的普通重授权不受影响。底层 unit API、预算键、正常 grant/revision 规则不变；首次应用后的纯显示列表读取仍保留既有失败 fallback。

该表的 replay 是当前权威状态上的匹配，不是另建历史 receipt store；被撤销的 install 不恢复为原成功，新的 owner/source 不继承旧 grant。`ExactReplay` 不产生新 revision、usage debit、world action 或 receipt。

**入口顺序和映射保持。** Viewer 继续按现有顺序检查：chain-linked local control 禁止 → request identity → recovery write fence → world/reorg → proof 存在/command 编码/签名 → session key → Agent access → current owner source → nonce。只有新鲜 nonce 才进入 install 的 scope/profile 校验和 mutation；旧 nonce 进入上述只读匹配，不挪入新请求 allowlist/校验来改变现有拒绝优先级。共享 matcher 不要求两分支完全相同的错误。

新鲜 nonce 的 `Applied` 在原 candidate 成功发布后才消费 nonce；新鲜 nonce 的 `ExactReplay` 按原流程消费新 nonce；旧 nonce exact replay 不再消费。拒绝不能提前消耗 nonce，失败不能将候选 World 发布为成功；不借此调整现有 publication/nonce 次序或宣称新增跨 World/sidecar 原子事务。返回仍为：首次应用 `data.authorizations`（允许纯展示读取，保留现有读取失败 fallback），exact replay 为 `data.authorization` + `idempotent_replay:true`。新的内部类型不改变 signed wire、签名域、外部 code/message/shape；多条件同时错误时按原分支映射，不统一改成 `grant_*_rejected`。

**能力 profile 不取并集。** 下列是不同层的既有范围，不是承诺每个入口支持所有底层动作：

| Profile/层 | Install action 范围 | 限制 |
| --- | --- | --- |
| Local Viewer owner-control | `MoveAgent`、`TransferMaterial`、`ScheduleRecipe` | 不开放 `WorldRule`/`BuildFactory`；新鲜 install 的 unsupported 仍为 `grant_scope_invalid` |
| Signed Runtime owner-control | 上述三项及 `WorldRule` | 仍不通过该签发入口开放 `BuildFactory` |
| 底层 grant storage/validation | 上述四项及 `BuildFactory` | 值可接受不等于入口可签发或动作可执行；领域效果仍满足 §3.2 与原领域准入 |
| Chain-linked Viewer local mutation | 无 | install/revoke/override/interrupt 仍返回 `canonical_agency_control_unavailable`；不包装成本地已接通 canonical capability |

Profile 判定可保留为小型具名 policy，独立于 grant 业务身份/生命周期；不能将支持范围并集化，也不能因底层接受 `WorldRule` 就声明 native electricity lane 支持其效果。旧 nonce 只读查看已存在的精确结果不是新签发能力。

**迁移和验收。** 先提供 Runtime typed outcome 与只读 matcher，并用现有 signed owner 测试覆盖完整签名、registry/current source；再把 Viewer 新鲜/旧 nonce 两条 delegation 分支改为消费它们，删去对应 grant 构造和历史状态机。需要时旧 unit-returning Rust API 可暂作包装；不改 persisted grant/budget key、revision/history、签名 payload 或数据 schema，回退无需 ledger migration。

使用同一 signed vectors 对照 Runtime outcome 和 Viewer adapter，覆盖表中每行、新鲜及已消费 nonce、scope/profile 拒绝、owner/key/world/reorg 变化、签名篡改、恢复 fence、读取及持久化失败、同时多错误的优先级。保留 [`signed_owner.rs`](../../../crates/oasis7_runtime/src/tests/agent_delegation/signed_owner.rs) 与 [`agent_memory_correction_control.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/agent_memory_correction_control.rs) 的 exact retry/nonce 不提前消费基线；补齐 delegation 向量及 snapshot/restore 后重试，对账 revision、余额、事件/receipt 数和 response/error 的新旧等价。验证实际受支持 local Viewer/API 组合路径；这不证明 chain-linked mutation、组织授权或完整经济转让已实现，也不阻塞独立的 grant selector/再授权业务修复。

### 6.1 实现落点与分片建议

现有实现按以下三片绑定真实调用点；后续补齐继续复用 canonical intent、domain receipt 与 gameplay snapshot，不建立脱离入口的辅助状态或平行世界效果 receipt store。产品/玩法权限与资产结果仍由现有 authority 提供，字段名与 serializer 由 runtime 实现确定。

| 实现片 | 主要文件与现有接口 | 实现责任 | 最小回归落点 |
| --- | --- | --- | --- |
| Runtime authority、intent 与 receipt | [`agent_cell.rs`](../../../crates/oasis7_world_state/src/agent_cell.rs) `AgentIntentV2`; [`agent_intent.rs`](../../../crates/oasis7_runtime/src/world/agent_intent.rs) `record_agent_chat_intent_with_authority` / replay disposition; [`agent_intent_terminal.rs`](../../../crates/oasis7_runtime/src/world/agent_intent_terminal.rs) exact terminal/completion APIs; `runtime/events/domain_event.rs`; `runtime/world/agent_intent_publication.rs`; existing `runtime/world/agent_delegation.rs` and `agent_delegation_controls.rs` for grant-view validation, the keyed usage ledger in persisted cognition state, and authenticated owner controls; `runtime/world/event_processing/action_to_event_economy_factory.rs` and `runtime/world/factory_authority.rs` for supported factory effects; `runtime/world/cognition_persistence_transactions.rs` and [`WorldCommitRecordV1`](../../../crates/oasis7_world_state/src/cognition_recovery.rs) only for cognition transactions using that seam | Bind current governance/owner source to existing intent submission and actual effect commit; revalidate and serialize debit with domain commit; persist pending recheck revision/attribution references; keep each domain's settlement receipt authoritative. The module is wired into cognition prepare/finalize and action admission; its causal read model references domain events rather than creating independent world effects. | `runtime/tests/agent_intent_v2.rs`, `agent_cognition_live_command.rs`, existing `runtime/tests/agent_delegation.rs` and `runtime/tests/agent_delegation/dissent.rs`; domain recovery/effect cases in `runtime/tests/economy_factory_lifecycle/receipts.rs` and `industry_history_publication_transaction_regressions.rs` |
| Agent correction lineage | [`continuous_agent_harness.rs`](../../../crates/oasis7/src/simulator/continuous_agent_harness.rs) `MemoryWriteIntentV1`; [`cognition_policy.rs`](../../../crates/oasis7/src/simulator/cognition_policy.rs) `MemoryContextSnapshotV1` / policy; [`agent.rs`](../../../crates/oasis7/src/simulator/agent.rs); [`llm_sidecar_lineage_persistence.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/llm_sidecar_lineage_persistence.rs); cognition receipt linkage in runtime | Bind correction identity, acceptance, affected scope and earliest effective action to the same decision identity; submit memory writes only for the matching committed receipt; include actual next memory-driven result or stale/ignored reason without changing memory-selection algorithms | `simulator/tests/agent_cognition_memory.rs`, `agent_cognition_live_harness.rs`, plus restart/replay proving one memory write and preserved correction result |
| Viewer and pure API projection | [`persist.rs`](../../../crates/oasis7/src/simulator/persist.rs) `PlayerGameplaySnapshot`; [`control_blocking.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_blocking.rs) `committed_receipt_tuple`; [`gameplay_snapshot.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot.rs) `build_player_gameplay_snapshot`; `viewer/runtime_live/player_gameplay.rs`; [`oasis7_pure_api_client.rs`](../../../crates/oasis7_client_cli/src/bin/oasis7_pure_api_client.rs) | Build both surfaces from the same authorized snapshot/receipt references; expose accepted/status/reason/next step and applicable causal/authority summary; retain source/freshness on local provisional feedback | `viewer/runtime_live/tests/snapshot_progress.rs`, `wait_resolution_quote.rs`, `auth_actions_provider_continuation_restart.rs`; same-world API/Web parity and unauthorized-field redaction |

具体字段集合继续由 [`PRD-GAME-014`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#2-user-experience-functionality) 与各专业 authority 管理；本文只冻结跨组件语义和禁止条件。

<a id="agent-authority-adopted-profiles"></a>
### 6.2 已采纳授权 profile 与同一累计责任

Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad；承接 [REQ-AGENT-AUTH-001](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001)。本节接收已有来源和方法；实现是否具备相应 adapter、正式入口和同候选证据单独判定。

| 适用组合 | 唯一来源与消费合同 | 生效、累计和可读结果 | 方法与适用边界 |
| --- | --- | --- | --- |
| 本地 owner native delegation | [§6.0](#owner-delegation-result-replay-contract) 的当前 owner、完整 signed request 和入口 profile | 原 grant/source/object/period、原 Electricity usage；install/revoke 的 Applied/ExactReplay 不代表后续动作已发生 | §6.0 同向量 Runtime/Viewer、nonce、restore 方法；不据 storage 接受的动作并集扩大入口 |
| 普通组织或个人代表的已采纳合同签署 | [普通签约政策](../../game/gameplay/gameplay-agent-mediated-contract-formation-policy.prd.md#formation-policy-principals)及 [OrganizationContractSigningClauseV2 / ContractMandateV2](agent-mediated-contract-formation.design.md#contract-formation-principals-mandates) | 原组织、charter、完整成员/批准主体及准确治理来源；代表任期、原 grant 与逐源范围分别核验。个人资金/Agent 的来源权利另验，组织名册不代签 | CF09；仅即时交换、有限供能、维护/维修、运输、共同工作和 R&D 等该政策明确采纳的 family，不变成一般组织管理或所有 Agent 动作权限 |
| 同一长期 mandate 的真实暴露 | [原 usage 上的 D/O/C](agent-mediated-contract-formation.design.md#contract-formation-cumulative-exposure) | 在原键内逐单位核已花 D、未结业务 O、全部后续费用 C；全体实际 writer 与领域效果同根更新。退款不减 D，换请求/代表/Agent/owner/显示期间不清旧暴露 | CF11、CF17；资源、材料、权利占用和风险仍各用原单位，不跨独立 grant 新建总额 |
| 失权、继受、未知和三端读取 | [原责任恢复](agent-mediated-contract-formation.design.md#contract-formation-common-recovery)与[当前受众读面](agent-mediated-contract-formation.design.md#contract-formation-common-read) | 撤销停止新承诺，已成立义务按原领域狭义处分权结清；原件查询不重新形成合同。可见来源、期限、D/O/C、blocker 和真实下一步来自同一 root | CF13、CF15、CF16；历史读取不授新写权，不把服务/R&D 继受称为 Agent 经济转让 |

运行时先按完整 action/family、主体、来源、对象和版本选择准确 profile，再验证该 profile 的全部条件；没有合法来源或 adapter 的组合返回明确 unsupported/unavailable。不得将 native grant 和合同 mandate 的允许范围拼接为更宽授权，也不得把同一来源的已有负担遗漏后当作另一份额度。高自治在已授权范围内可持续推进，真正新增范围或累计责任才走原明确授权程序。

一个已合法适用的原 grant 若被多个 writer 消费，接收新 mandate 的版本必须从同一 D/O/C 返回可用余额 `limit - D - O - C`，实际提交仍核完整当前源及原 hold。旧 native profile 没有该暴露时保持原语义；不识别新 mandate/负担的旧入口不能继续消费它，缺索引不按 O=C=0 处理。不同独立 grant 不合成授权总额，但底层实际资产的同一预留与可用量守卫仍必须共同遵守。

上述 CF09/11/13/15/16/17 均由[现有合同形成验收手册](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-09-organization)拥有。IA 验收复用其源、动作、独立算例和当前受众，不把目标规则或测试定义报告为已运行，也不再维护同义组织政策、第二份 usage 或新的成功 receipt。

<a id="agent-disposition-control-history-consumer"></a>
### 6.3 处置生效后的控制、配置与历史消费者

Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a；O7-REQ-42168149-c979-4211-9777-2d72f7050147。承接 [REQ-AGENT-AUTH-002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002) 和 [REQ-WR-AOS-005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)。经济/生命周期裁决、五门槛、资产/责任和费用仍归[唯一处置 producer](agent-economic-transfer-and-disposition.design.md#ad-effective-owner-generation)；本文拥有其对 Agent、配置和各入口的接收。以下是正式目标适配，不声称现有本地 Prompt 或 owner helper 已完成经济处置。

#### 6.3.1 实际来源及不可互换的版本

| 实际来源/消费者 | 当前可复用事实 | 目标接收义务 |
| --- | --- | --- |
| [当前 owner 与正式 signer](../../../crates/oasis7_runtime/src/service_admission/agent_authority.rs)、[本地 signed owner admission](../../../crates/oasis7_runtime/src/world/owner_control_admission.rs) | 正式 signer 同时核 current owner 与 capability identity 的 owner_binding；delegate 还核 identity generation、delegate generation 和 world/branch。本地 host 另核当前注册 key 与 Agent binding | 当前 owner resolver 只能消费领域提交的新合法来源；旧注册、制造/claim 来源和旧责任保留。不得用单次 AgentIdentityInstalled 或本地绑定替代经济交割 |
| [GameTaskControlV1](../../../crates/oasis7_agent_api/src/game/task.rs)、[GameTaskFenceV1](../../../crates/oasis7_agent_api/src/game/executor.rs)及[task retirement](../../../crates/oasis7_runtime/src/service_admission/game_task/task_retirement.rs) | task_revision、owner_generation、executor_epoch、目标引用和完整 world/history fence 各自有原合同；retire_task_contexts 关闭旧 task context，不代表 Agent 退休/报废 | 最终控制变更同根使旧 task/executor/认知许可失效；已提交 context/action 的原结果保留。新 owner 必须按原 task/goal/delegation 接口显式取得新上下文，不能把旧 task 的正文和批准重新标为新 owner |
| [AgentPromptProfile](../../../crates/oasis7_simulator/src/world_model.rs)、[版本读取](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/prompt_profile.rs)与[实际 apply](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/prompt_control_enhanced.rs) | profile 保存 system/short/long overrides、version、updated_at_tick/by；[sidecar](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/llm_sidecar.rs) 保存内存 history。增强 ACK 当前为 RuntimeInstance，persistence/sync 为 None；虚拟 AgentPromptUpdated 不是 canonical 处置历史 | 已实际使用的版本须有独立留存、来源与有效区间；将引用纳入处置候选，不能把当前 profile 覆盖成新 owner 后再称历史完整。旧本地 history 没有成为正式跨 owner reader 的证明 |
| [当前私有目标](../../world-simulator/llm/external-agent-game-api.design.md#已实施切片：owner-目标准备、发布与当前正文回读)及[协作目标/持久合同](player-runtime-collaboration.design.md#collab-goal) | 私有 prepared proof、盐承诺、activation commit 与当前 TaskControl 相互校验；当前 owner 回读在交付前重验权限。公共目标引用不是正文 digest | 保留原私有存储及授权边界；新 owner 不沿用旧 private-goal/current 资格读取旧正文。历史 reader 明确验证当前主体对原历史的权利，不能通过改 owner 参数或公开内容地址绕过 |
| [GoalSnapshotV1 / MemoryContextSnapshotV1](../../../crates/oasis7_agent_api/src/cognition.rs)及[provider lineage 恢复](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/llm_sidecar_lineage_persistence.rs) | decision 使用不可变 revision/scope/digest；lineage 保存原 capability identity，owner/generation 漂移使旧 provider turn stale，而非补发新 grant | 保留旧 decision、memory revision 与 receipt 的因果引用；新 context 的选择范围绑定当前 owner/control generation，旧私有记忆、provider session 和密钥不会因 Agent ID 相同而自动成为新上下文 |

owner/source revision、Agent identity generation、grant revision、task_revision、executor_epoch、Prompt version 及本地 binding/authority epoch 不互相替代，也不要求数值相等。消费方逐项比对来源声明的预期版本和实际提交结果；不能仅凭某个计数更大认定整条链已经追平。身份根更新若影响关联 Agent 的技术 fence，按 producer 的完整受影响集合处理；这不表示关联 Agent 一起转让。

#### 6.3.2 同根控制变化与未决请求

处置输出复用原事务 receipt 的 typed payload/索引，至少可验证：world/branch 与提交位置、原 request/business identity、Agent 稳定身份、处置类型/范围、旧新经济主体、旧新控制来源及各自 generation/revision、当前 disposition-plan guard 及其范围/状态、受影响 task/executor/grant fence、已提交结果/未决义务和配置历史引用；逐阶段保留实际前后值，Begin 不伪造这些未变化的控制版本。它不创造第二份成功 authority；收到 ACK、preview 或准备记录不能切换 owner。

[原处置政策](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#agent-disposition-policy)区分 Begin 与 Finalized：已权威提交的 Begin 启动原等待窗并安装本件持久 disposition-plan admission guard；它由 canonical 当前 plan/root 提供，具有精确作用范围和当前状态，不是本地 UI 暂停标记。旧经济 owner/source、identity generation、task_revision、executor_epoch、context 和 Prompt V 均保持，容量与退款不按最终转让处理。全部真实新 effect writer 在效果提交前重读该 guard；旧请求即使签名或 accepted 早于 Begin，也不能越过禁止的新风险。原已获权的窄 drain、原件查询及本件允许的取消/停止仍各走原权限，不把 guard 当作对全部原 grant 的撤销。仅领域 Finalized 的完整经济/控制提交才推进所需 owner/generation 与全体 fence。Abort/Expire 或失败后的合法结束只撤本件 guard、释放本案可释放部分，不自动派发旧 pending 或 Resume；原 context 仍有效时显式恢复沿原规则，另已失效才要求新的 current context/授权，不能复活失效 grant。

1. **提交前准备。** 各阶段读取同一 parent 的真实 current owner/source、identity、当前 disposition-plan guard 和完整前置；Finalize 再收齐全部受影响 task/executor、pending/已提交结果及已生效配置的来源版本。需要私有归档的正文先沿[协作持久合同](player-runtime-collaboration.design.md#collab-recovery)完成受控耐久准备，再把不泄密的引用加入根候选。若缺当前配置或本次依赖的历史/未决原件证明，返回准确 recovery_required；不能从新 owner 的空 profile、旧本地 ACK 或今天的默认值补造历史。
2. **唯一最终生效点。** 领域 Finalized 一次提交交割与控制来源、必要 identity/fence 更新、旧授权失效及 pending 处置标记。旧 task 停止新执行，旧 executor/lease 不再准入；原有正式 task 合同的 Suspended、版本推进和 context 收口语义各自保持。不得把 provider 进程停止或清缓存当作世界 fence。若所有所需控制写入不能进入同一根候选，该处置不生效。
3. **待决与已生效分开。** 在生效点前已经 committed 的动作、费用、记忆写和责任保持原 owner/cause edge。仅 submitted/accepted 的原请求按 DES-WR-IA-011 重评；失去旧来源后不续行、不扣新 owner 预算。继续需要当前有权主体的新确认时，保留原请求并建立新请求/替换关系。原结果未知先查原件，不用新请求探测其是否已发生；已成立合同只按原窄处分权处理旧义务，不能反向获得新生产或控制权。
4. **按当前阶段接收。** 正式 Service、local Viewer、native Harness 和 API adapter 始终读取当前 plan/root。Begin 的 guard 在实际 writer 和依赖派发处阻断其禁止的新效果，不使原 context 或控制版本失效；允许的原窄 drain 与读取继续。Final 后先消费已验证的新 control/root 并失效旧 Agent 作用域缓存，再接收新目标、Prompt、grant 或 context。sidecar/投影落后、当前来源缺失或任一实际 fence 不匹配时，暂停依赖不完整状态的新写入和私有 context 投递；退休/报废按终态禁止新运行，保留有权原结果读取及结清入口。Final 失效的是相应 Agent 控制，不是旧 owner 对其他对象的账户或全局凭据。

转让后的 Prompt 当前态沿原 Agent 版本采用 checked 递增并追加中性 profile，三个私有 override 均为 None；保留原处置 authority、参与者和 cause，不能伪标为新 owner 的既往选择。原 profile V 及其使用记录不覆盖，不以新 owner 的默认 version=0 重用旧历史键；无可验证前态或版本无法递增时阻塞该次处置。中性 profile 的引用属于同一根候选，adapter 追平只安装该版本，不再发一笔 PromptControl 写入或启动模型。之后新 owner 的显式合法修改才形成后续版本 V2。

新 owner 的默认接收状态没有继承的私有 override、激活目标、批准、executor 或可重用旧授权；可保留当前规则允许的非私有基础配置和公开世界事实。新目标/角色策略/Prompt 使用当前主体的原合法版本接口显式配置，从新提交点生效。新 owner 不自动取得旧 memory、模型账户、API key、设备密钥、私有 outbox 或私有目标。退休/报废没有新 owner 的执行上下文；历史读取、未决义务和原领域允许的救济/结清保持，不把任务 context retirement 混同 Agent 生命周期终态。 退休终态退出 active，保留 estate/原责任且不自动 reactivate 或变成可 Claim 的对象；报废保留稳定 identity/history 及不可执行的终态。reader 必须读取实际生命周期和 estate 状态，不能仅因仍有 owner/identity 记录就开放控制，也不能因无 active task 将其显示为未被认领。

#### 6.3.3 不可变配置、来源与受众

配置历史保存原 source kind、Agent、world/history binding、当时 owner/control generation、原 request/operation、版本/有效区间、内容引用及其授权可见性，并与已使用它的 decision/领域 receipt 相互引用。Prompt profile、canonical GoalVersion 与 Harness GoalSnapshot/MemoryContextSnapshot 分别保留自己的身份和摘要；不得把它们合成一个“目标版本”或用模型解释代替原配置。公开处置结果只给可公开的版本、作用范围和原不透明引用；私有正文、盐、proof 和可枚举正文摘要不复制进公共 World/CAS/事件流。

当前 owner 的配置写权限不自动包含旧私有版本的读取或重用权；rollback 在解析旧版本正文之前必须验证该版本的当前读权和可重用范围，再按当前 control/binding 与 expected_version 处理。缺该权限则拒绝，不能借旧版本号或历史列表把旧 owner 的配置装入新 context。

根候选只提交已有历史/内容对象的完整索引 root 和必要当前引用，不把全部历史正文装入一次交易。私有内容仍由原受控存储维护，准备成功不等于处置生效；提交后才发布对应历史视图。历史对象的完整性、当前依赖和去重引用必须可核，不能仅保存无法解引用的摘要。按原保留/归档规则分页读取，归档不删除责任、当前 pending、合法历史或证明所需对象；缺页、损坏或旧 schema 缺字段明确标记，不用空值证明“过去没有配置/结果”。 完整索引、当前引用、私有准备及恢复工作都纳入[处置完整预算](agent-economic-transfer-and-disposition.design.md#ad-bounds)，依实际 host/存储/transport 限额取严；使用引用不能绕过对象/页数、字节或实际调用预算，超限不截断历史后提交。

目标历史 reader 接入[处置读模型](agent-economic-transfer-and-disposition.design.md#ad-read-model)，按当前经过认证的稳定主体、原当事人/来源权利和本次读取范围裁剪。旧 owner 可以在当前仍获准的范围查看自己原配置、决定、授权及责任，但不能读取新 owner 的私有差异或产生新写入；新 owner 读取取得对象所需的公开来源、当前约束和历史责任，不因此获得旧私有内容；原组织代表过期后也须按其当前历史读权重新判断。仅知道 Agent/receipt ID、持有旧签名或旧 cursor 均不授读取权。每次私有解密与响应交付前重验当前受众，分页绑定同一 root/受众与权限版本；权限变化后停止投递并要求当前授权重读。

| 当前入口 | 消费内容与行为 | 实际接线边界 |
| --- | --- | --- |
| Web Viewer | 同一 root 的处置状态、生效位置、当前 owner/control 与有权历史；旧草稿标 stale，当前编辑区不从旧 profile 回填。历史页分开显示当时配置与当前可操作状态 | 现有 [mapping](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/mapping.rs) 和 [snapshot](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_blocking.rs) 只是现状来源，不能把原 sidecar profile 整包当作跨 owner 授权投影 |
| native Launcher / Game Client / Harness | 当前 control/task/executor 与 disposition receipt 匹配后才新建上下文；原 outbox 按当前读权核原件，旧请求不改写为新 owner | 复用[原操作 lifecycle port](../../world-simulator/llm/agent-game-client-boundary.design.md#original-operation-lifecycle-port)；跨账户/owner 不共享或自动解密旧缓存，query-only 不产生新业务原件 |
| pure API / Agent | 返回相同来源、生效/未知、可见历史和真实下一步；原 task/goal、action/result 的各自身份保留，字段 unavailable 不推断无历史 | 按[正式入口适用映射](../../world-simulator/llm/external-agent-game-api.design.md#api-primary-entry-applicability)声明真实协议/profile；SDK、旧 CLI 或 GET-only 引导面不证明处置写入和历史 reader 已提供 |

三端均从当前授权投影读取，不能各自重算生效时间、责任或 owner。缺正式 reader 时报告对应未实现/未执行；图稿、ACK、mock 或局部 API 结果不代签完整三端组合。

#### 6.3.4 恢复与独立预期

沿[处置恢复](agent-economic-transfer-and-disposition.design.md#ad-transaction-recovery)及原 [G1 generation 合同](../prd.md#runtime-generation-persistence-contract)：NotCommitted 丢弃本次 staged 候选并保留全部原状态；Committed 先核原结果，再幂等追平 sidecar/缓存/历史投影，不重交割、不重扣费用、不复活旧执行；StatusUnknown 保留完整 candidate/base generation，隔离整个旧 World/writer，停止后续 mutator、tick、save 和依赖派发，直到精确持久决定及匹配内存恢复。普通请求超时/查询 unknown 不自动等同 G1 StatusUnknown；仍按原请求合同保留原件。ordinary generation 的 Committed 不单独证明 canonical authority 或 serving 已恢复。 每次 Begin、Finalize 或 Abort 的 base 都是该次实际 parent；Finalize 的 NotCommitted 不能撤销先前已提交的 Begin 保全、已确认费用或原责任，投影也不能跳回整个处置开始前的旧控制版本。

独立组合预期为：旧 owner 的已提交结果 R 在处置后仍引用原配置 V 与原责任；同一切点仅 accepted 的请求 P 不因新 owner 到来自动执行；新 owner 的配置 V2 从其合法提交点起生效，不覆盖 V，也不回填旧 memory。旧方合法历史查询可返回 R/V，旧方新写拒绝；新方无额外授权读取 V 的私有正文被拒绝，仍可读取必要公开责任与当前控制。原处置丢响应只查同一原件；投影落后只能追平一次，不能执行第二次处置。完整方法由[处置历史验收](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-history)与其 owner/recovery 场景维护。

## 7. 状态、事务与持久化

- 状态身份：玩法分类保留对应 authoritative disposition、canonical intent/effect identity 与 producer/version；`accepted_intent_id`、feedback action ID、effect intent ID 和 domain receipt ID 之间需要显式、验证过的引用映射，禁止按字段名猜测等价。
- 提交点：世界效果只在 runtime 与领域 authority 的正式提交点成立；同一提交边界内关联 effect、适用累计额度 usage delta、权威 disposition 和已有领域 settlement receipt。UI ack、Agent plan、grant lookup 与 queue admission 不是提交点。
- 原子性：高后果路径在最新 grant revision 下读取累计余量并于 effect commit 序列化。成功时只写一次 effect、一次对应额度 delta 和一条可核验 receipt lineage；失败时不得遗留 effect、额度扣减或不可见债务。若真实领域无法合并事务，必须以同一 durable intent/effect identity 写出 recoverable pending marker，并在可证明的中间状态恢复；不能靠 UI/local reservation 假装提交。
- 授权变化：transfer/revoke/expiry/hard-boundary 更新进入 canonical event order；所有未产生权威 world effect 的请求通过其 pending identity 找到，并在下一 effect 前完成 recheck 或显式 terminal。已完成的结果、旧 owner 作用范围和新 owner 生效边界只追加记录，不修改既有 receipt。
- 重放与幂等：replay 从已持久化的事件、grant revision、usage delta 和 receipt reference 重建完全相同的决定，或明确返回不兼容/冲突；相同 request/effect identity 不能二次执行或扣账；projection、Agent memory reducer 和 provider 不在 replay 中重做副作用。
- 持久化与恢复：snapshot/journal 必须足以恢复 current authority revision、同 grant/source/object/period 的 committed usage、pending intent 的最近重评 revision、替换关系和 receipt 引用。缺少一项时，恢复为 unknown/pending/replan-required；绝不从旧 owner grant、记忆或 Viewer cache 授权执行。恢复后再次提交必须重新校验当前 authority，保持已经 committed 结果不可逆。
- 因果读模型：玩家面向读模型从 canonical intent + 已提交 domain receipt + 纠正结果构建 accepted/queued、processing/blocked、applied/not-applied、expected/actual consequence 与 next step；它引用而不覆盖领域 settlement truth。Viewer 和 pure API 从相同 `WorldSnapshot.player_gameplay`/authorized receipt references 投影，只有格式不同。
- 生命周期用词：`accepted`、`applied`、`persisted`、`published` 分别需要权威接受结果、世界提交、持久化确认和可消费发布边界；任一缺失都保持 pending/unknown。既有权威世界结果与归因历史保持可审计且不得追溯改写。

## 8. 部署、安全与运行约束

- 玩家只能读取和修改自己有权控制的意图面；组织/治理/冲突动作继续受各自 authority 校验。
- Viewer/API 不持有 world write authority；Agent 不绕过 runtime 权限与世界规则。
- owner/组织只能在当前有效范围内授权或 override Agent；WASM module grant、provider capability、Viewer session 与 Agent 自述均不等同此授权，也不能绕过硬边界。
- 记忆与理由只输出完成当前决策所需的脱敏摘要；私有 prompt、内部 trace、其他玩家记忆和凭据不得进入 receipt。
- provider、网络或入口不可用时，系统必须报告 unavailable/pending/blocked 的真实边界；deterministic/debug lane 不能冒充 active-LLM 或真实集成证据。
- 超时、资源预算和最大 payload 由具体接口 authority 定义。缺少对应证据时，本设计只要求结构化失败与可恢复下一步，不承诺性能。

## 9. 质量与容量

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| accepted vs applied | 受支持意图、固定候选 | 任一入口不把 ack 当作 world effect | [`PRD-GAME-014` acceptance](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#ac-13) | contract + integration test | 具体入口 coverage 未验证 |
| bounded response | PRD 定义的观察窗口 | 无推进时升级为 blocker/fallback/replan | [`NFR-CFC-6`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#prd-game-014-nfr) | QA matrix / session evidence | 时间阈值由专业 authority 冻结 |
| cross-entry parity | 同候选 Viewer 与 pure API | 四类 invariant 语义一致 | [`NFR-CFC-2`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#prd-game-014-nfr) | paired API/Web comparison | payload 可不同 |
| resume/replay | 可恢复 snapshot/journal/receipt | 恢复同一意图链或明确 replan-required | AC-WR-IA-002 | replay/reconnect scenario | 跨版本兼容需候选证明 |
| memory correction | 实际使用长期记忆的决策 | 摘要、来源、纠正与结果可关联 | [`NFR-CFC-7`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#prd-game-014-nfr) | Agent + surface scenario | 不要求未使用记忆的流程引入记忆 |

确定性、长期运行、真实 provider、浏览器和跨入口组合能力都必须以对应环境证据判断；文档检查不证明这些能力。

## 10. 兼容、迁移与回滚

- 新设计条款先建立映射，不自动改变现有 runtime enum、schema 或 transport。
- 实现若新增字段，应保持旧消费者能够读到明确 unavailable/unknown，而不是推导伪值；删除/重命名必须提供版本迁移与 consumer 验证。
- 旧快照或 journal 缺少 agency 字段时，恢复面应回退为 replan-required 或有界的 legacy summary，不能声称完整 continuation。
- rollout 按意图类型和正式入口逐项绑定 task 与证据；一个入口通过不能推导全部入口通过。
- 回滚回到已知实现基线时，必须同时撤回不再成立的 capability claim；已经产生的权威世界效果不可由 UI/文档回滚。

## 11. 验证设计与可追溯性

稳定链路为：

```text
产品 REQ/AC
  -> PRD-GAME-014 guarantee / failure signature
  -> DES-WR-IA-*（本文）
  -> PR 与按需关联的 Issue
  -> PR / fixed source / integration / tested tree
  -> test、role review、CI、artifact 与交付结论
```

### 11.1 验证映射表

| 上游 requirement / acceptance | 本设计条款 | 独立 obligation | 验证方法与 layer | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [`REQ-WR-IA-001`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [DES-WR-IA-001](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-001) | ack 不冒充 applied；world change 与 intent 可关联 | 使用 [`snapshot_progress.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/snapshot_progress.rs) 的现有投影回归并补 accepted≠applied identity 对账 | PR 验证记录 + CI/artifact | 未纳入的意图/入口 |
| [`AC-WR-IA-001`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#ac-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [DES-WR-IA-001](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-001) | 接受、推进、后果和下一步保持可区分 | 使用 [`snapshot_progress.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/snapshot_progress.rs) 验证 canonical intent 与 feedback action 不串 ID | PR 验证记录 + CI/artifact | 未纳入的意图/入口 |
| [`REQ-WR-IA-002`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-002) Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | [DES-WR-IA-003](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-003) | safe-wait quote 含完整 trigger、recheck、expected change、risk 与 alternative unlock；trigger 失败后重分类；fallback 比较成本/进度/机会成本/reason | 使用 [`wait_resolution_quote.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/wait_resolution_quote.rs) 补齐 AC-3B/3C 的 deterministic projection 场景 | PR 验证记录 + QA matrix | PRD 观察窗口与选项不变 |
| [`AC-WR-IA-002`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#ac-wr-ia-002) Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | [DES-WR-IA-006](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-006) | 离开/重连从 continuation evidence 恢复，失效时明确 replan-required | 使用 [`auth_actions_provider_continuation_restart.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/auth_actions_provider_continuation_restart.rs) 验证 restart、receipt 与下一步一致 | PR 验证记录 + QA matrix | 跨版本/跨设备范围需单独证明 |
| [`REQ-WR-IA-003`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003) Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | reason/stakes/expected consequence/alternative/correction/result 同链 | 使用 [`agent_cognition_live_harness.rs`](../../../crates/oasis7/src/simulator/tests/agent_cognition_live_harness.rs) 扩展同一 receipt lineage 断言 | PR 验证记录 + review artifact | 完整真实 provider/browser 样本需另行采集 |
| [`AC-WR-IA-003`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#ac-wr-ia-003) Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | 纠正结果绑定 accepted/rejected、范围、earliest action 与 next action 结果或 stale/ignored reason | 使用 [`agent_cognition_live_harness.rs`](../../../crates/oasis7/src/simulator/tests/agent_cognition_live_harness.rs) 验证 correction lineage | PR 验证记录 + review artifact | 不要求纠正必然改变 Agent 下一动作 |
| [`PRD-GAME-014 AC-3B`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#ac-3b) Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | [DES-WR-IA-003](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-003) | 选项 tradeoff 与完整 wait-resolution quote；触发失败后重新分类，不静默续等 | 使用 [`wait_resolution_quote.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/wait_resolution_quote.rs) 加可重复 trigger-expiry/expected-change regression | PR 验证记录 + QA matrix | bounded-response 数值阈值由 gameplay authority 冻结 |
| [`PRD-GAME-014 AC-3C`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#ac-3c) Trace refs: O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | [DES-WR-IA-003](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-003) | 选项 tradeoff 与完整 wait-resolution quote；触发失败后重新分类，不静默续等 | 使用 [`wait_resolution_quote.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/wait_resolution_quote.rs) 加可重复 trigger-expiry/expected-change regression | PR 验证记录 + QA matrix | bounded-response 数值阈值由 gameplay authority 冻结 |
| [`PRD-GAME-014 AC-13`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#ac-13) Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | 单链断言 intent、reason/evidence、stakes/expected、alternative、interrupt/correction+earliest effect、applied/not-applied domain receipt result 或 stale/ignored reason；accepted≠applied | 使用 [`agent_cognition_live_harness.rs`](../../../crates/oasis7/src/simulator/tests/agent_cognition_live_harness.rs) 的现有 lineage test 作基线并添加完整语义字段断言 | PR 验证记录 + review artifact | 当前测试仅证明部分 receipt linkage |
| [`PRD-GAME-014 receipt scope`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#21-reusable-causal-decision-receipt) Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376 | [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | 领域 settlement receipt 仍是 effect truth；shared read model 引用而不替换它 | 使用 [`agent_cognition_live_harness.rs`](../../../crates/oasis7/src/simulator/tests/agent_cognition_live_harness.rs) 检查 receipt reference 与 runtime commit marker 一致 | PR 验证记录 + review artifact | 每个受支持领域 action 的 coverage 逐项绑定 |
| [`PRD-GAME-014 SC-7`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#sc-7) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185; O7-REQ-ebb0248c-1dca-4d58-a283-8d198fb6cf63 | [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | Industrial gather/refine/move/schedule 改道、裁剪、等待、拒绝均保留原 intent、替换/阻断原因、expected/actual consequence、cost/progress 与 next recovery；debug/probe 不计玩家效果 | 使用 [`industry_history_publication_transaction_regressions.rs`](../../../crates/oasis7_runtime/src/tests/industry_history_publication_transaction_regressions.rs) 增加正式 receipt 与 debug exclusion cases | PR 验证记录 + full-tier artifact | 各动作只覆盖实际支持的领域 producer |
| [`REQ-AGENT-AUTH-001`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | [owner delegation result/replay](indirect-control-agency-execution-and-continuation.design.md#owner-delegation-result-replay-contract) | Applied/ExactReplay 同源；旧 nonce 不能 mutation；response/error/nonce 顺序保持 | [回归方法](../../../testing-manual.md#regression-oracle)；以 `signed_owner.rs` `../../../crates/oasis7_runtime/src/tests/agent_delegation/signed_owner.rs` 与 `agent_memory_correction_control.rs` `../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/agent_memory_correction_control.rs` 为回归落点，补 Runtime outcome → Viewer adapter 同向量合同矩阵、restore 与失败注入；不表示这些新增场景已通过 | PR/CI 的实际结果及兼容对照 | 不证明 chain-linked mutation 或完整产品转让 |
| [`REQ-AGENT-AUTH-001`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | 高后果授权显示/验证 issuer、source scope、目标、期限与同 grant/source/object/period 累计余量 | 使用 [`agent_cognition_live_command.rs`](../../../crates/oasis7_runtime/src/tests/agent_cognition_live_command.rs) 的 authority recheck 基线并接真实 grant view | PR 验证记录 + full-tier artifact | 不引入跨 grant aggregate |
| [`AC-AGENT-AUTH-001`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | 拆分/并发/重试/重连/切 owner 不复制额度；combined cost 超同 grant 剩余额时不得双效果或隐藏债务 | 使用 [`agent_cognition_live_command.rs`](../../../crates/oasis7_runtime/src/tests/agent_cognition_live_command.rs) 加并发 commit 与 retry-once cases | PR 验证记录 + full-tier artifact | 不跨独立 grant 合并预算 |
| authority PRD §6.2 [`AC-1`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-1) | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | 高自治/有界授权保持同一世界、资源、权限与治理边界 | 使用 [`agent_intent_v2.rs`](../../../crates/oasis7_runtime/src/tests/agent_intent_v2.rs) authority-context baseline 并添加 grant-boundary scenario | PR 验证记录 + `test_tier_required` artifact | 基线不单独证明 owner grants |
| authority PRD §6.2 [`AC-2`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-2) | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | 无效/越界/过期/撤销授权与 hard block 不产生高后果效果 | 使用 [`agent_cognition_live_command.rs`](../../../crates/oasis7_runtime/src/tests/agent_cognition_live_command.rs) recheck baseline 并加 owner/org source negatives | PR 验证记录 + `test_tier_required` artifact | capability grant 本身不证明 player delegation |
| authority PRD §6.2 [`AC-9`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-9) | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | 同 grant/source/object/period 累计；原子 effect+debit，重试仅返回原 receipt | 使用 [`agent_cognition_live_command.rs`](../../../crates/oasis7_runtime/src/tests/agent_cognition_live_command.rs) debit-on-retry baseline 并加 concurrent over-budget scenario | PR 验证记录 + `test_tier_full` artifact | 没有跨 independent grants 全局限额 |
| authority PRD §6.2 [`AC-3`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-3) | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | 团队扩张不复制权限/责任并消费玩法资产条件 | 使用 [`agent_claims.rs`](../../../crates/oasis7_runtime/src/tests/agent_claims.rs) claim-cap baseline 加组合授权场景 | PR 验证记录 + `test_tier_required` artifact | 不定义 team economy |
| authority PRD §6.2 [`AC-7`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-7) | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | 工业供给/quota 不扩大 delegation；仍满足资源/产能/交付与责任边界 | 使用 [`gameplay_protocol_regressions.rs`](../../../crates/oasis7_runtime/src/tests/gameplay_protocol_regressions.rs) economy quota baseline 加 authority interaction | PR 验证记录 + `test_tier_full` artifact | 既有 economy test 不单独证明 delegation |
| [`AC-WR-AOS-004`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004) Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) | 工业供给使用正式资源/产能/交付 producer，不能由 debug 注入替代 | 使用 [`industry_history_publication_transaction_regressions.rs`](../../../crates/oasis7_runtime/src/tests/industry_history_publication_transaction_regressions.rs) 验证正式结算 provenance | PR 验证记录 + `test_tier_full` artifact | 只证明纳入的工业 producer |
| [`REQ-AGENT-AUTH-002`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002) Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | [DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | 转让/重配置/处置保留身份、来源、旧配置和既有结果 | 使用 [`agent_claims.rs`](../../../crates/oasis7_runtime/src/tests/agent_claims.rs) ownership baseline 并加 authority history scenario | PR 验证记录 + full-tier artifact | 来源按[处置生效目标](agent-economic-transfer-and-disposition.design.md#ad-effective-owner-generation)绑定；现有测试基线不证明其实现 |
| [`AC-AGENT-AUTH-002`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-agent-auth-002) Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | [DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | 新策略自转让生效点生效，历史不改写 | 使用 [`agent_claims.rs`](../../../crates/oasis7_runtime/src/tests/agent_claims.rs) 添加 effective-boundary replay scenario | PR 验证记录 + full-tier artifact | 不证明未纳入的转让协议版本 |
| authority PRD §6.2 [`AC-4`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-4) | [DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | transfer/configuration 后 Agent identity、source、history 可追溯 | 使用 [`agent_claims.rs`](../../../crates/oasis7_runtime/src/tests/agent_claims.rs) 与 intent replay fixture 验证历史连续 | PR 验证记录 + `test_tier_required` artifact | economic gates 由 gameplay 测试补足 |
| authority PRD §6.2 [`AC-8`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-8) | [DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | revoke/expiry/transfer/hard-boundary 前后 pending 重评或明确终止，committed effect 不变 | 使用 [`agent_intent_v2.rs`](../../../crates/oasis7_runtime/src/tests/agent_intent_v2.rs) terminal/replay baseline 加 stale revision recovery | PR 验证记录 + `test_tier_full` artifact | Existing suite 未覆盖授权 transfer |
| [`AC-WR-AOS-005`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-005) Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | [DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | 资格、容量、资源、治理、反滥用五门槛逐项 stale/fail-before-effect；无部分转让，转让不新增权限 | 使用 [`agent_claims.rs`](../../../crates/oasis7_runtime/src/tests/agent_claims.rs) 加每门槛拒绝与原子性场景 | PR 验证记录 + `test_tier_full` artifact | 门槛语义由 gameplay owner 冻结 |
| authority PRD §6.2 [`AC-5`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-5) | [DES-WR-IA-012](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-012) | Agent objection、valid owner override、world/safety hard block 在 receipt 区分；hard block 不可 override | 使用 [`agent_intent_v2.rs`](../../../crates/oasis7_runtime/src/tests/agent_intent_v2.rs) hard-block baseline 加 objection/override matrix | PR 验证记录 + `test_tier_full` artifact | 当前 hard-block test 不单独证明 override |
| authority PRD §6.2 [`AC-6`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#ac-6) | [DES-WR-IA-012](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-012) | 归因来源包含适用 issuer、owner、organization、Agent 与载体；owner 主要责任，组织按授权/强制/受益共同承担，Agent 不替罪 | 使用 [`agent_cognition_live_harness.rs`](../../../crates/oasis7/src/simulator/tests/agent_cognition_live_harness.rs) receipt-lineage baseline 加 source/actor projection cases | PR 验证记录 + full-tier artifact | 产品责任语义不是法律判责 |
| [`REQ-AGENT-AUTH-001`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001) Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad | [已采纳授权 profile](indirect-control-agency-execution-and-continuation.design.md#agent-authority-adopted-profiles) | native 与普通组织来源分别核验；原键 D/O/C、当前读面和控制变化不扩大范围或洗除责任 | [AD01](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-authority)：原 §6.0 native 同向量方法与 CF09/11/13/15/16/17 的准确适用接收 | 原来源/签名、scope、usage/领域原件与当前受众的同根差分 | 未实施 adapter 不算正式可达，不跨独立 grants 聚合 |
| [`REQ-AGENT-AUTH-002`](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002) Trace refs: O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a | [处置控制与历史消费](indirect-control-agency-execution-and-continuation.design.md#agent-disposition-control-history-consumer) | 旧结果/配置 provenance 保留，新 owner 无旧私有上下文，当前三端读权与三态恢复保持一次效果 | [AD05](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-history)：原 V/R、新 V2、当前受众与 sidecar 落后；同手册 AD07 核完整候选/旧 World fence 和原件恢复 | 实际配置来源、不可变引用、当前读权、原决定及 candidate/base generation | runtime-instance ACK 不证明 durable 历史，ordinary Committed 不代正式最终性 |
| [`REQ-WR-AOS-005`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005) Trace refs: O7-REQ-42168149-c979-4211-9777-2d72f7050147 | [处置控制与历史消费](indirect-control-agency-execution-and-continuation.design.md#agent-disposition-control-history-consumer) | 唯一生效输出连接 owner/source/generation/task/executor，旧 pending 不重放或转扣新方 | [owner 生效与待决](../../testing/manual/agent-economic-disposition-acceptance.manual.md#agent-disposition-owner)：五门槛原结果、各 fence 与 accepted/committed 相邻顺序 | 原领域 receipt 和完整控制差分 | 五门槛/经济规则仍由处置 producer 拥有 |

### 11.1.1 已有实现的直接回归落点

这些是验证入口与覆盖边界，不是当前候选的 pass 声明；实际结果仍绑定 PR/CI。

| 条款 | 直接回归 | 覆盖与限制 |
| --- | --- | --- |
| DES-WR-IA-010/011 | [`agent_delegation.rs`](../../../crates/oasis7_runtime/src/tests/agent_delegation.rs) | native 累计 debit、并发超额、retry/restart、revoke/expiry/owner source、quote 防伪和 unsupported scope；不证明组织授权或完整经济转让 |
| DES-WR-IA-012 | [`dissent.rs`](../../../crates/oasis7_runtime/src/tests/agent_delegation/dissent.rs) 及 `agent_delegation.rs` owner-control cases | 有效授权下 dissent 不额外要求 override，保留异议/receipt 且恢复重试不重复扣账；hard boundary/revocation 仍拒绝；显式 override 仍认证且 one-shot |
| DES-WR-IA-007/008 | [`agent_memory_correction_control.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/agent_memory_correction_control.rs) | owner 签名、privacy、Inspect/snapshot 同源、纠正接受与 matching committed action 区分及 lineage reload；mock provider 不证明真实模型行为 |
| DES-WR-IA-007/008 | [`control_proof_panel.test.jsx`](../../../crates/oasis7_viewer/software_safe_src/control_proof_panel.test.jsx)、[`viewer_feedback_module.test.js`](../../../crates/oasis7_viewer/software_safe_src/viewer_feedback_module.test.js) | UI 因果身份、预测/实际结果、纠正/授权字段与 privacy 投影；组件测试不替代外部浏览器验收 |

### 11.2 本次交付连接

设计文件是长期系统合同，不固定到某次 delivery 的 Issue、HEAD、PR 或 verdict。每次实现由 PR 绑定具体 source/integration/tested tree、验收证据与评审结果，不依赖 Project 或额外任务标识。

## 12. 决策、长期风险与未决问题

| 决策 ID | 选择 | 未选择 | 理由与失效触发 |
| --- | --- | --- | --- |
| ADR-GAME-IA-001 | 以统一 causal chain 串联多组件 | 各入口自行组装控制叙事 | 防止 accepted/applied 和主因果漂移；authority 模型改变时复核 |
| ADR-GAME-IA-002 | 设计条款使用稳定 `DES-WR-IA-*` | 直接用 Task/TASK-GAME 标签充当设计身份 | 长期设计与一次交付生命周期分离；ID 规范变更时迁移并留映射 |
| ADR-GAME-IA-003 | parity 定义为语义 invariant 等价 | 强制 UI/API payload 相同 | 保留入口实现自由；任一入口无法证明四类 invariant 时失效 |
| ADR-GAME-IA-004 | gameplay 分类映射 runtime disposition | 在本文新增 runtime enum | 尊重 runtime authority；现有 disposition 无法表达义务时另开实现 task |

长期风险与未决问题：

- 哪些意图类型、入口、版本和环境进入第一批可验证范围，由 `producer_system_designer` 联合各专业 owner 在实际 task 中冻结；未绑定者保持 unsupported/unknown。
- 具体 intent/idempotency/continuation identity、bounded-response 时间窗口和 receipt schema 由对应专业 authority 设计并版本化；在此之前，本文不声称已实现。
- Gameplay PRD 的 acceptance 与 NFR 已补稳定 HTML anchor；如果后续出现逐条机器消费需求，再由 gameplay owner 为 AC/NFR 单项补充稳定 anchor 与迁移映射。
- 最大风险是只补 UI 文案或只恢复文档，却没有 canonical truth 与同候选证据；这种状态必须继续报告为设计已定义、实现未验证。当前测试只覆盖 canonical intent、override/reprioritize、wait quote、continuation 和 memory receipt 的若干切面；完整 causal receipt 与同候选 Viewer/pure API parity 仍未证明。
