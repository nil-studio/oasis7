# 可玩性证据与承诺边界

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-09-13
- 专业域权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、[`testing-manual.md`](../../../testing-manual.md)

本文承载“玩家为何值得继续、当前能据此作出多强结论”的长期产品边界。它不定义测试脚本、证据字段、产物 schema、角色调度、PRD-TESTING ID、具体玩法数值或当前 release verdict；这些由 `game`、`testing`、相关实现专业域与根 `README.md` 分别拥有。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 是证据层级与产品 claim 的判断合同，不定义独立玩家交互；测试、LiveOps 和发行 authority 保留相应实现与证据职责。
- 历史设计判定记录：本次分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，本次闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
本节的产品体验设计豁免不等于专业系统设计不适用；各要求的技术覆盖与实现证据分别按下列状态摘要核对。

## 1. 产品目标

oasis7 的持续世界、自动化回归或活跃模拟本身都不是可玩性的替代品。产品需要让玩家从自己的行动中获得可理解、可归因且能打开下一决策的世界后果，并在此基础上形成继续游玩的理由。

因此，产品结论必须同时回答：

1. 玩家采取了什么行动，系统是否接受并正确解释；
2. 世界因该行动发生了什么可归因的变化；
3. 该变化为玩家带来了什么新选择、恢复弹性、区域价值或下一次回访理由；
4. 当前证据只支持哪一级结论，仍缺少什么验证。

## 2. 玩家杠杆与持续价值

玩家杠杆必须形成同一条因果链：

`玩家行动 -> 可归因的世界变化 -> 新选择或可持续价值 -> 下一决策或回访理由`

- 环境事件、其他组织活动、Agent 自主推进、世界时间流逝、库存或吞吐增长，不能替代玩家自身影响的证明。
- 若样例只能证明世界在动，而无法证明玩家行动导致了可解释后果，则它只能说明环境活跃，不能支持“玩家已有效参与”或“值得继续玩”的产品结论。
- 首局、持续能力与成熟世界成长分别需要与其阶段相称的杠杆和继续理由；一次动作成功、单一指标改善或静态内容展示不能代签这些结果。
- 产品不要求每位玩家走同一条路线，但每条受支持路线都应让玩家理解主要收益、风险或约束、恢复选择以及下一次可决策点。

## 3. 分层证据与非替代边界

可玩性结论使用分层证据，而非单一“通过/失败”信号：

- 自动化与受控探针可以证明规则、可达性、回归和部分因果链没有失效，但不能单独证明玩家会愿意继续。
- 内部 synthetic 评审（`L4A`）可以暴露不同玩家视角下的困惑、断点或可能的继续动机；它仍是内部假设，不等于真实操作或外部玩家意愿。
- 具身 Agent 试玩（`L4B`）可以证明 Agent 已在真实入口执行操作并提供受限的继续游玩观察；它不能代替真实人类在真实环境、时间与机会成本下的意愿。
- 受控外部真实人类或真实环境信号（`L5`）才可回答相应范围内的外部玩家反应；它也不能被扩写为广泛市场成功或长期留存已获证明。

低层证据通过而高层证据缺失、冲突或被阻断时，产品结论必须停留在较低层，不得把自动化、内部评审、Agent 试玩或历史样例包装成更强的玩家价值承诺。

## 4. 产品结论边界

指标分三类：技术硬约束包含权威结果、守恒、授权、accepted/committed 区分、幂等及同世界资产/里程碑连续性，实际失败阻断相应能力开放；体验结构检查证明因果、真实选择、可执行恢复与继续路径，真人理解和动机另行观察；操作量、动作族、内容量、时长、等待比例及步骤重复是诊断量。18 操作、6 动作族、8 内容单元、30 分钟和 25% 等待不能作为通用最低通关门槛。

fixture、live 自动化和真人记录分别汇总；结构代理计数及默认 hook/replay 分数不能称为真人有效决策或回访意愿。首轮以 5–8 名目标测试者两次会话定位卡点，逐人记录独立完成、提示后完成、他人代操作、未完成或未覆盖，并说明真实选择改变了什么；同一选择跨快照去重。自然未出现的事故记为未观察。约定第二次会话证明接续，自发回访另记；报告原始人数和情境，不从小样本推导 D1/D7 留存。技术可用、体验可理解、愿意继续分别判定。

