# oasis7 Runtime：Agent cognition lifecycle PRD

- PRD-ID: PRD-WORLD_RUNTIME-047
- 对应任务：issue #3602 / task_2dbf6a6bdb274a54b7f27ae742c0113e
- 配套设计：agent-cognition-lifecycle.design.md
- 上位权威：doc/world-runtime/prd.md

> 专业域 authority：本文件拥有 Agent cognition 从 runtime wake、不可阻塞的外部决策、版本化 action envelope、提交校验、receipt 关联到恢复与 replay 的运行时合同。它不拥有 Agent 的 prompting、memory、goal、人格、模型选择或策略语义；这些语义必须留在 Agent 专业文档。它也不替代 AgentIntentV2 的 authority 语义、WASM ABI 或产品规则。

审计轮次: 1

## 1. 目标与边界

当前 runtime 可以执行确定性 world tick、持久化 world journal、snapshot/replay，AgentIntentV2 也已有 authority、request digest 与幂等 transition 基础。同步 simulator/兼容 runner 仍可能在 tick 调用栈内等待 provider；native runtime-live 的 Builtin 与 ProviderBacked 则都接入 AsyncAgentRunner，由 worker 调用决策并非阻塞地轮询结果。普通 ActionEnvelope 仍没有把决策绑定到产生它的 world 版本；native 接线也不证明完整 durable scheduler、远端恢复或全量非阻塞验收。本 PRD 把两条时间线明确分开：

1. world timeline 只由 runtime scheduler 与 deterministic execution pipeline 推进；
2. cognition timeline 可以等待、重试或恢复外部 provider，但绝不能阻塞或直接写 canonical world。

### 能力状态

| 能力面 | current | partial | target | proven 判据 |
| --- | --- | --- | --- | --- |
| World tick 与 cognition | simulator/兼容 AgentRunner::tick、tick_decide_only 和 ProviderBackedAgentBehavior::decide 保留同步调用；native runtime-live 的 Builtin/ProviderBacked 共用 AsyncAgentRunner worker 与非阻塞 mailbox/result polling，kernel action application 仍同步且权威。 | 已有 native actor 与 shadow-kernel adapter 接线；完整 scheduler、envelope、paired recovery/remote 生命周期及全部阻塞故障验收尚未由这些接线证明。runner logical-tick fixture 不等于实际 World tick/commit 证明；run_one_turn 是刻意等待的 tests/CLI helper。 | World::step 不等待 provider；AgentActor 在 runtime worker 外运行；结果通过 envelope 回到提交边界。 | 注入任意 provider 延迟或永久阻塞时，world tick 仍按 bounded deterministic work 完成，且 replay 不调用 provider。 |
| Action identity 与 MVCC | runtime ActionEnvelope 只有 action id 与 action；规则校验针对当前状态。 | AgentIntentV2 具备 intent_tick、world、authority_scope、request_digest，但它不是普通 cognition action 的 MVCC envelope。 | 每个 cognition result 都有 turn/request identity、base state、capability digest、有效期与 preconditions；过期或冲突结果 fail closed。 | 相同 fixture 下 fresh/stale/duplicate/conflict 四种提交产生稳定 disposition、事件与 state root。 |
| Cognition journal | WorldJournal、snapshot 与 runtime journal 能恢复 world state/event；decision trace 是诊断数据。 | DecisionRequest 有可选 replay_id，sidecar mailbox、runner、pending action 和 provider cache 仍是进程内状态。 | 独立、append-only、可 checkpoint 的 CognitionJournal 记录 turn lifecycle、canonical request/response、submit/receipt 与 recovery status。 | crash injection 覆盖每个边界；恢复后的 state/event/receipt digest 与无 crash 执行相同，已记录 response 不再次调用 provider。 |
| Continuation | legacy execute-until continuation 在 LlmAgentBehavior 内存中执行；WaitTicks 保留本地 timer。native 普通 Wait 经 Harness proposal/current-context 校验、Runtime admission 与 projection/readback，选取 Runtime wake 后以当前 context resume 并与 Harness reconcile；admission failure 不冒充成功等待。 | 已有 native 普通 Wait 的 admission/wake/resume seam 及有界 fixtures；它不证明完整 durable scheduler、WASM/browser 服务、远端 restart/reconnect/rebind/reorg 或全量 exactly-once 恢复，且不能与 WaitTicks timer 合并为同一持久化证明。 | continuation 由 AgentScheduler 持久化、按 logical tick/event/receipt 唤醒并以 wake identity 去重。 | snapshot/restart/乱序 wake 只产生一次等价 continuation outcome。 |
| Authority/capability | 仅在既有显式 DecisionRequest capability catalog/context、ProviderBacked capability-context 与 AgentIntentV2 paths 生成/校验；并非每个 turn 已自动 wiring。 | provider chat 已有 intent identity，普通 decision path 仍缺完整 turn identity。 | scheduler 为每个 turn 自动捕获并注入 capability snapshot；host 在 submit 时重新验证 authority、capability、policy 和 action 规则。 | provider 伪造或过期 capability、intent、actor、world、reorg 字段均被拒绝且不产生 world effect。 |

本文件新增的是 target contract；除上述 current/proven 局部能力外，不把 target 描述成已实现。

## 2. 非功能目标与非目标

### 目标

- **World never waits for provider**：World::step、commit、event apply 和 receipt publication 不得调用 HTTP/LLM/provider，不得等待外部响应或阻塞 channel receive。
- **可重放**：provider 是外部效应；replay 使用已持久化的 canonical response/decision，不再次调用 provider。
- **fail closed**：world 版本、authority、capability、有效期或 precondition 不匹配时，结果只能形成结构化拒绝，不能自动“更新到当前状态后继续执行”。
- **幂等**：provider invocation 可以 at-least-once；canonical world effect、accepted receipt 和同一 cognition request 的 terminal disposition 必须 exactly-once。
- **可恢复**：进程 crash、网络重试、重复 wake、乱序 receipt 和 snapshot restore 不得重复 debit、effect、event 或继续执行已完成 turn。
- **有界**：journal、prompt/trace artifact 引用、pending queue、continuation 与 retry metadata 均有配额和 retention；provider 的原始 prompting 内容不是 runtime 必存字段。

### 非目标

- 不定义 provider prompt、tool selection、model routing、memory write semantics、goal state machine、人格或产品策略。
- 不把 AgentIntentV2 改造成 cognition turn；不改变 AgentIntentV2 既有 authority、reorg、request digest 和 player-facing intent 语义。
- 不定义 WASM wire ABI、consensus/finality、跨 shard timeline 或产品规则。
- 不承诺网络层真正 exactly-once；runtime 只在 canonical commit boundary 提供 exactly-once effect。

## 3. Runtime contract

### 3.1 Async AgentActor 与 scheduler 边界

目标架构由三个 ownership 组成：

| 组件 | ownership | 明确禁止 |
| --- | --- | --- |
| World execution worker | 逻辑时间、state transition、authority/capability revalidation、ActionEnvelope 提交、event/receipt、snapshot/replay | provider I/O、等待 actor、使用 wall clock 改变 world、接受未绑定版本的 action |
| AgentScheduler | 按 committed world state 生成 wake、接收并验证 Harness-issued turn/request identity、持久化 in-flight status、按 agent 确定顺序投递与去重、管理 durable schedule/wake/recovery | 直接授予 capability、绕过 kernel 执行 action、把未提交的推理结果写进 world，或替 Harness 决定 continuation policy |
| AgentActor/provider adapter/Harness | 在 worker 外构造 Agent-owned context，执行 builtin/provider strategy，提出 continuation policy/proposal，返回 canonical response/envelope | 直接写 canonical world、声明新的 authority/capability、把 prompt/memory/goal 或 continuation policy 伪装成 runtime rule |

一次 wake 的目标顺序是：

