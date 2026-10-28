# Local Provider 与内置 Agent 体验等价（parity）验收方案（2026-03-12）

- 对应设计文档: `doc/world-simulator/llm/provider-agent-experience-parity.design.md`
- 专题入口与权威边界: `doc/world-simulator/llm/README.md`
- 上游产品目标: [外部 Agent Runtime 自主游戏](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)

审计轮次: 2

## 1. Executive Summary
- Problem Statement: 现有 `Decision Provider` 与 `Local Provider(Local HTTP)` 方案已回答“如何接入”和“首期如何启动 PoC”，但尚未把“对玩家来说必须获得与内置 agent 层等价的游戏体验”写成硬性目标。若没有独立的 parity 目标、场景矩阵和阻断线，`Local Provider` 可能在技术上可接，却在体验上长期低于内置 agent。
- Proposed Solution: 新增 `Local Provider vs 内置 Agent 体验等价（parity）` 专题，定义体验等价的范围、分层指标、场景矩阵、通过线与阻断线，并将 `Local Provider(Local HTTP)` 的交付目标从“可玩 PoC”升级为“在指定场景下达到用户可感知等价”。本专题引用 `doc/world-simulator/prd/acceptance/provider-agent-parity-scenario-matrix.md`、`doc/world-simulator/prd/acceptance/provider-agent-parity-score-card.md`、`doc/world-simulator/prd/acceptance/provider-agent-parity-benchmark-protocol.md`、`doc/world-simulator/prd/acceptance/provider-agent-parity-aggregation-template.md` 与 `doc/world-simulator/llm/provider-agent-profile-oasis7_p0_low_freq_npc.md` 作为统一场景、评分、聚合与玩法口径模板。只有通过 parity 验收的 provider 才允许进入默认体验或更大范围试点。
- Success Criteria:
  - SC-1: 对首期纳入范围的场景，`Local Provider` 与内置 agent 的任务完成率差值不超过 5 个百分点。
  - SC-2: 对首期纳入范围的场景，真实在线 LLM provider 的 parity 时延采用“分层口径”而非单一绝对值硬门禁：行为等价硬门禁看 `relative_wait_gap`（Local Provider 相对 builtin 的 `median_extra_wait_ms_gap` / `p95_extra_wait_ms_gap`），发布/扩面附加门槛看 `latency_class`。
  - SC-3: `Local Provider` 的无效动作率、超时率、非法 schema 率均不得高于内置 agent 基线 2 倍以上，且绝对值必须低于阻断线。
  - SC-4: viewer/QA 对两类 provider 的 trace 可解释性与错误恢复路径保持一致，不出现“agent 直连 lane 下无法定位问题”的观测断层。
  - SC-5: 若行为等价已达标但 `latency_class` 仅达到 `B (experimental)`，则该 provider 只允许保留在 `experimental` 或受限试点，不得默认启用。
  - SC-6: 首期 `P0` parity 样本必须使用固定的 Local Provider 玩法 profile（当前默认 `oasis7_p0_low_freq_npc`；旧别名 `legacy_p0_low_freq_npc` 已移除），并在 summary / scorecard 中保留该 profile 标识，避免“同场景不同 skill”造成假性通过。
  - SC-7: 只有当行为等价硬门禁通过且 `latency_class` 达到 `A (default-candidate)` 时，才允许把该 provider 作为默认体验或推进更大范围扩面。

### 产品目标与场景适用边界

本专题拥有体验可比性、场景指标和扩面条件；“用户带着已有 Runtime 持续自主玩游戏”的完整旅程、首期 Runtime 范围和支持声明由[上游产品 PRD](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)定义。

本文 P0/P1/P2 是 parity 场景分层，不等于产品首期的范围划分。原 `oasis7_p0_low_freq_npc` 六动作 smoke 只证明该 profile 的结果，不能代替代表性首局的生产、恢复、回访或高层指导验收。