`producer_system_designer` 可以基于当前、同一体验目标的分层证据给出 `go`、`watch`、`hold` 或 `block` 的产品建议，但该建议必须说明已经证明的范围与尚未证明的范围。

- `go`：仅表示在明确的体验目标、玩家入口和证据窗口内可以继续下一阶段决策；不自动升级公开发布、技术预览或长期留存 claim。
- `watch`：已有正向或不足以定论的信号，需要继续观察、补样或验证解释。
- `hold`：高层证据缺失、相互冲突，或玩家杠杆/继续理由尚未成立，不能扩大产品结论。
- `block`：发现玩家无法形成可理解因果链、关键行动无可恢复路径，或现有 claim 与证据边界冲突；应先回到对应产品或专业域问题处理。

每个结论必须绑定其 formal 玩家入口和当前证据窗口。`viewer` 与 `pure_api` 的证据不能互相代签；当前公开状态与对外 claim envelope 仍由根 `README.md` 和玩家接入与发行模块拥有。

### 4.1 结论比较单位

产品建议只能比较同一结论比较单位内的证据：`体验目标 + formal 玩家入口 + 候选版本或明确变更范围 + 证据窗口`。当体验或结论对运行平台、设备或兼容环境敏感时，适用环境必须进入明确变更范围，并成为该比较单位的一部分；非平台敏感体验不强制按环境拆分。其中任一适用项不同，信号都只能作为相邻观察，不能与当前单位合并后升级结论。

- 同一入口上的不同候选版本，或同一候选版本上的不同体验目标，必须分别说明其已证明与未证明范围；不得以一个单位的正向信号抵消另一个单位的断点。
- 若平台、设备或兼容环境会改变玩家可进入性、可执行体验或结论解释，结论必须明确其适用环境；一个环境的正向信号不得抵消另一环境的阻断或未验证状态。若环境不会改变该体验或结论，不必人为拆分比较单位。
- 当变更范围无法辨识，或证据无法回链到一个结论比较单位时，该信号只能支持 `watch`，不能支持 `go`；若它正被用于支撑既有较强 claim，则该 claim 应降为 `hold`，直至范围重新可比。
- 本规则定义产品结论的比较语义；兼容范围只约束何时可以比较或合并证据，不把工程脚本可执行性当作玩家价值证据，也不新增测试字段、产物 schema、版本标识格式或当前 release verdict；这些仍由相应专业域维护。

### 4.2 可验证的玩家因果与结论要求

<a id="req-wr-pe-001"></a>
### REQ-WR-PE-001：玩家行动必须打开可理解的下一决策