1. committed world state 产生 bounded wake reason；
2. scheduler 接收并验证 Harness-issued session/turn/request identity，固定 base tick、base world hash 和 capability snapshot，并 append TurnStarted/ContextCaptured；
3. actor/provider 异步执行；world 继续自己的 tick；
4. response 先作为 durable cognition record 落盘，再转换为 AgentDecisionEnvelope；
5. envelope 进入同一 runtime action validation/commit pipeline；
6. accepted/rejected disposition 与 world receipt 关联，scheduler 依 receipt 或 failure event 推进 turn/continuation。

同一 agent 默认至多一个 active turn；如果未来允许并发，必须为每个 turn 独立 mailbox、identity、base snapshot、budget 与 continuation，不得依靠全局 feedback 或共享可变 context 串联。

Scheduler enqueue 必须 non-blocking；bounded queue 满时只产生 durable pending/deferred disposition（例如 scheduler_backpressure），由后续 wake/recovery 重试，不得阻塞 world worker 或同步调用 provider。ready set 只使用 §3.1 固定的唯一 total comparator；每个 logical tick 受 `max_total_wakes_per_tick` 与 `max_wakes_per_agent_per_tick` 限制，agent 之间按 stable round-robin cursor 服务，pending 项可按 bounded aging 提升但不得越过 validity/reorg 检查。queue full 必须保留 wake identity 与下次 retry 序号；释放容量后的 retry 仍按该 comparator 且不得造成 starvation。

公平性 v1 fixture 配置固定为：`max_total_wakes_per_tick=8`、
`max_wakes_per_agent_per_tick=1`、`aging_after_ticks=2`、`max_starvation_ticks=4`，并固定
上述唯一 comparator 包含 cursor distance 与全部 canonical tie-break。eligible pending 项等待达到 `aging_after_ticks` 时每个 logical tick
至多提升一个 priority level；它仍不能越过 validity、reorg 或 cancellation 检查。任何 eligible
项从进入 ready set 起不得超过 `max_starvation_ticks` 个 logical tick 未被尝试服务；到达 ceiling
时必须在同一 durable scheduler transaction 中记录 `pending/scheduler_backpressure` 并暂时移出
ready set（不得静默丢弃），容量恢复后以原 wake identity 和 aging 重新加入。`scheduler_cursor`
使用以下 durable shape，并和 wake selection 一起原子推进：

~~~text
SchedulerCursorV1 {
  schema_version, logical_tick, last_served_agent_id?, cursor_seq, policy_config_digest,
}
~~~

`policy_config_digest = H_v1("oasis7.runtime.scheduler-policy.v1", canonical({
max_total_wakes_per_tick, max_wakes_per_agent_per_tick, aging_after_ticks, max_starvation_ticks,
initial_priority, comparator, service_order }))`; fixture values and field order are part of the digest, so a changed fairness policy
cannot silently replay an old cursor.

空 cursor 以 canonical 最小 agent id 起步；scheduler 在 append wake/turn record 前将新 cursor
与该 record 一起 durable commit，actor delivery 发生在 commit 后。恢复沿用该 cursor，缺失、非法
或与 journal 不一致时进入 `recovery_pending`，不得从头重排或调用 provider。

公平性的唯一 total comparator 为：
`(deadline_due DESC, next_wake_tick ASC, effective_priority DESC, starvation_deadline_tick ASC,
cursor_distance ASC, canonical(agent_id) ASC, canonical(continuation_id) ASC, wake_seq ASC)`。
其中 `deadline_due = current_tick >= starvation_deadline_tick`；它是第一排序键，故已到
starvation deadline 的 eligible item 绝对优先于所有未到 deadline 的普通 priority item。
Runtime 为每个 accepted schedule 分配 `initial_priority=0` 与 `eligible_since_tick`；
`effective_priority = min(7, initial_priority + floor((current_tick - eligible_since_tick) /
aging_after_ticks))`，不使用 provider/Harness 未声明的优先级。`starvation_deadline_tick =
eligible_since_tick + max_starvation_ticks`；在 deadline tick 仍 eligible 的项目必须先被尝试，
不能被普通 priority 越过。`cursor_distance` 是从持久化 `last_served_agent_id` 后一个
canonical ready-agent 的环形距离；空 cursor 距离从 canonical 最小 agent 开始计算。每次
service attempt（包括 queue-full ceiling 上的 backpressure disposition）都在同一 scheduler
transaction 中推进 cursor；backpressure_count、原始 `eligible_since_tick` 与 wake identity
持久化，重试不能重置 aging 或伪造新的优先级。crash 若发生在该 transaction 前，cursor 不变；
若发生在 transaction 后、actor delivery 前，恢复沿用已提交 cursor 并以同一 wake identity
重投递，不能重复 effect。该 comparator、cursor 与 policy_config_digest 共同构成 restart oracle。

### 3.2 AgentDecisionEnvelope 与 Intent MVCC

目标 envelope 至少包含以下字段。字段名是逻辑合同，具体 Rust/serialization 类型由 runtime 实现和 ABI 专题共同收敛。

| 字段 | 语义 |
| --- | --- |
| schema_version | envelope contract 版本；未知版本拒绝 |
| world_id、agent_id | canonical world 与 subject identity |
| branch_id、finality_epoch、finality_block_hash、finality_status | trusted committed branch/finality binding；字段类型与状态转换按 `FinalityBindingV1` 验证，runtime 不自行宣称共识 finality |
| agent_session_id、agent_turn_id、decision_request_id、retry_seq | cognition lifecycle identity；使用 paired Harness 的 canonical wire names；retry 使用同一 logical request key，新的策略 turn 使用新 turn/request |
| base_tick、base_world_hash | 产生决策时观察到的 logical tick 与 canonical world/state digest |
| reorg_epoch、runtime_manifest_hash | 绑定当前 execution/reorg/runtime schema |
| capability_snapshot_hash、authority_context_hash | runtime 在既有显式 paths 或 target per-turn wiring 中产生的 capability/authority snapshot digest，不是 provider grant |
| observation_digest、context_digest | 输入观察与 Agent-owned context 的 canonical digest；不要求 runtime 保存原始 prompt |
| issued_at_tick、valid_until_tick | logical validity window；禁止以 wall clock 作为 world validity |
| preconditions | canonical 排序的可验证前置条件集合 |
| decision_kind、action | 结构化 decision/action；最终仍由 host/kernel 校验 |
| request_digest、decision_digest、envelope_digest | 分层 canonical digest |
| provider_invocation_key | 从 Agent-owned versioned DecisionRequest 的 request_digest 派生，供 provider invocation at-least-once 去重 |
| envelope_idempotency_key | runtime 从 request_digest、decision/action 与 runtime bindings 派生，供 canonical submit exactly-once 去重 |
| origin_intent_ref | 可选的 AgentIntentV2 引用：intent_id、request_digest、authority tuple |
| source | builtin、provider、replay 或 recovery；只用于审计和路由，不授予额外权限 |

MVCC 验证顺序固定为：schema/world/agent/branch/finality_epoch/finality_block_hash/finality_status/reorg identity → session/turn/request correlation → idempotency lookup → base tick/hash → validity window → capability/authority snapshot → origin intent → preconditions → current action/kernel rules → staged commit。相同 envelope_idempotency_key 且 digest 相同的已处理请求返回原 disposition/receipt；同 key 不同 digest 形成 idempotency_conflict；不得重新执行 world effect。

### 3.2.1 PreconditionV1 与 WorldHashV1

`preconditions` 使用唯一的 bounded `PreconditionV1` 形状。v1 不支持嵌套布尔表达式；列表中的每一项都必须成立，列表按 canonical item bytes 排序，重复项拒绝。

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

`path_or_rule` 必须指向 runtime 可读的、版本化的状态字段或 kernel rule input；`operator`、path/schema、expected bytes 或其 derived digest 的 canonical encoding 未知时 fail closed 为 `precondition_failed`。所有条件在同一 committed parent snapshot 上、staged commit 前按 canonical 顺序求值；不得读取 wall clock、provider output 或非持久化进程状态。产品/玩法语义仍由 kernel authority 定义，Runtime 只负责绑定、限额与可重放求值。

