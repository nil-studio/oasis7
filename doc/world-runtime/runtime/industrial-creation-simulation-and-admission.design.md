# 工业创作、确定性模拟与受限准入系统设计

- 设计 ID：`DES-WR-IC`；生命周期：`active`，目标技术合同，不是当前实现或发布声明。
- Owner role：`runtime_engineer`；`gameplay_designer` 负责试验目的和评价规则，`wasm_platform_engineer` 负责隔离与确定性，治理/P2P owner 负责既有授权及权威顺序，`qa_engineer` 负责独立验收。
- 固定审读基线：`nil-studio/oasis7@ad2bf1bb9e29d315ea70b64603b57c206da1be0f`，2026-11-04。相对源码链接指该基线的组件位置；后续实现和测试候选另记实际版本。
- 配套方法：[工业创作验收手册](../../testing/manual/industrial-creation-simulation-and-admission.manual.md)。

## 1. 问题、目标与非目标

玩家提出新配方、材料、设施、模块或等价工业能力，需要知道假设能否复现、试验实际损失多少、准入究竟允许什么。技术模块发布能够承诺代码和 Profile，却不能代替工业模拟、有限试点及当前有范围的治理准入。本设计把这四阶段连接为可审计、可恢复的执行合同，防止一次构建、投票或成功试验提前取得正式使用权。

成功条件是：同一不可变创作修订经过可复核的四阶段；模拟没有世界经济效果；试点只能在限定资源、期限和作用域内产生证据；正式消费者只有在完整证据、当前准入和其原有权限共同成立时才能开始合法工作。缺项、失败、停止、到期和重连均保留真实投入、损失、剩余义务及下一步。

本设计承接 GR-009。作者源包继续由 PPE 拥有，普通发布仍走 PPE，配方六状态和存量工作仍走工业工作图合同。许可/版税由相邻专题拥有，本文不定义费率、垄断权、OC 资格、全球环境模型或跨世界事务，不新增 native 泛制度动作、第二套审批服务或管理员世界写入口。

## 2. 上游约束与相关角色

