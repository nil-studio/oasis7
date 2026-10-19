# Gameplay Agent 认领经济合同

- 对应设计文档: [`Gameplay Agent 认领经济合同设计`](gameplay-agent-claim-economy-contract.design.md)
- 产品承诺入口：[`Agent 所有权与持续经营`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)
- 可变执行状态: 对应 GitHub Project task 与 issue evidence comments

审计轮次: 8

<a id="public-claim-boundary"></a>
## Public claim boundary

本文件是 claim 经济规则的玩法专业权威，不是当前发行状态或公开可用性的证明。它消费玩家接入与发行模块的 [`REQ-ENTRY-CLAIM-003`](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003) 与 [`四层口径分离`](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#public-claim-four-layer-model)。

- 当前事实（current fact）：本文件只能确认已写入并经专业审查的 claim 规则、约束和失败边界；它不能单独确认某个版本、模式、入口或受控 grant 当前已经部署、开放或可供公众使用。
- 产品目标（product goal）：首个 claim 保持非零成本，受限 starter balance 只服务 `slot-1` 的窄用途，并保留 funding provenance、过期和回收边界。
- 假设（assumption）：受控 preview、allowlist 或 QA 窗口可能在当前权限、资金、runtime 和证据条件满足时使用 restricted grant；每个窗口都必须重新核验，不能从本文件的设计或历史执行记录推断当前可发放。
- 证据边界（evidence limit）：本文件和配套 design 证明规则意图与玩法约束，不证明运行时部署、真实账户资格、资金余额、QA 放行、public launch、可转账价值或一般公开 entitlement。
- 可发布公开 claim（releasable public claim）：只有在根 [`README.md`](../../../README.md)、产品 PRD、当前 runtime/QA 证据和对应 [`restricted grant LiveOps runbook`](gameplay-agent-claim-restricted-grant-liveops-runbook-2026-03-29.md#public-claim-boundary) 对同一窗口结论一致时，才可引用本文件支持一条受范围约束的公开事实；否则保持 draft/blocked，不把目标或假设写成已上线能力。

## 1. Executive Summary
- Problem Statement: 当前规则把 agent 认领完全绑定到 `liquid main token`。在“首个 claim 也不免费”生效后，limited preview / allowlist / QA seed 账号若没有可流通余额就无法进入中循环；但直接空投可转账 main token 又会打开刷号和套现路径。
- Proposed Solution: 保持“首个 claim 仍为非零 canonical 成本”的主规则不变，在 main token 账本中新增 `restricted starter claim balance`。该余额不可转账、不可提现、不可用于普通资产动作，只可用于 `slot-1` 的 claim / upkeep；当新账号首个 `slot-1` claim 没有现成 restricted grant/balance 且 dedicated pool 足够覆盖缺口时，runtime 应在 `ClaimAgent` 路径自动补足启动额度，而不是要求额外运营审批；由其资助的 bond refund 必须继续保留 provenance，不得洗成可流通余额。
- Success Criteria:
  - SC-1: 第 1 个 agent 认领仍不存在免费路径；所有成功认领都必须满足 `activation_fee_amount > 0`、`claim_bond_amount > 0`、`upkeep_per_epoch > 0`。
  - SC-2: `restricted starter claim balance` 仅允许用于 `slot-1` 的 upfront claim cost 与 `slot-1` upkeep；其通过 `TransferMainToken`、公开转账 API 或 explorer 导出的误放过率为 `0`。
  - SC-3: 任一 agent 在同一时刻只能有 1 个正式 `claim_owner_id`；并发争抢、重复认领和无成本续占的误放过率为 `0`。
  - SC-4: 单账号可同时认领的 agent 数在 v1 受 `reputation_tier` 限制为 `1/2/3` 三档，且 `slot-2/slot-3` 的总成本分别至少为 `slot-1` 的 `1.5x/2.0x`。
  - SC-5: 使用 restricted bucket 资助的 bond，在 release / reclaim / slash 后必须按 canonical provenance 退回 restricted bucket，不得洗成 `liquid main token`。
  - SC-6: `activation fee`、`upkeep`、`bond refund/slash` 与 restricted grant 的发放/消费/退款/过期必须全部形成可审计事件，并能进入现有 main token 源汇审计链路。
  - SC-7: `IssueRestrictedStarterClaimGrant / RevokeRestrictedStarterClaimGrant` 在进入 runtime grant 状态机前必须先通过正式 `admin registry` 门禁；当 registry 缺失、issuer 未登记或未绑定 signer allowlist policy 时，误放过率为 `0`。
  - SC-8: restricted grant admin registry 必须支持通过正式 runtime action 热更新；仅允许当前 `ecosystem_pool` treasury controller slot 绑定的 controller account 变更 `restricted_starter_claim_admin_account_ids` 子集，且变更 action 必须携带通过 signer allowlist / threshold policy 校验的主链签名 proof。
  - SC-9: `liveops_community` 的日常 restricted grant 发放/撤销/状态检查必须支持正式 CLI 入口，直接复用 runtime canonical action 与 world-state 真值，而不是要求运营手工拼接原始 action JSON 或直接编辑 world 文件。
  - SC-10: 在正式 CLI 之上，仓库还必须提供一层面向运营同事的短命令 wrapper，支持 `status / issue / revoke` 的位置参数和 `OASIS7_WORLD_DIR` 默认注入，同时不新增任何绕过 runtime / controller-governed admin registry 的旁路。
  - SC-11: governance registry 的 manifest/import/audit 工具链必须支持按 `slot_id` 声明独立 signer threshold；`liveops` 这类低权限 restricted grant admin slot 可以显式使用 `1-of-2`，而 treasury/controller 主槽位继续默认 `2-of-3`，且不得因为引入 `liveops` 特例而把其他 slot 一并降阈值。
  - SC-12: daily restricted grant 的出账源必须从 `ecosystem_pool` 切换到独立 `restricted_starter_claim_liveops_pool`；该专用池只能由当前 `ecosystem_pool` treasury controller slot 绑定的 controller account 通过正式 top-up action 从大池划拨资金，`liveops_community` 的日常 `issue/revoke/status` 只消费和回读该专用池，不直接动 `ecosystem_pool`。
  - SC-13: 当玩家处于首个 `slot-1` claim、当前没有 active restricted grant、账户也没有现成 restricted starter balance 时，只要 `restricted_starter_claim_liveops_pool` 能覆盖 upfront 缺口，runtime 就必须在 `ClaimAgent` 路径自动补足该缺口并继续认领，不再要求额外 `approval request -> operator review`；若 dedicated pool 仍不足，则只返回资金不足 blocker，不得生成 pending 审批状态。

## 2. User Experience & Functionality
- User Personas:
  - 中循环玩家/组织经营者：需要用明确代价换取 agent 控制权，而不是靠抢占或挂机囤位。
  - `producer_system_designer`: 需要把 agent 认领从“模糊权限”收成可平衡、可审计的经济规则。
  - `runtime_engineer`: 需要一套可确定执行的 claim / upkeep / reclaim 状态机与记账规则。
  - `viewer_engineer`: 需要在 UI / pure API 中向玩家清楚展示认领成本、受限余额、宽限、冷却和回收风险。
  - `qa_engineer`: 需要验证并发争抢、欠费、闲置、多号囤积、transfer guard 和经济审计没有旁路。
  - `liveops_community`: 需要向 limited preview / allowlist / QA seed 账号发放受限启动余额，并在需要时回收、停用或过期。
  - restricted grant admin operator: 需要一条正式 runtime 真值来判断 `issuer_id=liveops` 是否真的具备发放/撤销权限，而不是只依赖 runbook 约定。
  - restricted grant liveops operator: 需要低摩擦的日常操作入口来 issue/revoke/status restricted grant，而不是每次都理解底层 runtime action 或 controller payload 细节。
  - non-technical liveops operator: 需要更短、更稳定的脚本入口与环境变量约定，减少手工复制长命令时的误填与漏填。
- User Scenarios & Frequency:
  - 首次建立组织能力时：每个认真进入中循环的玩家至少 1 次。
  - limited preview / allowlist 发放时：每轮受控外放、QA 种子建号或运营定向补助时发生。
  - 扩展更多 agent 槽位时：随着玩家声誉提升，多次发生。
  - 日常持有期：每个 upkeep 结算 epoch 都会发生。
  - 释放 / 被回收时：主动退场、欠费或闲置时发生。
- User Stories:
  - PRD-GAME-011: As a 中循环玩家, I want every agent claim to keep a non-zero main-token-denominated cost and require ongoing upkeep, so that agent control reflects actual commitment instead of zero-cost squatting.
  - PRD-GAME-011A: As a `producer_system_designer`, I want the first claim to also be non-free, so that the world does not silently create a “starter free slot” that weakens the sink and encourages alt abuse.
  - PRD-GAME-011B: As a `qa_engineer`, I want forced reclaim and refund/slash outcomes to be deterministic, so that we can test abuse resistance instead of relying on manual moderation.
  - PRD-GAME-011C: As a limited preview / allowlist owner, I want the first `slot-1` claim to auto-draw dedicated-pool startup funds without granting transferable assets, so that test users can enter mid-loop without opening an airdrop abuse lane or waiting for manual approval.
  - PRD-GAME-011D: As a `runtime_engineer`, I want claim/refund accounting to preserve funding-source provenance, so that restricted starter funds cannot be converted into transferable main token through release or reclaim.
  - PRD-GAME-011E: As a new account entering PostOnboarding with `owned_claim_count=0`, I want a dedicated `slot-1` claim onboarding flow that reuses the canonical quote and still requires explicit confirmation, so that I can finish the first claim without hunting raw detail text or triggering a silent auto-claim.
  - PRD-GAME-011F: As a new account entering an already populated shared world, I want existing world Agents to be visible but clearly separated from my bound/owned Agent state, so that I do not mistake another world entity for my controllable Agent.
- Critical User Flows:
  1. Flow-AGC-001: `liveops / onboarding / QA seed 仍可显式发放 restricted starter claim balance；但默认首个 slot-1 claim 不再依赖运营审批，而是优先消费现有 restricted/liquid，并在 dedicated pool 足够时自动补足缺口`
  2. Flow-AGC-002: `玩家选择未认领 agent -> 系统返回 slot quote（activation fee / bond / upkeep / cap / eligible balances / auto starter funding）-> 若为 slot-1 则先评估现有 restricted 余额，再判断 dedicated pool 可自动补足多少缺口 -> 玩家确认 -> runtime 原子完成 auto-funding（若需要）、扣除 activation fee、锁定 bond -> agent 进入 claimed_active`
  3. Flow-AGC-002A: 当玩家处于 PostOnboarding、当前 `owned_claim_count=0` 且 canonical `next_claim_quote.slot_index=1` 时，Viewer 必须展示专用 `slot-1` onboarding CTA -> 玩家先选中未认领目标 -> HUD 回显 canonical quote / blocker -> 玩家显式点击 Prepare 后再 Confirm -> Viewer 才提交正式 claim action
  4. Flow-AGC-003: `每个 upkeep epoch 到达 -> 系统按 restricted -> liquid 优先级尝试结算 slot-1 upkeep -> 余额足够则继续持有 -> 不足则进入 upkeep_grace`
  5. Flow-AGC-004: `玩家在 cooldown 后主动 release -> 系统结清欠费 -> 按原 funding source 退还 bond 剩余部分 -> agent 回到 unclaimed`
  6. Flow-AGC-005: `claim 进入 grace 后仍未补足 upkeep 或连续闲置达到阈值 -> 系统执行 forced_reclaim -> 计算 slash / refund -> 按 funding source 回写退款 bucket -> agent 回到 unclaimed`
  7. Flow-AGC-006: `玩家尝试认领第 2/3 个 agent 或发起普通转账 -> 系统拒绝消费 restricted bucket，并返回结构化 blocker`
  8. Flow-AGC-007: `新 player_session 进入同一共享世界 -> 按 2.1 节区分该账号首次进入与原账号返回 -> 无绑定的新账号经合法显式注册/绑定取得自身第一个可控 Agent；原账号恢复已有绑定 -> 只有已有 canonical claimer 才进入经济 slot-1 ClaimAgent -> Viewer 默认控制列表只展示当前账号可控或绑定待同步 Agent，不以他人 Agent、世界列表顺序或清空世界补齐入口`
- Functional Specification Matrix:

| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| Restricted Starter Balance | `account_id`、`restricted_starter_claim_balance`、`issuance_reason`、`spend_scope`、`expires_at_epoch`、`issuer_id` | liveops / onboarding / QA seed 发放或回收受限余额 | `unissued -> issued -> partially_spent -> exhausted/expired/revoked` | `spend_scope` 固定为 `slot-1 claim + slot-1 upkeep`；默认不可与 `TransferMainToken` 共享 | 仅指定 issuer 可发放/回收；持有人不可转赠 |
| Claim Quote | `agent_id`、`claimer_id`、`claim_slot_index`、`reputation_tier`、`claim_cap`、`activation_fee_amount`、`claim_bond_amount`、`upkeep_per_epoch`、`transferable_liquid_balance`、`restricted_starter_claim_balance`、`auto_restricted_starter_claim_amount`、`eligible_claim_balance`、`release_cooldown_epochs`、`candidate_starting_location`、`candidate_specialty_summary`、`first_industrial_goal_help`、`candidate_risk_summary`、`candidate_recommendation_reason`、`upkeep_runway_epochs`、`next_upkeep_due_epoch`、`projected_grace_entry_epoch`、`low_runway_warning`、`slot_1_claim_choice_quote`、`candidate_agent_id`、`candidate_start_location`、`candidate_current_task`、`candidate_capability_summary`、`upfront_cost`、`eligible_balance_after`、`upkeep_epochs_after_claim`、`next_upkeep_epoch`、`grace_or_reclaim_risk`、`main_risk`、`alternative_candidate_hint`、`wait_or_fund_hint`、`recommended_claim_action`、`claim_choice_class` | 玩家查看未认领 agent 时返回成本报价、风险提示、候选差异理由与持有 runway；`slot-1` onboarding 不应只回答“付多少钱/为什么选这个 agent/确认后还能撑几轮 upkeep”，还要通过 `slot_1_claim_choice_quote` 让玩家在确认前比较当前候选、其他候选、等待或补资金三种路线 | `unclaimed -> quote_ready -> choice_ready` | `slot-1 multiplier=1.0`、`slot-2=1.5`、`slot-3=2.0`；`total_upfront_cost = activation_fee_amount + claim_bond_amount + upkeep_per_epoch`；`upkeep_runway_epochs` 按 eligible balance 扣除 upfront 后可覆盖的完整 upkeep epoch 粗算；候选差异条优先比较初始位置、擅长能力/当前任务、对首个工业目标的帮助、主要风险和推荐理由；`slot_1_claim_choice_quote` 必须 show / compare / classify / recommend，并把 `claim_choice_class` 固定为 `claim_now_route_fit / compare_candidates_first / wait_or_fund_first` | 仅当 agent 未被认领、玩家未超 cap，且 `eligible_claim_balance >= total_upfront_cost` 时可确认；`slot-2/3` 的 eligible balance 不得包含 restricted bucket；自动启动金额只允许出现在首个 `slot-1`；低 runway 只警告不静默拦截，除非余额已不足以通过 canonical claim；本合同不让首个 claim 免费、不重平衡 activation fee / bond / upkeep、不新增完整 agent class system 或候选排序 UI |
| Claim Activation | `claim_owner_id`、`claim_started_epoch`、`next_upkeep_epoch`、`claim_bond_locked_amount`、`claim_bond_locked_restricted_amount`、`claim_bond_locked_liquid_amount`、`activation_fee_burn_amount`、`activation_fee_treasury_amount`、`upfront_restricted_spent_amount`、`upfront_liquid_spent_amount`、`auto_issued_restricted_amount` | 点击确认认领后扣费并锁定 bond | `quote_ready/choice_ready -> claimed_active` | `activation_fee_split_bps = 5000 burn / 5000 treasury`；`slot-1` 优先花 restricted，再补 liquid；当 dedicated pool 自动补足时，auto-issued restricted amount 先进入同一 provenance 结算，再继续 claim；首个认领也必须扣费 | 同一 agent 同时只能成功 1 个 claim；并发失败方必须原子回滚 |
| Upkeep Settlement | `upkeep_due_epoch`、`upkeep_per_epoch`、`upkeep_paid_amount`、`upkeep_restricted_spent_amount`、`upkeep_liquid_spent_amount`、`grace_deadline_epoch`、`delinquent_amount` | 到达结算 epoch 时自动尝试扣除 upkeep | `claimed_active -> claimed_active` 或 `claimed_active -> upkeep_grace` | `slot-1` upkeep 先尝试 restricted，再尝试 liquid；`slot-2/3` upkeep 只能花 liquid；每次只结算 1 个 epoch 的应付额 | 仅系统结算；owner 可通过补足可用余额恢复 |
| Voluntary Release | `release_requested_epoch`、`release_effective_epoch`、`bond_refund_amount`、`bond_refund_restricted_amount`、`bond_refund_liquid_amount`、`cooldown_satisfied` | owner 主动放弃当前 claim | `claimed_active/upkeep_grace -> released -> unclaimed` | 退款金额 = `claim_bond_locked_amount - unpaid_upkeep - penalties`；restricted 来源的 bond 退款必须回 restricted bucket | 只有当前 owner 可 release；未过 cooldown 不允许 |
| Forced Reclaim | `forced_reason`、`forced_reclaim_epoch`、`forced_penalty_amount`、`bond_refund_amount`、`bond_refund_restricted_amount`、`bond_refund_liquid_amount` | 欠费超宽限或持续闲置时系统回收 | `upkeep_grace/inactive_reclaim_candidate -> forced_reclaimed -> unclaimed` | 欠费宽限 `grace_epochs = 2`；闲置阈值 `7` 个 epoch，最晚 `10` 个 epoch 完成回收；`forced_reclaim_penalty_bps = 2000`（作用于剩余 bond） | 仅系统可执行；owner 不能在最终回收点之后阻断 |
| Transfer Guard | `transferable_balance`、`restricted_balance`、`blocked_reason` | 玩家发起普通 main token 转账或 explorer 导出余额时，系统区分可转账与不可转账余额 | `eligible -> blocked_for_restricted_only` | `transferable_balance = liquid_balance`；`restricted_balance` 只做展示，不计入可转账金额 | `TransferMainToken`、公开转账 API、explorer 排名与总额不得消费或误计 restricted bucket |
| Reputation Cap | `reputation_tier`、`claim_cap`、`owned_agent_count` | 认领前校验当前账号可占有的 agent 上限 | `eligible -> eligible/blocked_by_cap` | `tier-0 cap=1`、`tier-1 cap=2`、`tier-2+ cap=3` | 非法 tier 或超 cap 直接拒绝，不允许只靠余额绕过 |
| First Claim Onboarding | `owned_claim_count`、`next_claim_quote.slot_index`、`target_agent_id`、`blocked_reason`、`actor_agent_id` | 无 bound Agent 时进入显式首次注册/绑定或原身份恢复；已有 claimer 且 `owned_claim_count=0`、`slot_index=1` 时才展示经济 `slot-1` CTA，要求选目标后 `Prepare -> Confirm` | `no_agent_for_account -> registration_pending -> bound_agent_available`，或已有绑定上的 `quote_ready -> onboarding_visible -> confirmation_open -> claim_submitted` | 消费当前入口公布的注册能力或 canonical quote/blocker；未支持逐账号注册时显示缺口与恢复路径，不伪装可执行 | 首次注册以已认证账号和世界准入为前提，不要求先有 claimer；`ClaimAgent` 仍要求该玩家已注册的 claimer，会话主体、执行 actor 与 claim target 分开校验；2.1 节的后继目标不等于当前固定 starter 入口已支持 |
| Player Session / Agent Visibility Boundary | `player_session.player_id`、`bound_agent_id`、`claim_owner_id`、`owned_claims`、`agent_player_bindings` | 进入共享世界时，Viewer 默认 Agent 控制列表只展示当前账号绑定或当前玩家绑定待同步 Agent；其他账号 Agent 与未绑定 Agent 不进入默认 Agent 列表，也不得看到 ready chat/command 主操作 | `no_agent_for_account -> bound_agent_available` 或 `no_agent_for_account -> slot_1_claim_visible` | “我的/可操作”只来自当前 session binding 或 canonical claim/owned fields；未绑定 Agent 视为数据不完整或迁移态，不作为正常玩家对象展示；其他账号活动若需要世界感，后续放地图实体、世界事件或只读 world layer，不混入控制列表 | 不得从全局第一个 Agent、左侧默认选中目标、隐藏的世界 Agent、未绑定 Agent 或 `state.agents.keys().next()` 推断当前账号 ownership |

- Acceptance Criteria:
  - AC-1: 首个 agent 认领没有任何免费分支；v1 必须显式校验 `activation_fee_amount > 0`、`claim_bond_amount > 0`、`upkeep_per_epoch > 0`。
  - AC-2: 认领成功后必须立即形成 `activation fee` 记账、`bond locked` 状态和下一次 upkeep 结算 epoch，不允许“先占坑后补票”。
  - AC-3: 同一 agent 被两个账号并发认领时，只允许一个成功；失败方不得丢失 token、不得产生脏 claim。
  - AC-4: `slot-1` 允许消费 `restricted starter claim balance`；`slot-2/3` claim 与对应 upkeep 不允许消费该 bucket，必须返回结构化 blocker。
  - AC-5: owner 主动 release 时，若已过 cooldown 且无未结债务，必须退回剩余 bond，并按原 funding source 将 restricted/liquid refund 拆分回对应 bucket。
  - AC-6: 欠费 claim 在 `2` 个 epoch 宽限后必须被回收；持续闲置 claim 必须在 `10` 个 epoch 内被回收到未认领池。
  - AC-7: 强制回收必须给出 `forced_reason`、`forced_penalty_amount`、`bond_refund_amount`，并通过统一事件与审计字段可追溯；restricted 来源的 refund 不得回到 liquid。
  - AC-8: Viewer / pure API 必须同时展示 `claim_slot_index`、`activation_fee_amount`、`claim_bond_amount`、`upkeep_per_epoch`、`grace_deadline_epoch`、`release_cooldown_epochs`、`restricted_starter_claim_balance` 与 `eligible_claim_balance`，不允许 UI/API 各算一套。
  - AC-9: `TransferMainToken`、公开转账 API、explorer 余额排序与总额展示必须显式区分 `transferable_balance` 与 `restricted_balance`；restricted 不得被视为可转账资产。
  - AC-10: 本机制不得被表述为现实货币付费解锁、公开售卖 agent 或永久产权出售；它是 gameplay 内部的 main token 承诺成本机制与受限启动资助工具。
  - AC-11: 当玩家处于 PostOnboarding、当前 `owned_claim_count=0` 且 `next_claim_quote.slot_index=1` 时，Viewer 必须展示专用 `slot-1` 认领引导卡；该卡必须直接复用 canonical quote / blocker，并要求玩家完成“选目标 -> Prepare -> Confirm”的显式确认链路，不允许静默自动认领。
  - AC-12: 当玩家处于首个 `slot-1` claim、没有 active restricted grant、账户也没有现成 restricted starter balance，且 dedicated pool 能覆盖 upfront 缺口时，`ClaimAgent` 必须原子完成 auto-funding + claim；若 dedicated pool 仍不足，则 runtime / viewer / pure API 只返回资金不足 blocker，不再生成 pending/operator-review 状态。
  - AC-13: 新账号进入已有 Agent 的共享世界时，Viewer / pure API 必须把 `其他账号已绑定 Agent`、`未绑定/数据不完整 Agent`、`当前 session bound Agent` 与 `claim_owner_id/owned_claims` 分开表达；Viewer 默认 Agent 控制列表不得展示其他账号已绑定 Agent 或未绑定 Agent。若当前账号没有 bound Agent 或 canonical claimer，不得显示为“我的 Agent”、不得作为 ready chat/command 主目标，也不得展示可执行经济 `ClaimAgent` CTA；显式首次注册不受“先有 claimer”这一经济动作前提限制，但仍须通过当前账号/世界准入。chat / prompt / actor gameplay action 不得隐式绑定。空世界首次进入、已有世界新账号及原账号返回分别按 [AC-AGC-ENTRY-001](#ac-agc-entry-001) 验收。
  - AC-14: 首个 `slot-1` onboarding quote 必须展示候选 agent 的最小差异条：初始位置、擅长能力或当前任务、对首个工业目标的帮助、主要风险和推荐理由；若只展示费用、余额、cap、cooldown，则该 claim 引导只能算成本确认，不能算路线选择。
  - AC-15: Claim Quote 在玩家确认前必须展示 `upkeep_runway_epochs / next_upkeep_due_epoch / projected_grace_entry_epoch / low_runway_warning`；若 eligible balance 扣除 upfront 后只能覆盖少于 runway 阈值的 upkeep，必须提示推荐下一步（补足余额、等待或降低扩张节奏），不得把持续成本包装成一次性解锁。
- Non-Goals:
  - 不在本专题内定义现实货币购买、法币结算或站外商城。
  - 不把 agent 认领做成永久不可回收的链上产权 NFT。
  - 不在本轮引入代理拍卖行、agent 二级交易市场或跨玩家租赁市场。
  - 不在本轮为 claim 成本拍死绝对 token 数值；v1 先冻结公式、状态机和不可突破的边界。
  - 不把 `restricted starter claim balance` 扩展成通用的“全游戏不可转账 main token”体系。

### 2.1 Player Session / Bound Agent / Claim Owner Boundary

本专题的非零成本、槽位、维护和回收规则适用于建立正式经济认领关系的 `ClaimAgent`。首次创建并绑定可操作 Agent 与经济认领是不同的世界结果：前者不能仅因界面动作名称包含 claim 就被解释为已经支付认领成本、锁定保证金或承担 upkeep；后者仍须遵守本专题完整经济合同。这一区分不新增注册资格，也不提供免费的经济认领旁路。

- `player_session`: 浏览器或 hosted public join 发放的访问身份，包含 `player_id`、device/session 信息和公钥。它不是世界内实体，也不等于 Agent claim owner；清理浏览器 localStorage 或换 profile 只会重建该访问身份，不会清空共享世界状态。
- `world-visible Agent`: world snapshot 中存在的 Agent。共享世界里的旧 Agent 不等于当前账号的可操作对象；若它已绑定到其他账号或缺少账号绑定，Viewer 默认不得把它放入当前账号的 Agent 控制列表。后续若需要表现“共享世界里有人活动”，应放到地图实体、世界事件或只读 world layer，而不是混进“我的 Agent”列表。
- `bound Agent`: runtime live / viewer control plane 中当前 `player_session -> agent_id` 的控制绑定。聊天、prompt、需要 actor 的 gameplay action、以及“我的可操作 Agent”表达必须以该绑定或等价 canonical auth 字段为准。
- `claim_owner_id`: `AgentClaimState` 中的正式 gameplay/economic claim owner。当前 runtime 语义下它是 claimer Agent id，而不是浏览器 `player_id`。`owned_claim_count`、`owned_claims` 和 cap 判断必须读取 canonical runtime claim snapshot，不得由 Viewer 自行按世界列表推导。
- 首次注册/绑定的适用性由当前账号在该世界的权威状态决定，不由世界是否已有其他 Agent 决定。`claim_first_agent` 是既有注册入口名，不代表经济 `ClaimAgent`，也不授权创建或重置世界。正式目标按下表收敛；当前固定 starter 入口尚不能证明逐账号共享世界注册，缺能力时必须明确 blocked/unavailable。
- Viewer / pure API 验收边界：不得从 `state.agents.keys().next()`、左侧列表第一个 Agent、默认 selection、隐藏的其他账号 Agent、未绑定 Agent 或 legacy `first_agent_id` 推断当前账号 ownership；必须用当前 session binding、`agent_player_bindings`、`owned_claims`、`next_claim_quote` 等 canonical 字段决定“我的/可操作/可认领/不属于当前账号所以不进入默认控制列表”。

<a id="player-session-first-agent-entry"></a>
#### 同一持久世界的首次进入与返回

| 情境 | 玩家路径与权威前提 | 完成与隔离边界 |
| --- | --- | --- |
| 已合法创建的空世界，首个账号尚无绑定 | 认证账号并确认该世界准入，显式申请注册/绑定自身第一个 Agent；无需预先拥有 claimer | 只有匹配该账号、世界和原请求的权威注册/绑定结果才建立可控对象；不由玩家入口新建 genesis，不自动形成经济 claim 或 starter OC 领取 |
| 已有其他 Agent 的同一世界，新账号尚无绑定 | 同样使用该账号的显式首次注册/绑定；已有他人 Agent 不替代注册，也不要求先执行依赖 claimer 的经济 ClaimAgent | 新结果与其他账号身份、Agent、库存和目标隔离；不得占用他人 Agent、借用全局 starter ID 或清空世界。原账号与新账号各自只能操作当前授权对象 |
| 原账号回到同一世界 | 按现行身份恢复合同重新认证并读取已有 binding、归属和原请求结果；身份或控制权不能确认时先恢复/补证 | 恢复原 Agent 与已保存成果，不重新注册、不重复领取、不重新认领；换设备、断线或未收到 ACK 不构成新的首次资格 |

注册/绑定、`claim_starter_oc` 与经济 `ClaimAgent` 是三个独立受理与结算边界。前一步成功不能代签后一步；首聊通过也不能代签 canonical owner goal 所需的身份/控制授权。重复和结果未知时沿原账户、世界及请求身份查询已有结果；已提交结果只恢复一次，冲突身份或不完整旧记录不升级为新资格。具体投影、提交与兼容边界见配对设计 [DES-CLAIM-ENTRY-001](gameplay-agent-claim-economy-contract.design.md#des-claim-entry-001)。

<a id="req-agc-entry-001"></a>
#### REQ-AGC-ENTRY-001：同世界逐账号首次控制身份与恢复

- 产品承接范围：[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) 的逐 primary-mode 入口/证据区分，以及 [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) 的身份/会话失败分类与真实恢复路径。

Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270

本条只承接首次注册、已绑定账号返回及其 blocked/未知情境；逐账号注册与绑定的具体义务仍由本专业要求定义，不由产品模式要求推导所有账号均可准入。

- 性质与范围：后继目标；承接 PRD-GAME-011F / AC-13 及[首次绑定与经济认领的区分](#player-session-first-agent-entry)，不表示当前入口已支持多账号。
- 要求：上述三种情境均有明确的合法路径或实际 blocker；新账号无需借用已有 claimer 完成自身注册，原账号不能以重连重新获得首次资格。注册不创建经济 claim，不替代 OC 领取或 owner/control grant。
- 验收：[AC-AGC-ENTRY-001](#ac-agc-entry-001)；系统承接：[DES-CLAIM-ENTRY-001](gameplay-agent-claim-economy-contract.design.md#des-claim-entry-001)。

<a id="ac-agc-entry-001"></a>
#### AC-AGC-ENTRY-001：第二位玩家进入与原账号返回

- 产品承接范围：[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) 的逐 primary-mode 入口/证据区分，以及 [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) 的身份/会话失败分类与真实恢复路径。

Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270

本条只承接首次注册、已绑定账号返回及其 blocked/未知情境；逐账号注册与绑定的具体义务仍由本专业要求定义，不由产品模式要求推导所有账号均可准入。

- 给定：固定候选与同一已合法创建世界，账号 A 已有绑定与成果，账号 B 已认证且获准进入但没有 claimer；另保留空世界首账号场景。
- 当：B 通过正式入口显式注册/绑定，A 断线后返回；分别重复原请求、丢失 ACK、重启恢复，以及尝试使用对方 Agent 或不匹配身份。
- 则：B 能取得自身合法可控 Agent，A 恢复原 Agent/成果，world/chain/genesis 不变；重复不新增 Agent、OC、经济 claim 或费用，跨账号与身份不匹配请求无越权效果。结果未知先查询，既有 binding 无法验证时不创建替代资格；未支持的当前入口记录 blocked，不能以空世界单账号或 fixture 代签通过。
- 同候选 Viewer、pure API 与 Agent 适用入口须分别表达注册、已绑定、经济认领和未知状态；经济 Claim 的 Prepare/Confirm 终结仍按原独立合同验收。

### 2.2 Cold-Start Resource Boundary

- `restricted starter claim balance`: 只服务 `slot-1` claim / upkeep 的受限 main-token bucket；它可以通过 dedicated pool 自动补足首个 `slot-1` upfront 缺口，但不能转账、提现、用于普通资产动作，refund 也必须保留 restricted provenance。
- `starter OC`: 有效 Agent 控制绑定建立后的一次性首聊解锁与初始 liquid OC 授予；它不建立经济认领。就 starter OC 这一道 gate 而言，非零 liquid OC **或**已有 `starter_oc_claims` 记录均可通过；只有 liquid OC 为零且确认没有该记录时才受阻。领取记录已存在后，即使 liquid OC 后续用尽，也不能重新把该 gate 解释成未解锁。通过此 gate 不替代有效会话、Agent 控制权、LLM 可用性等独立前提，也不授予重复领取资格。零 liquid OC 的 cold-start 动作链可为 `claim_first_agent -> claim_starter_oc -> first agent chat`。
- 两者不得混写：`restricted starter claim balance` 解决“能否付出首个 agent claim 承诺成本”，`starter OC` 解决“首个 agent 已存在后，是否已完成 LLM/agent agency 对话的首聊解锁”。starter OC 不是免费 agent claim、不是通用补贴，也不是 `slot-1` claim/upkeep 的 restricted bucket。
- 平衡口径仍未冻结：`starter OC` 的授予额度、实际消耗模型、滥用边界，以及未来是否要把 chat 重新绑定到可耗尽预算，需要后续基于真实 cold-start 样本单独复核；当前文档只冻结资源边界、runtime 现状和首读路由。

### 2.2.1 First `slot-1` Claim Commitment Packet

首个 `slot-1` claim 的 canonical quote / choice 必须作为一个决策包呈现，而不是将已有成本字段拆成需要玩家自行拼接的技术信息。确认前，它必须让玩家理解：

- 当前候选的用途、与其他候选的差异、对首个工业目标的帮助和主要风险；若推荐当前候选，必须有可读理由。
- 非零 upfront 成本，以及扣除该成本后可覆盖的 upkeep runway、下一次持续义务和低 runway 风险。
- 主动 release、欠费进入 grace 或持续闲置而 forced reclaim 的可读触发，并说明相应的保留/失去结果以及补足、恢复或重新选择的可行动路径。
- 当前候选之外的最佳决策：比较候选、等待可读条件，或先补足资金/降低当前扩张压力；不得把“现在确认”写成无替代的默认成功路径。

该决策包复用本专题已有的 canonical quote、claim、upkeep、release 与 reclaim 语义，不新增字段、数值、状态或 Viewer 布局。`restricted starter claim balance` 只可支持首个 `slot-1` 的 non-zero claim/upkeep 成本；其一次性受限帮助不是 free claim，也不是后续 upkeep 的无限/持续补贴。`starter OC` 则在 Agent 已经通过 `claim_first_agent` 建立后处理首聊的 liquid OC 解锁，两者不得相互计入、替代或被表述为同一资金来源。

<a id="first-chat-unlock-boundary"></a>
### 2.3 First-Chat Unlock Value Preview

玩家已经取得可操作 Agent、准备开始首次对话时，需要知道对话能帮助什么、为何出现解锁动作，以及现在领取或稍后再来分别意味着什么。预览必须解释这次选择的真实后果，不能把首次注册误写成经济认领，也不能让玩家以为首聊是持续支付认领维护费的另一道入口。

- `starter_oc_quote` / `first_chat_unlock_preview` 是 `claim_starter_oc` 确认前的玩家侧价值合同；查看、推荐和延后动作本身不产生领取、经济认领或维护结算，已有经济 claim 仍按原规则随时间推进。
- 最小字段：
  - `chat_purpose`：本次首聊如何把当前有控制权的 Agent 转成可理解的 agency 对话入口；绑定本身不证明已经存在经济认领。
  - `immediate_playable_help`：首聊立即能提供的、与当前 canonical gameplay state 相符的行动帮助或目标澄清；不得承诺尚未存在的 runtime 能力。
  - `first_question_or_action_hint`：玩家可立刻提出的首个问题，或首聊后可执行的下一步 action hint。
  - `resource_boundary`：明确 `starter OC` 是一次性首聊解锁和 initial liquid OC 授予，与 `restricted starter claim balance` 的 slot-1 claim/upkeep funding 分离；该字段本身必须说明“非零 liquid OC 或已有领取记录即可通过 starter OC gate”，包括领取记录存在但余额已用尽的情况，不能只在其他字段补充。
  - `defer_effect`：根据当前权威经济认领事实说明延后的后果。真实经济 claim 已存在时，只说明其既有维护规则继续适用；确认没有经济 claim 时，不得声称玩家已有 upkeep 责任。说明缺失或沿用未区分注册与经济认领的旧文案时，应明确义务尚不能确认，不能把缺失资料当作没有责任。延后不会取消已有义务、增加新义务或冻结 Agent；历史未决义务仍以权威状态为准。
  - `recommended_unlock_action`：在当前 gate 受阻的价值预览中说明 `claim_starter_oc` 这一下一步；实际能否执行继续由现有可用动作与 blocker 决定，字段本身不授予权限，也不要求已通过此 gate 的玩家重复领取。
- 合同边界：已交付文案切片只细化 cold-start disclosure；下述权威输入与未知状态是待实现的后继目标。二者均不改变 OR 规则、OC 额度、注册/认领资格、账本或独立授权前提；文档收敛不宣称 service gate 已修复。

<a id="req-agc-chat-001"></a>
#### REQ-AGC-CHAT-001：首聊资源边界准确说明一次性 gate

- 性质与范围：目标要求；适用于 Viewer、pure API 与 Agent 适用读面的首聊预览和解锁状态说明；service 权威投影仍是后继目标。
- 要求：`resource_boundary` 必须独立说明 OR 条件、余额用尽但领取记录仍有效，以及与经济 claim/upkeep 的分离。同一已认证主体/Agent/世界的适用权威事实中，正余额或已领取任一为真即可通过该 gate；只有已知零余额且已知无记录才阻塞，其余保持未知并提供刷新/补证路径。缺失、裁剪或失效输入不得补成零；规则说明与预览不替代最终提交校验。
- 上位承诺：[`Agent 所有权与持续经营`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)；规则权威为本专题 2.2、2.3 节。
- 验收：[AC-AGC-CHAT-001](#ac-agc-chat-001)；系统承接：[既有首聊 gate 投影](gameplay-agent-claim-economy-contract.design.md#des-claim-chat-002)、[service 权威输入后继合同](gameplay-agent-claim-economy-contract.design.md#des-claim-chat-005)。

<a id="req-agc-chat-002"></a>
#### REQ-AGC-CHAT-002：维护责任只来自真实经济认领

- 性质与范围：目标要求；适用于首聊预览中的绑定、经济认领和延后后果说明。
- 要求：预览必须区分首次注册/绑定、已确认的经济认领与未知认领事实；不得把绑定、后续认领报价、旧领取记录或缺失资料作为既有 upkeep 责任或不存在责任的证据。
- 上位承诺：[`Agent 所有权与持续经营`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)；规则权威为本专题 2.1、2.3 节。
- 验收：[AC-AGC-CHAT-002](#ac-agc-chat-002)；系统承接：[认领事实与延后说明](gameplay-agent-claim-economy-contract.design.md#des-claim-chat-003)。

<a id="req-agc-chat-003"></a>
#### REQ-AGC-CHAT-003：不同入口和语言保留同一选择后果

- 性质与范围：目标要求；适用于 Viewer 中英文、pure API、Agent 适用读面及旧预览兼容回读。
- 要求：同一权威预览在不同入口和语言中必须保留相同 gate 条件、义务范围与下一步；旧版缺失或不可验证的状态保持未知。身份切换、断线或来源失效不得沿用旧可执行结论；能力/权限不同的入口可以保持 unavailable，但不能伪造零余额或另一套门禁。语言转换与显示不改变资格或虚构维护责任。
- 上位承诺：[`Agent 所有权与持续经营`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)；规则权威为本专题 2.3 节。
- 验收：[AC-AGC-CHAT-003](#ac-agc-chat-003)；系统承接：[显示与兼容边界](gameplay-agent-claim-economy-contract.design.md#des-claim-chat-004)、[service 权威输入后继合同](gameplay-agent-claim-economy-contract.design.md#des-claim-chat-005)。

<a id="agent-manufacturing-v1"></a>
### 2.4 额外 Agent 工业制造首版专业政策

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928；承接产品 [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004) 及 [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)。系统实现输入见[额外 Agent 工业制造设计](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-001)。

本节是本次设计选择的 `agent-manufacturing-v1` 目标政策及默认参数，供实施与独立评审；不是当前已运行配方、平衡测量值或此前已批准的运营参数。首版只覆盖同一权威世界内、当前主体依法定世界规则已有操作/支出权限的装配设施。现有 runtime 仅允许工厂 builder 启动生产；本设计不把组织成员身份或受益人身份当作 builder 权限的替代。第三方撮合、制造服务价、跨账号代扣、远程成品物流和 AOS-005 经济转让不在首版。

#### 2.4.1 配方、成本与时间的唯一默认值

首版新增的配方身份为 `recipe.assembler.agent_mk1`、政策 revision `1`，产出是一份不可自由转让的待交付制造记录，最终匹配一次新 Agent 交付；它不是 `logistics_drone`、通用材料 `agent`、注册凭证或可以重复兑付的道具。下表里的既有材料名称与设施来自[标准配方目录](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)和[工厂目录](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs)；新增配方/参数尚未进入这些源码。

| 政策项 | 首版默认值与计算 | 成本或边界 |
| --- | --- | --- |
| 每单数量、型号 | 一个标准 Agent，`quantity=1`；采用固定版本的现有标准 body 模板，不开放自选属性/模块 | 不批量铸造，不生成新 starter OC、初始库存、声誉或治理资格；初始可支配资源为零，后续充能按普通资源转移执行 |
| 材料投入 | `alloy_plate × 8`、`motor_mk1 × 4`、`control_chip × 4`、`sensor_pack × 2`、`module_rack × 1` | 必须已合法到达该工厂 input ledger 并有匹配来源；不从世界公共材料或无关账号兜底，不把更早的物流费用/损失退回 |
| 制造电力 | `Electricity × 48`，由当前有权操作该设施的 builder 承担 | 确认时预留，生产开始一次消耗；设施维护仍按既有维护规则承担并在报价独立提示 |
| 制造费用 | 本首版不另收 OC 制造服务费；材料和电力仍全额投入 | 模拟器里的通用 hardware 扣费不得隐式叠加到这一正式 runtime 配方；新增收费必须作为新政策明确报价 |
| 认领费用 | 沿用本专题现有 `activation + bond + 首期 upkeep`，只允许 liquid 资金 | 制造和认领费用独立，确认时预留认领款，交付才结算/锁押金；未交付不产生新 upkeep |
| 设施与槽位 | `factory.assembler.mk1`；每单一个 recipe slot，每设施最多一个制造中或尚未生产的制造订单 | 普通 recipe 与制造共用容量，扣除制造后至少保留一个普通 recipe slot；被旧任务占满时无预留等待，不超卖 |
| 制造时间 | 累计 `12` 个有合法生产条件的 canonical world ticks；完成后在该工厂权威位置交付 | 停产/失权期间不累计生产进度，恢复重算预计完成时间但不延长硬截止；世界暂停不推进窗口，不换算成固定现实分钟 |
| 报价有效期 | `5` ticks，提交要求 `now < quoted_at + 5` 且相关事实仍匹配 | 过期、价格/配方/权限/物料/设施的相关版本漂移时重新报价；无关世界事件不使报价自动失效 |
| 确认后启动期限 | `10` ticks，最晚启动要求 `now < confirmed_at + 10` | 无合法启动条件则一次取消结算并解除预留；不续租占坑 |
| 生产完成后的待交付窗口 | `24` ticks，交付要求 `now < produced_at + 24` | 到期仍无法交付则一次报废；待交付占一个专用成品保管位，每设施最多一个，保管位未释放前不接新制造单 |
| 生产故障恢复窗口 | 生产后受阻起 `24` ticks；不得晚于 `started_at + 12 + 24` | 可恢复时继续原单，不能刷新期限；永久不可恢复或到期则一次报废，防无限占产能 |
| 取消/报废 | 开工前全部预留原路释放；开工后制造投入不退、不额外产回收材料，未结算认领款全额解冻 | 不自动转卖、转给其他受益人或重新领取启动支持；技术错误未提交时不视为物料已损失 |

预留是资产仍属于原持有者但暂不可支出；消耗是不可逆生产投入；交付结算才按现有经济合同分配激活费、锁定押金和首期维护。三个金额必须分别显示。quote 还须说明预留后的剩余可用资金/电力及其对既有维护义务的影响，不能因制造订单冻结或免除主 Agent/设施已存在的应付义务。若工厂维护另有周期费用，应展示费率/责任主体和已知风险，不能把这类持续费用伪装成本单固定总价。

#### 2.4.2 额外槽位、并发与计价

制造资格按同一受益主体的**去重可控制 Agent 集合**计算：当前主绑定 Agent 与其经济 claim targets 的并集，再加未终结制造预留。主 Agent 即使只是注册绑定，也计入制造的已有经营规模；这不为主 Agent 新增经济 claim、扣费或维护责任。首版继续使用既有声誉档位 `cap=1/2/3`；制造必须保留主 Agent，且新增后不得超 cap。无主绑定或无法证明组织的取得主体时返回资格 blocker，不能创建替代主身份。

制造报价的 `slot_index = 已有去重 Agent 数 + 未终结制造预留数 + 1`，至少为 `2`。使用既有逐项向上取整规则时，slot-2 为激活费 `150`、押金 `300`、首期/每期 upkeep `38`，确认需预留 `488` liquid OC；slot-3 分别为 `200/400/50`，预留 `650`。这些金额由[现有 claim 计价函数](../../../crates/oasis7_runtime/src/agent_claims.rs)派生，不另调现有基础费率；制造适配器必须将本节有效经营规模交给它，不能直接沿用只统计经济 claim 的旧调用。受限 starter bucket 与自动补足恒为零。

冻结的槽位和价格不会因为另一份 claim 释放而降价、重开 slot-1 支持或自动换受益人。交付时以当前集合和原预留再检查 cap；声誉降档、控制关系丢失或禁止取得时进入有期限的待交付，而非越限创建。启用本政策后，制造确认、普通 ClaimAgent 及所有新增取得入口必须使用同一个“主绑定 Agent ∪ 经济 claim targets ∪ 在途制造目标”的去重容量守卫，以本次取得后的集合核对 cap；制造交付将预留替换为真实 Agent，不释放一个可被重复认领占用的名额。对集合内同一既有目标建立经济 claim 不重复计人数，但仍需其原有资格和经济规则。普通 claim 的既有计价与历史义务不在此迁移；新容量守卫必须独立存在，不能只修报价函数或在预留期起效。

典型反例是主 Agent 只注册、`cap=2`，制造第二个完成后，旧 claim 计数只有一。如果普通 ClaimAgent 仍允许再取第三个，即使制造单本身没有超额也属于旁路。启用时发现历史已超新口径的主体，只阻止所有会进一步扩大集合的新增取得，不能追溯收费或自动回收旧 Agent；首次注册/返回合同保持原有语义。

#### 2.4.3 交付与失败的玩法判定

- 首版按 `terminal-admission` 完成：订单确认、生产开始、生产完成均不产生 Agent 控制权。完整交付同时消耗唯一待交付记录、创建唯一 Agent 身份及制造来源、结算认领费用、安装排他 claim 和非主绑定的合法控制关系。新 Agent 不需要重领 starter 支持：原主体可明确选择它查看获准 owner 信息、下达受现有规则约束的直接意图，后续自治/委托仍须显式授权。位置固定为原工厂权威点，不能依据 UI 坐标瞬移交付。
- 委托操作权限、设施权限、资金权限和受益人的取得资格分别校验。撤销某个操作者的授权不改变订单受益人/退款来源；合法主体可以重新认证后继续同单，不能将旧授权作为新交付许可。已交付结果保留原身份与 provenance，不因旧响应重试或重新报价再交付。
- 首版正常开工即一次消耗全部材料与制造电力，不做分段材料投入。生产失败与待交付过期只退尚未结算的认领预留；如需保全仍有价值的半成品，必须在未来新的 salvage 政策中定义，不能临时全额返料。玩家确认前须看见这一最坏损失。
- 生产受阻只能恢复同一配方、工厂、来源和受益人的订单；切配方、换厂家或换受益人需要新订单并先结清原单，原生产进度、费用和交付资格不能搬成免费新订单。
- 取得后沿用认领维护、grace、release/reclaim 及 Agent 自治/委托合同；订单不预设目标、不复制主 Agent 授权，不扩大世界权力。claim 正式释放/回收后旧控制来源失效，后续合法新 claim 才能取得新的控制关系，旧 grant 和未决执行不继承；原制造订单、身份、成本和责任保留。原根 owner 来源撤销或变更不把额外 Agent 自动给新主体，同一合法主体恢复后须重新认证；技术适配由[原子控制来源合同](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-004)承接，不通过额外主绑定或递归 owner 链实现。公共 quota 可以拒绝不合格订单，不能直接创建 Agent、补材料或免除产能。

专业验收按系统设计的 [11.1 验证映射](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#manufacturing-validation)及其[制造系统验收手册](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md)独立观察正常交付、各阶段失败、双请求与跨请求唯一性、权限变更、旧数据恢复、资金/物料守恒及可读下一步。当前只有本节目标政策，手册是未来计划，尚无新制造运行、平衡或玩家理解证据。数值假设的 owner 为 `gameplay_designer` / `producer_system_designer`；真实工业投入、等待体验或损失反馈证明扩张过易/不可达时，另修政策 revision 并只作用于重新报价/新单，不能回写旧单成本。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题不新增 AI 模型能力，仅涉及 gameplay 规则、账本 bucket 与状态机）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview:
  - gameplay 层新增 `agent claim economy` 规则：负责报价、claim 状态机、slot multiplier、cap 和回收逻辑。
  - runtime 负责原子扣费、bond 锁定、epoch upkeep 结算、slash / refund 和记账事件，并新增 `restricted starter claim balance` bucket、可消费范围与 refund provenance。
  - viewer / pure API 只读取 canonical claim 状态、报价字段与 restricted/liquid 余额拆分，不自行推导隐藏成本。
  - Viewer 的首次 `slot-1` onboarding 只负责把 canonical quote / blocker 和显式确认串成操作流，不新增第二套 claim 规则，也不在 HUD 内静默触发自动认领。
  - main token 账本继续作为唯一价值来源；claim 机制允许 `slot-1` 消费 `restricted starter claim balance + liquid main token` 的 canonical 组合，但转账面与公开资产面只消费 `liquid main token`，不旁路 signed action / audit 链路。
- Integration Points:
  - `doc/game/prd.md`
  - 对应 GitHub task issue evidence
  - `doc/game/gameplay/gameplay-top-level-design.prd.md`
  - `doc/product/world-infrastructure/distributed-consensus-and-state-availability.prd.md`
  - `doc/product/world-infrastructure/deterministic-world-execution.prd.md`
  - `doc/p2p/token/mainchain-token-allocation-mechanism.prd.md`
  - `doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.prd.md`
  - `testing-manual.md`
- Edge Cases & Error Handling:
  - 余额不足：报价可见，但确认认领必须拒绝，并明确缺少的是 `activation fee`、`bond`、`upkeep` 还是 `eligible_claim_balance` 总额不足。
  - 首个 `slot-1` dedicated pool 不足：若账户现有 liquid/restricted 与 dedicated pool 自动补足后仍不足，报价与 claim 都必须明确显示剩余缺口；不得回退成 pending approval request。
  - 账户已有 restricted grant / restricted balance：已有显式 grant 或现成 restricted starter balance 时，不再额外自动补新的一笔 dedicated-pool startup amount，避免把自动首发变成隐式重复补贴。
  - restricted grant admin 未登记：当 `governance_main_token_controller_registry.restricted_starter_claim_admin_account_ids` 缺失、为空、或不包含当前 `issuer_account_id` 时，issue / revoke 必须在 grant / treasury / beneficiary 逻辑之前直接拒绝。
  - restricted bucket 过期：已过 `expires_at_epoch` 的 starter balance 不得进入 quote 可用余额，且必须生成可审计的 `expired` 事件。
  - mixed funding：若 upfront cost 同时由 restricted 与 liquid 支付，系统必须记录 bond provenance；后续 refund/slash 结算必须按该 provenance 拆分。
  - 首次认领选中自己：若当前选中的 agent 是玩家自己的 claimer agent，Viewer 必须提示重新选择未认领目标，不得把 claimer agent 当成 claim target。
  - 首次认领已选中已持有目标：若当前选中目标已经属于玩家的 owned claims，Viewer 必须回到“重新选目标”状态，不得继续打开确认。
  - 首次认领候选缺少差异理由：若 `slot-1` quote 只能展示成本和资格，但缺少位置、能力/当前任务、首个工业目标帮助、风险或推荐理由，必须标记为 `candidate_rationale_missing`；玩家不应在非零承诺成本下盲选 agent。
  - 首次认领缺少持有 runway：若玩家能支付 upfront，但 quote 不展示确认后可支撑的 upkeep epoch、下一次 upkeep 时间或预计 grace/reclaim 风险，必须标记为 `claim_runway_missing`；玩家不应把长期占有成本误读成一次性解锁。
  - 首次认领缺少路线选择判断：若 `slot-1` quote 不能让玩家比较“当前候选 / 其他候选 / 等待或补资金”，或缺少 `claim_now_route_fit / compare_candidates_first / wait_or_fund_first` 分类与推荐动作，必须标记为 `slot_1_claim_choice_missing`；玩家不应把第一次 agent ownership 承诺误读成单纯付费门槛。
  - 首次认领承诺包不完整：若确认前无法同时读到候选用途/差异、upfront 成本、确认后 upkeep runway、release/grace/reclaim 触发、恢复或重新选择以及最佳等待/替代动作，必须标记为 `first_claim_commitment_packet_missing`；不得让玩家靠账本字段或事后回收事件重建风险。
  - 首聊解锁缺少价值预览：若 `claim_starter_oc` 前没有 `starter_oc_quote` / `first_chat_unlock_preview`，或缺少 `chat_purpose`、`immediate_playable_help`、`first_question_or_action_hint`、`resource_boundary`、`defer_effect`、`recommended_unlock_action` 中任一项，必须标记为 `first_chat_unlock_value_missing`；玩家不应在取得可操作 Agent 后仍把首聊解锁误读成第二道不透明支付门。字段存在不等于含义正确：OR gate 披露缺失、把注册误写成 upkeep、把未知认领当作不存在，或中英文改变义务范围，都不能据此通过首聊价值验收。
  - 新账号进入已有世界：若 world snapshot 已有 Agent 但当前 `player_session` 没有 `bound_agent_id` 或 canonical claimer，Viewer 必须进入“当前账号还没有 Agent”的 onboarding，并在 runtime 发布 `claim_first_agent` 时展示认领入口；若 runtime 只发布 canonical `slot-1` quote，则展示对应 onboarding。默认 Agent 控制列表不得展示其他账号已绑定 Agent 或未绑定 Agent。不得把全局第一个 Agent 自动标为“我的 Agent”，也不得把该 Agent 的 chat/command 入口显示为 ready。
  - 并发争抢：两个提交同时命中同一 `agent_id` 时，只允许第一个写入 `claim_owner_id`；第二个返回冲突，不得重复扣费。
  - upkeep 结算时余额不足：进入 `upkeep_grace`，并写出 `grace_deadline_epoch`；宽限内补足可用余额后恢复 `claimed_active`。
  - `slot-2/3` 误用 restricted：必须拒绝，并返回 `restricted_balance_not_eligible_for_slot` 一类结构化 blocker。
  - release 请求早于 cooldown：拒绝 release，但必须返回剩余 `cooldown_epochs_remaining`。
  - force reclaim 与 owner 同 epoch 操作冲突：以先完成的合法状态迁移为准，后到请求必须基于最新状态重试。
  - 闲置判断：若 `7` 个连续 epoch 无 owner 发起的有效控制动作或 agent 产出的有效推进事件，则进入 `inactive_reclaim_candidate`；若到 `10` 个 epoch 仍未恢复，则强制回收。
  - slash 后 refund 为负：退款下限固定为 `0`，不得从系统额外倒贴。
  - 转账面误读余额：若账户只有 restricted 余额、没有 liquid 余额，普通转账必须拒绝并返回 `transferable_balance=0`，不得因为显示“总余额 > 0”而误放行。
  - tier 异常：无效 `reputation_tier` 一律按最低 tier 处理，不允许读空后默认开放更多槽位。
  - UI/API 语义缺口：若某端未显示 canonical claim 成本与倒计时，或未显示 restricted/liquid 区分，则该端不得宣称支持正式 agent claim 管理。
- Non-Functional Requirements:
  - NFR-AGC-1: 首个 claim 免费路径命中次数必须为 `0`。
  - NFR-AGC-2: 同一 `agent_id` 的并发 claim 误放过率必须为 `0`。
  - NFR-AGC-3: `activation fee`、`upkeep`、`bond refund/slash` 与 restricted grant 发放/消费/退款/过期事件覆盖率必须为 `100%`。
  - NFR-AGC-4: viewer / pure API 在 canonical claim 字段、restricted/liquid 余额字段和 blocked reason 上的一致性必须为 `100%`。
  - NFR-AGC-5: 宽限到强制回收的检测延迟 P95 必须 `<= 1 epoch`。
  - NFR-AGC-6: 单账号 agent cap 默认不得超过 `3`，除非后续新 PRD 明确升级。
  - NFR-AGC-7: v1 的成本曲线必须保持单调不降，不允许出现“第 2 个 agent 比第 1 个更便宜”的参数。
  - NFR-AGC-8: restricted bucket 通过普通转账路径、公开资产导出路径或 slot-2/3 claim 路径的误放过率必须为 `0`。
  - NFR-AGC-9: 所有 claim 相关 token 变动必须能进入现有经济源汇审计，不得生成审计盲区。
  - NFR-AGC-10: restricted grant admin registry 的 source of truth 必须来自 runtime world-state 正式 registry，而不是文档约定、自由文本 `issuer_id` 或调用方本地分支逻辑。
  - NFR-AGC-11: restricted grant admin registry runtime update 必须可审计、可重放，并在 registry 尚未 bootstrap、`ecosystem_pool` controller slot 未配置、或提交者不匹配当前 controller account 时拒绝执行，不得退化为离线 import 才能变更的单一路径。
  - NFR-AGC-12: Viewer / pure API 对 `其他账号已绑定 Agent`、`未绑定 Agent`、`bound Agent`、`owned_claims` 和 `claim quote` 的误归属率必须为 `0`；尤其不得以世界列表顺序、默认 selection、隐藏的其他账号 Agent、未绑定 Agent、direct control API 隐式绑定或兼容 `first_agent_id` 作为当前账号 ownership 证据。
  - NFR-AGC-13: `slot-1` onboarding quote 对候选 agent 差异理由的覆盖率必须为 `100%`；至少包含初始位置、能力/当前任务、首个工业目标帮助、主要风险和推荐理由，不得把第一次重大 claim 退化为纯费用确认。
  - NFR-AGC-14: Claim Quote 对 upkeep runway 的覆盖率必须为 `100%`；任何可确认 claim 都必须能说明扣除 upfront 后可覆盖多少个完整 upkeep epoch，以及低 runway 时的推荐下一步。
- Security & Privacy:
  - claim / release / upkeep 结算不得绕过主链 token 的签名与审计路径。
  - 不在公开 UI 中暴露与 claim 无关的账户私密资产信息；只展示本次认领所需的必要成本和状态。
  - 强制回收必须基于可重放的状态与事件，不允许人工后台静默改 owner。
  - restricted starter balance 的 issuer、用途范围与过期处理必须可审计，且不得成为绕过主账本 transfer auth 的隐式旁路。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP: 冻结 `activation fee + claim bond + upkeep + release cooldown + tier cap` 的规则口径与状态机。
  - v1.1: 落地 `restricted starter claim balance`、canonical viewer / pure API 展示和 QA abuse suite。
  - v2.0: 基于真实 claim 数据评估 `slot multiplier`、`grace_epochs`、`penalty_bps` 与 starter balance 发放上限是否需要新一轮调参专题。
- Technical Risks:
  - 风险-1: 如果 claim 成本只锁 bond 不产生真实 sink，囤位问题可能仍然偏轻处罚。
  - 风险-2: 如果 upkeep 过高，会让 agent 控制变成纯惩罚，削弱组织扩张乐趣。
  - 风险-3: 如果 viewer 不清楚展示倒计时与宽限，玩家会把强制回收理解为 bug 而不是规则。
  - 风险-4: 如果 tier cap 只靠离线判断、不进 runtime，alt 账号仍可能形成事实囤积。
  - 风险-5: 如果 restricted bucket 被设计成通用不可转账 main token，而不是受限于 `slot-1 claim/upkeep` 的窄用途 bucket，会把 claim 启动补贴扩散成第二套资产语义。

## 6. Validation & Decision Record
- Test Plan & Traceability:

| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-GAME-011 | `TASK-GAME-039` | `test_tier_required` | 文档治理检查、根入口/索引/任务映射核验 | agent claim 经济专题挂载 |
| PRD-GAME-011 | `TASK-GAME-040` | `test_tier_required` + `test_tier_full` | claim / upkeep / release / forced reclaim 状态机回归、经济审计对账、并发争抢测试 | runtime 规则执行、token 记账、安全边界 |
| PRD-GAME-011 | `TASK-GAME-041` | `test_tier_required` | Viewer / pure API canonical 字段、报价展示、宽限/冷却倒计时回归 | 玩家表达层、UI/API 一致性 |
| PRD-GAME-011 | `TASK-GAME-042` | `test_tier_required` + `test_tier_full` | abuse suite、长稳回收、经济告警与不变量复核 | QA 守门、反囤积、经济审计 |
| PRD-GAME-011 | `TASK-GAME-043` | `test_tier_required` | producer 平衡复盘、调参边界与继续/回退决策回写 | 版本平衡、后续节奏裁决 |
| PRD-GAME-011 | `TASK-GAME-044` | `test_tier_required` | PRD / design / project / root doc 回写、术语一致性与追溯链核验 | restricted starter claim balance 规则入档 |
| PRD-GAME-011 | `TASK-GAME-045` | `test_tier_required` + `test_tier_full` | restricted bucket 账本、claim/upkeep/refund provenance、transfer guard、snapshot/replay 兼容回归 | runtime 账本、资金来源约束、安全边界 |
| PRD-GAME-011 | `TASK-GAME-046` | `test_tier_required` | Viewer / pure API 余额拆分、funding mix、blocked reason 与 explorer 展示回归 | 玩家表达层、资产可读性、UI/API 一致性 |
| PRD-GAME-011 | `TASK-GAME-047` | `test_tier_required` + `test_tier_full` | allowlist/QA seed 发放、slot-1/slot-2 guard、refund provenance、过期/回收与经济审计 matrix | QA 守门、反滥用、经济审计 |
| PRD-GAME-011 | `TASK-GAME-048` | `test_tier_required` | producer 对 starter balance 额度、过期与发放边界的首轮复核与继续/回退决策 | limited preview 节奏、版本平衡、运营口径 |
| PRD-GAME-011 | `TASK-GAME-049` | `test_tier_required` + `test_tier_full` | restricted grant 的 `issuance_reason / issuer_id / expires_at_epoch` 状态、issue/expire/revoke 事件、issuer-scoped 发放/回收动作与 token audit linkage 回归 | runtime grant lifecycle、经济源汇审计、资金来源约束 |
| PRD-GAME-011 | `TASK-GAME-050` | `test_tier_required` | liveops issuer 边界、allowlist/QA seed/campaign 发放口径、过期/撤销 runbook 与 incident fallback 核验 | 运营发放、对外口径、风险收敛 |
| PRD-GAME-011 | `TASK-GAME-051` | `test_tier_required` + `test_tier_full` | restricted grant lifecycle / audit matrix、expiry/revoke/source-sink 对账与 non-bypass 验证 | QA 守门、反滥用、经济审计 |
| PRD-GAME-011 | `TASK-GAME-052` | `test_tier_required` + `test_tier_full` | governance main-token controller registry 补齐 restricted grant admin registry / signer allowlist 绑定，并验证 registry 缺失、非 admin、非 allowlisted issuer 与 allowlisted admin 的 runtime action gate | runtime admin 真值、发放/撤销入口门禁、运营安全边界 |
| PRD-GAME-011 | `TASK-GAME-053` | `test_tier_required` + `test_tier_full` | 新增 `UpdateRestrictedStarterClaimAdminRegistry` controller-governed runtime action，验证 `ecosystem_pool` controller account 绑定、signer policy 约束、governance event apply 与“先更新 registry 再发 grant”的闭环 | runtime admin registry 热更新、主链钱包治理审计、运营解锁链路 |
| PRD-GAME-011 | `TASK-GAME-054` | `test_tier_required` | 新增 `oasis7_liveops_grant_cli`，封装 restricted grant 的 `issue/revoke/status` 日常操作，验证 CLI 仍复用 canonical runtime action / world-state 真值，且不开放 admin roster 直改旁路 | 运营操作降摩擦、runtime 真值复用、治理边界保持 |
| PRD-GAME-011 | `TASK-GAME-055` | `test_tier_required` | 新增 `scripts/oasis7-liveops-grant.sh` 作为运营 wrapper，验证位置参数、`OASIS7_WORLD_DIR` 默认注入与 `--print-cmd/--cli-bin` smoke，同时保持底层仍只调用 `oasis7_liveops_grant_cli` | 运营执行门槛进一步降低、字段误填率下降、治理边界不扩散 |
| PRD-GAME-011 | `TASK-GAME-056` | `test_tier_required` | 将 governance registry manifest/import/audit 扩成 per-slot threshold，验证 `liveops` 可显式声明 `1-of-2` 且通过 audit/import，而既有 controller/finality slot 继续保持默认 `2-of-3` 与单 signer 故障容忍审计 | 低权限运营槽位可正式落地、治理工具链不再全局绑死一个 threshold |
| PRD-GAME-011 | `TASK-GAME-059` | `test_tier_required` + `test_tier_full` | 新增 `TopUpRestrictedStarterClaimLiveopsPool` controller-governed runtime action、独立 `restricted_starter_claim_liveops_pool` treasury bucket 与 top-up 审计记录，验证 top-up 固定绑定 `ecosystem_pool` controller slot 的 signer allowlist / threshold policy，且 daily restricted grant 只从专用池出账/退款回流 | restricted grant 资金池分层、高权限大池审批与低权限日常发放解耦、运营口径收敛 |
| PRD-GAME-011 / PRD-WORLD_RUNTIME-040 | `task_313368c409c54cc2bcf8ef4f47919b65` | `test_tier_required` | 首个 `slot-1` claim auto-funding runtime / snapshot / API / viewer 回归，验证 dedicated pool 自动补足、旧审批路径移除与 funding provenance 保持可审计 | 新账号 onboarding、自动启动金、claim 直连闭环 |

- Decision Log:

| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-AGC-001 | 第 1 个 agent 认领也必须付费 + 锁 bond + 付 upkeep | 给首个 agent 免费资格，只对第二个起收费 | 免费首槽会直接成为 alt abuse 和零成本囤位入口，且削弱 token sink。 |
| DEC-AGC-002 | 采用 `activation fee + claim bond + upkeep` 三段式 | 只收一次性买断费 | 买断只解决入口，不解决长期占坑与闲置问题。 |
| DEC-AGC-003 | 用 `reputation_tier` 冻结 `1/2/3` 槽位上限，并让后续槽位更贵 | 只靠余额决定能占多少 agent | 只看余额会把组织控制权过度让渡给高资产账号。 |
| DEC-AGC-004 | 强制回收采用“欠费宽限 + 闲置回收”双触发 | 只在玩家手动 release 时释放 agent | 没有系统回收，agent 池会被长期冻结。 |
| DEC-AGC-005 | 先冻结状态机和不可突破边界，绝对价格留给后续平衡调参 | 现在就硬拍绝对 token 数值并直接宣称最终价格 | 当前阶段更缺结构性规则，而不是营销式定价数字。 |
| DEC-AGC-006 | 首轮 producer review 结论为“维持当前 v1 默认值，不新开调参专题” | 在缺少真实 claim 持有分布、回收率和玩家阶段数据前立即改 `slot multiplier / grace / penalty / tier cap` | 当前 runtime / viewer / QA 闭环已证明结构正确，但还没有足够平衡数据支持改价；过早调参只会引入第二套未验证默认值。 |
| DEC-AGC-007 | 在 main token 账本内新增 `restricted starter claim balance`，只作为 `slot-1 claim/upkeep` 的受限 bucket | 直接给首个 agent 免费资格；或直接空投可转账 main token | 免费会破坏“首个 claim 非免费”的规则；可转账空投会直接打开刷号和套现路径。 |
| DEC-AGC-008 | refund / reclaim 必须保留 funding-source provenance，restricted 来源的 bond 退款回 restricted bucket | 所有 bond refund 统一退回 liquid main token | 若 refund 统一回 liquid，玩家可把启动补贴通过 claim/release 洗成可转账资产。 |
| DEC-AGC-009 | restricted bucket 只允许 `slot-1 claim + slot-1 upkeep` 的窄用途，不扩成通用非流通代币 | 让 restricted bucket 覆盖任意 claim、任意 upkeep 甚至其他 main-token 动作 | 窄用途更利于 limited preview / onboarding 启动，不会意外制造第二套通用资产语义。 |
| DEC-AGC-010 | 允许 `slot-1` 使用 `restricted + liquid` 混合支付，并记录 provenance | 要么全部 restricted，要么全部 liquid | 混合支付能避免“restricted 不够一点就完全不能用”的体验断点，同时仍可通过 provenance 保证 refund 不洗钱。 |
| DEC-AGC-011 | 不收窄 `PRD-GAME-011` 对 restricted grant lifecycle / audit 的要求，并保持 starter balance 继续是 `slot-1` 专用、不可转账、可过期可撤销的受限余额 | 把当前实现退化为“只有一个数值 bucket、没有 issuer/expiry/audit 的临时补贴”并据此宣称专题完成 | QA 已证明 bucket 记账与展示闭环正确，但没有正式发放元数据、过期/撤销事件和审计链，就无法证明该补贴受治理、可回收、可复盘。 |
| DEC-AGC-012 | 在 runtime world-state 里把 restricted grant admin 收口到正式 registry，并要求每个 admin account 同时绑定现有 controller signer allowlist policy；issue/revoke 先过 admin gate，再进入 grant 业务校验 | 继续只依赖 runbook 约定 `issuer_id=liveops`；或另起一套与 controller policy 脱钩的独立 admin 配置 | `issuer_id` 只是业务字段，不足以证明操作者具备正式权限；复用既有 governance controller signer policy 能避免平行治理真值，同时把“非 admin”阻断前置到 runtime action 入口。 |
| DEC-AGC-013 | 不开放整份 controller registry 的任意热编辑，只新增一条由 `ecosystem_pool` treasury controller account 签名驱动的 `UpdateRestrictedStarterClaimAdminRegistry` 动作，专门更新 restricted grant admin 子集 | 继续只允许离线 import / 启动注入变更 registry；或把 admin roster 继续绑定到模拟内 passed proposal proposer；或开放完整 controller registry runtime 任意改写 | restricted grant admin roster 属于主链资产治理面，不应继续依赖模拟内 agent/proposal 真值；收窄到单一字段更新，并复用既有 controller slot + signer allowlist / threshold policy，能把 liveops/admin 轮换放回正式钱包治理路径，同时避免把更高风险的 controller policy / treasury slot 编辑一并放开。 |
| DEC-AGC-014 | 为运营补一层薄 CLI `oasis7_liveops_grant_cli`，只封装 `issue/revoke/status` 并继续复用底层 runtime canonical action | 继续让运营手工拼 action JSON；或让 CLI 直接编辑 world snapshot/journal；或顺手开放 admin roster 直改命令 | 问题在于日常操作摩擦过大，不在于底层规则错误；薄 CLI 能降低运营使用成本，同时保持 runtime/state/journal 真值与 controller 治理边界，不引入新旁路。 |
| DEC-AGC-015 | 在正式 CLI 之上补一层仓库脚本 `scripts/oasis7-liveops-grant.sh`，把 world-dir/issuer 缺省、位置参数与常用命令收口，但最终仍只转发到 `oasis7_liveops_grant_cli` | 继续要求运营直接敲长 `cargo run` 命令；或把更多运营逻辑复制进第二套脚本状态机 | 当前痛点已经从“没有 CLI”变成“正式 CLI 仍太长、太像开发命令”；薄 wrapper 可以减少误操作与培训成本，但不能复制第二份业务规则或引入脚本直改 world 的捷径。 |
| DEC-AGC-016 | 允许 `liveops` 这类 restricted grant admin 低权限 slot 在 manifest 中显式声明 `threshold=1`，并以 `1-of-2` signer policy 进入 governance registry；其余 treasury/controller 主槽位继续默认 `2-of-3` | 要么强迫 `liveops` 也走统一 `2-of-3`，抬高运营摩擦；要么直接把所有 controller slot 的 threshold 一起降到 `1` 或 `1-of-2` | `liveops` 只负责不可转账 restricted grant 的日常发放/撤销，权限面明显低于 treasury/controller 主槽位；把 threshold 下调收口到单独 slot，既能降运营 ceremony 成本，又不把更高风险的主槽位一起放宽。 |
| DEC-AGC-017 | 将 daily restricted grant 的 source bucket 从 `ecosystem_pool` 拆成独立 `restricted_starter_claim_liveops_pool`，并只开放一条由 `ecosystem_pool` controller-governed top-up action 负责向该池补款 | 继续让日常发放直接动 `ecosystem_pool`；或把一部分 liquid treasury 先转到 `liveops` 账户；或直接泛化成任意 treasury-to-treasury 转账框架 | 运营需要把高风险大池审批和低权限日常发放明确分层；专用池 + 固定 top-up action 既保留 `ecosystem_pool` 的 `2-of-3` 审批门槛，又不把 `liveops` 变成 liquid treasury 分发者，同时避免当前切片过早扩成通用 bucket 间转账框架。 |
| DEC-AGC-018 | 首个 `slot-1` 启动金默认走 dedicated pool 自动补足，而不是额外运营审批；自动补足只覆盖首个 claim 的 upfront 缺口，不把已有显式 grant/balance 再次叠加成重复补贴 | 保留 `approval request -> operator review -> approve/reject` 作为首笔启动 OC 的必经路径；或把自动补足扩成“只要有 dedicated pool 就总是替玩家垫付全部 slot-1 成本” | 用户要求把日常首笔启动 OC 改成无需审批；但若对已有显式 grant/balance 继续自动叠加，会把一次性启动资助放大为隐式重复补贴，因此自动路径只覆盖“无现成 restricted starter 余额/grant 的首个 slot-1 缺口”。 |
| DEC-AGC-019 | 将默认 Agent 控制列表限定为当前账号绑定或当前玩家绑定待同步 Agent；其他账号已绑定 Agent 与未绑定 Agent 默认不展示在控制列表中 | 把世界里第一个 Agent 继续当作当前账号的 implicit primary Agent；或把其他账号/未绑定 Agent 混进“我的 Agent”列表再用 badge 解释；或把 `player_id` 直接等同于 `claim_owner_id` | 换账号后的第一屏应服务“我的下一步”，不是调试 shared-world 全量实体。隐藏其他账号与未绑定 Agent 可以避免 ownership 误读；共享世界活动感后续应通过地图实体、世界事件或只读 world layer 呈现，不能借 Agent 控制列表补。 |
| DEC-AGC-020 | 将首个 `slot-1` claim 从“成本确认”提升为“候选 agent 路线选择”，在 canonical quote 中加入候选差异理由 | 只展示费用、余额、cap、cooldown，让玩家先付出承诺成本再自己猜 agent 用途 | 2026-07-08 gameplay slice 指出首个 claim 是玩家第一次带成本的重大承诺；缺少候选差异会削弱路线判断、角色代入和早期 ownership。 |
| DEC-AGC-021 | 将 claim quote 从“能否支付 upfront”扩展为“能否持续持有”的 runway 判断 | 只展示 activation fee、bond、upkeep 与 grace/cooldown 倒计时 | upkeep 是持续占有成本；玩家需要在确认前知道当前余额还能撑几轮，以及是否应该先补资金、等待或降低扩张节奏。 |

### 6.1 首聊价值预览的独立验收

以下是设计目标的验收场景；既有文案切片的验证见配对设计 [11.2 节](gameplay-agent-claim-economy-contract.design.md#first-chat-validation-matrix)，service 后继协议见 [11.3 节](gameplay-agent-claim-economy-contract.design.md#claim-entry-service-validation-matrix)。本节不记录某个版本已通过。只有适用权威事实参与 gate 判断，未取得的事实显式保持未知；同一预览的显示一致性不能替代真实 service 输入与 gate 的独立验证。

<a id="ac-agc-chat-001"></a>
#### AC-AGC-CHAT-001：OR gate 与资金边界

- 覆盖要求：[REQ-AGC-CHAT-001](#req-agc-chat-001)。
- 给定：同一有控制权 Agent 的适用权威余额与领取事实，覆盖零/正余额、有/无记录，以及各输入未知、裁剪、失效的组合。
- 当：玩家查看首聊资源边界、刷新或返回该入口。
- 则：适用权威输入中，任一 OR 分支已知为真即通过该 gate，即使另一分支未知；只有两分支均已知为假才受阻，其余为 unknown 并可刷新/补证。记录保留时余额耗尽不重锁，缺字段不按零；主体/世界/版本不匹配的输入先排除。其他授权/provider blocker 独立判断，提交时重验。本地完整状态与真实 service 分别验证，后者不得用前者代签。

<a id="ac-agc-chat-002"></a>
#### AC-AGC-CHAT-002：注册、经济认领和未知状态

- 覆盖要求：[REQ-AGC-CHAT-002](#req-agc-chat-002)。
- 给定：已注册/绑定但权威事实确认没有经济 claim、真实经济 claim 已存在，以及只收到旧版无条件维护文案或缺失义务说明的样本。
- 当：玩家查看预览并选择延后首次对话。
- 则：第一种不虚构 upkeep，第二种如实保留原有维护规则，第三种明确未知并引导复核；三者均不因预览或延后动作新增费用、不取消已有义务，也不把认领报价当成认领已生效。原经济状态机随时间发生的合法结算继续适用。

<a id="ac-agc-chat-003"></a>
#### AC-AGC-CHAT-003：语言、入口与旧资料一致性

- 覆盖要求：[REQ-AGC-CHAT-003](#req-agc-chat-003)。
- 给定：同一权威预览的 Viewer 中英文、pure API 与获准 Agent 读面，以及旧版文案/DTO、缺失说明、新预览、断线和身份切换样本。
- 当：切换语言或入口、回读旧资料、重新连接或刷新权威状态。
- 则：当前 gate、义务、未知原因和下一步同义；无权或未实现的读面明确 unavailable，不泄露他人资格。旧值不得跨身份复用，旧 DTO/缺失说明不被转写成零余额或无责任；恢复当前合法投影后才重新提供适用动作。语言兼容通过仅证明显示含义，真实 service 输入、最终提交及跨入口闭环仍需独立验证。
