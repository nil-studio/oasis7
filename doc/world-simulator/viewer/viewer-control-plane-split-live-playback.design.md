# Viewer 控制面：回放与 Live 分离设计

> 对应需求: `doc/world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md`
> 历史实施、验证与任务状态：实际 PR、CI 与评审记录；已有 Issue 保留作任务证据，Issue 不作强制前置。

## 结构

1. **协议与握手层**：`ViewerControlProfile`、`PlaybackControl`、`LiveControl` 与 `HelloAck.control_profile` 让客户端知道服务端的动作集合。
2. **服务端与路由层**：playback 和 live 各执行所属控制；legacy `Control` 只桥接兼容请求。现有 legacy live handler 收到 seek 时记录并忽略，保持世界不回退；这不是 profile-specific 结构化响应。
3. **表现层**：timeline、egui、Web test API 与 automation 不把 live seek 暴露为可提交动作；当前 Web 控制入口对 seek 在发送前给出通用 unsupported-action 反馈，不能以“重连后再试”掩盖语义不支持。
4. **兼容层**：profile 未知时可临时显示 legacy 动作集合；若旧调用方仍发送 seek，live handler 的 log-and-ignore 行为必须被视为兼容性边界，而不是成功、断链或回退。
5. **观测层**：`logicalTime` 与 `eventSeq` 分别表达已观察的逻辑时间和事件顺序；`tick` 仅为前者的兼容 alias。控制请求、重连计时器与空 mailbox 不得合成任一观测增量。
6. **PromptControl authority 层**：增强 PromptControl 通过 negotiated capability 暴露 request identity、authoritative result、control-loss precedence、race/idempotency 与 runtime-instance application scope；它不改变 playback/live profile，也不新增 persistence/sync。

## PromptControl system contract