PreconditionV1 v1 registry 固定为：`world.logical_tick`（u64）、`world.state_root`
（digest）、`world.runtime_manifest_hash`（digest）、`world.reorg_epoch`（u64）、
`agent.status`（enum）、`agent.position`（canonical `[i64,i64]`）、
`agent.resource.<resource_id>`（i64，resource_id 为 NFC UTF-8 registry id）、
`agent.inventory_digest`（digest）、`agent.capability_snapshot_hash`（digest）和
`intent.status`（enum）。`subject.kind` 只允许 `world | agent | intent`，且必须与 path
namespace 相符：`world.*` 用 `world`/当前 `world_id`，`agent.*` 用 `agent`/当前 `agent_id`，
`intent.status` 用 `intent`/`origin_intent_ref.intent_id`；`subject.id` 必须是非空 NFC UTF-8
canonical identifier，最多 128 bytes。`agent.status` 的 enum value registry 固定为
`idle | executing | blocked | waiting | unavailable`；`intent.status` 的 registry 固定为
`proposed | submitted | accepted | blocked | completed | rejected | expired | cancelled |
superseded`。`operator` 仅允许 `eq | neq | lt | lte | gt | gte`；digest/enum/tuple
字段只允许 `eq|neq`，数值字段允许全部六种比较。expected value 必须是该 path type 的
RFC 8949 deterministic-CBOR canonical bytes，字段名为 `expected_value_bytes`；Runtime 可记录
其 BLAKE3 digest，但不得只凭未验证 digest 猜测值。每项 path 最多 128 bytes、expected bytes 最多 512 bytes、最多 32 项、
总 canonical bytes 最多 4096 bytes；unknown path/operator、type mismatch、malformed bytes、
duplicate 或超限统一 rejected 为 `precondition_failed`，不得执行 world effect。

`base_world_hash` 的 v1 来源固定为 canonical committed parent identity：

~~~text
world_hash = H_v1("oasis7.runtime.world-state.v1",
  canonical({ world_id, branch_id, finality_epoch, finality_block_hash, finality_status,
              logical_tick, state_root, reorg_epoch, runtime_manifest_hash }))
~~~

生产 MVCC 必须提供 versioned `state_root`；不得以任意内存序列化、指针地址或 hash-map iteration 作为 fallback。`branch_id`、`finality_epoch`、`finality_block_hash` 与 `finality_status` 是 trusted upstream binding，Runtime 只验证其与当前 committed parent 一致；缺失或无法确认时进入 `recovery_pending`，不能以本地值补齐。

### 3.3 Canonical digests

request_digest 由 Agent-owned、带版本的 DecisionRequest canonical input contract 产生。Runtime 必须验证它与该版本 request/artifact 的绑定关系，并将其作为 opaque verified binding 传递；Runtime 不得按自己的字段集合重新计算或重定义 request_digest。Agent 专业文档拥有 prompt/context/memory/goal 如何进入 DecisionRequest 的语义。

Runtime 对 Harness `runtime_binding.branch_id/finality_epoch/finality_block_hash/
finality_status/reorg_epoch` 使用同一 canonical `FinalityBindingV1` 形状（不新增 wire alias）：

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

`finality_block_hash` 是 canonical 32-byte block digest；`finality_status=verified` 时必填，
其他状态可缺席但若出现仍必须通过 digest 校验。`finality_epoch` 与 `reorg_epoch` 为 u64，
`branch_id` 为 bounded canonical identifier；unknown status、负数、错误 hash 长度或 status/hash
组合不合法时进入 `recovery_pending`。这些字段来自 trusted upstream；Runtime 只比较并持久化，
不生成共识证明。

在 request_digest 已验证的前提下，runtime 只定义后续绑定：

- provider_invocation_key = H_v1("oasis7.cognition.provider-invocation.v1", request_digest)
- decision_digest = H_v1("oasis7.cognition.decision.v1", canonical({request_digest、decision_kind、canonical action}))
- envelope_digest = H_v1("oasis7.cognition.envelope.v1", canonical({request_digest、decision_digest、canonical action、world_id、agent_id、agent_session_id、agent_turn_id、decision_request_id、retry_seq、branch_id、finality_epoch、finality_block_hash、finality_status、finality_binding_digest、base_tick、base_world_hash、reorg_epoch、runtime_manifest_hash、capability_snapshot_hash、authority_context_hash、issued_at_tick、valid_until_tick、preconditions、origin_intent_ref}))
- envelope_idempotency_key = H_v1("oasis7.cognition.envelope-idempotency.v1", canonical({request_digest、envelope_digest}))

所有上述编码使用已版本化 domain separator、RFC 8949 deterministic-CBOR、明确字段顺序和长度前缀。prompt transcript、provider latency、token count、diagnostic trace 和 memory/goal prose 不直接进入 runtime envelope digest；若需要审计，保存为 bounded content-addressed artifact，并只在 journal 中引用 digest。

共享 hash registry 的 v1 entry 固定为：`blake3-256`；输入使用 RFC 8949
deterministic CBOR 的 `[domain, payload]` 二元数组；输出编码为
`blake3:<64 lowercase hex>`。算法自检向量为
`BLAKE3-256(empty) = af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262`。
P0.1 的完整 request/envelope golden fixture 必须由 Harness 与 Runtime 对同一 canonical
bytes、digest 和 key 逐字节比对；缺少该 fixture 时只能是 `target`，不能是 `proven`。

### 3.3.1 Cross-contract identity mapping

| Harness wire field | Runtime wire field / binding | Rule |
| --- | --- | --- |
| `agent_session_id` | `agent_session_id` | 必须原样保留；不得由 runtime session/local process id 替代 |
| `agent_turn_id` | `agent_turn_id` | canonical turn identity；实现局部变量不得把别名写上 wire |
| `decision_request_id` | `decision_request_id` | canonical logical request identity；transport retry 不变 |
| `agent_subject` | `agent_id` | 必须精确相等；不相等为 `authority_denied` |
| `runtime_binding.world_id` | `world_id` | 必须精确相等 |
| `runtime_binding.branch_id/finality_epoch/finality_block_hash/finality_status` | typed committed branch/finality identity | 必须由 trusted host 验证并参与 request artifact binding |
| `runtime_binding.base_tick/base_world_hash` | `base_tick/base_world_hash` | MVCC parent binding |
| `runtime_binding.reorg_epoch/runtime_manifest_hash` | 同名 runtime fields | execution epoch/schema binding |
| `capability_catalog_digest` | `capability_snapshot_hash` | runtime-produced catalog snapshot binding；不得由 provider 自报 |
| `capability_invocation_context_digest` | `authority_context_hash` | subject/authority invocation binding；submit 时重新验证 |
| `memory_snapshot_digest/goal_snapshot_digest/continuation_digest/adapter_protocol_version/budget_contract` | verified request artifact；聚合为 `context_digest` 引用 | Agent-owned semantics 保留在 request artifact，Runtime 只验证 digest binding，不复制 prose |
| Runtime `issued_at_tick/valid_until_tick` | runtime-derived envelope fields | scheduler 在 capture/submit contract 中生成；不是 provider 可改字段 |
| Runtime `preconditions` | response/action-derived bounded fields | trusted host 规范化后进入 envelope；不属于 outer request identity alias |
| `transport_attempt` | journal `transport_attempt` metadata；`retry_seq` separately records semantic lineage | 每次 at-least-once provider delivery 递增；不进入 logical request identity、request_digest 或 provider key |

Runtime 不得由缺失字段猜测 alias；legacy DTO 只能走显式 compatibility lane，并记录
`legacy_no_cognition_proof`。

