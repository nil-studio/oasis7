# LLM Agent Decision Provider 标准层 + Local Provider 外部适配可行性（2026-03-12）设计

- 对应需求文档: `doc/world-simulator/llm/decision-provider-contract.prd.md`
- 专题入口与权威边界: `doc/world-simulator/llm/README.md`
- 连续认知上层设计: `doc/world-simulator/llm/continuous-agent-harness.design.md`
- 异步调度、MVCC 与恢复设计: `doc/world-runtime/runtime/agent-cognition-lifecycle.design.md`

## 1. 设计定位
定义 world-simulator 中“世界内 Agent 契约”与“具体外部 agent provider”之间的标准层，使 `Local Provider` 之类的外部框架可通过 adapter 参与模拟，但不侵入 runtime 权威、规则执行与回放边界。

### 1.1 实现状态与证明门槛

本设计将接口存在、显式 bridge 和生产闭环分开记录：

| 状态 | 结论 |
| --- | --- |
| `current` | `DecisionProvider`、Mock/loopback contract、active manifest 的确定性 command projection、`CapabilityCatalogSnapshot`/`CapabilityInvocationContext`/`AgentCommandResponse` DTO 及基础校验已存在。 |
| `partial` | `ProviderBackedAgentBehavior` 可通过显式 context 接收 catalog，loopback 可校验 typed response，且有 `RuntimeTrustedModuleExecutor` seam；尚无证据证明每个 live Agent turn 由 runtime 自动生成并注入 subject-bound context。 |
| `partial` | 当前 catalog 主要是身份、schema version/hash 和 payload bound；`payload: Vec<u8>` 是 opaque bytes，envelope canonical CBOR 校验不等于制度参数 schema 的语义校验。 |
| `target` | runtime-owned per-turn discovery、受信 semantic schema retrieval、typed args 的 host canonical encoding、runtime 二次校验和未知制度 E2E。 |
| `proven` | 只有固定 fixture 同时证明成功闭环与负例无 sandbox/journal/state/charge 副作用，才能把该 target 标记为 proven。 |

因此，当前文档中的“catalog 已落地”仅指机器可发现的 projection/DTO seam；“Agent
capability discovery 已落地”必须等自动 wiring 与真实 Agent E2E 通过后才可使用。

## 2. 设计结构
- 世界权威层：`WorldKernel / runtime` 继续负责校验、状态变更、事件与 receipt。
- Agent 外观层：保留 `AgentBehavior` 作为 simulator 现有调用入口，对上保持 `decide/on_event/on_action_result` 语义。
- Decision Provider 标准层：在本地定义稳定的 request / response / feedback / trace 契约，作为唯一 provider 集成点。
- Provider Adapter 层：为 `Local Provider`、本地 mock provider、未来其他 agent framework 分别实现适配器。
- Memory / Trace 桥接层：把 provider transcript、tool trace、diagnostics、memory write intent 收敛到本地 `AgentMemory` 与 `AgentDecisionTrace`。
- 验证与评估层：通过 golden fixtures、错误签名与 QoS 指标，横向评估不同 provider。

### 2.1 需求承接与分配表

