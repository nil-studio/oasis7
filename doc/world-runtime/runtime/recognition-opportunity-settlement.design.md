# 成熟世界认可与有限物流机会结算系统设计

- 生命周期：`active`；Owner role：`runtime_engineer`；协作：`gameplay_designer`、`agent_engineer`、`viewer_engineer`、`blockchain_ops_engineer`、`qa_engineer`；Last reviewed：2026-11-04。
- 固定审读基线：`25ae2095e67e00179cc1e46bb8d2f6a428b9055a`，tree `505242f1cfb890b02bc1f0bd1fa9dc29061ee7aa`。下文区分源码事实、目标合同和静态算例；本文没有 Cargo、游戏、模型、正式客户端或故障注入运行结果。
- 上游：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)、[专业认可/机会政策](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-recognition-opportunity)。验证：[专用手册](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-01)。

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026。

## 1. 问题、目标与非目标

一次运输成功、历史贡献、情境声誉或 MetaGrant 分数，都不能说明玩家已经取得某个当前可用的稀缺机会。初版目标 profile 为 `local_route_recognition_v1`：一名原线路供给者预声明本地交付用途，审核真实交付事实，产生绑定个人受益者的有界认可；两个合法主体可以申领同一个只能结算一次的物流容量机会，获胜者凭 receipt 启动一次原 `TransferMaterial` 运输。原账本、运输费用、路径、在途和完成结果共同证明实际使用。

本 profile 只支持同一 canonical world/branch、一个原物流容量域、一个指定瓶颈 route、一个 material kind、一次不可拆分的正整数运输量，以及预声明的有限申领轮次。一个 program 绑定一个 opportunity occurrence；正常样例的量为 1，不是对整个游戏容量或经济参数的规定。认可和机会都绑定个人受益者及其当前控制的执行 Agent，不能成为可出售、代理或组织代持的 token。

本文不自动授予材料、电力、Data、OC、治理权、区域控制或全局优先级；不修改 FI-002 的 pioneer transfer，不把 CR-003 情境声誉转成通用资格，不扩张 GR-001 设施权限。它不实现陌生工厂代排产、共享 micro_depot 所有权、任意服务合同、跨世界机会、自动续期或新的资源 mint。已有普通物流继续承担独立行动路径；本设计不把认可设成普通运输的必需凭证。

## 2. 上游约束与相关角色