Legacy snapshot/journal readers 必须使用 versioned、read-only compatibility adapter。恢复时发现
旧的 queued/in-flight ActionEnvelope 只能形成 `rejected + reject_reason=legacy_no_cognition_proof`
（或显式 operator-owned pending compatibility record），不得重建为 fresh MVCC envelope、补造
base hash 或自动 submit。新执行必须创建新的 Harness session/turn/request；旧字段需保留至
configured retention window 和所有 reader 完成切换。Legacy adapter 只有在发现匹配的
`WorldCommitRecordV1(status=committed)` 与 canonical receipt 时才允许报告 success；旧的
success flag/summary 本身一律映射为 `legacy_no_cognition_proof`，不得伪造 committed。

### 3.3.2 Session/Turn mapping seam

Harness exclusively allocates and owns agent_session_id、agent_turn_id 和
decision_request_id。Runtime 不以 local process/session id 替换它们，只验证精确映射并持久化。

| Harness identity | Runtime binding | Runtime allocation |
| --- | --- | --- |
| agent_session_id / agent_turn_id / decision_request_id | 同名字段，必须与 request_digest、agent_id、world binding 一致 | none |
| transport_attempt | provider dispatch journal metadata | none |
| retry_seq | semantic request lineage 与 parent causal reference | none |
| turn journal event | session/turn/request correlation | journal_seq |
| submitted action | envelope identity | action_id |
| committed outcome | envelope identity + receipt binding | receipt_id |
| durable wake | continuation proposal/session/turn correlation | wake_id |

transport_attempt 是 at-least-once provider delivery attempt，每次实际 dispatch 递增；它不进入
logical request identity、request_digest 或 provider_invocation_key。retry_seq 是 semantic
request retry/new-request lineage；transport retry 不递增，semantic retry/new request 必须
保留旧 lineage reference。CognitionJournal 必须分别持久化 retry_seq 与 transport_attempt，
不得使用含义不明的 generic attempt 字段替代它们。

缺失或冲突的 session/turn/request mapping 在 first effect 前以
request_identity_collision 或 authority_denied fail closed。

### 3.4 Validity、precondition 与 stale rejection

以下拒绝必须结构化、可 replay、无 world effect：

| 条件 | disposition |
| --- | --- |
| base_tick/base_world_hash 与当前 committed parent 不匹配 | stale_base |
| current logical tick > valid_until_tick | expired |
| capability 或 authority snapshot digest 不匹配 | stale_capability_snapshot / authority_denied |
| branch/finality_epoch/finality_block_hash/finality_status/reorg binding 与当前 committed parent 不匹配或已失效 | reorg_invalidated |
| 任一 precondition 不满足 | precondition_failed |
| action 不符合当前 kernel/module policy、预算或资源状态 | action_rejected |
| intent ref 不存在、actor/agent/digest/authority tuple 不匹配 | intent_conflict |
| 同 key 不同 digest | idempotency_conflict |
| provider、persistence、schema 或 recovery 失败 | cognition_failed / recovery_pending |

重算 capability、重做 quote 或重新规划必须创建新的 request/turn，不能静默改写原 envelope。accepted action 的 receipt 必须记录 envelope_digest、request_digest、base identity 和 final state/receipt identity。

若 trusted `branch_id`、`finality_epoch`、`finality_block_hash`、`finality_status` 或 `reorg_epoch` 发生变化，所有尚未 canonical committed 的旧 turn、response、pending wake 和 continuation 都必须标记 `reorg_invalidated`，不得把旧 response 重新绑定到新 parent；Agent 只能在新 observation 上创建新的 session/turn/request。已 committed 的 receipt 由其原 commit record 保留，不能重复执行；任一 typed branch/finality 字段无法确认或组合不合法时统一进入 `recovery_pending`。

## 4. Durable cognition journal

### 4.1 Event sequence

CognitionJournal 是 append-only、带 sequence/parent digest 的 runtime-owned log；它可以和 world journal 分开存储，但必须在 checkpoint、receipt 和 replay manifest 中相互引用。只有 response-bearing turn 共享 `TurnStarted -> ContextCaptured -> RequestDispatched -> ResponseRecorded -> DecisionEnvelopeSubmitted` 前缀；pre-response failure/cancel/backpressure 走下述不含 response 事件的分支：

- committed：`DecisionValidated`（provisional）→ canonical commit finalized → `WorldReceiptLinked` → `CognitionTurnCompleted(status=committed)`；
- rejected：`DecisionRejected` → `CognitionTurnCompleted(status=rejected)`，不得有 receipt/link/world effect；
- failed：`CognitionTurnFailed(status=failed)` → `CognitionTurnCompleted(status=failed)`（若有显式新 retry，则旧 turn 先 terminal，新 turn 使用新的 `retry_seq`）；
- pending：`RecoveryDispositioned(status=pending)` 或 `ContinuationScheduled(status=pending)` 保持 durable pending，不得伪造完成或 receipt；后续只能转入上述 terminal 分支。

`WorldReceiptLinked` 只存在于 committed 分支。`ResponseRecorded` 与
`DecisionEnvelopeSubmitted` 只存在于 response-bearing branch；它们不得出现在 pre-response
failure、cancellation 或 scheduler-backpressure branch。Runtime 的 exact branch sequences 为：

| branch | exact durable event sequence | terminal/result rule |
| --- | --- | --- |
| response-bearing | `TurnStarted -> ContextCaptured -> RequestDispatched -> ResponseRecorded -> DecisionEnvelopeSubmitted -> DecisionValidated -> WorldReceiptLinked -> CognitionTurnCompleted(status=committed)` 或其 rejected/failed/pending suffix | response/action digest 必须先 durable；只允许按 normal validation/commit 收口 |
| provider/persistence failure before response | `TurnStarted -> ContextCaptured -> (RequestDispatched)? -> CognitionTurnFailed(status=failed) -> CognitionTurnCompleted(status=failed)` | 不追加 `ResponseRecorded`、envelope、receipt 或 effect；bounded retry 只能新 `retry_seq` |
| cancellation before response | `TurnStarted -> ContextCaptured -> (RequestDispatched)? -> CognitionTurnCancelled(status=rejected, reject_reason=cancelled) -> CognitionTurnCompleted(status=rejected)` | 不追加 `ResponseRecorded`、`WorldReceiptLinked` 或 effect |
| scheduler queue full before delivery | `TurnStarted -> ContextCaptured -> SchedulerBackpressure(status=pending) -> RecoveryDispositioned(status=pending)` | 不调用 provider；capacity wake 复用同一 identity，之后才可进入 response-bearing branch |
| cancellation after response but before commit | response-bearing prefix through `ResponseRecorded` (and optional `DecisionEnvelopeSubmitted`, `DecisionValidated`) -> `CognitionTurnCancelled(status=rejected, reject_reason=cancelled) -> CognitionTurnCompleted(status=rejected)` | 已 durable response 不得转为 effect；若 commit marker/receipt 已 committed，committed precedence 禁止 cancellation 覆盖 |
| late response after cancellation | `LateResponseRejected(status=rejected, reject_reason=late_response_after_cancel)` referencing the cancelled turn | 不追加 `ResponseRecorded`、不重开 turn、不追加 receipt/effect；若 response 已先 durable，则只记录 duplicate-late rejection |

任何未知或互相矛盾的 terminal path 都是 recovery fault，必须进入 `recovery_pending`，而不是猜测状态。

异常与辅助事件包括 CognitionRetryScheduled、CognitionTurnFailed、CognitionTurnCancelled、
LateResponseRejected、SchedulerBackpressure、ContinuationScheduled、ContinuationWoken 和
RecoveryDispositioned。每个事件至少携带 journal_seq、agent_session_id、agent_turn_id、
decision_request_id、request_digest、retry_seq、transport_attempt、envelope/response digest（适用时）、
status、runtime disposition/reject_reason（适用时）、logical tick 与 causal references；reason
使用 bounded versioned registry，不保存 provider free text。

原始 provider prompt/response、无限 transcript、memory payload 和 goal prose 不默认内嵌 journal；runtime 只保存完成 replay 所需的 canonical response/action 或 content-addressed artifact 引用，并受 retention/size policy 约束。

