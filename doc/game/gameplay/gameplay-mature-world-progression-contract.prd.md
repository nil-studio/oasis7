# 成熟世界小玩家成长合同

- 上层产品映射：本合同承接 `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md` 的成熟世界产品承诺与 `doc/game/prd.md` 的 `PRD-GAME-015`。
- 产品叶子入口：[`REQ-WR-MW-001`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；生产目标恢复比较承接 [`REQ-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002) / [`AC-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)；认可/机会专业叶子另承接 [`REQ-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)。本文保留成熟世界 lane、checkpoint、专业化、认可/机会玩家动作、生命周期、失败恢复与专业验收语义。
- 主题 authority：本文件拥有 generic recognition/opportunity 的 mature-world 适用性、事实/审核、作用域/用途/期限、预览/待决/receipt、重验、失效/复核/恢复与 anti-abuse 详细玩法语义，同时保留成熟世界 lane、checkpoint、专业化与恢复选择；不覆盖产品承诺、runtime schema、数值或 Viewer 布局。
- 可变执行状态：对应 GitHub Project task 与 issue evidence comments；当前实现完成度不得由本合同单独宣称。

## 2.9 成熟世界中的小玩家成长线

在 `PostOnboarding` 与首个持续能力之后，产品还必须回答：当世界已经存在更强组织、更深政治和更长历史时，小玩家/新玩家为什么还值得继续玩。

当前答案不应是“立刻加入大组织”，也不应只是“世界本来就很热闹”。正式路线应至少提供 1 条不依赖立即站队的 `small-player lane`，让玩家能在成熟世界里继续形成独立 leverage。

当前冻结的长期推荐轴是：

1. `local operator`：先建立并守住 1 条可恢复的小规模工业或服务能力，完成 1 次对世界有可见后果的阶段成果。
2. `regional specialist`：把这条能力转成短周期、区域性有用的专业化服务，而不是马上跳到全局治理或大型宏系统。
3. `limited-scope regional influence`：通过持续贡献获得有限且可审计的局部优先级、机会、可见度或协调位置，但不直接等价为 global governance 权力。

这些轴允许玩家按当前世界状态改道、重排或回退，不是必须逐级完成的职业树。组织、协议或治理等文明尺度项目只能作为自愿的共同扩展：它们可以形成更大范围的协作后果，但不能取代独立成长、成为唯一有效路线或构成全体玩家的胜利条件。

<a id="production-recovery-comparison"></a>
### 2.9.1 Disruption recovery comparison

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182；声明：[`REQ-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；产品验收：[`AC-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)。

当 disruption 阻断 active goal 时，gameplay 必须以同一个目标比较 `repair / rebuild / pivot`，并给出每条路线的时间/阶段成本、资源成本、保留/失去价值、主要风险和推荐理由。推荐应解释它如何最快或最可靠地恢复当前目标的玩家价值，而不是只按最低即时成本排序。

- `repair` 保持现有能力并修补关键缺口；`rebuild` 放弃或暂停旧位置/能力后重建同类能力；`pivot` 将已有投入转换为另一项能服务当前目标的区域用途。三者必须有可感知的恢复节奏或价值保留差异。
- 每项比较必须说明独立 `small-player lane` 是否仍可行。仅当独立路径当前确实不可行，才可推荐外部赞助或 major-power 依赖，并说明阻断约束、该依赖的用途及下次重评时机。
- 代表性验收至少覆盖一次局部停机、资源短缺、据点受压或路线失效：玩家能在同一 active goal 下作出有依据的选择，恢复后仍能回到本地立足、区域专业化或有限区域影响，而不是被静默降级为旁观者或强制站队。
- 本节只定义比较与可玩性验收；不新增 runtime 状态、数值、路径可用性算法或 Viewer 布局。

#### 2.9.1.1 本次生产比较的范围与权威

以下专业条款落实其中一个完整切片：首次持续能力之后，一个仍有效的生产目标被真实输入、能源、设施、站点或物流前置阻断时，玩家在同一目标和同一来源时点比较恢复方案、选择下一步，并在原执行结果后判断是否恢复。`repair / rebuild / pivot` 是方案分类，不是新增世界 Action。比较不会修复工厂、创建资源、冻结价格、替玩家选择或授予新的机会。一般据点攻防、组织站队、系统性危机恢复项目与尚无专业能力的迁移/改造不由本切片补齐；MW-002 的总体状态继续按其完整适用范围记录。

| 依赖权威 | 本比较复用的边界 |
| --- | --- |
| [M4 报价账本](../../world-simulator/m4/industrial-resource-flow-contract.prd.md#industrial-quote-ledger-semantics)与该文件的“工业方案成本与价值归因合同” | 实际消费账本、逐材料净缺口、原电力/材料规则、`planned / admissible / committed / actual` 与成本/价值归因；世界或其他主体的余额不能清除本工厂的缺口 |
| [工厂选址](./gameplay-industrial-factory-site-selection-contract.prd.md)与[工厂能力生命周期](./gameplay-industrial-factory-capability-lifecycle-contract.prd.md) | 当前 profile 支持的建设/能力选择、站点前置和旧工作处置；新建 receipt 不证明生产或交付完成 |
| [维护与计划停机](./gameplay-industrial-maintenance-planned-downtime-contract.prd.md)及[背压恢复](./gameplay-industrial-backpressure-recovery-contract.prd.md) | 维护事实、停机和容量原因各有自己的来源；不能用电池、低耐久或标签代替排产 blocker，不能自动暂停、释放、改道或退款 |
| [需求变化处置](./gameplay-industrial-demand-change-disposition-contract.prd.md)与[工业 cutover](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#industrial-cutover) | 改用途、接收方、配方、工厂或路径时，沿原专业规则处置旧工作并建立所需关联；不转移旧满足量、receipt、容量顺位、稳定窗口或奖励 |
| [原事务与恢复](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#industrial-transaction-recovery) | 真实执行、提交前重验、已发生效果和结果未知恢复；比较结果不替代原执行口或增加新世界准入规则 |

这些引用包含目标设计，不能据此宣称对应运行时能力已经存在。具体来源闭包、接口、上限、兼容和恢复技术由[生产恢复比较设计](../../world-runtime/runtime/production-recovery-comparison.design.md)拥有；本节的可玩性验收由[专用手册](../../testing/manual/production-recovery-comparison-acceptance.manual.md)承接。

#### 2.9.1.2 固定原目标，解释真实阻断

比较先说明“你原本要完成什么、哪项条件现在不成立”。原目标至少包含其权威身份/版本、玩家或被授权执行主体、材料与数量、适用工厂/地点/到达账本范围、用途与完成标准，以及该目标原本具有的接收方、期限或交付条件。没有某项条件时说明不适用；不得新加接收方、时限或交付门槛。`production_only` 的原目标按匹配产物入账判断；带交付要求的原目标继续等待其匹配 settlement，二者不能混用。

上述接受准则必须由原 goal authority 在创建目标或显式澄清/修改时接受，并绑定原目标 revision/content digest；比较只读消费已经接受的证明。若现有目标只有 `goal_text` / `soft_constraints`，没有可核查的材料、数量和允许账本准则，应返回 `goal_unresolved` 并进入原目标澄清入口，不能让模型解释、客户端复选框或本次比较中的一次确认补成已接受目标。真正改变产物、地点、用途、接收方或期限须按原目标权限形成显式 revision；澄清也不能静默放宽原硬约束。

来源还必须覆盖真实 factory/recipe/profile、所需输入与实际消费账本、相关电力/容量/站点/物流/权限、既有工作与原提交结果，标出时点和适用范围。已有权威 root/revision 时引用它；尚未接受的计划不能编造 root、已占用量或 receipt。旧报错可解释历史，当前 blocker 必须按当前来源重新判断；`goal_kind`、展示标签、自由文本中的“缺料”“赞助”或 Agent 建议均不能单独证明目标、路径或依赖。

读者、执行主体、资源所有者和目标受益者分别保留其身份。能够读比较不等于有权调料、建厂、改目标或使用他人产物；已有执行授权也不扩大私有目标、账本或其他主体承诺的读权。当前读权不足时只返回允许披露的缺失类别和下一步，不能通过隐藏候选的数量、精确余额或拒绝细节泄露他人状态。

#### 2.9.1.3 真实候选、可行性与原动作

每条候选写明方案分类、所服务的原目标、原专业能力支持的有序步骤、当前可开始的一步、后续成立条件、主要 blocker、预计产物/到达范围、旧工作处置和下次重评条件。路线之间须有真实的恢复节奏、投入、风险或保留价值差异；同一动作换三个标题不能算三个候选。

| 方向 | 对玩家的意义 | 必须证明或明确限制的内容 |
| --- | --- | --- |
| `repair` 修补当前路径 | 保留现有工厂或服务能力，补齐造成原目标受阻的具体条件 | 如合法调料进入实际消费账本、补足原电力义务或恢复受支持路径；步骤须能改变该 blocker。`MaintainFactory` 改善耐久本身不证明消除缺料、电力不足或排产拒绝 |
| `rebuild` 重建同类能力 | 承担新建设与准备阶段，绕开旧位置/能力的具体限制，继续服务原目标 | 合法站点、建设投入/能源、同类能力与原目标范围、新工厂输入/输出条件和旧工作处置均有来源。保留旧设施只表示不处理它，不自动回收旧建设成本；新设施不继承旧 WIP、预留或稳定进度 |
| `pivot` 转用现有投入 | 改配方或区域用途，保住仍可用的资源、能力或后续选择 | 只有原目标权威已允许替代产物/用途且当前候选确实满足它，才可称恢复原目标。否则明确 `needs_goal_change`：这是放弃、暂停或改写原目标的备选方向，需玩家按原权限显式改目标并处置旧义务；不能把另一个产物计为原目标完成 |

可行性分别表达“有完整来源支持、按列明条件可开始”“存在已知 blocker”“缺少或冲突的来源，尚不能判断”“当前执行 profile 未支持”。这些是玩家可读含义，字段由系统设计定义。多步方案可有可执行的准备步骤，但后续排产仍是条件结果，不能把“调料后再报价”写成“现在可排产”。缺少必要后续能力或关键成本来源时，不能将整条路线标为已核实可行；同样，已知当前不能直接排产并不否定一条来源完整、确实能补齐缺口的准备路线。

比较保留三类方向的解释，但不要求每次都有三条可用路线。代表性正常样例必须提供至少两条真实、有差别的同目标候选；其他情形如实说明已发现的路线、未覆盖范围与不可行/未知原因。只有一个已核实候选时，可以说明它在当前范围内能做什么，不能伪造第二条，也不能把“未找到”推成“全部独立路径均不可能”。

#### 2.9.1.4 成本、保留价值与阶段节奏

比较按每条路线分别列出：从现在起还需新增的资源投入；已经消费、无法由本方案自动追回的沉没投入；仍被旧工作占用的材料、电力或容量；允许保留/释放/回收的价值及其条件；下一步和后续阶段；主要失败风险与原目标的机会成本。按材料、资源、所有者和账本分别对账，保留原单位；不同材料的盈余不能抵销另一种缺口，也不能把材料、Electricity、阶段数与潜在收益相加为一个未经声明的分数。

`planned` 只表示预测，`admissible` 表示原规则允许，`committed` 只来自权威接受后的真实义务，`actual` 只来自对应已结算结果。只读比较不能新记一笔成本、回收、收益或预留。已消费材料不因选择 rebuild 自动返还；旧工作持有量不是可自由再用库存；production receipt 不等于交付、需求减少、机会解锁或地区服务收益。原目标如果只要求生产，也不得反向增设交付要求。

时间优先展示可核查的阶段与顺序，例如“调入原料 → 原账本重报 → 排产 → 原目标要求的入账/交付”。只有原 authority 提供有效时长/窗口时才展示相应数值，运输、排队与建设的不确定部分单列，不能用配方时长冒充整条恢复 ETA。原 profile 明确不适用的成本说明不适用；未声明或未取得维护/运输收费、损耗、价值来源时显示未跟踪或未知，不能推定不适用或补零。恢复没有额外的统一手续费、免费补偿、默认贷款或成功奖励。

旧工作仍按原 bucket 与 lineage 保留：未开工、WIP、在途、buffer、terminal-pending、已结算各自处置，不能重复计算同一批量。需要 cutover、目标变更、释放、回收或迁移时，仅消费原专业 authority 支持的结果；缺合法处置意味着该候选尚不能作完整恢复推荐。比较或切换标签不重置风险、不推进稳定窗口，也不复制历史成果。

#### 2.9.1.5 推荐、独立路径与具体取舍

推荐先解释该方案能否满足原目标及原期限/完成标准，再比较到达下一项有效世界结果的阶段、可靠性、仍可保留的能力、额外投入和暴露风险。应说明为何不选另一条有意义的候选，以及什么事实变化会改变推荐。产品不设统一权重或永远优先 repair；低即时成本但不能满足原用途、期限或合法处置的路线不能胜出，花费较多的 rebuild 也不能仅凭“新工厂更好”胜出。玩家可以选择另一条合法路线，或按原规则显式修改/暂停目标；比较不新增自动选择或执行授权。

独立路线只在已检查的主体、地点、能力、资源和时间范围内给出结论，并披露覆盖限制。有可行独立路线时，不得把赞助/联盟列为必需。缺证、无读权、预算截断、当前只检查一处工厂或未支持某类恢复，均不足以证明所有独立路线不可行。只有原权威能证明相关独立路线不可行，且外部依赖本身具有当前合法能力与明确条件时，才可将其列为受迫选项，并解释它解除哪个约束及何时重新比较。玩家自愿寻求外部合作继续服从原产品和授权规则，不能倒推独立路线失效；本节不创建赞助资格、组织准入或新机会。

正常缺料样例以同一个已生效目标 G 为比较基础：G 要求玩家 A 新增完成原配方一批，在原目标允许的 owner-bound 产物账本形成 3 单位 `iron_ingot`；已接受的准则预先允许 A 的 S0/S1 site ledger，不要求区域交付。S0 原 Smelter F0 有 1 单位 `carbon_fuel`、缺 4 单位 `iron_ore`。A 的材料账本有矿石 4、燃料 1 与原 Smelter 建材 `structural_frame 12 / heat_coil 4 / refractory_brick 6`，A 有 18 Electricity；空站点 S1 已有合法地点、建设授权及要求 10 Electricity 的 construction profile，其余原排产/输出前置均有效。原 [builtin 配方](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)的一批为 `iron_ore 4 + carbon_fuel 1 → iron_ingot 3 + slag 1`，需 8 Electricity，[原建材表](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs)按上述数量消费。建设电力来自此 fixture 的有效站点 profile；这些数值不是本节新定的经济规则，实际比较必须重新读取所选 profile。

| 同一初态的独立分叉 | 玩家看到的真实取舍与下一步 |
| --- | --- |
| repair F0/S0 | 按已授权的 direct、零距离转移把 A 的矿石 4 调入 S0 实际消费账本，重报后排产。该分叉消耗原配方 8 Electricity，A 剩 10；保留原 F0、全部建材和 A 的燃料 1，使用的是 S0 原有燃料。不能直接使用 world 余额，也不能把预计到料写成已入厂 |
| rebuild F1/S1 | 按原建设 profile 在 S1 新建同类工厂，消费上述建材和 10 Electricity；显式转入 A 的矿石 4/燃料 1 后重报排产，再消耗 8 Electricity，A 剩 0。保留旧 F0 及 S0 燃料 1；承担建设和准备阶段，不回收旧设施成本。只有来源另行证明旧站点受限、而 S1 能解除该限制时，才可把这种差异列为重建优势 |
| 另一配方/用途 | 若只生产别种材料且原 G 不接受该产物，显示“需要改目标，不能完成这笔铁锭目标”。说明可保留什么与放弃什么；不得把该方向与前两条一起标为 G 的已核实恢复 |

手册必须从同一初态分别执行这两条互斥分叉，给出完整合法 fixture、原建设/转移/配方来源与独立账本计算；两条路线不能合并使用同一份矿石或同一笔电力。正常样例预置原目标接受记录来验收目标设计，但该记录的生产适配尚未实现，不能伪称当前文本目标已具有机器可证准则。初始化 fixture 与实际恢复动作分证，不能用任意补库存代替执行调料，也不能把不存在的路径或建设能力当成已支持。若 F0 的调料条件完整且风险可接受，推荐 repair 应能解释保留能力与较少新增承诺；若其路径/时间条件不再满足，而 F1 的独立来源、建设和原目标条件完整，推荐可改变。该反向情景必须从真实变化推导，不得仅交换推荐文案。若 G 原本只接受 S0/F0 的结果，S1/F1 同样需要改目标，不能偷偷放宽 G。

#### 2.9.1.6 只读、时效、未知结果与有界重评

查看、切换比较候选、刷新和退出均不推进世界时间，不创建工厂、job、hold、root、目标修订或 receipt，不扣电/扣料、不改变优先级，也不执行维护、回收或补偿。读结果必须对应一个可复核的来源时点，返回时重验当前读权及决定推荐有效性的相关来源；漂移、失权或无法核实撤回当前推荐，保留允许展示的历史事实和明确的刷新/补证下一步。晚到的旧结果不能覆盖新目标、新草稿或新 revision，缓存不能把未知变成零成本或已核实可行。

玩家真正选择下一步时继续走原 Action、权限和提交规则，并按原规则重验原料、owner、设施、站点、配方及适用条件。比较过期本身不是新增 World 拒绝条件：当前原规则仍允许的动作保留可执行性；当前原规则不允许的动作正常拒绝或重报。一个准备步骤成功后，重新读取真实结果再判断后续步骤；旧比较不授权整条链盲目继续。

若原调料、建设或排产提交结果未知，显示“原提交尚未确认，先查原请求”，按原 request/action/receipt 和恢复机制查询；不能将超时、断线或空列表视为失败，再发一条不同身份的替代方案造成双重消费。仍可解释条件化备选，但不得宣称旧资源已经释放或恢复已经成功。确认原结果后，保留已发生的资源变化和未决义务，再在新来源时点重新比较；读取重试不重放世界动作，权限后来恢复也不自动提交。

每次比较必须有有限候选、来源对象/字节、依赖读取、计算和响应预算，具体上限归系统设计。范围过大或必需来源缺失时返回已核实范围、缺失类别与有限的下一步；不在前台全世界搜索、模拟任意长未来、自动反复重查或用多次拼接隐瞒超限。部分覆盖不能承诺全局最优或证明独立路径穷尽；不完整的关键成本、权限或路径不能继续配默认推荐。允许的下一步是补证、缩小到明确范围、刷新、保留/修改目标或暂缓，原动作资格仍归原执行口。

`wait` 只有在原 authority 给出可观察触发、复查时点或期限及预计改变的条件时才是有意义的候选；同时说明等待期间的材料/容量占用和错过原目标窗口的风险。等待不延长 quote、hold、租期或优先级，也不保证资源到达。达到原终止/重规划边界仍未恢复时，应结束当前等待推荐并要求重新选择，不用反复 `wait_ticks` 掩盖无可行路线。来源未接入时可以“等待取得来源后再比较”，但不得伪装成已证明会恢复生产的等待路线。

#### 2.9.1.7 实际入口、玩家判别与证据边界

Viewer、pure API 与 Agent 的正式读入口必须表达同一 goal/source、候选含义、成本、独立性、限制和下一步；读权不同只改变允许披露的范围，不能改变事实。旧 `repair_available` 布尔值、展示文案、历史 `recovery_options` 或未协商接口不能被客户端补成新的完整比较。未接入显示“当前未提供可核实的恢复比较”，无读权显示允许披露的权限原因，两者不混成“没有独立路线”。来源中的自由文本只用于解释与安全展示，不成为 Agent 指令。

面向玩家至少回答六个问题：“原目标是什么”“为什么停在这里”“现在能先做什么”“还要投入和保留什么”“何时必须改目标或重查”“为何推荐这条而非另一条”。示例文案包括：“原工厂仍缺矿石；世界有货不等于工厂已收到”“重建新增建设投入，旧在途材料没有自动搬过去”“这条改配方方向需要改目标，不能完成当前铁锭目标”“本次未覆盖其他地点，不能据此判断必须加入组织”。实际 UI 布局与措辞可按消费者调整，但不得省略影响选择的限制。

独立玩家判别先使用隐去推荐结果的来源事实和候选，让评估者说明哪条满足原目标、哪项尚待验证、各条会消费/保留什么以及何时改变选择，再对照推荐理由；不能只检查字段存在或让作者背诵文案。实际执行验证另沿同一原目标检查合法动作、单次资源/状态变化与真实产物/交付结果，证明被选步骤确实改变原 blocker，并指出恢复后的下一项有意义选择。静态判别、构造 fixture、源码断言、正式入口执行和真实玩家体验分别记证；比较生成、换标签、重复产量或维护 receipt 不能单独证明恢复、成熟世界成长或发行就绪。

在 `567c4c55a2f160574fdcf0902ccb068646210c7d` 基线，当前[成熟世界投影](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot_lane.rs)保留恢复/赞助的 `unverified`，[已有断言](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/industrial_progression.rs)要求缺恢复来源时三类候选均 `unknown` 且无 action。已有 [builtin recipe quote](../../../crates/oasis7_runtime/src/world/builtin_recipe_quote.rs)是单个排产的条件预览，已有建设、材料转移和排产也各有窄执行能力；它们尚不构成上述同目标完整比较、有限来源读取与三入口证据。本合同为目标设计，不把原先的定性投影、测试 fixture 或相邻目标设计当成新比较已实现。

#### 2.9.1.8 专业验收

<a id="ac-prc-01"></a>
##### AC-PRC-01 同一目标、真实阻断与来源

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182；声明：[`REQ-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；承接 [`AC-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)。

给定成熟世界一个有效生产目标及其真实 blocker，比较必须固定原目标的主体、材料/数量、适用范围与完成标准，消费原 goal authority 已接受且绑定 revision/digest 的准则与可复核当前来源，区分历史拒绝与现时条件。只有文本而无接受记录时为 goal_unresolved；模型、本次比较确认或省略原硬约束不能补成机器谓词。改变目标、消费账本或完成标准不能仍沿用旧结论；`goal_kind`、低耐久和文字标签不能替代因果。设计：[goal](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-goal)；方法：[01](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-01)。

<a id="ac-prc-02"></a>
##### AC-PRC-02 实际候选与原动作能力

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182；声明：[`REQ-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；承接 [`AC-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)。

正常缺料样例至少有两条来源完整、取舍不同且满足原目标的合法候选，各自映射到原 Action/专业 profile 与有序前置；可行、已知受阻、未知、未支持分开。世界有货不能跳过显式入厂，准备步骤可开始不表示后续排产已经准入；未支持的维护/迁移或同动作换标签不能凑数。设计：[candidates](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-candidates)；方法：[02](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-02)。

<a id="ac-prc-03"></a>
##### AC-PRC-03 成本、阶段与保留价值守恒

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182；声明：[`REQ-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；承接 [`AC-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)。

独立计算每条候选的阶段、逐资源/所有者/账本新增投入、沉没投入、旧工作占用与条件化可保留价值，并区分 planned/admissible/committed/actual。逐材料短缺、旧 WIP/在途/terminal 义务、byproduct 及未知成本不得互相抵销或补零；重建/改配方不自动退款、迁移、交付或复制稳定进度。设计：[costs](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-costs)；方法：[03](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-03)。

<a id="ac-prc-04"></a>
##### AC-PRC-04 目标适配、独立路线与推荐取舍

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182；声明：[`REQ-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；承接 [`AC-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)。

推荐必须解释服务原目标的理由、另一候选的真实取舍和改变推荐的条件；不以最低即时成本固定排序。替代产物/用途或新站点超出原接受准则时明确 needs_goal_change，须经原目标 authority 显式修改并绑定新 revision；未经变更不减少旧目标未满足量，本次比较不能代行修改。有独立路线或尚未穷尽相关来源时不得把外部依赖判为必需；受迫外部路线须给出真实约束、用途、原授权与重评条件。设计：[decision](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-decision)；方法：[04](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-04)。

<a id="ac-prc-05"></a>
##### AC-PRC-05 只读、当前权限与原执行恢复

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182；声明：[`REQ-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；承接 [`AC-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)。

预览、刷新、断线和重试不产生任何世界效果；失权或相关来源漂移撤回当前推荐，旧响应不覆盖新目标。真实动作只按原执行规则重验，过期比较不是新 World gate；原提交结果未知先恢复原请求，不能重发替代动作复制成本。保留原合法历史和未决义务，读权限不转化为执行权或他人私有来源访问权。设计：[execution](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-execution)；方法：[05](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-05)。

<a id="ac-prc-06"></a>
##### AC-PRC-06 有限比较、缺失范围与停止条件

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182；声明：[`REQ-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；承接 [`AC-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)。

一个正常两路线样例须在声明的有限预算内完整求值；超限、缺证、无读权或部分来源不得伪装为完整零成本、安全推荐或独立路径穷尽。等待必须给出原触发、复查和终止/重规划条件，不能无限刷新、隐式续期或自动继续旧计划。设计：[budget](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-budget)；方法：[06](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-06)。

<a id="ac-prc-07"></a>
##### AC-PRC-07 正式入口一致与独立玩家判别

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182；声明：[`REQ-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；承接 [`AC-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)。

Viewer、pure API 与 Agent 对同一允许披露范围给出同义比较，未接入与无权读取分别解释。独立评估者可不依赖推荐标签判断同目标可行路线、改目标方向、成本/保留价值与重查条件；实际执行另证明单次合法效果改变真实 blocker、按原目标完成标准判定结果，并保留下一项有意义选择。静态样例、源码证据与真实入口/玩家结果分别记录，不将比较本身算作生产、成长、机会授予或 readiness。设计：[consumers](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-consumers)；方法：[07](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-07)。

从 `local operator` 切到 `regional specialist` 之前，系统必须展示 `specialization_entry_quote` / `first_delivery_preview`：玩家要知道候选专业化的第一单交付会满足哪个本地需求、预计产出什么、需要哪些输入、多久形成价值、解锁哪种 `leverage_class`，以及交付后的回访 hook。否则专业化只是抽象标签，不能证明 mature-world 小玩家仍有可判断的经营取舍。

- 首单之后的持续区域交付也必须是可选择、可恢复的循环：玩家比较 `fulfill_next_order`、`reserve_capacity_for_local_need`、`reroute_or_pause_service` 与 `exit_specialization`，并看到当前需求、输入/容量窗口、预期区域价值、承诺期限/退出代价和下一次回访动作。缺料、路线、权限、需求失效或接收方不可用时，不得自动续约、吞没未交付库存或把推荐当成交付；玩家可 repair/rebuild/pivot、持有或重新报价。preview 只读且不授予长期市场资格/排他权；正向只产生一次可追溯交付与需求变化，负向/重连/replay 保持未交付状态。该补充不冻结服务费、质量、容量、合约公式或当前 runtime/Viewer 实现声明。
每个 small-player lane checkpoint 还必须展示 `leverage_checkpoint_summary`：`checkpoint_id`、`previous_leverage_class`、`new_leverage_class`、`new_option_unlocked`、`regional_usefulness_delta`、`recovery_resilience_delta`、`negotiation_position_delta`、`same_loop_repeat_count`、`grind_risk_reason`、`recommended_next_branch`、`leverage_checkpoint_class`。该 summary 需要将结果归类为 `new_option_unlocked / resilience_improved / negotiation_position_improved / regional_usefulness_increased / grind_only`；如果只展示 throughput、库存或同一产线重复执行，而没有新选择、恢复弹性、议价位或区域用途，不能判定为 small-player lane progression。

- 中期新增产能必须先给出 `expand_capacity_preview`：玩家比较 `add_parallel_line`、`upgrade_existing_line`、`stabilize_before_expand` 与 `defer`，并看到当前需求、输入/电力/物流可达性、预期新选择或区域用途、承诺/暴露的机会成本和回退动作。成功收益是打开不同的产能或服务路线；容量、权限、输入或电力不足时不得静默扩张、透支现有产线或伪造吞吐，玩家可修复、降载、等待或改道。preview 只读、不生成免费产出/重复里程碑；正向只形成一次可归因能力变化，负向/重连/replay 保持原产能。详细的已建工厂能力升级、重配置与退役选择见 [工厂能力生命周期合同](./gameplay-industrial-factory-capability-lifecycle-contract.prd.md)，仅在 profile 支持时表达且不改变本行扩容语义；该补充不冻结容量/产率/物流公式、状态机或当前实现声明。
这里所谓 `protected first industrial win`，保护的不是“不会被碰”，而是：

- 早期 footprint 小，不应一开始就与 major-power 主战略面重叠。
- 失败后存在 repair / rebuild / pivot 路径，不会立刻把玩家打回“只能投靠别人或只能退坑”。
- 玩家必须能明确回答“我做了什么、世界因此变了什么、下一步为什么仍值得继续”，而不是只看到世界自己在运转。
- 这条线不能只靠“再多做一点同样的工业”维持；每一阶段都必须新增一个 leverage class，例如更稳的恢复权、更短的交付周期、更有议价能力的局部服务位，或新的区域性选择权。
- 如果继续玩只能得到更高产量、库存、吞吐或重复次数，而没有新的局部用途、恢复弹性、协调位置或选择空间，这条线应判定为 grind-only，而不是 mature-world lane 成立。
- 如果专业化推荐缺少第一单交付预览，应标记 `specialization_delivery_preview_missing`，不得只用 `recovery_operator` / `conversion_specialist` / `regional_service_runner` 标签替代玩家侧收益说明。

这条线与 `PRD-GAME-012` 的 early-retention gate 保持分离；历史短期 focus order 见 [历史实施与验收记录](https://github.com/eng-cc/oasis7/issues/1192#issuecomment-6162409697)：
- `#165` 解决的是“首个持续能力之后如何继续有独立价值”，不是重新改写首个 10 分钟。
- 只有当成熟世界下的小玩家样本能持续给出 `player leverage != world activity only` 的证据时，这条线才算正式成立。

产品承诺见 [`doc/product/world-rules-core-gameplay/mature-world-progression.prd.md`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md)；本节拥有 lane、checkpoint、专业化与 anti-grind 的玩法合同。

Agent-facing 专业执行合同继续消费 `small_player_lane_id`、`leverage_class`、`same_loop_repeat_count`、`grind_only_flag`、`major_power_dependency_status`、`recovery_path_kind`、`requires_major_power_sponsorship` 以及 repair / rebuild / pivot 可用性，并保留 `selected_specialization_id`、`specialization_reason`、`preferred_next_action_class`、`dependency_boundary`、`recovery_escalation_reason` 等可解释摘要；若 guardrail 改写决策，还必须保留 `decision_rewrite` receipt。执行顺序默认先维持 `local_operator`，再按区域需求进入恢复、转换或区域服务专业化；只有玩家自愿升级，或 runtime 明确标记独立路线不可行时，才允许把 major-power dependency 作为有原因的升级路径。当 `same_loop_repeat_count >= 3` 且 leverage 仍是 `throughput_only` / `unclassified` 时，必须停止强化同一循环。

以下情况属于 blocker：独立路径可用时仍把 sponsor / alliance 写成必需；`grind_only` 后继续强化同一 throughput 循环；第一项专业化直接跳到全局治理、联盟领导或战争。没有 bounded canonical trigger 和复查时机时，`wait / wait_ticks` 不能替代 repair / rebuild / pivot。专业化预览缺少本地需求、第一项产出、交付时机或 leverage 解锁时，继续标记 `specialization_delivery_preview_missing`。

<a id="prd-game-015-recognition-opportunity"></a>
## 2.10 成熟世界认可与有限机会专业执行叶子

本叶子承接产品分册 [`REQ-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004) / [`AC-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004) 与 `MW-11` 的 gameplay 专业执行语义。它只适用于玩家完成首次持续能力后的成熟世界；不把首局结果、starter grant、普通行动成功或历史贡献自动写成认可、资格、优先级、容量或可使用机会。

本合同拥有玩家可操作的认可/机会循环、结果边界、失败恢复和可玩性验收；产品分册仍拥有玩家承诺与适用条件，runtime、Agent、Viewer/API、blockchain 与 QA 保留各自的状态、实现和证据 authority。本叶子不新增 runtime/API/schema 字段，不冻结数值、队列算法或 UI 布局，也不宣称当前实现或发行已就绪。

### 2.10.1 玩家动作与生命周期边界

成熟世界认可/机会循环按以下顺序对玩家可读；当前 `permission/authorization` 是提交与结算之间独立且必须持续有效的条件：

`相称事实或审核决定 -> 认可查看/复核 -> 机会预览 -> 待决提交 -> 有界 hold/排队 -> 当前条件重验 -> 单次 receipt -> 有限使用 -> 失效/复核/恢复`

1. `review_recognition_evidence`：玩家能看到认可所依据的相称世界事实或可复核的审核/治理决定，以及来源、地点/作用域、用途、开始与到期边界和适用的复核/申诉入口。单次行动成功、推荐或历史记录本身不能跳过事实/审核边界。
2. `preview_opportunity`：资格、邀请、推荐和预览只说明可以考虑或尝试的下一步，不产生世界资格、容量、排队顺位、优先级或其他效果。预览必须让玩家知道用途、作用域、期限、主要限制、当前 blocker 和失败后的替代方向。
3. `submit_opportunity`：提交绑定当下可复核且当前有效的 `permission/authorization`、来源、作用域、用途、期限、资格、容量和反滥用条件；在权威结果前保持 `pending`。有界 hold/排队只能表达暂时待决，不等于已经分配、已经成功或获得排他权。
4. `settle_once`：结算前重新读取当前事实与当前 `permission/authorization`；只有一份当前有效 receipt 才能形成一次可使用的有限机会。若提交后权限/授权缺失、被撤销或已过期，该旧 `pending` 必须先失效并转为拒绝、释放或过期之一，不能继续保持可结算状态，也不能产生 receipt 或任何世界效果；结果要说明原因与独立下一步。其他权限仍有效、仅因容量或资格等条件待决的请求，可以按既有规则继续保持 `pending`。receipt 至少能追溯认可来源、地点/作用域、容量单位、期限和实际世界效果，但不冻结字段名或 schema。认可不自动变成资产、OC、治理权、区域控制、全局权力或下一次行动成功。
5. `recover_or_appeal`：容量、资格、期限、来源、当前 `permission/authorization` 或反滥用事实变化时，玩家看到重新报价/拒绝/释放/继续待决的原因和下一步；系统不得自动重提、续期、跨区域携带或用历史认可补签。提交后权限/授权缺失、被撤销或已过期时，旧 `pending` 只能拒绝、释放或过期，不产生 receipt/世界效果并保留历史；若权限后来恢复，必须重新校验当前权限及其他条件并由玩家显式新提交，绝不自动重提。玩家仍可回到独立行动、补证、替代路线、repair/rebuild/pivot、恢复或申诉。
6. `bounded_abuse_review`：由本叶子 generic recognition 派生的认可/机会不可出售、出租、转让、拆分、叠加或由代理/组织代持。该 generic 规则不覆盖 [`FI-002 pioneer priority`](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002) 在其专门范围、条件与 authority 下允许的可转让性，也不重定义 FI-002 的资格、期限或重验条件。伪造、刷取、重复申领、循环背书、付费换取或批量自动化只能按预声明且可复核的审核/处置规则处理；未经审核的怀疑不得直接惩罚。处置必须说明事实类别、当前效果、复核路径和再次取得资格的条件。

情境声誉、pioneer priority 与区域设施容量分别回链 CR-003、FI-002 与 GR-001；本叶子可以消费这些窄范围 authority 的结果，但不得重新定义、扩张或互相转译它们。

<a id="prd-game-015-recognition-opportunity-acceptance"></a>
### 2.10.2 来源到叶子映射与专业验收

| 上游来源 | 本叶子承接的 gameplay obligation | 专业验收入口 |
| --- | --- | --- |
| [`REQ-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004) | 成熟世界适用条件、相称事实/审核、来源/作用域/用途/期限与不越权边界 | [AC-RO-01](#ac-ro-01)、[AC-RO-02](#ac-ro-02)、[AC-RO-07](#ac-ro-07) |
| [`AC-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004) | 预览/待决/hold、当前条件重验、receipt 单次效果、失效/拒绝/撤销与恢复 | [AC-RO-03](#ac-ro-03)、[AC-RO-04](#ac-ro-04)、[AC-RO-05](#ac-ro-05)、[AC-RO-06](#ac-ro-06) |
| `MW-11` 与 CR-003 / FI-002 / GR-001 窄范围 authority | 跨专业证据保持可追溯；不把情境声誉、pioneer priority 或区域容量扩张为 generic recognition | [AC-RO-07](#ac-ro-07)、[AC-RO-08](#ac-ro-08) |

<a id="ac-ro-01"></a>
- **AC-RO-01 阶段门槛**：样例必须发生在首次持续能力之后的成熟世界；首局、冷启动、starter grant、普通行动 receipt 或环境活动不能单独产生本叶子的认可/机会效果。

<a id="ac-ro-02"></a>
- **AC-RO-02 事实、权限与范围可读**：玩家能读到相称世界事实或可复核审核/治理决定，并能区分当前 `permission/authorization`、来源、地点/作用域、用途、开始时间、到期时间、复核/申诉边界；缺失、撤销或过期的权限/授权不得被推荐、预览或历史认可写成可结算认可。

<a id="ac-ro-03"></a>
- **AC-RO-03 预览与待决不生效**：预览、邀请、推荐和资格不改变资格、容量、排队、优先级或其他世界状态；提交保持待决，有界 hold/排队明确不等于已分配，并给出阻塞原因与下一步。

<a id="ac-ro-04"></a>
- **AC-RO-04 receipt 至多一次与竞争申领**：给定同一成熟世界作用域内一个只能分配一次的 scarce opportunity，以及两个 distinct 且当前 permission/authorization、资格与其他适用条件均有效的 eligible contenders；当两者提交、并发、重连或 retry 竞争该机会时，只有一个当前有效 receipt 可以产生一次可使用机会。获胜者之外的请求必须保持待决或原子拒绝/释放，说明冲突原因与独立下一步；不得产生第二次分配、隐藏优先级、隐藏 charge 或其他第二次世界效果。该规则也适用于 replay，但不改写 FI-002 专门 authority 下的 pioneer priority transfer 条件。

<a id="ac-ro-05"></a>
- **AC-RO-05 结算前重验**：来源、作用域、当前 `permission/authorization`、资格、容量、期限或反滥用事实变化时，系统必须重新校验；若提交后权限/授权缺失、撤销或过期，旧 `pending` 必须失效并拒绝、释放或过期，不得产生 receipt 或世界效果，也不得继续保持可结算状态，且要给出原因与独立下一步。权限后来恢复时，必须再次重验当前权限与其他条件并显式新提交，不得自动重提；其他权限仍有效、仅因容量或资格变化的合法 `pending` 可按既有规则继续待决。不得自动续期、跨区域携带或使用历史认可补签。

<a id="ac-ro-06"></a>
- **AC-RO-06 失效与恢复**：到期、拒绝、暂停、撤销或申诉结果保留历史，停止未来效果，并提供独立行动、补证、替代路线、repair/rebuild/pivot、恢复或复核下一步；失效认可不得封锁独立基线或静默重试。

<a id="ac-ro-07"></a>
- **AC-RO-07 非转让与反滥用**：由本叶子 generic recognition 派生的认可/机会不能出售、出租、转让、拆分、叠加或代理/组织代持；[`FI-002 pioneer priority`](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002) 在其专门 authority 下的可转让性及其原有资格、期限、范围和重验条件不被本 generic 规则改写。处置只使用预声明、可复核规则，未经审核的怀疑不直接惩罚，并反馈事实类别、当前效果、复核路径和再获资格条件。CR-003、FI-002、GR-001 的窄范围边界保持不变。

<a id="ac-ro-08"></a>
- **AC-RO-08 证据闸门**：本叶子只定义可玩性和专业合同，不提供当前实现、fresh mature-world sample、跨角色对账或发行 readiness 证据。没有同一候选的 gameplay、runtime、Agent、Viewer、blockchain 与 QA 组合证据时，结论必须保持 `pending/unresolved`，不得公开声称认可/机会闭环已通过。

本叶子的细节验收不改变产品分册的非目标：不新增首局认可或经济旁路，不拍数值，不把 generic recognition 变成治理/资产 authority，也不替代 CR/FI/GR 专题或任何专业实现与验证结论。