OpenClaw 与 Codex 分别保留真实 Runtime/version、model、adapter、profile、primary access mode 和 observation lane 的证据；一个 Runtime、mock 或 builtin 的通过不能为另一个 Runtime 代签。比较以声明场景的权威结果与恢复体验为准，不要求两者使用相同内部规划、私有记忆或工具实现。若基线或候选使用动作修正、脚本辅助等额外干预，必须标明并保持比较条件可核对，不能把辅助后的结果写成未经辅助的原生能力。本节不改动下文既有 SC、阈值或技术状态，也不将已有 smoke 结果升格为产品可用性。

### 当前验收边界（S7 bounded scope）

- native runtime-live 的 Builtin 与 ProviderBacked 已共用 `AsyncAgentRunner`；native `Wait` 经过 Harness current-context/proposal 校验、Runtime admission、durable wake selection/readback 后才能恢复。`WaitTicks` compatibility timer 仍不属于该 durable path。该基础设施状态不是 parity gate 通过，也不改变 P0/P1/P2 的目标条件。
- WASM/browser 本切片只接受 shared DTO/policy 类型的编译兼容性：`viewer::runtime_live` 以及 provider transport/async runner 模块均在 `cfg(not(target_arch = "wasm32"))` lane。没有新增 browser Local Provider service；compatibility timer 不能作为 native durable 或 parity release 证明。
- 当前 parity benchmark 是 simulator-world smoke/sample evidence，机器字段必须保持 `execution_authority=simulator_world_kernel`、`runtime_certification_status=not_certified`。任何 per-agent Runtime shadow receipt 都属于独立 Runtime/world authority，不是同一 simulator world 的 Runtime certification。
- `benchmark_status` 只表示执行/sample coverage；机器 `parity_status`/`release_gate` 不能替代真实 Local Provider paired artifact、Runtime receipt/journal 的 failure/restart/reconnect 证据，亦不能替代 QA 与 producer 双视角 scorecard。缺少这些证据时不得把 provider 标为 `proven`、release-ready 或 default-enabled。
- P0 继续使用已有 observation fixture、scenario script 与 action semantics。本专题不新增 unified world/observation 或 `Speak`/`Inspect`/`Interact` surface；该产品扩展按用户决定 deferred。P0/P1/P2 的 durable target 与完整验收条件保持不变。

## 2. User Experience & Functionality
- User Personas:
  - 玩家 / 制作人：希望切换到 `Local Provider` 后，不明显感觉到 agent 变笨、变慢、变脆弱。
  - `agent_engineer`：需要一套清晰的 parity 通过线，判断外部 provider 能否取代内置实现进入主体验。
  - `qa_engineer`：需要标准化场景、评分项和阻断结论模板，避免只凭主观印象判断“差不多”。
  - `producer_system_designer`：需要把“体验等价”作为产品目标，而不是纯技术可行性。
- User Scenarios & Frequency:
  - 版本候选评审：每次计划扩大 `Local Provider` 覆盖范围前，至少执行一轮 parity 评审。
  - 核心玩法回归：每次 Local Provider adapter、动作白名单、记忆注入、profile/skill 协议或 prompt 协议变化后执行。
  - 发布阻断判定：当目标场景未达 parity 时，明确给出“不允许默认启用”的结论。
- User Stories:
  - PRD-WORLD_SIMULATOR-038: As a 玩家 / 制作人, I want `Local Provider`-driven agents to feel equivalent to built-in agents in scoped gameplay scenarios, so that switching provider does not noticeably degrade the game experience.
  - PRD-WORLD_SIMULATOR-038A: As an `agent_engineer`, I want parity harnesses and provider requests to carry a stable gameplay profile / skill id, so that benchmark evidence reflects a fixed玩法口径 instead of accidental prompt drift.