本表只承接本次 §6 的 builtin 兼容合同；既有 discovery、外部 provider、Runtime 与持久化设计保持各自范围。

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-d1ea0352-7f1a-426b-907a-cf41571a1e7f；[Builtin 当前输入完整性](decision-provider-contract.prd.md#builtin-input-integrity) | 每次 builtin 请求保留必要 observation 和目标；已有查询时保留最新 reply；最小输入不可容纳则停止。 | [必要输入与预算流程](decision-provider-contract.design.md#des-builtin-input-integrity) | `agent_engineer` 的 profile/assembler/Responses adapter；Harness 的既有错误与 Wait 路径。 | 不扩展 ProviderBacked、C0 调用计数、retry/cache、付费 cognition 或持久化协议。 |
| Trace refs: O7-REQ-d1ea0352-7f1a-426b-907a-cf41571a1e7f；[输入完整性 AC](decision-provider-contract.prd.md#ac-builtin-input-integrity) | 以真实第二次 payload 和受阻请求无后续模型调用，独立证明输入保护。 | [输入完整性验证组](decision-provider-contract.design.md#ver-builtin-input-integrity) | `qa_engineer`；builtin behavior 与序列化后的请求捕获 seam。 | 不以 trace/History 代签，不证明真实模型质量或发布可用性。 |
| Trace refs: O7-REQ-be4bb896-dc4f-41d1-9f87-c381049ac190；[Builtin 参数说明一致性](decision-provider-contract.prd.md#builtin-parameter-consistency) | builtin 文字、tool schema、parser 和 Runtime 使用相同治理约束，保留合法端点和越界拒绝。 | [治理约束消费关系](decision-provider-contract.design.md#des-builtin-parameter-consistency) | `runtime_engineer` 与既有玩法治理参数 authority；`agent_engineer` 维护 builtin 消费者。 | 不扩大合法域、不修改主体/权限规则、不扩为全部工具 schema 重构。 |
| Trace refs: O7-REQ-be4bb896-dc4f-41d1-9f87-c381049ac190；[参数说明一致性 AC](decision-provider-contract.prd.md#ac-builtin-parameter-consistency) | 实际请求文字/schema 对账与 parser/Runtime 边界独立验证，保留 `6500` 正例和其它正常校验。 | [治理参数一致性验证组](decision-provider-contract.design.md#ver-builtin-parameter-consistency) | `qa_engineer`；参数生成、parser 与 Runtime 现有验证入口。 | schema 正确不代替文字断言；不重平衡玩法或调用真实模型。 |

<a id="21-目标-discovery--execution-sequence"></a>
<a id="21-目标-discovery-execution-sequence"></a>

### 2.2 目标 discovery / execution sequence

```text
WorldKernel live state
  -> active manifest + subject grant/policy intersection
  -> CapabilityCatalogSnapshot + CapabilityInvocationContext (one turn)
  -> schema_ref retrieval and canonical schema_hash check
  -> DecisionRequest(observation, catalog, context, semantic schemas)
  -> provider returns TypedCommandIntent
  -> trusted host validates typed args and encodes canonical CBOR envelope
  -> runtime revalidates catalog/context/schema/authz/cost/nonce/live state
  -> staged module execution -> committed ActionResult/receipt
  -> FeedbackEnvelope -> local memory policy
```

Runtime must own the first, second and final validation steps. The provider may choose only
an entry already present in the snapshot and may propose typed values; it cannot select the
subject, presenter, audience, grant, snapshot, nonce, caller, or cost quote. The context is
transport binding, not a bearer grant; grant, revocation, policy, budget and replay checks
remain runtime authority.

The per-turn generator is a target host service (not a provider feature):

1. Read the finalized live head, active module records, policy and the Agent subject's
   non-revoked grants for the requested audience.
   The immutable `AgentInvocationBinding` digest binds `world_id`, `branch_id`,
   `finality_epoch`, `finality_block_hash`, `policy_epoch`, `revocation_epoch`,
   catalog snapshot digest, exact audience/instance target and response nonce.
   These fields are part of the target canonical hash domain; omission from a
   compatibility DTO cannot silently omit the binding and instead rejects target execution.
2. Intersect those records with declared commands, sort entries deterministically, compute
   the snapshot identity and choose a fresh response nonce. Store/bind the invocation context
   before making the provider request.
3. Resolve each eligible `schema_ref` from a trusted manifest/schema store. Expose only a
   verified descriptor to the provider and keep the grant id/eligible grant list out of the
   provider's authority-bearing response.
4. Invalidate the snapshot at its tick/head/policy/revocation boundary; a changed catalog
   requires a new discovery request, not an automatic rebind or fallback.

The current explicit `with_capability_context` constructor path is useful for fixtures and
host-loop experiments, but it is not the per-turn generator and must not be described as
production auto-wiring.

<a id="22-continuous-harness-compatibility"></a>

### 2.3 Continuous Harness compatibility

The legacy `DecisionRequest/DecisionResponse/FeedbackEnvelope` DTOs are additive inner payloads,
not a replacement for the Continuous Harness outer context. The target tagged
`DecisionResponse.decision` variants include `wait`, `wait_ticks`, `act`, `query`,
`module_command`, and `module_command_response`; Query is read-only against the frozen snapshot,
while module responses must pass host schema/context/nonce validation plus Runtime validation.
`DecisionResponse.module_command` is a candidate typed module intent and must pass host schema encoding plus
like `action_ref`; a provider tool call is never committed execution. `FeedbackEnvelope` v1 maps
`action_id -> candidate_action_id`; `success=true` requires a committed Runtime receipt, while
`success=false` requires an explicit Runtime disposition or becomes
`legacy_feedback_ambiguous`. Missing session/turn/request/digest/receipt correlation is
`legacy_no_cognition_proof` and cannot enter the production async lane.

The request digest is computed from canonical V1 outer-context bytes excluding the output
`request_digest` field and transport-only `transport_attempt`, under
`oasis7.cognition.request.v1`; provider invocation uses the shared domain-separated derivation.
`FinalityBindingV1` and world binding fields follow the paired Runtime shape and use the shared
`H_v1` registry. The default target lane is strict. Legacy scope `short_term` may map
to `turn_private` only through an explicitly selected legacy route/adapter and
`memory_scope_alias_used`; heuristic action fallback is restricted to a legacy
fixture/profile and is excluded from target parity/proven evidence and automatic
memory/continuation semantics. The current HTTP bridge does not define a
`compatibility_lane` wire field.
P0 memory writes allow only `turn_private | session_private`; the reserved
`agent_private_long_term` enum value is disabled by default and deferred to P2/independent memory
authority.

## 3. 关键接口 / 入口
- `AgentBehavior`：`crates/oasis7/src/simulator/agent.rs`
- `AgentDecisionTrace`：`crates/oasis7/src/simulator/agent.rs`
- `AgentMemory`：`crates/oasis7/src/simulator/memory.rs`
- viewer 诊断协议：`crates/oasis7_proto/src/viewer.rs`
- runtime-live bridge 消费 `AgentDecisionTrace`；本专题拥有 request/response/feedback/trace/memory 语义，Viewer delivery 与 runtime event/snapshot mapping 分别由 `doc/world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md` 与 `doc/world-simulator/prd.md` 承载。
- launcher operator HTTP-JSON 机器接口专题：`doc/world-simulator/launcher/game-client-launcher-control-plane-and-machine-interface.prd.md`；该接口不等同于 `DecisionProvider` 或世界内 autonomous Agent。

## 4. 约束与边界
- 外部 provider 只提供决策建议，不得直接改 world state、存储或 runtime 内核。
- 所有 world action 必须先经过本地 action schema 白名单，再进入 runtime 校验。
- provider trace 需要可脱敏、可裁剪、可映射；不得把外部协议泄露为 viewer 唯一调试口径。
- trace 必须有界并脱敏，同时保留 provider latency、token/cost、repair diagnostics 与 action/result 关联。
- target trace bounds：每 turn 最多 64 条 transcript、64 条 tool trace；单条分别最多 2 KiB、1 KiB；input/output summary 各最多 1 KiB；upstream trace 最多 4 KiB；canonical trace 总计最多 32 KiB。超限不得静默截断：非-authority diagnostics 以 `trace_payload_too_large` 丢弃，authority-bearing overflow fail closed；credential/token/authorization/private-key/path 统一替换为 `<redacted>`。
- memory 的权威副本保留在本地 world-simulator；外部 memory 仅可作为 provider 内部缓存或检索辅助。
- Empty memory retrieval uses an explicit schema-defined snapshot (`revision=0`, empty entries,
  explicit scope and canonical digest), never omission/null/empty-string; it does not block a
  decision, while writes still require the Harness policy and committed Runtime outcome gate.
- Target `MemoryWriteIntentPolicyV1` encodes optional `summary` with explicit `present=false` when
  omitted; a present summary must be non-empty after NFC/trim. The legacy inner DTO must carry a
  non-empty summary string. Tags may be an empty list (omitted tags have the explicit canonical
  empty-list encoding), but empty tag elements are rejected.
- Target `DecisionResponse.memory_write_intents` carries `MemoryWriteIntentV1` (`schema_version`,
  `scope`, optional `summary`, and `tags`); the existing `MemoryWriteIntent` DTO remains unchanged
  and is accepted only through the explicitly selected legacy route/adapter mapping. The current
  HTTP bridge does not define a `compatibility_lane` wire field.
- `Local Provider` PoC 只允许在低频、低破坏性 agent 类型上试点；高频强一致 actor 不在首轮范围。
- required 测试必须可离线执行，因此标准层必须先支持 `MockProvider`，不能以外部联网能力作为主验证前提。

### 4.1 Semantic schema 与 typed args

`ModuleCommandCatalogEntry` 是 discovery identity，不是足够的 Agent 语义。target 为每个
entry 绑定 `schema_ref` 和 content-addressed `schema_hash`，descriptor 的 canonical
内容至少包含：command summary、input fields（name/type/required/default/bounds/enums）、
result/error shape、声明的可见副作用与 bounded cost hint。descriptor 不能包含私钥、
grant proof 或 host-only provenance，也不能授予 command 权限。

schema retrieval 由 trusted host 完成：

```text
active manifest declaration
  -> schema_ref lookup
  -> canonical descriptor bytes
  -> hash/version/field validation
  -> provider semantic tool schema
```

找不到 descriptor、canonical bytes 无法解码、版本不支持或 hash 不匹配时，host 在 provider
调用前返回稳定的 `capability_schema_unavailable` / `capability_schema_mismatch`。provider
不得用自己的 schema、自然语言相似度或历史缓存补洞。

target provider response 是 `TypedCommandIntent`：

```text
{
  catalog_snapshot_id,
  response_nonce,
  selected_entry: { module_id, module_version, instance_target,
                    namespace, command, schema_version, schema_hash },
  typed_args,
  trace_id
}
```

For a module-instance audience, `instance_target` is mandatory and carries the stable
`(world_id, module_id, instance_id)` key through snapshot, response, invocation digest,
nonce and execution receipt. A governed world/institution-scoped command uses an explicit
non-instance target variant; missing target is never interpreted as global-module fallback.

trusted host 依据已验证 descriptor 检查 typed args 的必填字段、类型、范围、枚举和大小，
再以唯一 deterministic canonical-CBOR 规则编码为 `ModuleCommandEnvelope.payload`，并
由 host 填充 namespace/name/schema version/hash。provider supplied raw bytes 只能作为
transitional compatibility lane，不能绕过 host encoding；当前 `decode_canonical` 只验证
envelope bytes 的 canonical form，不代表 opaque payload 已按制度 schema 通过语义验证。

runtime 随后重新执行 envelope canonical encoding、payload 完整语义解码、active module
和 declaration/hash/limit、subject/grant/policy/revocation、cost quote、nonce/replay
和 live head 检查。任一检查失败都不得进入 sandbox、metering、journal 或 partial state。

## 5. 设计演进计划
- 先冻结 `Decision Provider` 最小契约与 fixture 评估口径。
- 再实现 `MockProvider`，验证标准层与本地 trace / memory / action mapping 正常闭环。
- 再实现 `Local ProviderAdapter` PoC，并限制到单一低频 NPC 场景。
- 最后依据动作有效率、超时率、成本与诊断完整度，决定是否扩展到更多 agent 类型。

### 5.1 Proof-first 交付顺序

1. 在 runtime host 增加 per-turn subject-bound catalog/context 生成和注入，保留显式
   constructor seam 作为 fixture-only 路径。
2. 为 active module command 建立 semantic schema retrieval，验证 canonical descriptor
   hash 与 manifest declaration 一致。
3. 让 provider 只返回 typed args，由 host canonical encode，再由 runtime 做完整重验。
4. 用一个不在 Kernel closed `Action` 枚举中的 active governed module command 做未知制度
   E2E；成功后再推进统一 transaction、instance-targeted command/state 等 runtime 扩展。
5. 固定 scenario、agent profile、provider/adapter/protocol version、catalog/context、
   timeout、tick budget 和 cost budget，重复采样并单独报告 schema failure、context
   contamination、drift、stuck-loop、latency/token/cost；聚合均值不能掩盖单场景失败。

### 5.2 未知制度 E2E 与负例

成功 fixture 选择一个未进入 Kernel closed `Action` 的 command（例如
`economic_contract.open`），但该 command 所属 module 已通过治理并 active：

`live head -> subject grant/policy intersection -> catalog/context -> schema retrieval ->
provider typed args -> host canonical CBOR -> runtime live recheck -> staged module execute ->
committed result/receipt -> feedback -> memory policy`。

验收必须证明不添加 native Action、不接受 provider 自报 grant/schema、不依赖手工复制
context，也不把 provider 过程当作 world fact。至少覆盖下列拒绝：

| 负例 | 预期拒绝与副作用保证 |
| --- | --- |
| unknown/inactive module 或 command 不在 snapshot | stable error，收敛为 `Wait`/`ActionRejected`，不进 sandbox/journal/state/charge |
| schema missing、hash/version 漂移或无法 canonical decode | 在 provider 调用前 fail closed |
| revoked/expired grant 或 subject/presenter/audience mismatch | runtime deny，不按名称、旧 grant 或静态 Action fallback |
| stale snapshot、policy/branch/finality/cost quote 变化 | old intent 失效，要求重新 discovery |
| typed args 缺字段、类型/范围/枚举错误或超 bound | host 不生成 envelope |
| nonce replay 或同 nonce 不同 request hash | 返回历史 idempotent receipt 或 deterministic denial，不重复副作用 |
| world/branch/finality、policy/revocation epoch 或 snapshot digest 漂移 | old context 失效并重新 discovery，不跨 authority boundary 重放 |

The successful unknown-Institution fixture reuses runtime
`institution-migration-v1` and product SC-32: preview/stale/deny/no-effect
outcomes debit nothing, one accepted effect has one debit and receipt, and
retry/reconnect/restore/replay neither double-charge nor create replay credit.

<a id="builtin-request-contract"></a>

## 6. 内置 provider 适配与诊断流

- builtin Responses adapter 将感知、裁剪后的本地 memory、目标、有限工具 catalog 与 timeout budget 组装为 request；它只返回一个结构化候选。profile/provider 覆盖、endpoint 规范化、有限 retry 与错误分类属于 adapter 配置层，不能改变 world state 或 runtime 权威。
- **Target closed-action safety flow** 固定为 `perceive/context/goal -> request -> one structured decision -> catalog/schema guard -> host adapter -> runtime validate/execute -> committed ActionResult feedback -> Agent memory`。当前 builtin `AgentRunner` 只产出 decision，`action_result=None`，没有内建 runtime executor callback；只有显式 loopback/host seam 能继续执行，因此 runner decision 或 parser 成功都不是 committed feedback 证据。无终态、超量 repair、非法动作、超时或 provider failure 收敛为可诊断的 `Wait`/`ActionRejected`；不执行多片段 completion。
- Trace bridge 保存角色分离的 chat/tool trace、预算/latency/repair diagnostics、module-call intent/receipt 与 feedback。它裁剪和脱敏后供诊断使用；Viewer delivery、runtime event/replay 与玩家记忆纠正仍由各自专业 authority 负责。
- receipt flow 是 `prompt module-call -> diagnostic intent/receipt -> journal/trace -> later reconciliation`。它保留上下文与兼容默认值，但并未证明 action-result 因果、重放不重调 provider 或恢复不重复外部副作用；这些 T4/T5 proof 必须由 runtime 设计与验证完成。

<a id="des-builtin-default-system-prompt"></a>
#### 默认配置到实际请求

对应 [builtin 默认 prompt PRD](decision-provider-contract.prd.md#builtin-default-system-prompt)。本次保留现有策略，修正 PRD 把短样例误写成 fallback 的说法。

默认值的两个现存位置为 `crates/oasis7/src/simulator/llm_defaults.rs` 与 `crates/oasis7/src/simulator/llm_agent.rs` 的 `DEFAULT_LLM_SYSTEM_PROMPT`；两者须与 PRD 独立文本一致。`LlmAgentConfig::from_env_with` 处理空值 fallback，TOML 读取沿原有效配置优先级，`effective_system_prompt` 再应用 runtime override，`PromptAssembler` 消费最终 `base_system_prompt`。

检查默认值时分别捕获 env-only、合法 TOML 省略/空白字段且无其他有效配置来源的实际请求；检查覆盖时提供不同的 TOML/env/runtime 文本，并验证当前优先级及清除 runtime override 后回到解析配置。不能只测常量相等而绕开 assembler，也不能用 config.example 中显式短句证明默认值。复用离线 capture fixture 即可，不需要模型凭据或付费请求。这个文档决定不修改 prompt profile、ProviderBacked 配置或 local bridge 的独立 prompt，也不代签输入完整性和最终预算验收。

<a id="des-builtin-concurrency-retry"></a>
#### 结构化 concurrency-limit 分类

对应 [builtin 有界恢复 PRD](decision-provider-contract.prd.md#builtin-concurrency-retry)。`openai_retry::is_concurrency_limit_error` 应解析已支持的 JSON error payload，在同一 error 对象内读取字符串 type/message；按既有文本匹配语义判断 `rate_limit_error` 与 `concurrency limit exceeded`，不依赖序列化空格、换行或 key order。不得从兄弟对象、其他 event 或普通文本凑齐两个条件；无法解析或字段不符时沿原 DecodeResponse/失败路径返回。

重试仍受 `retry_attempts(max_model_calls)` 控制：None 最多额外一次，Some（含 0）不追加调用；原 250 ms 退避、上层预算及失败到 Wait/Rejected 映射不变。HTTP 状态码不是该 detector 的泛化条件，修复不启用所有 429 的额外 retry。

离线 mock 经过真实 Responses/SSE 解析和 complete 路径，分别发送紧凑、含空格、合法多行的等价 error JSON：首错后成功断言恰好两次请求，重复错误断言最多两次；缺字段、其它 type、无关 message、坏 JSON、type/message 分处不同对象均不触发。budgeted Some 请求断言只有一次。对 native lane 检查一次 250 ms 退避边界，不通过增加预算或重试次数让格式样例通过；这些预期由独立 fixture 给出，不能只对 detector 自生成的紧凑 JSON 作自证。

### 6.1 验收证据分层

| 证据层 | 能证明什么 | 不能证明什么 |
| --- | --- | --- |
| required | DTO round-trip、catalog/context binding、semantic schema hash、typed args host encoding、稳定错误码和 Mock 负例 | 真实 provider 可理解未知制度，或 live runtime 已自动 wiring |
| full | 未知制度 E2E 的成功/负例、runtime live recheck、staged commit、committed feedback/memory、replay/no-duplicate-side-effect | 更大规模默认体验或产品规则正确性 |
| rollout/parity | 在固定 profile/场景下的行为、时延、成本、trace 完整度和稳定性 | 任何 provider 绕过 runtime authority 的许可 |

只有 full 证据同时覆盖成功链路和上表负例，才可将 subject-bound discovery、semantic
schema、typed args execution 标记为 `proven`；单独的 catalog DTO、schema parser、
provider response validator 或诊断 trace 均只能保持 `partial`。

### 6.2 Builtin 输入合同的基线与承接

本节由 `agent_engineer` 维护，`runtime_engineer` 审读治理规则消费边界，`qa_engineer` 负责确定性验证。适用范围是 builtin `LlmAgentBehavior` 的 PromptAssembler、行为循环与 Responses adapter；独立 `ProviderBacked` 路径不由本节推定受影响。以下新增规则是待实现、待验证的 target 合同；文档收敛不表示缺陷已修复或实际 QA 已通过。

审读基线为 `eng-cc/oasis7@e01e29064517f4bf375767a19a09802335828249`，日期为 2026-10-12。固定基线只用于解释差距；实际 profile、工具集和源码改变后应重新验证，不能把下表数值提升为永久默认值或模型真实 token 数。

| 对象 | 固定基线事实与证据 | 目标与差距 |
| --- | --- | --- |
| 必要输入与有效预算 | [profile](https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/simulator/llm_agent.rs#L149-L161) 的 Balanced、Compact 有效文本预算分别为 `4608-896-480=3232`、`4096-768-352=2976`；按同基线估算器，required Tool Protocol 与 OutputSchema 合计 `3464`，尚未计入 Policy、Goals 和 Context。[现有裁剪](https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/simulator/llm_agent/prompt_assembly.rs#L412-L448) 会删除 History/Conversation 后继续处理 required Context。 | 以实际配置容纳必要输入；最小输入仍不可容纳则停止，不发出缺少当前反馈的下一次请求。 |
| 最终发送内容 | [行为循环](https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/simulator/llm_agent/behavior_loop.rs#L213-L282) 在 assembler 后追加 Dialogue Constraints、guard 和 repair 文本；[Responses builder](https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/simulator/llm_agent/openai_payload.rs#L695-L718) 再加入 tools。 | 预算校验覆盖最终模型可见输入，中间 section 统计不能作为发送许可。 |
| 治理参数说明 | [builtin 文字模板](https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/simulator/llm_agent/prompt_assembly.rs#L266) 与 [tool schema](https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/simulator/llm_agent/openai_payload.rs#L145-L154) 的范围不同；[玩法数值 authority](../../game/gameplay/gameplay-war-politics-mvp-baseline.design.md#des-war-politics-002) 已定义合法范围。 | 修复说明消费关系，不重新决定治理合法域。 |

<a id="des-builtin-input-integrity"></a>

### 6.3 必要输入与预算流程

#### 必要输入的来源和保护边界

行为循环先按本 Agent、本次决定过程和当前工具调用关联取得输入，再交给 assembler。每次请求都保护当前 observation 和目标；已有查询时再保护最新 reply，首轮无查询不构造回复。必要性按数据语义确定，不能由 `Context`、`History`、`Conversation` 的容器名决定。最新 query reply 需从当前 `ModuleCallExchange` 的实际结果取得；`last_query_result` 是独立机制，不能假定它已经携带普通查询工具的结果。

| 输入 | 必须保留的最小语义 | 可缩减范围与失败边界 |
| --- | --- | --- |
| 当前 observation | 现有 projection 的 `agent_id`、`time`、`pos`、`self_resources`；存在时保留最近动作的结果/拒绝原因，以及识别当前可见位置与相关前置条件所需的数据。 | 可按既有可见性权限缩减远处实体和重复详情；列表缩减保留省略/覆盖信息，不能把未展示解释成不存在。必要字段未知时保留显式未知语义，不构造值。 |
| 当前目标 | 实际生效的短期、长期目标及其已有空值语义。 | 去除重复包装，不删减或改写目标含义；超长目标使最小输入不可容纳时走配置不足路径。 |
| 最新 query-tool reply | 当前查询的工具名、关联输入、成功/错误状态与决定下一步所需的结果字段。对 `environment_current_observation`，保留查询实际提供的条件、缺失前置与阻断原因；字段不存在时不能补造。 | 可以去重以及采用确定性的字段投影；不能因其它历史被删除而丢掉最新结果，也不能只保留“查询已执行”。 |
| 协议与约束 | 生效 system 指令、tool-only 约束、本次允许工具所需的参数合同，以及适用的 Dialogue/guard/repair 约束。 | 可合并重复说明或按需提供已有 schema 内容，但必须保持支持的合法输入和 required/type/bounds/enum 语义；不能靠删除有效约束获得空间。 |

查询返回值是有来源的只读决策材料，无须等待世界动作 committed receipt 才能进入下一次 builtin 请求；查询成功也不表示动作已执行或目标完成。Runtime action feedback 的 receipt、memory 写入与世界效果合同仍按 [Harness feedback routing](continuous-agent-harness.design.md#7-feedback-routing-与污染防线) 执行，本节不把 query reply 转为权威 memory 或世界事实。

压缩采用本地、确定性的结构化选择和序列化；相同输入、profile 和投影规则产生相同内容。先选择字段与条目，再输出可完整解析的 JSON/文本结构，不对必要 JSON、结果或目标做字符前缀截断。可选旧历史、memory digest 与重复 query 副本使用剩余空间；必要字段、身份和错误状态不能被软上限或 peak-budget 二次裁剪覆盖。无法从有界 projection 保留必要语义时明确失败，不调用另一模型生成摘要。

#### 计算范围与发送前检查

输入容量与 [C0 model/tool call budget](continuous-agent-harness.design.md#43-c0-request-bound-call-budget) 分开处理。本节不改 `BudgetContractV1` 字段、计数、重试或缓存语义。

1. 从实际生效 profile 读取 context window、输出预留和 safety margin；不能用 `PromptBudget::default()` 代替 behavior 的实际配置。扣除预留后无有效容量应报告配置不足，不以最小值钳制制造容量。
2. 在分配可选历史前构造必要输入，并去重重复协议/schema。预算清单必须覆盖 `instructions`、最终 `input` 的全部文本与封装，以及实际启用的 `tools`/参数 schema；assembler 之后追加的 Dialogue Constraints、Anti-Repetition Guard、final-turn 和 repair 文本同样计入。
3. profile 的可用容量按 `context window - output reserve - safety margin` 计算。工具/schema 与协议封装可直接估算，或以对当前工具集有效且有 fixture 约束的显式上界预留；同一内容只计一次，不得省略，也不能在已用尽预算后再追加。若保留 assembler 文本子预算，它必须从该总容量扣除工具/封装成本后派生，并在诊断中与总量分列。
4. 估算口径须在实现与 fixture 中明确；`chars/4` 一类估算不等同真实 tokenizer。安全余量保留其用途，不能通过重命名统计项隐藏超限。工具集、schema、后追加文本或估算器改变时重新计算，超出预留上界视为预算检查失败。
5. 允许发出请求之前，对将要序列化发送的完整 payload 校验必要语义和最终估算/预留总量。校验应先于新的 `complete`/HTTP 调用；若 assembler 与 transport 分层，则最终校验必须覆盖两者所有新增内容。该路径只发送已通过检查的同一份 payload。

必要最小输入超过容量时返回稳定的“输入配置不足”原因，并沿既有可诊断错误/无动作 `Wait` 路径结束当前决定过程。诊断至少包含 profile、窗口/预留/有效容量、必要输入和最终输入的估算或上界、失败原因；不记录敏感原文。错误原因与 C0 调用次数耗尽可区分，具体编码由实现映射到既有错误体系。

不得再发起该次模型请求、自动扩大预算、静默切换 profile 或以缺反馈候选继续；此前已发生的只读查询与诊断可保留，不被宣称为回滚或 committed world effect。恢复需要明确修正输入/profile 后重新发起，不能无条件重试相同不可容纳输入。

<a id="des-builtin-parameter-consistency"></a>

### 6.4 治理约束消费关系

合法范围由 [政治数值基线](../../game/gameplay/gameplay-war-politics-mvp-baseline.design.md#des-war-politics-002) 与 Runtime 治理合同拥有；推荐参数模板不缩小合法域。builtin 的 `open_governance_proposal` 必须准确表达 `pass_threshold_bps=5000..=10000` 与 `voting_window_ticks=1..=1440`，保留端点；不支持 30% 门槛。

目标实现为这两个治理字段提供一份共享约束数据/常量，由 Runtime、parser、tool schema 和文字说明共同消费：Runtime 与 parser 读取相同边界，schema 的 minimum/maximum 和文字范围由同源值生成。共享定义归既有治理规则层维护，LLM adapter 不拥有第二份业务范围。若层级依赖要求保留派生数据，必须机械生成并用一致性 fixture 阻止漂移，不能保留无人校验的四份手写数字。

压缩 OutputSchema 时仍遵守该消费关系；同一次实际 Responses 请求中的文字与 tools 不得相互矛盾。合法范围的修复不跳过其它必填、选项、主体、权限、quorum 或 Runtime 校验，也不更改 tool-only 协议、动态 capability schema 或治理动作 ABI。

### 6.5 本次独立验证设计

以下是待实施的确定性验证计划。`builtin_input_integrity`、`builtin_governance_consistency` 是本节的验证组标识，尚不表示仓库中已有同名可执行测试；实现 PR 应把它们绑定到真实 test 名称和执行记录。使用固定候选源码、实际 profile/默认目标及离线 fake completion/HTTP transport，不调用真实模型。

<a id="ver-builtin-input-integrity"></a>

#### 输入完整性验证组

`builtin_input_integrity` 由 `qa_engineer` 在 builtin behavior/transport 层执行，方法如下：

1. 使用 Balanced/Compact × 短目标/默认目标四个组合。fake 首轮响应返回 `environment_current_observation` 调用，builtin 执行普通只读查询；在 [Responses payload builder](../../../crates/oasis7/src/simulator/llm_agent/openai_payload.rs) 之后捕获实际序列化 payload，解析第二次 `instructions/input/tools`，逐字段核对必要 observation、目标、查询状态和关键条件/缺失前置。同时覆盖可选历史被剪、最新 reply 保留；trace/History 保存或工具已执行不能代替 capture。
2. 分别构造必要 observation/reply/目标超量、无有效容量、tools/schema 预留不足和 Dialogue/guard/repair 追加超量。检查稳定配置不足原因、profile/预算分项，以及受阻请求后 completion/HTTP 调用次数为零。可选内容缩减后恰好满足容量的对照应可发出；必要内容恰好超过容量的对照应停止。
3. 为 [PromptAssembler](../../../crates/oasis7/src/simulator/llm_agent/prompt_assembly.rs) 的有界投影加入中文、嵌套 JSON、较长列表、普通查询错误和必要字段未知场景；核对确定性序列化、原始值/显式未知与省略标记，以及最新 reply 关联正确。投影无法保留必要语义时停止；首轮没有查询时不伪造 query reply。

实现 PR 保存候选 SHA、实际 profile/工具集、固定输入/预期 projection、四组合 capture、call counter、错误/Wait 断言和执行结果。这些证据不证明真实 tokenizer 的精确 token 数、模型决策质量、C0 或付费资源计量，也不新增 observation wire 字段或长期 memory 合同。

<a id="ver-builtin-parameter-consistency"></a>

#### 治理参数一致性验证组

`builtin_governance_consistency` 与输入完整性独立判定：

1. 捕获真实组装并序列化的 Responses payload，检查两字段文字范围与 tools minimum/maximum 一致，排除 `1000..=9000`、`8..=256` 及等价旧范围说明；核对生成源与 parser/Runtime 的共享约束。tool schema 正确不能代替正文断言。
2. 使用其它主体/权限/选项/quorum 均合法的固定输入；分别检查 parser 与 Runtime 的 `5000/10000`、`1/1440` 合法端点及其组合，`4999/10001`、`0/1441` 单字段越界拒绝。保留 [现有工具执行测试](../../../crates/oasis7/src/simulator/llm_agent/tests_tool_execution.rs) 的 `6500` 正例和原有正常校验负例，不以放宽 Runtime 达成提示一致。

实现 PR 保存候选 SHA、payload/schema 对账、parser/Runtime/工具路径的边界断言及实际执行记录。该组无需真实模型，不重平衡治理规则，也不证明模型会选择某个参数。

两个验证组独立完成后才支持本 builtin 输入修复的完成结论；本地确定性证据不替代既有真实 provider、full/parity、部署或发布验证。此次只收敛 §6 的兼容专业合同，未重写动态 capability、异步调度、存储或世界权限设计。

### 6.6 残余风险与复核

保留的风险是估算与模型真实计量差异、未来工具/schema 增长和投影规则变化；分别由 `agent_engineer` 在 profile/工具/后追加文本变更时复核，由 `qa_engineer` 重新运行以上针对性 fixture，不以历史样本维持通过结论。

## 11. 本次兼容合同的验证追溯

本次按系统设计写作规范的小型兼容编辑范围，只更新 §2.1、§6 与本节的承接/验证关系；无关的生命周期、部署和迁移设计继续由原章节与配对专业文档拥有。下表为 `planned`（待实施、未执行）的行为验证，不能因本次文档提交未实施或未运行而写成不适用。§6.5 的两个验证组标识不是已有同名测试；表中链接是计划方法的准确来源，不是已通过证据。实际测试名称、固定候选与输出由实现 PR 记录，现有固定源码差距与证明限制不因关系规范化改变。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-d1ea0352-7f1a-426b-907a-cf41571a1e7f；[Builtin 当前输入完整性](decision-provider-contract.prd.md#builtin-input-integrity) | [必要输入与预算流程](decision-provider-contract.design.md#des-builtin-input-integrity) | 最终请求保护当前必要输入，所有新增内容计入实际 profile 容量，配置不足不调用。 | planned；`test_tier_required`；按§6.5 输入完整性验证组；[现有 conversation fixture source](../../../crates/oasis7/src/simulator/llm_agent/tests_conversation_flow.rs) 中 `llm_prompt_profile_uses_relaxed_token_budget_for_stability` 当前仅断言 Compact/Balanced 预算，不证明输入保护；计划扩充请求捕获及负例，在固定候选的 builtin behavior/离线 fake completion/HTTP transport 层核对最终序列化 payload、实际 profile 容量与配置不足零后续调用；`agent_engineer/qa_engineer` 在实现 PR 接入真实 fixture/source 后记录执行结果，本次未实施或执行。 | 实现 PR 的 profile/工具集、最终 payload、预算分项和无调用断言。 | 文档检查不证明实现修复、真实 token 数或模型质量。 |
| Trace refs: O7-REQ-d1ea0352-7f1a-426b-907a-cf41571a1e7f；[输入完整性 AC](decision-provider-contract.prd.md#ac-builtin-input-integrity) | [输入完整性验证组](decision-provider-contract.design.md#ver-builtin-input-integrity) | 四组合第二次实际请求、必需输入不足与结构化投影边界独立验收。 | planned；`test_tier_required`；按§6.5 输入完整性验证组；[现有 conversation fixture source](../../../crates/oasis7/src/simulator/llm_agent/tests_conversation_flow.rs) 当前 profile 测试不捕获四组合第二请求，计划补充断言，在同一固定候选执行 Balanced/Compact × 短目标/默认目标四组合第二请求 capture、不足配置的错误/Wait 与零后续调用、结构化投影矩阵；`qa_engineer` 使用该节真实 builder/assembler source 和离线 fake transport，fixture 接入后记录实际测试源与输出，当前未运行。 | 实现 PR 的候选 SHA、四组合 capture、错误/Wait、调用计数与投影断言。 | 不代替真实 provider/full/parity，不声称测试已存在或通过。 |
| Trace refs: O7-REQ-be4bb896-dc4f-41d1-9f87-c381049ac190；[Builtin 参数说明一致性](decision-provider-contract.prd.md#builtin-parameter-consistency) | [治理约束消费关系](decision-provider-contract.design.md#des-builtin-parameter-consistency) | prompt/schema/parser/Runtime 共享治理约束，合法域与主体规则保持。 | planned；`test_tier_required`；按§6.5 治理参数一致性验证组；[现有工具执行 fixture source](../../../crates/oasis7/src/simulator/llm_agent/tests_tool_execution.rs) 的 `llm_agent_parse_open_governance_proposal_action` 仅证明 36/6500 正例解析，不证明四方同源；计划扩充独立对账，核对固定候选的共享来源与真实序列化 payload/schema、parser/Runtime 合法边界；`agent_engineer/runtime_engineer/qa_engineer` 建立同源消费及离线 fixture 后记录真实测试源和结果，本次代码未改、未运行。 | 实现 PR 的共享来源审读、payload/schema 对账和边界验证输出。 | 文档不证明四方代码已同源，不调整玩法数值。 |
| Trace refs: O7-REQ-be4bb896-dc4f-41d1-9f87-c381049ac190；[参数说明一致性 AC](decision-provider-contract.prd.md#ac-builtin-parameter-consistency) | [治理参数一致性验证组](decision-provider-contract.design.md#ver-builtin-parameter-consistency) | 正确正文和schema、合法端点、相邻越界、6500及其它正常校验分别证明。 | planned；`test_tier_required`；按§6.5 治理参数一致性验证组；[现有工具执行 fixture source](../../../crates/oasis7/src/simulator/llm_agent/tests_tool_execution.rs) 当前 36/6500 正例不能覆盖端点、相邻越界或 Runtime 拒绝，计划扩充断言，独立对账正文/tools schema、`5000/10000` 与 `1/1440` 合法端点及 `4999/10001` 与 `0/1441` 越界，保留该节现有测试 source 的 `6500` 正例及其它校验；`qa_engineer/runtime_engineer` 绑定同一候选和离线 fixture 输出，当前未新增或运行。 | 实现 PR 的正文/工具 schema 断言、parser/Runtime 边界与6500回归结果。 | 不以单层成功代签整体，不证明真实模型选择或治理平衡。 |
