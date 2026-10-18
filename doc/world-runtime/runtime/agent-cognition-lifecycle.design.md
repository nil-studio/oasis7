# oasis7 Runtime：Agent cognition lifecycle design

- 对应需求文档：agent-cognition-lifecycle.prd.md
- PRD-ID：PRD-WORLD_RUNTIME-047
- 设计状态：target contract；不是 implementation report
- 上位设计：doc/world-runtime/runtime/runtime-integration.md
- Agent module 配套：doc/world-runtime/module/agent-default-modules.design.md

> 本设计只规定 runtime lifecycle、identity、MVCC、journal、recovery、scheduler 与 continuation 的边界。prompting、memory、goal、model/provider policy 由 Agent 专业文档定义，runtime 只保存完成执行与 replay 所需的 digest、canonical response/action 和引用。

审计轮次: 1

<a id="1-boundary-map"></a>
## 1. Boundary map：目标与范围

~~~text
committed World
    │ deterministic wake/event
    ▼
AgentScheduler ── durable TurnStarted/ContextCaptured ──► AgentActor
    │                                                     │
    │ world keeps ticking                                 │ provider/builtin I/O
    │                                                     ▼
    └──── AgentDecisionEnvelope ◄── ResponseRecorded ◄────┘
                         │
                         ▼
       host authority/capability + MVCC + kernel validation
                         │
               accepted world receipt / structured reject
~~~

World execution owns the only canonical state transition. AgentActor may run on another task, thread or process, but its output is data, not authority. AgentScheduler owns durable schedule, logical wake, lifecycle delivery and recovery; Harness/Agent owns continuation policy and proposal semantics. The scheduler does not grant capabilities or bypass the kernel. World::step must never call provider, await a provider future, wait on a response channel, or use provider wall-clock completion to decide whether the world tick commits.

The existing synchronous runner can remain as a compatibility adapter during migration. The adapter must wrap its result in the same envelope and validation path; it must not be treated as proof that synchronous cognition is the target architecture.

Current source distinguishes that simulator compatibility lane from native runtime-live: `llm_sidecar_runner.rs:211–229` constructs `AsyncAgentRunner` for both Builtin and ProviderBacked; `async_agent_runner.rs:258–277,360–383,806–819` executes decisions on workers and polls without waiting while kernel action application stays synchronous and authoritative. Native ordinary Wait uses current-context Harness proposal validation, Runtime admission and projection/readback, then Runtime wake selection/resume and Harness reconciliation (`llm_sidecar_async.rs:490–529`, `llm_sidecar_cognition_wait.rs:419–592`, `llm_sidecar_cognition.rs:339–442,757`); WaitTicks retains a local timer. Source wiring and bounded mock fixtures do not prove complete durable scheduler/journal, remote restart/reconnect/rebind/reorg, browser service or full nonblocking acceptance. The logical-tick helper is narrower than a World commit; `run_one_turn` intentionally waits for tests/CLI. This note qualifies current lanes without changing the target records, algorithms or P0/P1 evidence gates below.

## 1.1 Canonical owner goal transport

Chain-linked `AgentChatRequest` uses the registered WorldService payload rather than mutating a Viewer projection. The original player proof signs the complete existing world/reorg/scope/replacement envelope and the optional `canonical_authority` branch/Agent-identity-generation extension. Legacy absence is omitted from request and signing bytes. Canonical admission requires the currently journaled Agent identity owner binding and unique matching `starter_oc_claims.public_key`; Hosted registration, economic allowance and provider signer delegation do not install or replace this owner authority.

The Node checks owner signature, world, branch, identity generation, monotonic owner chat nonce, bounded text and the exact active replacement intent through the existing intent kernel. Original signed correlation results resolve before mutable replacement/nonce freshness checks. Execution repeats admission against the current candidate, fences nonces across same-block staged results, journals intent replacement and publishes the result in the existing transaction. A stale or rejected candidate cannot replace the active goal. No cognition proposal gains goal/control authority or a larger execution budget.

`Received`/`Pending` transport ACKs have status `pending` and no Runtime intent/event reference; they echo the original signed nonce/intent tuple. Canonical acceptance comes from the committed original result. The Viewer restores goal metadata only from an authenticated committed owner View matched to the current intent/result. A verified Agent projection can publish a typed owner identity fence without goal text; initial owner reads bootstrap from the verified public commit and authenticated browser session when that fence is unavailable. The browser requests a session-bound read context and signs the complete existing `/v1/world/view` canonical-CBOR request with its owner key; the configured client forwards that original proof unchanged and verifies the trusted service signature, request digest and minimum commit. Owner read results stay in the connection session; an execution-only AgentHost projection may receive the accepted goal, while shared World/frame data and public prompt-profile copies omit it. Changed player/key/world/branch/reorg and known identity/current-intent fences invalidate the read context; a newer public commit clears an older cached private view, and clears the AgentHost goal when no current authorized Agent identity/current-intent fence is available, until a fresh owner read confirms it. With a matching current authorized Agent fence the internal execution goal can survive unrelated commits. Public-only browser reads refresh on snapshots/reconnect/pending-result polling; without such refresh execution can pause between commits. This bounded path does not establish unattended autonomous continuity. Public and delegated Views keep sanitized intent summaries and omit raw owner goal text. Every outbound legacy/mounted-observer snapshot copy filters canonical AgentChat results and queued signed chat inputs (including nested pending/inflight effect inputs), while preserving authoritative World, journal, checkpoint and commitments. The dedicated authenticated owner projection retains the effective goal. Operator/canonical execution records and raw on-disk/CAS data necessarily retain the unencrypted signed input; this is an access boundary, not cryptographic confidentiality. The browser signs with its own original owner key; the server does not sign on its behalf or copy sidecar goal caches into canonical state.

The provider-local prompt is only a disposable projection of an owner-authenticated committed goal. A delegate View cannot recover raw owner goal text. This bounded path does not establish autonomous provider continuity, a working real-provider route, lawful retained-world ownership bootstrap, control-grant revocation policy, independent deployment fault domains or complete production history. If canonical identity/owner claim is absent, admission and owner metadata fail closed; Hosted login alone is insufficient. Fixture registration/claim/identity tests are engineering evidence, not formal opening approval.

## 2. Logical records

### 2.1 需求承接与分配表