配对 PRD 的六个锚点是本 PromptControl system contract 的技术合同入口：[`authority contract`](viewer-control-plane-split-live-playback.prd.md#prompt-control-authority-contract)、[`request identity`](viewer-control-plane-split-live-playback.prd.md#prompt-control-request-identity)、[`result status`](viewer-control-plane-split-live-playback.prd.md#prompt-control-result-status)、[`control-loss precedence`](viewer-control-plane-split-live-playback.prd.md#prompt-control-control-loss-precedence)、[`race and idempotency`](viewer-control-plane-split-live-playback.prd.md#prompt-control-race-and-idempotency)、[`application scope`](viewer-control-plane-split-live-playback.prd.md#prompt-control-application-scope)。PRD 锚点优先于本设计中的说明；产品承诺仍由 [`Agent 对话与 Prompt 控制`](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md) 负责。

### Implementable spellings and recovery boundary

capability constant 固定为 `PROMPT_CONTROL_RESULT_CAPABILITY`，serialized token 为 `prompt_control_result_v1`，全局 `VIEWER_PROTOCOL_VERSION` 仍为 `2`。选中 capability 的 `HelloAck.authority_epoch: Option<String>` 提供 runtime namespace，成功且已授权的 `AuthoritativeRecoveryAck.binding_epoch: Option<u64>` 提供目标 Agent binding revision；两者都是 additive/default，legacy/未协商路径可省略，未授权错误不得暴露 binding epoch。实现 leaf 使用 `PromptControlResultStatus`、`PromptControlValueVisibility` 与 `PromptControlApplicationScope` 三个 `snake_case` serializable enum；对应 values 分别是 `accepted|applied|blocked|rejected|stale`、`hidden|metadata_only|latest_allowed` 与 `none|runtime_instance`。enhanced response/error 的字段名固定为 `request_id`、`authority_epoch`、`operation`、`preview`、`status`、`agent_id`、`player_id`、`session_epoch`、`binding_epoch`、`expected_version`、`version`、`digest`、`applied_fields`、`value_visibility`、`applied_scope`、`persistence_scope`、`sync_scope`、`reason_code`、`next_step`、`operation_digest`、`idempotent_replay`、`mutation_count` 和 `rolled_back_to_version`；新增字段仍为 optional/default。rollback wire 的 `to_version` 规范化为 digest preimage 的 `rollback_target`。

result ledger limits 固定在 `ViewerRuntimeLiveServerConfig` 的 `prompt_result_cache_capacity: usize = 256` 与 `prompt_result_receipt_max_bytes: usize = 65536`；无效零值拒绝启动/配置，字段不进入 Viewer wire 或 recovery persistence。`binding_epoch` 是按 Agent 的 runtime-only monotonic revision，命名空间属于当前 `authority_epoch`：新 instance 从空 map/`0` 开始，首次成功绑定为 `1`，每个成功 bind、unbind、release、transfer、force-rebind（含同 key）递增；unbind/release 后即使当前无绑定，也保留该 Agent 的最后 revision 作为 in-memory tombstone。旧 instance 请求先因 `expected_authority_epoch` 不匹配得到 hidden `blocked/result_unknown`，所以 restart 不需要持久化或重建 binding counter。

registration 使用既有 session-mutation rollback snapshot 时，snapshot 必须覆盖 binding maps 与 binding-revision map；persistence failure 恢复所有这些值，不得留下 revision advance。该恢复约束不修改 `recovery_receipt.rs`，不把 prompt profile、result ledger 或 authority epoch 加入持久化。

### 状态图与请求关联

`HelloV2` 仅在 runtime 支持完整合同时选择 additive capability `prompt_control_result_v1`。增强 `preview`、`apply`、`rollback` 请求携带 client-created `request_id`、`session_epoch`、`binding_epoch`、`expected_authority_epoch` 与 `expected_version`；这些字段与完整规范化 operation 一起进入 player proof。`operation_digest` 对 typed semantic operation 使用固定 domain-separated canonical-CBOR/BLAKE3-256 preimage，`request_id` 只作为 ledger key，不进入 digest。

authority 先解析 operation/request ID，再验证 proof、session/binding/epoch，之后查 fixed-capacity result ledger、消费 nonce、检查 expected version，最后 serialized commit。客户端先从 selected `HelloAck.authority_epoch`、再从成功授权 registration/binding `AuthoritativeRecoveryAck.binding_epoch` 取得 enhanced request 的两个 runtime values，不得本地猜测。可解析 request ID 的 structural malformed、unsupported、unknown-target 或 request-ID conflict 是 `rejected`；没有可解析 request ID 的 malformed request 只能返回 uncorrelated protocol error，不属于 W3 evidence。缺失或无效 proof 属于 hidden blocked/auth-control class，wire status 保持 `blocked`，reason code 为 `auth_required`/`auth_invalid`；session/binding loss 是 hidden `blocked/control_lost`，runtime epoch mismatch 是 redacted `blocked/result_unknown`。这些 auth/control blockers 都先于 expected-version 检查。有效 control 下的同 baseline 并发形成一个 `applied` 与一个 `stale`；control-loss race 形成 hidden `blocked/control_lost`，优先于旧 version。same-key retry replay 原结果且不重复 mutation；不同 digest 的同 request ID 返回 conflict；runtime restart 后旧 retry 为 hidden `blocked/result_unknown`，不重放。

### 结果可见性与应用边界

`accepted`、`applied`、`blocked`、`rejected`、`stale` 是唯一 enhanced statuses。`operation_digest` 在 visibility 允许时返回；`value_visibility=hidden` 的 `blocked/auth_required`、`blocked/auth_invalid`、`blocked/control_lost` 与 redacted `blocked/result_unknown` 省略 wire digest，仅授权 runtime trace 可保留。Preview 必须证明无 mutation；actual apply 的 scope tuple 固定为 `runtime_instance`、`none`、`none`（apply、persistence、sync），Viewer 不得由 ack、local snapshot、reconnect 或 reload 推导更大后果。

该合同只覆盖 AC-PROMPT-004、AC-PROMPT-007、AC-PROMPT-009 与 AC-PROMPT-011 的系统边界。它不引入 durable Prompt recovery、cross-session replication、cross-entry sync、production-supervisor activation 或新的 race endpoint。实现与证据分别由 protocol/runtime、Agent/runner、Viewer、QA leaf 消费，并在 W2 trusted merge 后才可发布 system leaf。

<a id="viewer-infrastructure-evidence-join"></a>
### Infrastructure result visibility and evidence join

既有 authority result 经授权 read projection 才形成 Viewer observation；基础设施 result/freshness/error/application boundary 消费 [PRD readback receiver](viewer-control-plane-split-live-playback.prd.md#viewer-infrastructure-readback-contract)、[QA evidence method](../../testing/design.md#qa-infrastructure-evidence-method) 与 [Harness evidence join](../llm/continuous-agent-harness.design.md#harness-infrastructure-evidence-join)。冻结同 candidate/world/root/revision/child/direct-parent receipt、provider/environment 和 evidence window，capture before/outage/fresh/recovery/retry/replay 的可见结果，再使用同候选 authority source/journal/API oracle 比较 disposition、freshness、no-effect、history/W 和 effect count，归档差异/artifact/hash，最后由 QA 评估 required/full。Viewer 不自造 receipt、freshness 或完成态。

read outage 只改变可见性，不改变权威 W/history/effects；retry/replay 不能产生第二次 effect。SC9 保留八格、authority 格内 root-absent pre-finality 与 root-present post-root-pre-child specimens，并核对 Runtime/API/Agent 相同 quantities/buckets/window/lease/W/progression/next_action/next_recheck。错误/恢复只呈现授权 disposition，私有 trace 不进入 browser。PromptControl scope、profile/wire、race/privacy 与 no-live-seek 合同保持原状；此方法不新增 UI/DTO/API、持久化、同步或 runtime 算法。

代码接点仍为下列既有 protocol/runtime/Viewer consumers；文档方法不证明它们已执行验证。[Playwright capture method](../../testing/manual/web-ui-playwright-closure-manual.design.md#playwright-source-capture-method) 提供 source/scene/environment/oracle/artifact 接点；[QA tier boundaries](../../testing/prd.md#qa-infrastructure-tier-boundaries) 保留 full 所需实际 provider-backed Agent parity/external headed desktop+narrow。missing proof 为 blocked/unverified，观察到 invariant violation 为 failed；[equal-version negative](../../testing/prd.md#qa-infrastructure-equal-version-negative) 仍 planned/unexecuted，shape-valid 不等于 semantic success。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| [Viewer readback](viewer-control-plane-split-live-playback.prd.md#viewer-infrastructure-readback-contract) | 同世界 result/freshness/error/read projection 与同候选 authority join | [Evidence join](viewer-control-plane-split-live-playback.design.md#viewer-infrastructure-evidence-join) | Runtime world/journal；Agent cognition；QA verdict；gameplay W/window oracle | 不新增 UI/DTO/API，不扩大 PromptControl scope，不证明执行/full |
| Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；[REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)| 请求方得到可归属反馈，其他玩家和新身份不受污染 | [隔离设计](viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation) | viewer_engineer；runtime_engineer；既有 session policy、world-service correlation | 不扩展 PromptControl 同步或持久化，不声称多人验证完成；专业合同：[反馈归属](viewer-control-plane-split-live-playback.prd.md#player-scoped-gameplay-feedback) |
| Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4；[REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)| 合法鉴权后的业务拒绝保留 code，消费者可给出安全下一步 | [错误码设计](viewer-control-plane-split-live-playback.design.md#gameplay-error-code-preservation) | runtime_engineer；typed builder、兼容 HTTP consumer | 不改变鉴权次序、HTTP 状态类别或错误隐私边界；专业合同：[错误码合同](viewer-control-plane-split-live-playback.prd.md#gameplay-error-code-contract) |
| Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d；[REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005)| 同一权威状态的 CollectData collector/资格一致，提交重新核验 | [预检设计](viewer-control-plane-split-live-playback.design.md#collect-data-preflight-consistency) | runtime_engineer；world-service owner-read 与 gameplay authority | 不把预检变成授权、资源预留或执行成功保证，不扩大本地模式前置；专业合同：[预检合同](viewer-control-plane-split-live-playback.prd.md#collect-data-preflight-contract) |
| Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111；[REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004) | authority accepted 与 applied 的区分 | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | protocol/runtime、Agent、Viewer、QA；沿用本节六个专业合同入口 | 不证明各入口完整 UI 消费、实际实现或跨会话持久性 |
| Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd；[REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007) | control loss 优先、hidden 值与重新校验 | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | protocol/runtime、Agent、Viewer、QA；沿用本节六个专业合同入口 | 不证明各入口完整 UI 消费、实际实现或跨会话持久性 |
| Trace refs: O7-REQ-92124092-6021-49e4-8def-33038e20b637；[REQ-PROMPT-009](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-009) | 当前身份/版本再校验、竞态和同键幂等 | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | protocol/runtime、Agent、Viewer、QA；沿用本节六个专业合同入口 | 不证明各入口完整 UI 消费、实际实现或跨会话持久性 |
| Trace refs: O7-REQ-36cc9e9d-eaa7-436f-b16b-6f6af089b04c；[REQ-PROMPT-011](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-011) | runtime_instance 应用，不推导持久化或同步 | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | protocol/runtime、Agent、Viewer、QA；沿用本节六个专业合同入口 | 不证明各入口完整 UI 消费、实际实现或跨会话持久性 |

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [Viewer readback](viewer-control-plane-split-live-playback.prd.md#viewer-infrastructure-readback-contract) | [Evidence join](viewer-control-plane-split-live-playback.design.md#viewer-infrastructure-evidence-join) | 同 candidate/world/receipt result/freshness/error 与 no-effect/read projection | [Viewer capture receiver](viewer-manual.manual.md#viewer-infrastructure-capture-receiver)：before/outage/fresh/recovery/retry/replay capture，关联同候选 authority oracle，按实际 provider/environment 和 S6 desktop+narrow 要求归档 | Future screenshots/console/state、source/journal/API readback、receipt lineage、freshness、count0/1 与 artifact hashes | 文档方法；无 provider/browser/runtime 执行，无 semantic/required/full pass |
| Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；[REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)| [隔离设计](viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation) | 匿名、A/B、同玩家多连接及身份切换后的反馈归属 | 在 [auth_actions_feedback.rs](../../../crates/oasis7/src/viewer/runtime_live/tests/auth_actions_feedback.rs) 更新/补充独立 session 用例；同候选测试迟到结果、撤销、轮换、公共故障恢复，检查每份 snapshot 和原 correlation；CollectData 同类用例位于 `auth_actions_collect_data.rs` | 实现 PR 的候选、测试输出与 A/B/匿名前后反馈；可见行为按 S6 补浏览器证据 | 回归必须检出旧共享写入及重放越权；真实多人 E2E 需另行执行；专业合同：[反馈归属](viewer-control-plane-split-live-playback.prd.md#player-scoped-gameplay-feedback) |
| Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4；[REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)| [错误码设计](viewer-control-plane-split-live-playback.design.md#gameplay-error-code-preservation) | 合法鉴权、非法 action/target/actor 返回具体 builder code | 按 [测试手册 S3](../../../testing-manual.md) 运行 `oasis7_chain_runtime` 的 `gameplay_submit_api_tests`：合法签名断言 400 与精确 code，保留解析失败、鉴权失败、nonce replay 和成功提交样例 | 实现 PR 对实际 HTTP response 的断言、候选与结果 | 仅测非法签名不足以证明 builder code 保真；部署环境验收与单元结果分别记录；专业合同：[错误码合同](viewer-control-plane-split-live-playback.prd.md#gameplay-error-code-contract) |
| Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d；[REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005)| [预检设计](viewer-control-plane-split-live-playback.design.md#collect-data-preflight-consistency) | 同一权威 projection 的零/一/多 claim 与提交资格一致 | 在 [auth_actions_collect_data.rs](../../../crates/oasis7/src/viewer/runtime_live/tests/auth_actions_collect_data.rs) 运行下述矩阵并覆盖 `world_service/gameplay.rs` 的共享资格逻辑；显式启用 world-service，固定同一 projection/version；各请求使用其自己的合法认证 nonce | 实现 PR 的 canonical collector/reason、CommitRef、前后世界资源和 nonce 检查、入口结果 | 普通 Minimal 本地测试不代替 service 路径；服务测试不代替真实多人试玩，也不承诺预检后未来状态不变；专业合同：[预检合同](viewer-control-plane-split-live-playback.prd.md#collect-data-preflight-contract) |
| Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111；[REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004) | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | authority accepted 与 applied 的区分 | 按既有合同执行：同一候选 negotiated capability 下分别观察 preview、accepted 和 actual apply，核对 authority status、mutation_count 与玩家显示；preview/accepted 不以 ack 到达代签 mutation；具体运行入口依 [testing-manual S6](../../../testing-manual.md) 选择 protocol/runtime 与真实 Viewer 表面；这是验证方法，不声明测试已经存在或执行 | 实际实现 PR 的固定提交、结果与适用表面记录；本轮未核实 | 本轮无行为执行，不代签完整产品验收或发行 |
| Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd；[REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007) | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | control loss 优先、hidden 值与重新校验 | 按既有合同执行：同一候选编辑后使 binding/session 失效，再提交过期版本；核对 control_lost 优先、latest value/diff 隐藏、无 mutation 与安全 re-auth 下一步；具体运行入口依 [testing-manual S6](../../../testing-manual.md) 选择 protocol/runtime 与真实 Viewer 表面；这是验证方法，不声明测试已经存在或执行 | 实际实现 PR 的固定提交、结果与适用表面记录；本轮未核实 | 本轮无行为执行，不代签完整产品验收或发行 |
| Trace refs: O7-REQ-92124092-6021-49e4-8def-33038e20b637；[REQ-PROMPT-009](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-009) | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | 当前身份/版本再校验、竞态和同键幂等 | 按既有合同执行：同一候选两合法客户端在同版本提交，核对一个 applied 与一个 stale；同key同digest重试不得二次mutation，不同digest conflict，控制丢失竞态保持hidden blocked；具体运行入口依 [testing-manual S6](../../../testing-manual.md) 选择 protocol/runtime 与真实 Viewer 表面；这是验证方法，不声明测试已经存在或执行 | 实际实现 PR 的固定提交、结果与适用表面记录；本轮未核实 | 本轮无行为执行，不代签完整产品验收或发行 |
| Trace refs: O7-REQ-36cc9e9d-eaa7-436f-b16b-6f6af089b04c；[REQ-PROMPT-011](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-011) | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | runtime_instance 应用，不推导持久化或同步 | 按既有合同执行：同一候选核对 actual apply 的 scope tuple 为 runtime_instance/none/none；重连、reload与旧instance重试不能把结果升级为永久保存或跨入口同步；具体运行入口依 [testing-manual S6](../../../testing-manual.md) 选择 protocol/runtime 与真实 Viewer 表面；这是验证方法，不声明测试已经存在或执行 | 实际实现 PR 的固定提交、结果与适用表面记录；本轮未核实 | 本轮无行为执行，不代签完整产品验收或发行 |

## Gameplay 反馈归属与提交资格设计

Owner 为 `viewer_engineer` 与 `runtime_engineer`，`qa_engineer` 核对场景。基线 `fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e` 的静态审读确认：[请求分发](../../../crates/oasis7/src/viewer/runtime_live.rs)、[CollectData 分发](../../../crates/oasis7/src/viewer/runtime_live/collect_data.rs) 将错误写入共享反馈，[快照生成](../../../crates/oasis7/src/viewer/runtime_live/control_blocking.rs) 再读取该反馈。兼容 [HTTP adapter](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/gameplay_submit_api.rs) 丢弃 builder code；预检尚未复用 canonical claim 解析。

本节是从 #2116/#2277/#2481 提炼的实现合同，测试与真实场景验收仍须分别核实。沿用现有 live/control、请求认证、world-service 与回执查询边界；不改变前文 PromptControl 的 wire capability、nonce/idempotency、application/persistence/sync scope，也不创建新的反馈持久化账本、公共权限 endpoint 或部署模式。

<a id="gameplay-feedback-isolation"></a>
### 反馈的生产、存储与可见范围

将当前单份 `latest_player_gameplay_feedback` 的职责拆成三类内部范围，由可信调用路径选择，不能按错误文本或请求自报的 player/Agent 猜测：

| 内部范围 | 写入来源 | 可读对象与寿命 |
| --- | --- | --- |
| 请求连接 | 解析/鉴权/nonce replay 拒绝、现有 Prompt/Chat 合法成功，以及尚未取得跨请求读身份的 service 请求结果 | 仅原请求连接的响应/临时展示；断开、身份切换时失效，不能因此读取玩家历史缓存 |
| 已验证玩家 | 已通过现有签名、当前 session/key 与 nonce 校验而取得玩家读身份的请求及其关联结果 | 同世界、同有效主体的已授权连接；运行时内有界保存，撤销/轮换/世界隔离变化后失效 |
| 世界故障 | runtime/service 确认的共享运行故障，例如公开读不可用或真实 chain sync failure | 获准观察该世界的连接；恢复时按故障生命周期清理，不能标为某玩家的操作拒绝 |

1. 在验签和 `validate_known_session_key` 成功处捕获请求主体及 fence：player/key、session epoch、世界及 branch/reorg、Agent binding revision。它只确定原请求的候选归属；现有 nonce 校验实际成功后才授予连接跨请求读取玩家缓存的身份。`RuntimeLiveSession.current_player_id`、仅声明 ID 的重连、旧 proof replay 和 service 幂等回放都不能单独升级私有读权限。认证失败和 nonce 重放的拒绝保持连接范围；通过现有认证后的业务拒绝可进入本人的缓存。
2. `GameplayAction` 与 `CollectData` 分发传递这一上下文，不再无条件写全局字段。现有 Prompt/Chat 的合法成功也投递到原连接并清除该连接旧的局部拒绝；其缓存重放不经过 fresh-success 写入点，不据此取得历史读权限。快照组装显式选取当前已验证玩家的反馈，并单独保留共享故障的运行状态语义；未知身份不得读玩家缓存。同玩家的两个连接也必须分别通过当前身份校验。
3. 身份切换、注销、key 轮换/撤销和 world/reorg 隔离变化清除旧连接投影。`scoped_gameplay_feedback` 至多保留 256 项玩家最近反馈，清除失效 fence，并按请求次序淘汰最旧展示项；展示缓存的淘汰不删除 canonical 结果。匿名请求不创建玩家槽位，重启后的世界结果由既有授权 lookup 重新读取。
4. 异步结果按原 `RequestCorrelation` 的 key digest 与 payload digest 找到原 fence 和请求次序。连接范围通过弱引用及 generation 回写；同一 correlation 的原 fence/order 独立固定，重试按连接替换收件目标、清除断开连接，至多保留 64 个临时收件目标，world-service 等待队列按 correlation 去重。只有已独立取得玩家读身份的请求可回写玩家缓存，已淘汰展示项不会被旧 terminal 重建。新 service 连接仍能显示自己的 pending/terminal，不要求额外预检来解锁操作；连接关闭或身份/请求代次改变后不覆盖当前展示。旧结果不能越过更晚请求，也不能按回调到达时的 `current_player_id` 改写归属。临时收件目标的淘汰不改变动作受理或 canonical 结果，原结果仍可按权限查询。
5. standalone/legacy chain 的 Gameplay 与 CollectData 受理后，在仍保留的原反馈上下文关联实际 `runtime_action_id`。[Runtime 反馈更新](../../../crates/oasis7/src/viewer/runtime_live/gameplay_feedback_runtime.rs) 只消费匹配 `caused_by` 或已关联 `job_id` 的 Runtime domain event：`ActionAccepted` 和无关世界推进不算完成，`FactoryBuildStarted`/`RecipeStarted` 转为 `executing`，`FactoryBuilt`/`RecipeCompleted` 依据同一 job 转为 `completed_advanced`（展示为 completed），即使后续完成事件的 `caused_by` 为空也不丢失关联。即时动作的明确结果、拒绝与生产阻塞只更新原动作；`AgentClaimReleaseRequested` 仅表示请求 `applied`，冷却和最终释放仍由 claim state 表达。终态清除动作关联；新请求、失效 fence 或展示淘汰均阻止旧事件改写/重建反馈。既有 `runtime_action_players` 继续供恢复和回滚归属使用。
6. Chat 的 `accepted` 和 Prompt apply/rollback 的 guidance 结果可保留在 `recent_feedback`，但不据此固定世界的 `execution_state`、因果信息或 `response_window_class`；这些值继续来自当前世界事实。请求成功不能证明 Agent 已执行后续意图。
7. 取消等待、关闭面板或断开只停止本地等待，不取消已经提交的世界动作。`OutcomeUnknown` 沿原请求 lookup 确认，不能换 nonce 重交；仅有最终权威结果才能说明世界效果。世界故障与请求拒绝不得制造或覆盖无关动作的 Runtime 结果。

内部上下文与缓存不加入 legacy 必填 wire 字段，不承担重启恢复。复用 [session policy](../../../crates/oasis7/src/viewer/runtime_live/session_policy.rs)、[owner read 失效处理](../../../crates/oasis7/src/viewer/runtime_live/owner_read.rs) 和 [world-service correlation](../../../crates/oasis7/src/world_service/correlation.rs)；未授权响应保留通用诊断，不附带其他玩家身份、私有 trace 或世界内部细节。

<a id="gameplay-error-code-preservation"></a>
### HTTP adapter 保留 typed builder 错误

`authorize_chain_gameplay_submit` 的私有错误类型从 `(u16, &'static str, String)` 改为拥有 code 的 `(u16, String, String)`。builder 失败保留 `(400, err.code, err.message)`，常量 code 转为 owned string，`write_gameplay_submit_error` 继续接收 `&str`。不从 message 反向推导 code，也不引入静态字符串泄漏。

JSON 的 `error_code` 仍为字符串，HTTP 状态类别与响应 shape 保持原合同。只有验签成功后的 builder 业务错误进入保真分支；解析失败仍可返回 `invalid_request`，鉴权失败与 nonce replay 继续原规则。客户端对已知 code 解释可执行下一步，对未知 code 安全降级；不得靠错误文本决定是否重试，也不得向未鉴权连接先暴露业务校验细节。

<a id="collect-data-preflight-consistency"></a>
### 共享只读 canonical 资格解析

从现有 claim 判断提取小型纯函数，输入为不可变 canonical `WorldState` 与验签所得 player/key，输出唯一 collector Agent 或 typed 拒绝原因。零份与重复匹配分别返回 `CollectDataQualificationError`；Agent 是否存在及资源是否足够继续在消费者的 quote/execution 层检查，不能取第一项或自动补 claim。Viewer 的 service preflight、world-service submit 和仍支持的兼容 HTTP CollectData adapter 复用该函数。

`WorldServiceProjection::from_world` 已在[字段 allowlist](../../../crates/oasis7_world_service/src/projection.rs)中保留 `starter_oc_claims` 与 Agent 资源，因此预检使用当前获准的 [VerifiedWorldView](../../../crates/oasis7_game_client/src/verified_world_view.rs)，无需扩大公开字段或新建 owner endpoint。一次计算固定其 projection 和 `ProjectionVersion/CommitRef`，claim、collector 和电力余额都来自该不可变视图。缺少可用权威视图、隔离身份不匹配或现行 freshness 条件不满足时返回需刷新/不可判断，不把未知数据当成空 claim 或默认资格；固定本次计算输入不等于要求开放历史 owner-read。

缺少 verified view 时，现有 provider readiness 检查可能先以 `llm_init_failed` 和 `canonical Agent observation unavailable` 拒绝；不统一改写成某个新同步错误码。验证应断言原有 gate 的真实结果，以及未消费认证 nonce、未改变世界状态，再分别测试可达的 canonical claim 拒绝分支。

保留现有 `data_collection_preflight` 的参数和资源计算。canonical collector 与本地显示绑定不一致时，明确要求刷新/重新确认绑定，不拿本地 Agent 的余额给另一个 collector 报价。standalone 非链模式继续自身合同，不新增 canonical starter claim 前置；legacy chain-linked 路径使用同一 canonical 资格检查。

共享的是纯资格函数，不是当前会消费 session nonce 的 `authorize_collect_data`。新函数不消费/分配 nonce、不写 session/sidecar、不扣资源、不创建 claim、不提交或试执行动作；每个预检/提交请求继续各自的签名、认证 nonce 和防重放规则，不复用同一个请求 nonce，也不因此跳过认证。

提交在当前权威状态上重新验证身份、claim 和资源。预检不锁资源、不授予权限；之后的撤销、key/归属变化、资源消耗或 reorg 可合法使提交拒绝。测试比较同一语义请求和同一权威状态的资格，不把预检到提交之间的正常状态变化误报为不一致；提交 ACK 仍只代表 received/pending，最终结果由 world-service 查询与回执证明。

| 定向场景 | 判定 |
| --- | --- |
| 有效 session/key，零/一/多份匹配 claim | 两端分别拒绝、得到同一 collector、拒绝 ambiguity；不得取第一项 |
| player/key 不匹配、Agent 缺失、余额不足 | 安全拒绝或不可执行，原因与对应权威状态一致 |
| 本地绑定与 canonical collector 不一致 | 提示刷新/绑定不一致，不混用 Agent 与余额 |
| 预检后权限/余额/世界版本变化 | 提交重新校验，可明确拒绝；旧预检不承诺未来执行成功 |
| 相同不可变视图反复调用资格函数 | world/claim/资源/动作 nonce 不变；入口自己的认证 nonce 按原规则处理 |

### 实施、兼容与未决边界

- 可按反馈隔离、错误码、CollectData 资格共享三个局部修改完成本控制面目标，测试复用表中现有文件；不扩展成所有 quote 的重构。渲染效率与视觉回归由[独立局部设计](viewer-pixel-world-bridge-rendering.design.md#render-reconcile-and-visual-regression-target)承担。
- 先让生产者保留可信上下文，再切换写入与 snapshot 读取，并同时替换锁定旧共享行为的测试。内部结构变更不需要世界数据迁移；重启清除临时反馈，已提交世界动作仍按原 correlation 查询。回退旧实现会重新引入污染，不能作为验证通过的降级；应修复或保持受影响展示不可用。
- 验证目标是确定性的归属、code 和同状态资格，不在未测量前承诺吞吐、延迟或跨 runtime 历史同步。可见 UI 变化按 S6 验证，行为修复按开发流程选择适用的独立试玩场景；缺少私有 Runtime 或图形环境时标记 blocked/unverified，单元测试不能代签。
- `runtime_engineer` 在实现时建立“有效 service session 但 canonical claim 不可用”的受支持负例，确认其当前可达性及对外错误映射；在此之前只确认代码判断条件不同，不声明线上已复现。`viewer_engineer`/`qa_engineer` 以 A/B/匿名、切换/迟到和共享故障恢复用例解除反馈隔离待验收状态。

## 客户端与离线回放的补充设计

<a id="viewer-client-request-lifecycle-design"></a>
### 客户端等待状态与结果相关性设计

本节是[客户端请求生命周期](viewer-control-plane-split-live-playback.prd.md#viewer-client-request-lifecycle)
的实现要求。静态基准 `5e930fa` 中，`viewer_prompt_control_module.js::handleError` 在检查
request 身份前清 timer 并改当前 feedback；`legacy_core.js::handleGameplayActionError` 在
quote facade 分流前清普通动作 timer。已有部分 ACK identity 校验不能证明这些错误路径安全。
本次补充设计，行为修复和测试结果仍由对应实现 PR 提供。

1. 在发起操作时保存不可变的本地上下文：operation/target、发起玩家、会话与连接代次、协议
   请求身份和该等待实例。身份取自已有协议/状态，不增加签名字段或另一权限来源。
2. 注册/签名等每次异步返回后重新核对该上下文与当前操作。旧 completion 不得把新操作从
   signing 变成 sent/accepted，也不得阻止新的签名完成后真正发送；只有实际发送才启动 ACK 等待。
3. ACK 和 error 都先做通道/operation/目标/现有身份校验，再在原等待实例上结束 timer、更新
   feedback 或历史。结束操作使用相同实例/代次，避免旧回调清掉替代请求的 timer。
   当前合法 control_lost 继续触发现有安全阻断；无归属的 protocol error 只作协议诊断。
4. quote facade 先识别是否为某个 quote 的结果，再处理该 quote 的状态；未认领的普通 action
   error 才进入普通动作处理。断连将旧连接 quote 变为可恢复的非在途状态；新连接 hello/snapshot
   不继承旧 pending。同连接 snapshot 不能清理仍有效的在途报价。已收到的工业报价按相关
   输入变化标 stale/不可执行，保持可读；报价消息被接纳时立即发布显示更新，不等待无关
   snapshot。展示缓存不授予执行旧报价的权利，工业数值和有效性仍由[资源流合同](../m4/industrial-resource-flow-contract.prd.md)定义。
5. 注销 callback 捕获发起会话的代次；其本地清理、注册取消和刷新调度只在该代次仍当前时生效。
   B 已登录/注册时，A 的成功或失败只结束 A 的操作记录，不清 B，也不根据 A 的结果推断 B 被撤销。
6. 缺相关字段的 legacy 路径保留受限单请求行为；只在可证明归属的发送上下文内消费。
   同连接超时后同目标重试等歧义不能靠本地 seq 补出远端没有 echo 的身份；保持未知并沿
   支持的权威查询/重新建立请求上下文恢复，不把旧 ACK 写成新历史或新 applied。

| 场景 | 必须断言的时序与结果 | 现有实现/测试接点 |
| --- | --- | --- |
| R1 Prompt 旧 error | A rollback(999)→B Apply，A error 不改 B ID/timer，B ACK 正常 applied/刷新；B 签名期、A 超时后重试、不同 Agent 与当前 control_lost 分别覆盖 | `viewer_prompt_control_module.js`、`viewer_prompt_control_protocol.test.jsx` |
| R2 Chat 旧 ACK/error | A 超时→B 发出→A ACK，B 仍在途且不写假历史，B ACK 才完成；暂停 B 签名注入 A ACK，恢复后 B 实际发送；不同玩家/Agent 与 legacy 无 seq 分开 | `legacy_core.js` 的 Chat handlers、`viewer_agent_chat_auth_module.test.js` 与 `viewer_canonical_goal_module.test.js` |
| R3/R4 quote 与普通动作 | 报价 pending 时断连，重连后可重新报价且拒绝旧连接结果；Q pending→G sent→Q error，G 在 30 秒 ACK 上限仍 timeout，或 G 正确 ACK 正常完成 | `war_declaration_quote_integration.js`、`viewer_quote_protocol_facade.js`、`legacy_core.js` 及对应 integration 测试；使用 fake clock |
| R5 receipt 生产/消费 | 支持的 AgentChat producer/consumer 对 pending、canonical accepted、失败和重复结果使用相同语义；编译覆盖实际受影响目标，不猜字段迁就编译 | `oasis7_proto` Viewer DTO、Cognition canonical goal producer、当前 Chat consumer；保留旧已完成 producer 映射 |
| R6 logout 代次 | A revoke ACK→B 登录/注册→A release 成功/HTTP 失败/网络失败，B 状态与刷新不变；普通/断网退出与重复点击仍正常 | 当前 auth/session module、logout handler 和受控 transport 测试 |

上述确定性回归不代签 #3670 的真实双会话产品体验。涉及可见交互时按 S6 选择实际表面；
本次纯合同修改没有发起登录、撤销、provider 或真实世界操作。

工业报价的显示更新与保留另外承接 [#3510](https://github.com/eng-cc/oasis7/issues/3510) 的
#2648/#2751：在 `viewer_quote_protocol_facade.js`、`power_survival_quote_state.js` 及相应
测试验证报价到达立即可读、普通 snapshot 不无条件抹去、相关条件失效明确 stale，且旧报价
不会绕过当前执行校验。此显示时序不由 R3/R4 的宣战报价通过结果代签。

### 离线 Step / Seek 设计

`server.rs::handle_playback_control` 对一次 Step 单独统计成功发出的 Event 数，并在 Seek 后
重建后续请求的 logical/sequence 基准。completion 的 `Advanced` 使用本次发送事实，delta
只报告真实差值；旧全连接最高 sequence 不能成为是否推进的判据。同连接 TCP 验收覆盖
`Step(3)→Seek(10)→Step(1)` 的同 tick 事件，核实际 Event 与对应 request ID ACK；
另覆盖重复 Seek、过滤器、EOF、首次 Step 和空 journal。复用 `server.rs` 现有 playback
completion 测试并补完整链，不引入 Web Seek 或 live seek。

## 不变量

- live 的世界推进单调，不提供回退或跳时控制。
- `seek` 枚举可以为非 live 兼容保留，但不得泄漏为 live 玩家或自动化动作。
- Web 发送前拒绝与 legacy handler 的记录并忽略必须如实区分；两者都不等于 channel failure、发送成功或世界回退。
- queued/accepted 是本地发送阶段，不能替代 runtime 事件、snapshot 或 completion feedback；空结果必须保持空结果。
- PromptControl 的本地 accepted、cached replay 与 actual applied 必须按 authority result 区分；hidden/redacted result 不得把 digest 或 profile value 泄漏到 browser。
- PromptControl application scope 不越过 runtime instance；persistence 与 sync scope 必须保持 `none`。

## 代码与验证接点

- 协议/服务：`crates/oasis7_proto/src/viewer.rs`、`crates/oasis7/src/viewer/{protocol.rs,server.rs,live_controls.rs,runtime_live.rs}`。
- PromptControl authority 与签名：`crates/oasis7/src/viewer/auth.rs`、`crates/oasis7/src/viewer/runtime_live.rs`、`crates/oasis7/src/viewer/runtime_live/control_plane.rs`、`crates/oasis7/src/viewer/runtime_live/control_plane/prompt_profile.rs`、`crates/oasis7/src/viewer/runtime_live/llm_sidecar.rs`。
- 表现/自动化：`crates/oasis7_viewer/software_safe_src/{legacy_core.js,legacy_core_control_gate.test.js}`；生成浏览器入口为 `crates/oasis7_viewer/viewer.js`。
- PromptControl 浏览器签名与反馈 consumer 还需对齐 `crates/oasis7_viewer/software_safe_src/{main.jsx,viewer_feedback_module.js,viewer_auth_surface_module.js,software_safe_state.js}`；runner/QA 使用真实 backend-issued grant 与 runtime trace。
- 本设计不改变世界规则、P2P/共识回退策略、视觉方向或测试发布判断；实现 leaf 不得在此 system leaf 中合并。
