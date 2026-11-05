# 成熟世界同窗口经济审计设计

- 状态：active；本文是目标技术合同，正式来源适配、报告服务与消费者接线尚未验证。
- Owner role：runtime_engineer；共同审读：gameplay、qa、blockchain、agent、viewer。
- 适用范围：`native_economic_window_v1`，核对同一候选、同一 World/branch、有界 C0→C1 窗口的原生生产、零距离材料转移、Runtime OC 发行/财库分发/转账、原耐久与生产状态变化及其治理依据。
- 审读基线：源码 `f4a3b3919d8a86a8617b551ec031b0809e1b268d`，tree `5b345136ca019c73308356a1fa82412dd13ff9b9`；这是源码静态核对，不是实现、实跑或发布证据。
- 产品依据：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)、[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)。专业规则：[同候选经济审计与治理影响解释](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-mature-world-economic-audit)。

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5。

## 1. 问题、目标与非目标

原 Runtime 已有 OC 供给、发行、财库分发和内部告警，也有实际生产、材料转移、耐久及治理事件。当前这些事实没有构成一个可证明完整来源、按同一窗口分账、解释非 levy 收支与治理实际影响的正式审计结果。当前 OC helper 的累计净发行与一个调用者指定的发行索引，也不等于长期经济审计。

本设计补齐一个有限但完整的首版技术责任：在明确的候选、历史切点、资源/账本/能力范围和读权下，取得完整有界原件，逐种资源与真实账本核对期初、实际变化和期末，并解释适用政策到实际结果的因果链。成功结果是可复算的**声明范围内结论**及其来源、缺口、异常和恢复关联；来源不足时必须保留未知，已知异常不能被未知覆盖。

首版使用原 canonical native recipe、direct `TransferMaterial`、Runtime OC 原政策/发行/分发/转账及原 factory 状态，不增加经济规则。正常例只占四 tick，足以验证转换、分配、治理规则适用和被动能力下降同时可解释；它不能证明长期稳态、资源稀缺合理性、经济公平或成熟世界整体健康。MW-005 的全资源、长窗口、其他 writer、跨节点与玩家理解义务继续保留，产品设计覆盖只能记为部分覆盖。

非目标：不新增货币、收费、补偿、征费或结算 authority；不重跑 World、WASM、LLM 或治理以生成证据；不更改旧 OC 算法、阈值或 longrun gate；不把审计 `blocked/hold` 接成世界 Action 门禁；不将 Runtime OC 与真实链上同名币种、receipt 或外部资金账合并；不生成发行或 release verdict。

## 2. 上游约束与相关角色

gameplay 拥有读者承诺、作用域和解释义务；Runtime 拥有本设计的原件适配、单位/账本核对及纯计算；原 World/治理/账本 authority 拥有实际事件、权限和状态；blockchain 拥有 formal commit/history 证明等级及真实链事实；QA 独立建立预期和判据；Viewer、Agent 与发布证据消费者只按各自原权限消费报告，不接管经济或发布 authority。

### 2.1 需求承接与分配表

