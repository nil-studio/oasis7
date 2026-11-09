# 成熟世界成长与区域参与

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-09-20
- 专业域权威：[`doc/game/prd.md`](../../game/prd.md)、[`gameplay-mature-world-progression-contract.prd.md`](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md)

本长期产品分册承载玩家完成首个持续能力后，在已有组织、治理和历史的成熟世界中继续形成独立价值的产品承诺。长期目标不是世界通关，而是持续完成有边界、可审计并留下世界后果的阶段成果。本文不冻结状态字段、数值、Agent 决策顺序、界面结构、任务状态或当前放行结论。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 直接表达成熟世界成长路线、repair/rebuild/pivot 和区域价值边界；它不另立玩家信息或控件 authority。
- 历史设计判定记录：本次分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，本次闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
本节的产品体验设计豁免不等于专业系统设计不适用；各要求的技术覆盖与实现证据分别按下列状态摘要核对。

## 1. 产品问题

世界持续发展以后，新玩家、小规模玩家和回流玩家不能只在“立即依附强组织”与“退化为旁观者”之间选择。产品必须让资源和影响力有限的玩家仍能靠自己的行动形成可读、可恢复且对区域有用的成长路线。

世界本身很活跃不代表玩家正在成长。有效路线必须让玩家回答：我采取了什么行动、世界因此发生了什么、我获得了哪种新选择或韧性，以及下次回来为什么仍值得继续。

## 2. 成熟世界成长主线

默认产品路径为：

`本地立足 -> 区域专业化贡献 -> 有限范围的区域影响`

这三段构成系统的长期推荐轴：建立并守住可恢复能力、服务区域需求、获得有限且可审计的区域影响。它们允许在当前世界状态下改道、重排或回退，不是必须逐级完成的职业树。组织、协议或治理等文明尺度项目是自愿共同扩展，不能取代该独立路线或成为唯一有效的成长答案。

### 2.1 本地立足

- 玩家先稳定一项小规模工业或服务能力，并完成一次对世界有可见后果的成果。
- 首个成果的“受保护”指失败影响范围有限、存在恢复路径且玩家贡献可见，不表示必然成功、永久免战、经济旁路或政治豁免。
- 该阶段不能要求玩家以加入 major power、接受强制赞助或进入全局治理作为唯一继续条件。

### 2.2 区域专业化贡献

- 玩家从重复维持转向满足具体区域需求的专业化贡献，例如恢复、转换、供应、维护或物流服务。
- 选择专业化前，玩家需要读懂第一项贡献服务什么需求、需要哪些主要投入、预计形成什么结果、带来哪类新能力，以及完成后下一次回来可继续什么。
- 专业化不能只是职业标签或同一产线的吞吐增长；它必须带来新的用途、恢复弹性、议价位置或区域选择。

### 2.3 有限范围的区域影响

- 持续的区域贡献可以形成局部优先级、机会、信任、可见度或协调能力。
- 这类影响必须低于全局治理权、联盟领导权或跨区域军政控制，不能把小玩家成长静默升级为 major-power 路线。
- 玩家可以自愿进入更大组织或更深治理，但产品不能把这种升级包装成成熟世界中唯一有效的成长答案。
- 文明尺度项目即使产生更大范围的协作或制度后果，也不构成全体玩家的胜利条件；只有在受影响范围内经授权形成后，才作为参与者可选择的共同目标。

### 2.4 区域服务筹资不能把独立路线变成隐性租金

- 可排他的区域设施或服务应以可读的使用费、服务费、维护费或自愿合同筹资。付款前，玩家必须能知道受益对象、用途、主要成本、服务范围、失效或退出条件，以及不购买时仍可采用的独立、重建或替代路径；一次报价、排队或登记不等于费用已经结算或玩家已承担持续义务。
- 只有无法按个体使用排除、且受益范围明确的公共品，才可使用区域 levy。levy 必须有 charter 的预先授权，并同时绑定用途、受益范围、上限或预算、到期时间、公开账目和定期复核；组织成员、历史贡献、到访、持有资产或选择某项专业化本身都不自动构成可持续征费资格。
- levy 不能成为加入组织、维持独立成长、保留基本通行/恢复路径或获得一般世界资格的默认前置，也不能转换为治理权、永久优先权、可交易额度或对特定玩家的隐蔽补贴。范围外、无效、到期、未经审计或已被复核撤销的征费不得产生扣减、服务拒绝、资格限制或其他世界效果。
- 筹资授权、受益范围或账目无法核验时，系统应安全地拒绝新的征费或续费，并让受影响玩家区分“提案/待审”“已授权但未结算”“receipt 支持的已结算”与“失效/撤销”。适用时必须给出补证、申诉、常态付费、独立恢复或重新规划的下一步；不能静默重试、累积欠费，或把临时保护表达为最终义务。

### 2.5 有界认可与区域机会申领

- 世界内认可必须追溯到与其作用相称的世界事实，或适用的、可复核的审核/治理决定；可核验的贡献、信誉或区域历史可以打开未来的机会、候选资格、可见度、协作邀请或有限优先级，但必须绑定来源、地点/作用域、用途、开始与到期边界及适用的复核/申诉。它不自动产生资产、行动成功、OC、治理权、区域控制或全局权力。情境声誉的主体、时间、更新与申诉记录由[`沟通、合同、声誉与 R&D 连续性`](communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)主责。
- 认可被拒绝、暂停、撤销或到期时，历史贡献、已确认合同结果与 receipt 仍可追溯；未来效果停止，但玩家仍可按普通世界规则行动、补充新证据、走替代路线或使用 repair/rebuild/pivot 恢复，不得以失效认可封锁独立基线。认可不得出售、出租、转让、拆分、叠加，或由代理/组织代持来绕过贡献、作用域、容量、期限、治理或独立性边界；也不得重放认可来洗成永久权力。
- 资格、邀请、预览或推荐只表示可以考虑或尝试申领，不产生世界内资格、容量、排队顺位、优先级或其他世界效果；提交到权威结果前保持待决。只有按当前权限、资格、容量、期限和反滥用条件接受的有界 hold/排队可以暂时预留，且仍不等于已分配；只有 receipt 支持的结果才形成可使用机会。receipt 必须能追溯认可来源、地点/作用域、容量单位、期限和实际世界效果，但不冻结字段名或 schema。并发、重复提交或重连重试至多产生一个结算效果，其余请求必须明确拒绝、释放或保持待决，并给出原因和独立下一步。
- 结算前若容量、来源、期限、资格或反滥用事实变化，必须按当前条件重新校验；不得自动重提、续期、跨区域携带或用历史认可补签。伪造、刷取、重复申领、循环背书、付费换取、批量自动化或其他不相称来源，只能按预先声明且可复核的审核/处置规则拒绝、暂停或撤销未来效果；未经审核的怀疑不得直接变成惩罚。处置必须保留历史并说明事实类别、当前效果、复核路径和下一次可重新取得资格的条件。
- 本分册拥有上述成熟世界中的通用成长、区域杠杆与机会组合语义；[`Frontier 扩展与世界信息边界`](frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002)继续主责 pioneer priority 的特定转让/到期/消费边界，[`受治理的区域能力与扩展`](governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001)继续主责区域设施报价/提交容量。二者不得被泛化为所有认可或机会的替代权威。

