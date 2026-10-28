# M4 工业资源流转合同

- 对应设计文档: `doc/world-simulator/m4/industrial-resource-flow-contract.design.md`
- 专业 PRD-ID: `PRD-WORLD_SIMULATOR-047`
- 上层产品映射: [`SC-21 多阶段工业流水线`](../../product/world-rules-core-gameplay/prd.md#5-done成功标准与验收)
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history；本文与 design 保留 M4 资源流转和确定性报价合同。

## 1. Executive Summary

本专题是 M4 工厂、材料物流与电力资源流转的现行专业合同。它收敛四组已完成历史三件套：工业 WASM、M4 电力系统、材料多账本/在途物流，以及 Location 电力池下线与辐射电厂建造；历史实现细节可从 Git history 和 GitHub task issue evidence 追溯。

工业执行协议的 legacy/current/target 状态与 schema authority 统一见 [`world-runtime` industrial execution status and authority matrix](../../world-runtime/prd.md#industrial-execution-status-and-authority-matrix)。本文件只维护 M4 domain semantics，不把目标 event/ID/schema 写成当前 runtime 已实现事实。

## 2. User Experience & Functionality

### In Scope

- 工厂按配方消耗输入、产生产物，并在确定性、可审计的世界状态中推进。
- 材料可在 world、agent、site 与 factory 账本间转移；跨账本调运遵守现有距离、损耗、延迟与吞吐限制，并保留快照/回放兼容边界。
- 电力归明确 owner 持有；Location 不持有有效 electricity 库存。`factory.power.radiation.mk1` 可把建造与 `PowerPlant` 注册接到 owner 入账路径。
- 当前不提供 `PowerStorage`、`DrawPower` 或 `StorePower` 路径；不得将历史储能或 Location 电力池模型重新表述为现行能力。
- 工业模块、builtin descriptor/catalog、artifact hash 与 identity manifest 继续由相应 ABI/runtime canonical contract 拥有；本专题不替代其 release policy。

### Unresolved player-readable debts

GitHub #2166 中的 `schedule_quote`、`logistics_transfer_quote` / `transfer_impact_preview`、`product_validation_quote` / `validation_unlock_preview`、`power_survival_quote` / `energy_recovery_preview`、`power_sale_quote` / `energy_liquidity_preview`、`market_quote_decision_preview`、`refine_quote` / `refine_preview` 与 data-access recovery 的可读性按实际交付范围验收。现有 quote、DTO 或局部 consumer 不应重新写成完全未实现；[#3510](https://github.com/eng-cc/oasis7/issues/3510) 承接仍未收口的独立语义/验收项。文档补充不证明代码修复、全部 Viewer/Agent surface、ABI 变更或发布 readiness。

工业恢复的玩家语义由 gameplay agency/top-level 合同承接：资源不足、采空、不可达或精炼短缺不能把 Agent 的自动裁剪、改道或拒绝伪装成原意图已经完成；正式 surface 将原 accepted intent、约束原因、保留进度和 wait / repair / reroute / reprioritize 的下一决策连在同一因果链。`refine_value_class` 的玩家词表保留 `enough_to_advance / partial_progress / poor_power_tradeoff`，不表示各项在当前资源模型都可达，适用范围见下文净缺口合同；历史 `enough_for_next_step` 只作该首项的迁移别名，不另行冻结公式或行为。

历史 LLM debug 资源注入保持 `LLMSKIP`/debug-probe 边界：默认不暴露给正式玩家，非 debug 模式必须拒绝，且不得作为工业 progression、经济结果、active-LLM formal lane 或 playability/release 证据。该边界不新增调试工具、玩家资源入口或运行时实现承诺。

### Out of Scope

- 完整物流寻路/拥塞网络、金融市场、科技树或职业系统。
- 电价、电耗、产率、阈值、损耗、速度、吞吐或维护公式重平衡。
- LLM/provider 策略、Viewer 图谱/缩放；simulator `PowerStorage` hard-removal 的当前边界已在本文收敛，历史实施仅从 Git history / PR 与实际验证记录追溯。

## 3. AI System Requirements (If Applicable)

N/A: 本专题不新增 Agent 推理、provider observation 或公共 action schema。任何未来把工业恢复诊断写入 Agent observation 的变更，仍须通过对应 LLM/provider contract 与 schema version。

## 4. Technical Specifications

### 工业与材料

- Recipe、Product 与 Factory 模块定义工业规则；运行时负责授权校验、状态落地、事件、回放与拒绝语义。
- `MaterialLedgerId` 可表示 world、agent、site 与 factory 账本。旧快照的材料兼容与 WASM 请求兼容必须由 runtime 处理，不能由产品或 Viewer 层伪造。
- `MaterialStack.kind` 是模块/domain 拥有的字符串材料标签，可以表达 `compound`、`hardware` 等工业材料；它不能扩展、伪装或自动映射为 simulator 内建 `ResourceKind`。内建资源仅有 `Electricity | Data`，模块资产的 manifest、hash、interface、capability、limits、owner 与安装/调用拒绝继续服从 WASM ABI/runtime authority。
- `TransferMaterial` 的即时或在途结果必须保持守恒、确定性和可审计；其具体字段、事件顺序和 ABI 以 runtime/ABI 真值为准。其中空 route/path binding 且 `auto_reroute=false` 的 zero-distance 请求保留 legacy direct-transfer compatibility；同一空 binding 下 positive-distance remains existing unbound transit。显式 route/path binding 或 `auto_reroute=true` 请求 graph mode，只有 path validation/selection 成功才成为 path-bound transit（即使有效路径距离为零）；invalid explicit binding 或无可用 graph selection 必须 atomic reject；unavailable explicit binding 只有在 `auto_reroute` 选出另一条有效 graph path 时才能替换，且不得 fallback 到 direct。该 runtime compatibility lane 不替代 gameplay authority 对玩家层 co-location/eligibility target 的定义。以上当前边界以 [`world-runtime` status matrix](../../world-runtime/prd.md#industrial-execution-status-and-authority-matrix) 为准，不能把 `ActionId/caused_by` trace 写成已经落地的 canonical root identity。

<a id="industrial-quote-ledger-semantics"></a>
### 报价账本、净缺口与转移失败边界

Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95; O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48；产品适用承接：[REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)、[REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)。 本产品承接限于首局工业链适用时的同一实际输入/消费口径、提交重验与重复无副作用；本节其余专业报价规则继续按原范围独立适用，不由这两项首局要求代替或证明完成。

以下条款补齐现有报价的领域解释与实现验收目标，保持 simulator 的 `Electricity | Data` 与 Runtime 的 `MaterialLedgerId` 两条权威边界，不引入旧 ResourceKind，也不让一条 lane 的公式覆盖另一条。

- **精炼前后余额：** simulator 的 `hardware_shortfall_before` 从动作前真正可用于当前 BuildFactory 目标的 Data 余额计算；after 从执行相同精炼转换后的净 Data 余额计算。旧字段名 `compound_mass_g`/`hardware_output` 不构成两份独立库存或“未精炼 Data 不可建厂”的资格。现行产率上限使产出不大于投入，净 Data 不增加，建厂缺口不能缩小；当前模型不得产生 `enough_to_advance` / `partial_progress` 的目标进展判定，也不要求这两类不可达正例。合法转换成功仍须按 `poor_power_tradeoff` 说明无改善或净损失，推荐量必须与同一净变化一致。
- **逐种材料缺口：** Runtime 报价先按当前材料同一性规则聚合重复请求，再对每种材料计算 `max(requested - selected_available, 0)`，最后求和。其他材料的盈余不得抵销本材料短缺；只截总量也不成立。独立行情保留现有 local/world 来源比较，不把两份余额重复合并；对 `ScheduleRecipe` 的准入报价，`selected_available` 只取实际 factory input ledger，不能用 world cover 清零排产缺口。DTO 的 allowed、分项/总缺口与推荐理由须同义，材料足够也不保证其它提交前置成立。
- **消费账本：** 对具体排产结果的报价必须读取该 factory/recipe 实际消费的 input ledger，而不是以发起 Agent 或 world ledger 替代；独立库存/行情查询可以说明自己的账本，但不能冒充另一条排产的可执行性。工厂不足而世界有料时，只能推荐受支持的显式采购/调运；材料实际入厂后重新报价和校验，不能以 `submit_with_world_supply` 直接提交或隐式跨账本扣料。报价显示 owner、消费账本与库存时刻，实际提交仍重新校验。
- **转移原子性：** simulator `TransferResource` 在任何源扣减前检查源与目标的资源种类支持及入账可行性；Location Electricity 必须提前拒绝。任何转移失败不得留下半笔扣减/入账，也不得通过恢复 Location 电力池或给拒绝事件补扣款来迁就错误；自转移与正常 Agent/合法 Data 路径保留各自既有语义。
- **物流解释：** 合法 direct/no-route 只表示没有 graph path binding，不等于 path unavailable。route、material default、explicit priority 与 keyword fallback 保留独立 reason；不可执行 quote 的 ETA 显式为不可用或以成立条件限定。引用 Runtime 原判断，不新增 blocker projection、路径或调度 authority，旧零距离兼容单独验收。

最小独立验证包含：默认 fixture 的 2500 Data/50 Electricity 同快照分叉，直接建厂与精炼 2500 后建厂须分别验证；若建设需 5 Data、精炼后余额为 2 Data，真实缺口是 `0 -> 3`，不能报 `5 -> 3` 进展。

另测本地/世界富余、一盈一缺、精确覆盖、普通短缺、重复材料、factory 与 Agent 余额不同，以及合法正数 Agent→Location 电力转移拒绝时源仍保留原余额。保留低电力、最小产出、推荐量、只读 quote、正常转移、自转移和余额不足；这些样例定义预期，不表示当前实现已经通过。系统映射见[配对设计](industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)。

### 工业外部性与 `operational_mitigation` authority（target-only）

本节只为未来可选的 profile-declared `industrial_externality` 建立 M4/domain authority 边界，不代表当前工厂或配方已有该能力。M4/domain eventual authority 负责 profile 在 factory/recipe/operation scope 的外部性事实、受影响范围、`operational_mitigation` 的可用/已占用容量，以及一次性的 externality ledger 与 impact receipt/disposition；产品层拥有玩家承诺，gameplay 拥有选择与机会成本，M4 不新增污染或治理规则。

- `operational_mitigation` 仅表示局部工业运行的缓解/处置能力，不能继承 systemic crisis containment、区域治理、shared-impact right、regional credit 或其他治理语义。没有 profile 的 legacy factory/recipe 保持原路径不变，不因本 target contract 追溯加闸。
- profile 已声明但 mandatory externality fact、scope 或 mitigation authority 缺失、过期或冲突时，结果必须是 `unknown/blocked`，在首个不可逆 sink 前禁止排程/运行；不得从材料名称、batch contamination、location radiation-source fact、客户端缓存或 Agent 建议推断“无外部性”或安全容量。
- M4 的 externality fact、mitigation capacity 与 impact disposition 必须沿既有 root/revision/operation lineage 持久化、可审计并可回放。`production result`、externality disposition 与 delivery/terminal settlement 是互相独立的事实；production receipt 不能冒充 mitigation、cleanup、terminal 或治理结算。
- 已有 profile 支持的运行至多产生一次 linked production result 与一次 externality/mitigation disposition；WIP、in-transit、buffer、terminal-pending 的处置必须各至多一次。terminal-pending 的 quantity、recipient、destination、admission、capacity obligation 与 provenance 必须保留至 matching settlement 或一次受支持处置，不得静默改绑、提前 settlement 或重复释放容量义务。重复提交、重连、Agent retry、snapshot restore、事件乱序和 replay 只能重读既有 receipt/disposition，不得重复扣容量、产生影响或发放进度/奖励。
- `world-runtime` 拥有未来 event/schema、apply order、状态与持久化、checkpoint/recovery、replay/idempotency 以及 current/target status；本节不定义这些字段或实现。当前 runtime 仅有 `LocationProfile.radiation_emission_per_tick` 等 location radiation-source 窄路径证据，不能据此宣称 factory/recipe industrial externality 已实现。

本 target authority 的 Non-Goals 是：不规定污染/温度/环境公式、积累、衰减、扩散、半径、阈值、清理经济、税费、区域 credit、生态模拟或治理/charter/claim 权利；不修改 runtime code、event/schema、API、ABI、UI 或 Agent 行为，也不提供自动缓解、清理、降级、改道、退款或补偿。未来 M4/runtime/QA composite evidence 形成前，不得把本节写成 current implementation 或 release readiness。

### 多阶段批次、预留与背压合同（PRD-WORLD_SIMULATOR-047）

- 每批中间品必须保留可回溯的来源阶段/边、父级 receipt、材料与规格/品质适用证据、数量及当前到达账本。下游只有在批次已经结算并到达、且按当前规则明确判为适用时才能消费；`待验证/证据不足` 与 `不适用` 都必须在下游 sink 或进度前 fail closed。材料名称、Agent 建议、Viewer 缓存或模块元数据不能替代适用性证据，也不能把未知批次推断、降级或混合成适用批次。
- 中间品预留是权威、数量有界且绑定批次、阶段/边、消费主体与生命周期的 hold；报价、计划、上游已接受或加工中状态不构成预留。同一数量不得同时承诺给多个下游。预留过期、撤销或前置失效时，只能按适用合同释放、保持待决或原子拒绝，不能重复扣料、生成第二份库存或静默转成欠费/补贴。
- 每条中间品边必须声明有限 buffer 与确定性的背压结果。下游不可用或 buffer 满时，只能保持尚未消费的上游投入、把已结算产出放入仍有容量的 buffer，或原子拒绝新的上游承诺；不得静默丢弃、瞬移、无限堆积、自动改道或伪造下游完成。overflow、返工、报废或 salvage 只有在对应专业合同存在时才能产生一次可审计结果，否则必须拒绝。
- split、混批与 merge 必须逐输入批次重新校验适用性，并使用稳定、可复现的分配与齐套规则；专业合同必须明确采用“只消费适用数量并保留其余批次”还是“整笔原子拒绝”，不得依赖客户端/Agent 提交顺序、隐藏混合或表现层缓存。阶段、配方、设施或边变化后，未结算承诺按当前规则重新裁决，不能继承旧候选资格或稳定进度。
- 需要多个独立 input edges 的 Recipe/Stage profile 必须声明 canonical join identity 与稳定有序的 required parent-edge/receipt set，绑定 candidate、recipe、executable cycle/batch、各 parent 数量/规格/版本及适用窗口；matching 必须与 parent 到达顺序无关，不能按材料名称、最新 receipt 或客户端顺序推断。默认 atomic-kit policy 在同一 snapshot 验证全部 parent 已 settlement、实际到达、适用且可独占消费后，才允许第一个 input sink/WIP/progress；早到 parent 保持在 ledger/有界 hold 的 join_pending。
- 只有 profile 显式声明 staged intake 时，才允许齐套前消费部分 parent；每个 parent sink 必须绑定 join identity 并记录 remaining obligations、hold/capacity 与 missing/invalid/expired/cutover parent 的 disposition。Parent reservation 不能同时属于多个 join，未消费 hold 至多释放一次；late arrival/资格恢复/容量释放只重评 unmet set。Cutover 保留旧 join/parent lineage，新候选不得复用旧 receipt；retry、reconnect、arrival reorder、恢复与 replay 只能重读同一 join disposition，每个 parent 至多 sink 一次并只产生一个 join/stage-start receipt。
- 跨阶段 lineage 必须在持久化、恢复与 replay 后仍能从子阶段承诺、边和中间品状态回溯到父级 receipt、预留、适用结论与实际损耗。重复提交、Agent 重试、重连或 replay 对每个阶段至多产生一次 sink 与产出，不能跳过未满足的前置，也不能把未知/不适用批次重试成适用。`PRD-WORLD_RUNTIME-001` 拥有执行顺序、事件 apply、持久化与 replay/recovery，目标 `PRD-WORLD_RUNTIME-043` 承接跨阶段 receipt identity、parent linkage 与幂等 finality；M4/domain 拥有批次、边、适用性和预留语义；game 拥有节奏、数值与机会成本。
- 两个或以上有效的 accepted intents 共同消耗同一 stage execution slot、edge throughput 或 destination buffer 时，必须形成一个权威 allocation domain，并在同一 allocation snapshot 上评估完整 contender 集、可用容量与既有 holds。`accepted` 不代表已经获得容量；每个 contender 只能取得 full、专业 profile 声明支持的 partial、deferred 或 denied 结果之一，且总 hold 与消费不得超过快照容量。裁决使用稳定冲突键、当前专业策略及稳定 intent identity 解 tie，不能由网络/调用到达顺序或逐请求局部写入决定。
- partial 结果必须只消费已分配数量并保留可追溯的未满足部分；deferred/denied 不得生成隐性 hold、sink 或进度。未消费 hold 在取消、过期、消费者失效或前置变化时至多原子释放一次；容量释放或上游到达后，仍有效 contender 只能从新的 canonical snapshot 重评，不能回滚已消费量、复制 hold/产出或静默提升旧意图资格。
- 重评必须保留原 intent identity、lineage、既有 hold、评估依据与结果；retry、reconnect 与 replay 不得刷新优先级、重置等待或产生第二次效果。专业 profile 必须声明可观察的重评条件，以及延期意图的过期、终止或重新规划边界；在容量反复可用且意图仍有效时，不得无解释地无限跳过。专业 surface 只能披露当前主体有权读取的 shared resource、held/unmet quantity、full/partial/deferred/denied、顺序理由、下一复查点与恢复动作，不得伪造精确 queue position/ETA 或泄露其他主体私密事实。
- 只有专业 profile 明确声明时，multi-tick pipeline 才具有 end-to-end service-window guarantee；否则为 best-effort。声明窗口时必须用独立 stable identity 绑定 candidate/batch/operation、start/target/end boundary，以及 stage slot、edge throughput、destination buffer、terminal capacity 的 mandatory reservation chain。每项 reservation 必须分类为 held-through-completion/arrival、boundary-revalidated conditional 或 non-reserved；accepted/hold 不等于 service started，未持有的未来容量不得计入保证。
- Profile 必须声明 none/soft/hard window policy 与各 lifecycle state 的 on-time/at-risk/late/expired disposition。Start、stage finish、transit launch/arrival、buffer admission 与 terminal settlement 按 canonical world-time/order 重验剩余窗口和 mandatory reservations；hard failure 在下一 irreversible sink 前 deferred/atomic reject，soft policy 可明确 late 继续。Pre-start expiry 只释放 unconsumed hold 一次；已存在 WIP/transit/buffer/production effect 时，只能 finish/pause/hold/quarantine/reroute/requote/reduce/reject 或走受支持补偿，不能抹除历史 sink、自动退款/续期或留下 orphan hold。
- Power-dependent Recipe/Factory/path profile 必须把每个 stage/cycle/branch/terminal power obligation 分类为 start-only sink、held-through-completion/arrival、boundary-revalidated conditional 或 non-reserved。Hard/soft service guarantee 只有 mandatory power 已纳入 reservation chain 时成立，否则明确 power-best-effort。Submit 与声明的 stage/transit/buffer/terminal 边界按记录 mode 重验 owner-held electricity；shortfall 在下一 irreversible sink 前产生一次 deferred/reject 或显式 at-risk/late/blocked disposition，已存在 WIP 走既有 disposition。不得 overdraft、implicit renewal/top-up、double debit 或以 quote/runway 冒充 hold。
- Power admission/sink/hold/release/revalidation/unmet receipt 必须绑定 root+owning revision/segment+cycle/branch 与 baseline expected power，且不是 material/terminal settlement。Diagnostics 区分 available/held/consumed/unmet/unknown，insufficient power 是 primary root，派生 backlog 是 secondary。共享 power contention 使用同一 authoritative allocation snapshot、stable order 与 single release/re-evaluation；`PRD-WORLD_RUNTIME-001` 拥有跨域原子 debit/hold/release、journal/persistence/replay，目标 `PRD-WORLD_RUNTIME-043` 承接这些效果的 root/revision/receipt identity；Recipe/Factory/Power profile 拥有 mode、obligation 与 shortage disposition。本文不冻结 power amount/formula/price、battery/storage/maintenance、queue 或 auto-top-up。
- Renewal/extension 必须从 fresh authoritative state 重验 owner、candidate、route/spec、capacity 与当前 state，并以链接 parent 的 continuation/new identity 产生一次结果；retry/reconnect/replay 不得延长 window、重置 age 或刷新 priority。Expiry/release/renewal 与触发的 fresh-snapshot re-evaluation 各至多一次，runtime replay 使用 journal 中记录的 tick/order/disposition，不按恢复时 wall clock 重算。`PRD-WORLD_RUNTIME-001` 拥有 clock/order、原子 state/capacity transition、journal、持久化与 replay，目标 `PRD-WORLD_RUNTIME-043` 承接 window/lease/continuation identity 与 receipt linkage；M4/domain profiles 拥有 window scope、reservation classification、on-time/late/expired 与 renewal/disposition policy；game 拥有 cadence、机会成本与稳定/交付承诺。
- 改变阶段、配方、设施能力/版本、边目的地或其他产出因果/消费资格的配置变更必须创建新候选身份，并以一个权威 cutover snapshot 线性化边界；只改变展示或非因果元数据的变更不得误切候选。同 tick 的变更与生产/运输结果沿用 runtime 的稳定冲突顺序。边界前产生的 hold、WIP、transit、buffer batch 与 receipt 保持旧身份，边界后新意图只属于新身份，不能按材料同名、目的相同或调用顺序静默重绑。
- Cutover 必须为每项旧状态产生一次确定处置：未消费 hold 只能在旧身份下保持、释放、延期或拒绝；WIP 只有在旧前置仍有效且专业 profile 明确允许时才能 drain/finish，否则暂停、终止或进入受支持的 rework/salvage；transit 保留原 receipt、edge、目的账本、规格和已发生损耗，失效时只能经显式 transfer/return/hold/reject 处理；buffer batch 保留旧 ledger 与 lineage，新候选仅能通过专业合同支持的 handoff/conversion 并重新校验适用性与容量后消费。没有对应处置能力时必须 fail closed，禁止隐式混批、重贴标签、瞬移、自动改道、同时退款与完成或半旧半新的状态。
- 新候选不得继承旧候选的稳定窗口、里程碑、排队资格、reservation 或 receipt。受支持的 handoff/conversion 必须只产生一次 receipt，并关联父 receipt、旧/新候选身份、数量、实际损耗与变更原因；旧 receipt 不能直接复用为新阶段 sink、进度或奖励。`PRD-WORLD_RUNTIME-001` 拥有 cutover 的原子顺序、journal/event apply、持久化与 replay/recovery，目标 `PRD-WORLD_RUNTIME-043` 承接 receipt identity、old/new candidate parent linkage 与幂等 finality；M4/domain 拥有导致失效的条件、各状态允许的处置、适用性和迁移语义；game 拥有换线节奏、选择与机会成本。
- Cutover boundary 与逐项 disposition 必须从同一有序权威输入确定并可由 journal 恢复；重复变更、retry、reconnect 或 replay 不得重复释放 hold、完成/复制 WIP、交付 transit、迁移 buffer、退款或重新开启旧候选。任何部分失败只能保持可追溯的旧状态或显式拒绝。专业 surface 至少表达旧/新身份、受影响状态、处置、保留/损失/占用、稳定窗口影响和下一步，不得把旧进度显示为新候选进度或伪造 ETA/queue position。
- 每个 terminal stage 必须声明最终产物的有界 disposition：owner-bound 的有限 product ledger/buffer、具备明确 recipient/purpose 的 delivery commitment，或专业 profile 支持的 hold/quarantine。Production settlement 只确认最终产物以实际数量/损耗进入合法终端账本或显式 transit claim；它不能自行代表 delivery、market/service settlement、需求减少或终端奖励。终端容量/owner/资格失效时，只能在产出 sink 前阻塞/原子拒绝，或把已合法生产的批次保持为有界 pending/held 状态；不得无限堆积、静默销毁/没收、自动转卖/改道或免费 credit。
- Preview、accepted/hold/allocation、in-transit/pending 与 released/rejected receipt 都不是可解锁下游或终端用途的 settlement receipt。每个 source -> edge -> destination handoff 必须绑定一个 operation identity 和可追溯的有序 journal 链，并收敛为“有效 source effect + 至多一次 destination settlement”、明确 pending/hold，或原子拒绝/无效果之一；不得出现 source debit 后目的丢失、无 source 的 destination credit、pending/settled 或 settled/rejected 同时成立。重复 submit/arrival、retry、reconnect、恢复或 replay 只能重读同一 terminal disposition，不得复制 destination credit、下游 sink、产出、需求减少、稳定进度或奖励。
- Settlement receipt 必须能回溯适用候选/edge、父 settlement、实际数量与损耗、destination ledger，以及它是否授权下游消费或终端用途。Pending 只可在 fresh authoritative condition 下推进，陈旧 receipt 不得解锁下一阶段；若专业合同支持纠错/补偿，必须生成独立 compensating receipt 并链接历史 settlement，不能改写既有 receipt。`PRD-WORLD_RUNTIME-001` 拥有一般 journal/order、state-root、持久化与 replay/recovery，目标 `PRD-WORLD_RUNTIME-043` 承接 operation/receipt identity、finality 与幂等链接；M4/domain 拥有即时/在途 handoff、终端 disposition、settlement/消费资格及支持的补偿语义；game 拥有终端选择、节奏与机会成本。
- Recipe + factory capability 必须声明 canonical executable quantum/cycle 及 full-only 或支持合法 partial 的策略；材料 batch 或 allocation 的 partial 结果不能自动授权 partial recipe。请求、报价、提交和结算必须可追溯 requested quantity、admissible quantity、committed/allocated cycles、实际 executed cycles 与 unmet/residual。Full-only recipe 在输入、能源、stage/edge/buffer 或 terminal capacity 不足一个合法单位时，只能在首个 sink 前 deferred/atomic reject，不得静默降额、部分扣料、生成半批主产物/副产物或提前进度。
- 支持 partial 的 profile 必须声明 deterministic quantization/rounding 与 residual disposition：carry/retain、return、合同明确的有界 loss 或 reject 之一；零可执行 cycle 不产生 sink。所有量化由专业 authority 在 canonical state 上完成，客户端/Agent/Viewer 不得浮点取整或改变配方。Shared allocation 的 partial 必须再次通过 batch admissibility；不足合法 quantum 时只能 deferred/denied。每个已完成 cycle 的 settlement receipt 记录适用 candidate/recipe profile、committed/executed cycles、实际 input sink、product/byproduct totals、residual/unmet 和父 receipt；副产物目的账本/容量失效时按 profile hold/block/reject，不得丢弃、并入主产物或按 requested quantity 重复结算。
- Retry、reconnect、恢复与 replay 只能重读原 batch commitment 与其量化结果，不能使用变更后的 recipe/yield/rounding 重算旧 WIP，也不能复制主产物、副产物或稳定进度；cutover 继续按旧候选 disposition 处理。M4/domain/Recipe module 拥有 executable unit、partial 策略、量化/residual、副产物路由和适用性；`PRD-WORLD_RUNTIME-001` 拥有确定算术、原子 apply、journal、持久化与 replay，目标 `PRD-WORLD_RUNTIME-043` 承接 sink/settlement receipt identity 与幂等链接；game 拥有批量节奏、机会成本与 anti-grind。
- 每个 executable cycle 的 Recipe/Product profile 必须声明 canonical output branch set/order、各 branch destination/eligibility/capacity，以及 fan-out policy。默认是 all-output atomic：一个 output-bundle identity 绑定 candidate/recipe/cycle、主产物、全部副产物与 parent receipt；runtime 必须在第一个 output credit/progress 前从同一 snapshot 预检全部 branch，任一 branch 不成立时只能整体 deferred/atomic reject 或进入 profile 支持的有界 hold/quarantine，不得留下主产物已 credit 而副产物丢失/blocked 的隐式部分状态。
- 只有 profile 显式声明 split fan-out 时，才允许 branch 独立 settlement；每条 branch 必须保留稳定 identity、parent bundle/receipt、目的 ledger/edge、实际数量/损耗、settled/pending/failed disposition 与 remaining obligation。Parent bundle completion、production/stable progress 和 terminal promise 必须服从已声明 policy。容量/资格恢复只从 fresh state 重评未决 branch，不能重放已结算 branch、重复 input sink、主/副产物或奖励，也不能把未决 branch 静默丢弃、改道、并入主产物或伪装成完整 bundle。
- `PRD-WORLD_RUNTIME-001` 拥有同一快照的 branch preflight、canonical apply order、跨 ledger 原子 state/journal、持久化与 replay/recovery；目标 `PRD-WORLD_RUNTIME-043` 承接 bundle/branch receipt identity、direct-parent linkage 与幂等 finality；M4/domain/Recipe/Product profile 拥有 branch membership/order、destination、eligibility 与 atomic/split policy；game 拥有 branch 用途、容量/时间机会成本与 anti-grind。本文不冻结 output ratio/yield、容量数字、queue/timeout、runtime schema/event 算法或 UI 布局。
- Industrial performance/diagnostic snapshot 是 read-only derived observation，必须绑定 canonical measurement-window identity、candidate/recipe/factory/stage/edge/buffer/terminal config epoch、world-time start/end boundary 与 journal/state-root anchor；snapshot read 不产生世界效果。只有 committed receipt/event facts 能进入 numerator/denominator：actual input sinks、executed cycles、product/byproduct settlement、edge loss、delivery/terminal outcome 与显式 rework/salvage/return/compensation。Preview、request、accepted intent、speculative hold、估算 ETA 与表现层采样不能计入 actual；缺失 authority 时必须标记 unknown/degraded。
- Plan baseline 只由 authoritative submit/allocation outcome 创建：每个 full/partial/deferred/denied accepted intent 必须原子获得一个 immutable baseline identity/revision，绑定 candidate/config epoch、batch/join/bundle/window identity、canonical snapshot/journal anchor、requested/admissible/committed/executable/unmet/residual，以及当时的 input/power/output/byproduct/stage/edge/buffer/terminal obligations。Preview/quote/request/speculative hold 不创建 baseline；atomic reject 且没有 accepted intent 时也不创建。Deferred/denied baseline 只记录计划结果，不构成容量、hold、sink 或 actual。
- 同一 authoritative accepted outcome 必须创建一个 immutable root operation identity；atomic reject 无 accepted intent 时不创建。所有 stage/job、join、bundle/branch、edge/transit、buffer/terminal、window、baseline revision、checkpoint、validation decision 与 production/delivery/compensating receipt 必须携带 root、owning revision/segment 和直接 parent/child role；不同 accepted intents 即使 payload/candidate 相同也保持不同 root。Fan-in/fan-out child 不得成为无 parent 的新 root，缺失/冲突链接必须在 first sink/credit/progress 前 fail closed。
- Checkpoint continuation 与 non-causal revision 保持 root 并新增 child segment/revision；causal cutover 创建 parent-linked child root/new candidate，旧 receipt/actual/W 不迁移。每条 receipt 只属于一个 root+revision/segment+child step/branch；root 经 terminal settlement 或 explicit abort 关闭后，不再接受普通新效果，late effect 只能走已有 pending/compensating policy。Runtime 当前仅以 `ActionId/job_id` 与可选 `caused_by` 提供 single-action/job/direct-cause trace；这不等于本节要求的 immutable root operation protocol。目标 runtime authority 由 `PRD-WORLD_RUNTIME-043` 承接 root issuance、ordered direct-parent linkage、owning revision/segment、journal/persistence、terminal finality 与 replay/idempotency；`PRD-WORLD_RUNTIME-001` 继续拥有一般确定性执行/回放，`PRD-WORLD_RUNTIME-019` 只拥有 factory blocked/resumed/completed 可解释状态。M4/domain profile 拥有 child role、fan-in/out、cutover 与 compensation relationship。Validation decision 是 root+revision+branch 下的 child receipt，由 bundle atomic/split policy 至多消费一次；本文不冻结 ID encoding/schema、hash/UUID 算法或 ABI。
- Product/module validation quote/preview 无 authority；validation decision 是 evidence-only child receipt；production/delivery settlement 才能 credit 材料或授权 finality。Product profile 必须声明 validation timing（submit/start/completion）、mandatory/advisory gate、freshness/drift disposition，以及每个 root+revision/segment+cycle/branch+validation epoch 的唯一 decision context：exact parent batch/receipt、quantity、target stage/recipe/factory profile/config/spec epoch、validator/module release 与 proof/state/journal anchor。Decision status 为 applicable/not_applicable/pending/unknown/expired；decision 本身无 sink/credit/W/reward/delivery effect。
- Mandatory validation 只有 matching positive decision 与 parent settlement/arrival 同时成立时才能授权 downstream first sink；missing/negative/pending/unknown/expired/wrong-scope decision fail closed。Batch/spec/owner/ledger/route/profile/candidate/module drift 只 supersede immutable old decision，并按 profile linked revalidation、hold/defer 或 reject，不能跨 root 复用、改写或自动降级。目标 `PRD-WORLD_RUNTIME-043` 持久化 root-scoped idempotent decision receipt，`PRD-WORLD_RUNTIME-001` 保证其确定性 apply、journal、recovery 与 replay；missing/conflicting anchor 在 output credit/progress 前 fail closed。Bundle atomic policy 等待全部 required branch decisions，split policy 只消费各 branch decision 一次；WASM authority 继续拥有 module release/hash/purity，本文不冻结 validation/seed/quality 算法或 ABI。
- 已 journaled baseline 不得被后续 profile/capacity drift、retry/reconnect/replay 或 replan 改写。受支持的 replan/reduce/reroute/renewal 必须在单一边界创建 parent-linked child revision，并使每条 receipt/actual 只归属 owning revision；因果配置变化仍使用既有 cutover/new candidate，非因果修订也必须分开 review。目标 `PRD-WORLD_RUNTIME-043` 承接 baseline/revision identity 与 parent linkage，`PRD-WORLD_RUNTIME-001` 拥有原子 capture、journal/state-root anchor、持久化及确定性 replay；M4 Recipe/Factory/domain profile 拥有 admissible/commit outcome、expected obligations 与允许的 revision policy。缺失/失效 anchor 只能是 plan_unavailable/unknown，不能用当前 plan/profile 回填。
- Domain profile 必须声明 measurement checkpoint 的合法原子边界。Canonical checkpoint identity 绑定 operation/window、baseline/revision、world-time/order、journal/state-root 与 pending WIP/transit/buffer/terminal/hold obligations；checkpoint 不能切开 atomic join、recipe cycle、output bundle/branch 或 source→destination settlement，也不能由 read/refresh/tiny-window request 创建。边界不合法时整个效果保持 pending 或等待结算，不能一半进入旧窗口、一半进入 continuation。
- Checkpoint 只读且只关闭一个 immutable review segment 一次。未完成 operation 的 continuation 必须链接 parent checkpoint/review，并把 backlog/join/WIP/transit/buffer/terminal/hold 作为 opening state，而非新的 accepted、allocation、sink 或 actual；默认继续同一 operation/baseline revision，显式 replan/cutover 才按既有 child revision/new candidate 处理。目标 `PRD-WORLD_RUNTIME-043` 承接 checkpoint identity、parent continuation 与 receipt attribution，`PRD-WORLD_RUNTIME-001` 拥有 ordered atomic closure、持久化及确定性 replay；M4/domain profile 拥有合法边界、bucket carry 与允许的 continuation disposition。Checkpoint 不续租、不释放容量、不改变 W/reward/finality；缺失或冲突 anchor fail closed 为 checkpoint_unavailable/unknown。
- Planned/requested、admissible、committed/executable 与 actual executed/produced/delivered 必须分层。Window boundary 上每个 lineage 只进入 accepted-unstarted backlog、join_pending/held、active WIP、in-transit、buffer-held、terminal-pending、settled 之一，不得跨状态重复计数。Domain profile 可以声明 yield/loss/utilization 的合法分类和 denominator 来源，至少区分 stage busy/idle/blocked、edge used/available throughput、buffer occupancy/peak/held、terminal held/admitted，但本文不冻结公式、目标值或采样 cadence。
- Bottleneck attribution 以 lineage 中 earliest canonical unsatisfied stage/edge/buffer/terminal blocker 为 primary root，派生后果为 secondary 且不重复累计；多个原因使用稳定 precedence 并保留 provenance，root 变化开启新 attribution segment。Cutover 在 canonical boundary 关闭旧 measurement window 并开启新 epoch；只有显式 handoff/conversion 可链接，不能静默跨候选合并。Recovery/replay 从同一 journal/state-root 与已记录边界重建相同 snapshot，不受 wall clock、读取顺序或重复/compensating receipt 影响。目标 `PRD-WORLD_RUNTIME-043` 承接 root/revision/window anchors 与 attribution linkage，`PRD-WORLD_RUNTIME-001` 拥有 order、reducer/snapshot、持久化及确定性 replay；M4/domain profiles 拥有事实分类、bucket、yield/loss/utilization 与 blocker attribution 语义；game 拥有玩家解读与行动取舍。

本合同只冻结多阶段资源流的确定性边界，不声明当前 runtime、Agent、Viewer 或 pure API 已实现该能力，也不冻结品质数值/公式、buffer 数字、runtime schema/枚举、图/队列算法或 UI 布局。

### 电力与设施

- Owner-held electricity 是唯一当前口径：初始化清洗 Location electricity，运行时拒绝 Location 入账与已下线充放电路径。
- 这里的 `PowerStorage` 仅指已移除的 simulator facility/action/event 语义；不等同于仍由 runtime 专业域管理的 `m1_power_storage` builtin，也不得据此删除 legacy scenario runner 对 `require_power_storages` 的明确拒绝。
- `BuildFactory(factory.power.radiation.mk1)` 的设施注册、owner/location 继承和发电入账由 runtime 实现拥有。
- 历史包含 storage 或 Location pool 的快照/事件必须由专业实现明确兼容或拒绝，不能静默吞错。

#### BuildFactory site authority and construction power obligation

`BuildFactory` 的持久站点 authority 与事件/state 字段由 [`world-runtime` Factory site admission and construction power contract](../../world-runtime/prd.md#factory-site-admission-and-construction-power-contract) 唯一承接；本节只冻结 M4/domain 对这些字段的语义输入，不创建第二份 site registry 或 runtime schema。

- M4 construction profile 的唯一 runtime lookup 是 `WorldState.factory_construction_power_profiles: BTreeMap<factory_id, FactoryConstructionPowerProfileV1>`；map key 必须与提交的 `FactoryModuleSpec.factory_id` 逐字相等。每条 profile 必须冻结 `factory_id`、canonical `factory_kind`、`source_module_id`（direct profile 可为 `None`）、非负整数 `electricity_amount`、唯一 `authority_revision`、`active` 与本批次允许的 `start_only_sink` mode。amount/revision/source 必须来自 M4-governed profile authority event/state；不能从 caller payload、WASM module output、simulator `factory_build_electricity_cost`、`FactoryModuleSpec.base_power_draw`、maintenance、recipe power 或 Agent battery 推导。`FactoryProfileV1` 只是按 `factory_id` 索引的能力/recipe metadata，不是 construction power source。
- 对 `BuildFactoryWithModule`，请求 `module_id` 必须与 exact profile 的 `source_module_id` 相等；对 direct `BuildFactory`，不要求 caller 提供 module id，但仍必须按 exact `factory_id` 解析同一 governed profile。缺 profile、inactive、key 不匹配、source mismatch 或 revision 冲突都必须 `unknown/blocked`，不得由 caller fabricated power 补齐。
- `start_only_sink` 表示在 runtime admission 成功时从 builder-held electricity 一次性消费；它不形成 completion hold、未来 reservation、Location pool credit 或自动 top-up。power 缺失/冲突/过期时为 `unknown/blocked`，在首个 construction sink 前不得接受。
- Site authority 的 `site_id`、`location_id`、owner、explicit access allowlist、`active`、`chunk_ready` 与 revision，以及 builder 的 `AgentLocationAuthorityV1.agent_id/location_id/active/authority_revision`，必须来自同一 fresh authority snapshot。co-location 只接受两个 canonical `location_id` 精确相等；M4 不允许按 GeoPos 距离、最近地点、共址猜测、Agent 推荐、材料账本或客户端缓存补齐缺失事实；owner 自动 access，非 owner 只有显式授权，空 allowlist 不是公开访问。
- 建设材料 obligation 与 electricity obligation 都必须记录 payer/ledger、数量、before/after 与 authority revision，并和 `FactoryBuildStarted` 的 pending commitment 原子落地。global/world electricity、Location electricity、已移除的 storage path 不能作为隐式 payer。
- Site registration、owner/access/readiness 更新的认证 authority 与 revision transition 由 runtime/governance 入口拥有；M4 profile 只声明 domain prerequisite，不允许 module output 绕过 runtime admission。已接受 job 固定 site/location/power identity，后续 site metadata 漂移不能静默迁移或重算；新 site candidate 必须另建 intent。
- 旧 snapshot/event 可由 runtime legacy adapter 读取；缺少 site registry 或 power obligation 时不得 retroactively 充电、猜测 authority 或获得新建厂资格。旧 Location pool/PowerStorage 事件只能明确兼容或拒绝，不能静默转成 owner-held electricity。

本批次不冻结绝对 construction 数值来源之外的价格/公式，不实现 site topology/path search、relocation、maintenance/runway、recipe/terminal settlement、PowerPlant activation 或 Viewer/Agent surface；这些仍由 runtime、产品/gameplay 和相关专业合同分别拥有。M4/runtime/QA 必须用同一 fixture 验证：缺 site/access/readiness 或 power 时无 sink；builder-held electricity 单次结算；direct/module parity；pending/completion/replay exactly-once；旧事件只读兼容且不绕过新 authority。

### 动态电力报价与订单合同

- 动态报价配置保留 `dynamic_price_enabled`、base/min/max price、scarcity adjustment 与 allowed price band；`market_distance_price_per_km_bps` 只作为 deprecated snapshot/config 兼容字段读取，当前报价不使用距离调整。具体数值由当前配置和 runtime 测试拥有，不在本 PRD 冻结。
- 启用动态报价且提交价为零或未显式指定时，交易使用权威 current quote；显式价格超出 current quote 允许带宽时拒绝，并返回可解释的 quote/band 原因。
- current quote 由 base price 加 scarcity 调整后钳制在 min/max 范围；相同权威状态必须产生相同报价，event/receipt 记录实际 quoted price 与 settlement amount，replay 不重新报价。
- limit order 只有在权威 current quote 同时落入买卖双方 limits 时成交；买单按最高限价优先、卖单按最低限价优先，同价按更早 order identity。未匹配订单保持 open 且可取消，不代表 escrow 或保证成交。
- 本合同保留价格约束与确定性规则，但不声称 `power_sale_quote` / `energy_liquidity_preview` 已实现；玩家可读机会成本仍是对应 PR 与实际验证记录中的未收口债务。

### 工业方案成本与价值归因合同

跨工厂、原料、配方与物流的玩家决策还需要一条可对账的经济闭环：玩家先比较一个方案的预期总成本、可交付价值与主要风险，提交后再按实际生产、运输和终端结算结果核对偏差，最后决定继续、减量、改道、换配方或转向其他目标。本节只冻结归因语义与玩家决策边界，不新增价格、产率、税费、运输费率、维护公式、runtime schema 或数值平衡。

- 首次提交前的方案比较绑定 candidate/config 与 authority snapshot，measurement window 仅在该比较实际声明时提供；此时不要求尚未颁发的 root/revision。只有权威 accepted outcome 创建 root/revision/segment 后，承诺、actual 与后续比较才绑定已颁发的对应 identity；继续既有 operation 时引用其真实 root，不伪造新 root。分开表达 `planned`、`admissible`、`committed`、`actual` 四层。`planned` 是候选预计投入/产出，`admissible` 是当前 profile 允许的合法单位与路径，`committed` 是权威接受后真正形成的投入/容量/义务，`actual` 只能来自已结算的 production、transport、byproduct、delivery/terminal receipt，或 profile 支持的权威 factory/setup/maintenance/downtime settlement evidence；preview、推荐、accepted ack、估算 ETA 与 speculative hold 不能进入实际成本或价值。
- 成本归因至少覆盖：实际消耗或占用的原材料批次；owner-held electricity 的预计、hold、消耗与未满足部分；物流 tariff/损耗、边吞吐与 buffer/terminal 容量占用；以及 profile 明确支持时的 factory setup、maintenance 或 downtime 成本。未实现或缺失的维护、价格、容量、终端价值或损耗 authority 必须显示 `unknown/degraded`，不能按零成本、零损耗、无限容量或免费机会成本继续比较。
- 价值归因必须把主产物、副产物、在途/缓冲状态和终端用途分开。副产物只有取得其声明的 ledger、local service、trade 或 quarantine disposition 后，才可按 profile 支持的 value class 计入；无法处置的 branch 只能计为待决、风险或成本，不得用主产物价值抵销。`production receipt` 只证明实际产物进入合法账本、buffer 或显式在途承诺，不能单独计入 delivery value、需求减少、市场/服务结算或终端奖励；这些结果必须等待匹配的 delivery/terminal settlement receipt。
- 提交前 surface 从 `run_full`、`run_reduced_or_partial`、`wait_or_restore_input`、`reroute_or_change_source`、`hold_for_terminal` 与 `pivot_to_other_recipe` 中，只提供当前专业 profile 真实支持的子集。未支持的选项可作为不可用说明，必须给出原因且不可提交；已有选项的 blocked/unknown 前置仍可只读比较，不要求把整组动作先实现才能解释报价。每个候选应说明预计总成本的组成、可执行/未满足量、预计交付量或 value class、仍占用的容量/电力/库存、失败风险、下一复查点和推荐理由；不承诺固定利润、精确 ETA 或未来价格。没有足够 authority 计算方案总成本或终端价值时，候选只能标为 `unknown/degraded`，不能伪装为安全或高收益。
- 接受后必须沿同一 root/revision 形成一次 immutable plan baseline，并按生产、物流 handoff/arrival、profile 支持且已有权威 settlement evidence 的 factory/setup/maintenance/downtime、buffer/terminal admission 与 delivery/settlement 实际 receipts 累积对账。对账至少区分 `planned/admissible/committed/actual` 数量、成本、损耗、待决义务与价值结果；profile 支持的 compensating receipt 必须链接原 receipt，不能改写历史成本或价值。因果换线、配方/工厂/边/终端变化建立 parent-linked 新 revision/candidate，旧方案的成本、已发生损耗和未决价值不迁移成新方案收益。
- 失败恢复必须保留已经消费、仍占用、已损失与可追回的价值，并沿 profile 支持的 `wait`、`release`、`reduce`、`reroute`、`hold`、`rework`、`return`、`salvage`、`requote` 或 `pivot` 路径各生效一次。终端容量、owner、资格、需求或价格 authority 失效时，不得自动退款、免费补偿、静默改道或把产出收益当作已交付；玩家应能看到继续当前方案与改走新方案的机会成本。
- 同一 root/revision 下的重复 submit、arrival、reconnect、retry、restore 与 replay 只能重读同一成本/价值 disposition，不得复制输入 sink、power debit、运输损耗、产物、副产物、delivery value、奖励或重新计算历史价格。不同 accepted intents 即使 payload 相同，也必须分别归因，不能合并成本或价值。

该闭环保持 **world-first**：成本与价值来自同一权威世界历史及 receipt，而非客户端估算；保持 **emergence-first**：源头近、终端近、低损耗、少占容量或高价值副产物之间存在可理解的机会成本；保持 **persistent / auditable**：计划、承诺、实际成本、损耗、终端价值与补偿跨重连、恢复和 replay 延续；保持 **extensible**：未来的 market、service、maintenance 或不同材料 profile 可声明自己的 value/cost policy，但不能绕过共同的分层和结算边界。

`test_tier_required` 至少覆盖一个两来源/两路径 tradeoff fixture：同一配方分别选择“原料近但输出路径拥挤”和“终端近但输入损耗较高”，能从 preview 读出 planned/admissible 成本、可执行量、容量/电力占用、预计交付量或 `unknown/degraded` value class；提交后只形成一次 committed baseline，生产、物流到达与终端结算分别追加 actual cost/value，且 production receipt 不减少需求、不发交付奖励。还须覆盖原料/电力/物流容量漂移、运输损耗、mandatory byproduct destination 失效、终端 owner/资格/容量失效与 supported wait/reduce/reroute/hold/requote/pivot 恢复；已消费和仍占用价值可追溯，不能自动退款、免费转化或静默改道。重复 submit/arrival、重连、retry、restore 与 replay 不得复制任何 sink、损耗、产出、delivery value 或奖励。

`test_tier_full` 在此基础上覆盖三阶段以上的多输入/多输出链、跨窗口 plan revision 与 causal cutover、maintenance-enabled profile 的 actual maintenance settlement 与 downtime opportunity cost、maintenance-unknown profile 的 `unknown/degraded`（不得补零或宣称安全）、多个并发 root 的成本/容量争用、长期 WIP/buffer 占用、跨账本损耗、byproduct quarantine/rework/salvage/return/compensation，以及维护/停机 evidence、replay 与 compensation 各自单次归因；还须覆盖持久化恢复后 planned/committed/actual 对账和 Viewer/pure API/Agent 对成本组成、价值状态、primary blocker、机会成本与下一动作的一致性。本文不冻结 cost/value 公式、价格/税费、数值、队列、runtime schema 或 UI 布局。

## 5. Risks & Roadmap

- 状态、回放、WASM 请求和 builtin identity 的兼容风险必须由 runtime 回归覆盖。
- quote 债务若只在执行后事件中展示后果，仍会削弱工业经营选择的可读性；该风险保持 GitHub #2166 路由，直到对应专业实现与验证完成。
- Viewer industry graph 继续可消费工业语义，但不因本专题而证明 Viewer 当前交互或视觉验证通过。

## 6. Validation & Decision Record

- DRF-001: 当前工业资源流转不得恢复 `PowerStorage`、`DrawPower`、`StorePower` 或 Location electricity pool。
- DRF-002: 四组历史三件套删除前，已将其现行语义、完成态 provenance、未收口 #2166 债务与相邻专业 authority 收敛到本三件套及引用入口。
- DRF-003: 产品承诺与当前体验 verdict 不在本文新增；产品层和 QA 仍须以各自权威与 fresh evidence 为准。
- DRF-004 / `PRD-WORLD_SIMULATOR-047`: `test_tier_required` 至少覆盖两阶段适用成功、未知/不适用拒绝、2–3 条 parent edges 的 arrival-order permutations、atomic join 齐套前无 sink/WIP/progress、missing/invalid/expired parent 的 join_pending/held/unmet 与 supported staged intake 的 remaining obligations、late arrival 只重评未决 parent、duplicate/replay 单次 join receipt；

  同一 allocation snapshot 上多 contender 的容量不足争用、full/partial/deferred/denied 守恒、释放后单次重评、buffer 满背压及根因/派生 blocker、稳定 tie/order witness、retry/reconnect/replay 不插队且单次效果；

  multi-tick stage→transit→buffer→terminal 路径覆盖 none/soft/hard window、所有 lease 有效的 on-time settlement、各边界 owner/route/capacity/expiry 漂移的一次 late/hold/reject 与一次 release、生产 on-time 但 transit/terminal late 无 delivery finality、renew/reconnect/replay 不延长时界/刷新顺位；

  preview/quote/speculative hold 不创建 plan baseline，full/partial/deferred/denied accepted outcome 各只原子创建一个 immutable baseline，atomic reject 不创建，后续 capacity/profile drift 不改写，parent-linked replan/cutover revision 分窗且 receipt 不跨 revision，同一 journal/state-root 与 measurement boundary 在 replay 后生成相同 diagnostic snapshot，receipt-only planned/actual 分层、互斥 backlog/join/WIP/transit/buffer/terminal buckets、loss/compensation 单次 reconciliation、primary root/secondary attribution、cutover 分窗与 unknown/degraded evidence；

  还须覆盖 full-only 与 supported-partial recipe 的 executable quantum、非量化请求的声明式 rounding/reject 与 residual、输入/容量漂移、partial capacity 不绕过 batch admissibility、主/副产物单次 totals；

  覆盖 all-output atomic bundle 全 branch 成功与单 branch owner/route/capacity/eligibility 失败无 credit/progress，以及 supported split fan-out 的 settled/pending/residual、late capacity release 只重评未决 branch、乱序/replay 不复制已结算 branch；

  分别在旧 hold、WIP、transit 与 buffer batch 存在时执行因果 cutover，证明稳定边界、旧/新身份分离、唯一 disposition、新窗口归零、未知/不兼容迁移 fail closed，且请求顺序置换或 replay 不改变结果；

  覆盖 terminal buffer 可用/已满、owner/需求失效、即时/在途 handoff、production/delivery settlement 分离、非 settlement 不解锁用途与重复 submit/arrival 单次 credit。

  `test_tier_full` 覆盖 long diagnostic-window rollover、3+ baseline revisions 与 checkpoint/recovery parent lineage、3+ stages/edges 跨窗口链、3+ input joins 与 3+ output branches、跨阶段不同 batch unit、不同规格批次的 split/merge 与共享 edge/buffer、跨账本运输损耗及副产物交付、parent/branch/window-specific failure/retry、长时量化无累计漂移、支持的 rework/salvage/return/handoff/conversion、重复换线、source/transit/destination 故障注入、乱序/重复事件、支持的 compensating receipt、持久化/边界 checkpoint 恢复/replay、长期争用无静默 starvation，以及 Viewer 与 pure API 的 lineage、baseline/revision、diagnostic/window/lease/join/batch/bundle/branch/分配/换线/终端结算结果、守恒、适用结论和单次效果一致性。
- DRF-004 checkpoint evidence: `test_tier_required` 还须覆盖 stage/cycle/join/bundle/branch/handoff 前后的合法 checkpoint、原子效果进行中时延后关闭、未完成 operation 的 parent-linked continuation 与 opening bucket 单次 carry、重复/乱序 checkpoint、retry/reconnect、snapshot restore/replay 得到同一 identity/closure 且无重复 receipt/hold release，以及 checkpoint 邻接 replan/cutover 和 missing/mismatched anchor fail closed。`test_tier_full` 覆盖 3+ continuation windows 的 stage→edge→buffer→terminal 链、checkpoint 后 failure/compensation、并发 revision 与持久化恢复；Viewer 与 pure API 必须表达同一 checkpoint/parent/baseline/bucket/continuation disposition。本文不冻结 cadence/TTL、schema/storage、metric formula、queue 或 snapshot 实现。
- DRF-004 root-operation evidence: `test_tier_required` 还须证明 3-stage join→bundle/fan-out→transit→buffer→terminal 的每个 receipt 都属于同一 root 与唯一 child，两个 payload 相同的并发 accepted intents 不混 root，deferred/denied 有 root 而 atomic reject 无 root；arrival/reorder/retry/reconnect/replay/recovery 不改变链接或复制效果。Checkpoint/non-causal revision 保持 root，causal cutover 建立 child root 且不继承 W/receipt/actual；validation crash/recovery 重读同一 child decision；missing/conflicting root/parent 与 closed-root late attachment fail closed。`test_tier_full` 覆盖 3+ nested joins/branches、多个 revisions/cutovers/checkpoints、rework/salvage/return/compensation 与持久化恢复；Viewer/pure API 对 player-authorized root path、segment、parent、finality 一致，不泄露无权读取的 raw identity。
- DRF-004 validation evidence: `test_tier_required` 还须覆盖 quote 无 decision/effect、matching applicable decision+settled/arrived parent 只解锁一次、decision-only 与 not_applicable/pending/unknown/expired/wrong-root-stage-batch fail closed；两个相同并发 roots 的 decision 不交叉解锁；module/profile/spec/owner/ledger drift 使用 frozen decision 或 linked revalidation policy 而不改写旧 receipt；validation 后 completion 前 crash/restore、duplicate/reordered event 与 replay 不重复 invoke 或改变 decision；atomic/split fan-out 分别遵循全 branch/逐 branch gate。`test_tier_full` 覆盖多阶段 joins、多个 validation epochs、cutover/rework/compensation、module release drift 与持久化恢复；Viewer/pure API 对 authorized root、decision status/scope/freshness、gating 与恢复一致。
- DRF-004 power evidence: `test_tier_required` 还须覆盖 start-only 在开始时只 debit 一次且后续余额漂移不重释旧 job；held-through 模式的 hold/consume/release 单次守恒；boundary-revalidated 模式在 stage/transit/buffer/terminal 边界的 owner/power drift；未纳入 power reservation 的 window 明确 best-effort；hard shortfall 在 sink 前 deferred/reject，soft shortfall 明示 at-risk/late，WIP 保留既有 disposition。并覆盖两个 roots 争用 owner power 的 deterministic full/partial/deferred/denied、unknown 不伪造 0、primary power blocker/secondary backlog、checkpoint/cutover/retry/reconnect/replay 无重复 debit/release/W/reward。`test_tier_full` 覆盖 3+ stages/branches、长期 power contention、多个 revisions/windows 与持久化恢复；Viewer/pure API 区分 path power mode、receipt、机会成本与 maintenance/runway。