Trace ID: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：canonical starter production_only路径已有玩家来源绑定的配方结算、owner输出和持久里程碑，实际snapshot/Viewer展示消耗、产出和下一Assembler候选，并明确仍需当前材料/电力/授权；这是本条玩家行动→因果结果→新选择的确切子范围。 [实际完成路径增加owner output ledger余额并记录持久starter milestone/settlement summary。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L454-L575) [实际milestone匹配且Assembler authority合法时开放open_assembler_candidate_only，非只读静态文案。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/starter_industrial.rs#L159-L207) [owner/site/origin/正量产出重验后把持久settlement投影到实际player feasibility。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/starter_industrial_outcome.rs#L11-L85) [实际组件展示结算投入、数量、owner及有条件的下一候选。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.jsx#L3-L24) [主Viewer挂载工业成果卡，非孤立组件。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2690-L2700) 本轮只读静态审读，未执行验收；实际实现交付PR映射未逐条核清，固定生产源码为交付子范围依据，不以未取得运行证据抹去已证交付。
- 核对日期与基线：2026-10-17；需求/专业设计/静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；本轮对比 main 仅文档变化，本条源码继续适用。无新产品运行验收。
- 剩余范围：不扩展为所有受支持玩法或成熟区域交付；仅候选资格不证明建造成功/稳定产能/交付。其余路径的玩家归因与真实下一行动、失败/重连恢复和正式入口实测仍需核清。

- 要求：每个受支持的游玩路径都必须形成“玩家行动 -> 可归因世界变化 -> 新选择或持续价值 -> 下一决策/回访理由”的链路；环境活跃、Agent 自主推进或静态展示不能代替玩家杠杆。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`首局与持续游玩`](first-session-and-continuation.prd.md)
- 验收：[AC-WR-PE-001](#ac-wr-pe-001)

<a id="ac-wr-pe-001"></a>
### AC-WR-PE-001：动作后玩家能归因并继续选择

Trace refs: O7-REQ-d2b0b0b0-51b3-4dc6-a6a5-de4c1a3ac4f3；声明：[REQ-WR-PE-001](#req-wr-pe-001)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-PE-001
- 给定：一个有明确 formal 玩家入口、体验目标和初始状态的代表性路径。
- 当：玩家完成一项被权威接受的行动并回到结果界面。
- 则：结果能说明行动造成的世界变化、收益/风险或 blocker，并提供至少一个真实下一选择或恢复路径；若变化来自环境或自动化而非玩家，样例不能声称玩家已有效参与。

<a id="req-wr-pe-002"></a>
### REQ-WR-PE-002：证据层级不得越级代签玩家价值

Trace ID: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1

- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：L4B runner确有watch/hold与human-equivalence文字、scaffold内部人类佐证说明；但未认定为本条防层级代签的直接交付：它仍以step/progress/ACK自动推导Agent继续与玩家杠杆。 [现行合同禁止step/progress/ACK自动生成Agent继续或杠杆分；旧runner产物默认为unreviewed且不覆盖已评审verdict。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/testing/prd.md#L183-L190) [真实runner summary writer把单次正向结果限定watch、负向hold并输出human_equivalence_note，不把本次Agent动作当作真人意愿。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/run-playability-l4b-agent.sh#L522-L590) [既有summary更新直接替换combined_stage_recommendation并写入human_equivalence_note；未读取比较单位或已有独立blocker，已由现行合同保留的实现缺口，非新finding。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/run-playability-l4b-agent.sh#L697-L739) 本轮只读静态审读，未执行验收；实际实现交付PR映射未逐条核清，固定生产源码为交付子范围依据，不以未取得运行证据抹去已证交付。
- 核对日期与基线：2026-10-17；需求/专业设计/静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；本轮对比 main 仅文档变化，本条源码继续适用。无新产品运行验收。
- 剩余范围：层级输出限制不是完整consumer防代签；human note含L4A/L5 corroboration歧义，其他产品评审/公开claim消费者未穷尽。单次runner仍以world-step/progress+submit ack产生leverage pass，不能据此证明实际玩家因果完成；默认构建/session等旧缺口来自#4438，PR4641合同承接且原Issue按决定停止独立跟踪，关闭不表示修复，本轮没有真实L4/L5执行。

- 要求：自动化、受控探针、L4A synthetic 和 L4B 具身 Agent 只能支持各自层级的规则、困惑或受限继续游玩结论；只有相应范围的 L5 真实人类/真实环境信号才可支持外部玩家反应，且不能扩写为广泛市场成功。
- 专业权威：[`doc/testing/prd.md`](../../testing/prd.md)、[`testing-manual.md`](../../../testing-manual.md)
- 验收：[AC-WR-PE-002](#ac-wr-pe-002)

- 局部专业设计导航：[评分子链](../../playability_test_result/design.md#feedback-score-evidence-boundary)仅说明评分来源、尝试与评审归属保留及数值不可越级代签。这不是本要求的完整系统设计依据；原未核实状态、剩余范围和实现证据边界保持不变。

- 局部专业设计导航：[玩法总纲的总纲评审证据分层](../../game/gameplay/gameplay-top-level-design.design.md#evidence-layer-boundary)仅接收本专题评审将结构回归、live/provider、L4A/L4B 和 L5 的证明范围分开；低层不能代签真人继续意愿。 此为总纲局部职责与待执行验证计划，不是本要求完整系统设计或实现通过依据；原状态、剩余范围及既有证据边界保持不变。

<a id="ac-wr-pe-002"></a>
### AC-WR-PE-002：低层正向证据保持低层结论

Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1；声明：[REQ-WR-PE-002](#req-wr-pe-002)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-PE-002
- 给定：自动化规则探针通过，或 L4A/L4B 样例出现正向继续信号，但没有相应 L5 证据。
- 当：产品评审准备给出玩家价值、公开可用性或长期留存结论。
- 则：结论明确停留在已覆盖层级，记录缺失/冲突证据，并保持 watch、hold 或 block；不得把世界在动、Agent 完成操作或内部模拟包装为真实玩家愿意继续。

<a id="req-wr-pe-003"></a>
### REQ-WR-PE-003：产品建议必须绑定同一比较单位

Trace ID: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07

- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：L4 scaffold实际记录git_head、formal_surfaces、run_id、known_blockers和证据路径；runner实际记录起止时间、target_claim/change_scope及原始状态路径，构成比较单位的部分归属资料。 [现行合同明确PR4641仅文档承接、#4438按决定关闭而实现未修；current_attempt和事实/体验判断须分离。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/testing/prd.md#L147-L195) [真实scaffold writer写git_head、formal_surfaces、run_id、known_blockers与artifact路径；这些是已交付证据归属记录，不是完整comparison-unit判定。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/prepare-playability-l4-review.sh#L429-L482) [manifest读入只提取output_dir/card_path/summary_path，未校验git_head/formal_surfaces与当前run。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/run-playability-l4b-agent.sh#L124-L152) 本轮只读静态审读，未执行验收；实际实现交付PR映射未逐条核清，固定生产源码为交付子范围依据，不以未取得运行证据抹去已证交付。
- 核对日期与基线：2026-10-17；需求/专业设计/静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；本轮对比 main 仅文档变化，本条源码继续适用。无新产品运行验收。
- 剩余范围：完整兼容比较与独立阻断保留未交付于已查summary updater：manifest读取未校验git_head/formal_surfaces，summary直接写单run watch/hold。该缺口已由#4438关闭决定及PR4641现行合同明确保留，非本轮新finding；尚未执行复现，不宣称发生实际错误发布，其他消费者未穷尽。

- 要求：`go/watch/hold/block` 只能比较同一体验目标、formal 玩家入口、候选版本/变更范围、证据窗口及适用环境（若环境影响解释）的证据；范围不明或证据不可回链时不得升级结论。
- 专业权威：[`doc/testing/prd.md`](../../testing/prd.md)、[`testing-manual.md`](../../../testing-manual.md)
- 验收：[AC-WR-PE-003](#ac-wr-pe-003)

- 局部专业设计导航：[评分子链](../../playability_test_result/design.md#feedback-score-comparison-boundary)仅说明描述性评分组与产品比较单位的区别及独立阻断保留。这不是本要求的完整系统设计依据；原未核实状态、剩余范围和实现证据边界保持不变。

- 局部专业设计导航：[玩法总纲的总纲评审比较单位](../../game/gameplay/gameplay-top-level-design.design.md#comparison-unit-boundary)仅接收本专题建议固定体验目标、formal 入口、候选或明确变更范围、证据窗口；环境影响解释时纳入范围。 此为总纲局部职责与待执行验证计划，不是本要求完整系统设计或实现通过依据；原状态、剩余范围及既有证据边界保持不变。

<a id="ac-wr-pe-003"></a>
### AC-WR-PE-003：不匹配样本不能抵消阻断

Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；声明：[REQ-WR-PE-003](#req-wr-pe-003)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-PE-003
- 给定：两个样本在入口、体验目标、候选版本、证据窗口或适用环境中至少有一项不同。
- 当：评审尝试合并证据或提升产品建议。
- 则：系统将其标为相邻观察；范围不明只能支持 `watch`，若被用来支撑更强 claim 则降为 `hold`，不同环境或入口的阻断不能被另一单位的正向信号抵消。

## 5. 与相邻产品专题的边界

- [`首局与持续游玩`](first-session-and-continuation.prd.md)定义首局信任、后引导和首次持续能力的具体产品结果；本文定义这些结果可被作出何种可玩性结论。
- [`成熟世界成长与区域参与`](mature-world-progression.prd.md)定义成熟世界中的独立成长与区域价值；本文要求其样例不能把世界活跃误写为玩家杠杆。
- `doc/testing/governance/` 保留分层测试机制、详细证据规则、operator 入口、评审卡和专业验收；本文只链接其结论边界，不复制或改写其专业权威。
- 当前公开可用性、发行阶段和社区承诺不由本文升级；它们必须遵守根 `README.md` 与 `player-entry-distribution` 的独立权威。

## 6. 组合验收

- PE-1：代表性样例能够用同一条因果链说明玩家行动、可归因世界变化、新选择或价值，以及下一决策或回访理由。
- PE-2：任何仅证明自动化通过、世界活跃或内部模拟正面的样例，均不会被表述为已证明真实玩家愿意继续。
- PE-3：产品结论明确标明其支持的层级、体验目标、formal 玩家入口、候选版本或明确变更范围、证据窗口和未证明部分；当体验或结论对运行平台、设备或兼容环境敏感时，明确变更范围必须说明适用环境。只有同一结论比较单位内的证据可以合并，低层证据不越级代签 `L4A`、`L4B` 或 `L5`。
- PE-4：`go/watch/hold/block` 建议不会扩大公开状态、发行就绪或长期留存承诺，并在证据冲突或玩家杠杆缺失时保持 `hold` 或 `block`。

### 6.1 验收追踪

| 成功标准 | 专业 owner | 专业域权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| PE-1 | producer_system_designer / gameplay_designer / qa_engineer | `doc/game/prd.md`; `doc/testing/prd.md` | player leverage 与 world-activity 区分的代表性样例 | test_tier_full |
| PE-2 / PE-3 | producer_system_designer / qa_engineer | `doc/testing/prd.md`; `testing-manual.md` | 分层证据、non-substitution、结论比较单位（含条件化兼容范围）与 formal-surface 组合审计 | test_tier_required |
| PE-4 | producer_system_designer / qa_engineer / liveops_community | 根 `README.md`; `doc/testing/prd.md`; `doc/product/player-entry-distribution/prd.md` | 当前证据窗口、产品建议与公开 claim 边界对账 | test_tier_full |

具体测试命令、卡片字段、角色触发矩阵、执行记录和当前 verdict 均由专业域与 GitHub task issue evidence 维护。

## 7. Non-Goals

- 不把内部评审、Agent 试玩或外部信号变成新的正式玩家角色、发行等级或默认 release gate。
- 不定义玩法数值、UI 表现、Agent 行为、测试脚本、证据 schema 或人工/自动化执行顺序。
- 不以本产品分册、历史任务完成态或旧样例宣称当前版本已证明好玩、已可公开发行或已具备长期留存。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-WR-PE-001](#req-wr-pe-001) / [AC-WR-PE-001](#ac-wr-pe-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/testing/prd.md`](../../testing/prd.md)、[`testing-manual.md`](../../../testing-manual.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-PE-002](#req-wr-pe-002) / [AC-WR-PE-002](#ac-wr-pe-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/testing/prd.md`](../../testing/prd.md)、[`testing-manual.md`](../../../testing-manual.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-PE-003](#req-wr-pe-003) / [AC-WR-PE-003](#ac-wr-pe-003) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/testing/prd.md`](../../testing/prd.md)、[`testing-manual.md`](../../../testing-manual.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |

## 未决问题与证据边界

每项产品建议仍需匹配实际意图、入口、候选版本/观察窗口与环境；低层证据不能替代玩家价值或扩大公开结论。尚未核清的样本与专业承接保持未核实，不据文档检查填补证据。

核对责任由本文 Owner role 联合所列专业 owner 承担；解除触发是相应功能进入实现或对外 claim 评审前，按本条 REQ/AC 核定专业承接、固定候选与实际证据。未核清前保留上列缺口，不用结构检查推导完成。