DecisionValidated 是 provisional：它只证明当前 envelope 通过 submit-time validation，不证明
world effect。提交必须创建 `WorldCommitRecordV1`，包含 envelope_idempotency_key、envelope_digest、
action_id、parent tick/hash、staged event/state roots、receipt_id/receipt_digest、reorg_epoch
和对应 cognition journal sequence。`prepared` 记录不产生 effect；只有同一 durable commit
boundary 将 world state/event journal、idempotency key+digest、canonical receipt 与 commit
record 标记为 `committed` 时才产生 effect。若底层存储不能跨表事务，则该 commit record/marker
必须是唯一 authoritative recovery anchor，并以幂等 projection 补齐各 journal/link。

`WorldCommitRecordV1` 的最小 canonical shape 为：

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

`abort_reason` 在 `status=aborted` 时必填且在其他 status 时必须缺席；它是 bounded、versioned
runtime disposition（例如 `stale_base`、`cancelled` 或 `recovery_operator_abort`），不是 provider
自由文本。`world_id`、`branch_id`、`finality_epoch`、`finality_block_hash`、`finality_status`、
`finality_binding_digest` 与 `runtime_manifest_hash` 必须和 envelope 及 `parent_world_hash` 精确
相等；marker 不得跨 world、branch、finality 或 runtime
manifest 重用；typed finality binding 不得被 opaque `finality` alias 替代。

Externally visible world state uses a separate trusted `WorldRootViewV1` derived only from the
world-store committed-head index:

~~~text
WorldRootViewV1 {
  schema_version, world_id, branch_id, logical_tick, state_root,
  head_status: canonical | recovery_pending, commit_id?, quarantine_id?,
}
~~~

The only public root is `state_root` when `head_status=canonical`; a staged root or a marker by
itself never becomes visible. On any marker/root/receipt/key conflict, the last verified canonical
head (`R_parent` in the fixture) remains the externally visible root, the conflicting candidate root
and receipt are placed under a durable `quarantine_id`, and the disposition is `recovery_pending`.
Quarantine records are inspectable by recovery but cannot produce Feedback committed, a receipt,
WorldReceiptLinked, provider retry or world effect. A valid committed marker with a missing
projection is not a conflict: it exposes `R_next` and reconstructs the recorded receipt.

WorldReceiptLinked 只能在 receipt durable 后追加；它是 projection，不是第二次 effect。CognitionJournal
与 world journal 分开存储时，任何一个投影缺失都不得靠 `DecisionValidated` 推断已提交。

提交 interleaving 的恢复合同固定为：

| crash prefix | recovery disposition | provider/effect rule |
| --- | --- | --- |
| 未写入 commit record | 重新校验同一 envelope key；可进入 stale/rejected | 不重调 provider；未有 commit 不得有 effect |
| `prepared` record，无 committed marker | 保持 `recovery_pending`；只有合法 durable `status=aborted` marker 才收口为 rejected | 不猜测、不执行 kernel、不产生 receipt/effect |
| committed marker/world state 已写，receipt 或 idempotency projection 缺失 | 从 `WorldCommitRecordV1` 补齐 receipt/key/projection | 不重新执行 effect |
| receipt/key/world commit 已齐，`WorldReceiptLinked` 或 TurnCompleted 缺失 | 补写 link/terminal projection | 不重新执行 effect |
| roots、key、digest 或 marker 相互冲突 | `recovery_pending` | 禁止 provider、world effect 与自动重试 |

唯一 write order 为：

1. durable append `DecisionValidated`；
2. durable write `WorldCommitRecordV1(status=prepared)`，此时 canonical world root 仍为
   `R_parent`，没有 receipt、idempotency terminal disposition 或 effect；
3. 在一个 world-store transaction 中按固定顺序写 staged world event/state（产生
   `R_next`）、idempotency key+digest、canonical receipt `(receipt_id, receipt_digest)`，并将同一
   record 从 `prepared` 标记为 `committed`；该 transaction 的 commit point 是唯一 effect
   linearization point；
4. transaction 成功后 append `WorldReceiptLinked`；
5. append `CognitionTurnCompleted`。

每个 crash fixture 的 oracle 必须绑定到明确的 parent/next root、receipt 与 disposition：

| crash prefix | final world root | receipt | Feedback/disposition after recovery |
| --- | --- | --- | --- |
| before prepared | `R_parent` | absent | `recovery_pending`，随后在 parent 未变时只允许一次 revalidate；成功为 `committed/R_next/receipt_id`，失败为稳定 `rejected/R_parent/absent` |
| prepared only | `R_parent` | absent | `recovery_pending`；只有合法 durable `status=aborted` marker 才变为 `rejected`，仍为 `R_parent`/absent |
| committed transaction | `R_next` | exact recorded `(receipt_id, receipt_digest)` | `committed`，恰有一个 receipt/effect |
| committed transaction but link/completed missing | `R_next` | exact recorded receipt | `committed`；只补 link/completed projection |
| marker/root/receipt/key conflict | 外部 canonical root 固定为最后 verified head=`R_parent`；冲突 candidate root/receipt 置于 durable quarantine | absent（冲突 candidate 仅在 quarantine） | `recovery_pending`；禁止 provider/kernel/effect/retry；不得将 marker 单独发布为 root |

### 4.2 Recovery rules

| 最后 durable 状态 | 恢复动作 |
| --- | --- |
| TurnStarted/ContextCaptured，无 Dispatch | 重建同一 request identity 后 dispatch；不创建新 turn |
| RequestDispatched，无 ResponseRecorded | 以同一 provider_invocation_key at-least-once 重试；不得让 world 等待 |
| ResponseRecorded，无 EnvelopeSubmitted | 直接从已记录 response 构造 envelope；不得再次调用 provider |
| EnvelopeSubmitted，无 Decision disposition | 重新执行 deterministic validation 或读取已有 submit disposition |
| DecisionValidated，无 WorldReceiptLinked 且无 committed commit marker | crash 在 canonical commit 前；以同一 envelope_idempotency_key 重新验证/提交，或进入 stale/rejected；不得重复 provider 或假设已 committed |
| `prepared` commit record，无 committed marker | 只有 durable abort marker 才能 abort，否则 recovery_pending；不得猜测或执行 |
| committed marker/world state 已写但 receipt 或 idempotency projection 缺失 | 从 `WorldCommitRecordV1` 补齐 receipt/key/link；不得重复 effect |
| DecisionValidated，无 WorldReceiptLinked 但已有 canonical receipt/committed marker | crash 在 world commit 后；读取该 receipt 并补写 WorldReceiptLinked；不得重复 effect |
| WorldReceiptLinked，无 TurnCompleted | 补写 completed projection，不改变 world |
| terminal failure/cancel | 只按显式 retry policy 建立新 retry_seq/turn；旧 terminal disposition 不覆盖 |

snapshot 必须绑定 cognition journal head、checkpoint digest、WorldCommitRecordV1/receipt registry head、每个 active turn 的 status/retry_seq/transport_attempt 和 continuation scheduler state。恢复时先验证 journal prefix、request/envelope/response digest、commit marker、envelope idempotency record 与 canonical receipt，再恢复 scheduler；任何缺失或冲突都进入 recovery_pending，禁止猜测或自动执行。

Checkpoint/GC 只能删除 checkpoint 之前、无 active turn、pending wake、retry lineage、未完成 commit 或可接受迟到 response 的 journal prefix。所有 terminal disposition（committed、rejected、failed、cancelled 及其稳定 reject/failure reason）的 envelope key+digest、receipt linkage 与完成 replay 所需 response artifact 必须保留为 tombstone/reference，直到 `retention_horizon` 到期；该 horizon 至少覆盖 provider retry、queue/wake lease、validity/late-response、snapshot restore 与 trusted reorg/finality window 的最大值并加配置安全余量。v1 envelope 的 required identity/digest 完整但其 `base_tick`/`issued_at_tick` 已低于 `gc_floor_tick` 或其 tombstone 已过 retention 时，稳定返回 `expired_idempotency`；缺失 v1 schema、session/turn/request/envelope proof 的 legacy DTO/snapshot 才返回 `legacy_no_cognition_proof`。两者都不得重新执行；artifact 只有在所有 checkpoint/replay/reference pin 消失后才能 GC。

