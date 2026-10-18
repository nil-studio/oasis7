# Gameplay Agent 认领经济合同设计

历史状态边界：§9 的 2026-03-27 样本与 §10 的 2026-03-29 reopening 是原窗口记录，不表示今日闭环、任务状态或重新调参。原 AGC-005 接收 Task 尚未唯一定位，原过程保留；经济参数、维持理由及重新评估条件继续有效，实际能力须取当前候选证据。

- 对应需求文档: [`Gameplay Agent 认领经济合同`](gameplay-agent-claim-economy-contract.prd.md)
- 产品承诺入口：[`Agent 所有权与持续经营`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)
- 可变执行状态: 对应 GitHub Project task 与 issue evidence comments

审计轮次: 2

<a id="public-claim-boundary"></a>
## 公开口径边界与风险

Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385

本设计文件只负责 claim 状态机、成本、资金来源和 provenance 的专业设计，不拥有发行状态或公开 claim。其消费合同是玩家接入与发行模块的 [`REQ-ENTRY-CLAIM-003`](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003) 与 [`四层口径分离`](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#public-claim-four-layer-model)。

- 当前事实（current fact）：设计中明确的是状态和约束的设计事实；它不能证明实现已经部署、某个账户已经获准、某个渠道已发布或当前可公开体验。
- 产品目标（product goal）：让首个 claim 有明确成本，让受限 starter balance 只用于 `slot-1`，并让回收、退款和失败边界可审计、可向玩家解释。
- 假设（assumption）：受控运营窗口会提供与设计一致的 runtime、viewer、QA 和 operator evidence；若任一条件不成立，不能继续沿用设计文案作为当前状态。
- 证据边界（evidence limit）：设计只支持规则解释和候选 claim 的限制说明，不支持 public launch、production settlement、一般账户资格、无条件可用性或资产价值承诺。
- 可发布公开 claim（releasable public claim）：只有产品 PRD、根 README、同一证据窗口和对应 [`restricted grant LiveOps runbook`](gameplay-agent-claim-restricted-grant-liveops-runbook-2026-03-29.md#public-claim-boundary) 一致时，才可引用本设计的受限规则；否则必须标记为 draft、blocked 或 stale。

## 1. 设计目标
- 把 agent 认领从“谁先点到谁拿到”的弱规则，提升为一条有成本、有维护、有回收的正式 gameplay 经济链路。
- 保持 `Agent 唯一性` 与资源守恒，不把 claim 机制做成可绕过 runtime 审计的侧门。
- 让 `runtime_engineer`、`viewer_engineer`、`qa_engineer` 对同一套 claim 状态机和字段工作。
- 让 limited preview / allowlist / QA seed 可以给首个 claim 提供受限启动资金，而不把启动补贴变成可转账资产。

## 2. 状态机

`unclaimed -> quote_ready -> claimed_active -> upkeep_grace -> forced_reclaimed -> unclaimed`

补充分支：
- `claimed_active -> released -> unclaimed`
- `claimed_active -> inactive_reclaim_candidate -> forced_reclaimed -> unclaimed`

关键约束：
- 任一 agent 任一时刻只允许 1 个 `claim_owner_id`。
- 首个经济 `ClaimAgent` 也必须走完整成本链：`activation fee + claim bond + upkeep`。首次 `RegisterAgent` / 会话绑定不创建该状态机中的经济 claim，不能仅凭绑定显示 bond 或 upkeep 义务。
- `restricted starter claim balance` 只改变 `slot-1` 的资金来源，不改变 canonical 成本，也不能旁路 `slot-2/3` 成本。
- `upkeep_grace` 与 `inactive_reclaim_candidate` 都必须带可见倒计时，不能只靠后台静默清退。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385；[REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003) | 认领经济说明区分当前事实、产品目标、假设与证据边界，缺少同窗依据不发布 | [公开口径边界](#public-claim-boundary) | 产品 / LiveOps / QA / 根 README | 只覆盖认领经济输入，不替代其他专业领域、全渠道审核或实际发布证据 |
| Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；[`REQ-WR-AOS-001`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001) | 首个经济 Agent 认领必须展示非零承诺包，并保持成本、资金来源与后续回收可审计。 | [`DES-CLAIM-ECONOMY-001`](#des-claim-economy-001) | `producer_system_designer` / `runtime_engineer` | 不证明具体实现、部署或公开 claim。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-AGC-ENTRY-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-entry-001) | 同世界新账号的显式首次注册不依赖既有 claimer，返回账号恢复旧绑定；两者与经济 Claim 分开。 | [DES-CLAIM-ENTRY-001](#des-claim-entry-001) | `runtime_engineer` / identity / `viewer_engineer` / `qa_engineer` | 后继目标，当前固定 starter 与空世界测试不证明逐账号准入；不修改经济 Claim Prepare/Confirm。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[AC-AGC-ENTRY-001](gameplay-agent-claim-economy-contract.prd.md#ac-agc-entry-001) | 同世界新账号的显式首次注册不依赖既有 claimer，返回账号恢复旧绑定；两者与经济 Claim 分开。 | [DES-CLAIM-ENTRY-001](#des-claim-entry-001) | `runtime_engineer` / identity / `viewer_engineer` / `qa_engineer` | 后继目标，当前固定 starter 与空世界测试不证明逐账号准入；不修改经济 Claim Prepare/Confirm。 |
| [REQ-AGC-CHAT-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-001) | 已交付文案切片按 OR 条件披露，保留余额耗尽后记录仍有效与其他独立 gate 的区别。 | [DES-CLAIM-CHAT-002](#des-claim-chat-002) | `runtime_engineer` / `viewer_engineer` | 该切片不改 OC 额度、领取资格、账本或现有 chat gate。 |
| [REQ-AGC-CHAT-002](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-002) | 注册/绑定不等于经济认领；权威延后说明按实际经济 claim 生成。 | [DES-CLAIM-CHAT-003](#des-claim-chat-003) | `runtime_engineer` / `gameplay_designer` | 不据此减免费用、清除历史未决义务或开放免费经济 claim。 |
| [REQ-AGC-CHAT-003](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-003) | 已交付显示切片让中英文、Viewer/pure API 保留同一含义；旧版或缺失义务说明保守回退。 | [DES-CLAIM-CHAT-004](#des-claim-chat-004) | `viewer_engineer` / `qa_engineer` | 该切片不增加协议字段或把显示状态持久化为世界状态。 |
| [REQ-AGC-CHAT-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-001) | service 以适用权威事实区分通过、阻塞与未知，跨入口保持范围、新鲜度与最终提交语义。 | [DES-CLAIM-CHAT-005](#des-claim-chat-005) | WorldService / `runtime_engineer` / `viewer_engineer` / Agent / QA | 待实现后继协议；不公开全账本、不扩执行/目标控制权限、不改变 OR 经济规则，既有文案测试不代签。 |
| [REQ-AGC-CHAT-003](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-003) | service 以适用权威事实区分通过、阻塞与未知，跨入口保持范围、新鲜度与最终提交语义。 | [DES-CLAIM-CHAT-005](#des-claim-chat-005) | WorldService / `runtime_engineer` / `viewer_engineer` / Agent / QA | 待实现后继协议；不公开全账本、不扩执行/目标控制权限、不改变 OR 经济规则，既有文案测试不代签。 |

<a id="des-claim-economy-001"></a>
### DES-CLAIM-ECONOMY-001：认领成本与资金来源承接

本设计的状态机、成本模型与 restricted funding 边界承接 `REQ-WR-AOS-001` 的非零承诺包；资金来源、回收和退款 provenance 必须保持可审计，但本表不把设计写成实现或发布证明。

## 3. 成本模型
- 三段式：
  - `activation fee`: 非退款，立即拆到 burn / treasury。
  - `claim bond`: 锁定后可在 release / reclaim 时按规则退款或 slash。
  - `upkeep`: 每个 epoch 结算一次，持续表达“占有这个 agent 就要持续承担成本”。
- 报价必须把持续成本换算成玩家可读 runway：
  - `upfront cost`: activation fee + claim bond + first upkeep。
  - `recurring upkeep`: 每个 epoch 的持续成本。
  - `runway`: 扣除 upfront 后，当前 eligible balance 还能覆盖多少个完整 upkeep epoch。
  - `reclaim risk`: 低 runway 时预计进入 grace / forced reclaim 的风险提示。
  - `recommended top-up or wait`: 若 runway 低于阈值，提示先补足余额、等待收入或暂缓扩张。
- runtime v1 暂定默认值（用于 canonical 实现与测试，不等于最终平衡定价承诺）：
  - `base activation fee = 100`
  - `base claim bond = 200`
  - `base upkeep = 25`
  - `activation fee burn split = 50%`
- 槽位曲线：
  - `slot-1 multiplier = 1.0`
  - `slot-2 multiplier = 1.5`
  - `slot-3 multiplier = 2.0`
- 声誉上限：
  - `tier-0 cap = 1`
  - `tier-1 cap = 2`
  - `tier-2+ cap = 3`
  - runtime v1 暂按 `reputation_score < 10 / >= 10 / >= 25` 映射到 `tier-0 / tier-1 / tier-2+`，后续由平衡专题复核。
- 受限 bucket：
  - `restricted starter claim balance` 是 main token 账本中的受限 bucket，不是第二种代币。
  - 允许消费范围：`slot-1 claim upfront cost` 与 `slot-1 upkeep`。
  - 禁止消费范围：`TransferMainToken`、公开转账 API、explorer 导出的可转账金额、`slot-2/3 claim` 与其它 main-token 动作。
  - 资金优先级：`slot-1` 先花 restricted，再补 liquid；`slot-2/3` 只花 liquid。
  - 发放来源：`allowlist / onboarding / qa_seed / liveops_campaign`；每笔发放都必须带 `issuance_reason / issuer_id / expires_at_epoch`。

## 4. 退款与 Provenance
- claim 时要显式记录：
  - `upfront_restricted_spent_amount`
  - `upfront_liquid_spent_amount`
  - `claim_bond_locked_restricted_amount`
  - `claim_bond_locked_liquid_amount`
- release / forced reclaim 时：
  - 先按 canonical 规则结清 arrears 与 penalty。
  - 再把剩余 bond 按 provenance 拆成 `restricted refund` 与 `liquid refund`。
  - restricted 来源的 refund 只能回 restricted bucket，不能洗成 liquid。
- 若 upfront 为 mixed funding，则 activation fee / upkeep 属于已消耗成本，不参与退款；只有 bond 部分需要保留来源拆分。

## 5. 回收与退款
- 主动释放：
  - `release_cooldown_epochs = 3`
  - 满足 cooldown 且无欠费时，按 provenance 退回剩余 bond。
- 欠费回收：
  - `grace_epochs = 2`
  - 逾期未补足则强制回收。
- 闲置回收：
  - 连续 `7` 个 epoch 无有效控制推进，进入 `inactive_reclaim_candidate`
  - 连续 `10` 个 epoch 仍无恢复，执行强制回收。
- 惩罚：
  - `forced_reclaim_penalty_bps = 2000`
  - 先扣未付 upkeep，再对剩余 bond 扣 penalty。

## 6. Runtime / Viewer / QA 边界
- `runtime_engineer`
  - 负责 claim 状态机、受限 bucket 账本、原子扣费、epoch 结算、refund / slash provenance 和事件。
- `viewer_engineer`
  - 负责 quote、restricted/liquid 余额拆分、funding mix、upkeep deadline、cooldown、idle risk、cap 阻断原因、refund 预估与 upkeep runway 的表达。
  - Claim quote 最小条必须包含 `upfront cost / recurring upkeep / runway / reclaim risk / recommended top-up or wait`，避免玩家把可支付 upfront 误读为可长期持有。
- `qa_engineer`
  - 负责并发争抢、受限余额发放、欠费、闲置、多槽位、refund provenance、transfer guard、审计字段和 UI/API parity 的 required/full 验收。
- `producer_system_designer`
  - 负责成本曲线、tier cap、宽限与回收边界，以及 starter balance 的用途、额度与过期策略。
- `liveops_community`
  - 负责 allowlist / campaign 发放策略、回收/停用策略与受控测试口径回流。

### 6.1 首次绑定与经济认领的组件边界

既有 [gameplay action 映射](../../../crates/oasis7/src/viewer/gameplay_actions.rs) 将 `claim_first_agent` 交给 `RegisterAgent`，将 `claim_agent` 交给 `ClaimAgent`。首次绑定只建立可操作 Agent；首聊维护说明需要另外读取该 Agent 作为 claimer 的真实经济 claim。

以下表格与 6.2–6.4 节记录已交付的文案/兼容切片；后继目标由 [DES-CLAIM-ENTRY-001](#des-claim-entry-001) 和 [DES-CLAIM-CHAT-005](#des-claim-chat-005) 单独规定，不能将目标能力读成该切片已有行为。

| 组件 | 已交付文案切片的职责 | 依赖与边界 |
| --- | --- | --- |
| [claim_snapshot.rs](../../../crates/oasis7/src/viewer/runtime_live/claim_snapshot.rs) | 读取调用方传入的 `WorldState` 和 `primary_agent_id`，保留现有 preview 生成条件，按真实 `owned_claims` 选择维护说明；输入覆盖范围见下文。 | 当前身份、控制权和动作 blocker 仍由既有控制面处理；本函数不接收旧 DTO、不写账本或执行动作。 |
| [first_chat_unlock_preview.jsx](../../../crates/oasis7_viewer/software_safe_src/first_chat_unlock_preview.jsx) | 本地化新的资源说明和两类权威维护文案；旧版无条件维护文案或缺失义务说明回退为 unknown。 | 只消费六字段 preview，不从字段缺省值、经济报价或英文句子反推 ownership 或动作资格。 |
| [viewer_app.jsx](../../../crates/oasis7_viewer/software_safe_src/viewer_app.jsx) / 既有 API 投影入口 | 沿用当前预览传递、展示和可用动作合同；同一预览在 Viewer / API 中保留相同含义。 | 不为此次文案修正新增授权、身份、快照版本或缓存失效机制。 |

本地 runtime 测试使用完整 `WorldState`：`main_token_balances[primary_agent_id].liquid_balance` 沿用缺少账户项为零的既有语义；`starter_oc_claims.contains_key(primary_agent_id)` 提供领取记录存在性；`agent_claims` 中 `claim_owner_id == primary_agent_id` 的记录组成现有 `owned_claims`。`next_claim_quote` 是未来选择，不能证明当前经济认领已经生效。

service 来源需单独限定：[world_service/projection.rs](../../../crates/oasis7_world_service/src/projection.rs) 的 allowlist 保留 `agent_claims` 与 `starter_oc_claims`，但不发布 `main_token_balances`；[world_service_link.rs](../../../crates/oasis7/src/viewer/runtime_live/world_service_link.rs) 用该裁剪状态重建 runtime world，再由 [control_blocking.rs](../../../crates/oasis7/src/viewer/runtime_live/control_blocking.rs) 调用本投影。经济 claim 事实仍可用于维护说明，但 service 的真实正 liquid 余额且无领取记录可能因余额未投影而按零值生成预览。这是既有余额输入缺口，已交付文案切片未修改字段公开范围或 gate；本地回归与同一预览的中英文/API 显示一致性不能证明真实 service gate 已正确。后继协议须按 6.5 节修复权威输入与消费边界。

已交付切片只修权威生产者文案与 Viewer 旧文案兼容，没有 DTO 字段存在性追踪、count/list 协调、gate 三态或资料新鲜度检测；这不是对后继协议的禁止。本地状态与当前 service 投影均保留 `agent_claims`；按当前 claimer 筛选的完整结果为空时，才支持没有 owned claim 的结论。经过默认值补齐的旧 preview DTO 不提供这种证据，Viewer 不使用它来生成“没有经济认领”的新结论。

<a id="des-claim-entry-001"></a>
### 6.1.1 同世界逐账号注册/绑定的后继目标

承接 [REQ-AGC-ENTRY-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-entry-001)。当前 [player_gameplay.rs](../../../crates/oasis7/src/viewer/runtime_live/player_gameplay.rs) 的 native `claim_first_agent` 仍限制固定 `FIRST_AGENT_CLAIM_TARGET_AGENT_ID`，该目标已绑定时拒绝；[auth_actions_claims.rs](../../../crates/oasis7/src/viewer/runtime_live/tests/auth_actions_claims.rs) 的空世界注册、旧 sidecar binding 恢复只证明局部路径。此设计不宣称当前 WorldService 或该固定入口已能接纳第二账号。

后继入口继续复用注册与身份绑定 authority，按以下顺序执行，不让 Viewer 直接创建 canonical 归属：

1. 认证稳定账号主体、当前 session/公钥和已存在世界的合法准入；将 world/chain/genesis、适用 branch/身份版本、原请求身份与 payload digest 绑定到注册请求。首次注册只在该受限入口免除“已有 claimer”的经济动作前提，不豁免 session、世界准入或其他动作授权。
2. 从权威绑定读取该账号在此世界的结果。已有匹配绑定时返回原结果或进入原身份恢复；原请求 pending/unknown 时查询同一请求。身份/绑定证据冲突、缺失或无法确认是否已有绑定时 blocked；只有权威确认该已认证账号尚无绑定，才进入第 3 步。不把清缓存、换设备或旧 sidecar 缺失当作新首次资格。
3. 只有合法、尚无绑定的新账号才分配自身 Agent identity 并注册/绑定。新身份不得占用其他账号 Agent 或复用已绑定的全局 starter ID；并发首次请求在同一账号/世界的绑定唯一性边界内协调。原请求结果、注册和 canonical 账号/Agent 控制绑定必须在同一可恢复提交边界确认后才公开为可控；绑定持久保留已认证账号主体、公钥与 Agent identity generation，后续读取核对现行轮换/撤销状态。失败不得留下可使用的半绑定或默认权限。
4. Viewer、pure API 与 Agent 适用入口读取该权威结果，分别显示注册待决、已绑定、恢复待补证或拒绝；receipt/结果要回指该主体、世界、目标 Agent 与原请求。完成后仍按独立 `claim_starter_oc`、经济 `ClaimAgent` 和 [canonical owner goal](../../world-runtime/runtime/agent-cognition-lifecycle.design.md#11-canonical-owner-goal-transport) 合同检查对应资格，不能把 session 注册 ACK 当作这些结果。

三种情境按 [PRD 入口矩阵](gameplay-agent-claim-economy-contract.prd.md#player-session-first-agent-entry) 验收。world/chain/genesis 固定，新账号加入不能重建世界；返回账号恢复原 Agent、目标与产物。幂等查询沿原请求 identity/digest，冲突 digest 拒绝；原结果尚未知时不能换 nonce 自动重提或另建 Agent。经济 Claim 的 Prepare/Confirm 终结与 fixture 隔离仍由其独立交付验收，本节不替代该状态机。

<a id="des-claim-chat-002"></a>
### 6.2 首聊 gate 与资源边界投影

承接 [REQ-AGC-CHAT-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-001) 的已交付文案切片。现有生成条件为 `liquid_balance == 0 && !starter_oc_claims.contains_key(primary_agent_id)`；文案修正不改变 chat gate。下表以余额与领取记录均完整的权威输入为前提，本地回归按此验证；service 当前限制见 6.1 节，待实现的未知状态协议见 6.5 节。

| 余额与记录完整的权威输入 | `first_chat_unlock_preview` | 必须表达的边界 |
| --- | --- | --- |
| liquid OC 为零且没有 starter OC 记录 | 生成现有六字段预览 | 资源和延后字段都解释完整 OR 条件；`recommended_unlock_action` 仍标识 `claim_starter_oc`。实际可执行性由既有动作面判断。 |
| liquid OC 大于零，或已有 starter OC 记录 | 不生成该解锁预览 | 任一条件都足以通过这一道 gate；已有记录时余额用尽不重开必领提示，也不产生重复领取资格。 |
| 调用方传入的 Agent 在世界中不存在 | 沿用既有 `None` 结果 | 不为缺失 Agent 生成首聊解锁预览。 |

`resource_boundary` 独立包含一次性首聊解锁与初始 liquid OC 授予、`liquid OC > 0 OR starter_oc_claims record exists`、记录保留时余额用尽仍通过，以及与经济 slot-1 claim/upkeep funding 分离。`defer_effect` 复用同一 gate 解释，避免两个字段产生不同含义。通过 starter OC gate 不代表其他授权、Agent 控制权或 provider 前提已经通过。

兼容旧文案时，Viewer 只补全上述稳定规则说明，不用它重新计算当前 gate 或发布可用动作。

<a id="des-claim-chat-003"></a>
### 6.3 经济 claim 事实与延后后果

承接 [REQ-AGC-CHAT-002](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-002)。`chat_purpose` 统一使用当前有控制权的 Agent；经济维护责任由独立的真实 claim 事实决定。

| 输入与证据 | `defer_effect` 规则 | 判定所在层 |
| --- | --- | --- |
| 完整经济 claim 集合中，当前 claimer 的 `owned_claims` 为空 | 暂缓不改变当前 Agent 绑定，也不新增经济认领或 upkeep 义务。 | Rust 权威快照生产者；不从未来报价或其他 Agent 的 claim 推导已有义务。 |
| 完整经济 claim 集合中，当前 claimer 的 `owned_claims` 非空 | 暂缓不改变已有经济认领，其当前维护规则继续适用。 | Rust 权威快照生产者；不额外收费、延长 runway 或清除原有义务。 |
| Viewer 仅收到旧版无条件 upkeep 文案，或 `defer_effect` 缺失、为空、只有空白 | 经济认领义务尚未确认，请刷新权威状态；暂缓不创建或清除义务。 | Viewer 兼容回退；该旧文案或缺失说明无法证明有 claim 或无 claim。 |

两类权威文案及 unknown 回退都附同一 OR gate 解释。所有变化限于披露，查看和延后不触发 claim、release、费用或账本动作。既有经济规则仍随世界时间运行；当前经济集合为空不证明所有历史未决义务已经结清。

<a id="des-claim-chat-004"></a>
### 6.4 显示、语言与兼容处理

承接 [REQ-AGC-CHAT-003](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-003)。沿用六字段 preview、现有动作面与精确字符串本地化映射。新生产者直接输出准确英文；Viewer 对同一含义显示对应中文。

| 场景 | 中文含义 | English meaning |
| --- | --- | --- |
| 资源边界 | 初始 OC 是一次性首聊解锁与初始可用 OC；可用 OC 大于零或已有领取记录即可通过这一道门。领取记录仍在时，余额用尽不会重新锁定此门；它与经济认领/维护资金分开。 | Starter OC provides a one-time first-chat unlock and initial liquid OC. A positive liquid OC balance or an existing starter OC record passes this gate, even after the credited balance is spent. Economic claim and upkeep funding are separate. |
| 已确认无当前经济 claim | 暂缓不改变当前 Agent 绑定，也不会新增经济认领或维护义务。 | Deferring leaves your Agent binding unchanged and creates no economic claim or upkeep obligation. |
| 已有经济 claim | 暂缓不改变已有经济认领，其现行维护规则继续适用。 | Deferring leaves your existing economic claims and their current upkeep rules unchanged. |
| 旧版或缺失义务说明 | 当前经济认领义务尚未确认，请刷新权威状态；暂缓不会创建或清除义务。 | Current economic claim obligations are not confirmed. Refresh authoritative state; deferring creates or clears no obligation. |

兼容转换只识别受影响的旧版精确文案：旧目的说明改为当前有控制权的 Agent，旧版或缺失资源说明补全静态 gate 规则，旧版或缺失义务说明改为 unknown。其他非空来源文本保持原义；缺少翻译时显示来源文本，不退回另一类经济义务句子。此转换不是从文案推断世界状态。

组件响应调用方提供的预览变化，替换上一条说明；身份选择和过期快照处理仍由原有调用方负责。更新该组件源码后通过既有构建生成 Viewer 产物，显示和翻译不提交领取请求或其他 world action。

<a id="des-claim-chat-005"></a>
### 6.5 Service 权威资格与未知状态的后继目标

承接 [REQ-AGC-CHAT-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-001) / [REQ-AGC-CHAT-003](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-003)。本节是尚未实现的后继协议，不是现有 DTO 或 gate 行为；它保留 OR 规则，修复裁剪输入被当作完整世界的歧义。

**生产与权限。** WorldService 为经认证且具有该 Agent 资格读取权限的主体产生最小 eligibility 投影，不把 `main_token_balances` 全账本加入公共 allowlist。该投影只需表达“liquid 是否为正”和“starter 领取记录是否存在”的已知/未知事实、适用范围及 blocker/刷新线索，不要求公开金额、其他账号记录或原始目标文本。Agent 只能消费其已获准的同范围投影；public 或仅有执行权限的读者不能由此取得 owner 私有读权，缺读权返回 unavailable。

**未领取时的窄读权。** 现有 [owner_public_key](../../../crates/oasis7_world_service/src/agent_authority.rs) 从唯一匹配的 `starter_oc_claims.public_key` 解析 owner；现有 authenticated owner View 因而不能作为“无领取记录”资格分支的唯一入口。后继实现选定独立的 `starter_eligibility` 读取用途：以 6.1.1 已提交的账号/world/Agent/identity generation/公钥绑定和原注册结果为权威基础，服务校验调用者对完整读取请求的原始签名、用途、主体/Agent/世界/branch、当前绑定及可信 commit；请求摘要与响应事实沿版本化签名读取协议绑定。绑定/公钥来源必须可从同世界合法历史验证，Hosted 登录、客户端自报字段或旧 sidecar 单独不足以建立此读权。缺证明、旧公钥已失效、错主体或未知版本均返回 unavailable。

该窄用途只返回本节 eligibility；它不调用依赖先有 starter claim 的 owner metadata 路径，也不读取私有目标、建立经济 claim、修改控制 grant 或扩大执行/委托权限。canonical owner goal 继续保留原有独立 owner/claim 验证；即使正余额使 OC gate 通过，其他 owner 前提未满足仍应明确受阻。普通 owner 私有读取的原验证不因该窄入口而放宽。未领取读权与注册绑定的可验证持久记录须随后继实现一同交付，否则保留 unavailable，不能以本地完整 WorldState 代签 service 无记录分支。

**来源绑定。** 投影用版本化表示保留 authenticated subject、Agent identity/identity generation、world/chain/genesis 与适用 branch/authority 绑定，以及既有可信读取协议的 commit/view version。两个事实必须来自同一已验证、一致且仍适用的读取窗口；消费者先验证主体、世界、版本和来源，再判断 OR。范围不匹配、来源冲突、已失效或无法证明新鲜度时，旧事实只可作 last-known 说明，不参与可执行资格。具体 wire 字段沿既有 WorldService 版本机制实现，不另造世界账本。

**判定。** `positive_liquid` 与 `starter_claim_exists` 在此表示带权威来源的三值事实，不是现有字段名；缺失/裁剪字段不能经过默认补齐变为 false。只有 producer 读取完整适用余额/记录集合时，才可沿账本既有规则证明零余额或无记录。

| `positive_liquid` | `starter_claim_exists` | 此 OC gate 的结果 | 玩家下一步 |
| --- | --- | --- | --- |
| 已知真 | 任意，包括未知 | 通过 | 继续检查控制权/provider 等独立前提，无需重复领取 |
| 任意，包括未知 | 已知真 | 通过 | 已有记录保持解锁，即使余额耗尽；不新增领取资格 |
| 已知假 | 已知假 | 阻塞 | 提供仍适用的 `claim_starter_oc` 或其真实 blocker |
| 已知假 | 未知 | unknown | 刷新/补证，不宣称确定锁定或解锁，不自动领取 |
| 未知 | 已知假 / 未知 | unknown | 刷新/补证，不宣称确定锁定或解锁，不自动领取 |

同一主体/世界/版本的相同事实在 Viewer、pure API 与获准 Agent 读面必须同义；布局与传输可以不同，未知不能被转成不同的余额结论。缺失 preview、空动作数组或旧英文句子均不证明 gate 已通过。缺控制权时先呈现控制权 blocker，不显示其他 owner 的资格；取得 OC gate 的通过结果也不绕过 canonical owner、grant 或 provider 检查。

**提交与失效。** eligibility 是只读预览，不是授权凭证。聊天或领取提交时，服务重新核对当前主体/Agent/world、权限与适用余额/记录；旧版本变化按当前状态重评或明确拒绝，不沿用旧资格，也不静默改为另一动作。原请求已有结果或仍未知时继续按原身份查询，不能因为预览变化生成第二次领取/聊天效果。断线、换账号/公钥/Agent/世界、已知 identity/authority 变更或旧读取窗口失效时撤下旧可执行结论；恢复合法的新投影后才重新开放对应动作。

**兼容。** 旧六字段 preview 继续可读为静态规则/历史说明，但不具备新投影的完整性与来源证明，后继消费者将动态资格保持 unknown 并提示刷新；不将旧 DTO 的缺省零、缺省 false 或 `None` 解释成账本事实。新服务与消费者先协商支持的投影版本；不能理解资格版本的组合保持 unavailable，不把未知编码为余额哨兵。当前切片的经济维护说明及 restricted/liquid provenance 规则不变，fragment refill 与 named scenario 投影不在本协议范围。

## 7. 设计边界
- 这不是现实货币付费功能，也不是永久产权出售。
- 这不是 agent 市场或 NFT 化系统。
- v1 先冻结规则结构和默认边界，不在本轮拍死最终绝对价格。
- 这也不是“通用不可转账代币”系统；restricted bucket 只服务于首个 agent 启动。

## 8. 演进顺序
- 先落文档与任务拆解，冻结“首个也不免费”的正式口径。
- 再落 runtime canonical 字段与记账事件。
- 然后落受限 bucket、transfer guard 与 refund provenance。
- 最后补 Viewer / pure API 表达与 QA abuse suite，再决定是否进入新一轮平衡调参。

## 9. 首轮平衡复核结论
- `TASK-GAMEPLAY-AGC-005`（2026-03-27）历史结论：继续维持 v1 默认值，不新开调参专题。
- 维持项：
  - `slot multiplier = 1.0 / 1.5 / 2.0`
  - `grace_epochs = 2`
  - `forced_reclaim_penalty_bps = 2000`
  - `tier cap = 1 / 2 / 3`
- 维持理由：
  - 该历史窗口记录 runtime / viewer / QA 三条闭环通过，当时更缺“真实持有行为样本”，而不是“再造一轮默认值”。
  - 当前成本曲线仍满足本专题最重要的结构边界：首个 claim 非免费、额外槽位单调更贵、欠费与闲置都能回收、refund/slash 可审计。
  - 在还没有真实 claim 分布、平均持有时长、grace 命中率、forced reclaim 占比之前，提前改参数只会稀释对当前默认值的验证意义。
- 后续只有在以下任一条件成立时，才重新开调参专题：
  - `liveops_community` 回流显示 claim churn、grace 命中率或 idle reclaim 占比异常。
  - `qa_engineer` 发现当前 cap / penalty 造成稳定的玩法退化或反滥用失效。
  - producer 拿到首轮真实组织扩张数据，能证明 `slot-2/3` 或 `tier cap` 已经系统性过轻或过重。

## 10. 本轮 Reopening 结论
- 2026-03-29：由于当前文档把 claim 资金来源写死为 `liquid main token`，与受控测试阶段“允许发受限启动资金、但不打开可转账空投”的产品结论冲突，`PRD-GAME-011` 重新进入 `in_progress`。
- reopening 范围：
  - 新增 `restricted starter claim balance` bucket。
  - 新增 `slot-1` 专用消费范围、transfer guard 与 refund provenance。
  - 不改变 `activation fee + claim bond + upkeep` 三段式，也不改“首个 claim 非免费”的根规则。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385；[REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003) | [公开口径边界](#public-claim-boundary) | 设计事实不得冒充认领能力已部署、账户已获准或渠道已发布 | 按 AC-ENTRY-CLAIM-003 人工审核同候选认领说明的四层字段，与根 README、专业规则及受限发放 runbook 同窗对账；加入缺证、历史样本与目标冒充事实负例。文档结构按 [testing-manual 文档验证入口](../../../testing-manual.md) 的 S0/docs-only 合同检查；该自动检查不代签口径审核 | 对应 PR/候选的 QA 与 LiveOps 审核记录；此处为方法，未新增执行结果 | 不证明当前上线、一般账户资格、结算就绪或其他领域/渠道的 claim |
| Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；[`REQ-WR-AOS-001`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001) | [`DES-CLAIM-ECONOMY-001`](#des-claim-economy-001) | 验证首个 claim 的非零成本、受限资金用途与回收边界保持同一规则链。 | [`agent claim runtime tests`](../../../crates/oasis7_runtime/src/tests/agent_claims.rs) | PR 与实际验证记录 / QA result | 不证明生产部署、公开资格或最终价格平衡。 |
| [REQ-AGC-CHAT-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-001) | [DES-CLAIM-CHAT-002](#des-claim-chat-002) | OR gate、余额耗尽后记录保留，以及资源字段独立披露。 | [首聊 snapshot 回归](../../../crates/oasis7/src/viewer/runtime_live/tests/snapshot_first_chat_unlock_preview.rs) 的 `compat_snapshot_projects_first_chat_unlock_preview_only_for_zero_liquid_without_starter_oc`；同一 source HEAD 的本地 required Rust 层。通过条件：零余额且无记录才有预览，两个文案字段均说明 OR 和余额用尽；这些组合区分实际 gate 与旧文案。 | PR 记录命令、HEAD、本地输入来源与实际测试 verdict。 | 不证明 service 余额投影与真实 gate 正确、provider 可用、重复领取资格或已完成真实对话。 |
| [REQ-AGC-CHAT-002](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-002) | [DES-CLAIM-CHAT-003](#des-claim-chat-003) | 实际注册与经济认领的义务区分。 | [首聊 snapshot 回归](../../../crates/oasis7/src/viewer/runtime_live/tests/snapshot_first_chat_unlock_preview.rs) 的 `first_chat_unlock_preview_distinguishes_registration_from_economic_claims`；同一 source HEAD 的 action → snapshot 层。通过条件：真实 RegisterAgent 无经济 claim，真实 ClaimAgent 有 owner/upkeep，其他 Agent 的 claim 不污染文案；同时检查世界记录和说明，避免仅有字符串自证。 | PR 记录世界状态断言与实际测试 verdict。 | 不改变经济状态机或证明所有历史未决义务已结清。 |
| [REQ-AGC-CHAT-003](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-003) | [DES-CLAIM-CHAT-004](#des-claim-chat-004) | 两种语言、旧版与缺失义务说明、预览更新保持相同含义。 | [首聊 Viewer 测试](../../../crates/oasis7_viewer/software_safe_src/first_chat_unlock_preview.test.jsx)；同一 source HEAD 的组件层并构建生成产物。通过条件：EN/ZH 对应、旧版和缺失说明为 unknown、无翻译保留来源、输入更新替换旧说明；覆盖精确字符串映射最易遗漏的兼容分支。 | PR 记录组件测试 verdict、构建结果与证据范围。 | 不证明新鲜度检测、全场景可玩性、真实留存或发行就绪。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-AGC-ENTRY-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-entry-001) | [DES-CLAIM-ENTRY-001](#des-claim-entry-001) | 三情境注册/恢复、第二玩家、隔离与幂等。 | 11.3 后继验证矩阵的 ENTRY 场景；以现有 [auth_actions_claims.rs](../../../crates/oasis7/src/viewer/runtime_live/tests/auth_actions_claims.rs) 空世界样例为基线，后继实现增加 canonical service 两账号与原请求恢复测试，再做同候选入口验证。 | 实现 PR 的固定 source/build、世界身份、原请求/结果及实际测试或试玩记录。 | 当前基线不证明逐账号注册、正式 owner bootstrap 或同世界多玩家验收；本文未运行这些场景。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[AC-AGC-ENTRY-001](gameplay-agent-claim-economy-contract.prd.md#ac-agc-entry-001) | [DES-CLAIM-ENTRY-001](#des-claim-entry-001) | 三情境注册/恢复、第二玩家、隔离与幂等。 | 11.3 后继验证矩阵的 ENTRY 场景；以现有 [auth_actions_claims.rs](../../../crates/oasis7/src/viewer/runtime_live/tests/auth_actions_claims.rs) 空世界样例为基线，后继实现增加 canonical service 两账号与原请求恢复测试，再做同候选入口验证。 | 实现 PR 的固定 source/build、世界身份、原请求/结果及实际测试或试玩记录。 | 当前基线不证明逐账号注册、正式 owner bootstrap 或同世界多玩家验收；本文未运行这些场景。 |
| [REQ-AGC-CHAT-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-001) | [DES-CLAIM-CHAT-005](#des-claim-chat-005) | 真实 service 三值事实、来源/隐私、失效/最终重验及三入口同义。 | 11.3 后继验证矩阵的 GATE 场景；以现有 [WorldService HTTP owner 隔离回归](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/world_service_agent_chat.rs) 的 `canonical_owner_goal_http_view_excludes_public_and_registered_delegate` 为基线，后继实现按真实 `WorldServiceProjection` producer→已认证 read→consumer→submit 增加资格、失效与集成回归。 | 实现 PR 的版本、主体/世界/commit 绑定、原始受限投影与各入口结果。 | 已交付本地 snapshot/组件及投影隔离测试不覆盖此后继资格协议；新增测试需在实现时落地，不声明现有自动化已通过。 |
| [REQ-AGC-CHAT-003](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-003) | [DES-CLAIM-CHAT-005](#des-claim-chat-005) | 真实 service 三值事实、来源/隐私、失效/最终重验及三入口同义。 | 11.3 后继验证矩阵的 GATE 场景；以现有 [WorldService HTTP owner 隔离回归](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/tests/world_service_agent_chat.rs) 的 `canonical_owner_goal_http_view_excludes_public_and_registered_delegate` 为基线，后继实现按真实 `WorldServiceProjection` producer→已认证 read→consumer→submit 增加资格、失效与集成回归。 | 实现 PR 的版本、主体/世界/commit 绑定、原始受限投影与各入口结果。 | 已交付本地 snapshot/组件及投影隔离测试不覆盖此后继资格协议；新增测试需在实现时落地，不声明现有自动化已通过。 |

<a id="first-chat-validation-matrix"></a>
### 11.2 首聊披露的最小验收矩阵

选择同一待验收 source HEAD，执行本节对应的本地权威投影与显示测试，实际 verdict 留在 PR 验证记录。本设计不因列出场景而宣称测试已经通过，也不把文档检查当作运行证据。

| 样本 | 权威前置或组件输入 | 必须观察的结果 | 主要验证层 |
| --- | --- | --- | --- |
| 首次注册、尚无经济认领 | 通过 `claim_first_agent` 映射的真实 `RegisterAgent` 创建 Agent；经济 claim 集合为空；零 liquid、无领取记录 | 有首聊价值预览；资源边界完整；不声称已有 upkeep；未来 `next_claim_quote` 不当作已完成认领。 | runtime action → snapshot |
| 已有真实经济认领及其他 Agent | 真实 `ClaimAgent` 成功，owner/target/维护状态可核对；零 liquid、无领取记录 | claimer 的延后说明保留既有维护规则；世界中有另一 Agent 的经济 claim 不使当前无 owned claim 的 Agent 获得维护责任文案。 | runtime action → snapshot |
| OR 正分支与领取后余额用尽 | 非零 liquid 且无记录；另测记录存在时 liquid 从正数变为零 | 均无必领预览；领取记录保留时不重新要求解锁。 | 状态序列 → snapshot |
| 旧版或缺失义务说明 | 旧版无条件维护句子；或 defer 字段为缺失、null、空白 | 义务说明为 unknown，不虚构已有 claim，也不改写成没有责任；资源说明保留完整 OR 条件。 | Viewer 兼容显示 |
| 中英文与无翻译回退 | 无 claim、有 claim、unknown 与无对应翻译的来源说明 | 两种语言的条件和义务一致；没有翻译时显示可读来源文本；保留六个字段标签及建议动作含义。 | Viewer 组件与生成产物 |
| 预览输入更新 | 调用方依次传入无 claim、有 claim、缺失义务说明的预览 | 组件替换旧说明，并按新输入显示对应含义；不残留上一条 claim 结论。 | Viewer 响应式显示 |

经济义务断言须与实际世界记录核对；只断言旧字符串存在或组件存在不足以验收。11.2 是已交付文案切片的验证矩阵，不覆盖真实 service 余额输入、DTO 完整性、gate 三态或快照失效检测；后继要求见 11.3，不能以旧结果或 Issue 关闭替代新协议的实际验证。

<a id="claim-entry-service-validation-matrix"></a>
### 11.3 逐账号入口与 service 资格的后继验证矩阵

以下是待实现协议的验证计划。先对固定候选执行普通合同/集成回归，再按 [Agent 游戏功能自测范围](../../engineering/workflow/source-of-truth.md#agent-gameplay-selftest) 完成所交付真实入口的适用独立试玩与图形观察；本次仅收敛设计，不启动世界或宣称这些目标已通过。

| 场景 | 必须核对的输入/序列 | 可判定结果与验证层 |
| --- | --- | --- |
| ENTRY-1 三种进入情境 | 空世界首账号；已有 A 的同世界新 B，无 claimer；A 返回 | 各自得到合法注册/恢复或真实 blocker。B 的成功不修改 A 的 Agent/成果，不新建 genesis；canonical admission、结果投影及同候选入口层 |
| ENTRY-2 原请求恢复与隔离 | 同账号并发首次请求、原 ACK 丢失、retry/restart、冲突 digest、换账号/Agent | 至多一个首次绑定；原结果查询不重建 Agent/领取/收费；跨账号拒绝；持久请求与权限回归。当前固定 starter 测试不足以代签 |
| GATE-1 三值 OR | 同主体/Agent/world/版本的两个事实各取已知真、已知假、未知，覆盖全部 9 组合；另测已领后余额由正到零 | 任一已知真通过、双假阻塞、其余未知；真实 service producer/read/consumer 与普通合同回归，不只构造完整本地 WorldState |
| GATE-2 裁剪与权限 | 缺余额字段、缺记录字段、明确完整集合的零/无、错误主体/Agent/world/版本、公共及仅执行读者 | 不把裁剪当零；不混拼版本；仅适用窄读权或独立 owner 权限可消费投影，不公开全账本或他人资格；服务序列化与鉴权负例 |
| GATE-2A 未领取读权 | 已提交注册/绑定而无 starter claim，分别零/正余额；持原始签名读取；另测自报绑定、伪造/撤销公钥、错误用途及旧 sidecar | 合法窄读返回同世界资格，零/无为阻塞、正/无通过 OC gate；无效证明 unavailable。均不暴露 owner goal、不取得额外 control grant，也不绕过原 owner metadata/goal 的独立 claim 前提；真实签名 service 集成层 |
| GATE-3 失效与兼容 | 新旧投影版本、旧六字段 DTO、断线/重连、换账号/公钥/Agent/世界、已知权威窗口失效 | 旧结论撤下，unknown/unavailable 与补证路径可读；刷新后恢复当前结论，不通过缺省值推断权限；Viewer/API/Agent 状态序列 |
| GATE-4 最终提交 | 预览通过后余额/记录/权限改变；预览 unknown 后补证；提交 ACK 丢失及原请求恢复 | 当前权威再校验或明确拒绝，无静默替代/重复效果；同世界 service submit/outcome 与持久请求回归 |
| GATE-5 跨入口 | 同候选、同世界和账号，在 Viewer 中英文、pure API、获准 Agent 读面读取同一权威窗口 | gate、未知原因、其他 blocker 与下一步同义；无读权入口 unavailable。API 与组件通过不代签实际图形交互、真实 provider 自主性或留存 |

ENTRY 与 GATE 的受测世界/身份必须来自获准的测试条件；工程 fixture、正式世界开放和真人回访仍分别取证。经济 Claim Prepare/Confirm 及场景/refill 的独立验收不因本矩阵通过自动完成。
