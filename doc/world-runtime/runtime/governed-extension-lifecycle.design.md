# 受治理扩展的期限、范围变更与有限退出设计

- 生命周期：`active`；本文是首版目标合同，不是已实现功能、已批准政策或运行验收结果。
- Owner role：`runtime_engineer`；`gameplay_designer` 拥有期限、处分和替代政策；`producer_system_designer` 拥有产品边界；治理、工业、WASM、持久化、Viewer 与 QA 分别提供适用 authority、执行和验证。
- 审读日期：2026-11-05；固定静态基线：`9f332f2973c31f5948269b970c7aa5d4e7a6fe82`，repository `nil-studio/oasis7`。
- 唯一玩法政策：[GAME-018 governed-recipe-lifecycle-policy-v1](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#governed-recipe-lifecycle-policy-v1)。本文补执行合同与工程上限，不从技术发布状态产生治理权限。
- 验证入口：[受治理扩展生命周期手册](../../testing/manual/governed-extension-lifecycle.manual.md)。本轮只做文档与静态源码审读，未运行 Rust、WASM、游戏、模型、浏览器、真实发布或玩家验收。

Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965

<a id="des-wr-gr-lifecycle-001"></a>
## 1. 问题、目标与非目标

PPE 的精确工件 freeze 已能阻止该工件的新准入；模块 active、历史 Apply 和配方 Profile 仍不足以说明某个工厂今天能否使用扩展、何时到期、撤销后能否完成已投入的工作，以及谁正在重叠范围执行。目标是把当前授权、实际 recipe 行为、有限工厂、每次新效果、旧工作处分与权威结果连成同一可恢复合同。

首版 `governed_recipe_factory_v1` 仅承接一个已完成[提案、模拟、试点和准入](industrial-creation-simulation-and-admission.design.md#creation-admission)的 exact Pure recipe/artifact，至多两个显式工厂，每厂至多一个未闭合 job，每 job 至多两批。首个受支持 binding 就是 §9.2 的 PPE 新 ID 铁锭候选及其明确产品验证闭包；其他 Pure recipe、输入/产物或 validator 组合须另有完整 adapter，不能套用本篇铁锭上限。只支持本地原输入、原输出 ledger 和一次原计划完成；不支持新 product/factory、物流、替代材料、转换、迁移或收费许可。适用许可须有原 authority 的有效零费或 `not_required` 决定；没有决定不能当成免费。普通工厂、原生内置配方和既有合法资产不因这个 profile 自动变为可撤销扩展。

本专题承接 GR-002/003 的这个有界切片，不覆盖任意插件、区域服务、一般基础设施或全部工业许可。模块兼容/迁移、配方六态、四阶段证据、版税、原资产和合同 authority 继续由原专题拥有。本文没有构建示例工件、实现新状态字段或验证持久化底座；产品覆盖保持 `partial`。

## 2. 上游约束与相关角色

合法治理来源决定有限扩展权；工业 authority 决定精确能力与存量处置是否可行；资产及资源 owner 同意实际操作和扣减。Runtime 解析这些当前事实，守住实际 sink、完成和原子发布。PPE required-role approver、artifact owner、普通多数票或技术 finality 都不能单独补齐扩展治理权。Viewer/API/Agent 读取同一权威投影，不自行解释 grant 是否有效。

### 2.1 需求承接与分配表

本表按八个 GAME-018 professional AC 分配唯一的上游/本设计关系。各 AC 的原声明及下列 Trace refs 反向承接 [REQ-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)、[AC-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-002)、[REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003) 与 [AC-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003)：保留完整授权、当前发布/激活、期限处分、历史与单一重叠授权的原产品义务，仅覆盖本 profile，不扩展为全部扩展类型。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [AC-GAME-018-07](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-07)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 完整 grant、处分、稳定能力与工厂范围；缺项不能扩大能力 | [身份](#extension-identity) | 当前合法 governance decision、creation lineage | caller 不能自定新 domain/alias 绕开冲突 |
| [AC-GAME-018-08](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-08)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911 | publication/install/activation 在真正提交时重验当前资格和 PPE freeze | [消费者](#extension-consumers) | module release/activation、artifact owner、WASM | raw bytes 上传不等于发布/启用权限 |
| [AC-GAME-018-09](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-09)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 每次实际效果核 current scope/时间，不让旧 plan、direct/native 或 due 旁路 | [当前守卫](#extension-current-guard) | recipe admission、资源/设施/产品 authority | ordinary active 或旧 receipt 不续授 |
| [AC-GAME-018-10](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-10)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 有限退出只处理原 job、原数量和原义务；无合法出口保留责任和历史 | [处分](#extension-disposition) | 六类工作处置、原 custody/terminal/许可 | 不实现 unsupported rework/return/reroute |
| [AC-GAME-018-11](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-11)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 新授权 pending，旧 stock 闭合后原子移交 F1，F2 不被全局停机 | [替代](#extension-atomic-replacement) | 当前权威顺序、单一 scope selector、CAS | 不允许旧 drain 与新生产在同范围并行 |
| [AC-GAME-018-12](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-12)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 完整 stock、调用、资源和输出容量有界；退出预算不被新生产耗尽 | [预算](#extension-budget) | exact manifest、metering、受保护资源 hold、工业容量 | 无实测吞吐承诺，不以单次 call cap 代替总预算 |
| [AC-GAME-018-13](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-13)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 并发、重复、未知提交和恢复保持单次效果、不续期、不补当前来源 | [事务与恢复](#extension-recovery) | authoritative journal、durable publication、schema reader | 不将未合入存储实现或兼容默认值当授权 |
| [AC-GAME-018-14](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-14)；Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 玩家看到当前 scope、依据、旧工作、成本、blocked 与合法下一步 | [读面](#extension-read) | Viewer/pure API/Agent、QA | UI 与文案不形成执行证据 |

## 3. 当前状态、目标状态与差距

下表是固定基线的源码观察。已有局部原子边界可以复用，但不等于本 profile 已接通。

| 对象 | 当前状态 | 目标状态 | 差距/假设 | 直接来源 |
| --- | --- | --- | --- | --- |
| 精确工件 freeze | key 为 module ID/version/WASM hash；register/install/upgrade/activate/rollback TO 检查同一 marker，历史 marker 持久化 | 新 grant 与这个硬门合取，不能解除 marker | freeze 不自动停止现存 recipe job，也没有工厂范围/期限 | [freeze guard](../../../crates/oasis7_runtime/src/world/module_admission_freeze.rs)、[marker](../../../crates/oasis7_world_state/src/state/module_admission_freeze.rs)、[ModuleChangeSet](../../../crates/oasis7_runtime/src/world/base_layer.rs) |
| PPE Apply | 复核 current artifact owner、当前 required roles、Shadow commitment、append-only profiles；install 与 release tail 有 prepared publication | 绑定扩展行为和当前治理决定后同一发布 | required roles 不是此专题的治理来源 | [Apply](../../../crates/oasis7_runtime/src/world/module_actions/release_support.rs)、[publication](../../../crates/oasis7_runtime/src/world/module_release_publication.rs) |
| Recipe Profile | recipe ID、substitution rules、bottleneck tags、stage gate、preferred factory tags | 另有不可变行为来源 binding，明确 active 与授权不同 | 当前 Profile 不包含配比程序、grant 或六态 authority | [ABI](../../../crates/oasis7_wasm_abi/src/economy.rs) |
| module-backed schedule | 当前 active manifest 执行后转普通 ScheduleRecipe；resolved action 无 module/version/hash 授权出处 | 传递验证过的 producer/plan/admission stamp，实际 sink 再验 | 不能只在外层 API 新加一次检查 | [经济模块解析](../../../crates/oasis7_runtime/src/world/economy.rs)、[普通排程](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs) |
| 工业与原生身份 | CommittedRecipeOrigin 记录 consensus submission；industrial completion guard 对带 root 的 native job 验原 root、owner、destination 和 builtin plan | 当前扩展守卫与原守卫合取，历史来源保留 | 普通无 root extension job 不受该工业 guard 的完整 authority 检查；consensus origin 不是 grant | [origin](../../../crates/oasis7_world_state/src/events.rs)、[工业完成](../../../crates/oasis7_world_state/src/world_rules/industrial_completion.rs) |
| 完成与重试 | pending/receipt/settled job 比较；产品验证保存 attempt/decision；未决旧 attempt 不重跑 | 精确 validator identity、当前处分和 job stamp 随完成持久化 | 产品 receipt 目前只有 module ID；with/no modules 两路必须一起适配 | [完成准备器](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs)、[产品记录](../../../crates/oasis7_world_state/src/state/industry_state.rs)、[due](../../../crates/oasis7_runtime/src/world/economy.rs) |
| 模块计费 | 按 artifact owner 收 Data/Electricity；找不到 owner/Agent 的现有分支不收费；trusted command 有最坏费用预检 | extension 执行前必须找到合法 payer、预算和 closure hold | 普通 economy call 未复用该预检，不能把缺 owner 当零费 | [metering](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)、[trusted command](../../../crates/oasis7_runtime/src/world/capability_authorization_command.rs)、[fee](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs) |
| 现有工业专题 | 四阶段、配方六态/六类处置和许可已有目标合同及部分代码 | 合取这些 authority，只补扩展授权生命周期 | 不重复实现原工作图、许可状态机，也不代签其完成 | [四阶段](industrial-creation-simulation-and-admission.design.md#creation-admission)、[配方](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-lifecycle)、[许可](industrial-license-and-royalty-settlement.design.md#license-lifecycle) |

## 4. 边界与结构

Runtime 的目标原生 lifecycle adapter 只组合治理与工业事实、比较 revision、准备限定写集和生成 receipt；它不执行另一个控制器 WASM。Pure recipe 只提供 plan，产品 validator 只提供判定，两者不直接改世界。领域记录可沿既有正式 publication 的版本化状态适配；必须使旧入口和最终 reducer 消费同一记录，不另存 UI 或外层服务的第二份授权真值。

<a id="extension-identity"></a>
### 4.1 精确行为、稳定能力与范围

`RecipeBehaviorBindingV1` 是目标逻辑记录，身份含 world/branch、所属 authority/实例、object kind/local ID、exact revision 与内容摘要。它绑定：原 creation/admission lineage；权威指派的 `protected_capability_slot`；不可变 recipe ID/revision/Profile digest；Pure module ID/version/WASM hash、完整 manifest digest、source/build identity；产品验证及适用许可 refs。Profile 元数据、原程序和授权分别有 owner、schema 与 digest，不能互相冒充。正文这些逻辑字段不是声称已存在的 wire/ABI 或 ModuleManifest 字段。

冲突索引唯一键为 `(world_id, branch_id, protected_capability_slot, factory_id)`。工厂集合是有界、去重、精确 ID 集合，不支持通配符、动态区域别名或“全部后继工厂”。scope selector 的 key 不包含 grant ID、module/recipe 版本、caller、source package 名或 authority domain；authority domain 是可验证来源信息，不是 caller 可选的分片。原治理目录先解析跨 domain 的同一受保护 slot，重叠管辖缺共同认可的当前来源即冲突，不能各建一个 selector。全厂数量、scope revision 与索引 membership 一起提交，查询结果不得漏掉来自另一 domain 的 claim。

每 key 至多一个当前 executable owner，owner 可以执行普通权内的效果或原决定明确的有限处分；`pending` successor 没有执行权。scope 中 factory 位置、owner、资产代际、原 ledger 与 recipe/能力资格均有精确引用；发生迁移、复用 ID、权限来源失效或缺 revision 时重新核验，不能借同字符串继承。

extension recipe 的每个新发布 ID 必须正向登记 binding。别名和改名解析到原 protected slot；无法证明来源的 ID 不进入执行目录。新 artifact、新行为版本或新能力不是自动 successor，必须完成原四阶段及新授权，不继承旧 root、库存或期限。独立新能力能否另有 slot 由原 authority 证明，caller 的新 lineage 字符串无效。

原生内置配方通过受信 builtin identity/版本目录及 `builtin_recipe_plan` 的精确 canonical plan 识别，不能仅以 recipe ID 前缀、caller 的 `native` 标签或 Profile 是否缺字段判断。普通入口收到内置 ID 下的自造 plan 仍须拒绝。用户可以显式选择其另外仍合法的内置路线，但该路线必须按自己的权限、资源和 plan 执行，不能附带被限制扩展的来源或效果。

### 4.2 既有 schema 来源与目标记录

当前模块结构来自 [WASM ABI](../../../crates/oasis7_wasm_abi/src/economy.rs)、[模块生命周期](../module/module-lifecycle.md#sr2-activation-contract)及其发布记录。目标 binding/grant/stock/receipt 必须各有 schema version、canonical encoding、精确字段许可、内容摘要及受支持 reader，按既有 [SR2 类型与身份](../wasm/wasm-interface.md#sr2-types)发布。原工件、完整 manifest、产品 validator 和政策 bytes 是可达的不可变输入；读取声明 schema/hash 并正向验证 producer 来源，不能从模块名、普通 WorldRule receipt 或 `serde(default)` 补授权。

## 5. 关键运行流程

<a id="extension-current-guard"></a>
### 5.1 当前授权与生效边界

grant 必须同时声明 exact binding、受权主体、明确 factory 集、用途与允许效果、有限 `[begin,end)`、当前治理来源/决定、对应 revision、到期与主动变更的存量政策、最小处置 actor/范围/硬截止算法、真实资源及依赖保留、撤销/复查条件。日期/时间只用 canonical `WorldTime`，不用本地墙钟、提交端时间或服务清理时间。`begin >= end`、checked 时间溢出、未知 schema 或缺任一处分条件拒绝。

每次效果在准备和最终提交时都满足：`exact behavior binding ∧ current governance source ∧ sole scope owner ∧ allowed effect/time ∧ current industrial admission ∧ current recipe/scoped lifecycle ∧ legal technical activation ∧ current asset/operator authority ∧ applicable license ∧ actual resource/capacity/dependency checks`。只有对应 authority 可明确判定某项 `not_required`。已核准的 policy/来源 hash 不替代其当前撤销状态。原 recipe 六态仍由[配方专题](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-lifecycle)拥有；F1 的扩展 scope 限制不把全局 recipe 写为 restricted 而误伤 F2，也不从全局 active 推导 F1 可以继续。

变更请求明确选择/组合：停止新发布/安装/激活；限制既有能力后续新效果；受控退出，并给出每项范围、顺序和同一个 canonical 生效边界。只声明“撤销”却没有既有效果决定是 incomplete，不能改当前授权。仅 stop-new-admission 的决定保留其显式允许的现存使用，不能被 UI 解释为停机；进入 restricted/exiting 的受影响 scope 同时禁止新增发布、安装、激活、扩权和新的控制权，只执行明确的最小动作。所有技术请求按完整 actual scope 检查，不能拿 F2 的合法 grant 为会影响 F1 的全局请求授权。要求收缩/退出的决定必须覆盖完整 stock 才可提交。

初始 grant 已预绑定到期处分政策，因此 `time >= end` 时普通发布/安装/激活与生产权直接为零，不等待 sweep 或通知；期限谓词只读也会返回 expired。Runtime 从完整 scope index 与原 job stamp 取得截止前已开始的有限 stock，按预批准政策生成/消费同一决定的处分记录。没有合格政策、当前原 authority 或完整 stock 证明时不产生收尾效果，显示 `exit_blocked`。旧 pending 请求在真正 effect 时重验；到期前到达队列、过去批准或缓存 plan 不锁住期限。

<a id="extension-disposition"></a>
### 5.2 六类工作、最小动作与退出

处分不追溯删除历史产物、已结合同、Agent/factory ID 或合法完成 receipt。scope 收缩前逐项列出原[六类工作](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)、数量、parent/job 身份、资源/终端/许可义务、现状、允许动作及终态条件；空集须由同一 revision 的完整索引证明。`closed` 表示该范围没有未闭合工作、受保护占用或终端义务，不要求删除已经合法归属 owner 的历史库存。

| 六类工作 | 本 profile 可执行的明确处分 | 不可推导的能力与闭合条件 |
| --- | --- | --- |
| 新排程 | ordinary grant、时间和全部实际检查成立才可开始；受限或过期范围拒绝 | 不把旧 plan/新 ID 变成开始权；拒绝不扣生产料、电力或新增 job |
| accepted、未首次 sink | 取消未执行意图；一次释放其真实未消费 hold 和槽；保留决定与因果记录 | 本 profile 的正式 start 原子建立 job 与 sink，不能拆出可执行半状态；已消费部分不返还 |
| WIP | 明确列出的原 job 在有限窗口内一次完成原 plan；否则保持原 job/消耗及 blocker | 不增加批数、改产物/目的地、不重跑 recipe planner、不借收尾授予新收益资格；没有 rework/salvage/退款 adapter |
| transit | 本 profile 不接受此类 stock；其存在即 scope 不符合首版处置能力 | 不假定在途为空，不调用通用 return/reroute；不得先切授权再等待实现 |
| buffer | 只支持已批准本地原输出 ledger 的有限待入账结果，随原完成原子入账 | 无中间运输、换 owner、转换或新用途；已完成并归属 owner 的库存是历史资产，不是待处分 stock |
| terminal-pending | 原 validator、原 owner/ledger、有限容量与原义务均成立时完成一次 terminal receipt | 原输出入账不冒充外部交付；若存在额外合同/版税/交付义务则不符合首版 normal，不能略去 |

最小处置 window 固定为 `[first_applicable_cutoff, min(first_applicable_cutoff+4, earlier_original_deadline))`，单位为 World tick；原 job 的第一次适用 cutoff、允许剩余批次/输出和 deadline 一经权威记录不可由重复决定、换请求 ID、重启或换 grant 标签重置。若区间为空则无可执行窗口。初始到期政策明确允许的原完成可以发生在普通 grant 到期之后，但必须继续满足当前原治理/工业/资产/验证 authority，只处理 cutoff 前的原 job 和已 sink 数量；这是有限处分，不是普通生产宽限。

每 job 至多消费一次原 planner 和一次 pinned iron validator attempt；completion 仅使用已提交 plan 和精确验证结果，最多两 job 的 closure 占用及输出上限见[预算](#extension-budget)。deadline、调用或预算耗尽，目的地/依赖/authority 失效，或旧 attempt 结果未知时转 `exit_blocked`，保留原有限量、消耗、占用来源及未结责任，不追加生产、不凭清理产生 closed/retired。独立合法的新处分必须重新取得正向决定、旧责任全量引用和有界可行预算；它不能自动续期旧决定、重跑未知 attempt 或复活普通权。

现有 `PauseFactoryProduction` 要求没有 active job，不能用它实现 WIP pause。这里的 blocked hold 是保留已有事实及拒绝新效果，不宣称已有物理暂停、返料、销毁或断电适配器。用户可查原结果、补回缺失证据/合法目的地、请求独立可行处置或等待；无合法出口时不得显示“已安全停止”。

<a id="extension-atomic-replacement"></a>
### 5.3 单一重叠授权与正常时序

替代只支持本 profile 的同 exact recipe/artifact 授权交接。对变更涉及的每个 selector，Runtime 读取当前 owner/revision、完整 stock、未确认请求和相关 authority；旧范围的待移交 stock/义务未闭合则返回 `replacement_blocked`，新 grant 保持 pending。不能把旧 grant 改名为 drain、同时把同范围的新生产权交给 successor。

stock 闭合后，一次 canonical CAS 同时收缩旧 grant scope、更新旧 scope revision、拒绝或标记旧未确认请求须显式重提、激活新 grant、更新唯一 selector/index、写 receipt。新 grant 的 `[begin,end)`、当前来源和资源资格仍须成立；失败保持完整旧状态，未知提交先查询原 intent。F2 的 selector 与业务权限保持，虽然共享 G0 记录因收缩而进入写集；重试不能重新生成一个新 grant。历史 job stamp 中的 grant revision 固定其接受来源；当前准入另核该 factory 的 selector/scope revision 和当前 grant 保留范围。若 G0 全局记录因删 F1 而升版，已开始的 F2 job 沿明确 before/after receipt 证明 F2 scope/binding 未变，再合取当前来源/时间/资产条件，不因全局 revision 不等而误拒，也不修改旧 stamp。真正影响 F2 或原 authority 的变化仍使其当前检查失败。技术 module 可以一直 active 服务两个非重叠 scope；这不是安装两个新实例，也不是靠全局 deactivate 实现 F1 收缩。

normal 是待实现的静态 fixture，真实前置包含四阶段、PPE 发布/激活、授权、精确 validator、各 owner 与预算；缺任何一项只能记 blocked，不能把下表当已有证据。

| canonical 时界/顺序 | 当前授权与动作 | 可复算结果 |
| --- | --- | --- |
| 初态 | `G0=[10,30)` 覆盖 F1/F2；同 exact recipe/artifact；G1 pending，拟 `[20,40)` 接 F1 | F1 输入 8 ore/2 fuel，F2 输入 4/1；三个实际 payer/用途按 §9.2 固定 |
| tick 18，先 start 再 restriction | G0@F1 开始一批 J1；同 tick 后续决定关闭 F1 新排程并列出 J1 一次 finish，window `[18,22)` | J1 已 sink 4/1 与生产 8E，原 plan/validator/ledger 不变；F2 的 G0 权限仍在 |
| tick 19 | J1 完成验证并原子入原 F1 output ledger；原 stock/义务关闭 | F1 产物 3 iron/1 slag；这些合法库存继续存在，不妨碍替代 |
| tick 20，先 CAS 再两个 start | 单 CAS 把 F1 交给 G1，G0 只保留 F2；随后 J2 在 F2/G0、J3 在 F1/G1 各开始一批 | 每厂仍仅一未闭合 job；J2/J3 不继承 J1 身份、旧授权缓存或已消费投入 |
| tick 21 | J2/J3 各一次 pinned validator 与原完成 | 总消耗 12 ore/3 fuel，产出 9 iron/3 slag，生产 24E；每项效果/费用只一次 |

如果 tick 20 前 J1 尚未完成、只收到 pending 验证通知或仍有未结义务，替代必须 blocked。另一个候选争抢 F1、跨 domain claim、旧新请求同 tick 或到期边界，统一按 canonical 顺序与 expected revision 决定，一个 winner 或全拒绝；不让到达墙钟决定权利。F2 后续到 tick 30 关闭普通权，G1 不受 G0/F2 到期牵连。

<a id="extension-consumers"></a>
### 5.4 实际消费者接缝

下列是一组共同的实施条件，不能仅实现一个新 API 后宣称本 profile 可用。守卫在 expensive module call 前拒绝显然失效的调用，在最终效果准备/提交时再次消费同一 binding 与当前 scope。

| 现有入口/消费者 | 必须新增或连接的检查与传播 | 不能接受的旁路 |
| --- | --- | --- |
| [release request/Shadow](../../../crates/oasis7_runtime/src/world/module_actions/release_request_support.rs)、[Apply](../../../crates/oasis7_runtime/src/world/module_actions/release_support.rs)、[release tail](../../../crates/oasis7_runtime/src/world/module_release_publication.rs) | 发布 payload commitment 关联 exact binding；真正 Apply 重验 current grant/owner/roles、处分和 freeze，绑定与 Profile 同根发布 | 旧 approved/Shadow 或 append-only 新 ID 自动重获资格 |
| [direct install/upgrade](../../../crates/oasis7_runtime/src/world/module_actions/artifact_actions.rs)、[ModuleChangeSet](../../../crates/oasis7_runtime/src/world/base_layer.rs)、[rollback TO](../../../crates/oasis7_runtime/src/world/module_actions/rollback_actions.rs) | register、install、activate、upgrade、rollback 激活都重验当前 publication/activation 资格，精确 freeze 继续合取 | 避开 release 流程、旧 module bytes 或 direct action 绕过当前资格 |
| [ScheduleRecipeWithModule](../../../crates/oasis7_runtime/src/world/economy.rs) | 解析指定 manifest 的精确身份、payer、当前 scope 与时间后执行；输出核 Pure/plan/数量，把 producer/plan/grant stamp 传给 resolved action | `module_id` 找到最新 active 版本就运行；映射为普通 action 时丢失授权出处 |
| [普通 ScheduleRecipe](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)、[RecipeStarted](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs) | direct/native/module 统一区分受信 builtin 与 extension；验证 plan stamp、current scope、真实 factory/owner/ledger/power/capacity，并原子建立 job stamp/hold/sink | caller 自造 plan、遗漏 stamp、raw 新事件、重命名 recipe 或 native 标签 |
| [native industrial start](../../../crates/oasis7_world_state/src/world_rules/industrial_work_start.rs)、[ScheduleEssentialRecipe](../../../crates/oasis7_runtime/src/world/event_processing.rs) | 保持当前只支持 builtin 的正向边界，extension binding 不可塞入；将来支持时须走相同 scoped guard | 临时改成 native 或紧急动作获得 extension 开始权 |
| [with/no modules due](../../../crates/oasis7_runtime/src/world/economy.rs)、[industrial guard](../../../crates/oasis7_runtime/src/world/industrial_completion.rs)、[RecipeCompleted](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs) | 两路使用持久 job stamp、current disposition、原 terminal、精确产品依赖及剩余预算；最终 reducer 重验入账权限与原 payload | no-modules 跳过 pin validator、旧 ready_at 或全局 active 代表可完成 |
| [产品验证 attempt/receipt](../../../crates/oasis7_world_state/src/state/industry_state.rs) 与[恢复消费者](../../../crates/oasis7_runtime/src/world/economy.rs) | job/output ordinal 绑定 exact validator manifest/artifact/activation、请求和结果；旧 decision 只作事实，提交后继效果仍验当前 authority | 找不到 validator 就 continue、选最新 `.product.*`、重跑未知 attempt、重连再收费 |
| [module command/tick](../../../crates/oasis7_runtime/src/world/module_runtime.rs)、[效果路由](../../../crates/oasis7_runtime/src/world/module_routing_runtime.rs) | 本 Pure profile 不接受自主 tick、通用 effect、额外 hook 或 command 发起生产；实际命中 recipe 仍落同一最终 guard | 通过 intent/effect/原生适配器跳过 schedule 检查 |
| [artifact 销毁/转让](../../../crates/oasis7_runtime/src/world/module_actions/release_actions.rs)、依赖读面与恢复 | 未完 job 引用的精确 bytes/schema、原收费同意和当前 owner 必须可用；关联变更纳入依赖校验/保留 | deactivate 后销毁 validator、缺 owner 免费运行、较新 cache 补旧来源 |

## 6. 接口与数据合同

<a id="extension-interfaces"></a>
以下是目标逻辑合同，不引入第二套共识或把新外层 API 当执行 authority。世界、分支、所属实例/专业 owner、对象类型、local ID/revision/digest 均显式；集合按唯一稳定键排序，数量为 checked 非负整数。schema、必需 facet 或 authority 未知即拒绝，不对 ID 做大小写或 trim 合并。

| 目标记录/逻辑接口 | producer → consumer 与必需内容 | 结果、幂等与失败语义 |
| --- | --- | --- |
| `RecipeBehaviorBindingV1` | 原 creation/发布 authority → lifecycle 与所有 recipe 消费者；§4.1 全部来源、slot、精确 recipe/module/validator/许可 refs | 不可覆盖；新 revision 重走其原准入；同字符串不同来源冲突 |
| `ExtensionGrantV1` | 当前合法 governance → Runtime；主体/用途、binding、显式 factories、时间、effect classes、source revision、预绑定 expiry/disposition policy、资源/依赖同意 | pending 无执行权；effect 必须解析最新 revision，旧 grant receipt 只证明历史 |
| `ResolveExtensionEffect` | actual consumer → 当前原 authority 与 scope selector；job/intent、effect kind、factory、time、expected revision、原 scope/plan refs | 返回当前 allowed/restricted/expired/revoked/conflict/unknown 及确定原因；不接受缺项为 allowed |
| `ExtensionDispositionPlanV1` | governance + 工业 disposition owner → Runtime；完整六类 stock、原 job/数量/ledger/义务、first cutoff、deadline、允许一次动作、payer/保留预算、closure 条件 | 任何缺失/unsupported stock 均不提交半份；到期政策、主动收缩与未来替代共享原责任，不重置截止 |
| `ExtensionRecipeStampV1` | 受信 runtime prepare → 原 RecipeStarted/job/完成；binding、slot/grant revision、source activation、plan/request digest、原 job/资源/输出/validator refs | 不是 caller 可自行填写的签名标签；final reducer 重验，重复使用于另一 job 拒绝 |
| `ExtensionTransitionReceiptV1` | canonical commit → 查询/恢复；request key/payload digest、before/after revisions、旧新 scope、stock/hold/decision refs、effective position、结果 | 同 key 同 payload 返回原结果，不同 payload 冲突；未知提交查询原 key，不先新建 |
| `ExtensionScopeViewV1` | 授权投影 → Viewer/API/Agent；当前 scope/期限、来源、最小处置、旧请求结果、成本、blocker 与允许下一步 | snapshot 标注 observed revision/time；陈旧读面不得驱动新效果 |

receipt core 的 canonical commitment 排除自身摘要/签名与外层 finality wrapper，按[既有 SR2 receipt](../wasm/wasm-interface.md#sr2-types)规则区分内容地址与执行身份；wrapper 证明该 core 的 canonical inclusion。不能把本次 block hash 递归放进其自身待确认 payload。exact manifest、schema、plan、request、source revision 与 receipt 引用不能互换。

## 7. 状态、事务与持久化

<a id="extension-recovery"></a>
### 7.1 状态与原子提交

扩展 grant 的 pending/current/expired/revoked、scope 的 ordinary/restricted/exiting/exit-blocked/closed、工作状态、配方六态和模块 active 分开记录。scope selector 永远最多一个 executable owner；expired/revoked 仍可查，但不自动拥有最小处分权。最小处分必须来自原预批准且当前合法的独立效果条件，不是一份默许的常驻旧 grant。

Runtime 在 immutable base 上读取 world/authority/slot/index/grant/recipe/job/dependency/资源 revision，prepare 完整变更，再比较 expected revisions。事务写集包含适用的旧 scope 收缩、唯一 selector、新 grant、旧 pending intent 决定、精确 stock disposition、真实 hold/释放、job/产品验证/终态 refs、receipt/nonce、journal 与 durable publication。实际 sink 或完成还包含原材料、电力、输出、费用、容量与 job 的既有 prepared transition。任一校验或写点失败均不安装部分候选；已获授权的真实调用费用按下段独立结果处理，业务拒绝不倒写未发生的计算。

模块前授权/资源拒绝为零 call、零 module fee、零生产效果。允许调用时先以原 intent/job+call role+ordinal 唯一键，持久准入一次 attempt、实际 payer 同意的费用上界、必要 slot/closure/容量保留；这只是有限调用准入，不授予生产。取得可核验结果后，按真实 metering receipt 一次结算费用，再与允许的生产提交或拒绝结果原子关联；原 job 与首次 sink 仍在同一个最终生产提交。planner 返回业务拒绝或 final authority 已失效时不建生产 job、不扣生产料/电，但已发生且获准的调用费最多一次；validator 拒绝时保留原已 sink 事实与责任，实际验证费同样最多一次。撤销/过期不能取消这个原调用的已发生费用义务，也不能用费用处理权重开生产。结果未知保留原 attempt/费用 hold，先查 receipt；没有可核验计量不猜为零、不重复扣费或重调原模块。未消费的生产/closure占用按原已授权取消处置一次释放，未知或仍承担原义务的部分不擅自释放。

替代不将 stock 分批处理后提前激活；超限请求首个效果前拒绝，不能截断列表后提交。允许的正常完成先独立关闭旧 job，再用其已确认 receipt 准备替代；这两件事各有自己的当前权限与因果记录。并发 start、撤销、owner 变化、支出、结束或另一 successor 竞争相同 scope/资源 revision，只有完整一致的一个候选提交，其余重新查询或显式重提。

### 7.2 持久闭包与恢复

同一 committed generation 保存 binding 与 exact source refs、schema/manifest/artifact、scope selector/index、grant/期限/首次 cutoff、剩余动作预算、job stamp、验证 attempt/decision、资源 hold/消耗/释放、完整 transition/completion receipts 与去重索引。read-set 中来源 bytes 要么在同 generation 可达，要么引用受验证的不可变内容；较新 cache、当前 active module 或 Profile 不能修补旧 generation。

恢复先核 generation/commit/内容摘要、索引覆盖、单一 selector、source closure、job/数量/费用守恒及预算；缺失、混代、未知 schema、冲突重复或不完整 tail 拒绝相关执行，允许授权诊断历史。scope index 是权威写集的可验证成员，不能只用可能漏项的缓存；若可由 committed records 重建，离线验证重建后再开放，热路径不以全世界扫描悄悄替代缺索引。

已提交事件/receipt 按原精确输入重放，不能调用当前 recipe/validator、重新治理、重新收费、产生新 job 或再放款。尚未提交的 pending action/plan 在恢复后按当前时间、scope 和来源重验。产品 attempt 已有 decision 就读取原结果；只有 attempt 没有可确认 decision 时沿现行 fail-closed 规则保留责任，不能以恢复名义重试并增加预算。late result 的身份必须匹配原 attempt/ordinal，记录事实不自动授权已过期的完成。

prepared 但未 durable commit 的候选丢弃；durable commit 已确认而响应丢失的请求返回原 receipt；结果未知先按原 request key 查 authoritative outcome。恢复与重启不改变 `first_applicable_cutoff`、deadline、pending successor、scope revision 或已用 call count。当前底座的 store/checkpoint/schema 适配须另行实现和验收；未合入 #5439 的增量存储工作不是本合同已成立的前提或证据。

## 8. 部署、安全与运行约束

profile 只有在全部消费者、原生 lifecycle adapter、来源/hold/容量/persistence reader 都支持同一版本时才能启用。部署不能只打开 release UI 或某个 API；旧 writer/reader 未识别新 schema 时明确 unavailable。签发、变更、替代由当前原 governance authority；工业 disposition owner 验实际可行性；factory/ledger owner 与实际 payer 分别同意。任何角色只有自己的权限，PPE owner、required roles 或 creator 无权为别人资金、许可、目的地续授。

本 profile 的 recipe Pure、零 effects、一次 emit、无自主 tick/new_state。输出计量使用执行器返回的真实完整 CBOR 字节数；不得信任模板 `output_bytes=512` 常量。[执行器](../../../crates/oasis7_wasm_executor/src/lib.rs)与[输出处理](../../../crates/oasis7_runtime/src/world/module_runtime.rs)的现有检查继续适用，新增授权检查不减少 sandbox、manifest、capability 或原资产守卫。

每次开始前，原 owner 明确同意保留该 job 完成所需的 exact validator/source/schema 及 closure 费用到固定期限。artifact active 检查不足以证明未完 job 的依赖安全。artifact ownership revision、当前 payer 身份、资源账户 revision、hold owner/purpose 和收费同意全部进入 prepare/commit 的 read-set。关联 owner transfer、销毁、activation 替换必须同时重验引用与 payer；本首版不支持未完 job 的隐式 payer/validator 迁移，故影响该闭包的变更拒绝或先走有权关闭。已经发生来源漂移则 completion blocked：不从新 owner 无同意扣款，不假装旧 hold 可支付当前第三方账户，不重复释放旧 hold；任何独立合法重准备引用原 attempt/hold/费用 receipt，不能借新 grant 重收。没有未完 stock 时可按原市场/发布规则变更，后续使用重新正向确认新 payer/来源。它不没收 artifact、不冻结无关实例或改变已结历史。

current artifact owner/Agent 缺失时，本 profile 明确失败，不使用现有 metering 的免收费分支。移除扩展文件、换 module ID、恢复旧 snapshot、直接提交 raw Profile/事件或从已失效来源复制同一 plan 都不能创建合法 stamp。可见性遵循原 authority：公开 scope/当前原因和允许下一步，原始签名、私有源包、资产细目仅向有权 reader 提供。

## 9. 质量与容量

<a id="extension-budget"></a>
### 9.1 首版有界合同

以下为 `governed_recipe_factory_v1` 的工程初始上限，不是实测性能或放大权限的依据；exact manifest/输入/依赖/事务若不能满足，则这个候选不能启用。更大范围必须独立设计，不把超限 stock 拆成不完整处分。

| 项目 | 上限/确定规则 | 组合要求 |
| --- | --- | --- |
| 能力与工厂 | 1 exact Pure recipe/artifact；最多 2 显式 factory；每 factory 最多 1 未闭合工作占用；每 job≤2 batches | 计数含已准入但未sink的planner attempt/slot/closure hold与正式job，不能只数pending_recipe_jobs；所有grant共用索引，换grant不重置 |
| 处分 | 首次 cutoff 起至多 4 World ticks，且服从更早原截止；最多 2 原 job/4 批 | 只剩余原完成，无新 planner/new sink；deadline/次数不因重试重置 |
| 待决与变更 | 单 scope 最多 1 pending successor；一次变更最多 4 已接受未执行 intents | 仅排队intent不占资源/slot；准入call前须原子取得每厂唯一占用，超额为0call/0fee；旧pending在边界确定拒绝/需重提 |
| recipe/validator call | 完整 canonical input≤4096 bytes；`max_output_bytes=1024`；`max_effects=0`、`max_emits=1`、Pure/no new_state | 每 job 1 planner + 1 pinned iron validator attempt；slag 须原 authority 明示 `not_required`；无额外 controller/hook WASM |
| 执行限额 | 每 call `max_gas=1,000,000`、`max_mem_bytes=16 MiB`、每模块每 tick `max_call_rate=2` | normal 的 planner/validator 分别在 tick18/19各1次、tick20/21各2次；额外调用使本例不成立 |
| 读取与写入 | 单原生控制/完成事务最多 128 个逻辑 authority/状态记录读项、128 个逻辑写项；request≤16 KiB，完整引用/proof payload≤128 KiB，journal/receipt新增量≤128 KiB | 所有嵌套来源、索引membership/范围证明、attempt、holds、负面证据均计入，bytes/proof超限首效果前拒绝 |
| 资源与容量 | 每 job 真实生产料、电力，加全部调用最坏费用；输出至少 6 iron/2 slag 的本地有限容量 | actual batch=1时只占3/1；已有余额与别的待完成输出同算，不借物料stack_limit冒充空闲容量 |

现行 economy request 使用 `material_stacks_by_ledger()`，due/工业容量也有 pending 集扫描；仅限制两厂不能声称这些现有实现已满足上表。目标 adapter 从 exact recipe 的两个输入物料、原 factory ledger、所需产品/authority 与 scoped job 索引取得有限 read-set，完整序列化后才校验 input cap；不得把全世界 ledger 偷带进调用。集合规模在 decode/分配前设上限，canonical 解析、proof读取与事务写入都计数；缺完整有界读取或可验证索引则拒绝。新 native 控制操作是有限确定性计算，不隐含一个未计费 WASM controller。

按[当前费用计算](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)，令 `u(x)=ceil(x/1024)`。合法单emit、零effect、无new_state的返回形态为 `Data=u(I)+u(O)+1`、`Electricity=2`，I/O 是实际完整 CBOR 字节数；这是normal六次调用的预期形态，不是所有失败的收费公式。原通式为 `Data=u(I)+u(O)+2×effect_count+emit_count`、`Electricity=1+effect_count+emit_count+has_new_state`，只按可验证的原计量receipt和已同意失败政策收费。零emit、trap、超限/非法输出或未知结果不能套normal；缺计量不补造、费用不越原同意上界、unknown不重调。当前preflight对new_state仍留一个单位，所以在上表limits下最坏为 `Data≤4+1+1=6`、`Electricity≤1+0+1+1=3`。必须在sandbox前确认这个上界与当前合法payer，再按真实receipt扣费；不把512当实际O、不把上界当真实消耗，也不把native recipe的零模块费照搬给extension。

每次start的call准入原子保护一次validator的closure reserve `6 Data + 3E`及原输出容量；计数包括尚未sink的已准入attempt及其未决hold，全部两厂合计至多两份并发closure，即 `12 Data + 6E`。正式job替换同一占用，不新增第二份；仅排队的最多四intent不能先各收一份closure。该reserve包含在每job两次调用的总承诺里，不是另外收费。recipe planner的payer、validator的payer与生产电力owner分账；每一方独立足额、同意且当前合法。复用原[受保护资源合同](checkpoint-power-service-fulfillment.design.md#service-resource-holds)：所有可冲突扣减入口、模块费用、转移和新生产都不能花掉closure hold；该全路径适配尚是依赖，不能只加本地余额检查后声称安全退出已有保障。

普通生产资源/剩余预算不足时可缩小未来请求或停止；不能消费 closure reserve 赌后续补款。新的 start 没有完整来源、预检预算或真实输出容量就不运行 planner/不 sink。实际出现额外 validator、政策 hook、控制器或收费项时，必须先列完整调用图并取得新有界同意；本 profile 的未知调用不是免费、不是 `not_required`，不能沿用下例数字。

### 9.2 normal 的来源、费用与守恒

唯一行为源沿 [PPE §8.3 交接例](../module/player-published-entities.design.md#ppe-creator-example)：现有[铁锭源码](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/src/lib.rs)、[共享 recipe 模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_recipe_module_template.rs)和其 [source input/Cargo 声明](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/Cargo.toml)。制作候选把 module/recipe 分别改成 `example.m4.recipe.smelter.iron_ingot.v1`、`example.recipe.smelter.iron_ingot.v1`，其真实 source paths/includes/SDK lock、build identity、manifest、WASM hash 与 Profile 必须一致。参数保持 4 ore/1 fuel→3 iron/1 slag、生产8E、计划1tick；extension plan 实际输出须与所接受批数下这些输入/产物/电力/时长精确匹配。F1/F2只是文档简称，真实完整factory ID满足template的smelter marker，但 marker 不证明工厂权限。示例是未制作的身份说明，不是当前注册 artifact；normal G0/G1 只替代授权，始终引用同一真实候选。

铁锭 validator 固定真实 [`m4.product.material.iron_ingot`](../../../crates/oasis7_builtin_wasm_modules/m4_product_material_iron_ingot/src/lib.rs)及[产品模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_product_module_template.rs)对应的 exact manifest/version/hash/activation 和 payer V，其真实 limits/输出必须符合 §9.1。slag 由原产品 authority 明确不需额外 validator，不能用当前 resolver 查无 module 的 continue 分支证明。实际 snapshot 的铁锭 [maintenance sink](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs)必须确认为空，stage/factory/material/product及原普通权限均有效；否则输入/费用重算，不能使用本算例。原输出 ledger 需容纳 F1 累计6 iron/2 slag、F2累计3/1，且 no external delivery/royalty/其他 terminal 义务有正向证据。

| 账户/用途 | start 前完整额度或来源 | 三个 job 的实际结果 | 未消耗部分 |
| --- | --- | --- | --- |
| 工厂 owner B 的生产 | F1输入8 ore/2 fuel，F2输入4/1；生产24E，适用原工厂/账本授权 | J1/J2/J3各一批，总12/3→9/3，生产24E | 不退已 sink 投入；产物按原ledger归属 |
| planner artifact owner P | 3次×上界`6 Data+3E`=`18 Data+9E`，每次当前payer检查 | 3次调用，共6E；Data为3次`u(I)+u(O)+1`之和 | 正常完成后E余3；Data余`18-实际planner Data`，不得负值 |
| validator artifact owner V | 3次×上界`6 Data+3E`=`18 Data+9E`，按job保护closure部分 | 3次调用，共6E；Data为3次`u(I)+u(O)+1`之和 | 正常完成后E余3；Data余`18-实际validator Data`；原hold释放一次 |

完整模块费用预检为 `36 Data+18E`，加生产为总 `42E` 的分账户可用承诺；正常真实 module E=12，因此总实际 E=36，释放未消费费用余量6E。真实总 Data 保留六次实际 I/O 公式，不猜字节数。原收费目的账按现有 schedule 记录六次合计 Data 与12E，生产24E仍是独立sink；payer扣减、目的账增加和费用receipt一起校验，不能只看个人余额下降。P/V/B 在例中是各自可授权、足额的 payer，不通过三账户总和掩盖其中某个不足；artifact owner 不等于工厂 builder。native governance/closure adapter没有额外 WASM调用，normal 没有隐含 hook、收费许可或外部控制费；正式环境存在这些项即不符合此normal前置。

J1 在tick18开始并在19闭合；tick20 J2/J3各开始，tick21各闭合。因此 planner/validator 分别最多同tick两次；并发closure始终≤2份，而三次累计调用预算仍全计。若 V 只能支付 planner 之外的一次验证或输出容量不足，下一 start 不能开始；已经保留的 J1 closure 不被新工作挪用。重复请求、断线、重放、换grant和到期不增加材料、输出、剩余call或可退款量。

## 10. 兼容、迁移与回滚

本 profile 不从旧无 binding/无 stamp 的 recipe/job 自动补治理权。已提交旧历史按原 record 可读/重放；旧 pending 或未闭合 extension job 缺可信 binding/stamp 时保持 blocked，不能凭当前 module ID、registry、同名 Profile 或缺字段猜成 native 后继续。启用前须由完整旧来源证据和原 authority 支持显式迁移/绑定适配，或保持该工作 blocked 等待原合法处置；本首版不提供自动迁移，也不把未支持的迁移当启用前置已完成。正向确认的原合法 builtin 仍沿自己的 identity/plan/authority 执行，不因新 guard 全局失效。

PPE `rollback_stop_new_admission` 继续是精确 artifact 的新准入硬门，[既有 rollback](../module/player-published-entities.design.md#ppe-rollback-disposition)不等于本篇最小退出。rollback TO 也检查当前资格；不同 artifact/recipe、activation 或 schema 的 successor 需原四阶段/兼容/授权，不能只复用 G1 的名字。global active、其他实例和 F2 的合法 scope 不因 F1 决定隐式改变。

回退部署不能读取不懂的 grant/stamp 后按旧开放路径继续；先保留完整 committed generation 与可用 reader，停用新的不受支持执行，继续提供历史查询。已 committed 的授权替代和合法生产不倒写未发生；修正只能另有当前合法决定与关联 receipt。未提交候选可以丢弃，不能回滚真实已确认经济效果或重置 deadline。

## 11. 验证策略

本轮检查文档关系、源合同、字段一致、静态算例与方法可执行性；运行证据仍需未来同一真实候选完成。实现验收须对 full consumer closure 做正常和负例、状态/费用差值、失败点与恢复验证，不能以 mock grant、单个入口或仅 presence/grep 通过代签。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [AC-GAME-018-07](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-07) | [完整身份/范围](#extension-identity) | 当前原authority证明完整grant、处分、行为与稳定范围，缺项不扩权 | [GEL-M01](../../testing/manual/governed-extension-lifecycle.manual.md#gel-m01)：源合同审读及test_tier_required target producer/consumer正负例；使用手册§1固定真实候选与前态 | 原治理来源、binding、exact artifact/Profile、scope index；缺字段/别名/跨domain冲突零新效果 | 方法设计；不代签任意扩展 |
| [AC-GAME-018-08](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-08) | [发布/激活接缝](#extension-consumers) | 真实publication/install/activation提交消费当前完整资格和精确freeze | [GEL-M02](../../testing/manual/governed-extension-lifecycle.manual.md#gel-m02)：test_tier_required逐一正式直接入口及最终publication；固定实际候选的请求/身份/权限前态 | release/direct install/upgrade/register/activate/rollback TO在当前资格失效时拒绝；freeze保留 | 未运行，不从PPE已实现推导整体完成 |
| [AC-GAME-018-09](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-09) | [实际effect/时间](#extension-current-guard) | 所有实际效果入口遵守同一范围和期限；当前active/旧plan不续授 | [GEL-M04](../../testing/manual/governed-extension-lifecycle.manual.md#gel-m04)：test_tier_required direct/module/native/due/final reducer及同tick边界；固定同世界同候选 | `[begin,end)`两侧、旧plan、raw事件、no-modules、缺validator均有当前判定；新sink非法时零效果 | 未运行；普通active不是证据 |
| [AC-GAME-018-10](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-10) | [六类处分/硬截止](#extension-disposition) | 原stock在当前有限权下处分，硬截止不重置，合法历史与未结责任保留 | [GEL-M05](../../testing/manual/governed-extension-lifecycle.manual.md#gel-m05)：test_tier_required有限处置、test_tier_full完整存量组合；按方法取得可达原job/原义务前态 | 完整stock/明确空集、首次cutoff、4tick窗口、原数量/ledger/预算；无出口保责任且不伪造closed | 不代签return/reroute等unsupported能力 |
| [AC-GAME-018-11](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-11) | [原子替代normal](#extension-atomic-replacement) | 重叠scope只一个executable owner，待决无权，F1原子交接且F2原合法工作继续 | [GEL-M03](../../testing/manual/governed-extension-lifecycle.manual.md#gel-m03)：test_tier_required同候选顺序normal；同手册GEL-M07补test_tier_full并发/同tick与F2已有未闭合job控制组 | t18–21三个job及G0/G1 receipt、每scope唯一owner、F2合法连续、未闭合替代blocked；另以切换前已开始F2 job验证旧stamp保留且当前合法完成 | 静态normal，未执行 |
| [AC-GAME-018-12](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-12) | [完整预算](#extension-budget) | 全stock、调用、分payer资源和输出容量有界；新工作不挪用closure预算 | [GEL-M06](../../testing/manual/governed-extension-lifecycle.manual.md#gel-m06)：test_tier_required上限/+1、分payer/closure，test_tier_full真实manifest/容量；同手册GEL-M03提供独立normal总账 | 六次真实I/O/fuel/费用、各owner、生产与hold、输出、scope read/write计数；不得把512当实测 | 工程初值非性能测试 |
| [AC-GAME-018-13](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-13) | [事务/恢复](#extension-recovery) | 并发、重复、未知提交与恢复保持原结果、单次效果和不可重置期限 | [GEL-M07](../../testing/manual/governed-extension-lifecycle.manual.md#gel-m07)：test_tier_required重复/冲突、test_tier_full各写点故障及generation/tail/重启；固定同一真实存储候选 | before/after roots、原key结果、完整source/stock/holds、无重复收费/产物/续期；unknown先查询 | 不以未合入storage或mock恢复代签 |
| [AC-GAME-018-14](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-14) | [统一读面](#extension-read) | 同一authority投影解释scope、期限、旧责任、成本、blocked和合法下一步 | [GEL-M08](../../testing/manual/governed-extension-lifecycle.manual.md#gel-m08)：test_tier_full同候选Viewer/pure API/Agent parity与玩家解释；业务前态沿同手册§1 | 三面同scope/revision/原因/成本/下一步，pending/exit_blocked不显示ready或safe stop | 本轮未运行UI、Agent或玩家验收 |

<a id="extension-read"></a>
### 11.2 玩家与消费者读面

授权读面显示旧/当前/候选 grant、exact recipe/工件来源、当前 factory 集和时间、限制原因/权威 receipt、普通权与最小处置的区别、原 job/义务、不可重置的deadline、已用/保留费用、待决请求结果和允许下一步。G1 pending 显示“尚无执行权”；F1关闭new而J1可finish显示“仅该原job有限收尾”；`exit_blocked` 显示具体缺失条件及原责任，不能显示已安全停机。

发布目录、模块 active 图标、工厂生产按钮和Agent建议均消费相同projection；缓存显示observed revision/time，真正操作仍重验。读面可建议补证、缩小未来范围、查询原结果、明确重提、等旧工作关闭后替代或选择独立合法builtin路线；不能把建议自动改成新授权/新请求。未知权限、持久化或最终提交结果按unknown展示，而非“可重试即会成功”。

## 12. 决策、权衡与风险

首版选择单recipe/artifact、有限工厂、本地完成和先闭合后替代，换取可列全的工作/费用/权限闭包。代价是不能在旧WIP未结时无缝迁移，也不能把通用六类动作词当成已有能力；真实玩家仍能保留F2生产、在F1合法收尾后换授权，或在失败原因可见时停止扩张。

稳定slot而非版本决定重叠，使换ID/domain不能逃避限制；代价是原authority必须提供跨domain一致的能力来源与可验证索引。四tick处置、一次validator attempt和预留预算限制新风险；它不保证在所有合法目的地/authority同时失效时强制完工。`exit_blocked`保留可核查责任是必要结果，不能用无限宽限或伪退款隐藏。

实现顺序必须闭合：原来源/schema与selector producer → 有界当前解析 → publication/activation与recipe双重守卫 → 所有due/最终完成 → 费用/依赖保护 → durable generation与恢复 → 三面读面和同候选手册。任一关键消费者不支持时，profile保持不可用。未覆盖的物流、转换、不同artifact迁移、收费许可或更大scope，继续由原专题和后续独立组合设计承接；本文件不改变其产品状态，也不宣称#5409整体完成。