### 4.3 Exactly-once 与 at-least-once

| 边界 | 保证 |
| --- | --- |
| provider/network invocation | at-least-once；调用方必须传递稳定 provider_invocation_key，不能承诺远端 exactly-once |
| actor wake、queue delivery、receipt notification | at-least-once；由 turn/request/wake identity 去重 |
| cognition journal append | 每个逻辑事件一个唯一 sequence；重试不得覆盖不同 digest |
| runtime action validation/commit | 对同一 envelope_idempotency_key exactly-once；重复提交返回既有 disposition |
| world event、debit、effect、receipt | canonical commit boundary exactly-once |
| replay | 只读取已记录 response/receipt，不调用 provider、不重执行外部业务 effect |

## 5. Continuation persistence

Continuation 是 runtime-owned durable schedule record，不是 AgentBehavior 私有内存的隐式控制流。Harness/Agent 拥有 continuation policy 与 proposal semantics；Runtime 只验证 proposal、持久化 schedule、分配 wake_id、执行 wake、恢复和去重。Runtime projection `AgentContinuation` 至少包括：

~~~text
AgentContinuation {
  continuation_id, wake_id,
  world_id, branch_id, finality_epoch, finality_block_hash?, finality_status,
  reorg_epoch, runtime_manifest_hash,
  agent_id, agent_session_id, agent_turn_id, decision_request_id,
  origin_turn_id, origin_request_digest,
  continuation_proposal_id, proposal_digest,
  action_or_envelope_digest?,
  wake_conditions: WakeConditionV1[], next_wake_tick?,
  remaining_budget: { unit: steps | ticks, value: u64 },
  valid_until_tick?, precondition_digest, wake_seq,
  status: ContinuationStatusV1, terminal_disposition?
}
~~~

`AgentContinuation.status` 使用唯一的 `ContinuationStatusV1` enum：
`scheduled | pending | waking | consumed | completed | cancelled | invalidated | expired | rejected`。
允许迁移固定为 `scheduled -> pending|waking|cancelled|invalidated|expired|rejected`、
`pending -> waking|cancelled|invalidated|expired|rejected`、
`waking -> consumed|cancelled|invalidated|expired|rejected`、
`consumed -> scheduled|completed|cancelled|invalidated|expired|rejected`；
`completed/cancelled/invalidated/expired/rejected` 是 terminal，未知迁移进入
`recovery_pending`，不得隐式重试。`terminal_disposition` 只在 terminal status 出现，
`pending` 必须有 durable scheduler/recovery reason。

`continuation_status_digest = H_v1("oasis7.cognition.continuation-status.v1", canonical({
continuation_id, wake_id, wake_seq, from_status?, to_status, logical_tick, world_id, branch_id,
finality_epoch, finality_block_hash?, finality_status, reorg_epoch, proposal_digest,
terminal_disposition? }))`；status 使用 canonical enum text，optional presence 显式编码，
digest 自身排除。`continuation_digest` 必须包含当前 `ContinuationStatusV1` 与
`continuation_status_digest`，禁止用空字符串/null 替代。

Incoming `ContinuationProposalV1` remains exactly the paired Harness wire schema; the projection only adds Runtime-owned schedule identifiers and status. Runtime must not rename proposal fields or make Agent-owned policy fields authoritative.

允许的 wake condition 使用唯一的 bounded `WakeConditionV1` 形状；v1 的 `wake_conditions` 是 all-of 列表，按 canonical condition bytes 排序，不支持嵌套布尔表达式：

~~~text
WakeConditionV1 {
  schema_version,
  kind: at_or_after_tick | world_event_committed | receipt_linked | state_predicate,
  logical_tick?, event_digest?, receipt_id?,
  subject: { kind, id }?, path_or_rule?, operator?, expected_value_bytes?
}
~~~

每种 `kind` 的 one-of 字段注册表固定如下；未列出的可选字段必须缺席：

| kind | required fields | forbidden fields | bounds/evaluation |
| --- | --- | --- | --- |
| `at_or_after_tick` | `logical_tick` | `event_digest`, `receipt_id`, `subject`, `path_or_rule`, `operator`, `expected_value_bytes` | `logical_tick` 为 u64；满足条件的最早 tick 贡献 `next_wake_tick` |
| `world_event_committed` | `event_digest` | `logical_tick`, `receipt_id`, `subject`, `path_or_rule`, `operator`, `expected_value_bytes` | canonical event digest，编码长度 ≤ 128 bytes；只匹配 committed event |
| `receipt_linked` | `receipt_id` | `logical_tick`, `event_digest`, `subject`, `path_or_rule`, `operator`, `expected_value_bytes` | canonical receipt id，编码长度 ≤ 128 bytes；只匹配 `WorldReceiptLinked` |
| `state_predicate` | `subject`, `path_or_rule`, `operator`, `expected_value_bytes` | `logical_tick`, `event_digest`, `receipt_id` | 完全复用 PreconditionV1 path/type/operator registry；expected bytes ≤ 512 |

`wake_conditions=[]` 在 v1 一律 rejected 为 `wake_conditions_empty`；不把空列表解释为
立即唤醒、永久保持或任意 provider callback。列表最多 16 项、每项 canonical bytes ≤ 768 bytes、
总 canonical bytes ≤ 4096 bytes、嵌套深度为 1；所有项必须成立（all-of）且按 canonical condition
bytes 排序。Runtime 取所有 `at_or_after_tick`
中的 tick 和当前 logical tick 的最大值，写入派生的 `next_wake_tick`；无 tick 条件时
`next_wake_tick` 为空，表示 event/receipt/state-predicate 驱动的 schedule；scheduler 在相关
committed event、receipt link 或 committed state-head 变化时重新求值，不能用 wall clock 代替
event-driven wake。求值固定读取同一个 committed world head，并在 wake event 中记录 evaluation
tick/hash/reorg；未出现的 event/receipt 或未满足
的 predicate 为 false，且仅在未过期时保持 pending。过期或引用 artifact 已超出 retention 时进入
`wake_condition_expired` terminal disposition。不得使用未经持久化的 wall-clock timer、线程 sleep、
provider callback closure 或全局 feedback queue 作为唯一唤醒依据。

每次 wake 消费旧 schedule，再由 Harness/Agent 提出并经 Runtime 验证的 ContinuationProposalV1 创建下一次 wake；同一 wake_id + wake_seq 重复投递必须幂等。过期、precondition 变化、action reject、reorg/finality invalidation、agent/world removal 或 retry budget 用尽时进入 terminal disposition，不隐式重新规划。

ContinuationProposalV1 的唯一 wire schema 包含 schema_version、continuation_proposal_id、
world_id、agent_id、agent_session_id、agent_turn_id、decision_request_id、origin_turn_id、
origin_request_digest、action_or_plan_kind、可选 action_or_envelope_digest、
remaining_budget `{ unit: steps|ticks, value: u64 }`、baseline_observation_digest、goal_digest、
policy_digest、policy_revision、precondition_summary、precondition_digest、wake_conditions、
可选 valid_until_tick、source 和 proposal_digest。Harness 分配并拥有 proposal identity 与
policy；Runtime 验证当前 world/agent/session/turn/request binding 后分配 continuation_id、
wake_id、wake_seq 和 next_wake_tick，并以 ContinuationScheduled/ContinuationWoken 事件返回
相同 correlation。wire 上不存在 proposal_id/proposer_agent_id/request_digest/action_digest/
remaining_ticks/expires_at_boundary alias；steps budget 不得改写为 ticks，proposal policy 不由
Runtime 解释或重写。

