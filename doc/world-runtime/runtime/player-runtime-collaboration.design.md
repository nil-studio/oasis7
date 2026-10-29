# 玩家与外部 Runtime 持久协作系统设计

- 设计状态：target contract，待实现与独立评审；不是当前能力或发布报告。
- Canonical repository：`eng-cc/oasis7`；审读基线：`03fb58784a65de9f02a43dfeafc3e882bb04e5f2`（已合入产品 PR #4493）。
- Owner role：`runtime_engineer`；最后审读：2026-10-10；新增合同 revision：`1`。
- 产品入口：[外部 Runtime 游玩](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)；[玩家与 Runtime 协作](../../product/agents-world-simulation/player-runtime-collaboration.prd.md)。
- 系统总入口：[LLM / external runtime 系统入口](../../world-simulator/llm/README.md)；写作规范：[系统设计规范](../../engineering/doc-governance/system-design-writing-standard.design.md)。

本文中的新组件、路由、记录和测试场景均为后续实施输入。`accepted` 只表示所属权威已受理，世界效果必须读取既有权威回执；文档合入不代表这些接口已经上线。

## 1. 问题、目标与非目标

Web 发布目标不等于 Codex/OpenClaw 已收到或世界已执行。需要可跨设备、跨执行进程恢复的游戏任务，明确目标、消息、审批与当前执行方，且不能产生第二个拥有世界写权的任务平台。

本设计负责 task/goal/executor fence、协作消息、授权申请、版本冲突、持久化与补送。世界动作和 grant 的最终判定仍使用现有 Runtime；模型上下文属于外部 Runtime，跨 Runtime 私有记忆无损迁移、无限期托管和把聊天解析成批准不在范围内。

## 2. 上游约束与相关角色

Runtime owner 负责世界侧 control fence、顺序、授权及恢复；Agent owner 负责实际执行器和上下文关联；Viewer owner 区分状态与来源；QA 核验迟到、重复、撤销和掉电窗口。本文是产品协作分册的技术承接，不改变 Prompt 的 accepted/applied 语义。

### 2.1 需求承接与分配表

