# Continuous Agent Harness：连续认知与 provider-neutral 决策设计（2026-08-31）

- 对应需求文档：`doc/world-simulator/llm/continuous-agent-harness.prd.md`
- 上游 provider 契约：`doc/world-simulator/llm/decision-provider-contract.design.md`
- 专题入口：`doc/world-simulator/llm/README.md`

<a id="1-设计定位与状态"></a>
## 1. 设计定位与状态：目标与范围

本设计把 Agent 从“一次同步 decide 调用”提升为可关联、可隔离、可恢复的连续 cognition boundary。它不把 provider 变成 runtime，也不把已有 DTO、trace 或显式 constructor seam 误报成生产闭环。

| 状态 | 本设计口径 |
| --- | --- |
| `current` | native `LlmAgentBehavior` 已有 request-bound model/tool call admission、zero-is-deny、repair/transport retry 单调计数与 exhaustion-to-Wait；native runtime-live 已从 host prompt profile 构建并绑定 GoalSnapshot。simulator/兼容 `AgentBehavior::decide` 仍同步，legacy builtin execute-until continuation 保留内存语义；native runtime-live 的 Builtin/ProviderBacked 均使用 `AsyncAgentRunner` worker 与非阻塞 mailbox/result polling，另有诊断 trace 与 provider DTO。 |
| `partial` | native 普通 `Wait` 已有当前 context/Harness proposal、Runtime admission、projection/readback、Runtime wake selection/resume 和 Harness reconcile seam；admission failure 不冒充成功等待，`WaitTicks` 仍走本地 timer。`DecisionProvider`、loopback、capability context、`MemoryWriteIntent` 等接线与有界 fixtures 不证明完整 identity/反馈隔离/provider convergence、durable production scheduler、WASM/browser 服务、远端 restart/rebind/reorg、全量恢复或真实 provider parity。 |
| `target` | 本文的 Session/Turn/request identity、canonical digest、single in-flight、共同 adapter lifecycle、feedback/memory policy 和 bounded goal/continuation 合同。 |
| `proven` | 仅当 PRD verification matrix 的 required/full/rollout artifact 证明对应成功与负例，才可标记；本文不预先标记任何 target 为 proven。 |

<a id="2-组件与-ownership"></a>
## 2. 组件与 ownership：边界与结构

```text
                 ┌──────────────────────────────┐
                 │      Runtime authoritative    │
                 │ head / policy / action /     │
                 │ receipt / journal / replay   │
                 └──────────────┬───────────────┘
                                │ observation + receipt
                 ┌──────────────▼───────────────┐
                 │       Continuous Harness      │
                 │ session/turn, memory read,    │
                 │ goal/continuation, correlation│
                 │ single-flight, policy/trace   │
                 └──────────────┬───────────────┘
                                │ common DecisionProvider contract
                 ┌──────────────▼───────────────┐
                 │    Builtin / ProviderBacked   │
                 │      provider adapters        │
                 └──────────────────────────────┘
```

Harness owns cognition state and protocol safety. Runtime owns live truth and every authoritative side effect. Adapter owns transport, model selection, provider-specific transcript and bounded retry mechanics, but not identity authority or action execution.

