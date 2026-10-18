# Provider 切换窗口中的 Agent 意图连续性系统设计

- 设计 ID：DES-PSW
- 状态：active target contract；不代表实现或 provider readiness
- Owner role：agent_engineer
- 适用范围：world-simulator Continuous Agent Harness、provider adapter 与 provider/lane 选择之间的切换和恢复
- 上游产品要求：[AC-AGENT-PARITY-002](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-002)
- 专业依赖：[Continuous Agent Harness](continuous-agent-harness.design.md)、[Agent cognition lifecycle](../../world-runtime/runtime/agent-cognition-lifecycle.design.md)、[indirect-control runtime authority](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md)
- 审读基线：结合上游文档的现行 target contract 阅读；实现状态、候选版本和任务证据由实际 Task/PR 与验证记录维护；本轮未核清具体实现记录，文档迁移任务见 [#4694](https://github.com/eng-cc/oasis7/issues/4694)
- Last reviewed：2026-09-25

本文只冻结 provider/lane 切换时的 Agent 路由绑定和意图 lineage。它不声明任何 provider 当前受支持、可用、默认或已就绪。

## 1. 问题、目标与非目标

provider 或 execution lane 变更发生在一个 Agent session 或 cognition turn 尚未收口时，若当前路由被覆盖，旧 response 可能被新 adapter 当成新输入处理；若断线后用新 provider 重建请求，则可能重复产生意图，或让玩家误以为原请求已取消、失败或成功。

目标是在 provider/lane 选择变化和进程重连下保持清晰的边界：旧 turn 沿原绑定和身份收口；新绑定只服务新建的语义 turn；Runtime 是已提交意图的唯一权威裁决者。目标是可评测的设计合同，不是当前实现结论。

非目标：重新定义 action schema、provider 支持矩阵、lane 的玩法含义、Runtime intent/receipt schema、撤回或替换协议、Viewer 布局、parity 阈值、默认 provider 或发布准入。

## 2. 上游约束与相关角色

`producer_system_designer` 拥有切换窗口的产品承诺和适用范围；`agent_engineer` 拥有 provider/lane 路由快照、Agent turn 与 semantic replan；`runtime_engineer` 拥有已提交请求的接受/拒绝、幂等、receipt、持久化和恢复；`qa_engineer` 证明切换与恢复组合；`viewer_engineer` 投影候选、待决和终态。

产品 PRD 要求切换只影响后续新意图、候选不得自动交给新 provider 重试、pending 不因切换或重连改变语义，且只有 committed receipt 表达世界后果。意图 lineage、世界效果和 UI 投影仍由各自专业 authority 决定。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5；[AC-AGENT-PARITY-002](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-002) | active session 的 provider/lane 路由快照不可被切换追溯改写；后续新 turn 使用新绑定，且绑定差异参与该 turn 的请求身份。 | [DES-PSW-001](provider-switch-intent-continuity.design.md#des-psw-001) | Agent Harness owner；依赖 [Continuous Agent Harness](continuous-agent-harness.design.md) 的 session、turn 和 digest 合同。 | 不指定 provider 矩阵、支持状态或对外 UI 字段。 |
| Trace refs: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5；[AC-AGENT-PARITY-002](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-002) | pre-switch candidate 保留原 turn 来源，不自动交给新 provider；提交仍须符合原 turn 既有授权和 Runtime 当前校验。 | [DES-PSW-002](provider-switch-intent-continuity.design.md#des-psw-002) | Agent Harness owner；依赖 [DES-WR-IA-001](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-001) 的接受/生效边界。 | 不定义 AgentIntentV2、撤回或 replacement schema。 |
| Trace refs: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5；[AC-AGENT-PARITY-002](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-002) | provider failure、重连或 receipt 丢失时，只能按原绑定和原 identity 恢复；新 provider 不得承接或重发旧请求。 | [DES-PSW-003](provider-switch-intent-continuity.design.md#des-psw-003) | Agent Harness 与 Runtime；依赖 [DES-WR-IA-004](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-004) 和 [DES-WR-IA-006](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-006)。 | 不规定 Runtime journal、receipt schema 或保留期。 |
| Trace refs: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5；[AC-AGENT-PARITY-002](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-002) | required-tier 覆盖身份和状态负例；full-tier 在同一 fixture/profile 上覆盖真实 provider、Runtime receipt/journal 与 restart/reconnect。 | [DES-PSW-004](provider-switch-intent-continuity.design.md#des-psw-004) | QA 与 Agent；full-tier 的 authoritative outcome 需要 Runtime evidence。 | 本设计不报告任何 candidate 的通过、parity 或 release 结论。 |

### 2.2 当前状态与目标差距

| 对象/能力 | 当前状态（基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| Request identity | Continuous Harness target contract 已把 provider config/profile revision、adapter protocol 与认知上下文纳入 request digest；transport retry 与 semantic retry 分开。 | 每个 turn 的完整 provider/lane 绑定都不可变，且可从 request identity 复原。 | 现有材料未明确 lane 是否已进入绑定 digest，也未定义配置切换时的 session drain 语义。 | [Continuous Agent Harness design](continuous-agent-harness.design.md)；实现覆盖须由当前 task evidence 证明。 |
| Runtime request recovery | Runtime target contract 定义 request identity、幂等和 receipt/recovery owner。 | 重连只恢复原 authority request 的 pending/terminal disposition。 | provider/lane cutover 本身不能成为 Runtime 的取消或重放信号。 | [Agent cognition lifecycle design](../../world-runtime/runtime/agent-cognition-lifecycle.design.md)。 |
| Player-facing parity | 产品 AC 定义 candidate、pending、committed 与 rejected 的玩家边界。 | 切换前后身份与实际 Runtime 结果可追溯，且状态不会因换路由被改写。 | Viewer 与真实 provider/restart 样例未由本文证明。 | Product PRD、Viewer owner、QA evidence。 |

文档中 `target` 只描述未来合同。没有固定实现 candidate 和对应验证证据时，不能报告为 `current` 或 `proven`。

## 3. 决策概览

一个活动 AgentSession 绑定一个不可变 provider profile revision 与 execution lane selection。provider/lane 配置改变时，Harness 将该 session 置为 draining；已创建 turn 继续引用旧快照。活动 provider attempt 必须先在原 route 上完成或到达 Agent/provider 既有有界 timeout/recovery boundary。若其 Runtime request 仍为 pending，Harness 可在重新读取到权威 pending disposition 后持久化精确 recovery handoff，再释放 Agent active-turn slot 并开启新 session/turn；旧 Runtime request 继续留在原 authority chain。handoff 无法读取或持久化时，slot 保持占用，直到原请求能被安全恢复。

该 handoff 是 Continuous Agent Harness 单飞行规则的有界例外：每个 Agent 同时仍至多有一个活动 adapter attempt；只有 adapter attempt 已结束、handoff 已 durable commit 后，新的 provider-bound turn 才可启动。handoff 不取消、替换或结算旧 Runtime request，也不重置已消耗的调用预算。

若当前 request context 无法把有效 provider/lane selection 唯一绑定到该 turn 的 canonical identity，则不能声称满足本合同。应先演进 request-context/digest version，再让新 turn 使用该格式；不得在旧 turn 上补写 lane 或 profile 字段。

## 4. 边界与结构

~~~text
player / host route selection
  -> Agent Harness freezes provider profile + execution lane for session/turn
  -> original provider adapter call and response identity check
  -> candidate remains advisory data
  -> Runtime validates any submitted request and owns disposition / receipt / effect
  -> Harness recovers matching feedback before forming a later turn
~~~

- Harness owns the cognition session/turn binding, candidate lineage, adapter route and decision to create a fresh semantic replan.
- Provider adapter owns transport and provider-specific invocation. It cannot change request identity or turn an old candidate into a new request.
- Runtime owns only requests already submitted to world authority. It validates, deduplicates, commits or rejects, and supplies authoritative receipt/recovery facts.
- `player_parity` and `headless_agent` remain execution lanes, not access modes. The lane selection does not alter permissions, action semantics, world authority or product access promises.
- Runtime may correlate a submitted request to its Agent turn, but provider/lane metadata is Agent-side provenance. This design adds no Runtime enum or schema field.

## 5. 关键运行流程

### 5.1 正常切换

1. The host resolves a provider profile and lane before starting a session. The binding is immutable for that session and every turn it issues.
2. On a provider/lane change, the Harness marks the old session draining. It does not edit the old request context, digest, provider invocation key, candidate, Runtime request or receipt reference.
3. An active provider invocation is allowed to finish on the old route or reach its existing bounded timeout. Before old-turn closure, a response is usable only if all old turn identity and digest fields match. After timeout/handoff closure, a late response is diagnostic for the old identity only; it cannot create a candidate or Runtime submission.
4. If the old turn has no unresolved Runtime request, close it under its existing contract and open a new session with the selected profile/lane revision.
5. If the old Runtime request remains pending, follow the durable bounded handoff in §5.4. Only after its recovery correlation is persisted may a new session start. Its first request is a fresh semantic turn with new identity and a new digest.

### 5.2 Candidate at cutover

A candidate created before cutover stays attached to its old turn. The cutover neither sends it to the new provider nor authorizes it for world execution. If the existing Agent contract permits that exact candidate to proceed, it may be submitted once through its original lineage and still must pass current Runtime validation. If a new provider decision is needed, the Agent retires the prior candidate according to its existing contract and creates a fresh semantic request with a causal reference; it must not copy the old candidate payload into the new provider request as a retry.

If the candidate has already been submitted, it is no longer a provider-routing concern. Reconnect and observation use the same Runtime request identity and let the same authority chain return its existing pending or terminal disposition. A new recommendation does not replace, cancel or supersede it without an independently supported replacement path.

### 5.3 Reconnect and lost acknowledgement

- Reconnect without a binding change resumes the same Agent session and request identity. A transport retry may increment `transport_attempt` only under the existing same-key/same-digest adapter contract.
- If provider response delivery was uncertain, retrieve or idempotently retry only through the original provider binding. If that provider cannot prove same-key deduplication or retrieve its response, do not replay through another provider; wait for the old turn's explicit bounded failure/recovery path.
- If Runtime submission may have succeeded but the acknowledgement or receipt was lost, query/retry that exact Runtime request identity and digest. Same identity plus same digest recovers the original disposition; same identity plus a different digest fails closed.
- Runtime `pending` remains pending until Runtime provides its terminal disposition. Provider switch, reconnect, timeout observation or client-side transport cancellation alone cannot make it cancelled, rejected, committed or failed.
- A fresh post-switch semantic replan has a new turn/request identity and causal link to the prior result. It remains a candidate until independently validated by Runtime; Runtime must not infer retry, merge, shared authorization or deduplication from matching action text.

### 5.4 Runtime pending handoff

1. Preserve the per-Agent adapter single-flight. Do not begin handoff while the old provider attempt is active. First receive its response, or reach the existing Agent/provider bounded timeout/recovery boundary. After that boundary, a late response may be retained as bounded diagnostics for the old identity only; it cannot create a candidate, Runtime submission or input to the new turn.
2. Read the old submitted request from its Runtime authority. If it is terminal, reconcile that result normally. If Runtime reports pending, prepare a durable Agent recovery-handoff record containing the exact old agent_session_id, agent_turn_id and decision_request_id, the Runtime request/intent identity, request_digest, the Runtime's existing idempotency key, the last authoritative pending disposition and a recovery-correlation identity.
3. Commit that record before releasing the Agent active-turn slot. The record is recovery bookkeeping only: it does not terminalize the old Runtime request, cancel or replace it, reassign its receipt, release its idempotency key, or infer a world effect. The old request continues on its original Runtime identity and authority chain.
4. After durable commit, open the new provider-bound session with fresh session/turn/request identities and a causal reference to the old request/handoff. Any new candidate uses current observations and must pass ordinary current-head/current-authority Runtime validation. Matching action text does not imply retry, merge or deduplication.
5. A later Runtime response or receipt for the old request reconciles only against the old identities and handoff record. It cannot replace the new turn's candidate or outcome. A late provider response cannot resume the old decision or create a world request after handoff. Recovery rereads authoritative Runtime state because the stored last-pending disposition may since have become terminal.
6. Handoff is allowed only within existing durable recovery and pending-capacity limits. If Runtime state is unavailable, the recovery record cannot be durably committed, or the configured bound is exhausted, keep the active-turn slot held and block new cognition until reconciliation; do not create an in-memory-only handoff.
7. Handoff does not refund or reset model/tool credits. Preserve consumed counts and the remaining allowance under the existing governing budget contract; a new semantic request receives no additional credits merely because the session changed.

## 6. 接口与数据合同

### 6.1 Immutable routing binding

The logical binding consists of the selected provider configuration reference, its immutable profile revision, adapter protocol version, execution-lane selection/revision, and the originating session/turn identity. Credentials, raw endpoint secrets, mutable display names and transport attempt counters are not binding identity.

The binding must be represented by the existing request context fields or an explicitly versioned additive context that participates in canonical request digest and provider invocation key derivation. The design does not require a particular JSON/Rust field layout. If an existing lane selection cannot be recovered from the bound profile or included in that digest, that context version is ineligible for this target behavior; do not silently add an unhashed side channel.

| 接口/条款 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| Session route snapshot | trusted Agent host → Harness/adapter | session id plus selected profile revision and lane revision | frozen before first turn; no in-place mutation | unknown/invalid binding fails closed before provider invocation | legacy context without complete binding remains compatibility-only and cannot claim this contract |
| Provider decision call | Harness → selected adapter | existing session/turn/request ids, request digest, provider invocation key and adapter protocol version | transport retry preserves identity/digest; semantic replan creates new turn/request | response must echo/match the old context; late or mismatched response is rejected for that turn | existing DTO remains inner decision payload; no second action schema |
| Runtime request recovery and Agent handoff | Harness → same Runtime authority; Harness also persists its recovery correlation | Runtime-owned request/intent identity and digest plus exact Agent session/turn correlation | retry/reconcile same identity and digest only; conflicting digest is rejected; persist pending handoff before releasing the active-turn slot | authoritative pending/terminal disposition and receipt determine world result; the handoff record creates no disposition | Runtime schema and status mapping remain owned by Runtime design; handoff is Agent recovery bookkeeping |

No cross-provider fallback is implicit. A fallback after failure is a new semantic turn on the newly selected provider, not transport retry. Any profile or lane change affecting request semantics changes the digest identity and requires a new request.

## 7. 状态、事务与持久化

This design reuses the target Harness session/turn lifecycle and Runtime request/receipt authority. It adds an Agent-owned durable recovery correlation for a bounded pending handoff, but introduces no new Runtime request state or status enum.

| Cutover view | Required meaning | Authority |
| --- | --- | --- |
| Session draining | no later turn is routed through the old session after its current turn closes; old identity remains addressable for recovery | Agent Harness |
| Candidate not submitted | advisory candidate tied to old request; no world effect; neither auto-routed to the new provider nor authorized by provider selection | Agent Harness plus existing authorization contract |
| Provider call in flight | old binding remains selected for that request; late response must match old identity; provider timeout does not prove Runtime cancellation | Agent Harness / adapter |
| Runtime pending before handoff | same submitted request is unresolved; provider attempt must first end; no later turn may start until a durable recovery handoff is committed | Runtime disposition; Agent controls slot |
| Durable recovery handoff | exact old agent_session_id/agent_turn_id/decision_request_id, Runtime request/intent identity, request_digest, existing idempotency key, last authoritative pending disposition and recovery correlation are persisted; old Runtime request remains pending | Agent Harness persists; Runtime remains authority |
| Runtime pending after handoff | old request remains on its original identity; a fresh provider-bound turn may run; later old receipt/disposition reconciles only to old handoff identity | Runtime for disposition; Agent Harness for correlation |
| Runtime terminal | matching authoritative receipt/disposition closes the old request; only committed receipt proves world effect | Runtime |
| Fresh semantic replan | distinct request lineage, new profile/lane snapshot, independent validation | Agent Harness creates; Runtime judges any submitted candidate |

Provider routing/session evidence may be retained as bounded Agent diagnostics under existing privacy and retention policy. It must not replace Runtime journal/receipt data, and raw prompts, credentials and private provider transcripts are not needed to recover request identity.

## 8. 安全与运行约束

- Provider output and cached response are untrusted decision data. Reconnection must verify the exact session, turn, request, digest and protocol identity before accepting it.
- Missing/ambiguous provider or lane binding, unknown version, digest mismatch or an unresolved old response fails closed; no silent default provider, lane coercion or automatic cross-provider replay.
- A transport-level abort is not a semantic cancel. Only a supported Agent or Runtime cancellation path can produce that result, and the relevant authority must confirm it.
- Existing request timeout, model-call budget, provider credentials and privacy bounds continue to apply. Provider switching does not reset or grant additional calls, budget, action authority or Runtime quota.
- The handoff record must be durably committed before releasing single-flight ownership. A persistence error, unreadable Runtime disposition or exhausted recovery capacity keeps the Agent blocked on the old correlation; an in-memory flag is insufficient.
- A stalled or unavailable old provider may delay new cognition until its attempt reaches the existing timeout/recovery boundary and any pending handoff is durably committed. The recovery action must be visible and bounded by existing policy; switching providers alone is not a recovery proof.

## 9. 质量与容量

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| Provider/lane cutover during one provider call | one Agent, one active session, one pending decision, fixed observation and profile revisions | old call retains old identity; no new-provider call for that turn; next session uses new binding | zero cross-route dispatch for old request; identity and digest match; timeout remains within existing profile contract | `DES-PSW-004` scenarios PSW-01 and PSW-02 | target design; implementation unverified |
| Cutover after candidate and before submit | same Agent and Runtime head; candidate not yet admitted by Runtime | no automatic candidate transfer/retry; original authorization path or explicit fresh replan only | zero automatic new-provider requests and zero unauthorized Runtime submission | `DES-PSW-004` scenario PSW-03 | target design; implementation unverified |
| Runtime acceptance with lost acknowledgement | same Runtime branch and request identity; reconnect/restart at each acknowledgement boundary | restore original pending/terminal disposition; a fresh turn may start only after pending handoff is durably committed | exactly one authoritative outcome for the same identity; conflicting digest fails closed; late receipt stays correlated to old request | `DES-PSW-004` scenarios PSW-04 and PSW-05 | paired Runtime proof required |
| Original provider unavailable after switch | bounded Agent/provider timeout; Runtime request may still be pending | after durable pending handoff, a fresh provider-bound turn may start while old request remains pending | no cross-provider replay or inferred terminal state; new turn has fresh identity and causal link; budget is not reset | `DES-PSW-004` scenarios PSW-06 and PSW-09 | target design; provider behavior unverified |
| Handoff persistence failure or capacity exhausted | one Agent with old Runtime request pending; durable store or recovery capacity unavailable | do not release the active-turn slot or invoke the new provider | no in-memory-only handoff; old request remains unchanged until reconciliation | `DES-PSW-004` scenario PSW-07 | target design; implementation unverified |
| Handoff budget continuity | one Agent with nonzero model/tool usage before cutover | retain consumed counts and remaining allowance across the new session | no credits refunded or added by handoff | `DES-PSW-004` scenario PSW-08 | target design; implementation unverified |

This design sets no new latency or cost threshold. Existing parity policy and provider profile own those limits. Local fixtures can prove identity/dispatch logic only; full-tier parity requires the same fixture/profile with real provider evidence and Runtime receipt/journal recovery.

## 10. 兼容、迁移与回滚

Legacy request contexts that cannot prove the complete provider/lane binding may continue only in their explicitly declared compatibility scope. They cannot be reinterpreted as target parity evidence, upgraded in place, or resumed through a different provider. Existing saved sessions retain the binding/context version with which they started.

Enablement requires a compatible implementation that freezes and verifies the binding, preserves old request identity during reconnect, and passes the negative cutover cases below. To revert provider selection, open a fresh session with the prior profile/lane revision after the old provider attempt ends and either the Runtime request becomes terminal or its pending recovery handoff is durably committed. Rollback cannot retract a Runtime-committed effect, rewrite a receipt, or make a pending old request terminal; unresolved old requests follow Runtime recovery or operator intervention under its runbook.

## 11. 验证设计与可追溯性

验证应覆盖 Agent route selection、provider response matching、Runtime submission and recovery, and player-visible state projection as separate proof surfaces. Existing unit fixtures are necessary but cannot prove real provider parity or hosted readiness.

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立义务与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5；[AC-AGENT-PARITY-002](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-002) | [DES-PSW-001](provider-switch-intent-continuity.design.md#des-psw-001) | session route is immutable; changed provider/lane begins a new bound semantic turn, and lane identity cannot be an unhashed side channel. | PSW-01/02 are planned required-tier Agent tests in `crates/oasis7/src/simulator/tests/agent_cognition_provider_switch.rs` (new module, to be registered in `simulator/tests/mod.rs`). Existing [agent cognition identity tests](../../../crates/oasis7/src/simulator/tests/agent_cognition_identity.rs) are adjacent baseline only; they do not currently assert provider/lane cutover. | After implementation, record the exact tested tree, focused command/output, old/new binding and request-digest assertions, and review of digest inputs in actual Task/PR evidence. | The planned fixtures and their implementation do not exist yet; no actual provider support, cost, parity or release conclusion. |
| Trace refs: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5；[AC-AGENT-PARITY-002](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-002) | [DES-PSW-002](provider-switch-intent-continuity.design.md#des-psw-002) | a pre-switch candidate is never passed to the new provider as a retry; any Runtime submission uses the original permitted lineage and current validation. | PSW-03 is a planned required-tier Agent test in the new `agent_cognition_provider_switch.rs` module. Existing [live Harness tests](../../../crates/oasis7/src/simulator/tests/agent_cognition_live_harness.rs) are baseline only and do not currently exercise a provider switch with a candidate awaiting submission. | After implementation, record candidate lineage, per-provider call ledger, Runtime submission/result, exact tested tree and focused command/output. | PSW-03 implementation/test is pending; this does not prove Viewer status readability or a real provider's behavior. |
| Trace refs: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5；[AC-AGENT-PARITY-002](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-002) | [DES-PSW-003](provider-switch-intent-continuity.design.md#des-psw-003) | reconnect preserves the submitted Runtime identity/digest and recovers pending/terminal status without a second effect; pending handoff releases the Agent slot only after durable correlation; conflicting digest fails closed. | PSW-04/05/07/09 Agent-side handoff, recovery and correlation tests are planned in new `agent_cognition_provider_switch.rs`; PSW-04/05/06/09 Runtime authority tests are planned additions to [Runtime cognition recovery tests](../../../crates/oasis7_runtime/src/tests/agent_cognition_recovery.rs). The existing Runtime file covers committed response replay, duplicate committed projection and legacy snapshot compatibility only; it does not cover Agent pending handoff, its persistence/readback failure, budget continuity or old-receipt correlation. | After implementation, record exact old/new Agent IDs, durable handoff write and readback (including failure case), unchanged old Runtime identity/digest/key and authoritative disposition, plus Runtime journal/receipt readback for the Runtime cases. | All PSW-specific tests and implementation are pending; current Runtime recovery tests are baseline only and must not be counted as PSW coverage. |
| Trace refs: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5；[AC-AGENT-PARITY-002](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-002) | [DES-PSW-004](provider-switch-intent-continuity.design.md#des-psw-004) | Required-tier cutover fixtures and full-tier provider-pair evidence are separate proof layers; neither can substitute for the other. | The complete planned PSW-01..09 test/source/status mapping and required-tier execution rule are in the table below. Full-tier pairs builtin and real Local HTTP on the same approved P0 profile, fixture, lane, seed, observation sequence and timeout after provider-backed setup in the [local real-provider test manual](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md). Include a PSW stimulus only after the paired runner supports it or QA approves an equivalent. | For full tier, link [provider-parity-p0.sh](../../../scripts/provider-parity-p0.sh) and same-fixture/profile artifacts; record authoritative Runtime receipt/journal and restart/reconnect readback; attach QA/producer signoff using the [parity scorecard](../prd/acceptance/provider-agent-parity-score-card.md). The [live pilot parity module](../../../crates/oasis7/src/simulator/tests/agent_cognition_live_pilot_parity.rs) remains required-tier fixture support only. | No PSW-01..09 test is currently implemented or verified. The current paired script covers admitted P0 scenarios, not PSW cutover injection by itself; no provider support/default/release conclusion follows without the required evidence. |

The existing identity, live Harness, budget and Runtime recovery modules are related baseline sources, not evidence that any PSW scenario is covered. The Agent module and PSW-specific Runtime additions below are planned test work; until those sources and implementation land, required-tier PSW coverage is pending. After they land, execute `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib psw_` on the candidate tree and retain the exact command, output and commit identity. Agent evidence must include provider-call/turn lineage, handoff write plus readback or injected failure, and usage counters; Runtime evidence must include original request identity/digest/idempotency key and authoritative pending/receipt/journal readback. A passing fixture suite is required-tier evidence only.

| Scenario | Planned required-tier Agent test (`agent_cognition_provider_switch.rs`, new) | Planned Runtime test (`agent_cognition_recovery.rs`, addition) | Current status |
| --- | --- | --- | --- |
| PSW-01 late old provider response | `psw_01_late_provider_response_stays_on_original_binding` | — | Pending implementation and test. |
| PSW-02 provider/lane digest binding | `psw_02_new_binding_changes_request_identity` | — | Pending implementation and test. |
| PSW-03 pre-submit candidate cutover | `psw_03_candidate_is_not_replayed_to_new_provider` | — | Pending implementation and test. |
| PSW-04 pending Runtime request handoff | `psw_04_handoff_is_durable_before_new_turn` | `psw_04_pending_request_keeps_original_authority_identity` | Pending implementation and both tests. |
| PSW-05 late receipt correlation | `psw_05_late_receipt_matches_only_old_handoff` | `psw_05_late_receipt_keeps_original_disposition_and_effect` | Pending implementation and both tests. |
| PSW-06 conflicting digest | `psw_06_fresh_turn_does_not_reuse_old_request_identity` | `psw_06_same_identity_with_conflicting_digest_fails_closed` | Pending implementation and both tests. |
| PSW-07 handoff/readback failure | `psw_07_failed_handoff_keeps_single_flight_slot` | — | Pending implementation and test. |
| PSW-08 budget continuity | `psw_08_handoff_preserves_model_and_tool_budget` | — | Pending implementation and test. |
| PSW-09 bounded provider timeout and fresh lineage | `psw_09_timeout_handoff_starts_fresh_lineage` | `psw_09_old_request_remains_pending_after_new_turn` | Pending implementation and both tests. |

### 11.2 必需场景

- PSW-01: switch provider while the old adapter request is in flight; a late old response matches only the old request, and no request is issued through the new route for that turn.
- PSW-02: change lane with all other inputs fixed; the new lane binding is in the new request digest or a new context version is required; old request digest is unchanged.
- PSW-03: switch after an Agent candidate exists but before Runtime submission; do not invoke the new provider with the old candidate or submit it without its prior authorization path.
- PSW-04: Runtime remains pending after the provider attempt and acknowledgement are lost; persist the exact handoff record, then start a new provider-bound turn while the old Runtime request remains pending.
- PSW-05: a later old receipt resolves only against the old session/turn/request and recovery correlation; it cannot overwrite the new turn or create another effect.
- PSW-06: retry the same Runtime identity with a different digest; fail closed and produce no second effect.
- PSW-07: handoff persistence or Runtime readback fails; retain the active-turn slot and do not start a new provider attempt.
- PSW-08: handoff preserves model/tool usage and remaining budget; changing session does not reset or refund consumed credits.
- PSW-09: old provider becomes unavailable after cutover; after its bounded attempt ends, a durable handoff may release the slot; the new provider gets fresh IDs and causal lineage, never the old request/candidate.

The current paired parity CLI accepts P0-001 through P0-005; it is a real provider-pair procedure but does not yet declare a PSW cutover scenario. Before full-tier cutover can be claimed, add the PSW stimulus to that paired runner or record a QA-approved equivalent procedure in task evidence. Until that method is selected and executed, PSW full-tier evidence remains pending.

## 12. 决策、长期风险与未决问题

| 决策 | 选择与理由 | 代价 / 风险 | Owner / 复核触发 |
| --- | --- | --- | --- |
| Provider/lane change starts a new immutable session binding | Fits existing session profile revision and prevents late response rebinding. | A new session waits while the old adapter attempt is active or a pending handoff cannot be durably committed. | Agent owner; review if Harness replaces session-scoped profile revision. |
| Runtime identity is reused only for recovery of the same submitted request | Preserves Runtime idempotency and authoritative result; fresh Agent intent gets fresh lineage. | Requires reconnect to locate old correlation and receipt. | Runtime owner; review on request/receipt contract change. |
| Durable bounded handoff separates adapter single-flight from Runtime pending | Lets a fresh provider-bound turn begin after the old provider attempt ends while preserving the unresolved Runtime request on its original chain. | Requires durable Agent recovery metadata and existing Runtime readback; cannot release the slot on uncertain state. | Agent and Runtime owners; review on recovery persistence or pending-capacity changes. |
| No automatic cross-provider fallback | Avoids changing the model decision behind an existing identity or replaying old candidate. | Provider outage can reduce continuity until an explicit new turn is safe. | Product/Agent owners; review only with an explicit fallback contract and paired QA evidence. |

Residual risk: current target documents establish stable provider profile, request digest, single-flight, retry, Runtime idempotency and receipt recovery separately, but do not prove that each live route includes lane selection in its digest, that all adapters can recover the same response after process restart, or that a durable Agent handoff store is wired to Runtime readback. The bounded handoff is an explicit exception to the current Harness no-next-turn-while-pending rule; its implementation must update that lifecycle projection while preserving a single active adapter attempt. `AgentIntentV2` chat-path identity is scoped to its own actor/authority/message inputs and is not proof that provider/lane identity is bound for every parity lane. The implementation owner must verify the actual route, digest, handoff durability and recovered Runtime correlation; otherwise the affected route remains outside this contract.

No unresolved schema, support-matrix or player-promise decision is delegated by this design. Any change to action semantics, Runtime status mapping, player messaging or supported provider combinations returns to the corresponding owner through TPM.

<a id="des-psw-001"></a>
### DES-PSW-001：session provider/lane binding is immutable

Each Agent session resolves its provider configuration reference, profile revision, adapter protocol version and execution-lane selection/revision before its first turn. The effective binding is immutable for all requests in that session. Every target request digest must bind that complete selection, directly or through a versioned profile/context reference that deterministically resolves it. An unbound lane is not target evidence.

A provider/lane change drains the old session and applies to a new session/turn only. It does not rewrite any old context or digest. A pending Runtime request may outlive the old active turn only through the durable bounded handoff in DES-PSW-003; without that handoff the new session remains blocked. The runtime meaning and permission scope of a lane do not change here.

<a id="des-psw-002"></a>
### DES-PSW-002：pre-switch candidates keep their original lineage

A candidate is advisory output belonging to the request that produced it. Provider cutover never passes that candidate payload to the newly selected provider as a retry and does not grant permission to submit it. If the old candidate follows its existing authorization path, it retains its original request lineage and still undergoes current Runtime validation. When a fresh provider decision is needed, Agent creates a new semantic request with a causal reference to the old turn; the new provider receives a new observation/context and cannot inherit old request identity.

<a id="des-psw-003"></a>
### DES-PSW-003：reconnect recovers one authority chain

Reconnect resolves the original provider binding for an unresolved provider call. Transport retry keeps the same request identity and digest and is allowed only under same-key idempotency or response-retrieval semantics. A provider that cannot provide that guarantee cannot be replaced in-place; the old adapter attempt must first reach its existing bounded recovery boundary.

If the provider attempt has ended but the submitted Runtime request remains pending, Agent may release the active-turn slot only after a durable recovery handoff is committed. That Agent-owned record preserves the exact old agent_session_id, agent_turn_id and decision_request_id, Runtime request/intent identity, request_digest, Runtime's existing idempotency key, last authoritative pending disposition and recovery-correlation identity. It records no new Runtime disposition and does not change the original authority chain. If Runtime state cannot be read or the handoff cannot be durably committed, keep the active turn blocked until reconciliation.

After handoff, one fresh session/turn may run under the new provider/lane binding while the old Runtime request remains pending. It has fresh identities and a causal reference; it cannot reuse the old candidate or request. A late provider response, old request response or receipt correlates only to the old recovery record; provider output arriving after handoff cannot become a Runtime submission. Pending remains pending until Runtime returns an authoritative terminal disposition. Same identity plus different digest fails closed. Cutover and reconnect never infer cancel, reject, commit, expiry or a second world effect. The handoff does not refund consumed calls or reset the remaining model/tool budget.

<a id="des-psw-004"></a>
### DES-PSW-004：cutover evaluation separates required and full evidence

Required-tier PSW-01 through PSW-09 tests are planned, not present or verified. Their Agent/Runtime source split, test identifiers and execution/evidence rules are specified in §11.1; the existing `agent_cognition_recovery.rs` tests cover committed response replay, duplicate committed projection and legacy snapshot compatibility only. The bounded handoff remains an explicit exception to the current Harness no-next-turn-while-pending rule and must be incorporated into the lifecycle projection before implementation enablement; one active adapter attempt remains invariant. Full-tier evaluation runs the declared builtin and real Local HTTP provider pair on the same approved fixture/profile and lane, then pairs its artifacts with authoritative Runtime receipt/journal evidence across restart/reconnect and a QA/producer scorecard. The live pilot parity test module is required-tier fixture support only. Candidate selection rules and provider scope come from the linked parity authority; no mock, local fixture, script output, historical sample or design check alone expands provider support, default eligibility or release readiness.
