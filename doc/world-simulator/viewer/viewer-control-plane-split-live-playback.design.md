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

## 具体协议编解码边界

<a id="live-control-state-design"></a>
### Live 调度状态观察

Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；上游为
[Live 调度控制合同](viewer-control-plane-split-live-playback.prd.md#live-control-observation)。
Viewer 服务为新决策调度保存独立的随机 process observation epoch 与单调 revision；epoch
不授予任何权限。revision 从 1 开始，限于 JSON 安全整数；所有共享调度修改入口统一更新观察，
同状态重复请求返回同 revision。安全暂停必须先停止新决策；若版本耗尽而无法发布真实状态，
永久失效本进程观察并关闭已协商该能力的连接，不复用旧版本或在连接中重置 epoch。失效后
协商连接的 Resume 不得重新启用新决策；未协商旧客户端保留原协议行为。`LiveControlState` 固定 scope 为
`viewer_new_decisions`，不携带世界快照、私有目标或模型内容。

仅 HelloV2 协商 `live_control_state_v1` 的 live session 接收 `live_control_state`。
握手和显式快照请求强制发布，周期循环仅在该 session 尚未观察当前 revision 时发布；成功
Play/Pause 带原 request_id 立即发布。Step 原 completion ACK 不改语义；若 Step 改变调度
策略，当前控制状态另行同步，不把零世界增量标作 Advanced。

Web 先按当前 socket/连接代次过滤，再固定首次 epoch。较低 revision 不改当前策略；同
revision 冲突使观察失效。确认按原 request_id 和原 play/pause 意图匹配；迟到确认不得完成
新请求。断连或旧 peer 未协商时显示未确认。可见文案区分“Viewer 新决策已暂停/已启用”
与世界时钟、既有任务及在途模型请求；不能显示伪造行动回执。

验证覆盖 handler 成功 Play/Pause 的原请求关联、无世界增量、重复请求、另一连接周期同步、
重连、旧 epoch/较低 revision/同 revision 冲突、迟到确认与旧 peer 无新消息。Web 必须经
真实 handler→接收状态→可见文案测试；正式浏览器证据与模型/世界结果另外核对。

`oasis7_viewer_codec` 依赖 `oasis7(default-features=false)`、`oasis7_proto` 与请求合同 owner `oasis7_client_api`，持有具体 `ViewerResponse<WorldSnapshot, WorldEvent, AgentDecisionTrace, RunnerMetrics, WorldTime>` 的非泛型 JSON writer、字符串包装和请求解析入口。Viewer runtime 依赖 codec，并 re-export 同一响应类型，保留既有公开路径和 native/WASM 条件类型。具体 archive 类型仍归 `oasis7::simulator`；codec 不拆分 archive 或空间投影，也不启用新的 Runtime/Node features。

普通和带原请求 nonce 的 flatten envelope 编码均在 codec 中生成；nonce 为 None 时省略。公开 writer 参数保持 `&mut dyn Write`。Runtime live 使用直接编码入口；Offline TCP 使用 codec 内的 8 KiB 有界缓冲入口；Event 响应复用直写入口，避开 Linux 大 Event 实测的额外缓冲开销；具体开销机制未归因，字符串包装复用同一缓冲 serializer、请求 1 KiB 缓冲，并在未排出块时直接复用尾部 Vec。这避免原具体 BufWriter 和 Vec 路径逐片段虚分派的实测退化。普通响应有两种固定 writer 实例（动态 writer 与 `BufWriter<&mut dyn Write>`），均留在 codec 库内，不向上层导出泛型序列化树；实例数量取舍须一并测量冷构建与运行时成本。

小字符串保留容量可由原 128 B 增至 1 KiB；Offline 临时增加 8 KiB 缓冲。返回编码或 writer 错误前拆开缓冲，丢弃未写尾部，不在 codec 内重试写入；此保证不扩展到 panic 路径。

codec 不做权限判断、不拥有 socket，不写 newline 或 flush。Runtime live 在原权限/可见性选择与 Event 清理后，把响应直接流式写入受限 Outbox writer；完整帧仍由上层 newline 与 flush 提交。单帧、单轮、帧数、partial frame、deadline 和错误映射保持原合同；不能先生成无界缓冲再交给 Outbox。

编译复用验收使用同配置 ECS 三 binary 测量：冷构建、no-op、Viewer 编排 touch 与 codec 输入 touch 分别记录真实 Cargo unit、Fresh/rebuilt、timings 和资源。新增包或搬迁代码不代表已证明加速；构建和运行时成本的当前候选结果放在实际 PR/CI。

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

<a id="prompt-authorized-value-projection"></a>
## Prompt 默认值、生效值与来源的授权读投影

Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d；O7-REQ-a9797fdb-39f7-48fb-9202-9700d9d86159。本节承接 REQ-PROMPT-003/008，给出现有 Prompt authority 的目标读消费者，不改变上述写入、幂等、强认证或 `runtime_instance/none/none` 合同。当前 `AgentPromptProfile`/snapshot 主要包含三项 override 与版本；仅有该 map 不能证明 runner 当前 effective，也不能从 null 推出默认值。完整投影、Builtin 读回接点及其 UI 尚待实现，以下正常、失败和恢复规则是实施输入。

### 唯一生产者与读取身份

目标 `PromptValueProjectionV1` 是**现有 current-player 授权 snapshot/Prompt 读回适配层的可选私有扩展**，schema revision 固定为 1；不写入公共 World model、公开事件、共享广播缓存或 Prompt mutation ledger。普通 snapshot 请求可请求/接收当前合法绑定 Agent 的投影，不借自动 Preview 消耗写 nonce、强认证或 256 条结果槽位。没有该扩展的客户端仍按原协议工作，但不能声称已展示 default/effective 或本节清除预告。

producer 先核当前连接的真实主体与 session/key、当前 Agent binding/control、允许的字段和原可见性政策；输入的 player/Agent ID、历史 profile、曾有 owner 身份都不授予读取。读取完成后及敏感回包前再次核对原身份/代际，失权即按原 hidden 边界丢弃正文；不能因旧 read 开始时合法而交付给新 owner、另一个 Agent 或另一个账号。已有 world-service 对私有 short-term goal 的隐藏继续有效；canonical goal 的正文与 intent/receipt 仍从其原受保护读面取得，不能借新增 Prompt default 字段补露或把它改称 sidecar override。

| 投影部分 | 实际来源与必须保留的字段 | 消费含义 |
| --- | --- | --- |
| 当前读取绑定 | 原 world/authority 实例、Agent、经认证的当前 player/session/binding epochs、原 profile version/digest；本次 read correlation 与读取时点 | 绑定本次获权读取，不是新写授权。与当前客户端目标/连接/身份不符的迟到回包不替换当前值 |
| 配置默认值 | Builtin 在 `ensure_runner_initialized` 为该 Agent 实际加载并交给 `LlmAgentBehavior` 的 `LlmAgentConfig` 三个文本；每字段安全来源标签（常量、加载配置、环境或 Agent 专属选择）及同次非秘密配置 revision | 使用**已加载的**值，不在页面或读请求时重新读取环境/配置文件。revision 绑定这三项值、解析版本与 Agent 选择，不包括 API key、endpoint secret 或私有文件路径 |
| 权威 profile override | sidecar 当前 `AgentPromptProfile` 的三项 `Option<String>`、version/digest 与原 provenance | `None` 表示该 profile 未设置 override，不能据此推断 runner 已使用 config；空值也不等于缺字段/被隐藏 |
| runner 当前 effective | 对同一已注册 Builtin runner 的只读观察：runner/配置实例关联、最近实际消费的完整 profile 关联或初始化关联、三项实际 override 与 `effective_*` 解析结果 | 与 authority profile 分开；只有真实已消费/读取到的值可称该 runner 当前 effective。effective 是此层供后续 Prompt 组装使用的三项值，不代表在途模型请求已更改、完整 assembled Prompt、私有记忆或 canonical goal |
| 来源与可用性 | 每字段的 config/profile/初始化来源及有权读取的值，完整读取、等待 runner 读回、不可用或 hidden 的原因；被隐藏/未实现不是合法 null | 初始化短期目标标明 `runtime_live_phase1_short_term_goal` 来源；显示层不能假称它是当前 config 默认或玩家配置。缺真实读取时给有界未确认原因，不伪造值 |

真实接点是[配置与 effective 解析](../../../crates/oasis7/src/simulator/llm_agent/behavior_prompt.rs)、[配置/override 处理](../../../crates/oasis7/src/simulator/llm_agent.rs)、[runner 初始化与完整 profile 下发](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/llm_sidecar_runner.rs)、[当前 profile/history](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/prompt_profile.rs)及 current-player snapshot 构建。目标 Agent/runner adapter 在初始化时保留三项安全配置来源，在真正消费 Prompt 更新时记录对应 profile 关联，并提供只读返回；不能用“已 enqueue”补一份 fake runner ACK。原 source 尚无这份完整投影，这不免除该正常 producer 的实现义务。

### 同一读取切点与当前配置

Runtime 先在原序列化边界固定身份、profile version/digest 与 runner/config 实例，再向该 runner 读取；回包必须匹配该实例及最近消费的初始化/profile 关联。profile、配置实例或权限已变化时不拼接新 profile 与旧 effective：保留各自真实时点，报告等待重读；当前仍合法才允许显式刷新。native async runner 已 enqueue 而尚未消费时，可分别显示“authority profile 已应用”“runner 当前已确认值/等待更新”，不能把 enqueued 当模型已采用；同步 Builtin 也须从真实 behavior 读取。

本目标首版的已加载三项 config 在该 runner/authority 实例内不可热改。确需更换配置或重建影响来源的 runner，沿原实例更换/重启流程取得新的 authority epoch，再接受新增强请求；旧 epoch 的结果按原 `result_unknown`/历史恢复处理，不重新执行。它不新增配置热更入口，也不要求普通 Prompt patch 轮换 epoch。若适配器会在同 epoch 偷换配置/runner，却无法保证该读取和提交基准，则不能启用本目标投影；必须先接通这一实例边界，而不是让客户端用新默认值覆盖旧提交含义。

本地草稿和 preset **不是 authority 记录的一部分**。客户端给草稿保存原 Agent、读取绑定、基准 profile 和安全来源引用；preset 仅标记本地模板来源，填充之后仍是 draft。原字段只读、不支持或被隐藏时不填入伪默认；当前读取正常的合法 Builtin 路径必须返回上述真实来源和值，不能以统一 unknown 占位取代 producer。

### 完整 patch 的清除与预览算法

沿用[原 patch helper](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane.rs)：`unchanged` 保留原 profile 字段；`clear` 变为 `None`；`set(x)` 按原 trim 后空串转 `None`、非空保留规则。`changed` 仍比较完整 profile 的三项 override。新增读投影不改该规范化、签名 preimage、`applied_fields`、版本或 mutation 计数。

1. 页面以完整当前 profile 克隆候选，对三个字段应用本地 patch，分别列出 **profile 差异** 和 **runner effective 影响预告**。默认值只作带来源的只读参考，不自动复制为 `set`；未修改默认/override 发送 `unchanged`，不制造新版本。
2. 若候选三字段均未变，原 enhanced authority 返回 `accepted/no_change`、`mutation_count=0`，不下发 driver；effective 保持真实当前值。因此 profile short-term 为 `None`、runner 因初始化实际为 B 时，单独 clear 该空字段不会恢复 config A，不能把按钮或结果写成“已恢复默认”。
3. 若任一字段变化，原路径将**完整三字段候选**下发 runner。Builtin 各字段按候选 override 或其已加载 config 解析；例如只改 system，short-term 候选仍为 `None`，也可能使该字段从初始化 B 变为 config A。预览须列出这种间接影响，不能只列 `applied_fields` 或玩家点击的字段。预计结果标记为尚未应用；实际 effective 仍待匹配的 runner 读回。
4. 原 profile 已有 override C 时，明确 clear 后候选为 `None`；当整 profile 确有变化并成功被 Builtin 消费，effective 回到该字段当前已加载 config A，不恢复历史初始化 B。空白 `set` 经原规范化等同 clear，应在提交前明确说明；产品不提供“保存空字符串”的另一语义。
5. 用户显式 Preview/Apply/Rollback 继续使用原完整身份、expected version 与权限；本地计算不是 authority 预检。原 Preview 零 mutation，原 Apply 的 accepted/applied scope 不扩大。driver 不支持/拒绝/尚未初始化时显示原原因，不能用本地预告制造成功。ProviderBacked 的 production Prompt 注入仍 unsupported；原 Hosted local-mock 仅属其已准入测试范围，不能作为生产支持证明。

### 合法 stale、失权及恢复

仍有当前 control、但 profile version/digest 已变时，保留原草稿 D 与原基准，按原 `stale/latest_allowed` 重新读取当前投影；成功后可比较有权读取的旧基准、最新 profile/effective 与 D，并选择重新编辑、重载或放弃。重新编辑必须显式基于最新版本形成新请求，不替换旧请求身份，也不因刷新成功自动 Apply。刷新失败保留 D 及未确认说明，不从缓存拼“最新值”，可在仍获权时重试同类读取或退出。

control/session/binding 丢失优先于 stale；清理当前页面中无权再显示的 profile/default/effective 与差异，草稿只按产品原规则保持未提交的安全本地内容，不提供重发/转移；敏感原文不写公共诊断，也不自动写入持久 preset；显式本地 preset 操作仍服从其原存储范围。原请求结果未知按原 ledger/epoch 合同核对，scope 缺失不升级为永久保存。迟到 A 的读取/Preview/Apply 不清 B 的等待状态、不覆盖 B 草稿，直接复用[客户端请求生命周期](#viewer-client-request-lifecycle-design)。

### 读取容量、兼容和交接

每个当前页面/读取上下文至多一个 runner 值读取在途，使用原读取 deadline，超时只结束该次观察；不取消模型或重发 Prompt。每条完整私有投影限 64 KiB（包含三字段、来源、身份和封套），实际 transport/host 更小上限取严；超限返回有界不可用原因，不截断文本后说完整，不修改原值/配置/写入限额。每次 snapshot 可复用身份与实例仍匹配的已确认投影，但必须标实际时点；变更/撤权使其失去当前资格。查询不消耗模型调用、世界模块费用或 Prompt mutation/result-ledger 容量；既有读取鉴权、速率与资源限制继续适用。

协议/Runtime owner 实施可选私有投影及当前权限过滤，Agent owner 实施真实 config/effective 读取与异步关联，Viewer owner 按[五源交互](viewer-account-agent-uiux.design.md#prompt-source-clear-stale-consumer)展示，QA 执行[独立来源/清除/恢复方法](viewer-manual.manual.md#prompt-source-clear-stale-validation)。新增读取不能改变旧客户端 wire 解码或当前 unsupported 范围；未接线的层如实保持未实现，现有 profile snapshot 或静态稿不代签完整链路。

<a id="prompt-command-entry-applicability"></a>
### Prompt/Chat 的实际入口接收

Trace refs: O7-REQ-14ddd9ba-2d94-48e8-b2e4-0dbae26ddc36；O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293；O7-REQ-b6140b6c-ca2c-4734-84ce-7963f9385fff。分别承接 [REQ-PROMPT-001](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-001) 的目标/当前控制、[REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002) 的内容类型及 [REQ-PROMPT-005](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-005) 的能力收窄。复用上文六个 PromptControl 合同、[授权值投影](#prompt-authorized-value-projection)及 [API 正式入口接收](../llm/external-agent-game-api.design.md#api-primary-entry-applicability)，不改变 patch、签名、权限或 scope。

**适用单位与有限目录。** 以下接点核对固定于 `d53e6e745d68c88f59d9aca009cd94451fb6be44`。接收单位是实际 build、入口/适配器、endpoint/world、后端及已协商协议的组合；在该组合原有界面或结构化输出中逐项区分 Chat、preset 填充、本地草稿、profile 读取、Preview、Apply、Rollback 和原结果恢复的实际支持/受阻状态。该能力记录是消费规则，不新增 wire 字段、全局能力注册表或第三种 mode。只有原已采纳且适用的能力才要求接通；未提供 preset/编辑器的入口不用造 GUI，有能力但未接完整消费者则如实留作实施差距。

| 实际入口 / 接点 | 目标、身份和内容消费者 | 能力、回执与接收边界 |
| --- | --- | --- |
| canonical Web Command/Chat 与 Advanced Prompt Settings：[页面](../../../crates/oasis7_viewer/software_safe_src/viewer_app.jsx)、[控制模块](../../../crates/oasis7_viewer/software_safe_src/viewer_prompt_control_module.js)、[草稿/选择](../../../crates/oasis7_viewer/software_safe_src/legacy_core.js) | 当前选中 Agent 与当前账号绑定/控制分开显示；Chat draft 与三字段 Prompt draft 分离。实际注册、绑定及能力读回控制可操作状态；共享世界的可见对象不因此可控制 | 已有 Preview/Apply/Rollback 和协议协商接点；实际 backend、版本/历史、hosted strong-auth 仍分别准入。未选增强能力只按 legacy 结果解释；缺 epoch/当前绑定不能冒充增强就绪。新五源投影尚须真实接线，不能由页面默认空值推导 authoritative default |
| native 交付及确实支持的 native 适配 | 当前 native Launcher 的 [打开游戏页](../../../crates/oasis7_client_launcher/src/main_app_shell.rs)调用 `open_browser(current_game_url)`；到达的实际 Web build 沿上一行接收。另有明确声明且实际实现的 native 协议消费者时，按下列真实 legacy/enhanced 路径逐项核其目标/身份与输出 | Launcher 运行、native SDK 存在或诊断 fallback 不证明另有完整 Prompt 编辑器；已退役 Standard-3D/egui Viewer 不恢复为支持入口。真实 native 包装只承担其已声明能力，缺预览/回滚/回执必须明确，不能将打开页面或重新加载叫恢复成功 |
| TCP/JSON legacy；当前 `oasis7_pure_api_client` 的 `prompt-preview/apply/rollback`：[命令](../../../crates/oasis7_client_cli/src/bin/oasis7_pure_api_client.rs)、[构造及 matcher](../../../crates/oasis7_client_cli/src/bin/oasis7_pure_api_client/support.rs) | 显式 agent/player、签名、expected version/rollback target 按原命令绑定；命令参数/本地待发内容是用户意图，不是当前获权证明或已生效 profile。原 authority 执行当前身份与目标检查，输出保留原命令/目标及真实结果 | 固定源码只协商 canonical chat，增强四身份字段为空，terminal 按 ACK/error 类型；这些命令真实存在，但不能据此宣称增强关联、原件持久恢复或同步已接通。preview/rollback 按实际 legacy handler 能力与错误分别保留，不因缺增强字段一概称整个协议不支持 |
| 已协商 enhanced TCP/JSON；使用原协议的实际适配器 | [协议分流](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/prompt_control_enhanced.rs)选择 `prompt_control_result_v1` 后，完整 request ID、player/Agent、session/binding/authority epoch、expected version 与原操作必须进入签名；当前授权先于 cache/version。只提供部分增强字段不能降级后执行 | 原 Preview、Apply、Rollback 各守原状态/可见性/ledger；未协商却发送增强字段按原 capability 错误拒绝，已协商缺字段按原缺项拒绝。适配器的当前读权、原结果关联和下面的未知恢复必须实际消费；服务端支持不代签旧 CLI 已接线 |
| HTTP/API/SDK 或其他包装 | 当前 HTTP `/v1/game` 引导面 `writes_available=false`；SDK 的 [Viewer 协议类型](../../../crates/oasis7_client_api/src/viewer_protocol.rs)、[Prompt 签名函数](../../../crates/oasis7_client_api/src/viewer_auth/prompt_control.rs)与 GameClient 的 Chat/task 能力分别记录，类型/签名函数不能代替一个可调用 Prompt route | 当前只读 HTTP 不发送 Prompt 写请求；提供 Chat 的 SDK 不自动支持 profile Apply。实际包装若已接原 Prompt route，则按该 route 的上述协议、权限和结果接收；未提供时明确未提供及真实可用读面/入口，不伪造字段、成功、历史或完整恢复，也不要求所有包装补齐 CLI flag |

**共同消费者规则。** 进入或切换目标时，先把目标 Agent、当前请求主体及实际控制来源对应起来；本地选择、参数、旧绑定或另一入口的成功均不能充当当前控制。读面不足以判定时明确待核/受阻，使用原入口真实提供的认证、绑定同步或授权读取，不以自动试写获取权限，也不新增统一 preflight 协议。显式请求仍由原 authority 复核；控制丢失隐藏私有值、阻止草稿自动转移。迟到 A 的响应只归属 A 的原请求，不能覆盖当前 B 或终止 B 的等待。

内容始终标明其用途和来源：一次 Chat 是本次显式消息操作；preset 只在实际提供的表面填入本地草稿；本地编辑不发送、不改变 profile；持续 Prompt/目标改变只沿各自原 authority。canonical Chat 可能按原意图合同形成持续目标，不能把“一次消息”误写成保证无后续业务效果，也不能把它自动复制为 Prompt override。Preview 只表示候选检查，Apply/rollback 的接受、authority 更新和 runner 实际消费沿上文区分；新增/清除/未改变仍遵守原三态 patch，不能把未提供/隐藏的值当作 clear。

缺能力和缺结果分别处理。仅展示该组合真实可用的 Preview/Rollback、历史与 scope；原 Rollback 需要当前授权、当前版本和原 authority 可解析的 target，不能用本地撤销、Reload current 或重新填入旧文本冒充。增强结果保持 `runtime_instance/none/none`；legacy 回包没有的字段不得用默认值补成增强证明。收到未关联 ACK、断连/超时或结果缺失时保留原操作与最后已核身份，显示该原结果未知；授权读取当前 profile 不裁决原操作成功或未发生。同 instance 且完整原身份/缓存仍适用时，才按原 same-key 合同精确重试；换 instance/epoch、缓存丢失、原件或当前读权缺失则保持受阻并给出实际安全检查/退出路径，不换 nonce 自动重发、不把 canonical chat 专用重试推广给 Prompt。恢复再失败保留各自原因，不能以轮询次数或入口切换宣布成功。

接收采用 [PV13–PV15 跨入口方法](viewer-manual.manual.md#prompt-command-entry-validation)。原产品条件只覆盖实际支持且已声明的表面；缺消费者是实施剩余、缺执行是验证剩余，均不以本节文字代签当前可玩或支持升阶。

<a id="viewer-infrastructure-evidence-join"></a>
### Infrastructure result visibility and evidence join

既有 authority result 经授权 read projection 才形成 Viewer observation；基础设施 result/freshness/error/application boundary 消费 [PRD readback receiver](viewer-control-plane-split-live-playback.prd.md#viewer-infrastructure-readback-contract)、[QA evidence method](../../testing/design.md#qa-infrastructure-evidence-method) 与 [Harness evidence join](../llm/continuous-agent-harness.design.md#harness-infrastructure-evidence-join)。冻结同 candidate/world/root/revision/child/direct-parent receipt、provider/environment 和 evidence window，capture before/outage/fresh/recovery/retry/replay 的可见结果，再使用同候选 authority source/journal/API oracle 比较 disposition、freshness、no-effect、history/W 和 effect count，归档差异/artifact/hash，最后由 QA 评估 required/full。Viewer 不自造 receipt、freshness 或完成态。

read outage 只改变可见性，不改变权威 W/history/effects；retry/replay 不能产生第二次 effect。SC9 保留八格、authority 格内 root-absent pre-finality 与 root-present post-root-pre-child specimens，并核对 Runtime/API/Agent 相同 quantities/buckets/window/lease/W/progression/next_action/next_recheck。错误/恢复只呈现授权 disposition，私有 trace 不进入 browser。PromptControl scope、profile/wire、race/privacy 与 no-live-seek 合同保持原状；此方法不新增 UI/DTO/API、持久化、同步或 runtime 算法。

代码接点仍为下列既有 protocol/runtime/Viewer consumers；文档方法不证明它们已执行验证。[Playwright capture method](../../testing/manual/web-ui-playwright-closure-manual.design.md#playwright-source-capture-method) 提供 source/scene/environment/oracle/artifact 接点；[QA tier boundaries](../../testing/prd.md#qa-infrastructure-tier-boundaries) 保留 full 所需实际 provider-backed Agent parity/external headed desktop+narrow。missing proof 为 blocked/unverified，观察到 invariant violation 为 failed；[equal-version negative](../../testing/prd.md#qa-infrastructure-equal-version-negative) 仍 planned/unexecuted，shape-valid 不等于 semantic success。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d；[REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005) | 全部已提供预检入口的共享资格、估计与实际提交能力，漂移及原件未知恢复 | [全入口适用性](viewer-control-plane-split-live-playback.design.md#preflight-family-applicability) | Runtime/领域 authority；Viewer/GameClient；QA | 不新增未提供的预检或动作，不把签名/字段名/估计等同提交资格，不证明已实现 |
| [Viewer readback](viewer-control-plane-split-live-playback.prd.md#viewer-infrastructure-readback-contract) | 同世界 result/freshness/error/read projection 与同候选 authority join | [Evidence join](viewer-control-plane-split-live-playback.design.md#viewer-infrastructure-evidence-join) | Runtime world/journal；Agent cognition；QA verdict；gameplay W/window oracle | 不新增 UI/DTO/API，不扩大 PromptControl scope，不证明执行/full |
| Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；[REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)| 请求方得到可归属反馈，其他玩家和新身份不受污染 | [隔离设计](viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation) | viewer_engineer；runtime_engineer；既有 session policy、world-service correlation | 不扩展 PromptControl 同步或持久化，不声称多人验证完成；专业合同：[反馈归属](viewer-control-plane-split-live-playback.prd.md#player-scoped-gameplay-feedback) |
| Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4；[REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)| 合法鉴权后的业务拒绝保留 code，消费者可给出安全下一步 | [错误码设计](viewer-control-plane-split-live-playback.design.md#gameplay-error-code-preservation) | runtime_engineer；typed builder、兼容 HTTP consumer | 不改变鉴权次序、HTTP 状态类别或错误隐私边界；专业合同：[错误码合同](viewer-control-plane-split-live-playback.prd.md#gameplay-error-code-contract) |
| Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d；[REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005)| 同一权威状态的 CollectData collector/资格一致，提交重新核验 | [预检设计](viewer-control-plane-split-live-playback.design.md#collect-data-preflight-consistency) | runtime_engineer；world-service owner-read 与 gameplay authority | 不把预检变成授权、资源预留或执行成功保证，不扩大本地模式前置；专业合同：[预检合同](viewer-control-plane-split-live-playback.prd.md#collect-data-preflight-contract) |
| Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111；[REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004) | authority accepted 与 applied 的区分 | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | protocol/runtime、Agent、Viewer、QA；沿用本节六个专业合同入口 | 不证明各入口完整 UI 消费、实际实现或跨会话持久性 |
| Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd；[REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007) | control loss 优先、hidden 值与重新校验 | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | protocol/runtime、Agent、Viewer、QA；沿用本节六个专业合同入口 | 不证明各入口完整 UI 消费、实际实现或跨会话持久性 |
| Trace refs: O7-REQ-92124092-6021-49e4-8def-33038e20b637；[REQ-PROMPT-009](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-009) | 当前身份/版本再校验、竞态和同键幂等 | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | protocol/runtime、Agent、Viewer、QA；沿用本节六个专业合同入口 | 不证明各入口完整 UI 消费、实际实现或跨会话持久性 |
| Trace refs: O7-REQ-36cc9e9d-eaa7-436f-b16b-6f6af089b04c；[REQ-PROMPT-011](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-011) | runtime_instance 应用，不推导持久化或同步 | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | protocol/runtime、Agent、Viewer、QA；沿用本节六个专业合同入口 | 不证明各入口完整 UI 消费、实际实现或跨会话持久性 |
| [REQ-PROMPT-003](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-003)；Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d | 五源中的权威 default/profile/effective 生产者、完整 patch/no-op 与清除含义 | [授权值投影](#prompt-authorized-value-projection) | Runtime/Agent runner；Viewer；QA | 不改原 patch/权限，不扩大 ProviderBacked 或持久化/同步 |
| [REQ-PROMPT-008](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-008)；Trace refs: O7-REQ-a9797fdb-39f7-48fb-9202-9700d9d86159 | 当前读权、同版本关联、合法 stale 最新值与失败恢复 | [授权值投影](#prompt-authorized-value-projection) | Runtime/Agent runner；Viewer；QA | 不改原 patch/权限，不扩大 ProviderBacked 或持久化/同步 |
| [REQ-PROMPT-001](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-001)；Trace refs: O7-REQ-14ddd9ba-2d94-48e8-b2e4-0dbae26ddc36 | 实际支持入口的目标、当前控制与迟到结果归属 | [跨入口接收](#prompt-command-entry-applicability) | 原 Runtime/身份 authority；实际 Web/native/API 适配器；QA | 不由可见对象/本地参数授予控制，不新增入口或协议 |
| [REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)；Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293 | 实际支持入口区分一次 Chat、preset、草稿与持续 profile/目标 | [跨入口接收](#prompt-command-entry-applicability) | 原 Chat/Prompt authority；实际适配器；QA | 不强制未提供的 preset/GUI，不改变 canonical Chat 意图效果 |
| [REQ-PROMPT-005](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-005)；Trace refs: O7-REQ-b6140b6c-ca2c-4734-84ce-7963f9385fff | 逐入口真实预览/回滚/回执/scope 与缺结果安全下一步 | [跨入口接收](#prompt-command-entry-applicability) | 原协议/ledger 与当前读权；实际适配器；QA | 不补造增强结果、持久化、跨实例重放或无能力入口的写支持 |

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d；[REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005) | [全入口适用性](viewer-control-plane-split-live-playback.design.md#preflight-family-applicability) | 逐入口同状态共享资格、权威缺失、漂移、只读与未知恢复 | 按本设计 PF-01～06 完整方法在同一实际 candidate/backend 扩充执行；[service 测试接点](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/auth_actions_collect_data_service.rs)接收同状态/无写入对照，其余真实生产者及消费者逐项接入并保留独立 oracle | 逐入口来源/参数/当前授权、准入原因、nonce与账本差量、原请求和对应真实表面 | 现有单一 CollectData fixture 不代签其他入口；本轮仅设计审读，方法待实施执行 |
| [Viewer readback](viewer-control-plane-split-live-playback.prd.md#viewer-infrastructure-readback-contract) | [Evidence join](viewer-control-plane-split-live-playback.design.md#viewer-infrastructure-evidence-join) | 同 candidate/world/receipt result/freshness/error 与 no-effect/read projection | [Viewer capture receiver](viewer-manual.manual.md#viewer-infrastructure-capture-receiver)：before/outage/fresh/recovery/retry/replay capture，关联同候选 authority oracle，按实际 provider/environment 和 S6 desktop+narrow 要求归档 | Future screenshots/console/state、source/journal/API readback、receipt lineage、freshness、count0/1 与 artifact hashes | 文档方法；无 provider/browser/runtime 执行，无 semantic/required/full pass |
| Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；[REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)| [隔离设计](viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation) | 匿名、A/B、同玩家多连接及身份切换后的反馈归属 | 在 [auth_actions_feedback.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/auth_actions_feedback.rs) 更新/补充独立 session 用例；同候选测试迟到结果、撤销、轮换、公共故障恢复，检查每份 snapshot 和原 correlation；CollectData 同类用例位于 `auth_actions_collect_data.rs` | 实现 PR 的候选、测试输出与 A/B/匿名前后反馈；可见行为按 S6 补浏览器证据 | 回归必须检出旧共享写入及重放越权；真实多人 E2E 需另行执行；专业合同：[反馈归属](viewer-control-plane-split-live-playback.prd.md#player-scoped-gameplay-feedback) |
| Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4；[REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)| [错误码设计](viewer-control-plane-split-live-playback.design.md#gameplay-error-code-preservation) | 合法鉴权、非法 action/target/actor 返回具体 builder code | 按 [测试手册 S3](../../../testing-manual.md) 运行 `oasis7_chain_runtime` 的 `gameplay_submit_api_tests`：合法签名断言 400 与精确 code，保留解析失败、鉴权失败、nonce replay 和成功提交样例 | 实现 PR 对实际 HTTP response 的断言、候选与结果 | 仅测非法签名不足以证明 builder code 保真；部署环境验收与单元结果分别记录；专业合同：[错误码合同](viewer-control-plane-split-live-playback.prd.md#gameplay-error-code-contract) |
| Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d；[REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005)| [预检设计](viewer-control-plane-split-live-playback.design.md#collect-data-preflight-consistency) | 同一权威 projection 的零/一/多 claim 与提交资格一致 | 在 [auth_actions_collect_data.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/auth_actions_collect_data.rs) 运行下述矩阵并覆盖 `world_service/gameplay.rs` 的共享资格逻辑；显式启用 world-service，固定同一 projection/version；各请求使用其自己的合法认证 nonce | 实现 PR 的 canonical collector/reason、CommitRef、前后世界资源和 nonce 检查、入口结果 | 普通 Minimal 本地测试不代替 service 路径；服务测试不代替真实多人试玩，也不承诺预检后未来状态不变；专业合同：[预检合同](viewer-control-plane-split-live-playback.prd.md#collect-data-preflight-contract) |
| Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111；[REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004) | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | authority accepted 与 applied 的区分 | 按既有合同执行：同一候选 negotiated capability 下分别观察 preview、accepted 和 actual apply，核对 authority status、mutation_count 与玩家显示；preview/accepted 不以 ack 到达代签 mutation；具体运行入口依 [testing-manual S6](../../../testing-manual.md) 选择 protocol/runtime 与真实 Viewer 表面；这是验证方法，不声明测试已经存在或执行 | 实际实现 PR 的固定提交、结果与适用表面记录；本轮未核实 | 本轮无行为执行，不代签完整产品验收或发行 |
| Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd；[REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007) | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | control loss 优先、hidden 值与重新校验 | 按既有合同执行：同一候选编辑后使 binding/session 失效，再提交过期版本；核对 control_lost 优先、latest value/diff 隐藏、无 mutation 与安全 re-auth 下一步；具体运行入口依 [testing-manual S6](../../../testing-manual.md) 选择 protocol/runtime 与真实 Viewer 表面；这是验证方法，不声明测试已经存在或执行 | 实际实现 PR 的固定提交、结果与适用表面记录；本轮未核实 | 本轮无行为执行，不代签完整产品验收或发行 |
| Trace refs: O7-REQ-92124092-6021-49e4-8def-33038e20b637；[REQ-PROMPT-009](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-009) | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | 当前身份/版本再校验、竞态和同键幂等 | 按既有合同执行：同一候选两合法客户端在同版本提交，核对一个 applied 与一个 stale；同key同digest重试不得二次mutation，不同digest conflict，控制丢失竞态保持hidden blocked；具体运行入口依 [testing-manual S6](../../../testing-manual.md) 选择 protocol/runtime 与真实 Viewer 表面；这是验证方法，不声明测试已经存在或执行 | 实际实现 PR 的固定提交、结果与适用表面记录；本轮未核实 | 本轮无行为执行，不代签完整产品验收或发行 |
| Trace refs: O7-REQ-36cc9e9d-eaa7-436f-b16b-6f6af089b04c；[REQ-PROMPT-011](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-011) | [PromptControl system contract](viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract) | runtime_instance 应用，不推导持久化或同步 | 按既有合同执行：同一候选核对 actual apply 的 scope tuple 为 runtime_instance/none/none；重连、reload与旧instance重试不能把结果升级为永久保存或跨入口同步；具体运行入口依 [testing-manual S6](../../../testing-manual.md) 选择 protocol/runtime 与真实 Viewer 表面；这是验证方法，不声明测试已经存在或执行 | 实际实现 PR 的固定提交、结果与适用表面记录；本轮未核实 | 本轮无行为执行，不代签完整产品验收或发行 |
| [REQ-PROMPT-003](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-003)；Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d | [授权值投影](#prompt-authorized-value-projection) | 五源中的权威 default/profile/effective 生产者、完整 patch/no-op 与清除含义 | [来源/清除/恢复方法](viewer-manual.manual.md#prompt-source-clear-stale-validation)：PV01–PV10，固定真实配置/profile/runner及当前身份，在同候选逐层对照独立前后值 | source/权限/版本、profile与effective、完整影响及零/一次 mutation/driver 更新 | 方法待实施执行；原 snapshot、fixture 或文稿不代签完整 producer/真实表面 |
| [REQ-PROMPT-008](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-008)；Trace refs: O7-REQ-a9797fdb-39f7-48fb-9202-9700d9d86159 | [授权值投影](#prompt-authorized-value-projection) | 当前读权、同版本关联、合法 stale 最新值与失败恢复 | [来源/清除/恢复方法](viewer-manual.manual.md#prompt-source-clear-stale-validation)：PV01–PV10，固定真实配置/profile/runner及当前身份，在同候选逐层对照独立前后值 | source/权限/版本、profile与effective、完整影响及零/一次 mutation/driver 更新 | 方法待实施执行；原 snapshot、fixture 或文稿不代签完整 producer/真实表面 |
| [REQ-PROMPT-001](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-001)；Trace refs: O7-REQ-14ddd9ba-2d94-48e8-b2e4-0dbae26ddc36 | [跨入口接收](#prompt-command-entry-applicability) | 进入/切目标的当前控制与原结果归属 | [跨入口方法](viewer-manual.manual.md#prompt-command-entry-validation)：PV13，固定 catalog、真实身份/目标，合法/失权/迟到结果独立对照 | 每实际组合的目标/权限读回、签名和原关联、零/一次实际请求及可见状态 | 方法待执行；本地参数、退役表面或无 route 包装不代签支持 |
| [REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)；Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293 | [跨入口接收](#prompt-command-entry-applicability) | 一次 Chat/preset/草稿/profile 的来源和提交分离 | [跨入口方法](viewer-manual.manual.md#prompt-command-entry-validation)：PV14，固定不同文本、profile 及独立请求/变更计数，核恢复不自动重投 | 实际消费者输出与 Chat/Prompt 各自原件、profile 前后版本、草稿隔离 | 不强制未提供的 preset/编辑器；Chat receipt 不代签世界效果 |
| [REQ-PROMPT-005](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-005)；Trace refs: O7-REQ-b6140b6c-ca2c-4734-84ce-7963f9385fff | [跨入口接收](#prompt-command-entry-applicability) | legacy/enhanced/无能力组合的真实 scope、回滚和未知结果 | [跨入口方法](viewer-manual.manual.md#prompt-command-entry-validation)：PV15，真实协商/路由、合法 target、缺 ACK/换 epoch/恢复再失败的独立 oracle | 实际回包/原件与 ledger/mutation 差量、只读和零新写、每组合能力说明 | 不由单元 fixture 代签 UI/CLI 完成、持久化或跨入口同步 |

## Gameplay 反馈归属与提交资格设计

Owner 为 `viewer_engineer` 与 `runtime_engineer`，`qa_engineer` 核对场景。基线 `fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e` 的静态审读确认：[请求分发](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live.rs)、[CollectData 分发](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/collect_data.rs) 将错误写入共享反馈，[快照生成](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_blocking.rs) 再读取该反馈。兼容 [HTTP adapter](../../../crates/oasis7_chain_runtime/src/host/gameplay_submit_api.rs) 丢弃 builder code；预检尚未复用 canonical claim 解析。

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
5. standalone/legacy chain 的 Gameplay 与 CollectData 受理后，在仍保留的原反馈上下文关联实际 `runtime_action_id`。[Runtime 反馈更新](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_feedback_runtime.rs) 只消费匹配 `caused_by` 或已关联 `job_id` 的 Runtime domain event：`ActionAccepted` 和无关世界推进不算完成，`FactoryBuildStarted`/`RecipeStarted` 转为 `executing`，`FactoryBuilt`/`RecipeCompleted` 依据同一 job 转为 `completed_advanced`（展示为 completed），即使后续完成事件的 `caused_by` 为空也不丢失关联。即时动作的明确结果、拒绝与生产阻塞只更新原动作；`AgentClaimReleaseRequested` 仅表示请求 `applied`，冷却和最终释放仍由 claim state 表达。终态清除动作关联；新请求、失效 fence 或展示淘汰均阻止旧事件改写/重建反馈。既有 `runtime_action_players` 继续供恢复和回滚归属使用。
6. Chat 的 `accepted` 和 Prompt apply/rollback 的 guidance 结果可保留在 `recent_feedback`，但不据此固定世界的 `execution_state`、因果信息或 `response_window_class`；这些值继续来自当前世界事实。请求成功不能证明 Agent 已执行后续意图。
7. 取消等待、关闭面板或断开只停止本地等待，不取消已经提交的世界动作。`OutcomeUnknown` 沿原请求 lookup 确认，不能换 nonce 重交；仅有最终权威结果才能说明世界效果。世界故障与请求拒绝不得制造或覆盖无关动作的 Runtime 结果。

内部上下文与缓存不加入 legacy 必填 wire 字段，不承担重启恢复。复用 [session policy](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/session_policy.rs)、[owner read 失效处理](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/owner_read.rs) 和 [world-service correlation](../../../crates/oasis7/src/world_service/correlation.rs)；未授权响应保留通用诊断，不附带其他玩家身份、私有 trace 或世界内部细节。

<a id="gameplay-original-result-lookup"></a>
### 浏览器与 Runtime 重启后的原请求回读

Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；O7-REQ-a15a1b8e-8b7f-4c6a-9cb5-2f949730496c。承接[个人反馈唯一声明](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)、[SC-9 唯一声明](../../product/player-entry-distribution/prd.md#sc-entry-009)及[反馈隔离](#gameplay-feedback-isolation)，不新增游戏动作或提交权。

浏览器恢复加密胶囊中的原 `GameplayActionRequest` 后，不能只恢复 unknown 文案并等待已经丢失的 Runtime 内存队列。双方协商 `gameplay_result_lookup_v1` 后，浏览器发送只读 `lookup_gameplay_action`，携带原签名请求，不换 nonce、payload 或目标。按原请求和当前身份限频回读；旧 Runtime 未协商能力时保留未知，不发送不兼容帧。Runtime 的 `gameplay_action_result` 关联 player、action、target 与原 auth nonce，响应沿当前连接返回；查询不能进入 action submit 或消费原 action nonce。

成功的、经过现有 nonce/授权校验的 signed `RegisterSession` 可为该 socket 建立当前 gameplay 读取身份；仅声明玩家 ID 的 `ReconnectSync` 仍清除该身份。查询前验证原请求签名、当前 session key/hosted 授权与当前 socket owner fence；锁外使用既有 WorldService verified lookup，回包前重查同一 fence。旧 key、撤销、跨玩家、world/branch/reorg 或绑定变化均不得借原请求扩大权限。

原请求胶囊的持久恢复 scope 绑定 endpoint、world、source fence 与 reorg epoch；Runtime 本地 Prompt authority epoch 不属于该持久身份，兼容已保存的旧 epoch 胶囊。当前发送和在途查询仍核对完整实时 scope、当前身份与连接，旧 epoch 的迟到结果不能越过当前查询 fence。

浏览器只采纳当前 scope、身份、原请求及当前查询匹配的结果。只有 committed、rejected 或 expired 的权威结果允许结束原胶囊；Unknown、HistoryUnavailable、限流、传输失败与查询权限失败均保留原请求及真实未知。查询错误不能冒充原操作 rejected。旧 received/pending 快照不能覆盖已查明终态。committed 只证明原请求处置，Agent 归属和后续玩法效果仍以当前 canonical 视图为准。此通道不自动重投 volatile Received 后丢失的原请求，不把查询 Unknown 当成可重领证明。

验证覆盖浏览器胶囊重载、Runtime 空待查询队列、原 identity 回读与同一回执关联；signed registration 与 asserted reconnect 的边界；查询前及返回前的撤销/身份变化；查询始终零 submit；查询受阻不清胶囊及旧 admission 快照不降级终态。生产原请求是否仍有可查询历史与所需权限，须在实际环境另行核对，局部测试不代签真实试玩。

<a id="gameplay-error-code-preservation"></a>
### HTTP adapter 保留 typed builder 错误

`authorize_chain_gameplay_submit` 的私有错误类型从 `(u16, &'static str, String)` 改为拥有 code 的 `(u16, String, String)`。builder 失败保留 `(400, err.code, err.message)`，常量 code 转为 owned string，`write_gameplay_submit_error` 继续接收 `&str`。不从 message 反向推导 code，也不引入静态字符串泄漏。

JSON 的 `error_code` 仍为字符串，HTTP 状态类别与响应 shape 保持原合同。只有验签成功后的 builder 业务错误进入保真分支；解析失败仍可返回 `invalid_request`，鉴权失败与 nonce replay 继续原规则。客户端对已知 code 解释可执行下一步，对未知 code 安全降级；不得靠错误文本决定是否重试，也不得向未鉴权连接先暴露业务校验细节。

<a id="collect-data-preflight-consistency"></a>
### 共享只读 canonical 资格解析

从现有 claim 判断提取小型纯函数，输入为不可变 canonical `WorldState` 与验签所得 player/key，输出唯一 collector Agent 或 typed 拒绝原因。零份与重复匹配分别返回 `CollectDataQualificationError`；Agent 是否存在及资源是否足够继续在消费者的 quote/execution 层检查，不能取第一项或自动补 claim。Viewer 的 service preflight、world-service submit 和仍支持的兼容 HTTP CollectData adapter 复用该函数。

`oasis7_world_service::projection::from_world` 已在[字段 allowlist](../../../crates/oasis7_world_service/src/projection.rs)中保留 `starter_oc_claims` 与 Agent 资源，因此预检使用当前获准的 [VerifiedWorldView](../../../crates/oasis7_game_client/src/verified_world_view.rs)，无需扩大公开字段或新建 owner endpoint。一次计算固定其 projection 和 `ProjectionVersion/CommitRef`，claim、collector 和电力余额都来自该不可变视图。缺少可用权威视图、隔离身份不匹配或现行 freshness 条件不满足时返回需刷新/不可判断，不把未知数据当成空 claim 或默认资格；固定本次计算输入不等于要求开放历史 owner-read。

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

### 精炼报价的权威边界

`quote_refine_compound` 的兼容本地 Runtime 入口可继续从自己的只读状态与 Simulator 经济配置生成预览；验签和现有 session/owner 资格保持不变。链连接或 world-service 模式不能把 disposable 本地 World、sidecar binding 与本地 `snapshot_config.economy` 组合成当前 canonical 精炼报价。当前已验证 projection 虽包含 Agent 资源，但并不携带这组 Simulator 精炼经济参数，也没有正式精炼 quote service 合同，因此在合法身份验签和 session 校验后返回稳定的 `refine_quote_unavailable`，不消费 nonce、不修改资源或绑定，不将缺能力伪装成绑定丢失。

消费者应明确说明该世界暂不提供权威精炼预览，保留刷新权威世界和查看已支持生产方案的入口；刷新不保证能力出现，不引导玩家盲目重新登录或按本地估值提交。未来补齐正式 quote 合同、权威参数及同版本资源/owner 资格后才能恢复该世界的精炼预览。本边界不撤销产品的提交前预览要求，也不证明该要求已交付。

<a id="preflight-family-applicability"></a>
### 已提供预检的行动范围与共同资格

Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d；承接 [REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005) 与原 [AC-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#ac-agent-surface-005)。本节补齐原要求的适用性，沿用既有行动和领域权威，不要求所有行动新增 quote endpoint。以下是完整目标合同及固定源码审读，不能由表格存在推定消费者已经接入。

**判定单位。** 对每一个实际入口分别登记 world/backend/profile、行动种类、经验证主体及其当前 owner/claim/session fence、完整候选参数、实际执行 authority 与版本/commit cut。配置预览使用其原配置 authority/epoch/version，不伪装成世界 canonical commit。预检必须从该执行 authority 取得与真实准入相同的、无业务副作用且有界的共享资格计算；临时 Kernel、seed、sidecar 或另一 backend 的可运行同名动作不自动获得当前世界的资格。一个签名请求通过认证也不证明来源为 canonical。缺少所需 authority、参数或当前读权时，该资格部分为不可确认或阻塞，不能用零余额、旧值或兼容本地数据补齐。

预览拆开三类事实：当前共享资格、条件后果/价格/时长估计、当前入口是否提供实际提交。仅共享资格相同的部分要求检查与准入一致；估计、未来执行以及需实际模块运行才能知道的结果保留原专业边界。[已知产物限制](../m4/industrial-resource-flow-contract.design.md#known-product-stack-admission)只对可信规则的已知限制下确定结论，未知 validator 规则不被伪造成已通过，也不因此擅自新增全局禁止准入规则。UI 字段名含 `Preflight`、`submission_allowed` 或 `submission_feasible` 均不能越过其真实计算范围。

| 已有入口族与生产者 | 同状态下必须共用的资格与绑定 | 当前适用边界及消费者要求 |
| --- | --- | --- |
| 安全替代候选：[正式入口](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/safe_alternative.rs)、[候选合同](../../world-runtime/runtime/safe-canonical-alternative-confirmation.design.md#alternative-candidates)与[明确确认](../../world-runtime/runtime/safe-canonical-alternative-confirmation.design.md#alternative-confirmation) | 原不支持请求必须先确认无未知执行承诺；候选绑定当前 owner/Agent/world、原请求、set generation、完整候选与安全/成本/风险，Confirm 与 Execute 各自重验实际范围 | Propose/Reevaluate 是可记录候选审计元数据的控制事务，候选预览不取得 hold、队列资格或玩法效果；Confirm 仍不是执行。按原合同登记唯一确认/执行，相关漂移需明确重评，未知先核原 lineage，不因只读比较替玩家确认或另建执行 |
| PromptControl Preview：[原系统合同](#promptcontrol-system-contract)、[增强生产者](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/prompt_control_enhanced.rs)、[legacy Runtime](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/prompt_control_legacy.rs)及 [LiveWorld](../../../crates/oasis7_viewer_runtime/src/viewer/live/live_auth_prompt.rs) | 原经验证 player/Agent/control、完整 patch、expected_version；增强入口另绑定当前 authority/session/binding epoch 与规范化 operation digest；preview 与 apply 共用当前补丁/版本资格 | 只计算候选配置，不写实际 profile/driver；各自支持的 legacy/runtime/正式 provider 能力独立说明，不把模拟或 preview 变成 applied/持久保存。现有认证允许消费 preview 自己的 nonce并记录其有界结果，不能消费另一正式 apply 的 nonce；重启后原 epoch 不可用保持原 result_unknown，不借 gameplay lookup 假造配置恢复 |
| CollectData：[只读资格](#collect-data-preflight-consistency)、[请求入口](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/collect_data.rs) | 同一 verified projection 的唯一 canonical collector、当前身份/claim、实际 Data 收集动作的资源和规则；零/多 claim 保留原原因 | 本地和 world-service 各用自己的执行 authority；provider ready、claim 资格与收集条件分别表达，预检不消耗正式提交 nonce |
| 精炼：[生产者](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/player_gameplay.rs)、[M4 计算](../m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design) | 同一原料/数量/精炼规则与资源账，净 Data 成本及适用身份 | 兼容本地只对其自身执行链提供估值；chain/world-service 缺少正式参数端口时沿前节 `refine_quote_unavailable`，不显示 canonical 可提交 |
| 排产：[Viewer 生产者](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/schedule_recipe_quote.rs)、[原生配方报价](../llm/external-agent-game-api.design.md#当前实施候选原生权威配方报价与排产) | requester/factory builder、实际 factory 输入账、配方/批数、实际计划、电力付款人、运行规则及确定可检查的 output 限制 | Viewer 兼容 Kernel 计算不能代签远程 authority；原生已签名 recipe-quote 按当前 canonical pin/任务授权复用真实准入。quote、submit 和实际计划沿[同方案绑定](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)，world supply 可覆盖不等于 factory input 已到位 |
| 建厂：[原生 factory-quote](../llm/external-agent-game-api.design.md#当前实施候选建厂到首批生产) | requester、目标位置/类型、成本和厂位、当前区域/控制/任务资格与当前原生建厂规则 | 原生读取在返回前重核 pin 与授权；只对实际支持的建厂路径承诺已检查范围，不借原生端口推定 Viewer 各模式都有建厂预检 |
| 运输：[生产者](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/transfer_material_quote.rs) | requester、from/to ledger、资源/数量、实际 route/priority/reroute 参数，当前 source、容量、路径/关税与权限 | `submission_feasible` 只在同一真实执行来源与完整参数绑定成立时表示已检查条件；预计到达/损耗不是入账回执，预检不预留材料或运力 |
| 售电：[生产者](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/power_sale_quote.rs) | 实际 seller/buyer、数量、限价、当前余额及适用经济规则；安全余量与收益计算分开 | 当前入口从本地 World/sidecar/seed 投影至 Kernel；此事实不证明远程 SellPower 可提交。消费者保留条件估计和其支持模式，不把推荐行动升级为新动作能力 |
| 续航/恢复：[生产者](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/power_survival_quote.rs) | 明确区分 BuyPower 与 HarvestRadiation 的候选/付款人/数量/价格或时间条件；使用各自真实执行来源 | 当前亦为本地 Kernel 投影；续航预测不保证未来供给，恢复建议不证明当前远程入口有相应动作。不能把环境供能、世界 Electricity 和其他私有能量混账 |
| Product 校验：[生产者](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/player_gameplay.rs) | 实际 product/amount、当前 actor、所选 module/validator 版本及该路径实际可验证的条件 | 当前 world-service 缺少执行 quote 端口时返回 `product_validation_quote_unavailable`；本地校验不证明后续入库、模块授权或一次生产已完成，实际 validator 执行沿原专业合同 |
| 市场材料估计：[生产者](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/market_quote_decision.rs) | actor 的材料账与请求消耗向量，当前局部/世界供给、运输和税参数 | 当前 `submission_allowed` 仅等于总材料缺口为零；它不是排产/建厂/交易的完整资格，不能直接解锁不同 ledger 的生产。选择具体行动后仍走其已有预检/准入 |
| fragment 补充预览：[生产者](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/fragment_refill_preview.rs) | 当前 backend 的补充能力、目标 chunk/frag 和权威剩余量/时序 | 当前本地 backend 不执行补充、service 缺权威投影时分别保留 disabled/unavailable；无玩家“等待即制造补充”的提交资格，不用初始 seed 预言后续资源 |
| 治理与战争：[投票](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/governance_vote_quote.rs)、[宣战](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/war_declaration_quote.rs) | 条件估计绑定 actor/proposal/option/weight 或双方/intensity、适用世界规则与来源 | 当前 [R-QUOTE-07](viewer-quote-forms-uiux.design.md#r-quote-07)/[08](viewer-quote-forms-uiux.design.md#r-quote-08) 的提交未开放；估计不制造投票或宣战权。War 的 Defer 决策不由已有 quote 解除 |
| 社会五动作及联系：[生产者](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/social_quote.rs)、[共享资格](../kernel/social-fact-ledger-declarative-reputation.design.md#social-obligation-1) | 声明、发布、裁决、撤销及其专业预览目录逐项绑定实际候选与[来源截面](../kernel/social-fact-ledger-declarative-reputation.design.md#social-preview-source)，当前读权与新动作权分开 | Viewer 已有 route 与五动作专业目标分别登记；实际未接通的 Agent Action/WorldRule 不借 quote 冒充网络提交。联系估计不自动产生边或事实；采用消费者与未采用者按原专业目录说明 |

**状态变化与消费顺序。** 预检响应绑定原查询代次、候选 digest、身份/world/backend 与来源 cut；消费者改变任何参数、目标、身份或世界后即失效，迟到响应只能归原查询。返回前重核当前读权；切换身份或撤权后不返回旧私有正文。提交使用当前正式动作的独立认证及其原幂等合同，重新读取当时 authority 并运行同一共享判定；quote 不占正式提交 nonce、不预留资源、不替玩家签名，也不承诺版本被锁定。预览自身的认证结果与 SafeAlternative 候选/控制审计按各自原合同记录，不等同玩法效果；纯 quote 的零写入条件不能反向删除这些必要元数据。状态漂移时解释当前实际原因和可以修正、重报、等待或安全返回的方向，不能仅以“上次通过”继续发送旧参数。

对于已送出而结果未知的动作，重新预检只能描述另一时点的条件，不能证明原动作未生效。沿[原请求查询](#gameplay-original-result-lookup)和该行动的原操作合同核对；缺查询能力或当前权限时保持未知/受阻，不能通过新 nonce、重新报价或模式切换消除未知。取消只读预检释放该读的临时资源，不取消已送出的动作。

**入口清单与分类边界。** 本次从完整 `ViewerRequest` 及嵌套命令核得 14 个 `Quote*`、`PreviewFragmentReplenishment`、`CollectData::Preflight`、`PromptControl::Preview` 和 `SafeAlternative::{Propose,Reevaluate}`；再接收上表独立原生 factory/recipe 端口。清单仅证明此固定基线的实际入口分类，不以某个目录数量冻结未来功能。`RequestWorldView`、owner-read、Feed/Snapshot 及 `LookupGameplayAction` 是状态/原结果读取；Live/Playback、Chat、GameplayAction 和 Recovery/Challenge 的正式控制动作不因内部存在校验就新增玩家预检义务。已有 resource-basis 的 Transfer/Build/Schedule owner-resource 组件仍归原三类，fixture-only 或正式端口 unavailable 不能算当前支持的玩家来源。

`AuthoritativeRecovery::ReevaluateRollbackReadiness` 单独沿[原恢复 runbook](../../game/gameplay/gameplay-longrun-p0-replay-rollback-runbook-2026-03-06.md)及 [recovery 入口](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/recovery.rs)/[receipt 处理](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/recovery_receipt.rs)：它是 operator/governance 对原持久 receipt、target root/reorg、当前 registry、strict audit、补偿及归因的恢复状态重评，可提交 recovery generation/审计 nonce，不是本条玩家行动的纯预检。`GetRollbackReceipt` 是原授权读取；两者不授予新玩法能力，也不由本节放松 holding/all-clear 或 world-service 不支持边界。

**实施分工与变化接收。** Runtime/各领域 owner 提供既有真实 admission 的无业务副作用共享判定和当前来源；Viewer/GameClient owner 以本表逐入口登记“资格/估计/提交能力”及原 query correlation，QA 独立核对。新增实际预检或 backend 需要在同一变更更新本表及下述方法接收；未提供预检的行动保留原提交合同，不强制新增入口。2026-11-05 固定基线 `4f6f20ece923c5705e26ef879c2b83e0e637b3f9` 的上述代码审读仅证明现有生产路径及差异；各非 CollectData 入口的完整接线、负例与真实表面证据仍待实施验证。

<a id="preflight-applicability-validation"></a>
### 全入口预检资格的独立验证方法

这是待执行方法，适用同一明确的 source candidate/world/backend/配置与当前有效授权；不得将设计审读或现有测试文件存在记作通过。先从实际请求分发、[协议入口](../../../crates/oasis7_proto/src/viewer.rs)及原生端口导出入口清单，与上表逐项对账；对每个未提供实际提交的估计入口也记录理由及禁用/替代行为，不能从成功样例抽样后宣布全部覆盖。

1. **PF-01 同状态资格。** 对每个支持实际提交且提供预检的入口，以真实公共入口建立状态，固定同一 authority cut，分别在两个独立副本运行预检与真实准入；分别覆盖合格、不合格和无法确认来源。独立 oracle 从原始权限/claim、输入账、计划及当前规则计算共享条件，核对结论与具体原因；预检不改变资源、claim、job、玩法效果 journal/receipt 或实际 Prompt profile/driver，也不消费正式提交的 nonce。各请求使用自己的合法认证；Prompt 预览按原协议推进自身 auth nonce/有界结果账是认证处理，单列核对，不能误判为世界/配置已应用，不能将其改成无认证读。SafeAlternative 候选/确认的控制记录同样单独核其合法新增；按 [ALT-VERIFY-001](../../world-runtime/runtime/safe-canonical-alternative-confirmation.design.md#alt-verify-001)对比资源/hold/队列资格/玩法状态，不要求整个 canonical root 不变。不可复制预检结果当提交期望值。源入口包括 [CollectData fixture](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/auth_actions_collect_data.rs)、[service fixture](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/auth_actions_collect_data_service.rs)、[排产 fixture](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/schedule_recipe_quote.rs)、[Prompt 预览 fixture](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/prompt_control_enhanced_preview.rs)及各上表生产者对应真实准入路径；现有 fixture 未覆盖者由实施 PR 补足。
2. **PF-02 参数与 authority。** 分别换主体/绑定、目标/ledger/数量、backend/world、模块版本和来源 cut；本地值与 canonical 值故意不同。核对不混读、不用 seed 补缺、不用公共余额替代实际付款账；排产特别对照 world supply 足够但 factory input 不足，以及已知/未知 validator 限制。原生 recipe/factory quote 核对读前与返回前 pin/授权；中途失效不得返回旧私有正文或资格。
3. **PF-03 预览分类。** 独立输入市场缺口零但具体排产不合格、售电/续航推荐但当前模式无提交、fragment disabled/unavailable、治理/战争禁用，以及社会 quote 可读而真实动作未接通。逐项核对估计仍可有用，但不出现不实可提交、成功或自动切模式；采用 [R-QUOTE-01～10](viewer-quote-forms-uiux.design.md) 的原字段/能力限制和[社会恢复方法](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-06)。
4. **PF-04 漂移与乱序。** 在检查后改变资源、授权、控制/claim、规则或行动参数，再真实提交；提交按当前事实拒绝/重验，旧响应不覆盖新候选。确认 quote 未预留资源或消费正式 nonce，提交使用自身合法认证。记录原候选、两次 cut、拒绝原因与实际世界差量，不能只核 UI 文案。
5. **PF-05 已提交未知。** 让真实动作送出后丢响应、重启或断连，再运行 quote 与原件只读 lookup；分别覆盖已 committed、未确认、历史/权限不可用。核对 quote 永不重送，原 operation/digest 不变，效果次数最多为原合同允许的一次，无法查询保持未知。关闭预览与查询失败不清除原请求。
6. **PF-06 表面等价。** 按 [Viewer QA 清单](viewer-manual.manual.md#target-qa-evidence-checklist)及 [LH-REVIEW](viewer-localization-and-context-help.manual.md#lh-review) 在实际适用 Web/native/原生客户端、桌面/窄屏与中英文分别读取同一原输入、typed 原因、来源和结果；预检、已受理、世界结果、能力不支持、连接中断分别表达，关键事实和安全下一步不因折叠/缺译丢失。没有该入口或环境时记录具体未覆盖项，不以其他 backend 的 fixture 代签。

实施证据随原 PR 记录逐入口 candidate、有效配置、原请求/来源/资格 oracle、nonce/账本前后值、世界结果与真实表面 captures。静态映射完成只证明已定义这些义务和方法，不证明代码、真实多人试玩、性能或生产环境已通过。


### 实施、兼容与未决边界

- 原 #2116/#2277/#2481 的反馈隔离、错误码、CollectData 资格共享三个局部修改保留原交付范围；本轮 [全入口适用性](#preflight-family-applicability) 单独承接产品已有预检义务，按各实际 producer/consumer 补接，不把历史局部交付视为全部 quote 已完成。渲染效率与视觉回归由[独立局部设计](viewer-pixel-world-bridge-rendering.design.md#render-reconcile-and-visual-regression-target)承担。
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

7. 自动恢复的次数属于原逻辑操作上下文，不属于可替换的 feedback 或 timer。对于当前请求明确返回且仍有授权的可恢复 `session_not_found`，同一逻辑操作至多执行一次自动会话恢复；替换反馈、重签名或重建等待实例不得重置预算。再次失败停止自动循环，保留原因和安全下一步。原副作用结果未知时先按原身份查询，不能把会话恢复当作重新提交世界效果的许可；用户后续显式操作仍须重新核验当前授权与原结果。

本条承接既有 [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004) 的安全下一步要求（Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4），补充恢复预算生命周期，不改变错误码、签名或世界准入。验收以 fake clock/受控 transport 连续返回两次 `session_not_found`，同时替换 feedback、注入迟到响应：自动恢复最多一次、第二次有终止反馈，unknown 路径无新副作用提交。实际测试结果由实现 PR 提供。

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
- PromptControl authority 与签名：`crates/oasis7_client_api/src/viewer_auth.rs`、`crates/oasis7_viewer_runtime/src/viewer/runtime_live.rs`、`crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane.rs`、`crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/prompt_profile.rs`、`crates/oasis7_viewer_runtime/src/viewer/runtime_live/llm_sidecar.rs`。
- 表现/自动化：`crates/oasis7_viewer/software_safe_src/{legacy_core.js,legacy_core_control_gate.test.js}`；生成浏览器入口为 `crates/oasis7_viewer/viewer.js`。
- PromptControl 浏览器签名与反馈 consumer 还需对齐 `crates/oasis7_viewer/software_safe_src/{main.jsx,viewer_feedback_module.js,viewer_auth_surface_module.js,software_safe_state.js}`；runner/QA 使用真实 backend-issued grant 与 runtime trace。
- 本设计不改变世界规则、P2P/共识回退策略、视觉方向或测试发布判断；实现 leaf 不得在此 system leaf 中合并。

### Signed request response correlation

Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6; O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4; O7-REQ-92124092-6021-49e4-8def-33038e20b637. This refines the existing player feedback isolation, typed failure and request identity contracts above.

Gameplay ACK `submission_status` preserves WorldService `received`, `pending` and `committed` dispositions; none independently grants owner control or proves a gameplay effect. Typed rejection is distinct from unknown/unavailable original outcome. Gameplay, chat and signed PromptControl ACK/error envelopes carry `request_auth_nonce` copied from the original request proof, as correlation metadata only; it never grants authorization. A client with an outstanding signed request accepts only a matching nonce, action and target from the current connection. Quote responses use the same envelope correlation. Scoped gameplay `recent_feedback.request_auth_nonce` retains the original verified nonce through execution and terminal result updates, so the matching authoritative result supersedes temporary transport feedback without replacing a newer same-action request. Legacy responses without correlation cannot terminate a newer correlated operation. Missing ACK, disconnect and timeout preserve the original signed request for read-only reconciliation; they do not authorize a new submission.

Hosted Agent prepare/publish 不得阻塞共享观察锁；具体 single-ticket、publication generation、
原胶囊先发布后发送与 scope fence 合同见 [Runtime cognition preparation](../../world-runtime/runtime/agent-cognition-lifecycle.design.md#hosted-preparation-and-private-checkpoint-publication)。Viewer Pause 与独立观察连接不得被旧准备结果覆盖。