Native `RuntimeDecisionRunner::{Builtin, ProviderBacked}` already shares the async runner and ordinary-`Wait` admission/wake/resume seam; simulator compatibility and wasm32 `AgentRunner` retain their separate synchronous qualification. This observed wiring does not prove complete target identity, feedback, memory or recovery convergence. Both arms must satisfy the same target Harness lifecycle and verified memory, continuation and error contracts. The paired [Runtime current/partial qualification](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md#1-目标与边界) retains world/journal/scheduler authority.

Historical source locators were `viewer/runtime_live/llm_sidecar_runner.rs:211–229`, `llm_sidecar_async.rs:490–529`, and `llm_sidecar_cognition_wait.rs:419–592`; they are retained as provenance, not current line evidence. At fixed source `35961e32b7e7ad3469b7cd7db2e718fb1d6795b1`, inspect [`ensure_runner_initialized`](https://github.com/nil-studio/oasis7/blob/35961e32b7e7ad3469b7cd7db2e718fb1d6795b1/crates/oasis7/src/viewer/runtime_live/llm_sidecar_runner.rs#L246-L263) for native Builtin/ProviderBacked `AsyncAgentRunner` construction, [`next_async_provider_decision`](https://github.com/nil-studio/oasis7/blob/35961e32b7e7ad3469b7cd7db2e718fb1d6795b1/crates/oasis7/src/viewer/runtime_live/llm_sidecar_async.rs#L364-L399) for non-blocking `poll_completed`, and [`admit_provider_wait_continuation`](https://github.com/nil-studio/oasis7/blob/35961e32b7e7ad3469b7cd7db2e718fb1d6795b1/crates/oasis7/src/viewer/runtime_live/llm_sidecar_cognition_wait.rs) for the existing Wait admission seam. Located code and bounded fixtures are not execution, canonical-world progress, full durable recovery or proven evidence.

### 2.1 需求承接与分配表

This table only refines the existing infrastructure consumer acceptance and QA tier relations into six bounded product sub-obligations through the paired professional PRD receivers. Correlate same-world canonical receipt, pending/no-effect, memory/goal feedback, exclusive lineage and all SC9 Agent projections; record actual provider/lane provenance. Required active-provider API and full real-provider headed desktop+narrow plus Agent parity remain independent. No new memory/goal policy, scheduler, MVCC, digest or world authority is allocated here. All original lifecycle and verification sections remain applicable; product state cards are unchanged.

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405; [professional_acceptance: Finality/pending consumer](continuous-agent-harness.prd.md#harness-infrastructure-pending-receiver) | Correlate the same candidate/world/request and authoritative receipt; distinguish pending, rejected, stale, expired and effective results. A provider reply, queued status or transport ACK cannot establish completion, authoritative memory or world effect; recovery rejudges under current authority. | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Harness/Agent consumes correlated context; Runtime owns world/journal/receipt; gameplay owns quantities/W/window; Ops owns outage classification; QA owns scene/tier verdict. | No finality, scheduler, journal or complete recovery implementation claim. |
| Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6; [professional_acceptance: Exclusive lineage consumer](continuous-agent-harness.prd.md#harness-infrastructure-lineage-receiver) | Retain ordered intent and original/withdraw/replace association, root/revision/child/direct-parent receipt. For professionally marked exclusive members, consume the first effective winner and the others' zero-effect terminal results; rejected/expired members do not cancel independent intents. | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Harness/Agent consumes correlated context; Runtime owns world/journal/receipt; gameplay owns quantities/W/window; Ops owns outage classification; QA owns scene/tier verdict. | No new lineage marking, canonical ordering, atomic disposition or implicit withdrawal/replacement support. |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4; [professional_acceptance: Same-world recovery consumer](continuous-agent-harness.prd.md#harness-infrastructure-recovery-receiver) | Consume only matching world/manifest/snapshot/journal/receipt recovery material; preserve confirmed history and agent/session/turn/request/digest feedback correlation. Missing/conflicting recovery proof cannot become a recovered-world, goal-completion or authoritative memory claim. | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Harness/Agent consumes correlated context; Runtime owns world/journal/receipt; gameplay owns quantities/W/window; Ops owns outage classification; QA owns scene/tier verdict. | Consumer join only: not complete trust-chain validation, nonempty journal-suffix replay, new-request/module/action positive continuation, durable exactly-once or GoalGraph proof. |
| Trace refs: O7-REQ-05c84021-ee48-41ef-b94a-8aa9ceb8048c; [professional_acceptance: Industrial result consumer](continuous-agent-harness.prd.md#harness-infrastructure-industrial-receiver) | Join immutable root, revision/child/direct parent, authoritative receipt and Agent readback of held/consumed/unmet/residual quantities, W/window, progression, earliest blocker and next action/recheck. Do not confuse accepted with reserved, local completion or transit with production/delivery completion. | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Harness/Agent consumes correlated context; Runtime owns world/journal/receipt; gameplay owns quantities/W/window; Ops owns outage classification; QA owns scene/tier verdict. | No recipe, capacity, W/window, resource, progression or economic policy change; one local stage does not prove the complete industrial pipeline. |
| Trace refs: O7-REQ-4792541c-aaa4-4ac1-9a2d-374df44eba6a; [professional_acceptance: Eight-cell outage and tier consumer](continuous-agent-harness.prd.md#harness-infrastructure-outage-receiver) | Retain all eight SC9 authority/read × stage_finish/transit/buffer_admission/terminal_settlement cells; each authority cell keeps root-absent pre-finality and root-present post-commit/pre-child specimens. Read outage never mutates world/W/history; only canonical gameplay interruption may reset unfinished W once. Preserve quantities/window/receipt/progression/blocker/next_action/next_recheck parity. | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Harness/Agent consumes correlated context; Runtime owns world/journal/receipt; gameplay owns quantities/W/window; Ops owns outage classification; QA owns scene/tier verdict. | No missing-cell N/A, mock/history substitution, actual provider/browser execution, full/parity/readiness or release claim; Ops classifies the dependency and gameplay owns W/window. |
| Trace refs: O7-REQ-2b7cfef9-d917-46ea-a3a9-e3a4fb96d730; [professional_acceptance: Explicit world-scope consumer](continuous-agent-harness.prd.md#harness-infrastructure-world-scope-receiver) | Before submit distinguish actual global-authoritative versus isolated local/development world and match world/manifest across request, root, receipt and result. Missing/mismatched/reused receipt identity fails closed with zero global effect. | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Harness/Agent consumes correlated context; Runtime owns world/journal/receipt; gameplay owns quantities/W/window; Ops owns outage classification; QA owns scene/tier verdict. | Environment labels/local receipts do not prove global finality; no world-identity, receipt schema or authority expansion. |

## 3. Canonical data model

### 3.1 Session and turn

```text
AgentSession {
  agent_session_id,
  agent_subject,
  protocol_version,
  profile_revision,
  opened_from_runtime_head,
  status: active | draining | closed,
}

AgentTurn {
  agent_session_id,
  agent_turn_id,
  retry_seq,                         // semantic retry sequence
  transport_attempt,                 // delivery attempt; excluded from request digest
  base_runtime_head,
  observation_digest,
  memory_snapshot_digest,
  goal_snapshot_digest,
  continuation_digest,
  status: preparing | in_flight | awaiting_receipt | completed | rejected | cancelled | stale | failed | pending,
}
```

`agent_session_id`、`agent_turn_id` 和 `agent_subject` 必须由 trusted Harness host 唯一生成；Runtime 只验证、持久化这些 cognition IDs，并分配其自身的 journal/action/envelope/receipt/continuation/wake IDs。provider 不能把 session 当作可移植的 bearer authority，也不能在跨 Agent 时复用 session。

### 3.2 DecisionRequest / response additive outer context

Continuous Harness 不复制上游 decision-provider contract 的 request/response。既有 provider
DTO 作为 inner decision payload 保持兼容；Harness 增加版本化的
`ContinuousAgentRequestContextV1` outer context，绑定 session/turn/request identity、
runtime binding、digests、budget 和 policy。兼容 adapter 把 outer context 映射到既有
`DecisionRequest` 字段并在 response 外层回显 identity；旧 fixture 若没有该 context，只能
留在明确的 compatibility lane，不能满足 production async claim。target provider response
仍返回既有 tagged candidate/wait/query/module-response semantics，不产生第二套 action contract。

```text
ContinuousAgentRequestContextV1 {
  base_decision_request: ExistingDecisionRequestV1, // upstream DTO, unchanged
  context_discriminator: "oasis7.continuous-agent-context",
  context_version: 1,
  protocol_version,
  agent_session_id,
  agent_turn_id,
  decision_request_id,
  retry_seq,                         // semantic retry; new request/turn
  transport_attempt,                 // transport retry only; excluded from digest
  agent_subject,
  runtime_binding: {
    world_id, branch_id, finality_epoch, finality_block_hash?, finality_status,
    base_tick, base_world_hash, reorg_epoch, runtime_manifest_hash
  },
  observation_digest,
  capability_catalog_digest,
  capability_invocation_context_digest,
  memory_snapshot_digest,
  goal_snapshot_digest,
  continuation_digest,
  adapter_protocol_version,
  budget_contract,
  request_digest,                     // output; excluded from request digest input
}

ContinuousAgentResponseContextV1 {
  base_decision_response: ExistingDecisionResponseV1, // upstream DTO, unchanged
  context_discriminator,
  context_version,
  agent_session_id,
  agent_turn_id,
  decision_request_id,
  retry_seq,
  transport_attempt,
  request_digest,
}
```

`outcome`、candidate、memory intents、continuation update 和 trace 仍属于
`base_decision_response` 的既有 semantics；outer context 只增加身份、binding 和 digest，
不复制动作字段。inner `DecisionRequest/DecisionResponse` 的 identity 必须通过 outer context
完整匹配。provider 回传不同 identity、缺字段或未知 version 时，Harness 产生稳定
`response_identity_mismatch`，不尝试从自然语言或旧 response 推断动作。

Response identity has an independent digest version. The target semantic digest uses
`H_v1("oasis7.cognition.response.v2", canonical(decision, module_command, provider_error, memory_write_intents))`;
`diagnostics` and `trace_payload` are observability only and are excluded, including measured
latency, token and cost fields. The pre-fix full-`DecisionResponse` digest remains named
`oasis7.cognition.response.v1` only for explicit legacy classification. Target response validation
rejects that digest with `legacy_response_digest_unsupported`, and the local bridge feedback-state
schema is bumped so an old persisted state cannot be loaded as current lineage. There is no
automatic migration: the old stored digest has no self-describing version and response content is
not persisted, so a future migration must be an explicit, reviewed rewrite of the complete
response/artifact lineage. The outer context schema remains V1 because its fields are unchanged;
the response digest domain carries the independent algorithm version.

既有 inner DTO 的字段映射为：`observation`、`action_catalog`、`provider_config_ref`、
`timeout_budget` 保持在 `base_decision_request`；`decision/action_ref/args`、
`provider_error/diagnostics/trace_payload` 与 `memory_write_intents` 保持在
`base_decision_response`。outer 只添加 discriminator/version、cognition IDs、runtime
binding、digests、budget 与 policy。版本不支持或未知 authority 字段必须 fail closed；已知
非-authority diagnostics 可由兼容 adapter 丢弃且不进入 digest。legacy 无 outer context
只能标记 `legacy_no_cognition_proof`。

兼容 adapter 的 field-level mapping 固定为：

| ExistingDecisionRequest/Response field | V1 location | compatibility rule |
| --- | --- | --- |
| `observation`, `action_catalog` | `base_decision_request` + corresponding digests | semantics unchanged；snapshot/digest 由 trusted Harness host 提供。 |
| `provider_config_ref`, `timeout_budget` | `base_decision_request` + profile/budget fields | 缺 profile 不得静默换 profile；timeout 不单独构成 identity。 |
| `decision`, `action_ref`, `args` | `base_decision_response` | 保持 candidate/Wait/Rejected semantics，不复制动作字段。 |
| `module_command` | `base_decision_response` | 作为 typed module candidate；必须经 host schema encoding 与 Runtime validation，不是已执行 action。 |
| `provider_error`, `diagnostics`, `trace_payload` | `base_decision_response` + bounded outer trace | diagnostics 可按兼容策略丢弃，但不进入 identity digest。 |
| `memory_write_intents` | `base_decision_response` | 仍是 intent，必须经过 `MemoryWriteIntentPolicyV1` 和 Runtime outcome gate。 |

Target outer response 保留 `base_decision_response.decision` 的 tagged variant，不把 Query 或
module response 压扁为 `act`：

| Existing `ProviderDecision` variant | Target mapping | Harness/runtime semantic |
| --- | --- | --- |
| `Wait` / `WaitTicks { ticks }` | `base_decision_response.decision` | 无副作用等待；保留同一 outer identity。 |
| `Act { action_ref, action }` | `base_decision_response.decision` | candidate，仍须经 Runtime action validation。 |
| `Query { query_ref, query }` | `base_decision_response.decision` | 只读 query；使用当前 frozen world snapshot，产生 `AgentQueryResult`，不得生成 action/envelope、memory fact 或 world effect。 |
| `ModuleCommand { module_command }` | `base_decision_response.decision`；旧 adapter 可按明确 projection 放入 `base_decision_response.module_command?` | typed module candidate；没有 trusted executor 时收敛为 `Wait`/`module_command_unroutable`，不得伪装为执行成功。 |
| `ModuleCommandResponse { response }` | `base_decision_response.decision` | 完整 `AgentCommandResponse`；Harness/host 先校验 catalog、subject/audience、nonce/context，再交 Runtime；不转成 core `Action`。 |

`DecisionResponse.module_command?` 是 additive extension field，不能替代或覆盖 tagged
`ModuleCommandResponse`；outer wrapper 对两种模块 variant 都回显同一 cognition IDs/digest。
Query/result 与 typed module response 的结果仍属于该 turn 的 bounded response/feedback lineage，
不会绕过 `ContinuousAgentResponseContextV1` 或 Runtime receipt boundary。

`context_discriminator` 必须等于 `oasis7.continuous-agent-context`，`context_version=1` 仅在
对应 schema 已实现时协商；缺失 discriminator、未知 version 或未知 outer authority field
都 fail closed（分别为 `unsupported_context_version` 或 `unknown_context_field`）。未来
additive field 只能进入新版本并纳入 digest；旧 inner DTO 没有 outer context 时只记录
`legacy_no_cognition_proof`，不能进入 production async lane。

Legacy compatibility lane 必须由 adapter route 或 fixture/profile 显式选择；缺省和 production
async lane 均使用 target outer contract。该 lane 只允许把旧 DTO 映射到上述 outer fields；不得
把缺少 cognition proof 的 response 标成 target/proven。当前 HTTP wire 不定义
`compatibility_lane` 字段。任何 legacy heuristic fallback 仅限低风险 fixture/profile，
不得进入 production async claim、target parity 或自动记忆/continuation 语义。

### 3.3 FeedbackEnvelope

```text
FeedbackEnvelope {
  feedback_id,
  feedback_seq,
  agent_session_id,
  agent_turn_id,
  decision_request_id,
  candidate_action_id?,
  runtime_receipt_id?,
  status: committed | rejected | failed | pending,
  request_digest,
  envelope_digest?,
  reject_reason?,
  committed_event_summary?,
  world_delta_summary?,
  provenance: runtime_authoritative,
}
```

Runtime 负责声明 `committed/rejected/failed/pending`；stale、expired、authority、
precondition 与 idempotency conflict 都使用 `rejected` status 加稳定 `reject_reason`，而不是
发明第二套 status；没有世界效果使用 `rejected` 加 `reject_reason=no_effect`，而不是新的
status。其 disposition-to-feedback mapping 以 paired runtime lifecycle 文档为准
（见 [`agent-cognition-lifecycle.prd.md`](../../world-runtime/runtime/agent-cognition-lifecycle.prd.md)
与 [`agent-cognition-lifecycle.design.md`](../../world-runtime/runtime/agent-cognition-lifecycle.design.md)）。Harness 只负责匹配、脱敏、排序和进入下一次 cognition context。无 `runtime_authoritative` provenance 的 adapter transcript 不是反馈。

## 4. Canonical digest 与幂等算法

### 4.1 Digest domain

共享 `request_digest` 的 canonical domain 使用版本化、长度前缀的 deterministic encoding。它是 Harness 与 Runtime 共同消费的 cognition input digest；provider invocation key 只是它的 domain-separated 派生值。字段顺序为：

1. domain/version；
2. session/turn/request/subject identity；
3. world/branch/finality/base tick/head；
4. observation digest；
5. capability context/catalog digest；
6. memory retrieval snapshot digest；
7. GoalSnapshot 与 continuation digest；
8. provider config/profile revision 与 adapter protocol version；
9. normalized budget contract。

Object key 按 canonical byte order 排序；list 默认保留 schema 的语义顺序，只有 schema 明确声明
order-insensitive/set-like 时才按 canonical element bytes 排序；没有声明时不得猜测排序。字符串使用
normalized UTF-8；缺省值必须显式编码。wall-clock、transport endpoint、credentials、raw provider output、
日志顺序、`transport_attempt` 和本地 mailbox 地址都排除在 digest 外。

```text
request_digest_input = ContinuousAgentRequestContextV1 without request_digest and transport_attempt
canonical_request_bytes = canonical(request_digest_input)
request_digest = H_v1(
  domain="oasis7.cognition.request.v1",
  payload=canonical_request_bytes,
)
provider_invocation_key = H_v1(
  domain="oasis7.cognition.provider-invocation.v1",
  request_digest=request_digest,
)
```

`H_v1` 使用 paired runtime contract 冻结的 `blake3-256` registry entry：输入为
RFC 8949 deterministic CBOR 编码的 `[domain, payload]` 二元数组，输出为
`blake3:<64 lowercase hex>`。算法自检向量固定为
`BLAKE3-256(empty) = af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262`。
P0.1 必须再提交包含完整 `ContinuousAgentRequestContextV1` 的 immutable golden
fixture，并以不同 `transport_attempt` 值证明 canonical bytes、`request_digest` 和
`provider_invocation_key` 不变；在该 fixture 和 paired runtime 输出逐字节一致前，不得把
digest contract 标为 `proven`。

Runtime action/module envelope 另有 runtime-owned `envelope_digest/envelope_idempotency_key`，用于提交和副作用幂等；它不等于也不替代 `request_digest` 或 `provider_invocation_key`，具体定义留给 paired runtime docs。

Observation、memory、goal、capability schema 等大字段以已验证的 content digest 进入 identity；digest 的来源 bytes 必须可从 fixture 或 trusted host 复现。若 digest 版本、字段或 authority input 不可解释，拒绝 request，而不是使用兼容猜测。

### 4.2 Retry 与 collision

- transport retry 保持 session/turn/request id 和所有 canonical inputs 不变，`transport_attempt` 单独递增；
- semantic retry 必须递增 `retry_seq`，创建新的 turn/request，并保留对旧 turn/request 的 causal reference；
- adapter cache/dedupe 以 `provider_invocation_key` 保存终态或稳定 failure；
- same key/same digest 且结果已确定时返回原逻辑结果/稳定失败；未决 attempt 只按原预算恢复；same request id/different digest 生成 `request_identity_collision`，并使该 turn fail closed；
- 新 observation、runtime head、profile revision、goal/memory/capability digest 或 budget contract 变化必须开新 turn/request；
- 不允许再使用 `{session_key}-{timeout}` 作为最终 identity，timeout 变化也不能遮蔽逻辑 request 的其他输入。

<a id="harness-retryable-attempt-design"></a>
#### Attempt、结果缓存与反馈关联

承接 [PRD 的 retryable attempt 合同](continuous-agent-harness.prd.md#harness-retryable-attempt-outcome)。缓存应分别表达请求身份、未决尝试信息与可重放逻辑结果；下表是内部状态语义，不新增 HTTP wire enum。

| 内部语义 | 写入与读回 | 下次同 identity 请求 |
| --- | --- | --- |
| 未决 attempt | 保存已校验请求关联、原预算消耗和有界错误诊断；可计算本次响应摘要，但不得把它当作不可变最终摘要 | 校验全部原 canonical inputs，仅在 retryable 且预算尚余时允许下一个 attempt |
| 逻辑结果确定 | 合法 Wait/candidate 或稳定失败与其现行 v2 语义摘要一起固定；后续 Runtime receipt 仍是另一条权威结果 | 返回已固定结果；不再次调用上游，若外来结果语义不同则拒绝为 `response_identity_collision` |
| Harness/Runtime 已终结 | 取消、失效、不可重试或预算耗尽的 disposition 与原请求保持关联 | 不复活原 turn；按既有终态/迟到响应合同处理 |

`provider_error.retryable` 必须结合请求是否仍未决与剩余预算解释。含 retryable 错误的 fallback Wait 不能被误认成正常 Wait 的最终结果；没有 provider error 的合法 Wait 则不能被误认成尚待完成的 attempt。保留 `cognition_response_digest` 的现行 v2 算法及成功碰撞保护，不排除 `provider_error` 字段，也不扩大预算来掩盖错误→成功的恢复路径。

请求关联须覆盖未决 attempt 到最终结果的演进，不能以先冻结暂态错误摘要为接受后续 feedback 的前提。只有匹配的 Runtime feedback 能影响 context/memory/continuation；attempt 错误本身只作诊断。

配置反馈持久化时，请求状态、最终结果及关联的更新应按同一持久化边界发布；失败保留可重试的旧状态并给出明确错误，重启不得丢失已确定结果或重置预算。旧持久化 schema 的处理继续服从 §3.2 的显式版本规则，不通过猜测旧摘要重建逻辑结果。

离线验收通过真实 HTTP bridge 与 adapter、隔离 mock invoker：第一次可重试错误，第二次分别返回合法 Wait 和 Act；session/turn/request/request_digest 不变，仅 `transport_attempt` 增长，不能得到 collision/400。另证不可重试和耗尽停止、终态重放无新 invocation、不同成功语义仍碰撞，以及配置持久化后的 attempt 重启和最终反馈关联。这些样例不调用真实 provider。

### 4.3 C0 request-bound call budget

`budget_contract` 已经是 outer context 的输入。当前 C0 在 timeout/repair 字段之外已有
request-bound model/tool call admission；这不是 paid cognition。`BudgetContractV1` 只覆盖 native
`LlmAgentBehavior` lane，包含两个不带默认值的上限字段：

```text
BudgetContractV1 {
  max_model_calls,
  max_tool_calls,
  // existing timeout/repair fields remain part of compatibility evolution
}
```

它们是 trusted Harness host 绑定到一个 native logical request 的非负上限。当前 Runtime Live
host context 选择 `max_model_calls=4`、`max_tool_calls=3`，这是 host/profile policy bound，
不是 universal default 或玩法经济平衡；其他 host/profile 必须显式提供自己的值，或进入兼容
lane。值为 zero 就是拒绝该类调用，不能解释为 unlimited、缺省值或自动补充。旧 outer
context/legacy fixture 缺少这些 additive 字段时，adapter 必须显式选择 compatibility lane；
不能静默填充、升级为 C0 target，也不能把 legacy request 当成 paid economy。Rust wire evolution、
旧 snapshot 反序列化和 compatibility marker 由 Agent implementation slice 冻结，但必须保留
上述 fail-closed 语义。

V1 `budget_contract` 的字段集合固定为 `max_latency_ms`、`max_repair_attempts`、
`max_model_calls`、`max_tool_calls`。嵌套预算对象也必须拒绝未知字段；不能静默丢弃
`unlimited`、其他 limit alias 或未来 authority 字段后按较窄的合同继续调用。
缺失调用上限仍在 wire decode 拒绝；outer `validate_value` 对未知预算字段返回
`unknown_context_field`，不进入 executor、不产生 action 或 memory effect。新增字段需要
显式协议演进与兼容路径，不能把“可反序列化”当成 V1 认可。

C0 admission 的顺序和计数边界固定为：

1. 在 native lane 进入 `LlmCompletionClient::complete` 前检查并预留一个 model-call credit；
   在进入 `run_prompt_module` 前检查并预留一个 tool-call credit。预留失败时不得进入对应
   执行器。C0 不把一次 `decide` 推断为一次 model call。
2. 预留是同一 logical request 内单调的；每一次实际 model/module invocation 都必须先有
   对应预留，native adapter/model error、repair 和 transport retry 都不能重置或补充剩余预算。
   transport retry 仍沿用原 session/turn/request/digest；semantic retry 才创建新 turn/request
   并显式携带新 contract。
3. 无剩余 credit 时产生稳定 `budget_exhausted`，状态收口为无副作用 `Wait`。该路径不得
   再发起超限的 native provider/module call，不提交新的 candidate/action，不产生
   projected/durable `MemoryWriteIntent`，不改变 world、authority 或 Electricity/Data 余额。
   admission 前已经产生的本地 private `AgentMemory` observation/diagnostic bookkeeping 可以
   保留；它不是 authoritative memory commit、Runtime receipt 或玩家可见事实。
   `Pending`、backpressure、未启动 continuation 和 diagnostics 不消耗 credit。

当前 C0 implementation 不计量 opaque ProviderBacked adapter 内部的 model/tool calls、
provider-side retry 或 provider 自己的 tool loop，也不把外部 provider 的一次 `decide` 推断为
一次调用。ProviderBacked 的 self-metering、usage propagation 与 adapter contract 是后续
provider/runtime slice；在该 slice 到来前，外层 C0 fields 不能宣称覆盖这些内部调用。已有
`active_execute_until` 等不调用 provider/module 的 deterministic continuation 可以继续且不
消耗 call budget；只有它后续实际尝试进入上述 native executor 时才执行 admission。

`max_model_calls` 与 `max_tool_calls` 必须作为 normalized budget contract 进入
`request_digest`/`provider_invocation_key` 的输入；改变任一上限就是新的 logical request，
不得在原 request 上静默重绑。C0 的 counters/reservations 只是 Harness protocol guard，
token/latency/cost 仍是 diagnostics/evaluation inputs，不能被记录为 lease、quote、receipt
或 paid cognition。

C0 只提供 economy-neutral native admission seam。C1 需要 product/gameplay authority 冻结付费资源、
使用单位、owner/purpose/scope/authorization、quote/reserve/settle/release、expiry/refund 和
版本治理；C2 才由 Runtime/QA 冻结并证明 atomic lease/receipt、journal/replay 和 no-duplicate
side effects。本文不设 numeric rate；Native TurnEngine、dynamic ToolRegistry 与 durable
continuation convergence 只能在 C0 seam 可消费后分别接入，不得把 C0 解释为完整经济能力。

## 5. 单 Agent single in-flight 状态机

```text
Idle
  -> Preparing
  -> InFlight(request identity)
     -> Candidate
        -> AwaitingReceipt (candidate only)
           -> Completed | Rejected | Cancelled | Stale | Failed | Pending
     -> Wait -> Pending
     -> Rejected | Cancelled
```

实现约束：

- `InFlight` 下只能有一个活动 adapter attempt；retry 必须先结束上一次 attempt；
- single-flight mutex 的主键是 `agent_subject`：同一 Agent 同时至多一个 active session 和一个 active turn；session id 只用于分区，不能放宽该全局互斥。新 session 只有在旧 session `draining/closed` 或已完成显式 recovery handoff 后才能激活；
- `AwaitingReceipt` 下不能开启下一 turn，避免未确认 action 与下一次 observation 交叠；
- 新 observation 只能 coalesce 为下一 turn 的输入，不能覆盖 active request；若 authority head 已变化，active turn 进入 `Stale`；
- 不同 Agent 的 state machine 可并行，但所有队列、cache、feedback buffer 必须以 session/subject 分区；
- 任何重入都返回 `agent_busy` 或 `protocol_violation`，不会向 provider 发第二次逻辑调用；
- world 是否在 `InFlight` 期间推进属于 runtime scheduler 合同。Harness 只保持 turn identity 和 pending outcome，不假设同步阻塞 world。

### 5.1 Session/Turn lifecycle mapping

Harness 的 cognition 状态必须能映射到 Runtime 的生命周期，但不取代 Runtime 的真相：

| Harness state | Runtime state/disposition | 处理语义 |
| --- | --- | --- |
| `Preparing` | `queued` / `running` | Harness 已分配 session/turn/request IDs；Runtime 验证并持久化 envelope 后才允许运行。 |
| `InFlight` | `waiting_provider` | 一个逻辑 request 只能有一个活动 provider attempt；Runtime lease 仍有效。 |
| `Candidate` | `response_recorded` / `ready_to_submit` | Harness 已规范化 candidate；Runtime 仍须验证 authority、precondition 和 envelope。 |
| `AwaitingReceipt` | `accepted` / `rejected_*` / `failed_*` / `recovery_pending` | 等待 Runtime authoritative disposition；不得以 provider transcript 当 receipt。 |
| `Wait` | `pending` | Agent 选择无副作用等待；保留因果 identity，等待 Runtime wake/recovery；不启动同一 Agent 的下一逻辑 turn。 |
| `Completed` | `committed` | 只在收到匹配的 Runtime receipt 后结束，并按 policy 提交 memory/continuation。 |
| `Stale` | `rejected_stale` / `recovery_pending` | head、lease、precondition 或 digest 失效；丢弃 candidate，清除不兼容 continuation。 |
| `Failed` | `failed_*` | 终止当前逻辑 request；保留 bounded failure trace，禁止 heuristic fallback。 |
| `Rejected` | `rejected_*` | Runtime 拒绝或 Harness 规范化拒绝；释放 single-flight、清除不可复用 continuation；迟到 response 只能进入 `stale/expired` diagnostic。 |
| `Cancelled` | `cancelled -> rejected` with `reject_reason=cancelled` | 终止 turn、释放 single-flight、清除 continuation；迟到 response 按下方 `late_response_after_cancel` 规则进入 `stale/expired` diagnostic。 |
| `Pending` | `scheduler_backpressure` / `recovery_pending` | 保留同一 identity 等待 capacity/recovery；不调用 provider、不开新逻辑 turn。 |

`retry_seq` 是 semantic retry 序号：新的逻辑 turn/request 必须递增并建立因果引用；同一逻辑
request 的 transport retry 只递增 `transport_attempt`，不改 `request_digest` 或
`provider_invocation_key`。Harness 独占分配 session/turn/request cognition IDs；Runtime 验证、持久化
它们并分配 journal/action/envelope/receipt/wake IDs。

Runtime lease expiry 必须把 active turn 变为 `pending` 或 `failed_*`，关闭 Harness 的 in-flight
占用并清除不可复用 continuation；过期后到达的响应只能成为 `stale/expired` diagnostic 或匹配
既有 receipt，不能重新打开 turn。崩溃恢复在 journal 证明同一逻辑 request 时保留 session/turn/request
identity；无法消解的冲突进入 `recovery_pending`。Session 只允许 `active -> draining -> closed`，
`closed` 后不接受 cognition 或 feedback。

Runtime 的 `scheduler_backpressure` 必须释放本地 attempt lease、保留 bounded pending record，
并在 capacity/recovery wake 后只恢复原 identity；`cancelled`、rejected 或 failed 均清除不可复用
continuation。Runtime 的 canonical feedback status 仍只有 `committed/rejected/failed/pending`，
因此 `cancelled` 必须以 rejected reason 映射给 Harness，而不是泄漏为第五个 status。

状态 precedence 固定为：`Wait` 只表示 Agent 的无副作用决定；Runtime
`scheduler_backpressure` 或 `recovery_pending` 一旦产生，Harness 进入 `Pending` 并覆盖本地
Wait 标签。`Pending` 不是第二种 feedback status，也不得与 `Wait` 合并为重复的 state row。

取消后的响应映射固定为四层：Runtime 内部 disposition/code 为
`late_response_after_cancel`；若 Runtime 生成 FeedbackEnvelope，字段为
`status=rejected`、`reject_reason=late_response_after_cancel`，且无 receipt/effect；Harness
将其映射为已关闭 turn 的 `stale`（若 lease/retention 已过期则为 `expired`）bounded
diagnostic，保留原 `feedback_id/feedback_seq` 和 reason。该诊断不是新的 AgentTurn status，
不得把 `cancelled` turn 重新打开，也不得进入 prompt、memory 或 continuation。

## 6. 统一 adapter lifecycle

所有 provider adapter 实现同一抽象生命周期：

```text
open_session(runtime binding)
  -> prepare_turn(observation, memory, goal, continuation)
  -> issue(request)              // one active attempt
  -> normalize(response | error)
  -> handoff(candidate)          // Runtime authority
  -> accept_feedback(feedback)   // correlated only
  -> close_turn / retry / stale / cancel
```

Builtin 的 Responses/tool-only prompt 和 ProviderBacked 的 external session/tool protocol 只能改变 `issue/normalize` 的内部实现。两者不可改变：

- request identity、digest 和 timeout/retry semantics；
- candidate 必须回到 Runtime validation；
- memory intent 的 policy、continuation 的 bounds 和反馈 correlation；
- error code、safe Wait/Rejected 和 trace completeness fields。

Adapter 不得自己选择 subject/grant/audience/nonce/cost quote，也不得把 provider tool call 当成已提交 action。旧的显式 `with_capability_context` 可作为 fixture seam；只有 runtime-owned per-turn injection 才能进入 production target。

Provider trace bounds are fixed for the target contract: at most 64 transcript entries and 64
tool-trace entries per turn, each entry at most 2 KiB and 1 KiB respectively, input/output
summaries at most 1 KiB each, upstream trace at most 4 KiB, and total canonical trace payload at
most 32 KiB. Non-authority trace overflow produces `trace_payload_too_large`, drops only the
diagnostic artifact, and never changes a valid candidate's Runtime disposition; authority-bearing
overflow is a protocol violation. Redaction uses the fixed `<redacted>` marker for credential,
token, authorization, private-key and path fields; unrecognized sensitive values fail closed.
There is no silent truncation. Required fixtures cover each overflow and redaction case.

## 7. Feedback routing 与污染防线

Harness 为每个 session 建立 bounded feedback partition：

```text
feedback_partition[(agent_subject, agent_session_id)]
  -> ordered { feedback_id, feedback_seq, agent_turn_id, decision_request_id,
               candidate_action_id, runtime_receipt_id }
```

路由前按下列顺序检查：

1. session/subject match；
2. turn/request match 当前 active 或 recently completed turn；
3. action/receipt lineage match；
4. sequence 不重复且未越过 retention boundary；
5. summary size、redaction 和 sensitivity policy 通过。

`feedback_seq` 必须按 Runtime session sequence 单调消费：gap 只进入 bounded hold buffer，
不进入 prompt；缺口超过 retention boundary 返回 `feedback_sequence_gap`，关闭当前 feedback
projection 但不按到达时间重排。相同 sequence+digest 是幂等 duplicate；相同 sequence+不同
digest 是 `feedback_identity_collision`，fail closed。

<a id="harness-feedback-watermark-design"></a>
### 7.1 摘要容量与 session 序列生命周期

承接 [PRD 的有效 session 水位合同](continuous-agent-harness.prd.md#harness-feedback-session-watermark)。以 `(agent_subject, agent_session_id)` 为相同隔离键，区分三类保留责任：

| 状态 | 淘汰或关闭边界 |
| --- | --- |
| bounded `recent`、transcript 与已完成 turn 摘要 | 可依既有容量策略淘汰；不回退连续消费水位、不改变 held，也不授权重放旧反馈 |
| 仍有效 session 的连续消费水位、必要去重/tombstone、projection 状态 | 跨 turn 保留；不能随 recent partition 或 accepted-request cache 一起清零 |
| 已关闭 session | 拒绝新 turn，并保留足以识别迟到/重试的关闭事实；释放内容遵守已声明 retention。只有 host 颁发的新 session 才能开始新序列 |

新 turn 的准入、请求反馈关联和 projection 可用性必须在同一个生命周期判断下成立。容量不足以保存新 session 时，保留原有效 session 并拒绝/背压新的准入，或按受支持流程显式关闭选定 projection；不能先成功接纳 turn，再因被淘汰的水位将正常下一 seq 永久 held。关闭结果必须让 host 可区分并停止旧 session 的新工作；重建用新 session 身份，不从“这个 key 当前不存在”推断旧 session 已获重新开始许可。

真正的 seq gap 仍进入 bounded held，补齐缺失 seq 后按 Runtime 顺序连续释放；相同 id/seq/digest 的重复不能二次投影，held duplicate 的成功 ACK 只表示已保留该消息，不能伪造已经进入 recent。缺口超 retention 或无法证明原水位时显式关闭 projection，不能接受任意更大的 seq 跳过事实。Runtime 已 ACK 的旧消息可能不再存在于 outbox，容量策略不得依赖它们自动重送。

持久化状态应包含上述连续水位/关闭事实、held 与仍需关联的请求；更新失败不得对外承诺已完成投影。accepted-request cache 可独立有界淘汰，但不能删除仍需反馈的关联或使有效 session 水位归零；无法保留的请求按显式关闭/拒绝处理，不把新请求的正常反馈归为永久未知。重启加载沿现有 schema 兼容边界校验，不能把缺失水位当作 0。

行为回归必须覆盖 A seq1 已 ACK，其他合法 subject/session 累计引发容量压力，再以 A 原 session 新 turn 产生 seq2 并重试。无须假设大量并发 Agent；其他 Agent 依次关闭/开启 session 即可形成该压力。

预期只能是 seq2 正确连续投影，或 A 新 turn 在调用 provider 前收到明确关闭/重建结果；同时覆盖重启、真正乱序补洞、duplicate、旧 session 关闭、新 session/跨 Agent 隔离，以及 accepted cache 与摘要独立淘汰。只检查 65 个 partition 的数量或加载成功不能证明该合同。

检查失败的 feedback 只生成 bounded diagnostic (`late_feedback`、`unknown_feedback`、`cross_agent_feedback` 或 `duplicate_feedback`)。它不会进入 prompt、memory、continuation，也不会被全局 recent buffer 复用。多 Agent 交错 fixture 必须验证在相同到达顺序和不同到达顺序下，双方 context 仍只包含自己的反馈。

未匹配的 feedback 本身不会结束活动 turn；活动 turn 只能由匹配的 Runtime `pending`/`failed_*`
disposition 或 lease expiry 终结。终结后迟到响应走 `stale/expired` 或既有 receipt 路径，并保持
bounded diagnostic，不得重新注入已关闭 turn。

旧 `FeedbackEnvelope` v1 的 field-level mapping 固定为：`action_id -> candidate_action_id`；
`success=true` 只有在 Runtime 提供 committed receipt 时才映射为 `status=committed`；
`success=false` 必须携带 Runtime disposition 才能映射为 `rejected`/`failed`/`pending`，
否则 fail closed 为 `legacy_feedback_ambiguous`；`reject_reason` 原样作为稳定 reason，
`emitted_events/world_delta_summary` 只进入 bounded、redacted projection。旧 DTO 没有 session/
turn/request/digest/receipt correlation 时只能进入 `legacy_v1` compatibility lane，不能关闭
target turn 或进入 target memory/continuation。

Feedback projection 的上限固定为：canonical envelope 总计最多 16 KiB、`emitted_events`
最多 32 项、`committed_event_summary` 最多 512 UTF-8 bytes、`world_delta_summary`
最多 1024 UTF-8 bytes。Runtime 必须在发送前满足上限；Harness 收到超限 projection 时
记录 `feedback_projection_too_large`，只保留已验证的 identity/status/reason，不把超限摘要
注入 context/memory，并仍按 authoritative terminal disposition 关闭 turn。redaction 固定
移除 credential/token/authorization/private-key/path 字段并替换为 `<redacted>`；无法安全
识别的敏感 payload 直接拒绝 projection，不做静默截断。超限、敏感字段和 gap/乱序都必须
有负例 fixture。

## 8. Memory policy 与 provenance

### 8.1 Retrieval

Harness 在打开 turn 时冻结 `MemoryContextSnapshot`：本地权威 memory 的 selected entries、revision、scope 和 digest。provider 读取的是这个不可变 projection；它不能在 request 内替换 memory backend 或隐式读其他 Agent。

无 memory entry 时使用 schema-defined empty snapshot（`revision=0`、空 entries、显式 scope
和其 canonical digest）；不得用缺字段、JSON `null` 或空字符串替代。空 snapshot 不阻止
decision，但任何 write intent 仍须通过本节 policy 和 committed outcome gate。

### 8.2 Write intent

处理顺序为：

```text
provider intent
  -> source/session/turn check
  -> scope/size/tag/content policy
  -> intent digest + dedupe
  -> wait for authoritative Runtime outcome
  -> local memory policy commit or reject
```

P0 只允许 Agent-private、bounded、可脱敏的 intent policy；共享世界/玩家可见/长期事实语义需要另行授权。写入必须记录 source turn、request digest、Runtime receipt lineage 和 provenance。没有 committed receipt 的 intent（包括 Wait、timeout、malformed、stale、deny）不得进入 authoritative memory；可保留 diagnostic discard reason。

`intent_digest` 相同的 retry 只允许一次 commit；scope、size、敏感度或 provenance 不符合 policy 的 intent 被拒绝且不影响其他 intent。Runtime/journal owner 负责持久化 exactly-once 和 replay proof，本设计不伪造该证明。

### 8.3 MemoryWriteIntentPolicyV1

target memory candidate 的 wire schema 固定为：

```text
MemoryWriteIntentV1 {
  schema_version: 1,
  scope: turn_private | session_private | agent_private_long_term,
  summary?,                         // optional; explicit presence encoding
  tags: string[],                   // omitted tags encode as explicit empty list
}
```

`summary?` 的存在性和值由 Harness 规范化；`intent_digest` 是 Harness 根据下方 policy
计算的 derived artifact，不是 provider 可自报的 authority。现有 inner
`MemoryWriteIntent { scope: String, summary: String, tags: Vec<String> }` 保持不变，仅能经
显式选定的 legacy route/adapter 映射；target lane 不按旧 DTO shape 猜测版本。当前 HTTP wire
不定义 `compatibility_lane` 字段。

P0 的 memory write gate 是确定性的 `MemoryWriteIntentPolicyV1`。scope registry 保留
`agent_private_long_term` 作为版本化保留值，但默认 P0 allowlist 只有
`turn_private | session_private`；long-term scope 默认返回 `memory_scope_denied`，并延后到
P2/独立 memory authority，不得作为 P0 proven 行为：

- `scope` wire enum 为 `turn_private | session_private | agent_private_long_term`；P0 默认只允许 `turn_private | session_private`，共享世界、玩家可见和跨 Agent scope 均拒绝。
- 每 turn 最多 8 个 intent；每个 `summary` 最多 512 UTF-8 bytes；每 intent 最多 8 个 tags；每个 tag 最多 64 bytes；全部 intents 的 canonical bytes 总计最多 4096 bytes。
- `summary?` 是可选字段：省略时以显式 `present=false` 编码；若出现则先做 Unicode NFC 和 trim，且必须非空、无 NUL/control 字符。`tags` 可以是显式空列表或按 schema 省略（省略编码固定等同于 canonical empty list）；每个 tag 先 NFC/trim 后必须非空、无 NUL/control 字符。tags 以 canonical UTF-8 bytes 排序并去重；其他 list 不得套用该规则。
- `intent_digest = H_v1("oasis7.cognition.memory-write-intent.v1", canonical(payload))`；payload 固定为 policy version、agent/session/turn identity、`request_digest`、scope、summary presence/value、canonical tags 与 host-assigned provenance，并排除 digest 自身、transport attempt、raw transcript 和日志顺序。adapter 不得用空 summary、JSON `null` 或缺失字段替代显式的 summary presence encoding。
- 兼容 lane 可把旧 `scope=short_term` 显式映射为 `turn_private` 并记录 `memory_scope_alias_used`；旧 DTO 的 summary 必须为非空字符串。target lane 不接受该 scope alias，必须返回 `memory_scope_denied`；空 tags 仍允许，但空 tag element 必须返回 `memory_tag_invalid`。`agent_private_long_term` 只有显式 future authority/feature gate 才可开启，仍不属于 P0 target。
- 稳定拒绝码为：`memory_source_mismatch`、`memory_scope_denied`、`memory_intent_count_exceeded`、`memory_summary_too_large`、`memory_tag_invalid`、`memory_tag_count_exceeded`、`memory_payload_too_large`、`memory_sensitive_content`、`memory_no_committed_outcome`、`memory_intent_duplicate`、`memory_digest_mismatch`。
- provider 不得自称 `runtime_fact` 或授予 provenance；只有匹配的 Runtime `committed` receipt 才可授予该 intent 的 authoritative provenance。`rejected`（包括 `reject_reason=no_effect`）、`failed`、`pending`、stale 或 lease expiry 均不得写入 authoritative memory。

## 9. GoalSnapshot 与 bounded continuation

### 9.1 P0 GoalSnapshot

```text
GoalSnapshot {
  revision,
  short_term_summary,
  long_term_summary,
  blocked_reason?,
  provenance: harness_projection,
  digest,
}
```

它是兼容性 projection，不是完整 GoalGraph。projection precedence 固定为 trusted host
Agent config/runtime goal projection > legacy provider `goal_summary/blocked_reason`；provider
字段不能覆盖已存在的 host 值。各摘要先 NFC+trim，`short_term_summary`、`long_term_summary`
各最多 512 UTF-8 bytes，`blocked_reason` 最多 256 bytes；超限返回
`goal_snapshot_too_large`，不得静默截断。无值使用 `revision=0`、空摘要、无
`blocked_reason` 的显式 empty snapshot。provider 可以返回 `goal_update_intent`，但 world goal
completion、权威 priority、玩家承诺和跨 Agent goal 关系需要产品/runtime authority。

当前 native runtime-live 的 `trusted_provider_goal_snapshot` 从 host-owned
`AgentPromptProfile` 投影目标：revision 为 `max(profile.version, 1)`（无 profile 时为 1），
短期目标使用 host override 或 phase-1 host goal，长期目标使用 host override 或空字符串。
这是一份有 host 目标的快照，不是通用 empty snapshot；provider 输出不作为 goal source。
快照经过 NFC/trim、大小和 digest 校验后同时进入 turn context 与 outer request digest。
规范化后的目标文本或 snapshot revision 改变必须改变 goal digest；这不证明旧待决请求的清理、
远端重绑定、恢复或完整 GoalGraph 已完成。

### 9.2 Continuation

```text
ContinuationState {
  origin_turn_id,
  origin_request_digest,
  action_or_plan_kind,
  remaining_budget: { unit: steps | ticks, value: u64 },
  baseline_observation_digest,
  goal_digest,
  policy_digest,
  precondition_summary,
  valid_until_tick?,
}
```

Harness 提交 `ContinuationProposalV1`，Runtime 将其验证并投影为 `AgentContinuation`：

```text
ContinuationProposalV1 { // Harness-owned policy proposal
  schema_version,
  continuation_proposal_id, // Harness allocated cognition ID
  world_id,
  agent_id, // Harness subject; Runtime validates current binding
  agent_session_id,
  agent_turn_id,
  decision_request_id,
  origin_turn_id,
  origin_request_digest,
  action_or_plan_kind,
  action_or_envelope_digest?,
  remaining_budget: { unit: steps | ticks, value: u64 },
  baseline_observation_digest,
  goal_digest,
  policy_digest,
  policy_revision,
  precondition_summary,
  precondition_digest,
  wake_conditions: WakeConditionV1[],
  valid_until_tick?,
  source,
  proposal_digest,
}

AgentContinuation { // Runtime-owned durable projection
  continuation_id, // Runtime allocated schedule ID
  wake_id, // Runtime allocated schedule ID
  world_id,
  branch_id,
  finality_epoch,
  finality_block_hash?,
  finality_status,
  reorg_epoch,
  runtime_manifest_hash,
  agent_id,
  agent_session_id,
  agent_turn_id,
  decision_request_id,
  origin_turn_id,
  origin_request_digest,
  continuation_proposal_id,
  proposal_digest,
  action_or_envelope_digest?,
  wake_conditions: WakeConditionV1[],
  next_wake_tick?,
  remaining_budget: { unit: steps | ticks, value: u64 },
  valid_until_tick?,
  precondition_digest,
  wake_seq,
  status,
  terminal_disposition?,
}
```

Harness 独占 proposal policy、bounds 和 `continuation_proposal_id`；Runtime 分配并持久化
`continuation_id`、`wake_id`、`wake_seq`、`next_wake_tick` 等 schedule fields，且依据 paired
runtime disposition-to-feedback mapping 返回终态。以上 `ContinuationProposalV1` 是唯一 wire
schema，不存在 `proposal_id`、`proposer_agent_id`、`request_digest`、`action_digest`、
`remaining_ticks` 或 `expires_at_boundary` 等 wire alias。Runtime 由 wake_conditions 确定
`next_wake_tick`；当 remaining_budget.unit 为 steps 时不得偷换成 tick，Runtime durable record
保留 unit/value 和 proposal digest。origin_request_digest 只作 lineage 校验，不得冒充 action
digest。Runtime ID 不能由 Harness 或 provider 伪造。

`FinalityBindingV1` 是 Runtime 与 Harness 共享的 branch/finality binding 子结构，不向
`ContinuationProposalV1` 添加第二套 wire 字段；proposal 的 `world_id` 由 Runtime 校验，
projection 与 continuation context 携带同一组 Runtime world binding：

```text
FinalityBindingV1 {
  schema_version, branch_id, finality_epoch: u64,
  finality_block_hash?: Digest32, finality_status: pending | verified | reorged | suspended,
  reorg_epoch: u64
}
finality_binding_digest = H_v1(
  "oasis7.runtime.finality-binding.v1", canonical(FinalityBindingV1))
```

`AgentContinuation` 的 `world_id/branch_id/finality_epoch/finality_block_hash/finality_status/
reorg_epoch/runtime_manifest_hash` 必须
来自同一已验证 binding；reorg/finality/world drift 进入 Runtime rejection，而不是由 Harness
或 provider 修补。该形状与 paired Runtime continuation projection 保持字段名和语义一致。

Digest 规则固定为：

```text
proposal_digest = H_v1(
  "oasis7.cognition.continuation-proposal.v1",
  canonical(ContinuationProposalV1 without proposal_digest),
)
continuation_digest = H_v1(
  "oasis7.cognition.continuation-context.v1",
  canonical({ continuation_proposal_id?, proposal_digest?, continuation_id?, wake_id?, wake_seq?,
              world_id?, branch_id?, finality_epoch?, finality_block_hash?, finality_status?,
              reorg_epoch?, runtime_manifest_hash?, remaining_budget?, valid_until_tick?,
              status?: ContinuationStatusV1, continuation_status_digest?, terminal_disposition? }),
)
```

`ContinuationStatusV1`、允许迁移和 `continuation_status_digest` 的字段/domain 完全采用 paired
Runtime contract；Harness 只消费该 Runtime-owned projection，不得生成或改写 status digest。

两者使用共享 BLAKE3-256/RFC 8949 deterministic CBOR registry；digest 字段自身排除在输入
外。没有 continuation 时仍编码上述 context 的全 absent canonical object，作为唯一 empty
sentinel。Runtime 必须验证 proposal_digest，随后把 Runtime-owned schedule correlation 投影为
continuation_digest；adapter 不得用 raw JSON、空字符串或本地 null 替代。

Continuation 的 policy、proposal、bounds、advance/invalidate 语义由 Harness 拥有；它只能作为下一 turn 的 bounded hint。Runtime 拥有 durable schedule、wake、crash recovery 和最终执行时机，并在每次接受 candidate 时检查 live action semantics/preconditions。

Harness 每次继续前检查 observation/goal/policy digest 和 remaining budget。receipt reject、stale、timeout、expiry、reorg/finality invalidation 或 changed digest 清除 continuation；不得自动换成相似 action，不得绕过 provider 或 Runtime。Builtin 与 ProviderBacked 共享该状态机。

GoalGraph、belief memory、长期 preference correction 和 player-facing goal explanation deferred beyond P0；它们不能通过扩展 `GoalSnapshot` 字符串被隐式宣称已交付。

## 10. Failure normalization

Adapter 将 transport/model-specific failures 映射到稳定 Harness errors：

| normalized error | 触发 | 行为 |
| --- | --- | --- |
| `request_identity_collision` | same logical request id, different digest | fail closed；不覆盖 dedupe entry |
| `request_stale` | runtime head/context/policy/capability digest 变化 | 结束旧 turn，`NeedsRediscovery`，无副作用 |
| `provider_unavailable` | timeout/connection/provider unavailable after budget | bounded retry then `Wait` |
| `invalid_decision` | malformed/unknown/disallowed output | `ActionRejected`，不 heuristic fallback |
| `budget_exhausted` | native timeout/token/repair/model/tool call cap reached | `Wait`，保留 bounded trace；不再发起超限 native provider/module call |
| `response_identity_mismatch` | response 未回显同一 identity/digest | `ProtocolViolation`，丢弃 candidate/intents |
| `feedback_not_correlated` | late/unknown/cross-agent/duplicate | diagnostic-only；保持 active state，直到 Runtime pending/failed 或 lease expiry 终结 |
| `late_response_after_cancel` | Runtime 已将 turn 置为 cancelled 后仍收到 response | 映射为 `stale` 或 `expired` diagnostic；不重新打开 turn、不进入 context/memory/continuation |
| `memory_intent_rejected` | scope/size/provenance/dedupe policy failure | 丢弃该 intent，保留 rejection trace |
| `runtime_denied` | Runtime authoritative validation reject | 使用拒绝 receipt，清除不兼容 continuation |

所有失败都必须是 deterministic、可诊断，且无未授权副作用。禁止以“模型大概理解了”作为重试/解析/动作 fallback 的理由。

## 11. Rollout design

### C0 request-bound call budget（native implemented / bounded verification）

C0 是 Harness 的 economy-neutral prerequisite，先于任何 paid Cognition Economy 或 Native
TurnEngine claim，且只适用于 native `LlmAgentBehavior` lane。Agent implementation 必须在
`LlmCompletionClient::complete` 与 `run_prompt_module` 之前分别执行 model/tool admission，使用
`BudgetContractV1.max_model_calls`/`max_tool_calls`，并为每次实际 invocation 先建立单调
reservation。zero-is-deny；C0 不把一次 `decide` 推断为一次 model call。repair、transport retry
和同一 logical request 的 bounded retry
共享 identity 与剩余预算，不能 reset/replenish。exhaustion 必须稳定映射为
`budget_exhausted -> Wait`，并证明没有超限 native provider/module call、新 candidate/action、
projected/durable `MemoryWriteIntent`、world 或资源余额副作用；admission 前已有的 local
private `AgentMemory` observation/diagnostic bookkeeping 可保留，但不构成 authoritative memory
commit。

旧 outer context/legacy fixture 缺少新字段时必须显式选择 additive compatibility lane，不得
静默填充或标记为 C0 target/proven。C0 fixture 只验证 identity binding、调用计数、zero/positive
边界、retry/repair 和 no-side-effect；不定义 token/latency/cost rate，也不定义 Electricity/Data
debit、lease、reserve、settle、release、refund 或 receipt。验收顺序固定为 product
C0 contract -> Agent implementation -> QA focused fixtures。C0 通过后，Native TurnEngine、
dynamic ToolRegistry 和 continuation convergence 可在对应 authority seam 上继续，但不能从
本阶段外推完整 Cognition Economy。opaque ProviderBacked 的内部 model/tool calls、provider-side
retry 与 provider 自己的 tool loop 由后续 adapter self-metering/usage contract 负责，不在 C0
证明范围内。无调用的 `active_execute_until` continuation 可继续且不消耗 call budget；仅在其
后续实际进入 native executor 时执行 admission。

### P0.1 identity/correlation proof

P0.1 冻结 outer context、shared `request_digest`、`provider_invocation_key`、per-agent global single-flight、feedback partition、memory intent gate、GoalSnapshot/continuation projection 和稳定错误码。P0.1 required fixtures：digest golden vectors、same-retry/new-turn/collision、single-flight reentrancy across sessions、A/B feedback interleave、memory intent rejection/commit gate、legacy module/feedback mapping、bounds/redaction/overflow/cancelled/backpressure、Builtin/ProviderBacked MockProvider contract parity。

### P0.2 async actor + action MVCC coupled proof

P0.2 要求 Harness async AgentActor/single-flight 与 Runtime action MVCC/stale/precondition disposition 共同形成可验证 seam；scheduler/MVCC 具体实现仍由 paired runtime docs 冻结。本阶段未通过前不得声称 production async cognition。

### P0.3 minimum journal/recovery proof

P0.3 要求 active turn/request、provider invocation result、Runtime disposition 和 memory intent disposition 在 crash/retry/reconnect 后可恢复且不重复副作用；journal/recovery schema、存储和 replay 实现仍由 paired runtime docs 冻结。P0.1、P0.2、P0.3 全部 required 通过后，才允许 production async claim。

### P1 provider convergence / durable continuation

P1 将两个 provider runner 接入同一 Harness lifecycle，并把 continuation policy/proposal 与 Runtime durable schedule/wake/recovery seam 接通。P1 full fixtures：provider disconnect/retry、Runtime deny/stale、receipt correlation、restore without double intent、continuation invalidation 和 bounded trace/cost diagnostics。

### P2 advanced goals/memory/economy / pilot

P2 在 P0/P1 proof 后执行一个低频、低破坏性 NPC/未知 governed command fixture，再进行固定 epoch 的多样本评测。Cognition economy、GoalGraph、belief memory 和共享/玩家可见 memory 均需额外 authority 与 gates；未通过单场景负例或成本/稳定性门槛时保持 experimental。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405; [professional_acceptance: Finality/pending consumer](continuous-agent-harness.prd.md#harness-infrastructure-pending-receiver) | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Correlate the same candidate/world/request and authoritative receipt; distinguish pending, rejected, stale, expired and effective results. A provider reply, queued status or transport ACK cannot establish completion, authoritative memory or world effect; recovery rejudges under current authority. | Planned verification, not executed: V-EXECUTION/V-CONSUMER: compare before/outage/fresh/recovery observations and receipt-gated context, memory disposition, goal feedback and next action. Pending/rejected inputs have no new effect; private observation/diagnostic bookkeeping stays separate. Methods: [S9A GWSC QA receiver](../../../testing-manual.md#s9a-gwsc-qa-receiver). Freeze source OID/clause, candidate/tree/world/window and professional oracle; keep existing 17 QA negatives/10 runtime groups/8 SC9 cells independently. | Future same-candidate correlated raw artifacts, exact bytes/digest/length/locator, phase comparison and terminal disposition; missing mandatory proof/environment is blocked/unverified, observed invariant violation failed. | No finality, scheduler, journal or complete recovery implementation claim. Source/method existence is not passing behavior evidence. |
| Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6; [professional_acceptance: Exclusive lineage consumer](continuous-agent-harness.prd.md#harness-infrastructure-lineage-receiver) | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Retain ordered intent and original/withdraw/replace association, root/revision/child/direct-parent receipt. For professionally marked exclusive members, consume the first effective winner and the others' zero-effect terminal results; rejected/expired members do not cancel independent intents. | Planned verification, not executed: V-EXECUTION/V-CONSUMER: join both linked request orders, retry/replay and exact receipt identity; prove count at most one and preserve unsupported-operation truth. Runtime owns the exclusive-lineage oracle. Methods: [S9A GWSC QA receiver](../../../testing-manual.md#s9a-gwsc-qa-receiver). Freeze source OID/clause, candidate/tree/world/window and professional oracle; keep existing 17 QA negatives/10 runtime groups/8 SC9 cells independently. | Future same-candidate correlated raw artifacts, exact bytes/digest/length/locator, phase comparison and terminal disposition; missing mandatory proof/environment is blocked/unverified, observed invariant violation failed. | No new lineage marking, canonical ordering, atomic disposition or implicit withdrawal/replacement support. Source/method existence is not passing behavior evidence. |
| Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4; [professional_acceptance: Same-world recovery consumer](continuous-agent-harness.prd.md#harness-infrastructure-recovery-receiver) | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Consume only matching world/manifest/snapshot/journal/receipt recovery material; preserve confirmed history and agent/session/turn/request/digest feedback correlation. Missing/conflicting recovery proof cannot become a recovered-world, goal-completion or authoritative memory claim. | Planned verification, not executed: V-RECOVERY/V-CONSUMER: compare original history with fresh recovery/retry/replay context and receipt-gated memory/goal projection; effect count stays at most one. QA and Runtime retain their independent positive recovery oracles. Methods: [S9A GWSC QA receiver](../../../testing-manual.md#s9a-gwsc-qa-receiver). Freeze source OID/clause, candidate/tree/world/window and professional oracle; keep existing 17 QA negatives/10 runtime groups/8 SC9 cells independently. | Future same-candidate correlated raw artifacts, exact bytes/digest/length/locator, phase comparison and terminal disposition; missing mandatory proof/environment is blocked/unverified, observed invariant violation failed. | Consumer join only: not complete trust-chain validation, nonempty journal-suffix replay, new-request/module/action positive continuation, durable exactly-once or GoalGraph proof. Source/method existence is not passing behavior evidence. |
| Trace refs: O7-REQ-05c84021-ee48-41ef-b94a-8aa9ceb8048c; [professional_acceptance: Industrial result consumer](continuous-agent-harness.prd.md#harness-infrastructure-industrial-receiver) | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Join immutable root, revision/child/direct parent, authoritative receipt and Agent readback of held/consumed/unmet/residual quantities, W/window, progression, earliest blocker and next action/recheck. Do not confuse accepted with reserved, local completion or transit with production/delivery completion. | Planned verification, not executed: V-INDUSTRIAL/V-CONSUMER: join request/receipt/snapshot/memory-disposition/goal-feedback artifacts with gameplay quantity/W/window and Runtime pre-sink/root oracles; retry/replay cannot duplicate hold release, sink, credit, receipt or reward. Methods: [S9A GWSC QA receiver](../../../testing-manual.md#s9a-gwsc-qa-receiver). Freeze source OID/clause, candidate/tree/world/window and professional oracle; keep existing 17 QA negatives/10 runtime groups/8 SC9 cells independently. | Future same-candidate correlated raw artifacts, exact bytes/digest/length/locator, phase comparison and terminal disposition; missing mandatory proof/environment is blocked/unverified, observed invariant violation failed. | No recipe, capacity, W/window, resource, progression or economic policy change; one local stage does not prove the complete industrial pipeline. Source/method existence is not passing behavior evidence. |
| Trace refs: O7-REQ-4792541c-aaa4-4ac1-9a2d-374df44eba6a; [professional_acceptance: Eight-cell outage and tier consumer](continuous-agent-harness.prd.md#harness-infrastructure-outage-receiver) | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Retain all eight SC9 authority/read × stage_finish/transit/buffer_admission/terminal_settlement cells; each authority cell keeps root-absent pre-finality and root-present post-commit/pre-child specimens. Read outage never mutates world/W/history; only canonical gameplay interruption may reset unfinished W once. Preserve quantities/window/receipt/progression/blocker/next_action/next_recheck parity. | Planned verification, not executed: V-OUTAGE/V-CONSUMER: compare before/outage/fresh/recovery/retry/replay for every cell/specimen. Record actual provider four fields, model/profile/adapter/protocol, lane and redacted route/config/window. Required active-provider pure_api and full same-candidate real-provider integration, provider-backed Agent parity and external headed desktop+narrow S6 evidence are independently mandatory. Methods: [S9A GWSC QA receiver](../../../testing-manual.md#s9a-gwsc-qa-receiver); the same root manual S6 receiver supplies the separately required headed capture method, with the QA tier boundary preserved in §12. Freeze source OID/clause, candidate/tree/world/window and professional oracle; keep existing 17 QA negatives/10 runtime groups/8 SC9 cells independently. | Future same-candidate correlated raw artifacts, exact bytes/digest/length/locator, phase comparison and terminal disposition; missing mandatory proof/environment is blocked/unverified, observed invariant violation failed. | No missing-cell N/A, mock/history substitution, actual provider/browser execution, full/parity/readiness or release claim; Ops classifies the dependency and gameplay owns W/window. Source/method existence is not passing behavior evidence. |
| Trace refs: O7-REQ-2b7cfef9-d917-46ea-a3a9-e3a4fb96d730; [professional_acceptance: Explicit world-scope consumer](continuous-agent-harness.prd.md#harness-infrastructure-world-scope-receiver) | [Infrastructure evidence join](continuous-agent-harness.design.md#harness-infrastructure-evidence-join) | Before submit distinguish actual global-authoritative versus isolated local/development world and match world/manifest across request, root, receipt and result. Missing/mismatched/reused receipt identity fails closed with zero global effect. | Planned verification, not executed: V-WORLD-SCOPE/V-CONSUMER: compare distinct global and local/development worlds through pre-submit target, intent, receipt and result, including missing/mismatched/reused/replayed identity. Retain exact bytes/digest/locator and the actual Agent projection. Methods: [S9A GWSC QA receiver](../../../testing-manual.md#s9a-gwsc-qa-receiver). Freeze source OID/clause, candidate/tree/world/window and professional oracle; keep existing 17 QA negatives/10 runtime groups/8 SC9 cells independently. | Future same-candidate correlated raw artifacts, exact bytes/digest/length/locator, phase comparison and terminal disposition; missing mandatory proof/environment is blocked/unverified, observed invariant violation failed. | Environment labels/local receipts do not prove global finality; no world-identity, receipt schema or authority expansion. Source/method existence is not passing behavior evidence. |

## 12. Verification matrix 与 observability

<a id="harness-infrastructure-evidence-join"></a>
### Infrastructure consumer evidence join

Harness consumes the [PRD consumer matrix](continuous-agent-harness.prd.md#harness-infrastructure-consumer-matrix) through the frozen [QA evidence method](../../testing/design.md#qa-infrastructure-evidence-method). Freeze source OID/clause, tested candidate/tree, world/manifest, provider/lane and capture window; select the [QA scene family](../../testing/prd.md#qa-infrastructure-scene-families) and its domain-owned oracle before comparing any consumer output.

| Join step | Harness evidence and decision |
| --- | --- |
| Freeze and correlate | Bind session/turn/request/response, ordered intent, root/revision/child/direct-parent receipt and world identity to the same candidate. Keep original/withdraw/replace lineage; provider reply, queued status and transport ACK are observations, not authoritative effects. |
| Compare dispositions | Join before/outage/fresh/recovery/retry/replay Runtime snapshot/journal/receipt to Agent context, memory-intent disposition, goal feedback and next action/recheck. Pending/rejected/stale/expired input cannot produce completion or authoritative memory/world effect; retain private observation/diagnostic bookkeeping separately. Runtime owns publication, gameplay owns quantities/W/window, and Harness consumes their correlated result. |
| Compare outage cells | Consume all [eight SC9 cells](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-sc9-eight-cells), retaining both root-absent pre-finality and root-present post-commit/pre-child specimens within each authority cell. Read-outage recovery reconciles actual truth without world/W/history mutation; retry/replay cannot duplicate root, hold release, sink, credit, receipt or reward. |
| Check provenance and environment | Record `agent_decision_source`, `agent_provider_backend`, `agent_provider_contract`, `agent_provider_transport`, actual model/profile/adapter/protocol and safe route/config identity. Follow [required/full boundaries](../../testing/prd.md#qa-infrastructure-tier-boundaries) and [external headed capture](../../testing/manual/web-ui-playwright-closure-manual.design.md#playwright-source-capture-method); mock or legacy fixtures cannot replace active-provider API or full real-provider Agent evidence. |
| Classify and archive | Missing mandatory identity/phase/cell/proof/environment/consumer/archive material is blocked/unverified; an observed invariant violation is failed. Archive exact receipt/proof bytes, digest/length/locator, comparison and terminal disposition under the [QA evaluation method](../../testing/prd.md#qa-infrastructure-source-scene-method). Shape-only pass is not semantic/full/proven proof. |

The merged GWSC semantic validator already requires unequal migration versions; the [planned equal-version negative receiver](../../testing/prd.md#qa-infrastructure-equal-version-negative) supplies shape-valid `1→1` rejection/no-effect/state-preservation acceptance, not an executed result. This join does not change digest/schema algorithms, MVCC, journal, scheduling, GoalGraph, durable memory, economy or real-model quality gates.

| 证据层 | 主要断言 | 关键 artifact | 禁止外推 |
| --- | --- | --- | --- |
| required | schema/identity/错误/隔离契约 | canonical bytes、digest、request/response/feedback fixtures | 不能证明真实 provider 或 world commit |
| required | C0 native request-budget zero/positive、pre-`complete`/pre-`run_prompt_module`、repair/retry、exhaustion、no-call `active_execute_until` | native policy identity binding、bounded model/tool invocation count、稳定 `budget_exhausted -> Wait`、无新 action/projected-durable memory/world/resource side effect | opaque ProviderBacked internal calls、paid cognition、token/latency/cost rate、lease/reserve/settle/refund/receipt |
| required | legacy lane、trace/feedback bounds、redaction、cancelled/backpressure cleanup | explicit lane markers、bounded projections、terminal disposition、late-response fixtures | 不能证明真实 provider 或 target parity |
| full | Runtime causal loop/replay/no duplicate | head-bound receipt、stale/deny/retry/restore logs、journal proof | 不能证明产品 parity 或 economy |
| rollout | 行为/延迟/成本/稳定性 | fixed scenario/profile/provider/adapter/protocol epoch，per-scenario artifacts 和 repeated samples | 均值不能掩盖单场景 failure |
| proven | 同一 epoch 的成功+负例闭环 | artifact digest、identity binding、副作用检查、reviewed fixture manifest | 未覆盖 provider/module/goal 不可继承 proven |

最低诊断字段：session/turn/request ids、request/response digest、provider/adapter/protocol version、base head、outcome/error code、attempt count、native model/tool budget limits and consumed counts、latency/token/cost diagnostics、feedback lineage、memory intent disposition、continuation disposition。

C0 counters 只表示 native Harness protocol admission；opaque ProviderBacked usage 必须由其 adapter 自行提供，不能从该字段推断。token/latency/cost 在 P0 只是 diagnostics/evaluation inputs；不代表资源扣账或发布证据。

可执行入口固定为：文档阶段运行 `./scripts/doc-governance-check.sh`；P0.1 实现阶段运行
`env -u RUSTC_WRAPPER cargo test -p oasis7_provider_local_bridge --bin oasis7_provider_local_bridge -- decision_identity`
与 `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib provider_loopback_adapter`；集成阶段运行
`env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_mvcc`（P0.2 paired runtime）与
`env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_recovery`（P0.3 paired runtime）；
`./scripts/ci-tests.sh required`，完整多 Agent/retry/restore/parity 证明运行
`./scripts/ci-tests.sh full`。focused filter 必须由实现切片建立并输出 immutable golden
fixture、canonical bytes/digest/key、provider invocation count 与 feedback partition；命令
不存在或未执行时仍是 target，不是通过。

### 12.1 C0 与 host goal 的局部验证

- `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib cognition_budget`：wire 必填/未知字段、
  model/tool zero、repair/transport retry、计数与 exhaustion 无新效果；同时检查预算任一上限
  改变会改变 request digest，原 request key 重绑预算产生 collision。
- `env -u RUSTC_WRAPPER cargo test -p oasis7_viewer_runtime --lib runtime_live_host_goal`：host 默认目标、
  profile override、revision/text digest 变化、NFC/trim 与超限拒绝。
- `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_live_harness`：共同 actor
  消费 host goal、receipt-gated memory、失败不冒充成功等待与 continuation 终态隔离。

上述测试只支持局部实现结论；未执行或失败不得算通过。它们不替代 required/full、
真实 provider、远端恢复、玩家表面或发布验收。

## 13. Paired specialist follow-ups

- `runtime_engineer`：scheduler 非阻塞语义、candidate/action MVCC、stale/precondition、receipt/journal/replay 和恢复的权威合同；Harness 只接收其 binding。
- `qa_engineer`：required/full/rollout fixture 的 deterministic assertions、交错并发、cross-talk、stale/replay/no-duplicate side-effect 和 failure matrix。
- cognition economy authority：C0 call admission 是 Harness prerequisite；付费资源/rate、预算
  reserve/settle/release、expiry/refund、计费与 receipt 仍明确晚于 P0，并需 product/gameplay/
  Runtime/QA 的独立 authority chain。
- gameplay/product/Agent authority：GoalGraph、belief memory、偏好、共享/玩家可见 memory 和目标完成语义。
- viewer/runtime-live owners：trace delivery 与玩家可见诊断；不得把 Viewer transcript 当成 cognition journal。

同一目的继续更新原 PR，follow-up 按实际范围处理；本设计不创建第二份 mutable task truth。
