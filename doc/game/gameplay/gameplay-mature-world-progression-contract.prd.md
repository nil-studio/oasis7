# 成熟世界小玩家成长合同

- 上层产品映射：本合同承接 `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md` 的成熟世界产品承诺与 `doc/game/prd.md` 的 `PRD-GAME-015`。
- 产品叶子入口：[`REQ-WR-MW-001`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；生产目标恢复比较承接 [`REQ-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002) / [`AC-WR-MW-002`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-002)；认可/机会专业叶子另承接 [`REQ-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；有界经济审计承接 [`REQ-WR-MW-005`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)。本文保留成熟世界 lane、checkpoint、专业化、认可/机会玩家动作、生命周期、失败恢复与专业验收语义。
- 主题 authority：本文件拥有 generic recognition/opportunity 的 mature-world 适用性、事实/审核、作用域/用途/期限、预览/待决/receipt、重验、失效/复核/恢复与 anti-abuse 详细玩法语义，同时保留成熟世界 lane、checkpoint、专业化与恢复选择，并定义经济审计的范围、结果解释与专业验收；不覆盖产品承诺、runtime schema、数值或 Viewer 布局。
- 有限区域供给：[`库存供料与实际用途`](#regional-supply-use-policy-v1)承接 MW-001 的一条独立路线，由[区域贡献系统设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md)和[专用手册](../../testing/manual/mature-world-regional-contribution.manual.md)落实；交付、用途、成长解释与认可/机会分别判断。
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

<a id="regional-supply-use-policy-v1"></a>
### 2.9.2 库存供料与可验证的区域用途

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；声明：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；产品验收：[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)。

本节把一条成熟世界路线具体化：玩家用已有合法库存帮助另一位玩家完成一个已声明的本地制造用途，并在回访时判断实际贡献和下一步。首版 `native_regional_supply_use_v1` 只支持同一明确地点范围、两名不同受益主体、一项材料需求、一次原运输和一批原生下游生产。它不要求库存充足的供给者重新生产，也不把已有库存或生产次数重新计为成长。需求适配、来源证明与正式消费者仍是目标设计；[系统设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-need)拥有技术接口、事务、完整来源与预算，[验收手册](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m01)拥有受控输入和运行方法。

#### 2.9.2.1 需求先于贡献，双方各自决定

供给者 A 已完成原规则要求的首次持续能力，其阶段证据、已有设施和历史投入保持可追溯；三批产物、库存数量或一个 lane 标签不能替代阶段证据。接收者 B 是另一个经过当前身份和控制来源核验的受益主体，使用自己有权控制的工厂与账本；不同请求键、Agent 别名或客户端会话不足以证明双方独立。两方属于同一当前有效地点锚点所界定的本地范围，实际站点与运输几何另外核验，不能由地点名称推测路线或扩大为跨区资格。

B 先通过原目标权威接受一个具体用途 U：在指定 Assembler、输入和输出账本执行一批原生齿轮配方。原 SC-31 目标权威据该用途接受有限输入需求 G，明确需要两单位铁锭、接受合法既有库存、接收位置、用途、版本及有限窗口。目标文本、预计需求、附近有工厂或首单推荐不能生成这份接受事实。G 的有限适配沿[工业需求目标](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-001)承接，不另设区域需求数量账；只接受新生产材料、不同材料或其他用途的目标不属于本 profile。

B 的需求应有实际消费账本的短缺及原用途的其他前置支持；来源不全时显示未核实，不能承诺真实区域缺口已成立。A 可以选择提供、保留库存、继续原经营或退出此次尝试。首版是自愿无对价供料：不承诺付款、返利、认可、积分、未来订单或优先权；必须事前说明生产历史成本、这次运输费用、交付后所有权和不自动返还的边界。B 后续是否实际开工、能否完成仍受其决定与原规则约束，供料不保证其成功。

正常合作不构成加入组织、受雇或依附 major power。A 的供料意图不授权控制 B，B 的接收和用途声明也不取得 A 的设施或剩余库存。本节的成熟阶段、历史覆盖及用途证明只决定专题结论；缺少这些证据不能额外禁止原规则允许的普通生产、运输、持有或退出。

#### 2.9.2.2 首单预览与正常过程

提交前，A 至少能判断：“谁在什么范围需要多少材料”“该需求何时有效、用途是什么”“自己从哪个合法账本供料”“原运输需要什么费用、容量和到达条件”“何时只算交付、何时能证明实际用途”“保留库存或不参与有什么影响”。预览只消费当前获准披露的事实，不扣料、占槽、收取费用或替任一方提交。选择供料后仍按原执行规则重验 owner、站点、路线、可用量、费用与接收条件；首版 Site 出库只提供当前所有者的直接执行入口，既有委托能力未支持该来源时明确不可代办，不能由推荐补出新的写权限。

受控正常例使用 A 原有的九单位铁锭与三单位炉渣；B 原输入账本铁锭为零，已声明的一批 `recipe.assembler.gear` 需要两单位铁锭并产出一单位齿轮。A 本窗口不再启动铁锭生产。原材料、产物和电力规则来自[原生配方](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)，原运输计费、完整时间顺序和账本预期见[正常验收输入](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m03)，示例不改变正式世界数值。

| 阶段 | 实际结果与玩家含义 | 仍可决定或须继续核对的内容 |
| --- | --- | --- |
| 供料受理 | A 选择从原输出账本供两单位铁锭；原运输记录出库、在途、费用和占用，A 留下七单位铁锭与原炉渣 | 出库和在途不是收到；A 的旧投入、剩余库存与其他义务不消失 |
| 到达与需求匹配 | 原运输到达 B 的正确输入账本；原 matching settlement 只把 G 的已交付量增加两单位、剩余量减至零一次 | 该结果证明交付；不能据此声称 B 已使用、原下游终端已结算或 A 已获得奖励 |
| B 独立采用用途 | G 满足后停止为 G 追加供料。B 另外按预先声明且当前有效的 U 发起原生齿轮排产，实际消费两单位铁锭与原生产电力 | U 是 B 的独立决定，不能由 G matched、A 的提交或刷新自动续产，也不形成第二份区域需求账 |
| 原用途完成与回访 | B 的原 job 完成并向其合法输出账本写入一单位齿轮；完整来源满足时可证明 A 的材料用于这一用途 | 齿轮仍归原 owner；首次或新增成长还须核历史范围，之后只呈现当前有依据的继续方向 |

成本按原主体、资源和阶段列示。A 负责其原运输费用，B 作为该例路段 owner 收到原运费收入，并自行承担原生产消耗；一次转账收入不能重复算作世界新增资源。B 声明用途时须已有满足该用途所需的资金来源，不能以尚未到账的运费作为最初可行依据。预览不收费、不预留，也不执行产品验证。原产物验证、维护、输出容量或其他费用是否适用必须有正面依据；缺 module、owner 或记录不能被解释为免费、不需要验证或无限容量。

#### 2.9.2.3 交付、实际用途与成长解释分层

G 的满足边界沿[生产与匹配结算分层](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-003)：只消费匹配原目标、版本、材料、实际数量和接收账本的 settlement。原目标只要求交付时，不因本节增加后续生产门槛。G 满足、取消或到期后的既有工作继续沿[需求变化处置](./gameplay-industrial-demand-change-disposition-contract.prd.md)处理；不得为已满足的 G 追加新的不可逆 sink。U 的原命令独立核验和执行，既不是 G 的自动补产，也不自动产生 SC-31 terminal settlement、认可或奖励。

区域用途必须追到真实输入消费和原用途完成。首版采用可完整核对的窄范围：B 实际消费账本在起点没有铁锭，相关完整窗口只有这次两单位收料和 U 的两单位输入消耗，并有匹配的原 job 完成。核对须覆盖已有待决工作及一切影响这些量的其他入账、转出、占用、维护、补偿、治理与恢复写入；不能只看期末余额相等或从多笔入料中挑选有利两笔。其他来源混入、数量被别处使用或覆盖不完整时，可保留已经证明的交付与生产事实，但该供应的用途归因保持不确定。证明过程不新增 lot、托管库存、材料 sink 或资源 claim。

A 的贡献归属固定在原供料接受时的受益主体、Agent 与控制来源，结合原出库、运输完成及当前允许读取的证据判断。Agent 之后换 owner、换名或换会话不能重分配旧贡献；B 的完工也不能由当前控制者倒签为另一人的供料成果。正常例只证明 A 的旧库存被 B 实际使用，不宣称来自本窗口新增的 A 生产批次。

checkpoint 是对既有事实的只读解释。它可以报告“本次供料已在该范围用于 B 的既定制造用途”；首版用途类别固定为“原生齿轮制造的输入供应”（native_gear_input_supply），首次性只在同一受益主体、世界、分支、规范地点和这个固定类别内判断。只有完整历史证明此前未有该用途，才可进一步报告该范围的首次外部用途或新的成长变化；更换接收者、G/U、请求、Agent 别名或政策版本均不能重算首次性，也不能使用自定义类别绕过历史。历史索引缺项、被隐藏的过去或只有近期列表时，保留“实际用途已证，首次/新增成长未核实”，不能用缺记录当零历史。该不足不撤回已经合法发生的业务，也不限制普通动作。

相同原交付、U 和用途不能因重复查询、重连、换 request/goal/Agent 标签或恢复而复制 checkpoint。新编号不证明新需求或新用途；重复供同一已满足目的、单纯增产和增加库存仍受本合同原 anti-grind 条款约束。已有 `same_loop_repeat_count >= 3` 且 leverage 为 `throughput_only / unclassified` 时停止强化同一循环的规则保持不变；不同于旧循环的区域用途须由事实证明，不能由展示标签重置计数。本节不发资产、治理权、机会资格或信誉分；需要认可/有限机会时仍由[原认可合同](#prd-game-015-recognition-opportunity)独立决定。

#### 2.9.2.4 失败、未知与恢复

| 条件 | 玩家应看到的结果 | 合法下一步与保留边界 |
| --- | --- | --- |
| 提交前需求、输入、站点、费用、路线或权限不满足 | 当前方案受阻、未支持或需要重报；保留准确缺项 | 按原能力补证、重报、保留库存或退出，预览不制造占用或扣费 |
| 原运输已经受理，G 随后变化或到期 | 原在途、实际到货及费用保持原结果；不符合 G 的到货显示未匹配 | 查原运输并按原处置处理；不减少失效需求、不自动改道、返还或创建新目标 |
| 材料已交付，B 拒绝/取消 U，或者 U 受阻、失败或超出用途窗口 | 交付事实保留；用途待核、未完成或不具本次窗口资格，已发生投入另列 | B 按原权限自行处理工作，A 可查询允许披露的结果、停止供料或选择其他方向；没有自动退款、惩罚或补偿 |
| B 的来源或历史覆盖不完整 | 已知交付、消费、产出与不确定的归因/首次结论分别显示 | 在原范围补证或使用已证明的窄结论；不能以查不到记录为由重做业务或要求普通动作先取得专题证明 |
| 原提交结果未知、断线或重启 | 原供料、运输或 U 的请求身份与已知阶段仍可定位 | 先查询原权威结果；确认后才按当前条件显式决定新工作，不换键重发、重复扣费或自动续期 |
| 当前读权撤回或私有用途不再允许披露 | 只保留当前 audience 可见的事实与限制类别 | 停止展示受限明细并走原读权或补证入口，不借缺项数量、精确余额或摘要泄露私有目标 |

声明窗口限制本 profile 的匹配和用途资格，不删除原 job、在途、资源所有权、费用或责任。到期后可以按当前读权核实原窗口中已经发生的事实，不能把读取时点当作延长期限。恢复继续使用同一世界和原结果，历史重放不再执行运输、生产或结算；已消费、仍待决与可合法释放的部分分别保留。

#### 2.9.2.5 回访与独立继续

回访先呈现原供料和用途结果，再提供至多三个有实质差别、当前来源支持的方向：例如查看另一份真实需求、保留库存并回到原经营、或退出该供给尝试。存在新需求时必须重新核其目的和条件；G 已满足不能自动滚动成下一单。没有新需求就明确显示没有，不能用推荐模板、旧订单或让同一需求换名制造继续理由。保留库存或退出不自动创建 reserve、释放他人 hold、转移资产或改变原承诺。

若推荐 A 继续自己的铁锭产线，须核其现有材料、当前生产/验证费用来源、维护、设施与输出条件；只知道原料和配方电力不足以证明完整路线可行。报价只是未执行的未来选择，不提前扣除验证者费用或产生模块调用。有来源支持的正常个人经营无需加入组织；缺证、当前 profile 不支持或只检查了有限地点，都不能被推断为“只能接受赞助”。暂不可行时说明具体限制与重查条件，仍允许原规则支持的持有、退出或改目标。

等待须具有可观察触发、有限复查边界和到限后的重新选择，不能无限轮询、续约或等待不存在的订单。读取在[系统预算](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-budget)内完成；超限只形成范围明确的不完整结论，不能隐式扩成全世界搜索或用多次候选查询刷新共同预算。

#### 2.9.2.6 正式入口与完成边界

Viewer、pure API 和 Agent 应使用同一需求、运输、用途与历史来源，区分“已供出”“已收到并匹配”“实际用途已证”“首次/新增成长未核实”和“当前可选下一步”。形式可以不同，读权只改变可披露范围。B 的私有目标和上下文不会因供料自动向 A 开放；用途、数量、期限和必要证明按最小披露规则提供，每个入口和缓存返回均重验当前 audience。供料文案与用途说明是世界数据，不成为 Agent 指令或新的执行授权。

可用表达包括：“两单位铁锭已交付，受领方用途仍待确认”“本次用途已有生产结果；首次贡献历史尚未核全”“暂没有新的已发布需求，你可以保留库存或回到原经营”。独立判别者应能从允许披露的事实说明谁付出了什么、什么确已发生、什么仍待核，以及为何选择继续或退出，而不是只复述“区域贡献成功”标签。

本节完成的是一条有限路线的政策。实际实现、跨入口采用与 fresh mature-world 样例必须分别取证；静态算例、字段齐全或 CI 通过不证明玩家留存、完整区域经济、所有成长路线或发行就绪。其他常态服务、跨区域用途和恢复项目各沿原 authority 扩展，不通过本 profile 自动获得支持。

#### 2.9.2.7 专业验收

<a id="ac-mrc-01"></a>
##### AC-MRC-01 阶段、真实需求与身份

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；声明：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；产品验收：[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)。

给定同一明确地点的 A、B 与已接受用途，专题必须从原阶段证据、真实消费缺口、当前站点/身份和接受在前的 G 建立首单来源。库存数量、旧三批、目标文本、别名或 action 标签不能代替这些事实；要求新增生产的原目标不能用旧库存满足。缺专题来源不额外禁止原规则允许的普通动作。

承接：[阶段、真实需求与身份设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-need)；方法：[MRC-01](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m01)。

<a id="ac-mrc-02"></a>
##### AC-MRC-02 只读预览、有限承诺与成本

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；声明：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；产品验收：[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)。

预览应让玩家判断受益用途、供料来源、主要成本、无对价条件、原所有权变化及失败风险，并能选择供料、保留或退出。只有实际支持的直接执行入口可提交；预览和推荐不产生费用、占用或任何世界效果，未知验证/维护/运输条件不补为零。

承接：[只读预览、有限承诺与成本设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-preview)；方法：[MRC-02](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m02)。

<a id="ac-mrc-03"></a>
##### AC-MRC-03 匹配交付与逐资源守恒

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；声明：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；产品验收：[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)。

原供料、在途、到达、价费与 G matching settlement 分层，实际匹配数量只减少 G 一次。A 剩余库存及旧投入保留，收料不自动产生下游终端结算、奖励或用途完成；G matched 后不再为 G 追加不可逆 sink，变化/到期时仍保留原运输结果与合法处置。

承接：[匹配交付与逐资源守恒设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-delivery)；方法：[MRC-03](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m03)。

<a id="ac-mrc-04"></a>
##### AC-MRC-04 独立用途、完整来源与原归因

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；声明：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；产品验收：[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)。

B 必须按当前有效的独立 U 显式启动原消费并完成原用途，不能由 G 满足自动续产。首版仅在起点零铁锭、唯一匹配收料与绑定输入消费、原 job 完成及完整相关 writer/待决覆盖均成立时证明这笔供料的用途。来源混入或缺证保持归因不确定；个人归属不随之后的 owner 或别名变化。

承接：[独立用途、完整来源与原归因设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-use)；方法：[MRC-04](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m04)。

<a id="ac-mrc-05"></a>
##### AC-MRC-05 只读 checkpoint 与 anti-grind

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；声明：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；产品验收：[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)。

既有原事实支持的实际用途、首次/新增成长与重复工作分别判断。首次或新用途结论须有相应完整历史依据，缺记录不能补为零；换请求、目标或 Agent 标签与重复读取不能复制 checkpoint、重置反复循环或产生任何权益。已有三次 throughput_only/unclassified 的停止规则与认可 authority 保持有效。

承接：[只读 checkpoint 与 anti-grind设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-checkpoint)；方法：[MRC-05](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m05)。

<a id="ac-mrc-06"></a>
##### AC-MRC-06 独立继续与真实回访方向

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；声明：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；产品验收：[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)。

回访只给至多三个有来源的实质方向，G 满足不续单，没有新需求不编造。正常例保留 A 原个人经营、持有或退出选择；完整生产可行性包括实际验证/维护/容量与费用，有限检索或缺证不能证明必须依附强组织。等待有触发、复查与终止边界。

承接：[独立继续与真实回访方向设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-continuation)；方法：[MRC-06](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m06)。

<a id="ac-mrc-07"></a>
##### AC-MRC-07 当前权限、未知结果与恢复

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；声明：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；产品验收：[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)。

供料和 U 各按原当前权限执行，私有目标按当前读权最小披露。失权、目标变化、用途失败与响应未知保留已经发生的交付、消耗和剩余义务；重连先查原请求，历史重放不重跑业务。迟到读取只能核实原窗口事实，不续期、补发效果或改写接受时归因。

承接：[当前权限、未知结果与恢复设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-recovery)；方法：[MRC-07](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m07)。

<a id="ac-mrc-08"></a>
##### AC-MRC-08 正式消费者与证据范围

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；声明：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；产品验收：[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)。

Viewer、pure API 与 Agent 对同一允许披露范围给出同义的需求、交付、用途、成长限制和下一步，旧静态文案不能补成权威事实。独立判别能区分正常用途、仅交付、来源混入和历史不足；静态检查、受控业务与真实玩家样例分层，未验证范围不升级成熟世界或发行结论。

承接：[正式消费者与证据范围设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-consumers)；方法：[MRC-08](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m08)。


<a id="prd-game-015-recognition-opportunity"></a>
## 2.10 成熟世界认可与有限机会专业执行叶子

本叶子承接产品分册 [`REQ-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004) / [`AC-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004) 与 `MW-11` 的 gameplay 专业执行语义。它只适用于玩家完成首次持续能力后的成熟世界；不把首局结果、starter grant、普通行动成功或历史贡献自动写成认可、资格、优先级、容量或可使用机会。

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026；唯一声明仍在上述产品要求。

本合同拥有玩家可操作的认可/机会循环、结果边界、失败恢复和可玩性验收；产品分册仍拥有玩家承诺与适用条件，runtime、Agent、Viewer/API、blockchain 与 QA 保留各自的状态、实现和证据 authority。下述首版只冻结地方交付认可及一次物流机会的专业政策与有限竞争规则，不制定全世界工业分配参数、费用或容量数值、runtime/API/schema 字段或 UI 布局。[系统设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md)与[专用验收方法](../../testing/manual/recognition-opportunity-acceptance.manual.md)承接实现义务；这些目标合同不宣称当前实现或发行已就绪。

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

<a id="recognition-local-delivery-profile"></a>
### 2.10.2 首版地方交付认可与有限物流机会

#### 2.10.2.1 先声明用途，再复核真实交付

首版供给者与认可发行人是同一主体 O：O 当前拥有原生物流路段 R，并由有权控制 O 的主体显式发布本地贡献 program。发布前明确认可用途、接收材料及其原单位、O 自己的 Agent 接收账本、有限观察期、交付达成条件、认可起止时间、可申领机会、审核与申诉规则。参与者在贡献前可以读取这些条件；不得事后更换目的、抬高已接受条件，或用一个新的 program 改写旧事实。改变承诺须形成显式新版本并保留旧版本及其未结义务。

符合成熟世界阶段的参与者按原运输权限完成交付后，O 的当前合法控制者才能显式复核和签发该用途的认可。复核同时追溯原运输接受与完成事实、请求主体、源账本、接收账本、材料、实际收到量、损耗及单次结算身份，确认 program 已在该次贡献接受前发布，且交付满足当时声明的条件。贡献的个人归属必须来自原运输接受时的受益者、Agent 与控制来源绑定；不能因 Agent 后来换了控制者，就用当前 owner 把旧个人的贡献归给新个人。只有 tariff 或 owner payout 的简化收据不够；尚在运输、只有发送成功、缺失或冲突的个人归因及完成来源也不能补成实际交付。普通完成 receipt 只证明原运输事实，仍须经过该 program 的审核，不能直接增加认可、容量或资格。

认可明确归属被审核的受益玩家与贡献 Agent，并绑定 program、用途、路段作用域和期限；当前合法控制与执行授权独立检查。更换请求键、Agent 别名、控制者或组织不能复制或接管认可；同一玩家使用其当前受权 Agent 执行，也不能把权益交给另一受益者。首版不接入全局信誉分、OC、投票、组织职级或其他所有者的容量，不用支付或历史评分代替实际贡献与审核。审核拒绝、暂停、撤销或复核更正均给出事实依据、当前效果、可补证范围和再获资格条件，并保留原决定。

#### 2.10.2.2 供给承诺来自原容量，并保留普通通路

每个首版 program 绑定一个明确且只能结算一次的机会 occurrence。O 明示 R 的原身份、方向、材料、容量单位、此次不可拆分数量、有限申领与使用时间，以及原运输的费用、损耗和其他必要条件。该承诺只允许尝试取得现有 R 的有限容量，不新增路段、材料、免费运力、目的地权限或后续成功保证。它不自动滚动为下一轮供给；新机会须由供给者按当前权限显式发布，并重新核对其来源、范围和条件。

发布、审核通过、预览和 `pending` 都不占用 R。只有结算后的有效 receipt 才占有此次未使用分配；在结算前，普通活动先用掉可用容量会导致该申领真实待决或拒绝。普通运输、工业工作、已有在途及已结算未使用机会共同受原路段容量约束，不能由认可模块另外记一份可用容量。原有材料来源权限、路径每一跳的可用性与容量、目的地条件、损耗和 tariff 均继续有效。

供给声明与结算必须证明普通独立通路仍成立：要么原容量 authority 明确保留可供普通动作使用的份额，要么存在对相应主体确实可用的独立原路径，并有其权限、容量、费用及时间来源。普通保留份额可以正在被普通动作合法使用，但不能被本机会的未使用分配或后续运输侵占。既有普通/工业义务不可被驱逐或降级为认可的附属队列；缺少真实通路来源时，不得发布或结算本 profile。不能把唯一必经路段全额划给认可，再要求普通玩家取得认可、加入组织或付费才能继续原活动。

#### 2.10.2.3 有限轮次内公开竞争

供给者在申领开始前固定轮次起止世界时间、最晚结算时间和有限重评条件；关闭轮次时，取原容量域同一权威快照中的完整候选集合。原已接受占用与普通/工业分配政策是本机会不得越过的约束；一个 occurrence 的认可候选作为同域容量请求参与分配，不能先在私有池分出胜者和容量，再让原 allocator 补救。

首版机会候选采用确定的窄范围顺序：先按每个受益者已固定的首次有效闭合轮次序号 `first_eligible_round` 升序，再按其权威 canonical beneficiary 身份稳定排序。同一 occurrence 每个受益者只有一个候选；重试、重连和相同事实的重放保留原轮次与身份，不以调用到达、预览点击、可重生成 request ID 或额外请求次数争先。合法待决保留等待来源；提交后权限失效的旧请求终态结束，恢复权限后的显式新提交从新的有效轮次起算，不能借旧等待复活。

分配在同一原容量域内共同验证这些候选、全部普通/工业 contenders 和既有义务，并对该 occurrence 施加至多一份分配的限制。只有仍满足当前条件且原容量政策允许的首个候选可以结算；其余请求说明本轮未分配原因、下一次有限重评条件或终止原因，不获得排他 hold、隐藏优先级或 charge。该顺序只解决本 profile 的机会候选竞争，不覆盖原工业 allocator。原域不能接纳该候选类别、完整集合或这项窄规则时，该 profile 不启用；不得以 legacy 的到达先后替代。

#### 2.10.2.4 receipt 必须接到真实使用与恢复

单次结算同时登记原容量占用与唯一 receipt，receipt 明确受益者、认可来源、R、数量、用途、期限和实际分配效果。使用必须由该受益者通过当前有效的原 `TransferMaterial` 权限显式发起，源材料仍来自其受权账本。原提交还须检查完整路径、其他路段容量、材料、目的地与 tariff；获得 R 的机会不代表整个运输已受理。首版不新增认可或机会业务费，但真实制度执行仍承担预先明示的原 WASM 执行成本。O 必须同时是该 module artifact 的实际 owner，并显式承担其原 Data/Electricity 执行费；不能藏入运输 tariff、转嫁 A/B 或利用缺 owner 的分支假称免费。A/B 的只读预览与通用协议 inbox 中的待决登记保持无费用、无排他效果；该登记不执行认可审核或分配。真实运输继续承担原 tariff，不因认可获得减免或重复扣费。

O 发布时须接受实际执行版本、费用来源和有限操作次数，并从自身真实资源保护有界的操作与闭合费用预算；各资源按原执行成本分别计算，不新增费率。原通用资源适配器及所有可能消费该余额的入口共同保护这些预算，不能用模块私有数字代替已保护资金，也不能借未来 tariff 收入垫付。操作预算不足时停止新工作，已接受机会仍有足额闭合预算用于原处置与释放；不得无限占槽或临时向申领者补收费。执行版本、artifact owner 变更或停用须先保证旧义务能按已接受的真实费用来源闭合，不能暗换 payer、挪用闭合预算或删除原历史。

原运输接受时，把该 receipt 尚未使用的 R 容量原子转为此次 transit 的占用，机会随即标记已使用。转换不能先释放再重新竞争，也不能同时保留两份容量。原运输拒绝且无效果时，尚有效的 receipt 仍按原用途和期限处理，或因明确失效原因单次释放；不能把失败使用当成又一次供给。使用前到期、撤销或失去必要权限只释放未消费分配一次，保留 receipt 历史，且同一 occurrence 不重发第二份 receipt。运输已经受理后，认可失效不能抹去原在途、材料、费用或交付义务；完成和容量释放继续由原运输 authority 单次处理。

提交或使用结果未知时，玩家看到“结果未知”及查询原请求、receipt 或 transit 的下一步；重连先恢复原权威结果，同身份同载荷的重放只解析原结果，不能自动另提一单、换键争抢、续期或退款。只有证明原请求未接受且当前条件仍有效后，才允许按原提交语义显式新提交。已经使用或终态失效的 receipt 永不恢复为可使用；复核更正也不能复制历史分配。失败、到期或复核期间，玩家仍可进入上述普通独立通路，或按原规则补证、修复、重建、改道。

#### 2.10.2.5 首版可玩性对照

正常竞争使用两个独立受益者 A、B，各自从自己的 Agent 材料账本经独立 feeder 到 H，再共同使用 R：`H → agent:O`。两者各有当前授权、真实来源材料和已复核的地方贡献；不能用同一 builder、depot owner 或 Agent 来源冒充两个有效竞争者。关闭同一轮次后，只有一个候选取得 R 的一次分配，并通过原运输实际使用；另一个读到准确原因与独立下一步。

同一候选还要让未获机会的 B 不携带机会 receipt，经原普通入口实际使用保留份额，并与获胜者的机会运输共同对账。具体路段容量、机会数量和材料/费用是[受控验收方法](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-04)的输入，不是全局平衡数值。只显示“仍有独立路径”、只签发 receipt 或只让赢者看到成功文案，均不证明机会可用或普通基线受到保护。

<a id="prd-game-015-recognition-opportunity-acceptance"></a>
### 2.10.3 来源到叶子映射与专业验收

| 上游来源 | 本叶子承接的 gameplay obligation | 专业验收入口 |
| --- | --- | --- |
| [`REQ-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004) | 成熟世界适用条件、相称事实/审核、来源/作用域/用途/期限与不越权边界 | [AC-RO-01](#ac-ro-01)、[AC-RO-02](#ac-ro-02)、[AC-RO-07](#ac-ro-07) |
| [`AC-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004) | 预览/待决/hold、当前条件重验、receipt 单次效果、失效/拒绝/撤销与恢复 | [AC-RO-03](#ac-ro-03)、[AC-RO-04](#ac-ro-04)、[AC-RO-05](#ac-ro-05)、[AC-RO-06](#ac-ro-06) |
| `MW-11` 与 CR-003 / FI-002 / GR-001 窄范围 authority | 跨专业证据保持可追溯；不把情境声誉、pioneer priority 或区域容量扩张为 generic recognition | [AC-RO-07](#ac-ro-07)、[AC-RO-08](#ac-ro-08) |

<a id="ac-ro-01"></a>
#### AC-RO-01 阶段门槛

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026；声明：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；产品验收：[AC-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004)。

样例必须发生在首次持续能力之后的成熟世界；首局、冷启动、starter grant、普通行动 receipt 或环境活动不能单独产生本叶子的认可/机会效果。

承接：[阶段与来源设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-stage)；方法：[RO-01](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-01)。

<a id="ac-ro-02"></a>
#### AC-RO-02 事实、权限与范围可读

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026；声明：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；产品验收：[AC-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004)。

玩家能读到相称世界事实或可复核审核/治理决定，并能区分当前 `permission/authorization`、来源、地点/作用域、用途、开始时间、到期时间、复核/申诉边界；缺失、撤销或过期的权限/授权不得被推荐、预览或历史认可写成可结算认可。

承接：[发行与当前权限设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-authority)；方法：[RO-02](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-02)。

<a id="ac-ro-03"></a>
#### AC-RO-03 预览与待决不生效

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026；声明：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；产品验收：[AC-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004)。

预览、邀请、推荐和资格不改变资格、容量、排队、优先级或其他世界状态；提交保持待决，有界 hold/排队明确不等于已分配，并给出阻塞原因与下一步。

承接：[预览与待决设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-preview-pending)；方法：[RO-03](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-03)。

<a id="ac-ro-04"></a>
#### AC-RO-04 receipt 至多一次与竞争申领

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026；声明：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；产品验收：[AC-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004)。

给定同一成熟世界作用域内一个只能分配一次的 scarce opportunity，以及两个 distinct 且当前 permission/authorization、资格与其他适用条件均有效的 eligible contenders；当两者提交、并发、重连或 retry 竞争该机会时，只有一个当前有效 receipt 可以产生一次可使用机会。获胜者之外的请求必须保持待决或原子拒绝/释放，说明冲突原因与独立下一步；不得产生第二次分配、隐藏优先级、隐藏 charge 或其他第二次世界效果。该规则也适用于 replay，但不改写 FI-002 专门 authority 下的 pioneer priority transfer 条件。

承接：[单次分配与真实使用设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-once-settlement)；方法：[RO-04](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-04)，同时检查真实容量消费者、公平顺序与普通基线。

<a id="ac-ro-05"></a>
#### AC-RO-05 结算前重验

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026；声明：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；产品验收：[AC-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004)。

来源、作用域、当前 `permission/authorization`、资格、容量、期限或反滥用事实变化时，系统必须重新校验；若提交后权限/授权缺失、撤销或过期，旧 `pending` 必须失效并拒绝、释放或过期，不得产生 receipt 或世界效果，也不得继续保持可结算状态，且要给出原因与独立下一步。权限后来恢复时，必须再次重验当前权限与其他条件并显式新提交，不得自动重提；其他权限仍有效、仅因容量或资格变化的合法 `pending` 可按既有规则继续待决。不得自动续期、跨区域携带或使用历史认可补签。

承接：[当前条件重验设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-current-revalidation)；方法：[RO-05](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-05)。

<a id="ac-ro-06"></a>
#### AC-RO-06 失效与恢复

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026；声明：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；产品验收：[AC-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004)。

到期、拒绝、暂停、撤销或申诉结果保留历史，停止未来效果，并提供独立行动、补证、替代路线、repair/rebuild/pivot、恢复或复核下一步；失效认可不得封锁独立基线或静默重试。

承接：[失效、未知结果与恢复设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-recovery)；方法：[RO-06](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-06)，分别检查使用前释放与使用后原运输义务。

<a id="ac-ro-07"></a>
#### AC-RO-07 非转让与反滥用

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026；声明：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；产品验收：[AC-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004)。

由本叶子 generic recognition 派生的认可/机会不能出售、出租、转让、拆分、叠加或代理/组织代持；[`FI-002 pioneer priority`](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002) 在其专门 authority 下的可转让性及其原有资格、期限、范围和重验条件不被本 generic 规则改写。处置只使用预声明、可复核规则，未经审核的怀疑不直接惩罚，并反馈事实类别、当前效果、复核路径和再获资格条件。CR-003、FI-002、GR-001 的窄范围边界保持不变。

承接：[反滥用与复核设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-abuse-review)；方法：[RO-07](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-07)。

<a id="ac-ro-08"></a>
#### AC-RO-08 证据闸门

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026；声明：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)；产品验收：[AC-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004)。

本叶子只定义可玩性和专业合同，不提供当前实现、fresh mature-world sample、跨角色对账或发行 readiness 证据。没有同一候选的 gameplay、runtime、Agent、Viewer、blockchain 与 QA 组合证据时，结论必须保持 `pending/unresolved`，不得公开声称认可/机会闭环已通过。

承接：[正式消费者与证据边界设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-consumers)；方法：[RO-08](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-08)。

本叶子的细节验收不改变产品分册的非目标：不新增首局认可或经济旁路，不拍数值，不把 generic recognition 变成治理/资产 authority，也不替代 CR/FI/GR 专题或任何专业实现与验证结论。

<a id="prd-game-015-mature-world-economic-audit"></a>
## 2.11 同候选经济审计与治理影响解释

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5；声明：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；产品验收：[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)。

当玩家、治理参与者或 QA 需要判断“一段世界历史里，资源去了哪里，政策改变了什么”时，只看库存上涨、某次发行告警为空或一次长跑成功不足以回答。本节要求把实际资源变化、原规则和治理效果放在同一候选、同一完整窗口中核对，并说明读者还不能据此判断什么。原 [OC 内部审计](../../../crates/oasis7_runtime/src/world/main_token_economy_audit.rs)与 [longrun 判定](../../../crates/oasis7_runtime/src/world/operability_release_gate.rs)提供真实但有限的子结果；下面是目标专业合同，不把这些子机制写成已经接通的成熟世界审计入口。

### 2.11.1 首版范围与读者动作

首版 `native_economic_window_v1` 选择一个有界的原执行窗口：核对 native 生产及零距离材料转移的非 levy 资源变化，并将 OC 的原发行、财库分发、转账及相关政策效果作为另一组独立单位核对。耐久、可生产状态等原规则导致的能力变化也在窗口内保留，不能因为资源余额未变就跳过。具体事件、账本、适配器、公式和预算由[系统设计](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-identity)唯一规定，受控算例由[验收手册](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-01)给出；算例输入不改变正式世界参数。

读者先选定要核对的候选、世界/分支、完整起止切点及资源/账本范围，再检查实际覆盖和缺项。只有范围内证据充分，才能读到“本批输入在何处消费、产物进入哪个账本、转移由谁收取、发行和财库付款各改变了什么”；若证据不全，则先看到无法解释的类别、已知事实和可行的补证出口。范围选择不授权读取他人私有余额，也不触发新的生产、征费或治理。

| 首版核对对象 | 读者应能判断的结果 | 继续保留的边界 |
| --- | --- | --- |
| 原生产与材料转移 | 区分投入已消费、生产待决/完成与实际收料/产出账本；生产完成和最终交付分别说明，转移不重复计为全域新增产出 | 建厂、生产或转入不自动满足原目标、区域需求或认可条件；仍以那些原 authority 的真实结果判断 |
| OC 政策、发行与财库分发 | 区分政策被接受、适用发行索引、实际发行、财库转出及接收方余额变化；原转账不增加总供给 | OC 与 Electricity、Data、材料保持独立单位，不折算为世界购买力、玩家成长或治理权 |
| 原耐久与生产能力变化 | 说明窗口内能力为何下降或受限，以及它与原生产/时间推进的关系 | 不能只看材料余额平衡就声称能力无损、生产可持续或经济长期健康 |

同一窗口若出现会影响上述资源、账本、能力或治理解释的其他写入，必须纳入已支持的来源闭包；尚未适配时明确阻断相应审计结论。不得通过删事件、缩小已声明范围或选择未受影响账户隐藏异常。只支持局部范围时，报告保留这一范围，其他资源、组织、地区、长期窗口和跨节点组合不会随之通过。

### 2.11.2 分账、来源与关键变化

每一种材料与资源分别核对期初、期末、真实形成、消耗、内部转移、结算和损失，并保留所有者与实际账本。生产是按原配方发生的不同材料变化，不把不同材料数量直接相加；Electricity/Data 资源账与同名材料、OC 账户与财库也不能混成一笔。请求、报价和计划中的数量不进入实际变化。WIP、在途、escrow、受保护余额和已结算持有量按原权威区分，不能将同一批资源同时算作自由库存和待交付资产。

来源既要解释正向产出，也要解释已发生但没有形成产出的消耗、转出和损失。例如原生产启动已经消费输入，随后完成或产物验证失败时，只能按实际事件解释期末状态，不能为了配平补造产物、退款或补偿。全域内一次转移的两端可以分别解释账户变化，但不能重复算成全域新增与销毁。

关键变化以起止状态及窗口内真实执行为依据，至少覆盖所选 profile 的资源变动、能力变化和政策生效结果。比较原指标时保留其公式、分母、时间范围、阈值来源、取整或钳制含义；累计指标不能改写成窗口流量，发行请求中的 `epoch_index` 不能当作 World 当前时间或正式候选窗口。旧报告的零值或空告警只有在来源证明相应事实时才有“没有发生”的含义；零发行需要完整范围的无事件依据，不能由缺记录的默认零推断。

### 2.11.3 治理决定与实际经济后果

审计应沿原机制读取提案/授权、原接受结果、实际采用政策及执行事件，说明是谁按哪项有效规则改变了哪些资源、资格、成本、义务、权限或替代路线。政策被批准、被安排生效、被执行使用及资源实际变化分别表述。首版正常对照应展示：政策变化前后按各自适用规则发行、财库向明确接收方分发、个人之间转账，以及它们对供给、流通和各账本的不同影响。

只存在批准记录不证明所有申请内容已执行，执行 receipt 也不证明更广泛的宪制或法定治理已通过。审计只在原接受机制的实际范围内说明授权和效果；申请参数、实际处理时间与生效索引分别核对，历史授权不能推断为当前可再次执行的权利。需要解释的批准、适用规则或实际效果缺失时，保留缺口，不按余额变化反推一项不存在的治理决定。

审计不代替原事件产生者补充扣费主体或资金来源。若原费用记录只有金额/供给/财库变化而没有可核对的付款账户及配对写入，不能把它包装成完整现金流，不能从“供给减少”反推出某玩家已经付款。缺失付款来源/配对的缺口与已取证账本能证明的不平分别记录；账户闭包完整且已证明残差不为零时必须保留异常，不能因缺少付款来源而将已知异常降为未知。

### 2.11.4 结果、失败与继续方式

报告分别呈现“声明范围内核对完成”“证据不足，暂不能判断”和“已发现可定位的异常”。核对完成表示来源覆盖、账目和已声明解释在该范围内成立，不表示资源分配公平、玩法平衡、没有其他滥用或世界长期健康。完整来源下发现的异常继续是异常；缺失或冲突的来源不能用其他正常子项抵消。具体 `blocked/hold` 或窄结论由消费入口沿原合同表达，不新增世界 Action 准入门禁。

读者应能从异常回到原事实、受影响范围和下一步：补齐指定来源、核对冲突版本、联系原权限 owner，或只使用已证明的窄结论。无权取得材料时显示允许披露的缺失类别，不能泄露他人余额、身份关联或隐藏请求。未知结果先核对已有审计任务和已保存结果；不自动重做世界操作、重新结算或改变窗口来得到更好结果。

### 2.11.5 有界读取与补证恢复

审计请求只消费当前读权允许的来源，在明确的记录、字节、证明、时间和并发预算内工作。达到上限时说明尚未覆盖部分及合法继续方式；不得截断后仍报告完整，也不以审计名义无界遍历历史、复制完整 World 或持有无限缓存。分段读取只有在同一窗口、完整性和去重条件都可核验时才能合并，隐藏或无法取得的部分仍算缺口。

补证重核保留原候选、世界/分支、窗口、范围及历史结论，关联新增的权威材料并明确哪些缺口得到解决。缺失材料无法取回时保持原限制；换用新候选、窗口或范围时形成可区分的新核对，不覆写旧异常。读取被取消、进程重启或历史恢复不授权再次产生业务效果，也不恢复已撤销的读权。

### 2.11.6 实际消费者与完成边界

正式审计消费者必须读取同一份有来源的范围、结论、异常与恢复关联；玩家可读解释、QA 核对和发布 evidence 输入不能各自重新计算出相互矛盾的“正常”。界面或 API 接收请求、生成文件、原 OC 告警为空和单次组件测试均不足以证明该消费者已采用完整合同。报告只能证明其实际证据层，原 S8/S9/S10、跨节点/恢复、真人理解与公开 claim 仍须适用的同候选组合证据。

本节不新增货币、收费、补偿、制裁、授权或统一发行开关，不修改原指标阈值。有限窗口及受控 fixture 用于验证来源和核算，不能独自宣称长期经济、MW-005 整卡或发行通过。正式实现、运行与玩家证据按原产品状态维护。

### 2.11.7 全资源、长窗口与覆盖进度

原 MW-005 的完整范围由[全资源与长窗口设计](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-full-scope)接续首版。读者选择固定起止历史、实际世界及核对范围；全资源声明必须包含所有适用来源、持有类别、关键变化与治理影响，不能只挑正常账户或容易适配的 writer。各单位保留原义，原消费、在途/托管、未完生产、已确认损失与独立资产分别解释。

长窗口可以有界分段完成，但玩家读取的仍是同一完整范围。每段、每个必要来源和跨段义务都核齐后才有完整结论；已完成分项、尚缺部分和全部异常分别可读。中间多记和少记即使末端抵消也保留异常，跨段请求不重新获权、计费、产出或退款。规则变化按原发生版本解释，不以今天规则重写旧账。

读取中断或预算不足时，可查看已保存结果并显式继续固定范围；缺权限/原件时说明可披露的原因及合法补证出口。续办和补证保持原用量、历史缺口和异常；改变范围、候选或终点属于明确的新核对。多个节点重复原件不重复计账，跨世界/链单位没有原映射时分别呈现。具体验收见[EA-09～12](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-09)，不新增统一发行开关或经济参数。

<a id="economic-audit-acceptance"></a>
### 2.11.8 专业验收

下列八项分别承接原产品验收，正常、拒绝与恢复都以所声明的同一候选和来源范围判断；只通过任一子项不能代签其余项。

<a id="ac-ea-01"></a>
#### AC-EA-01 候选、窗口与覆盖完整

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5；声明：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；产品验收：[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)。

给定固定候选、世界/分支、起止切点和资源/账本范围，审计必须证明初态、完整窗口写入、终态与所用规则来自同一组合。漏事件、未知 writer、混用 root/版本、仅有截断 journal 或缺覆盖水位时，不得报告范围完整；原发行索引不能替代这些身份。无关范围正常不能掩盖所审范围缺口。

承接：[身份与完整窗口](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-identity)；方法：[EA-01](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-01)。

<a id="ac-ea-02"></a>
#### AC-EA-02 非 levy 分账与资源变化可复算

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5；声明：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；产品验收：[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)。

给定原生产、材料转移和独立 OC 账本，按原单位/所有者分别重算期初、形成、消耗、转移、损失及期末，并与实际状态相符。原真实非 levy 效果不可由 OC 报告代签；不同材料不能直接抵销，内部转移不能重复创造全域资源，待决或被占用量不能充当已结算自由余额。缺付款账户/配对写入不得补造完整扣费链。

承接：[来源与分账核算](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-accounting)；方法：[EA-02](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-02)。

<a id="ac-ea-03"></a>
#### AC-EA-03 关键变化和原指标具有准确含义

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5；声明：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；产品验收：[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)。

正常样例须解释实际生产、材料转移、能力变化和政策前后经济结果；余额变化为零的原耐久/能力写入仍要说明。指标名称、公式、分母、适用窗口、取整/钳制与阈值来源可追溯，累计数、预测数、发行索引与窗口流量不混用。缺记录或零采样不能被读为“没有异常”，未执行的政策效果不能记作变化。

承接：[变化与指标解释](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-changes)；方法：[EA-03](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-03)。

<a id="ac-ea-04"></a>
#### AC-EA-04 治理影响连接原授权与实际效果

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5；声明：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；产品验收：[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)。

同一窗口中，原批准/授权、政策适用边界、实际发行/财库分发及受影响账本可相互核对；申请、原接受、已安排生效与真实效果分别报告。正常例和“获批未执行、执行被拒绝、治理来源缺失”的对照应得到不同解释，不按余额倒推授权、不把旧 helper 状态或局部执行推成更广的治理合法性，也不由审计触发再次执行。

承接：[治理实际影响](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-governance)；方法：[EA-04](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-04)。

<a id="ac-ea-05"></a>
#### AC-EA-05 缺证和异常不能由局部正常抵消

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5；声明：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；产品验收：[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)。

来源缺失、冲突、过期、不适配及可定位的核算异常必须分别保留。OC 正常但材料异常、单节点/单专题/短时正常、应有的 epoch 记录缺失或只有生成文件等对照均不能得到完整通过；窄结论明确其资源、账本和窗口，不暗含未覆盖部分正常。阻断的是相应审计/claim 结论，不新增原游戏行为的限制或惩罚。

承接：[结论与失败分类](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-verdict)；方法：[EA-05](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-05)。

<a id="ac-ea-06"></a>
#### AC-EA-06 来源读取有界且服从当前读权

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5；声明：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；产品验收：[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)。

给定正常预算、超限、来源分页及读权撤回的对照，所有读取和中间结果均受已声明预算与当前权限约束；超限或覆盖不完整时明确停止或按合法同窗口分段继续，不能截断成功、隐式全历史扫描或泄露隐藏主体。读取与解释不调用会产生新经济效果的业务动作。

承接：[权限与工程预算](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-budget)；方法：[EA-06](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-06)。

<a id="ac-ea-07"></a>
#### AC-EA-07 补证与恢复保留原身份和缺口历史

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5；声明：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；产品验收：[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)。

缺证、取消、重启或结果未知后，先恢复原核对及已知结果；补证只在原候选/窗口/范围下重核并关联旧缺口，撤回的读权仍有效。换候选或窗口必须区分为新的核对；原材料不可得则保留未核验，不能静默补零、覆写异常、重做原生产/转账/治理或产生新的 release verdict。

承接：[补证与持久恢复](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-recovery)；方法：[EA-07](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-07)。

<a id="ac-ea-08"></a>
#### AC-EA-08 正式消费者保留证据层级与全域边界

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5；声明：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)；产品验收：[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)。

同一报告经正式读取/导出、QA 及适用解释入口消费时，身份、范围、事实、异常与下一步一致且可回到来源；receipt、UI 状态或文件生成不替代其真实采用证据。首版单窗口、受控输入、内部 API、单节点与局部指标结果分别说明，未取得适用的 S8/S9/S10、恢复、跨角色和公开 claim 组合证据时，不宣称 MW-005 完整审计、长期健康或发行通过。

承接：[消费者与组合边界](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-consumers)；方法：[EA-08](../../testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-08)。