各行复用上述 MW-005 Trace；专业 AC 各自回链同一产品声明，不能把本表当新要求声明。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [AC-EA-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-01) | 固定候选、World/branch、C0/C1、规则和声明范围；证明完整窗口与所有适用 writer | [身份与完整窗口](mature-world-economic-audit.design.md#economic-audit-identity) | 原 commit/history authority 与同切点覆盖证明；Runtime 来源 adapter | 当前 ring journal、单个 epoch、客户端事件清单不能替代完整历史 |
| [AC-EA-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-02) | 对非 levy 生产/材料转移和 OC 分别核对形成、消费、转移及期末；不重计 | [分账与守恒](mature-world-economic-audit.design.md#economic-audit-accounting) | 原材料、资源、生产和 OC writer；QA 独立账本 | 不混单位，不把转换当同质数量守恒；未适配 writer 不被默认为零 |
| [AC-EA-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-03) | 保留耐久/能力等关键变化；原指标准确保留公式与时间范围 | [变化与原指标](mature-world-economic-audit.design.md#economic-audit-changes) | 原 factory/recipe/time 与 OC helper；gameplay 解释 | 累计指标不改名为窗口流量，窄指标不证明长期健康 |
| [AC-EA-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-04) | 原提案/授权、接受、政策适用和实际执行逐层关联 | [治理影响链](mature-world-economic-audit.design.md#economic-audit-governance) | 原治理 action admission、policy schedule、账本 publication；blockchain 正式来源 | 不以 Approved 状态补签完整法定治理，不以余额差倒推授权 |
| [AC-EA-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-05) | 缺证、冲突、异常及窄正常分项保留；局部正常不能消除缺口 | [结论与失败](mature-world-economic-audit.design.md#economic-audit-verdict) | 各原件 authority；QA 独立反例 | 不新增 World gate、自动修账或 release verdict |
| [AC-EA-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-06) | 当前读权、精确范围、记录/字节/证明/时限与并发均有界 | [权限与预算](mature-world-economic-audit.design.md#economic-audit-budget) | 原 current-root reader binding；有界存储/历史 reader | 不先 clone World、扫描 values/journal 再截断；不自动提高权限 |
| [AC-EA-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-07) | 同候选同窗口补证、版本化重核及持久保留旧缺口/异常 | [恢复与历史](mature-world-economic-audit.design.md#economic-audit-recovery) | 原历史保留与受权报告存储；QA evidence owner | 新候选/新窗口不是旧报告恢复，不重放经济效果 |
| [AC-EA-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-08) | 正式只读消费者共享同一来源、范围和结论，保留全域/发行边界 | [消费者边界](mature-world-economic-audit.design.md#economic-audit-consumers) | Viewer/Agent/QA、WorldService 与原发布 evidence authority | 静态图、文件、请求 ACK、helper green 不证明正式接线或整卡通过 |

## 3. 当前状态、目标状态与差距

| 对象/能力 | 当前状态（基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| OC 内部审计 | `World::main_token_economy_audit_report` 读取累计 issued/burned、指定 epoch 记录和当前供给；分发财库使用全表 `values()`；缺发行记录返回零 | 保留原子结果的原义，新增独立的同窗口分账结果 | 无候选/根/完整窗口/读权关联，不能直接作为有界来源 reader | [原 helper](../../../crates/oasis7_runtime/src/world/main_token_economy_audit.rs)；Runtime |
| longrun consumer | 原 `evaluate_longrun_operability_release_gate` 还消费 SLA、backpressure、当前 journal rollback、紧急制动与 OC alerts | 新审计只作为其原 authority 可选择读取的 evidence，不改旧算法/准入 | 调用者 stage/epoch 不绑定完整经济窗口；没有因此获得全域审计 | [原 gate](../../../crates/oasis7_runtime/src/world/operability_release_gate.rs) |
| 原经济写入 | 原生材料转移/recipe、factory、OC publication 和治理 admission 存在 | 精确引用每次实际 writer 的前后值、原动作、规则和提交 | 原 reducer 不是新的审计 adapter，测试 setter 不证明合法产生路径 | §4.2 实际 writer 表 |
| 原件读取与历史 | 已有 exact-root OC policy cell 读取、formal business history 及 verified view 的分层合同 | 同 cut 的有界 source adapter，覆盖所有必需字段及完整窗口 | policy cell 只覆盖其原 schema 家族；原世界 journal 会裁前缀；完整经济索引/组合 reader 未接通 | [policy reader](../../../crates/oasis7_world_state/src/incremental_main_token_policy_records_read.rs)、[formal history](../../../crates/oasis7_game_client/src/formal_business_history.rs)、[publication](../../../crates/oasis7_runtime/src/world/event_processing/publication.rs) |
| 正式 producer | `/v1/chain/main-token/submit` 当前 main-token command enum 支持 genesis 与 vesting；原 owner/delegation 路径可承接部分 native 动作 | 每项被审实际动作都有当时合法入口及原接纳/执行证明 | 原受控 action 可计算，不代表 issuance/distribution/transfer 均已有该 HTTP producer；提交 ACK 不等于执行 | [main-token submit API](../../../crates/oasis7_chain_runtime/src/host/main_token_submit_api.rs)、[原 delegation 校验](../../../crates/oasis7_runtime/src/world/agent_delegation.rs) |
| 报告与消费 | 未找到满足本合同的正式审计服务、完整持久报告及玩家/QA 共享入口 | 目标只读报告、不可变版本与受权投影 | 本文接口名是目标职责，不是现有 endpoint 或已实现 schema | Runtime/QA/Viewer/Agent 共同 owner |

本表的“当前”只表示列明源码存在；没有执行过本设计正常例、性能、网络或浏览器验收。已有 OC/longrun 测试验证其窄机制，部分使用直接设置供给、财库或记录的 fixture；这些 setter 不能作为本设计正常来源。正在独立演进的存储工作及在途 PR 不构成本审读基线已具备的能力。

## 4. 边界与结构

### 4.1 只读组件和信任边界

`EconomicAuditSourceV1`（目标）从原 World/账本/治理与 formal history authority 取得精确投影和覆盖证明；`EconomicWindowAuditV1` 计算器（目标）只对该有限输入执行确定核算；现有 WorldService/QA evidence 设施中的报告存储与受权读层（目标）持久化报告及原件引用；Viewer、Agent、QA 和发布 evidence 读取同一报告版本。

这些职责不增加独立经济服务、World 状态、货币/治理 writer 或权限。计算器没有 `step`、action submit、WASM sandbox、LLM、扣费/退款或修账 capability。来源中已有的执行收费照实核对，审计本身不调用收费的业务入口。报告写入只是 evidence 存储，不能伪装成原 WorldEvent 或链 receipt。

<a id="economic-audit-identity"></a>
### 4.2 身份、完整窗口和真实 writer

请求固定 `candidate_ref`（source OID、实际 tested tree/artifact/config 身份及未验证字段）、`world_id/genesis_id/branch_id`、原 schema/manifest 与 profile revision，以及 C0/C1 的 qualified commit、state root 和原 history cursor/era。C0 是已闭合初态，窗口为 **(C0, C1]**；C1 包含选定终止阶段之前的全部原写入。必须证明 C0 是同一合法历史中的 C1 祖先，不能用 tick 相等、当前余额或文件 hash 推断。源码版本、候选与运行状态根是不同身份，不相互替代。

窗口内顺序按原提交/阶段/cursor；`WorldEvent.id/time` 及 `caused_by` 只保留其原义。跨恢复的 event era、分支和 commit 不能丢失。due recipe completion 可没有 `CausedBy::Action`，必须沿真实 job/start/original action 关联，而非因此断言无来源。原结构见 [WorldEvent](../../../crates/oasis7_world_state/src/world_event.rs)。

v1 要求**整个所选 World 窗口的有界 canonical 事件范围证明**，再依真实 write scope 分类声明的账本/能力范围，不接受客户端挑出的事件子集。目标 reader 在读取前取得同切点 range descriptor：原起止 cursor、era、所属 root、连续页边界、完整计数/水位、索引生成版本与证明等级；每页、空范围及范围终点均要有原 authority 的覆盖依据。适用 writer 索引必须在同一原 publication 下维护或可验证地绑定完整 canonical 范围；落后水位、缺页或只知道某事件存在，均不能证明其他事件不存在。没有这样的来源能力时返回 `source_unavailable`，不通过后台无界扫描补齐。

v1 的范围较小：超过 §9 的完整窗口上限就不能完成本 profile，不能先过滤掉坏事件、隐私不可读事件或超额事件再报告完整。证明与 scope 完全无关的 writer 可保留为 `outside_scope`；需要真实字段写集和所用规则依赖证明，不能仅看 event 名称。未知 writer 或无法判定相关性的隐藏写入使所需解释 blocked。下表是当前基线的分类规则；目标适配必须枚举真实 `WorldEventBody`/DomainEvent/治理 effect/reducer writer，而非只注册正常例的 action 名。

| writer 家族及实际来源 | v1 读取的键、范围和事实 | 处理与限制 |
| --- | --- | --- |
| `PreparedMaterialTransfer::prepare/install`；[direct transfer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transfer.rs) | 两端真实 `MaterialLedgerId`、material kind、原 `transfer_id` receipt、同动作接受与前后 cell；direct 条件和原权限/地点 | 支持零距离 direct 路径；两端等量变化。同一 receipt/event/state delta 只计一次。当前 native 要求匹配原 requester/source 与授权；receipt 本身不补授权 |
| `PreparedRecipeLifecycle`；[recipe lifecycle](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs)、[原 action/quote 入口](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy_support.rs) | factory、job/start/completion、consume/output ledger、canonical recipe/profile revision、实际 consume/produce/byproduct/power、settled/failure disposition；原 requester=builder=power owner | 支持 canonical native recipe；启动已消费与完成产出分别入账。pending、验证失败、blocked/resumed/paused 和生产 snapshot 不能漏；不借目标 adapter 执行一次 recipe |
| `process_factory_depreciation` 与原 factory reducer；[economy](../../../crates/oasis7_runtime/src/world/economy.rs)、[factory lifecycle](../../../crates/oasis7_world_state/src/state/industry_transition/factory_lifecycle.rs) | 每 tick 前后 durability、原 spec、pending/manufacturing workload、production state、原完整 factory/job 相关范围 | 保留原被动能力变化。零资源 delta 不豁免；相关制造 workload 未适配时不能用“无 recipe”推断 idle |
| `PreparedMainTokenMonetaryEvent::prepare_epoch/prepare_transfer`；[OC publication](../../../crates/oasis7_world_state/src/world/main_token_monetary_publication.rs) | supply cell、精确 epoch issuance key、四 treasury bucket、from/to account、transfer nonce、原 action 的 actual stake 与适用 config | 支持原发行和普通 OC 转账；发行目标索引与原处理时间分别保存，不从 cumulative 差额反推具体发行记录 |
| `prepare_policy/prepare_distribution`；[governance monetary publication](../../../crates/oasis7_world_state/src/world/main_token_governance_monetary_publication.rs)、[原分发 admission](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_core_main_token.rs) | 原 proposal/状态/授权、schedule effective key 与实际 config、distribution ID/bucket/recipients/processed time、受影响账户 | 支持原政策 schedule 和三种允许分发 bucket 的实际分发。proposal 被原机制接受与执行效果分层，不能扩大为完整法定治理证明 |
| genesis/vesting；同上 OC publication 的 `prepare_genesis/prepare_vesting` | C0 前合法初始化/申领的原动作、bucket、recipient、nonce、原 release 规则与 C0 supply/account | 用作正常期初来源；窗口内再次出现时，v1 尚未交付相应完整现金流 adapter，保留事实并阻断所需完整审计，不删除历史 |
| `prepare_fee` / `MainTokenFeeSettled`；同上 OC publication | 原 amount、burn、treasury、fee kind、当时 config、supply 与 treasury 实际变化 | 能读到真实供给/财库事实；没有 payer/account debit，完整账户现金流不成立。来源缺口和可证明 residual 异常并列，见 §5.4 |
| 其他当前/后续 writer | 原 [DomainEvent](../../../crates/oasis7_world_state/src/events/domain_event.rs)、[WorldEvent](../../../crates/oasis7_world_state/src/world_event.rs)、step 自动 lifecycle 与原实际 write scope | `ResourceTransferred`、Data/Power、node points bridge、restricted starter/Agent claim/upkeep、运输/损耗/tariff/escrow、建厂维护回收、其他工业/产品/经济合约、治理资源/资格/成本/义务/权限变更、module metering/effects 等，若影响声明范围而无完整适配则 blocked。旧 `materials` 与 `world` ledger 的兼容镜像、无经济影响的原元数据更新仍需分类去重；不把所有 ModuleEvent 或拒绝事件自动当无影响 |

完整 writer 分类还覆盖 [step_inner](../../../crates/oasis7_runtime/src/world/step.rs) 的动作前折旧、due economy/transit、world rule/gameplay/organization、restricted claim/Agent claim、consensus/cognition 等自动阶段及 raw/trusted publication。原执行代码可以有内部全表操作，但审计 reader 不能照搬那些扫描。源码、schema、effect 或自动阶段改变后，旧分类版本不能继续自称闭包完整。

### 4.3 原状态键和闭包规则

目标 source 以原 [WorldState](../../../crates/oasis7_world_state/src/state.rs) 的身份读取最小投影：`material_ledgers[(ledger, kind)]`；原 Agent 的 Electricity/Data；`factories[factory_id]`、原 recipe job/settled receipt/failure；`main_token_supply`、`main_token_balances[account_id]`、四 `main_token_treasury_balances[bucket]`、issuance/schedule/distribution/nonce 原 key。政策的有效值需要原 schedule 范围中 `<= target_index` 的最后一项及此前不存在更近适用项的证明，不能只取请求携带的 config。

余额缺 key、值为零、原 schema 默认零、历史无法读取是四种不同情况。只有同根存在/不存在证明与完整相关 writer 范围才能支撑“确实为零/没有发生”。不得调用返回 `unwrap_or(0)` 的普通 getter 来制造该证明。

ledger 身份按原 domain、owner/ledger ID、unit key 去重。canonical `FactoryBuilt` 的 input/output 都是 `Site(site_id)`，wire 为 `site:<site_id>`：这是**同一本材料账**，只以两种工序用途被引用；不能拆成两本资产重复加总。原兼容 `materials` 镜像也不是额外库存。Runtime OC 独立于材料、Electricity/Data、外部链币及节点积分；genesis bucket 的 allocated/claimed 元数据不是账户 vested 之外的另一份资产。

首版可对有界选定账本给出逐 cell 方程，同时独立检查 Runtime OC 总供给 counter。只有另行证明所有适用账户、vested/restricted、treasury、escrow/claim/bridge 等持有类别和完整成员范围，才允许写全域 OC 余额合计。正常例是两账户/四 bucket 的受控闭包；真实大 World 不能只选 A/B 就称全域配平。读者声明的义务包含全域而预算/来源不能完成时应 blocked，不能事后改成 A/B 窄结果冒充原请求成功。

## 5. 关键运行流程

### 5.1 请求、来源验证和计算

1. 当前读者显式选候选、C0/C1、scope 与原指标阈值来源。host 校验请求版本、当前读权、有效 root/history cut、祖先关系和预算；只建立只读计算任务，不提交 World action。
2. source 先取有界 range descriptor 和必需根证明；超量、未知 writer taxonomy、根/索引不一致或缺权限立即留下具体缺口。按同一 cut 获取完整窗口和期初/期末字段，再取精确原动作、治理、配方、授权和相关历史引用；每次读取含失败都计预算。
3. 按原提交顺序连接动作→接受/拒绝→实际 publication→原 receipt/job→状态变化，识别 no-effect、实际 consume 与实际 completed。去重依据原 effect identity；相同 ID 不同 body 是冲突，不能任取其一。
4. 计算器按 §5.2–5.4 建立独立账本、原指标、关键变化和治理链。它可以核算已经完整取证的局部字段，但不通过其他正常字段抹去缺证或异常。
5. 报告原子持久化完成记录后，消费者读取同一 report digest/revision 的受权投影；保存失败或未知时只查询原 request key，不能重做世界操作。结果附范围、证明等级、实际预算、未证事项和恢复关联。

<a id="economic-audit-accounting"></a>
### 5.2 分账和独立方程

对每个真实 `(domain, ledger, unit)` 分别计算：`residual = closing - opening - formed - inbound - released + consumed + outbound + lost + locked`。每项来自真实 writer，符号与所属资产类别固定；同一原 reservation/escrow 不能既算自由余额又算待交付。v1 正常 direct 路径没有 transit/escrow，若原范围出现这类资产而尚无 adapter，不伪造零项。内部转移只改变两端，闭包内净额为零；跨闭包转移保留边界流量。

recipe 核对原输入/输出向量，而非把不同材料相加要求数量相等。输入和 power 在 `RecipeStarted` 已消费；完成验证失败或仍 pending 时不得预记产出、自动退款或用计划补平。原 output 和 byproduct 分别标记真实 kind 与 ledger；产出在 site 不等于交付给另一玩家。Electricity、Data、每种 material 与 OC 不存在本审计引入的换算率。

Runtime OC 独立检查：`total_supply(C1)=total_supply(C0)+actual_issued-actual_burned`；每个 treasury bucket 分别以发行份额、实际收入和实际分发解释；每个账户 liquid/vested/restricted 依原类别分别核对。circulating 的原写入单列，与完整可流通账户闭包的合计再比较，不能把 circulating 当一个额外账户加到供给中。原 genesis/claim/nonce/receipt 是来源或元数据，不重复加入资产总额。

数值运算使用可检查的整数宽度和明确舍入；累加、乘法、符号转换或分母不合法时给出 arithmetic failure，不饱和成健康结果。新窗口指标可输出 `window_issued`、`window_burned`、`window_treasury_distributed` 和 signed `window_net_issued`；如展示比率，另名 `window_net_issued_bps_of_opening_supply = signed_trunc(10000 × window_net_issued / opening_supply)`，不钳制 10000，不对零分母补 1，零分母记 undefined。v1 不给这个新比率另设货币阈值。

<a id="economic-audit-changes"></a>
### 5.3 能力变化和旧 OC 指标

每个窗口保留 factory durability、active jobs、生产状态/阻塞原因、完成/失败事实和原生产 snapshot 的前后变化。原折旧发生在每 tick 动作之前，按原 maintenance 与负载计算；正常余额配平也不能推出耐久没有损失或产能可持续。若其他治理改变资格、成本、义务、权限或替代路径，必须纳入 scope 的解释或标记缺口，不因未写余额而忽略。

旧 [OC helper](../../../crates/oasis7_runtime/src/world/main_token_economy_audit.rs) 原义必须逐项保留：

- `mint_total/burn_total/net_flow` 是累计 `total_issued/total_burned/issued−burned`；不是 C0→C1。
- `issued_this_epoch` 按调用者指定 epoch key 读取；缺项默认零不是本设计的无事件证明。`treasury_distributed_this_epoch` 按 distribution 的 `distributed_epoch` 过滤，而该字段来自原处理 `now`。
- 分母为当前 `total_supply.max(1)`；净流量比率只取非负部分；bps 用整数除法并钳制至 10000；告警按严格 `>` 原阈值，默认分别 2500/1000/1200 bps。`gate_passed()` 只表示 alerts 为空。
- helper 对分发的 `.values()` 全量扫描和饱和累加不能作为新有界 adapter。目标只读报告若复现该子指标，必须取得其完整原依赖并注明原算术；数据不足时标 unknown，不能借新窗口总额替代原 epoch/lifetime 数据。

新窗口比率与旧 helper 并列、另名、单独注明分母/时间，不改变旧结果。原阈值告警只是一个 finding 家族；没有告警不能覆盖来源缺口、已知账本差额或全域未证。

<a id="economic-audit-governance"></a>
### 5.4 治理影响和费用缺口

读取原 proposal、其当时状态和原控制/授权，连接实际 action payload、原 admission/拒绝、schedule/分发 publication 及受影响字段。原 [monetary evaluator](../../../crates/oasis7_runtime/src/world/event_processing/main_token.rs) 接受 Approved/Applied proposal 等原条件；这些检查不能证明提案正文已完整批准任意 payload，不能升级为宪制或法定治理已完成。WorldState 的 `prepare_policy/prepare_distribution` 只凭 proposal_id 等结构/状态条件不会重新证明这些治理批准；raw publication 或事件携带该 ID 都不能替代 action producer、原 governance guard 与提案原件。缺少当时有效授权/正式 producer 时，只报告实际受控执行层，正式治理来源仍缺证。

时间必须分开：政策更新以原处理 World time 加 2 安排 effective key；发行按请求 `epoch_index` 查适用政策，该索引并不自动等于当前 World time；fee 则按原处理时 `state.time` 解析有效 config；treasury distribution 的 `distributed_epoch` 也是实际 `now`；原治理 epoch 又是 `time / max(epoch_length_ticks,1)`。因此报告写“政策被某一指定发行记录采用”，不能只写“治理已生效”并推及所有 writer、共识高度或真实链账本。

首版政策影响比较固定同一实际发行输入，重新按旧/新原公式计算差值，只解释局部规则作用。它不是另一真实 World、经济预测或长期治理因果实验。分发还要核对原允许 bucket、exact recipient list、实际扣财库/增 liquid/circulating 与 receipt；转账独立核对余额/nonce，不使总供给增长。

原 `MainTokenFeeSettled`/`prepare_fee` 没有 payer，也不写账户余额：它减 circulating 全额、减 total_supply 的 burn、增 total_burned 和相应 treasury。报告保留这些真实计数器/财库事实，同时写明付款账户与配对写入缺失，完整账户现金流 blocked。若账户闭包已有完整证据并能算出 `sum(liquid)-circulating ≠ 0` 或持有量/供给差额，还必须保留**已证异常**；缺 payer 不把该异常降成未知。不能补造扣款、删 fee、换窗口或改写原实现使报告配平。

### 5.5 独立可算的四 tick 正常例

此例是后续验证输入，本文没有运行。C0 前必须有真实合法原件：canonical Smelter 的建造/归属及既有持续生产历史、当前 A 的控制与读权、原材料/电力来源、无未解释待决义务、genesis/vesting 和治理 proposal。只安装 factory/Approved 状态/余额或使用测试 setter，不能证明这些前置；原受控 native 路径与正式 signed producer 证据分别记录。

canonical factory 的 builder/requester/power owner 均为 A，input/output 同为 `site:S`；A 经当前 owner 的原签名 delegation 获得 `agent:A->site:S:iron_ore`、`agent:A->site:S:carbon_fuel` 和该 factory 的 ScheduleRecipe 授权，实际 grant source/revision/期限/预算与 action 关联。当前显式 Transfer grant 不能从非 `agent:A` 的 site 源账本起转；所以本例只把原料送入 site，成品留在 site，不伪造 site→B 授权。当前 signed Build/其他正式 producer 的缺口不能被旧受控 FactoryBuilt 原件代签。

C0 的材料增量基线：A 持 iron_ore 4、carbon_fuel 1，`site:S` 的 ore/fuel 期初为 0；已有 iron_ingot/slag 及其他资产读取真实 C0 基线，不为本算例清零。A Electricity 至少 8。canonical iron recipe 为 `4 iron_ore + 1 carbon_fuel → 3 iron_ingot + 1 slag`，Electricity 8；本窗在原 site 成品基线上新增 iron3/slag1；consume 使用原 factory input，不触发 world fallback scarcity delay，base duration 1。原 [recipe catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)、[duration](../../../crates/oasis7_agent_api/src/world_quote/duration.rs)、[factory catalog](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs) 保持不变。

受控 OC 配置输入 initial_supply=1000、一 bucket 分配 10000 bps 给 A、start/cliff/linear 为 0，经原 `InitializeMainTokenGenesis` 和 `ClaimMainTokenVesting`（原 nonce）形成 C0：A liquid=1000、vested=0，circulating=total=1000，issued=burned=treasury=0，B 余额确证不存在/为零。原初始政策固定 300 bps（base=min=max，gain=0），epochs_per_year=10；新政策仅固定率改为 800 bps；split 沿原 6000/2000/1500/500，burn policy 不变，max supply 不截断。本 fixture 使用原 validation 允许输入，不修改生产世界 hardened 初始供给/费率。

P/P2 在 C0 前具有真实原机制所要求的批准/授权原件。以 t1=t0+1、E=t1+2 记目标政策 key，八个请求按表中顺序处理。direct transfer 的 route_id=None、route_ids=[]、auto_reroute=false、effective distance=0、path=None；没有隐含多跳、tariff、WASM 或 module fee。其他自动 writer 是否无效果必须有完整范围证明，不能以本表未列出推断为零。

| 时点 | 原动作和实际效果 | 分账与关键变化 |
| --- | --- | --- |
| t1 | 动作前折旧；UpdateMainTokenPolicy(P) 安排 E；两次 A→site 原料 direct transfer；ScheduleRecipe | ore4/fuel1 先等量转移再消费；A Electricity−8，job ready_at=t2；durability−1000 ppm |
| t2 | 动作前折旧；ApplyMainTokenEpochIssuance(E−1, actual_stake=6000)；动作后 due recipe completion | 发行 floor(1000×300/(10×10000))=3；site 产 iron3/slag1；折旧时一个 pending job/2 slots，durability−1500 ppm |
| t3 | 动作前折旧；ApplyMainTokenEpochIssuance(E, actual_stake=6000) | 前次发行只进 treasury，circulating 仍 1000，发行 floor(1000×800/(10×10000))=8；durability−1000 ppm |
| t4/C1 | 动作前折旧；DistributeMainTokenTreasury(P2, unique ID, staking_reward_pool, B:2)；TransferMainToken(A→B:10, valid nonce)；本 tick 原自动阶段闭合 | 分发使 treasury−2、B liquid/circulating+2；转账 A−10/B+10，供给不变；durability−1000 ppm |

原 `prepare_factory_depreciation_event_bodies` 用 maintenance_per_tick=1、每单位 1000 ppm、`util=min(active_jobs×10000/slots,10000)`，`decay=floor(1000×(10000+util)/10000)`。四 tick 合计 4500 ppm，终态 `D0−4500`，D0 从合法 C0 读取且足够满足原生产条件。不是每 tick 减 1，也不能为消除折旧将 Smelter 的 maintenance 改成 0；recipe maintenance_inputs 和 factory 被动折旧是不同来源。

| 单位/账本 | 独立预期结果 |
| --- | --- |
| agent:A 与 site:S 的 iron_ore/carbon_fuel | A 分别 −4/−1；site 先 +4/+1 再 −4/−1，期末该两 kind 为 0；转移不算全域新产出 |
| site:S 的 iron_ingot/slag | 分别 +3/+1；只记一次 site 资产，不能按 input/output 字段分成两份 |
| A Electricity / Data | Electricity−8；Data 或其他资源为零变化只在完整来源确证无适用 writer 后成立 |
| 两次 OC split | 发行3→staking1/node0/ecosystem0/security2；发行8→staking4/node1/ecosystem1/security2；security 收原 floor 后剩余，不能独立 floor 成0 |
| C1 OC | A990、B12、circulating1002、total_supply1011、total_issued11、burn0；四 treasury 为3/1/1/4，合计9；1000+11=1011，990+12+9=1011 |
| 政策局部对照 | 同一实际 circulation1000，旧率得3、新率得8，局部差5；不是已执行的第二个反事实世界 |
| 旧 helper 与新窗口 | C1 调旧 helper 目标 E：累计 net11，bps108；issued(E)=8，bps79；分发发生 t4=E+1，因此目标 E 分发为0。新窗口分发为2，两者范围不同；新窗口 signed net/opening 比率为110 bps |

<a id="economic-audit-verdict"></a>
### 5.6 失败、异常与窄结论

报告至少分开保留 `coverage`（完整/缺证/冲突）、`reconciliation`（逐方程相等/已证残差/不可算）、`legacy_alerts`（原指标及来源）和 `governance_explanation`（批准/接受/适用/实际效果各层）。findings 是累积集合；其中一项 blocked 不能隐藏另一项已确定异常，全部旧 alerts 为空也不能自动令 coverage 完整。

只有声明范围的必需来源、权限、方程、规则适用和解释均满足时，消费者才能写“该范围已核对”。已证异常或必需来源缺失时，整体保持可解释的 hold/blocked；局部完成保留确切 field/ledger 范围，不显示为整个世界或 MW-005 green。错误候选、混用 root、坏 proof、算术溢出、同 ID 冲突、历史断口、未知 writer 和预算不足均不能默认为空、零、成功或自动回退到新 root。

一个可独立复算的费用反例：在正常 t4 转账后、同一 tick 内追加原 `SettleMainTokenFee(GasBaseFee,10)`；保持 burn3000 bps，原 writer 产生 burn3/treasury7。A990/B12 不变，circulating 从1002变992，total 从1011变1008，treasury 从9变16。完整账户闭包下 `1002−992=10`，且 `1002+16−1008=10`，都是已证异常；payer/配对写入缺失同时成立。供给 counter 的 `1000+11−3=1008` 成立也不能抵消两项差额。若动作实在 t5 执行，必须改窗口并核对额外折旧，不能套 t4 结果。

其他负例必须保留：删 due completion 时不可补造产物；漏一项被动折旧时能力解释不完整；只读 Approved 而没有 schedule/执行不报告政策效果；错用发行 epoch 作为 World 时窗、缺页却默认无发行、隐藏范围内非 v1 writer、重复 receipt、混合 Runtime/链账本、只挑正常账户均不能得到完整结论。来源缺口向用户仅披露其当前权限允许的类别与下一步。

## 6. 接口与数据合同

以下名称表达目标组件合同；目前不存在可据此宣称已提供的 endpoint、wire ABI 或正式入口。实现必须复用原 verified view/history 的 assurance 与 reader binding，不能自造 proof_level 名称替换它们；[VerifiedWorldView](../../../crates/oasis7_game_client/src/verified_world_view.rs) 的 signed service assertion 与独立 qualified formal proof 保持区别。

| 接口/条款 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| `EconomicAuditRequestV1` | 已鉴权 reader → 只读 host | request_key/digest、candidate_ref、world/genesis/branch、C0/C1、scope、profile/schema revision、原 threshold source、current reader authority ref、budget profile | 同 key 同 digest 读原任务；同 key 异内容拒绝；先校验身份再读取 | admitted 仅指只读任务接收；权限/预算/历史不满足返回具体 gap | 不嵌入旧 Action 或 main-token submit enum，不默认旧请求已有完整来源 |
| `EconomicAuditReadSetV1` | 原 source authority 的目标 adapter → 计算器 | 原 key/value revision/commit/proof、range descriptor、writer taxonomy、分页连续性与无事件证明 | 固定 (C0,C1]；原 effect 去重；C0/C1 point read 和 history 同根关联 | verified/confirmed absent/unavailable/conflicting 分别记录；越界停止 | 原 schema 未覆盖字段标 unsupported；不能以 policy-cell schema 覆盖全经济 |
| `EconomicWindowAuditV1` | 纯计算器 → 报告存储 | exact request identity、input digest、算法/profile revision、逐账本 opening/movements/closing/residual、原指标、关键变化、治理链、findings、实际预算 | 相同规范化输入产生同结论；不依墙钟/随机/模型 | coverage/reconciliation/alerts 分项；partial output 不标 completed scope | 无 release_passed、Action effect、付款/治理授权或自动修复字段 |
| `EconomicAuditReportRevisionV1` | evidence store → 受权消费者 | report digest/revision、原件保留引用、publication receipt、previous revision/gap links、读者投影身份 | 不可变版本；同一已完成任务查询幂等；补证新 revision | persisted/published 要有原存储确认；保存未知返回 unresolved | 历史版本保留其原算法/范围/证明等级；不以新版本静默改旧结论 |

request 的 scope 至少列真实账本/单位、factory/能力与治理主题，声明是否要求全域余额闭包；自由文本名称不当 authority。read set 只能携带有界必要字段，规范化摘要必须覆盖 source cut、数值、原件来源和覆盖信息。对外报告摘要允许受权裁剪，但不能删掉必需限制后保留原“完整”标题，也不能在不同 consumer 中各自重新推导一个 green。

## 7. 状态、事务与持久化

<a id="economic-audit-recovery"></a>
### 7.1 任务、报告版本和提交点

只读任务的阶段为 accepted→reading→evaluating→persisting→finished；取消、deadline、来源拒绝或进程中断可以结束本次读取并保存 incomplete 处置。该阶段描述工作是否完成，与报告中已知异常/未知证据是两条轴；`finished` 可以是一份完整保存的 blocked 报告，不表示经济通过。业务 World 不参加这个状态机。

报告完成提交必须原子关联 exact input digest、全部 findings、scope 与 evidence refs；不能先发布“通过”再补来源。原件 owner 仍是 World/账本/治理/正式历史系统；报告 store 保留受权不可变引用、可重核的必要证据投影及其原证明，不把自己保存的文件变成原事实 authority。存储沿原 evidence owner 的容量、访问与保留合同；容量不足返回 persistence gap，不能通过删除旧异常或无限缓存接受新报告。引用之外的原件若已淘汰、签名或证明不可取得，历史报告仍在，但新复核必须显示无法重证。

每个 request key 只对一个 digest；并发重复请求用原 evidence store 条件写入/CAS 得到同一任务或已完成 report ref。读取固定 immutable roots；World 后来继续前进不改本次窗口，也不让报告成为当前 release 结果。历史分支被回滚/不再 canonical 时，保留原 branch/context 并更新可见的当前适用性，不能改写旧发生过的账目或重放效果。

### 7.2 补证与重启恢复

补证请求使用新的 `request_key`，携带 `previous_report/revision`，并保留原 candidate、World/branch、C0/C1、scope 和算法版本；其 request digest 同时绑定新增材料引用及前一报告。相同补证 key/digest 幂等读取该补证任务，异内容拒绝；不得覆盖旧 finished task 或在旧 key 下静默再执行。新增材料必须证明属于同一原切点，当前读权重新校验。重新计算产生不可变新 report revision，并逐项标明哪个 gap 已消除、哪个异常仍在、哪些结论变化及原因。旧 revision、异常和未取到来源保留，不能覆盖成“从未有缺口”。

更改候选、窗口、scope 或算法是可区分的新核对，可链接旧报告但不能冒充补证恢复。没有可验证历史时保持 unknown；权限恢复不自动重新执行；任务结果未知先按 request key 查询。重启只恢复只读任务元数据和已保存状态，重新读取必须仍固定根并在全新有界预算内由明确重核请求触发；不无限后台重试，不产生 World 业务副作用。

## 8. 部署、安全与运行约束

<a id="economic-audit-budget"></a>
### 8.1 当前权限与最小读面

目标适配部署在现有受权 WorldService/QA 读取路径，不打开未认证全 World 导出。reader 对当前 root 下请求范围的权限，与历史某动作当时的执行权限分别验证；历史成功不授权当前阅读、当前拥有 Agent 也不自动取得其旧主人的私有历史。原 [formal business history](../../../crates/oasis7_game_client/src/formal_business_history.rs) 要求的完整请求/reader/原发布绑定不能被候选文件 hash 或客户端 supplied proof 替代。

进入 source 调用前检查 current authority，报告 publication/再次读取前复核相应读权；撤权后中止后续原件读取/披露，已取得的私有数据不能通过错误日志、gap 详情或共享 cache 绕过权限。当前不可取得、原件确认不存在、隐私遮蔽分别记录；不得将无权读取解释成零资产。必要来源不可披露时，报告只给允许的缺失类别和窄结论，不能借 service 权限向用户扩权。

不索取新密钥，不代用户签治理/经济动作。真实链与 Runtime 来源各自带原 domain/root/receipt identity 和 assurance，只有原映射合同明确证明某效果才可建立关联；同名 OC、相同数量或提交 ACK 均不足。

### 8.2 有界源读取与故障隔离

reader 对已验证根提供精确 point/range 投影；原完整范围的 header/计数/证明本身也计成本。source call 必须接受剩余 items/bytes/proof budget 与 cancellation，先检查长度再分配内存，嵌套集合每个 entry 都计数。重复页、失败读、拒绝 proof、空结果和网络重试均消耗本次预算，不能在 adapter 外隐式重试。分页只能使用原固定 cut 的 continuation，不能切到最新 root；越界或缺页使完整性不成立。

当前 `main_token_account_balances().values().cloned().collect()`、helper 分发表扫描、完整 World clone、全部 journal 物化及先遍历全部对象再挑 key 都不是 v1 reader。现有 exact-root policy 读取原语不能凭名称扩大成不存在的完整经济 reader。目标完整历史与字段投影接缝未完成时禁用完整报告 claim，保留原经济运行与原读入口。

## 9. 质量与容量

### 9.1 工程初值和共同预算

以下是首版受控、单 host 的工程上限，非经济规则或已测 SLA。修改须保留原权限/完整性并重新验证同条件组合；超限结束本次只读核对，不扩大业务权限、不丢超额项后报告通过。

| 预算项 | v1 初值与计数规则 |
| --- | --- |
| 时间/范围 | 窗口最多64 tick；即使 tick 少也必须满足完整事件/来源预算；请求编码≤16 KiB |
| 原件逻辑量 | 总 source logical items≤256，其中窗口 event items≤128；每原件及其必要编码≤4 KiB；嵌套 list/map entries 同计，总 ledger/unit keys≤64 |
| 读取调用 | ≤64 次 source call；单页≤16 items；range header、point read、失败/空页、proof 获取同计；每次调用有剩余全局预算 |
| 字节/证明 | raw 原件累计≤1 MiB，decoded 投影≤2 MiB，proof≤2 MiB 且≤4096 nodes；证明节点/路径按实际编码与解码成本计，不能把任意巨树算一次读取 |
| 内存/输出 | 工作区≤1 MiB，report 编码≤128 KiB；每任务保留8 MiB，上述 input/proof/output/工作区必须在该保留内；4并发最多32 MiB任务内存，host原有固定开销另列 |
| 结束/并发 | 单任务 host deadline 2 s、单次尝试、4并发；未占槽请求立即给 busy，不形成无界等待队列；deadline 只是未实测结束边界，未承诺 P95 延迟 |

raw/decoded/proof/workspace/report/request 的预算合计为 `1+2+2+1+0.125+0.015625 = 6.140625 MiB < 8 MiB`，剩余是有界索引/allocator metadata 余量，实际实现必须测量并计入，不能视作无限余量。至少在所有大分配前检查对应限额；proof解码的增量及索引也不可绕过总保留。单项 cap 和所有总 cap 同时适用。

### 9.2 正常路径的可复算输入包络

四 tick 例按真实 key 去重后有两材料账本、四相关材料 kind、A 的两个资源 kind、两 OC 账户、四 treasury、一个 supply、一个 factory、一个 recipe job、两 issuance、一个 schedule 和一个 distribution；input/output 同指 site，不能借字段名增成两账。完整成员/无事件证明及历史授权仍必须取，不能只计这些余额。后续验收按以下 envelope 分配总 source items，若真实所需闭包超量则本例尚未满足预算，不声称已经跑通：

| 来源组 | 最大 logical items 分配 | 需取得的内容 |
| --- | --- | --- |
| 窗口原事件与八个 action 引用 | 64 | 原接受/执行、四折旧、due completion、自动阶段及 consensus 元数据；不能只数经济事件 |
| C0/C1 状态投影、成员与无事件依据 | 64 | 去重 ledger/unit、账户/四财库/supply、factory/job、受影响状态及闭包范围 |
| 原历史/规则来源 | 64 | 原建造/成熟生产、genesis/vesting、配方/工厂/货币配置和原 policy 选择证据 |
| 当前/历史权限、治理和读范围 | 40 | 原 reader/control、三 delegation、P/P2、动作认证/原接受关联及必需 schema/manifest |
| 嵌套 entry 与覆盖 header 余量 | 24 | recipients、split/input/output vectors、完整范围页头等按实际重复/嵌套规则计数 |
| 合计 | 256 | 每项≤4 KiB，故 raw≤1 MiB；proof、decoded 与总内存仍单独受限 |

上述数目是待验证的最大输入包络，不是对当前未知正式证明编码的测量。读取依赖依次是身份/权限与根→完整范围描述→同 cut 窗口及端点→精确原因/规则→核算→一次 evidence publication。C0/C1 点读可在已验证身份后并行；range 页依原 continuation 顺序。以每页16条计，256条至多16数据页，连同身份/证明/补充点读仍须≤64 calls；不能以“一次 API”隐藏更多 IO。

4096 proof nodes/2 MiB 的真实可行性、2 s 内完成率、allocator 开销与正式存储网络延迟均未实测。若根证明/成熟前置无法在共同上限内取得，应返回明确 budget/source gap，由 Runtime/storage owner 修改工程预算或另行设计可验证分段；不改原 fixture 事实、不忽略前置、也不以这张算术表宣称性能达标。分段合并仍要证明同 cut、无重叠/无缺口和完整 writer 覆盖，当前 v1 不提供跨任意窗口自动合并。

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| 正常与重算确定性 | 同一受控候选、四 tick、原件包≤上述共同上限，1/4并发分别记录 | 同 input digest 输出相同分账/事实结论；不新增 World event | §5 独立原公式及上述工程初值 | [EA-02](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-02)、[EA-06](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-06) | 仅设计，未实跑 |
| 超限/取消/坏证明 | 第257项、第129窗口事件、过大单项/嵌套/proof、2 s deadline及取消 | 完整性不成立，保存允许的缺口，无自动业务重试 | §8–9 预算与权限合同；测实际峰值/调用/时长，不能只看返回码 | [EA-06](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-06) | 未实测性能/网络 |
| 恢复和消费者一致性 | 同 candidate/window 补证，存储未知/重启/撤权；同 report revision 多 consumer | 原异常保留、受权重新读取、同结论/范围；无第二次经济效果 | §7–8 和原 formal read authority | [EA-07](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-07)、[EA-08](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-08) | 未证明持久恢复、跨节点/平台 |

## 10. 兼容、迁移与回滚

既有 OC report/gate、WorldAction/DomainEvent、货币参数、经济执行和 formal read assurance 不因本设计改变。目标报告使用显式新 profile/schema/算法版本；旧记录缺少候选、根、payer、完整 range 或权限来源时保留 unknown，不能给缺字段补虚构默认值后自动迁移为完整审计。旧 snapshot/服务 assertion 只按其原证明等级消费。

实施顺序由原 authority 分别交付：有界原件投影/完整历史范围与当前读权→各 writer adapter/独立核算→持久报告/恢复→正式受权消费者。每项只在对应真实来源和同候选证据成立后声明能力；policy-cell 或其他在途存储改变不自动完成这些步骤。本设计不新增开发准入协议。

禁用或回滚目标 audit consumer 时停止新核对/披露，回到本审读基线的原读入口与原 helper；已持久报告以原版本继续保留，必要时标当前不再适用。它们不是经济事实，删除报告也不能撤销实际发行、分发、材料消费或历史异常；业务修复仍由原 owner 的独立流程处理。

## 11. 验证设计与可追溯性

正常 oracle 必须从原始动作、原配方/配置、前后状态和权威历史独立计算；不得从被测报告字段反推预期。手册保留正常路径、遗漏/冲突/超限/费用残差等反例与 source/source-absent 的区别。本文没有运行这些方法；文件/链接/追踪检查通过只证明文档相应结构。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [AC-EA-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-01) | [身份与完整窗口](mature-world-economic-audit.design.md#economic-audit-identity) | 同候选/完整窗口、原 writer 闭包和 C0/C1 身份 | [EA-01](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-01)：固定候选与受权 canonical cut，核正常来源和错根/缺页/未知 writer | 原窗口/范围证明、身份与分类对照、缺口记录 | 截断 journal、纯 fixture 不证明正式历史接线 |
| [AC-EA-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-02) | [分账与守恒](mature-world-economic-audit.design.md#economic-audit-accounting) | 逐真实 ledger/unit 方程、非 levy 转换/转移和 OC 独立核算 | [EA-02](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-02)：同 C0/C1 独立算3/8/2/10、site与资源账，注入遗漏/重复/错误单位 | 独立预期表、原 state/event/receipt 与 residual 比对 | 不推算全世界库存、公平/长期经济或外部链币 |
| [AC-EA-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-03) | [变化与原指标](mature-world-economic-audit.design.md#economic-audit-changes) | 原折旧/能力与旧累计/epoch 指标范围准确 | [EA-03](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-03)：核四 tick −4500 ppm及原 helper范围；漏能力/错epoch不可通过 | 原 factory/job/time、helper公式与新窗口独立对照 | 非 v1 工业能力及长期趋势仍缺覆盖 |
| [AC-EA-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-04) | [治理影响链](mature-world-economic-audit.design.md#economic-audit-governance) | 原批准/授权、接受、适用和实际影响逐层可追溯 | [EA-04](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-04)：P/P2原件、指定发行索引、局部3→8对照与实际分发 | 原 admission/action/config/publication 链；未证治理层说明 | generic Approved 不证明完整法定治理/正式 producer |
| [AC-EA-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-05) | [结论与失败](mature-world-economic-audit.design.md#economic-audit-verdict) | 缺证与已知异常并列，局部正常不升格 | [EA-05](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-05)：同tick fee10及独立residual10、缺payer、原helper green/全域claim负例 | 分项 findings、原费用/余额/供给对照、被拒绝claim | 不修复当前fee实现，不提供World/release gate |
| [AC-EA-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-06) | [权限与预算](mature-world-economic-audit.design.md#economic-audit-budget) | 当前读权、完整有界来源、预分配/读取/证明/取消均计费于预算 | [EA-06](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-06)：受权/撤权、cap+1、嵌套/坏proof，记录真实读取和峰值 | 调用计数、字节/节点/内存/时长、权限拒绝及无World写入证据 | 设计初值不是生产性能/隐私/跨平台实测 |
| [AC-EA-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-07) | [恢复与历史](mature-world-economic-audit.design.md#economic-audit-recovery) | 同候选补证、新revision及旧缺口/异常不可变保留 | [EA-07](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-07)：同window补证、换window、保存未知/重启/撤权 | request/report digest、原件与revision链、重核差异 | 不承诺未实现存储或原历史无限保留 |
| [AC-EA-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-08) | [消费者边界](mature-world-economic-audit.design.md#economic-audit-consumers) | 正式只读consumer共享范围与结果，保留更广义务 | [EA-08](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-08)：同candidate的Viewer/Agent/QA读取同report；按实际层核端到端证据 | 正式入口、受权投影、同revision对照和未证层清单 | SVG、静态文件、组件通过不证明真人/S8–S10/发行 |

<a id="economic-audit-consumers"></a>
### 11.2 正式消费者与完成边界

首版具体消费者是受权的经济审计详情：读者可定位本批 ore/fuel 如何被消费、iron/slag 留在哪一本账、A 的 Electricity 与 factory 耐久如何变化、两次发行用了哪一版政策、B 的2分发和10转账来自哪里，并从每个结论回到允许披露的原件。Viewer 的 [T-ECO-08](../../world-simulator/viewer/viewer-economic-targets-uiux.design.md#t-eco-08-req-wr-mw-005) 只提供已条件定义的表现入口；静态图不证明此目标 reader 已接通。

Agent/QA 消费同 report revision 的结构化事实/限制，而不是各跑一次旧 helper 得出另一“正常”。发布 evidence 只能引用该候选/范围/证明层，原 longrun、S8/S9/S10、正式节点、恢复、跨角色组合和真人理解证据继续由原 authority 判断。任何消费者都不得把范围核对完成改写为“经济无异常”“MW-005 完成”或“可发行”。

目标 source、计算、持久化和正式读者接线未完成时，可以审查原算法和受控预期，不能发布已形成正式经济审计的产品 claim。当前已实现的原经济动作与窄 helper 不被抹去；其已有能力和缺口按 §3 同时保留。

## 12. 决策、长期风险与未决问题

| 决策/风险 | 选择理由与后果 | Owner / 复核触发 |
| --- | --- | --- |
| 一个完整有界 profile，而非再包装 OC helper | 同窗口直接承接非 levy 生产/转移、OC 分配、能力与治理；首版规模小，完整World窗口超预算会诚实 blocked | Runtime/gameplay；新增 writer、规模或长期窗口需求时重新核闭包 |
| source completeness 先于摘要 | 余额差、最近 journal、旧 report/default0 无法证明遗漏不存在；完整范围/同根适配是必要缺口 | Runtime/storage/blockchain；正式历史/索引交付后以 EA-01/06验证，不借在途实现承诺 |
| 保留原 fee 事实与不平 | 不在审计设计里修货币写入；付款归因缺口和已证 residual 都必须暴露 | Runtime/QA；payer/扣款机制若正式改变，按新实际 schema/历史兼容另审 |
| 原生受控路径与正式入口分层 | 最小例可依据现有原 reducer/验证算清；不能冒充完整 signed governance/economy producer | Runtime/Agent/blockchain；补齐各 action 正式来源与合法授权后再声称相应层ready |
| 有界 source/证明/持久化仍为目标 | 预算可计算但真实 proof size、网络时长、内存与恢复尚未验证；不足时结束审计，不牺牲完整性 | Runtime/storage/QA；实际 reader 和存储实现后记录同条件测量及必要工程初值调整 |
| 全域长期与其余 writer 继续未覆盖 | first profile 不覆盖复杂物流、其他工业/claim/治理影响、跨节点长期组合及真人理解 | gameplay/对应 domain/QA；选新profile时复用原MW-005义务，不能用本文件存在代签整卡 |

本设计没有待用的新增经济规则或批准开关。未决的是上表具名来源/适配/验证的交付与证据；解除条件分别回到原 owner 和同候选方法，不通过降低上游义务、静默缩窗或把缺证字段改成零解除。
