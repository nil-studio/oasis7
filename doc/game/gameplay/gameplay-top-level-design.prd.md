# 游戏可玩性设计总纲 v0.1  

- 对应设计文档: `doc/game/gameplay/gameplay-top-level-design.design.md`
- 可变执行状态: 对应 GitHub Project task 与 issue evidence comments

审计轮次: 4

## 当前交付切线

当前唯一代表性链采用 `starter-industrial-smelter-to-assembler-v1` / `production_only`：合法地点建成 `factory.smelter.mk1`，以 `recipe.smelter.iron_ingot` 完成合法 cycle，在该 Smelter 的 owner-bound output ledger 形成首笔匹配正量铁锭结算，再开放 Assembler 候选。排程、资源扣减、入列和普通库存变化不能替代结算；首产物不要求建成 Assembler，不代签稳定窗口、交付或需求满足。实际可用性由同候选运行证据判定。

10 分钟信任窗口、首产物结算、15–45 分钟或 1–3 局持续能力窗口、单次会话长度和长期成长周期分别命名与评价；时间用于观察节奏，不要求玩家凑足时长。


## 文档 authority 与适用范围
- 本文件是核心玩法骨架及 `PRD-GAME-012` early-retention 专业合同的 topic authority：定义跨循环体验脊柱、早期体验判断和本专题的可玩性验收。
- 产品承诺入口：[`首局与持续游玩`](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)、[`可玩性证据与承诺边界`](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)；本文保留玩法骨架与专业验收，不复制产品承诺。
- `doc/game/prd.md` 是 game 模块的活跃基线与路由根入口，拥有 PRD-ID、默认首读路径和当前状态指针；本文件不发布模块级状态，也不取得 gameplay 的模块级 authority。
- 其他 gameplay 专题在其声明范围内拥有详细合同；本文件只在上述玩法骨架/`PRD-GAME-012` 范围内优先，不能覆盖 agent claim、agency、区域设施等专题，亦不能覆盖 `doc/product/` 四模块的产品承诺。
- 历史 ROUND-002 的主从表述仅是历史治理记录，不再用于判断现行 authority。

## —— 让玩家快乐，并愿意长期沉浸的系统设计

> 本文档目标：  
> 在宏大的 AI 文明模拟与区块链架构之上，建立一个真正“好玩”的游戏体验结构。  
> 核心目标不是技术先进，而是：**玩家快乐、持续上线、形成社交与情绪投入。**

---

# 0. 设计摘要（必备字段）

## 0.1 目标

- 建立可长期运营的可玩性框架，让玩家在日常决策中持续获得反馈与成就感。
- 将“战略、工业、经济、协作、治理”整合为统一体验曲线，避免系统堆砌但不好玩。
- 为后续工程落地（WASM 模块化玩法层）提供玩家体验约束与验证基准。

## 0.2 范围

### In Scope
- 玩家体验目标与动机模型。
- 微循环/中循环/长循环的节奏设计。
- 工业、协作与治理的玩法原则与平衡约束。
- 新手前 30 天体验路径与长期沉浸机制。

### Out of Scope
- 具体链上协议实现细节与共识算法实现。
- 底层物理内核/渲染系统/网络协议的代码级设计。
- 数值表、产消参数、伤害公式等实现期调优细节。

## 0.3 接口/数据

- 玩家输入接口：目标设定、提示词注入、治理投票、协作协议、资源分配决策。
- 世界反馈接口：事件日志、组织状态、资源/能源状态、供给变化、提案状态。
- 核心体验数据：
  - 玩家成长数据（组织规模、治理权重、区域控制度）。
  - 工业成长数据（原料/半成品/制成品库存、产线稳定度、工厂开工率、停机原因分类）。
  - 协作数据（协议状态、供给关系、关键协作历史）。
  - 社交治理数据（组织关系、提案通过率、协作调整事件）。

## 0.4 里程碑

- M0（设计冻结）：完成可玩性顶层目标、范围、风险定义。
- M1（原型验证）：闭环验证微循环可玩性（5~15 分钟内有有效决策和反馈）。
- M2（中期验证）：完成工业/协作/治理中循环验证（1~3 天内可达阶段成果）。
- M3（长期验证）：验证可恢复能力、区域服务与有限可审计影响的持续成长；文明尺度项目仅作为可选共同扩展（数周级别持续投入）。

## 0.5 风险

- 规则复杂度过高导致新手理解成本过大，首周流失升高。
- 宏系统过早曝光，可能压过工业与控制感主线，削弱早期留存。
- 政治系统若缺乏反制机制，可能出现长期垄断并降低参与感。
- 长期目标若反馈过慢，会导致“努力无感”，降低回访率。
- 如果前期工业成长只表现为抽象数值上涨，而没有“首个制成品/工厂运转”的可见成果，新手会难以确认自己是否真正推动了世界。
- 如果一次间接控制可以长时间停留在“已接受但无主因果、无 fallback、无升级建议”的灰区，玩家会快速退化为被动旁观者。

---

## 0.6 历史 closure 归并后的稳定边界

- 玩家动作必须经过 canonical authority、资源、时间和权限约束；规则可通过受限模块扩展，但不能借扩展绕过这些边界，也不由历史 architecture/closure 文档推导任意 community mode 承诺。
- Micro-loop 的稳定可读性链是：`supported action -> accepted/rejected -> progress/blocker -> readable consequence -> next/recovery`。视觉证据必须来自真实 player surface，包含 action-to-visible-state、console-clean interaction、截图和专业 visual review；一次性 checklist、template、handoff 或 round evidence 只保留在 Git history / PR 与实际验证记录，不能晋升为 release/QA pass。
- Runtime/WASM refactor 不扩张玩家能力承诺。Gameplay module 的 manifest、ABI、权限、计量、identity、install/upgrade/disable、执行失败与 replay 由 world-runtime/WASM 专业权威定义；runtime readiness 不是玩法可玩性或后果可读性验收。
- 发布或恢复期间，受影响玩家动作必须得到可见处置：保留、replay，或带恢复路径的拒绝，禁止静默丢失。当前发布、长稳、rollback 和 incident 规则只能引用 testing/runtime/P2P/ops 的现行权威，不复用历史 `pass` / `Go` 结论。
- 战争与政治的数值、评分、成本、冷却和反支配风险由专业权威 `doc/game/gameplay/gameplay-war-politics-mvp-baseline.design.md` 承载；产品树只保留玩家结果与跨域承诺。战争仍不是当前 player-facing 主线，任何重启都需要独立平衡与玩家证据。

# 第一部分：可玩性方法论

<a id="11-可玩性的底层公式"></a>
## 1.1 核心体验假设

核心体验假设是：玩家能理解目标与约束，作出有后果的决定，看见可归因成果，并选择继续、改道或自然离场。目标清晰、反馈可信与真实选择是共同检查项；社交博弈、未知信息、危机强度和规模扩张是可组合的偏好，不要求每个玩家同时喜欢。

这些是待体验证据验证的设计假设，不是能够预测所有玩家流失的公式。独立建造、谨慎经营或观察 Agent 也可有价值；不参与竞争不等于免除资源、权限和已承诺义务。

---

## 1.2 三层游戏循环（Game Loop）

### 一、微循环（5~15 分钟）

- 查看世界变化
- 查看库存、制成品与工厂状态
- 阅读 Agent 行为日志
- 发现问题/机会
- 下达新目标
- 预测结果
- 等待反馈

设计要求：
- 每次上线都有“变化”
- 有可调整的决策空间
- 有即时风险与机会
- 能明确知道产线是否在推进、哪里停机、下一步该修什么
- 任一关键意图在一个 bounded-response 窗口内都必须被归类为 `执行中 / 被阻塞 / 被改道 / 已完成但无有效进展 / 已完成并形成进展` 之一，不允许长时间停留在“像是在做事但我不知道发生了什么”。
- 如果当前意图没有带来有效推进，系统必须给出最小 next-decision surface：`继续等待`、`修复阻塞`、`改道到次优动作`、`没有安全 fallback，重新定目标或升级约束` 四者至少其一；若进入 `fallback_ready`，还必须展示 `fallback_tradeoff_preview`，说明 `blocked_intent_id`、`blocker_reason`、`wait_option`、`repair_option`、`reroute_option`、`progress_kept`、`opportunity_cost`、`recommended_fallback_action_id`、`fallback_value_class`，并把推荐分类为 `safe_wait / repair_now / reroute_now / no_safe_fallback`。
- `safe_wait` 不能只是一条“继续等待”的标签；它必须附带 `wait_resolution_quote`，最小字段为 `resolution_trigger`、`expected_wait_class`、`next_recheck_tick_or_event`、`state_change_expected`、`wait_risk_if_unresolved`、`alternate_action_unlock_condition`。只有当 `resolution_trigger` 来自 canonical world/runtime truth，且当前替代动作不可用或其已披露机会成本更差时，系统才可推荐 `safe_wait`；若不存在有界触发条件，或到了 `next_recheck_tick_or_event` 仍未发生预期状态变化，则必须重新分类为 `repair_now / reroute_now / no_safe_fallback`，不得静默延长等待，也不得伪造精确 ETA。
- `no_safe_fallback` 是透明的终止分类，不是无动作死路；它必须提供 `no_safe_fallback_reason`、`required_next_decision_action_id` 与 `required_next_decision_class`，其中下一决策仅允许 `reprioritize_goal / escalate_constraint / return_to_goal_selection`，让玩家明确结束当前 blocked intent 并进入新的可操作决策面。
- 该 fallback 合同只要求玩家能比较等待、修复、改道的最小取舍，并能回答“在等什么、何时复查、何时换方案”；不扩展为完整恢复系统、quest tree、respec 系统或 UI 重写。
- 如果玩家/API/agent 请求当前未开放的细粒度物理动作（例如挖掘、放块、跳跃、攻击或 local physics），不能只返回 `unsupported_action`；必须展示 `fine_grain_action_translation`，用 `show / compare / classify / recommend` 说明当前动作粒度边界、最接近的间接控制目标和下一步。最小字段为 `requested_granularity`、`why_fine_action_deferred`、`canonical_replacement_action`、`closest_playable_goal`、`player_next_step_hint`、`replacement_value_class`，分类固定为 `replacement_available / no_safe_replacement / future_embodied_candidate`；该合同不开放 block editing、直接具身控制、完整 3D UI 或 runtime 物理实现。

---

### 二、中循环（1~3 天）

- 做出首个制成品
- 建成首条稳定生产链
- 落成首座工厂单元
- 推动一项组织协议通过
- 完成一次协作协议调整
- 完成一次治理或供给取舍

设计要求：
- 有阶段性成果
- 有清晰可见进展
- 有阶段奖励或影响力提升

---

### 三、长循环（数周）

- 建立并守住可恢复的区域能力
- 把能力转为区域服务、局部协调位置或新的可执行选择
- 在自愿协作中参与组织、协议或治理项目

设计要求：
- 每个长期阶段成果必须有完成边界、可归因后果和下一方向，而不是把世界设为通关目标。
- 新选择、恢复弹性、局部协调位置或区域用途才构成成长；只有库存、吞吐或重复次数增加时标记为 `grind_only`。
- 文明尺度项目是受影响参与者可选择的共同扩展，不能成为独立成长的默认前置、唯一有效路线或全体玩家终局。

---

# 第二部分：玩家体验设计文档

## 2.1 玩家角色定位

玩家的主要角色是文明的战略引导者：决定目标、取舍与当前支持的约束，再由受授权 Agent 代办已接入步骤。战略引导不等于禁止所有手动操作，也不等于任意目标都可以执行。

当前正式玩家入口已经支持的建设、转移、排产等动作，可按其实际能力与权限由玩家提交；未接入代办的步骤保留明确的玩家操作基线。打断、重排与恢复的可用性遵循[间接控制合同](gameplay-indirect-control-agency-contract.prd.md)，不从本节推导新的动作或权限。

提示词、模块开发和组织调整只在对应玩家能力真实开放时出现；operator/debug 或开发能力不是新人必经路径。不支持的动作应说明边界并指向当前可做的替代，不以“战略引导者”一句话掩盖入口缺口。

---

## 2.2 玩家核心动机

1. 胜任与成长：做成一条可理解、可恢复的生产链，不只扩大组织。
2. 创造与精进：优化布局、办法或服务，在当前开放能力内表达自己的方案。
3. 关照与归属：关心一个受支持的 Agent、设施或协作承诺的后果，不预设新增人格/情感模拟。
4. 自主与影响：选择规模、风险、独立经营或自愿协作，明确自己改变了什么。
5. 探索与博弈：对愿意参与者提供新环境、交易和治理取舍，而非所有人的必经终局。

这些动机可组合、可变化，不按上线天数分配固定职业；规模、权力或竞争不是唯一有效成长。

---

## 2.3 玩家每日关注点

- 我的能源是否稳定？
- 我的制成品产线有没有断料或停机？
- 是否有人渗透？
- 是否要扩张？
- 是否需要谈判？
- 是否要推动治理提案？
- `PublishSocialFact` / `DeclareSocialEdge` / `ChallengeSocialFact` 若影响谈判、合作、黑名单、治理或 claim 表面，必须展示 `social_fact_impact_quote` / `relationship_consequence_preview`：影响对象、可见社交表面、合作机会变化、争议/stake 风险、治理/claim 关联和推荐社交动作；玩家不应只看到事实 ID、关系边或事件日志。

---

## 2.4 情绪节奏设计

进入 → 理解变化与当前目标  
决策 → 比较真实取舍  
执行 → 观察可解释进展  
结果 → 获得成果或理解损失  
修正/离场 → 保留掌控感与下一方向

紧张、惊喜或遗憾可以自然出现，但焦虑、复仇和持续在线不是必需情绪或验收目标；平静地完成一个有限目标也可以成立。

---

## 2.5 前期工业引导成就闭环

前期引导不应该围绕“造了一个建筑”，而应该围绕“建立了持续运转的经营与生产能力”。

在当前世界观下，玩家作为文明的战略引导者，最早期、最可感知的一组成就应围绕工业成长展开：

1. 首个制成品：首次跑通“原料 -> 加工 -> 产出”，证明玩家已经让世界发生可见变化。
2. 首条稳定生产链：连续多个 tick 保持产出，不因缺电、缺料或物流阻塞中断。
3. 首座工厂单元：把一次性加工升级为持续生产能力，形成长期产能。
4. 首个可交易工业品：第一次感受到分工、交换与协作的价值。
5. 首个受保护工业节点：第一次为产能配置治理、维护或协作保障。

设计要求：
- 每个里程碑都必须有世界状态变化与 Viewer 可见反馈。
- 反馈必须区分 `已接受`、`执行中`、`已产出`、`停机/阻塞` 四类状态。
- 停机必须给出最小原因分类：缺电、缺料、物流阻塞、治理限制、危机或维护窗口影响。
- 奖励应优先体现“能力解锁/组织成长”，而非纯数值奖励。
- 工业里程碑应通向有实际用途的持续经营选择；协作、治理与扩张是可选方向，不能作为首产后强制入会的单一路径。独立经营仍承担真实供给、维护和危机后果。
- 独立经营的反例路径：首产后先核对当前真实用途，第二次决定将可用铁锭投入已可达的后继准备或继续服务原用途，第三次根据实际供给、输出容量与已承担义务决定补料、调整计划或停止新增执行。只有一条合法路线时如实说明；没有安全恢复时披露损失与改目标边界。停止不暂停世界、不取消未知提交，也不免费保住资产。该路径不需要先证明交易、加入组织或完成治理。
- 工业成长反馈必须优先展示“新能力 / 新选择 / 新修复手段 / 新区域用途”，而不是只展示库存与产量上涨。
- 首个制成品、首条稳定产线与首个可交易工业品都必须绑定最小经济可读性：玩家应能看懂 `投入了什么 / 产出了什么 / 为什么值得继续 / 下一步可换来什么`。
- `首个可交易工业品` 不能只以库存增加或“可交易”标签作为里程碑，必须在第一次 offer / purchase / local-service 选择前给出 `first_trade_preview`：产品与可用数量、当前买方/本地需求、offer/limit price 或价值等级、ownership/eligibility、预计交付路线与 ETA/损耗、交易后保留库存、主要风险、下一目标关联和 `recommended_trade_action`。玩家至少能比较 `offer_for_trade`、`hold_for_local_service`、`split_batch`、`reroute_or_reprice` 与 `defer`；成功收益是把制成品转成一次可归因的区域用途、结算结果或新的交付/协作选择，而不是承诺必然成交或立即获得长期市场权利。
- 首次交易/交付的失败成本与恢复必须可读：无匹配、价格不兼容、ownership/eligibility、active-use、路线不可达、交付损耗/超时或结算未确认时，不得静默没收、自动降价、后台改道或把 offer 当作已成交；玩家应能取消/持有、补足前置、重新报价、改道、转本地服务或回到当前工业目标重排。已发生的在途损耗、结算 sink 或本地服务价值不得默认全额退款，receipt 只确认实际发生的一次结果。
- 首个可交易工业品的 anti-abuse 边界：预览只读且不锁库存、不生成 escrow 或世界效果；未成交订单可取消但不保证买方；active-use/权限/库存不足不能被交易入口绕过；重连、重复投递与回放不得复制交易、交付、结算或里程碑奖励。该补充只冻结玩家动作、收益、失败恢复和验收语义，不新增订单簿、价格/税费/结算公式、市场资格或当前实现完成声明。
- 首个可交易工业品验收至少覆盖：正向样例能从 preview 读出需求、数量、代价/价值、路线、ETA、损耗、交易后保留库存与下一步，显式确认后只产生一次可追溯交付/结算并打开下一项用途；无匹配/价格不兼容/ownership 或 active-use blocker 样例保持库存与未成交状态，提供取消、持有、补证、重报价或本地服务路径；在途损耗/超时或结算未确认样例不伪造成交，玩家能看到实际损失与重规划动作；重连、重复提交与回放不复制世界效果或“首个交易”奖励。
- “首条稳定生产链”必须绑定同一条 canonical 产线身份，至少包含 `factory / recipe / 输入来源 / 电力前置 / 物流前置`；更换 factory、recipe 或任一决定产出因果的前置后，属于新的候选产线，不能继承旧窗口。稳定窗口由适用验证 profile 提供 `W` 个连续运行 tick 或完整生产 cycle，本文不冻结 `W` 的平衡数值；窗口内每个计数单位都必须满足输入、电力与物流可达，并按同一 recipe 产生有效产出。首次产出只证明生产链跑通，不等于已经稳定。
- 物流前置必须由显式、可审计且不可变身份的 `logistics_path` authority 承载，而不是仅从 ledger 或 bottleneck tag 推断。网络由不可变的有向边组成；每条边固定 `source ledger`、`destination ledger`、兼容的 `material kind`、不可转移的 `owner`、capacity 与以 Electricity 计价的固定 tariff。边在 operational 时对所有参与者公开可用；只有该边 owner 能改变 operational availability，不能借此形成私有 ACL 市场或转移 ownership。Recipe 的非空 `logistics_route_ids`/path binding 是显式 opt-in：每个绑定必须与 destination ledger、规范化 material kind 及该 recipe 的 consumed material kind 精确兼容，并沿 transit/job/completion 保留其 effective path identity。
- 多跳路径只在 ledger graph 上选择 operational、material-compatible 且容量可用的边；搜索必须遵守有界 hop policy、禁止循环，并持久化有序 edge identity。可行候选按 `(arrival_ticks, total_tariff_electricity, total_expected_loss, hop_count, lexicographic ordered edge IDs)` 升序确定；这是 replay-stable 的 canonical tie-break，`TransferMaterial` 的 `priority` 只影响队列/预留顺序，不参与路径比较。本文不冻结任何 numeric hop/capacity 上限、费率、税率或 balance tuning。
- 选定路径的 capacity reservation/release 必须 whole-path atomic：任一边不可用或容量冲突时不得留下部分 reservation；成功、失败、取消或改道都必须对整条路径各边恰好释放一次并写入可关联 receipt。settlement 只针对最终 effective path 记录一次 sender Electricity debit、沿途 owner payout 与 tax attribution；失败候选、重复提交和重放不得产生第二次 debit/payout/tax，也不得泄漏 reservation，且不得对固定 tariff 重新定价。
- 自动改道不是默认能力：只有显式 opt-in 且选定路径在提交前变为 unavailable 或 capacity-conflicted 时，系统才可按同一 canonical comparator 至多尝试一次 deterministic reroute。旧尝试的 reservation 释放与新路径预留必须在同一 all-or-nothing reroute transition 中完成，任一侧失败都不得落下部分状态；reroute receipt 必须同时记录原 path、effective path、触发原因、reservation 生命周期与唯一 settlement。无可行替代路径或一次改道再次失败时，必须拒绝并保留失败 receipt，不得后台重试、随机选路、重复收费或遗留容量占用。
- stable-line identity 必须使用 effective path identity（含其稳定版本/有序 edge identity）；任何 effective path 变化，包括被 opt-in 接受的 reroute，都会启动新的 stable-line candidate window；同一路径的 metadata-only retry、重连和重放不得复制进度、事件或产出。legacy direct route ID 与空的 `logistics_route_ids` binding 继续按既有兼容语义解码和执行，不从旧数据凭空合成 multi-hop path、reservation 或 market settlement；只有显式 path binding 才进入本 authority。
- 所有 path/edge 选择、reservation、settlement、reroute reason/receipt 与 stable-line identity 必须进入事件、serde 与 replay；replay 只应用已记录的 effective path、settlement 和 release 结果，禁止重新规划、重新计价、再次扣款或改变 owner availability。显式 non-goals：auctions/orderbook、ownership transfer/private ACL market、terrain/fleet simulation、stochastic routing、UI/rendering 与 balance tuning。