玩法依据为 [GAME-018 AC-GAME-018-01](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-01) 及其 §3：继续、补证、缩小范围、延期、停止、既有路线和重提必须是真实选择。工业提案不默认继承区域 charter 的 OC/本地贡献双资格；只有原事项与 scope 确实适用时才消费该资格。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | 审计来源、假设、影响、范围、风险、退出及修订，提案不直接授世界能力。 | [提案](#creation-proposal) | GAME-018；PPE 作者交付；当前主体权限 | 不代签源权利、实际构建或发布。 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | 同一权威输入下比较模拟，输入可重现且不扣资源、不创能力。 | [确定性模拟](#creation-simulation) | Runtime canonical 输入；WASM 精确工件和隔离 | 不把 Shadow、离线算例或构建一致当工业模拟实证。 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | 试点有明确 scope、有限窗口、累计预算、退出和失败记录。 | [受限试点](#creation-pilot) | 原资源/工作图/外部性消费者；当前试点权限 | 不授正式生产、商业交付或版税资格。 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | 四阶段强制顺序，有范围治理准入在所有真实消费者重新校验。 | [准入与消费](#creation-admission) | 既有治理分类、charter、配方生命周期、许可 | 技术角色、ordinary passed 和许可不独立产生准入。 |
| [AC-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | 记录身份、跨域版本及单次结果明确；不能用请求重试复制证据或能力。 | [接口](#creation-interfaces) | 现有 ABI 编码与模块命名空间；工作 Root/Child | 本文类型是目标逻辑合同，未新增现成 wire/ABI。 |
| [AC-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | 重复、并发、失联、恢复和 replay 不重复扣材、发证或准入，保留既有历史。 | [事务与恢复](#creation-recovery) | ExecutionTransaction、authority-profile commit、原工业处置 | 不把局部 prepared publication 宣称完整持久事务已实现。 |
| [AC-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | 各阶段成本、损失、scope、原因、复查和安全替代在正式读面同义。 | [读面](#creation-read) | Viewer/API/Agent；当前读取权限 | 不证明现有 UI、真实玩家理解或平衡验收。 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | 有界模拟与试点能同时满足停止、保管和恢复；超限不截断安全检查。 | [限额与可行性](#creation-limits) | 当前 ModuleLimits、资源适配器与版本化部署 profile | 工程初值和纸面算例不等于实测吞吐或经济平衡。 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | 新路径严格准入，旧历史不补造阶段，既有冻结政策不被扩展。 | [兼容](#creation-compatibility) | PPE exact artifact freeze；历史 schema/manifest | 不借新合同关闭合法旧实例或改写旧回执。 |

## 3. 当前状态、目标状态与差距

| 对象 | 固定基线的事实 | 目标与真实差距 |
| --- | --- | --- |
| 作者材料 | [PPE 输入](../module/player-published-entities.design.md#ppe-creator-inputs)、[交付包](../module/player-published-entities.design.md#ppe-creator-delivery)及离线铁锭例已定义来源、编辑对象、参数与阶段缺口。 | 复用其交接；新增可执行的证据链、模拟与试点，不重造内容台账。 |
| Shadow / Apply | [Shadow hash](../../../crates/oasis7_runtime/src/world/module_actions/release_normalization.rs) 校验 module changes/Profile commitment；[release 消费者](../../../crates/oasis7_runtime/src/world/module_actions/release_support.rs)检查当前 owner/roles 并完成发布。 | 此链不含工业模拟与试点；技术 `Applied` 不能充当四阶段结果。 |
| 工业使用 | [经济模块入口](../../../crates/oasis7_runtime/src/world/economy.rs)有 Build/Schedule/Validate 消费者，recipe 读取 factory ledger 与 builder 电力；[普通经济动作](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)另做实际排程检查。 | 原生、模块和后继执行均须接入同一 stage/purpose guard，不能只检查发布或 UI。 |
| 工作与配方 | [工作图与六态](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-lifecycle)已有完整目标合同；当前/目标差异由原专题持续维护。 | 本文只供应 creation evidence/admission 输入，不重复 #4856 工作图实现或代签在途实现。 |
| 完整链 | PPE、GAME-018 与 Viewer GR-009 均明确四阶段未完整承接。 | 本文补目标合同和方法；新的 producer、adapter、记录/恢复与消费者组合尚须实现和同候选验收。 |

本次为有界静态审读；没有执行业务测试、构建、真实试点或发布，不外推任意未检视的外部模块能力。

## 4. 边界与结构

创作者提交 intent 与可编辑材料，不自行签发成功证据。已治理的工业领域模块维护创作和审查记录，确定性评估器产生可重算结果，原资源/工作图消费者执行受限试点，现有合法治理 authority 决定 scoped admission；Runtime 检查这些事实并通过原 typed transaction 提交。记录可存于版本化模块实例状态，不能让普通 state bytes 代替资产托管、世界能力或 canonical receipt。

治理复用[事项分类与当前来源](governance-classification-and-constitutional-execution.design.md#governance-classification)。试点权限可来自已有、明确允许该 bounded operation 的 standing capability 或本事项合法决定，不强制另建一次普遍审批；它必须覆盖实际主体、资源和 scope。需要区域审查时消费[现行 charter authority](regional-charter-authority-and-recovery.design.md#charter-authority-interfaces)。required roles、当前 owner、vote 或签名分别只有原合同的效力，不组成新的万能审批位。

同一能力的技术发布、creation stage、配方生命周期、许可和实际工作各有独立状态。对 recipe，新工业准入是既有 `validated_pending_admission → active/restricted` 转换的必要输入；没有该转换，不因本文的 admission record 得到排程权。已有 `restricted/retiring/retired`、外部性、资产保护及当前控制权限继续合取适用。

所有权威判断遵循[共同 canonical 输入规则](../design.md#runtime-gameplay-authority-designs)。网络资料、LLM、来源附件和客户端 receipt 只是待核实材料；可影响结果的外部证据必须先形成有权来源、内容摘要和版本绑定，不在执行时进行即时 HTTP 查询或模型复判。

## 5. 关键运行流程

<a id="creation-proposal"></a>
### 5.1 可审计提案与修订

1. `inspect_opportunity` 返回具体瓶颈、可用既有路线、来源新鲜度及当前权限。预览不预留资源、审查顺位、能力或奖励。
2. 当前有权的作者提交 `creation_id/revision`、完整能力身份/来源闭包、假设、拟改行为和 Profile、影响区域/用途、风险、候选失败情形、投入上限、退出/返还与替代路线。来源材料引用 PPE 包，不把一个 Cargo license 字段当工业许可。
3. 专业接收器验证字段和权限、能力因果指纹及依赖闭包。接受只形成审查记录和缺项列表；不足时返回具体缺证或范围问题。身份与提交 disposition 同事务，无法提交时不能预先发一个权威创作身份。
4. 纯文字说明更新可追加 annotation，不覆盖原字节。行为、投入产出、工厂适用性、scope、风险/退出、评价判据或依赖变化必须建立 linked revision，旧 simulation/pilot/admission 不自动迁移。仅缩小本次执行数量但不改变已验证因果内容时，可以在原上限内申请更小试点；不能通过改名称、拆请求或换 module ID 绕过同一来源/能力的限制。
5. `defer/abandon` 不删除历史；有活动试点时只能触发其停止/处置流程。没有试点和剩余义务才可关闭当前修订。重新尝试引用原失败，明确新假设或新条件，不能把重连当新实验。

<a id="creation-simulation"></a>
### 5.2 同权威输入下的确定性模拟

`SimulationInputV1` 固定可验证 committed parent、相关业务 read-set、runtime/manifest/ABI/工件、metering schedule、候选和既有基线方案、初始资源/设施/权限事实、有限场景及其有序刺激、seed 和限额。基线与候选使用同一初始闭包，只允许提案声明的候选变更；现实资源不足不能偷偷补库存。敏感假设每项单独成为具名场景，不能把多份不同世界拼成“同输入”。

有权服务端 evaluator 对每个方案每场景运行两次，比较确定性结果摘要；这两次本身就是产生 verified 结果的权威验证运行，不在其后另加一套未计费/未计 fuel 的 producer→verifier 重跑。客户端预计算不作为权威证据。必须包含常态、至少一个资源不足反例及至少一个 scope/权限/失败退出反例。若候选不是 recipe，使用其真实类型的 validate/build/operation 路径，不套用配方计划为其他能力验收。报告同时保留资源消耗、预计产物/副产物、工期/容量、调用费用预期、失败及替代结果；收益估计没有真实完成 receipt，不进入生产、需求、稳定产线或版税账本。

运行在只读 committed 输入派生的隔离 overlay 中。候选 definitions 只在本次 overlay 生效；不能注册/激活到 canonical registry、创建真实 recipe/factory/job、改变库存/容量、扣 Compute/Electricity、发送外部 effect 或产出正式工业 success receipt。仿真使用服务器执行配额，不把原会计路径的运行扣费偷偷带入世界。记录已验证模拟证据可以形成审计元数据，其提交不应被误称为整个 canonical root 完全不变；必须保持经济、权限、registry、工作、奖励和 outbox 的业务投影不变。

正式采纳结果必须验证上述有权 evaluator 的完整输入和两次运行，而非只接收客户端 `pass`、签名数量或输出 hash。3 场景×2 方案×2 次权威运行共 12 调用；若部署另设复算层，该层的调用和 fuel 必须显式纳入其完整请求预算，不能仍声称只有 12 次。多节点独立重执行按每个执行节点相同上界配置，节点数不改变逻辑结果。读取超出声明闭包、未知版本、缺工件、不同结果、超限或不支持的副作用均不形成 `verified`。wall-clock watchdog/进程故障是 `incomplete/outcome_unknown`，不作为可提交的确定性工业失败；[确定性 executor](../../../crates/oasis7_wasm_executor/src/lib.rs)已有关闭 wall-clock timeout、要求正有限 fuel/memory/output 的模式，但该构造函数本身不证明本链已接通。

模拟身份由 exact creation revision、输入摘要和 evaluator version 唯一确定。同输入重复请求读取原结果，不追加第二份正面 evidence。正式开始试点前重验相关 read-set；只变了无关世界对象可保留原输入来源，任何影响结论的资源/设施/权限/规则变化要求新的输入及模拟结果。不得要求整个世界 head 永远不动，也不得把不完整 read-set 宣称无关。

<a id="creation-pilot"></a>
### 5.3 原型/受限试点、累计预算与退出

试点与模拟不同：它可以占用并消费明确同意投入的真实资源，但结果只作为该范围内的实验事实。首版 `IC-BoundedV1` 支持有限、可列举的 trial steps；一个 creation revision 同时至多一个未关闭试点。没有真实类型消费者或有界退出适配器的能力返回 `unsupported_pilot_mode`，不能改走普通排产冒充试点。

`prepare_pilot` 只读产生准确 permit 草案：匹配的 verified simulation、当前试点 authority、所有资源/设施/custody owner 同意、scope、`[start_tick,end_tick)`、最多步骤/批次、各输入/能量/执行费用上限、不可动用的退出保留、有限输出保管容量、正常终结/暂停/失败/到期的逐项处置。原工作图的 Root/Revision/Child 和 reservation/custody 模型保存试点实际，明确 `purpose=pilot`；每个素材 lot 的剩余、消耗、损失和返还使用同一真实账本，不能建立又可被普通排产使用的一份余额。

`start_limited_pilot` 是显式确认。提交点再次验证模拟、authority、资源 owner、可用量、设施/终端容量、外部性义务和退出适配器；全体通过才原子创建 permit、试点 root、真实占用与一次 start receipt。资源请求不得超过提案/模拟/当前 authority 的交集。模拟/待审/排队从不占资源。与普通工业 contender 竞争同一原资源池，沿原分配规则；创作者、OC 持有或重试不获得优先权，失败请求不能靠换键保留或累积队列位置。

每次 step 使用稳定 `(pilot_id,step_ordinal,effect_role)` 和原工作 Child 引用，检查当前 tick、scope、actor/control、适用安全/处置状态、实际可用输入、累计使用量和退出保留。候选代码只在已批准的受限执行上下文给出 proposal；Kernel 只允许 permit 列举的材料/能量/设施及试点 custody delta。普通模块 release/install/activation 前置仍由原入口负责，不能用试点绕过注册、版本或 PPE 冻结；隔离候选求值本身不生成全局安装/激活。候选需要不支持的外部写入或未列举的义务时，在首 sink 前拒绝。

试点入口必须调用独立目的的 `ResolvePilotCandidate`，不能调用要求 formal `active` 的普通 `ResolveRecipeAdmission` 后才允许试点，否则会形成“先正式准入才可取试点证据”的循环。对新 recipe，首版允许 exact candidate version 仍为 `pending_validation/validated_pending_admission` 时，在 verified simulation、当前 permit、完整资源/安全/退出前置下执行受限真实 recipe planner/验证及 trial 资源适配器；它不把配方转成 active，也不经旧普通 ScheduleRecipe 偷做正式工作。build/validate 类型同样调用本类型受限 adapter，不能用纯算术假装完成设施/材料试验。试点 Root/Child 继续使用原工作身份、资源守恒和处置机制，但 `purpose=pilot` 单独验证权限和产物去向；旧 schema 不支持该目的时明确 blocked。

候选的安全验证拒绝、已生效禁止范围、撤回/retiring/retired 或必需 artifact/处置不可用时不能取得新的试验 sink；permit 不是绕过安全限制的豁免。现有 active/restricted 能力的维护变更先建立 parent-linked 新候选，不将旧版本原地降级为试点。运行中发生上述限制时只允许当前仍合法的原有安全退出/保管处置，不能重新标成 pending 继续。正式采用另按下一节全部 formal gate，试点许可与正式准入不会互相推出。

对每个 `(真实资金/资源 owner,资源单位)` 分别满足 `initial_hold = consumed + released + remaining`；同时强制 `remaining >= proposed_step_cost + exit_reserve` 和 `consumed + proposed_step_cost + exit_reserve <= total_cap` 才能继续。`proposed_step_cost` 是全部子调用、生产及必要完成的受支持上界，不是猜测的实际费用。`remaining` 是当前仍受本 permit 有效托管的数量，不能包括已返还、被其他工作占用或未经本次合法占用的余额；pause/release 后 resume 不能仅凭旧 total_cap 消费已返量，也不能暗中补扣原 owner。两式及数量求和均 checked，任何溢出拒绝。材料转换另按绑定配方的量子、单位和产率验证，不能把矿石与铁锭裸数量相加。费用预留包括当前 schedule 下每次可能调用及明确收尾成本，生产电力与 Runtime 调用费用分列；实际消费取 committed receipt。每个 step 还须证明其不可中断段和必要安全处置可在 permit 约定边界内完成，否则不开始。没有有限上界的调用、工期或关闭成本不准入。

费用 payer 沿原 Runtime authority 解析，不能假设调用者就是付款人。[当前计量入口](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)按 exact artifact owner 解析 payer，Compute 费用当前以 `ResourceKind::Data` 扣减，生产电力则来自原工业 owner；本文不新增 Compute 币种或重定向 payer。试点预算必须逐 payer 有真实同意和受权占用适配器，并让 preflight/实际扣减消费同一受保护余额；没有该适配器就 blocked，不能一面锁掉余额又让旧 preflight 读取零余额，或在 hold 外再扣一次。原路径找不到 payer 的行为不是新试点免单依据；需要当前合法收费来源，明确无费只能来自适用现行规则。

候选产物、副产物、设施/WIP 保留试点 purpose 和有限 custody。它们不得进入可交易普通库存、商业 delivery、需求结算、稳定产线奖励或版税计量；新能力获得准入后也不自动把旧试点产物洗成正式成果。首版退出只允许恢复尚未消费原投入、已支持的安全停止/有限保管/销毁或另有既存合法规则的处置；任何转成商业用途都需要独立合法转换合同，本文不授予。

主动暂停、失败、撤销、达到任一 cap 或到达 `end_tick` 时，立即禁止新试验 sink，进入 `stopping`。这不依赖清理 worker 是否已运行。主动暂停先按原处置处理已开始的不可中断段；仅在原窗口、权限、有限 custody、尚需的 hold 和退出预算均仍有效，且没有启动本次终结性返还/销毁时，才进入可恢复的 `paused`。失败、撤销、到期或已进入终结性处置的停止不能转成 `paused` 洗掉结果；它们继续有权退出至 `closed`，处置受阻则保留 `stopping` 和具体 blocker。原 permit 事先绑定的退出权限只允许无新增试验收益的保全/返还/处置，且不能覆盖撤销后已不合法的资产或安全动作；必要主体失联时由事先有权的 custody/退出接收器处理，不临时给 operator 权限。已消费和实际损失不退回，剩余原投入向原合法 ledger 单次返还；目的地不可用则保持已预留的有限保管，记录 blocker，不瞬移、不删除。

逐资源的 `remaining >= proposed_step_cost + exit_reserve` 用于批准新的试验步骤。已批准的退出动作可以消费为该动作预留的费用；每次只要求本次退出费用加尚未执行的必要退出步骤上界足额，随实际提交递减退出剩余预算，不再次为已经消费的 close 费用保留同一份量。暂停不得挪用这份剩余保留，恢复仍重验原累计量、当前余额和原窗口。

`closed` 要求所有 held/WIP/transit/buffer/terminal 义务都有明确终态并对账。`completed/failed/cancelled/expired` 是试点结果原因，`met_criteria` 是对预先冻结判据的独立核验；完成、花完预算或有输出不等于成功。失败试点保留 evidence，但不充作满足准入的正面证据。暂停恢复须原窗口、剩余累计预算、权限和全部前置仍有效；延期/增额不是 resume，必须重新提议并重验，不重置旧消耗或搬旧批准。

<a id="creation-admission"></a>
### 5.4 有范围治理准入与实际消费

`request_admission` 必须引用同一不可变 capability/creation revision 的 accepted proposal、verified simulation 和已关闭且满足预定判据的 pilot result，完整列出负面证据、实际投入/损失、剩余风险、用途/区域/主体/对象、有限有效区间、复查和撤销/处置条件。请求只进入既有合法治理事项，不占用任何世界权力或资源；缺步骤、错版本、scope 超过已验证集合均 `needs_evidence` 或拒绝，不能手动豁免为“低风险”。

现有 authority 按完整效果集合与受保护边界决定 `needs_evidence/rejected/admitted`，确认者集合从该事项当前合法来源解析，不由提案人挑选。`admitted` 必须同时满足当前来源、有效窗口、全部证据一致和支持的处置能力；准入范围是请求、证据覆盖与授权范围的交集。缺法定授权不能用 ModuleRelease required-role、技术 finality 或 ordinary `passed` 补齐。声明复查点到达而没有所需复查事实时暂停后续新准入，不默默续期；申诉/重提本身也不续期。

`IndustrialAdmissionRefV1` 证明一次 scoped 结果；当前是否可用需 `ResolveIndustrialCreationAdmission` 读取其最新 revision、时间、撤销与相关 authority。正式使用必须满足：

`matching_creation_evidence ∧ current_industrial_admission ∧ current_type_lifecycle ∧ legal_module_activation ∧ current_asset_and_execution_authority ∧ required_externality_conditions ∧ applicable_license`。

只有实际适用的项才可由其 authority 明确返回 `not_required`；缺字段、未知版本或网络缺项不能推断不适用。许可消费[许可使用合同](industrial-license-and-royalty-settlement.design.md#license-use)；pilot 不产生该合同的 paid-use receipt。首版付费计量仅正式 recipe 的成功完整批次，由许可专题绑定原 Root/Child/完成事实；其他能力的明确零费不等于无准入。

准入 guard 必须覆盖 technical release 后的实际采用，以及 native/module 的 build、schedule、validate、首次 sink、相关后继边界、tick directives 和恢复待决动作。对 recipe，它与既有 `ResolveRecipeAdmission` 合取；`restricted/retiring/retired` 不能被本设计的历史 `admitted` 覆盖。模块返回的 plan、Profile reader 或 `ModuleReleaseApplied` 均不构成执行许可。

到期、撤销和收缩按 canonical 顺序阻止受影响新工作/新 sink，已提交合法历史保持。存量工作只沿[六类工作处置](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)及原保管/合同边界逐项处理；许可、creator 或技术 operator 不能自行续授收尾权。没有安全路径时保留事实与有限义务，显示 `no_safe_creation_recovery`；不得通过换 ID/新 root 复制旧投入、成果或收益。范围外用途、新因果版本或能力扩张重新进入完整四阶段。

## 6. 接口与数据合同

<a id="creation-interfaces"></a>
所有对象全限定身份为 `(world_id, branch_id, owning_module_instance_id, object_kind, local_id)`，引用绑定 exact revision 和 content digest；嵌套引用不把 consumer 实例冒充 owner。能力身份包括类型、稳定 lineage、product/recipe/factory/module ID、适用 version、行为 artifact hash、Profile digest 和依赖集合。没有行为模块的类型显式声明适用实现和内容摘要，不填假 hash。

目标记录使用[既有类型/规范化规则](../wasm/wasm-interface.md#sr2-types)：非负整数、checked 运算、显式单位、唯一稳定键排序的集合及保序步骤。未知 schema/字段/必需 facet 拒绝；ID 不用 trim/大小写转换合并。内容摘要与 request key、world receipt、finality wrapper 分别拥有，不让包含本次结果的 block hash 递归进入自身 identity。具体模块 schema 按既有发布流程版本化，不能以日志字符串或 `serde(default)` 补出授权。

| 逻辑接口/记录 | producer → consumer 与必需内容 | 顺序、幂等、结果与兼容 |
| --- | --- | --- |
| `CreationProposalV1` | 受权作者 → 工业模块；全限定 creation/revision、source/candidate/dependency refs、scope、hypotheses、criteria、风险/退出/替代、payload digest。 | 登记/修订只形成事实；同 intent key 同内容返回原决定，不同内容冲突。旧版本不继承证据。 |
| `SimulationInputV1 / SimulationResultV1` | canonical 输入解析器/有权 evaluator → 审查/试点；parent/root/read-set、exact artifact/runtime/schedule、场景/seed/limits、baseline/candidate、结果和 verified 来源。 | 同 creation revision+input digest+evaluator version 唯一；允许查旧失败，不得将 unknown 编成 pass；采用前复核相关版本。 |
| `PilotPermitV1 / PilotResultV1` | 现有 authority+资源 owner+Runtime → 试点模块/工业消费者；simulation ref、范围/窗口、累计 cap、各资源 hold 与退出保留、criteria、work/root refs、actual/loss/custody/termination refs。 | permit start 与资源/Root 一次提交；step ordinal/effect role 稳定；关闭结果唯一，未结义务不能 closed。 |
| `ResolvePilotCandidate` | 受限类型 adapter → Runtime；exact candidate version、当前 validation/安全限制、verified simulation、permit/scope/窗口、Root/Child/purpose、真实资源和退出能力。 | 允许未正式 active 的新候选仅执行批准的 trial；拒绝 retiring/retired、禁止范围和安全失败，不转换 formal lifecycle，不复用旧普通排程绕过。 |
| `IndustrialAdmissionRefV1` | 合法治理模块经 Runtime 提交 → recipe/其他能力/许可/读面；全限定 admission/revision、exact capability、creation revision、simulation/pilot、scope、`[valid_from,expires_at)`、review/revoke/disposition、policy/authority revisions、source commit ref。 | 仅查询已提交决定；有效性动态重验，旧 ref 不可自动升级新 revision；scope/期限缺失时不可执行。 |
| `ResolveIndustrialCreationAdmission` | Runtime 同快照 authority reader → 实际消费者；exact capability、actor、purpose、operation/Child、scope、expected revisions。 | 返回 allowed + exact refs，或 blocked/rejected +原因/下一复查；只读不创建许可/资源；执行点再验证同 read-set。 |
| `IndustrialExecutionContextV1` | 原消费者 → Kernel/工作图；`purpose=pilot/formal/explicit_legacy`，对应 permit/admission/license refs、Root/Child、当前权限。 | purpose 与来源在 first sink 固定；不得把 pilot receipt 改标签；legacy 只按明确兼容边界解析。 |
| `ReadIndustrialCreation` | 权威投影 → 有权 Viewer/API/Agent；来源锚、各维度状态、actual/held/loss、原结果及可行动作。 | 分页有界；超时回 unknown/stale，不补发创建、试点或准入。 |

语义去重槽：proposal submission 用既有 authenticated intent identity；simulation 用上述不可变输入键；pilot start 用 `(creation_id,revision,pilot_attempt)`，step 用 `(pilot_id,step_ordinal,effect_role)`；admission 用 `(creation_id,revision,admission_decision_revision)`。`pilot_attempt` 只能由显式新尝试、当前条件和可用资源原子分配，重试不得自行递增；同 revision 未关闭试点阻止新 attempt。工业资源 effect 继续使用[原 IndustrialReceiptV1](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-model)的 `(root,revision,child,effect_role)`，双索引互相绑定而非形成两次效果。

错误至少为 `unsupported_schema/unsupported_pilot_mode/missing_source/missing_stage/evidence_mismatch/simulation_non_deterministic/simulation_incomplete/stale_input/scope_denied/authority_invalid/window_expired/budget_exhausted/exit_unavailable/custody_unavailable/criteria_not_met/identity_conflict/outcome_unknown`。每次保留原 identity、是否已知有结果、可公开依据、已消费/剩余义务及安全下一步。没有安全替代时使用 `no_safe_creation_recovery`，而不是创建默认能力。

## 7. 状态、事务与持久化

<a id="creation-recovery"></a>
提案为 `recorded/needs_evidence/withdrawn/superseded`；simulation 为 `pending/verified/failed/incomplete`；pilot 为 `prepared/running/paused/stopping/closed` 并单独保存结果原因和 criteria verdict；admission 为 `pending/needs_evidence/rejected/admitted` 并单独解析当前 `effective/expired/revoked/restricted/review_due/unknown`。这些维度不合并为一个 success。`prepared` permit 尚无 hold；`running` 需 committed start；`closed` 需完整处置；`effective` 需当前授权；`persisted/finalized` 仍由原 Runtime/P2P 证明。

每次接受、step、停止、返还、证据采纳或准入都使用[ExecutionTransaction disposition](../design.md#622-disposition-and-idempotency-contract)：读取不可变 base，检查 expected revisions，prepare 完整 typed delta，再一次提交。事务包含本次状态、ordinal/累计量、资源/费用/容量/custody、原工作记录、domain receipts、去重索引、journal 和 required publication。任何一项失败不能留下半个扣料、已用预算、半份证据或已发 admission。simulation 的结果登记只允许审计元数据 delta，不夹带工业效果。

同一 lot、cap、step ordinal、准入 revision 或退出处置并发时使用同一冲突边界，canonical 顺序中的胜者提交，其他请求读原结果或重验拒绝。正常完成与停止/返还不能各自消费同一份量。申请更多资源/不同 scope 必须新受权修订，不能清空已有累计量。拒绝和确定故障只记录原合同允许的稳定 disposition，不以第二笔 best-effort 审计掩盖提交失败。

丢响应先查原请求和 durable decision；`outcome_unknown` 时相关 publication fenced，不另起 attempt/root。若未知来自整个 World generation 的持久提交，则优先执行 [G1 全 writer fence](runtime-storage-footprint-governance.design.md#storage-generation-g1-design)：旧 World 的全部后续 mutator/save/依赖派发停止，核清精确候选的持久决定并对齐内存后才恢复，不能以这里只影响一个 pilot 为由继续无关写入。已确认未提交的准备态可丢弃；已提交结果即使投影或通知失败仍保留，经原 outbox 重读/交付。canonical replay 使用历史 schema/manifest、已验证有序输入和记录，不重新发送外部请求、重跑计费试点或新增 evidence。共识重执行确需计算时仍用历史精确输入，不能按当前政策重裁已确认结果。

checkpoint/generation 联合保存 source/evidence 引用、所需原工件/规则、创作修订、permit/authority 撤销与时间、root/child、held/consumed/released、custody、处置、receipt 和两类去重索引。恢复核对来源闭包、唯一引用、单位守恒、累计预算、终态义务和 root；缺一项允许诊断历史但禁止相关新效果，不用当前 Profile 补旧证据。归档须保留原内容可达性和最小去重/累计/终态证明，不能因试点关闭清零旧 attempt 或因许可开放删除历史。

## 8. 部署、安全与运行约束

模拟 worker 是不持 canonical world 写能力的有界计算接收者；试点 controller 仅持原受治理模块和 scoped capability，经 Kernel 资源适配器提交 typed delta。它们不拥有私钥集合、原始数据库写权、临时 operator 超级角色或任意外部发布能力。候选 binary/profile 的输入可视为不可信，所有输出类型、数量、费用、目标和作用域都须验证；拒绝未知 effect 或输出截断，不“尽量执行”剩余部分。

源包、私有配方、未公开假设和身份依据依当前 audience 最小披露；公开审计保留必要版本、范围、结论和 receipt，不泄露密钥、他人完整 ledger 或私密来源。模拟闭包读取本身也须授权，报告不能通过差分暴露未授权事实。没有资料读取权时显示缺项，不用全世界 material ledger 作为无条件公开输入。

先接 reader/reducer/guard、bounded evaluator、真实资源和退出适配器，再启用该 profile。任一入口没有一致支持则该新能力保持 `blocked`。停用新试点不能停止已承诺的合法退出处理；落后、失去 authority、缺工件和不可恢复输入只隔离相关工作，不冻结无关已合法活动。

<a id="creation-read"></a>
### 8.1 跨入口读面与机会成本

Viewer、pure API、Agent 对同一 read anchor 分别呈现 proposal/simulation/pilot/admission、技术 release 和实际能力状态；显示 actor 可见的范围、期限、审查来源、权限、已持有/已消费/损失/可返还/剩余保管，以及继续、缩小、补证、等待、停止、既有路线的机会成本。预计与 actual 使用不同字段及标签；未取得 receipt 不以“已节省”“已盈利”表达。

`next_recheck` 指向具体依赖变化或权威 tick，例如材料补齐、模拟重新验证、治理来源更新、试点退出完成；不能用无条件自动 retry 消费资源。无源、超时、缺入口和陈旧缓存分别保留 `incomplete/unknown/blocked/stale`。用户选择改变 scope/投入必须显式确认，Agent 只能在当前委托内提交该选择；推荐、刷新和点击查看证据没有写入权。

## 9. 质量与容量

<a id="creation-limits"></a>
### 9.1 首版有界 profile 与公平边界

首个实现以 `IC-BoundedV1` 为可复核工程初值：单请求规范化输入至多 1 MiB、32 个 evidence/dependency refs；模拟至多 8 个场景、每场景 8 个有序步骤、**全部 baseline/candidate 与两次验证运行总计至多 32 次调用、40,000,000 fuel**，单次不超过现行 `ModuleLimits` 与 10,000,000 fuel/64 MiB memory/256 KiB output 的较小者，完整报告至多 1 MiB。各约束合取：8 场景不代表每场景都可以运行 8 步，必须预先证明总调用及 fuel 够用；不足整项拒绝或由作者显式缩小方案，不截掉负例。

试点窗口最长 64 世界 ticks，最多 16 个执行 step、16 个工作节点/未结 lot、32 个非零资源或容量条目；一个修订至多一个活动试点，输出各 material 的最大量由全部已允许步骤的逐项上界推导并预留有限 custody。数量/单价没有通用虚构上限，受实际非负整数范围、owner 现有余额、permit 明确 cap 和 checked 运算共同约束；负数、无限值、未知单位、溢出及未经证明的零退出保留不允许启动。真实不收费/不耗资源的退出可以声明相应分量为零，必须有有界实际适配器证明，不能以缺实现填零。

这些数值限制单次工作量和必须可收尾的集合，不声称平衡或性能已验证。它们不能靠客户端变大；调整通过既有 profile/manifest 变更并重验受影响闭包/退出场景。当前宿主更小的限制优先，不为适配示例上调 executor。首版模拟服务按认证 actor 至多一个活动及一个等待请求、每世界最多 64 个等待请求、有界 FIFO 调度；队满拒绝新请求，重试保留原位置，不驱逐已有任务。服务调度只影响可用性，不进入确定性结果，不宣称抵抗多身份。试点资源仍沿原完整 contender/公平重评合同，不引入付费/OC/作者优先权。

### 9.2 约束组合与最小闭环

最小完整路径为：提案登记 → 在既有资源/能力快照上准备 3 个场景 → 两方案各两次求值（共 12 调用）→ 验证证据登记 → start 时一次联合占用 → 有限试点 step → 有限退出/关闭 → 既有治理准入提交 → 实际消费者当前重验。若每次声明 fuel 至多 1,000,000，则模拟总上界 12,000,000，落在首版总预算内；实际超出该单次限额为失败/不完整，不追加未经同意的调用。计算在不可变输入上完成后才提交有界 evidence，不长期占用世界写锁。

试点开始前同时核对 `生产消耗上界 + 全部执行费用上界 + 安全退出保留 <= 可用且同意锁定资源`、输出最坏量不超保管容量、步骤和退出可在其约定窗口完成。执行费用包括 start、规划、完成验证、正常/异常控制和 close 实际需要的全部调用，不能只算 recipe planner。cap、可中断边界、原工业处置和退出 actor 必须组合可行；任何一个未满足就不占资源。到期只阻止新试验行为，已有受权保管/返还需要的有限责任不被时间清理丢弃；其处理预算和容量在 start 前另行保留。没有有界退出适配器就拒绝开始。若开始后所有合法目的地或处置权又失效，保留原有限量和责任为 blocked、明确复查，不能在任意 authority 故障下承诺强制完成，也不能继续积累新产物或用清理冒充 closed。

<a id="creation-example"></a>
### 9.3 可复算铁锭样例与成本闭包

这是设计算例，不是真实源包、world、artifact hash、授权或运行证据。基线来自[铁锭源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/src/lib.rs)及[共享模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_recipe_module_template.rs)：每批矿石 4、碳燃料 1 → 铁锭 3、炉渣 1，生产电力 8，`duration_ticks=1`；批数为请求与各资源可支持批数的最小值，时长不乘批数。假设候选只将每批生产电力改为 6，使用 PPE 样例之外的全新 capability/version；这是假设改进，不宣称其合理平衡或实际可发布。

| 同一输入场景 | 基线静态预期 | 候选静态预期 |
| --- | --- | --- |
| 申请 3 批，矿石 12、燃料 3、生产电力 18，身份/设施匹配 | 2 批；用 8/2/16，产 6 铁锭/2 炉渣 | 3 批；用 12/3/18，产 9/3 |
| 同资源，仅生产电力 12 | 1 批；用 4/1/8，产 3/1 | 2 批；用 8/2/12，产 6/2 |
| 申请 1 批，矿石 3、燃料 1、生产电力 18 | 0 批，材料不足 | 0 批，材料不足 |
| 请求越出 permit 工厂/scope | 执行 guard 拒绝，不调用真实生产 | 同样拒绝，不以较省电放行 |

试点例使用候选、`[100,104)`、最多 3 批/2 次排程、单一有权设施，分别在 tick 101/102 开始 2 批/1 批，ready 分别为 102/103，停止余量仍在窗口内。输出隔离容量至少为铁锭 9/炉渣 3；用途始终 pilot。例中完整调用图为 start controller、planner 1、completion controller 1、planner 2、completion controller 2、close controller，共 6 调用；原产品验证为已支持的 native 判定，不再隐含其他 WASM。所有实际 payer/资源 owner 在这个 fixture 中为同一有权主体，受保护 hold adapter 已支持；其他主体组合必须分账，不跨 payer 凑余额。

每次实际 input/output 各为正且不超过 1 KiB、1 emit、0 effect；planner 无 new_state，4 次 controller 各有一次 new_state。对应 manifest limits 明确 `max_output_bytes=1024,max_effects=0,max_emits=1`，fuel/memory 为正且不超本 profile；现行 ModuleLimits 零 default 不是不限额。按[现行 schedule 映射](../wasm/wasm-interface.md#sr2-metering-contract)和[实际费用源码](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)，每次实际 Compute 为 `1+1+1=3`，planner Electricity 为 2，controller 为 3；**现有 upper_bound 即使对 planner 也保留 new_state 的 1，因此每次 preflight 的 Electricity 上界为 3**。这是明确 fixture 假设；真实接收器、验证路径或字节数多一项调用/费用，就须重算完整占用，不能照抄算例。

start 同事务计入其费用并建立总承诺：矿石 12、燃料 3、Electricity `18生产+6次×3上界=36`、Compute `6次×3=18`，其中 close 的 Electricity 3/Compute 3 始终不可用于新生产。完成前 2 批后，累计消耗矿石 8、燃料 2、Electricity `12+3+2+3=20`、Compute 9，保留原投入 4/1、电力 16、Compute 9；隔离产物为 6/2。此时主动退出再花电力 3、Compute 3，最终消耗电力 23/Compute 12，单次返还原料 4/1、电力 13/Compute 6，隔离产物另按同一已批准 close 处置。若继续第 3 批并关闭，实际累计电力为 `18+2×2+4×3=34`、Compute 18，返还未消费电力 2，产物 9/3；无版税、交付或产线奖励。

故意只提供 Electricity 33 时不能开始该完整例：虽足够生产和退出前调用的预检上界，却缺 close 保留 3。第一次已消费后再试图执行 2 批也拒绝，因为总批数/资源超额。重连或改请求 ID 不增加余额、产物或 evidence。上述数值由源规则及明确假设可复算；真实 handler、窗口、计量、custody 与 guard 未接通时按手册记 blocked，不伪造通过。

| 场景 | 环境、刺激与预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 同输入与隔离 | 同候选两次执行；错工件/缺 read-set/副作用尝试 | 同摘要；世界经济/权限/registry/outbox 变化为零；本节有限预算 | [IC-M02](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m02) | 目标，未运行 |
| 试点预算/退出 | 算例边界、双请求争用、窗口末端、收尾能力缺失 | 累计不超 cap，返还一次，未结义务不丢失 | [IC-M03](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m03) | 目标，算例仅静态 |
| 准入与跨入口 | 缺任一阶段、撤销/六态冲突、direct/module/tick | 非法使用零效果；合法范围一次结果 | [IC-M04](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m04) | 目标，未运行 |
| 故障与恢复 | 所有 touched 写点失败、失联、重启和 replay | 全体提交或零部分效果，原结果和守恒一致 | [IC-M05](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m05) | 目标，未运行 |
| 上限与可用性 | 每项上限/上限+1；退出与满输出容量组合 | 超限首 sink 前拒绝，不截断负例/处置；无吞吐 SLA 声明 | [IC-M08](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m08) | 首版工程初值，需候选实测 |

## 10. 兼容、迁移与回滚

<a id="creation-compatibility"></a>
新路径以明确 world/profile 激活边界启用；旧 `Requested/Shadowed/Approved/Applied`、旧 Profiles、普通 job/receipt 和既有合法能力按原版本保留，不补造 creation/simulation/pilot/admission。新产物或新因果版本不能声称 legacy 来规避四阶段；既有工作的延续和迁移由原工业/模块合同判断，不由缺字段默认成功。

先部署新 reader/reducer、旧格式读取、exact identity/purpose guard 与恢复能力，再开放新提案/模拟，最后在完整试点/退出/准入消费者支持时启用新正式采用。新类型不支持则明确拒绝；不 silent fallback 到普通排产。回退目标必须仍能识别所有已写入 schema、累计预算、原工件和处置状态，否则停止相关新写并保留可恢复历史；不可删除字段或重置 attempt 来回旧 binary。

保留 [PPE 有限回滚](../module/player-published-entities.design.md#ppe-rollback-disposition)：exact FROM artifact marker 只按原政策阻止新发布/激活，不自动停用其他已存实例、不删除 Profile/资产，也不改历史。本文的工业 admission 到期/撤销来源于独立当前工业授权，不能反向把技术 marker 扩成通用消费冻结。已提交试点损失、正式工作、许可收益和原 receipt 不因代码回滚撤销；需要纠正时另走有权、链接原事实的处置。

## 11. 验证设计与可追溯性

手册分开静态算例和真实 runtime 验收。required 覆盖四阶段正负例、边界和单次结果；full 增加 canonical 权威排序、持久化/重启、跨节点及正式消费者。所有运行记录必须包含同一实际候选、world/profile/manifest/schema、输入与 authority refs、前后账本/receipt 和实际入口。缺支持记 blocked/not_run，文档或已有局部测试不能代签。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [提案](#creation-proposal) | 来源、假设、scope、修订、撤回和无直接权力。 | [IC-M01](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m01)；同候选 required，按逐项刺激/前后状态核验。 | 提案/缺项/修订及零业务效果差分。 | 本轮未运行，源权利与实际制作另验。 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [确定性模拟](#creation-simulation) | 同输入两方案两次、无经济效果、缺证阻断。 | [IC-M02](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m02)；静态例与同候选 required/full 分开，记录 exact 输入和结果。 | 规范化输入/输出、来源、业务投影差分。 | 手算不证明 evaluator 或跨节点确定性。 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [受限试点](#creation-pilot) | 有限 scope/窗口/累计预算及真实退出，pilot 无商业收益。 | [IC-M03](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m03)；同候选 required，full 加并发和真实处置。 | permit、Root/Child、hold/消费/返还/custody/关闭 receipts。 | 算例非真实试点或平衡证据。 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [准入与消费](#creation-admission) | 完整链、当前治理与原消费者合取，scope 漂移不生效。 | [IC-M04](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m04)；required/full 的 native/module/tick/恢复入口，逐项缺条件。 | 独立阶段 refs、admission 和实际消费者结果。 | 不代签许可全域或旧工业工作图实现。 |
| [AC-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [接口](#creation-interfaces) | 全限定身份、版本、purpose、稳定语义槽。 | [IC-M05](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m05)；同候选 required/full 的冲突键/改名/跨实例/重试。 | 原结果、双索引和 payload 冲突证据。 | 目标 schema 未由文档落实到 ABI。 |
| [AC-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [事务与恢复](#creation-recovery) | 完整提交、未知结果查询、历史恢复与不重复消费。 | [IC-M05](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m05)；逐写点故障与 snapshot+tail，full 加权威恢复。 | 前后 root/资源/证据/receipt/去重/义务一致。 | 局部 prepared seam 不证明完整运行和独立故障域。 |
| [AC-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [读面](#creation-read) | 成本、损失、scope、下一步与安全替代同义且不越读权限。 | [IC-M06](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m06)；同候选同 anchor/audience 的 Viewer/API/Agent required/full。 | 各读面回读及权限/机会成本核验。 | 本轮没有 UI 接通或真实玩家证据。 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [限额与可行性](#creation-limits) | 有界执行、累计 cap、退出费用/容量组合可行。 | [IC-M08](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m08)；静态复算及同候选限额/上限+1/组合拒绝。 | 调用/fuel/内存/字节、费用和退出预算明细。 | 工程初值不是实测 SLA 或经济平衡。 |
| [REQ-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313 | [兼容](#creation-compatibility) | 新能力受控启用，旧历史/现有合法实例/PPE freeze 不变。 | [IC-M07](../../testing/manual/industrial-creation-simulation-and-admission.manual.md#ic-m07)；同候选旧/新快照、激活边界和回退 full。 | 历史 bytes/结果、新路径拒绝与原 marker 行为。 | 不声称已执行 migration 或 production rollback。 |

## 12. 决策、长期风险与未决问题

选择独立 creation evidence 与 admission 记录，复用原治理和工业工作，而不是扩展技术 release status 为万能审批。这样可以核对失败、累计投入和当前权利，代价是每个真实消费者必须读取额外 exact refs；未经接通的入口不能被声明已支持。

选择试点产物隔离及明确退出保留，避免用实验免费制造正式价值，也避免预算耗尽后只能无限保管。首版不支持无法有界关闭的试点、自动商业转正和免费重试；新增这些模式需实际处置、守恒和权限合同，不能仅放宽 UI。工程限额由 Runtime/WASM/QA 在候选实测后通过原 profile 流程修订，修改时重审完整组合而非只调单项上限。

四阶段、失败恢复、有限范围/期限/资源、单次结果和旧历史保护均已由上游决定，不作为待决政策。本设计不留这些机制给未来自由决定。具体创作的性能/收益假设、声明资源、适用治理来源和判据由其实际提案及现行合法 authority 提供，缺失则该提案 blocked；它不意味着所有产品规则未设计。批准阈值、经济率、独立身份与正式 finality 继续由原 owner 拥有，本文不以任意初值替代。

实现时仍须完成真实隔离 evaluator、资源/退出 adapter、schema、全部消费者与持久恢复，并按本手册取得同候选证据。这些是明确的后续代码/验收工作，不是设计空白的替代名称。改变世界权限、资源隔离、持久兼容或冻结边界时需要适用的独立专业评审；文档状态不代表可用、平衡、交付完毕或 release ready。