### 2.6 长期经济审计与治理影响

- 成熟世界的长期经济审计不能只核对区域 levy。对同一候选版本和证据窗口，必须能够追溯适用的权威资源 source/sink（包括非 levy 的形成、消耗、转换、转移、结算或损失）、关键经济变化，以及治理决定对资源、资格、成本、义务、权限或可替代路线的影响；产品层只冻结这些结果应被审计和解释的边界，不冻结指标、公式或字段。
- 正常路径应把资源 source/sink、经济变化和治理影响连到同一世界因果链，并区分正常结果、待核验与异常；单节点、单专题或短时 green 不能单独掩盖异常，也不能把局部经济样例扩写为长期世界健康或当前发行结论。
- 若 source/sink、关键变化、治理影响或候选/窗口对应关系缺失、冲突、过期或无法核验，结果保持 `blocked`、`hold` 或更窄的目标性结论，不静默补齐、重试或放行。恢复路径是取得适用权威证据、在同一候选和窗口重新核对并保留原有缺口/异常历史；补证本身不产生新的世界效果或 release verdict。

<a id="3-失败恢复与独立性"></a>
## 3. 失败、恢复与独立决策

局部停机、资源短缺、据点受压或路线失效后，体验应提供可比较的恢复方向：

- `repair`：保留当前路线并修复关键缺口。
- `rebuild`：更换位置或重新建立同类小规模能力。
- `pivot`：转向另一种区域专业化，让已有投入形成新的用途。

恢复选择必须说明主要时间或阶段成本、资源成本、可保留收益、风险和推荐理由。只有在独立恢复确实不可行时，才可以把外部赞助或强组织依赖标记为有原因的受迫路径；常态路线应保留不依赖 major power 的继续空间。

等待只有在存在明确触发条件、复查时机和预期变化时才是有效恢复。无期限等待、反复同一操作或隐藏自动改道不能伪装成持续游玩。

### 3.1 面向当前目标的恢复比较

每次代表性 disruption 都必须把 repair、rebuild 与 pivot 放在同一个当前目标下比较，而不是只给出技术上可执行的修复动作。比较至少回答：

| 路径 | 相对当前目标的时间/阶段成本 | 资源成本 | 保留与失去的价值 | 主要风险 | 推荐理由与独立路线 |
| --- | --- | --- | --- | --- | --- |
| `repair` | 恢复现有能力所需的阶段与延后。 | 为修补关键缺口所需的主要投入。 | 尽可能保留现有位置、关系、进度或用途；说明仍会损失什么。 | 原故障可能复发，或恢复后仍受原约束限制。 | 适合现有能力仍能服务当前目标且可独立恢复时；说明为什么不是盲目维持。 |
| `rebuild` | 放弃或暂停当前据点后，重新形成同类能力的阶段与延后。 | 重建地点、基础能力或首个交付所需的主要投入。 | 保留可迁移经验、关系或部分成果；失去旧据点、时间或已锁定投入。 | 新位置/能力形成前的暴露期，或重建后仍不适配目标。 | 适合旧路线不能可靠恢复而同类独立能力仍值得重建时。 |
| `pivot` | 转换为不同区域用途前的学习、准备或目标改写阶段。 | 将已有投入转为新用途所需的主要补足或机会成本。 | 保留可复用的能力、材料、信誉或本地知识；失去原路线的专用收益。 | 新用途的需求、协作或供给条件可能变化。 | 适合原路线不再服务当前目标而已有投入仍能支撑独立的区域贡献时。 |

推荐必须指向当前目标的最合适路径，而不是默认选择最低眼前成本。若三条独立路径均不可行，必须说明是哪项当前约束使独立 lane 暂不可行、外部依赖解决什么、以及何时可重新比较；不得把加入 major power、接受赞助或退出当前目标伪装为唯一正常答案。