- Critical User Flows:
  1. Flow-PARITY-001（单场景对标）:
     `同一 observation fixture / 场景脚本 -> 内置 agent 运行 -> Local Provider provider（固定 agent_profile）运行 -> 对比完成率、等待差值、无效动作率、非法 schema 率、trace 完整度`。
  2. Flow-PARITY-002（玩家试玩盲测）:
     `制作人或 QA 使用同一场景分别试玩 builtin / agent_direct_connect -> 记录主观评分与关键阻断差异 -> 汇总 parity 结论`。
  3. Flow-PARITY-003（阻断判定）:
     `行为等价硬门禁未通过 -> 标记为 parity failed -> provider 保持 experimental；行为等价通过但 latency_class=B -> 仅允许受限试点，不进入默认体验`。
  4. Flow-PARITY-004（扩面准入）:
     `P0 行为等价通过 -> 扩到多轮记忆场景 -> 再扩到多 agent 并发；默认启用仍需单独满足 latency_class=A`。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| Parity 场景集 | `scenario_id/tier/goal` | QA 选择场景集执行对标 | `pending -> running -> passed/failed` | 先单 agent，再多轮记忆，再多 agent | QA / producer 可审阅 |
| 行为质量对标 | `completion_rate/invalid_action_rate/illegal_schema_rate/timeout_rate/retry_count` | 自动对比 builtin vs Local Provider | `bench_done -> compared` | 使用同批 observation/seed | 只读指标 |
| 时延体感对标 | `relative_wait_gap_ms/latency_class` | 记录玩家可感知等待 | `sampled -> aggregated` | 先看与 builtin 的相对差值，再判定 `A/B/C` rollout class | 只读指标 |
| 记忆连续性对标 | `memory_hit_quality/context_drift_count` | 检查多轮行为是否连续 | `session_done -> reviewed` | 按对话/任务回合统计 | 只读指标 |
| 观测与恢复对标 | `trace_completeness/error_recoverability` | QA 校验能否解释问题并恢复 | `issue_seen -> diagnosed -> recovered/blocked` | 缺任一观测面视为失败 | QA 裁定 |
| 发布准入 | `parity_status/release_gate` | 仅 parity 通过才允许默认启用 | `experimental -> gated -> default_ready` | 任一阻断线触发则回退 experimental | producer 决策 |
- Acceptance Criteria:
  - AC-1: 文档定义“体验等价”的正式口径：以玩家可感知行为结果、等待体感、记忆连续性、错误恢复与可解释性为主，不要求内部实现完全一致。
  - AC-2: 文档定义至少 3 层 parity 场景：`P0 低频单 NPC`、`P1 多轮记忆/对话`、`P2 多 agent 并发`。
  - AC-3: 文档为每层场景给出行为等价硬门禁与发布/默认启用附加门槛；未通过时必须明确保持 `experimental`，不得默默视作“差不多可用”。
  - AC-4: 文档定义 builtin 与 Local Provider 的对标指标与采样方法，避免不同输入条件导致比较失真。
  - AC-5: 文档要求 QA/producer 双视角输出 parity 结论：自动指标 + 主观试玩评分。
  - AC-6: 文档要求 `Local Provider(Local HTTP)` 专题与 `Decision Provider` 专题后续任务都以 parity 为上线目标，而非仅以“接通”作为完成条件。
  - AC-7: 文档明确 native common `AsyncAgentRunner` 与 Wait→Harness→Runtime admission→wake wiring 只是当前基础设施状态，不替代 parity gate、Runtime paired proof 或 P0/P1/P2 target。
  - AC-8: 文档明确 benchmark 的 simulator-world authority、`runtime_certification_status=not_certified`、WASM compile-only 边界、deferred unified interaction surface，以及真实 remote paired/restart/reconnect 与 QA/producer scorecard 均是独立证据条件；缺任一时不得声明 release/default。
- Non-Goals:
  - 不要求 `Local Provider` 与内置 agent 在内部 prompt、工具栈或 memory backend 上实现完全一致。
  - 原低频 NPC P0 parity 不涵盖高频战斗或完整经济关键路径；该范围不免除上游产品代表性首局的生产结果要求。
  - 不在本专题直接定义 `Local Provider` 的安装包分发与商业化方案。

## 3. AI System Requirements (If Applicable)
- Tool Requirements:
  - parity 评估必须可读取 `AgentDecisionTrace`、provider diagnostics、错误码与动作结果。
  - 若使用真实 `Local Provider`，必须能导出最小 trace 摘要，以保证可解释性比较。
