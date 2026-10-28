# Viewer 控制面：回放与 Live 分离

> 本文是回放/Live 控制 profile 与 live 无 seek 语义的当前专业 authority。它收敛两个 2026-02 源三件套；历史变更仅从 Git、PR 与实际验证记录追溯。

- 对应设计: `doc/world-simulator/viewer/viewer-control-plane-split-live-playback.design.md`
- 历史实施、验证与 task 状态：GitHub task issue evidence。

## 目标

将 Viewer 控制语义分为 playback 与 live 两个 profile，使 live 世界只单调前进，不能把回放专属的 `seek` 误作连接失败或可回退操作。

## 范围

- 覆盖 profile、握手、控制请求路由、legacy bridge、timeline、automation 与 Web test API 的动作集合。
- 玩家动作反馈隔离、业务错误码和 CollectData 预检一致性的实现与验收要求见[动作反馈目标](#player-action-feedback-targets)；它们不改变既有 PromptControl 专属结果合同。
- 不重写世界规则、共识/P2P 语义、非 live 历史浏览、视觉方向或完整 timeline 架构。

## 接口 / 数据

- `ViewerControlProfile::{playback, live}`、`PlaybackControl`、`LiveControl`。
- `ViewerRequest::{PlaybackControl, LiveControl}` 与 `HelloAck.control_profile`。
- `window.__AW_TEST__` 的 `controlProfile`、动作描述/发送接口，以及 legacy live handler 的无回退行为。

## 当前合同

| profile | 支持控制 | 不支持控制 | 表现要求 |
| --- | --- | --- | --- |
| `playback` | `play`、`pause`、`step`、`seek` | — | timeline 与 automation 可按 playback 暴露 seek |
| `live` | `play`、`pause`、`step` | `seek` / `seek_event` | 当前 Web 控制入口在发送前以通用 unsupported-action 反馈拒绝 seek；legacy live request 收到 seek 时记录并忽略，不回退世界，也不把它报告为连接失败 |

- `HelloAck.control_profile` 是握手后的控制面真值；`ViewerRequest` 分别使用 `PlaybackControl` 与 `LiveControl`。
- 握手前可暂按 legacy 能力展示；当前 Web 入口不提供 seek 提交，未知或 live profile 的 seek 以发送前通用 unsupported-action 反馈拒绝。
- legacy `Control` 仅是兼容桥接；若旧调用方仍向 live handler 发送 seek，handler 只记录并忽略该请求，世界保持单调前进。
- live 禁止 seek 只适用于 live 服务，不改变非 live 的历史浏览/回放语义。

<a id="playback-step-progress"></a>
### 离线 Step 的进展与回执

离线 TCP playback 的 `Step` 以本次请求实际写出的 Event 数判断是否 `Advanced`。Seek 后即使
发出的事件与目标 snapshot 处于同一 tick，或 sequence 小于 Seek 前历史最大值，只要本次确实
发出事件，就不能报无进展/timeout。Seek 更新后续 Step 的位置基准；`delta_logical_time` 和
`delta_event_seq` 依真实 Seek 后基准计算，允许 delta 为零，不伪造时间推进以凑出 Advanced。
completion ACK 保留本次 `request_id`；无事件的 EOF/空 journal 按既有无进展结果返回。
重复 Seek、过滤器及首次 Step 分别验证。此合同不增加 Web Seek 入口，不改变 live 无 Seek。

<a id="viewer-client-request-lifecycle"></a>
### 客户端请求、等待和迟到结果

本节承接 [#3425](https://github.com/eng-cc/oasis7/issues/3425) R1–R6 的客户端边界；
服务器的 PromptControl identity、去重、隐藏和应用 scope 仍由下文定义。

- 每次用户意图绑定原目标、玩家/会话/连接上下文、请求身份与等待阶段。签名/注册中尚未发送
  的新操作不能被旧 ACK 终结；当前在途操作只能由可关联到它的结果改变 ID、timer、状态和历史。
- Prompt response 与 error 使用相同相关性校验；带旧 request ID 的错误不改写新请求，
  包括新请求尚在签名期、旧请求已超时和不同 Agent 的场景。真实当前 error/control_lost
  仍按安全隐藏规则处理，不能通过忽略全部错误达到“稳定”。
- AgentChat receipt 先匹配原意图及已发送请求，再结束等待或追加历史。canonical goal 的
  transport pending 与已提交的 accepted intent 继续按 [Cognition 合同](../../world-runtime/runtime/agent-cognition-lifecycle.design.md)
  区分；accepted goal 不是 Agent 已执行目标，且不扩大 Prompt profile 的持久化范围。
- quote 与普通 gameplay action 各自拥有等待状态。quote error 只结束对应 quote，不能清除
  无关 action 的 ACK timer；展示延迟不是 ACK 截止时间。断连终止旧连接 quote 的本地等待，
  新连接恢复后可重新报价；同连接普通 snapshot 不取消仍在途报价，旧连接结果不污染新报价。
- 已接纳的工业报价在消息到达后立即刷新可读结果，不等待下一次 snapshot。普通 snapshot
  不无条件清空已收到的 `power_survival` 报价；相关输入改变时明确标记 stale/失效，并保留
  解释。保留报价不表示可执行旧报价，数值与执行资格继续消费[工业资源流合同](../m4/industrial-resource-flow-contract.prd.md)。
- 注销/释放的异步结果只可清理发起该退出的会话。A 的 release 成功、HTTP 失败或网络失败
  迟到时，不清除随后登录 B 的 auth/cache/注册/刷新状态；此客户端隔离不改变服务端撤销语义。
- legacy 缺 `request_id`/seq 的响应不冒充增强相关性：正常、唯一可关联的已发送请求可沿原
  合同消费；超时重试或代次重叠导致归属不明时保留“结果未知”及权威刷新/恢复路径，不按
  “同 Agent”强行完成新操作，也不静默升级协议保证。缺字段的诊断不得被猜成某条成功回执。

验收必须走真实 handler→state→panel 的受控时序，先验证旧结果无副作用，再验证当前正确
ACK/error 仍能完成；只加双击锁、延长超时或执行全套绿色测试不替代这些断言。具体场景与
接点见[客户端生命周期设计](viewer-control-plane-split-live-playback.design.md#viewer-client-request-lifecycle-design)。

<a id="prompt-control-authority-contract"></a>
### PromptControl authority contract

- `prompt_control_result_v1` 是增强 PromptControl 结果合同的协商 capability；通过 `HelloV2` 选择，不改变全局 Viewer protocol version。只有实现完整合同并显式协商后，请求才属于 W3 证据；旧客户端继续走可兼容的 legacy 行为，但不能据此声称 W3 结果。
- 既有 serialized runtime authority 是唯一应用来源。客户端本地发送、accepted/queued、ack 到达、重连和本地 snapshot 都不等于 runtime 已应用。产品承诺与四个目标验收项见 [`Agent 对话与 Prompt 控制`](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md) 的 AC-PROMPT-004、007、009、011；本合同只规定系统边界与可观察结果。
- 配对设计中的状态图与证据示例见 [`viewer 控制面设计`](viewer-control-plane-split-live-playback.design.md)。本节以下六个锚点是本专业 authority 的可消费合同。

#### Frozen wire and runtime spellings

- capability constant 使用 `PROMPT_CONTROL_RESULT_CAPABILITY`，其 `HelloV2` serialized token 固定为 `"prompt_control_result_v1"`；`VIEWER_PROTOCOL_VERSION` 保持 `2`。
- enhanced epoch 的 bootstrap/readback wire fields 固定为 `HelloAck.authority_epoch: Option<String>` 与 `AuthoritativeRecoveryAck.binding_epoch: Option<u64>`，均 additive/default 以保持 legacy decode。选中 `prompt_control_result_v1` 的 `HelloAck` 必须提供当前 `authority_epoch`；成功且已授权的目标 Agent registration/binding ack 必须提供该 Agent 当前 `binding_epoch`。未协商 capability 的握手与旧 registration response 可省略这些字段；这不改变全局 protocol version，也不向未授权错误暴露 binding epoch。
- enhanced result 的 Rust enum 名称与 wire values 固定为 `PromptControlResultStatus::{Accepted, Applied, Blocked, Rejected, Stale}` → `accepted`、`applied`、`blocked`、`rejected`、`stale`；`PromptControlValueVisibility::{Hidden, MetadataOnly, LatestAllowed}` → `hidden`、`metadata_only`、`latest_allowed`；`PromptControlApplicationScope::{None, RuntimeInstance}` → `none`、`runtime_instance`。三个 scope 字段均使用 `PromptControlApplicationScope` 的 snake_case 序列化值。
- enhanced response/error 的 wire field names 固定为 `request_id`、`authority_epoch`、`operation`、`preview`、`status`、`agent_id`、`player_id`、`session_epoch`、`binding_epoch`、`expected_version`、`version`、`digest`、`applied_fields`、`value_visibility`、`applied_scope`、`persistence_scope`、`sync_scope`、`reason_code`、`next_step`、`operation_digest`、`idempotent_replay`、`mutation_count` 与既有 `rolled_back_to_version`；新增字段继续使用 optional/default wire compatibility。
- existing `PromptControlOperation` 与 request wire field `to_version` 保持不变；canonical operation preimage 的 `rollback_target` 是从 `to_version` 规范化得到的 typed value，不能另造第二个 rollback target 字段。
- result ledger limits 属于 `ViewerRuntimeLiveServerConfig`，字段固定为 `prompt_result_cache_capacity: usize`（default `256`）与 `prompt_result_receipt_max_bytes: usize`（default `65536`）。两者为正值才可启动/接受配置；它们不出现在 Viewer wire，也不进入 recovery persistence。

<a id="prompt-control-request-identity"></a>
### PromptControl request identity

- enhanced `preview`、`apply`、`rollback` 请求的新增 DTO 字段都必须是 additive optional/default，以保持旧 JSON 客户端可解码。协商 capability 后，`request_id`、`session_epoch`、`binding_epoch`、`expected_authority_epoch` 和非空 `expected_version` 均为必需；缺失字段返回稳定的 `prompt_control_field_required`，不产生 mutation。未协商增强 capability 的客户端不能获得增强语义。

| 字段 | enhanced 规则与合同含义 |
| --- | --- |
| `request_id: Option<String>` | 非空 opaque identity，最多 128 个 UTF-8 bytes；同一逻辑操作的 transport retry 必须复用它。 |
| `session_epoch: Option<u64>` | 当前 player session 的 authoritative revision；revoke/rotate 后递增。 |
| `binding_epoch: Option<u64>` | Agent/player/key binding revision；bind、unbind、release、transfer、force-rebind（包括同 key 变化）后递增。 |
| `expected_authority_epoch: Option<String>` | 启动时生成且不持久化的 runtime-instance epoch；变化后不得把旧 ledger 当作当前结果。 |
| `expected_version: Option<u64>` | 草稿所依据的 profile version；preview 也必须携带，null 不表示 latest。 |
| operation payload | 既有 patch/rollback 字段；先按既有 trim/empty 规则规范化，mutating apply/rollback 的 `updated_by` 必须等于 `player_id`。 |

- player proof 必须覆盖完整 enhanced operation：`request_id`、Agent/player identity、以上三个 epoch、`expected_version`、operation/mode、`preview`、规范化 patch、rollback target 和 `updated_by`。hosted strong-auth grant 必须由真实 backend 签发并继续绑定 action/player/key/Agent；它不替代 player proof。
- enhanced client 必须先从选中 capability 的 `HelloAck.authority_epoch` 捕获当前 runtime namespace，再从针对目标 Agent 的成功授权 `AuthoritativeRecoveryAck.binding_epoch` 捕获当前 binding revision；客户端不得猜测或从本地 snapshot/ack arrival 推导任一 epoch。重启以新的 authority epoch 使旧 request 失效，rebind/bind transition 以新的 binding epoch 使旧 binding identity 失效；缺失 bootstrap/readback 时不能构造 W3 enhanced request。
- `binding_epoch` 是 authority 内按 Agent 维护的 in-memory monotonic revision，并与 `authority_epoch` 成对解释。新 server instance 生成新的 `authority_epoch`、空 ledger 与空 binding-revision map；首次观察到的 Agent 无 revision 时为 `binding_epoch=0`，首次成功建立绑定为 `1`，之后每次成功 bind、unbind、release、transfer 或 force-rebind（包括同 key 变化）递增一次；unbind/release 后即使当前无绑定，也保留该 Agent 的最后 revision 作为 in-memory tombstone。重启后计数器可从 `0` 重新开始，因为旧请求首先因 `expected_authority_epoch` 不匹配而得到 hidden `blocked/result_unknown`，不存在跨 instance 的 epoch 比较或恢复持久化。
- 支持 backend registration 的同一 session-mutation rollback snapshot 必须同时保存 binding maps 与 binding-revision map。若 registration persistence 失败，必须把 session、binding、key map 与 revision counter 全部恢复到提交前值；失败尝试不得留下 binding epoch advance。`recovery_receipt.rs` 与 prompt profile/result ledger durability 均不因本合同扩展。
- `operation_digest` 是 typed semantic operation 的 canonical digest，而不是 raw JSON：

```text
operation_digest = h_v1(
  "oasis7.viewer.prompt-control.operation.v1",
  PromptControlOperationIdentityV1 {
    operation, preview, agent_id, player_id,
    session_epoch, binding_epoch, expected_authority_epoch,
    expected_version,
    system_prompt: PatchField::{unchanged|clear|set(utf8_string)},
    short_term_goal: PatchField::{unchanged|clear|set(utf8_string)},
    long_term_goal: PatchField::{unchanged|clear|set(utf8_string)},
    rollback_target: Option<u64>, updated_by
  }
)
```

  `h_v1` 使用仓库已有 domain-separated canonical-CBOR `[domain,payload]` 与 BLAKE3-256；字段顺序、UTF-8 bytes、trim/empty normalization 以及 unchanged/clear/set 的类型区分都固定。auth nonce/signature 与 grant transport timestamps 不进入 digest，`request_id` 作为 logical-operation key 也不进入 digest。当前签名 consumer 见 `crates/oasis7_client_api/src/viewer_auth.rs:124-148,834-872`，浏览器构造入口见 `crates/oasis7_viewer/software_safe_src/legacy_core.js:1649-1692`。

<a id="prompt-control-result-status"></a>
### PromptControl result status

- enhanced result 的 `status` 只能是 `accepted`、`applied`、`blocked`、`rejected` 或 `stale`；可解析请求应 echo `request_id`，并按 authority 结果提供 `authority_epoch`、身份/epoch、版本/摘要、`value_visibility`、scope tuple、`reason_code`/`next_step`、`operation_digest`、`idempotent_replay` 与 `mutation_count`。
- `accepted` 可表示无 mutation 的 Preview 或 authenticated verified no-op；AC-PROMPT-004 的 Preview 必须明确 `preview=true`，scope 全为 `none`，并以 profile/event count 证明未 mutation。`applied` 只表示 authority 已 commit，并携带 `mutation_count=1` 与 runtime-instance scope。`blocked` 表示 authority 未应用；`auth_required`、`auth_invalid`、`control_lost`、`result_unknown` 是稳定 reason code，并按隐藏策略返回；缺失/无效 proof 属于 hidden blocked/auth-control class，不能仅因 proof malformed 改成 `rejected`。`stale` 表示控制仍有效但 commit 时 version/digest 不匹配；旧 patch 不应用。可解析 request ID 的 malformed/unsupported/unknown-target/request-ID conflict 是 `rejected`，不能 silent retry；没有可解析 request ID 的 malformed request 只能返回 uncorrelated protocol error，不属于 W3 evidence。
- `value_visibility` 由 authority 设置为 `hidden`、`metadata_only` 或 `latest_allowed`，不能由 client 选择。`operation_digest` 在结果可见性允许时必须出现在 wire result；`value_visibility=hidden`（包括 `blocked/control_lost` 与 redacted `blocked/result_unknown`）必须省略 wire digest，只能保留在 authorized runtime trace。browser error 不得暴露该 trace。

<a id="prompt-control-control-loss-precedence"></a>
### PromptControl control-loss precedence

每个 enhanced operation 都由既有 serialized runtime authority 按以下顺序评估：解析 operation 与 request ID；验证 player proof；校验 session key/`session_epoch`；校验 Agent/player/key binding/`binding_epoch`；校验 `expected_authority_epoch`；查询 bounded result ledger；消费 fresh auth nonce 并校验 grant/operation；最后在 commit 前校验 `expected_version` 与当前 digest。可解析 request ID 的 malformed/unsupported operation 返回 `rejected`；缺失或无效 player proof 属于 hidden blocked/auth-control class，wire status 保持 `blocked`，reason code 分别为 `auth_required`/`auth_invalid`，不得仅因 proof malformed 就改成 `rejected`；session 或 binding 丢失返回 hidden `blocked/control_lost`；runtime epoch 变化返回 redacted `blocked/result_unknown`。这些 auth/control blockers 均先于 expected-version 检查，控制丢失优先于值变化。

`blocked/auth_required` 与 `blocked/auth_invalid` 使用同一 auth privacy boundary：仅保留可安全关联的 `request_id`（可解析时）、已知 `agent_id`、`status=blocked`、对应 reason code、generic re-auth `next_step` 与可用的 `authority_epoch`；必须 `value_visibility=hidden`，省略 player/session/binding/version/profile/diff/applied fields、digest、persistence/sync scope 与 retry/transfer action，且不得消费 nonce 或 mutation。`blocked/control_lost` 仅保留可安全关联的 `request_id`（可解析时）、已知 `agent_id`、`status=blocked`、`reason_code=control_lost`、generic re-auth/rebind `next_step` 与可用的 `authority_epoch`；必须 `value_visibility=hidden`，省略 player/session/binding/version/profile/diff/applied fields、digest、persistence/sync scope 与 retry/transfer action，且不得 mutation。`blocked/result_unknown` 同样 redacted、`value_visibility=hidden`，省略 wire `operation_digest`，不得 retry 或 mutation；authorized runtime trace 可保留 digest。

<a id="prompt-control-race-and-idempotency"></a>
### PromptControl race and idempotency

- AC-PROMPT-009 必须在 evidence 中区分两种真实并发：同一 expected version 与有效 binding/session epochs 的两次 authorized request 产生一个 `applied` 和一个 `stale`；若 force-rebind/revoke 在 A 的 revalidation/commit 前由 B 提交，A 必须产生一个 hidden `blocked/control_lost`，即使 A 的 expected version 也旧。两者都必须以 runtime trace 证明 baseline、lock/commit order 和 browser/result correlation。
- 不增加 production barrier、direct sidecar mutation、hidden scheduler mode 或 unauthenticated race endpoint。两名真实 authorized clients 与 runtime lock/serialization 是默认路径；如需确定性调度，只能使用 runner-side 或明确 test-only instrumentation。
- mutation idempotency key 固定为 `(authority_epoch, authenticated_player_id, request_id)`。runtime-only ledger 使用 fixed-capacity、non-evicting 配置 `prompt_result_cache_capacity=256` 与 `prompt_result_receipt_max_bytes=65536`；zero/invalid 配置 fail closed，禁止 LRU/silent eviction。新请求在 cache full 时返回 `blocked/result_cache_full`，oversize receipt 返回 `rejected/result_receipt_too_large`，两者都在 nonce consumption/mutation 前发生。
- cache lookup 必须在 cryptographic verification、principal/session/binding/epoch checks 后、auth nonce consumption 前进行。同 key 同 digest replay 原结果并标记 `idempotent_replay=true`，不同 digest 返回 `rejected/request_id_conflict`；session/binding loss 必须先于 cached success redaction。runtime restart 生成新 authority epoch 与空 ledger，旧 retry 返回 hidden `blocked/result_unknown`，不重新执行。

<a id="prompt-control-application-scope"></a>
### PromptControl application scope

- W3 的 actual apply 只承诺 `applied_scope=runtime_instance`，`persistence_scope=none`，`sync_scope=none`。既有 sidecar map、driver update、virtual `AgentPromptUpdated` event 与同 instance snapshot projection 可以保留，但不能把它们解释为 durable recovery、cross-session replication 或 cross-entry sync。
- AC-PROMPT-011 的 `applied` 必须来自 authority result；Viewer 不得从 ack arrival、`applyPromptAckLocally`、local snapshot、reconnect 或 reload 推导更大 scope。AC-PROMPT-004 Preview 与所有 blocked/rejected/stale 结果不得产生 profile/event mutation。
- Recovery-generation Prompt persistence、durable result receipts、跨 session replication 与 production-supervisor activation 不属于本合同；这些需要新的 system authority 与 acceptance set。实现入口仍分别由 protocol/runtime、Agent/runner、Viewer 与 QA leaf 消费，不能在本 system leaf 合并。

### 观测字段与控制完成边界

- live drive 只由一个可处理的 mailbox/runtime 事件触发；一次触发至多形成一次 runtime drive。空 mailbox 或零结果保持静默，不能合成事件、逻辑时间、完成态或进度；之后到达的有效触发仍可继续推进。
- `getState().logicalTime` 是当前 Web surface 的逻辑时间观测值；`eventSeq` 是独立的事件顺序观测值。两者都只能由收到的 runtime 消息推进，不能由客户端控制请求、空 mailbox 或重连计时器自行递增。
- `tick` 只作为 `logicalTime` 的兼容 alias 保留；它不是 browser polling、mailbox drive 或“每个 step 必有进展”的承诺。新自动化优先读取 `logicalTime` 与 `eventSeq`。
- `sendControl` 的 accepted/queued 只表示客户端已通过本地校验并已尝试发送。只有后续的 runtime snapshot/event 或 completion feedback 才能标记观察到的增量；无增量不得伪造 event。
- `seek_event` 与 `seek` 一样不是 live 控制动作。playback 可按 profile 暴露 seek；live 必须在发送前返回 unsupported-action，而非以断链、重连或成功替代该结果。
- `prompt_control`、`agent_chat`、目标重排和记忆纠正的本地发送、accepted/queued 同样不等于 runtime 已应用或已产生世界后果。Viewer 只能按 canonical snapshot/feedback 呈现 accepted/applied/rejected/blocked、原因、影响范围和可执行恢复入口；不得从客户端状态、重连或 `logicalTime`/`eventSeq` 推导这些结果。玩家因果与恢复合同见 [`间接控制 agency 合同`](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md)，Prompt 结果语义见 [`Agent 对话与 Prompt 控制`](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md) 与 [`Viewer 手册`](viewer-manual.manual.md)。
- live 的 `seek` / `seek_event` 不支持不是玩家纠正、重排或恢复机制。纠正、重排和续玩只能走权威的 Prompt/Agent feedback、`reconnect_sync` / resume anchor 或明确的 reprioritize flow，不能通过本地回退逻辑时间、事件序号或重放 Viewer snapshot 实现。记忆驱动行动时，Viewer 仅呈现脱敏的摘要、来源、当前用途、staleness 和 correction outcome；不得暴露私有 prompt、内部 trace，也不得自造记忆真值。

## 验证与边界

<a id="viewer-infrastructure-readback-contract"></a>
### Infrastructure result readback receiver

本 receiver 只承接以下产品义务的 Viewer 授权读投影，不取得 runtime、共识或玩法裁定权：

- Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[REQ-DWE-002](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002)：区分尚无效果待决与已提交结果，恢复后仅呈现真实处置；不以 ACK、重连或本地样本代替世界效果。
- Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005)：呈现服务等级、受影响操作、最后可信边界、blocker 与真实下一步；不由 Viewer 自行开放写入。
- Trace refs: O7-REQ-4792541c-aaa4-4ac1-9a2d-374df44eba6a；[SC-9](../../product/world-infrastructure/prd.md#sc-infra-009)：按既有八格与 specimens 投影同源结果；读取故障不改变世界效果、W 或历史，玩法 W/window 仍由原 authority 裁定。
- Trace refs: O7-REQ-2b7cfef9-d917-46ea-a3a9-e3a4fb96d730；[SC-10](../../product/world-infrastructure/prd.md#sc-infra-010)：读面保持目标世界及 intent/receipt/result 的身份边界，不把 local 结果表述为 global；不替代 runtime 身份校验。

这些精确产品关系不证明任一要求已完整设计、实现或验证；完整同候选的专业与消费者证据仍按下述 receiver 合同独立判定。

- 基础设施结果由既有 authority 提供；Viewer 只消费同 candidate、world identity、root/revision、child/direct-parent receipt 与 disposition 的授权只读投影。按 [QA source/scene method](../../testing/prd.md#qa-infrastructure-source-scene-method) 和 [Harness consumer matrix](../llm/continuous-agent-harness.prd.md#harness-infrastructure-consumer-matrix) 核对，连接成功、ACK、queued/accepted、本地 snapshot 或 debug 样本不能替代 finality、completion 或世界效果。
- freshness 必须关联同证据窗口的权威 logical position/source/readback；browser polling、`logicalTime`/`eventSeq` 单独不能证明 fresh result。pending、read outage、rejected、stale、timeout/expiry 保持 no-new-effect；错误与恢复入口仅呈现已授权 disposition，不暴露私有 prompt/trace。fresh recovery、retry/replay 须与原 history/receipt 关联，不能隐藏重提交或增加第二次 effect。
- SC9 投影须消费 [QA scene families](../../testing/prd.md#qa-infrastructure-scene-families) 的全部八格及 authority 格内 root-absent pre-finality/root-present post-root-pre-child 两个 specimens；quantities/buckets/window/lease/W/progression/next_action/next_recheck 与 Runtime/API/Agent 的同源 oracle 一致。read outage 只改变观察，不能重置 W/history；canonical gameplay interruption 才按其规则改变它们。
- 此 receiver 不扩大 PromptControl `runtime_instance/none/none`，不定义新 UI、DTO、API 或控制能力。验证按 [QA tier boundaries](../../testing/prd.md#qa-infrastructure-tier-boundaries) 与 [capture method](../../testing/manual/web-ui-playwright-closure-manual.design.md#playwright-source-capture-method) 关联 authority oracle；missing proof 保持 blocked/unverified，实际 invariant violation 为 failed。shape/source presence 与历史截图不证明 semantic/full success；[equal-version negative](../../testing/prd.md#qa-infrastructure-equal-version-negative) 保持 planned/unexecuted。

- 不得把 live seek 的发送前拒绝或 legacy handler 的记录并忽略误报为断链、发送成功或世界回退；若未来引入 profile-specific dispatch 结果，必须同时实现并测试所有保留的 live 请求路径。
- 断链、被 gameplay gate 阻断、无可观察进展和不支持的 profile action 是不同状态；实现与 automation 不得将其中任一状态折叠为 fabricated event 或连接成功。
- 定向验证必须覆盖“空 mailbox 不输出、不推进观测值”以及“其后的有效触发仍能继续 drive”；Viewer 的 event drive 不拥有 node/consensus tick。
- Viewer UI、timeline、automation 与 `window.__AW_TEST__` 的 `getState`、`describeControls`、`fillControlExample`、`sendControl` 必须对齐 `controlProfile` 与支持集合。
- 受影响实现改动应覆盖协议 round-trip、live/playback handler、发送路由和 Web test API；可见 UI 改动按 `testing-manual.md` S6 提供 browser/console/语义证据。历史文档迁移本身不产生 browser 证据，后续实现按实际候选单独验证。

<a id="player-action-feedback-targets"></a>
## 玩家动作反馈与预检目标

本节承接 [#2116](https://github.com/eng-cc/oasis7/issues/2116) 中 #2277、#2481 的有效要求。源码审读基线为 `fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e`（2026-10-12）：请求拒绝仍写入共享反馈，兼容提交接口仍折叠 builder 错误码；CollectData 预检与 world-service 提交使用不同的资格依据。以下是本次修复必须满足的合同；实现与实际验证分别记录，不由历史 PR 合入或旧测试存在推导验收完成。

产品结果由[玩家可读表面与反馈连续性](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)拥有；技术设计与逐项验证见[动作反馈设计](viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation)。优先处理反馈正确性与错误可诊断性；权限一致性的实际可达状态按当前 world-service 路径验证。

<a id="player-scoped-gameplay-feedback"></a>
### 请求反馈的归属

- `GameplayAction` 与 `CollectData` 的未鉴权拒绝只返回请求连接，不写入其他玩家可读的最近反馈。请求自报的 player/Agent、已有连接标签和目标 Agent 都不能独自证明反馈归属。
- 已鉴权请求的反馈只能进入已验证玩家的可见范围；玩家 A、玩家 B、匿名连接交叉请求时，彼此的操作结果不被覆盖。身份切换、rebind、延迟结果与断线后的回读继续校验归属。
- 世界级运行故障保留其共享、脱敏的运行状态语义，不能伪装成某位玩家的动作被拒绝。对已收到但结果未知的提交，继续呈现未知/待确认，不能凭超时写成最终失败或自动重试。
- 验收覆盖请求错误与异步结果；当前单客户端测试中“匿名失败必须留在全局反馈”的断言应被隔离断言替代。

<a id="gameplay-error-code-contract"></a>
### 业务错误码保真

- 验签成功后，typed gameplay builder 的错误码与安全错误信息应保持至接口响应；JSON/请求结构无法解析仍可用 `invalid_request`，鉴权失败继续遵守原有隐私和状态码边界。
- Viewer 与接口消费者按稳定 code 分类，未知 code 使用安全通用提示。不得把业务拒绝改报为连接中断、执行成功或必然可重试。
- 验收至少包含合法签名搭配未知 action、错误的非空 target 和缺少必需 actor 的请求，确认具体 builder code 未被传输适配层抹掉；签名规范化已拒绝的空 target 仍按鉴权边界处理。

<a id="collect-data-preflight-contract"></a>
### CollectData 预检与提交资格

- 对同一世界、权威状态版本、已验证 player/key 和同一业务请求，预检与提交使用同一 canonical Agent 归属及动作资格规则。canonical claim 缺失、重复或与有效绑定不一致时，预检不能给出可执行承诺。
- 新的共享资格检查只读，不扣资源、不创建 claim、不消费动作 nonce。预检请求本身的签名、session nonce 和重放保护仍遵守既有认证规则，不由此承诺整个请求没有认证状态变化。
- 预检只是当时的资格和资源判断，不预留资源，不替代提交时重新鉴权与校验；状态变化、权限撤销、余额变化均可使随后提交被拒绝。收到提交 ACK 仍只表示收到请求，世界效果由权威结果证明。
- 验收固定同一权威状态，覆盖零/一/多份 claim、player/key 不匹配、余额不足，以及预检后状态变化。普通本地 Minimal fixture 不代替 world-service 入口的一致性验证；仍受支持的兼容 HTTP 接口按实际消费者覆盖。

## 里程碑

- M1（已完成）：协议、握手与 server/live handler 按 profile 拆分。
- M2（已完成）：Viewer 路由、Web test API、automation 与 timeline 对齐 live 无 seek。
- M3（已完成）：已建立稳定 authority、切换入口并退役已吸收源三件套。

## 风险

- 若客户端忽略 profile，可能把不支持的 seek 误报为断链或发送成功；当前文档必须保留 Web 发送前拒绝与 legacy handler 忽略的区别。
- `seek` 保留为非 live 兼容枚举，不能被误解为 live 能力。
- 文档迁移不验证 UI 行为；未来任何可见控制面改动仍需单独 S6 取证。

## 追溯

当前实现入口包括 `crates/oasis7_proto/src/viewer.rs`、`crates/oasis7/src/viewer/{protocol.rs,server.rs,live_controls.rs,runtime_live.rs}`，以及 `crates/oasis7_viewer/software_safe_src/{legacy_core.js,legacy_core_control_gate.test.js}`；生成的浏览器入口为 `crates/oasis7_viewer/viewer.js`。操作入口见 `viewer-manual.manual.md`。