| 上游条款 | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 排除与未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-091d6c22-fca5-4284-91dd-2edce9ef440b；[AC-EXT-006](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-006) | 目标调整真正进入有效上下文，旧待决结果独立处理 | [collab-goal](#collab-goal) | agent_engineer / runtime_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；[AC-EXT-008](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-008) | 断连重启保留原世界任务且不复活失效授权 | [collab-recovery](#collab-recovery) | agent_engineer / runtime_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-62007da5-b12a-4b8c-98bb-82ab53a35069；[AC-COLLAB-001](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-001) | 跨设备共享任务并隔离被替换的执行方 | [collab-binding](#collab-binding) | runtime_engineer / agent_engineer / viewer_engineer | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-edd52061-73c0-433c-9531-30d86b1db459；[AC-COLLAB-002](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-002) | 保存、读取、采纳、用于决策分别可验证 | [collab-goal](#collab-goal) | runtime_engineer / agent_engineer / viewer_engineer | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-b100c55e-5c35-46dc-aaf6-e66c02b323d8；[AC-COLLAB-003](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-003) | 计划和阶段汇报不能冒充权威成果 | [collab-content](#collab-content) | runtime_engineer / agent_engineer / viewer_engineer | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-2f463269-c3c0-4da1-95f8-90011c35799a；[AC-COLLAB-004](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-004) | 问题/回答关联原任务和目标，旧问题不改新目标 | [collab-content](#collab-content) | runtime_engineer / agent_engineer / viewer_engineer | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-f5341eef-d8f6-42ae-b7b6-751803abd588；[AC-COLLAB-005](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-005) | 有效 owner 批准与累积预算在执行时重验 | [collab-approval](#collab-approval) | runtime_engineer / agent_engineer / viewer_engineer | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-6044cb04-5fbb-4c68-81a6-fc57cfdd608a；[AC-COLLAB-006](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-006) | 重复消息、迟到和历史缺口安全恢复 | [collab-recovery](#collab-recovery) | runtime_engineer / agent_engineer / viewer_engineer | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-8dad5caa-d064-4ac8-8c98-40a702b13685；[AC-COLLAB-007](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-007) | 同一任务中证明离开 Viewer、停止、离线改目标、续接、问答、明确授权及世界结果 | [collab-runtime-journey](#collab-runtime-journey) | runtime_engineer / agent_engineer / viewer_engineer | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-d30925fe-d798-4056-9641-062255931a61；[AC-EXT-002](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-002) | 同 Runtime 自有/受托资源隔离，撤销不扩散 | [collab-relationship-isolation](#collab-relationship-isolation) | runtime_engineer / agent_engineer | 不把同一进程视为 owner 授权，不新增经济角色 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线 | 目标 / 未闭合部分 |
| --- | --- | --- |
| GoalSnapshot | `crates/oasis7_agent_api/src/lib.rs` 导出目标与上下文合同；Harness PRD §9 描述 revision/digest | 承接 owner 发布版本、activation、executor read/adopt 与新决策使用，不以同一布尔值替代 |
| 生命周期 | `agent-cognition-lifecycle.design.md` 有 turn、envelope、journal、wake 与 recovery 合同 | 增加任务控制元数据及跨消息恢复；不宣布原合同所有 target 已实现 |
| 玩家控制 | Prompt 产品 PRD 约束 control-lost、stale draft 和 accepted/applied | 新协作端点必须使用同一 owner 校验和冲突规则，不绕过旧边界 |
| 权威运行 | Viewer 手册记录 chain runtime 单独管理持久 execution-world/checkpoint | 协作影响动作许可的部分必须进入相同权威写序，不能由网关内存批准 |

## 4. 边界与结构

<a id="collab-binding"></a>
### 4.1 唯一控制记录

每个 world/Agent 首期最多一个 active task 和一个 active decision executor。`TaskControl` 由当前世界权威维护，包含 task_id、owner/control revision、active_goal_revision、active_goal_reference、grant_revision、executor_id、executor_epoch、task_status 与最后控制提交引用。公开目标引用保存不透明的私有盐承诺与准备/发布操作关联，不保存正文 digest；私有 digest 仅在受权正文投影内使用。

`executor_epoch` 是 Runtime 持久单调 fence，不是客户端时间、token expiry 或内部 session ID。初次绑定、重新接管、替换及 stop 均经 owner 授权并形成新的 control revision；旧 epoch 即使仍持有未过期 token 也不能发起新副作用。不同执行进程可以共享只读历史但不能并行占有一个 Agent；内部多工具不获得独立 fence。

身份服务证明“调用者是谁”；TaskControl 证明“谁现在可以替该 Agent 作出提案”。读取历史仍须按当前 owner/audience 判定，转让后不默认向新执行方披露旧 owner 私有消息。玩家所有权转让/撤销的生效点消费现有专业合同，不重新定义资产权利。

<a id="collab-relationship-isolation"></a>
### 4.1.1 自有与受托的同协议隔离

Trace refs: O7-REQ-d30925fe-d798-4056-9641-062255931a61；承接 [REQ-EXT-002](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-002) / [AC-EXT-002](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-002)。合法自主入场沿 [Game API bootstrap](../../world-simulator/llm/external-agent-game-api.design.md#api-autonomous-entry)取得关系，不修改本节的执行权限。

调用者类型或 Runtime 名称不是权限来源。每次 owner/control 操作验证该稳定主体在目标 world/Agent 的当前 canonical owner；每次 executor 读写验证真实 delegation 与完整 task/executor fence。一个 Runtime 同时服务自有 Agent A 与玩家 B 的 Agent B 时，各自的 owner/identity generation、delegation generation、task、goal、executor epoch、grant、预算与私有 audience 必须独立；任何 token、context、签名或 receipt 都不能跨资源复用。无需引入 `self_play / delegated_play` 的全局开关。

自有参与主体可以通过独立受保护 owner 路径创建 task、选择/发布目标及授予独立 executor；服务端仍重验当前 owner、目标版本、明确授权范围和适用确认条件。owner 控制请求须来自独立受保护控制面并满足明确的 owner policy，普通 executor 不能任意选择 owner 角色或访问其 signer。不能因 owner 与 executor 在同一软件里就用 owner key 充当 delegate，也不能给默认 executor 增加 goal 写入或审批能力。对他人角色，executor 只建议/申请，owner 决定继续走原专用控制路径。

撤销按授权实际作用域生效：仅撤销 B 的 delegation/任务执行资格时，拒绝旧 B fence 的新副作用并对账其 pending，不撤销 A 或其他独立委托；账号/凭据全局撤销仍按其真实范围影响全部关联资源。每 Agent 单 active task/decision executor 与现有 canonical writer 规则不变，不能以同 Runtime 多角色为理由绕过。验证使用 [api-auth 共存负例](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-auth)。

### 4.2 两类数据，不同权威

**权威控制元数据**：目标版本/不透明承诺引用、executor fence、grant 与预算、任务运行许可，进入 Runtime control journal/checkpoint，与世界行动按一致顺序重验。

**私有协作正文**：Prompt、计划、问题和回复使用 audience-scoped 的持久内容存储，密文/索引放在 Runtime 所属的受控存储域；世界公开快照仅保存授权可公开的承诺或不透明引用，不公开可枚举的私有正文指纹；私有 goal digest 只在受权上下文内使用，不复制明文到公共 P2P、广播事件或玩家公开世界流。首期复用现有执行世界存储代际机制，新增私有 namespace，不要求独立数据库或跨服务分布式事务。

报表、心跳和模型自报是 advisory。server 可以证明已接收，但不能据此证明模型真的理解了目标，或将自报“完成”写成生产成果。

## 5. 关键运行流程

<a id="collab-goal"></a>
### 5.1 目标发布到实际用于决策

1. 玩家读取 task snapshot，同时取得 expected goal/control revision。写入时先重验身份/控制权；control-lost 不返回新 owner 的内容或差异。
2. `PUT goal` 必须带 client_operation_id、expected revision、完整目标与允许的软约束。expected goal revision 比较 latest_goal_revision（包含已保存但尚未激活版本），task_revision 同时防止改绑竞态。旧基准 `409 goal_conflict`；保留草稿，不自动最后写覆盖。
3. 完整正文先持久化并校验 digest；提交 GoalVersionSaved 引用。目标在权威控制提交中激活后才更新 active_goal_revision；accepted/pending activation 与 active 分开返回。
4. 执行器首次接入、重启或 cursor reset 时先验证 snapshot 内联的激活目标正文、revision/digest 与切点；已建立基线后再增量读取变化，分别提交 `received` 与 `adoption_reported`，绑定 task、revision、executor_epoch 和独立消息 ID。旧确认可以保留历史，但不前推当前版本的状态。
5. Harness 捕获该激活版本的上下文。边界记录 `context_presented`；执行器返回合法提案、等待或阻塞时记录 `decision_submitted`，世界 admission 验证时记录 `decision_context_validated`。这些只证明版本关联，不能证明模型内部语义服从；界面不得把单纯自报升级为“已实际执行”。
6. 目标再次变化使旧 context 不可用于新的副作用；旧提案必须重新取得上下文并作出新决策。已经受理的待决动作按当时及提交时有效的授权与 Runtime 规则重验，已结算成果不撤销。

自然语言偏好与可强制授权分离。预算、对象范围、风险限制和撤销必须以 typed grant/control 字段表达并由 Runtime 校验，不能依赖模型阅读 Prompt 来强制执行。

<a id="collab-approval"></a>
### 5.2 申请、明确批准与世界执行

申请由 executor 提出，记录 task/Agent、当前 owner revision、goal revision、executor epoch、action/proposal digest、对象/资源来源、累计额度、有效期、风险及拒绝后路径。普通文本与“好/同意”只进入消息历史，不能生成 grant。

owner 决定调用专用 decisions 端点，带 expected request revision 与幂等 ID；当前认证、控制权、目标/申请适用性先于返回差异。审批有 `open / approved / rejected / expired / invalidated`；同一申请只能进入一个终态。超时不批准；目标、owner 或执行方变化默认使该目标绑定申请 invalidated，新的申请必须重新确认。

approved 表示权威决定已提交并产生引用现有授权体系的 grant，不代表行动成功。行动 admission/commit 重新检查 grant 是否仍有效、remaining cumulative allowance、资源来源、对象范围及其他前置；同一 grant 的并发预留和消费由 Runtime 原子执行，不能把每次上限误当总上限。失败释放预留必须关联原 reservation，不能负消费或释放两次。

授权撤销/stop 不等执行器 ack：从 Runtime 的有效提交点开始拒绝新受限提案。已经提交但未结算的高后果动作按既有 Runtime 主责重新评估；API 不能承诺所有任务瞬间物理停止、所有模型费用立即停止或已结算结果回滚。游戏授权与操作系统、模型账户、第三方工具授权保持独立。

<a id="collab-content"></a>
### 5.3 汇报、问答与任务完成

Report 包含 task、目标版本、来源执行方、reported_at/received_at、phase、summary、blocker、next_actor 与 evidence_refs。服务器只校验引用是否存在及读取权限，不信任任意 URL 或未经回读的 receipt 字符串。UI 清楚标记“执行器汇报/估计”和服务器接收时间；不保存或展示模型内部推理链与秘密。

Question/Answer 使用稳定 question_id、message_id、in_reply_to、goal revision 和 actor。提问或回答是协作消息，不写世界。旧目标的问题被新版本替代后，迟到答案留为 stale history；要影响新任务必须有明确的新问题或目标修改，不自动搬运批准。

可机器验证的任务结束依赖绑定的 gameplay completion evaluator 与权威 receipt。任意自由文本目标无法机械判断时只能 `completion_reported` 或由 owner 显式结束委托，不能称世界已证明完成。日常请求批准不应替代 Agent 自主选择；等待期间仅允许原授权内不规避 blocker 的独立行动。

## 6. 接口与数据合同

HTTP 路由由 Game API 设计拥有；本节定义它们调用的 RuntimePort 命令与记录。所有整数 revision/seq/epoch 用 JSON 十进制字符串，类型未知、内容超限和相同消息 ID 异内容一律拒绝。成功响应包含 operation/ref 和当前权威状态，不把内存写入当已持久化。

```text
TaskSnapshotV1 = {world_binding, task_id, agent_id, owner_revision,
  task_revision, latest_goal_revision, active_goal_revision, active_goal_digest, grant_revision,
  executor_binding, task_status, activation_operation?,
  active_goal: GoalVersionProjectionV1|null, goal_delivery, latest_reports, unanswered_questions, pending_approvals,
  unsettled_operation_refs, snapshot_seq, snapshot_id, page_index, total_counts,
  next_snapshot_cursor, resume_cursor, completeness: partial|complete}
GoalVersionProjectionV1 = {schema_revision, task_id, goal_revision,
  goal_digest, activation_commit_ref, content: GoalContentV1}
GoalContentV1 = {goal_text, soft_constraints: string[]}
ExecutorBindingV1 = {executor_id, executor_epoch, device_ref,
  permitted_scopes, activated_at_tick, revoked_at_tick?}
GoalDeliveryV1 = {goal_revision, executor_epoch, received_at?,
  adoption_reported_at?, context_ref?, decision_ref?, admission_ref?}
CollaborationMessageV1 = {message_id, kind, actor_ref, task_id,
  goal_revision, executor_epoch?, in_reply_to?, body,
  evidence_refs, received_at, applicability: current|stale|invalidated}
ApprovalRequestV1 = {approval_id, request_revision, task_id,
  owner_revision, goal_revision, executor_epoch, proposal_digest,
  requested_grant_scope, cumulative_limit, valid_until_tick,
  status, decision_ref?, grant_ref?}
```

`active_goal` 是 `GET T/snapshot` 的必需字段：有激活目标时，内联返回该调用者有权读取的完整 `GoalVersionProjectionV1`，而不只是 revision/digest 或无法解引用的存储地址。`content` 与 `PUT goal` 接受并持久化的完整目标正文使用同一 `GoalContentV1` schema，不以摘要、裁剪文本或尚未激活的新版本替代；硬授权仍单独来自 grant。只有切点 S 上确实没有激活目标（active_goal_revision="0"）时才返回 `active_goal=null`、`active_goal_digest=null`，此时不能开启有目标的决策。

投影的 task_id、goal_revision、私有证明承诺引用和 activation_commit_ref 必须与同一切点 S 的 TaskControl、world_binding 一致；正文按版本化 GoalVersion codec 重算并匹配私有 owner proof 内的 digest，再重算私有盐承诺以匹配公共 owner 发布签名的引用。该私有正文 digest 与 Harness 的 GoalSnapshot/context digest 各有原合同，不得混为一个哈希，也不能将其复制到公开 TaskControl。goal_text 非空，完整正文继续受 4 KiB 上限约束，任何超限/截断不得伪装成功。latest_goal_revision 可大于 active_goal_revision，但未激活内容不得作为 executor 当前目标。

已实施 SDK 合同位于 `oasis7_agent_api::game`：`GoalContentV1` 的 4 KiB 上限按完整紧凑 JSON 编码的 UTF-8 字节数计算，包含字段名、软约束与转义开销；不裁剪、不重排、不规范化原文。`game_goal_version_digest` 复用 canonical CBOR/BLAKE3，将 `["game_goal_version/v1", {schema_revision: 1, task_id, goal_revision, content}]` 编码后取 `blake3:<lowercase-hex>`，其中 goal_revision 是规范十进制字符串。activation_commit_ref 不进入正文摘要。`GoalVersionProjectionV1::validate_against` 比对的 expected digest 必须来自已验证私有 owner proof，不能取自公开控制记录；公共控制中的 task/revision/承诺引用与历史激活提交另外核对。调用者继续验证 WorldBinding 和当前 audience；仅解析或自洽摘要不证明激活或读权。

目标发布的私有前置合同为 `GameGoalPrepareV1`：完整正文、重算的私有 goal_digest、独立随机 client_nonce 和稳定 client_operation_id 一起签名，并绑定世界/genesis、task/Agent、owner key/binding/generation、branch/reorg、expected task/latest goal revision 与严格递增的新 goal revision。Runtime 的独立 `private_goal_prepare` 模块使用专用私有签名 domain、拒绝弱 Ed25519 公钥并执行严格验签；验签后仍须与受信当前 TaskControl 逐项比较。正文、摘要和该签名证明仅用于私有传输及存储，不能包装为公共 canonical intent。

`PrivateGoalStore::prepare` 将完整原始 proof、独立生成的私有 32 字节随机盐和 salted commitment 引用放在单份不可覆盖的加密记录内。文件名为 operator 密钥派生的 keyed operation locator，作用域包括 world/genesis、task/Agent、owner key/binding/generation、branch/reorg 和准备 operation ID；不包含正文、digest、nonce 或 revision。同操作相同 proof 重试返回原记录，不同 proof 冲突；响应丢失、并发发布与重启都重读原胜者，没有第二份权威索引事务。引用为 canonical CBOR `["oasis7/private-goal-reference/v1", salt_lowercase_hex, SignedGoalPrepareV1]` 的 BLAKE3 lowercase hex；盐和完整 proof 始终保持私有，因此不是可枚举正文地址。客户端保留原 proof，验证准备响应的配置服务签名和请求关联，再调用 `validate_against(original)` 重算引用；不能只信服务提供的引用字符串。

owner 另以 `game_goal_publish/v1` domain 签署 `GameGoalPublishV1`，只公开 owner/world/history/task fences、expected/new revision、不透明引用、准备操作 ID 和独立发布操作 ID。普通 `/v1/world/submit` 与 `/lookup` 沿用持久关联与 canonical receipts。执行时重验当前 owner、严格签名、版本前驱和同块 staged 控制记录；成功提交同时推进 task/latest/active goal revision，任务仍 suspended 且 executor 未绑定。公共状态不记录 private proof、盐或正文 digest。激活记录只保存 committed height 和发布关联；读取历史 pin 取得真实 CommitRef，避免把参与 state hash 的 receipt 再引用自己的 root。共识不读取节点私有文件，canonical 激活不保证正文副本可用；缺失正文使私有回读及后续 Context 阻塞，不改变已提交历史。`GameGoalPublish` 是新增 canonical payload，旧节点不能解析它或恢复包含其回执的历史；首次发布前必须升级所有相关执行、验证和恢复节点，已有新发布历史不能回滚至旧 binary。读取 schema revision 3 不构成节点能力协商或 mixed-version 升级屏障。

原生 chain status 服务提供默认关闭的 owner 目标子集：POST `/v1/world/private-goal/prepare` 与 `/v1/world/private-goal/current`。必须同时配置 `OASIS7_PRIVATE_GOAL_DIR` 和 `OASIS7_PRIVATE_GOAL_KEY_FILE`，实际 listener 必须是 loopback；专用目录和密钥路径不得位于公共 world/records/CAS 存储内。密钥是 operator-owned、mode 0600、无链接的精确 32 字节文件，祖先分量禁止符号链接；密钥不进入公共配置或快照。明文 loopback 传输要求所有可监听本机端口的进程受信，不能只依赖同 UID 文件隔离；客户端发送正文前未认证 HTTP 对端，配置服务公钥只验证回复，不保护已发送正文。不受信多用户部署须使用发送前验证服务端的 TLS 或等效传输，不能暴露公开明文私有入口。私有客户端禁用代理、重定向并限制回复至配置值与 64 KiB 的较小值。入口拒绝 query、Origin、Authorization 与转发 header，响应 no-store。准备先检查当前 owner/world/task/history，再查询原操作；已发布的原 proof 可以重试，新的准备才比较当前 expected revision。回读先检查当前 owner，再验证历史激活、私有 proof/承诺/绑定，并在交付前重新 pin 当前控制与世界分支。返回完整当前 owner 目标、当前读取 CommitRef 和历史 activation CommitRef；它明确不是完整 TaskSnapshot，也不授予认知或执行权限。

Runtime 的 native library 提供独立私有目标存储基础 `world_service::private_goal_store`：operator 显式传入专用目录和 32 字节密钥，完整正文及重算 digest 仅存于 XChaCha20Poly1305 密文；公开可引用值为独立随机的不透明引用。关联数据绑定 world/genesis、task/Agent、owner identity/generation、branch/reorg 与 goal revision。namespace 以随机 challenge 的加密密钥验证记录固定当前密钥；打开及读写时重验该记录，错误密钥、篡改或缺失记录均拒绝。初始化只允许严格空目录，原子发布且不覆盖并发胜者；没有验证记录却存在目标或暂存文件时不自动接管，operator 须核查初始化残留并恢复原验证记录。该存储版本不填写尚未发生的 activation_commit_ref，存储成功不等于目标激活。

存储只在支持 Unix 私有权限的系统可用；目录与文件权限、所有权、符号链接及普通文件类型检查均须通过，有界读取后解密并重新验证完整正文。文件和目录同步完成后才返回引用；对象不可覆盖，不自动回收被引用的版本。operator 必须把 namespace 和密钥放在公共 CAS、P2P、公开备份/快照之外，确保祖先目录和同一系统账号受信；目录句柄与禁止链接访问不防御拥有该账号权限的恶意进程，也不自动证明目录已排除在公开存储之外。独立保留密文与密钥才能恢复；公共世界 checkpoint 不包含这些正文，也不证明跨节点私有副本已恢复。

旧 `put/get` 基础存储仍只接受调用者提供的绑定，不自动授予当前 owner/audience 资格。新 prepared 记录接入上述 owner 发布/回读；原生 Context、executor fence 和目标驱动决策已有[有界实施切片](../../world-simulator/llm/external-agent-game-api.design.md#当前实施候选原生任务执行链)，不再属于全部待实现范围。完整协作 TaskSnapshot、delivery/adoption、问答/审批/报告与双 Runtime 首局仍未由该切片闭合。World Service canonical intent 会保留完整请求，故只能承载 public-only PublishGoal，不能承载私有目标正文、盐、正文签名或可枚举 digest；这些值不得进入公共 CAS、P2P 或公开世界流。

解密/投影前及响应交付前按当前 owner/audience 和 executor fence 验证读取资格；切点 S 或旧 token 不保留已撤销的读权。无权请求不返回正文、正文指纹或新 owner 的差异。元数据引用的正文缺失、损坏或无法验证时，返回 `503 ErrorV1(code=recovery_required, refresh=task_snapshot)`，不返回伪完整 TaskSnapshot、空目标或公开内容 URL，相关新 context 保持阻塞。客户端只能在授权仍有效后重新读取 snapshot，不挂载私有存储或依赖历史事件取回正文。

端点不接受客户端填写的“世界结果 status=committed”来写回权威任务成果。`goal-receipts` 只允许 received/adoption_reported；context/decision/admission 的证据由各自受信边界记录。对没有外部观测能力的纯 Runtime 自报，保留 reported 等级，不伪造硬件或模型证明。

控制命令至少包含 `PublishGoal / BindExecutor / StopDelegation / SubmitApprovalDecision`，复用 typed owner authority。消息类命令 `PostReport / AskQuestion / AnswerQuestion / AcknowledgeGoal` 不能改 grant。每条命令都携带稳定 operation ID、expected revision 和所属世界分支；通道切换不改变身份作用域。

<a id="collab-snapshot-pages"></a>
### 6.1 同切点快照分页合同

承接 [AC-COLLAB-006](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-006)
与 [AC-EXT-008](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-008)。
复用 [Game API](../../world-simulator/llm/external-agent-game-api.design.md#api-wire) 的目标
`GET T/snapshot`，不另建公共内容下载入口。本节是尚未发布的完整 TaskSnapshot 合同补充；
已实施的 task-control/private-goal 子集不因此具备分页、完整恢复或新的读取授权。

**读取请求与切点。** 首次请求不带 `snapshot_cursor`，可带十进制整数 `limit`（1–200，默认 200）；
Runtime 在已提交控制记录与私有正文投影间固定一致切点 S，建立有界只读 pin，再返回第一页。
后续请求仍调用同一路由，只携带服务端返回的 `snapshot_cursor`；不能更改 limit、切点或排序。
游标绑定 snapshot_id、world/genesis/branch、task/Agent、已认证主体、audience/读权限版本、
适用 executor fence、S、页位置与到期时间；它不是 bearer 权限，不含正文、正文指纹或私有存储路径。
传输、日志和缓存按私有协作数据处理，响应 `Cache-Control: no-store`，游标不进入公开日志。

| 字段 | 含义与消费规则 |
| --- | --- |
| snapshot_id / snapshot_seq | 本次只读 pin 的不透明身份与切点 S；各页必须一致，不能与另一轮 snapshot 混合 |
| page_index | 从 `"0"` 连续递增的十进制字符串；重试同游标返回同一页，客户端不能重复追加 |
| total_counts | 四个列表在 S 上、该 audience 可见的确切总数，均为十进制字符串；不披露其他受众计数 |
| next_snapshot_cursor | 还有当前态项目时的下一页游标；最后一页为 null，不以短页或空列表判定结束 |
| resume_cursor | 非最后页为 null；最后页给出与同一 S 绑定的 after-S 事件游标，不从末页读取时间开始 |
| completeness | `partial` 表示仍有续页；`complete` 只标记末页，不表示末页本身就是整份快照 |

每页保留同一 S 的 world/control/goal/executor 元数据及完整 active_goal；四个列表只放该页项目。
按 `unanswered_questions → pending_approvals → unsettled_operation_refs → latest_reports` 固定集合顺序，
集合内按原稳定 question/message、approval、operation、report ID 的 UTF-8 字节顺序升序枚举。
同集合 ID 唯一，S 上的内容与版本也固定；禁止在不断变化的当前列表上使用 offset 分页。
`latest_reports` 的当前投影同样读齐，但完整历史报告仍沿既有授权历史入口读取，不借快照承诺无限历史。

**完整度与字节预算。** limit 是一页四个列表合计的条数上限，仍不得超过 200；服务端按上述顺序
逐项加入，并计算整个未压缩 UTF-8 JSON 响应的实际编码字节，包含字段名、转义、分隔符、重复头部和游标。
达到 2 MiB 前提前结束该页，不能按字符数或正文相加估算，也不能裁剪单个项目。只要仍有项目，成功页必须
至少前进一项。空任务返回 page_index=0、四个计数为零、complete 和 after-S 游标。

为保证合法单项总能装入一页，完整快照的非列表头部（含游标/计数）和每个完整当前态项目各不超过
64 KiB 编码字节；goal 4 KiB、message/report 8 KiB 的既有正文限制和每 task 256 个待决项总限额不变。
新控制/消息/申请写入及当前投影更新在持久接受前检查完整编码，超限明确拒绝；追加历史不无限膨胀当前投影。
不符合此尚未发布合同的旧草案记录保留原数据并返回 recovery_required，不能删记录、切正文或把 partial
改成 complete。格式修复与兼容处理完成前不宣称该候选具备完整恢复能力。

**读取期间变化。** pin 保留 S 上的当前项、所引用的私有正文版本以及可接续的 after-S 事件窗口；
事件传输日志压缩不得删除这些当前项。新问题、回答、决定及目标更新只改变 S 之后的事件和当前权威状态，
不会改写 pin 的成员或原页内容。读取 pin 不持有 World::step 的长事务锁；pin 寿命与并发配额由 LimitsV1
公布实际值，不能由重试无限续租。资源不足在建立 pin 前返回 429；pin 到期、进程重启后无法恢复该 pin、
分支变化或保留窗口无法保证时，返回 409 cursor_reset_required / refresh=task_snapshot 并终止该轮，
不能把新切点拼入旧页，也不能用空页掩盖被回收的增量。

每页在解密前及交付前重验当前读权限；无权时先返回不泄露对象/计数的 401/403，再谈任何版本差异。
audience/读权限版本或适用 executor fence 变化使旧 pin 失效；仍有读权的主体从新 snapshot 重建，
不能因历史切点继续取得被撤销的正文。客户端失去读权后清除有关私有缓存；此前已经合法交付的信息无法
被服务端追溯收回，不作这种承诺。

客户端只在从第零页开始、页号连续、身份/切点/固定头一致、稳定 ID 去重且每集合计数等于 total_counts，
并取得唯一末页后，才原子替换本地基线，再从 S 消费增量；此前保持 recovery_required，不启动新认知或批准。
断连期间只缓存私有暂存页，不发布伪完整状态。已知未结算 operation 先对账，snapshot 中出现待批项目不执行批准。
新 context、owner 决定与世界提交仍使用当前版本及授权检查；S 上的许可不成为今天的写权限。

## 7. 状态、事务与持久化

<a id="collab-recovery"></a>
### 7.1 原子提交与恢复切点

私有正文写入新代际并 fsync，随后 Runtime control transaction 原子提交版本引用、fence/grant 更新与待发送事件。事件只从已提交记录投影；崩溃在正文写入与 control commit 之间留下不可见孤儿，安全回收不能丢失已提交引用。commit 后回复丢失通过原 operation ID 返回相同结果；不把网络投递当成事务的一部分。

单权威部署沿现有持久 world/journal 路径保证进程重启可恢复。多节点使用当前已获准的 writer/replication 合同，不能让 API 自选主节点；承诺容灾前必须验证私有正文存储与 control reference 的可用性。若元数据存在但正文不可读或校验不符，任务进入 recovery_required，新认知暂停，绝不使用空目标代替。

控制状态 `active / suspended / closed` 与执行诊断 `awaiting_executor / thinking / waiting_world / waiting_player / blocked / offline / unknown` 正交。心跳只有诊断意义；即使实例报 thinking，也不授予 epoch。wall-clock 失联计时不直接决定确定性世界提交；所有失效授权与 writer 交接经 Runtime 有序生效。

### 7.2 增量投递和缺口

Runtime 生成 task-scoped 单调 event_seq，投递至少一次。cursor 是服务端签发的不透明值，绑定 world branch、task、受众与权限 revision、位置及有效窗口；不能将全局消息总量或其他人的事件通过游标泄露。

收到消息后执行器先持久化本地 inbox/去重位点，再确认。服务端同 message_id 同内容幂等，不同内容冲突。任务 snapshot 按[同切点分页合同](#collab-snapshot-pages)读齐后包含一致读取切点 S：该切点的当前目标、授权、未答问题、待决批准和未结算 operation，与 `events after S` 接续。snapshot 生成时并发新消息只会出现在增量中，不会落在两者之间。激活目标正文作为该 snapshot 的必需当前态，不依赖 S 之前的 GoalVersionSaved/Activated 事件是否仍在重放窗口内。首次连接且没有任何本地缓存的执行器也能仅凭全部授权 snapshot 页与 after-S 事件完成初始化；若取得 context 前目标再次激活，按 expected revision 拒绝并重新同步，不把旧正文重新标注为新版本。

游标过期、分支/受众变化或存储窗口外返回 `cursor_reset_required`，提供允许访问的当前 snapshot 获取方式；客户端标记历史缺口，不当作“无新事件”。恢复只消费当前有效目标/批准，不按旧历史顺序重演副作用；已知世界 operation 逐项先对账。

### 7.3 停止、交接与数据寿命

Stop 的效果是持久失效当前执行许可并阻断新受限提案，保留问题、审批和回执；模型进程终止是 adapter 的另一条 best-effort 操作。同一已授权设备的重启可在当前委托明确允许续接时，经 Runtime 的 CAS 重新取得执行代际，不要求每次重启都重新人工审批，也不能延长原授权；已撤销、到期或换设备则必须重新获得有权主体确认。新 worker 只有在有效绑定与新 fence 生效后才可取得 context，旧实例的迟到操作拒绝且可审计。

终态控制记录、幂等 tombstone、审批和世界结果引用与该世界的可审计历史一致保留；事件传输日志可以压缩，不能删去 current snapshot 需要的 pending 信息。私有内容保留期、数据删除与受众规则由部署公开说明，首期传输重放窗口初值为 7 天且每任务最多 100000 项，以先到达的限制为准并在接口公布实际最早可读位置；超出时显式 snapshot reset，而不是丢掉当前问题。备份恢复必须验证世界/control generation 一致并旋转受影响凭据；旧 fence 不得因恢复较早备份重新有效；必须同时验证既有 WorldBinding 的 branch/reorg/finality 变化和身份服务的撤销状态。无法证明旧执行资格已失效时保持 RecoveryRequired，不在本地私自生成一个“更大 epoch”冒充世界权威。

## 8. 部署、安全与运行约束

读任务、订阅事件、查看报告、审批和查原操作分别鉴权，不能只在最初 WebSocket 握手验证一次。配对 token 不能签发 owner 决定；公开日志不含 private prompt、审批材料、refresh token 或本机路径。Content store 加密密钥由 Runtime operator 管理，不写公开 chain snapshot。

新目标可以在 executor 离线时保存并激活，但界面显示 awaiting_executor。skill 不负责保活，客户端不能在用户不知情时启动本机 Codex/OpenClaw。审批审计记录保留做出决定的当前身份、scope、版本和 control commit，不把 UI 勾选或普通文字当世界 grant。

## 9. 质量与容量

复用 Game API LimitsV1：goal 4 KiB，message/report 8 KiB，单页列表合计最多 200，单响应 2 MiB。每 task 首期最多 256 个未答问题加待决申请；超过时在接受新项前返回 quota_exceeded，不丢弃已接受项。201 个同类或合计 256 个合法待决项通过[同切点分页](#collab-snapshot-pages)完整读取；分页数量限制不是待决项准入限制。完整编码、单项可装入性、pin 配额和失效处理按 §6.1 执行，不能依赖 after-S 找回 S 前被遗漏的待决项。保留索引是可重建投影，pending 的 source 必须持久。

恢复目标是同一完整 checkpoint 中已确认 control operation 不丢失；这是目标，不是现有线上 RPO 声明。并发验证必须含两个 owner 窗口、两个 executor、目标更新/撤销与世界 commit 交错。事件读取不能阻塞 World::step，report 洪泛不得挤占 stop/撤销的有界控制队列。

## 10. 兼容、迁移与回滚

新增协作记录使用 revision 1 独立 namespace，旧 Prompt/profile 可作为显式初始 GoalVersion 导入，保留来源与 owner 重新确认；不得把历史聊天批量变成新批准。旧 provider history 可以只读追溯，不迁移其不可信“成功”成 world receipt。

实现可复用既有 goal/journal，但必须提供新语义的完整回读，不通过 alias 默默丢掉版本。回滚关闭新命令、保留任务与操作结果查询；schema 不可读时 fail closed，不清空状态或自动回退到旧 executor。未满足持久一致性与私有存储验证前不能对外承诺跨节点恢复。

本修订补齐尚未发布的 revision 1 目标快照合同，不需要为缺少 active_goal 的草案实现保留静默兼容；读取不完整旧形状或不识别分页完整度时停止认知并要求 schema 对齐；不能把第一页当旧式完整快照。

## 11. 验证设计与可追溯性

先在普通 CI 使用 fake RuntimePort/fake driver 和实际持久存储做故障注入，再以真实 Runtime + Web/native 做跨设备体验。主动扩大 scope/修改旧世界权限不属于本次文档交付。

A1 的完整恢复容量回归由 [collab-snapshot-pages](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-snapshot-pages)
接收，联合下表 AC-EXT-008 / AC-COLLAB-006 的恢复与增量负例；验证计划不等于已存在测试实现。

### 11.1 验证映射表

| 上游条款 | 本设计条款 | 独立 obligation 与适用条件 | 准确验证方法与场景 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-091d6c22-fca5-4284-91dd-2edce9ef440b；[AC-EXT-006](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-006) | [collab-goal](#collab-goal) | 目标调整真正进入有效上下文，旧待决结果独立处理 | [collab-goals](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-goals)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；[AC-EXT-008](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-008) | [collab-recovery](#collab-recovery) | 断连重启保留原世界任务且不复活失效授权 | [collab-recovery](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-recovery)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-62007da5-b12a-4b8c-98bb-82ab53a35069；[AC-COLLAB-001](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-001) | [collab-binding](#collab-binding) | 跨设备共享任务并隔离被替换的执行方 | [collab-bind](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-bind)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-edd52061-73c0-433c-9531-30d86b1db459；[AC-COLLAB-002](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-002) | [collab-goal](#collab-goal) | 保存、读取、采纳、用于决策分别可验证 | [collab-adoption](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-adoption)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-b100c55e-5c35-46dc-aaf6-e66c02b323d8；[AC-COLLAB-003](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-003) | [collab-content](#collab-content) | 计划和阶段汇报不能冒充权威成果 | [collab-report](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-report)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-2f463269-c3c0-4da1-95f8-90011c35799a；[AC-COLLAB-004](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-004) | [collab-content](#collab-content) | 问题/回答关联原任务和目标，旧问题不改新目标 | [collab-questions](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-questions)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-f5341eef-d8f6-42ae-b7b6-751803abd588；[AC-COLLAB-005](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-005) | [collab-approval](#collab-approval) | 有效 owner 批准与累积预算在执行时重验 | [collab-approval](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-approval)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-6044cb04-5fbb-4c68-81a6-fc57cfdd608a；[AC-COLLAB-006](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-006) | [collab-recovery](#collab-recovery) | 重复消息、迟到和历史缺口安全恢复 | [collab-events](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-events)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-8dad5caa-d064-4ac8-8c98-40a702b13685；[AC-COLLAB-007](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-007) | [collab-runtime-journey](#collab-runtime-journey) | 同一任务中证明离开 Viewer、停止、离线改目标、续接、问答、明确授权及世界结果 | [collab-session](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-session)；按该场景的候选、环境、故障注入和判据执行 | 每个声明支持的真实 Runtime/profile 独立成案，保留同任务关联链 | 不证明自然语言策略正确或第三方工具已获授权 |
| Trace refs: O7-REQ-d30925fe-d798-4056-9641-062255931a61；[AC-EXT-002](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-002) | [collab-relationship-isolation](#collab-relationship-isolation) | 自有/受托并存、跨资源拒绝和定域撤销 | [api-auth](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-auth) | A/B 当前权限、拒绝和原回执对账 | 不以单角色路径代签多关系隔离 |

<a id="collab-runtime-journey"></a>
### 11.2 同任务的真实 Runtime 组合验收

[AC-COLLAB-007](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-007)
由 [collab-session](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-session)
组合既有 executor、adoption、questions、approval 与真实世界结果场景。QA 主责证据关联，Agent owner
提供真实 Runtime/worker 会话，Runtime owner 核对控制提交及世界回执，Viewer owner 核对实际使用的界面。
每个声明支持的 Runtime/profile 独立保留同一 task 的过程，目标 revision 和 executor epoch 的变化必须
有连续来源，而不是强行相等；另一任务的成功、另一个 Runtime 的截图或 fake driver 的运行不能补齐该链。

该组合只增加关联和缺口判定，不要求重跑所有确定性负例与平台的笛卡尔积。可引用同候选、明确适用范围的
普通回归；真实过程必须覆盖离开 Viewer 后仍执行、停止后的真实离线、离线发布、重新授权/绑定与续接，
以及同任务的问答、专用批准和后续唯一世界效果。停止进程与 StopDelegation 的权威提交分别留证，不能
把重启当旧授权复活。任一环节未实现、未获准或未运行，该 Runtime 的对应验收保持 blocked/not_run，
不把本节验证计划、文档合入或 native 子集证据升级为整体通过。

## 12. 决策、长期风险与未决问题

ADR-COLLAB-1：控制许可放在世界权威序列，私有正文单独保护；选择一致性而不是网关内存立即生效。正文投影不拥有 grant，代价是不可达时需要显式 pending/recovery。

ADR-COLLAB-2：一 Agent 一当前执行方，epoch 隔离旧实例；首期不做多控制者冲突仲裁。自然语言服从不可由 ack 证明，因此报告、上下文关联与世界效果分层展示。

Runtime owner 在实施中必须确定 private namespace 与现有 generation/checkpoint 的实际落点，并验证 restore 不回退有效 fence；身份 owner 需验证 owner 转让时私有历史可见策略。任一未就绪只阻塞相关写入/恢复承诺，不通过新建第二套数据库或管理规则掩盖缺口。