- Evaluation Strategy:
  - `test_tier_required`: fixture 对标、mock provider、自动指标比较、错误恢复演练。
  - `test_tier_full`: 真实 `Local Provider(Local HTTP)` 低频 NPC + 多轮记忆试玩，输出自动指标与主观评分卡。
  - `test_tier_required` 的 mock/loopback 输出只证明 schema、identity、顺序、聚合与 negative fixtures；`test_tier_full` 仍要求同一 fixture/profile 的真实 provider paired run、Runtime receipt/journal failure/restart/reconnect evidence 与 QA/producer scorecard。

## 4. Technical Specifications
- Architecture Overview:
  - parity 评估层建立在 `Decision Provider` 与 `Local Provider(Local HTTP)` 方案之上，不新增 provider 传输层。
  - 使用统一 `scenario fixture + observation log + trace collector + score aggregator` 对 builtin 和 Local Provider 两条链路做对照。
- Integration Points:
  - `doc/world-simulator/llm/decision-provider-contract.prd.md`
  - `doc/world-simulator/llm/provider-loopback-http-contract.prd.md`
  - `doc/world-simulator/prd/acceptance/provider-agent-parity-scenario-matrix.md`
  - `doc/world-simulator/prd/acceptance/provider-agent-parity-score-card.md`
  - `doc/world-simulator/prd/acceptance/provider-agent-parity-benchmark-protocol.md`
  - `doc/world-simulator/prd/acceptance/provider-agent-parity-aggregation-template.md`
- `crates/oasis7/src/simulator/agent.rs`
- `crates/oasis7/src/simulator/memory.rs`
- `crates/oasis7_proto/src/viewer.rs`
- Parity Scope Definition:
  - `P0 低频单 NPC`：移动、观察、对话、简单交互。
  - `P1 多轮记忆/对话`：跨 3~5 轮任务目标保持、失败后重试、上下文连续性。
  - `P2 多 agent 并发`：2~5 个低频 agent 并发，不要求高频战斗 parity。
  - 上述场景使用已有 fixture/observation/action semantics；unified world/observation 与 `Speak`/`Inspect`/`Interact` 新 surface 不在本轮 parity scope，待后续产品决策与对应 authority 合同后再纳入。
<a id="parity-metric-object-contract"></a>

