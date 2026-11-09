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
| Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；[REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) | 受阻后的原目标、真实已发生价值与可达恢复选择连续 | [交付恢复比较](#mature-delivery-recovery-comparison) | 原 SC-31/服务/物流与当前 authority / QA | 故障或换路线本身不构成玩家贡献，比较不执行新世界动作 |
| [AC-PRC-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-01)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | 绑定已接受原目标、真实 blocker 与完整来源切点 | [目标与来源](#production-recovery-goal) | 原 goal authority、World 及身份 authority | 文本解析、现余额和本次确认不能补造原接受准则 |
| [AC-PRC-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-02) | 有限模板映射到实际动作，区分首步和条件后继 | [候选](#production-recovery-candidates) | 原 action admission、站点/工厂/配方 profile | 未支持维护、迁移和任意搜索不能凑可行路线 |
| [AC-PRC-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-03) | 逐步核时间、逐账本增量成本、沉没/占用及保留价值 | [成本](#production-recovery-costs) | 原材料、电力、完成/处置 authority | 不同单位抵销、提前使用未来回收、复制稳定进度 |
| [AC-PRC-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-04) | 原目标适配、独立路径和取舍推荐，显式改目标 | [选择](#production-recovery-decision) | Gameplay、原目标/需求 authority | 有限未找到不证明必须赞助或全局最优 |
| [AC-PRC-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-05) | 只读比较、当前权限、漂移及原提交未知恢复 | [执行与恢复](#production-recovery-execution) | 原任务、GameClient、World Service 恢复 | 无跨动作原子提交或比较器写路径 |
| [AC-PRC-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-06) | 完整有界读取、求值、等待停止和不完整范围 | [预算](#production-recovery-budget) | 有界原件 adapter、宿主资源计量 | 分页拼完整、无限未来模拟、自动反复等待 |
| [AC-PRC-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-07) | 三入口同义消费，独立判别与真实恢复证据 | [消费者](#production-recovery-consumers) | Viewer/API/Agent、QA | 静态卡片、fixture 或单个 quote 不代签正式实玩 |
| [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | 原交付目标/路线失效下两条真实条件链、成本/价值及独立选择 | [交付恢复比较](#mature-delivery-recovery-comparison) | SC-31、原物流、Build/产品规则及原goal authority | 原native v1仍仅production_only，不新造道路/价格/升级/统一迁移 |
| [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | 接收已有三类损坏恢复、服务/旧工作、Unknown与跨入口消费 | [领域适配](#mature-recovery-source-adapters) | CC002、CR002、工业cutover、原operation/G1 | 依原权限/精确剩余处理，不用新目标或新请求清旧义务 |

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
| Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；[REQ-WR-PE-001](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) | [交付恢复比较](#mature-delivery-recovery-comparison) | 同目标恢复后果及下一选择不能由 arrival、重置旧工作或未知补造 | [PRC-08](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-08)：同候选/C0 走原可达 repair/rebuild 正例、matching 与费用/权限漂移反例；联合 PRC-09 核旧义务、原请求恢复和真实读面 | 原接受目标、两路线原动作/结果、已匹配和剩余量、成本与合法下一步 | 静态比较不代签正式执行、所有扰动恢复或真人偏好 |
| [AC-PRC-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-01)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | [目标与来源](#production-recovery-goal) | 原接受准则、same cut、当前 blocker与私有主体 | [01](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-01)：V1 unresolved、v2原prepare/publish/activation、旧余额/receipt、跨cut/隐藏范围 | 原goal/任务证明、源闭包、拒绝与当前性 | 本文未执行goal producer或正式读证明 |
| [AC-PRC-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-02) | [候选](#production-recovery-candidates) | 有序合法准备链与共享规则，不把现态拒绝当后态结论 | [02](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-02)：§6.3共同前态、两合法分支、missing grant/site/profile、Maintain反例 | 每步前态、实际quote/action、条件及拒绝 | 未接通的维护/迁移/任意模块仍不支持 |
| [AC-PRC-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-03) | [成本](#production-recovery-costs) | 逐账本峰值、时间顺序、沉没/占用保留与副产物 | [03](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-03)：E18→10/0、同源互斥账、t1..t5、目的溢出/旧工作/假退款 | 独立算式、原sink/完成/ledger delta | 静态复算不证明真实时延或收益 |
| [AC-PRC-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-04) | [选择](#production-recovery-decision) | 原目标适配、真实改变条件、无伪造赞助必要性 | [04](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-04)：推荐repair、S0失效后改判、copper pivot/目的地改动需新goal | 候选取舍、goal-fit、覆盖与原目标revision | 不证明全局最优/所有独立路径穷尽 |
| [AC-PRC-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-05) | [执行与恢复](#production-recovery-execution) | 只读、失权不泄露、source漂移非新Worldgate、原operation未知恢复 | [05](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-05)：无写对照、迟到/失权、合法原动作仍可执行、原请求恢复后重比 | 原request/事件/单次扣款、当前授权检查 | 不声称跨动作原子或自动回滚 |
| [AC-PRC-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-06) | [预算](#production-recovery-budget) | 同一完整闭包满足所有资源限，等待/清理有界 | [06](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-06)：178对象共同样例、各边界组合、慢I/O/响应/满槽、等待停止 | 原件/节点/调用/内存计量、取消及恢复时间线 | 尚无真实性能测量或bounded adapter |
| [AC-PRC-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-07) | [消费者](#production-recovery-consumers) | 三入口同义、独立判断与真实新增产物 | [07](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-07)：正式Viewer/API/Agent同goal/source、去推荐判别、选择一链到真实RecipeCompleted | 消费记录、独立判别、原完成与ledger证明 | 不以fixture/文案证明实玩或MW-002整体完成 |
| [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | [交付恢复比较](#mature-delivery-recovery-comparison) | 同交付目标下repair/rebuild/pivot、当前资源与原完成边界 | [PRC-08](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-08)：同candidate/C0成熟前态，已匹配2/残余3、两路线、费用/授权/goal漂移；required/full原动作层 | 原接受/匹配与逐payer资源账、真实R/B结果、独立取舍 | 正常fixture不证明正式来源、当前完整实现或必然交期 |
| [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | [领域适配](#mature-recovery-source-adapters) | 原旧工作/服务单位、损坏恢复、同身份Unknown和有界消费 | [PRC-09](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-09)：同cut五类旧工作/原CSF与CC002、丢响应/G1/limit/读权/三入口；required/full组合层 | 原lot/unit/receipt、残余/earned/unpaid/费用、原结果恢复和完整源计量 | 未执行故障域、性能及真人方法不以静态文档代签 |

## 12. 风险、取舍与实施顺序

首先补原goal authority的typed接受版本；否则“同一个目标”仍无法机器核验。两路正式验收还要求原owner delegation producer提供合法BuildFactory grant来源：当前signed install缺该scope，须由原producer在原owner签名、object/period/limit及native quote校验下增加明确支持并验证恢复，不由比较器发grant；此前B只可在明确预装grant的受控fixture验证。其次补当前代码没有的bounded keyed/range adapter，把grant选择、pending/root/制造/占用集合、模块解析、market依赖和正式证明纳入同一预算，并将原admission/reducer纯化为共享接缝；不以旧全量quote临时绕过。随后用§6.3同一源样例完成两条可达链的差分/故障核对，再接正式三入口及独立玩家判别。

初版主动限制 candidate数量、零距离direct准备与可证明的native完成，换取可复算、可停止的整条路径。非零物流、补电、维护、回收、delivery或因果迁移后续只有在各自原authority、当前正式入口、完整来源和预算齐备时才扩profile；本设计不替它们作已实现声明。即使本文所有静态关系检查通过，typed goal、bounded source、共享求值、正式集成、性能与玩家验收仍为待交付，不能据此改写SC-25或MW-002的全量覆盖。

<a id="mature-delivery-recovery-comparison"></a>
## 13. 同一交付目标、路线失效与原义务的恢复比较

Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；本节承接[受阻路径的玩家杠杆](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)：保留造成当前状态的原行动和实际后果，在同一已接受目标下解释已得、在途、已耗、损失与未结义务，再比较当前可达的 repair/rebuild/pivot。玩家选择与后续效果仍沿原执行和 matching 关联；推荐、预览、arrival 或外部自然恢复不能算成本次玩家完成。当前没有可执行候选时如实给出原请求核对、等待条件或安全停止，不能因缺证宣称所有独立路径均不可行。

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182；承接 [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002) / [AC-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)在已采纳交付与服务范围的原义务。本节是目标 bounded_delivery_recovery_v2；§1–12 的 native_production_recovery_v1 及其 production_only/直接转移限制继续有效，不将旧客户端或单个 quote 宣称为新组合已实现。

### 13.1 已接受目标与当前阻断

请求只读引用原 goal/服务 authority 已接受的稳定目标、revision/digest、主体、产物/服务单位、数量区间、允许来源/用途、接收者/目的地、partial/whole predicate、截止及必要依赖；这些约束既不能从自然语言补出，也不能为让新候选可行临时增加。普通生产目标仍走原 v1；交付目标使用原 SC-31 匹配边界，持续服务则引用其原 obligation/checkpoint/unit 和 own/joint completion。服务价格、责任、治理程序及工业目标都不由本比较器新设。

从同一当前 cut 建立原目标的数量与义务账：已合法匹配/完成、已接受未完成、原不可逆 sink、实际在途/custody/loss、可用与被保护资源、earned/paid/unpaid、正常/关闭费用以及原结果未知分别读取。同一原单位可以有多个证据引用，不能因 receipt/outbox/服务读面各有一条而计算多次；原 held 金额是同一 gross 的负担，不是额外余额。新供应额度按原目标未完成量和已承诺未结量的原规则计算；不可将仍在途或未知单位当作可重新生产、交付、扣款的空缺。

route unavailable、接收准入失效、容量不足、缺料、已损失、原工作受阻及来源未知分别解释。路线失败不自动证明货物丢失、原费用可退或目的地可换；到达不等于 storage acceptance、title/terminal 或 SC-31 matching。只因范围缺证不能宣称“全部独立路径不可行”或强制赞助。

<a id="mature-recovery-source-adapters"></a>
### 13.2 复用的完整领域适配

| 已采纳范围与原 authority | 比较必须接收的完整有限事实 | 保持的原行为 |
| --- | --- | --- |
| [SC-31 目标/匹配](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.design.md)、[有限供应用途](mature-world-regional-contribution.design.md#contribution-need) | 已接受完成准则、原 allowed scope、匹配单位/承诺未结量、取消/到期与当前接收；必要的G/U各有原身份 | 生产不补交付，改变目的地/用途须原目标修改；匹配G不自动运行U |
| [跨区物流/条件结算](cross-region-delivery-and-escrow-settlement.design.md#des-wr-cr-001)及[原恢复](cross-region-delivery-and-escrow-settlement.design.md#des-wr-cr-002) | 当前路径逐边权限/可用性/容量/损耗/运费、源/目标账本、真实dispatch/arrival/custody/title/terminal、原safe disposition | 现有已知替代路径可被显式选择；没有原支持的reroute/return不展示。返回也是实际运输，不瞬移、不退款补料 |
| [持续服务](continuing-service-fulfillment-and-remedies.design.md#service-obligations)及[恢复范围](continuing-service-fulfillment-and-remedies.design.md#service-scoped-recovery) | 原未完成单位、依赖、repair window、当前mandate、价款/风险/费用图、受保护责任、stay及真实物理source/outbox | 只处理允许残余；修复、终止、争议或继受沿原合同，比较不解除stay、不转债、不替当事人接受 |
| [工业旧工作](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)及[cutover](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-cutover) | 完整accepted-unstarted/WIP/transit/buffer/terminal-pending清单、原选择/执行receipt、合法profile/换算、终端义务与共享占用 | 每项原处置与后续执行分别记录；successor不继承旧W/顺位/已完成量，不重复sink或退款 |
| [三类损坏恢复](chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-recovery) | 冲突/区域危机/一般经营损失、本人原目标、真实维修/建造/场地与逐步overlay、原损失/责任 | 直接接收CC002及其[两路线/无安全路方法](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#loss-recovery-cases)，不要求普通损坏先参战，不重做维修量或chassis规则 |
| [原有界恢复项目](bounded-recovery-projects-and-contribution-settlement.design.md#recovery-milestone) | 原containment/项目授权、参与资格、实际贡献与里程碑/目标effect、退出义务 | 只有原条件成立才作为可选参与；项目贡献不能替本人修复或默认成为唯一继续路线 |

**Site出库的当前权限与正式接线。** 正常例的S0/S1是canonical Site ledger，原行动Agent A分别是其当前FactorySiteAuthorityV1.owner_agent_id，另记录A的真实个人受益者/当前控制与签名来源；builder-access allowlist不授材料处分权。原[材料源授权](../../world-simulator/llm/external-agent-game-api.design.md#cognition-material-source-authority)与[Runtime校验](../../../crates/oasis7_runtime/src/world/agent_delegation.rs)在无grant兼容路径仍核真实Site owner/Factory builder，且该Agent不能已有任何native grant、不能携override；有grant的当前路径仍只允许本Agent源账。不得因Site出库被拒绝去掉grant、撤掉任务或改走trusted raw action，也不能先加一次Site→Agent转移绕过同一个源权限问题。

目标正式current-owner材料操作适配复用原owner身份与操作authority，不新增grant、Action或比较器权限账。接纳前必须同时验证：(1)原入口的已鉴权owner/acting Agent/current identity generation及签名的完整原请求，按原规范绑定world/branch、operation/nonce、payload digest；(2)该正式manifest要求的task、goal revision/activation、executor/signature与step/finality fence，不能消费legacy no-task例跳过；(3)同cut原Site存在、site_id、正authority_revision与owner_agent_id=A，完整native grant/override状态与原规则相容；(4)精确from Site→B接收ledger:material:quantity、route/path、当时处分/接收/容量与费用前置；(5)G的已接受来源/目的/单位范围、原actor和本次operation关联，不凭owner身份放宽目标。提交前重验相同来源与原版本，沿原Transfer事务记录实际action/job、原goal/受益者归因和原结果索引；它们同根发布，后继matching只能引用这一原结果。精确恢复回读同一已提交结果，不能以当前owner重新归属或再次花费。上述额外证明/索引/返回重核均计入§13.6，不使用旁路读取。

每个正式入口须声明其实际支持的current-owner适配/schema并保留原task/step策略。若该入口必须采用当前Agent-source native grant而没有合法Site-owner路径，则本候选在该入口为unsupported，不能降级成兼容模式或声称整链first_step_admissible；无可验证接线为unknown。正向正式验收必须由原owner producer在满足这些校验的配置中真正提交，不由比较器造授权。现有owner-signed no-task测试仅是legacy/dev-local或组件来源证据，不能代签strict formal可达；当前未接入正式owner适配是实现剩余，所需验证、关联与拒绝合同在此固定。原Build、Agent入料与Schedule每步仍核其自己的入口/权限；若它们要求安装native grant而使后续Site-owner前置不成立，整链不得报可行，不在步骤之间删除grant迁就样例。

适配器使用对应原 schema/manifest 的有界共享 admission、完成/处置规则及同切点来源；没有纯读能力时返回明确 unsupported/unknown，不调用任意 WASM/模型试跑、World.clone+step 或复制一套规则。原 authority 可提供有权验证的 scoped projection/断言，必须标明对象、cut、谓词、完整范围与证明等级；不能把摘要hash当已读完原work/fee/case目录。原生产、服务和损坏的既有正常方法继续独立有效，本节补的是它们与同交付目标的比较接点。

### 13.3 三类候选与取舍

| 候选 | 有限原步骤与原目标适配 | 必须解释的价值和限制 |
| --- | --- | --- |
| repair | 保留原接收者/用途，补齐一个明确阻断；例如对尚未承诺的准确残余从原库存走已存在、当前可用的替代路径，或按原服务规则修复受支持的未完单元 | 新路径、费用、损耗、接收、截止及旧工作处置须完整。此处修复交付方案，不授予更改route availability或创建道路的权限 |
| rebuild | 原目标允许时，在已选合法新地点/能力重新形成同类供给，再沿真实路径交付准确残余；Build、入料、原生产/验证、运输及matching各保留自己的边界 | 新建设、输入/能源/费用、准备阶段和旧能力保留成本真实存在；旧库存/WIP/许可不免费迁入，不重用retired身份 |
| pivot | 一个明确替代用途/recipe/服务或受益范围；按原goal-fit比较 | 满足原允许替代准则才可恢复原目标，否则needs_goal_change。新目标须显式接受，旧已履行、未付和在途不能清账 |

每类至多一个显式候选，分别从同一初态求值；不为凑三路自动搜索地点、市场或第三方承诺。步骤同时记录 first_step_admissible、后继条件、预计可匹配的原单位、原时界和 next_recheck。某一步可提交不代表整链已可行；关键来源、未决原结果或后续必要规则未知时不能返回确定的整链可行或默认推荐。受控正常例至少给两条真实不同的同目标路线，真实世界不保证总有两条。

候选按原参数逐步计消耗、转移、损失、占用、实际原费用及最大当前支出。服务模块的预签完整图、license/validator、closing和程序保护沿原source/purpose计数；同一invoke去重，两个真实ordinal不合并。不能借另一候选余额、尚未到账价款、未来退款或未释放hold承担前一步。保持资源向量；时间由原action/due/terminal条件及原受权计划证明，不把测试tick或未知网络等待映射为保证交期。

推荐继续使用§6.4的可行性/支配与明确取舍：优先满足原目标、当前可负担、阶段与风险可解释的路线。新增地点/能力可作为rebuild的保留价值，但不能用该未来价值冲销当前资金缺口。不完整独立范围为unknown；现有普通许可/付费通路不等于必须加入强组织。wait只有原可观察触发/复查截止及保留成本齐备时有效，不能用“以后route可能恢复”补一条可行路线。

### 13.4 可复核的交付正常对照

固定原成熟前态与目标 G：总需 iron5，原有效matching已完成2、无在途/承诺未结，准确残余3；G事前允许S0合法旧库存或S1新生产，原接收者/目的地B不变且允许这些实际交付，截止容纳两条原完整阶段。S0有真实iron3；A有原smelter建材frame12/coil4/brick6、ore4/fuel1及E24，S1有合法新F1/站点/建设授权。原失效路径不再准入，但两条明确现存替代路径及共享容量来源完整：S0→B的R为2km、tariff2E/单位；S1→B的R1为1km、tariff1E/单位；均能承载3，材料采用原Medium10bps且本次decay0。原路线参数是有来源的测试前态，不是新增通行费规则。

- repair只把S0原iron3经R交付：本体运费6E、loss=floor(3×2×10/10000)=0，A E24→18、S0 iron3→0，B匹配新增3，G已满足量2→5。开始时货物在途、费用escrow，只有原完成/接收/matching成立才归账。
- rebuild复用[原smelter建设/配方](#production-recovery-costs)：合法新F1建设10E及上述建材，ore4/fuel1生产iron3+slag1需8E，再经R1交付3需3E，损耗0；A业务本体E24→3，S1留下slag1，S0旧iron3仍在。BuildStarted、FactoryBuilt、RecipeStarted、RecipeCompleted、transport和matching不是同一结果。
- 当前完整费用图必须正面核实。正常生产使用已批准原铁validator时，实际owner V独立有Data20/E10、I/O及manifest符合§9和[原计量](mature-world-regional-contribution.design.md#contribution-budget)；一次正常模板1emit/无effect/new_state，实际V扣E2，Data扣ceil(I/1024)+ceil(O/1024)+1，完整实际字节须采集。未发生生产的repair不扣该次validator费用。该例其他新增费用为零只有其原规则明确不适用及完整writer证明才成立；若实际有额外费用，逐payer按原source加入并重新核前余额，不能削减图或让V成为免费执行者。
- 两分支原已完成2及其历史成本不重收；目的地旧库存是否还在不是matching量的来源。并列显示R的较低增量和较少准备阶段，以及B新增合法F1选择、维护负担和额外投入。E降为20且其他来源不变时，R仍可负担、B仅业务本体就少1E；若S0处分权失效但S1整链仍有效，则新cut可只保留B。G不允许S1时B不能沿旧目标通过。
- 加入原承诺未结1后，原未完成3中只有2可新承诺；不能仍新发3。原1为unknown或无安全处置时，整目标完成不可确认，但已知实际量仍可解释。whole单元或部分交付规则不允许拆量时整项按原规则拒绝，不偷偷裁剪。

### 13.5 真实选择、未知与跨入口

原GUI/API/Agent消费同一goal/source envelope，显示repair/rebuild/pivot含义、原单位、逐payer费用/峰值、已发生价值、候选当前状态和实际下一步；使用[正式入口适用合同](../../world-simulator/llm/external-agent-game-api.design.md#api-primary-entry-applicability)，不由SDK存在或CLI旧command推导新比较已接通。原业务不增加必须先有comparison_id的gate，真正选择仍逐步沿原入口、当前authority与原quote执行。

目标/读权/source漂移使旧推荐过期；新的无副作用读取不是旧提交的重试。任何原工作、处置、付款或matching结果未知，先通过[原operation端口](../../world-simulator/llm/agent-game-client-boundary.design.md#original-operation-lifecycle-port)核定原identity与payload；更换恢复方向也不能新ID代替它。原物理已完成但服务核定未完时只恢复source-use/outbox及未执行节点；不重复动作/费用。G1 StatusUnknown依[原generation合同](../prd.md#runtime-generation-persistence-contract)保留完整候选并隔离旧World，普通客户端timeout与只读source缺失不伪造这一持久结论。确认结果后以新cut保留实际变化重新比较，不回滚已合法结果、不迁走原责任。

### 13.6 有界适配与验收

继续使用§9.1共享宿主、source arena、对象/bytes/I/O/proof、取消及恢复通道；新profile每候选至多8个原动作或完成边界合计、3候选，总原规则求值≤32。步骤不能把完整模型/模块图、全域搜索或无限case目录藏成一个常量操作；一个原业务必须由其有界共享规则/受权scope证明完整适配，不能只算正常几条effect。不支持、超量或缺原件显示精确范围，原动作合法性仍由原领域拥有。

正常对照的目标容量分配为：目标/原matching12对象，身份/地点24，账本32，工厂/规则32，路线/共享占用36，权限/费用36，proof64，返回重核8，共244≤256；每组范围/absence、各kind、实际成员及proof逐项计，完整原始+解码仍≤4MiB、总读取≤10批。B的8个原边界为Build开始/完成、ore转移、fuel转移、Schedule、生产完成、运输开始/完成；matching在原完成事务内，若所选authority必须另调用则按真实步骤及费用计，不假装零节点。R两边界、pivot发现goal不符而停止，加当前原阻断/适配检查后按实际≤32计量。具体编码/访问峰值和3秒边界仍待实测；分配只是适配工程预算，不证明任意复杂世界都能容纳。

本节不把unsupported全部当作设计收口：上述已采纳原来源、两条正常方案与安全恢复必须有实际适配和方法；它们目前仍是实现与运行证据义务。未采纳的任意新路线建设、统一迁移、服务价格或补偿制度没有因此成为新能力。