- **调运 lane：`direct_handoff` 与 `path_bound_transit`。** `TransferMaterial` 的玩家语义必须先区分两条不可互相冒充的 lane：`direct_handoff` 仅在 fresh authoritative snapshot 已确认 `from_ledger != to_ledger`、source/destination 共址且权威有效距离为零、调用者具备两账本的 access/eligibility、没有 `route_id`/`route_ids` 或其他显式 path binding 且 `auto_reroute = false` 时成立；caller 提供的 `distance_km = 0` 不是共址或距离证明，若共址/距离/access/eligibility authority 缺失或过期，必须 atomic reject 或 fresh requote，不得进入任一 lane。满足条件时它在执行步骤产生一次 source debit 与 destination credit 的 `MaterialTransferred` receipt，不进入在途、路径容量或 path reservation。任一显式 route/path binding 或 `auto_reroute = true`（即使距离为零）在路径校验/选择成功后才进入 `path_bound_transit`，保留 effective path、在途与到达边界；路径无效或不可用时 atomic reject；正距离但未绑定路径的既有距离运输继续服从现有 `TransferMaterial` transit 合同，不得被此补充改名为 direct 或 path-bound。当前 runtime 的无绑定零距离即时结果仅是兼容性的 partial evidence，不构成上述权威共址/access/eligibility 或完整玩法闭环已实现的声明。
- `direct_handoff` 的预览/确认必须让玩家看到“立即交接、无运输等待、无路径容量占用、无路径 tariff、无在途损耗”，不得展示会让玩家等待或预留运输的 path ETA；兼容 quote 即使保留 `ready_at`/`ticks_until_arrival`/`inflight_before`/`inflight_capacity` 字段，也必须把 direct 的 `ticks_until_arrival`/运输等待标为零，把 `ready_at`（若展示）标为执行/receipt 边界而非 path ETA，并把 inflight 字段标为一般兼容性容量上下文而非 direct reservation，不能告诉玩家正在等待。`path_bound_transit` 必须让玩家看到 effective path、容量/预留占用、Electricity tariff、预计损耗/实际收到量与到达 tick/`ready_at`。上述是玩家语义标签，不新增 quote 字段、数值公式或 ABI；若既有 preview 同时包含执行时间与运输 ETA，必须明确二者不是同一边界。
- 该补充的 deterministic acceptance 至少覆盖：fresh authority 确认共址/access/eligibility 的零距离、无绑定样例只产生一次 `MaterialTransferred`，无 pending transit、path reservation、tariff 或 transit loss；零距离显式 path 且校验/选择成功的样例只走 `MaterialTransitStarted`/completion，保留 path identity 与声明的容量/tariff/loss 结果，不能出现 immediate transfer；不同且非共址账本伪造 `distance_km = 0`，以及 stale/unauthorized ledger 样例均 atomic reject 或 requote、不回退 direct 且无世界效果；stale/invalid path 样例同样 atomic reject 或 requote；Viewer 与 pure API 对 lane、成本、时间、容量、损耗、完成边界和下一动作同义；同一 action/request identity 的重复投递、重连或 replay 不复制 transfer/arrival、reservation、production 或 delivery progress，新的 action/request identity 按新的确认独立处理。本文只冻结玩家动作、失败恢复、完成边界与 anti-abuse 验收，不新增 runtime 字段/schema/API、物流公式、UI 布局或当前实现完成声明。
- 显式 path 选择一旦提交，不得因路径无效、过期或容量漂移而静默回退到 `direct_handoff`；提交前后只能在首个不可逆 sink 前 atomic reject 或 fresh requote，并保留无扣料、无部分在途的失败成本。玩家若改变 route/path binding，必须重新确认一个新的 lane；同一 action/request identity 的重复投递、重连或 replay 不能替玩家清空绑定、绕过路径代价或刷新原意图，Agent 建议也不能代替该确认；新的 action/request identity 按新的确认独立处理。
- 两条 lane 都只表达材料 handoff：`direct_handoff` 的即时 receipt 与 `path_bound_transit` 的 arrival receipt 都不算 production receipt 或 delivery/terminal settlement receipt，也不能直接推进生产、稳定产线、需求满足或交付奖励；只有既有 recipe execution 与独立 terminal/delivery 合同才能产生相应后果。
- 稳定状态按 `candidate -> stable` 判定：只有同一候选产线完成整个 `W` 窗口才达成里程碑。任一未计划的缺电、缺料、物流阻塞、治理限制、危机或维护停机都必须把当前候选窗口进度清零，并留下可关联的 blocker/result；旧的达成历史可以保留，但不得作为新窗口的进度。计划内暂停同样必须把当前候选窗口进度清零，既不增加进度，也不允许跨暂停继承暂停前计数或奖励；为避免停开或切线刷取首个持续能力，恢复后必须从 `candidate` 重新完成窗口。
- 产线中断后必须回到既有 `wait / repair / reroute / reprioritize` 中至少一个真实可执行的恢复方向。恢复反馈必须关联原产线和 blocker，说明当前保留的窗口进度（首个稳定产线合同中为 `0`）、主要机会成本与下一次复查点；未经 opt-in 的自动改道、后台重试或仅恢复为可排程状态都不能被包装成已经稳定，只有一次确定性 reroute 成功并写入 receipt 后才可按新的 effective path 重新计数。
- 稳定产线验收至少覆盖以下边界：`W-1` 时缺料会清零且修复后重新计数；物流尚未到达或路径容量不足时保持 blocked、不计稳定进度；`W-1` 时计划维护暂停会清零、不继承暂停前计数或奖励，恢复后重跑；effective path 变化（含一次 opt-in reroute）会清零并启动新窗口，同一路径重试、重连、重复提交与事件回放不复制窗口计数、reservation、settlement 或里程碑奖励；不同 factory/recipe 之间不继承窗口。正向样例必须证明连续完成 `W` 后只产生一次可归因的稳定里程碑；负向与恢复样例必须让 Viewer 与 pure API 都能回答当前投入/产出、候选或稳定状态、停机原因、恢复动作和下一复查点。本文只冻结玩家玩法与验收语义，不新增 runtime 字段、UI 布局、配方数值或当前已实现声明。
- **稳定产线的共享输入短缺必须形成显式配给/降载闭环**：当已接受的两个或以上意图在运行中共同依赖的输入来源变为不足时，玩家不能只等后台拒绝；在下一次不可逆输入 sink 前，玩家必须能比较 `protect_critical_order`（把可用输入保给当前关键交付）、`curtail_or_defer_noncritical`（降载/延期其他意图）、专业 profile 支持时的 `run_partial_allocation`，以及 `restore_input_source`（补料、改走合法来源、重报价或等待）。预览必须说明权威可见的 available/held/unmet 输入、受保护意图的即时保留收益（例如继续当前交付/终端窗口）、被降载意图的延迟与容量机会成本，以及下一次重评边界；不承诺固定顺位、精确 ETA 或自动补料。
- 配给结果沿既有 lifecycle bucket 与 allocation outcome 落地：受保护意图只能得到同一权威快照下的 `full`，或 profile 明示的 `partial`；其余意图得到 `deferred`/`denied` allocation outcome，或保留在 `queued` lifecycle bucket，并保留 unmet/residual 与原 root/lineage。任何缺料导致 canonical 稳定窗口无法完成一个计数单位时，当前候选必须转为 blocked/at-risk 并按稳定产线合同将 `W` 归零；被保护的交付收益不能伪装成稳定里程碑，已消费输入也不能退款。恢复只能通过来源恢复、释放/重分配未消费 hold、`reroute/requote`、减少/延期或 profile 支持的继续/处置，在新的 allocation snapshot 上各至多重评一次。
- 配给 anti-abuse 与验收必须可追溯：同一 allocation snapshot 使用稳定 intent identity 和既有专业顺序裁决，玩家确认的保护/降载不能免费插队、重复占用输入或延长 hold；自动重试、重连、重复提交与 replay 不刷新优先级，不复制 input sink、产出、hold release、W 或交付奖励，也不得静默替玩家选关键订单。deterministic fixture 应覆盖稳定产线与两个争用意图的短缺、保护/降载/partial/恢复四条路线：Viewer 与 pure API 对 available/held/unmet、full/partial/deferred/denied、root blocker、机会成本、W 清零与下一复查点给出一致结果；来源恢复后只产生一次可归因重评，未知 authority 显示 `unknown/degraded`。该补充只冻结运行中共享输入短缺的玩家决策、失败成本、恢复与 anti-abuse 验收，不新增 runtime 字段/API、配给权重、队列/公平公式、输入数值或当前实现完成声明。
- **关键输入安全库存与保供取舍（补齐）**：现有共享输入合同处理“来源已经不足”后的保护/降载，但没有处理短缺发生前是否为下一合法 cycle 留出有界余量。安全库存是叠加在既有 input ledger、batch、buffer 与 allocation/hold 之上的玩家策略语义，不是新的库存类型、第二套账本或自动补货承诺；它只保护当前已选的 factory/recipe/root 与其下一个可验证生产或交付目标。
- 在下一次不可逆 input sink 前，玩家必须能打开只读 `input_reserve_preview`，比较 `consume_to_goal`（先做当前周期）、`set_bounded_reserve`（为下一周期保留）、`split_batch_with_reserve`（当前周期降载并保留余量）、`source_or_transfer_to_floor` 与 `defer`。该 preview 的玩家语义字段至少包括：当前 root/candidate、材料 batch/quality/owner/ledger、当前 `available`、当前 cycle 的 `required`、计划保留量或可覆盖周期（只有 profile 能证明时才显示，否则为 `unknown`）、执行后剩余量、保留所占仓储/buffer/物流容量、即时吞吐与交付机会成本、对稳定窗口 `W` 的影响、来源/路线条件、`next_recheck` 与 `recommended_reserve_action`。上述所有动作名称及字段都是玩家语义标签；每个可执行选择只有在对应 profile 已声明、且能映射到真实可提交的既有 action/hold/allocation/transfer authority 时才展示，unsupported 能力不得展示，也不得为其创建 API、隐式 fallback 或新 reservation；本 preview 不新增 runtime schema、数值公式或 UI 结构。
- 安全库存带来的即时收益是降低下一 cycle 因缺料停机的概率，并保护稳定产线候选、交付窗口或下一次区域用途；代价是当前少产出、占用仓储/边容量并承担批次质量/保管风险。玩家确认后，只有已有专业 profile 支持时才能通过既有 bounded hold/allocation 机制表达保留；任何 effectful reserve 必须绑定不可变上下文：既有 root/candidate、下一个可执行 `next executable cycle`、material batch/owner/ledger、profile/revision 与 authority snapshot；仅该既有 hold/allocation identity 拥有该 reserve 的 release 与 re-evaluation，且各自 exactly-once。不支持时仍是 advisory-only choice，不产生任何 effect，也没有 release/re-evaluation lifecycle；保留量被实际消费时，只能按对应 cycle 的普通 input sink 结算，不因“保供”额外推进 `W`、奖励或交付。
- 失败与恢复必须沿现有资源链闭环：来源耗尽、当前库存不足、仓储/buffer 已满、批次质量/时效失效、共享争用或 quote/authority 漂移时，不得静默补料、透支、改配方或扩大 reserve。玩家可释放/重分配未消费保留、补料或调运至 floor、重报价、减少/延期当前 batch、换合法来源/路线；每次释放与 fresh-snapshot 重评最多生效一次。保留不足导致当前 cycle 无法形成合法 quantum 时，按既有 `full/partial/deferred/denied` 与 stable-line `W` 规则处理，不能把预留本身包装成已生产。
- 冷启动或没有可验证 forecast/profile 时必须显示 `reserve_not_tracked` / `unknown`，仅降低推荐置信度，不阻断首个工业 cycle，也不能给出“安全覆盖 N 个周期”的伪精确承诺。来源恢复、补充或容量释放前，等待只保留当前已披露的机会成本，不自动续期旧 quote、刷新优先级或制造新的 reserve。
- 安全库存 anti-abuse 与验收：preview 不锁库存、不生成新 hold、不推进 tick/生产/稳定窗口；同一 reserve 不能同时服务多个 root 或下游 join，重复设置、拆批、重连、Agent retry、snapshot restore 与 replay 不复制材料、容量、优先级、`W` 或奖励；玩家确认不能替玩家静默选择关键订单。`test_tier_required` 至少覆盖同一 deterministic fixture 下“立即满批”与“保留下一 cycle 余量”的可比较收益/代价、reserve 支持与 advisory-only 两种 profile、库存/仓储/质量/争用/quote 漂移负例，以及 release/reallocate/source recovery 后的一次性重评；Viewer、pure API 与 Agent 必须同义表达 `available/required/reserved/unmet`、机会成本、root blocker、`next_action` 与 `next_recheck`。`test_tier_full` 扩展到两条以上连续 cycle、共享来源与多 root 争用、跨 transit/buffer/terminal 的 reserve 释放、恢复/replay 和跨窗口 lineage，证明无隐式 starvation、无限等待或累计重复效果。
- 本合同的非目标是安全库存数值/公式、需求预测、自动补货、永久锁定、队列公平或优先权重、仓储/物流 runtime 枚举与 schema、UI 布局，以及任何当前实现完成声明。`game` 拥有当前周期与下一周期的玩家取舍、节奏、机会成本和 progression；M4 继续拥有 ledger/batch/hold/容量守恒，`world-runtime` 拥有现有 state/receipt/replay，source/replenishment 与质量 profile 继续服从各自专业权威。若现有 authority 无法证明保留量、覆盖周期或 reservation，产品面必须保持 `unknown/degraded`，而不是由 gameplay 文档创建第二套真值。
- **工业流水线状态到玩家动作映射（补齐）**：`doc/world-simulator/m4/industrial-resource-flow-contract.prd.md` 的 canonical lifecycle bucket（`accepted-unstarted`、`join_pending/held`、`active WIP`、`in-transit`、`buffer-held`、`terminal-pending`、`settled`）不能只以内部状态出现；player-facing surface 必须把每个 bucket 映射为下表中的四类反馈、当前 primary root blocker（若有）、保留/占用的机会成本、下一复查点和至少一个真实可执行动作。动作只能来自该 profile 已声明的能力，不能把 accepted/held/produced 推荐冒充已完成或交付。

  | canonical bucket | 玩家反馈类别 | 玩家此刻可比较/执行的动作 | 失败/恢复与 progression 边界 |
  | --- | --- | --- | --- |
  | `accepted-unstarted` | `已接受`（尚未开工） | `wait_for_capacity`、`reduce_or_defer`、`release_or_reallocate_hold` 或修复最早 blocker 后重报价 | 不产生 sink、WIP、稳定窗口或产出；hold/容量占用和下一复查点必须可读，重复提交不能刷新顺位。 |
  | `join_pending/held` | `停机/阻塞`（齐套等待） | `wait_for_all_parents`、补足缺失 parent、释放/重分配 hold、支持时改走替代来源或 staged intake | 缺失/不适用/过期 parent 是 primary root；atomic profile 在齐套前不扣输入、不进 WIP、不计 `W`，late arrival 只重评未决 edge。 |
  | `active WIP` | `执行中` | 继续当前 stage；若 profile 支持则 `pause`、`repair`、`quarantine` 或 `replan`，否则明确显示不可用 | 已消费投入、WIP 与下一边界必须保留原 lineage；中断不能伪造退款或完成，恢复后只允许一次继续/处置结果。 |
  | `in-transit` | `执行中` 或 `停机/阻塞`（按 arrival/expiry） | 等待到达；只有 profile 在提交前明确声明 in-flight `reroute`、本次 intent 选择该能力且其原子 reservation/release 与 receipt 规则适用时才可 `reroute`；否则仅显示 M4 `transfer`、`return`、`hold`、`reject` 及 profile 支持的 `requote`、减少或延期。 | 运输损耗/迟到只影响实际到达和后续 blocker，不产生 delivery finality；不支持的 in-flight reroute 保持不可用且不产生额外 sink/reservation，状态漂移按一次 transfer/return/hold/reject 处置；重连/replay 不复制发送、到达或扣款。 |
  | `buffer-held` | `已产出`（待下游） | `route_to_next_stage`、`hold_in_output_ledger`、等待/释放容量或 profile 支持的本地用途 | 产物已保留但尚未解锁下游；buffer 满不得静默丢弃、瞬移或自动改道，容量恢复只重评未决批次。 |
  | `terminal-pending` | `停机/阻塞`（待终端） | `deliver_to_terminal`、预留/等待终端容量、`hold`、`requote`、支持时改道或减少 | production receipt 不等于 delivery/settlement；owner、资格、需求或容量失效时必须保留产物与恢复路径，不发终端奖励或里程碑。 |
  | `settled` | `已产出`（已结算） | 只读 `pipeline_run_review`，再按 profile 支持进入下一配方、交易/本地服务或新的工业目标 | 只有匹配的 production/delivery settlement receipt 才能打开相应用途；review 本身不推进世界效果；重复打开、重连、retry/replay 不发第二次奖励、需求减少或稳定进度。 |

  以上映射的最小可读摘要固定为 `state_class`、`root_blocker`（无则 `none`）、`held_or_consumed_summary`、`next_action`、`next_recheck` 与 `progression_effect`；未知 authority 必须显示 `unknown/degraded`，不能以 `0`、空列表或“已完成”代替。状态变化沿同一 root/lineage 复用，不因 Viewer 刷新、Agent 重试或 pure API 重连创建新运行。该补充只收口玩家动作、反馈、失败恢复和 anti-abuse 语义，不新增 runtime 字段、UI 布局、队列/产率公式或当前实现完成声明。

  流水线状态闭环验收至少覆盖：三阶段 `join -> stage -> transit -> buffer -> terminal` 的 deterministic fixture 能逐 bucket 读出上述 `state_class` 与一个真实 `next_action`；在每个阶段注入缺料、容量、路线、owner/资格或终端 blocker 时，Viewer 与 pure API 给出相同 primary root、机会成本、复查点和恢复动作。`accepted-unstarted` 与 `join_pending/held` 不计稳定 `W`；`buffer-held`/`terminal-pending` 则必须分别覆盖两种 canonical line/profile policy：terminal admission 为因果条件时保持 blocked 且不计 `W`，production-only profile 可推进 production-stable/`W` 但仍保持 undelivered/unfinalized，delivery/terminal reward/progression 仍须等待合法 settlement。未声明的 boundary 不得由 bucket 名称推断；不支持的 in-flight reroute 负向样例保持不可用且不产生额外 sink/reservation。重连、重复提交、arrival reorder、retry、snapshot restore 与 replay 保持同一 bucket/root/lineage 且不复制 sink、产出、delivery、奖励或 hold release。