- Metrics & Thresholds:
  - 行为等价硬门禁:
    - `completion_rate_gap <= 5pp`
    - `invalid_action_rate <= 3%` 且不超过 builtin 2 倍
    - `illegal_schema_rate <= 3%` 且不超过 builtin 2 倍
    - `timeout_rate <= 2%`
    - `relative_wait_gap_median <= 5000ms`
    - `relative_wait_gap_p95 <= 8000ms`
    - `trace_completeness >= 95%`
    - `recoverable_error_resolution_rate >= 90%`
  - `recoverable_error_resolution_rate` 的 canonical metric contract（`v1`）:
    - 统计单位是一个由 Harness/Runtime trace 识别的 `recoverable_error` occurrence，不是
      sample 数、`goal_completed` 数或 provider 自报的“已恢复”。`denominator` 是所有有效样本
      中的 recoverable error occurrence，包含最终 timeout、failed、rejected 或没有后续恢复事件的
      occurrence；`numerator` 是其中存在一个合法、有序、同一 recovery chain 关联的
      `recovery_resolved` event 的 occurrence，每个 `error_id` 最多计一次。
    - 合法恢复必须由 benchmark host/Runtime 产生，而不是 provider transcript：事件必须与同一
      `sample_id`、`agent_id`、`agent_session_id`、`recovery_chain_id` 和 `error_id` 关联，且
      `recovery_resolved.event_seq > recoverable_error.event_seq`。恢复可以跨新的 retry turn，
      但必须携带 `origin_turn_id`/`origin_request_digest`，并有 Runtime/fixture-host authority
      reference；被拒绝、过期、pending、重复、串 Agent、错误链或乱序事件不计入 numerator。
    - `value = numerator / denominator`。当 `denominator == 0` 时固定输出
      `value: null`、`zero_case: "not_applicable"`、`gate_status: "not_evaluable"`；当
      `denominator > 0` 时 `zero_case` 必须为 `null`、`gate_status` 必须为 `evaluable`。
      绝不输出 zero-case `1.0`，也不能以 zero-case 通过恢复门禁。缺失/非法事件 schema 是
      `blocked`，不是从 denominator 中静默排除。

    - `illegal_schema_rate` 是独立于 `invalid_action_rate` 的 machine-readable metric object：只统计
      `error_counts.invalid_action_schema` 对应的非法 schema 失败，sample 级 `numerator` 为
      `illegal_schema_count`，`denominator` 固定为该 sample 的 `decision_steps`。每个 sample 必须
      输出 `illegal_schema_count` 与包含 `numerator/denominator/value/zero_case/gate_status` 的
      `illegal_schema_rate`；聚合 summary 与 `combined.csv` 也必须保留同名 `illegal_schema_rate`。
      `illegal_schema_count` 超过 `decision_steps`、与 `invalid_action_schema` 计数不一致，或任一
      必需字段/metric object 缺失时，sample 与批次均为 `blocked`，不得静默按缺失为零或缩小
      denominator。`decision_steps == 0` 时沿用 `zero_case: "not_applicable"` 与
      `gate_status: "not_evaluable"`，不能作为通过证据。

    benchmark 实现者必须在每个样本的 summary 中输出以下 machine-readable shape（字段名和
    enum 固定；`trace_validity` 为 `valid | invalid_fixture | blocked`；`event_seq` 由
    host/trace collector 分配并在 sample 内严格递增）。旧的 scalar
    `recoverable_error_resolution_rate` 不足以作为 gate 证据，必须输出这个 object：

    ```json
    {
      "metric_schema_version": "recoverable_error_resolution_rate.v1",
      "sample_id": "sample-001",
      "trace_validity": "valid",
      "decision_steps": 1,
      "illegal_schema_count": 0,
      "illegal_schema_rate": {
        "numerator": 0,
        "denominator": 1,
        "value": 0.0,
        "zero_case": null,
        "gate_status": "evaluable"
      },
      "recoverable_error_count": 1,
      "recovery_events": [
        {
          "event_kind": "recoverable_error",
          "event_seq": 4,
          "error_id": "error-001",
          "error_code": "timeout",
          "sample_id": "sample-001",
          "agent_id": "agent-0",
          "agent_session_id": "session-001",
          "recovery_chain_id": "chain-001",
          "agent_turn_id": "turn-002",
          "decision_request_id": "request-002",
          "request_digest": "blake3:0000000000000000000000000000000000000000000000000000000000000000"
        },
        {
          "event_kind": "recovery_resolved",
          "event_seq": 6,
          "error_id": "error-001",
          "sample_id": "sample-001",
          "agent_id": "agent-0",
          "agent_session_id": "session-001",
          "recovery_chain_id": "chain-001",
          "agent_turn_id": "turn-003",
          "decision_request_id": "request-003",
          "request_digest": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "retry_seq": 2,
          "origin_turn_id": "turn-002",
          "origin_request_digest": "blake3:0000000000000000000000000000000000000000000000000000000000000000",
          "authority": "runtime_or_fixture_host",
          "runtime_outcome": "action_committed",
          "authority_ref": "receipt-or-recovery-record-001"
        }
      ],
      "recoverable_error_resolution_rate": {
        "numerator": 1,
        "denominator": 1,
        "value": 1.0,
        "zero_case": null,
        "gate_status": "evaluable"
      }
    }
    ```

    `runtime_outcome` 允许 `action_committed` 或 `next_turn_admitted`；后者仍必须是 host/Runtime
    权威的后续合法 cognition turn，不得由 Wait、目标字符串或 `goal_completed=true` 单独替代。
    示例中的 `blake3:` 值是格式有效的 64-hex 占位 digest；真实样本必须写入对应请求的规范 digest，
    且 resolved event 的 `request_digest` 必须与 origin request 不同、`origin_request_digest` 必须与
    originating error 的 `request_digest` 相同。
    聚合结果必须对所有有效样本累加 numerator/denominator 后再计算 ratio，不得按 sample 的
    `goal_completed` 反推恢复率；聚合 summary 也必须保留同名 object 及 `gate_status`。

    benchmark 必须至少通过以下验收案例（示例均假设其它 parity gate 已满足）：

    | Case | 输入事件 | numerator / denominator | value / gate_status | P0-005 结果 |
    | --- | --- | --- | --- | --- |
    | `P0-005-happy` | 10 个样本各有 1 个 timeout，均有严格后序、同链、host/Runtime-authorized `recovery_resolved` | `10 / 10` | `1.0 / evaluable` | 可通过恢复项 |
    | `P0-005-unresolved-timeout` | 10 个样本中 9 个有合法恢复，1 个 timeout 后没有 recovery event | `9 / 10` | `0.9 / evaluable` | **必须 failed**；P0-005 要求 `numerator == denominator` |
    | `P0-005-goal-flag-only` | 1 个 timeout，无后续合法恢复，但 summary 错误地写 `goal_completed=true` | `0 / 1` | `0.0 / evaluable` | **必须 failed**；goal flag 不能代替 recovery event |
    | `no-recoverable-error` | 有效样本中没有 recoverable error | `0 / 0` | `null / not_evaluable` | 不得以 zero-case 通过 |
    | `malformed-or-out-of-order` | 缺 `authority_ref`，或 recovery 的 `event_seq <= error.event_seq`，或 chain/Agent 不匹配 | 不产出可通过的 ratio | `blocked` | **必须 blocked**，不得缩小 denominator |
    | `illegal-schema-metric-missing` | sample 缺少 `illegal_schema_count` 或 `illegal_schema_rate`，或两者与 `decision_steps` 不一致 | 不产出可通过的 ratio | `blocked` | **必须 blocked**，不得按缺失为零 |

  - `P0-005`（拒绝路径恢复）的验收是比全局 90% 更严格的 scenario gate：每个有效样本必须
    注入且只注入一个 recoverable error，故 `denominator == valid_sample_count`；每个 error
    都必须有严格后序的合法 `recovery_resolved` event，故 `numerator == denominator`，并且
    不得存在 unresolved error。timeout 后没有任何合法后续恢复，即使样本最终
    `goal_completed=true`，也必须标记 sample/scenario `failed`，不能标记 `passed`。
  - 设计验收与发布证据分开：deterministic mock/loopback 只可验收上述 schema、顺序、聚合、
    zero-case 和 P0-005 negative fixtures；它不能证明真实 provider 的行为等价、Runtime
    paired receipt/recovery、restart/reconnect 后无重复副作用，也不能把 capability 标为
    `proven` 或把 provider 提升为默认启用。发布/扩面仍需 builtin 与真实
    `Local Provider(Local HTTP)` 同 fixture/profile 的 paired artifact，以及包含 failure/restart/
    reconnect 的 Runtime/Journal 证据。
  - 发布/默认启用附加门槛:
    - `latency_class A (default-candidate)`: Local Provider `median_extra_wait_ms <= 500ms` 且 `p95_extra_wait_ms <= 1500ms`
    - `latency_class B (experimental-only)`: Local Provider `median_extra_wait_ms <= 15000ms` 且 `p95_extra_wait_ms <= 20000ms`
    - `latency_class C (blocked)`: 超出 `B` 的上限，不允许默认启用，也不允许进入更大范围扩面
