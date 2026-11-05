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
- 将 agent 认领从“谁先点到谁拿到”改为有成本、需维护、可回收的正式 gameplay 经济链路。
- 保持 `Agent 唯一性` 与资源守恒，不把 claim 机制做成可绕过 runtime 审计的侧门。
- 让 `runtime_engineer`、`viewer_engineer`、`qa_engineer` 使用同一套 claim 状态机和字段。
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
| Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；[REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001) | 同候选确认前逐项说明用途、差异、首个目标帮助、风险、完整 upfront/runway 与实际替代选择 | [候选承诺包](#claim-choice-commitment) | runtime 权威 producer / Viewer、API、Agent consumer | 不从模块名、候选唯一或 UI 存在推导正式适用；不新增经济参数或布局 |
| Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；[AC-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-001) | 只读预览无预留，确认时对同一目标、条款、来源、资格、容量与资金当前重验，漂移/争抢不半扣费 | [确认重验](#claim-confirmation-revalidation) | claim publication / current authority / 共享取得守卫 | 保留制造、首次注册及首次支持各自原准入，不让普通 ClaimAgent 绕工业供给 |
| Trace refs: O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287；[REQ-WR-AOS-002](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-002) | 原因、范围、费用与仍存责任分别接收主动释放、欠费/闲置回收和真正适用的限制 | [失去控制与连续性](#claim-loss-continuity) | 原 claim、身份、制造、合同与适用保护 authority | 不新增罚则、通用申诉、资产转让或让治理资格限制变成全 Agent 禁用 |
| Trace refs: O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287；[AC-WR-AOS-002](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-002) | 重认领、恢复、原案救济与安全停止分别核当前来源；旧结果、旧义务和一次效果不断链 | [当前恢复](#claim-current-recovery) | 原请求查询 / 当前读权 / 原领域终结器 / G1 | 不把旧 receipt、余额变化或 active 列表缺项当作新取得资格 |
| Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；[REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001) | 确认丢 ACK、重连、来源失效后仍定位原目标和原结果，不静默重认领 | [当前恢复](#claim-current-recovery) | 原 GameplayAction / 正式结果回读 / QA | 查询不重新消费业务 nonce 或创建第二次 claim/support |
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
  - `upkeep`: 每个 epoch 结算一次，体现持续占有 agent 的成本。
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

<a id="claim-purpose-source"></a>
**情境来源采用。** 上述 `reputation_score` 是旧 runtime 输入的事实，不是无范围累计分继续授予新资格的目标合同。本轮接收[用途政策](gameplay-contextual-reputation-contract.prd.md#contextual-reputation-consumer-policy)：在明确启用的采用版本中，用当前稳定主体的 `agent_acquisition` 合格来源值进入原 `10/25` 阈值和 `1/2/3` cap；只读来源、签署用途、实际双向履行、完整覆盖、独立对手、到期及更正的具体计算归[来源准入](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-purpose-admission)。不复制新的全局分数，不改费用、tier cap 或 restricted/liquid 规则。

quote 与最终 claim publication 必须核对同一适用 policy、主体、world/branch、来源 root/有效窗口及当前容量；读取与提交间失效或更正后重新报价/拒绝。已证无来源和未知覆盖分别表达，不能在新入口沿用旧 `unwrap_or(0)` 解释未知，也不能由客户端传入 tier。制造取得复用同一来源与去重容量守卫；旧持有、bond、upkeep 和历史 receipt 不追溯重算。正向额外槽、重复/关联主体、失效、原件缺失及并发入口的方法由[情境声誉验收](../../testing/manual/contextual-reputation-acceptance.manual.md)与原 claim/制造方法组合验证；本次未运行这些业务场景。

## 4. 退款与 Provenance
- claim 时要显式记录：
  - `upfront_restricted_spent_amount`
  - `upfront_liquid_spent_amount`
  - `claim_bond_locked_restricted_amount`
  - `claim_bond_locked_liquid_amount`
- release / forced reclaim 时：
  - 先按 canonical 规则结清 arrears 与 penalty。
  - 再把剩余 bond 按 provenance 拆成 `restricted refund` 与 `liquid refund`。
  - restricted 来源的 refund 按原 provenance 与有效期回原 restricted balance 或原 source treasury bucket，不能洗成 liquid；准确分流见[失去控制与连续性](#claim-loss-continuity)。
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

既有 [gameplay action 映射](../../../crates/oasis7/src/runtime_host/gameplay_actions.rs) 将 `claim_first_agent` 映射为 `RegisterAgent`，将 `claim_agent` 映射为 `ClaimAgent`。首次绑定只建立可操作 Agent；首聊维护说明需要另外读取该 Agent 作为 claimer 的真实经济 claim。

以下表格与 6.2–6.4 节记录已交付的文案/兼容切片；后继目标由 [DES-CLAIM-ENTRY-001](#des-claim-entry-001) 和 [DES-CLAIM-CHAT-005](#des-claim-chat-005) 单独规定，不能将目标能力读成该切片已有行为。

| 组件 | 已交付文案切片的职责 | 依赖与边界 |
| --- | --- | --- |
| [claim_snapshot.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/claim_snapshot.rs) | 读取调用方传入的 `WorldState` 和 `primary_agent_id`，保留现有 preview 生成条件，按真实 `owned_claims` 选择维护说明；输入覆盖范围见下文。 | 当前身份、控制权和动作 blocker 仍由既有控制面处理；本函数不接收旧 DTO、不写账本或执行动作。 |
| [first_chat_unlock_preview.jsx](../../../crates/oasis7_viewer/software_safe_src/first_chat_unlock_preview.jsx) | 本地化新的资源说明和两类权威维护文案；旧版无条件维护文案或缺失义务说明回退为 unknown。 | 只消费六字段 preview，不从字段缺省值、经济报价或英文句子反推 ownership 或动作资格。 |
| [viewer_app.jsx](../../../crates/oasis7_viewer/software_safe_src/viewer_app.jsx) / 既有 API 投影入口 | 沿用当前预览传递、展示和可用动作合同；同一预览在 Viewer / API 中保留相同含义。 | 不为此次文案修正新增授权、身份、快照版本或缓存失效机制。 |

本地 runtime 测试使用完整 `WorldState`：`main_token_balances[primary_agent_id].liquid_balance` 沿用缺少账户项为零的既有语义；`starter_oc_claims.contains_key(primary_agent_id)` 提供领取记录存在性；`agent_claims` 中 `claim_owner_id == primary_agent_id` 的记录组成现有 `owned_claims`。`next_claim_quote` 是未来选择，不能证明当前经济认领已经生效。

service 来源需单独限定：[world_service/projection.rs](../../../crates/oasis7_world_service/src/projection.rs) 的 allowlist 保留 `agent_claims` 与 `starter_oc_claims`，但不发布 `main_token_balances`；[world_service_link.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/world_service_link.rs) 用该裁剪状态重建 runtime world，再由 [control_blocking.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_blocking.rs) 调用本投影。经济 claim 事实仍可用于维护说明，但 service 的真实正 liquid 余额且无领取记录可能因余额未投影而按零值生成预览。这是既有余额输入缺口，已交付文案切片未修改字段公开范围或 gate；本地回归及同一预览的中英文/API 显示一致性，不能证明真实 service gate 正确。后继协议须按 6.5 节修复权威输入与消费边界。

已交付切片只修权威生产者文案与 Viewer 旧文案兼容，没有 DTO 字段存在性追踪、count/list 协调、gate 三态或资料新鲜度检测；这不是对后继协议的禁止。本地状态与当前 service 投影均保留 `agent_claims`；按当前 claimer 筛选的完整结果为空时，才支持没有 owned claim 的结论。经过默认值补齐的旧 preview DTO 不提供这种证据，Viewer 不使用它来生成“没有经济认领”的新结论。

<a id="des-claim-entry-001"></a>
### 6.1.1 同世界逐账号注册/绑定的后继目标

Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270, O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196。

承接 [REQ-AGC-ENTRY-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-entry-001) 与 [REQ-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-018)。当前 [player_gameplay.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/player_gameplay.rs) 的 native `claim_first_agent` 仍限制固定 `FIRST_AGENT_CLAIM_TARGET_AGENT_ID`，该目标已绑定时拒绝；[auth_actions_claims.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/auth_actions_claims.rs) 的空世界注册、旧 sidecar binding 恢复只证明局部路径。此设计不宣称当前 WorldService 或该固定入口已能接纳第二账号。

后继入口继续复用注册与身份绑定 authority，按以下顺序执行，不让 Viewer 直接创建 canonical 归属：

1. 认证稳定账号主体、当前 session/公钥和已存在世界的合法准入；将 world/chain/genesis、适用 branch/身份版本、原请求身份与 payload digest 绑定到注册请求。首次注册只在该受限入口免除“已有 claimer”的经济动作前提，不豁免 session、世界准入或其他动作授权。
2. 从权威绑定读取该账号在此世界的结果。已有匹配绑定时返回原结果或进入原身份恢复；原请求 pending/unknown 时查询同一请求。身份/绑定证据冲突、缺失或无法确认是否已有绑定时 blocked；只有权威确认该已认证账号尚无绑定，才进入第 3 步。不把清缓存、换设备或旧 sidecar 缺失当作新首次资格。
3. 只有合法、尚无绑定的新账号才分配自身 Agent identity 并注册/绑定。新身份不得占用其他账号 Agent 或复用已绑定的全局 starter ID；并发首次请求在同一账号/世界的绑定唯一性边界内协调。原请求结果、注册和 canonical 账号/Agent 控制绑定必须在同一可恢复提交边界确认后才公开为可控；绑定持久保留已认证账号主体、公钥与 Agent identity generation，后续读取核对现行轮换/撤销状态。失败不得留下可使用的半绑定或默认权限。
4. Viewer、pure API 与 Agent 适用入口读取该权威结果，分别显示注册待决、已绑定、恢复待补证或拒绝；receipt/结果要回指该主体、世界、目标 Agent 与原请求。完成后仍按独立 `claim_starter_oc`、经济 `ClaimAgent` 和 [canonical owner goal](../../world-runtime/runtime/agent-cognition-lifecycle.design.md#11-canonical-owner-goal-transport) 合同检查对应资格，不能把 session 注册 ACK 当作这些结果。

三种情境按 [PRD 入口矩阵](gameplay-agent-claim-economy-contract.prd.md#player-session-first-agent-entry) 验收。world/chain/genesis 固定，新账号加入不能重建世界；返回账号恢复原 Agent、目标与产物。幂等查询沿原请求 identity/digest，冲突 digest 拒绝；原结果尚未知时不能换 nonce 自动重提或另建 Agent。经济 Claim 的 Prepare/Confirm 终结与 fixture 隔离仍由其独立交付验收，本节不替代该状态机。

#### Canonical 首次绑定协议与信任边界

首次绑定使用专用 `participant_register` 用途。现有 `player_session` / `player_session_release` 只授权相应会话操作，不能作为此用途的 grant。稳定业务原件固定 identity service/environment/instance/account、world/genesis、owner 公钥与版本、客户端 operation 和受限注册参数；owner 独立签名。identity issuer 在当前账号、凭证、session、准入及配额检查后，对相同业务 digest、owner、world、branch、policy generation 和有限 tick 窗口签发专用准入证明。登录密钥、owner 密钥与 executor 密钥分别保管，owner 不成为默认 delegate。

准入 grant 与稳定业务相关性分离。刷新登录或准入只重新授权相同原请求及 digest，不改变 operation、业务签名、nonce 或绑定唯一性。历史 owner 签名不是永久登录态，公开查询和重放均须当前认证；当前认证失败时不回退到匿名读取私有绑定。原请求已提交且响应丢失时先查原结果；无法确定旧绑定或丢失原件时保持 unknown/blocked。

Runtime 使用版本化 participant admission policy，默认禁用。policy 明确 issuer namespace/key、identity epoch、world/genesis、branch/authority generation、确定性有效 tick 范围、注册预算及迁移完成条件，并由 canonical 管理事件安装或更新。执行和重放只读取已提交 policy 与执行上下文，不能读取环境变量、墙钟、身份数据库或网络。HTTP 层的当前认证不能替代 canonical 验签。已有世界缺少完整历史绑定证据时不能以空 registry 推断新首次资格；启用前须显式完成相应迁移，不自动重建账号或 Agent。

客户端准入协议与 policy 管理授权来源分开。官方客户端与外部客户端共用上述身份、owner/executor、唯一性、任务及回执检查，不携带部署类型选择器，也不承担世界治理初始化。正式网络的 policy 安装/更新必须携带治理 finality proof。获批准 proposal 的 manifest 明确承诺专用 install/update purpose、完整 policy digest 和已签署迁移 manifest digest，certificate 必须批准该 manifest；不得借任意历史 Approved proposal 的 signer 集另签新文本。附加签名集与证书 signer 集一致，变更 generation 连续，关闭只停止新首次资格，保留已绑定主体的当前认证恢复、历史 policy 和原结果。正式网络的 issuer key 轮换通过同一治理更新，不从 HTTP 或环境变量覆盖 canonical policy。

DevLocal policy 由显式启用的本地 operator 管理来源授权，使用独立用途签名、固定 operator 公钥及独立历史记录，不构造 proposal、certificate 或 finality epoch。Host 从最终生效配置核验真实 `StorageProfile::DevLocal`、非 hardened policy 和不存在正式网络/guarded authority，再建立不可由客户端请求反序列化的本地管理上下文。单独的环境变量、请求正文中的 profile 名称或 `proof=None` 都不能授予安装权限。operator 签名绑定完整 policy 与 issuer-signed 迁移 manifest；issuer 完整性、前态 digest、world/genesis/branch/reorg、owner 映射、连续 generation 和永久唯一性检查与正式来源共用。

本地来源持久化到 canonical event、授权根与 policy 历史，快照恢复和 journal tail replay 先获得明确的 Host 安全上下文再校验。默认恢复不暗授本地管理能力；缺失 operator pin、错误 pin、非 DevLocal、hardened 或正式网络 authority 均拒绝带本地历史的世界。不能用切换 storage profile、删除来源标记或重启将本地授权升级成治理授权；转换到正式治理须另有明确的治理迁移协议。既有正式 proof 和历史 wire 形状保持可读，两个管理来源不得静默混用。

本地管理入口是持有现有 world writer 排他锁的 node 配置/执行驱动控制面，政策变更与正常 canonical candidate 一起提交；公开 participant/owner/executor API 不安装信任根。相同已提交配置重启后只回读原结果，不重复迁移；无效、过时或冲突配置不发布半状态。运行中引入正式 validator admission、finality 或其他冲突 authority 的变更在事件发布和 canonical 持久化前拒绝，保留普通 proposal，不写出下一次读取无法恢复的快照。既有世界不因启用本地管理而重建或发放额外资产。管理权限只建立信任规则，后续每次入场仍需正常 session、owner 签名、有限 grant 及 canonical 注册检查。

旧绑定迁移 manifest 由受信 issuer 按专用用途签署，固定 identity namespace/instance、世界、branch/reorg、数据库 snapshot epoch、水位、完整清单 digest、完整性声明及 canonical registry/legacy-owner 前态 digest；每个账号到初始 Agent/owner 的映射还须独立 owner 签名。相应管理来源（正式网络治理或 DevLocal operator）授权同一清单，承担“issuer 提供的是其权威旧绑定全集”的信任边界；canonical 节点不能证明外部数据库不存在遗漏。执行逐项核对实际 Agent、当前 owner 与 generation，拒绝重复账号/Agent、错世界、过时前态、错误映射和缺失签名，不改 starter/经济资产。显式空清单同样必须由 issuer 签署并经相应管理来源授权，不把本地空 map 当证据；必要材料未知时阻塞迁移。

永久唯一索引以稳定 namespace/account/world 为键，branch 只作为执行 fence，不产生第二份首次资格。绑定保留 Agent、owner/version、identity generation、原 operation/digest 和权威结果引用；结果归档不能删除资格记录。上述 policy、绑定及变更事件进入能力授权状态、授权根、快照、恢复校验和重放。旧快照缺少 policy 时默认拒绝新注册，保留原世界状态。

旧 DevLocal Preview 的 resource manifest 若丢失 genesis 引用，仅在显式 operator 授权下，从原 node key 独立派生的 service 公钥验证原初始化收据与 genesis artifact，核对原世界 identity 及同一 signed policy 后，在安装策略的 canonical candidate 补回该原引用。不能从 policy 或普通 identity 文件单独创造 genesis，不能替换已有非空引用；正常提交和重启持续固定已绑定身份。这只是原身份的有限核验，不替代完整历史/初始 publication archive 的恢复验证。

稳定 namespace 只含 identity service/environment；identity instance epoch 与 branch 均不是首次资格的一部分。离线推进身份 epoch 保留账号，policy 允许 epoch 单调推进，旧 session/grant 失效；已绑定账号通过当前认证和当前 epoch grant 查询其历史业务原件/receipt，不重签、不改原 digest、不新建 Agent。历史 epoch 必须曾被该世界 policy 授权，未知来源不能凭同名账号恢复。

在一个可回滚 canonical candidate 内检查唯一性和预算，分配不占用他人的 Agent ID，实际执行既有 `RegisterAgent` 并核对成功事件，再安装 canonical owner identity、写绑定与结果。仅 `submit_action` 入队不构成注册成功；不得调用 `step()` 顺带执行其他待办动作。任一步失败不发布半绑定、owner、资产或成功 receipt。同账号的竞争请求最多创建一个 Agent；不同 operation 读取原绑定时不得替换 owner 或重复给付 starter。starter 和经济 Claim 继续由各自 authority 单独确认。

当前实现候选必须以真实 Agent/owner 结果、并发、失败回滚、快照/重放一致性及错 issuer/world/branch/owner/用途负例验证本节；这些测试不代签公开 bootstrap 或两个实际 Runtime 的完整冷入场。

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

<a id="claim-choice-commitment"></a>
### 6.6 同候选的完整认领承诺包

Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；承接 [REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001) 与专业 PRD §2.2.1。本节细化既有 quote/choice 的生产与消费，不增加 claim 状态、经济字段、数值或 Viewer 布局。它适用于实际形成经济 claim 的入口；注册/会话绑定仅依其真实结果解释，已绑定主 Agent 也不能据此认领任意世界背景实体。

**真实生产者与已知缺项。** [claim_snapshot.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/claim_snapshot.rs) 的 `build_player_agent_claim_snapshot` 产生成本/runway，`build_slot_1_claim_choice_quote` 枚举候选并调用 `build_candidate_rationale`；当前实现仅在恰有一个已知理由候选时填充五个共同理由字段，多候选则保留 `compare_candidates_first` 但这些字段为空。现有候选按 body/frame 和模块 ID 过滤，不含完整正式能力/取得准入证明。[选择回归](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/claim_choice.rs)与[runway 回归](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/claim_runway.rs)是本地兼容 snapshot fixture，不能据此签署正式多候选比较、Service 接线或所有入口已实现。

最小目标 producer 仍使用原五个 `candidate_starting_location / candidate_specialty_summary / first_industrial_goal_help / candidate_risk_summary / candidate_recommendation_reason` 字符串。每名候选先独立生成五项；单候选沿原表达，多候选在每个字段内按候选 ID 稳定顺序分段，五项各段的 ID 必须精确指向同一 cut 的候选条目。字段不能只保留一条无对象的共享理由，也不能只列 module 名充当用途差异。客户端完整显示这些来源文本，不解析段落来授权、排序或构造新 action。现有多候选空理由需要 producer/consumer 接线后验证，不能以 `unknown` 作为完成设计的替代。

| 必需信息 | 唯一事实输入与确定性生成 | 缺项、比较与消费边界 |
| --- | --- | --- |
| 对象、位置与可取得性 | 同一可信 World/branch/commit 的候选 ID、当前 AgentCell、适用原取得来源与当前可见范围；位置用原坐标和单位，目标以原 ID 选择 | 主绑定、经济 claim、制造来源分别验证；未获读权不泄露其他主体私有目标/配置。可见实体不等于当前可 claim |
| 用途、能力与差异 | 原 body/frame 及当前采用版本的能力目录、实际绑定/安装/激活与可用状态；从原能源、感知、移动、cargo 等支持能力说明可承担的工作 | [Body/附件能力合同](../../world-runtime/module/agent-default-modules.design.md#adm-body-binding)拥有实体/能力规则。默认模块 ID、未知 manifest 或本地占位只可报告其窄事实，不能证明实际 capability；缺 cargo 与已知完整配置应得到不同理由 |
| 首个工业目标帮助 | 当前已发布的工业阶段/目标及其已有前置，逐项对应该候选已证明能力；Bootstrap 只消费该版本已支持的第一目标 | 不读取他人 private goal，不由 LLM/客户端补目标，不保证产出、路线到达或设施权限；已知能力缺口明确列出，未知目标/来源明确待刷新 |
| 风险与推荐 | 原配置确有的限制、已证明缺失能力、当前取得 blocker、完整费用/runway；比较列出实际差异，不发明得分、价格优惠或“最佳”排名 | 只有已知完整集合中恰一名适用且能力齐全、无已证明高风险、资金足且 runway 达原建议阈值时，才能沿原 `claim_now_route_fit` 建议。多名保持比较，缺资金/低 runway 沿原 wait/fund 建议；建议不改变动作资格 |
| 后续义务和替代选择 | 同一目标的 activation/bond/首个 upkeep、后续每 epoch 维护、原 release/grace/idle 条件，以及当前可执行的比较、等待、补足或保持现有经营 | 预览不锁候选、扣费或授支持；低 runway 是风险提示，不凭此添加经济硬门槛。没有合法替代动作时准确给 blocker/下一次核对条件，不伪造恢复按钮 |

producer 先固定一次当前已授权读取范围和一致 view，再生成全部字段；候选数据、成本、用途来源和推荐不能混用不同 commit、账号或 source revision。有限入口必须遵守原读取/载荷预算和当前权限；分段/分页只描述该范围，未读部分不得当作无其他候选，部分集合一律不能形成“全局唯一”或 `claim_now_route_fit` 结论。若当前入口没有可验证完整范围/版本，仍可给已知事实和明确 incomplete，不发布可执行完整承诺；不得为凑完整包额外扫描全世界、展开私有记忆或调用模型猜测。

成本继续原规则：默认 slot-1 `100 + 200 + 25 = 325`，资金消耗顺序和自动受限支持只按 §3–5 真实来源。已知 eligible=375 时，确认后50支持2个完整 upkeep epoch；374时49只支持1个，均能支付325，后者警告不变成拒绝理由。下一次义务是原 `paid_through + 1`，预估 grace 只是假定未来无其他收入/支出的条件预测；不能借刷新延长维护。已到账合法 liquid 沿原可用语义，首聊解锁记录、未来未发支持或 bridge quota 不能再次算作付款余额。

<a id="claim-confirmation-revalidation"></a>
### 6.7 显式确认、来源漂移与一次取得

Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；承接 [AC-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-001)。Prepare/比较/查询只生成原决策输入。确认继续使用正式原请求及目标 ID，不从候选文本导出身份、授权或价格；接收端绑定其实际 world、当前主体、目标和所确认的条款，按原请求协议保留签名/nonce/原件，不另造业务 receipt。

1. 先选择真实入口 profile，核当前 principal→claimer→target、session/owner/source/identity 和原用途读写权限。`ClaimAgent` 的 [industrial acquisition guard](../../../crates/oasis7_runtime/src/world_rules/industrial.rs) 对已绑定或已有 claim 的主体限制额外非制造取得，制造 actor 不可递归取得；候选可见、primary 已绑定或有余额都不能旁路它。制造交付/后继合法 claim 依各自原来源，首次注册仍不产生经济 claim。
2. 在同一可发布 parent/staged view 重验目标未被他人认领、原 policy/费用/资金 provenance、[取得情境来源](#claim-purpose-source)、当前去重容量、真实可用资金及相关预留。来源撤销/更正、目标变化、相关条款漂移或资金不足，撤下旧确认并返回当前 blocker/重新读取；不得自动换候选、降费、补省略来源或重领支持。若本入口只重算当前费用而不能绑定此前确认条款，则它尚不满足本确认合同，不能用 accepted 响应代签。
3. 无关世界推进不是换价依据，但实际 epoch、资格窗口或完整条件已改变时必须依原规则核对。明确已知无来源和未知覆盖分别处理；采用新来源政策的入口不得用 legacy `unwrap_or(0)`、客户端 tier 或历史 receipt 补足未知。
4. [原 claim publication](../../../crates/oasis7_world_state/src/world/agent_claim_economic_publication.rs) 拥有本次完整资金分拆、原支持发放、bond、claim 及适用控制来源/取得序号的原子结果；任何拒绝无部分 claim、扣款、支持或容量效果。两个合法主体争同一 target、同一主体争最后一个容量/同一份资金时，按 canonical 顺序仅允许满足全部条件的结果；失方保留真实拒绝，不另收费完成一次失败的 claim。
5. 世界规则/技术调用原本适用的 Data、Electricity 或 capability 计量仍按各自合同结算，不能把“未产生 claim 经济效果”写成所有入口无协议/运行成本。原请求重复查询、重放和新业务请求分开；一次原 claim 成功不会因换 request、重启或 UI 重开而再次产生效果。

<a id="claim-loss-continuity"></a>
### 6.8 失去控制的原因、资金与原责任

Trace refs: O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287；承接 [REQ-WR-AOS-002](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-002)。结果包由原 domain result 与当前受众允许的状态组合，至少说明 target/原关系、原因、当前生效范围、原费用/退款出处、保留的成果和未结义务、下一合法动作及 blocker。阶段、控制可用性、资金结算、合同责任分别表达；`owned_claims` 只是当前集合，空列表或旧 active snapshot 均不能证明历史已终结、欠费清零或可以重领。

| 原因/来源 | 真实生效与资金边界 | 当前下一步及连续性 |
| --- | --- | --- |
| 主动 ReleaseAgentClaim | [原请求](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_gameplay_claims.rs) 将接受时 epoch 记为 `release_requested_at_epoch`，ready=requested+原 cooldown，默认3；非 claim 起点。重复请求拒绝，不刷新 ready；请求本身未释放名额、控制来源或退款 | 冷却内 upkeep 继续。实际 epoch processor 先用可用 restricted/liquid 结当前 upkeep，满足 ready 且无欠费才发布 Released；不能套新处置的 bond 抵欠规则提前完成旧 Release |
| 欠费 grace / forced reclaim | [原 epoch producer](../../../crates/oasis7_runtime/src/world/agent_claims.rs) 以首次实际缺付检查点 e 设 deadline=e+grace−1，current>deadline 才强收；grace 中补足按实际累计 due 结算，不重置历史 | paid_through=0、逐 epoch 检查且默认25/2：e1缺25、deadline2，e2未回收，e3欠75；bond200先收75，再对余125罚25，余100按原 provenance 返还。跳 epoch 使用其实际检查点，不套本逐步 fixture |
| 闲置 warning / forced reclaim | 按目标 Agent 的原有效活动时间，默认7 warning、10 forced；到期、原罚和退款来自原事件，不用页面刷新/重连假造有效活动 | warning 可报告原合法控制/维护途径；是否恢复由真实后态判断。终态保留历史和原责任，不能把主动退出显示为惩罚或把强收称为自愿释放 |
| restricted 发放到期/撤销 | 它先改变该资金源可用性和原退款 sink，不自动等于 Agent claim 终态。原 claim 的固定 restricted source bucket 优先原路返回；否则有效 grant/无 grant 走原 restricted balance，Expired/Revoked 走原 source treasury bucket | 不将退回受限来源洗成 liquid，也不把尚锁在 bond 的金额当当前可付 upkeep。普通 Released 不收新罚；欠费/闲置处罚仅消费原已采纳规则 |
| 当前身份/来源、授权或争议限制 | 精确原 scope 决定哪些后续动作受阻，经济 claim、会话、grant 与治理资格不合并成一个所有权布尔值。[保护目录](../../world-runtime/runtime/protective-governance-authority-and-recovery.design.md#protective-action-catalog) 的治理 freeze/demotion 不能被解释成所有 Agent 行为禁用；实际个人/地点限制另核其已批准 policy | 原案有适用[救济入口](../../world-runtime/runtime/protective-governance-authority-and-recovery.design.md#protective-review-expiry)才展示本人当前可用 query/appeal。申诉/到期仅解除本件原范围，其他有效限制继续；没有原适用救济时提供准确阻断/安全停止，不新建通用 appeal |
| 制造控制来源终结 / 独立经济处置 | 制造 release/reclaim 沿 [DES-AGENT-MFG-004](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-004) 撤旧来源并推进 identity/acquisition generation；[独立处置](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-effective-owner-generation) 只在其新 family、双方完整接受和 Final 合同内成立 | 初次注册、Release+Claim、制造 WIP Scrap 与 Agent 转让/报废互不替代。已完成交付、原配置/责任历史保留，新 owner/source 不自动继承旧 grant、待决意图或私有上下文 |

当前 claim 结束不删除其真实关联业务。[原合同责任](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-recovery)继续以原 principal、source、对象和义务键承担；已付、已交付、未履行、费用 hold、争议中责任和未知结果分别保留，不能把已完成工作重记为收入或把未结项转嫁新 claimer。仅当原领域实际证明解除/结清，才可按其狭义 authority 释放相应 hold；这不为旧普通 Release 增加“先关闭全世界所有合同”的新门槛，也不把新处置独有的债务接收/退款政策追溯应用到旧路径。

结果与义务读取沿目标/原请求/原义务的准确索引与原分页、权限和预算，消费原 canonical refs；不新增全世界债务扫描或第二份债务账。索引缺失、私有部分不可读、来源冲突时分别表达当前可见范围与 unknown，不据此清除责任。世界历史的可审计性不自动授予新主体旧 owner 的完整私有读权。

<a id="claim-current-recovery"></a>
### 6.9 当前重认领、原件恢复与安全停止

Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf, O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287；分别承接 [REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001) 的未知确认与 [AC-WR-AOS-002](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-002) 的当前恢复。恢复先识别原操作及真实来源，不以“再次 Claim”覆盖所有失败。

- 尚存 claim 的支付/身份恢复只沿其当前原合同；制造 suspended 来源可按原专用 Refresh 接收合法当前根，不能复活旧 grant。终态后新 claim 是新准入，必须重新验证当时资格、取得工业来源/容量、资源、原 restricted 用途与反滥用条件；旧收据、曾有主绑定或短暂退出都不足以授予新效果。
- 首次支持发放记录、绑定、制造 order/lot、claim acquisition sequence、原接受/业务 nonce 和历史结果保持各自原键。旧请求重复不发第二份支持、不创建第二个 Agent、不重复退 bond；真正新的授权发放仅按原发放条件处理，不能从“重新选择”推导一次补贴，也不能把合法历史 liquid 重标为不可转让资产。
- 断线/重启/丢 ACK 先走[原 GameplayAction 结果回读](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#gameplay-original-result-lookup)，核原签名、当前读权、world/branch、目标、身份 fence 和原 nonce；query-only 不再次 submit 或消费原业务 nonce，仍保留原读取鉴权、限频和有限协议成本。当前余额或 owned 列表不能代替原请求结果。入口未接查询/历史不可得时保留原件与 unknown，明确恢复原权限/连接/受支持 reader 或安全停止；不换 nonce 猜测重试。
- 原 committed/rejected/expired 结果与当前控制状态分别显示；replay 可按原 delta 重建应有 claim/余额/序号，不发起第二次实时业务或外部副作用。丢 ACK 不等于 World 故障。[G1](../../world-runtime/prd.md#runtime-generation-persistence-contract) 真正 StatusUnknown 时保留完整 staged/base generation，隔离整个旧 World 的写入、tick、save 与依赖 dispatch，先查原 generation；NotCommitted 保留该次原前态，Committed 只发布其既有结果。迟到读面先追平再依赖，不能回滚已提交经济效果。
- Viewer、pure API、获准 Agent 对同一对象、原结果和当前 cut 的原因/资金/义务/下一步保持同义。换账号、世界、主绑定、当前读权或来源 revision 后撤下旧可执行建议；不得跨主体展示旧私有内容。静态示意图、旧字符串、本地 fixture 和 UI 测试各保留其证明范围，不因本设计/方法补齐而宣称所有 UI 资产、正式 producer 或实玩已交付。

方法由[候选与承诺](../../testing/manual/agent-claim-and-stewardship-acceptance.manual.md#claim-01-choice)、[确认重验](../../testing/manual/agent-claim-and-stewardship-acceptance.manual.md#claim-02-confirmation)、[失去控制](../../testing/manual/agent-claim-and-stewardship-acceptance.manual.md#claim-03-loss)、[当前恢复](../../testing/manual/agent-claim-and-stewardship-acceptance.manual.md#claim-04-recovery)分别给同候选独立前后态与正反例；本次是设计/静态审读，不产生运行或发行结论。

## 7. 设计边界
- 这不是现实货币付费功能，也不是永久产权出售。
- 这不是 agent 市场或 NFT 化系统。
- v1 先冻结规则结构和默认边界，本轮不敲定最终绝对价格。
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
  - 该历史窗口记录 runtime / viewer / QA 三条闭环通过，当时更需要真实持有行为样本，而非另一套默认值。
  - 当前成本曲线仍满足本专题最重要的结构边界：首个 claim 非免费、额外槽位单调更贵、欠费与闲置都能回收、refund/slash 可审计。
  - 在取得真实 claim 分布、平均持有时长、grace 命中率、forced reclaim 占比之前，提前改参数只会削弱对当前默认值的验证。
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
| Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；[REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001) | [候选承诺包](#claim-choice-commitment) | 真实逐候选用途/差异、非零承诺、runway 与替代选择 | [CLAIM-01](../../testing/manual/agent-claim-and-stewardship-acceptance.manual.md#claim-01-choice)：同 source/profile/cut 的单候选、多候选、能力缺项及375/374独立算例；required producer 与 full 真实适用入口分层 | 原来源输入、字段分段、金额/时期 oracle 与真实读面 | 旧多候选空理由、静态 UI/素材或模块 ID 不代签完整正式承诺 |
| Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；[AC-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-001) | [确认重验](#claim-confirmation-revalidation) | 只读零业务效果、同目标条款当前重验、竞争和漂移原子拒绝 | [CLAIM-02](../../testing/manual/agent-claim-and-stewardship-acceptance.manual.md#claim-02-confirmation)：正式适用 claim producer，固定前态逐一改当前资格/资金/容量/目标/来源并竞争 | 前后 claim/余额/bond/支持/容量、原结果及拒绝差分 | 不绕工业取得守卫，不把 received 或 lookup 当取得 |
| Trace refs: O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287；[REQ-WR-AOS-002](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-002) | [失去控制与连续性](#claim-loss-continuity) | 原生命周期原因、端点、退款 provenance 与旧责任/适用限制 | [CLAIM-03](../../testing/manual/agent-claim-and-stewardship-acceptance.manual.md#claim-03-loss)：旧 claim 的 release/grace/idle 与真实受限来源、原领域限制对照；required/full 各记适用范围 | 原事件、时期、费用/退款、source generation 与未结义务 | 不新增通用处罚/救济，不将新处置政策套旧 Release |
| Trace refs: O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287；[AC-WR-AOS-002](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-002) | [当前恢复](#claim-current-recovery) | 新准入、旧结果保留、当前权限、重启/replay 与真实安全下一步 | [CLAIM-04](../../testing/manual/agent-claim-and-stewardship-acceptance.manual.md#claim-04-recovery)：同世界原件查询/重认领、来源失效与 snapshot+非空 suffix；区分丢 ACK/G1 | 原请求/结果、受众、恢复切点及一次效果 oracle | 不以缺历史证明未提交或复活旧 grant，不代签实际恢复运行 |
| Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf；[REQ-WR-AOS-001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001) | [当前恢复](#claim-current-recovery) | 未知确认不重复经济 claim、支持发放或原 nonce | [CLAIM-04](../../testing/manual/agent-claim-and-stewardship-acceptance.manual.md#claim-04-recovery)：同原件丢 ACK/查询、错主体及 reader 不可用的零业务重投对照 | 查询与提交计数、原 nonce 和当前权威结果 | 只读认证/限额仍适用；不把协议存在当每入口已接线 |
| [REQ-AGC-CHAT-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-001) | [DES-CLAIM-CHAT-002](#des-claim-chat-002) | OR gate、余额耗尽后记录保留，以及资源字段独立披露。 | [首聊 snapshot 回归](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/snapshot_first_chat_unlock_preview.rs) 的 `compat_snapshot_projects_first_chat_unlock_preview_only_for_zero_liquid_without_starter_oc`；同一 source HEAD 的本地 required Rust 层。通过条件：零余额且无记录才有预览，两个文案字段均说明 OR 和余额用尽；这些组合区分实际 gate 与旧文案。 | PR 记录命令、HEAD、本地输入来源与实际测试 verdict。 | 不证明 service 余额投影与真实 gate 正确、provider 可用、重复领取资格或已完成真实对话。 |
| [REQ-AGC-CHAT-002](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-002) | [DES-CLAIM-CHAT-003](#des-claim-chat-003) | 实际注册与经济认领的义务区分。 | [首聊 snapshot 回归](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/snapshot_first_chat_unlock_preview.rs) 的 `first_chat_unlock_preview_distinguishes_registration_from_economic_claims`；同一 source HEAD 的 action → snapshot 层。通过条件：真实 RegisterAgent 无经济 claim，真实 ClaimAgent 有 owner/upkeep，其他 Agent 的 claim 不污染文案；同时检查世界记录和说明，避免仅有字符串自证。 | PR 记录世界状态断言与实际测试 verdict。 | 不改变经济状态机或证明所有历史未决义务已结清。 |
| [REQ-AGC-CHAT-003](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-003) | [DES-CLAIM-CHAT-004](#des-claim-chat-004) | 两种语言、旧版与缺失义务说明、预览更新保持相同含义。 | [首聊 Viewer 测试](../../../crates/oasis7_viewer/software_safe_src/first_chat_unlock_preview.test.jsx)；同一 source HEAD 的组件层并构建生成产物。通过条件：EN/ZH 对应、旧版和缺失说明为 unknown、无翻译保留来源、输入更新替换旧说明；覆盖精确字符串映射最易遗漏的兼容分支。 | PR 记录组件测试 verdict、构建结果与证据范围。 | 不证明新鲜度检测、全场景可玩性、真实留存或发行就绪。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-AGC-ENTRY-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-entry-001) | [DES-CLAIM-ENTRY-001](#des-claim-entry-001) | 三情境注册/恢复、第二玩家、隔离与幂等。 | 11.3 后继验证矩阵的 ENTRY 场景；以现有 [auth_actions_claims.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/auth_actions_claims.rs) 空世界样例为基线，后继实现增加 canonical service 两账号与原请求恢复测试，再做同候选入口验证。 | 实现 PR 的固定 source/build、世界身份、原请求/结果及实际测试或试玩记录。 | 当前基线不证明逐账号注册、正式 owner bootstrap 或同世界多玩家验收；本文未运行这些场景。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[AC-AGC-ENTRY-001](gameplay-agent-claim-economy-contract.prd.md#ac-agc-entry-001) | [DES-CLAIM-ENTRY-001](#des-claim-entry-001) | 三情境注册/恢复、第二玩家、隔离与幂等。 | 11.3 后继验证矩阵的 ENTRY 场景；以现有 [auth_actions_claims.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/auth_actions_claims.rs) 空世界样例为基线，后继实现增加 canonical service 两账号与原请求恢复测试，再做同候选入口验证。 | 实现 PR 的固定 source/build、世界身份、原请求/结果及实际测试或试玩记录。 | 当前基线不证明逐账号注册、正式 owner bootstrap 或同世界多玩家验收；本文未运行这些场景。 |
| [REQ-AGC-CHAT-001](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-001) | [DES-CLAIM-CHAT-005](#des-claim-chat-005) | 真实 service 三值事实、来源/隐私、失效/最终重验及三入口同义。 | 11.3 后继验证矩阵的 GATE 场景；以现有 [WorldService HTTP owner 隔离回归](../../../crates/oasis7_chain_runtime/src/host/execution_bridge/tests/world_service_agent_chat.rs) 的 `canonical_owner_goal_http_view_excludes_public_and_registered_delegate` 为基线，后继实现按真实 `WorldServiceProjection` producer→已认证 read→consumer→submit 增加资格、失效与集成回归。 | 实现 PR 的版本、主体/世界/commit 绑定、原始受限投影与各入口结果。 | 已交付本地 snapshot/组件及投影隔离测试不覆盖此后继资格协议；新增测试需在实现时落地，不声明现有自动化已通过。 |
| [REQ-AGC-CHAT-003](gameplay-agent-claim-economy-contract.prd.md#req-agc-chat-003) | [DES-CLAIM-CHAT-005](#des-claim-chat-005) | 真实 service 三值事实、来源/隐私、失效/最终重验及三入口同义。 | 11.3 后继验证矩阵的 GATE 场景；以现有 [WorldService HTTP owner 隔离回归](../../../crates/oasis7_chain_runtime/src/host/execution_bridge/tests/world_service_agent_chat.rs) 的 `canonical_owner_goal_http_view_excludes_public_and_registered_delegate` 为基线，后继实现按真实 `WorldServiceProjection` producer→已认证 read→consumer→submit 增加资格、失效与集成回归。 | 实现 PR 的版本、主体/世界/commit 绑定、原始受限投影与各入口结果。 | 已交付本地 snapshot/组件及投影隔离测试不覆盖此后继资格协议；新增测试需在实现时落地，不声明现有自动化已通过。 |

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
