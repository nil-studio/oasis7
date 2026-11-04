# 生产目标恢复比较验收手册

- 生命周期：`active`；Owner role：`qa_engineer`；协作：`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`；Last reviewed：2026-11-04。
- 上游：[REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)、[AC-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)；专业依据：[同目标恢复比较](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#production-recovery-comparison)与 AC-PRC-01–07。
- 技术依据：[生产恢复比较设计](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-goal)。本手册拥有同目标、同读点下的跨域比较与结果验收方法，不替代原材料转移、建造、生产、许可、工作处置或原请求恢复合同。

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182。方法分开报告静态审读、受控 fixture、正式来源、三入口运行与玩家理解；未执行的方法不产生通过结论。

## 1. 执行前置与可复用来源

每次验证固定 source HEAD、tested tree/build、world/branch、当前 owner/reader/executor 与 generation、原 active goal/revision/完成条件、来源读点和 profile 版本。保存完整输入、候选关联、原动作/operation、实际调用入口、输出、逐账本前后量与独立 oracle。缺正式能力或来源记 `blocked`；能力存在而本次未运行记 `not_run`。局部 Rust 构造不能替代正式鉴权、真实库存来源、完整恢复效果或玩家理解。

审读基线为 `567c4c55a2f160574fdcf0902ccb068646210c7d`。以下入口与方法可复用，但组合能力须另证：

| 层次或已有方法 | 真实来源与作用 | 本手册检查的接缝 |
| --- | --- | --- |
| 原目标 authority | [`GoalContentV1`](../../../crates/oasis7_agent_api/src/game/goal.rs) 的 `goal_text` / `soft_constraints` 及原 revision/digest/activation | 当前 V1 没有机器可证的材料、数量、允许账本等接受准则；设计要求原 goal authority 先接受结构化准则，比较只读消费。新 producer/适配未实现时为 `goal_unresolved`，不能以模型解读或本次比较里的确认补真 |
| 主体能力查询 | [`gameplay_capability.rs`](../../../crates/oasis7_world_state/src/gameplay_capability.rs) 与 Runtime [`world_rules.rs`](../../../crates/oasis7_runtime/src/world_rules.rs)；`WorldState::gameplay_capability_query`、`World::gameplay_capability_query` | 当前 repair/rebuild/pivot 保留 unsupported 原因及 unknown；仅 `current_recipe_run` 复用当前配方 quote。它不提供完整三方案、跨步骤成本或独立路线判定 |
| 真实原动作与条件报价 | [`Action`](../../../crates/oasis7_agent_api/src/world_action/action.rs)、[`builtin_recipe_quote.rs`](../../../crates/oasis7_runtime/src/world/builtin_recipe_quote.rs)、[`builtin_factory_quote.rs`](../../../crates/oasis7_runtime/src/world/builtin_factory_quote.rs)、[`agent_delegation.rs`](../../../crates/oasis7_runtime/src/world/agent_delegation.rs) | `TransferMaterial`、`BuildFactory`、`ScheduleRecipe` 是可定位的原动作。一个动作可准入不证明整个方案有足够资源；Maintain/Pause/Recycle 有领域分支，却不在当前 native grant allowlist，不能补成正式恢复快捷键 |
| Build grant 的实际 producer | [`owner_delegation_control.rs`](../../../crates/oasis7_runtime/src/world/owner_delegation_control.rs) 的签名 `InstallDelegation` scope 只允许 MoveAgent/TransferMaterial/ScheduleRecipe/WorldRule，未含 BuildFactory；native install 虽允许 Build，正式 [factory quote host](../../../crates/oasis7_chain_runtime/src/host/world_service_private_factory_quote.rs) 只消费已有合法 grant | 预装 Build grant 的 fixture 只验证 native 链，不证明当前 owner 入口能产生该授权。正式重建须有原 owner authority 合法生产 Build grant 的原件；依赖当前签名 Install 的这层前置为 `blocked`，不能以 client/host 已有 Build 路由代签，比较服务无权安装或扩展 grant |
| Pure API / Agent Client | [`projection.rs`](../../../crates/oasis7_world_service/src/projection.rs) 在已鉴权 projection 发布 `world_rule_status.gameplay_capability`；[`GameClient::industry`](../../../crates/oasis7_game_client/src/device_client.rs) 读取签名 `/v1/world/view-json`；原私有 task [recipe quote](../../../crates/oasis7_client_api/src/world_service/task_recipe_quote.rs)、[factory quote](../../../crates/oasis7_client_api/src/world_service/task_factory_quote.rs) 各有捕获的 context/base binding | 有界 industry 观察不提供完整来源覆盖或 transfer quote；两个 task quote 也不是 MW-002 比较 endpoint。Agent 需要当前授权和正式适配，不从自然语言自行发布可行性 |
| Viewer | [`control_blocking.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_blocking.rs)、[`gameplay_snapshot.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot.rs) 与原 [R-GAME-24](../../world-simulator/viewer/viewer-gameplay-details-uiux.design.md#r-game-24) | 已有只读比较呈现及同源查询接缝；不新增 UI 布局或第一局试玩设计。旧字段、固定文案或共享 DTO 不能证明新增组合已贯通 |
| 工厂与生产原件 | [`economy_factory_lifecycle.rs`](../../../crates/oasis7_runtime/src/tests/economy_factory_lifecycle.rs)、[维护](../../../crates/oasis7_runtime/src/tests/economy_factory_lifecycle/depreciation_and_maintenance.rs)、[建造/回收收据](../../../crates/oasis7_runtime/src/tests/economy_factory_lifecycle/receipts.rs)、[失败处置](../../../crates/oasis7_runtime/src/tests/economy_module_validation/product_validation.rs) | 保留实际 build/start/complete/failure receipt、材料损失与 replay 判据；测试中直接安装 authority、设置库存或注入模块输出均须标明 fixture。故障样例直接补库后重排产不证明免费恢复 |
| 既有领域方法 | [工厂能力生命周期](../../game/gameplay/gameplay-industrial-factory-capability-lifecycle-contract.prd.md)、[背压恢复](../../game/gameplay/gameplay-industrial-backpressure-recovery-contract.prd.md)、[替代验证](../../world-runtime/runtime/recipe-substitution-decision-and-conversion.design.md#sub-verify-001)、[许可 IL02/04/07](industrial-license-and-royalty-settlement.manual.md#il02-use)、[原请求恢复](../../../testing-manual.md#request-recovery-regression) | 复用各自规则与结果，不把材料替代、换配方、许可资格或处置成功直接等同于恢复原目标；不重复另定费用、退款或迁移规则 |

可在仓库根运行下列只读定位；它们只证明源码定义和分支存在，不启动游戏或代签场景：

```bash
rg -n 'gameplay_capability_query|current_recipe_run|unsupported_recovery' crates/oasis7_world_state/src/gameplay_capability.rs crates/oasis7_runtime/src/world_rules.rs
rg -n 'TransferMaterial|MaterialTransferred|MaterialTransitStarted' crates/oasis7_runtime/src/world/event_processing/action_to_event_core.rs
rg -n 'MaintainFactory|RecycleFactory|ScheduleRecipe|FactoryBusy|consume_ledger|duration_ticks' crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs
rg -n 'recipe.smelter.iron_ingot|recipe.smelter.copper_wire|factory.smelter.mk1' crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs crates/oasis7_agent_api/src/world_quote/factory_catalog.rs
```

## 2. 同一前态的两个正常分支

在隔离世界固定读点 C0。原目标 G 的接受记录已绑定原 revision/content digest/activation：owner A 新增一批 `recipe.smelter.iron_ingot` 的合格产物 `iron_ingot:3`，允许接收到 A 的 S0 或 S1 site ledger，完成口径为 `production_only`；它没有外部收货人、交付承诺或自动稳定线完成。接受范围必须先由原 goal authority 接受，不能为迁就重建在比较请求中临时增加 S1。本例的结构化接受记录属于目标设计 fixture；当前 V1 文本目标及尚未实现的 producer 不因此具备该能力，正式层保持 `blocked`。

完整成熟世界场景须先以原合法建设、既有成功批次及原 canonical stable-line 证据建立 F0 的持续能力，保留相应历史；若仅直接安装 F0/profile，只能验证受控比较方法，不能通过“首次持续能力之后”的完整产品前置。C0 不清空旧产物、旧 job 或历史窗口；记录旧产物基线，G 的新增量通过本次原目标/候选到 operation、job 与实际完成事件的关联核算。旧批次、另一 goal 的产出或单纯转移入库不计入 G；缺该关联时不能仅凭余额差分认定完成。

F0 是 S0 已建成的 canonical smelter，原件与当前 profile 可查，无占用生产槽的 pending job。S1 尚无 F1，但 A 的 site/location、chunk readiness、工厂 profile、建造电力 profile 与相应 grant 已有效；F1 是未使用、未退休的新 ID。选用建造电力 10 的合法测试 profile，生产没有额外 maintenance sink。转移是该 profile 下合法的零距离 direct transfer，source 为 A 的 Agent ledger，`route_id=None`、`route_ids=[]`、`auto_reroute=false`。若真实世界要求的来源、运输、许可或授权不能满足，正式层保持阻塞，不能用 fixture 安装替代。

| C0 账本/事实 | 固定值及核对方法 |
| --- | --- |
| A 的 Agent material ledger | `iron_ore:4`、`carbon_fuel:1`、`structural_frame:12`、`heat_coil:4`、`refractory_brick:6`；每种材料记录来源和可用/已占用状态 |
| S0 / F0 input 与 output ledger | `iron_ore:0`、`carbon_fuel:1`；本次新增 iron_ingot/slag 均为 0。直接排产因缺 ore4 受阻，不能借 world 或另一个 site 的矿补足 |
| S1 site ledger | 上述生产材料与新增产物均为 0；建材由 A 的 Agent ledger 扣取，不预先复制到 S1 |
| A 的 Electricity | 18；本例 A 同时是 builder 与执行主体。建造扣 10、生产扣 8，其余资源保持原账本 |
| 原配方和建造规则 | [原 catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)：ore4 + fuel1 → ingot3 + slag1，E8；[smelter spec](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs)：frame12 + coil4 + brick6，建造基础时长 1。电力 10 来自上述测试 profile，不是统一游戏价格 |

初态若通过 `set_*` 或直接安装 profile/authority 构造，只报告受控 fixture；构造完成后冻结 C0，恢复分支中不得再次补库存、改许可或写完成事件。正式来源验证须用实际合法动作、治理/来源记录建立同等前态并保留原件；没有该环境时将这层记为 `blocked`。不能把 fixture 的前置库存称为恢复所赚取的材料。

从 C0 分别建立隔离、同源的 R 与 B 分支；每个分支只选择一条路线，不能先执行 R 后用其剩余库存评价 B。独立 oracle 由原 catalog、原动作事件及 ledger 差分计算，不调用被测比较器或照抄其 cost/recommendation 字段。

| 分支与实际动作顺序 | 独立结果账与目标判据 |
| --- | --- |
| R / repair：显式 A→S0 转入 ore4；回读该原 operation；显式 `ScheduleRecipe(F0, recipe.smelter.iron_ingot, 1)`；按原 ready_at 回读完成 | 转入后 A/ore 为 0、S0/ore 为 4，尚未产生 ingot；start 后 S0 ore/fuel 各为 0，A/E 从 18 到 10；完成后 S0 新增 ingot3/slag1。A 的 fuel1 与全部建材保留，F0 身份保留；仅匹配的真实生产完成满足 G |
| B / rebuild：显式 `BuildFactory` 在 S1 新建 F1；分别 A→S1 转入 ore4、fuel1；显式同一 canonical recipe 一批；每个提交先保留原结果身份，再推进后继 | Build start 扣 A 的 frame12/coil4/brick6 和 E10；不能把建造受理当成建成。转移各减 A、增 S1 对应账；Recipe start 再扣 S1 ore4/fuel1 与 A/E8，最终 A/E 为 0；完成后 S1 新增 ingot3/slag1。旧 F0 和 S0/fuel1 保留，没有回收收入、免费搬迁或继承旧工作进度 |

两条都是同目标下的条件方案，后继步骤仍受执行时原规则约束；首步可以提交不代表方案全部可完成。R 较少消耗新建材、电力及建造阶段的理由可由账本独立说明；B 保留一条同类新能力的选择，不需要先 Pause 或 Recycle F0。当前 `ScheduleRecipe` 不以 durability 限制准入、吞吐或时长，故不使用“低耐久停产→Maintain 修好”作为恢复因果。

各阶段记录提交时点、真实 start、ready_at 与 complete。按无并发干扰、每个原动作各驱动一次 step 的隔离样例：R 为 t1 转移、t2 排产、t3 完成；B 为 t1 建造开始、t2 矿石转移并在该 tick 完成建造、t3 燃料转移、t4 排产、t5 完成。Schedule 前必须已回读 FactoryBuilt；Build 提交已 committed 与建成是不同事实。零距离直接转移没有 transit 等待。基础 tick 不是真实秒数，也不包括尚未证明的网络等待、授权更新、运输或交付时间；阶段条件以[技术成本条款](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-costs)和实际事件为准。

<a id="production-recovery-01"></a>
## PRC-01：同一目标、真实阻塞与来源

承接 [AC-PRC-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-01)；依据 [production-recovery-goal](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-goal)。

1. 用 C0 的原 G、owner、world/branch、scope、目标 revision/digest/activation 和已接受准则请求比较。先取只有 `goal_text` / `soft_constraints` 的 V1 正反样例，预期为 `goal_unresolved` 并指向原澄清入口；同样文本经原 authority 合法接受且有绑定证明后，才评价该准则。模型推断、当前读者复选框或比较现场确认不能取代 producer。自由文本中的指令只作原文，不触发动作或修改准则。分别定位 F0、实际 input ledger、配方和当前资源/授权原件；验证缺 ore4 的原条件报价或拒绝依据。不能从旧 narrative、全局工业阶段、另一玩家工厂或已有余额反推本人目标与失败原因。
2. 保持输入文字相同，分别改变 goal revision、owner generation、world/branch、接受产物/数量、允许的 site、完成口径；原响应不可被当作新目标的比较。用原接受记录只许 S0 的独立负例，在比较请求中添加 S1、更换产物或延长原 deadline，预期拒绝绑定不符或明确 `needs_goal_change`，不能现场确认后沿用原 revision。经原目标权限显式变更并接受新 revision 后，重新比较只属于新目标；旧目标的未满足量和义务仍按原规则保留。
3. 用缺料的 C0 与材料已到达的 C1 混合、旧 factory/profile 与新 grant 混合、仅有 ACK 的 operation 或旧 snapshot 请求比较。预期保留 stale/缺源/未知，不产出已完整核验方案；同 tick 相等不足以证明同一来源切点。
4. 依次提供完整空候选、无读权、源未启用、读取失败、原 source 未完页、版本不支持。只在完整范围确无匹配时解释为空；其余说明实际限制。受限读者不能通过候选数量、成本差异或失败原因获知不可见原件。

保存逐项来源对应及覆盖范围。至少一组负例由独立 fixture 改变原事实构造，不能只篡改最终响应 DTO；来源恢复后重新读原 G，不补写目标或生产记录。

<a id="production-recovery-02"></a>
## PRC-02：真实候选、原动作与可行性状态

承接 [AC-PRC-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-02)；依据 [production-recovery-candidates](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-candidates)。

对 C0 比较 R/B，每个候选保留完整动作参数、前置、后继阶段及原准入理由。分别验证 `route_status` 的 `conditional_feasible / blocked / unknown / unsupported / needs_goal_change`，并独立核对 `first_step_admissible` 与 `goal_fit`；条件链可行不表示后继现在可提交，首步可执行也不证明整链满足目标，不能用 false 或空列表兼容掉区别。

| 独立输入或反例 | 预期 |
| --- | --- |
| 只返回 `current_recipe_run`，或只证明 F1 可 Build | 不能宣称完整 repair/rebuild/pivot。单步真实 quote 可以展示，但组合剩余步骤及最终目标仍须证明 |
| C0 仅将 A/E 改为 17 | R 的 E8 仍足够；B 的 Build E10 单步足够，但后续 E8 总额不足，B 不能标为整条可行。不能分别对原 17 重复检查两步后得出可行 |
| A 有矿而 F0 input ledger 无矿；另将矿放到 world 或无权 site | 未转入前排产仍缺料。可观察库存不是本工厂可用材料，别处余额不替代当前 owner、ledger 或转移权限 |
| F1 ID 已 retired、S1 无有效站点/位置或 chunk 未就绪 | 对应新建候选受阻；不能重用退休 ID、静默换 site、安装 authority 或省略建造来取得可行结论 |
| 有读权却无 Transfer/Build/Schedule 当前执行资格；只允许 Agent 出料 | 已知资格不足按原条件报告；不扩大为 site 出料或其他主体供料。缺权限证据保持未知，不用旧 grant 补签 |
| F0 槽位被真实 pending job 占满；profile 不允许目标 recipe；许可未生效/到期 | 分别显示容量、recipe fit、许可约束与其作用范围，不合成“缺钱”或“不能生产”。合法新位置不能自动继承旧许可或 capacity reservation |
| 只有低 durability 或一个 Maintain 成功事件 | 不构成当前缺矿的修复路径；Maintain 的材料消耗与耐久效果可按原合同显示，但不能代证产线恢复 |
| 替代/重配 adapter 未开放，或只取到替代列表的一部分 | 标明未支持/不完整，不能强制三类各有一个可执行按钮；一个类别没有已知方案不证明所有独立路线不存在 |

不从同一被测函数生成预期；用上述原事实及实际 admission/rejection 分支核对。正式组合能力尚未发布时，本节组合运行记 `blocked`，仍可审读窄动作源码并单独记录已有测试的原覆盖边界。

<a id="production-recovery-03"></a>
## PRC-03：阶段时间与逐资源价值守恒

承接 [AC-PRC-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-03)；依据 [production-recovery-costs](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-costs)。

1. 执行 R/B 独立分支，按 owner、ledger、material/resource kind、数量与单位对账，区分需新投入、当前占用、已消耗损失、实际回收和保留价值。材料转移的出入两笔不是两份消耗；A/fuel1 与 S0/fuel1 是不同位置的两份资源，不能在同一方案重复使用。未有换算 authority 时，不把 ore、frame、电力和声誉相加成统一“成本 23”。
2. 用 A/E17 反例核多步累计；再让建材或 ore 被另一真实 operation 先占用/消耗，核剩余可用量。若获取承诺或库存尚未落账，其值是待决条件，不是立即可用余额。候选间复用同一 C0 的替代比较不代表两条都获预留。
3. 独立加入有 active job、已消耗失败、在途、buffer、待 terminal 结果等已有原件，按[原工作处置](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)逐项核对。失败处置的 `consumed_lost` 不因重新排产消失；新 factory/recipe/root 不继承旧 W、完成收据或奖励。补料、转换完成、建成与原目标完成分别判定。
4. 只在原领域支持、权限与 fixture 合法的独立分支核 Recycle/Maintain：回收必须取原 durability 和每种 build_cost 的原公式、整数舍入与实际 output ledger；现有回收系数为 700000 ppm，不能全额退款。active job 时回收拒绝，退休 ID 不可同 ID 重建。Maintain 请求 `parts` 可大于实际 `consumed_parts`，成本以真实事件和余额差分为准。这些领域分支不补入当前正式 native grant 或正常 B 方案。
5. 对照真实 start/ready_at/complete 核时间，分别变化 profile、非零运输、排队/容量窗口、许可有效期和 terminal 条件。只更改工厂展示 throughput/durability 不能编造当前 direct Schedule 的时长因果；当前持续时间来自原 accepted plan 和实际适用的 scarcity 规则。未知阶段不能填 0，最长已知阶段不能冒充总耗时。

验收证据应能用原账本复算 R 的 E10 与 B 的 E0、两分支各自保留材料及新增产物。使用 module-backed 生产失败 fixture 时，以其真实 plan/失败 receipt 另算；旧 logistics_drone 测试不属于本例 canonical iron_ingot 配方，不能把它的输入、power10 或失败输出套用到本例的 ore4/fuel1/E8。

<a id="production-recovery-04"></a>
## PRC-04：目标适配、推荐与独立路线

承接 [AC-PRC-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-04)；依据 [production-recovery-decision](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-decision)。

1. 对 C0 询问“哪条更适合完成 G、为什么、什么条件会改变推荐”。比较必须引用两条真实阶段、增量代价、保留/损失价值与未决风险；不能把最便宜的单步、默认 `repair` 标签或当前世界更成熟直接当作推荐理由。推荐不选择、不预留、不提交。
2. 在独立 fixture 给 A 合法 copper_ore3 和相应当前资格，展示 canonical copper_wire 配方的技术可执行性；它产出铜线而非 G 的 iron_ingot3，必须显式为 `needs_goal_change` 或保留不适配说明。改变名字、把 copper_wire 当钱、生成新 goal 或选择“转向”按钮均不能完成旧 G。只有当前原目标已接受的用途/产物和完整真实来源才可认定同目标 pivot。
3. 将原目标接受范围限制为 S0，或加入原 recipient、quantity、quality、deadline、region/terminal 条件。分别核 B 及替代产出是否仍适配，不把 owner 相同、总库存增加、FactoryBuilt 或 RecipeCompleted 单独当成交付满足。
4. 保持 C0 独立 R 可行，仅变推荐文案为“必须加入组织/找赞助”。预期拒绝这种受迫依赖结论。再使已覆盖的独立候选确实受阻，但不给任何合法外部方案原件，预期为“该范围独立路线已证受阻，外部方案未核实”，不能报“必须找赞助”。只有相关独立范围完整且已证不可行，同时外部路线自身的当前授权、能力、用途和条件均有原件，且确能解除具体约束，才可说明该范围的受迫依赖及重评条件；首版未接入该 authority 时保留外部未核实，不新增赞助动作。受限、超预算、未知或不支持不能证明全球独立路线均不存在。
5. 同事实只改语言、文案和标签顺序，候选状态、goal-fit、赞助必要性和费用不变。已知关键缺口未补齐时，下一步应指向查证、修改候选、刷新或暂缓，不能把未知包装成默认提交。

独立玩家判别使用第 PRC-07 节的方法；文本能读懂不替代实际动作效果，技术可行也不保证玩家愿意牺牲原位置与材料。

<a id="production-recovery-05"></a>
## PRC-05：只读、当前权限与原 operation 恢复

承接 [AC-PRC-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-05)；依据 [production-recovery-execution](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-execution)。

1. 对正常、拒绝、超时、取消及重复比较请求，比较世界 snapshot/journal、pending actions/jobs、材料/电力、hold、授权、目标与工作状态前后值。预览不得转移、建造、排产、退款或推进时间；按原合同允许的鉴权/限流诊断元数据单独记录，不将其误判为世界业务效果。
2. 取旧比较后分别撤销读权、撤销动作 grant、换 owner generation、消费 ore、改变 F0 capacity、使 S1/profile/许可过期。返回与再次取原件均检查当前读权；有读权不等于动作权限。迟到响应不得覆盖当前 reader/goal/revision。提交原动作按当前原规则重验：不合法时明确失败；仍合法时保留原可执行性和实际结果，只把旧比较标为陈旧，不新增“必须持有最新比较”的 World gate。
3. R 的转移已提交而响应丢失、B 的 Build 已提交而结果未明、Recipe 已 start 而完成结果暂不可读各一例。保留原 operation/request、完整原 payload、world/task/owner/fence 关联；先按[原请求恢复](../../../testing-manual.md#request-recovery-regression)查询原结果。不得为得到新比较而另换 ID 重发、重复扣款、补建第二个 factory 或把源读取超时当业务失败。
4. 分别给 received/pending/unknown/history unavailable、经证明 rejected/expired 和 committed；原 operation committed 还要区分 BuildStarted/FactoryBuilt、RecipeStarted/RecipeCompleted/失败处置。近期窗口缺项或查不到不证明原请求未执行；有 terminal 证据后再按当前权限选择获准后继，读恢复不授予任何新写权。
5. 对 Stop、撤权、重新连接与服务恢复，验证只保留原关联与当前允许的查询；无读权时保留本地关联并报告阻塞，不换主体探查。原结果查询不等待新比较成功。需要原合同允许的安全重投时使用同一原身份与内容；没有该保证则只回查。合法原请求恢复后再执行获准后继，核世界结果，不能仅以“没有重复”代签恢复可用。

当前 [`operation_lifecycle.rs`](../../../crates/oasis7_game_client/src/operation_lifecycle.rs) 的 recovery policy 是读取/继续原入口的建议；它不是新动作授权或通用 exactly-once 证明。不同入口保留各自原操作身份、重投条件及历史保留范围。

<a id="production-recovery-06"></a>
## PRC-06：有界求值、不完整与有条件等待

承接 [AC-PRC-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-06)；依据 [production-recovery-budget](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-budget)。

实现验证前固定技术预算表的版本，以其唯一数值为上限，不在手册另设配额。用正常 R/B 样例记录实际输入字节、候选/步骤、引用及授权/覆盖 proof、索引探查、解码对象/字节、读取次数、响应、并发与截止耗时；原件和辅助证明均计量。特别核对 scarcity、maintenance、成本及价值规则所需的全局/范围依赖：不能用一份小 DTO 隐藏全库扫描，也不能把未读取的适用规则当空集/免费；缺有界来源证明时该结论保持未知。未经计量的整 world clone、全库扫描或无限前推不能充当有限比较的实现证据。

| 可控制的边界输入 | 判据 |
| --- | --- |
| 每个独立预算维度恰好在界内、恰好越界；候选/步骤合法但 proof/授权依赖超限 | 按同一计数规则判断，越界明确对应未知/不完整；不截掉代价、最后一个 blocker 或引用后称完整，不把单个低成本候选当完整候选覆盖 |
| 重复 source 引用、复用已读对象、隐藏依赖或 source 仍有未读页 | 去重方式不能漏记实际工作和证明；完整性来自精确范围，不靠响应成功或对象数量。内部重试、重开读取、分批拼装不得规避单次预算 |
| 慢读、迟到响应、取消及 reader 并发竞争 | 停止新增工作，资源按实际结束/清理释放；未结束 I/O 不能提前复用槽位。保留正常原 operation 回查路径，不以新的比较负载饿死既有恢复 |
| 当前 pending transit/job 有原 ready_at 或 capacity release trigger | 等待指向具体原身份、时钟、触发与下一次复查；到点先查真实变化，不能用 `wait_ticks` 本身证明到货、释放或生产完成 |
| 无可证明触发、来源持续缺失，或有限复查后状态无变化 | 明确暂缓/停止及重新开启比较所需事实，不自动循环刷新/等待、改目标或要求赞助。过预算和未知不改写成零成本/零时间/全部不可行 |

source 的合法后续页仅属于来源读取；是否能完成本次比较仍受该请求总预算与完整覆盖约束。不得在 UI/Agent 中发明能无限续页、拼齐后追认“原比较完整”的接口。预算和源恢复后允许显式重新读原 G，不能恢复旧执行授权或补记原目标完成。

<a id="production-recovery-07"></a>
## PRC-07：三入口、独立玩家判别与真实结果

承接 [AC-PRC-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-prc-07)；依据 [production-recovery-consumers](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-consumers)。

当目标组合接口具备正式适配时，在同一候选/当前权限和可证明同一 source cut 下，分别走 pure API、Agent 的正式工具、Viewer 的真实读取入口；记录实际 endpoint/handler、签名/任务边界及原输出。逐字段对照原目标、候选和原因、已知/未知、阶段、逐资源成本、保留/损失、goal-fit、执行/复查边界。不得直接把同一个内存 DTO 喂给三个 presenter 后认定三入口通过。

API 当前有观察和窄 quote、Viewer 有现有只读 surface、Agent 有可解释摘要，这些各自可验证；新增比较尚未接线的入口单独记 `blocked`。旧 snapshot/旧 DTO 缺新字段时保留未知或明确能力未提供，不能恢复早期按 `goal_kind` 或关键词补真的 fallback。能力协商和回滚不得开放未发布动作，也不改变已有 first-settlement、当前 stable-line 与 failure disposition 的语义。

玩家理解分为两层，均以独立预期卡判定：

1. 静态读法：把 C0 的 R/B、E17、铜线转向、交付目标、受限来源和提交 unknown 六组文本比较交给未参与编写的人，首轮隐去推荐标签。先由 QA 从原 catalog/事件/账本写好答案，再要求读者指出原目标、至少两条候选差异、哪些是已知/未知、哪份材料仍保留、是否需要改目标、下一步是什么及何时需要查原 operation；再显示推荐，询问它的依据和改变条件。保存回答和误判原因；不能把讲解后的复述、作者自评、跟随标签选择或模型自报理解当独立证据。
2. 正式运行：在已授权测试环境中，读者从实际入口选择 R 或 B 并显式逐步操作，核真实材料、电力、factory/recipe 状态与匹配的生产/交付原件。先补料/建成只能证明该步骤；完成一批也不自动证明长期稳定线或区域需求。若原 G 要交付，继续按原 terminal/recipient receipt 判定，不能拿本手册的 `production_only` 样例代签。

Agent 正式执行沿[原自测手册](agent-runtime-self-playtest.manual.md)取真实工具与世界结果；Viewer 运行沿既有 S6 与 R-GAME-24 表面范围取证。本次文本方法不另定试玩目标、第一局 UI 或正式 seed 未决项。最终在原 Task/PR 记录七项适用结果、未运行层次与具体缺口；只有本次局部方法闭合，不能因此提升 MW-002、成熟世界产品或 #5409 的整体实现/验收状态。