- Edge Cases & Error Handling:
  - 若 builtin 和 Local Provider 场景输入不一致，则该轮样本作废，不计入 parity 结论。
  - 若 `Local Provider` 请求成功但返回持续 `Wait` 造成任务停滞，应计入 completion gap，而不是简单视作“无错误”。
  - 若 builtin 本身也慢于 `latency_class A`，不得据此直接判定 parity 失败；应先看 `relative_wait_gap` 是否满足行为等价硬门禁，再看 Local Provider 是否仅可保留在 `experimental`。
  - 若用户主观评分与自动指标明显冲突，必须要求 `qa_engineer` 输出失败签名解释，不得只采信单一维度。
  - recoverable error event 缺少 `error_id`、authority reference、严格顺序或 recovery-chain
    identity 时，整条样本/批次进入 `blocked`；实现不得通过缩小 denominator 将其变成通过。
  - 非法 schema metric 缺少 `illegal_schema_count`/`illegal_schema_rate`，或声明值与
    `decision_steps`、`error_counts.invalid_action_schema` 不一致时，整条样本/批次进入 `blocked`；
    实现不得把缺失 metric 当作零。
- Non-Functional Requirements:
  - NFR-1: parity 评估脚本必须支持固定 fixture / seed / timeout，以保证对比可复现。
  - NFR-2: 主观评分卡必须与自动指标一并归档，不允许只有截图结论没有数值。
  - NFR-3: parity 结果必须能追溯到具体 provider 版本、adapter 版本、协议版本与 `agent_profile`。