生产受阻的首版比较由[同目标恢复专业合同](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#production-recovery-comparison)承接。候选必须能追到原能力、当前权限与真实投入；不同产物、地点或用途只有已被原目标允许时才算服务该目标，否则明确需要玩家修改目标。原工作的已消耗、仍占用和待决结果保留，补料、重建或转向本身不代替目标产出与交付的完成证据。

## 4. Anti-grind 与玩家影响判据

每个阶段成果至少需要证明以下一项有真实变化：

- 解锁新的可执行选择；
- 改善失败后的恢复弹性；
- 改善玩家的局部议价或协调位置；
- 增加对区域可识别的用途。

如果结果只有产量、库存或重复次数上升，而没有上述变化，应判定为 grind 风险，不能作为成熟世界成长成立的证据。

系统默认维持一个当前主目标并提供继续路径；阶段成果后最多呈现少量实质不同的下一方向，玩家也可以主动换向。目标作用域识别、canonical 转译、资源与权限校验、共同治理、反支配和审计留在后台，除非它们改变当前选择的成本、锁定、恢复、共同承诺或可用替代路径。该边界不要求玩家逐行动确认或审核内部流程。

玩家影响证据必须形成同一条因果链：

`玩家行动 -> 可归因的世界变化 -> 新增能力或区域价值 -> 下一决策或回访理由`

环境事件、其他组织活动或 Agent 自主推进不能替代玩家自身的影响证据。

## 5. 生命周期与相邻分册边界

- [`首局与持续游玩`](first-session-and-continuation.prd.md)负责从首局到首次持续能力及最初的中循环选择。
- 本分册从首次持续能力之后开始，负责成熟世界中的独立成长、专业化、区域影响与失败恢复。
- Agent、设施、治理和世界基础设施可以支持该路线，但其字段、权限、执行与数值由对应专业域维护。
- 本路线不改变当前 early-retention 优先级，也不证明 preview、stage 或公开 claim envelope 已升级。

## 6. 组合验收

- MW-1：代表性成熟世界样例证明玩家不必立即依附 major power，也能完成一次可归因、可读且有后续价值的区域贡献。
- MW-2：首个区域专业化选择能说明本地需求、主要投入、预期结果、新增 leverage 和回访理由，而不是只展示角色标签。
- MW-3：阶段成果证明新增选择、恢复弹性、议价位置或区域用途；只增加吞吐或库存的样例不能通过。
- MW-4：失败样例允许比较 repair、rebuild 与 pivot；若只剩强组织依赖，必须说明独立路线为何不可行并保留后续决策权。
- MW-5：区域影响保持有限，不被误报为全局治理权、联盟领导权或跨区控制。
- MW-6：Viewer 与 pure API 分别提供玩家可读证据，runtime、Agent 和 gameplay 专业域对行动、后果、恢复与依赖边界保持一致。
- MW-7：`test_tier_required` 证明合同和可读性；`test_tier_full` 的 fresh mature-world 样例才能给出当前路线 verdict。历史完成态或文档迁移本身不能代替 fresh evidence。
- MW-8：代表性 disruption 样例针对同一个 active goal 比较 repair、rebuild 与 pivot 的时间/阶段成本、资源成本、保留/失去价值、主要风险、推荐理由和独立 lane 可行性；若独立路径不可行，样例说明约束与重评条件。
- MW-9：成熟世界样例证明系统围绕三条长期推荐轴提供一个当前主目标、继续路径及少量实质不同的分支或主动换向；文明尺度项目保持自愿共同扩展，后台治理/转译/反支配护栏只在实质相关时进入玩家决策。
- MW-10：区域服务筹资样例区分可排他服务的自愿费用与不可排他公共品的有界 levy；缺少 charter 授权、用途/受益范围、上限或预算、到期、公开账目或复核任一条件时，征费原子拒绝且不阻断玩家的基本独立、通行或恢复路径。样例同时区分待审、已授权未结算、receipt 支持的已结算与失效/撤销，并证明失效授权不会重试、累积欠费或产生资格/治理旁路。
- MW-11：成熟世界的认可与机会样例证明认可追溯到相称世界事实或可复核的审核/治理决定，并能读出来源、范围、用途、期限和复核边界；资格/邀请/预览/推荐不产生世界资格、容量、排队顺位、优先级或其他世界效果，认可不自动变成资产、OC、治理、区域控制或全局权力。结算前条件变化会重新校验，不自动重提、续期、跨区域携带或用历史认可补签；receipt 可追溯认可来源、地点/作用域、容量单位、期限和实际效果但不冻结 schema。认可不可出售、出租、转让、拆分、叠加或代理/组织代持。反滥用必须按预声明、可复核的处置规则执行，未经审核的怀疑不得直接惩罚，处置保留历史并提供事实类别、当前效果、复核路径和再获资格条件。失效/拒绝/撤销回到独立行动、补证、替代路线或恢复选择；并发、重复或重连请求至多一个结算效果，其余明确拒绝、释放或待决并给出原因和下一步。情境声誉、pioneer priority 与区域设施容量分别回链 CR-003、FI-002 与 GR-001，不互相扩大 authority。
- MW-12：同一候选版本和证据窗口的长期经济审计覆盖适用的非 levy 资源 source/sink、关键经济变化与治理影响，并能把正常结果、待核验和异常连到同一世界因果链；单节点、单专题、短时或局部 green 不得掩盖缺证或异常。source/sink、变化、治理影响或候选对应关系缺失、冲突、过期或无法核验时保持 `blocked`、`hold` 或更窄结论；恢复必须在取得适用权威证据后重新核对同一候选/窗口，且保留缺口历史，不产生静默经济效果或当前 release verdict。

### 6.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| MW-1 / MW-3 / MW-5 | gameplay_designer / qa_engineer | PRD-GAME-015 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/testing/prd.md` | mature-world player leverage 与 anti-grind 样例 | test_tier_full |
| MW-2 | gameplay_designer / agent_engineer / viewer_engineer | PRD-GAME-015 / PRD-WORLD_SIMULATOR-001 | `doc/game/prd.md`; `doc/world-simulator/prd.md` | 专业化第一项贡献的玩家可读预览 | test_tier_required |
| MW-4 | gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer | PRD-GAME-015 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md` | failure 到 repair / rebuild / pivot 的组合证据 | test_tier_required |
| MW-6 / MW-7 | qa_engineer / runtime_engineer / viewer_engineer | PRD-TESTING-003 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 | `doc/testing/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md` | Viewer、pure API 与权威状态对账及 fresh sample verdict | test_tier_full |
| MW-8 | gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-015 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 同一 active goal 下三条恢复路径、独立性与受迫外部依赖边界的组合证据 | test_tier_required |
| MW-9 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-007 / PRD-GAME-014 / PRD-GAME-015 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 三条长期推荐轴、单一主目标、继续/分支/换向、文明项目自愿性与后台护栏组合证据 | test_tier_required |
| MW-10 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | PRD-GAME-015 / PRD-WORLD_RUNTIME-001 / PRD-P2P-003 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | 自愿服务费/有界 levy 分类、完整授权、原子拒绝、无隐性独立路径门槛、结算状态与失效后的申诉/恢复组合证据 | test_tier_full |
| MW-11 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / viewer_engineer / qa_engineer | PRD-GAME-015 / PRD-WORLD_RUNTIME-001 / PRD-P2P-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md`; [`沟通、合同、声誉与 R&D 连续性`](communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003); [`Frontier 扩展与世界信息边界`](frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002); [`受治理的区域能力与扩展`](governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001) | 认可来源/范围/期限/复核、失效保留历史与独立恢复、预览/待决/hold/receipt、并发单次效果、拒绝/释放下一步及 CR/FI/GR 专题不越权的组合证据 | test_tier_full |
| MW-12 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer / liveops_community | PRD-GAME-015 / PRD-GAME-018 / PRD-WORLD_RUNTIME-001 / PRD-P2P-001 / PRD-P2P-002 / PRD-P2P-003 / PRD-WORLD_SIMULATOR-042 / PRD-WORLD_SIMULATOR-045 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md`; [`同候选门禁`](../player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001) | 同一候选/窗口的非 levy 资源 source/sink、关键经济变化、治理影响、正常/失败/恢复与异常不被单节点/单专题/短时 green 掩盖；缺证保持 blocked/hold/窄结论，不生成当前 release verdict，并与 S8/S9/S10 专业 evidence 组合核对 | test_tier_full |

## 6.2 产品要求、叶子验收与未决问题

<a id="req-wr-mw-001"></a>
### REQ-WR-MW-001：成熟世界成长必须提供独立且有后果的区域价值

Trace ID: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce

- 系统设计状态：已覆盖
- 实现状态：未核实
- 系统设计依据：[需求、身份与范围](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-need)、[原匹配交付](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-delivery)、[实际用途](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-use)、[只读 checkpoint](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-checkpoint)与[独立继续](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-continuation)保留原生供应用途 v1；[持续服务、扩容与有限影响组合](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-continuing-growth)接收现有服务、工厂能力、认可与机会的真实来源和消费者，[组合预算](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-growth-composition-budget)约束同切点读取、跨专题依赖与恢复。首单、续作、扩容和 checkpoint 分别以真实新增用途、选择或恢复条件判断，原费用、旧义务及独立路线保留。
- 产品表达与验证方法：[有限区域供应用途政策](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#regional-supply-use-policy-v1)及其 [AC-MRC-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-01)～[AC-MRC-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-08)与[专用手册](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m01)分别说明玩家取舍和独立验收方法；方法存在不代表运行通过。
- 实现与验证依据：当前[首单预览](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/branch_commitment.rs)仍按动作名生成本地需求与回访文案，[lane 投影](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot_lane.rs)仍有按目标类型推导的标签；原[材料运输](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs)和[原生配方](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)提供真实业务机制，但不能单独证明供给到区域用途及新增成长。原 SC-31 有限输入目标/匹配适配、接受时个人归因、完整材料 writer 与历史用途索引、三入口同源读面仍待接线，未取得整卡同候选业务验收。
- 历史实现与验证依据：已核当前默认工业分支、snapshot和M5 meta：可用动作筛选与世界反馈真实存在，但local_need/区域价值/回访理由由action-id静态模板产生；累计meta points并不等于成熟区域贡献及有限影响。 [branch](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/viewer/runtime_live/branch_commitment.rs#L8-L168)；[snapshot](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L124-L246)；[dispatch](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/event_processing.rs#L379-L478)；[bootstrap](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/bootstrap_gameplay.rs#L14-L99)；[emit](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L254)；[meta-write](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/governance_meta_publication.rs#L496-L550)。本轮仅固定源码/文档静态审读，未执行业务测试；未取得本卡同候选完整验收证据。实际交付PR映射未逐条核清，不把旧测试定义或历史round当新运行证据。
- 历史核对日期与基线：2026-10-17；需求PR #4844 `5d03f1fcda523b071c2d75f634165af344ca7b6b`；源码main `e27f4d990bc4d481097c003ff75afc582fd8f65d`；缓存逐Git blob匹配此固定树。本轮无安装/部署/业务执行，无新产品验收。
- 核对日期与基线：2026-11-05；源码 main `b65af49ca13f7b23f9b85a6209c21a01a4b0f61b`。静态核对上述来源与直接消费者，未执行业务、部署或真人验收；原历史证据保留。
- 组合验证方法：[成长组合方法](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m09)与[失效、重复及恢复](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m10)接收现有各域方法并给出独立因果与差分预期；供给到 Site 的用途不自动成为其他 program 的认可，重复订单和改名不重置 anti-grind。
- 设计补核日期与基线：2026-11-05；已合入 main `2731f781536018a857e5daf2cfa7ede504e9cd69` 及本次专业组合。原源码/实现核对基线保留；本次只有设计、静态走查与文档检查，未执行业务或真人验收。
- 剩余范围：原 SC-31 目标与匹配、完整有界来源、原作用域和个人归因、历史首次性/重复判据及正式读面仍需实现并组合验收；持续服务、能力变化与有限机会还须按各原合同接入同一成熟成长读面，取得实际新增选择/区域用途和回访的同候选证据。已交付、用途已证与新增成长分别判断，不能用设计、历史积分、静态模板或吞吐增长代签实现、长期经济平衡与成熟世界体验；未采纳的新专业化或自定义能力仍由其原 authority 决定。

- 性质：`目标要求`
- 适用条件：玩家完成首次持续能力后，在已有组织、治理和历史的成熟世界继续游玩。
- 要求：产品必须围绕本地立足、区域专业化贡献和有限区域影响提供可归因的玩家行动、世界变化、新能力/区域用途和下一次决策；不能只以产量、库存或重复次数增长作为成长。
- 理由：成熟世界需要让小玩家和回流玩家仍能形成独立价值，而不是只能投靠 major power 或旁观。
- 上位承诺：成熟世界成长主线与 anti-grind 判据。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`gameplay-mature-world-progression-contract.prd.md`](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md)。
- 验收：[AC-WR-MW-001](#ac-wr-mw-001)。

<a id="req-wr-mw-002"></a>
### REQ-WR-MW-002：失败后必须比较 repair、rebuild 与 pivot

Trace ID: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182

- 系统设计状态：已覆盖
- 实现状态：部分实现
- 系统设计依据：[目标与来源](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-goal)、[真实候选](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-candidates)、[成本与保留价值](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-costs)、[推荐与独立路线](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-decision)、[权限与恢复](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-execution)保留原生产目标比较；[交付与路线失效组合](../../world-runtime/runtime/production-recovery-comparison.design.md#mature-delivery-recovery-comparison)及[原来源适配](../../world-runtime/runtime/production-recovery-comparison.design.md#mature-recovery-source-adapters)补齐同交付目标、当前路线和原在途/服务义务的比较。
- 关联专业依赖：已采纳的据点受压、冲突、区域危机及一般经营损坏由[原同目标恢复](../../world-runtime/runtime/chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-recovery)承接，工业维护、能力生命周期、背压及[旧工作处置](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)继续拥有各自真实动作与成本；比较不重造这些规则。
- 历史实现与验证依据（2026-10-15）：已交付 repair/rebuild/pivot 三类定性比较投影；当时默认可用性按 goal_kind 补值，不能据此宣称真实恢复执行已完成。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot_lane.rs#L23-L56)、[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot_lane.rs#L155-L237)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/tests/industrial_progression.rs#L603-L651)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 其余实现交付 PR 与适用运行记录未逐条核定；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 实现与验证依据：当前固定输入 `567c4c55a2f160574fdcf0902ccb068646210c7d` 下，[主体能力查询](https://github.com/nil-studio/oasis7/blob/567c4c55a2f160574fdcf0902ccb068646210c7d/crates/oasis7_world_state/src/gameplay_capability.rs)将 repair/rebuild/pivot 保持 unknown，未支持原因与 action/evidence 分开；[Runtime 查询](https://github.com/nil-studio/oasis7/blob/567c4c55a2f160574fdcf0902ccb068646210c7d/crates/oasis7_runtime/src/world_rules.rs)只对有执行资格的当前产线附加一批 `current_recipe_run` 原配方 quote。[Viewer 当前投影](https://github.com/nil-studio/oasis7/blob/567c4c55a2f160574fdcf0902ccb068646210c7d/crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot_lane.rs)不再从目标标签或文案生成赞助依赖与可执行恢复；[当前断言定义](https://github.com/nil-studio/oasis7/blob/567c4c55a2f160574fdcf0902ccb068646210c7d/crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/industrial_progression.rs)覆盖这些未知边界。现有[私有目标 V1](https://github.com/nil-studio/oasis7/blob/567c4c55a2f160574fdcf0902ccb068646210c7d/crates/oasis7_agent_api/src/game/goal.rs)只有文本与软约束，不提供机器可证的产物/数量/接收范围；结构化完成条件还须由原目标权威接受并与目标版本绑定。已有来源与窄 quote 不等于同目标多方案计算、成本比较或真实恢复闭环，本轮仅静态审读，未执行这些测试。
- 历史核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 核对日期与基线：2026-11-04；输入 `567c4c55a2f160574fdcf0902ccb068646210c7d`。本次补首版生产比较的政策、技术与方法；未运行 Rust、游戏、正式入口、故障恢复或真人验收，历史样本保持原候选范围。
- 设计补核日期与基线：2026-11-05；已合入 main `2731f781536018a857e5daf2cfa7ede504e9cd69` 及本次专业组合。实读原损坏恢复、工业旧工作及服务/运输合同；历史实现状态与固定源码证据保持，本次无业务运行。
- 剩余范围：权威结构化目标、候选及依赖读取、逐步成本计算、交付/服务来源适配、原请求衔接与正式 Viewer/API/Agent 消费仍须实现和验证，并取得停机、缺料、据点受压、路线失效与多阶段旧义务的同候选组合证据。三类标签、窄 quote、arrival 或比较结果不能证明路径可行、目标交付完成或独立 lane 已成立；未被原专业域采纳的能力继续保持真实不支持边界，不以有限搜索推断必须依附强组织。
- 产品表达与验证方法：[同目标比较政策及七项专业验收](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#production-recovery-comparison)、[生产恢复比较手册](../../testing/manual/production-recovery-comparison-acceptance.manual.md)、[交付路线及旧义务方法](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-08)与[原损坏恢复方法](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#loss-recovery-cases)分别核原目标、可达前态、逐资源/阶段成本、旧效果和未知恢复；静态算例和方法存在不代表运行通过。

- 性质：`目标要求`
- 适用条件：局部停机、资源短缺、据点受压或路线失效，且玩家仍有一个当前主目标。
- 要求：产品必须在同一当前目标下比较 repair、rebuild 与 pivot 的时间/阶段成本、资源成本、保留/失去价值、主要风险、推荐理由和独立 lane 可行性；外部依赖只有在独立路径确实不可行时才可标为受迫路径。
- 理由：恢复是下一次有意义的选择，不能把最低眼前成本或加入强组织伪装成唯一答案。
- 上位承诺：失败、恢复与独立性。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`gameplay-mature-world-progression-contract.prd.md`](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)。
- 验收：[AC-WR-MW-002](#ac-wr-mw-002)。

<a id="req-wr-mw-003"></a>
### REQ-WR-MW-003：区域筹资不得变成独立成长的隐性租金

Trace ID: O7-REQ-7e2cd84b-9145-4e2f-b3c2-f9c451cec6c5

- 系统设计状态：已覆盖
- 实现状态：部分实现
- 系统设计依据：2026-10-19 设计核对：[费用与恢复读面](../../world-runtime/runtime/public-levy-authorization-and-settlement.design.md#levy-read-model)、[失效与关闭恢复](../../world-runtime/runtime/public-levy-authorization-and-settlement.design.md#levy-expiry-recovery)。专题 §2.1 与 §11.1 按原 Trace ID 承接本条义务并列出目标验证；该结论仅为设计覆盖，代码实现与运行证据仍按本卡实现状态及剩余范围记录。
- 实现与验证依据：当前可排他物流服务已具真实报价与费用/路线/替代建议；TransferMaterial按当下容量/余额预验，开始托管费用、完成分配owner并保存receipt，报价不等于结算。这是本条服务费直接子范围，非借GR008状态汇总。 [service-quote](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/logistics.rs#L860-L952)；[service-budget](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/agent_delegation.rs#L388-L417)；[service-producer](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/event_processing/action_to_event_core.rs#L640-L701)；[service-escrow](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L45-L140)；[service-receipt](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L288-L354)；[service-read](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/state/projection.rs#L675-L690)；[levy-boundary](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/governance_meta_publication.rs#L238-L401)；[bootstrap](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/bootstrap_gameplay.rs#L14-L99)；[emit](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L254)。本轮仅固定源码/文档静态审读，未执行业务测试；未取得本卡同候选完整验收证据。实际交付PR映射未逐条核清，不把旧测试定义或历史round当新运行证据。
- 核对日期与基线：2026-10-17；需求PR #4844 `5d03f1fcda523b071c2d75f634165af344ca7b6b`；源码main `e27f4d990bc4d481097c003ff75afc582fd8f65d`；缓存逐Git blob匹配此固定树。本轮无安装/部署/业务执行，无新产品验收。
- 剩余范围：继续实现/核验levy的charter预授权、用途/受益范围、上限/预算、到期、公开账目、复核及失效原子拒绝；当前普通票局/默认builtin不提供该合同。完整受益/失效/退出解释、独立/基本通行/恢复不被隐性租金阻断及四状态读面仍缺本条完整证据。 实施待安排： [Refs #4861](https://github.com/nil-studio/oasis7/issues/4861)；本状态审查不关闭该任务。

- 性质：`目标要求`
- 适用条件：区域服务费、维护费或公共品 levy 的提案、授权、结算、失效和复核。
- 要求：产品必须让玩家区分自愿费用与有界 levy，并在征费前说明用途、受益范围、上限/预算、到期、公开账目和复核；授权缺失、过期或无法核验时原子拒绝且不阻断基本独立、通行或恢复路径。
- 理由：公共服务可以产生真实成本，但不能借收费把独立成长、一般资格或治理权变成隐性租金。
- 上位承诺：区域服务筹资与独立路线边界。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)。
- 验收：[AC-WR-MW-003](#ac-wr-mw-003)。

<a id="req-wr-mw-004"></a>
### REQ-WR-MW-004：世界内认可与区域机会必须有界且单次生效

Trace ID: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026

- 系统设计状态：已覆盖
- 实现状态：未核实
- 系统设计依据：本地交付认可与有限物流机会的首版 Runtime 设计分别承接[成熟阶段](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-stage)、[来源与当前授权](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-authority)、[预览与待决](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-preview-pending)、[单次分配及实际使用](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-once-settlement)、[连续授权与当前重验](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-current-revalidation)、[失效及原请求恢复](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-recovery)、[不可转让与有界复核](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-abuse-review)和[实际消费者](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-consumers)。
- 已采纳范围与产品表达：该具体 profile 保留原范围；其他当前已采纳用途按[专业接收表](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#recognition-adopted-consumers)接收，不将全部情境来源直接转成机会。
- 已采纳用途的组合承接：[完整来源与主体](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-source-families)、[明确用途及当前依据](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-purpose-admission)与[真实消费者](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-all-consumers)分别承接事实、用途与执行边界；[claim 原消费者](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#claim-purpose-source)和制造沿原取得规则使用合法来源，地方机会继续要求原 program/审核及真实物流使用。合作、岗位与 R&D 咨询只提供有范围的事实，不自动任命、发邀请、授权或承担义务；FI/GR 的专门权利保持其原规则。
- 专业条款与验证方法：[成熟世界认可与机会专业政策](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-recognition-opportunity)保留 [AC-RO-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-01) 至 [AC-RO-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-08)，补充地方交付认可与有限物流机会政策；[验收手册](../../testing/manual/recognition-opportunity-acceptance.manual.md#recognition-opportunity-01)为原八项义务各给独立核验方法，[REP09](../../testing/manual/contextual-reputation-acceptance.manual.md#rep09-all-subjects)、[REP10](../../testing/manual/contextual-reputation-acceptance.manual.md#rep10-acquisition)、[REP11](../../testing/manual/contextual-reputation-acceptance.manual.md#rep11-consumers)与[REP12](../../testing/manual/contextual-reputation-acceptance.manual.md#rep12-review-recovery)补齐真实来源、取得用途、全部已采纳消费者及更正恢复的独立对照；方法存在不表示实际通过。
- 实现与验证依据：固定基线可核到原生[路线与容量读取](https://github.com/nil-studio/oasis7/blob/25ae2095e67e00179cc1e46bb8d2f6a428b9055a/crates/oasis7_runtime/src/world/logistics.rs)、[路线登记与物料运输动作](https://github.com/nil-studio/oasis7/blob/25ae2095e67e00179cc1e46bb8d2f6a428b9055a/crates/oasis7_runtime/src/world/event_processing/action_to_event_core.rs)、[在途占用、交付与费用结算](https://github.com/nil-studio/oasis7/blob/25ae2095e67e00179cc1e46bb8d2f6a428b9055a/crates/oasis7_world_state/src/state/industry_transition/material_transit.rs)及[委托校验](https://github.com/nil-studio/oasis7/blob/25ae2095e67e00179cc1e46bb8d2f6a428b9055a/crates/oasis7_runtime/src/world/agent_delegation.rs)。这些原生组成能力不能代签新认可来源发布、机会申领、跨普通/工业消费者的统一容量分配、未用权利转实际运输或正式入口。新协议和适配仍按目标设计处理；本轮只做源码与文档静态核对、设计评审及文档检查，未执行该业务，未取得同候选完整验收证据。
- 核对日期与基线：2026-11-04；需求 PR #4844 `5d03f1fcda523b071c2d75f634165af344ca7b6b`；本轮源码/文档 main `25ae2095e67e00179cc1e46bb8d2f6a428b9055a`（#5448 合入后）。设计补核不提升实现、部署或产品验收状态。
- 历史实现核对（2026-10-17）：已正向追到social fact生命周期以及默认economic overlay的合同去重→MetaGrant→points/achievement/tier→Viewer事件。上述真实机制不具本条认可来源/地域用途/期限→容量机会申领与receipt消费者，不能以social/合同去重或meta累积分数代签。 [social](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/simulator/kernel/social.rs#L397-L645)；[meta-producer](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_builtin_wasm_modules/m5_gameplay_economic_overlay/src/lib.rs#L116-L230)；[meta-write](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/governance_meta_publication.rs#L496-L550)；[meta-state](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/gameplay_state.rs#L220-L233)；[meta-read](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/viewer/runtime_live/mapping.rs#L477-L490)；[dispatch](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/event_processing.rs#L379-L478)；[bootstrap](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/bootstrap_gameplay.rs#L14-L99)；[emit](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/gameplay_loop.rs#L25-L254)。本轮仅固定源码/文档静态审读，未执行业务测试；未取得本卡同候选完整验收证据。实际交付PR映射未逐条核清，不把旧测试定义或历史round当新运行证据。
- 历史核对基线：2026-10-17；需求PR #4844 `5d03f1fcda523b071c2d75f634165af344ca7b6b`；源码main `e27f4d990bc4d481097c003ff75afc582fd8f65d`；缓存逐Git blob匹配此固定树。本轮无安装/部署/业务执行，无新产品验收。
- 设计补核日期与基线：2026-11-05；已合入 main `2731f781536018a857e5daf2cfa7ede504e9cd69`。实读原认可合同及 CR-003 的来源、用途、消费者、claim 接缝与 REP09–12；只接收成熟阶段中的适用组合，不给原 claim/制造或普通运输增加成熟阶段前置，原实现证据及其基线保持。
- 剩余范围：实施并逐层验证首版认可发布/审核、申领与当前授权、完整竞争集合及统一容量账、单次分配到真实物流使用、失效/未知恢复和正式消费者；已采纳的情境来源、取得用途与全部 writer 还需按各自原合同实现并取得同候选组合证据。CR-003、FI-002、GR-001 保持各自主责，新增来源或机会须由其 authority 明确采用；本次覆盖不预授权未来形态。历史 meta 累计进度、只读咨询和设计接收均不代签运行闭环。

- 性质：`目标要求`
- 适用条件：可核验的贡献、信誉或区域历史被用于成熟世界中的未来机会、候选资格、可见度、协作邀请、有限优先级或容量申领。
- 要求：产品必须要求认可追溯到相称的世界事实或可复核的审核/治理决定，并绑定来源、地点/作用域、用途、开始与到期边界及适用的复核/申诉；认可不得自动产生资产、行动成功、OC、治理权、区域控制或全局权力。预览、邀请、推荐和资格不产生世界内资格、容量、排队顺位、优先级或其他世界效果，提交到权威结果前保持待决；只有当前条件接受的有界 hold/排队仍是待决，只有 receipt 支持的结果才形成可使用机会，receipt 可追溯认可来源、地点/作用域、容量单位、期限和实际世界效果但不冻结字段或 schema。结算前事实变化必须重新校验，不得自动重提、续期、跨区域携带或用历史认可补签；失效、拒绝、暂停或撤销必须保留历史并提供独立行动、补证、替代路线、恢复或复核的下一步。认可不得出售、出租、转让、拆分、叠加或被代理/组织代持；反滥用只能依预声明且可复核的审核/处置规则处理，未经审核的怀疑不得直接惩罚；并发、重复和重连重试至多产生一个结算效果。
- 理由：成熟世界成长可以让局部贡献打开有限机会，但不能把情境证据或一次成功套利成永久权力、隐藏排队或第二次世界效果。
- 上位承诺：成熟世界成长主线、独立路线与有界认可/机会组合边界；入口侧组合追踪见[`免费进入、世界内成长与有界认可`](../player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-002)。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)，以及 [`PRD-GAME-015 成熟世界认可与有限机会专业执行叶子`](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-recognition-opportunity)。该 gameplay leaf 承接玩家动作、生命周期、失败恢复与专业验收；情境声誉、pioneer priority 与区域设施容量分别由 CR-003、FI-002 与 GR-001 保持其窄范围 authority，不能被 generic recognition 语义扩张。
- 验收：[AC-WR-MW-004](#ac-wr-mw-004)。

<a id="req-wr-mw-005"></a>
### REQ-WR-MW-005：长期经济审计必须覆盖资源流、关键变化与治理影响

Trace ID: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5

- 系统设计状态：已覆盖
- 实现状态：未核实
- 系统设计依据：首版 `native_economic_window_v1` 已由[候选与完整窗口](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-identity)、[非 levy 分账核算](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-accounting)、[关键变化与原指标](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-changes)、[治理实际影响](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-governance)、[缺证与异常结论](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-verdict)、[读权与来源预算](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-budget)、[补证恢复](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-recovery)、[正式消费者边界](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-consumers)承接；[全资源来源与适配](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-full-scope)、[长窗口分段及跨段义务](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-segments)、[有界续办与版本/节点恢复](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-long-recovery)和[统一消费者](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-long-consumers)补齐完整目标的来源、组合和恢复方法。首版与长窗口的真实实现/证据分别核验。
- 实现与验证依据：2026-11-05 固定源码静态核对：[OC 内部审计](https://github.com/nil-studio/oasis7/blob/f4a3b3919d8a86a8617b551ec031b0809e1b268d/crates/oasis7_runtime/src/world/main_token_economy_audit.rs)已有真实 supply/发行/财库计算，[longrun 内部消费](https://github.com/nil-studio/oasis7/blob/f4a3b3919d8a86a8617b551ec031b0809e1b268d/crates/oasis7_runtime/src/world/operability_release_gate.rs)已有告警拒绝；仍未绑定完整产品候选/窗口，缺发行记录读为零。[原费用写入](https://github.com/nil-studio/oasis7/blob/f4a3b3919d8a86a8617b551ec031b0809e1b268d/crates/oasis7_world_state/src/world/main_token_monetary_publication.rs)只更新供给/财库，不能证明具体付款账户扣减。这些窄子机制、新设计与静态算例不证明正式产品审计或完整验收。
- 历史实现核对（2026-10-17）：内部OC审计已真实从权威supply/发行/财库记录计算source/sink与告警，longrun内部API消费告警并能拒绝；这不是仅存在audit字段。但已核调用链未确立同候选/证据窗口的成熟世界产品审计入口，缺epoch记录还被读为0，不能当缺证hold。 [oc-write](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/main_token_monetary_publication.rs#L390-L459)；[oc-audit](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/main_token_economy_audit.rs#L8-L100)；[longrun](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/operability_release_gate.rs#L8-L142)；[audit-tests](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/tests/operability_release_gate.rs#L1-L98)。本轮仅固定源码/文档静态审读，未执行业务测试；未取得本卡同候选完整验收证据。实际交付PR映射未逐条核清，不把旧测试定义或历史round当新运行证据。
- 核对日期与基线：2026-11-05；当前源码 main `f4a3b3919d8a86a8617b551ec031b0809e1b268d`。保留历史 2026-10-17 的需求 PR #4844 `5d03f1fcda523b071c2d75f634165af344ca7b6b` 与源码 `e27f4d990bc4d481097c003ff75afc582fd8f65d` 审读依据；本次仍为源码与文档静态核对，未执行业务或取得新产品验收。
- 设计关系核对：2026-11-05；已合入源码基线 `4f6f20ece923c5705e26ef879c2b83e0e637b3f9` 与本次文档候选。独立语义审读核对全来源、原子分段、跨段义务、并发预算与崩溃补证、版本/节点和消费者方法；原实现证据基线及历史核对保留，不新增运行结论。
- 剩余范围：设计合同已覆盖；以下仍待实现与验证。同候选、完整窗口的正式权威读取/覆盖证明、审计查询与发布 evidence 接线仍待实现；全资源/writer、跨段/版本/节点和多世界来源已有目标方法，准确 source adapter、全部持有类别映射、原范围证明及正式消费者仍须逐项实现，并取得同候选长期组合与玩家理解证据。首版来源不适用、缺付款账户或来源不足时保持 blocked/hold/更窄结论，不以内部 OC 告警为空或本次设计通过升级完整经济健康、公开 claim 或 release verdict。现有内部 OC 子机制保留真实已存在状态，整卡实现仍未核实。

- 专业条款与验证方法：[八项专业验收](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#economic-audit-acceptance)与[独立验收手册](../../testing/manual/mature-world-economic-audit-acceptance.manual.md)承接同一要求；静态复算、原生夹具、正式消费者及长期组合分别取证。

- 性质：`边界要求`
- 适用条件：同一候选版本和证据窗口内，对成熟世界长期经济健康、候选门禁或公开 claim 输入进行产品审计。
- 要求：审计必须覆盖适用的权威资源 source/sink（包括非 levy 的形成、消耗、转换、转移、结算或损失）、关键经济变化及治理影响，并把它们绑定到同一候选/窗口的世界因果链。正常结果、待核验和异常必须可区分；单节点、单专题、短时或局部 green 不得代签完整审计。source/sink、变化、治理影响或候选对应关系缺失、冲突、过期或无法核验时，产品结论只能保持 `blocked`、`hold` 或更窄的目标性结论。
- 失败与恢复：不得静默补齐、重试、放行或把历史结果当作当前证据；恢复必须先取得适用权威证据，再在同一候选/窗口重新核对，并保留原缺口、异常和处置历史。补证不产生新的世界效果、经济结算或 release verdict。
- 理由：CR-5 的长期经济健康不仅是 levy 合规，也包括非 levy 资源守恒、关键经济变化与治理影响是否可审计，以及异常能否被及时识别而非由局部 green 掩盖。
- 上位承诺：长期经济与发行健康、同候选证据边界。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)。
- 验收：[AC-WR-MW-005](#ac-wr-mw-005)。

<a id="ac-wr-mw-001"></a>
### AC-WR-MW-001：阶段成果形成可归因的区域价值

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce；声明：[REQ-WR-MW-001](#req-wr-mw-001)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-MW-001。
- 给定：资源和影响力有限的玩家在成熟世界完成一次本地服务或区域专业化贡献。
- 当：贡献结算并进入下一次游玩。
- 则：玩家能读到行动、可归因的世界变化、新增选择/恢复弹性/议价位置/区域用途和回访理由；只有吞吐、库存或重复次数增加的样例不能通过。

<a id="ac-wr-mw-002"></a>
### AC-WR-MW-002：恢复选择服务同一当前目标

Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182；声明：[REQ-WR-MW-002](#req-wr-mw-002)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-MW-002。
- 给定：同一 active goal 下发生 disruption，且 repair、rebuild、pivot 至少有两条候选或明确说明独立路径不可行。
- 当：玩家比较恢复方向并作出选择。
- 则：每条路径显示时间/阶段成本、资源成本、保留/失去价值、主要风险、推荐理由和独立 lane 可行性；外部依赖说明其必要约束与重新比较条件。

<a id="ac-wr-mw-003"></a>
### AC-WR-MW-003：失效筹资不会产生隐性世界效果

Trace refs: O7-REQ-7e2cd84b-9145-4e2f-b3c2-f9c451cec6c5；声明：[REQ-WR-MW-003](#req-wr-mw-003)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-MW-003。
- 给定：一个可排他服务费提案和一个缺少 charter 授权、用途/范围、预算/上限、到期、账目或复核条件的 levy 请求。
- 当：玩家查看、提交、续费或遇到授权失效/撤销。
- 则：自愿费用和有界 levy 的状态可区分；不完整或失效授权的征费原子拒绝，不重试、不累积欠费、不产生资格/治理旁路，也不阻断基本独立、通行或恢复路径。

<a id="ac-wr-mw-004"></a>
### AC-WR-MW-004：认可失效与竞争申领保持独立恢复和单次结算

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026；声明：[REQ-WR-MW-004](#req-wr-mw-004)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-MW-004。
- 给定：一个带来源、作用域、用途、期限和适用复核边界的世界内认可，以及两个合格主体竞争同一有限机会。
- 当：认可被使用、到期、拒绝、暂停或撤销，或主体提交、重连、重复申领并在结算前遇到容量、资格、期限或反滥用条件变化。
- 则：玩家能读到认可与机会的当前边界及其相称事实/审核来源；预览/邀请/推荐/资格不产生世界资格、容量、排队顺位、优先级或其他世界效果，提交保持待决，有界 hold/排队不等于已分配，只有一份当前有效 receipt 产生可使用机会，且 receipt 可追溯认可来源、地点/作用域、容量单位、期限和实际世界效果。结算前事实变化触发当前条件重验，不自动重提、续期、跨区域携带或历史认可补签；认可不出售、出租、转让、拆分、叠加或代理/组织代持。反滥用仅按预声明且可复核的审核/处置规则执行，未经审核的怀疑不直接惩罚，处置保留历史并给出事实类别、当前效果、复核路径和再获资格条件。其他请求原子拒绝、释放或保持待决，不产生第二次分配、隐藏欠费或优先级；失效/拒绝/撤销保留历史并给出独立行动、补证、替代路线、恢复或复核下一步。情境声誉回链 CR-003，pioneer priority 回链 FI-002，区域设施容量回链 GR-001，均不扩大其自身 authority。

<a id="ac-wr-mw-005"></a>
### AC-WR-MW-005：经济审计缺口不能由局部 green 代签

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5；声明：[REQ-WR-MW-005](#req-wr-mw-005)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-MW-005。
- 给定：同一候选版本和证据窗口，包含适用的非 levy 资源 source/sink、关键经济变化、治理决定/影响、正常样例，以及缺失、冲突、过期、异常或仅单节点/单专题/短时 green 的对照证据。
- 当：评审长期经济健康、候选门禁或公开 claim 输入。
- 则（正常）：完整审计能把资源 source/sink、关键变化和治理影响追溯到同一候选/窗口的世界因果链，并区分正常结果、待核验和异常；专业 evidence 仍由各自 authority 提供，产品层不代签实现或当前 readiness。
- 则（失败）：任一适用 source/sink、关键变化、治理影响或候选对应关系缺证、冲突、过期或无法核验，结果保持 `blocked`、`hold` 或更窄结论；单节点、单专题、短时或局部 green 不能抵消缺口或异常。
- 则（恢复）：下一步必须是取得适用权威证据并重跑同一候选/窗口的核对，保留原缺口、异常和处置历史；不得静默重试、补造经济效果、升级 release verdict 或把文档迁移当作 fresh evidence。

### 6.3 未决问题与证据边界

- 尚未决定：成熟世界三条推荐轴、MW-11 认可/机会样例、MW-12 长期经济审计和 fresh mature-world sample 何时具备同一候选的 gameplay、runtime、Agent、Viewer、blockchain 与 QA 证据；影响 MW-1 至 MW-12 的当前路线 verdict，决策负责角色为 `producer_system_designer` 联合 gameplay、runtime、Agent、Viewer、blockchain_ops_engineer 与 QA，触发条件是进入相应实现或公开 claim 审查前，解决前临时不承诺 mature-world 体验、认可/机会、长期经济审计或相关链上证据已通过。
- 本分册证据只能证明指定成熟世界样本、入口、版本/窗口和环境中的成长、认可/机会与恢复行为；历史完成态、文档迁移或局部专业 green 不能证明真实留存、完整区域经济、认可/机会闭合或发行 readiness。

具体字段、状态转换、Agent 决策顺序、界面呈现和 pass/watch/block 证据由专业域文档与 GitHub task issue evidence 维护，不复制到本产品分册。

## 7. Non-Goals

- 不新增免费 claim、无限补贴、经济旁路或永久保护区。
- 不承诺完整职业树、固定专业化数值或全局影响力成长曲线。
- 不把区域专业化扩展为默认战争、联盟或全局治理主线。
- 不规定服务费、levy、预算、上限、期限、受益计算、资格、账目格式、申诉程序或任何扣减/结算实现。
- 不定义资源 source/sink 的实现、经济公式、关键变化指标、治理影响阈值、异常检测算法、证据 schema、S8/S9/S10 执行步骤或 release gate/runbook；本分册只定义长期经济审计应覆盖的产品边界和缺证时的结论边界。
- 不把文明尺度共同项目、目标作用域、canonical 转译或治理校验包装成逐动作的玩家表单、重复确认或默认主线。
- 不用历史任务完成态、旧样本或本次文档整理声称当前 mature-world 体验已经通过。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-WR-MW-001](#req-wr-mw-001) / [AC-WR-MW-001](#ac-wr-mw-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-mature-world-progression-contract.prd.md`](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md) | [原供应用途设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-need)、[持续成长组合](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-continuing-growth)及[组合方法](../../testing/manual/mature-world-regional-contribution.manual.md#mrc-m09)接回首单、续作、扩容、有限影响与 anti-grind；同候选运行证据另取 | `test_tier_required` |
| [REQ-WR-MW-002](#req-wr-mw-002) / [AC-WR-MW-002](#ac-wr-mw-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-mature-world-progression-contract.prd.md`](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md) | [生产比较](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-goal)、[交付恢复组合](../../world-runtime/runtime/production-recovery-comparison.design.md#mature-delivery-recovery-comparison)、[原损坏恢复](../../world-runtime/runtime/chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-recovery)及[组合方法](../../testing/manual/production-recovery-comparison-acceptance.manual.md#production-recovery-08)分别接收原目标、当前来源、资源/在途/旧义务和下一步；运行证据另取 | `test_tier_required` |
| [REQ-WR-MW-003](#req-wr-mw-003) / [AC-WR-MW-003](#ac-wr-mw-003) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-mature-world-progression-contract.prd.md`](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-MW-004](#req-wr-mw-004) / [AC-WR-MW-004](#ac-wr-mw-004) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-mature-world-progression-contract.prd.md`](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-recognition-opportunity)、[`gameplay-mature-world-progression-contract.prd.md`](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-recognition-opportunity-acceptance)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、[`communication-contracts-reputation-and-rd-continuity.prd.md`](communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)、[`frontier-expansion-and-world-information-boundaries.prd.md`](frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002)、[`governed-regional-capabilities-and-extensions.prd.md`](governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-001) | 本专题对应要求/验收、入口 [`REQ-ENTRY-FREE-002`](../player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-002) / [`AC-ENTRY-FREE-002`](../player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#ac-entry-free-002) 与 FE-3/FE-6/FE-7 的组合追踪；generic recognition 的 gameplay leaf 与 acceptance anchor 可追溯；[已采纳用途接收](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#recognition-adopted-consumers)、[RO 方法](../../testing/manual/recognition-opportunity-acceptance.manual.md)、[REP10](../../testing/manual/contextual-reputation-acceptance.manual.md#rep10-acquisition)及[REP11](../../testing/manual/contextual-reputation-acceptance.manual.md#rep11-consumers)对应真实来源、当前重验、容量/用途、单次使用、失效恢复及普通独立路径，不扩大 CR/FI/GR authority；运行证据另取 | `test_tier_full` |
| [REQ-WR-MW-005](#req-wr-mw-005) / [AC-WR-MW-005](#ac-wr-mw-005) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 同一候选/窗口的非 levy 资源 source/sink、关键经济变化、治理影响与正常/失败/恢复/异常边界的可导航追踪；专业证据按 S8/S9/S10 及对应 authority 核对，产品层不生成当前 release verdict | `test_tier_full` |