Runtime 必须验证 `proposal_digest = H_v1("oasis7.cognition.continuation-proposal.v1",
canonical(proposal_without_proposal_digest))`。写入 durable schedule 后，以
`continuation_digest = H_v1("oasis7.cognition.continuation-context.v1", canonical({
continuation_proposal_id?, proposal_digest?, continuation_id?, wake_id?, wake_seq?, world_id?,
branch_id?, finality_epoch?, finality_block_hash?, finality_status?, reorg_epoch?,
runtime_manifest_hash?, remaining_budget?, valid_until_tick?, status?, continuation_status_digest?,
terminal_disposition?}))`，供下一 request 使用。两者遵循共享
BLAKE3-256/RFC 8949 deterministic CBOR registry，排除 digest 自身；无 continuation 使用
全 absent canonical context object，不能由 adapter 自选 empty/null 表示。

## 6. AgentIntentV2 与 authority/capability 边界

AgentIntentV2 继续拥有 player/authenticated intent 的 intent_id、intent_tick、world/reorg、authority_scope、status、source、request_digest 与 durable transition。cognition envelope 只通过 origin_intent_ref（可选）关联它：

- 不因 cognition turn 增加 AgentIntentV2 的 prompt、model、memory、goal 或 continuation 字段；
- 若存在 origin_intent_ref，runtime 必须核对当前 ledger 的 agent、actor、kind/source、request digest、world/reorg/authority tuple 和状态；
- 若没有 origin intent，AgentActor 仍必须通过 runtime 的 subject authority 入口获得可提交资格；
- provider 只能返回 candidate action/response，不能创建 grant、修改 capability catalog 或推进 AgentIntentV2；
- submit 时始终重新验证当前 authority、capability snapshot、policy、module binding、预算和 kernel action rules；
- 既有 AgentIntentV2 的 idempotent replay/transition 继续复用，不复制一套 cognition authority ledger。

## 7. Failure model 与 rollout

### 失败状态与反馈映射

queued、running、waiting_provider、response_recorded、ready_to_submit、accepted、rejected_stale、rejected_precondition、rejected_authority、rejected_capability、rejected_duplicate、retry_scheduled、failed_provider、failed_persist、cognition_failed、cancelled、recovery_pending、completed 是可观察的 lifecycle 状态。Runtime canonical FeedbackEnvelope status 仅为 committed/rejected/failed/pending；产品层 no-effect 结果映射为 rejected + reject_reason=no_effect，不建立 no_effect status。状态迁移必须由上述 durable event 驱动；未知迁移 fail closed。

Runtime disposition 到目标 FeedbackEnvelope 的映射必须稳定：

| Runtime disposition | FeedbackEnvelope.status | reject_reason | receipt / WorldReceiptLinked |
| --- | --- | --- | --- |
| accepted/committed | committed | absent | canonical receipt；必须有 WorldReceiptLinked |
| stale_base、expired | rejected | 同名稳定 reason | absent；不得有 WorldReceiptLinked、world delta 或 effect |
| stale_capability_snapshot、authority_denied、intent_conflict | rejected | 同名稳定 reason | absent；不得有 WorldReceiptLinked |
| reorg_invalidated、finality_anchor_mismatch | rejected | 同名稳定 reason | absent；不得有 WorldReceiptLinked、world delta 或 effect |
| precondition_failed、action_rejected | rejected | 同名稳定 reason | absent；不得有 WorldReceiptLinked |
| idempotency_conflict | rejected | idempotency_conflict | absent；不得有第二次 effect 或 WorldReceiptLinked |
| same envelope_idempotency_key + same digest | 与原 disposition 相同的 canonical status | 与原 disposition 相同 | 返回原 receipt（若原路径 rejected 则仍 absent） |
| failed_provider、failed_persist | failed | 稳定 failure reason | absent，除非已有 durable committed receipt；不得猜测 receipt |
| cognition_failed | failed | cognition_failed | absent；若已有 committed marker/receipt 则返回 committed，不得降级 |
| recovery_pending | pending | 稳定 recovery reason | absent，除非已有 durable committed receipt；不得猜测 receipt |
| retry_scheduled | pending | retry_scheduled | absent；保留同一 retry lineage，不结束旧 turn 或执行 effect |
| scheduler_backpressure | pending | absent（reason 进入 durable scheduler disposition） | absent；bounded enqueue 失败不得产生 receipt 或 WorldReceiptLinked |
| cancelled | rejected | cancelled | absent；不得有 WorldReceiptLinked、world delta 或 effect |
| `WorldCommitRecordV1(status=aborted)` | rejected | 记录中的 `abort_reason` | absent；不得有 WorldReceiptLinked、world delta 或 effect |
| late_response_after_cancel | rejected | late_response_after_cancel | absent；不得重开 turn 或执行 effect |
| legacy_no_cognition_proof | rejected | legacy_no_cognition_proof | absent；不得有 WorldReceiptLinked、world delta 或 effect |

canonical status vocabulary 仅为 `committed/rejected/failed/pending`。产品层 no-effect
结果必须映射为 `rejected + reject_reason=no_effect`，不是第五个 status。现有
FeedbackEnvelope 的 success/failure 或 summary 字段只能由显式 compatibility adapter 映射：
只有 `success=true` 且存在匹配 `WorldCommitRecordV1(status=committed)` 与 canonical receipt
时才是 `committed`；没有该 marker/receipt 的 legacy success 一律为
`rejected + reject_reason=legacy_no_cognition_proof`。`success=false + runtime reject -> rejected`，
transport/provider failure -> `failed`，未决恢复 -> `pending`；missing outer field or
implementation-specific null 不得猜测，adapter 不得把 stale reason 提升成新 status。
所有 rejected/no-receipt negative fixture 都必须断言无 WorldReceiptLinked、无 world
event/effect、无 debit，并且 replay 后反馈仍相同。

`retry_scheduled` 是非 terminal 的 pending projection：保留原 turn/request lineage，按 bounded
retry policy 唤醒；transport retry 只增加 `transport_attempt`，semantic retry 才创建新的
`retry_seq`/turn/request。`cognition_failed` 是 terminal failed disposition；`cancelled` 关闭
active turn，迟到 response 只能得到 `late_response_after_cancel`，不得重开 turn。任何
`WorldCommitRecordV1(status=committed)` 或匹配 canonical receipt 的证据优先于后续
`failed_persist`/projection failure，Feedback 必须保持 committed 且只补齐 link/projection，
不得降级为 failed。

### P0.1–P2 rollout

| 优先级 | bounded scope | 出口 |
| --- | --- | --- |
| P0.1 | identity/correlation：透传 Agent-owned opaque request_digest；定义 turn/request/origin correlation、provider_invocation_key、envelope_digest 与 envelope_idempotency_key | 所有 request/response/envelope/journal/feedback/receipt 可稳定关联；不得宣称 production async |
| P0.2 | async actor + MVCC：provider 脱离 World::step；AgentDecisionEnvelope 携带 base state、capability/authority bindings、valid_until/preconditions；stale/idempotency/authority rejection | world tick 不受 provider 阻塞；过期结果无 effect；重复 envelope key 无重复 effect；不得宣称 production async，除非 P0.1/P0.2/P0.3 均通过 |
| P0.3 | minimum journal/recovery：Turn lifecycle、canonical response/action、FeedbackEnvelope mapping、checkpoint head、crash recovery 与 replay no-provider path | response durable 后不再调用 provider；recovery/replay 产生相同 receipt/state root；P0.1–P0.3 证据齐全后才可作 production async claim |
| P1 | Runtime scheduler/actor status 与 continuation schedule/wake/recovery durable；Harness proposal 与 Runtime validation 分界；active-turn mailbox 隔离；Builtin/ProviderBacked 统一 envelope/journal；feedback 按 agent/turn/request 隔离；bounded retention | restart/乱序 wake 可恢复；多 agent 不串扰；两种 backend 共享 lifecycle contract |
| P2 | Agent-owned memory/goal/prompt 语义的跨 turn projection、成本/延迟产品策略和更丰富的 provider policy | 仅在 Agent 专业文档定稿后接入；不得反向放宽 runtime authority 或 replay contract |