- Security & Privacy:
  - parity 采样不得输出用户本地 token、敏感 provider 配置或完整私密会话内容。

## 5. Risks & Roadmap
- Phased Rollout:
  - M1 (2026-03-12): 定义 parity 目标、指标与场景层级。
  - M2: 为 `P0` 场景落地自动对标与主观评分卡。
  - M3: `P0` 行为等价硬门禁通过后推进 `P1` 多轮记忆 parity；若仅达到 `latency_class B`，继续保持 `experimental`。
  - M4: `P1` 通过后推进 `P2` 多 agent 并发 parity。
  - M5: 仅当目标层级行为等价通过且 `latency_class` 达到 `A`，才允许扩大默认覆盖范围。
- Technical Risks:
  - 风险-1: 若 observation / trace 抽样不足，可能出现“指标看起来接近，体验仍明显不同”的误判。
  - 风险-2: Local Provider 延迟与外部模型波动可能让 parity 难以长期稳定维持。
  - 风险-3: 若只做低频场景 parity，就过早宣称“整体等价”，会产生口径风险。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-WORLD_SIMULATOR-038 | TASK-WORLD_SIMULATOR-114 | `test_tier_required` | `./scripts/doc-governance-check.sh` | 模块文档入口、parity 目标建模、上线口径 |
| PRD-WORLD_SIMULATOR-038 | T1/T2/T3 | `test_tier_required` | fixture benchmark + score aggregation + error recovery drills | parity 指标、场景覆盖、失败签名 |
| PRD-WORLD_SIMULATOR-038 | TASK-WORLD_SIMULATOR-157 | `test_tier_required` | `./scripts/doc-governance-check.sh` + `fix3` evidence rescore | 分层 latency gate、行为等价复签、默认启用口径 |
| PRD-WORLD_SIMULATOR-038 | T4/T5 | `test_tier_full` | 真实 `Local Provider(Local HTTP)` 对标试玩 + QA/producer 评分卡 | 体验等价结论、默认启用准入 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-PARITY-001 | 把“体验等价”定义为独立专题和上线门禁 | 仅保留“能接入/能运行”的技术目标 | 技术接通不等于玩家体验等价，必须有独立准入标准。 |
| DEC-PARITY-002 | parity 以玩家可感知结果和恢复体验为准 | 要求内部实现、prompt、memory 完全一致 | 产品目标是体验等价，不是实现完全同构。 |
| DEC-PARITY-003 | 分层推进 `P0 -> P1 -> P2` parity | 一次性宣称全量 agent parity | 分层更可验证，也更符合当前 Local Provider 首期低频接入边界。 |
| DEC-PARITY-004 | 对真实在线 LLM provider 采用“行为等价硬门禁 + rollout latency_class”双层口径 | 继续用单一绝对时延阈值作为 parity fail | 在线模型时延存在天然波动；产品上应把“能否默认启用”与“是否已经行为等价”分开治理。 |

时延的采样边界、保守聚合、历史 extra_wait 别名和缺样本解释统一见[benchmark 时延合同](../prd/acceptance/provider-agent-parity-benchmark-protocol.md#parity-latency-contract)；现有阈值不因文档澄清而改变。
