# oasis7 主链 Token 创世分配与早期贡献奖励口径

- 对应设计文档: `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 3
## 1. Executive Summary
- Problem Statement: oasis7 已具备主链 Token 创世分配、锁仓领取与 treasury 分发能力，但此前尚未冻结“创世怎么分、谁控制多少、绝对总量是多少、何时释放、早期玩家是否发币”的统一口径。若继续口头决策，容易在 limited playable technical preview 阶段过早流通、单人过度控盘，或误滑向 `play-to-earn`。
- Proposed Solution: 冻结一版 producer-owned 初始分配与早期贡献奖励 PRD，明确 `10000 bps` 创世分配表、当前 `main_token_config.initial_supply = 10,000,000,000 OC`、项目战略控制比例、创始人个人直持上限、低流通门禁，以及 limited preview 阶段仅凭已完成最终 actual-value review、确认实际发放金额的 canonical README contributor reward ledger 批准记录执行 reserve 发放；贡献资格、证据分类和 review taxonomy 由 [reward pack PRD](../../readme/governance/readme-limited-preview-contributor-reward-pack.prd.md)、[reward ledger PRD](../../readme/governance/readme-limited-preview-contributor-reward-ledger.prd.md) 及其执行文档定义。本 PRD 保留反空投 / P2E 护栏，并映射到现有 runtime 创世/金库能力。
- Success Criteria:
  - SC-1: 创世分配表明确写出 `10000 bps` 总量分配、bucket、控制主体、锁仓方式与释放路径，且当前 `main_token_config.initial_supply` 固定为 `10,000,000,000 OC`。
  - SC-2: 项目战略控制口径固定为 `5000 bps`，其中单人直接受益控制目标 `500~1000 bps`、硬上限 `1500 bps`。
  - SC-3: 协议奖励池口径固定为 `3500 bps`，且不得被计入创始人或团队可自由处置库存。
  - SC-4: 创世液态流通硬上限 `500 bps`；按 `10,000,000,000 OC` 口径，首 12 个月非团队外部释放目标为 `100,000,000~200,000,000 OC`、硬上限 `500,000,000 OC`。
  - SC-5: 早期奖励只允许按可审计贡献发放，不允许 `play-to-earn`、`login reward`、`time played = token` 或对外宣传“来玩就有币”。
  - SC-6: `TIGR-5` 必须把创世参数草案收成正式执行清单，固定每个 bucket 的 slot id、控制主体、签名规则、runtime 落点、amount rounding 规则与 pre-mint freeze gate。
  - SC-7: 当前链上代币的正式产品命名、runtime `main_token.symbol` / ticker 与公钥派生账户前缀已统一固定为“绿洲币 / Oasis Coin” / `OC` / `oc:pk:`。
  - SC-8: 7 个 bucket 在 `10,000,000,000 OC` 口径下的绝对分配额、首年释放绝对边界与 freeze gate 已回写到正式文档，且不改变当前 `genesis_liquid=0` 与低流通治理约束。

<a name="2-user-experience-functionality"></a>

## 2. User Experience & Functionality
- User Personas:
  - `producer_system_designer`：需要在主网上线前冻结长期经济结构与控盘边界。
  - 金库/治理维护者：需要把创世分配映射到具体 bucket、账号、多签与 vesting 参数。
  - `runtime_engineer`：需要把创世分配从逻辑草案推进到可执行的 pre-mint freeze sheet，而不是临场拼接参数。
  - `liveops_community`：需要为 limited preview 早期贡献奖励制定对外可执行、不过度承诺的标准。
  - `qa_engineer`：需要验证创世配置、低流通与奖励语义没有越界。
- User Scenarios & Frequency:
  - 创世发行前：冻结 Token 初始分配口径时使用，一次为主，后续仅在重大治理修改时重审。
  - limited preview 期间：每轮准备发放早期贡献奖励时使用。
  - 阶段升级前：每次需要复核 circulating / founder control / reward framing 时使用。
- User Stories:
  - PRD-P2P-TOKEN-INIT-001: As a `producer_system_designer`, I want one frozen genesis allocation table, so that project control, protocol reward and personal holding boundaries are auditable before mint.
  - PRD-P2P-TOKEN-INIT-002: As a treasury operator, I want each bucket to have an explicit controller, vesting rule and release path, so that no one confuses treasury custody with personal inventory.
  - PRD-P2P-TOKEN-INIT-003: As a `liveops_community` owner, I want reserve issuance to consume an approved canonical reward ledger record, so that each distribution stays auditable and cannot become a marketing airdrop or play-to-earn loop.
- Critical User Flows:
  1. Flow-TOKEN-INIT-001: `制作人冻结分配表 -> runtime/治理维护者映射 bucket_id/recipient/vesting -> QA 审核控盘与流通边界 -> 创世配置进入候选`
  2. Flow-TOKEN-INIT-002: `canonical README reward pack/ledger 按其规则完成贡献资格、证据与初始 producer review -> producer 在 distribution 前完成 actual-value review 并于 ledger 确认或下调最终 actual amount -> reward multisig 仅按该已复核金额从 early_contributor_reward_reserve 发放 -> ledger 回填 distribution ref / date`
  3. Flow-TOKEN-INIT-003: `团队或基金会到达解锁窗口 -> 按 vesting 领取 -> QA 复核 circulating 与单人持仓上限 -> 若越界则阻断后续释放`
  4. Flow-TOKEN-INIT-004: `外部提交早期奖励候选 -> 按 canonical README reward pack/ledger 的资格和 review taxonomy 处理 -> 若只是登录/注册/在线时长/泛化引流且未形成满足要求的 approved record，则不得从 reserve 发放`
- 创世参数表（TIGR-1 草案，假设 `genesis_epoch=0`，并以 `1 epoch ~= 1 day` 作为当前锁仓换算口径；若最终链上 epoch 节奏不同，需在创世冻结前按同等自然时间重算）：
| bucket_id | ratio_bps | recipient | start_epoch | cliff_epochs | linear_unlock_epochs | genesis_liquid | claim_policy | 说明 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `team_long_term_vesting` | `2000` | `protocol:team-core-vesting` | `0` | `365` | `1095` | `0` | cliff 结束后按季度 claim；未经团队多签批准不得提前 claim | 核心团队长期锁仓，目标 `12m cliff + 36m linear` |
| `early_contributor_reward_reserve` | `1500` | `protocol:early-contributor-reward` | `0` | `0` | `3650` | `0` | 仅在贡献审批批次通过后按预算 claim，不得为拉新一次性 claim | 用于 limited preview 早期贡献奖励，10 年线性释放上限约束首年可领取容量 |
| `node_service_genesis_custody` | `2000` | `protocol:node-service-custody` | `0` | `180` | `1825` | `0` | 仅在后续协议决议确认需要补充节点激励时按批次 claim | 注意这是创世 custody 账户，不是 runtime `node_service_reward_pool` treasury bucket |
| `staking_genesis_custody` | `1500` | `protocol:staking-custody` | `0` | `180` | `1825` | `0` | 仅在后续协议决议确认需要补充 staking 激励时按批次 claim | 注意这是创世 custody 账户，不是 runtime `staking_reward_pool` treasury bucket |
| `ecosystem_governance_reserve` | `1500` | `protocol:ecosystem-governance` | `0` | `90` | `1460` | `0` | 仅在 grant / ecosystem plan 获批后按季度 claim | 生态与 grant 储备，避免创世即形成大额液态筹码 |
| `security_reserve_emergency` | `1000` | `protocol:security-council-reserve` | `0` | `0` | `0` | `0` | 默认不 claim；只有安全事故、补偿或防御动作才 claim | 为应急保留即时可用能力 |
| `foundation_ops_reserve` | `500` | `protocol:foundation-ops` | `0` | `90` | `730` | `0` | 仅在基础设施/合规/运营预算批准后月度或季度 claim | 限制早期运营盘一次性流出 |
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 核心团队长期锁仓 | `bucket_id=team_long_term_vesting`、`allocation_bps=2000`、`recipient=team multisig / vesting beneficiaries`、`cliff=12m`、`linear=36m` | 创世写入锁仓；到期后按 vesting 释放 | `frozen -> cliff -> linear_unlock -> claimable` | 占总量 `20%`；不得在创世即液态 | 仅团队多签与受益人可按 vesting 领取 |
| 早期贡献奖励储备 | `bucket_id=early_contributor_reward_reserve`、`allocation_bps=1500`、`recipient=governed reward reserve multisig` | limited preview 期间保持独立 reward multisig 执行；只接受 canonical README reward ledger 中明确批准、完成最终 actual-value review 且已确认实际金额的可追溯记录作为发放依据；不得按登录/时长自动发放 | `frozen -> approved_record_received -> distributed` | 占总量 `15%`；发放需附带 canonical contribution ledger record；当前不并入 `ecosystem_pool` | reward eligibility、review taxonomy 与 ledger 状态按 [reward pack](../../readme/governance/readme-limited-preview-contributor-reward-pack.prd.md) / [reward ledger](../../readme/governance/readme-limited-preview-contributor-reward-ledger.prd.md) 执行；reserve 多签仅执行经 actual-value review 确认金额的 approved record |
| 节点服务创世储备 | `bucket_id=node_service_genesis_custody`、`allocation_bps=2000` | 作为协议长期储备，不视为团队库存 | `frozen -> protocol_distributable` | 占总量 `20%`；后续是否转入/补充 treasury 需单独决议 | 只能按协议/治理批准后的 custody claim 路径使用 |
| 质押创世储备 | `bucket_id=staking_genesis_custody`、`allocation_bps=1500` | 作为协议长期储备，不视为团队库存 | `frozen -> protocol_distributable` | 占总量 `15%`；后续是否转入/补充 treasury 需单独决议 | 只能按协议/治理批准后的 custody claim 路径使用 |
| 生态治理储备 | `bucket_id=ecosystem_governance_reserve`、`allocation_bps=1500` | 用于 grant、生态激励或未来治理计划；后续如需导入 `ecosystem_pool`，必须单独决议；不等于早期玩家普发 | `frozen -> governance_distributable` | 占总量 `15%`；需治理记录 | 只能通过治理绑定分发 |
| 安全储备 | `bucket_id=security_reserve_emergency`、`allocation_bps=1000` | 仅用于安全事故、应急补偿或协议防御 | `frozen -> emergency_only` | 占总量 `10%`；常态不可外发 | 仅受限治理或安全委员会可动用 |
| 基金会/运营储备 | `bucket_id=foundation_ops_reserve`、`allocation_bps=500`、`recipient=ops multisig` | 用于基础运营与合规/基础设施费用 | `frozen -> vested_ops` | 占总量 `5%`；建议同步锁仓 | 仅运营多签可按规则动用 |
| 单人直持边界 | `founder_direct_target_bps=500~1000`、`founder_direct_cap_bps=1500` | 创世前检查任一自然人直接受益份额是否超限 | `candidate -> approved/rejected` | 超过 `15%` 直接拒绝；目标区间 `5%~10%` | producer 与 QA 共同审计 |
| 正式命名、symbol 与账户前缀 | `display_name_cn=绿洲币`、`display_name_en=Oasis Coin`、`symbol=OC`、`account_prefix=oc:pk:` | 冻结 public naming，并统一 runtime/account 当前真值与文档/运营口径 | `unnamed -> named -> migrated` | 正式产品名固定，当前 runtime symbol 为 `OC`，公钥派生账户前缀为 `oc:pk:`；若未来再次调整，必须另开专题评估 API/UI/兼容性影响 | `producer_system_designer` 定义口径；runtime/viewer/liveops 只按边界消费 |
- Acceptance Criteria:
  - AC-1: 创世分配表固定为以下比例，且总和必须为 `10000 bps`；当前 `main_token_config.initial_supply` 固定为 `10,000,000,000 OC`：
    - 核心团队长期锁仓 `2000 bps`
    - 早期贡献奖励储备 `1500 bps`
    - 节点服务奖励池 `2000 bps`
    - 质押奖励池 `1500 bps`
    - 生态金库 `1500 bps`
    - 安全储备 `1000 bps`
    - 基金会/运营储备 `500 bps`
  - AC-2: 项目战略控制口径固定为 `5000 bps`，由 `team_long_term_vesting + early_contributor_reward_reserve + security_reserve_emergency + foundation_ops_reserve` 组成。
  - AC-3: 协议长期储备口径固定为 `3500 bps`，由 `node_service_genesis_custody + staking_genesis_custody` 组成，且不得对外表述为创始人/团队自由库存。
  - AC-4: 单个自然人的直接受益持仓目标为 `500~1000 bps`，硬上限 `1500 bps`；超过上限的部分必须转入团队锁仓、多签金库或协议池。
  - AC-5: 创世液态流通不得超过总量 `500 bps`；按 `10,000,000,000 OC` 口径，创世液态流通理论硬上限为 `500,000,000 OC`，但当前 `TIGR-1` 参数表全部 bucket 仍必须保持 `genesis_liquid=0`；首 12 个月非团队外部释放目标为总量 `100~200 bps`（即 `100,000,000~200,000,000 OC`），硬上限 `500 bps`（即 `500,000,000 OC`）。
  - AC-6: 早期奖励的候选资格、可计分贡献类型、证据准入、评分 / 建议档位及 review taxonomy 以 canonical [reward pack PRD](../../readme/governance/readme-limited-preview-contributor-reward-pack.prd.md) 和 [reward ledger PRD](../../readme/governance/readme-limited-preview-contributor-reward-ledger.prd.md) 为准；reserve 发放仅允许使用 ledger 中明确 `approved`、已完成 distribution 前 actual-value review、已确认最终实际金额且带有审批与贡献证据引用的记录，禁止按登录、注册、在线时长或单纯“试玩”自动发放。
  - AC-7: 早期奖励不要求产品级 invite-only 机制；产品准入状态本身不构成 reward eligibility、批准或链上准入门槛。实际资格与审核流程按 canonical README reward pack / ledger 执行，reserve 发放仍须消费其中明确批准的记录。
  - AC-8: 分配表必须能映射到现有 runtime 能力：创世分配走 `InitializeMainTokenGenesis`，锁仓释放走 `ClaimMainTokenVesting`；`TIGR-1` 输出的 `protocol:*` recipient 当前表示 custody account，而不是直接初始化 `main_token_treasury_balances`。
  - AC-9: `TIGR-1` 必须产出 7 条创世 bucket 参数草案，明确 `recipient/start_epoch/cliff_epochs/linear_unlock_epochs/genesis_liquid/claim_policy`，并把 `node_service/staking/ecosystem/security` 的创世 custody 账户与 post-genesis treasury bucket 语义分开。
  - AC-10: `TIGR-4` 必须冻结当前执行路径为“`early_contributor_reward_reserve` 在 limited preview 期间保持 `protocol:early-contributor-reward` 多签治理执行，且只根据已完成 distribution 前 actual-value review、已确认最终金额的 canonical README reward ledger 批准记录发放；不并入 `ecosystem_pool`”；只有在真实奖励轮次、审计台账与治理成熟度都跑出来后，才允许另开专题重审是否合并。
  - AC-11: `TIGR-5` 必须输出正式执行清单，至少包含 `recipient_slot_id/controller_slot_id/signer_policy/runtime_target/allocated_amount_rule/freeze_status` 六类字段，并明确当前哪些 slot 仍待真实地址绑定。
  - AC-12: 创世金额换算必须固定为 runtime 真值：先按 `floor(initial_supply * ratio_bps / 10000)` 计算每个 bucket 的 `allocated_amount`，再按 `ratio_bps` 降序、`bucket_id` 升序分配 remainder；执行清单不得使用与 runtime 不一致的手工舍入规则。
  - AC-13: `TIGR-9` 必须把 `10,000,000,000 OC` 口径下的 7 个 bucket 绝对分配额、首年外部释放绝对边界，以及“当前 rounding remainder = 0”的执行说明同步回写到专题 PRD / design；任务追溯写入 GitHub 与 formal freeze sheet。
  - AC-14: token 相关活跃文档、运营口径、模块入口与 runtime/account 派生实现必须统一把当前链上代币称为“绿洲币 / Oasis Coin”，并以 `OC` / `oc:pk:` 作为当前 symbol/account 真值；`AWT` / `awt:pk:` 仅允许保留在历史语境或兼容说明中。
- Non-Goals:
  - 本专题不决定 `max_supply`、长期通胀终值、二级市场价格目标或 FDV 叙事；当前仅冻结创世 `initial_supply = 10,000,000,000 OC` 与对应 low-float 治理边界。
  - 不在本专题给出法律意见、证券属性判断、税务结论或上市计划。
  - 不在本专题启动公开空投、二级市场流动性、交易所上币或做市。
  - 不建设产品级 invite-only 准入系统，也不把“可玩技术预览”改写为 `closed beta`。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题不新增 AI 模型能力要求）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 使用现有 `main_token` runtime 的创世分配、锁仓领取、增发与治理绑定 treasury 分发能力承接该口径。创世时先把总量按 bucket 写入 recipient 账户的 `vested_balance`，而不是直接写入 `main_token_treasury_balances`；后续释放严格区分“团队/项目战略控制”“协议奖励池”“外部可流通”三层，不把 custody account 误当作 treasury bucket 或个人库存。limited preview contributor reward 的资格与 review taxonomy 由 canonical README reward pack / ledger 定义，P2P reserve 发行仅消费其中已批准、完成 distribution 前 actual-value review 且已确认最终金额的可追溯 reward record。
- Integration Points:
  - `doc/p2p/token/mainchain-token-allocation-mechanism.prd.md`
  - `doc/p2p/token/mainchain-token-allocation-mechanism.prd.md`
  - `doc/p2p/token/mainchain-token-genesis-freeze-sheet.md`
  - `doc/readme/governance/readme-limited-preview-contributor-reward-pack.prd.md` (contribution eligibility, evidence and review taxonomy authority)
  - `doc/readme/governance/readme-limited-preview-contributor-reward-ledger.prd.md` (approval and distribution record authority)
  - `doc/readme/governance/readme-limited-preview-contributor-reward-pack.md`
  - `doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md`
  - `doc/game/prd.md`
  - `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`
  - `crates/oasis7_runtime/src/main_token.rs`
  - `testing-manual.md`
- Execution Path Decision:
  - `TIGR-4` 当前选定 `early_contributor_reward_reserve -> protocol:early-contributor-reward -> reward multisig issues only the final actual-value-reviewed amount recorded in an approved canonical README reward ledger row` 作为 limited preview 执行路径；贡献资格、证据分类、review status、producer approval 与 distribution 前 actual-value review 语义以该 pack / ledger 为准。
  - 当前不把 early contributor reserve 合并进 `ecosystem_pool`，避免在 runtime 语义尚未直连 treasury bucket、且真实奖励轮次尚未跑完时，把“贡献奖励”与“生态 grant”混成同一个公开口径。
  - 若未来需要 fully on-chain、proposal-bound 的 contributor distribution，应在新的治理专题里同时回答“运行时映射”“审计透明度”“社区预期”三项问题后再迁移。
- Formal Freeze Sheet Decision:
  - `TIGR-5` 使用 slot-based execution sheet 把逻辑账户、控制主体和签名要求冻结为正式执行清单；在真实地址未绑定前，允许 `ready_pending_address_binding`，但不得宣称可直接 mint。
  - 当前创世绝对发行量已冻结为 `10,000,000,000 OC`；执行清单除固定 `allocated_amount` 算法与 slot registry 外，还必须提供 7 个 bucket 的绝对分配额与首年释放绝对边界。
- Edge Cases & Error Handling:
  - 若创世 bucket 比例和不为 `10000 bps`，则候选配置直接拒绝。
  - 若任一自然人直接受益份额超过 `1500 bps`，则创世配置直接退回。
  - 若把创世 recipient 误写成 treasury bucket 语义并假定 runtime 会自动记入 `main_token_treasury_balances`，则必须退回；当前实现只会记入 recipient account 的 `vested_balance`。
  - 若 execution sheet 的 `recipient_slot_id` 已冻结，但真实 `recipient_account_id` 未绑定，则最多只能给 `conditional_draft_only`，不得进入最终 mint 执行。
  - 若 execution sheet 使用与 runtime 不一致的 rounding 规则，则必须退回；不得靠人工补差额绕过 `allocated_sum == total_supply` 约束。
  - 若 reward ledger record 未明确批准、尚未完成 distribution 前 actual-value review、未确认最终实际金额，或无法回链到可审计贡献证据与审批引用，则不得从 reserve 发放。
  - 若奖励记录把 raw `public key` 直接当作领取名称或执行字段原文暴露在对外文案中，则必须退回；底层 `public_key` 仅保留在签名/账户绑定专题。
  - 若外部文案把奖励描述为 `play-to-earn`、`airdrop for playing` 或“来玩就有币”，则 `liveops_community` 必须退回改稿。
  - 产品没有 invite-only 功能不阻止适用 README reward governance 下的独立 reward review；不得将产品准入或贡献记录宣称为链上准入门槛，reserve 仍只接受明确批准的 canonical reward ledger record。
  - 若有人主张把 early contributor reserve 立即并入 `ecosystem_pool`，但拿不出真实贡献轮次数据、治理审批节奏与 runtime 映射方案，则该提案直接退回；当前 producer 决策是继续保持独立多签执行。
- Non-Functional Requirements:
  - NFR-TOKEN-INIT-1: 创世分配表字段完整率 `100%`，至少包含 `bucket_id/allocation_bps/recipient/controller/vesting/release_path`。
  - NFR-TOKEN-INIT-2: 创世配置审计时必须同时输出三类汇总：项目战略控制比例、协议奖励池比例、单人直接受益比例。
  - NFR-TOKEN-INIT-2A: 当前创世绝对发行量固定为 `10,000,000,000 OC`；若未来需要调整，必须另开专题并重跑 formal freeze sheet、QA checklist 与 module-level 追踪回写。
  - NFR-TOKEN-INIT-3: 创世液态流通硬上限为 `500 bps`；若超过即不允许进入发币执行。
  - NFR-TOKEN-INIT-3A: `TIGR-1` 参数表中的 `genesis_liquid` 必须全部为 `0`；任何 bucket 不得在创世时直接形成 liquid balance。
  - NFR-TOKEN-INIT-3B: 按 `10,000,000,000 OC` 口径，首 12 个月非团队外部释放目标必须落在 `100,000,000~200,000,000 OC`，硬上限 `500,000,000 OC`；若计划值缺失或超限，创世准备直接阻断。
  - NFR-TOKEN-INIT-4: 早期奖励外部文案中，`play-to-earn`、`login reward`、`time played = token` 命中次数必须为 `0`。
  - NFR-TOKEN-INIT-5: 任何早期奖励发放记录都必须回链到 canonical README reward ledger record，并可追溯到贡献证据、`Approval ID`、实际发放数量、`Distribution Ref` 和发放日期。
  - NFR-TOKEN-INIT-6: 若后续需要修改上述比例或控盘上限，必须新开专题 PRD，不允许只在聊天、海报或运营帖中变更口径。
  - NFR-TOKEN-INIT-7: limited preview 阶段任何 early contributor reward 执行都必须由 canonical README reward ledger 中已完成 distribution 前 actual-value review 且已确认最终金额的明确批准记录授权，并走独立 reward reserve 多签执行链；`ecosystem_pool` 的治理 grant 流程不得被拿来替代或掩盖贡献奖励发放。
  - NFR-TOKEN-INIT-8: 正式产品名、symbol/ticker、账户前缀与 runtime 字段语义必须保持单一当前真值；现行口径固定为“绿洲币 / Oasis Coin” / `OC` / `oc:pk:`，不得再把旧值写成当前实现。
  - NFR-TOKEN-INIT-8A: 正式产品名、symbol/ticker 与 runtime 字段语义必须分层表达；未经过专题评审，不得把“命名冻结”外推成“ticker 已改”“symbol 已迁移”或“客户端/API 已自动切换”。
- Security & Privacy: 创世分配配置、控制账户与奖励记录必须可审计；安全储备不得与运营或个人钱包混用；涉及个人身份映射时只记录必要的链上账户与贡献证据，不在文档中暴露敏感个人信息；raw `public_key` 仅保留在签名/账户绑定流程中。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP: 冻结比例、控盘边界、低流通门禁、approved-record reserve issuance 与反空投 / P2E 护栏；贡献资格和 review taxonomy 按 canonical README reward governance 执行。
  - v1.1: 输出具体创世 bucket/account/vesting 参数表与 QA 审计清单。
  - v2.0: 至少跑出 1~2 轮真实贡献奖励台账并完成治理复盘后，再决定是否合并进 `ecosystem_pool` 或扩展新的治理型分发路径。
- Technical Risks:
  - 风险-1: 若把项目多签控制与个人直持混为一谈，会高估创始人个人库存并放大外部质疑。
  - 风险-2: 若 early contributor reserve 长期停留在链下多签操作，治理透明度会弱于 treasury-bound 路径；因此需要用真实台账、审批记录与后续专题评审补足可审计性。
  - 风险-3: 若 limited preview 为了拉新而放宽奖励口径，极易从“贡献制”滑向“时长挖矿”。
  - 风险-4: 若创世初期释放过快，会直接冲掉当前 `limited playable technical preview` 阶段应保持的低流通与低承诺策略。
  - 风险-5: 若过早把 early contributor reserve 并入 `ecosystem_pool`，会把“贡献奖励审批”与“生态 grant 治理”混成同一口径，放大对外理解偏差。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-P2P-TOKEN-INIT-001 | TIGR-0/TIGR-1/TIGR-5/TIGR-9 | `test_tier_required` | 分配表、bucket/account/vesting 参数表、正式执行清单、`10,000,000,000 OC` 口径下的绝对分配额、比例求和、单人直持上限与 `genesis_liquid=0` 审计 | 创世配置与控盘边界 |
| PRD-P2P-TOKEN-INIT-002 | TIGR-1/TIGR-2/TIGR-4/TIGR-5/TIGR-9 | `test_tier_required` | runtime 映射检查、金库/多签控制路径检查、slot registry、`initial_supply` 冻结值与流通上限门禁 | 创世落地路径与 treasury 执行 |
| PRD-P2P-TOKEN-INIT-003 | TIGR-2/TIGR-3/TIGR-4 | `test_tier_required` | canonical reward ledger 的 approved record、distribution 前 actual-value review 与最终金额核验；reserve 多签按确认金额发行；运营文案禁语检查 | limited preview 奖励发放与外部口径 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-TOKEN-INIT-001 | 先冻结比例和控制边界，再在后续明确绝对发行量与执行节奏 | 先发币再补规则 | 当前最需要先锁定的是边界，而不是营销节奏。 |
| DEC-TOKEN-INIT-002 | 项目战略控制目标设为 `5000 bps`，单人直持目标 `500~1000 bps`、硬上限 `1500 bps` | 创始人直接持有大比例流通筹码 | 降低个人过度控盘观感，保留项目推进所需控制力。 |
| DEC-TOKEN-INIT-003 | 早期奖励采用 contribution-based reward，并只凭完成 distribution 前 actual-value review、确认最终金额的 canonical README reward ledger 批准记录执行 reserve 发放 | 开放式 play-to-earn / login reward / time-play mining；在本 P2P PRD 重复定义贡献资格与 review taxonomy | 当前阶段仍是技术预览，不能把代币激励建立在泛流量和挂机行为上；具体资格、审核分类与最终金额复核由 reward pack / ledger 权威维护。 |
| DEC-TOKEN-INIT-004 | 协议奖励池与项目战略控制分开记账和对外表述 | 将 treasury custody 与团队库存混用 | 避免治理资产与个人/团队资产混淆。 |
| DEC-TOKEN-INIT-005 | limited preview 期间保持 `early_contributor_reward_reserve` 独立多签治理执行，不并入 `ecosystem_pool` | 现在就把贡献奖励储备并入 `ecosystem_pool` | 当前 runtime 创世语义仍以 custody account 为主，且真实奖励轮次与治理成熟度尚不足以支撑立即合并。 |
| DEC-TOKEN-INIT-006 | 用 slot-based 正式执行清单冻结创世参数，并把真实地址绑定留到 mint 前最后一步 | 继续只保留逻辑草案，等执行当天再临场补账号与舍入 | 创世参数一旦进入执行，需要预先冻结 slot、签名要求、runtime 落点和 rounding 规则，减少临场错误面。 |
| DEC-TOKEN-INIT-007 | 先冻结当前链上代币正式产品名为“绿洲币 / Oasis Coin”，再单开专题迁移 runtime symbol/account 真值 | 在未评审兼容性影响前直接改 runtime symbol / 账户前缀 | 产品名、ticker 与账户派生的治理半径不同；先冻结 public naming，才能把后续 runtime 迁移收成独立可审计任务。 |
| DEC-TOKEN-INIT-008 | 在独立迁移专题中，把当前 runtime symbol、公钥派生账户前缀与签名鉴权前缀统一切到 `OC` / `oc:pk:` | 继续让 `AWT` / `awt:pk:` 作为现行真值 | public naming 已冻结后，继续双轨会让 runtime、API、viewer/client 与 liveops 口径长期分叉；应尽快完成单一当前真值迁移。 |
| DEC-TOKEN-INIT-009 | 当前创世绝对发行量冻结为 `10,000,000,000 OC` | 改为 `100,000,000 OC` 或 `1,000,000,000 OC` | 该值保留大整数公开叙事空间，同时仍由 `genesis_liquid=0`、首年外部释放 `100~200 bps` 目标与 `<=500 bps` 硬上限约束低流通边界；在当前 7 个 bucket 比例下还能得到无需 remainder 的整数分配额。 |


## 既有经济结果的产品接收

以下仅把本文件既有义务准确接收到世界规则产品根，不修改原参数、角色、权限、状态、历史或执行真值。每项只引用根唯一身份，原 SC/AC/DEC 与 §6 验收仍有效；本节不是实施/发放批准。

<a id="economic-contract-genesis-allocation"></a>
### 冻结分配可核算到各bucket与recipient的既有专业接受关系

Trace refs: O7-REQ-585609e7-ee93-4315-b9c7-fe58857164b4；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-genesis-allocation)。

- 原义务：SC-1/8、AC-1/8/9/12/13、DEC-TOKEN-INIT-009；按既有冻结总量和七桶参数核算分配及recipient vested_balance；采用原runtime rounding，创世custody account不等于treasury bucket。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[冻结分配可核算到各bucket与recipient设计](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-allocation)；不变更总量/比例/账户，不执行InitializeMainTokenGenesis。

<a id="economic-contract-genesis-control"></a>
### 控制主体与个人受益及释放边界分别可审计的既有专业接受关系

Trace refs: O7-REQ-2ee6964f-7c70-453c-958d-616b07c5d9d9；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-genesis-control)。

- 原义务：SC-2..4、AC-2..5、DEC-TOKEN-INIT-002/004；项目战略控制、协议储备、个人直接受益与外部流通分别核算；原vesting、claim、genesis_liquid与首年释放门槛继续独立约束。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[控制主体与个人受益及释放边界分别可审计设计](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-control)；不改控制人/vesting/释放额度，不把协议储备当团队库存。

<a id="economic-contract-genesis-freeze"></a>
### 逻辑冻结不等于真实mint-ready的既有专业接受关系

Trace refs: O7-REQ-6ca62d73-1934-4a80-bb18-1c21e35e2a15；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-genesis-freeze)。

- 原义务：SC-6、AC-11、DEC-TOKEN-INIT-006、正式执行清单；slot逻辑冻结与public signer规则不代替真实recipient/controller绑定、个人受益表和最终QA pass；pending或conditional状态不授予mint许可。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[逻辑冻结不等于真实mint-ready设计](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-freeze)；本轮不绑定地址/账户、不运行ceremony、不改变signer set或执行交易。

<a id="economic-contract-genesis-reward"></a>
### 独立reserve只消费最终复核的批准记录的既有专业接受关系

Trace refs: O7-REQ-e84d3e5c-af7c-4293-8d02-3b466af6dbfa；唯一声明：[世界规则根结果](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-genesis-reward)。

- 原义务：AC-6/10、DEC-TOKEN-INIT-003/005、Execution Path Decision；独立reward reserve只按canonical README ledger中approved且完成distribution前actual-value review的最终金额执行；资格/评分仍归pack/ledger，不并ecosystem_pool，不按登录或游玩自动发放。
- 原验证入口：[本文件验证与决策记录](#6-validation--decision-record)；具体业务场景仍按本文件第4章与原AC，不以§6文档迁移检查代签业务测试。
- 有限接收：[独立reserve只消费最终复核的批准记录设计](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-reward)；本轮无发放授权，不修改资格/金额/审批；模板不是已批准批次。