## 8. Acceptance 与 deterministic/replay verification

实现完成前不得把本 PRD 标为 proven。最小验收必须包括：

1. provider 延迟大于多个 world tick、永久不返回和网络断开时，world tick 仍完成，且执行线程没有 provider wait；
2. fresh envelope accepted；tick/hash/capability/authority/precondition 任一变化产生稳定拒绝且 world snapshot、event、receipt 不改变；
3. 同一 provider_invocation_key 或 envelope_idempotency_key + digest 重试只返回原 disposition；同 key 不同 digest 产生 idempotency_conflict；
4. capability catalog/context、AgentIntentV2 actor/digest/authority mismatch 均在 first effect 前拒绝；
5. 在每个 journal 边界注入 crash，恢复结果必须严格匹配 §4.1 的 prefix-specific oracle：
   pre-commit/prepared 保持 `R_parent` 且无 receipt，committed 为 `R_next` 并补齐 canonical
   receipt，冲突固定暴露最后 verified head 并 quarantine；不得笼统要求所有 prefix 等于
   no-crash accepted root；
6. response 已 durable 后恢复不再调用 provider；replay 只消费 canonical response/receipt；
7. duplicate/乱序 wake、snapshot restore、provider retry 与 receipt retry 不重复 debit/effect/event；所有 rejected/no-receipt path（包括 no_effect）都没有 WorldReceiptLinked；
8. Builtin 与 ProviderBacked 在相同 envelope fixture 下产生相同 host validation/rejection semantics；backend-specific prompt/memory/goal 不进入 runtime contract；
9. journal、pending queue、continuation 和 artifact 引用受 bounded retention/size 约束，超限形成结构化 failure；
10. 旧 ActionEnvelope 和旧 snapshot 通过显式 compatibility adapter 读取；未升级数据不得静默获得新的 MVCC 语义。
11. 固定容量 scheduler queue 被填满时，enqueue 立即返回而不阻塞 world worker，追加
    durable `pending/scheduler_backpressure` disposition；provider invocation count、receipt、
    WorldReceiptLinked、world event/effect/debit 均不增加。释放容量后的 deterministic wake
    只恢复一次原 turn，并最终收敛为一个 terminal disposition。
12. 多 agent、同 tick、乱序 wake 与持续 backpressure fixture 按唯一 fairness comparator 收敛；
    每 agent/global service budget 生效，round-robin/aging 不发生 starvation，provider、receipt
    与 WorldReceiptLinked 数量有界。
13. `WakeConditionV1` 的每个 kind、组合、缺失 event/receipt、unknown predicate、过期与 artifact
    retention 边界均可 replay；evaluation head/hash 相同则 wake disposition 相同。
14. crash 覆盖 `WorldCommitRecordV1` prepare、commit marker、world journal/state、idempotency
    record、receipt、CognitionJournal link 的每个 interleaving；任何 ambiguous prefix 都是
    `recovery_pending`，不重调 provider、不重执行 kernel/effect。
15. checkpoint/GC 在 retention horizon 前保留 terminal key tombstone、receipt link、response
    artifact 与 reorg window 引用；horizon 后的重复 key 只得到 expired/legacy rejection，绝不
    重新产生 effect。
16. branch/finality/reorg epoch 变化会使所有未提交 turn/wake/continuation 得到
    `reorg_invalidated`，已提交 receipt 仍只被读取一次；旧 ActionEnvelope/in-flight snapshot
    只能得到 `legacy_no_cognition_proof`，不得自动升级为 fresh MVCC action。

验证输出必须包含 fixture input digest、request/envelope digest、journal head、world/receipt/state roots、provider invocation count、recovery disposition 和 schema/version。只有执行、拒绝、crash recovery、snapshot restore 与 replay 全部对齐，能力才可写为 proven。

### 8.1 可执行验证入口

| 层级 | 命令 | 预期证据 |
| --- | --- | --- |
| docs current | `./scripts/doc-governance-check.sh` | paired authority、索引与状态词治理通过 |
| P0.1 digest target | `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_identity` | shared request/envelope golden bytes、digest registry、wire mapping 与 collision fixtures 通过 |
| P0.2 MVCC target | `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_mvcc` | stale/expired/precondition/capability/idempotency rejection 均无 world effect |
| P0.3 recovery target | `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_recovery` | journal-edge crash 后不重调 completed provider、不重复 receipt/effect，state root 一致 |
| scheduler backpressure target | `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_scheduler_backpressure` | bounded queue-full 时 world 不阻塞、不调 provider、无 receipt/effect；容量恢复后单次 wake 收口 |
| scheduler fairness target | `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_scheduler_fairness` | 多 agent deterministic order、service budget、round-robin/aging 与 no-starvation 通过 |
| wake condition target | `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_wake_conditions` | WakeConditionV1 canonical evaluation、expiry、missing reference 与 replay 通过 |
| commit interleaving target | `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_commit_interleavings` | prepare/commit/receipt/key/journal crash prefix 全部 recovery_pending-safe 且无重复 effect |
| retention/GC target | `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib agent_cognition_retention_gc` | checkpoint pin、terminal tombstone、artifact horizon 与过期重复请求 fail closed |
| required implementation gate | `./scripts/ci-tests.sh required` | required-tier runtime/provider regression 通过 |
| full proof gate | `./scripts/ci-tests.sh full` | multi-Agent、long-run、restore/replay、backend parity 通过 |

focused filter 是实现必须建立的稳定验证 surface；不存在或没有实际输出时只表示 target。
artifact 至少记录 canonical fixture bytes、request/envelope digest、journal head、provider
invocation count、feedback/receipt disposition 与 state root。

## 9. 证据锚点与专业跟进

当前实现证据锚点包括：

- simulator/runner.rs:470、573–607：simulator/兼容 tick 与 tick_decide_only 的同步 AgentBehavior::decide；simulator/decision_provider.rs 的同步 DecisionProvider::decide、DecisionRequest、ProviderBackedAgentBehavior 是决策接口，不能据此推断 native world thread 同步等待；
- viewer/runtime_live/llm_sidecar_runner.rs:211–229：native Builtin/ProviderBacked 均构造 AsyncAgentRunner；simulator/async_agent_runner.rs:258–277、360–383：worker 调用 decide、try_send/try_recv 与不在 world thread join provider。:792 的 logical-tick helper、:806 的 kernel-step adapter 和 :825 的等待式 run_one_turn 各有独立边界；native 接线不证明 browser runtime-live 服务；
- viewer/runtime_live/llm_sidecar_async.rs:490–529、llm_sidecar_cognition_wait.rs:419–592、llm_sidecar_cognition.rs:339–442、757：普通 Wait 的当前 context/Harness proposal、Runtime admission、projection/readback、wake selection/resume/reconcile；WaitTicks 走本地 schedule_provider_wait。simulator/tests/agent_cognition_live_actor.rs:60、76、377 与 viewer/runtime_live/tests/auth_actions_provider_context.rs:786–886 是有界 actor/native mock fixtures，未执行于本次文档修正，不能代替真实 provider、distributed finality 或全量 durable recovery 证据；
- runtime/events.rs 与 runtime/world/actions.rs：仅有 action id + action 的 ActionEnvelope；
- runtime/agent_cell.rs、runtime/world/agent_intent.rs、runtime/events/domain_event.rs：AgentIntentV2 authority、digest 与 durable transition；
- world-runtime/runtime/runtime-integration.md：LLM 是外部效应，replay 不再次调用 LLM；Scheduler::tick 为目标接口。

实现前需由 runtime_engineer、agent_engineer、WASM/platform owner 和 TPM 共同冻结 envelope 编码、state/world hash 范围、journal retention、provider idempotency 传递和跨进程 adapter conformance。本文不宣称任何未完成 implementation。