- 多输入阶段必须在提交前提供只读 `join_readiness_preview`，按 canonical candidate/join/cycle 列出每条 required parent edge/branch 的 eligible-arrived、held/reserved、unmet quantity、lineage/receipt、`ready / partial / missing / incompatible / expired`、edge/buffer capacity 与下一复查点。玩家可比较 `wait_for_all_parents`、专业合同支持的替代来源/路线或重报价、`release_or_reallocate_hold`、显式 staged profile 的 `start_staged_intake`、延期或选择其他配方；preview 不预留 parent、不自动混料/降级/替代，也不伪造 ETA。
- 默认 atomic kit 在所有 parent 同一快照齐套前不扣输入、不进入 WIP、不计稳定窗口；早到输入继续占用已披露的 ledger/buffer/hold，等待的机会成本是库存/容量被占用与交付延迟。只有 profile 明示 staged intake 时，surface 才能显示各 edge 已消费量、remaining obligation、partial-kit 容量/额外时间/电力与失败处置；partial material 或 shared allocation 不能自行开启 staged mode。Missing edge 保持 root blocker，并提供真实 wait/supply/release/reroute/replan；不支持的 cancel/staged/rework 不展示。
- Join 玩法验收至少覆盖：2–3 个 parent 的不同到达顺序得到同一 ready/blocked 结果；atomic 模式在缺失/不适用 parent 时保留 arrived/held/unmet、无 sink/progress，齐套后只启动一次；supported staged 模式只结算声明 parent 并保留 pending/residual，late capacity/arrival 只重评未决 edge；preview 后库存/容量/owner/spec 漂移原子拒绝或重报价且无 double hold。重复 arrival/submit、重连、retry、replay 不复制 input sink、kit completion、稳定进度或奖励，partial kit 不计 `W`；Viewer 与 pure API 对 join/parent identity、逐 edge readiness、机会成本、root blocker 和恢复动作保持一致。本文不冻结 recipe ratio/minimum、timeout、容量数值、queue/fairness、route 算法、runtime schema/event、UI 布局或当前实现声明。
- 当两个或以上已接受工业意图竞争同一生产位、物流边吞吐或目的 buffer 时，提交前的共享容量预览必须让玩家比较 `deliver_current_goal`、`reserve_capacity_for_critical_edge`、`reduce_or_split`、`reroute_or_alternate_source` 与 `defer`；至少说明当前可用容量、玩家有权读取的竞争 holds、预计服务/复查窗口、提交后持有量，以及对下游交付与稳定窗口的机会成本。该预览不保证取得容量、精确队列位置或 ETA，也不暴露其他主体的私密细节。
- 接受后的玩家结果必须区分 queued/deferred、full/partial allocation、in_transit、released、expired 与 denied；展示争用 stage/edge/buffer、held/unmet quantity、顺序理由、下一重评条件及真实可用的 wait/release/reallocate/reduce/reroute/pause 动作。开始前释放只释放未消费容量一次；开始后只能使用专业合同真实支持的取消、salvage 或继续路径，不能展示虚构退款。重复取消/重提、重连、retry 或 replay 不能免费插队、延长 hold、垄断容量或复制稳定窗口/里程碑。
- 共享容量玩法验收至少覆盖：两个有效意图争用不足容量时，同一权威状态得到确定的 full/partial/deferred/denied 结果且未获容量者无 sink/进度；报价后容量漂移原子拒绝或重评；hold 释放/过期只释放一次并打开一次可归因重评；重复提交不提高优先级；持续高优先级流不能让仍有效的低优先级意图被无说明地无限跳过，而应按专业 profile 进入可读等待、过期、终止或重规划。未到达的交付仍保持 blocked 且不计稳定窗口；Viewer 与 pure API 对容量、结果、机会成本和恢复动作保持一致。本文不冻结 priority 权重、fairness/aging 公式、队列算法、runtime 字段或当前实现声明。
- 下游 stage、edge、buffer 或 terminal 容量不足时，暂停/降载上游、持有输出、合法改道、释放未开工容量或等待恢复的完整玩家因果循环见 [工业流水线背压恢复合同](./gameplay-industrial-backpressure-recovery-contract.prd.md)；该专题不改写本段 allocation 事实、队列策略或 capacity authority。
- 流水线 conditional service window、lease/expiry、hard/soft 迟到恢复、四类 power obligation、掉电处置与验收统一见 [`工业流水线服务窗口与电力连续性合同`](gameplay-industrial-pipeline-service-window-contract.prd.md)；Product 与 M4/Power/runtime 继续分别拥有玩家承诺和执行真值，本顶层只保留路由。
- 在 canonical operation/service/measurement window 完成或声明 checkpoint 后，玩家必须能读取确定且无世界效果的 `pipeline_run_review`。Review 绑定 frozen candidate/config epoch、plan baseline、world-time interval 与 receipt/journal anchor，比较 requested/committed/executable cycles、预期 input/power/output/byproducts/stage/transit/terminal obligations 与实际 executed/produced/delivered；cutover、batch split 或 plan drift 必须分开 review，缺少权威 baseline 时显示 `plan_unavailable`，不能拿新配置回填旧运行。
- `plan_baseline_id/revision` 只能在 canonical submit/allocation 接受结果处由权威 commitment snapshot 冻结一次；preview、request、speculative hold 与 atomic reject 不创建 baseline。玩家必须能读到 capture boundary、requested/admissible/committed/executable/unmet、预期 input/power/product/byproduct 与 stage/transit/buffer/terminal obligations、candidate/config epoch 和相关 join/bundle/window identity；full/partial/deferred/denied 结果都保留自己的 immutable plan outcome，但 deferred/denied 不冒充容量、sink 或 actual。
- 同一接受边界必须创建一次玩家可追踪但不泄露私密 raw ID 的 root operation continuity。Stage、join、bundle/branch、edge/transit、buffer/terminal、service/review window、baseline revision、checkpoint 与 production/delivery/compensating receipt 都显示同一 root 的稳定玩家标签、当前 segment/revision 与直接 parent；两个相同 recipe/candidate 的独立 accepted intents 仍是两次不同运行。Missing/mismatched root 不能靠材料名、到达顺序或 Viewer 缓存猜测，必须在 sink/credit/progress 前 pending/atomic reject 并显示 `operation_identity_unavailable/unknown`。
- Checkpoint continuation 与非因果 plan revision 保持 root 并链接 child segment；因果 changeover/cutover 创建 parent-linked child root/new candidate，旧/新 review 不聚合且继续服从 `W=0`。继续原 root 保留 WIP/hold 与因果可比性，也承接已披露的 space/capacity/power/time/late risk；启动 child root 只通过既有 replan/changeover/reroute 合同发生，并承担已有 release/renewal/cutover 成本，不新增自动迁移动作。Root terminal settlement/abort 后不再接纳普通 late receipt；rework/salvage/return/compensation 只能以既有 parent linkage 结算一次。Retry/reconnect/replay/tiny-run 不能新建 root、复制 W/reward/priority/capacity 或把两个运行合并。
- `product_validation_preview` 必须说明授权可见的 root label、segment/revision、parent stage/join/bundle/branch、product stack/owner/ledger、validation 对该 root 是 mandatory 还是 advisory、proof/module/profile version、quality/quantity scope，以及专业合同声明的 time/power/data/slot cost。玩家只比较真实支持的 `validate_for_this_root`、`use_existing_receipt_for_this_root`、`hold_until_validation`、`reroute/defer`；preview/quote 只读且不能解锁 downstream、生成 receipt 或把 advisory 推荐伪装成保证。
- Accepted validation 只产生一个 evidence-only child receipt，显示 pending/applicable/not_applicable/unknown/expired、actual quantity/quality/provenance 与 freshness；它不 sink/credit 材料、不推进 W/奖励/交付。Mandatory gate 要求 matching applicable receipt 加 settled/arrived parent 才能下游开工；owner/module/profile/root/branch/spec/ledger drift 使旧结果 superseded，并只提供 revalidate/hold/alternate path 中专业合同支持的动作，不能跨 root 使用 global latest、自动 downgrade 或静默转移。现在验证会消耗已披露资源并保住用途资格，hold/reroute 能保留资源却延迟交付或降低用途/value；已有 receipt 但 stage 未 ready 时仍是 validated-but-nonconsumable。Retry/reconnect/replay 返回同一 child receipt，不复制 unlock/W/reward。
- 后续库存、容量或 profile 漂移只能改变 actual/result，不能重写 baseline。专业合同允许调整时，玩家在下一不可逆 sink 前比较 `keep_baseline_and_finish` 与 `revise_plan_and_start_new_revision`：保留旧计划维持 variance/root attribution，但继续承担已披露的 input/space/capacity/power/time 与 late risk；修订适应新需求/路线，却创建 parent-linked revision/new review，并承担既有 release/renewal/cutover 代价。因果 recipe/factory/edge/terminal 变化继续按换线合同创建新 candidate 且 `W=0`；非因果修订可保留 candidate，但 actual receipt 仍只能归属 owning revision，旧 baseline 不回填。Stale revision preview 原子拒绝或重报价；retry/reconnect/replay 返回同一 revision，不复制 hold/sink/metric/reward。
- Review 至少展示实际 throughput/timing、productive 与 queued/WIP 时间、窗口结束时 backlog/join_pending/WIP/in-transit/buffer-held/terminal-pending 的互斥快照、运输 sent/received/loss/delay、production 与 delivery/lease/window outcome，以及权威可得的 stage/edge/buffer/terminal utilization 分类。未知或未追踪事实显示 `unknown/not_tracked`，不能伪造 `0`、精确 ETA 或良率。Primary bottleneck 来自最早明确 causal blocker/expiry/lease/segment receipt，派生下游缺料/积压保持 secondary；review 不新增 blocker taxonomy、重写 service-window 结果或跨 actor 泄露私密事实。
- `pipeline_run_review` 只从权威值说明 held input/space/capacity/power/time、延迟交付/本地用途等 opportunity cost，并给出 `keep_and_stabilize`、`repair_root_bottleneck`、`release_or_reallocate_capacity`、`reroute/requote`、`reduce/defer`、受支持的 `switch_recipe` 中一个真实下一动作；它不自动执行或承诺顺位。Blocked/incomplete/terminal-held run 仍保持未完成，review 不改变 W、lease、receipt、reward 或 progress。相同 operation/window 的重复打开、重连、tiny run、等待循环和 replay 得到同一 review，不发奖、不刷新优先级或容量；Viewer 与 pure API 对 baseline/actual、bucket、loss/utilization、root/secondary、unknown 与下一步保持一致。本文不冻结 throughput/yield/utilization 公式、目标值、采样 cadence、metric schema/storage、dashboard/UI、自动优化或当前实现声明。
- **复盘后的后续选择（补齐）**：仅给出一个 `next_action` 不足以让玩家根据实际偏差作出取舍。当同一 `pipeline_run_review` 下存在两个或以上专业合同真实支持的后续动作时，必须提供只读的产品语义 `review_followup_preview`（不是新的 runtime schema/API），最多前景化 2–3 个选项；候选至少包括适用的 `keep_and_stabilize`、`repair_root_bottleneck`、`release_or_reallocate_capacity`、`reroute/requote`、`reduce/defer` 或 `switch_recipe`。每项必须绑定原 review 的 root/revision，并说明 `immediate_gain`、`cost_and_held_value`、`goal_or_terminal_impact`、`failure_or_residual_risk`、`reversibility`、`next_recheck` 与 `recommended_reason`。修复根瓶颈的收益是减少已观测延迟或保住下一次执行机会，代价是维护/电力/容量占用；改道或重新报价的收益是改善到达或终端机会，代价是运输费、损耗、等待，若改变产出因果则按既有新候选与 `W=0` 处理；降载/延期的收益是保留输入和容量弹性，代价是吞吐与交付延迟。只有一个真实选项时仍须说明为何没有安全替代；没有安全选项时必须返回 `no_safe_fallback` 及下一次决策动作，不能用推荐文字伪造选择。
- `review_followup_preview` 与 `pipeline_run_review` 都保持只读，不锁定或刷新输入、容量、lease、优先级、baseline、W 或奖励；玩家明确确认后才可提交既有 follow-up action。提交必须把同一 root/revision 作为提交前授权与 fresh authoritative revalidation 上下文；若所选既有 reroute/changeover/revision 合同要求建立 child revision/root，可至多创建一个与原 review parent-linked 的 child，但原 review 保持 immutable，receipt/actual 不跨 revision，hold/sink/release/receipt 的既有 exactly-once 约束不变。状态漂移只能 stale/requote 或原子拒绝且无新 sink/hold/receipt；不得自动执行、免费重跑、隐式改道或修改已关闭的 review。任何 effectful follow-up 只能沿既有 action/receipt 产生一次与原 review/root 可关联的结果；重复确认、重连、Agent retry、snapshot restore 与 replay 只能返回同一 disposition，不得复制容量、输入、W、奖励或优先级，也不能把 review 重新打开为新的授权。
- 复盘后的玩法验收至少覆盖：同一 deterministic fixture 中 production 按时但 transit 迟到，且 `repair_root_bottleneck` 与 `reroute/requote` 均为真实可用选项；review 能让玩家比较两者的即时收益、成本/占用、终端目标影响、风险、可撤回性与 `next_recheck`，显式确认后只产生一次 linked follow-up effect。若容量/路线在确认前漂移，旧 preview 必须 stale/requote 或 atomic reject；`plan_unavailable`、unknown 或没有安全动作时不产生 follow-up effect。重复打开、提交、重连、retry、replay 不复制 sink、receipt、W、奖励或顺位，且 Viewer 与 pure API 对偏差、候选、推荐、根因和下一步保持一致。该补充只冻结复盘后的玩家比较、失败恢复与 anti-abuse 语义，不冻结优化算法、指标公式、队列/优先级、数值平衡、UI 布局或当前实现声明。
- Plan-baseline 玩法验收至少覆盖：preview/speculative hold/atomic reject 无 baseline；full/partial/deferred/denied accepted outcome 各只创建一个 immutable baseline；submit 后库存/容量/profile 漂移不改写计划侧；partial execution 后修订在单一边界关闭旧 review、创建 parent-linked revision，receipt/actual 不跨 revision 且不重复 W/lease/reward；因果改动触发既有 cutover/`W=0`，非因果修订保留 candidate 但分开 review；缺失 anchor 显示 `plan_unavailable`。重复 submit/revise、重连、retry、checkpoint recovery 与 replay 得到相同 baseline/revision，Viewer 与 pure API 对 identity、boundary、changed fields、opportunity cost 和下一步一致。本文不冻结 baseline schema/storage、metric 公式、queue/fairness、UI 布局、自动 replan 或当前实现声明。
- 只有专业 profile 声明的合法原子边界才能关闭 diagnostic checkpoint。提交前的只读 `checkpoint_continuation_preview` 必须说明 checkpoint/parent segment、operation/baseline revision、边界上的 backlog/join_pending/WIP/transit/buffer/terminal/hold、仍占用的 input/space/capacity/power/time/lease，以及 `continue_same_operation` 与 `close_and_rebaseline` 中真实支持的选择。Checkpoint 不得切开 join/cycle/output bundle/branch/handoff，也不能由反复刷新、tiny run 或 Viewer 本地采样制造；边界尚未合法时显示 pending/下一复查条件，不伪造半份 review 或 ETA。
- `continue_same_operation` 保持同一 operation/baseline，并创建 parent-linked continuation segment；边界上的未决状态只作为 opening state 延续，不重新计入 actual、重新接受或分配。`close_and_rebaseline` 仅在既有专业合同允许时创建新 baseline/review；非因果修订可保留 candidate，因果变化继续触发 cutover/new candidate/`W=0`。继续能保留计划差异与根因可比性，但承接已披露的 WIP/容量/lease/late 风险；重建基线能适应新计划，却失去直接旧基线比较并承担现有 release/renewal/cutover 成本。Checkpoint 自身不释放/续租、不改变 W/receipt/reward/finality，也不自动执行 repair/reroute/reduce。
- Checkpoint 玩法验收至少覆盖：长 operation 在 active WIP/transit/held input 下合法关闭一次并续段，每个 receipt/bucket 只归属一个 segment；原子 join/cycle/bundle/handoff 中途请求延后且无半份 sink/actual；rebaseline 在精确边界创建 parent-linked review，旧 review immutable，因果变化服从 cutover。重复 checkpoint/open、重连、retry、restore/replay 与 tiny-window 尝试不复制 review、progress、priority、capacity、hold release 或奖励；缺失/冲突 anchor 显示 `checkpoint_unavailable/unknown`。Viewer 与 pure API 对 checkpoint/parent/baseline、opening bucket、机会成本和下一步一致。本文不冻结 checkpoint cadence、metric formula、queue/fairness、runtime schema/storage、UI 布局、新恢复 taxonomy、自动优化或当前实现声明。
- Root-operation 玩法验收至少覆盖：3-stage join/fan-out/terminal 的所有玩家可读 receipt 属于一条 root path，两个完全相同的并发运行保持分离；fan-in/out 乱序不改 root 或交叉 credit；缺失/冲突 root 原子拒绝或 pending 且无 sink/W/reward；checkpoint/revision 保持 root，causal cutover 使用 linked child root 且旧/新 review、W 与 actual 分开；rework/salvage/return/compensation 保留 parent 一次；terminal settlement/abort 后的迟到普通效果被拒绝或走现有 compensation。重连、retry、replay 不复制 root/segment；Viewer 与 pure API 对授权可见的 root label、segment、parent、finality 和恢复下一步一致。本文不冻结 ID 格式、runtime schema、跨 actor 私密信息、queue/fairness、UI 布局、自动分组或当前实现声明。
- Product-validation 玩法验收至少覆盖：两个相同并发 roots 验证同一 product 时各有独立 child receipt 且不交叉解锁；quote、denied/missing/mismatched root 无 receipt/sink/progress；验证后 owner/module/profile/quality/ledger 漂移产生 stale pending/reject 与真实 revalidate/hold/alternate path；applicable receipt 在 stage 未 ready 时仍不消费，前置恢复后只产生一次 downstream sink；cutover/revision/rework 保留 parent 且不转移旧 W/reward；retry/reconnect/replay 不重复 decision/unlock。Viewer/pure API 对 authorized root label、parent、status、quantity/quality、mandatory/advisory gate、freshness 与下一步一致。本文不冻结 validation/WASM 算法、quality/yield 公式、权限系统、global latest projection、runtime schema、UI 布局或当前实现声明。
- 玩家在旧候选仍有 hold、WIP、transit 或 buffered batch 时提出配方、设施或边的因果换线，必须先获得只读 `changeover_preview`。预览只比较专业合同真实支持的 `drain_old_wip / finish_and_deliver`、`quarantine_wip`、`rework_to_new_recipe`、`salvage_wip`、`abandon_wip` 与 `defer_changeover`；只有声明了独立容量时才可提供 `parallel_new_candidate`。它至少说明旧/新候选身份、当前阶段与 lineage、已消费/仍 hold 的投入、占用的 stage/edge/buffer 容量、各选择保留或损失的价值、交付/停机影响、下一复查点，以及新候选稳定窗口从 `0` 开始。预览不能把通用取消包装成全额退款，也不能承诺未有权威依据的 ETA 或队列位置。
- 换线结果必须区分 draining、quarantined、rework pending/complete、salvaged、abandoned 与 changeover ready，并保留旧在制品的原身份直至一次权威处置完成。旧 hold 的开始前释放不同于已开始 WIP 的处置；隔离继续占用专业合同声明的空间，返工/迁移必须产生新 lineage/receipt 与披露的额外时间、资源和损耗，不支持的动作不展示或原子拒绝。旧 WIP 可以按旧候选完成并生成一次旧 receipt，但不能贡献新候选稳定窗口或里程碑；重复换线、取消/重提、重连、retry 或 replay 不能复制排空、返工、salvage、放弃、释放、退款或刷新共享容量优先级。
- 换线玩法验收至少覆盖：在 `W-1` 且四类旧状态分别存在时换线，新候选窗口为 `0`，旧状态各有唯一可追溯处置；排空/完成只交付一次并只释放一次空间，隔离保留 lineage 与声明的占用及真实复查/释放路径，支持的返工产生一次新 receipt 且不继承稳定进度，不支持的返工/salvage/放弃不显示或原子拒绝；旧状态漂移使 stale preview 原子拒绝或重评。恢复/replay 和反复切换不得复制效果、返还、容量或里程碑，Viewer 与 pure API 对旧/新身份、处置、保留/损失、交付与稳定窗口机会成本及下一步保持一致。本文不冻结换线时长、返工/损耗公式、状态字段、UI 布局或当前实现声明。
- 换线的撤销窗口必须与不可逆处置边界分开：`changeover_preview` 之后先进入只读的 `changeover_pending`，只有在尚未释放旧 hold、尚未产生 WIP/transfer/rework/salvage/abandon sink、且新候选尚未获得任何效果时，玩家才能执行 `revoke_changeover` 或 `defer_changeover`。撤销只撤掉待决意图，旧候选、旧 `W`、旧 lineage 与仍有效的 hold 保持原样，不发奖励、不返还不存在的成本、不刷新共享容量顺位；preview 状态漂移必须原子拒绝并要求重新报价。第一次不可逆处置一旦发生，状态转为 `changeover_committed`，不得再以“取消/撤销”伪造退款或回到旧进度；玩家只能按 profile 支持的 `finish_and_deliver`、继续隔离、完成/重试受支持的返工、salvage 或 abandon 路径恢复。旧候选处置完成并可换线后若再次选择旧配方，也必须新建一次 parent-linked 因果候选并从稳定窗口 `0` 重新开始，不能复用旧 receipt/里程碑。相同 root 的重复撤销、提交、重连、retry 或 replay 只能得到同一状态转移，不能免费来回切线、延长 hold、刷新优先级或复制释放/产出。
- 撤销/提交边界验收至少覆盖：`W-1` 的旧 WIP 在首个 sink 前撤销后仍为旧候选且无额外 sink/释放/奖励；首个 sink 后请求撤销得到当前 `draining`/`quarantined`/`rework pending` 等真实状态与恢复动作，不产生退款或旧 `W` 回填；受支持的返工只生成一次 child receipt 且新候选 `W=0`，切回旧配方仍须新 preview；报价后 hold、容量或 owner 漂移原子拒绝，重复撤销/提交、snapshot restore 与 replay 不复制效果。Viewer 与 pure API 必须一致表达 `changeover_pending`、`changeover_committed`、机会成本、不可逆边界和下一步。该补充只冻结撤销/恢复的玩家语义与 anti-abuse 验收，不新增 runtime 字段、取消 API、返还比例、换线时长或当前实现完成声明。
- 最终阶段排产前必须提供只读 `terminal_output_preview`，说明 terminal owner/recipient、目的 ledger/buffer 与可读容量、产品适用性/交付资格、生产后保留库存、交付是否属于当前 canonical line，以及 `deliver_to_terminal`、`hold_in_output_ledger`、仍存在后续阶段时的 `route_to_next_stage`、`reduce` 或 `defer` 中专业合同真实支持的动作。预览不锁容量、不保证需求/价格/ETA，也不能把 production completion 推荐成已经交付。
- 玩家结果必须区分 produced/held for delivery、in transit、delivered/settled 与 delivery rejected/expired。Recipe/production receipt 只证明产物进入合法输出账本或在途承诺；只有独立 delivery/settlement receipt 才能减少需求、获得交易/服务/区域用途奖励或完成终端交付里程碑。若 canonical line 声明 terminal admission 是稳定条件，未准入周期保持 blocked 且不计 `W`；若只验证 production，则只能报告 production-stable，不能标成 delivered/finalized。Owner、容量、需求或路线在生产前后失效时，保留 production provenance 和实际产物，提供真实的 hold、预留容量、改道、本地用途、重报价或放弃路径，不自动退款、没收、降价或绕过资格。
- 终端玩法验收至少覆盖：合法终端先产生一次 production receipt、后产生一次 delivery receipt；buffer 满、owner/需求失效在产出 sink 前按 profile 阻塞/原子拒绝，或把已生产批次保持为可读的 held/pending；生产后目的失效保留产物与恢复路径，在途损耗/过期只按实际到达量结算；稳定窗口服从已声明的 terminal-admission 策略。重复生产但未交付不能刷稳定/终端/交易里程碑，重复提交、arrival、重连、retry 或 replay 不复制 demand reduction、reward 或 finality；Viewer 与 pure API 对 owner、容量、production/delivery 状态、实际数量、机会成本和下一步保持一致。本文不冻结容量数值、价格/撮合、receipt/runtime 字段、2PC、超时/损耗公式、UI 布局或当前实现声明。
- 首局推荐采集目标必须把 `target_frag_id / expected_material_hint / starter_value_reason / first_recipe_relevance` 接到第一工业目标；玩家应知道“为什么先采这个 frag”，而不是只看到最近可采集物。
- 首局与后续补料不能从推荐目标直接跳到采集。进入 extraction 前，玩家必须获得只读 `source_assessment`；它把地图/Agent 发现提示与当前权威可采机会分开，并复用下文 `mine_now`、`reduce_batch`、`move_to_other_frag`、`wait_for_replenish`、`restore_power_first` 以及 `resource_replenishment_quote` / `fragment_refill_preview` 的专业 action/quote authority，而不是新建第二套动作。其他 source profile 只能增加其已声明的准备访问/路线、改用合法材料或配方与延期动作。每个候选说明 source 身份、预计可满足量及估计/权威边界、材料/质量提示、访问与竞争条件、采集和物流成本、枯竭/补充状态、对当前配方或首个工业目标的影响、主要风险、`next_action` 与 `next_recheck`；没有安全来源时不得无限推荐等待、静默自动换源或把同名材料视为等价输入。Chunk/fragment 生成、预算与补种事实仍由 [`chunked fragment generation`](../../world-simulator/scenario/chunked-fragment-generation.prd.md) 专题拥有。
- 来源选择的核心取舍是“更近但拥挤、较远但稳定、减量尽快开工、等待补充、换材料/配方或延期”，不是刷新地图直到库存出现。提交按 fresh authoritative snapshot 重验；多个玩家/Agent 争用有限来源时只能消费专业合同公开、确定的 allocation 结果。成功只对实际获配量生成一次 source receipt 与 material batch；不足/耗尽/权限或路线漂移必须给出真实的 `full/partial/deferred/denied` allocation 结果、重新报价或无 sink 的原子拒绝；`expired` 仅表示专业合同已声明的 quote/intent 到期处置，不是新的 allocation outcome。预览/扫描不锁资源或顺位，拆批、重连、retry、乱序与 replay 不刷新竞争优先级、不复制采出/补充/奖励。
- Source/deposit 玩法验收至少覆盖：可采来源完成一次评估→提交→source receipt→batch；旧评估在竞争或权限变化后失效且无隐式 sink；耗尽且不支持补充的来源不展示虚假等待，玩家能改源、减量或换材料/配方；支持补充的来源在权威事件前显示等待成本与复查点，事件后只增加一次可分配量；两个意图争用时总获配不超量且等价重排/拆批/retry 不改变已声明规则。Viewer、pure API 与 Agent 必须同义表达 source 状态、估计/权威边界、已承诺/未满足量、选择的即时收益、失败成本与下一步；本文不冻结地图表现、资源分布、发现/产率/补充数值、所有权、队列或 runtime schema。
- 原材料批次的适用性、合法替代、换源/换配方、因果隔离与 sink 前后恢复统一见 [`工业原材料适用性与替代决策合同`](gameplay-industrial-raw-material-qualification-substitution-contract.prd.md)；Product SC-30 与 M4/runtime 继续分别拥有替代规则和 batch 执行真值，本顶层只保留路由。
- `BuildFactory` 的 construction/activation 前置必须在提交前给出可比较选择：玩家比较 `build_now`、`prepare_inputs`、`move_to_site`、`restore_power_first` 与 `defer`，并看到 owner/location 共址与 chunk readiness、factory kind/recipe fit、电力与 hardware 成本、建设后余额、首个工业目标关联和下一步。成功收益是一次可追溯的工厂能力/配方入口；位置、owner、kind、重复 ID、chunk 或资源失败时原子拒绝且不扣资源、不生成设施，玩家可补料/补电、改站点或延后。preview 只读且不绕过 owner/location、免费建造或重复 replay；正向只产生一次 `FactoryBuilt`，负向/重连/重试保持原状态。详细站点/拓扑选择由 [工厂选址与拓扑合同](./gameplay-industrial-factory-site-selection-contract.prd.md) 承接，gameplay 负责玩家取舍与恢复语义，product/M4 分别负责产品承诺与路径事实；该补充不冻结建设公式、activation 状态机、runtime 字段或当前实现完成声明。
- `BuildFactory` 报价的当前冻结范围只包括玩家可读事实与 anti-abuse：目标 owner/site/location、factory kind/id、candidate/config/world revision、稳定 quote correlation、owner/location/chunk/recipe-fit/existing-or-pending ID blocker、owner-held electricity 与全部 construction input obligations 的 kind/quantity/ledger/before/after、construction power mode、profile 选定的建设完成或激活 output boundary、首个工业目标关联及下一复查点。只展示专业合同真实支持的 `build_now` / `prepare_inputs` / `move_to_site` / `restore_power_first` / `defer`，每项说明目标/范围、追加成本与仍占用、预计结果/复查时延、风险/损失、可撤回性与推荐理由。报价不预留资源或容量，不推进 tick、账本或设施；`electricity_after` 只表示预计扣除后余额，不得被展示为 runway、maintenance safety 或未来 reservation。
- 在成本/资源映射、power mode、output boundary 与 authority revision 回写到 [`world-runtime` status matrix](../../world-runtime/prd.md#industrial-execution-status-and-authority-matrix) 前，任何实现公式都只是未冻结输入，不能被 surface 当作当前产品保证。提交必须对同一 revision 的 fresh state 重验；漂移只能按当前状态至多一次结算并产出 receipt，或 atomic reject 且无 sink、设施或义务。重复提交、重连、retry 与 replay 不得复制扣款、`FactoryBuilt`、激活、PowerPlant 输出、稳定进度或奖励；建设 sink、recipe power、maintenance 与 Agent battery runway 必须分开表达。`test_tier_required` 需覆盖正常/任一输入不足/地点与 existing-or-pending ID blocker、无 mutation quote、漂移重验、单次 receipt、四类 power mode 的 sink/hold/revalidation/best-effort 与争用/unknown、PowerPlant 在 profile output boundary 后才输出及 Viewer/pure API parity；本补充不冻结公式、runtime schema、队列、UI 或当前实现完成声明。
- 首次 compound extraction/mining 必须把采集收成提交前路线选择：玩家比较 `mine_now`、`reduce_batch`、`move_to_other_frag`、`wait_for_replenish` 与 `restore_power_first`，并看到 target frag、预计材料混合/compound、采集电力成本、frag/location 剩余预算或上限、第一配方关联和复查点。成功收益是按已披露成本获得可精炼 compound（以 `compound_mass_g` 表示）；超量会消耗稀缺 frag/地点预算并延迟后续目标，数量/共址/chunk/预算/单次或地点上限/电力失败必须原子无消耗，玩家可减量、换点、等待补种、补电或调运。preview 不采集、不推进 tick、不隐藏自动采矿或免费 compound；正向只产生一次 `CompoundMined` 并按实际元素扣减，负向/重连/重复/replay 保持原资源与预算。该补充不改采矿配方、产率、上限、资源守恒或当前 runtime/Viewer 实现声明。
- 高负载工厂或维护 sink 影响首条稳定产线时，玩家的提交前取舍必须分成两条不能互相冒充的信号：`ScheduleRecipe` 当前的 `electricity_cost / electricity_after` 与 `runway_before_ticks / runway_after_ticks / downtime_threshold_ppm / continue_production_risk` 是排程账本可负担性与 Agent idle-battery 安全信号；它不代表工厂维护 runway。当前排程不扣 battery，所以两个 runway 值相等；`maintenance_pressure_delta = unchanged` 也不构成维护消耗、折旧变化或“可安全继续生产”的证据。因而仅当 player-facing surface 已暴露该 quote 时，玩家才能据此在 `restore_power_before_scheduling` 与 `schedule_now` 之间决定；当前尚无 Viewer/LLM 闭环证据，也不能把 `recommended_maintenance_action` 误读成已经可用的维修系统。
- `ScheduleRecipe` 的接受不是可被换目标或断线抹掉的瞬时点击。提交前比较必须说明接受时投入/能源何时形成 sink 或 hold、预期产出与副产物去向、预计完成阶段，以及当前规则是否允许开始前取消、开始后中止、部分保留或 salvage；没有对应 runtime 能力与证据时必须明确“不可用”，不能用通用取消按钮、Agent 改道或本地清队列伪造退出。
- `ScheduleRecipe` 之前必须完成配方生命周期决策；六态的玩家选择、机会成本、旧工作处置、successor/`W=0`、current/target cutline 与验收见 [工业配方生命周期决策合同](./gameplay-industrial-recipe-lifecycle-decision-contract.prd.md)。Product SC-24 继续拥有六态与转换 truth，本路由不把通用 `recipe-fit` blocker 或同名版本提升为可排程资格。
- `ScheduleRecipe` 还必须提供只读 `schedule_batch_preview`，让玩家对同一 canonical factory/recipe 候选比较 `run_full_batch`、专业 profile 支持时的 `run_minimum_or_reduced_batch`、补料/补电/等待容量后再排程与 `defer`。预览至少说明 requested 与当前 admissible/可执行 batch/cycle、若提交将 committed 的数量、输入/电力/hardware 承诺、耗时或缺口延迟、主产物/副产物总量、unmet remainder、生产后余额，以及下游/终端 readiness 的引用；它只读且不预留未来容量、价格或 ETA。批量减少会保留库存、runway、边/buffer 容量与恢复弹性，但延后吞吐/交付；full batch 会更早形成产出，也会占用更多稀缺投入和空间。profile-declared `industrial_externality` 与 `operational_mitigation` 的玩家取舍见 [工业外部性与运营缓解合同](./gameplay-industrial-externality-contract.prd.md)，不改变本行既有 batch 语义。
- 提交必须按当前 snapshot 重验所选 batch amount 与合法 executable unit。不支持 partial 的 profile 只能全量接受或在首个 sink 前延期/原子拒绝；支持 partial 时必须显式返回 committed/executed 与 unmet，不得隐藏截断、客户端取整或自动缩小请求。Accepted batch commitment 的实际 cycle、输入、主产物与副产物各只结算一次，terminal delivery receipt 继续与 production 分离；状态漂移、重连、retry 与 replay 不复制或按新配方重算。批量大小变化本身既不新建候选，也不增加稳定窗口；等价成功工作拆为多次小批与一次整批取得相同进度/奖励，重复 tiny-batch 不能刷里程碑、插队、绕过 setup/吞吐成本或 terminal admission。
- 批量玩法验收至少覆盖：preview 确定且只读，full 与合法 reduced 只产生披露的投入、耗时、主/副产物、余额和机会成本差异；输入/电力/阶段/边/终端容量在报价后漂移时，全量原子拒绝或按已声明 partial 返回明确 unmet，无负库存、隐藏 hold 或半批产出；`accepted_batches`/合法 cycles 只对应一次 production settlement，held output 尚无 delivery receipt；同量成功工作按 full-vs-split 提交得到相同候选进度与单次里程碑，重复 reduced submit 不提高共享容量顺位。Viewer 与 pure API 对 requested/committed/executed/unmet、产品/副产物 totals、stable progress、blocker 与下一步保持一致。本文不冻结 batch min/max、yield/rounding/setup/吞吐数值、队列算法、runtime 字段、UI 布局、自动 batch resizing 或当前实现声明。
- `schedule_batch_preview` 还必须把本次批量放回当前 demand goal，而不是只回答“能生产多少”。最小 `production_goal_disposition_preview` 绑定 demand-goal identity、唯一 `goal_authority_ref`、factory/recipe candidate、plan revision 与同一 authority snapshot，区分 target、already committed、production-settled、delivery-settled、terminal-settled、remaining demand 和本次 expected executable output，并给出 `shortage / matched / surplus / unknown`、库存或 terminal 占用、机会成本与下一复查点。产品 PRD 拥有 demand-goal identity、数量分层和处置承诺；每种目标 profile 指向唯一拥有 purpose/recipient、目标量、取消/到期与满足边界的专业 authority，M4 只拥有成本/价值归因，runtime 只拥有权威状态与 receipts。玩法层不从市场热度、Agent 推荐、旧订单或客户端缓存推断需求，也不合并 delivery 与 terminal settlement，不冻结预测、价格或阈值。
- 玩家围绕有效目标只比较真实支持的 `produce_full`、`produce_reduced`、`replenish_or_transfer`、`hold_surplus`、`stop_or_defer` 与显式补产。`demand_unknown / not_tracked`、expired 或 authority conflict 时不得显示 matched/surplus，生产/补产与旧 schedule 均不可选；玩家只能建立/选择有效目标、保留既有合法产物、停止或延期，目标恢复后从 fresh snapshot 重报价。即时收益是用最少必要批次推进一次可交付目标并保留输入、电力、物流和容量弹性；失败成本是缺货延期、合法批量带来的盈余占用、运输/生产损耗和补产机会成本。目标满足后旧 schedule 不得静默继续；盈余不得自动倾销、销毁或伪造成交；profile 唯一声明的 matching settlement（delivery 或 terminal）不足时，玩家选择补产必须创建一次链接原目标与 receipts 的 parent-linked revision，选择停止或延期则保留 shortage 且不创建补产 revision；非 matching settlement 不减少需求。玩家的下一步必须明确是补料/调运、继续一批、持有盈余、停止、延期或改换新目标，而不是后台无限生产。
- 需求目标玩法验收至少覆盖小于、等于和大于一个合法 batch 的三类目标：不足时显示 remaining demand 与一次补产路径；恰好满足时由 profile 声明的 matching delivery 或 terminal settlement 单次减少需求并要求显式 stop/hold，另一 settlement 不重复满足，production receipt 本身不减少需求；合法 batch 超过剩余目标时披露 surplus、占用与减产/持有/停止取舍，不产生自动销售、销毁或额外成长。还须覆盖 unknown/not-tracked、expired 与 conflicting authority 均无 matched/surplus、生产/补产、需求减少或奖励，恢复后只 fresh requote 一次。目标、库存、batch quantum、产率或 terminal capacity 在报价后漂移时重报价、原子拒绝或保持 profile 明示的有界 pending；重复 submit/delivery、重连、乱序、Agent retry、snapshot restore 与 replay 不复制生产、交付、需求减少、盈余处置、奖励或容量释放。Viewer、pure API 与 Agent 对数量分层、状态、blocker、动作与复查点保持同义。
- Demand goal 变为 `matched / cancelled / expired / unknown / reduced` 后，`accepted-unstarted / active WIP / in-transit / buffer-held / terminal-pending` 既有工作的完整玩家处置、机会成本与新用途绑定边界见 [工业需求变化后的既有工作处置合同](./gameplay-industrial-demand-change-disposition-contract.prd.md)；该专题不改写 demand identity、matching settlement、bucket 或 receipt authority。
- 多输出 recipe 还必须提供只读 `output_bundle_preview` / `byproduct_disposition_preview`，按一个 bundle 列出主产物与每个副产物 branch 的数量/lineage、owner/destination、适用性、接收 ledger/buffer 容量、下游解锁或本地服务/交易用途，以及额外 edge/time/power 机会成本。玩家只比较专业 profile 支持的 `route_to_next_stage`、`hold_in_output_ledger`、`route_to_local_service_or_trade`、扩容/等待、减量或延期；preview 不锁容量、不保证价格/需求/ETA，也不自动新增 route、salvage 或 disposal。
- 默认 all-output atomic policy 下，任何 mandatory branch 在提交/完成前失去 owner、资格、容量或路线时，整个 bundle 在 output credit/progress 前延期、hold/quarantine 或原子拒绝，不能显示主产物成功而副产物消失。只有 profile 明示 split fan-out 时，surface 才能显示逐 branch settled/pending/failed、remaining obligation 与 parent bundle 是否完成；生产后的 route change 也必须是受支持且链接原 branch receipt 的一次 handoff。Branch 路由本身不创建新候选或额外 stable progress，除非既有 terminal-admission 合同已声明该 destination 是因果前置。
- 多输出玩法验收至少覆盖：全部 branch 可用时每条只产生一次 linked receipt；一条副产物 branch 满/失效时，atomic profile 无任何 branch credit/progress，split profile 仅结算允许 branch 并保留可读 pending/residual；容量释放只恢复未决 branch，不重复主产物、输入 sink、需求减少、奖励或稳定窗口。Route-to-next-stage 与 hold/local-use 展示不同但真实的容量、时间和用途取舍；重复 reroute、submit、arrival、重连、retry、replay 和 branch/event 乱序不复制任何 branch 或把 partial bundle 伪装完成。Viewer 与 pure API 对 bundle/branch identity、数量、destination、settled/pending/failed、production/delivery 状态和恢复动作保持一致。本文不冻结 yield/route/value 公式、容量数值、queue/timeout、runtime 字段/事件算法、UI 布局或当前实现声明。
- 已接受的工业任务必须让玩家区分 `已接受/待开始 / 已开始 / 进行中 / 已完成 / 被阻塞 / 已中止` 的玩法结果；`被阻塞` 必须能恢复并继续，或按专业合同明确转为终止，不能充当隐含完成。若开始前允许释放、撤回或过期，还必须把它表达为未开始的终止结果，并把设施、配方与原 accepted intent 保持在同一因果链中。目标换向、Agent interrupt、维护/权限变化、停机、重连、重试或 replay 不能静默取消或迁移任务，也不能重复扣除投入、复制产出/副产物，或让同一任务既完成又获得返还。
- 中断或适用的中止反馈必须比较 `继续 / 等待 / 修复 / 改道 / 中止 / 重新规划` 中真实可用的选择，并说明已投入、仍占用、已形成产出、可保留/返还/损失的价值、稳定窗口是否清零、主要机会成本与下一复查点。已经发生的 sink 或在制进度不得默认全额退款；salvage、部分返还或安全停机只能按专业 profile 的有界规则结算一次。该合同补足玩家取舍，不冻结 cancel action、返还比例、队列/状态字段或当前实现承诺。
- 在途工业任务验收至少覆盖：正常完成只产出一次；报价后状态变化在扣料前重新校验；若专业合同支持中止，分别验证开始前与开始后的投入/在制品/产出处理，若不支持则验证 surface 不展示虚构中止；设施、维护或权限中断保留可读原任务与恢复路径，恢复后继续只产生一次完成结果；换向、重连、重复提交和 replay 不复制 sink、产出、salvage 或里程碑。Viewer 与 pure API 必须对阶段、保留/损失、稳定窗口影响和下一步给出同义结果，Agent interrupt 必须留下旧意图 handoff。当前 runtime 若只有开始后最终完成路径，只能证明该窄路径，不得宣称完整中止/恢复闭环。
- 工厂维护真值、计划停机、`maintain_before_run / run_at_risk / reduce_load / defer` 的玩家取舍、既有工作处置与 current/target evidence cutline 见 [工业工厂维护与计划停机合同](./gameplay-industrial-maintenance-planned-downtime-contract.prd.md)；当前 `ScheduleRecipe` 的电力/battery 反冒充警告仍以本节前文为准。
- `RefineCompound` / `refine_compound` 若作为首个工厂或首个制成品前置恢复动作，必须在提交前展示 `refine_quote` / `refine_preview`：`compound_mass_g`、`electricity_cost`、`hardware_output`、`electricity_after`、`hardware_shortfall_before`、`hardware_shortfall_after`、`first_goal_relevance`、`recommended_refine_amount`、`refine_value_class`；该合同必须让玩家看见投入与产出、比较目标缺口变化、分类为 `enough_to_advance / partial_progress / poor_power_tradeoff`，并推荐继续精炼、先补电或改走采矿/等待路线。历史 `enough_for_next_step` 仅归并为现行 `enough_to_advance` 的旧称，不另建数值、行为或玩家承诺。采矿、移动或精炼被约束/改道时，仍须把原 accepted intent、原因、保留进度与可执行恢复方向呈现在同一因果链中。它只约束提交前可读性，不重平衡精炼公式、电力成本、产率，也不扩展为完整加工链。
- `refine_quote` / `refine_preview` 必须把转换成功与目标推进分开：玩家比较执行合法 `RefineCompound`、`mine_more_compound`、`restore_power_first` 与 `defer`，并依据 `hardware_shortfall_before/after`、`electricity_after` 和 `refine_value_class` 判断净效果。`refine_to_goal` / `refine_minimum` 只有存在能减少目标缺口的合法精炼量时，才能作为推进目标的建议；当前 simulator 的共享 Data 映射不满足该条件。成功转换按明确电力成本产生可归因 output，但不能据此声称推进建厂前置；已有目标材料时继续精炼的机会成本是失去可用于补电、采矿、物流或下一动作的资源，不得默认把全部 compound 一次性转化。
- 精炼的玩家收益必须按真实净变化解释。当前 simulator 的旧 compound/hardware 字段名均使用 `Data` 账本，before 是动作前可用于 BuildFactory 的全部有效余额，after 是扣投入再加产出的净余额；不得先扣投入再伪称 before，也不能把 gross output 当作两份独立库存。现行产率上限保证产出不大于投入，因此该模型的净 Data 不会增加，`enough_to_advance` / `partial_progress` 对缩小建厂 Data 缺口不适用；三种分类词表仍保留，但本模型须以 `poor_power_tradeoff` 说明无改善或净损失并允许暂缓，推荐量也遵守该净效果。公式、默认 2500 Data 分叉反例及验证见 [M4 报价合同](../../world-simulator/m4/industrial-resource-flow-contract.prd.md#industrial-quote-ledger-semantics)，不改产率或恢复旧 ResourceKind。
- 精炼失败成本、恢复与验收必须可读：compound/electricity 不足、零硬件产出、owner/chunk 前置或报价状态漂移时，提交原子拒绝且不扣资源、不生成 hardware，玩家可减量、补采/调运 compound、先 `BuyPower`/`harvest_radiation`、等待后重报或回到当前目标重排；quote 只读、不推进 tick/账本、不预留资源，不得用推荐绕过 owner、资源守恒或 `RefineCompound` 的现有拒绝条件。当前合法精炼成功样例只产生一次 `CompoundRefined`，并证明净 Data 保持或减少、建厂缺口不会缩小，不能要求不可达的 `enough_to_advance` / `partial_progress` 正例；低电/低 compound/零产出拒绝时保持原资源，重连、重复提交与回放不得复制已有转换、hardware、里程碑或恢复奖励。该补充只冻结玩家选择、机会成本、失败恢复与 anti-abuse 验收，不改精炼公式/产率/电力数值、动作 ABI 或当前实现完成声明。
- `market_quotes` 若影响排产或材料采购，必须展示 `market_quote_decision_preview`：`recommended_source`、`local_vs_world_cost_delta`、`tax_contribution`、`transit_contribution`、`remaining_shortfall`、`cost_pressure_class`、`recommendation_rationale` 与 `next_cost_reduction_action`；推荐结果可落到本地采购、外部调运、延后、治理调整或拆分来源，玩家不应只看到 `effective_cost_index_ppm`。该合同只补玩家提交前的来源取舍，不扩展订单簿、撮合交易或市场数值重平衡。
- `market_quote_decision_preview` 必须把采购/排产前的市场信息收成明确的玩家动作：玩家比较 `submit_with_local_supply`、`reduce_requested_amount`、`use_local_materials`、`source_missing_materials` 或 `wait_and_requote`，并看到 `submission_allowed`、`total_shortfall_amount`、`market_pressure`、`conditional_notice`、每种材料的 local/world 可用量、world cover、shortfall、transit loss、governance tax、effective cost 和 `next_action`。当前 `ScheduleRecipe` 只消费 factory input ledger；世界供给只是采购/调运的来源信息，须经受支持的显式补料使材料实际入厂，再重报价和提交。`submit_with_world_supply` 不能作为当前直接排产成功路径；世界覆盖不保证取得库存、固定价格、即时到货或长期资格。
- 市场缺口与允许状态必须来自同一消费账本和材料集合：先聚合同类重复需求，再逐种计算 `max(requested - selected_available, 0)`，最后相加；一种材料盈余不能抵销另一种短缺。对 `ScheduleRecipe` 的准入报价，`selected_available` 是该 factory input ledger 的可消费余额，不能由发起 Agent 或 world ledger 的库存替代；独立行情的 world cover 也不能清零该排产缺口。材料足够不等于其余排产前置已通过。Viewer/pure API/Agent 的分项、总缺口、`submission_allowed` 与推荐理由保持同义，不能显示负缺口或“材料不足但推荐提交”；收到报价即刷新其读面，详细实现边界见 [M4 设计](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)。
- 市场报价失败成本与恢复路径必须在提交前可读：`submission_allowed=false`、来源库存不足、世界供给不足、权限/ownership、配方前置或报价条件变化时，整次排产原子拒绝且不部分扣料、不静默切换账本、不把推荐当结果；玩家必须能减少数量、补充/改用材料、改走本地服务、等待后重新报价或回到当前目标重排。quote 是只读、确定、conditional 的当前快照，不预留库存、不推进 tick、不生成 escrow；提交时按当前库存、税费、运输、权限与前置重新校验。
- 市场 quote anti-abuse 与验收：factory input ledger 足够且其他前置通过的正向样例显示 `submit_with_local_supply`，后续 receipt 只消费一次该账本库存；工厂不足而 world-covered 的样例只能提示来源、税费/运输条件与补料下一步，直接排产仍拒绝。通过受支持的显式补料实际入厂后，须重新报价与校验，方可提交并产生一次配方结果；local/world 均不足的负向样例显示 `reduce_or_source_materials`、shortfall 与补救动作，且不产生部分生产；报价后状态漂移必须原子拒绝并保留原库存/任务，重报价后才能重试；重连、重复提交与回放不复制材料消费、配方启动或奖励。该补充只冻结玩家选择、失败恢复与边界，不冻结价格/税费/运输公式、市场制度或当前实现完成声明。
- `TransferMaterial` 若影响首条稳定产线或当前配方阻塞，必须展示 `logistics_transfer_quote` / `transfer_impact_preview`：预计到达量、损耗、到达 tick、优先级理由、吞吐占用、调运前后阻塞变化和推荐调运动作；玩家不应只在 `MaterialTransitCompleted` 后才发现这批材料是否赶上产线。
- `TransferMaterial` 的 quote 必须把一次调运收成可比较、可恢复的玩家决策，而不是“发起后再看结果”：玩家先比较 `submit_transfer`、`wait_for_transit_capacity`、`reduce_amount_or_source_materials` 或改走本地/替代来源，再显式确认。quote 至少同时显示 `submission_feasible`、`max_transferable_amount`、`source_amount_before/after`、`destination_expected_amount_after`、`expected_loss_amount`、`expected_received_amount`、`ticks_until_arrival/ready_at`、`inflight_before/inflight_capacity`、`effective_priority/priority_reason` 与 `conditional`；`remaining_shortfall` 与 `recommended_transfer_action` 可由当前配方阻塞上下文派生到玩家 surface，不把它们伪装成新的 runtime quote 字段。成功收益是按损耗后的实际到达量解除当前配方缺口、保住稳定产线候选或打开下一次交付选择，不保证请求量全额抵达或立即产出。
- 物流理由必须保留 producer 的语义：合法 direct/no-route 不显示 Path unavailable；`route_priority`、material default、`explicit_priority` 与 keyword fallback 分别说明来源，不能一律称物料默认。`submission_feasible=false` 时 ETA 明确不可用或标注成立条件，不显示为必然到达；正常 direct、真正不可用路径、各优先级来源及旧零距离兼容分别验收，不新增 blocker projection 或物流算法。
- 调运失败成本和恢复路径必须在提交前可读：跨区调运承担明确的损耗、等待与吞吐占用；距离超限、库存不足、无可用在途容量、权限/账本不符或报价快照失效时，动作原子拒绝且不扣减、不产生部分在途货物，surface 必须说明原因与下一步（等待容量后重新报价、减少数量、改用其他来源/路线、先修复权限或回到当前目标重排）。不能把“已接受”、排队或推荐动作伪装成货物已到达，也不能静默降额、后台重试或改道并继续计入稳定窗口。
- quote 是只读、确定且带条件的当前快照：不预留材料、不占用吞吐、不推进 tick；提交时必须用当前权威库存、距离、容量、优先级和权限重新校验。若报价后状态发生漂移，整笔提交失败并保留原库存/在途状态，玩家必须看到旧 quote 已失效并能重新报价；同一 accepted intent 的重连、重复投递或回放不得复制第二笔调运或第二次产线推进，若当前 runtime 没有可证明的 lineage/idempotency 能力，surface 不得自动重提而应先展示既有结果或要求新的显式决策。
- `TransferMaterial` 物流闭环验收至少覆盖：跨站正向样例能从 quote 读出损耗、到达 tick、实际收到量和配方缺口变化，并在一次完成 receipt 后只解除一次缺口；吞吐已满时显示 `wait_for_transit_capacity`、发送/接收量为零且不扣料，容量释放后重新报价；请求量超过来源库存时显示 `reduce_amount_or_source_materials` 且不部分扣减；距离超限或权限失败不产生世界效果并给出修复/改道动作；报价后库存或容量漂移时原子拒绝、保留全部原状态；重连、重复提交与回放不复制调运、损耗、到达或稳定窗口进度。该补充只冻结玩家动作、收益、失败、恢复与 anti-abuse 验收，不新增物流算法、价格/损耗/吞吐数值、runtime 字段或当前实现完成声明。
- 数据采集、跨 Agent 数据转移或数据合约若受电力成本或访问许可约束，提交前必须说明预计电力成本、数据 owner、recipient/use、许可状态与拒绝后的授权或替代路径；玩家不应只看到 access denied，也不得为提升可读性绕过 owner consent。
- 数据型工业前置必须把授权与替代路径收成玩家可比较的动作：在 `data_access_preview` / `data_transfer_preview` 中展示预计电力成本、owner、recipient/使用主体、purpose、scope、授权有效性、预计 sink/收益和下一步，玩家比较 `request_authorization`、`narrow_purpose_or_scope`、`use_local_owned_data`、`transfer_from_authorized_source` 与 `defer_or_abandon`。成功收益是让已授权 Data 合法支撑当前验证、配方、模块调用或工业交付；缩小用途或改用本地来源的收益是降低权限/等待风险，代价是可能减少目标覆盖，不得把预览当作已经获得访问权。
- Data-access 失败成本、恢复与验收必须沿完整生命周期可读：授权过期、撤销、scope 变化、owner/recipient 不符、数据或电力不足、结算前无法证明有效时，提交必须原子拒绝或进入明确待决，不产生 Data sink、访问收益或隐性义务；玩家可重新授权、缩小用途、改用合法来源、等待状态确认或放弃。预览只读且不授予权限、不转移/消费 Data、不让 Agent 自动继承越界授权；只有一次 receipt 支持的结算才能产生一次使用/转移结果，重连、重试、重复提交与 receipt replay 不得复制结果或复活失效授权。正向样例须能追溯 owner、recipient、purpose、scope、授权依据和实际结果；负向/恢复样例须保持资源与未结算状态、显示替代动作，并证明已结算合法 provenance 不因后续撤销被抹除。该补充只冻结工业玩家选择、失败恢复、可追溯 AC 与 anti-abuse 边界，不新增许可状态机、权限字段、数据经济/电力公式或当前 runtime/Viewer 实现完成声明。
- `ValidateProductWithModule` / `ProductValidated` 现在通过 `task_c0177461965146a8a1f7bfb99caf9b16`（GitHub #2599）提供已签名、只读且提交前可重复请求的 `product_validation_quote`，并在 Viewer 的 quote card 展示 `product_id`、用途/战略角色、可交易性、验证前后阶段、解锁/价值等级、推荐行动、缺失前置与可达推进/恢复路径；提交后的 `validation_unlock_preview` 继续解释已验证产品的用途与下一步。quote 不提交验证、不执行任意 WASM 模块、不生成 receipt 或改变权威状态。阶段前提在 quote 中是建议；除非 runtime 的实际提交规则阻止，不能把它描述为已禁用提交。该闭环不新增科技树、成就系统、产品链或数值平衡。
- `BuyPower` / `harvest_radiation` / 等待发电若用于恢复低电、临界电力或停机风险，必须展示 `power_survival_quote` / `energy_recovery_preview`：补电量、成本、恢复后状态、可行动 runway、下一步动作可负担性、防停机原因和推荐补电动作；玩家不应只看到缺电拒绝或补电事件。
- `power_survival_quote` / `energy_recovery_preview` 必须把低电恢复收成有边界的路线选择：玩家比较 `buy_power`、`harvest_radiation`、`wait_for_generation` 与 `defer_or_reduce_next_action`（有合法来源时也可比较已授权转移），并看到 `power_state_before/after`、恢复量估计、price/time cost、`survival_runway_ticks`、下一动作可负担性、停机避免理由、来源/权限条件和下一复查点。成功收益是恢复到能执行当前主目标的状态并保住可解释 runway；等待或降载的收益是节省交易/位置机会成本，但可能延迟交付，不保证下一 tick 自动足够。
- 电力报价的可读历史与当前可提交性分开：普通 snapshot 不得无条件抹去刚收到的结果；相关条件变化时保留该报价的来源/输入并显式标 stale 或失效，重新提交仍重验。请求 `price=0` 若表示按当前市场价解析，应保留原价格选择与有效报价价格各自语义，不能仅因解析后的正价格不等于 0 就判为输入改变；真实价格或其它依赖漂移仍失效。该合同不改变 self-sale、价格或执行规则；报价/执行一致性须独立核对。
- 低电恢复的失败成本、恢复与验收必须可读：买电可能因资金、卖方/权限或余额失败，采集可能因辐射不可用、过热或上限只得到较少电力，等待可能在临界状态下让下一动作不可负担并扩大停机机会成本；无论哪种失败都不得静默扣电、发电或推进 tick，玩家可减少请求、改采集/合法来源、先降载、等待一个有界复查点或重排目标。quote 必须只读、确定且不把 `electricity_after` 伪装成长期 runway/不停机保证；不得绕过 owner/location 电力边界、用无限等待或连续同参数 harvest 刷资源。正向 normal/critical 样例须显示恢复后状态与下一步并在提交后只产生一次可追溯电力事件；资金不足、辐射不可用、thermal overload、报价失效、重连/重复/replay 样例保持原状态并给出替代路径。该补充只冻结玩家取舍、失败恢复与 anti-abuse 验收，不重平衡电价、发电/消耗、阈值、容量或当前 Viewer/LLM 完成声明。
- `SellPower` 若用于短期变现或缓解现金流，必须展示 `power_sale_quote` / `energy_liquidity_preview`：售电量、预期收入、售电后状态、剩余 runway、下一动作可负担性、产线中断风险和推荐售电动作；玩家不应只看到卖电收入而不知道是否牺牲能源稳定。
- `power_sale_quote` / `energy_liquidity_preview` 必须把售电收成可比较的提交前决策：玩家至少比较 `sell_now`、`sell_less`、`hold_power` 与 `restore_power_before_sale`，并看到 `sale_amount`、损耗后实际交付量、预期收入、售电后电力状态、`remaining_runway_ticks`、下一动作可负担性、`production_interrupt_risk`、`recommended_sale_action` 与风险理由。成功收益是把明确承担的电力余额转成当前目标所需的现金/结算机会；少卖或持有的收益是保住下一动作和稳定产线，不保证固定价格、立即结算或长期流动性。
- 售电推荐的 canonical `recommended_sale_action` 为 `sell_full`（按本次请求量出售 / Sell the requested amount）、`sell_partial`（减少出售量 / Reduce the sale amount）、`defer_sale`（暂缓出售 / Defer the sale）。中文、英文与 Agent 消费均识别这三个 producer token，`sell_full` 不表示卖尽全部持有电力；真正未知 token 仍保守提示重新报价。使用真实 Rust quote producer 产生三个分支的跨语言 fixture，不能只用前端自造别名证明一致；推荐仍只读，不保证成交、不自动出售。
- 售电失败成本与恢复必须可读：售电过量可能把电力推入 critical/shutdown、打断生产并牺牲后续产出或交付机会；金额、价格、ownership、权限、范围、余额或报价漂移不满足时，提交必须原子拒绝且不扣电、不发收入，玩家可减量、持有、先 `BuyPower`/`harvest_radiation`、等待后重报或重排当前目标。quote 只读且不推进 tick、账本或结算；不得免费造能量/现金、绕过卖方 ownership/许可或位置池规则、静默部分成交/自动重试，重复提交和回放不得复制 `PowerTransferred`、收入或里程碑。正向验收须证明安全卖出只产生一次可追溯结算并显示售后 runway；临界样例须显式警告并支持减量/持有；余额不足、权限失败、报价失效与重连样例保持原状态且给出恢复动作。该补充只冻结玩家动作、收益、失败恢复与验收语义，不新增价格/损耗/税费/结算公式、流动性制度或当前实现完成声明。
- `FragmentsReplenished` / 运行期 frag 补种若影响缺料恢复或第一工业目标，必须展示 `resource_replenishment_quote` / `fragment_refill_preview`：当前 frag/chunk 剩余量、下一次补种 tick、预计补种量、等待成本、第一工业目标关联和推荐资源行动；玩家不应只在后台补种事件后才知道该不该等、换目标或改路线。
- `resource_replenishment_quote` / `fragment_refill_preview` 必须把等待与绕行收成同一提交前选择：玩家比较 `wait_current_chunk`、`move_to_other_frag_or_chunk`、`switch_alternative_material_route` 与 `reprioritize_goal`，并看到当前 frag/chunk 剩余摘要、补种是否启用、`next_replenish_tick` / `ticks_until_replenish`、预计补种量、`wait_cost_summary`、材料提示（非保证掉落）、第一工业目标关联、下一次复查点和 `recommended_resource_action`。等待的收益是保留当前位置与目标关系并等待可能的本地补种；移动/切线的收益是用明确的时间、电力、路线或加工代价换取更快的进度，不保证补种量或替代材料等价。
- 补种失败成本与恢复必须可读：补种关闭、无下一 tick、chunk 已满或估计量为零时继续等待会拖延目标；移动会承担时间/电力/位置与路线风险，切换材料可能增加加工、质量或市场机会成本。预览只读、确定且不生成 frag、不改库存/账本、不推进 tick；不能把估计量当成保证、用等待绕过生成周期/守恒或后台自动采集/传送。正向验收须在启用且未到 tick 时显示等待成本与 recheck，实际 `FragmentsReplenished` 后才产生一次可追溯增量；关闭/已满/无 tick 的负向样例必须推荐移动或切线而不是无限等待；等待未触发或重连/重复提交/回放时保持原状态并可重新分类为移动、切线或重排。该补充只冻结玩家动作、机会成本、恢复与 anti-abuse 验收，不修改补种周期/比例、材料守恒、目标规则、runtime/viewer 字段或当前实现完成声明。

### 2.5.1 首个工业目标结算边界：权威合同见 [工业首产物结算合同](./gameplay-industrial-starter-completion-contract.prd.md)；代表性配方执行 walkthrough 见 [代表性工业配方执行 Walkthrough 合同](./gameplay-industrial-representative-execution-walkthrough.prd.md)，本节只保留路由摘要，不重复规则。
## 2.6 PostOnboarding 阶段承接

首次行动闭环完成后，系统不能只留下“一次性总结 + 继续探索”的静态提示，而必须进入正式的 `PostOnboarding` 阶段。

该阶段的目标不是继续教按钮，而是把玩家从“会操作”切换到“建立持续经营、生产与恢复能力”；组织能力是可选扩展，不是独立者的隐性毕业条件。阶段成果是有完成边界、可归因世界后果和下一方向的有限进展，不是世界通关或强制终局。因此：

1. 第一个阶段主目标默认应围绕工业成长、产线恢复或组织能力稳定化。
2. 主目标必须同时展示进度、主要阻塞和建议下一步。
3. 首局至首个持续能力期间，预设目标可以作为必要引导脊柱；形成该能力后，它们必须降为当前世界状态下可选的模板，不能变成固定职业、强制阵营或无限任务清单。
4. 正式入口默认只前景化一个当前主目标，并提供低负担的“继续”路径；玩家只在阶段成果后的 2 至 3 个实质不同方向，或主动要求换向时作出路线选择。其他目标必须可找回但不争夺当前决策。
5. 玩家达成首个持续能力里程碑后，系统必须显式展开中循环方向，如生产扩张、治理影响或协作保障。
6. 目标作用域、canonical 转译、资源/权限校验、共同治理与审计是后台护栏；只有在改变当前目标的成本、锁定、恢复、共同承诺或可用替代路径时，才以简短原因和可执行替代路径进入前台，不得变成逐动作确认或审核。
7. `10-minute trust gate` 只证明玩家已经信任“控制是可靠的、目标是可读的、继续玩是值得的”；`first capability gate` 再证明首个持续能力在后续 `15~45` 分钟或 `1~3` 次会话内闭环，不得把两层 verdict 混写成一个。
8. `PostOnboarding` 默认不允许把玩家丢回“自由漂浮观察态”；如果当前主目标暂不可达，应解释原因并给出当前真实可用的恢复、止损或替代目标；没有可达路线时明确缺口与可能损失，可停止新增执行或自然离场，但仍保留既有义务、未决请求与持续世界风险。不能保证保全或替代胜利，也不靠无解释的状态观察掩盖阻塞。

产品承诺与组合验收见 `doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md`；本节继续拥有阶段选择、阻塞分类与玩法承接的专业规则。

### 2.6.1 分支承诺的 gameplay 合同

`branch_ready` 不是路线标签展示，而是在当前世界状态允许时给出 2 至 3 项可比较的分支承诺。规范类别为：扩张（将能力转为更大覆盖/选择）、稳定/恢复（守住能力并移除已知阻塞）与专业化/服务（把能力转成当前本地需求或协作的可交付贡献）。它们不是固定职业、任务树或默认强制站队。

- 每个候选必须表明即时收益，并分别说明选择后的第一个与第二个 beat 如何改变玩家的目标、可用选择、压力或可交付成果；两个候选在这两个 beat 内回到同一循环时，属于 `route_tradeoff_fake_choice`。
- 候选必须说明主要约束、风险或锁定，以及下次会话恢复的目标和第一动作。专业化/服务路线不得把与 major power 的绑定伪装成默认前置。
- 可回退候选必须说明回退窗口、主要代价和回退后保留/失去的价值；不可回退或没有安全回退时须明确告知，而非用“可调整”淡化承诺。
- 本合同只定义玩家决策与验收语义；路线是否可达、行动细节、成本、状态字段与 Viewer 呈现由相应专业域和权威状态决定。

<a id="27-当前两周收口重点2026-04-09"></a>
## 2.7 历史 early-retention 优先级来源

2026-04-09 的两周 focus order、延期顺序与冲刺冻结记录见 [历史实施与验收记录](https://github.com/eng-cc/oasis7/issues/1192#issuecomment-6162409697)。它不是当前排期；长期 gate、恢复和 control-feeling 合同由下节及[间接控制合同](gameplay-indirect-control-agency-contract.prd.md)持有。`10-minute trust gate` 与 `first capability gate` 分别判定，debug/probe 不代签正式玩法验收；历史 hold/not_run/pass 不代表当前候选。

### 2.7.1 PRD-GAME-012 稳定 early-retention 合同

产品承诺统一见 [`首局与持续游玩`](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md)。本节拥有不随短期任务日期变化的 gameplay 专业合同；当前 verdict、task trace 与复跑边界统一由对应 PR 与实际验证记录 和 `doc/testing/evidence/` 确认。

#### Gate 与 verdict 隔离

- `10-minute trust gate` 判断首次控制可信、主目标可读、玩家后果可见、阻塞可恢复，以及玩家是否出现继续游玩的基础意愿。
- formal headed Web/UI 首次控制地板要求最近样本的首次成功率 `>= 95%`；依赖手动 reopen/reload 才进入可控态、出现 `control ack timed out without progress`，或发生阶段回退伴随冻结的样本均计为失败，并使 trust gate 保持 `hold`，不能由后续恢复后的成功冲淡。
- `first capability gate` 判断首个持续能力是否在后续 `15~45` 分钟或 `1~3` 次会话内闭环；不得因为它没有在首个 10 分钟完成而把 trust gate 判为失败。
- `progression_pass`、`attraction_pass` 与 `motivation_density_pass` 分别表达推进与体验结构；`content_volume_pass` 只表达内容量诊断参考目标满足。目标覆盖、first capability、动机密度、内容量和真人继续意愿不得互相代签。
- formal lane 能推进但缺少真实选择、玩家因果或回访理由时标记 `progression_pass_but_attraction_weak`；`content_volume_weak` 只表达诊断目标缺口。`blocked` 且无可执行恢复不能被其他卡片高分抵消。
- active-LLM / headed live 样本与 deterministic-provider-backed required evidence 必须分开。required tier 证明合同、回归和设计充分性；真实玩家留存、生产 provider 体验或 release/playtest claim 仍需 live/provider evidence。

#### 0~30 分钟 beat 合同

| 时间 | 玩家问题与动词 | 必须形成的反馈/收益 | 失败签名与恢复 |
| --- | --- | --- | --- |
| `0~1m` | 我是谁、现在做什么；进入/读取 | 主目标包含动作、完成条件、阶段预期与下一步 | raw operator/debug 噪音抢焦点或 empty world 无可执行入口 |
| `1~3m` | 我的指令生效了吗；`play/step/select/action` | `Player Intent -> World Consequence -> Recovery Move -> Next Move` 控制证明 | rejected/blocked 缺权限、模式、前提或修复动作 |
| `3~5m` | 世界哪里因我改变；查看/对比 | accepted/executing/produced/blocked/cost 可区分，玩家可指出自己的因果 | 只有世界 tick / Agent 活动，标记 `world_activity_only` |
| `5~7m` | 下一步修、扩、等或改道；选择 | 至少一次带收益、代价、阻塞与预期结果的 meaningful decision | 只能无界等待，必须 fallback 或 reprioritize |
| `7~10m` | 小阻塞是否可处理；repair/fallback/reprioritize | blocker taxonomy 与 wait/repair/reroute/no-safe-fallback 中的可执行恢复 | 无恢复或升级解释，标记 `agency_weakened` |
| `8~12m` | 我获得了什么；确认产出 | 可见产出、小胜利、新用途或下一步价值 | 只有库存/吞吐上涨，标记 attraction weak |
| `12~18m` | 能力是否持续；补料/排程/稳定产线 | 一次产出转为可维护、可恢复并打开新选择的能力 | 只有 progress percent，标记 `progression_pass_but_attraction_weak` |
| `18~23m` | 接下来如何变强；expand/repair/specialize/tradeoff | 分支说明即时收益、后续 beat、风险/锁定与回访 hook | 只给路线标签或重复同一循环，标记 `branch_commitment_missing` / `grind_only` |
| `20~25m` | 能否打断或纠偏 Agent；interrupt/reprioritize/correct | 玩家能读到打断、重排、纠偏或 handoff | 只能等 Agent 自行推进，判定 agency 风险 |
| `25~30m` | 为什么下次回来；选择下一目标/回访包 | 本局成果、choice memory、新 leverage、下一目标和 first action on return | `forced_major_power_dependency` 或 `world_activity_only` 不能算 pass |

#### 内容量、动机与 anti-script 判据

- 内容量诊断参考保留为 `effective_play_minutes >= 30`、`player_operation_count >= 18`、`content_unit_count >= 8`、`distinct_action_family_count >= 6` 与 `passive_wait_share <= 0.25`，用于定位负担、无聊与节奏，不是通用最低通关量，也不直接阻断体验充分性。玩家主动离开后的授权执行与被迫盯页等待分别记录，不能机械以等待比例限制自治玩法。
- motivation/attraction evidence 至少记录 `hook_score`、`replay_intent`、`meaningful_decision_count`、`reward_or_unlock_count`、`stall_or_wait_periods`、`biggest_boredom_point`、`continue_reason` 与 `return_hook`；这些是 evidence DTO，不得写成 runtime 世界真值。
- `route_tradeoff` 必须影响后续至少 2 个 beat；`accelerate` 与 `stabilize` 至少在中途可见指标、事故后果或回访目标上不同，否则标记 `route_tradeoff_fake_choice`。
- 显式交付/委托 profile 必须产生可见、可命名、可交付成果，并推进同一 `local_demand_id` 的 before/after 进度；只有 reward ID 或静态成果卡不能通过。这不构成 `production_only` 首产物的隐藏前置，没有交付时不生成交付结论。
- 第二局首屏必须能恢复上一局 choice memory，并据路线、成果或修复选择生成不同的 `next_session_goal` 与 `first_action_on_return`。
- 连续 `step/wait/refresh` 等被动 CTA 即使推进进度，也必须由 boredom negative regression 判为 attraction weak；`quick_patch` 与 `root_cause_fix` 必须展示时间、保留进度、残留风险或稳定收益上的真实差异。
- required summary 必须标注 `runtime_backed / viewer_fixture_only / visual_only / live_verified` 等 provenance；共享 scenario driver 可用于 deterministic 回归，但 mock/fixture 不得冒充真实 0~30 分钟 live gameplay。

#### 选择可读性与 quote/preview 合同

- 早期 session 的 quote/preview 必须先服务一个当前主要决策，并突出一个主导 blocker 或成本；可恢复、不会改变当前选择的补充细节可以延后，但必须保留回看条件或时机。该仲裁只重排/解释同一权威事实，不得省略权威成本、改写动作语义或把推荐冒充成结果。
- 涉及损失、锁定、authority transfer、不可逆行动或恢复可用性变化的任何事实，必须从延后信息升级为当前显式决策信息；不能以“信息过载”掩盖高后果取舍。缺失标记 `early_preview_arbitration_missing`。
- `branch_offer`：每条推荐路线必须包含 `route_label / immediate_gain / future_beat_changed / risk_or_lockin / next_session_hook`，其中 `future_beat_changed` 必须明确第一个与第二个后续 beat 的实质差异，而不是泛称“会有更多选择”。
- 路线可回退时必须提供 `rollback_deadline_beat / rollback_cost_summary / rollback_kept_benefit / rollback_lost_benefit`；不可回退或无安全回退时必须同样显式说明。缺失标记 `route_rollback_quote_missing`。
- `Opportunity Scan` 的未推荐 hook 必须说明 `hook_value_summary / hook_readiness_state / defer_reason / unlock_precondition / revisit_timing_hint / value_vs_immediate_action`；缺失标记 `opportunity_discard_reason_missing`。
- starter frag 推荐必须说明粗粒度材质预期、可达性与第一工业目标关联，不承诺精确掉落或必然完成能力链。
- `ScheduleRecipe` 必须在提交前展示基础耗时、本地稀缺延迟和当前电力/battery 风险；仅当 runtime 存在权威维护状态时，才另外展示维护 sink、折旧压力、维护前后 runway、维护停机临界点与维护建议。电力/battery runway、`maintenance_pressure_delta = unchanged` 或执行后 receipt 均不能替代维护 quote。
- `RefineCompound` 必须展示质量、电力成本、hardware 产出、精炼后电力、目标缺口前后变化、推荐量与适用的 `refine_value_class`；三种词表的适用范围遵守上述净余额合同，当前共享 Data 模型不得显示建厂缺口改善。
- `market_quotes`、`TransferMaterial`、`ProductValidated` 分别必须解释采购来源与税费/运输成本、调运到达量/损耗/ETA/产线阻塞变化、验证后的用途/可交易性/能力解锁/下一步。
- `BuyPower`、`harvest_radiation` 或等待发电必须解释补电量、成本、恢复后状态、可行动 runway 与防停机收益；`SellPower` 必须解释收入、售电后 runway、下一动作可负担性和停机风险。
- `FragmentsReplenished` 或运行期补种必须解释当前余量、补种触发/时机/数量、等待成本、第一工业目标关联，以及等待、转移或替代材料路线。

#### TASK-GAME-076 自动化与证据边界

- required 入口保持 `./scripts/verify-gameplay-attraction-automation.sh --tier required`；live 玩家路径与 pure API 证据使用 `./scripts/verify-gameplay-attraction-automation.sh --tier live`。
- required tier 必须覆盖 scenario summary writer、weak-sample regression、route branch、second-run hook、anti-script、boredom negative 和 truth/provenance coverage；live tier 才能把真实 browser/player-path 或 pure API gameplay 标为 `live_verified`。
- 历史 required 样例曾记录 `34/30` 分钟内容与 `22/18` 次操作；它仅为结构与诊断参考。当前 verdict 由对应候选证据决定；汇总分别呈现 fixture 结构、候选 live 操作和真人观察，缺真人来源保持未验证。`meaningful_decision_count` 是动作/分支/打断等字段的结构代理，不能报告为玩家实际决策次数；真人选择按具体决策去重并说明后果，默认样例 hook/replay 分数不能代签真实回访意愿。
- runtime/viewer/agent canonical truth 或对应 surface 发生变化时，重跑 required；需要真实体验、provider 或 release/playtest 判断时，再跑 live/provider sample 并由 QA/producer 分别给出验证与阶段判断。

## 2.8 物理世界尺度与玩家交互尺度

oasis7 的世界不是无尺度表格。

当前正式口径必须同时成立两件事：

1. 世界物理真值有尺度，而且当前 canonical 空间坐标、距离与尺寸继续以厘米整数合同落地。
2. 玩家当前默认玩的不是 Minecraft 式第一人称逐块编辑，而是“通过目标、动作链和组织能力间接改变一个有物理尺度的世界”。

因此需要区分四层尺度语义：

1. `canonical physical scale`
   - 用来回答“世界真实存的是什么”。
   - 包括位置、距离、半径、尺寸等厘米真值。
2. `subsystem native resolution`
   - 用来回答“某个子系统实际按什么粒度工作”。
   - 例如 chunk / voxel / location / facility 可以比厘米更粗，但必须声明映射规则。
3. `player interaction scale`
   - 用来回答“玩家当前默认改世界，是通过什么粒度的动作”。
   - 当前正式主路线仍是 `agent/location/facility/recipe/governance` 这类间接控制动作，而不是 block placement / digging / local terraforming。
4. `presentation scale`
   - 用来回答“Viewer 为了可读性放大了什么”。
   - 视觉夸张允许存在，但不能冒充物理真值。

这意味着：

- `1cm` 是世界真值合同，不自动等于“玩家已经拥有 1cm 级直接操作主玩法”。
- coarse-grained 子系统不是违背尺度设计，而是需要被正式声明和审计。
- Viewer 的 marker 放大、semantic map、2D 抽象不等于世界尺寸改变。
- 过细动作只能映射到 canonical 可执行替代动作；没有安全替代时，必须安全停止、解释边界并给出下一次可决策点，不能由 UI 伪造动作或只返回无解释失败。
- 未来如果要引入 embodied / block-editing 候选能力，必须先证明它强化的是当前间接控制文明模拟主路线，具备对应专业域合同与验证，并经显式跨域决策；不得把产品切成第二套游戏或提前写成当前承诺。

产品承诺见 [`doc/product/world-rules-core-gameplay/prd.md`](../../product/world-rules-core-gameplay/prd.md)；本节保留玩法侧四层尺度合同与未来候选 guard。

## 2.9 成熟世界中的小玩家成长线

详细的独立成长、恢复、专业化、leverage checkpoint 与 anti-grind 玩法合同见 [成熟世界小玩家成长合同](./gameplay-mature-world-progression-contract.prd.md)。

补充口径：
- `10-minute trust gate` 负责回答“是否已经值得继续玩”。
- `first capability gate` 负责回答“首个持续能力是否已经闭环”，并继续与 `PostOnboarding` 的 `15~45` 分钟里程碑口径对齐。

## 2.10 间接控制下的 control-feeling 合同

oasis7 当前正式主路线不是 direct control，而是 indirect control。

这不等于玩家应该接受“AI 自己做决定，我只看结果”。

当前正式口径要求，玩家在间接控制主路线里仍然必须持续回答 4 个问题：

1. 我刚刚让系统做了什么。
2. 系统有没有接受这件事。
3. 为什么现在这样推进、没推进或改道。
4. 我现在最有效的下一步是什么。

如果任一问题不能在 headed Web/UI 或 pure API 的正式玩家 surface 上被直接回答，那么即便世界还在推进，也不能视为 control-feeling 合格。

因此当前 gameplay 正式冻结 4 条 guarantees：

1. `accepted intent`
   - 玩家必须能读到当前被接受的主意图，而不是只看到原始事件流。
2. `execution causality`
   - 玩家必须知道当前是执行中、被阻塞、被 override、无进展完成还是有进展完成。
3. `interrupt / reprioritize / recover`
   - 玩家必须能打断、重排或恢复 agency，而不是只能等 AI 自己继续。
4. `bounded consequence readability`
   - 玩家必须看懂当前代价、世界变化、主阻塞与最短下一步。

这 4 条 guarantees 的专题合同见 `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md`。

当前 defer 范围：
- 暂不继续扩大高风险对抗/治理/元进度在首局中的曝光。
- 暂不把新的宏系统入口包装成 first-session 主卖点。
- 暂不把 `--no-llm` 调试 lane 重新定义为正式游玩入口。
- 暂不把 Prompt Ops / operator-only 入口作为默认玩家主路径。

early-retention 产品承诺见 `doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md`，专业玩法合同见本文件 2.7.1，当前 verdict 由同候选 PR 与实际验证记录 与 `doc/testing/evidence/` 确认。

## 2.11 纯 API 客户端等价

纯 API 客户端不应该只是“能看日志、能发 step”的调试通道，而应是正式玩家入口之一。

设计原则：

1. 纯 API 与 Web/UI 必须共用同一套世界事实、阶段目标、阻塞解释和下一步建议。
2. 允许表现形式不同，但不允许信息粒度和动作能力降级。
3. 如果 UI 能告诉玩家“我现在在哪个阶段、为什么被卡住、下一步该做什么”，那么 API 也必须能告诉玩家。
4. 如果 UI 能继续玩到中循环，API 也必须能继续玩到中循环，而不是停留在首局或探针模式。

产品模式、证据隔离与发行组合验收见 `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`；本节继续拥有 pure API 持续玩法等价的专业规则。

## 2.12 Future：玩法 mode 的参与与准入可读性（未实现）

这是后续目标，不改变当前 `GameplayModeReadiness`、runtime 或 UI 行为，也不承诺匹配、排队、邀请或自动补人。未来若某个 surface 暴露 mode 进入，必须把**当前参与状态**与**这次准入**分开：对同一 mode 的所有 active gameplay module，`(min_players, max_players)` 必须完全相同（`min_players == max_players` 是合法固定人数 mode）；任一边界不一致即 fail closed，不做 min/max、交集或其他聚合。

- 当前参与状态只说明已有玩家：`current_players < min_players` 为 `waiting_for_players`，否则最低人数已满足；有限上限且 `current_players >= max_players` 时同时为 `full`。它不决定最后一个空位能否被准入。
- 准入是 prospective：模块 ready 且有限上限满足 `current_players < max_players` 时可准入，故即使当前仍 `waiting_for_players`，最后一个空位也可被加入；无上限时只要模块 ready 即可准入。`current_players >= max_players` 必须拒绝为 `full`。
- `enterable` 只能由“模块 ready + prospective 可准入”得出。模块未 ready 时必须明确模块/缺失原因，不得伪报为等待或满员；等待时必须说明仍缺人数但不虚构 ETA；满员时必须说明容量原因并给出返回其他当前可进入 mode 的恢复路径。人数重判不得隐式替玩家加入或退出。

---

## 2.13 经营与模拟游戏的参照取舍

以下是设计分析，不是竞品规则移植或 oasis7 体验已验证结论。已有的因果、授权、分支与恢复合同优先，参照用于寻找反例和细化玩家判断，不另立权威。

| 参照与可核对来源 | 借鉴的设计判断（本项目推论） | 本项目落点与不采用的部分 |
| --- | --- | --- |
| Factorio [双向配方百科](https://www.factorio.com/blog/post/fff-397)、[生产统计](https://wiki.factorio.com/Production_statistics) | 先看“需要什么/有什么用”，再定位吞吐约束；自动化释放重复劳动 | 沿[首局具体样本](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#first-session-concrete-sample)解释投入、用途与下一决定；不复制单人全知资产权限、配方数量或无限扩张目标 |
| RimWorld [最小模拟与故事](https://tynansylvester.com/2013/06/the-simulation-dream/)、[失败后继承旧遗址](https://ludeon.com/blog/2024/03/anomaly-expansion-and-update-1-5-announced/) | 具体对象的后果与有代价的失败后续，比随机事故数量更重要 | 沿现有历史、恢复与[危机参与边界](#crisis-voluntary-cooperation)审阅；不由参照开放复活、债务重置、性格/关系或继承权限 |
| 戴森球计划 [官方产品描述](https://store.steampowered.com/app/1366540/Dyson_Sphere_Program/) | 从单设施到生产模块/供给关系，规模变化应改变决策单位 | 沿[经营与续玩设计](../../product/world-rules-core-gameplay/first-session-and-continuation.design.md#gameplay-experience-priorities)审阅；不以更多重复设施或全局远程控制冒充成长 |
| 缺氧 [官方生存与模拟定位](https://www.klei.com/games/oxygen-not-included) | 供给余量、耦合后果与不依赖失效链的恢复 | 先用现有输入、电力、输出和权限约束检查[数值方法](gameplay-top-level-design.design.md#balance-review-method)；环境负担只是后续候选，不一次新增气液热、疾病与心理全模拟 |

当前 Smelter 首产物打开 Assembler 候选，不等于已建成第二条产线。每增加一种实体或规则，先说明它带来的不同选择和代价；只增加字段、配方长度或维护点击而没有新判断时，优先简化。后续社会/环境示例不成为当前首产物验收前置。

### 2.13.1 复杂度预算：新增内容先回答什么

这是 gameplay 方案评审问题，不是新的全仓准入或强制表单。评价单位是相关决策循环，不要求每个实体各有两条策略或独立数值。

- 新资源、设施或规则让玩家决定什么？它与既有选择的差别是否来自真实用途、成本、时机或风险，而不是改名、更多点击或更长等待？
- 在相关循环内，谨慎维持、追求产能、专业化或自愿协作等替代办法是否仍有可解释价值？一个适用情境只有一个合理解不自动失败，但整段体验只有同一必经操作时需要调整。
- 新概念在何时才必须理解？用一个具体后果能否解释，还是要先读完整协议？早期先暴露会改变当前决定的内容，详细事实仍可回查。
- 它会产生什么可读的失败与恢复问题？只引用现有 authority 支持的处置；无适用失败机制时说明不适用，不为了丰富故事加随机惩罚或免费救援。
- 如果移除这项候选机制，选择是否基本不变？若是，优先推迟或简化机制/呈现，不因此删除已存在的人物、历史或玩家资产。

例如十个只换名称、后果完全相同的新资源未必增加决策；十个有真实参与历史的 Agent 也不必各自再造性格倍率。环境副作用若只要求不断补耗材而没有可比较的生产/治理取舍，先不扩展模拟；物流升级若只是重复摆放更多同样设施，先检查是否真正减少操作负担并改变供给关系。

# 第三部分：爽点曲线模型

## 3.1 爽点来源分类

### 1. 控制感

- 目标被成功执行
- 协议调整或供给安排取得预期效果

### 2. 逆转感

- 劣势翻盘
- 组织重组后崛起

### 3. 创造感

- 新模块被广泛使用
- 自己的制度成为标准

### 4. 区域影响感

- 区域控制
- 资源垄断

### 5. 危机生存感

- 风暴中维持能源
- 危机中保住核心设施

---

## 3.2 爽点曲线结构

### 阶段 1：探索兴奋期
- 快速成长
- 学习系统
- 小规模成功

### 阶段 2：压力增长期
- 供给压力增多
- 风险变高
- 决策复杂化

<a id="阶段-3政治博弈期"></a>
### 阶段 3：区域能力与可选协作
- 深化独立产线、韧性或区域服务
- 在真实需求与授权存在时选择分工、交换或联盟
- 愿意参与者进入治理取舍，不把政治竞争设为持续成长的必经阶段

### 阶段 4：可选的文明尺度协作
- 在受影响范围内自愿推动协议升级
- 选择参与治理或保留独立区域路线
- 形成可审计的共同后果，而非默认取得世界规则控制权

---

# 第四部分：协作与治理系统设计

## 4.1 协作设计原则

- 协作必须有成本
- 协作必须有可审计承诺
- 协作必须能解释收益与风险
- 协作不能退化成单一最优路线

---

## 4.2 协作形式

### 供给协议
为能源、材料、制成品和维护能力建立短期或长期供给关系。

### 资源共享
在组织、区域或项目目标之间分配资源和维护窗口。

### 治理提案
通过提案调整规则、优先级和风险承担方式。

`OpenGovernanceProposal` / `CastGovernanceVote` / 改票若影响治理提案结果，必须展示 `governance_vote_quote` / `proposal_outcome_preview`：剩余时间、quorum/pass 缺口、玩家票权影响、可能结果变化、通过后的规则/优先级变化、失败或冷却代价和推荐治理动作；玩家不应只看到提案日志、阈值参数或最终结算。

### 战争宣告
`DeclareWar` / 调整战争强度若影响联盟冲突，必须展示 `war_declaration_quote` / `conflict_outcome_preview`：双方联盟、推荐强度、持续时间、双方评分估计、预计胜负、冲突窗口占用、重入阻塞、结算风险、替代行动和推荐宣战动作；玩家不应只看到宣战事件或最终战报。

### 危机响应
在供应中断、设施故障或链上异常时组织修复、让渡和恢复。

---

## 4.3 协作平衡机制

- 承诺需要资源或信誉成本
- 违约、失约和延期必须留下事件证据
- 治理门槛和冷却窗口限制频繁改约
- 投票影响、策略更新和声誉收益必须经过授权并受反滥用、反刷取和反通胀边界约束；具体权重、阈值与奖励公式由当前 runtime / balance 真值维护，不在产品或玩法总览中冻结历史数值。
- 危机响应优先保护可恢复性，而不是放大不可逆惩罚

---

## 4.4 政治系统

### 组织协作系统

- 多玩家形成组织或协作关系
- 内部协议可制定
- 可投票决策

### 提案机制

- 提交规则变更提案
- 需满足投票权门槛
- 通过后进入升级流程

### 内部分歧

- Agent 内部可能存在分歧
- 组织内部投票可能失败
- 退出、拆分或重新授权可能发生

---

# 第五部分：前 30 天新手体验流程

## 第 1~3 天：生存学习

- 学习能源循环
- 构建第一个稳定发电模块
- 做出首个制成品
- 理解时间消耗机制

目标：建立稳定能源基础，并第一次看到世界被自己加工。

---

## 第 4~7 天：扩张阶段

- 建立第二个能源节点
- 建成首条稳定生产链
- 落成首座工厂单元
- 开始空间移动
- 以低承诺的交易/服务、互助或信息交换方式首次接触其他玩家；是否进入组织或治理必须留到后续显式升级动作

目标：第一次拥有持续工业能力，并在不强制站队的前提下理解合作可能带来的价值；组织、治理或更高承诺的扩张取舍仍需等到玩家主动升级。

### 首次社交接触预览合同

第 4~7 天的首次社交入口必须在玩家确认前提供 `first_contact_preview` / `social_contact_quote`，把“接触其他玩家”收成一个可理解、可延后的下一步，而不是默认加入组织、承担治理义务或开启宏观外交。每个候选接触必须展示：

- `contact_purpose`：本次要解决的当前本地问题，例如出售少量产出、请求一次运输协助或交换路线/价格信息。
- `expected_mutual_value`：玩家与对方各自立即可见的收益，不得只写“建立关系”。
- `risk_or_commitment`：本次最多暴露什么资源、时间、信誉或后续义务；低承诺接触不得隐含组织身份、治理票权、长期供给或排他协议。
- `solo_lane_preserved`：明确完成、拒绝或延后该接触后，`local operator -> regional specialist -> limited-scope regional influence` 的独立路线仍可继续，且不会失去当前工业/服务主目标。
- `recommended_contact_action`：推荐的具体接触动作及其理由，或推荐保持独立推进。
- `defer_reason`：若当前不适合接触，说明可以延后的原因、保留的收益和下一次值得回看的触发条件。

`first_contact_class` 只能取以下枚举：

- `trade_or_service`：一次性或范围受限的交易、服务交换。
- `mutual_aid`：围绕当前 blocker 的可回收互助，不建立持续成员关系。
- `information_exchange`：交换路线、价格、风险或机会信息，不承诺资源、票权或组织身份。
- `defer_contact`：当前独立路线更优或风险不可接受时，保留延后理由与回访条件。
- `organization_escalation`：明确加入组织、承担长期供给、治理或排他承诺；此类动作不是第 4~7 天首次接触的默认结果，必须作为第 8~14 天或之后的独立确认步骤展示。

如果首次社交入口没有 `first_contact_preview`，或把 `trade_or_service`、`mutual_aid`、`information_exchange` 默认升级为 `organization_escalation`，标记 `first_contact_preview_missing`。该合同只定义当前 limited playable technical preview 中的玩家理解与选择边界，不新增组织、治理、外交、社交图谱或 runtime/viewer 实现范围。

---

## 第 8~14 天：可选协作或区域服务

- 生产首个可交易工业品
- 将能力转成一次区域服务、互助或资源共享贡献
- 可选择建立组织协作关系或继续独立路线
- 在实际影响共同承诺时才作出治理或供给调整

目标：把工业能力转为新的区域用途、恢复弹性或协调位置；协作与治理是可选扩展，不是阶段通关条件。

---

## 第 15~21 天：区域影响或改道

- 建立中型能源网络
- 提供或恢复区域服务
- 获得有限且可审计的局部机会、可见度或协调位置
- 按当前世界状态继续、修复、重建或改道

目标：形成不依赖 major power 的区域价值；影响不等价为全局治理权。

---

## 第 22~30 天：自愿文明参与

- 可选择推动协议升级提案
- 可选择组织多玩家行动
- 也可继续深化区域服务、恢复能力或独立路线

目标：在受影响范围内形成有边界、可审计的共同成果；不是从参与者到全体世界决策者的强制阶梯。

---

# 第六部分：长期沉浸设计

## 6.1 不完全信息

- 不公开全部资源数据
- 情报需侦察获得
- 谈判存在欺骗空间

---

## 6.2 周期性危机

- 小行星风暴
- 能源衰减周期
- 链上分叉危机

<a id="crisis-voluntary-cooperation"></a>
Trace ID: O7-REQ-3f259d96-508e-462d-a8c9-e1851e85e1ed

危机可以让合作更划算，但不得把加入组织、让渡权利或参与治理设为唯一可继续的默认解。设计应保留有真实代价的独立恢复或停止当前目标/退出当前活动的路径；代价、仍保留的价值、损失范围与下一决定必须可解释。合作优势来自可说明的分工、资源或风险分担，不来自暗中封锁独立恢复权限。

这不是保证生存、全额退款或救回每项资产。没有安全恢复时如实说明不可保全的部分，以及当前受支持的止损、重新定目标或退出选择；不得发明免费救援或后台重置。关闭会话、停止未来代办、放弃目标和退出组织分别按现行合同处理，均不抹去已消费成本、债务、对方权利、归属/托管义务或结果未知的已提交操作。独立与协作路线切换不重置危机、不复制补偿，不自动扩张授权。

适用验证至少对照独立经营者和协作参与者：都能读懂当前合法选择与代价；独立者不会因拒绝入会失去唯一止损入口，协作退出不撤销已发生的权利义务。具体恢复、组织连续性、死亡与结算仍由各专业 authority 决定；本条不新增惩罚、复活或债务公式。

---

## 6.3 历史记录系统

- 记录重大治理调整
- 记录组织协作关系形成
- 记录工业中心与工厂网络形成
- 记录协议升级

形成可追溯的世界历史。玩家关心的故事先来自具体对象、承诺、选择与真实后果；叙事摘要不能凭空增加关系、债务、功劳或世界状态，也不把制度日志的存在当作人物故事已成立。

---

<a id="第七部分沉迷设计原则"></a>
# 第七部分：可持续投入与自然停点

长期吸引力来自已完成的有限成果、理解后的新选择以及值得回看的世界变化，不以目标永远做不完、焦虑或强迫在线维持。

- 每段主目标有可确认的完成/停止边界；下一目标可选择、改道或暂缓，不以领取本次成果为筹码强迫继续。
- 离场前区分已结算成果、有效未来委托、已提交待决工作和仍在推进的世界风险。停止未来代办不暂停世界，也不撤销未知提交；未知暴露保持未知，不许诺没有依据的离线收益或全额保护。
- 回来先恢复成果、条件变化和下一有效决定，不用错过签到、错过强制危机或没有及时加入组织解释失败。确有已确认的期限、义务或内生风险暴露时仍诚实显示其后果；不参与组织不等于危机免疫。
- 自主离场、平静建设、观察 Agent 与主动回访分别记录；在线时长、确认次数或未完成事项数量不能单独代表乐趣。通知与离线推进仅按现有能力，不在此新增提醒服务或暂停机制。

---

# 第八部分：可玩性评审组织方案

## 8.1 评审目标

- 以可操作证据确认微循环（5~15 分钟）、中循环（1~3 天）、长循环（数周）是否可验证。
- 识别“有体验目标但无工程观测”的断层，避免只靠叙事判断可玩性。
- 形成进入下一轮数值和模块迭代前的评审结论、阻塞项与责任人。

---

## 8.2 评审输入包

- 玩法与目标基线：`doc/game/gameplay/gameplay-top-level-design.prd.md`；工程实现边界见 `doc/world-runtime/prd.md`。
- Gameplay 生产落地证据：
  - 玩家侧治理、战争、危机与元进度合同由本文对应章节、`doc/game/gameplay/gameplay-war-politics-mvp-baseline.design.md` 和当前测试矩阵承接；战争仍不是当前玩家-facing 主线。
  - 生命周期协议、tick 推进、模块 bootstrap/readiness 与 replay/恢复边界由 `doc/world-runtime/prd.md#gameplay-生命周期协议边界` 承接；历史 layer closure 实现过程从 Git history 与 PR 与实际验证记录 追溯。
  - 模块驱动生产切片的完成状态由本专题 T3、下方测试矩阵、当前代码与回归测试共同承接；原增量 closure 三件套已退役，历史实现过程从 Git history 与 PR 与实际验证记录 追溯。
- 测试入口与执行规范：`testing-manual.md`（S1/S2/S3/S6/S7）。

---

## 8.3 评审流程（90 分钟）

- 00~15 分钟：回放范围与目标，确认“微/中/长循环”的判定口径。
- 15~45 分钟：演示微循环与中循环样例链路（动作 -> 事件 -> 状态 -> 可见反馈）。
- 45~70 分钟：对照测试与日志验证“可复现性”（非一次性演示）。
- 70~90 分钟：汇总结论、记录阻塞项、指定下一轮 owner 与截止时间。

---

## 8.4 可验证性判定门槛

- 微循环可验证：玩家在单次会话内可完成“观察 -> 决策 -> 反馈 -> 调整”闭环，且对应链路可由 S3/S6 证据复现；对前期工业链路，至少能复现一次“首个制成品”或“停机 -> 恢复 -> 继续产出”样例。
- 中循环可验证：工业扩张、协作、治理或危机恢复至少一条 1~3 天目标链路可重复达成，并能用运行时状态与事件历史解释结果。
- 长循环可验证：30 天路径能证明可恢复能力、区域用途或有限区域影响中的阶段成果具有连续积累证据，并保留继续、改道或自愿共同扩展的空间，不依赖人工临时修正。

---

## 8.5 评审输出模板

- 结论：通过 / 附条件通过 / 不通过。
- 证据：对应测试命令、关键事件、状态快照路径。
- 问题清单：按 P0/P1/P2 标注影响范围与复现路径。
- 行动项：每项包含 owner、计划完成日期、回归测试入口。

---

# 第九部分：爽点曲线量化指标映射

## 9.1 指标设计原则

- 状态、成本、吞吐和动作指标应能回指权威事件；理解、偏好和继续意愿必须另取玩家表达或行为证据。两者并列解释，不用事件计数替代感受，也不用主观评分证明世界结果。
- 指标必须按“微循环/中循环/长循环”分层，避免只看单一留存。
- 指标必须能对应玩法动作（工业、协作、治理、危机、元进度），用于定位失真环节。

---

## 9.2 核心指标矩阵（v1）

下表保留既有首轮目标数值，作为待按适用人群和阶段校准的设计参考，不声明已接线、已达到或已验证能提高留存。每项分别记录“定义可计算、数据接线可用、已观察、已校准”；缺数据、样本为零或来源不可靠时为未知，不填零。主动参与危机应对、治理与元进度的体验指标，应按实际开放能力与参与者说明分母，不是首局或独立路线的新门槛。被动受灾、既有债务与适用治理规则的影响仍须观测，不因玩家未主动参与而排除后果或授予豁免。

| 维度 | 指标 ID | 计算口径 | 事件数据源 | 目标区间（首轮） |
|---|---|---|---|---|
| 留存 | `retention_d1` | D+1 回访玩家数 / 当日新增玩家数 | 会话日志 + 玩家活跃快照 | `>= 35%` |
| 留存 | `retention_d7` | D+7 回访玩家数 / 当日新增玩家数 | 会话日志 + 玩家活跃快照 | `>= 15%` |
| 前期成长 | `first_finished_good_time_p50` | 新玩家从首次有效输入到获得首个制成品的 p50 ticks | 生产完成事件 + 玩家进度快照 | `<= 1800 ticks` |
| 工业稳定 | `stable_line_rate_24h` | 24h 内至少拥有一条稳定生产链的活跃玩家占比 | 产线状态变更事件 + 库存快照 | `>= 55%` |
| 工厂开工 | `factory_uptime_100ticks` | 工厂单元在最近 100 ticks 内的平均开工占比 | 工厂状态事件 + 工厂快照 | `>= 70%` |
| 停机可解释性 | `production_blocked_reason_coverage` | 停机状态中带明确原因分类的占比 | Viewer/runtime 状态快照 | `>= 95%` |
| 危机触发频次 | `crisis_freq_100ticks` | `CrisisSpawned` / 100 ticks | `DomainEvent::CrisisSpawned` | `1 ~ 6` |
| 危机恢复质量 | `crisis_recovery_rate` | 成功恢复或替代完成数 / 危机触发数 | `DomainEvent::CrisisResolved`、`DomainEvent::CrisisTimedOut` | `>= 65%` |
| 协作活跃度 | `cooperation_active_rate_24h` | 24h 内发生治理投票、经济合约或资源共享动作的组织数 / 组织总数 | `DomainEvent::GovernanceVoteCast`、`DomainEvent::EconomicContractOpened` | `>= 45%` |
| 治理参与度 | `governance_participation_24h` | 24h 内治理唯一投票者数 / 24h 活跃玩家数 | `DomainEvent::GovernanceVoteCast` | `>= 40%` |
| 危机参与质量 | `crisis_resolve_success_rate` | `CrisisResolved(success=true)` / `(CrisisResolved + CrisisTimedOut)` | `DomainEvent::CrisisResolved`、`DomainEvent::CrisisTimedOut` | `45% ~ 75%` |
| 长期成长 | `meta_progress_velocity_24h` | 24h 内人均元进度积分增长 | `DomainEvent::MetaProgressGranted` | `>= 8` |

说明：
- `first_finished_good_time_p50` 用于衡量新玩家多久才能第一次确认“我真的加工了世界”。
- `stable_line_rate_24h` 与 `factory_uptime_100ticks` 共同衡量前期工业成长是否从一次性成功过渡到持续能力。
- `production_blocked_reason_coverage` 直接对应“动作已接受但没有前进”的反馈缺口，必须优先盯。
- `crisis_freq_100ticks` 的历史区间只支持适用危机场景的节奏研究；低于下限不自动说明无聊，不得为凑频次制造事故。内生连锁后果与专门注入事件分开观察；没有危机也可有建设、优化和发现的有效选择。
- `cooperation_active_rate_24h` 是“社交治理是否真实发生”的主观测替代指标。
- `crisis_resolve_success_rate` 的中间区间是历史研究参考，不得为命中区间压低合法恢复成功率或制造失败；更高成功率也可能来自有效规划，应结合成本、理解与策略差异解释。

---

## 9.3 指标回看节奏与告警

- 日看板：`retention_d1`、`first_finished_good_time_p50`、`stable_line_rate_24h`、`production_blocked_reason_coverage`。
- 周评审：`retention_d7`、`factory_uptime_100ticks`、`crisis_recovery_rate`、`governance_participation_24h`、`meta_progress_velocity_24h`。
- 在同口径且有足够样本的连续 3 个观测日，按每项比较方向判断是否偏离：上限指标超上限、下限指标低于下限、区间指标越界；首产物更快不能因“低于下限”误报警。偏离触发原因分析而非自动改规则；缺数据先修观测，不把未知判成玩法失败。样本门槛与统计方法尚未确定时只作描述性回看，不开启统计显著性告警或声称改善显著。

---

### 9.3.1 时钟、分母与比较单位

- 世界 tick 是规则时间，不默认等于秒；墙钟包含玩家等待、模型/provider 延迟、网络与暂停。记录首次进入成本、首次有效输入、matching 首产物结算、会话离场及实际 tick 推进区间，分别报告安装/登录、世界内游玩与暂停等待，不能转换出未声明的分钟承诺。
- 留存使用同一玩家 cohort 与明示时区/日界；24h 与 100ticks 不混用。报告样本量、观察窗口、入口/profile、未完成与失访范围。首产物 p50 同时报告尚未结算比例和观察截止点，不能删掉未完成者后宣称首局改善。
- 稳定链按其同一产线身份及 canonical W/完整 cycle 条件判断；库存增长、生产过一次和开工率不代签。开工率需声明运行、阻塞、合法暂停、无需求空闲及未知时间的分母，合法保留余量或无订单不自动等于玩法失败。
- 危机恢复比例按同一触发 cohort 追踪终态与未决，不把跨窗解决数除以本窗新触发数；成功恢复和有代价的目标放弃分别列出，不能靠退出粉饰成功率。

## 9.4 历史战争/政治数值基线边界

- 战争/政治数值基线已规范为专业 design 权威 `doc/game/gameplay/gameplay-war-politics-mvp-baseline.design.md`，用于 runtime 兼容、专业验证和未来受控重启参考；产品层不复制数值真值。
- 当前玩家-facing 总纲不再把战争作为主玩法承诺；当前主线优先工业、经济、协作、治理与危机恢复。

---

# 终章

这是一个：

- 战略博弈场
- 政治演化系统
- 文明竞争模拟
- AI 行为生态

技术可运行只是前提。体验是否成立，要看玩家能否理解自己促成的结果、作出有意义的下一次选择，并在想离开时清楚保留成果与剩余风险。愿意回来优化供给、继续建设或观察故事是有价值的信号；熬夜、焦虑与持续担忧不是成功标准。

## 9.5 市场决策与交换

- 当市场动作会改变当前工业目标时，预览必须说明 offer/limit price、结算资源与数量、ownership/eligibility blocker、相关库存或 power runway、当前会成交还是保持 open、可取消/恢复动作以及它与下一目标的关系。
- 模块 artifact 是有 owner 的可转移能力：玩家可 offer、bid、purchase、delist 或 destroy，但 active use、权限、资源不足或不兼容价格必须形成可读拒绝与下一安全动作。
- power 交易必须继续满足 `power_survival_quote` / `power_sale_quote`：展示交易后的续航、下一动作可负担性和 interruption/shutdown 风险，不能只展示现金收益。
- offer/order 不承诺成交或 escrow。权威 runtime 决定资格、报价、price/time 确定性撮合、结算与回放；未成交订单保持可取消，失败至少区分资金不足、ownership/permission、价格不兼容、active-use destruction blocker 或当前无匹配。
- 本文不冻结 action/event 字段、订单 ID、费用公式或价格带数值；这些属于 runtime/simulator 实现和专业验证真值。

