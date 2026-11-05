# 成熟世界区域供应与下游用途证明系统设计

- 生命周期：`active`；本文冻结目标技术合同，不声明当前实现、实玩验收或部署就绪。
- Owner role：`runtime_engineer`；协作：`gameplay_designer`、M4 工业与物流 owner、身份/World Service owner、`agent_engineer`、`viewer_engineer`、`qa_engineer`；Last reviewed：2026-11-05。
- 固定审读基线：`nil-studio/oasis7@b65af49ca13f7b23f9b85a6209c21a01a4b0f61b`，tree `baac82a099080aa4e459d8147de437cd2f4c7f69`。源码事实、目标适配和静态算例分别标明；没有 Cargo、WASM、游戏、模型、正式客户端或故障注入运行证据。
- 上游：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)、[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)、[专业供应用途政策](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#regional-supply-use-policy-v1)。验证：[专用手册](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m01)。

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce。

## 1. 问题、目标与非目标

当前成熟玩法读面可以按 action ID 描述“本地需求”和“区域价值”，但没有把具体受益人的当前需求、玩家实际供应、接收方实际使用和回访结论连成可核验的因果链。库存、累计产量、MetaGrant 或一次运输成功都不能独立证明这种价值。

首版 profile 为 `native_regional_supply_use_v1`：已有首次持续能力的 A 自愿、无对价地从合法旧库存供应 2 iron_ingot，原物流将其送到独立主体 B 的实际工厂 input Site；原 SC-31 输入目标 G 只结算匹配交付。B 再显式选择其独立用途 U，实际消耗这 2 iron_ingot 完成一批原生 gear。Runtime 依据原事实，只读证明“A 的供应被 B 用于这一具体制造用途”，并在历史完整时区分首次用途与重复用途。A 的旧库存不被改称新产物，也不要求 A 为了贡献重复生产。

成功结果包括需求和成本可读、原运输和需求结算单次发生、下游实际消耗与完成可归因、回访不重跑业务，以及仍有当前真实的持有、退出或独立继续选择。该切片只覆盖 MW-001 的一种有限区域用途；它不证明全部成熟阶段、议价权、恢复弹性、长期平衡或完整端到端产品验收。

本设计不建立第二份 `RegionalDemand` 数量账，不引入材料 lot/新 escrow，不产生积分、币、奖励、认可、机会分配、治理资格、charter 或材料所有权转移给 A。跨区运输、付费服务、许可/版税、levy、多需求匹配、代办 Site 出货、自动补产、自动改道和任意终端结算均不属于首版。

## 2. 上游约束与相关角色

Gameplay 拥有阶段、贡献、首次/重复用途和继续选择的含义；SC-31 拥有目标身份、数量分层、匹配结算及取消/到期；M4/物流拥有原料、生产、电力、容量、损耗与费用。身份和 World Service 拥有真实个人、Agent 当前控制与正式接受上下文。Runtime 连接这些原 authority，维护原事务内的关联事实、完整索引和授权只读投影；Viewer、API、Agent 不作第二次结算。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [AC-MRC-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-01)；Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce | 原首次持续能力之后，以独立受益人、当前真实 G/U 和准确地点范围接受一次旧库存供应 | [阶段、需求与身份](#contribution-need) | 原稳定线历史、SC-31、个人/Agent 与 Site authority | action 标签、库存数或两个 Agent 名称不代签阶段和独立主体 |
| [AC-MRC-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-02) | 只读比较成本与不可逆边界，显式确认一个有限供应，不隐含回报 | [预览与有限承诺](#contribution-preview) | 原 quote、当前 G/U、共享物流容量和正式提交 | preview 不占材料、顺位、容量或创造需求 |
| [AC-MRC-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-03) | 原运输守恒；只有原匹配交付按实际量减少 G 一次 | [交付与匹配结算](#contribution-delivery) | 原 transit/receipt/账本 writer、SC-31 匹配 adapter | 产量、到货提示和普通 receipt ID 不自动完成 G 或父目标 |
| [AC-MRC-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-04) | B 完整有限输入窗口证明唯一来源，独立 U 的实际 sink 与完成共同证明用途 | [实际使用与来源](#contribution-use) | 原 RecipeStarted/Completed、全 writer 索引、用途授权 | 到货、库存增长、U 接受或不完整事件页不算实际使用 |
| [AC-MRC-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-05) | checkpoint 只读；首次/重复依 canonical 历史，不靠改名重置或累计产量 | [只读 checkpoint](#contribution-checkpoint) | 原事实投影、历史首次用途证明、既有反 grind 规则 | 不发行权益、奖励或新治理资格，不创建第二 claim 账 |
| [AC-MRC-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-06) | 供应后仍可持有、退出或按真实 quote 独立继续；新需求须显式重新选择 | [回访与独立继续](#contribution-continuation) | 原当前能力/预算/可选动作、用途 producer | 非成员标签、虚构订单或未付费用不证明独立可继续 |
| [AC-MRC-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-07) | 当前权限和历史归因分开；未知先对账，恢复不复制业务或延长窗口 | [状态、事务与恢复](#contribution-recovery) | 正式执行、当前 audience、原持久 generation 和索引覆盖 | 旧 owner 不授予当前权限，重放不补造新贡献或授权 |
| [AC-MRC-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-08) | 同一候选的 Runtime、World Service、Viewer、pure API、Agent 消费同源事实与真实下一步 | [正式消费者](#contribution-consumers) | 各入口 adapter、QA 同候选证据 | 共用 DTO、静态算例、文档检查和旧 CI 不代签正式闭环 |
| [REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce | 原成熟阶段的持续交付、扩容取舍与实际新选择/用途，不重做原领域规则 | [成长组合](#contribution-continuing-growth) | 原SC-31/服务、SC-25/Build、认可/权利authority | 原v1仍只证明有限用途；组合不发奖励、升级参数或新认可 |
| [REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce | 四方向有界来源、旧义务/首次历史、当前读权及原operation恢复 | [组合预算与验证](#contribution-growth-composition-budget) | 同cut来源、原authority/恢复、Viewer/API/Agent | 不按profile增开资源池，不以未知默认零或缺候选证明必须赞助 |

## 3. 当前状态、目标状态与差距

| 对象/能力 | 固定基线的源码事实 | 本设计目标与未交付接缝 |
| --- | --- | --- |
| 成熟价值呈现 | [branch_commitment::first_delivery_preview](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/branch_commitment.rs) 按 action ID 提供固定 local_need、leverage、return 文案；[gameplay_snapshot_lane](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot_lane.rs) 按 goal_kind 填 leverage 分类 | 替换本 profile 的事实来源；尚无真实 G、原供应与 B 使用的生产者/消费者闭合，其他旧展示不因此成为已验证贡献 |
| 元进度 | [PreparedMetaProgress](../../../crates/oasis7_world_state/src/world/governance_meta_publication.rs) 累加 track/total points，[M5 meta](../../../crates/oasis7_builtin_wasm_modules/m5_gameplay_meta_progression/src/lib.rs) 按阈值产生后续 directive | 保留原用途；不消费 points 作为区域需求、供应来源或首次用途 |
| 原持续能力 | [factory_has_canonical_stable_line](../../../crates/oasis7_world_state/src/state/industry_transition/mod.rs) 要求 repeat count 至少 3 且 canonical snapshot 与 recipe ID 匹配；[recipe lifecycle](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs) 更新完成事实 | 复用原判据，但需 exact factory 的历史 build/control、三个连续开始/完成与完整窗口读取；当前 helper 和 [current_stable_line](../../../crates/oasis7_world_state/src/gameplay_capability.rs) 不单独证明历史首次阶段 |
| 原工业需求 | [SC-31 产品设计](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.design.md#1-设计命题) 已拥有目标与 matching settlement 语义 | 本版有限 G/U 绑定、原目标接受/修订/取消/到期和单次匹配尚待实现；不能称为现成可调用的目标 API |
| 地点与 Site | [LocationAnchorV1 / FactorySiteAuthorityV1](../../../crates/oasis7_world_state/src/state/industry_state.rs) 有 exact location、active、revision、owner、allowlist、chunk_ready；location anchor 没有通用 region membership | 两个 Site 指同一有效 canonical anchor，作为本版有限范围；不猜坐标或新增 charter 空间权 |
| 原物流及当前授权 | [TransferMaterial](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_core.rs) 解析原路线并检查材料、电费和在途；[agent_delegation](../../../crates/oasis7_runtime/src/world/agent_delegation.rs) 有当前 own-Site 路径，但带 grant 的 Transfer 仅支持 Agent 源账本 | 本版只走当前 owner 的正式 Site 出货；原接受点需关联 beneficiary/control generation；Agent 字段或 route owner 本身不足 |
| 原物流结算 | [transit reducer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs) 扣源料、保留 tariff escrow、占路线，完成时入目标账本、释放并付 route owner；[LogisticsSettlementReceiptV1](../../../crates/oasis7_world_state/src/state.rs) 主要保存 job/path/routes/tariff/payout | 必须连同原 start/complete、settled index 和接受身份读取；现有结算 receipt 不单独包含完整材料和个人归因 |
| 原生下游制造 | [recipe catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs) 有 `recipe.assembler.gear`：2 iron_ingot → 1 gear，4E；[ScheduleRecipe](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs) 以实际 Site 输入/输出、工厂标签/slot、builder 电力等准入 | 可复用原消费和完成；原 job 尚不提供本 profile 的 G/U 绑定和供应用途证明，不自动结算原需求 |
| 验证、维护与费用 | [economy resolver](../../../crates/oasis7_runtime/src/world/economy.rs) 优先 exact builtin product module，再找 active product suffix fallback；[bootstrap_economy](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs) 没有 gear 默认 product profile/内置 validator，已有 profile 不被默认覆盖；有效投入会加维护 sink | 必须有受批准的 pinned gear policy 明确 not_required、无适用 active fallback、维护为空；缺配置不等于免费。任何实际模块调用仍按 [module metering](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs) 收费 |
| 来源与读取 | [IndustrialWork qualified lot](../../../crates/oasis7_world_state/src/world_rules/industrial_work_admission.rs) 可从真实 aggregate ledger 扣入自身 escrow；[StartRoot](../../../crates/oasis7_world_state/src/world_rules/industrial_work_start.rs) 消耗准确 parents | 本版不用该 escrow：普通 Transfer 不会自动运输其中的 lot。改用 B 零基线和完整有限 writer 闭包；专用 keyed/range 读取、覆盖证明和首次用途索引尚待接通 |
| 恢复与执行 | [persistence](../../../crates/oasis7_runtime/src/world/persistence.rs) 与原 journal/typed publication 已存在；[step](../../../crates/oasis7_runtime/src/world/step.rs) 先做原工厂折旧和 actions，再 due economy，后 due material transit及其他生命周期 | 新绑定和投影必须进入相同 generation；现有基础不证明新恢复链。原背景 writer 保留，未合入的增量存储工作不是本版可用依赖 |

本表的“已有”仅指上述原机制和源码事实。下文的逻辑记录与 adapter 均为目标合同，不宣称已有同名 Rust 类型、endpoint、索引或正式运行证据。

## 4. 边界与结构

| 事实/操作 | 唯一 producer / writer | 本设计消费边界 |
| --- | --- | --- |
| 阶段、个人、Site 与地点 | 原持续能力历史、个人 owner resolver、location/Site authority | 历史归因与当前操作授权分别读取；不得按当前 owner 倒填旧事实 |
| G 及其数量、窗口、匹配 | 原 SC-31 工业目标 authority，经原 Runtime 执行边界 | 只扩有限输入目标适配；接受合法旧库存并绑定 U，不复制需求余额 |
| U 的用途声明与执行 | B 的正式原工业 intent 接受记录、当前 owner 与原 recipe writer | 声明不等于 job 已开始；G matched 后仍由 B 独立显式执行、拒绝或取消 |
| 原供货、运费和物理到达 | 原物流/容量和账本 writer | 维持原 job、route、tariff、损耗与 once settlement；读取不再扣款 |
| B 的投入与产出 | 原 RecipeStarted/Completed、工厂和资源 writer | 必须实际 sink 对应输入并完成；gear 仍归原 B 输出账本 |
| checkpoint、首次/重复及回访 | 对原事实的可重建 Runtime 只读投影 | 无资产/资格/奖励 effect；新 report ID 不产生第二结果 |

[恢复项目贡献合同](bounded-recovery-projects-and-contribution-settlement.design.md#recovery-authority) 拥有 containment 后的项目/里程碑及 source claim；本版不是恢复项目，不占用或假称复用其全域 claim index。[区域 charter 双资格](regional-charter-authority-and-recovery.design.md#charter-dual-qualification) 消费持续本地贡献并结合独立治理资格；这里的一次供应用途证明不能签发 `LocalContributionRef` 或治理权。[认可与机会](recognition-opportunity-settlement.design.md#recognition-stage) 的制度审核/权益同样保持独立；只可复用原阶段、运输接受身份来源适配，不能继承认可效果。

原目标和物理 writer 继续服从 [Runtime native 分类](../prd.md#native-surface-分类与冻结规则)与原工业执行协议。本版没有新的 institution 判断、收费资格或 EconomicContract 语义扩张，不选择 SC-32 pilot，不以 native handler 新建一个区域贡献制度。新增的 G/U 接受与匹配是原 SC-31 执行接缝，实际 namespace/schema 与原工业 authority 一并交付；只读判断不需要收费 WASM 或额外治理写入。

<a id="contribution-need"></a>
### 4.1 阶段、真实 G/U、身份与范围

阶段 producer 按原 canonical helper 证明 A 已完成首次持续能力：合法 FactoryBuilt、当时 builder/control、同一 canonical stable-line snapshot 下的三个连续完成 job 及各自开始/完成和完整窗口。三批旧产物 iron9/slag3 是正常 fixture 的库存前提，不是阶段判据的替代。后来的停机或权限变化不删除历史阶段，也不自动证明当前能力仍可用；缺原历史为 `maturity_source_unavailable`。

范围固定为 `(world_id, branch, canonical location_id)`。A output Site、B input Site 的当前 active/chunk_ready authority 必须指向同一个 active LocationAnchor，并保留各自 revision；路线的真实几何/距离另从原 route 读取。location 名称、物理坐标、一般 region 字符串、同一联盟或空间邻近都不代签此关系。范围没有 charter、跨区权限或全局地域含义。

B 与 A 是原身份 authority 解析出的不同受益个人，使用各自当前控制的 Agent 和 Site。接受时记录 canonical beneficiary、Agent、owner source/generation、正式请求认证来源及目标 ledger；不同 Agent/request ID 或别名不证明不同个人。控制来源不足只返回 `beneficiary_unverified`，不指控滥用。历史供应属于原接受个人，接管 Agent 不转移旧贡献；当下执行和披露仍须当前授权。

G 是原 SC-31 的一个有限输入目标：`iron_ingot`、正整数 2、准确 B input Site、既有库存可接受、`[t10,t18)`、准确原 root/goal/revision/digest、固定用途类别 `native_gear_input_supply`，并包含 B 对原生 gear 一批的 U 用途正文承诺。G 的原接受和最小披露必须早于 A 的供应接受；事后补写用途不能追认此前随意到货。要求新产批次、来源 lot 或其他规格的目标不适用本版。

U 绑定 G 的准确版本、B 的 factory/input/output Sites、canonical recipe ID、1 batch、2 iron 输入和 1 gear 输出、用途范围/窗口及原 B 接受身份。其初始接受仅确认用途声明，不提前创建 RecipeStarted、占输入或扣 4E。U 的后续当前合法性独立可核，B 始终保留显式选择。G matched 只停止为 G 追加供应/生产；U 的实际 gear job 不属于“matched G 自动续产”，也不把 G 或父 root 推成新的 terminal settlement。

G 当前修订、取消、缩量、到期和 unknown 遵守 [原需求变化处置 §4](../../game/gameplay/gameplay-industrial-demand-change-disposition-contract.prd.md)。每个下一不可逆动作分别重验；目标和 U 变化不自动改写已发生物流、已消费材料或历史归因。权限不足的 A 只获得 B 显式授权披露的用途、数量、期限、匹配和使用证明；不能要求公开 B 的完整私人目标、预算或控制原件。

## 5. 关键运行流程

<a id="contribution-preview"></a>
### 5.1 同切点预览与一次有限承诺

只读预览从同一 canonical cut 取得 G/U、A 的当前旧库存和控制、B 的准确输入缺口/用途、R 的可用量/费用/距离/损耗、相关工厂和预算。展示目标尚缺 2、A 现有 iron9 中本次送 2/保留 7、无回报承诺、2E 运费、1 tick 运输、交付后 B 是否执行仍不确定，以及本次不会替 A 再开生产。需要披露的事实无授权或未知时保留原因，不按零成本、可用或无需验证补齐。

预览不创建 G/U、hold、transit、queue、资源 sink、优先权、匹配结算或 checkpoint。A 显式选择后，原 SC-31 commitment 接受必须与原 TransferMaterial 开运的准备/提交处于同一 canonical 根事务：重验 G/U revision、当前 A 身份/Site 源权限、原目标尚未被其他供货承诺占用的数量、R 当前容量/费用与原在途上限；原 commitment 只绑定这一 transfer job 和供货意图。任一失败整批无新承诺、源料扣减或运费 escrow。不要先接受“贡献”再绕原运输执行补救。

`remaining` 仍只随原 matching settlement 下降；准入另外从原 SC-31 已接受且未结/未处置的 supply 绑定得到 `committed_unsettled`，核验 `2 <= remaining - committed_unsettled`，checked 运算失败或来源缺失时拒绝。接受事务同时登记本次唯一供货绑定，使本例该量从0到2；原 matching 同根把这份未结承诺转入 delivery-settled，本例恢复为0。取消/失败只按原 linked disposition 解除该匹配占用，保留实际物料、在途、已付运费及其原义务，不自动退款或回滚。这是原 committed 数量分层的索引/投影，不另维护一个平行需求余额；原目标内容修订与当前提交版本分别核验，数量登记不暗改已接受用途。

原幂等键绑定 authenticated A、原供应 intent 和 payload digest；同键同内容查询原结果，同键异内容拒绝。改变 request ID 不得占 G 的同一剩余数量两次；原目标 revision/匹配消费索引负责此约束。当前 Site-source 出货只支持 A 本人的正式 current-owner 路径；本设计不扩展现有仅支持 Agent 源账本的 delegated Transfer grant。

<a id="contribution-delivery"></a>
### 5.2 原运输、到达与匹配分别结算

正常 R 是原来已存在的有向单 hop：A output Site → B input Site，kind=iron_ingot，距离 1km，当前容量至少 2，tariff=1E/unit，owner=B。必须读取当前共同 allocator、已有保留/在途、路线权限和可用状态；拥有 route 不赋予取走 A 材料的权利。供给接受不私建容量池，也不挤掉原合法已接受使用。

原 [pure helpers](../../../crates/oasis7_world_state/src/world/pure_helpers.rs) 以 Medium 材料给出 10bps/km，原 [loss helper](../../../crates/oasis7_runtime/src/world/logistics.rs) 得 `floor(2 × 1 × 10 / 10000)=0`；运输时间为 1 tick。开运同时从 A Site 扣 2 iron、把请求人 A 的 2E 放入原 tariff escrow，并占 R 原容量。完成才向 B input Site 加 2 iron、释放同一占用并按原 receipt 付 route owner。实际 profile、费用或 owner 漂移时以原规则和真实 receipt 为准，不把静态算例数值当新退款或结算规则。

到货是物理事实；G 的 matching adapter 还须在同一提交切点核 exact goal/root/revision、已接受供货绑定、目的 Site/material/单位、实际 received、G 当前窗口/剩余量和既有消费键。只有通过才与原交付事实共同提交 G `delivery-settled += 2`、`remaining -= 2` 和原匹配 receipt；材料到达、数量更新和对应索引不能留下“已减需求但无交付”的可见半状态。

G 先取消/到期/不匹配时，原合法 transit 仍按原协议完成和付费；该提交记录 `unmatched` 及原因，G 不减少。匹配先成功时，后续取消或 revision 承认该原 receipt，仅处置剩余义务。普通 arrival 永远不能冒充 G 的匹配收据；匹配也不意味着 B 已使用、父 root/terminal 已 fulfilled 或任何奖励已发放。

匹配效果键固定原 goal identity、原 accepted supply identity、原 transfer settlement 与对应量，不能通过 goal revision/request/report 改名重复消费。此不可拆分 2 单位 profile 不支持把同一来源分给多个目标、部分新用途或两份 surplus；发生量/目标漂移时按原 SC-31 返回实际 matching/unmatched 和可重新选择条件。

<a id="contribution-use"></a>
### 5.3 B 的独立执行与完整有限来源证明

B 在看到原到货与匹配后，从当前合法 U 显式提交原 `ScheduleRecipe`。原工厂必须由 B 的 builder Agent 当前拥有，未 paused/retired、符合 assembler 标签/阶段、slot 和输出容量；输入严格为原 B input Site，输出为原 B output Site。实际有效 consume 必须为 2 iron，原 maintenance/output policy 与 pinned validator 规则符合本 profile。B 在初始 cut 已有足够 4E，未来收到的 tariff 不能代替初始资金能力。

原 RecipeStarted 原子消费这 2 iron 和 B builder 的 4E；U 的接受不代签此 sink。原 RecipeCompleted 还须与同一个 job/recipe/batches/factory/Sites/原 consume 及实际 gear1 对齐。失败、停留 pending 或仅有输出余额差时分别为 `use_pending/use_failed/source_unavailable`，不能升为已证用途。已消耗投入和真实费用保留，按原 job disposition 处置，不由本只读报告退款。

来源证明只承诺“A 合法供应的旧库存被 B 使用”，不标称可辨认的 A 新生产批次。需由原 ledger writer 的目标 keyed/range adapter 返回：

1. 同一 cut C0 上准确 B input Site 的 iron=0、当前 ledger revision/root，以及窗口前已存在但可能在窗口写入的 pending/hold/obligation 完整集合。
2. 从 C0 到该 B job 完成 cut 的所有相关 ledger/material 写入，含原 transfer、其他转入/转出、所有 recipe/maintenance、退款/补偿、治理 effect、恢复 delta 和库存 escrow 移入/移出；覆盖完整性来自原 writer 注册/索引版本与 generation，不由 caller 挑选事件。
3. 其中唯一正 credit 为已绑定 transfer 的 2 iron，唯一负 debit 为绑定 gear job 的 2 iron；没有其他相关 credit/debit/hold 或无法解释的写入。该 job 的原消费、完成和输出在同一因果链上。
4. A 的原接受个人、合法 source control、源账本扣减、transit start/complete/settled index，以及原 G matching receipt 和 U 当前/历史接受记录均匹配。

存在额外相关写入、旧 pending 不能排除、缺页、索引落后、未知 writer 或跨 generation 时，返回 `ambiguous/source_unavailable`；不得仅筛出“看起来相关”的两条事件后宣称 complete。普通合法物流和生产仍保持成立，不因归因不清而处罚玩家、逆转交易或阻断原本允许的业务。缺少可证明全 writer 闭包的历史不能靠一次期末余额或另建虚拟 lot 修复。

<a id="contribution-checkpoint"></a>
### 5.4 只读 checkpoint、历史首次与重复

最小 `SupplyUseProof` 是原事实的派生读结果：exact world/branch/location、原 beneficiary、G/U、原 supply/transfer settlement、matching receipt、B use job、purpose class、source cut/coverage、actual sent/received/consumed/output、费用引用和结论。不写材料、需求量、个人权益或新贡献 claim；可缓存其摘要，但缓存不是 authority。

物理 source-use key 绑定原 world/branch、transfer settlement、G matching 和 B use job、canonical purpose。重复读取、不同入口、重连、重试或更换 report ID 只得同一原事实。G delivery settled、U use pending/failed、已证 use、first/repeat/unknown 分别展示，不能压成一个 success。

“首次外部用途”的键固定 canonical beneficiary、world、branch、canonical location 和 `native_gear_input_supply`；不含 recipient、G/U/request/Agent 别名或 policy revision。固定类别由专业政策定义，不接受 caller 自由文本。需 exact-key 历史投影返回原最早合格 source-use、稳定来源和完整覆盖/无先前用途证明；“当前页没找到旧记录”不等于首次。改变参与 Agent、收货人、goal 名称或读报告不刷新该键；canonical 身份和 location 的等价/别名必须按原 authority 解析。

历史不完整仍可显示本次实际供应/用途，但 first/new-growth 为 `history_unavailable`。重复同类真实供货是合法经营，可以继续说明实际用途，不能重复标作新的成长。旧三批稳定线、旧贡献与原 repeat 计数保留；既有 `same_loop_repeat_count >= 3` 且 `throughput_only/unclassified` 的停止强化重复规则继续有效。新的 leverage 解释必须有真实新用途或其他原权威后果，不能从产量、积分或动作改名产生。

<a id="contribution-continuation"></a>
### 5.5 回访、独立继续与显式新选择

回访从原已提交结果与当下能力分别读取：这次供应满足了哪个输入目标、B 是否实际使用、首次/重复是否可核、A 保留什么、当前下一步有什么成本。最多展示 3 个确有来源且有不同后果的选择；不足 3 个如实少给。原 G 已 matched 后停止追加供货，不自动续单、补产或把剩余 iron7 计成长；新需求必须是另一个真实、当前已接受的用途，而不是重命名旧 G。

正常例在其他相关资源 delta 已证明为零的条件下，A 保有 iron7/slag3、原 input Site 的 ore4/fuel1、E18 和经过原折旧后的工厂。可立即合法持有或退出；若展示独立继续生产的选项，必须取得当前准确原 quote，覆盖工厂/维护/slot/output room、8E 原生产成本及真实铁产品 validator 的 owner、module artifact 和预算。只凭 ore4/fuel1/8E 或“非联盟成员”不能宣称该选项已可执行。

本例可选未来 quote 的实际 validator payer 为独立 V（Data20/E10），其确实拥有被 pin 的原铁 validator artifact；V 不成为本次供货的第三名参与者。query 不调用模块、不 reserve 或扣 V 预算；真实执行若以后被选择，仍由原 authority 接受当时费用与权限。没有受支持的独立生产 quote 时保留持有/退出和明确 blocker，不虚构缺省免费验证、额外订单、共享容量或供能。

### 5.6 静态正常算例与阶段对照

以下是有源码依据的设计 fixture，不是运行结果。C0 为 t9 的完整已提交切点，G/U 已在其前合法预声明/接受并最小授权披露，G 可执行窗口为 `[t10,t18)`。A 已有原首次持续能力证明，原 output Site 为 iron9/slag3，input Site 为 ore4/fuel1，E20；B 原 input iron0、output gear0、E10，当前自有 assembler 可用。两厂均在 t9 之前建成，实际 spec 的 maintenance_per_tick=1、recipe_slots=2，耐久足以经本窗口原折旧仍合法运行，无其他已接受制造负载。地点、R、material/product policy 和当前权限均满足前述约束；实际 iron/gear 材料 profile 的 decay 字段为0，不能修改真实配置只为凑算例。

| 切点 | 原因与原事实 | G / U / 用途读结论 | 材料与费用 |
| --- | --- | --- | --- |
| C0=t9 至 t10 开运前 | G/U 预声明和原接受已经完成，A 只读确认旧库存供应 | G remaining2；U 声明有效，未开始；无用途事实 | 预览无 sink/hold/费用 |
| t10 开运 | A 显式确认；原 commitment 与 Transfer start 同根提交 | G committed_unsettled0→2、remaining仍2，尚未matching | A iron9→7，E20→18；原 transit2、tariff escrow2、R 占用2；slag3不变 |
| t11 due transit | 原 arrival 向准确 B Site 入2，原匹配检查通过 | G delivery-settled0→2、remaining2→0、committed_unsettled2→0；U仍未开始，use pending | B iron0→2，E10→12；route占用释放，原tariff付B2E |
| t12 action | B 显式选择仍合法的 U，原 gear ScheduleRecipe start | G 不再追加；U原job进行中，不能宣称完成用途 | B input iron2→0，E12→8；原生产sink4E |
| t13 due economy | 同一原job完成gear1且来源闭包完整 | 本次实际用途可证明；first/repeat另看完整历史 | B output gear0→1；A继续保有iron7/slag3和原其他资产 |
| 后续回访 | 原receipt查询加当下只读能力/用途历史 | 不重跑物流、matching或制造；G窗口已结束也可核其原在窗事实 | 无新扣费、奖励、材料或所有权变更 |

时间顺序依据原 [step](../../../crates/oasis7_runtime/src/world/step.rs)：actions 在 due economy 之前，due transit 在其后；因此 t11 末到货不能被 t11 更早的 action 消费，下一正常 action 切点为 t12。原 [duration](../../../crates/oasis7_agent_api/src/world_quote/duration.rs) 给 gear base1tick；实际 Site input 使 [scarcity helper](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy_support.rs) 返回0，故 t13完成。其他支持 profile 以原 ready_at 为准，不改 tick 或暗排同 tick 自动 job。

表中是供给/制造业务本体 delta：A 净扣2E；B 收原 tariff2E、付原生产4E，净扣2E；这条业务链总 E 减4，2E 运费是转移而非第二次 sink。完整账分别为 A `18E+W_A`、B `8E+W_B`，W 包含同窗原模块/自动生命周期/claim等其他实际资源 delta；只有当前合法配置及完整有限来源证明 W_A=W_B=0，才可将18/8写成最终余额，不能停用真实 writer 或漏算费用。其他相关材料 delta 同样按原账保留，B input iron出现额外相关writer即按§5.3返回ambiguous。

原 [工厂折旧](../../../crates/oasis7_runtime/src/world/economy.rs) 在每个step的actions之前执行：t10至t13，A无job，每次1000ppm，共减少4000ppm；B前三次各1000ppm，t13完成前仍有1active job/2slots，减少1500ppm，共减少4500ppm。原耐久变化及事件保留，并用于之后的可用性quote；这项维护不另扣E或材料，不能把spec中的base_power_draw再当本窗额外E sink，也不能把product maintenance为空误解释为工厂无折旧。

本窗没有 A 铁生产/铁 validator 调用，V 的该可选quote无费用，历史成本仍是历史。gear 的批准政策明确无product maintenance、validation not_required 且 registry 无适用 fallback 时，没有该制造验证的 Data 费；其他实际背景模块费仍计入W及完整Data账。缺条件不能沿用静态费用结论。A 多余库存、单纯到货、U接受、RecipeStarted、完成但来源含混、重复同用途分别成为独立对照，不合并成一个成长信号。

## 6. 接口与数据合同

### 6.1 最小逻辑记录与身份

所有 ref 带 world/branch、原 domain、schema version、稳定对象 ID、revision/digest 和 commit/generation；字符串相同不能跨世界拼接。数量为原域 checked 整数单位，时间为 canonical world tick，窗口开始含/结束不含。下列名称只表达目标接口语义。

| 记录/投影 | 最小内容 | owner 与不变量 |
| --- | --- | --- |
| 原 `InputGoalBinding` | 原G/root/revision、target2iron、B input Site、scope、accepted旧库存策略、U用途正文摘要、窗口、当前状态/剩余、原供货未结绑定及其committed_unsettled投影、最小披露授权 | SC-31原目标记录；只读原承诺分层，没有额外RegionalDemand余额 |
| 原 `UseIntentBinding` | 原B接受身份、准确G版本/digest、factory/input/output、recipe/batch、用途类别、当前有效/取消信息、原job引用 | 原工业intent记录；声明不提前sink，不是新的terminal目标 |
| 原 `SupplyAcceptanceBinding` | A beneficiary/Agent/control source/generation、原正式intent/payload、G/U版本、源/目的Site、kind/amount、route、原transfer/job与接受cut | 在原commitment与开运同一根提交；当前owner不能改历史归因 |
| 原 `MatchingDeliveryReceipt` | G/accepted supply、原transfer settlement、actual received与matching量、before/after remaining、匹配或拒绝理由、原effect key | SC-31唯一writer；不再次写物料或把G完成扩到父root |
| `LedgerWindowProof` | exact ledger/material、C0余额/revision、pending/hold完整集合、窗口原event refs、writer coverage版本、索引watermark、结束cut、counts/bytes、complete/缺项 | 原writer索引投影；缺页/未知writer不返回complete |
| `SupplyUseProof` / `FirstUseProjection` | 上述原refs、actual use、source-use key、固定first-use key、最早合格原件或absence witness、coverage、当前audience可见结果 | 可重建只读结果，无claim/奖励/权限状态；缓存过期不改变业务 |

G 的 payload 固定 U 用途正文的摘要；U acceptance 反指已定的 G revision/digest，避免两个完整 record digest 互相包含形成循环。G/U 修改产生原 parent-linked 新 revision，旧 supply 不自动搬迁；历史 ref 和实际原 effects 永久保持因果身份。

### 6.2 跨组件接口

| 接口/条款 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| 原 G/U 接受与变更适配 | B正式工业intent → SC-31/Runtime | 原goal/root/revision、用途正文、B认证、窗口 | 在A接受前提交；当前revision CAS；旧接受不迁移 | accepted / stale / invalid_use / not_authorized | 目标新增接缝，不伪称已有endpoint |
| `ReadSupplyUsePreview` | 授权Runtime scoped reader → 三读面 | G/U、A、scope、source cut、audience | 只读；共享预算和相同来源版本 | available / blocked / source_unavailable / budget_exceeded | 无原目标/新schema时明确unsupported |
| 原有限供应接受与Transfer | A正式current-owner → 原工业/物流根事务 | 原intent/payload、G/U、transfer/action、current revisions | commitment、开运、归因和索引同根；重试先查询 | committed / stale / denied / outcome_unknown | 原运输校验和费用不可跳过；不新增delegation |
| 原matching adapter | 原arrival和G authority → SC-31 | 原supply/transfer settlement、G、稳定matching key | 与到货同切点，减量至多一次；不匹配保留到货 | matched / unmatched / already_settled | 不猜旧运输归属，不降级按产量结算 |
| 原U显式ScheduleRecipe | B当前正式owner → 原工业writer | U/G来源、原job/action、factory/recipe/batches | 独立新命令；当前U可拒绝，原job once执行 | started / rejected / outcome_unknown | 不自动续产matched G，不新增terminal结果 |
| `ReadLedgerWindow` / `ReadFirstUse` | 原持久索引 → Runtime证明器 | exact ledger/material/cuts或canonical first-use key | indexed point/range；同generation完整覆盖 | complete / ambiguous / incomplete / not_authorized | 无全World/journal扫描回退，旧缺证据保持unknown |
| `ReadSupplyUseResult` | Runtime只读投影 → Viewer/API/Agent | 原source-use identity、current audience、read cut | 重连/重试同原件；当前能力单独刷新 | matched/use_pending/proven/failed/ambiguous/history_unavailable | 旧客户端不得把未知枚举显示为成功 |

超时返回原 identity 和已知 submitted/accepted/committed/persisted 阶段；客户端先查询原 intent/job/receipt，不换 G 或重新扣料。读取错误与实际无目标/无历史是不同结果。后续正式 endpoint/编码必须遵守这些语义、原 ABI/数据版本及 §9 双重界限，不能因更名取消义务。

<a id="contribution-consumers"></a>
### 6.3 最小正式消费者闭合

1. 原 SC-31 producer 提供已接受 G/U、准确匹配规则与最小授权披露；World Service 把真实认证/当前控制带入原供应接受，返回原业务身份。
2. 原物流和 recipe writer 共同提供真实 debit/credit、原开始/完成、费用、matching、U引用与同提交索引；Runtime只读证明器将它们连接，不让任一 presenter 自行推断用途。
3. Viewer、pure API 与 Agent 从相同 cut/audience 的投影读取 G交付状态、U执行状态、归因/first状态、实际成本、保留资产、blocker及最多3个当前动作。用户读权变化时各字段重新授权，不能从缓存、模型文字、日志或另一个入口泄漏 B 私有目标。
4. Agent 建议说明依据与未决项，不签需求、不自动选新用途、不把 API 可读性当可写权。当前Agent不能代办Site出货时只能解释/建议交回本人正式入口，不能展示一个不存在的delegated能力。

三入口 parity 以各自真实接受/执行/读取和同原receipt核对。仅共用 DTO 或相同静态文字不证明闭合；正式Runtime、World Service、实际Viewer、pure API及Agent消费者缺任一，实施完成结论保留该缺口。

<a id="contribution-recovery"></a>
## 7. 状态、事务与持久化

### 7.1 状态分层与竞争

G 沿原目标 authority 的 accepted/current、matched、cancelled、expired、reduced/unknown；原物流沿 transit/settled，U 沿声明有效/取消、原job pending/completed/failed。派生读面只组合这些状态，不持久一个能反向改业务的“贡献成功”总状态。`accepted` 证明原意图被接受；`committed` 证明原 canonical effects；`persisted` 需原持久 generation 确认；发布/可读与跨节点 finality 仍由各自 authority 证明。

原供应接受与开运、原到货与匹配、原B开始/完成各自是独立因果提交。每个提交只对其 touched G/U、身份/权限、Site、route/容量、资源和原job进行同切点检查，并原子写原变化、receipt、关联索引、journal与原提交元数据。不得把三阶段跨数tick包装成不可失败的单一承诺；原物理工作可以完成而 G匹配或MW归因失败。

G/U 变更、Site owner变化、撤销、到期和完成按 canonical 顺序竞争：先提交的合法effect保留；后动作只能承认原结果并处理剩余。当前读权不等于原操作权；历史接受身份固定，后续动作重验当前控制。不读取外部实时页面、host wall clock或模型判断补签scope/期限。

`[t10,t18)` 限定本profile的接受、匹配和用途合格窗口；t18及之后不能产生新的在窗合格结果。原在途/job在窗口外依法完成仍保留物料/费用/结果，只不算本目标的新增matching或合格用途。窗口结束后的只读查询可以验证原来在窗发生的事实，不删除历史、不自动延长G/U，也不靠重启续期。

### 7.2 持久模型与恢复

原 G/U revision与状态、供应接受身份、matching effect key、原transit/job/资源变化及receipt须随同一 snapshot/journal generation 保留。ledger writer覆盖版本、范围索引watermark、pending覆盖和首次用途投影可重建，但必须有准确原事件与接受身份，不能在恢复时从当前余额或当前owner反造。

恢复先核world/branch、schema、profile/manifest、目标/用途ref、原effect/receipt、root与generation一致性。已提交返回原结果；提交是否发生未知先对账原intent和durable generation，禁止第二次开运/扣料/匹配；已确认未发生的动作只有经过原结果查询、当前条件和新的显式选择才可继续。replay仅安装原delta和关联记录，不重新跑运输、recipe、validator、goal匹配或奖励逻辑。

新原业务记录缺失、半个原事务或资源/receipt矛盾时按原恢复协议拒绝相应可写状态；仅派生索引缺失/落后而原世界完整时，将本证明功能降级为`source_unavailable`并有界重建，不回滚正确世界。重建属于原持久投影维护，不能在用户查询中全扫World/journal，不能借未合入的存储工作宣称已完成。

首次用途索引保留 canonical key 的原最早事实、完整历史覆盖和必要tombstone，UI分页/缓存失效不得清零。离线历史不足或缺旧个人归因时不补“首次”；业务事实仍按当前授权展示。读权撤销隐藏当前不再可披露的证明原件，不删除历史归因或给新owner授予旧个人成果。

## 8. 部署、安全与运行约束

首版只在原正式世界/World Service执行边界内启用，不新增外部服务、HTTP oracle或浏览器依赖；本地可信fixture不能替代正式认证。exact LocationAnchor、Site控制、SC-31 G/U接受/匹配、原业务写点及完整索引、授权消费者必须共同可用；任一未实现时保持本profile未启用，普通合法物流/生产仍走原协议。

SC-31和U producer只接受当前B对其工厂/目的Site及用途的声明；A只可从自己当前有权的source出货；route owner仅获得原tariff。任一角色都无权写另一个主体的私有目标、搬运其库存、处分其gear或扩大地点范围。读字段依当前audience最小授权，proof摘要也不得成为猜测私密goal、控制关系或余额的侧信道。

新 writer、恢复 delta、registry/maintenance policy变化会影响来源闭包和实际费用。实现必须在同一原写事务更新范围索引/覆盖版本；不认识的新writer使有关证明incomplete，不能默认排除。读取达到预算就停止并返回明确原因；不能为得到肯定结论取消验证、关闭收费、调用无owner模块或静默扩大窗口。

禁用新供应接受不取消已接受运输/生产；仍须保全原查询、结算、处置和恢复。没有真实网络、跨节点、正式客户端或长期运行证据时，只声明实际验证的本地/合同范围，不由本文给出部署完成或finality结论。

## 9. 质量与容量

<a id="contribution-budget"></a>
### 9.1 首版工程界限与组合可行性

以下为有限切片的初始工程上限，非性能实测或游戏经济阈值；由Runtime/QA在同候选验证其可行性，修改须复核完整性、隐私、消费者和profile版本，不能查询中自动放宽。

| 维度 | 上限与计数范围 | 越界行为 |
| --- | --- | --- |
| 业务范围 | 1个G、1次A/B供应、1个transfer、1个B job、2名参与受益人、1条原route、2座工厂和其显式Sites；窗口≤8 world ticks | 超出本profile；保留原业务事实，不拼成多个未声明贡献 |
| 后续选择 | 最多3个有不同实际后果的当前候选，复用同一查询预算 | 缺来源不补候选，已匹配G不重列为新订单 |
| 来源数量 | 每次整个查询≤128逻辑source records、≤64原event refs；包含历史阶段、pending/所有相关writer、首次用途和候选来源 | 提前停止并标incomplete/budget_exceeded，不截断后声称complete |
| 分页与调用 | 每页≤16items、整个查询≤8次source fetch；exact point/bounded range且同cut | 不全局扫描，不按候选/页重置额度；未知总数不能视为空 |
| 字节 | request≤16KiB；全部source/proof累计≤256KiB；输出≤64KiB | counts和bytes同时强制；超大单条亦拒绝，不仅按条数控制 |

正常证明依次读取原G/U与scope/身份、原供应/匹配和当前账本窗口、Bjob、历史阶段/首次用途，再给至多3个继续候选；独立原件可并行读取，但必须共享cut和计数器。可分配最多32records给身份/地点/配置、24给G/U与本次原业务、24给阶段/首次用途、48给完整writer/pending和候选，共128；这只是检验上界能同时容纳最小案例的预算分区，不保证实际序列化大小或历史覆盖一定满足。

64原event refs须覆盖三个旧稳定线job的开始/完成、原build/control关联、G/U/供应接受/匹配、当前transport和Bjob、两厂原折旧，以及所有相关干扰/待决/资源背景来源；同一个原ref只作为一个逻辑来源，但每次实际取得的字节和fetch仍计费于查询预算。历史首次性由exact-key完整投影证明，不逐条遍历无限历史；它的覆盖元数据及证明同样占预算。若任何必需闭包超过上限，返回较窄的已知事实，first/use证明保持unknown；不能用降低完整性换取通过。

正常时间线从t10到t13完成，处于8tick窗口内；每阶段独立读取/提交，后续查证可以晚于窗口。原native命令和持久化保留各自真实事务/资源预算，以上只读上限不替代原command约束。当前没有这些专用索引的实测数据；实施时须测实际record/event/bytes峰值、拒绝和恢复路径，确认上限内可完成正常例。读取不足只限制证明，不以额外sink、WASM或全量fallback补足。

### 9.2 原费用与未来独立 quote

本profile只读报告不执行module，不新增Data/E费用或预算reserve。正常供给/制造业务本体只有原运输2E与B原生产4E，其他实际背景费用按§5.6的W单列；历史A铁生产和历史validator费用从原receipt保留，不再次计入。

若向A显示未来独立铁生产，原 [铁validator](../../../crates/oasis7_builtin_wasm_modules/m4_product_material_iron_ingot/src/lib.rs) 是 `m4.product.material.iron_ingot`、stack limit500；原 [manifest](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs) 为Pure Domain、64MiB、gas2,000,000、call_rate64、max_output_bytes4096、max_effects0、max_emits2。当前profile必须绑定实际artifact owner V及其有效Data20/E10，实际完整canonical CBOR输入≤4096B。原 [fee arithmetic](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs) 给预检上界 `ceil(I/1024)+4+2≤10Data`、`1+0+2+1=4E`；V预算足够，A保有生产8E的能力。

真正以后执行时，成功的原模板产生1emit、无effect/new state，实际E=2，实际Data为 `ceil(I/1024)+ceil(O/1024)+1`，I/O须来自真实编码。模板里的output_bytes常量不替代执行器重写的实际输出长度。当前metering对missing owner存在兼容跳过收费分支，本profile不把它解释成免费；owner/预算不足时不展示该独立生产为可执行。未来quote本身无模块调用，因此本正常窗口V由该quote引起的费用为零；V其他原业务的费用仍保留。

| 质量场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| 有因果正常链 | 固定候选、C0与t10–t13、G/U/R和原输入输出 | 各阶段独立成立，成本守恒，首次另证 | 本profile、原recipe/transport公式 | 手册MRC-M01至MRC-M05 | 静态源核查，未业务运行 |
| 归因完整性 | B相关额外writer、旧pending、缺页或未知writer | ambiguous/incomplete，无伪阳性、不逆转合法业务 | 全writer覆盖与C0=0 | 手册MRC-M04 | 目标，未运行 |
| 容量与字节 | 达到/超过records、events、fetch、bytes和window上限 | 有界结束，完整性不降级成成功 | §9.1工程上限 | 手册MRC-M07、MRC-M08 | 未承诺P95、吞吐或内存实测 |
| 并发与恢复 | G/U变化、当前权限漂移、commit未知、snapshot加非空journal | 同原身份、单次物理/匹配、已知事实保留 | 原事务/持久化与本profile | 手册MRC-M07 | 不证明跨故障域部署 |
| 多入口与私密性 | 同候选真实World Service、Viewer、API、Agent与不同audience | 状态/成本/下一步同义，私有字段不泄漏 | AC-MRC-08、原读权 | 手册MRC-M08 | 正式组合证据待交付 |

## 10. 兼容、迁移与回滚

旧目标、材料、transit、recipe、MetaProgress及原receipt保持原语义；历史只有普通运输或生产时不能自动迁成G匹配/区域用途。旧snapshot缺B零基线、接受个人、G/U或全writer覆盖时明确unknown，不从现在的库存、stage或owner填补。旧reader不识别新状态时显示unsupported，不误译为完成或零成本。

实施顺序是原身份/接受来源与同generation读取、SC-31有限G/U/匹配和原写事务关联、全writer/历史投影与恢复、授权三读面，最后开放本profile新供应接受。读写方和replay缺一时不开放；没有迁移中的双需求账、双结算或临时管理员补签路径。

回退可以冻结新profile准入和只读成长分类，但仍需运行能理解新G/U/matching记录的已知兼容版本，处理已接受原运输、job、查询和恢复。固定基线并不认识这些新增记录，不能未经兼容确认直接降回它继续写。合法已发生材料/费用/匹配不能通过代码回滚或删projection撤销；需要纠正时由原authority追加真实linked处置，绝不重跑原业务。

## 11. 验证设计与可追溯性

下表是验证方法，不是执行结果。实施在同一source/build、module/manifest、world/branch、权限/配置和数据fixture上先做有意义的合同/负例/故障层，再以真实正式入口完成组合证据。源核查、静态数学、编译、文档结构或旧CI仅证明各自范围。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [AC-MRC-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-01)；Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce | [阶段、需求与身份](#contribution-need) | 旧持续能力、独立个人、G/U先接受及canonical同地点分别有原件 | [MRC-M01](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m01)：固定本profile候选/C0；阶段缺件、同人多Agent、无需求/用途、范围漂移和合法正常例；required来源/准入层 | build/stable-line历史、个人来源、G/U原接受与Site/anchor refs及拒绝结果 | 不证明其他成熟阶段、通用region或charter |
| [AC-MRC-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-02) | [预览与有限承诺](#contribution-preview) | 无副作用比较、明确无对价、一次供货当前授权及成本 | [MRC-M02](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m02)：同候选正常quote、stale/容量/费用缺失、直接owner与无Site委托、并发重复；required提交层 | 预览前后不变、fresh接受/原Transfer和归因同根、实际扣料/费用或原子拒绝 | 不证明收费服务、跨区或delegated Site能力 |
| [AC-MRC-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-03) | [交付与匹配结算](#contribution-delivery) | 原运输/费用守恒、准确G单次matching与不匹配保留事实 | [MRC-M03](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m03)：t10/t11、loss/amount/目的漂移、取消到期竞争和重放；required原账本/匹配层 | 原start/complete/settled、tariff escrow/payout、G before/after和唯一matching key | 不证明父root、terminal或奖励完成 |
| [AC-MRC-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-04) | [实际使用与来源](#contribution-use) | B独立U实际sink/完成，C0零和全writer闭包证明唯一来源 | [MRC-M04](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m04)：t12/t13、仅到货/仅开始/失败、其他credit/debit/hold、旧pending/缺页和恢复writer；required因果层 | 原B输入/电力sink、同job gear完成、完整窗口/coverage与ambiguous结果 | 不证明新A产物lot或来源不明库存的批次身份 |
| [AC-MRC-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-05) | [只读 checkpoint](#contribution-checkpoint) | 首次/重复由稳定canonical历史证明，读取不发权益 | [MRC-M05](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m05)：同候选重复读取/改G/U/request/Agent/recipient名、历史缺口与repeat阈值；required只读/防重复层 | source-use/first-use key、最早原fact/absence witness、无世界effect及旧repeat保留 | 不证明其他用途类别、新认可或治理资格 |
| [AC-MRC-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-06) | [回访与独立继续](#contribution-continuation) | 持有/退出与真实未来quote可比较，满足后不自动续供 | [MRC-M06](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m06)：A剩余资产、V真实payer/预算、不可用quote及显式新用途；required下一步层 | 当前原quote/来源明细、≤3候选、无reserve/validator调用/隐含续单 | 不证明未执行未来生产、长期收益或独立恢复 |
| [AC-MRC-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-07) | [状态、事务与恢复](#contribution-recovery) | 当前权利、窗口、未知对账和非空journal恢复保留单次原效果 | [MRC-M07](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m07)：同候选在各提交点注入失败/丢响应，G/U/owner竞争、过窗、snapshot+suffix与完整/缺失索引；required/full恢复层 | 同generation原receipt/账本/目标/归因、无双扣/双减/重跑及unknown诊断 | 不证明未测部署或跨节点finality |
| [AC-MRC-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-08) | [正式消费者](#contribution-consumers) | 正式多入口同义、当前最小读权、预算完整性和实际产物可核 | [MRC-M08](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m08)：同一候选真实World Service/Viewer/pure API/Agent，切换audience，达到/超过共享预算；full组合层 | 每入口原请求/结果/ref、私有字段拒绝、counts/bytes/fetch与原世界事实对照 | 静态DTO、文档、fixture不代签全部正式入口 |
| [REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce | [成长组合](#contribution-continuing-growth) | 续单/服务、扩容四选择、真实checkpoint和有限影响，各依原源 | [MRC-M09](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m09)：同候选成熟前态，续单、原CSF服务、PRC扩容及RO机会；required/full组合与独立判别 | 原goal/unit/receipt、逐资源成本、新可用选择及重复/错源反例 | 方法及静态算术不证明实际业务、全部入口或玩家体验通过 |
| [REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce | [组合预算与验证](#contribution-growth-composition-budget) | 旧工作/费用、同cut完整性、原未知与共享有界宿主 | [MRC-M10](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m10)：固定candidate/authority，关键失败cut、各limit与limit+1、正式三入口及读权；required/full恢复层 | 原operation/generation、无重复业务/收费、240对象分配与真实bytes/调用、当前合法下一步 | 未实跑不证明性能、跨节点finality或当前mature-world验收 |

## 12. 决策、长期风险与未决问题

选择合法旧库存供应，是因为真实用途不应强迫玩家在库存足够时继续扩产；代价是本例只证明供应者和下游使用，不能声称原料属于某个新的 A 生产批次。选择 B 零基线与完整有限writer闭包，避免把现有IndustrialLot escrow误接普通Transfer；代价是混合库存、复杂维护和高并发写入会如实得到ambiguous，未来扩大需要原领域更强provenance，不能在本版补虚拟lot。

选择 G 与 U、物理到货与matching、实际use与first历史分层，使每阶段可以独立成功或失败，并保留已发生成本。选择只读checkpoint，使报告、重连和恢复不需要新的资源/奖励/治理写入；可重建投影的正确性仍依赖原事实和完整索引，缺失时不能给肯定成长结论。

| 剩余实现义务/风险 | owner | 解除条件与复核触发 |
| --- | --- | --- |
| SC-31有限G/U、同根供应接受、当前matching与原数量索引尚未实现 | 原工业/Runtime、gameplay | 正式原命令和receipt、重复/竞争/恢复证据完整；扩到多个目标、拆分量或新生产约束重新审profile |
| 个人接受归因、准确Site/location与当前披露来源不足 | 身份/World Service、Runtime | 原接受cut持久认证关联、当前read/write authority可验证；owner/别名/跨入口变化触发重核 |
| 全writer范围读取、历史阶段和first-use索引不完整或超过上限 | Runtime持久化/各writer、QA | 覆盖清单、同提交索引、bounded complete读取/重建和实际bytes计数；新增writer/schema触发覆盖复核 |
| gear实际维护/validator政策与算例不一致 | M4/Runtime | 批准且pin的not_required与registry无fallback证明；有真实validator/维护时须其支持profile和费用，不绕过原收费 |
| 三入口未闭合或独立未来quote缺预算 | World Service/Viewer/Agent、QA | 同候选正式产物、最小披露、真实原quote及V预算；任何缺项保留未实现/未验证，不以设计存在结项 |

本专题属于原成熟玩法的供应用途技术接缝；相邻恢复比较、认可机会、经济审计和区域扩展合同继续各自承接原要求。实施或验证范围扩大时按这些实际authority逐项复核，不把一次gear用途推广为整个区域经济、治理或成熟世界成长已完成。

<a id="contribution-continuing-growth"></a>
## 13. 持续服务、扩容与有限影响的成长组合

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；上游仍为 [REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001) / [AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)及[成熟成长合同](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#29-成熟世界中的小玩家成长线)中持续交付、expand_capacity_preview 和 checkpoint 的原义务。

### 13.1 组合范围与来源

目标读面 mature_growth_composition_v1 接续首次持续能力之后的原成熟阶段，连接本地立足、区域专业化与有限影响；不是必须逐级完成的职业树，也不是新正式入口或新业务执行器。§1–12 的 native_regional_supply_use_v1、其两个主体/一单用途/八 tick 窗口及全部原方法保持原范围。组合请求固定同一 world/branch、当前受益主体及控制来源、原已接受目标/需求或服务义务、明确地点/作用域、当前 cut、比较阶段和适用规则版本；不把其他主体的目标合成自己的主目标。

| 原来源与唯一 authority | 成长消费者可读取的事实与真实下一步 | 不可作出的替代 |
| --- | --- | --- |
| [原供应用途](#contribution-use)、SC-31 的原 G/匹配结果 | 本次实际交付、实际用途、首次/重复及原需求剩余；显式选择另一份真实需求或保留库存 | 旧 G 改名不是新需求，供出不等于使用；不重复发首次成长 |
| [持续服务原单位](continuing-service-fulfillment-and-remedies.design.md#service-obligations)及[原领域结果](continuing-service-fulfillment-and-remedies.design.md#service-domain-evidence) | 已采纳的供能、维护/修复、物料运输、共同工作；当前 required 单元、真实结果、受益范围、正常/安全退出工作图、价款/责任/费用及各自 source；续作仍经原共同形成或原有效义务准入 | accepted、active、请求维修量、arrival、某个 child 完成不代表 whole 履约；performed_unsettled 先核原 outbox，不重做物理工作 |
| [能力生命周期](../../game/gameplay/gameplay-industrial-factory-capability-lifecycle-contract.prd.md)、[原 cutover](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-cutover) | 当前可服务范围、profile 支持的扩展/升级/重配、真实成本与旧工作处置；原 Build/能力动作及原 receipt | 不补造 upgrade 动作、迁移或吞吐公式；建成不自动完成生产/区域交付，successor 不继承旧 W/receipt/顺位 |
| [地方认可及真实 Use](recognition-opportunity-settlement.design.md#opportunity-once-settlement)、[明确采用的情境用途](contextual-reputation-evidence-and-review.design.md#reputation-all-consumers) | 原审核/采用政策允许的当前机会、资格、可见事实及有效期；使用仍走其自己的 claim/Use/执行口 | 咨询、贡献或 checkpoint 不签认可；agent_acquisition 不是地方物流资格，FI/GR/RD/治理不相互兑换 |
| [恢复项目原里程碑](bounded-recovery-projects-and-contribution-settlement.design.md#recovery-milestone)、[RD 原工作与权利](rd-charter-provenance-and-share-transfer.design.md#rd-contribution-source) | 已采纳且当事人有权读取的实际贡献、恢复效果、工作与当前可用权利；只在它确实改变本人的有限选择时解释 | 原维护不是项目完成；研究、holder、个人作者和可用权利分开，不开新奖励、岗位或许可 |

source adapter 固定原 schema、查询选择器、规则/采用版本、实际 writer 与当前受众，返回对应 cut 的完整有限投影及其覆盖依据。需要原 authority 验证的私有材料，使用其有权出具且绑定原对象/cut/谓词的最小断言，保留 service assertion 与正式证明的原等级；hash、聚合分数和过滤后的空集不能代替完整来源。新源未适配只限制相应比较/成长结论，不封锁原规则允许的普通行动，不以无限未来插件作为本组合已采纳范围。

### 13.2 续作的四种选择

每次只比较一个明确需求/义务集合，按原单位分别列出已完成、已接受未完成、已损失、可用库存/容量及实际费用。下表四种方向均解释适用性；只有有真实动作及完整前后条件的方向才显示可执行，不能把缺能力写成零成本。新一期/新单必须有当前原需求或完整同版接受，不由到期、回访或历史成功自动滚动。

| 方向 | 必需来源、结果与退出代价 |
| --- | --- |
| fulfill_next_order | 原新 G 或已接受服务下一未完成单元、实际受益需求、完整输入/容量/权限与费用；只按其原完成 predicate 和一次结果计量。旧已完成单元不能重开 |
| reserve_capacity_for_local_need | 仅使用原 allocator/服务合同已支持的有界预留操作；明确 owner、容量单位/窗口、既有占用、成本、到期与释放条件。没有合法预留 producer 时保留持有/延期方向，预览或本地标记不造 hold |
| reroute_or_pause_service | 原物流、服务或工业 profile 支持的确切残余范围、当前路线/接收/安全处置与必要重新接受；已发生 sink、在途、earned/unpaid、原费用和责任保留。不能把暂停当退款或把另一个目的地当原目标完成 |
| exit_specialization | 停止本人尚未承诺的新风险，逐原领域处置旧义务；可以保留库存/设施/合法普通经营。原资金、在途或未知结果未结时明确退出经营与结算未完分别存在，不要求入会或接受赞助 |

每条预览说明服务哪个真实需求、第一项或下一项实际交付、主要增量投入、阶段/截止、保留与失去的价值、可能打开的选择及下一次回访触发。原已签价款、保证金、实际花费 D、残余业务 O 与未来费用 C 沿原键分开；退款不重置 D，未来收入不支付当前原费用。原出价/工作图有模块、验证者、许可或关闭费用时引用其完整报价与 payer，不在成长查询中执行模块或用缺 owner 解释免费。

### 13.3 中期扩容的四种选择

expand_capacity_preview 固定原需求/目标及 current factory/capability，而非从产量增长反推需求。比较 add_parallel_line、upgrade_existing_line、stabilize_before_expand、defer；各方向分别绑定原 Build、SC-25 profile 或当前维护/生产/安全处置动作。并行新建使用合法新 ID/站点/输入来源，不能免费复制旧厂；upgrade 只在原 profile 明确支持时可选，绝不让比较器生成升级参数。stabilize 说明具体阻断、原动作能改变什么及到哪个真实边界再比较；defer 不预留容量、不延长报价或承诺不存在的订单。

每个方案从同一初态独立求值，检查原厂与新增候选共同需要的输入、电力、物流、terminal/共享容量以及仍存旧义务。按原规则逐步计算增量、峰值与阶段，不用净收益掩盖先付不足，也不把两候选同时花同一资源。旧工作沿原 cutover/disposition：accepted-unstarted、WIP、transit、buffer、terminal-pending 均保留原权利/数量/费用；未发生的选项不改变当前能力。原目标不接受新地点/产物/用途时明确需要原目标修改，不能为让扩容胜出而扩 scope。

同目标方案先排除真实阻断、缺关键来源及不支持路径，再展示成本/阶段/保留价值的可核差异。没有原统一价值/价格依据时保持资源向量和明确取舍；不以总吞吐、静态推荐词或外部赞助标签排序。有限范围内没有可行方案不证明只能依附强组织。成功后的当前能力及下一原目标须重新读取，不能将预期新产能投影成实际产出。

### 13.4 checkpoint 的事实和历史

leverage_checkpoint_summary 仍是只读解释。每项结论保存原 beneficiary/world/branch/规范地点或事项、原效果/权利身份、当时与当前规则、前后可用条件、历史覆盖及当前合法下一步：

| 原成长分类 | 足够的正向依据 | 独立反例 |
| --- | --- | --- |
| new_option_unlocked | 原已提交能力/权利确实增加一项此前不具备、当前仍可使用的明确选择，并保留其成本/期限/权限 | 仅 quote、建厂受理、分数上升、当前资格无实际可消费结果，均不补成可用机会 |
| resilience_improved | 原能力/处置结果使一个明确故障条件下新增合法恢复分支；前后条件及原动作均可核 | condition 增加但原 blocker 未改变，不宣称消除缺料/物流/权限风险 |
| negotiation_position_improved | 原有效协议/权利明确改变本人有限协商选择、期限或可退出条件 | 收款、名气、组织名字或模型赞誉不推导市场议价权；无原依据保持未证 |
| regional_usefulness_increased | 原真实受益需要、本人可归因贡献及实际用途/服务 predicate 完成；解释确切新用途或范围 | 重复同用途合法经营可以有价值，但不是每次新增成长；一项 child 不代表整组完成 |
| grind_only / unverified | 分别为已证只有原重复量增长，或缺少上述必要来源/历史 | 不能以缺页判零历史，不能因无法证明成长抹去真实交付或禁止普通操作 |

沿 §5.4 的供应用途首次键不变。其他原已采纳服务的用途类别固定为原 typed obligation kind、目标 predicate 和规范事项/地点；actor、请求、接收者别名、合同编号、epoch 或政策改名不创造新类别。历史等价关系由原 authority/schema 迁移提供；缺原键/前态/完整索引时只报告本次实际结果，first/new-growth 未核实。服务/RD 多视图只引用同一原 effect，已完成单位不能换角色再计；已有 same_loop_repeat_count 与 >=3 且 throughput_only/unclassified 的停止强化规则保持，比较不重置它。

有限影响只解释原已生效的局部权利/机会及实际可见性。原供应例给 B 的 Site 送 iron，不自动符合地方认可的预声明 program 与 agent:O 接收条件；如要走该路线，须在原政策下明确选择新的合法贡献/审核/申领，旧贡献不倒签。受益人仍可继续普通生产/运输、持有、退出或恢复；认可失效、来源到期或未赢得容量不把其降为旁观者。

### 13.5 消费、选择与恢复

Runtime 的有界来源组合提供同一 read envelope，World Service 保持当前读权及证明等级，Viewer/pure API/Agent 按声明能力显示同义的原需求、四方向适用性、成本、已发生/未发生、checkpoint 和下一步。原 v1 DTO、静态 branch 文案、聊天或 SDK 局部能力不能代签新读面已接通；缺接线为明确未提供，不伪造可执行 action。Agent 的建议只读，显式选择后仍逐步经原权限、quote、签名及 authority 执行；不强制普通动作必须先运行本查询。

每步保留原 operation/action/receipt；未知先走[原操作恢复](../../world-simulator/llm/agent-game-client-boundary.design.md#original-operation-lifecycle-port)，不得因旧回应丢失改为新订单、另一扩容或新机会。原物理已 committed 而服务/成长投影落后时保留原结果，只补核未完成投影，不再生产/供电/运输/计费。业务 G1 StatusUnknown 继续按原 authority 隔离整个旧 World；纯只读报告丢失不创造该世界状态。查询重试重新核当前读权与 cut，不发布新业务事实；来源/目标/授权变更撤下当前推荐，已发生结果按原版本保留。

<a id="contribution-growth-composition-budget"></a>
### 13.6 有界组合与验证

组合只接一个明确阶段（续作或扩容）及同一当前目标/原义务，至多四方向、每方向一个显式候选；每候选至多八个原动作/完成边界合计，不搜索全世界，不在后台遍历任意模块图。来源包含所有适用原占用、目标/义务、历史/费用、背景 writer 与返回重核；未知范围不能作为空范围。

采用[比较宿主的来源/内存/并发/取消计量](production-recovery-comparison.design.md#production-recovery-budget)：请求32KiB、256对象、存活原始+解码 source 4MiB、10批读取、1024次索引/proof访问；本组合最多40次有界原规则求值，响应256KiB，overlay512KiB。单槽请求32+source4096+scratch256+overlay512+response256+工作128=5280KiB，仍在6MiB内；两个原 v1 与后继组合共享同宿主4槽/24MiB、每主体1槽、无内队列和3秒取消边界，不按 profile 另开池。工程初值不代表实测性能。原 source reader 的嵌套读取同样计入，不能调用各自满预算接口后只计一条摘要。

作为正常组合的适配容量分配，目标/身份/地点24对象、原结果与历史32、当前候选依赖64、权限/真实费用48、独立证明节点64、返回重核8，合计240；各组含逐项业务集合与实际证明，剩余16不能藏无限历史/费用图。每条实际大小、总4MiB、10批和40次求值仍需同时验证，分配表不是已测编码或完整性证明。精确当前索引与原 authority 的有界范围证明缺失、单个闭包超限时返回 incomplete 与原合法下一步；不得删旧义务、把完整图上界当实收或扩大 scope。

本节补足原成长组合职责，不提升现有实现或真实 mature-world 证据。原 v1 的细粒度用途证明不被组合摘要替代，既有专业事务和费用也不由比较器重写。
