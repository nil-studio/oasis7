# 战争与政治机制数值基线

审计轮次: 5

本页是战争与政治成本、收益、冷却、评分和反支配风险的稳定玩法专业数值权威；
产品层只承诺玩家结果与跨域边界，实现锚点与专业验证仍由 gameplay、runtime
和测试域承载。

所属产品模块：[`世界规则与玩法系统`](../../product/world-rules-core-gameplay/prd.md)。

产品边界入口：[`区域冲突、软赛季与可恢复损失`](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；本页保留战争/政治的成本、收益、冷却与反支配数值权威。

## 目标

- 为 Gameplay 战争与政治玩法提供可执行、可验证、可回归的首轮数值基线。
- 明确“成本 / 收益 / 冷却”三类约束，避免进入单一最优策略或无反馈状态。
- 对齐当前 runtime + m5 gameplay wasm 实现，减少设计与实现口径偏差。

## 范围

### In Scope
- 战争：宣战门槛、强度范围、结算时长、胜负评分、重入冷却约束。
- 政治：提案窗口、法定人数、通过阈值、投票重投规则、提案重试节奏。
- 与实现的常量映射和测试入口。

### Out of Scope
- 大规模平衡性调优（多赛季、跨服差异化参数）。
- 联盟外交声望系统与跨服政治机制。
- UI 层可视化和引导文案优化。

## 接口/数据

- 动作入口：
  - `Action::DeclareWar`
  - `Action::OpenGovernanceProposal`
  - `Action::CastGovernanceVote`
- 事件出口：
  - `DomainEvent::WarDeclared`
  - `DomainEvent::WarConcluded`
  - `DomainEvent::GovernanceProposalOpened`
  - `DomainEvent::GovernanceVoteCast`
  - `DomainEvent::GovernanceProposalFinalized`

## 里程碑

- B1：基线冻结（本文档）。
- B2：按基线运行 1 周回放，输出偏差报告（冲突频次、治理参与度）。
- B3：仅在偏差超过阈值时进入第二轮调参（保持小步迭代）。

## 风险

- 若战强度奖励过高，可能诱发“无脑高强度宣战”。
- 若治理窗口过长，政治反馈滞后导致玩家体感“决策无效”。
- 若提案阈值过高，治理参与会收敛到少数头部玩家。

---

## 1. 战争数值基线（MVP）

| 维度 | 基线值 | 成本/收益/冷却含义 | 实现锚点 |
|---|---|---|---|
| 宣战强度 `intensity` | `1..=10` | 强度提高进攻评分与结算变化幅度，也增加宣战资源成本、冲突时长及 m5 模块疲劳 | `crates/oasis7_runtime/src/world/event_processing.rs`、`crates/oasis7_builtin_wasm_modules/m5_gameplay_war_core/src/lib.rs` |
| 宣战动员成本 | 电力 `12 + 4 * intensity`；数据 `8 + 3 * intensity` | 由进攻方在宣战时支付，形成随强度增长的前置投入 | `crates/oasis7_runtime/src/world/event_processing.rs` |
| 战争持续时长 | `6 + 2 * intensity` ticks | 形成显式投入成本（占用冲突窗口） | `crates/oasis7_runtime/src/state.rs`、`crates/oasis7_builtin_wasm_modules/m5_gameplay_war_core/src/lib.rs` |
| 战争结算评分 | core fallback：`members*10 + reputation/10`，进攻方再加 `intensity`；m5 模块再减 `fatigue/4` | 推荐强度必须使用当前激活结算路径的完整评分项，不能只看成员数 | `crates/oasis7_runtime/src/world/gameplay_loop.rs`、`crates/oasis7_builtin_wasm_modules/m5_gameplay_war_core/src/lib.rs` |
| 胜负判定 | `aggressor_score >= defender_score` 时进攻方胜 | 平分时进攻方胜，鼓励主动冲突但保留成员规模价值 | `crates/oasis7_runtime/src/world/gameplay_loop.rs` |
| 同对联盟重入 | 同一联盟对在 active 期间不可重复宣战 | 作为首轮“冲突冷却”约束，避免刷宣战事件 | `crates/oasis7_runtime/src/world/event_processing.rs` |

### 1.1 战争推荐操作区间（用于评审）

当前评分和平分规则下，`minimum_winning_intensity` 表示“在当前预览快照与激活结算路径中，达到目标胜负结果的最低强度”，而不是通用强度带。

预览必须先计算不含进攻强度的双方基础评分：

- core fallback：`10 * members + reputation / 10`。
- m5 战争模块：`10 * members + reputation / 10 - fatigue / 4`。
- 整数除法按实现截断；进攻方在基础分之上再加 `intensity`，平分时进攻方胜出。

在 `1..=10` 中选择能使进攻总分不低于防守总分的最小值；若不存在，推荐补强、谈判或等待。成员数相同且双方声望、有效疲劳也相同时，强度 `1` 是最低胜利强度；这只是等条件示例，不是仅由成员差推导的通用结论。

- 高于最低胜利强度会增加动员资源、冲突占用时间、声望/资源结算幅度及 m5 模块疲劳；玩家必须同时看到这些变化，而不能把“更容易达成胜判”等同于“更高净收益”。
- 若连续 3 天 `conflict_freq_100ticks > 8`，先检查最低胜利强度的重复宣战是否构成刷取路径；本基线不通过调整推荐带代替数值平衡。

### 1.2 战争收益说明

- 当前实现中的状态与叙事收益包括：`winner_alliance_id`、战报摘要、冲突历史沉淀。
- 当前 core fallback 还会把败方实际扣除的电力与 Data 汇总为战利品，按排序后的胜方成员均分，余数依成员顺序逐一分配；没有胜方成员时不分配。该内置资源结算与 gameplay 模块 directive 提供的额外经济/元进度奖励分开；可选模块奖励不属于内核保证。本段记录当前实现，不新增或变更经济规则。
- 当前胜方每个成员获得 `2 * intensity` 声望，败方每个成员失去 `3 * intensity` 声望；败方名义资源损失为电力 `6 * intensity`、数据 `4 * intensity`。core fallback 会按现有非负余额限制实际损失，m5 模块则发出完整名义变化，因此预览必须标识激活的结算路径。
- 预览中的 `expected_narrative_or_module_reward` 必须区分当前结算路径的胜负/历史结果、core fallback 资源战利品与可选模块奖励；未获得模块回执时，不得把可选奖励显示为保证收益。

### 1.3 当前平衡边界

当前基线仍存在“当预览快照已冻结时，选择最低胜利强度”的局部支配策略；但胜利阈值同时受成员、声望及模块疲劳影响，不能化约为固定成员差表。未来若受控重启战争主线，必须通过独立平衡任务验证动员成本、结算幅度、疲劳和平分优势是否形成有意义的风险收益选择。本轮仅纠正文档口径，不完成数值重平衡。

### 1.4 宣战后果预览

当玩家准备 `DeclareWar`、调整强度、暂缓或先谈判时，玩家侧必须能读取 `war_declaration_quote` / `conflict_outcome_preview`，用于判断当前宣战是否值得、强度是否合适、冲突窗口会被占用多久，以及结算后果是否优于替代行动。

最小字段：
- `actor_alliance_id`
- `target_alliance_id`
- `action_kind`: `declare_war` / `change_intensity` / `defer` / `negotiate_first`
- `intensity`
- `settlement_path`: `core_fallback` / `m5_gameplay_war_core`
- `minimum_winning_intensity: u32 | null`：按 `settlement_path` 对应的当前成员、聚合声望、模块疲劳与平分规则推导的最低胜利强度；无可达强度时返回 `null`，并由 `recommended_war_action` 返回“补强/谈判/等待”
- `war_duration_ticks`
- `aggressor_score_estimate`
- `defender_score_estimate`
- `likely_winner_before_action`
- `victory_margin_estimate`
- `conflict_window_blocked_until`
- `reentry_cooldown_or_active_conflict_blocker`
- `expected_narrative_or_module_reward`
- `settlement_risk`
- `alternative_action`: `negotiate` / `recruit` / `wait` / `governance_proposal`
- `recommended_war_action`
- `why_this_war_is_worth_or_risky`

Edge case: 当玩家准备 `DeclareWar` 或调整强度，但看不到激活结算路径、预计胜负、持续时间、冲突占用、结算风险、最低胜利强度或替代行动理由时，标记为 `war_declaration_quote_missing`。

Acceptance: 玩家宣战前至少能看懂“为什么现在打、打谁、用多大强度、会持续多久、大概能不能赢、输了/赢了会发生什么、是否应该先谈判或补强”。

Non-goal: 本预览不重平衡战争强度、评分公式、持续时长或胜负判定，不新增完整战斗模拟、战术地图、外交谈判系统或复杂联盟声望系统，不改变 `DeclareWar` / `WarDeclared` / `WarConcluded` runtime ABI，也不把战争收益硬编码进经济或元进度结算。

---

## 2. 政治数值基线（MVP）

| 维度 | 基线值 | 成本/收益/冷却含义 | 实现锚点 |
|---|---|---|---|
| 投票窗口 `voting_window_ticks` | `1..=1440` | 窗口越长，参与覆盖更高，但反馈延迟更大 | `crates/oasis7_runtime/src/world/event_processing.rs` |
| 通过阈值 `pass_threshold_bps` | `5000..=10000` | 阈值越高，提案稳定性越高，但通过成本更高 | `crates/oasis7_runtime/src/world/event_processing.rs` |
| 法定人数 `quorum_weight` | `> 0` | 避免“零参与通过”，保证最小治理成本 | `crates/oasis7_runtime/src/world/event_processing.rs` |
| 选项数 | 至少 2 个唯一选项 | 防止伪提案，确保存在真实选择 | `crates/oasis7_runtime/src/world/event_processing.rs` |
| 重投规则 | 同一投票者可重投，后票覆盖前票 | 允许策略更新，但保持单人单权重口径 | `crates/oasis7_runtime/src/state.rs`、`crates/oasis7_builtin_wasm_modules/m5_gameplay_governance_council/src/lib.rs` |
| 过期处理 | `now > closes_at` 的投票拒绝；到期自动结算 | 显式治理冷却边界，避免无限拖延 | `crates/oasis7_runtime/src/world/event_processing.rs`、`crates/oasis7_runtime/src/world/gameplay_loop.rs` |

### 2.1 政治推荐参数模板（首轮）

- 常规经济提案：
  - `voting_window_ticks = 24..72`
  - `quorum_weight >= 3`
  - `pass_threshold_bps = 6000`
- 制度/战争相关提案：
  - `voting_window_ticks = 48..120`
  - `quorum_weight >= 5`
  - `pass_threshold_bps = 7000..8000`

### 2.2 提案冷却建议（流程约束）

- 由于 `proposal_key` 全局唯一，建议采用 `proposal.<topic>.<epoch>` 命名。
- 同主题提案建议最短重提间隔 `>= 48 ticks`（流程约束，后续可实现为 runtime 硬约束）。

### 2.3 治理投票结果预览

当玩家准备发起提案、投票、改票、等待或暂缓时，玩家侧必须能读取 `governance_vote_quote` / `proposal_outcome_preview`，用于判断当前票局、自己行动是否关键、以及通过/失败会带来什么变化。

最小字段：
- `proposal_id`
- `proposal_topic`
- `actor_id`
- `action_kind`: `open_proposal` / `cast_vote` / `change_vote` / `wait` / `defer`
- `closes_at_tick`
- `ticks_remaining`
- `current_quorum_weight`
- `required_quorum_weight`
- `current_pass_bps`
- `required_pass_bps`
- `actor_vote_weight`
- `vote_swing_potential`
- `likely_outcome_before_action`
- `likely_outcome_after_action`
- `affected_rule_or_priority`
- `world_change_if_passed`
- `cost_or_cooldown_if_failed`
- `recommended_governance_action`
- `why_this_vote_matters`

Edge case: 当玩家准备发起提案、投票或改票，但看不到剩余时间、quorum/pass 缺口、自己票权影响、通过后的世界变化或失败/过期代价时，标记为 `governance_vote_quote_missing`。

Acceptance: 玩家在发起提案或投票前，至少能看懂“这项提案还差多少能成立/通过、我的动作会不会改变结果、通过后具体改变什么、失败或暂缓有什么代价、现在应该投/改票/拉人/等待的理由”。

Non-goal: 本预览不重平衡治理阈值、投票窗口、票权公式或冷却时间，不新增复杂议会 UI、拉票系统、外交谈判系统或全局政治模拟，不改变 `OpenGovernanceProposal` / `CastGovernanceVote` runtime ABI，也不把治理结果硬绑定到经济定价或社会声誉分。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [`REQ-WR-CC-001`](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001) Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | 战争推荐与后果预览必须保持在已声明冲突授权、成本和恢复边界内。 | [`DES-WAR-POLITICS-001`](#des-war-politics-001) | `gameplay_designer` / `runtime_engineer` | 不重平衡战争参数或改变 runtime ABI。 |
| [`REQ-WR-GCB-001`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001) Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b | 治理提案、票权、法定人数和通过阈值必须遵守普通治理白名单边界。 | [`DES-WAR-POLITICS-002`](#des-war-politics-002) | `gameplay_designer` / `runtime_engineer` | 不新增复杂议会、外交或全局政治模拟。 |

<a id="des-war-politics-001"></a>
### DES-WAR-POLITICS-001：战争成本与结果预览承接

战争数值基线、最低胜利强度和结算风险预览承接 `REQ-WR-CC-001` 的冲突授权与可恢复损失边界；实现锚点和测试结果仍由专业实现与 QA 证据拥有。

<a id="des-war-politics-002"></a>
### DES-WAR-POLITICS-002：治理参数与普通治理边界承接

政治数值基线、投票窗口、法定人数和通过阈值承接 `REQ-WR-GCB-001` 的普通治理白名单边界；本设计不扩大治理权限或修改 runtime ABI。

---

## 3. 回归测试入口

- 基线动作协议与状态闭环：
  - `env -u RUSTC_WRAPPER cargo test -p oasis7 runtime::tests::gameplay_protocol:: -- --nocapture`
- 模块驱动结算链路：
  - `env -u RUSTC_WRAPPER cargo test -p oasis7 runtime::tests::gameplay_protocol::step_with_modules_applies_gameplay_directive_emits_to_domain_events -- --nocapture`
- 全量 required-tier 门禁：
  - `./scripts/ci-tests.sh required`

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [`REQ-WR-CC-001`](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001) Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | [`DES-WAR-POLITICS-001`](#des-war-politics-001) | 验证战争预览保留授权边界、成本、胜负与恢复风险的可读关系。 | [`war declaration quote tests`](../../../crates/oasis7_runtime/src/tests/war_declaration_quote.rs) | PR 与实际验证记录 / QA result | 不证明跨服平衡或生产发布。 |
| [`REQ-WR-GCB-001`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001) Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b | [`DES-WAR-POLITICS-002`](#des-war-politics-002) | 验证治理预览保留法定人数、通过阈值、票权影响与失败/过期代价。 | [`governance vote quote tests`](../../../crates/oasis7/src/viewer/runtime_live/tests/governance_vote_quote.rs) | PR 与实际验证记录 / QA result | 不证明完整政治模拟或最终平衡。 |