These two relations qualify the affected current lanes only; the original logical records and lifecycle targets below remain authoritative. The two added product relations below are limited recovery dependencies via the existing professional PRD §4.2 receiver, not complete allocation or acceptance of this lifecycle design. Identity/hash, MVCC, all journal prefixes, scheduler/continuation, retention/GC, authority and rollout obligations remain independently applicable. The existing [collab-recovery authority](player-runtime-collaboration.design.md#collab-recovery) remains the principal product receiver; product coverage and implementation states are unchanged. Both product trace tables already designate [professional_acceptance: PRD §4.2 recovery](agent-cognition-lifecycle.prd.md#42-recovery-rules); the following rows reference their existing requirement IDs and corresponding ACs without declaring new IDs.

| 上游条款 | 专业义务 | 本设计承接 | Owner / dependency | 排除与未证明范围 |
| --- | --- | --- | --- | --- |
| [professional_acceptance: PRD §1](agent-cognition-lifecycle.prd.md#1-目标与边界) | Distinguish synchronous simulator compatibility from native Builtin/ProviderBacked shared AsyncAgentRunner worker and nonblocking polling; current wiring is narrower than the target scheduler. | [Boundary map](agent-cognition-lifecycle.design.md#1-boundary-map) | Runtime owns world execution/admission/journal; Agent owns cognition/memory/goal/provider policy; QA qualifies proof and fixture applicability. | No new envelope/schema/hash/MVCC/journal/scheduler/restart implementation; no full remote/browser/finality or universal nonblocking proof.  Source-evidence applicability sub-obligation only: Product N/A; reason: locating and qualifying bounded source evidence does not independently establish a player result; scope: only native Builtin/ProviderBacked worker/poll versus synchronous simulator compatibility source qualification. Result obligations (not N/A): canonical World advancement while provider remains in flight, and successful durable Wait only after admission with scheduled/pending readback and wake/resume; retained PRD §1/§9 and body acceptance apply. Product-leaf alignment of those result obligations is not resolved by this package or by the two limited recovery dependencies. |
| [professional_acceptance: PRD §9](agent-cognition-lifecycle.prd.md#9-证据锚点与专业跟进) | Qualify native ordinary Wait current-context proposal/admission/projection/readback/wake/resume/reconcile separately from WaitTicks timer; failed admission is not successful durable waiting. | [Boundary map](agent-cognition-lifecycle.design.md#1-boundary-map) | Runtime owns admission/world facts/wake/readback; Agent owns continuation proposal/policy; QA separates located mock fixtures from executed real-provider proof. | No change to record/digest/MVCC or durable recovery algorithms; no complete exactly-once scheduler, remote restart/reconnect/rebind/reorg, browser or finality certification.  Source-evidence applicability sub-obligation only: Product N/A; reason: locating and qualifying bounded source evidence does not independently establish a player result; scope: only ordinary Wait admission/readback/wake qualification versus WaitTicks local timer. Result obligations (not N/A): canonical World advancement while provider remains in flight, and successful durable Wait only after admission with scheduled/pending readback and wake/resume; retained PRD §1/§9 and body acceptance apply. Product-leaf alignment of those result obligations is not resolved by this package or by the two limited recovery dependencies. |
| Trace refs: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；[product_requirement: REQ-EXT-008](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-008) / AC-EXT-008 | Existing professional PRD §4.2 recovery receiver → design §5.1–5.3, with §7 integration and §9 verification: after ResponseRecorded, replay the recorded response without provider redispatch; reconcile unknown submission against the original marker/key/receipt, repair committed projections only, never repeat canonical world effects or revive old branch/authority. | [Journal/recovery](#5-journal-layout-and-recovery) | Runtime owns canonical commit/recovery; Agent owns provider/continuation policy; collaboration authority owns product recovery and QA owns acceptance. | Limited Runtime dependency only: excludes private-context-loss notice/trusted replanning UI, real remote restart, complete production history and full EXT-008 acceptance. EXT remains partial design/implementation. |
| Trace refs: O7-REQ-6044cb04-5fbb-4c68-81a6-fc57cfdd608a；[product_requirement: REQ-COLLAB-006](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#req-collab-006) / AC-COLLAB-006 | Existing professional PRD §4.2 recovery receiver → design §5.1–5.3, with §7 integration and §9 verification: after ResponseRecorded, replay the recorded response without provider redispatch; reconcile unknown submission against the original marker/key/receipt, repair committed projections only, never repeat canonical world effects or revive old branch/authority. | [Journal/recovery](#5-journal-layout-and-recovery) | Runtime owns canonical commit/recovery; Agent owns provider/continuation policy; collaboration authority owns product recovery and QA owns acceptance. | Limited Runtime dependency only: excludes collaboration message/snapshot pagination, unanswered questions, pending approvals, cross-device redelivery and complete COLLAB-006 acceptance. Existing design-covered/partially-implemented product state is unchanged. |

### 2.1 Turn and request

~~~text
CognitionTurn {
  schema_version,
  world_id,
  agent_id,
  branch_id,
  finality_epoch,
  finality_block_hash?,
  finality_status,
  agent_session_id,
  agent_turn_id,
  decision_request_id,
  retry_seq,
  transport_attempt,
  status,
  base_tick,
  base_world_hash,
  reorg_epoch,
  runtime_manifest_hash,
  capability_snapshot_hash,
  authority_context_hash,
  observation_digest,
  context_digest,
  issued_at_tick,
  valid_until_tick,
  request_digest,
  journal_seq,
}
~~~

agent_turn_id identifies one logical Agent cognition turn. decision_request_id identifies one immutable input request within the turn, and agent_session_id preserves the Harness partition across process recovery. Harness exclusively allocates these three identities; Runtime validates and persists them but does not replace them with local IDs. The request_digest comes from the Agent-owned versioned DecisionRequest canonical input contract; runtime verifies its binding and treats it as opaque. A transport retry keeps all three identities and increments transport_attempt; it does not create a new strategy turn. A semantic retry/new request increments retry_seq, creates a new request/turn identity and retains a causal reference to the old one.

base_world_hash is `H_v1("oasis7.runtime.world-state.v1", canonical({world_id, branch_id,
finality_epoch, finality_block_hash, finality_status, logical_tick, state_root, reorg_epoch,
runtime_manifest_hash}))` for the canonical committed
parent identity, not a serialization of arbitrary process memory. Production MVCC requires a
versioned state_root and trusted typed branch/finality binding; missing or uncertain inputs enter
recovery_pending rather than using a local fallback. Wall-clock timestamps, pointer addresses,
hash-map iteration order and diagnostic trace fields are excluded.

### 2.2 AgentDecisionEnvelope

~~~text
AgentDecisionEnvelope {
  schema_version,
  world_id,
  agent_id,
  branch_id,
  finality_epoch,
  finality_block_hash?,
  finality_status,
  agent_session_id,
  agent_turn_id,
  decision_request_id,
  retry_seq,
  base_tick,
  base_world_hash,
  reorg_epoch,
  runtime_manifest_hash,
  capability_snapshot_hash,
  authority_context_hash,
  observation_digest,
  context_digest,
  issued_at_tick,
  valid_until_tick,
  preconditions[],
  decision_kind,
  action,
  request_digest,
  decision_digest,
  envelope_digest,
  provider_invocation_key,
  envelope_idempotency_key,
  origin_intent_ref?,
  source,
}
~~~

PreconditionV1 is the only v1 precondition shape:

~~~text
PreconditionV1 {
  schema_version,
  subject: { kind, id },
  path_or_rule,
  operator,
  expected_value_bytes,
  missing_behavior: fail,
}
~~~

`preconditions[]` is a canonical, bounded all-of list sorted by canonical item bytes; duplicate
items, unknown schema/path/operator or missing `expected_value_bytes` fail closed as
`precondition_failed`.
All items are evaluated against one committed parent snapshot before staging. Product-specific rule
meaning remains in the kernel/gameplay authority; the envelope only makes the dependency explicit
and replayable.

The v1 path registry is fixed to `world.logical_tick` (u64), `world.state_root` (digest),
`world.runtime_manifest_hash` (digest), `world.reorg_epoch` (u64), `agent.status` (enum),
`agent.position` (canonical `[i64,i64]`), `agent.resource.<resource_id>` (i64),
`agent.inventory_digest` (digest), `agent.capability_snapshot_hash` (digest), and `intent.status`
(enum). `<resource_id>` is an NFC UTF-8 registry id. `subject.kind` is exactly
`world | agent | intent` and must match the path namespace: `world.*` uses `world`/current
`world_id`, `agent.*` uses `agent`/current `agent_id`, and `intent.status` uses
`intent`/`origin_intent_ref.intent_id`; `subject.id` is a non-empty NFC UTF-8 canonical identifier
of at most 128 bytes. The `agent.status` enum registry is exactly
`idle | executing | blocked | waiting | unavailable`; the `intent.status` registry is exactly
`proposed | submitted | accepted | blocked | completed | rejected | expired | cancelled |
superseded`. Operators are exactly `eq | neq | lt | lte | gt | gte`; digest/enum/tuple paths allow only `eq|neq`, while numeric paths allow all six. Expected
values are type-checked RFC 8949 deterministic-CBOR canonical bytes in `expected_value_bytes` (not an
unverified digest-only claim; a digest is derived for indexing only). Each path is at most 128 bytes,
expected bytes at most 512 bytes, at most 32 items and 4096
total canonical bytes. Unknown path/operator, type mismatch, malformed bytes, duplicate or overflow
is `precondition_failed` with no effect.

origin_intent_ref is optional and contains the exact AgentIntentV2 id, request digest and authority tuple used to create the candidate. It is a reference, not a second intent record. Provider output must never be accepted as a capability grant, player intent transition or module installation command without host validation.

### 2.3 Continuation

~~~text
AgentContinuation {
  continuation_id,
  wake_id,
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
  status: ContinuationStatusV1,
  terminal_disposition?,
}
~~~

`AgentContinuation.status` is the sole `ContinuationStatusV1` enum:
`scheduled | pending | waking | consumed | completed | cancelled | invalidated | expired | rejected`.
Allowed transitions are exactly `scheduled -> pending|waking|cancelled|invalidated|expired|rejected`,
`pending -> waking|cancelled|invalidated|expired|rejected`,
`waking -> consumed|cancelled|invalidated|expired|rejected`, and
`consumed -> scheduled|completed|cancelled|invalidated|expired|rejected`.
`completed/cancelled/invalidated/expired/rejected` are terminal; an unknown transition enters
`recovery_pending` and is never an implicit retry. `terminal_disposition` is present only on a
terminal status, and `pending` carries a durable scheduler/recovery reason.

`continuation_status_digest = H_v1("oasis7.cognition.continuation-status.v1", canonical({
continuation_id, wake_id, wake_seq, from_status?, to_status, logical_tick, world_id, branch_id,
finality_epoch, finality_block_hash?, finality_status, reorg_epoch, proposal_digest,
terminal_disposition? }))`; status is canonical enum text with explicit optional presence, and the
digest field itself is excluded. `continuation_digest` includes the current `ContinuationStatusV1`
and `continuation_status_digest`; an empty string or null cannot stand in for either.

WakeConditionV1 is the canonical runtime evaluation shape:

~~~text
WakeConditionV1 {
  schema_version,
  kind: at_or_after_tick | world_event_committed | receipt_linked | state_predicate,
  logical_tick?, event_digest?, receipt_id?,
  subject: { kind, id }?, path_or_rule?, operator?, expected_value_bytes?
}
~~~

The `wake_conditions[]` list is v1 all-of, sorted by canonical condition bytes, with no nested
boolean expressions. Its fixed one-of registry is:

| kind | required fields | forbidden fields | bounds/evaluation |
| --- | --- | --- | --- |
| `at_or_after_tick` | `logical_tick` | `event_digest`, `receipt_id`, `subject`, `path_or_rule`, `operator`, `expected_value_bytes` | u64 tick; contributes to derived `next_wake_tick` |
| `world_event_committed` | `event_digest` | `logical_tick`, `receipt_id`, `subject`, `path_or_rule`, `operator`, `expected_value_bytes` | canonical digest encoding ≤ 128 bytes; committed events only |
| `receipt_linked` | `receipt_id` | `logical_tick`, `event_digest`, `subject`, `path_or_rule`, `operator`, `expected_value_bytes` | canonical receipt id ≤ 128 bytes; `WorldReceiptLinked` only |
| `state_predicate` | `subject`, `path_or_rule`, `operator`, `expected_value_bytes` | `logical_tick`, `event_digest`, `receipt_id` | exactly the PreconditionV1 path/type/operator registry; expected bytes ≤ 512 |

`wake_conditions=[]` is rejected in v1 as `wake_conditions_empty`; it is never interpreted as an
immediate wake, an indefinitely true condition or a provider callback. Each item is at most 768
canonical bytes; the list is at most 16 items and 4096 total canonical bytes, with nesting depth 1.
Unknown schema/kind/path/operator, malformed or duplicate items, a
forbidden field, type mismatch or any bound overflow rejects the proposal. Runtime derives
`next_wake_tick = max(current logical tick, every at_or_after_tick.logical_tick)`; it is absent when
there is no tick condition, it denotes an event/receipt/state-head-driven schedule: Runtime
re-evaluates it when the relevant committed event, receipt link or state head changes, and it must
not be replaced by a wall-clock timer. Evaluation reads one committed world head and records
evaluation tick/hash/reorg. Missing event/receipt or a false
predicate remains pending until expiry; expired or GC'd references terminate as
`wake_condition_expired`. A continuation may request a next wake, suspend, or terminate. The
scheduler consumes the old schedule before creating the next one, so a stale wake cannot self-replicate.

## 3. Canonical identity and hashing

The Agent-owned DecisionRequest contract defines request_digest using a versioned canonical input encoding. Runtime verifies the declared binding and does not re-hash request fields under a competing schema. Runtime-owned records use the shared v1 registry: BLAKE3-256 over RFC 8949 deterministic CBOR `[domain, payload]`, rendered as `blake3:<64 lowercase hex>`. Arrays are sorted only when their schema declares order-insensitive semantics; maps use canonical key order; optional values have explicit presence tags; no floating wall-clock or provider trace value enters a world-effect digest. The v1 parent world hash is `H_v1("oasis7.runtime.world-state.v1", canonical({world_id, branch_id, finality_epoch, finality_block_hash, finality_status, logical_tick, state_root, reorg_epoch, runtime_manifest_hash}))`; production MVCC requires a versioned state_root and trusted typed branch/finality binding. The algorithm self-test is `BLAKE3-256(empty) = af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262`; P0.1 additionally requires one complete shared request/envelope golden fixture.

Harness `runtime_binding.branch_id/finality_epoch/finality_block_hash/finality_status/reorg_epoch`
is represented at the Runtime boundary by the same `FinalityBindingV1` names and ordering (no new
wire alias):

~~~text
FinalityBindingV1 {
  schema_version,
  branch_id,
  finality_epoch: u64,
  finality_block_hash?,
  finality_status: pending | verified | reorged | suspended,
  reorg_epoch: u64,
}
finality_binding_digest = H_v1(
  "oasis7.runtime.finality-binding.v1", canonical(FinalityBindingV1))
~~~

`finality_block_hash` is a canonical 32-byte block digest and is required when
`finality_status=verified`; other statuses may omit it but must validate it when present.
`finality_epoch` and `reorg_epoch` are u64, and `branch_id` is a bounded canonical identifier.
Unknown status, negative values, invalid hash length or an illegal status/hash combination enters
`recovery_pending`. These typed fields are trusted upstream bindings; Runtime compares and persists
them but does not create a consensus proof.

~~~text
provider_invocation_key =
  H_v1("oasis7.cognition.provider-invocation.v1", request_digest)

decision_digest =
  H_v1("oasis7.cognition.decision.v1",
    canonical({ request_digest, decision_kind, canonical(action) }))

envelope_digest =
  H_v1("oasis7.cognition.envelope.v1",
    canonical({ request_digest, decision_digest, canonical(action), world_id, agent_id,
                agent_session_id, agent_turn_id, decision_request_id, retry_seq,
                branch_id, finality_epoch, finality_block_hash, finality_status,
                finality_binding_digest, base_tick, base_world_hash,
                reorg_epoch, runtime_manifest_hash, capability_snapshot_hash,
                authority_context_hash, issued_at_tick, valid_until_tick, preconditions,
                origin_intent_ref }))

envelope_idempotency_key =
  H_v1("oasis7.cognition.envelope-idempotency.v1",
    canonical({ request_digest, envelope_digest }))
~~~

The provider adapter receives provider_invocation_key unchanged. A session key may be useful for provider conversation routing, but it is not a cognition idempotency key and must not be used alone. Prompt text, memory summary, goal prose, token counts and latency can be stored as bounded artifacts referenced by digest; they cannot change the host action identity after the request is captured.

### 3.1.1 Session/Turn mapping seam

Harness exclusively allocates and owns agent_session_id, agent_turn_id and decision_request_id.
Runtime validates exact equality with agent subject, world binding and request_digest, then persists
the correlation. Runtime allocates journal_seq, action_id, receipt_id and wake_id only.

| Harness field | Runtime binding | Retry rule |
| --- | --- | --- |
| agent_session_id / agent_turn_id / decision_request_id | same wire fields; identity must match request_digest and world binding | transport retry keeps all fields |
| retry_seq | semantic request retry/new-request lineage and parent causal reference | transport retry does not increment |
| transport_attempt | provider dispatch metadata on RequestDispatched/ResponseRecorded/retry events | increments for every at-least-once delivery attempt |
| continuation proposal identity | validated proposal correlation | Runtime assigns wake_id |

CognitionJournal must persist retry_seq and transport_attempt separately. A generic attempt field
must not replace them: retry_seq describes semantic lineage, while transport_attempt describes each
at-least-once provider delivery attempt.

## 4. State machine

### 4.1 Turn states

~~~text
queued
  -> running
  -> waiting_provider
  -> response_recorded
  -> ready_to_submit
  -> accepted -> receipt_linked -> completed(status=committed)
  -> rejected_* -> completed(status=rejected)
  -> failed_* -> retry_scheduled | recovery_pending | completed(status=failed)
  -> pending -> recovery_pending | accepted | rejected_* | completed(status=failed)
~~~

queued, running, waiting_provider, response_recorded, ready_to_submit, accepted/rejected, retry_scheduled,
cancelled, cognition_failed and terminal states are durable projections of journal events. Unknown
state or illegal transition is a recovery fault, not an implicit retry. `retry_scheduled` remains
pending until a bounded retry or terminal failure; `cognition_failed` is terminal; cancellation closes
the active turn and cannot be reopened by a late response.

### 4.2 Validation order

The runtime submit path is deterministic and must use this order:

1. Decode and validate schema version, world_id, agent_id, branch_id, typed
   finality_epoch/finality_block_hash/finality_status and reorg_epoch.
2. Validate exact agent_session_id/agent_turn_id/decision_request_id correlation and its opaque request_digest binding.
3. Look up envelope_idempotency_key. Return the existing disposition/receipt for the same digest; reject a digest conflict.
4. Compare base_tick and base_world_hash with the current committed parent.
5. Check current logical tick against valid_until_tick.
6. Rebuild/verify current authority and capability context; compare snapshot digests.
7. Validate origin_intent_ref against the current AgentIntentV2 ledger when present.
8. Evaluate PreconditionV1 items in canonical order against one parent snapshot.
9. Run the normal kernel/module/action validation and affordability pipeline.
10. Stage the action, event, receipt and journal links; publish only at the WorldCommitRecordV1 atomic commit boundary.

The path must not refresh a stale envelope in place. Replanning, new capability discovery or a changed quote starts a new request/turn.

### 4.3 Disposition table

| Check | Stable disposition | Effect |
| --- | --- | --- |
| base tick/hash mismatch | stale_base | no effect |
| validity window elapsed | expired | no effect |
| capability digest changed | stale_capability_snapshot | no effect |
| branch/finality_epoch/finality_block_hash/finality_status/reorg binding changed | reorg_invalidated | no effect |
| authority/intent tuple invalid | authority_denied or intent_conflict | no effect |
| precondition false | precondition_failed | no effect |
| ordinary kernel/module rule fails | action_rejected | no effect |
| same key, different digest | idempotency_conflict | no effect |
| same key, same digest | prior disposition | no second execution |
| retry scheduled | retry_scheduled | pending, no receipt or effect |
| cancelled turn or late response after cancel | cancelled / late_response_after_cancel | no effect; no turn reopen |
| cognition persistence/provider terminal fault | cognition_failed | no effect unless a committed record already exists |
| persistence/journal unavailable | recovery_pending or cognition_failed | no world effect unless prior commit is already durable |

When the trusted branch_id, finality_epoch, finality_block_hash, finality_status or reorg_epoch changes, every non-committed turn,
response, pending wake and continuation bound to the old parent is durably invalidated as
`reorg_invalidated`; it must not be rebound to a new parent. A new observation must create a new
Harness session/turn/request. An already committed `WorldCommitRecordV1` and receipt are read-only
history and can be projected once, never executed again. If any typed upstream finality/branch state is
unavailable or contradictory, the disposition is `recovery_pending`.

## 5. Journal layout and recovery

### 5.1 Event records

Every CognitionJournal event has:

~~~text
JournalEvent {
  schema_version,
  journal_seq,
  parent_event_digest,
  event_kind,
  world_id,
  branch_id,
  finality_epoch,
  finality_block_hash?,
  finality_status,
  reorg_epoch,
  agent_id,
  agent_session_id,
  agent_turn_id,
  decision_request_id,
  logical_tick,
  request_digest?,
  response_digest?,
  envelope_digest?,
  receipt_id?,
  retry_seq,
  transport_attempt,
  status,
  payload_digest,
  causal_refs[],
}
~~~

The event payload is bounded and canonical. The full provider response is either represented by a canonical response/action needed for replay or by a content-addressed artifact whose availability and hash are verified. A diagnostic trace without canonical response/action is insufficient for recovery.

Only response-bearing turns use this common ordered event prefix; pre-response failure,
cancellation and backpressure use the explicit branches below:

~~~text
TurnStarted
ContextCaptured
RequestDispatched
ResponseRecorded
DecisionEnvelopeSubmitted
~~~

Terminal paths are explicit: `DecisionValidated` (provisional) → `WorldReceiptLinked` →
`TurnCompleted(status=committed)`; `DecisionRejected` → `TurnCompleted(status=rejected)` with no
receipt/link; `CognitionTurnFailed(status=failed)` → `TurnCompleted(status=failed)` only when the
turn is terminal; and `RecoveryDispositioned(status=pending)` or `ContinuationScheduled(status=pending)`
remains pending until one of those terminal paths. `WorldReceiptLinked` is forbidden on rejected,
failed or pending paths.

The exact non-response and cancellation sequences are:

| branch | exact durable event sequence | terminal/result rule |
| --- | --- | --- |
| response-bearing | `TurnStarted -> ContextCaptured -> RequestDispatched -> ResponseRecorded -> DecisionEnvelopeSubmitted -> DecisionValidated -> WorldReceiptLinked -> TurnCompleted(status=committed)` or its rejected/failed/pending suffix | response/action digest is durable before validation; only normal validation/commit may close it |
| provider/persistence failure before response | `TurnStarted -> ContextCaptured -> (RequestDispatched)? -> CognitionTurnFailed(status=failed) -> TurnCompleted(status=failed)` | no `ResponseRecorded`, envelope, receipt or effect; bounded retry uses a new `retry_seq` |
| cancellation before response | `TurnStarted -> ContextCaptured -> (RequestDispatched)? -> CognitionTurnCancelled(status=rejected, reject_reason=cancelled) -> TurnCompleted(status=rejected)` | no `ResponseRecorded`, `WorldReceiptLinked` or effect |
| scheduler queue full before delivery | `TurnStarted -> ContextCaptured -> SchedulerBackpressure(status=pending) -> RecoveryDispositioned(status=pending)` | no provider call; capacity wake reuses identity and may then enter response-bearing branch |
| cancellation after response but before commit | response-bearing prefix through `ResponseRecorded` (and optional `DecisionEnvelopeSubmitted`) -> `CognitionTurnCancelled(status=rejected, reject_reason=cancelled) -> TurnCompleted(status=rejected)` | durable response cannot become an effect; a committed marker/receipt takes precedence |
| late response after cancellation | `LateResponseRejected(status=rejected, reject_reason=late_response_after_cancel)` referencing the cancelled turn | no `ResponseRecorded`, no turn reopen and no receipt/effect; pre-existing response is only duplicate-late evidence |

DecisionValidated is provisional: it proves submit-time validation only and never implies a world
effect. The commit protocol is:

~~~text
WorldCommitRecordV1 {
  commit_id, envelope_idempotency_key, envelope_digest,
  world_id, branch_id, finality_epoch, finality_block_hash?, finality_status,
  finality_binding_digest, runtime_manifest_hash,
  action_id, parent_tick, parent_world_hash,
  staged_event_root, staged_state_root,
  receipt_id, receipt_digest, reorg_epoch,
  cognition_journal_seq, status: prepared | committed | aborted, abort_reason?,
}
~~~

`abort_reason` is required only when `status=aborted` and is absent for `prepared` or `committed`;
it is a bounded versioned Runtime disposition such as `stale_base`, `cancelled` or
`recovery_operator_abort`, never provider free text. `world_id`, `branch_id`, `finality_epoch`,
`finality_block_hash`, `finality_status`, `finality_binding_digest` and `runtime_manifest_hash` must
exactly match the envelope and the
bindings committed in `parent_world_hash`; a marker cannot be reused across worlds, branches,
finality bindings or runtime manifests.

The externally visible root is a separate trusted `WorldRootViewV1` obtained only from the
world-store committed-head index:

~~~text
WorldRootViewV1 {
  schema_version, world_id, branch_id, logical_tick, state_root,
  head_status: canonical | recovery_pending, commit_id?, quarantine_id?,
}
~~~

Only `state_root` with `head_status=canonical` is public; a staged root or marker alone is not.
For any marker/root/receipt/key conflict, the last verified canonical head (`R_parent` in the
fixture) remains the external root, while the conflicting candidate root and receipt are stored under
a durable `quarantine_id` and the disposition is `recovery_pending`. Recovery may inspect quarantine
records, but they cannot produce a committed Feedback, receipt, WorldReceiptLinked, provider retry or
world effect. A valid committed marker with only a missing projection is not a conflict: it exposes
`R_next` and reconstructs the recorded receipt.

`prepared` stores the staged transition but produces no world effect. The one canonical finalize
boundary atomically (or through the durable commit marker when stores are separate) publishes world
state/event journal, the idempotency key+digest, canonical receipt and `status=committed`. The
marker is the sole recovery anchor; `WorldReceiptLinked` is appended only after receipt durability
and is a replay-safe projection, never a second effect.

The required crash interleavings are: before `prepared` (no effect; revalidate the same key only),
prepared without committed (abort or remain `recovery_pending`, never guess/apply), committed world
state with a missing receipt/key/link (reconstruct from the commit record, never rerun kernel),
complete receipt/key/world commit with missing cognition projection (append the projection), and any
root/key/digest/marker conflict (`recovery_pending`, no provider/effect/retry). Provider failure,
cancellation, retry and continuation events reference the same turn/request and never mutate an
already terminal disposition.

The single marker protocol has one write order: (1) append durable `DecisionValidated`; (2) write
`WorldCommitRecordV1(status=prepared)` while the canonical root remains `R_parent`; (3) in one
world-store transaction write staged world event/state to `R_next`, idempotency key+digest and
canonical receipt `(receipt_id, receipt_digest)`, then mark that same record `committed`; this
transaction commit is the sole effect linearization point; (4) append `WorldReceiptLinked`; (5) append
`CognitionTurnCompleted`. A `prepared` record alone is never visible as a world effect.

Each crash fixture uses the following exact oracle; `R_parent`, `R_next`, receipt ids and dispositions
are fixture-bound values, never an unbound root assertion:

| crash prefix | final canonical world root | final receipt | final Feedback/disposition |
| --- | --- | --- | --- |
| before `prepared` | `R_parent` | absent | `recovery_pending`; if the parent is unchanged, one revalidation may finish only as `committed/R_next/receipt_id` or stable `rejected/R_parent/absent` |
| `prepared` only | `R_parent` | absent | `recovery_pending`; a separately durable abort marker closes it as `rejected`, still `R_parent`/absent |
| committed transaction | `R_next` | exact record `(receipt_id, receipt_digest)` | `committed`, exactly one receipt/effect |
| committed transaction with link/completion missing | `R_next` | exact recorded receipt | `committed`; append projections only |
| marker/root/receipt/key conflict | last verified canonical head `R_parent` remains the external root; conflicting candidate root/receipt is durable under `quarantine_id` and is not canonical | absent externally; candidate receipt only in quarantine | `recovery_pending`; no provider, kernel or effect re-execution |

The durable marker is authoritative only for the values listed in its record; a malformed marker or
an impossible root/receipt binding remains `recovery_pending` rather than being silently repaired.

### 5.2 Recovery matrix

| Durable prefix | Recovery operation | Provider call allowed? |
| --- | --- | --- |
| Started/ContextCaptured | rebuild the same request and dispatch | yes, at-least-once with same provider_invocation_key |
| RequestDispatched | retry/observe the same invocation | yes, same provider_invocation_key only |
| ResponseRecorded | reconstruct envelope from stored response | no |
| EnvelopeSubmitted | rerun deterministic validation or read stored disposition | no |
| DecisionValidated without WorldReceiptLinked and without committed commit marker | crash before canonical commit; revalidate/submit the same envelope_idempotency_key or enter stale/rejected; never repeat provider | no |
| prepared commit record without committed marker | only a valid durable `status=aborted` marker closes it as rejected; otherwise recovery_pending | no |
| committed marker/world state with missing receipt or idempotency projection | reconstruct receipt/key/link from WorldCommitRecordV1; never repeat effect | no |
| DecisionValidated without WorldReceiptLinked but with canonical receipt/committed marker | crash after world commit; read receipt and append WorldReceiptLinked; never repeat effect | no |
| Receipt linked without completed | append projection completion | no |
| terminal failure/cancel | wait for explicit new turn/retry policy | no for old request |

Recovery must verify journal prefix, commit marker and all referenced digests before scheduling. If the
response artifact, receipt, idempotency record or commit projection is missing/mismatched, reconstruct
only from `WorldCommitRecordV1`; if the record is ambiguous, enter recovery_pending and expose the
structured fault. Do not issue an unrecorded provider call or re-run a world effect.

### 5.3 Commit and delivery guarantees

The provider invocation, actor wake, queue delivery and receipt notification are at-least-once. The provider is expected to deduplicate the stable provider_invocation_key, but runtime cannot make an external service exactly-once. The runtime guarantees exactly-once for envelope_idempotency_key at the `WorldCommitRecordV1` finalize boundary. Journal sequence, world event, debit/effect and execution receipt are not replayed as a second business effect. Replay reads the response/receipt and never performs provider I/O.

Checkpoint/GC may delete only a journal prefix before the checkpoint that has no active turn, pending
wake, retry lineage, unfinalized commit or admissible late response. Every terminal disposition
(committed, rejected, failed, cancelled and its stable reason) keeps its envelope key+digest tombstone,
receipt link and canonical response artifact pinned until `retention_horizon`, which is at least the
maximum of provider retry budget, queue/wake lease, validity/late-response, snapshot-restore and
trusted reorg/finality windows plus configured safety margin. A complete v1 envelope whose
`base_tick`/`issued_at_tick` is below `gc_floor_tick` or whose tombstone is past retention returns
`expired_idempotency`; a legacy DTO/snapshot missing v1 schema, session/turn/request or envelope proof
returns `legacy_no_cognition_proof`. Neither condition can execute. An artifact is GC-eligible only
when no checkpoint, replay manifest, commit record or continuation references it.

## 6. Scheduler and continuation protocol

After a committed tick, the scheduler computes wake reasons using the sole total comparator in this
section: `(deadline_due, next_wake_tick, effective_priority, starvation_deadline_tick, cursor_distance,
canonical(agent_id), canonical(continuation_id), wake_seq)` with its declared directions. It appends the wake/turn record
before delivering to an actor. Delivery may duplicate; the actor/scheduler deduplicates by
agent_session_id/agent_turn_id/decision_request_id or continuation_id/wake_seq. Each logical tick
has `max_total_wakes_per_tick` and `max_wakes_per_agent_per_tick`; agents are served by a durable
stable round-robin cursor, with bounded aging for pending items and no starvation. Harness/Agent owns
the policy that proposes a continuation; Runtime owns durable schedule validation, persistence, wake
and recovery. Scheduler enqueue is non-blocking: a bounded queue full condition records
scheduler_backpressure with wake identity and retry sequence as durable pending/deferred state and
never blocks the world worker or calls provider synchronously. Capacity recovery resumes in the same
canonical order.

The v1 fairness fixture freezes `max_total_wakes_per_tick=8`,
`max_wakes_per_agent_per_tick=1`, `aging_after_ticks=2`, and `max_starvation_ticks=4`, with the
sole total comparator below (including cursor distance and all canonical tie-break fields). An eligible pending item waiting `aging_after_ticks`
receives at most one priority-level promotion per logical tick and never bypasses validity, reorg or
cancellation checks. No eligible item may wait more than `max_starvation_ticks` logical ticks; if a
budget or full queue prevents service, Runtime atomically records durable
`pending/scheduler_backpressure` at the ceiling, removes the item from the ready set temporarily,
and re-adds it after capacity returns with its original wake identity and age. `scheduler_cursor` is
the following durable record and advances atomically with the wake selection:

~~~text
SchedulerCursorV1 {
  schema_version, logical_tick, last_served_agent_id?, cursor_seq, policy_config_digest,
}
~~~

`policy_config_digest = H_v1("oasis7.runtime.scheduler-policy.v1", canonical({
max_total_wakes_per_tick, max_wakes_per_agent_per_tick, aging_after_ticks, max_starvation_ticks,
initial_priority, comparator, service_order }))`; the fixture values and field order are covered by this digest, preventing an old
cursor from silently replaying under a changed fairness policy.

An empty cursor starts at canonical minimum agent id. The scheduler commits the updated cursor and
wake/turn record before actor delivery. Recovery resumes from that exact cursor; missing, invalid or
journal-inconsistent cursor enters `recovery_pending` and never resets ordering or calls the provider.

The sole total fairness comparator is
`(deadline_due DESC, next_wake_tick ASC, effective_priority DESC, starvation_deadline_tick ASC, cursor_distance ASC,
canonical(agent_id) ASC, canonical(continuation_id) ASC, wake_seq ASC)`. Runtime assigns every
accepted schedule `initial_priority=0` and `eligible_since_tick`;
`effective_priority = min(7, initial_priority + floor((current_tick - eligible_since_tick) /
aging_after_ticks))`, never a provider/Harness-supplied priority. The
`starvation_deadline_tick = eligible_since_tick + max_starvation_ticks` and `deadline_due =
current_tick >= starvation_deadline_tick`; because `deadline_due` is the first key, an eligible item
at its deadline is attempted before every non-deadline ordinary-priority item. `cursor_distance` is the
ring distance from the persisted `last_served_agent_id` to the next canonical ready-agent; an empty
cursor starts at canonical minimum. Every service attempt, including queue-full ceiling
backpressure, advances and persists the cursor in the same scheduler transaction. The durable record
retains `backpressure_count`, original `eligible_since_tick` and wake identity, so retries cannot
reset aging or invent priority. A crash before that transaction leaves the cursor unchanged; a crash
after it but before actor delivery resumes with the committed cursor and same wake identity. The
comparator, cursor and policy_config_digest are the restart oracle.

An active turn has a bounded lease/status record. Lease expiry is a scheduler recovery fact, not permission to execute the old response against a new world. A late response is submitted through normal MVCC and becomes stale/expired or, if already committed, resolves to the prior receipt.

For continuation:

1. consume the current schedule and append ContinuationWoken;
2. check current world event/tick/receipt and precondition digest;
3. if valid, create the next envelope or deterministic continuation action;
4. if invalid, append a terminal disposition;
5. persist the next wake only after the current continuation transition is durable.

The scheduler must not depend on LlmAgentBehavior.active_execute_until, a process-local sleep, a provider callback closure or a global feedback deque. Those may remain compatibility inputs during migration but cannot be the recovery truth; a Harness continuation proposal must first become a durable Runtime schedule.

ContinuationProposalV1 is the wire seam from Harness to Runtime:

~~~text
ContinuationProposalV1 {
  schema_version,
  continuation_proposal_id,
  world_id,
  agent_id,
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
  wake_conditions[],
  valid_until_tick?,
  source,
  proposal_digest,
}
~~~

The fields and names above are byte-for-byte the paired Harness `ContinuationProposalV1` schema;
`wake_conditions[]` is `WakeConditionV1[]`, and no Runtime alias is accepted on the wire.
Harness owns continuation_proposal_id and policy semantics. Runtime validates current
world/agent/session/turn/request binding, persists the accepted proposal, allocates continuation_id,
wake_id, wake_seq and next_wake_tick, and emits ContinuationScheduled and ContinuationWoken with the
same correlation. Runtime retains remaining_budget unit/value; it never converts steps into ticks.
There are no proposal_id, proposer_agent_id, request_digest, action_digest, remaining_ticks or
expires_at_boundary wire aliases. An invalid or stale proposal is rejected without a wake, receipt or
world effect.

Runtime verifies `proposal_digest = H_v1("oasis7.cognition.continuation-proposal.v1",
canonical(proposal_without_proposal_digest))`. After allocating durable schedule correlation it
computes `continuation_digest = H_v1("oasis7.cognition.continuation-context.v1",
canonical({continuation_proposal_id?, proposal_digest?, continuation_id?, wake_id?, wake_seq?,
world_id?, branch_id?, finality_epoch?, finality_block_hash?, finality_status?, reorg_epoch?,
runtime_manifest_hash?, remaining_budget?, valid_until_tick?, status?, continuation_status_digest?,
terminal_disposition?}))`. Both use the shared
BLAKE3-256/RFC 8949 deterministic CBOR registry and exclude their own digest field. The unique empty
sentinel is the all-absent canonical context object; adapters cannot substitute an empty string,
missing outer field or implementation-specific null.

<a id="7-integration-boundaries"></a>
## 7. Integration boundaries：边界与结构

### Runtime

Runtime validates and persists Harness-issued session/turn/request identity, and owns canonical runtime hashing, journal append, snapshot references, MVCC validation, action commit, receipt linking, replay and recovery. Runtime allocates journal_seq, action_id, receipt_id and wake_id; it does not replace Harness agent_session_id, agent_turn_id or decision_request_id with local IDs. Existing ActionEnvelope can be read through an explicit legacy adapter; a legacy action has no stale safety and must not be labeled as an AgentDecisionEnvelope until it is wrapped with a captured base identity and explicit compatibility disposition.

### Agent

Agent owns prompting, context assembly, memory selection/write semantics, goal evaluation, model/provider choice, and strategy. Agent returns a response/envelope candidate and diagnostic artifacts. It cannot write world state or turn a diagnostic trace into a replay input. Agent-specific continuation semantics must map to the bounded scheduler continuation contract before they can survive restart.

### FeedbackEnvelope mapping

Runtime dispositions map to a target FeedbackEnvelope as follows:

| Runtime disposition | status | reject_reason | receipt / WorldReceiptLinked |
| --- | --- | --- | --- |
| accepted | committed | absent | canonical receipt and exactly one WorldReceiptLinked |
| stale_base, expired | rejected | stable disposition name | absent; no WorldReceiptLinked, world event, effect or debit |
| stale_capability_snapshot, authority_denied, intent_conflict | rejected | stable disposition name | absent; no WorldReceiptLinked |
| reorg_invalidated, finality_anchor_mismatch | rejected | stable disposition name | absent; no WorldReceiptLinked, world delta, or effect |
| precondition_failed, action_rejected | rejected | stable disposition name | absent; no WorldReceiptLinked |
| idempotency_conflict | rejected | idempotency_conflict | absent; no second effect or WorldReceiptLinked |
| duplicate with same envelope_idempotency_key and digest | original canonical status | original reason | original receipt only when the original was committed |
| failed_provider, failed_persist | failed | stable failure reason | absent unless a committed receipt was already durable |
| cognition_failed | failed | cognition_failed | absent; a committed marker/receipt takes precedence and remains committed |
| recovery_pending | pending | stable recovery reason | absent unless a committed receipt was already durable |
| retry_scheduled | pending | retry_scheduled | absent; preserve lineage and do not finish the old turn |
| product no-effect outcome | rejected | no_effect | absent; no WorldReceiptLinked, world event, effect or debit |
| scheduler_backpressure | pending | absent; reason remains in durable scheduler disposition | absent; no WorldReceiptLinked or world effect |
| cancelled | rejected | cancelled | absent; no WorldReceiptLinked, world delta or effect |
| `WorldCommitRecordV1(status=aborted)` | rejected | recorded `abort_reason` | absent; no WorldReceiptLinked, world delta or effect |
| late_response_after_cancel | rejected | late_response_after_cancel | absent; no turn reopen or effect |
| legacy_no_cognition_proof | rejected | legacy_no_cognition_proof | absent; no WorldReceiptLinked or world effect |

The canonical status vocabulary is committed/rejected/failed/pending. A product no-effect outcome
maps to rejected with reject_reason=no_effect; it is not a fifth status. The legacy
success/failure and summary fields must be mapped by an explicit compatibility adapter: only
`success=true` with a matching `WorldCommitRecordV1(status=committed)` and canonical receipt becomes
committed; legacy success without that marker/receipt becomes
`rejected + reject_reason=legacy_no_cognition_proof`. `success=false` with a runtime disposition
becomes rejected; provider/transport failure becomes failed; unresolved recovery becomes pending.
Stale remains a reject_reason, not a new status. Rejected/no-receipt fixtures must assert no
WorldReceiptLinked and no world mutation after submit and replay.

`retry_scheduled` is non-terminal pending; transport retries only increment `transport_attempt`,
while semantic retries create a new `retry_seq`/turn/request. `cognition_failed` is terminal failed.
Cancellation closes the active turn, and a late response becomes `late_response_after_cancel` without
reopening it. A committed `WorldCommitRecordV1` or matching canonical receipt always takes precedence
over a later `failed_persist` projection error; Feedback remains committed while missing links are
recovered.

### AgentIntentV2

The optional origin_intent_ref is checked against the existing durable ledger. Cognition lifecycle status is stored in CognitionJournal; AgentIntentV2 status remains the authority for its own player/authenticated intent. A failed cognition attempt does not invent or transition a player intent, and a player intent transition does not imply provider completion unless an explicit receipt link exists.

### Capability and policy

Current capability generation is limited to explicit existing DecisionRequest capability
catalog/context, ProviderBacked capability-context and AgentIntentV2 paths; it is not proof that
every turn is automatically wired. The target scheduler captures a per-turn snapshot and injects
its digest. At submit, host/runtime rebuilds the current catalog/context and revalidates the
candidate response, required capabilities, policy, module binding, budget and action rules. A
provider cannot widen a catalog, mint a grant, choose an unbound module instance or use an old
capability snapshot.

### WASM and external effects

WASM/module calls remain within the existing staged deterministic execution pipeline. An envelope may request a module command, but module ABI/schema/capability/output validation remains the WASM/runtime boundary. External effects are emitted only after canonical commit through the existing receipt/outbox contract; cognition replay never re-dispatches an external business effect.

<a id="8-rollout-and-compatibility"></a>
## 8. Rollout and compatibility：决策与风险

### P0.1

Identity/correlation only: propagate the Agent-owned opaque request_digest, define turn/request/origin references, provider_invocation_key, envelope_digest and envelope_idempotency_key, and persist enough metadata to correlate request, response, envelope, feedback and receipt. This phase cannot claim production async.

### P0.2

Async AgentActor plus MVCC: move provider I/O out of World::step, return data through a non-blocking queue, and enforce base state, capability/authority bindings, valid_until and preconditions at submit. This phase cannot claim production async until P0.1 and P0.3 also pass.

### P0.3

Minimum journal/recovery: persist the turn lifecycle, canonical response/action, feedback mapping, checkpoint head and recovery disposition. Crash/replay must not call provider after ResponseRecorded and must reproduce receipt/state roots. Production async is claimable only after P0.1–P0.3 evidence is complete.

### P1

Make Runtime scheduler/actor status and continuation schedules durable while Harness owns continuation proposals; isolate mailboxes and feedback by agent/turn/request; add checkpoint references and crash injection coverage; converge Builtin and ProviderBacked on the same envelope and journal. Keep legacy synchronous runner only behind an explicit compatibility mode.

### P2

Integrate Agent-owned memory/goal/prompt policies and bounded provider cost/latency metadata after their professional contracts are separately approved. These additions may enrich context/artifacts but cannot alter runtime identity, authority, stale rejection or replay rules.

Legacy snapshot/journal readers must use an explicit versioned, read-only compatibility adapter.
An old queued/in-flight ActionEnvelope on restore is surfaced as
`rejected + reject_reason=legacy_no_cognition_proof` (or an explicitly operator-owned pending
compatibility record), never reconstructed as a fresh MVCC envelope, given a synthetic base hash or
auto-submitted. Missing cognition records mean legacy_no_cognition_proof, not proof that the old
action was fresh or replay-safe. New execution must require the new envelope contract unless a
compatibility lane explicitly records reduced guarantees; old fields remain readable until the
configured retention window and all readers have migrated.

## 9. Verification matrix

| Fixture | Required assertion |
| --- | --- |
| blocked provider + advancing world | world tick completes without provider wait or world mutation from the actor |
| same request retry | same key/digest returns one disposition and one receipt |
| key collision | same key/different digest is rejected before effect |
| delayed response | base tick/hash mismatch is stale and leaves state/journal business effect unchanged |
| capability/authority change | stale capability or AgentIntentV2 mismatch fails before kernel effect |
| crash at every journal edge | restore yields the §5.1 oracle: `R_parent`/absent for pre-commit prefixes, or `R_next` plus the recorded receipt for committed prefixes, with the table disposition |
| response already recorded | recovery provider call count does not increase |
| duplicate/late wake | one continuation/wake sequence, no duplicated action |
| bounded scheduler queue full | enqueue returns without blocking the world worker; one durable pending/scheduler_backpressure record; provider call, receipt, WorldReceiptLinked, event/effect/debit counts remain zero; releasing capacity produces exactly one recovery wake and terminal disposition |
| scheduler fairness/starvation | multi-Agent same-tick and long-backpressure inputs follow the sole comparator `(deadline_due, next_wake_tick, effective_priority, starvation_deadline_tick, cursor_distance, canonical(agent_id), canonical(continuation_id), wake_seq)` with its declared directions; per-agent/global service budgets, durable round-robin/aging and no-starvation bounds hold |
| WakeConditionV1 | every kind, missing event/receipt, false predicate, unknown field, expiry and GC'd reference is deterministic, bounded and replayable with the recorded evaluation head/hash |
| commit interleavings | crash at prepare, commit marker, world state/event, idempotency record, receipt and CognitionJournal projection boundaries either completes from WorldCommitRecordV1 or enters recovery_pending; no provider/kernel/effect re-execution |
| checkpoint/GC horizon | active/pending/retry/commit/reference pins prevent deletion; terminal key tombstones and response artifacts survive the horizon; expired duplicate keys fail closed |
| reorg/finality invalidation | old branch/epoch pending turns, responses and continuations become reorg_invalidated without receipt/effect; committed receipt is projected once and never re-executed |
| legacy restore | old queued/in-flight action becomes legacy_no_cognition_proof and is not auto-submitted or given synthetic MVCC freshness |
| replay | no provider/network call; same event sequence, receipt digest and state root |
| Builtin/provider parity | backend differences stay in Agent-owned artifacts; host rejection/acceptance semantics match |
| retention/size overflow | bounded structured failure, never silent truncation of canonical records |

The implementation review must record immutable fixture input, schema/version, request/envelope digests, journal head, provider call count, recovery disposition, event/receipt/state roots and compatibility adapter status. Until those artifacts exist, this design remains target/partial rather than proven.

Executable targets are fixed as follows: docs use `./scripts/doc-governance-check.sh`; P0.1 uses
`env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_identity`; P0.2 uses
`env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_mvcc`; P0.3 uses
`env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_recovery`; integration uses
`env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_scheduler_backpressure` for the
bounded queue-full fixture, `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_scheduler_fairness`
for service budgets/starvation, `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_wake_conditions`
for condition evaluation, `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_commit_interleavings`
for atomic crash recovery, and `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_retention_gc`
for checkpoint/GC safety. Integration also uses
`./scripts/ci-tests.sh required`; multi-Agent/long-run/restore/parity proof uses
`./scripts/ci-tests.sh full`. These focused filters are required implementation deliverables. Until
they exist and emit immutable inputs, digests, journal/provider-call counts, dispositions, receipt
and state roots, the corresponding capability remains target rather than proven.

### 11.1 验证映射表

The method below is a persistent validation plan with source-inspection evidence for this documentary correction. Fixture references identify existing bounded tests, not executed results or substitutes for the original §9 deliverables.

| 上游条款 | 本设计承接 | 接受边界 / environment | 验证方法与精确入口 | 证据目标与当前结果 | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [professional_acceptance: PRD §1](agent-cognition-lifecycle.prd.md#1-目标与边界) | [Boundary map](agent-cognition-lifecycle.design.md#1-boundary-map) | Pinned source 9410adcb443e09d4b56ff559d3a48cec402441b9, native runtime-live Builtin/ProviderBacked; synchronous simulator compatibility remains separately applicable, not native target proof. | Inspect llm_sidecar_runner.rs:211–229 and async_agent_runner.rs:258–277,360–383,792–825 against runner.rs:470,573–607. Locate [actor fixture](../../../crates/oasis7/src/simulator/tests/agent_cognition_live_actor.rs), world_tick_does_not_wait_for_an_outstanding_provider:60: blocking provider plus logical counter progress; distinguish that oracle from the actual kernel-step adapter and waiting run_one_turn helper. | Issue #4116 source-author evidence records immutable source/diff hashes, locators, preservation and governance output. Wiring and fixture assertion read; fixture not executed in this documentation correction. | No arbitrary-fault bounded World commit, replay/provider prohibition, full durable scheduler or remote/browser proof from logical-counter/mock evidence.  Source-evidence applicability sub-obligation only: Product N/A; reason: locating and qualifying bounded source evidence does not independently establish a player result; scope: only native Builtin/ProviderBacked worker/poll versus synchronous simulator compatibility source qualification. Result obligations (not N/A): canonical World advancement while provider remains in flight, and successful durable Wait only after admission with scheduled/pending readback and wake/resume; retained PRD §1/§9 and body acceptance apply. Product-leaf alignment of those result obligations is not resolved by this package or by the two limited recovery dependencies. |
| [professional_acceptance: PRD §9](agent-cognition-lifecycle.prd.md#9-证据锚点与专业跟进) | [Boundary map](agent-cognition-lifecycle.design.md#1-boundary-map) | Same pinned 941 native lane; ordinary Wait admission/current-context readback is distinct from legacy execute-until and WaitTicks timer. | Inspect llm_sidecar_async.rs:490–529, llm_sidecar_cognition_wait.rs:419–592 and llm_sidecar_cognition.rs:339–442,757. Locate [native Wait fixture](../../../crates/oasis7/src/viewer/runtime_live/tests/auth_actions_provider_context.rs), runtime_builtin_wait_enters_the_shared_harness_lifecycle:786: mock HTTP and injected binding/capability fixture assert shared admission; inspect admission failure branch separately rather than treating a Wait decision as success. | Issue #4116 evidence records pinned source and fixture hashes, ordinary Wait versus timer locators and unexecuted qualification. Current seam/source inspected; mock fixture not executed and not an independently verified finality certificate. | No actual real-provider paired artifacts, complete crash/snapshot-tail recovery, remote restart/reconnect/rebind/reorg, full exactly-once or browser service proof.  Source-evidence applicability sub-obligation only: Product N/A; reason: locating and qualifying bounded source evidence does not independently establish a player result; scope: only ordinary Wait admission/readback/wake qualification versus WaitTicks local timer. Result obligations (not N/A): canonical World advancement while provider remains in flight, and successful durable Wait only after admission with scheduled/pending readback and wake/resume; retained PRD §1/§9 and body acceptance apply. Product-leaf alignment of those result obligations is not resolved by this package or by the two limited recovery dependencies. |
| Trace refs: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；[product_requirement: REQ-EXT-008](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-008) / AC-EXT-008 | [Journal/recovery](#5-journal-layout-and-recovery) | Existing professional PRD §4.2 recovery receiver → design §5.1–5.3, with §7 integration and §9 verification: after ResponseRecorded, replay the recorded response without provider redispatch; reconcile unknown submission against the original marker/key/receipt, repair committed projections only, never repeat canonical world effects or revive old branch/authority. Typed in-memory fixture seams; not process-kill or external durable-store fault injection. | Inspect [recovery fixtures](../../../crates/oasis7_runtime/src/tests/agent_cognition_recovery.rs), especially durable_response_replay_does_not_invoke_provider_again and duplicate_replay_is_projection_idempotent_and_has_one_effect_receipt_event; inspect agent_cognition_commit_interleavings.rs, committed_missing_receipt_key_or_link_repairs_projection_only and marker_root_key_or_receipt_conflict_exposes_parent_and_quarantines_candidate. Keep every original §9 focused/required/full target. | Source assertions located at 9b6a7674458cd0b7a61fcca2fabbe1a5377ab737; not executed. Provider probe unchanged, duplicate root/receipt/disposition/event/effect/debit unchanged; missing projection repairs=3 with zero new effect; conflicts quarantine with retry=0. | Limited Runtime dependency only: excludes private-context-loss notice/trusted replanning UI, real remote restart, complete production history and full EXT-008 acceptance. EXT remains partial design/implementation. No all-platform, real-provider/finality, actual disk crash or production certification. |
| Trace refs: O7-REQ-6044cb04-5fbb-4c68-81a6-fc57cfdd608a；[product_requirement: REQ-COLLAB-006](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#req-collab-006) / AC-COLLAB-006 | [Journal/recovery](#5-journal-layout-and-recovery) | Existing professional PRD §4.2 recovery receiver → design §5.1–5.3, with §7 integration and §9 verification: after ResponseRecorded, replay the recorded response without provider redispatch; reconcile unknown submission against the original marker/key/receipt, repair committed projections only, never repeat canonical world effects or revive old branch/authority. Typed in-memory fixture seams; not process-kill or external durable-store fault injection. | Inspect [recovery fixtures](../../../crates/oasis7_runtime/src/tests/agent_cognition_recovery.rs), especially durable_response_replay_does_not_invoke_provider_again and duplicate_replay_is_projection_idempotent_and_has_one_effect_receipt_event; inspect agent_cognition_commit_interleavings.rs, committed_missing_receipt_key_or_link_repairs_projection_only and marker_root_key_or_receipt_conflict_exposes_parent_and_quarantines_candidate. Keep every original §9 focused/required/full target. | Source assertions located at 9b6a7674458cd0b7a61fcca2fabbe1a5377ab737; not executed. Provider probe unchanged, duplicate root/receipt/disposition/event/effect/debit unchanged; missing projection repairs=3 with zero new effect; conflicts quarantine with retry=0. | Limited Runtime dependency only: excludes collaboration message/snapshot pagination, unanswered questions, pending approvals, cross-device redelivery and complete COLLAB-006 acceptance. Existing design-covered/partially-implemented product state is unchanged. No all-platform, real-provider/finality, actual disk crash or production certification. |

Source-inspection qualification (9b6a7674458cd0b7a61fcca2fabbe1a5377ab737): the 9410ad source-author baseline and its historical line references above are retained as provenance, not relabeled as current. In [AsyncAgentRunner](../../../crates/oasis7/src/simulator/async_agent_runner.rs), `step_world_without_waiting_for_provider` at 856 increments a logical counter and polls; `step_kernel_without_waiting_for_provider` at 870 calls kernel.step; `run_one_turn` at 889 intentionally waits. The actor fixture's logical_tick+1/in-flight assertions do not establish bounded canonical World commit under arbitrary faults. The native Wait fixture at 786–954 uses mock HTTP and injected finality/capability, asserts scheduled/pending continuation then wake/resume, two requests and lifecycle journal correlation; it is not real-provider, independent finality or cross-process recovery evidence.

The companion [commit interleavings source](../../../crates/oasis7_runtime/src/tests/agent_cognition_commit_interleavings.rs) supplies the additional marker/projection assertions named in the two recovery rows. The recovery source also defines legacy snapshot rejection with `legacy_no_cognition_proof`, `auto_submitted=false` and no receipt. Commit fixtures distinguish before/prepared `R_parent` with no receipt and zero provider/kernel/effect/debit from committed `R_next` and the exact recorded receipt. The aborted fixture's internal report disposition `aborted` does not replace the §7 Feedback mapping `rejected` plus recorded abort_reason. These are bounded assertion definitions, not executed results; historical source comments saying “RED fixtures” neither establish current failure nor does source presence establish passing acceptance.

The §5 sequences use both `TurnCompleted` and `CognitionTurnCompleted`; this naming/adapter ambiguity remains for professional review. This documentary correction does not rename either event, change a wire contract or resolve that ambiguity by inference. The #4744 non-wasm cfg hunk on RuntimeContinuationAuthority is a platform-specific wiring change, not evidence of native/wasm/remote parity or a reason to suppress the preserved lane correction. P0.1–P0.3, P1/P2 and all original §9 obligations remain target/partial until their own required artifacts are obtained.