Gameplay 拥有相称事实、资格、有限机会和复核政策；原线路 authority 拥有 route、capacity、availability、tariff 及普通服务规则；个人身份、Agent 控制及 delegation authority 拥有当前授权。Runtime 负责同切点校验、容量守恒、完整轮次、一次性效果及恢复。World Service、Viewer、API 和 Agent 只消费符合当前读权的事实。持久提交与 finality 沿原 runtime/blockchain authority，模块不能自行宣布成功。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [AC-RO-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-01)；Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026 | 首次持续能力之后才可产生本 profile 的认可；普通成功不自动授予 | [阶段与事实](#recognition-stage) | 成熟世界/持续能力事实 producer、原运输历史、供给者审核 | starter、直接设置 stage、meta points 不证明成熟来源 |
| [AC-RO-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-02) | 当前权限、来源、地点/用途/期限与复核边界分别可读且独立校验 | [原 authority](#recognition-authority) | 当前 owner/identity、delegation、route authority、受权读面 | route owner 不获得请求人的货物所有权；受限信息不补成有效 |
| [AC-RO-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-03) | preview 无世界效果；generic inbox pending 无排他占用、费用或顺位增益，accepted 制度状态的 O 原费另明示 | [预览与待决](#opportunity-preview-pending) | 有界只读 adapter、同根事务免业务费用协议登记 | 当前收费 WASM 写入不代签免费 inbox |
| [AC-RO-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-04) | 两名合法竞争者、同一 occurrence 和容量单位，最多一次 receipt 及实际使用 | [一次结算与使用](#opportunity-once-settlement) | 共同 route capacity authority、完整 allocation round、原物流 consumer | 单独 DTO、局部模块锁、先分配后补救、隐藏优先级 |
| [AC-RO-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-05) | 提交至结算的权限持续有效；撤销后恢复也不复活旧 pending | [当前重验](#opportunity-current-revalidation) | 原授权历史/失效屏障、当前同切点来源 | 仅检查当前布尔值、恢复后自动重提或沿用旧等待身份 |
| [AC-RO-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-06) | 失效保留历史、释放未使用占用、停止未来使用并保留独立行动 | [状态与恢复](#opportunity-recovery) | 原 transit 完成/费用 authority、完整持久 generation、操作恢复 | 失效不撤销已合法运输，不退款、不重发旧 occurrence |
| [AC-RO-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-07) | 非转让/叠加、受限审核及 FI/CR/GR authority 边界 | [反滥用与隔离](#recognition-abuse-review) | 预声明审核与申诉政策、个人当前控制来源 | 怀疑直接惩罚、全局评分资格、跨专题权利转译 |
| [AC-RO-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-08) | 同候选真实 producer/消费者与跨角色证据；静态设计不代签闭环 | [消费与证据](#recognition-consumers) | 正式 Runtime/World Service/Agent/Viewer/API、blockchain、QA | 单机 fixture、共用 DTO 的 presenter 和历史 CI 不证明实玩 |

## 3. 当前状态、目标状态与差距

| 对象 | 固定基线的源码事实 | 本设计目标与未交付接缝 |
| --- | --- | --- |
| 物流供给 | [LogisticsRouteV1](../../../crates/oasis7_world_state/src/state.rs) 有 exact from/to/kind、owner、capacity、reserved、tariff；[注册与停用](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_core.rs)分别记录 requester 为 owner、核对现 owner | 这些原件可作为供给来源；尚无本文 program/review/offer 的正式原 owner producer。原 native 注册字符串和可信 fixture 不能代替当前 owner 签署 |
| 原路径 | [logistics](../../../crates/oasis7_runtime/src/world/logistics.rs) 验证每 hop 的端点衔接、同 kind、无重复 ledger/route，最多 8 hops；quote 与 action 复用路径解析 | 两个不同 Agent 不能把同一 Agent 源账本冒作共有；可用 A→H、B→H 两条 feeder 共享 H→O 的瓶颈 |
| 占用与实际使用 | [material transit reducer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs) 在开始时逐 route 增 reserved、扣实际源料与 requester tariff，完成后给目的账本入料、释放 route 占用并产生 settlement receipt | 当前 reserved 主要承载原在途占用，没有认可 allocation；需要所有入口共同消费的容量 adapter 和 allocation→transit 原子转换 |
| 原物流授权 | [delegation](../../../crates/oasis7_runtime/src/world/agent_delegation.rs) 按原 action kind 和 from→to:kind 绑定；带 grant 的 Transfer 要求 source 为该 Agent 账本；原 quote 算 tariff | 认可不取代原 grant。目标 Use 必须复用该验证与一次预算消费，不能只检查新 receipt |
| 原 owner 操作 | [owner install](../../../crates/oasis7_runtime/src/world/owner_delegation_control.rs) 的正式 allowlist 含 TransferMaterial，不含 RegisterLogisticsRoute | 可用既有正式 Transfer grant；不能宣称已具备所有 route owner 配置/发布流程 |
| 事实证明 | 开始/完成事件保留 actor、两端账本、kind、数量、路径和时效；`LogisticsSettlementReceiptV1` 保留 job/path、tariff/payout，未单独包含完整材料/actor 来源 | 必须连同原开始/完成、settled 索引和当前 source authority 读取。仅 receipt ID 或普通完成不是认可 |
| 完整轮次 | [工业 allocation 合同](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-allocation) 已要求同 snapshot、完整 contender、稳定公平策略和重试等待连续 | 当前 native 物流直接容量检查不是该目标轮次的完整实现。本文窄 profile 的共同适配与可执行 comparator 尚未接通 |
| preview / 登记费用 | [preview](../../../crates/oasis7_runtime/src/world/cognition_command.rs) 有只读入口；[module metering](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs) 向实际 artifact owner 收 Data/Electricity | 免费 generic inbox 与有实际制度状态迁移的 WASM 调用分层；后者保留原计量。尚缺同根协议接纳、O 的明确费用授权及受保护 operational/closure hold 适配 |
| 工业/供给接缝 | [industrial start](../../../crates/oasis7_world_state/src/world_rules/industrial_work_start.rs) 引用 logistics route/path，不因此创建新运输占用；[旧 DispatchSupply 分支](../../../crates/oasis7_world_state/src/world_rules/supply.rs) 虽调用 transit reducer，但 `valid` 当前恒为 false，AuthorizeSupply 也拒绝该材料路径 | 不把未激活分支或预装授权当合法工业供给样例；它和未来其他 writer 必须纳入启用覆盖清单，不能为本设计改 false 或绕原产品边界 |
| generic 认可消费 | 原社会事实、MetaGrant 和情境声誉各有自己的意义 | 尚无本 profile 的事实审核、持续权限屏障、一次 occurrence、receipt Use、恢复索引或正式三入口组合证据；当前为目标能力，启用 blocked |

§3 的事实只证明可复用的原机制存在。下述接口、状态、容量分类和新 capability 均为目标合同，不是当前可调用 API 或已实现 Rust 类型。

## 4. 边界与结构

认可规则放在受治理、按 `(world, module_id, instance_id)` 隔离的 WASM institution 状态中；Publish/Review、有限轮次排序、allocation 和 Use/处置的制度判断均由当前 governing manifest 选择的模块产生有界 proposal。Runtime/Kernel 只做原结构、当前权限、资源、容量、业务键和事务校验，不能在 native handler 重写制度规则。遵守 [Native 分类与冻结](../prd.md#native-surface-分类与冻结规则)及 [Institution Migration Test](../design.md#61-institution-migration-test)：本文不选择 SC-32 pilot、不直接扩 core Action enum 或 WorldState 顶层；新增的是原统一执行边界的受限 host capability 和版本化模块合同，不能另起一个无治理的 allocator。

<a id="recognition-stage"></a>
### 4.1 成熟阶段、相称事实与审核 producer

供给者 O 的当前个人 owner 通过已鉴权的 `PublishRecognitionProgramV1` 声明：exact world/branch、原 route/capacity domain、个人 issuer/执行 Agent、local delivery 用途、material kind、接收账本、事实观察窗口、认可开始/到期、申领和使用边界、审核/申诉规则及唯一 occurrence。首版接收账本是 `agent:O`；若要改为 site/组织接收方，必须另有其原 authority 适配，不能从文本名称推断。

发布登记明确供给意图和有限 occurrence 上限，不占物理容量，不创建 claimant 顺位，也不保证随后分配；O 自己明确接受的执行费用及预算占用另按 §9.2 处理。原 owner 的供给许可不授权从任何他人账本取料。route 必须已由合法来源存在，且普通服务保护份额有原容量证明；缺正式来源时该前置未成立，不能由 program 顺带注册、补造或扩大 route。

`ReviewRecognitionSourceV1` 由同一当前 issuer owner 显式发起，包含 program revision、受益个人及其 Agent、原交付 job/开始/完成引用、相称事实类别、审核规则版本、理由类别和复核路径。WASM 消费经 host 验证的同一 canonical 历史，按预声明制度规则产生决定；Kernel 校验决定引用、当前权限及受限效果。必须核对：

1. 受益者已有首次持续能力的完成事实，且样例发生在其之后。首版只接原 canonical factory 稳定线历史：合法 FactoryBuilt、同一受益者的 builder/control 来源、连续完成窗口与原 `FactoryProductionSnapshot`。现有 [canonical helper](../../../crates/oasis7_world_state/src/state/industry_transition/mod.rs) 要求相同 canonical snapshot 的 repeat count 至少 3；该计数和 snapshot 由 [recipe completion reducer](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs) 更新，不在本设计另定门槛。目标来源 adapter 以受权 exact factory/历史 cut 为键，返回该原 snapshot、三个连续完成 job 的开始/完成关联、当时个人归因和完整窗口证明，并按当时规则验证。原 writer 随 canonical 完成维护可重建的历史索引；不先全世界搜 factory 或扫 journal。当前 [capability query](../../../crates/oasis7_world_state/src/gameplay_capability.rs) 的 `current_stable_line` 是当前状态，不能替代历史证明；后来受阻不抹去已经完成的历史持续能力。只设置 stage/count、starter grant、普通动作、旧 sample 标签或模型叙述不能补签；无可验证历史时 `maturity_source_unavailable`。
2. program 的实际发布 commit 必须早于源交付的权威接受/开运，且交付在声明的观察窗口内；不能倒填观察期补签已有交付。原 transport 开始与完成的 actor、source/destination、kind、sent/received/loss、route/path、job 及 settled 索引一致，满足预声明用途和接收 ledger，实际收到正数。原事件的 Agent 字段还不证明个人归属：必须读取开运接受当时的 beneficiary/Agent/control source/generation 与原认证上下文，而非以审核时的当前 owner 倒填。目标 attribution adapter 在原正式 Transfer 接受的同一根提交，从已验证 owner/delegation context 持久关联原个人与 action/job/start，completion 按 job 回链；该索引不是新资格。旧历史没有该原件或可验证等价认证记录时 `source_unavailable`；接管 Agent 的新个人不能取得旧个人交付认可，余额差也不能替代因果。
3. 审核者当前控制 O，O 是指定 route 的原 owner；审查结论只对本 program/local scope 生效。正常 tariff 是原运输服务费，不自动构成购买认可，也不能被额外付费替代真实贡献审核。
4. 同一 `(program, source review, beneficiary)` 的重复请求只读原结论；同源冲突理由/字段拒绝。审核写入后才有有界 recognition record，完成事件本身不自动授予。

`RecognitionRecordV1` 保留 immutable source/review、issuer、beneficiary、purpose/scope、有效期与可复核处置版本。它是资格依据，不是资源、可用票据或全局分数。一个 program 对应一个 occurrence；不以本例发明全世界永久独占的 source token。另一显式新 program 是否允许消费同一事实，仍由其预声明范围、用途、期限与审核规则决定，不复制既有 receipt 或未结义务。

<a id="recognition-authority"></a>
### 4.2 当前 authority、来源闭包和容量 owner

| authority / producer | 必须证明的当前事实 | 允许产生的结果 |
| --- | --- | --- |
| 个人身份与原 Agent owner resolver | current owner、identity generation、合法经济控制来源、个人/Agent 绑定 | 认证 issuer、beneficiary 和操作人；Agent 是该个人的执行者，不能替第三方保管认可 |
| 原 delegation / permission | 原 Transfer grant/source/revision/period、精确 source→destination:kind、剩余额度、有效期与失效历史 | 允许提交和最终原运输；认可不新增 grant、不消费别人的账本 |
| 原物流/容量 authority | route owner、端点/kind、availability、capacity/占用、tariff、capacity epoch、完整同域候选与原策略 | 唯一容量分配/释放与原 transit；模块不得用自存计数绕过它 |
| program/review authority | 已发布用途/期限/规则、相称原事实、明确审核、当前处分/申诉结果 | 当前局部资格判断与一次 opportunity occurrence |
| 原世界与提交 authority | world/genesis/branch、parent/root、canonical tick/phase、manifest、authority epoch、完成 generation | 有效 pending、allocation、use 和原完成结果的提交证明 |

每次读返回 exact source cut 与逐项完整/缺失状态；按当前 audience 限制原件。`source_unavailable`、读权不足、索引落后和业务上确认不存在分别处理，不把空值或缺页解释为有效/无占用。必须有 keyed/range adapter 完整读取所选 program、recognition、claim、source job 与容量域；不能先复制完整 World 或扫描全部 journal 再声称有界。

地点/作用域以该 world 中的原 route、实际端点 ledger 和 material 为准，客户端可附受权的原地点说明；自由文本区域名不能替代原引用，更不能产生领地控制。历史 source 的 read proof 与当前资格/权限必须分别标识：历史成功不提供当前权限，当前可读也不代表可执行。

`RouteOpportunityCapacityV1` 是原 route authority 的目标适配。其同切点账簿按原 reservation/job 身份把权利分成互不重叠的原占用、未使用认可 allocation `L`、已使用后仍在途的 recognized transit `T_rec`；`T_rec` 已包含在原在途 `T` 中，不能再次加总。既有工业 reservation 只有尚未计入 T 的独立权利才另计 `I`。必须证明 `T + I + L <= C`，并保留原已受理义务与 profile 声明的普通服务份额。

普通份额不是永远空置的容量。若原 policy 保护 `ordinary_floor`，应维持 `L + T_rec <= C - ordinary_floor`；普通在途合法占用和仍可供普通使用的量共同保留该份额。普通请求可以依原规则使用未分配容量；发布 program 不先建隐藏 H 池，认可不能挤走原已承诺工作。容量、角色分类和索引必须由同一原提交维护，不能把已使用 recognized transit 改标 ordinary 来腾出新的认可额度。

## 5. 关键运行流程

<a id="opportunity-preview-pending"></a>
### 5.1 预览、显式申领与无排他待决

`PreviewOpportunityV1` 返回当前认可依据、供给者、用途、原路径、预期单位/费用、当前 blocker、明确期限、可用下一步及证明等级。它只读，不运行收费 sandbox，不写资格、队列、顺位、capacity、资源或后台动作；邀请、推荐和“当前符合条件”同样不产生权利。预览引用不能在之后替代 fresh 校验。

`SubmitOpportunityClaimV1` 由当前 beneficiary 显式发起，绑定 immutable program/occurrence、recognition、beneficiary/Agent、原授权来源和连续性屏障、exact 原 Transfer payload，以及原 namespace 下的稳定 request key/digest。首版 payload 指定自己的 Agent 源账本、program 接收账本、一个 material、正 quantity、显式不超过两 hop 的路径、`auto_reroute=false`；必须包括指定瓶颈一次，不能通过自动选路换到另一 scope。报价继续采用原规则。

接纳只形成 `pending` 协议记录及下一轮/重验条件。所谓 hold 是有界待决标记，不是 `L`，不冻结材料、电力、delegation 预算、容量或私有排他锁，不获得请求先到的顺位。当前可用性会变化，申领不保证成功。重复预览、重试、重连、请求 ID 变体和多 Agent 代持不能增加候选份数；每个 beneficiary/occurrence 只有一个活动候选。

协议登记必须在原 [ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0) 内有 generic bounded inbox capability：只接受已鉴权 request/digest、原权限绑定和 pending/协议终态元数据，不运行 sandbox、不做认可/资格/排序判断、不收费或发布制度效果。现有收费 WASM command 不能冒充这个免费 inbox；该接缝缺失时禁用写入口，不以客户端数据库充当权威 pending。

后续 Publish/Review/CloseRound/Use 若形成实际 accepted 制度状态/效果，必须执行治理 WASM 并保留原 Data/Electricity metering。首版实际 artifact owner 必须是发行人 O，O 的当前 owner 显式接受可读执行绑定、命令次数和自身受保护费用预算；这与 A/B 仅在原运输启动时承担的 tariff 分账。没有额外认可购买费、申领费或败方 charge，不意味着原模块执行免费。原费用、合法 payer、同根结算及 no-effect 分类由 §9.2 唯一规定；不得依赖缺 owner 免费分支或改变旧费率。

<a id="opportunity-once-settlement"></a>
### 5.2 完整轮次与一次 allocation

此 profile 在发布前固定 `claim_open_tick`、严格递增的 `round_close_ticks`、`settle_before_tick`、`use_before_tick` 及观察/认可有效区间。新增区间均半开，必须满足 `claim_open_tick < round_close_ticks[0]`、`last(round_close_ticks) < settle_before_tick < use_before_tick`，且 Use 截止不晚于认可到期；最后 close 等于 settle 截止时拒绝发布，不能承诺一轮已经不可结算的竞争。所有推导使用 checked world-time，缺时基或溢出拒绝发布。原 native grant 的 inclusive/exclusive 解释保持原值。某 close 只包含此前已经 canonical 接纳的 claims；同 tick 的边界以原提交阶段/序号明确划开，边界后的新请求只能进入下一预声明轮次，没有下一轮则已关窗。未提交的轮次不会因重启延长截止。

每轮以同一 parent/capacity epoch 读取完整相关集合：已受理普通/工业候选、原在途与 reservation、所有在本域竞争的认可 claims 及 occurrence 预算。原入队 writer 在同一 authority 下维护候选范围/完整水位；达到工程上限时仍必须给出完整性或 overflow 结果，不能让先录入者变成封闭全集。第 33 个合法候选使 32 上限被超过时，该轮全部新分配被冻结/拒绝，不能拒绝第 33 个后让前 32 个继续结算；事前声明的独立域分区才可分别处理。

认可候选的具体稳定顺序是 `(first_eligible_round, canonical_beneficiary_id)` 升序。`first_eligible_round` 是首次以有效状态进入已关闭轮次的序号，序号较早者等待较久；不是累计等待次数升序。canonical beneficiary 是原不可自选的个人身份绑定，不能取显示名、request ID、preview 时间或网络到达顺序。每人一候选，重试/重连不刷新原轮次；仍有效的容量 deferred 保留该值。失效授权后的新提交是新的候选，不能继承旧请求的可结算身份或伪造等待历史。

该 comparator 是原 capacity allocator 在本 profile 的认可候选分支内采用的确定规则，不是在其外另做一次分配。完整域的原普通/工业 policy、已承诺义务和普通保护份额先作为不可越过的约束；认可候选连同 `occurrence_remaining <= 1` 约束在同一次求解中决定。不得先给 A/B 私分 L、再让原 allocator 修补；也不得让 generic receipt 插队于原等待规则。若原域尚不能表示该候选类别、完整集合和上述具体 comparator，profile 拒绝激活；旧 native 入口不被静默改成另一套排序。

每轮只有在这些前提同时成立时结算：program/review/权限当前有效且持续性成立；原 payload 的 source、kind、路径、grant、材料、电力和时限满足；瓶颈有可由原 policy 分配的 q 单位；其他 hop 与原全局在途数当前可准入；occurrence 仍未结算。L 只承诺指定瓶颈，不预占其他 hop 或全局在途 job 槽；已有两条无关 transit 时不能宣称可立即使用，Use 仍须再检原全局上限。prepare 暂存 `L += q`、winner claim、原 capacity reservation、immutable allocation receipt、occurrence 的一次预算消耗及败方处置，最后在同一 parent 上 CAS 并一次提交。

**普通原入口如何接入。** 活跃域的原 native action batch、受治理 effect proposal 与可用工业请求，在原业务扣减前向同一 capacity session 提供已鉴权候选及资源集合；关闭轮次的 parent 明确哪些是已提交 T/I、哪些是本批尚未分配候选，不能同时遗漏或重复计数。原普通候选仍按其原域规则在该次提交获得原 transit 或原拒绝，不自动制造新的持久普通排队权；认可只在其关闭边界参与，其他时刻的普通准入也必须看见 L。原 accepted outcome 若无法在该边界维护完整集合，返回 capability 不支持而禁用本 profile，不能让一条 native 立即 reserve 旁路与模块 round 并行。新的完整性索引、candidate→原 reducer 和 receipt-aware quote 适配随域启用一起安装，原现有 policy 不兼容时保持 legacy 普通服务并禁止认可发布。

实际 writer 清单先覆盖 `action_to_event_core::TransferMaterial`、共用路径 quote/selection、`PreparedMaterialTransit` 的 start/completion 及原受保护资源扣减；raw/trusted publication 同样受最终守恒校验。`industrial_work_start` 的 route/path 引用不是新增容量写入。旧 `DispatchSupply` 当前因 `valid=false` 及 AuthorizeSupply 拒绝而未激活，且其端点只支持 Agent→Agent，不能拿它冒充正常 site:H→O 的合法冲突请求；保留显式 disabled 记录。任何后续启用的 supply/工业 writer 必须先补真实原授权/来源并接入同一 authority，否则只能禁用其触达该活跃域，不为 MW-004 绕过原拒绝。

容量竞争不会向未获分配的 A/B 扣费或产生 L。只读判断、deny、重复或没有任何新 module state/effect/emit 的 deferred 是 no-effect，原新费用为零；若该轮第一次固定 `first_eligible_round`、更新合法等待/完整轮次决定，属于 accepted 制度状态迁移，即使没有 L 也按 O 已明示预算承担一次原执行费。generic inbox 不保存伪造的资格/排序决定来规避 metering，普通审计行也不能把纯 deny 变成收费成功。首版 occurrence 只发一次 receipt；获胜结算后其他候选原子终止为 `rejected_once_allocated`，保留其历史并给出普通运输或其他独立路径。未有获胜者且权限有效的容量 deferred 可以等下一预声明轮次；过了最终边界则终止，不无限等待或自动开新 offer。

### 5.3 Receipt 的实际 Use consumer

`UseOpportunityV1` 是独立版本的受治理命令/capability envelope，包含原 receipt、稳定 use request 身份及 exact 原 Transfer payload；不往旧 `TransferMaterial` wire 偷塞 receipt 字段。Runtime 从当前原件重验主体、当前权限/预算、路径各 hop、材料、电力、最大在途数、截止点和未使用 L，采用相同的原路径解析、tariff、loss、duration 和 ledger 规则。

原 receipt-aware quote/admission 对瓶颈只给当前 receipt 自有的 q 单位转换权：其他 L 仍算占用；其他 hop 沿原余量检查。它不能把所有 L 从报价里抹去。一次 root transaction 同时执行 `L -= q`、原瓶颈 `T += q`、各 feeder 原占用、源材料扣减、requester 的 tariff escrow/原 delegation 预算消费、O 的真实模块计量/费用 hold 消费、`MaterialTransitStarted`、receipt→原 job 链接与 `used` 处置。瓶颈总占用不增加第二份；全部效果都成功 prepare 才发布。

现有 `prepare_started_with_reservation(..., reserved=true)` 仅表示源材料已经预留，仍会增加每 hop 的 reserved；不能把这个布尔值当作 L→T 转换。目标适配必须显式消费原 capacity reservation 并让普通 Transfer/工业路径共享同一检查；如果只有模块自存 L，现有 native route check 仍会双订，该 profile 不得启用。空 route 的 direct/unbound compatibility 不消费 R，也不能作为该 receipt 的 Use；它只有在原主体、材料、地点/权限与其他原规则独立成立时才是另一普通路径，不能把去掉 route 字段称作通过 R。

Use 成功表示原运输已经启动，不表示货物已到。原 due completion 继续按其原 job、loss、接收账本和 tariff payout 完成，释放原 route T，产生原 `LogisticsSettlementReceiptV1`。认可 receipt 的世界效果链先指向唯一 L，随后通过同次 Use 记录指向唯一原 start/job/completion；不改写 immutable allocation receipt，不伪造另一个运输结算 authority。

Use 前非终态条件如余额不足、其他 hop 暂满，只返回有界可重试 blocker，保留尚未到期的 L 和原截止点；不能扣部分费或续期。错误主体、改 payload、已使用、失效范围/权限和到期按各自终态处理。明确放弃可释放未使用 L；物理槽回到原 authority 不使该 occurrence 的一次预算复原。

<a id="opportunity-current-revalidation"></a>
### 5.4 持续权限与变化重验

申领记录绑定原 current owner/identity generation、grant/source/revision/period、scope、program/review revision 和授权连续性屏障。原 authority 的撤销、缺失、到期或控制来源失效写入必须推进单调屏障或留下同等可验证的 canonical invalidation 记录，并可查到覆盖申领至当前 cut 的完整历史。

结算只看到当前 `authorized=true` 不足。发生过失效的旧 pending 必须转为 `rejected`、`released` 或 `expired`；即使撤销后又恢复也不能产生 receipt。索引漏掉失效区间或来源完整性未知时暂停其结算，返回 unresolved，不能推定持续有效。恢复后的申请须当前主体显式新提、全量重验并关联旧终态，不复制旧顺位或自动激活旧请求。

同样重验原 issuer 控制、recognition/source、用途/地域、资格、反滥用处置、route epoch/availability、quantity、期限与当前原 grant。权限仍有效而仅容量或可恢复资格条件改变，可以按原轮次继续 deferred；不能把已失去 permission 的请求混入这类等待。所有写事务在 commit 前对相关 revisions/屏障再 CAS，不能用 read→write 空隙恢复历史授权。

settled 但尚未 Use 的 L 在撤权/撤销/到期后停止使用并至多释放一次。已经 Use 的 receipt 不再有 L；随后失效只阻止未来新操作，不删除原 transit、不退已扣 tariff、不返已发材料或改写合法历史。原异步完成义务仍由物流 authority 处理。

## 6. 接口与数据合同

### 6.1 目标记录与身份

新 payload 使用独立 `schema/profile_revision`，canonical CBOR 和域隔离摘要，沿 [WASM 执行合同](../wasm/wasm-interface.md#sr2-execution-contract) 的编码规则；旧 JSON route ID、旧 Action 和旧 receipt 编码保持原值。新摘要域包括 `oasis7/recognition-program/v1`、`oasis7/recognition-request/v1`、`oasis7/opportunity-allocation/v1`、`oasis7/opportunity-use/v1`；摘要正文不包含自身 digest、签名、本地时间、网络重试序号或外部 block wrapper。

| 记录 | producer / 必需内容 | 不变量 |
| --- | --- | --- |
| Program / recognition | 原 issuer owner 的明确决定；source/review、个人绑定、route/purpose、观察/认可/申领/使用边界、规则版本 | 一 program 一 occurrence；版本改变不静默放宽已接受 scope 或延长旧有效期 |
| Claim | 本人显式提交；program/recognition、原 Transfer payload、授权来源/屏障、request key/digest、first eligible round、状态/理由 | pending 无 L；同人/occurrence 一个活动候选；未知不能新建身份绕过去重 |
| Round | 原 allocator；round boundary、parent/root、capacity epoch、完整 contender/既有义务集合摘要、原 policy 与本 profile comparator 版本 | 不以到达顺序补不完整集；每次 re-evaluation 有原 trigger+domain 去重 |
| Allocation receipt / disposition | 原 capacity authority；occurrence、winner、source/review、place/purpose、q/单位、有效期、reservation/effect ref、commit proof | occurrence 唯一 winner、一个有效未使用 L；receipt 身份与可变处置分离 |
| Use linkage | 同一次原运输提交；allocation、原 use request、exact Transfer digest、原 action/job/start ref、消耗状态 | 一 receipt 一 original job；任何业务键重试都不得二次扣料、扣费、占用 |
| Operation / invalidation index | 原 writer 随提交维护的 request 结果、授权屏障、review处置、历史个人归因和原 job 链 | 完整 generation 可重建；不另创资格账本或将私有原件公开 |
| Fee authorization / holds | O 明确接受的当前 artifact payer/source、schedule/manifest、有限成功命令/分槽、operational/closure Data/E hold 与实际 charge refs | 原 gross 余额的受保护占用，不是新资产；closure 不借给 review/round/Use，no-effect 不收费 |

请求键同时受 `(world, instance, operation_role, original_request_key)` 和业务键约束：审核用 program/source/beneficiary，申领用 occurrence/beneficiary/明确新提交序次，结算用 occurrence，Use 用 allocation receipt。新网络键不能绕过业务键；同键同 payload 返回原结果，同键不同 payload 为 `idempotency_conflict`。新提交序次只在原请求已知终态且尚未消费 occurrence 的条件下由权威推进，不由客户端随机生成。

### 6.2 跨组件接口

| 接口 | producer → consumer | ordering / idempotency | 成功、错误与兼容 |
| --- | --- | --- | --- |
| `PublishRecognitionProgramV1` / `ReviewRecognitionSourceV1` | 当前 issuer → 治理 WASM / 原 host authority | expected revision、业务键、原 source cut、O 费用授权；同根事务 | accepted 制度状态与真实 metering，或无效果拒绝；缺 owner/source/maturity/hold capability 不启用 |
| `PreviewOpportunityV1` / `ReadRecognitionOperationV1` | 有权客户端 → 原只读 adapter | 同 cut、当前读权、bounded keyed/range；不写 | complete/blocked/incomplete/unknown；旧客户端缺 capability 不假填 available |
| `SubmitOpportunityClaimV1` | beneficiary → 免业务费用协议 handler | 原 request/digest 与业务键；记录当前屏障 | pending 或无效果拒绝；ACK 不等于 allocation |
| `CloseOpportunityRoundV1` | 原 canonical round boundary/trigger → 治理 WASM / 同域 allocator | trigger+domain 去重、完整集、CAS、固定轮次费用槽 | allocation/accepted 等待状态或 no-effect deferred/terminal；新等待状态也有 O 原费，不得人工挑 winner |
| `UseOpportunityV1` | 同 beneficiary 的当前执行 Agent → 原物流事务 participant | receipt+use key、原 action/job 链；L→T 与费用/料一次提交 | used-started / original blocker / commit-unknown；旧 Transfer 不接受伪新增字段 |
| `ReleaseOpportunityV1` / `ReviewDispositionV1` | 本人或原合法到期/审核 authority → 治理 WASM / 原容量事务 | 未使用 L 和状态 CAS、受保护 closure 槽；独立 effect role 去重 | accepted released/expired/revoked 与 O 原计量；已 Use 只读原结果，不造退款 |

错误至少区分 `unsupported_profile`、`maturity_source_unavailable`、`source_unavailable`、`not_authorized`、`authorization_discontinued`、`scope_mismatch`、`capacity_deferred`、`round_incomplete`、`once_already_allocated`、`already_used`、`expired`、`idempotency_conflict` 与 `commit_unknown`。响应携带可读的原操作身份、效果已知程度、真实下一步和未决义务；不能把 timeout 翻译成失败重投许可。

<a id="recognition-consumers"></a>
### 6.3 Viewer、API、Agent 与可观察结果

同一 read envelope 区分 `recognition reviewed`、preview、pending、capacity allocation、transport started、transport completed、失效及 commit unknown；只把原 authority 的当前结果投影成可用动作。Viewer/API/Agent 都必须展示来源、用途/作用域、当前权限与 blocker、期限、原费用、receipt 实际效果、待决原因和独立下一步，不能从 UI 文案、模型建议或 receipt 名字推导成功。

Agent 可解释和整理，但显式申领/Use 的主体与原授权不可替换。客户端先恢复原未知 operation，再决定是否按已确认结果重试；不从预览循环启动后台申领，不在权限恢复后自动重提，不重新报价延长旧票据。读权变化时撤下旧可执行投影，历史是否可见仍遵守当前 audience，不向败方泄露胜方私有材料/预算或精确身份。

事实原件包含受限身份、授权、贡献和审核材料；公开读面仅给必要结果与可理解的事实类别。正式 read proof、受信 service assertion、fixture 状态各标其原等级；派生索引 hash 不升级成独立证明。新 envelope 与原 GameClient/World Service 签名、任务 fence、恢复及可信读取适配均是实现前置，本文没有宣称三入口已上线。

## 7. 状态、事务与持久化

<a id="opportunity-recovery"></a>
### 7.1 状态与终态

| 对象/状态 | 可触发的转换 | 效果与恢复边界 |
| --- | --- | --- |
| recognition reviewed / active | 原到期、合法复核/申诉、控制或来源失效 | 保留原事实与决定；停止将失效记录用于未来结算，不惩罚无关独立行动 |
| claim pending / deferred | 完整新 round、显式放弃、持续权限断裂、期限 | pending 无排他权；仅有效请求可再评，失效转终态，恢复也需新提 |
| occurrence unallocated | 唯一成功 allocation | 同时生成 L、receipt、winner/loser 处置；剩余一次预算变为已消费 |
| allocation usable | 原子 Use，或未使用的释放/到期/撤销 | Use 变 used 并 L→T；其他分支释放 L 且 terminal；两者竞争只能一个先提交 |
| used / in transit | 原物流 due completion / 原合法异常处置 | 不再有可重用 L；原材料、费用、loss 与在途义务保持，旧认可到期不退款 |
| terminal / completed | 原件查询、受权申诉的新决定 | 只返回历史；不重新发行该 occurrence，不因 slot 空闲复活 receipt |

### 7.2 原子性、unknown 与重启

一次结算提交共同包含原 capacity 记录、instance 业务状态、winner/receipt/败方处置、O 的实际计量/费用 hold delta、request 索引、journal 与执行证明；一次 Use 再包含原材料/电力、delegation spend、原 transit 和绑定关系。复用原 root transaction 的 prepare、parent/revision CAS、单次安装和 [authority commit/recovery](../design.md#runtime-authority-commit-recovery-target)，禁止 inner commit 或先扣资源后写 receipt。普通 Transfer、工业写入、SetAvailability 和审核失效与本事务冲突时必须在同一 authority 顺序上重验。

confirmed precommit failure 丢弃全部本次暂存业务效果；其拒绝记录只有自己提交成功后才可宣布。commit 结果未知时保留原请求、业务键和完整 payload，先读原 operation/receipt/generation；不能根据“暂时查不到”重发新 occurrence 或新 Use。原 writer 状态无法判定时按原恢复服务闸门 fence 写入，不把未知回滚成可重做。

完整 generation 必须一起保存 program/review、当前处置/授权屏障、claims/round 与等待身份、capacity L/原占用、once budget/winner、use→原 job、O 的费用授权/holds/actual charge 与剩余程序槽、request 去重、journal/receipt 及历史 manifest。恢复验证 world/root/schema、数量守恒、唯一业务键、已使用与原 transit 的相互引用、terminal 与释放一致性；半个 L→T、缺旧 winner、混代 generation 或未知 schema 拒绝安装。

canonical replay 重建已提交事实与 delta，不重跑审核、重新分配、重新报价、延长时间、发模型请求或再次提交物流。投影/响应丢失只恢复原结果。已持久但尚无效果的 pending 恢复后仍需当前完整权限及连续性检查；原失败/撤权/到期历史不能被 snapshot 省略。清理只移除可重建缓存；未决请求、receipt/tombstone 与原 transit 关联须按原保留策略保留，不能为腾空间提前丢弃去重依据。

## 8. 部署、安全与运行约束

<a id="recognition-abuse-review"></a>

启用以能力集合而非一个 UI 开关判定：正式 issuer/source producer、个人当前认证、成熟 witness、免费 generic inbox、治理 WASM/原 metering、O 的全路径受保护 operational/closure hold、完整同域 allocator、L→T participant、连续权限历史、持久/重放、正式读面与当前消费者必须全部支持同一 profile/版本。缺任一项返回明确 blocked；不借旧 native 入口、任意 WASM 或人工写库绕过。该规则不改变已支持的普通运输接口。

认可、claim、allocation 和 Use 固定 beneficiary/Agent/control source。不能出售、出租、转让、拆分、叠加多个 receipt 抵一笔更大运输、由组织代持、换 actor 取源料或换 kind/route/目的地跨域携带。当前 owner 变更不会把旧认可自动交给继任者；后续个人取得资格须按新 program 的原规则显式审核。

伪造事实、重复申领、循环背书、付费换取和批量自动化的处置只能使用 program 预声明的事实类别、审核 authority、效果边界、复核/申诉和再获资格条件。未审核的怀疑只作受限待核信号，不扣资源/声誉、不自动撤权或封锁普通物流。确定的签名错误、payload 冲突和重复请求可按协议无效果拒绝，这不是对主体追加惩罚。

FI-002 pioneer priority 的专门可转让性与原重验规则不接入此 generic receipt；CR-003 声誉只在原局部解释内可读，MetaGrant points 不能直接充资格；GR-001 的设施 owner/service 规则保持原值。公开投影不返回他人原始签名、授权正文、精确预算或私有审核事实。

## 9. 质量与容量

<a id="opportunity-budget"></a>
### 9.1 有界工作与约束组合

以下是首版工程保护上限，不是游戏容量、资格阈值、服务时效或收费参数。原部署/profile 可以更小，不能在一个请求中静默调大；改变上限须按真实读写成本重新核验。实际容量 C、普通份额、q、窗口和 tariff 仍来自原 authority 的明确配置。

| 维度 | 首版工程界限与计量位置 | 超限与结束行为 |
| --- | --- | --- |
| 单个 typed 请求 | 编码后最多 16 KiB；所有字符串、数组、证明引用及 payload 计入 | 入口拒绝，未登记/未收费；不先无限解码 |
| 单个 source review | 最多 8 个原 job 事实，每个必须含完整开始/完成/settled 关联 | 缺失或超限停止审核，不取前几条认定符合 |
| 一个机会 | 一个 world/domain/瓶颈/kind/occurrence、一个不可拆分 q；Use 最多 2 hops，仍服从原全局 8-hop 上限 | 超出 profile 为 unsupported，不自动选新路径 |
| 一个完整 allocation round | 最多 32 个同域候选、16 个参与 route/resource 引用、64 个现存 reservation/job 权利；全部类别一起计数 | 超限整轮无新效果；原 operator 明确分域/升级前不截断集合，不丢既有义务 |
| 一次读/prepare | 最多 256 个 keyed records 与总计 256 KiB 解码值/证明材料；每次索引访问和签名/摘要输入都计入 | 超限 incomplete/blocked，不能靠分页拼成同一次无界执行 |
| 单个 program 活动记录 | 每域最多一个未闭合 program；最多 32 个 claimant；轮次边界列表最多 16 个，具有有限最晚结束点 | overflow 在原 authority 上冻结该轮全部新分配，不能拒绝第 33 个而放行前 32；域分区须事前声明 |
| 治理 WASM 与 host | 每个命令最多 1 次制度调用，无嵌套模型/WASM；manifest 的正 `max_gas/max_mem_bytes/max_call_rate` 及 output/effect/emit 上界全部固定并可读；source read 最多 256 次 keyed 访问 | 任何必要额外调用、缺预算或 output 越界均原子拒绝，不把异步外部执行 ACK 当提交 |
| 争用与网络恢复 | 一个操作一次提交尝试；stale 返回原原因，客户端显式重读；未知仅按原键查询/恢复 | 不在服务内无限 CAS 重试、自动重提或重复收费；状态查询受相同读预算 |

candidate 上限不是排队奖励。配置必须让声明作用域内的完整合法集合可处理；第 33 个符合范围的请求由原 inbox/索引留下 bounded overflow 事实，该轮不得从已存前 32 个产生新 L。不能通过不登记第 33 个或不断改窗口隐藏范围不完整。已存在的 L/T 不受查询失败影响，释放/完成走原有界 job/reservation key；关闭新 intake 时仍需保留原恢复、到期与完成能力，不能因满额丢 obligation。

首版工程 manifest 初值为 `max_gas=10000000`、`max_mem_bytes=16777216`、`max_call_rate=8`、`max_output_bytes=65536`、`max_effects=8`、`max_emits=8`；调用输入总上限 256 KiB 包括 typed request、旧 instance state、host 提供的全部记录与证明，不能只数最外层 16 KiB request。它们是可调整的工程保护初值，无吞吐/时延已通过声明；当前环境不能在这些界限内完成完整合法程序则拒绝激活，不裁掉必要源或闭合步骤。每个活跃 program 为既有 closure 保留至少一个原调度调用槽，新的可选操作不能耗尽它。

host 在进入 canonical execution 前做有界源读取，单次 source 获取最多等待 2000 ms，取消/失去读权立即停止读取并返回 unavailable；这个墙钟保护不写成 canonical 世界拒绝事实或延长世界期限。进入执行后的确定性 gas/memory/output 边界由 manifest 强制，不能用机器快慢改变分配。超时/取消若提交状态未知，仍走原 operation 查询；不得假称回滚。请求取消不停止已经接受的原 transit 或有资金保护的 closure。

最小正常路径需要两个合法 claims、三个 route、一个 occurrence、各自原 grant/ledger、完整源审核和成熟 witness。一次 settle 只分配瓶颈 q；Use 还需要 feeder 余量、原最大在途数及实际材料/电力。因此 receipt 不是全路径硬时效保证；原 [service-window](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-window) 所需完整 reservation 链缺失时不能宣称保证到货。

### 9.2 原计量、O 的真实预算与闭合

复用 [原资源 holds](checkpoint-power-service-fulfillment.design.md#service-resource-holds)和 [fee budgets/drain](checkpoint-power-service-fulfillment.design.md#service-fee-budgets) 的目标通用 adapter。O 对 exact program/instance、当前执行 manifest/artifact owner、原 schedule、命令/程序槽和两类费用向量明确接受；Publish/activate 的同一 accepted transaction 建立 O 自己 Data/Electricity 的 `operational_fee`、`closure_fee` hold，并支付实际激活计量。这里没有 price/collateral 或新的收费政策，也没有 route H。`available = gross − Σ active_hold.remaining`；reserve 不增加资产，consume 同减原 gross/hold 并入原 treasury，release 只恢复原可用量。O 的原 tariff、普通资源转移、其他模块费用、控制/回收等所有写入口都必须共享此保护，缺任一通路则 profile blocked。

真实模块 payer 仍由当前 manifest 的 `module_artifact_owners` 解析，首版要求它就是 O；不是由请求者任填 `payer=O`。费用授权和原 hold 给当前合法 execution/drain 明确的狭义消费范围，不能借它执行新程序、取 A/B 的资源或扩大金额。不要把未来原运输收入、未到账 tariff、其他 purpose hold 或新 owner 的资产当费用来源。

原 [fee arithmetic](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs) 定义 `units(0)=0`，正字节数为向上取整的 KiB。实际 `D = units(input_bytes) + units(output_bytes) + 2×effects + emits`，`E = 1 + effects + emits + is_some(new_state)`；以真实 host input/output、effect/emit 个数和 new_state 复算，并与 `ModuleRuntimeCharged` 及原 world treasury 对账。预检用同文件 `upper_bound` 与整个允许调用闭包，不按预估实际值。§9.1 的最大输入 256 KiB、输出 64 KiB、effects/emits 各 8 给每调用保守上界 `U_D=344, U_E=18`，只是已有公式的上界，实际不按 344/18 一律收费。

| 操作分类 | accepted 次数/预算槽 | 原费用及效果 |
| --- | --- | --- |
| preview、read、generic inbox pending/协议拒绝、原请求精确重读 | 只有有界协议 CPU/存储限额，无游戏费用槽 | 不跑 sandbox；无 Data/E debit、游戏预算、L 或 queue 优先级 |
| Publish/activate | 1 个 operational 槽 | 当前 O 明确授权；真实 module state、实际计量、费用 holds 同根提交 |
| 受权 source review / 有效申诉与纠正 | 每 beneficiary 的预声明完整程序分别列 `V_b` 个合法成功槽，首版 `V_b<=8` | phase/actor/source 分槽，不相互借用；合法 accepted 决定按实际 metering 收 O 一次，纯 deny 或无变化不收费 |
| CloseRound | 每个预声明 round 至多 1 个成功 operational 槽，`R<=16` | 新 first_eligible_round/等待或完整轮次状态是 accepted module state，要计 O 原费；完全无 new_state/effect/emit 的 deferred 为零新费，不占成功槽 |
| Use | 1 个 operational 槽 | 原模块费由 O 的准确 hold 支付；A/B 另付原 tariff。原物料/容量/费用/used 同根，失败不部分扣减 |
| 终止、撤销/到期释放、原已用结果归档与最后关闭 | 每 beneficiary 预留认可终态 1、claim 终态 1；另 allocation release 1、used-result closure 1、program final 1、最后费用 release 1，共 `2N+4` 个 closure 槽 | 只执行已签狭义处置，按真实成功调用收费；可同次合并而少用调用，不能借 closure 做新 review/round/Use |

程序表是激活输入的一部分，必须包含原预声明审核/申诉规则允许的全部成功变更；`V_b`、phase 和 actor 都不能以“日后再定”补齐。规则需要超过上限或未有明确终结路径时拒绝本 profile，不通过预算限制删掉已承诺申诉。每个 actor/phase 保留自己的槽，重复、无新事实和越权请求不消费另一人的机会。每次 accepted round 包含本轮所有有限候选的新等待决定及此前 generic 协议终态的必要制度归档，不能拆成任意多个收费小调用；中途新申请仍先进入免费 inbox，只有预声明轮次才形成新的制度等待状态。最终 closure 的每 beneficiary 槽处理该时点唯一尚未闭合状态，旧已归档请求不能再收费或重新占槽。

`operational_calls = 1 + Σ V_b + R + 1`；`closure_calls = 2N + 4`。按各自真正允许的 command 上界 checked 求和分别 reserve 两种资源，激活当前费用也包含在上述 1 次中；任何 overflow、资金或 hold coverage 不足全体拒绝。剩余 operational 不足只停止新的可选程序，不能占 closure；expiry/abort 必须通过保留预算与调度槽完成 L 释放和终态。最后关闭先计入准确 terminal fee，再同根释放未使用费用余量；不能先解锁后另起收费，也不调用一个绕 L2/计量的“免费 cleanup”。无效果重试不消耗保留资金，unknown 先查询原 charge/状态。

上述 adapter 当前尚未完整实现。原控制/identity 或普通 grant 的撤回不让已经明确授权的狭义旧费用结清消失，也不恢复新的认可/Use 权限。artifact owner、manifest 或费率变更必须由当前治理版本支持原 program 的 bounded drain、真实原 payer/source、足额闭合上界，或先合法结清；不能变成 P 后继续扣 O 的旧 hold，或静默让 P 支付。破坏闭合能力的切换拒绝，只停止新 intake 不能替代已有义务的结清。

### 9.3 可复核正常算例

此处数值只用于独立复算。A/B 是不同个人控制的 Agent，各有当前合法 grant 和自身 `steel_plate` 1 单位、足够电力；O 是原三条线路 owner 与接收方。A→H 和 B→H 两条 feeder 各 100 km、capacity 至少 1、tariff 1；共同 R 为 H→`agent:O`，100 km、capacity 2、tariff 2。一个已审核 program 的 occurrence 只提供 1 单位机会，普通保护份额为 1；三个 route 均 available。

路径为 `[feeder_A, R]` / `[feeder_B, R]`，两人 source 始终是各自 Agent ledger；中间 H 只承担原路径拓扑，不因此扣 H 账本。kind 没有额外 transport-loss profile 时原 [pure helpers](../../../crates/oasis7_world_state/src/world/pure_helpers.rs) 给 5 bps/km、100 km/tick；每次 amount 1、合计距离 200、tariff 3、duration 2 ticks，loss 为 `floor(1 × 200 × 5 / 10000) = 0`。该例不选择其他 tariff 或新的税。

A/B 同一首次有效 round 时，固定 canonical beneficiary tie 决定一名 winner，令其为 A。结算后 R 为 `L=1,T=0`；B 的失败没有隐藏费用或资源扣减。B 可以显式走原普通 Transfer 使用另一单位，此时 `L=1,T=1`。**必须在 A Use 前做正交反例**：独立前态令 B 源料至少 2、电力至少 6、feeder B capacity 至少 2，第一笔之后第二笔的其他原准入都仍成立；原 global inflight 只有 1、native reserved 只有 1，额外 B Transfer 仍必须因 R 的 `T+I+L=2` 拒绝，无第二笔扣料/费。这才识别原 writer 绕过 L，不能等 T=2 后用全局上限掩盖缺陷。当前未激活的 DispatchSupply 只核拒绝和覆盖清单，不伪装可达工业正例。

随后 A Use 将自己的 L 转成 T，得到 `L=0,T=2`，两条在途正好不超过现有 `MATERIAL_TRANSFER_MAX_INFLIGHT=2`；A 的 receipt-aware Use 不能在报价时被自己的 L 堵住，也不能把同单位加成第三份。此正常例 `I=0`；有真实独立工业 hold 的已支持环境必须用去重后的 `T+I+L` 核算。

每个原开始只扣其 source 1 与 requester 电力 3，记录唯一原 job。原完成各给 `agent:O` 材料 1、按三条 route 的 owner 汇总给 O 电力 3，原 settlement 中 governance tax 为 0，并释放对应两 hop 的原 reserved。两次完成后 O 材料合计增加 2、原 tariff 收入合计 6，A/B 各扣原 tariff 3，H 不变，R 恢复为无该批占用；该 occurrence 仍已消费，不能再次签发。O 的净 Electricity 为 `+6−Σ实际执行E`，Data 为 `−Σ实际执行D`，各费用进入原 treasury；不能把 tariff 收入 6 当作净余额结论。对照 cut 若在 program/review 后，另保留此前 setup 费用和剩余 hold，不能抹掉它们。

**正常闭包的共同预算。** 取 `N=2,R=1`，样例明确声明每人 initial review 与一次原规则允许的复核决定，共 `V_A=V_B=2`；这不是全世界统一申诉次数。operational 上界为 7 调用，closure 为 8，按上述最坏 manifest 分别保护 O 的 `2408 Data/126 Electricity` 与 `2752 Data/144 Electricity`，合计 `5160 Data/270 Electricity`，均来自发布前可用余额。正常成功实际需要 publish、两次初审、一次 closed round、一次 Use、一次可合并的终态/余费关闭，共 6 次制度调用；两个原运输开始/完成仍走原 consumer，不暗增第三个 module 调用链。未用复核/终止槽只释放原余额占用，不扣成收入；环境需要额外强制调用时必须先完整重新定额，不能沿用本例 6 次断言。

对应正常同切点读取闭包：world/manifest/profile/program/module 状态不超过 8 条；O 的当前 payer/权限、两资源余额、四个 fee holds 和完整占用索引不超过 12 条；每名候选的当前主体/授权/连续性、原 maturity 的三个完成因果、原交付归因、recognition/claim 和 source ledger/电力最多 32 条，共 64 条；三条 route/原域及完整范围证明最多 12 条；round/operation/receipt/transit 关联最多 12 条，总计不超过 108 条，留在 256-record 上限内。所有旧/new module state、证明和响应字节仍与请求一起计入 256 KiB input/64 KiB output，不能以条数够用推定字节已通过；运行前实际编码核验不满足则该候选 blocked。各审核分开按 exact keys 读取，原数据缺完整索引时不临时扫全世界。一次命令一项治理 WASM、同根有限 host 资源/原物流 participant，使用后两个原 due job 有独立有界 key；资金、读取、输出、gas/memory 与关闭预算共同成立后才准入。

这是根据固定源码的静态算例，不是执行结果。正式样例仍须证明原 owner 发布、真实成熟史和审核来源、合法 grant、当前状态、完整 allocator，以及这些数值在实际同候选运行中的原账本/receipt；缺项报告 blocked。

### 9.4 质量判据

| 场景 | 环境 / 输入 | 预期响应与指标来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 稀缺竞争 | 同候选、两合法主体、同一 occurrence；完整原域 | 一次 allocation/Use；无超容量、重试插队或第二费用；AC-RO-04 | [RO-04 方法](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-04) | 目标，未运行 |
| 原权限与失效恢复 | 撤销→恢复→旧 pending 结算；expiry/Use 竞争 | 旧 pending 终态；恢复显式新提；已 Use 原物流继续；AC-RO-05/06 | [RO-05](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-05)、[RO-06](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-06) | 目标，未运行 |
| 完整性与工程边界 | §9.1 各边界内及越界一项；ordinary/industrial 并发 | 无截断分配、局部发布或遗失旧义务；同输入/版本重放同结果 | [RO-04](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-04)、[RO-08](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-08) | 未提供吞吐、延迟或跨节点性能实测 |

## 10. 兼容、迁移与回滚

旧 route、原 Transfer payload、原 job/settlement、原费用和 source authority 保持其历史解释。新 profile 通过显式 capability/version 协商启用；普通客户端不会因未传 receipt 被拒绝正常运输。对启用域内所有 native/module/industrial writer 同时安装容量适配，不能只升级认可消费者，也不能把旧 arrival-order 路径暗改为新 round。原 policy 不兼容时继续普通 legacy 服务、拒绝新 program。

激活前从同一完整 generation 读取真实现存 jobs/reservations 并按其原身份建立可重建索引，核对无新 L、无补造历史认可及所有写入口覆盖。旧 transit 仍为其原占用；不从历史 points、普通 receipt、昵称或当前余额批量迁移资格。未知 schema/profile 或缺历史 artifact 拒绝本 profile 的执行和恢复，不以默认空集合绕过。

禁用先停止新 program/claims/settlement，保留原件查询、受保护费用/closure、未使用 L 的原终态释放及已启动原运输完成。不能回滚到不认识活动 L/once tombstone/use linkage/费用 holds 的二进制；只有在未决权益已按原规则终结、所有原 transit 已结清、费用按原 payer 结清/释放且新历史可继续读取时，才可移除运行适配。已经入账、扣费和完成事实不可用文档回滚或删 module state 撤销。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [AC-RO-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-01)；Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026 | [阶段与事实](#recognition-stage) | 成熟前拒绝；真实交付与明确审核才形成局部认可 | [RO-01](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-01)：固定同候选、实际 maturity/source producer；区分受控 fixture 与正式成熟样本 | 原成熟因果、program、原 transport 链、审核决定及反例 | 静态 source review 不证明真实成熟史 |
| [AC-RO-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-02) | [原 authority](#recognition-authority) | 权限/事实/范围分别可读，错误主体不能借 receipt 取他人货物 | [RO-02](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-02)：same cut 正反主体、缺源/缺权、受限读面 | 当前 owner/grant/source/proof、脱敏读面、拒绝原因 | 当前可读不代签原请求可执行 |
| [AC-RO-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-03) | [预览与待决](#opportunity-preview-pending) | 预览零效果、inbox 无排他权/隐藏费/顺位；accepted 制度决定与 O 原费用分清 | [RO-03](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-03)：前后同作用域账本/容量/队列/预算，含真实 module metering 路径 | inbox 零效果、真实 accepted O fee、pending 与普通服务对账 | mock 免费 handler 不证明正式登记免费，不以未得 L 免除真实状态迁移计量 |
| [AC-RO-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-04) | [一次结算与使用](#opportunity-once-settlement) | 两合法 contender、完整域、原容量/receipt/Use 一致且至多一次 | [RO-04](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-04)：§9.3 normal、普通剩余容量、并发/retry/replay、额度越界；同候选真实物流 participant | round 全集/排序、唯一 winner、L→T、原 start/completion、逐账本与费用 | 单独 receipt DTO、无权败方或局部模块锁不证明竞争 |
| [AC-RO-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-05) | [当前重验](#opportunity-current-revalidation) | continuous permission、CAS 漂移和恢复后显式新提 | [RO-05](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-05)：submit→revoke→restore→settle、过期与来源变化、缺历史区间 | 原 invalidation 历史、旧终态、新请求来源及零旧效果 | 仅当前 authorized=true 不能通过 |
| [AC-RO-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-06) | [状态与恢复](#opportunity-recovery) | before-use 释放、after-use 保留原物流、unknown 恢复及 once tombstone | [RO-06](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-06)：提交各故障点、重启/失效/申诉、普通独立路线 | 完整 generation、原操作查询、容量守恒、未决/终态对账 | 本地模拟不代签正式持久/跨节点 finality |
| [AC-RO-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-07) | [反滥用与隔离](#recognition-abuse-review) | 不转让/拆分/叠加/代持；有声明审核；FI/CR/GR 边界不变 | [RO-07](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-07)：换主体/route/quantity、重复 source、未审核怀疑及合法复核 | 预声明政策、受限事实与处置、拒绝与申诉链、baseline 对照 | 未实现专门 authority 的 transfer 不在此补签 |
| [AC-RO-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-08) | [消费与证据](#recognition-consumers) | 同候选真实端到端、跨角色对账及玩家理解 | [RO-08](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-08)：固定源码/配置/世界、正式 producer、Viewer/API/Agent 原操作与 blockchain 结果、独立 QA | 各层版本/入口/结果、fresh mature-world sample、残余未证范围 | 文档/CI/fixture 和旧 sample 不证明 MW-004 闭环通过 |

## 12. 决策、长期风险与未决问题

| 决策或风险 | 选择与理由 | owner / 解除或复核条件 |
| --- | --- | --- |
| 真实有限机会 consumer | 选原 multi-hop logistics：两主体保持自己的源料/grant，有限中间容量可一次分配并真实运输；不借 owner-only micro_depot 或 factory slot 伪造共享权 | runtime/gameplay；新增 consumer 时重新核原准入与可消费结果 |
| 无隐藏供给池 | 发布与 pending 不占 H；只有原同域轮次结算产生 L；ordinary floor 同时考虑普通已用量 | 原 capacity authority；启用前验证所有 writer、分类和原 policy 同步 |
| 公平策略 | 有限完整 round；首次有效轮次、canonical beneficiary 稳定 tie；不以网络先到、重试或可变 ID 排序 | 原 allocator/profile owner；行为已在本设计确定，当前缺实现适配不等于等待另选算法 |
| 协议登记与费用 | 免费 generic inbox 只做协议记录；WASM 制度状态保留原 metering，O 显式承担并保护 operational/closure，真实 Use 另沿原 requester tariff/grant | runtime/World Service；缺全路径保护与真实 fee/closure 对账则 blocked，不改旧 WASM 费率或让 native 代制度执行 |
| 原 producer 与持续权限 | 正式 issuer/source/maturity、失效历史及业务去重都有明确来源；不能以 UI 确认或当前布尔值补证 | 原 identity/gameplay/source owner；完成 adapter 与同候选证明后才可启用 |
| once 与物理容量分离 | receipt 释放后原物理槽可普通复用；同 occurrence 永久保留一次结算事实 | storage/runtime；保留策略、schema 升级和恢复都须维持 tombstone 与原 job 链 |
| 交付边界 | 本文覆盖上述首版的目标技术义务；generic 其他供给类型须各有原 authority/Use adapter，当前实现及真实闭环仍未验证 | gameplay/runtime/QA；按 §11 同候选组合证据判断，不由 active 文档或合并状态推导通过 |
