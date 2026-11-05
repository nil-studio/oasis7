# 同一生产目标的修复、重建与转向比较系统设计

- 生命周期：`active`；Owner role：`runtime_engineer`；协作：`gameplay_designer`、`agent_engineer`、`viewer_engineer`、`qa_engineer`；Last reviewed：2026-11-04。
- 固定审读基线：`567c4c55a2f160574fdcf0902ccb068646210c7d`。本文交付目标设计，未执行 Cargo、游戏、模型或正式客户端；源码事实、静态复算、目标接口与实测结果分别记证。
- 上游：[REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)、[生产恢复比较政策](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#production-recovery-comparison)；验证：[专用手册](../../testing/manual/production-recovery-comparison-acceptance.manual.md)。

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182。只承接成熟世界同一生产目标受阻后的有限比较，MW-002 的其他扰动与 SC-25 的完整能力生命周期继续保持原覆盖状态。

## 1. 问题、目标与非目标

现有执行口可以分别转移材料、建设和排产，单个 native quote 能检查一次当前动作；它们没有证明“为了原目标，补旧厂还是另建厂更合适”。旧目标标签和定性卡片也不能证明合法资源来源、后续步骤或实际恢复。目标是输出同一已接受目标、同一世界来源切点下的有限候选，说明每步成立条件、增量投入、旧价值与风险，让玩家选择原动作，并在原结果确认后重新判断。

初版 profile 为 `native_production_recovery_v1`，仅接 `production_only` 目标、原生 canonical factory/recipe，以及已证明有限依赖的准备和完成规则。交付型目标没有实际 delivery authority 适配时返回 `unsupported_goal_mode`；不得删去其接收方、期限或 settlement 条件而降级成生产目标。本文不增加世界修复 Action、恢复资源、费用、预约、自动规划执行器或新长期服务，不实现任意 WASM/模型模拟、全世界寻路、SC-25 升级/改造、旧 WIP 自动迁移和退款。

`repair / rebuild / pivot` 是候选分类。`MaintainFactory` 的存在不证明所有生产 blocker 可由维护解除；改变产物的 pivot 也不当然满足原目标。成功标准是玩家读懂至少两条有真实差异且满足原目标的可行条件链，实际选择后原事件和账本证明新增产物；比较生成本身不是恢复。

## 2. 上游约束与相关角色

Gameplay 拥有方案含义、目标适配和玩家结果。原 goal authority 拥有目标创建、接受、版本及激活；工业、材料、能源、站点与 delegation authority 拥有实际操作规则。Runtime 负责有限状态求值，World Service 负责同切点读取、当前读权与证明等级，客户端负责展示和原 operation 恢复。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [AC-PRC-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-01)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | 绑定已接受原目标、真实 blocker 与完整来源切点 | [目标与来源](#production-recovery-goal) | 原 goal authority、World 及身份 authority | 文本解析、现余额和本次确认不能补造原接受准则 |
| [AC-PRC-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-02) | 有限模板映射到实际动作，区分首步和条件后继 | [候选](#production-recovery-candidates) | 原 action admission、站点/工厂/配方 profile | 未支持维护、迁移和任意搜索不能凑可行路线 |
| [AC-PRC-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-03) | 逐步核时间、逐账本增量成本、沉没/占用及保留价值 | [成本](#production-recovery-costs) | 原材料、电力、完成/处置 authority | 不同单位抵销、提前使用未来回收、复制稳定进度 |
| [AC-PRC-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-04) | 原目标适配、独立路径和取舍推荐，显式改目标 | [选择](#production-recovery-decision) | Gameplay、原目标/需求 authority | 有限未找到不证明必须赞助或全局最优 |
| [AC-PRC-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-05) | 只读比较、当前权限、漂移及原提交未知恢复 | [执行与恢复](#production-recovery-execution) | 原任务、GameClient、World Service 恢复 | 无跨动作原子提交或比较器写路径 |
| [AC-PRC-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-06) | 完整有界读取、求值、等待停止和不完整范围 | [预算](#production-recovery-budget) | 有界原件 adapter、宿主资源计量 | 分页拼完整、无限未来模拟、自动反复等待 |
| [AC-PRC-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-07) | 三入口同义消费，独立判别与真实恢复证据 | [消费者](#production-recovery-consumers) | Viewer/API/Agent、QA | 静态卡片、fixture 或单个 quote 不代签正式实玩 |

## 3. 当前状态、目标状态与差距

| 对象 | 固定基线的实际行为 | 本设计需要的目标/缺口 |
| --- | --- | --- |
| 原目标 | [GoalContentV1](../../../crates/oasis7_agent_api/src/game/goal.rs) 只有完整 `goal_text / soft_constraints`，上限 4 KiB；private prepare/publish 与 TaskControl 绑定 revision/digest/activation | 现有文本不是机器接受准则；需 §6.1 的原 goal producer 版本能力 |
| recovery 投影 | [WorldState query](../../../crates/oasis7_world_state/src/gameplay_capability.rs) 对 repair/rebuild/pivot 返回 unknown；[Runtime query](../../../crates/oasis7_runtime/src/world_rules.rs) 只为 `current_industrial_capability` 加一次 `current_recipe_run` quote | 该 goal 常量不代表任意 active goal；尚无多步比较服务 |
| Viewer 消费 | [lane](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot_lane.rs) 保留 unverified；[feedback](../../../crates/oasis7_viewer/software_safe_src/viewer_feedback_module.js) 将旧 comparison 数组置空，展示有来源 assessment | 旧布尔值、成本文案、面板 fixture 不证明比较已交付 |
| 转移 | [TransferMaterial](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_core.rs) 区分同 tick 直传与在途；[reducer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transfer.rs) 预检目的账本、扣源并按 transfer_id 去重 | v1 只采用明确的直接零距离转移；非零距离/路径仍需原运输能力的完整适配 |
| 建设 | [native Build](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy_factory.rs) 检查 exact factory profile、站点/位置/anchor、建设电力及 builder Agent 建材；pending/active/retired ID 不能重用 | 站点/profile 的[设置接口](../../../crates/oasis7_runtime/src/world/factory_authority.rs)属于可信 authority，不是比较器可代办的玩家动作 |
| 排产 | [ScheduleRecipe](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs) 检查原 owner、slots、stage/tags、产品、实际 site 输入、电力；新提交不回退世界材料账 | 基线未用 durability 或 throughput_bps 改写该 direct native 路径的时长/准入；不可从低耐久推导停产修复 |
| 维护/回收/暂停 | 同文件：Maintain 按 hardware_part 恢复耐久；Recycle 拒绝 active jobs、回收到原 output ledger；Pause 只允许无 pending job 且 Idle。没有通用 DestroyFactory/RepairFactory 动作 | [native grant](../../../crates/oasis7_runtime/src/world/agent_delegation.rs) 和正式 CLI 未提供这三者的完整授权/提交选择；v1 不自动编入可执行链 |
| 原 quote | [recipe](../../../crates/oasis7_runtime/src/world/builtin_recipe_quote.rs)、[factory](../../../crates/oasis7_runtime/src/world/builtin_factory_quote.rs)、[resource basis](../../../crates/oasis7_runtime/src/world/native_resource_basis.rs) 使用私有 World clone 复用原校验，无 World.step | clone、delegation JSON 全量解码、全局 map 扫描并非有界多步 adapter；不能按“调用一次 quote”隐藏成本 |
| 正式入口 | [factory host](../../../crates/oasis7_chain_runtime/src/host/world_service_private_factory_quote.rs) 与 [recipe host](../../../crates/oasis7_chain_runtime/src/host/world_service_private_recipe_quote.rs) 绑定 canonical pin/task/context 并在发送前重验；[GameClient build](../../../crates/oasis7_game_client/src/device_client_build.rs) 优先恢复原请求，再报价/提交 | 可复用签名、身份和恢复边界；新比较、typed goal、有界来源及三入口都尚未接通 |
| Build grant 来源 | native grant 内部允许 BuildFactory，但[正式 owner install](../../../crates/oasis7_runtime/src/world/owner_delegation_control.rs)的 allowlist 尚不含 BuildFactory | build客户端/quote能消费已有grant，不证明正式用户能取得它；fixture预装与正式grant来源分开，后者是两路实玩必需前置 |
| 完成 | [economy](../../../crates/oasis7_runtime/src/world/economy.rs) 在 due 时创建 FactoryBuilt/RecipeCompleted；正式模块路径还经过产品验证与[工业终端 guard](../../../crates/oasis7_runtime/src/world/industrial_completion.rs) | 只预览 ready_at 不保证成功；新服务不得运行任意模块、跳过终端义务或代造完成事件 |

[factory catalog](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs) 的两种固定值是 `factory_kind`，函数随后把 `spec.factory_id` 设为调用者选择的 exact ID；正式 GameClient 接受该分离。第二座同类工厂可以用新 ID，但该 ID 的实际 profile、建设电力、站点和当前 grant 必须齐全。fixture 安装配置只证明构造前态的方法，不证明正式环境已经具备这些来源。

## 4. 边界与结构

| 组件 | 负责的输入与输出 | 边界 |
| --- | --- | --- |
| 原 goal prepare/publish producer | 私有目标正文、typed 接受准则、版本、原激活证明 | 唯一接受或修改目标的入口；比较服务没有写权 |
| 已鉴权服务宿主 | current owner/task、exact source cut、服务证明与返回前重验 | 区分 service assertion 与独立正式读证明；沿原世界读取/恢复边界 |
| `RecoverySourceAdapterV1`（目标） | 从 canonical pin 做有界 keyed/range 读取，给出原件、完整相关集合/缺口 | 不先 full-clone 再裁剪，不向读者开放整个私有上下文/其他主体原件 |
| `RecoveryEvaluatorV1`（目标） | 有限模板、版本化原规则、独立候选 overlay、成本与条件 | 共享有界的原 admission/reducer 纯逻辑；不提交、安装 grant、World.step 或调用模型/WASM |
| Viewer/API/Agent | 同一版本化只读 envelope、原动作选择、原 operation 恢复 | 不从文案猜可用性、不自动执行整链；读取能力不授予资源所有权 |

引用[工业 work graph](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-transaction-recovery)的原 root、lot、reservation、sink 和 disposition；本文不重新定义这些权利。既有 accepted-unstarted/WIP/在途/buffer/terminal-pending 若关联所选资源或设施，必须读到其真实占用/处置。v1 不会处置它们；不能证明互不影响就将相关路线标为 unknown 或 unsupported，而不是把这些工作省略为零。

## 5. 关键运行流程

1. 消费者提交已激活目标引用、原 blocker 引用、最多三个明确候选选择及读者身份。正文/接受准则必须从原 goal authority 读取；请求中的重复字段只用于一致性检查。
2. 宿主先验证当前 owner/task/读权与请求大小，接纳一个有限工作槽，捕获 immutable exact source cut，包括 world/branch/reorg、commit、tick 与维护/完成阶段；取得目标、现态 blocker 和所有依赖的同切点完整覆盖。
3. adapter 按 §6.2 固定模板展开候选。当前原排产失败只证明现态失败；不能直接把这份失败 quote 用作“补料后的排产”或“新厂完成后的排产”的结论。
4. evaluator 从同一切点为每条候选建立独立 sparse overlay；逐步检查原规则、更新假设账本/容量/预算与本候选工作，保留等待和后续重验条件。任何未读取依赖、未知模块、未支持处置或溢出停止该路线的完整可行性断言。
5. 仅对来源完整、原目标适配且条件可行的路线生成取舍；整个相关集合不完整时响应标 incomplete，不能给默认最优、零成本或必须外部依赖结论。
6. 发送前重验当前 owner/task/goal activation、读权、source generation 和规则版本；漂移处理见 §7。客户端选择下一步时沿原 GameClient 流程，已有未知 operation 先恢复；每步结算后重新读取，再决定后继。

## 6. 接口与数据合同

下列类型/字段是目标逻辑接口。新增 wire 版本必须由原 producer/client capability 协商；没有相应能力的旧接口返回未提供，不把缺字段解释成 false、零或已验证。

<a id="production-recovery-goal"></a>
### 6.1 原目标接受、主体与 exact source cut

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182。

**目标接受的 producer。** 原私有 goal prepare/publish 链增加明确的 typed 版本能力 `production_acceptance_v1`。原 owner 在创建目标或原入口显式澄清/改写时提交结构化准则，原 authority 验证 current owner、task fence、expected revision 与完整内容，再生成 prepared 引用；只有原 publish/activation 成功才可供比较使用。准备成功不表示已激活。比较服务不能接受新的谓词、写 sidecar、改 goal，或用本次 owner 复选框补签旧文本。

`ProductionAcceptanceV1` 至少固定：`mode=production_only`、受益 owner 与执行 Agent、material canonical ID/单位、正 `additional_quantity`、允许 factory/site/output-ledger 的有限集合、原用途、原来存在的 deadline（没有则 null）、计数起点策略 `from_this_revision_activation`，以及 `completion_profile`。策略表示从本revision原激活时起算；实际activation commit来自随后原发布证明，不作为prepare内容中尚未产生的自引用字段。v1只接一个material、正i64最小单位量、最多两个allowed site/output-ledger对；可选exact factory集合最多两个，缺省仅表示该已接受site范围内符合owner/能力的工厂，并非全世界任意账本。首版完成profile为“激活后、匹配原动作因果的新增RecipeCompleted入账”；不是余额达到阈值。硬条件全部由typed内容表达；自由说明/soft_constraints不可添加另一个隐藏硬条件，不能机器化的原硬约束使目标不适用该profile。

**版本与摘要。** 目标版本 2 使用单独的严格 tagged 内容：`schema_revision=2, display_content, production_acceptance`，完整编码仍限4 KiB；以原 canonical CBOR/hash 实现计算新域 `game_goal_version/v2`，载荷明确包含 task_id、goal_revision 与完整 v2 内容。原 producer 为 prepare/publish/context codec 增加显式 v2 capability，原 goal_ref/activation 绑定该版本与摘要，private store 保留原完整内容。V1 的 `game_goal_version/v1`、codec 与已有摘要不变；`deny_unknown_fields` 的 V1 payload 不能直接塞新字段。不能让 v2 字节走 v1 路由、签名域或摘要解释，也不能把同 revision 的两个不同版本内容视为同一目标。

旧 V1 只有文本时返回 `goal_unresolved`，只给一次明确的原目标澄清入口。澄清使用原 expected-revision 机制生成显式新 revision；不得复用旧 digest、原地扩大地点/产物/期限或静默复制满足量。真正修改目标仍由原权限接受/激活；比较不因支持 v2 而迁移旧目标。正常例预先有原 authority 激活的 typed 目标，这是本次尚待实现的前置，不是 fixture 可以代签的现有正式 API。

**请求与输出。** `RecoveryComparisonRequestV1` 绑定 schema/profile、world/task fence、已激活 goal ref/revision/digest、原 blocker/action ref、当前客户端 draft 身份，以及 `repair(F0)`、`rebuild(S1,F1,kind)`、可选 `pivot(F0,recipe)` 的 exact 选择。`RecoveryComparisonV1` 返回同一身份、source cut/证明等级、有限覆盖范围、候选状态、逐步条件/成本/旧工作、推荐依据/改变条件、缺口和原入口下一步。comparison_id 只标本次读结果，不是世界 request、grant 或可提交票据；同一在途/client draft 中 id 与输入摘要冲突应拒绝，重启不承诺记住所有历史读 id。

**来源闭包。** 至少读取原 goal/task/activation、当前 owner/read audience、原 blocker 与当前 factory、exact recipe/spec/profile、所有实际 consume/output/build ledgers、builder 电力、被选站点/位置/anchor/建设电力、原 delegation grant/source/period/remaining、相关 pending/retired/industrial 占用与输出容量、维护 sink、产品/模块解析版本、原规则及相应完成条件。recipe 的 material-market 解释还读取所需 kind 的 world/local 余额、transport class 和 gameplay tax；它不代表实际购料或 charge。与候选无关的对象不能为了“可能有用”扩张读取。

必须有被选范围的同切点完整集合或不存在证明：例如没有活跃 job、没有第二个同 scope grant、未 retired、没有关联旧义务。空结果来自受众过滤、索引落后或枚举被截断时均为 unknown。exact cut 必须由原 source adapter 支持；seed clone、按 tick 拼不同世界、journal 重编号、先全量过滤再称有界都不成立。正式环境复用[原读证明等级](../../../crates/oasis7_game_client/src/verified_world_view.rs)；尚无最小投影/覆盖证明时 `source_unavailable`，不能降级成本地 World 冒充正式来源。

**具体有界读取接口。** `RecoveryReadSetV1` 返回 `cut/schema/native_rule_version`、逐记录 `key/revision/value/proof_ref` 和逐查询 `range/predicate/count/coverage_generation/complete`；所有内容受当前 reader scope 约束。最小 key/range 与输出如下，不能用“已加载 World”跳过计量。

| 原 authority 的查询键/索引 | 受限输出与完整性判据 | 原 writer 与就绪条件 |
| --- | --- | --- |
| `Task(task_id)`、`Goal(task_id,revision)`、`Context(decision_request_id)` | 固定身份/activation/version、完整有限 goal 与受权 context binding；私有接受范围的每个 entry 计数 | 原 task/goal/context producer 随原持久记录维护；旧V1或缺activation不就绪 |
| `Material(ledger,kind)`、`Resource(agent,kind)` | 一个 owner/ledger/kind/amount/单位元组或 exact absence；不返回整个 ledger | 原材料/资源 reducer 随原 commit 更新 keyed 值；每个实际 kind 独立计对象 |
| `Factory(id)`、`Site(id)`、`Location(agent)`、`Anchor(id)`、各 `Profile(id)` | 被选对象的有界字段、revision、状态及有效期；canonical 固定 stack/tag 也计字节与访问 | 原 factory/authority/profile writer；F1 active/pending/retired 三种存在性各自核验 |
| `PendingRecipeByFactory`、`PendingBuildByFactory`、`IndustrialRootByFactory`、`ManufacturingByFactory`、`ObligationByLedgerKind` | 原完整有序集合、range边界/count及逐entry记录；accepted/WIP/在途/buffer/terminal占用不靠净余额倒推；制造Producing订单及其eligibility影响原折旧load | 各原 job/root/hold/transfer/manufacturing reducer 在原提交中维护 reverse index，释放/完成同步删或改状态；Assembler候选缺制造闭包则unknown |
| `GrantByAgentObject`、`Spent(source,object,period)`、`OwnerControl(agent)` | 完整agent+object候选集合，再按exact action kind校验；同F1的Build/Schedule共用一个完整range，不漏过期/撤回/竞争项；read根不代动作grant | 原 delegation install/revoke/spend/owner变更 writer 维护 typed 索引；不能query时全量解码旧cognition JSON |
| `ProductModuleByKind`、`NativeRuleVersion`、必要 `WorldMaterial(kind)` | 原 module selector 的完整结果及active代码身份/absence，实际sink/stage/tax/market字段 | 原模块注册/激活、profile与规则 writer；未知验证器不运行，返回不支持 |
| `GoalOperations(task,revision)` → `DecisionCommit(request)` → `JobCompletion(job)` | 受权原context到原commit/action/job及completion角色的因果链和完整范围 | 原私有任务执行历史、World commit和recipe reducer维护既有记录的派生连接；不把goal或其摘要写进公开World事件 |

这些是原 writer 的可重建索引，不是第二份权利或会计 authority。索引条目包含原 canonical key/commit，coverage generation 只有完整追到该 pin 的原提交后才发布；异步更新可以落后，但落后就返回 incomplete，不能作 absence。replay/branch rollback 使旧 generation 失效，按原历史重建并重新核验后才可读。查询不现场全库回填；新增索引与原写入必须在原事务内更新，或在响应前用完成水位证明到达同一 cut，不声称跨库原子提交。正式证明验证仍到原 root/commit；可信宿主的 service assertion 按原等级标识，不把派生索引 hash 当独立证明。

**新增满足量的因果接缝。** 选中下一步后，原 GameClient/cognition 捕获的 task fence/active goal ref，绑定原 decision_request_id 与 exact action digest；原 World commit marker 再把该 decision 连接到 action/job。目标 reader 只沿这条受权原链匹配该 activation 后的 RecipeStarted/RecipeCompleted 及 output ledger，按原 job+completion-role 去重。每次重比用 `GoalOperations` 的完整范围确认已满足量；只找到一个完成事件而范围未完整，不能断言总余量。缺少原连接为 attribution_unknown，不能由 comparison_id、当前owner或余额补造。完成计数仍是派生读，不增加世界goal完成receipt；后续真实链的记录也占 §9 同一预算，过大保持不完整。

<a id="production-recovery-candidates"></a>
### 6.2 有限候选与逐步求值

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182。

| 模板 | 有限展开与条件 | v1 明确限制 |
| --- | --- | --- |
| repair | 同一 F0：对原有效 consume 清单逐 kind 算本地缺口；从同一执行 Agent ledger 做明确零距离 direct TransferMaterial；随后原 canonical ScheduleRecipe；等原完成边界 | 最多四个需补 kind；当前 grant 必须匹配每个 from→to:kind；不能凭世界库存补账，不处理未知能源来源/维修接口 |
| rebuild | 一个已选 S1、新 F1 和 canonical kind：原 BuildFactory；等 FactoryBuilt；所需 direct 入厂转移；原 canonical ScheduleRecipe；等完成 | exact profile/站点/位置/建设 power/current grant 齐全；保留 F0 是不处置旧厂，不是隐式 Pause/Recycle；active/pending/retired ID 不重用 |
| pivot | 一个显式选择的现厂/替代 canonical recipe；先校验原 goal-fit，再按同样的有限补料/排产规则求值 | 产物、用途、目的地不满足原准则则 `needs_goal_change`；不能为凑第三路扫描全部市场/配方或声称免费转换旧 WIP |

每类最多一个候选，稳定排序按类别和 exact 选择，不靠文本权重产生额外分支。只枚举请求明确的范围，响应列出 `evaluated_scope`；未查其他站点/方案不等于它们不存在。模板变动是 profile 版本变动，不允许模型现场追加命令。

**共享原规则的实现接缝。** 从现有 action_to_event、材料 preflight、factory/recipe reducer 与完成 guard 中提取仅访问 `BoundedWorldRead` 的纯 admission/effect 函数；真实执行口与 evaluator 调用相同版本。读接口只返回已 pin 且已计量的记录，任何新 key/range 请求都经过同一预算，缺读返回 `dependency_missing`，不能默认为空。实现必须与原 native quote 的拒绝、effect 和整数舍入进行差分核对；直接复制一份“差不多”的公式不是共用规则。

每个候选按顺序：校验动作身份和当前/假设前态 → 原 admission → 在本候选 overlay 应用预计 sink、grant spend、pending/capacity → 在明确的相对 tick 到达后只求该候选 job 的受支持 completion → 校验后继。overlay 的事件是带 `hypothetical` 的内部 effect，不给真实 action_id、receipt、root 或 world commit；不得发布/回放进世界。每条分支从原 cut 单独开始，互不扣对方材料，也不共享对方未来收益。

不能使用 World.clone+step 处理虚构未来，因为它还会推进无关 job、生命周期和授权。折旧、已有工作、同 tick action-before-due 次序及所选完成规则若影响本分支，必须在有限依赖内明确计算；不能以跳过不利变化来证明可行。未知模块、未知产品验证或无法覆盖的 industrial terminal guard 只给条件缺口，不执行任意代码。v1 正常例选择有完整“无模块绑定、无工业 root 关联”的来源；若实际环境有绑定，需已审读且有界的纯 adapter，否则 unknown。

`route_status` 分为 `conditional_feasible / blocked / unknown / unsupported / needs_goal_change`，另给 `first_step_admissible`、`goal_fit`、后续 predicates 和覆盖。conditional_feasible 表示本切点、所列环境保持时整条有限链通过共享规则；不表示后继现在可提交或未来 grant 不会撤销。完整判断需要首步及后续全部关键来源，不能以一条可做的转移包装未知终局。已知当前不足且补齐步骤通过可以形成 conditional_feasible，而不是被当前 Schedule 的缺料拒绝一票否决。

<a id="production-recovery-costs"></a>
### 6.3 增量成本、保留价值与可达正常例

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182。

每个 step 的成本以 `(owner, ledger/resource account, kind, unit)` 分账；相同 key 汇总后检验峰值需求，不能只算最终净差。材料守恒为每步 `before + 已完成入账 − 实际/假设 sink = after`；在途、未结算产物、可能回收和另一分支库存不提前计入。电力从实际 payer 扣，delegation period remaining 是另一项约束，不能和余额相加。代扣/异地权限、同 material 不同 ledger 都不能互换。

直接 recipe 消费为原 canonical consume 加实际 product maintenance_sink；同 kind 聚合并校验数量域、溢出与目的容量。base duration 按原 plan，当前 native 路径是 `max(duration_ticks,1)+scarcity_delay`；site-bound consume 使该路径原 scarcity helper 返回 0，不能额外乘 throughput 或把 base_power_draw 当建设电力。参数来源分别是 [recipe catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)、[duration](../../../crates/oasis7_agent_api/src/world_quote/duration.rs)、[maintenance/supply helper](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy_support.rs) 与实际 construction power profile。

沉没、占用、可保留和可回收分列。历史建材支出只有匹配原 construction 事实才标 actual；spec 的 build_cost 不自动证明历史已花金额。当前设施保留不折成一笔可花材料；未知旧成本写 unknown。原 Maintain 每份 hardware_part 修复 25,000 ppm、上限 1,000,000，且先核请求 parts 的库存、再按实际恢复量向上取整消费。Recycle 原回收为逐 kind `floor(build_cost × 700000 × durability_ppm / 10^12)`，进原 output ledger；busy、目的加法容量及 retired 身份仍受原规则限制。这里只解释原语义，v1 不提供这两个未接通正式入口的操作建议。

**共同正常前态。** 这是可用原规则构造、待正式链验证的设计 fixture，不是已跑 PASS。已激活目标 G：owner A 的执行 Agent A 自 activation 后新增生产 `iron_ingot=3`，原准则就允许 A 的 `site:S0 / site:S1` 接收；无外部交付、额外品质/期限条件，当前已满足量为 0。旧余额或 activation 前 receipt 不计数。F0 是 S0 上 A 所有 canonical Smelter MK1，无 pending/WIP/在途/root 义务；其当前排产 quote 有真实 `InsufficientMaterial(iron_ore,requested=4,available=0)`。所有适用 stage/tag/product gate 已通过，iron/slag maintenance_sink 为空，输出加法可容纳，相关模块绑定不存在。[制造资格](../../../crates/oasis7_world_state/src/world_rules/industrial.rs)要求exact canonical Assembler，故此Smelter样例可按原规则排除额外制造load；一般Assembler候选仍须读取制造订单，不能只数pending_recipe_jobs。两候选从同一after-tick cut t0独立开始。

完整成熟世界前态还要有 F0 的原合法建设与既有成功批次，在原 cut 上达到 canonical stable-line 的证明；现在受阻可以使 current stable=false，不抹掉已经有过持续能力的历史。读取先前已验证的 factory canonical snapshot及其原cut证明，不能用当前标签反推。旧产物可保留 H，表中0是本次新增基线，不要求清空世界；转入、其他goal和旧批次产物不计G。若只直接安装F0或接受准则，分别只证明比较fixture/目标设计fixture，不证明真实成熟世界或正式goal producer。正常闭包含原失败P0的context/commit关联与GoalOperations范围，证明只有该未产出的失败尝试、G尚无匹配完成。

| 位置/来源 | t0 可用量或条件 | 参数依据 |
| --- | --- | --- |
| `agent:A` 材料账 | iron_ore 4、carbon_fuel 1、structural_frame 12、heat_coil 4、refractory_brick 6 | 配方/建厂 canonical 原数量；这是预先拥有的存量，不在恢复过程中注入 |
| A Electricity | 18 | 样例 actual 建设 profile 的 10 加 canonical 配方的 8；10 来自[原生命周期 fixture](../../../crates/oasis7_runtime/src/tests/economy_factory_lifecycle.rs)的配置值，不是全局建设价格 |
| `site:S0` | iron_ore 0、carbon_fuel 1；iron_ingot/slag 新增基线均 0 | F0.input_ledger/output_ledger 均为此 site；世界或其他账本的 ore 不可替代 |
| `site:S1` | 所列材料为 0；与 A 当前合法位置相同的另一已授权 site | actual active site/anchor/location；非比较器创建，避免暗含 Move/迁居能力 |
| 新 F1 | 不 active/pending/retired；kind=Smelter MK1；exact profile 与建设 power=10 已登记 | 建材 12 frame/4 coil/6 brick、build_time=1、slots=2 来自 factory catalog |
| 原 grant | F0 Schedule、F1 Build+Schedule、A→S0 ore、A→S1 ore/fuel 各object范围完整、对应action资格唯一、有效且期限覆盖所示阶段 | F1同period剩余额度至少18；F0至少8；直传费0但grant仍需合法正limit。当前正式owner install缺Build scope，预装此grant只算fixture；正式来源未补时B不可标为已接通 |

两条链都选 canonical `recipe.smelter.iron_ingot` 一批：4 iron_ore + 1 carbon_fuel → 3 iron_ingot + 1 slag，Electricity 8，base duration 1。零距离无 route/path、auto_reroute=false 的直接转移在提交事件中完成，不占在途槽、无运输损耗/电费；该前提由原 route resolver 验证，不给任意远程站点假填零距离。

| 路线与逐步前态 | 原动作/边界 | 本步结果与资源账 |
| --- | --- | --- |
| repair R1：A ore4，S0 ore0 | TransferMaterial A→S0 iron_ore4，直传 | A ore0；S0 ore4/fuel1；电仍18；只有物料到位，尚未生产 |
| repair R2：已见 R1 原结果、F0 空闲 slot、site 齐料 | ScheduleRecipe F0/iron/1 | S0 ore0/fuel0；A 电18→10；启动1个 job，尚无 ingot/slag 入账 |
| repair R3：该 job 到 ready_at，终端/输出前提仍成立 | 原 RecipeCompleted | S0 新增 ingot3/slag1；释放该 slot；G 新增满足3。A fuel1和全部新建材保留，旧厂保留 |
| rebuild B1：A 建材齐、S1/F1/权限合法 | BuildFactory F1/S1/canonical kind | A frame/coil/brick 均→0；电18→8；pending build，未得到可排产工厂 |
| rebuild B2：A ore4，S1 ore0 | TransferMaterial A→S1 iron_ore4 | A ore0，S1 ore4；独立等原 FactoryBuilt，不能用 build accepted 代替 |
| rebuild B3：A fuel1，S1 fuel0 | TransferMaterial A→S1 carbon_fuel1 | A fuel0，S1 ore4/fuel1；S0 原 fuel1 和 F0 均保留 |
| rebuild B4：已见 FactoryBuilt、B2/B3 结果及当前新 quote | ScheduleRecipe F1/iron/1 | S1 ore0/fuel0；A 电8→0；新 job/新工厂自身进度，不继承 F0 stable window |
| rebuild B5：新 job 的完成前提仍成立 | 原 RecipeCompleted | S1 新增 ingot3/slag1；G 新增满足3；F0 未被暂停、回收或替换身份 |

repair 增量 sink 是 ore4（A 来源）、fuel1（S0 来源）、电8；rebuild 是 ore4/fuel1（A 来源）、全部新建材与电18。每条分支本来就使用同一份 A ore4，互斥比较不预留两份。两者都有副产物和原库存变化，没有凭比较增加资源。若 G 原本只允许 S0，则本例 rebuild 输出 S1 不满足 G，必须 needs_goal_change 或另有真正受支持的交付链；不能事后把 S1 加进集合。

**时间口径。** [World.step](../../../crates/oasis7_runtime/src/world/step.rs) 先推进 tick、折旧、处理 actions，再处理 due jobs；不能在某 tick 的 action 阶段用它稍后才完成的工厂。按每一步原 operation 结果确认后、下一 tick 才提交的示例节奏：repair 在 t1 直传、t2 启动、t3 完成；rebuild 在 t1 建设、t2 直传 ore 后完成建设、t3 直传 fuel、t4 启动、t5 完成。步骤允许并行的原执行能力不被改写；本模板选择串行操作与明确依赖，数字不是最快排程或现实秒数保证。真实提交迟延、tick 跳变、验证等待与资源竞争另列 unknown/条件；quote 中 duration=1 不表示从 t0 起一 tick 就完成目标。

保留设施仍会按原维护阶段折旧。若 t0 F0 durability 为 D、无其他负载且足够大，本示例 repair 到 t3 的损失为 1,000+1,000+1,500=3,500 ppm（最后一 tick 为 1/2 slots 负载），rebuild 到 t5 的 F0 为 5,000 ppm；F1 在 t2 完成后 t3/t4/t5 共 3,500 ppm。依据是原 economy 的 1,000 ppm/maintenance unit 与 slot load 公式；不是额外电费，也不声称低耐久阻止本次 native 排产。实际 cut 不满足这些条件时重新计算，不硬填示例数。

<a id="production-recovery-decision"></a>
### 6.4 推荐、原目标适配与独立路径

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182。

先排除不满足原准则、关键来源缺失和当前执行 profile 不支持的路线，再比较有证据的时间阶段、峰值投入、保留价值和已知风险。没有统一材料价格/价值 authority 时不造货币总分或把电力与材料相加。若候选在这些维度不可直接支配，显示具体取舍和玩家原约束，不强制制造唯一赢家；推荐固定算法/profile 及依据，不由自由文本标签决定。

正常例推荐 repair：同样满足原新增3目标，准备阶段更少，省10建设电力和全部新建材，F0能力可继续使用；rebuild 的真实好处是新增 S1/F1 选项，代价是建设及额外准备，且保留旧厂仍有维护负担。若 S0 的合法可用性变化、原路径不能补齐而 S1 全链仍成立，应在新 cut 改判；不能为了固定推荐忽略这个反例。pivot 改成 copper_wire4 时，原 iron_ingot3 目标没有替代接受准则，明确 needs_goal_change，旧满足量不减少。

`independent_lane` 只说明已核候选是否依赖外部主体的新承诺/资源/许可；当前既有合法通用规则不等于必须站队。覆盖有限或任一必要来源 unknown 时，`requires_major_power_sponsorship=unknown`，不能取默认 true。只有原独立范围完整且每条确受阻，并且外部路径自己的承诺/权限/用途和重评条件完整，才可把外部依赖作为受迫建议；v1 不凭全局无方案证明自动生成它。

等待不是第四个无限搜索分支。只有已知原 job/在途/原 authority 条件给出触发、ready_at/截止和可观察结果时，才显示明确的 wait/recheck 步骤；本例是建设和生产原完成边界。到原截止、goal/grant/环境改变或未得到期望结果时停止该等待推荐并重新比较。客户端不自动轮询无限次、续期或保持虚构 reservation；未知来源只允许提示取得来源后再读，不能说等待必然恢复。

## 7. 状态、事务与持久化

<a id="production-recovery-execution"></a>
### 7.1 只读生命周期、当前性与原请求恢复

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182。

比较状态是 `validating → reading → evaluating → ready / incomplete / stale / denied / unsupported / cancelled`。这些都是派生读结果；没有 prepared World transaction、世界 receipt 或资源锁。exact cut 内正确不等于响应时仍当前：返回前按当前 authority 重验读者、owner/task/active goal、read audience、source generation 与规则/profile 版本。读权失效拒绝私有 payload；goal/source 漂移使旧比较失去当前性，可在当前读权允许时只标历史，否则丢弃。不得把旧结果改名为新 cut。

原 goal v2 的 prepare/activation 持久化仍归原 goal authority，使用原 expected revisions、operation 幂等和原恢复机制；同一 goal_ref 对应唯一版本化内容。compare 内存/cache 不作 canonical 接受记录。原目标内容及可枚举摘要保持私有，不进入公开 journal/日志/旁观者响应。

比较 stale 本身不是新的 World 准入 gate。真正提交时，GameClient 使用新的 captured context 和原 quote/签名/权限规则；若原规则仍允许该 exact 动作，它仍可以执行。不得给原 Action ABI 增加必须 comparison_id、必须旧 source cut 相同或必须先做比较的条件。每一步只提交其原操作；Build 的 action 结果 settled 后仍可能只到 FactoryBuildStarted，Schedule 必须等真实 FactoryBuilt，RecipeStarted 同样不等于新增产物。

**未知原结果先恢复。** 客户端保留每步原 operation/request/action/captured context，lookup/recover 优先于新 quote；超时/断线/过滤空列表不能证明未提交。原结果 unknown 时可以解释条件备选，但依赖相同资源/工厂的后继不自动提交、不假定已退款。只有原 authority 确认结果/未提交，才用新 cut 继续；已成功准备步骤不回滚、不换 id 重放。改选 rebuild/pivot 也必须保留先前已消费和未决义务，不把整条计划包装成原子事务。

比较只读请求可在当前身份下重试新读取，不需要持久去重 ledger。取消/超时停止新增读取、模板和派生工作；尽早释放不再使用的 source pin。底层 I/O、decoder 或待发响应尚未终止时继续计入原工作槽和字节，不能先复用槽制造无界后台任务/慢客户端 buffer。原 action 恢复使用原服务通道，不依赖比较成功、比较槽或新的恢复池。

## 8. 部署、安全与运行约束

部署在已有 World Service 只读宿主与 Runtime adapter 内，不增设独立调度服务/第二份 canonical 日志。profile 只有在 typed goal producer、可信 bounded source、共享有界 evaluator 和返回重验同时可用时启用；其他环境明确 source/profile unavailable。世界恢复/G1 沿[原 Runtime 边界](../design.md#runtime-recovery-design)处理，只读比较不绕过恢复状态直接取得可写 World。

读者、执行 Agent、owner/资源 payer、site authority、受益者与候选 proposer 分开。v1 正常例只用当前 owner 的 private task 与本人资源；旁观者只能读原公开来源允许的概括，不能借比较读取私有 goal/精确 grant/隐藏余额。跨 owner 或组织授权 resolver 未支持时返回窄范围 unknown/unsupported，不把 Agent ID 或联盟成员身份当权利。错误只披露当前受众可见的类别，不能用隐藏候选数量或差异错误做存在性探测。

goal说明、blocker detail、工厂/材料 display name、notes、来源标签和 consumer 文案均是不可信文本。按原文安全转义呈现，不执行为代码/HTML/Agent 指令，不传入 World 写工具。算法仅消费验证后的 typed 字段；模型不得补未知、扩展 goal、生成授权或执行夹带动作。

<a id="production-recovery-consumers"></a>
### 8.1 Viewer、API、Agent 与真实恢复证据

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182。

三入口使用同 profile envelope 与错误语义：目标/新增量/允许范围、当前 blocker、每类候选及首步/后继条件、分账成本与保留价值、覆盖/证明等级、推荐理由/改变条件、原恢复入口。读权不同只影响必要披露，不改事实。Viewer 卡片不得从 `repair_available` 或短字符串重造完整路线；Agent 不从“推荐 repair”直接批量执行；pure API 不把 quoted/admissible 当 actual。

当前正式 build/schedule/transfer 由[GameClient 命令](../../../crates/oasis7_game_client/src/commands.rs)及签名 task 链承载；新比较 reader、Viewer mapping 和 Agent tool adapter 是待实现入口。仅将 DTO 填进旧 full-clone projection 或 fixture 面板不算接通。静态消费者应能回答原目标、受阻原因、首步、后续风险、增量/保留价值和改选条件；独立评估者先看去掉推荐的事实再给理由，作者自读不代替该判别。

实际恢复证据逐项关联已选择步骤：原 request/action 结果、真实 transfer/build/job 事件、唯一完成与 ledger delta、目标激活后的匹配满足量。去重键使用原 world/branch + 原 job/action 身份和完成角色；同 receipt 出现在三入口不能重复计数。原余额已有3、转入3、旧 recipe 完成、wrong owner/site/goal revision 都不能冒充本 goal 的新增生产。工作完成后下一项有意义选择可继续原小规模生产或原目标的显式调整，不自动授予区域贡献/资格/收益。

## 9. 质量与容量

<a id="production-recovery-budget"></a>
### 9.1 唯一工程预算表

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182。

以下是 v1 工程初值，适用单宿主、同世界 pin、内存已有原生索引的有限查询；是硬计量/停止目标，不是已测延迟或吞吐承诺。不得为使样例通过而静默放宽；更改 profile 必须复算同一正常链和失败清理。

| 项目 | 上限与计量方式 | 超限/失败行为 |
| --- | --- | --- |
| 请求 | 完整签名/目标引用/候选选择 32 KiB；最多3候选；每候选最多6原动作、3个明确完成/等待边界；最多4补料 kind | 解析前限字节；不截断清单或自动换目标 |
| 原件 | 一次请求最多256个对象，单对象含其解码表示最多256 KiB；整个存活 source arena（原始字节与解码副本合计）最多4 MiB | 每个业务记录、实际range entry、goal scope entry、独立证明节点均计对象；缺任一关键闭包使该结果 incomplete |
| 来源 I/O | 最多10个批读取，每批最多64对象、512 KiB；累计同时受原件总限；包含 proof 和返回重核 | 不开隐藏分页，不自动重开读请求续预算；完整比较不得拼多个 cut/请求 |
| 探查与求值 | 最多1,024次 native index/proof 节点访问，最多32次原 admission/完成规则调用；访问到新依赖先计量 | 没有有界索引的全表扫描不启动；读完后才裁剪不合格 |
| 工作内存 | source 4 MiB + 请求32 KiB + 解码 scratch256 KiB + overlay512 KiB + 响应256 KiB + 计量/证明工作128 KiB = 5,280 KiB；每槽硬配额6 MiB | 所有仍存活的原件、分支、迟到 I/O 和发送 buffer 同槽计量；不以 wire 小代替解码大小 |
| 并发/排队 | 每宿主最多4槽，每当前主体最多1槽；总计量工作配额24 MiB；无内部等待队列 | 超额 busy，给一次可重试提示；不轮换身份/重开请求抢无限后台任务 |
| 截止/取消 | 自接纳起3秒为求值/响应取消截止，包含解析/证明/重验 | 到时停止新工作并关闭本响应；不承诺底层 I/O 清理已完成，清理完成前原槽不复用 |
| 重试/等待 | 自动比较重试0次；新读取有新预算但不能作为旧完整结果续页；原 job wait 只到原触发/截止 | 不新设长期 wait worker、租约或原操作恢复费用；原提交恢复走原入口 |

对象去重只可复用同 schema/key/cut/验证版本的同一原件；当前返回重核是新读，重新计数。固定schema的标量tuple字段不拆成对象，但里面的业务集合逐entry计：ledger每个kind是一个对象，goal每个allowed scope是一项，grant和spent分别计，range header与每条真实记录分别计。canonical常量或单profile内的固定stack/tag字段最多16项，每项仍计字节/访问；超限不把动态清单伪装常量。ledger projection不能把无限map/bundle算1。证明树独立节点逐项计对象/字节/访问；引用hash不等于读到节点。空range header本身不证明完整，仍需受信覆盖证明。原生常驻树的有限访问和新增解码proof两种成本分别计；不能藏在一次RPC名下。

### 9.2 代表性闭包与组合可行性

以下为 §6.3 同一正常 fixture 的**目标适配预算分配**，并非测量报告。每组字节是原件与解码的合计配额；具体 adapter 必须用同一 fixture 证明对象/字节/访问同时成立，不能分别用不同小输入过关。不存在的键以该切点原生索引范围/absence 证明覆盖；证明节点单列，不隐藏无限祖先。没有这些索引/投影时该前置仍未交付。

| 闭包组 | 对象与具体范围 | 合计配额 |
| --- | --- | --- |
| 目标/身份/原阻断 | 11：pinned cut、TaskControl、v2 goal主体、两个allowed site/output scope entry、captured context、owner binding、读权来源、compare规则版本、原失败 action、原拒绝结果 | 160 KiB |
| 账户/材料 | 16：A Electricity元组；A五个kind，S0/S1各四个kind（ore/fuel/iron/slag）；world ore/fuel两个市场解释值 | 256 KiB |
| 工厂/站点 | 9：F0状态、F1不存在结果、F0/F1 profiles、S0/S1 authorities、A location、anchor、F1建设power | 160 KiB |
| 规则依赖 | 10：canonical catalog版本、iron recipe profile、iron/slag product profiles、ore/fuel material profiles、gameplay税策略、industry stage、两个产品module解析结果 | 160 KiB |
| 当前操作权限 | 16：五个agent+object完整range header、五个exact grants、五个各自period spent记录、一个当前owner控制记录；F1 Build/Schedule共用同range和grant余额 | 216 KiB |
| 覆盖/旧实际/因果 | 17：两厂pending recipe范围、F1 pending build/retired、A/S0/S1相关占用、两厂industrial root关联范围共9个空range；F0原建设start/complete/原power来源3份；历史稳定factory snapshot及原cut2份；GoalOperations完整range、原失败P0的context与commit连接3份 | 272 KiB |
| 独立证明节点 | 最多96个，每个原始/解码合计≤1 KiB；对上述键的同cut路径、range及absence，不夹带业务记录 | 96 KiB |
| 返回重核 | 3：当前owner/read根、TaskControl/goal activation、source head及规则版本；按目标返回接口的明确有限字段核验 | 32 KiB |

合计 **178对象、1,352 KiB**，距总限余78对象、2,744 KiB。前六组各1批，proof分2批，返回重核1批，共9次读取；没有把材料kind、目标scope、原因果链、授权range、proof或最后head复核放到预算外。coverage的占用范围用所选ledger加有限kind谓词，不藏真实记录；正常例为空，非空的每条原义务均另外计对象。返回三个对象只能是固定身份/root/version元组，若需要额外授权或私有记录必须另外计数。代表性有限索引访问分配≤192次，proof节点访问≤192次，共≤384次；其中每个独立节点已计入96对象，重复访问仍计访问。若具体树形/完整范围需要更多节点，按总预算实际计量；超限失败，不以静态表证明任意世界树都符合。

原blocker求值1次，repair两动作及完成共3次，rebuild四动作/建成/配方完成共6次，总10次原规则调用；pivot在goal-fit发现产物不匹配，不做虚构后继。normal source加其余工作配额为 `1352+32+256+512+256+128=2536 KiB`，小于单槽6 MiB；全上限5,280 KiB也小于6,144 KiB。4槽最多24 MiB是受计量工作buffer上限，不是进程RSS/常驻World或底层库内存承诺；无法计量的全clone、provider buffer或工作分叉不得进入该宿主路径。

| 场景 | 环境/刺激 | 预期与指标 | 验证入口/当前范围 |
| --- | --- | --- | --- |
| 正常共同闭包 | 同一 fixture、上述178对象分配、最多4主体并发 | 两候选完整、9次来源读取、10规则调用、所有上限共同满足且在3秒内完成 | 手册06；静态可复算，实际延迟/adapter未验证 |
| 边界/关联爆炸 | 请求/对象/字节/证明/动作/访问各limit与limit+1；同一记录的无限数组 | 有界拒绝/incomplete，无截断完整或默认推荐，source reader真实计数 | 手册01/06；待实测 |
| 取消/慢响应 | 第四槽慢I/O或慢客户端；取消/3秒到期后仍有I/O | 不新增派生工作，保留计量槽直至清理；新请求不能扩大并发 | 手册06；待故障注入 |
| 原操作恢复 | 比较满槽时原transfer/build/schedule结果未知 | 原lookup通道独立可用；不等待比较、不重复扣款 | 手册05/06；原入口与新组合待验证 |

## 10. 兼容、迁移与回滚

先实现原 goal producer 的明确版本能力及 old-v1/read-v2 测试，再实现最小可信 source/共享 evaluator、最后接三入口。新 profile 未就绪时沿当前 unknown/unverified 展示；旧 consumers 可以继续读原单动作 quote，但不能据此升级成多步完整比较。新增 goal codec、私有存储、TaskControl 版本解释、context capture 和客户端签名验证必须一起支持 v2，任一缺失则不激活该目标版本能力；不得双写不同摘要或静默降级正文。

比较 envelope、source schema、算法与消费者能力明确协商。旧字段缺失、未知枚举/版本或旧缓存均不能填默认可用；升级不重写原历史 blocker/receipt，也不把原V1 goal迁移成已接受typed目标。关闭本profile只停止派生读取；已经激活的v2目标和已提交原操作仍由原authority维护/恢复，不能通过回滚比较器撤回世界支出、消除未决结果或把v2强制解码为v1。

## 11. 验证设计与可追溯性

先用同一源 fixture 做共享规则与账本的独立复算，随后验证真实服务/消费者/恢复；源码断言和合成前态只证明相应层。发布前必须实际取得成熟世界前置、原goal激活、当前scope grants与正式源记录；未取得时报告具体阻点，不用普通CI或静态文档替代玩家结果。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [AC-PRC-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-01)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | [目标与来源](#production-recovery-goal) | 原接受准则、same cut、当前 blocker与私有主体 | [01](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-01)：V1 unresolved、v2原prepare/publish/activation、旧余额/receipt、跨cut/隐藏范围 | 原goal/任务证明、源闭包、拒绝与当前性 | 本文未执行goal producer或正式读证明 |
| [AC-PRC-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-02) | [候选](#production-recovery-candidates) | 有序合法准备链与共享规则，不把现态拒绝当后态结论 | [02](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-02)：§6.3共同前态、两合法分支、missing grant/site/profile、Maintain反例 | 每步前态、实际quote/action、条件及拒绝 | 未接通的维护/迁移/任意模块仍不支持 |
| [AC-PRC-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-03) | [成本](#production-recovery-costs) | 逐账本峰值、时间顺序、沉没/占用保留与副产物 | [03](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-03)：E18→10/0、同源互斥账、t1..t5、目的溢出/旧工作/假退款 | 独立算式、原sink/完成/ledger delta | 静态复算不证明真实时延或收益 |
| [AC-PRC-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-04) | [选择](#production-recovery-decision) | 原目标适配、真实改变条件、无伪造赞助必要性 | [04](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-04)：推荐repair、S0失效后改判、copper pivot/目的地改动需新goal | 候选取舍、goal-fit、覆盖与原目标revision | 不证明全局最优/所有独立路径穷尽 |
| [AC-PRC-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-05) | [执行与恢复](#production-recovery-execution) | 只读、失权不泄露、source漂移非新Worldgate、原operation未知恢复 | [05](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-05)：无写对照、迟到/失权、合法原动作仍可执行、原请求恢复后重比 | 原request/事件/单次扣款、当前授权检查 | 不声称跨动作原子或自动回滚 |
| [AC-PRC-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-06) | [预算](#production-recovery-budget) | 同一完整闭包满足所有资源限，等待/清理有界 | [06](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-06)：178对象共同样例、各边界组合、慢I/O/响应/满槽、等待停止 | 原件/节点/调用/内存计量、取消及恢复时间线 | 尚无真实性能测量或bounded adapter |
| [AC-PRC-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-07) | [消费者](#production-recovery-consumers) | 三入口同义、独立判断与真实新增产物 | [07](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-07)：正式Viewer/API/Agent同goal/source、去推荐判别、选择一链到真实RecipeCompleted | 消费记录、独立判别、原完成与ledger证明 | 不以fixture/文案证明实玩或MW-002整体完成 |

## 12. 风险、取舍与实施顺序

首先补原goal authority的typed接受版本；否则“同一个目标”仍无法机器核验。两路正式验收还要求原owner delegation producer提供合法BuildFactory grant来源：当前signed install缺该scope，须由原producer在原owner签名、object/period/limit及native quote校验下增加明确支持并验证恢复，不由比较器发grant；此前B只可在明确预装grant的受控fixture验证。其次补当前代码没有的bounded keyed/range adapter，把grant选择、pending/root/制造/占用集合、模块解析、market依赖和正式证明纳入同一预算，并将原admission/reducer纯化为共享接缝；不以旧全量quote临时绕过。随后用§6.3同一源样例完成两条可达链的差分/故障核对，再接正式三入口及独立玩家判别。

初版主动限制 candidate数量、零距离direct准备与可证明的native完成，换取可复算、可停止的整条路径。非零物流、补电、维护、回收、delivery或因果迁移后续只有在各自原authority、当前正式入口、完整来源和预算齐备时才扩profile；本设计不替它们作已实现声明。即使本文所有静态关系检查通过，typed goal、bounded source、共享求值、正式集成、性能与玩家验收仍为待交付，不能据此改写SC-25或MW-002的全量覆盖。
