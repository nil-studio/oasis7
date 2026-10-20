# 首局与持续游玩产品设计

## 文档身份

- 配对产品 PRD：[`first-session-and-continuation.prd.md`](first-session-and-continuation.prd.md)
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-15
- 专业域权威：[`gameplay 工业 walkthrough 合同`](../../game/gameplay/gameplay-industrial-representative-execution-walkthrough.prd.md#3-五阶段玩家-walkthrough)、[`gameplay 首产物结算合同`](../../game/gameplay/gameplay-industrial-starter-completion-contract.prd.md#3-五阶段结算表)、[`M4 工业资源流转合同`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md#4-technical-specifications)、[`world-runtime 专业 PRD`](../../world-runtime/prd.md#consumer-compatibility-industrial-profile-and-stage-execution)、[`testing 专业 PRD`](../../testing/prd.md#6-validation-decision-record)

本文说明玩家怎样理解并继续一条首局工业因果链。配对 PRD 拥有产品要求和验收；专业 authority 拥有玩法顺序、结算、状态、receipt、接口和测试实现。本设计不把目标态写成当前实现或发行承诺。

## 1. 设计命题

当前首轮路线复用 `starter-industrial-smelter-to-assembler-v1` / `production_only`：合法 Smelter 按所选配方与合法输入/电力完成真实 cycle，第一笔匹配的 owner-bound 正量 `iron_ingot` 结算与持久首产物里程碑一致，仅打开 Assembler 候选。首产物不要求建成 Assembler；持续能力和交付保留独立条件。再次进入先读取同一世界与归属下的成果、有效目标、未完成事项和原请求身份；pending 按恢复时授权、资源与 manifest 重新裁决。过期/撤销授权只提供重新授权、缩小范围或安全停止等真实选项，不自动继承旧条件。玩法阻塞恢复、节点灾备与回访接续各自取证，技术故障注入不要求真人强制经历事故。

目标体验是让第一次进入世界的玩家从一个可解释的工业目标走到当前 `starter_completion_profile` 声明的真实完成边界，并在阻塞、重连或完成后仍保有下一次选择。`production_only` 在匹配 production receipt 后完成首产物并进入 `produced/undelivered`；`terminal-admission` 才继续到匹配 delivery/terminal settlement。玩家需要知道目标、当前权威状态、已保留或消耗的价值、主要 blocker、完成边界和下一步。

核心决策是：在 feasibility card 与当前事实支持的范围内，选择等待、补足、改源、改配方、改道、延期或继续推进哪一条路径。选择必须有可读的机会成本；预览和推荐只提供比较信息，不产生世界效果。

设计假设是先建立一条可归因的因果链，再展开持续能力分支，能帮助玩家判断是否继续。该假设需要对应入口的体验证据验证，结构检查或局部自动化不能替代玩家体验结论。

<a id="des-first-concrete-sample"></a>
### 1.1 当前 Smelter 样本：先做成一笔可解释的生产

本样本承接 [PRD 具体样本与决策边界](first-session-and-continuation.prd.md#first-session-concrete-sample)，是当前 canonical 链的首读路径；后文通用节点只按 profile 的真实依赖展开，不反过来增加当前门槛。

**处境与用途。** 玩家进入可控制世界后，先确认自己实际可用的地点、控制/授权、材料与电力。目标是用合法 Smelter 生产第一笔铁锭，为评估 Assembler 建设准备材料。缺少合法输入或控制权时从真实 blocker 开始，不假定拥有免费启动资源，不替换成 debug 世界。已有合法 Smelter 可直接核对，不要求重复建厂。

| 片段 | 玩家真正决定什么 | 执行与默认反馈 | 完成后发生什么 |
| --- | --- | --- | --- |
| 采用目标 | 是否把当前可用资源投入首笔铁锭；也可保留资源、暂缓或选择真实可用的其他目标 | 展示目标用途、可用对象、主要投入/缺口与首产物边界；接受目标不扣料 | 进入同一对象的生产方案，而非另一套任务清单 |
| 工厂就绪 | 工厂尚未建成时，是否承担本次建厂的实际成本 | 按现有 BuildFactory 能力与授权提交，读取真实建设结果；建厂未接入 Agent 授权路径时由玩家入口执行，不伪称自动委托已可用 | 使用实际建成或已有且合法的 Smelter，不能把“选中了工厂”当就绪 |
| 准备与排产 | 是否采用这份明确的固定方案；真实缺口出现时选择补足、暂缓或其他受支持动作 | 同一 factory/recipe/batches 的权威预览说明实际 input ledger、材料、电力与预计产出。补料只走受支持的显式转移；实际到账后重查，禁止拿世界总库存代替工厂可用量 | 按玩家确认的同一参数提交，不在提交时暗换批量、工厂、配方或消费账本 |
| 授权内代办 | 确认目标、资源/时间等实际支持的授权边界；出现需要新判断的变化时再选择 | Agent 只代办当前能力与有效授权同时覆盖的补料/排产等步骤，说明已接受、执行、阻塞或未知。没有对应接线时保留玩家操作基线并标明能力缺口 | 不把每个内部检查变成确认按钮；新的成本、越界或不可逆承诺才交还决策 |
| 第一笔成果 | 是否继续利用已得材料，或在此结束本次会话 | 先显示实际投入和所得，再按精确因果解释玩家贡献；matching production receipt、owner-bound 正量铁锭与持久里程碑一致时明确庆祝首产物完成 | Assembler 是后继候选，不是已经建成；稳定能力、交付与需求满足独立判断 |
| 下一次选择 | 继续补齐 Assembler 的真实建设条件、选择其他当前支持的用途，或暂缓 | 展示已满足条件、逐项真实缺口和可执行的补足动作；只能查看候选时使用查看/比较入口，不提供虚假的建设提交 | 离场保留成果摘要、待决请求和有效委托的事实；回来先核对当前条件，再执行合法下一步 |

这里选择**固定方案、准确预览、明确提交**作为操作基线。固定参数取自同一规范动作来源，不在产品层另抄一组常量；独立可编辑查询必须标为查询，不能被误认成已配置正式动作。只有预览、提交、结算和后继预算已共同验证时才开放可编辑批量。

经营分支与 Agent 主体感分别验证。小批/大批必须证明所保留资源的实际替代用途、另一方案不可被等成本续批替代的优势，以及后续两次决策的价值差异；成本线性且可以等成本续批时，只算参数选择，不命名为“效率路线”。当前无可证明分支时如实保留固定基线，不凭新增奖励、时限或强制事故制造取舍。等成本比较同时核对实际时长、占槽和时机，不能只凭材料/电力线性判定拆批等价。Agent 主体感则要求玩家能指出哪个目标/约束改变了后续行为；更多解释、确认次数或单次生产成功均不能替代它。

### 1.2 成果、后继与自然离场

成果首读分为三个层次：**实际付出/所得**来自领域结算；**玩家贡献**仅在原意图、实际 action/job 与该结算关联成立时显示；**后继用途**来自当前规则和可达条件。只匹配 owner、最近一次结果或同名材料时，可展示归属成果，不能称为当前决定的功劳。没有精确归因不压掉已被证明的真实成果，也不使用 Agent 叙事补齐因果。

成果卡引用已有单一 Action Receipt，不再颁发第二份回执。重连、回放或重新挂载恢复摘要，不把旧成果再次当成新成功播报。首笔生产完成可以是自然停点：离场说明已保留结果、仍待决请求、当前有效委托及回来后的第一件可做之事；关闭界面不取消请求，不承诺尚未验证的离线收益。回访分别重建成果事实、当前目标和待决状态，旧授权不因回来而复活。

0–30 分钟是体验观察窗口，不是按时触发剧情或事故的脚本；首次持续能力仍按玩法总纲的独立窗口评价，不要求首产物完成者当场继续。失败恢复与单约束理解迁移可在对应验证场景观察，不强迫所有新玩家经历事故。

<a id="gameplay-experience-priorities"></a>
### 1.3 经营决策与持续游玩：设计范围与优先顺序

玩家已经做成第一笔铁锭，却仍可能不知道自己的决定改变了什么、为什么下一步值得做，以及离开后回来该从哪里继续。本节围绕这些问题细化体验方案，继续由现有 PRD 统一定义要求；原 REQ/AC、首产物与首次持续能力的完成边界不变。

先将经营分支、目标委托、可读因果和回访组合在同一工业链上。真实合作和策略复用是后续方案：当前只明确不误导的用途表达和复用边界，不把它们提升为邀请测试或首产物的新前置。下表的顺序表示设计取舍，不是排期或已完成状态。

| 体验目标 | 设计方案 | 对应产品要求 | 范围边界 |
| --- | --- | --- | --- |
| 有机会成本的经营 | 比较真实投入、余量、占用、形成能力及后续两个决策 | [REQ-FIRST-BRANCH-001](first-session-and-continuation.prd.md#req-first-branch-001)、[002](first-session-and-continuation.prd.md#req-first-branch-002) | 不以更多批量按钮、隐藏惩罚或必经升级顺序代替策略 |
| 默认代办、逐步开放定制 | 玩家给目标与支持的边界，Agent 执行授权内步骤，需要新增判断时再请示 | [具体样本](first-session-and-continuation.prd.md#first-session-concrete-sample)、[间接控制](indirect-control-agency-and-continuation.prd.md#req-wr-ia-003) | 不要求新人先编程，不把软偏好当强制授权 |
| 因果可预测、失败可纠正 | 当前方案、投入、主要阻塞和真实下一步放在同一决策面 | [REQ-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#req-first-industrial-002)、[005](first-session-and-continuation.prd.md#req-first-industrial-005)、[006](first-session-and-continuation.prd.md#req-first-industrial-006) | 不另建诊断系统，不让玩家从原始日志拼接结果 |
| 值得记住的成果与回访 | 按原意图与权威结果组织进展摘要，帮助恢复下一次决定 | [REQ-FIRST-INDUSTRIAL-008](first-session-and-continuation.prd.md#req-first-industrial-008) | 不强制制造事故，不用生成叙事补齐缺失事实 |
| 真实需求带来合作 | 当前区分潜在用途与已确认需求，后续验证一笔真实合作 | [生产/交付边界](first-session-and-continuation.prd.md#req-first-industrial-004)、[工业需求专题](industrial-demand-goals-and-settlement.prd.md) | 不先搭建完整市场、政府或强制全体协作终局 |
| 成功办法沉淀为能力 | 后续先复用现有目标配置或受支持模块，采用时重新核验条件 | [自治与授权](../agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#agent-delegation-boundary)、[区域能力与扩展](governed-regional-capabilities-and-extensions.prd.md) | 不先做策略商城，不保存或复制永久授权 |

这些方案的跨组件接线、当前接点与差距、异常恢复和场景映射由[经营决策、目标委托与回访续接系统设计](../../game/gameplay/gameplay-decision-delegation-return.design.md#21-需求承接与分配表)承接。产品功能的状态仍按配对 PRD 与实际证据表达；本节不把设计细化升级为实现完成、真人验证或公开可用。

<a id="operating-branch-experience"></a>
### 1.4 经营分支：比较可达路线与投入顺序

优先比较现有“深化 Smelter 生产线”和“建设 Assembler、开放第二条生产线”，但只在对应阶段、当前规则和真实可用动作支持时展开。首笔铁锭完成时可以先查看 Assembler 的用途和缺口，不要求立即提供两条可提交路线，也不要求当场建成。

合金生产与 Assembler 建设都需要铁锭和铜线，建设还需其他材料与合法地点；已经生产铁锭不等于两条路线都已就绪。先核对材料是否位于各自动作实际消费的账本，以及建设后首个配方的真实前置。合金可用于后续制造，但没有当前可达用途时，不能把库存增加包装为立即开放新能力。具体数量由正式方案提供。

| 当前事实 | 应表达的决策 | 不能得出的结论 |
| --- | --- | --- |
| 只有一条合法路线 | 是否采用目标、处理前置或暂缓；作为教学与操作基线 | 一条路线加一个灰按钮已经满足经营分支 |
| 两条路线均合法，资源与时机允许先后完成且无实质代价差异 | 投入顺序或互补扩展 | 永久职业、互斥阵营或必然有深度的战略分支 |
| 共享的真实材料、执行窗口或其他约束使优先投入产生后继差异 | 对照两条合法路径，说明本次放弃/延后的用途及后续两个决策 | 用路线标签、预测利润或虚构时限制造机会成本 |

这些分类用于设计与观察，不新增玩家必须选择的模式。目标不同可以使相同方案具有不同价值，但必须说明目标差异；不要求所有玩家均匀选择两条路线，也不能用选择比例代替价值分析。

| 比较维度 | 深化现有生产线 | 建设后继生产能力 | 玩家得到的判断 |
| --- | --- | --- | --- |
| 这次投入什么 | 对应配方的真实材料、电力与生产占用 | 实际建设材料、合法地点与其他真实前置 | 这次采用方案需要承担什么，而不是比较两个路线名称 |
| 留下与放弃什么 | 本次生产后剩余资源，以及同一产线可做的其他用途 | 建设后剩余资源，尚未产出前仍需补齐的条件 | 被占用资源是否真有替代用途；未知余量不写成零 |
| 形成什么能力 | 实际产物和它支持的下一用途 | 实际建设成功后开放的受支持配方范围 | 产物、已建成能力和仅解锁候选分开 |
| 接下来两次决定 | 产物如何使用，是否继续投入该路线 | 如何准备并选择首个合法配方，如何协调现有供给 | 是否真的出现不同问题，而非换文案后回到同一必经按钮 |
| 什么时候能改主意 | 按原专业合同说明未提交、待决和已结算部分 | 同样区分可撤回部分与不可追溯取消的建设结果 | 不把“换目标”包装为退还全部投入 |

界面先给差异最大的成本或约束、形成能力和下一决定，详细配方与账本按需展开。无法证明的收益、工期、需求或效率优势保持未知；候选详情不能取代正式报价。若两条路线最终只是等成本的同一操作，回到固定方案基线并保留经营分支缺口，由玩法 owner 调整真实规则或候选，而不是美化文案。

反证信号是玩家只记住“哪个按钮推荐度高”，却说不出放弃了什么，或后续两个决策没有变化。验证先做同初始状态下的规则对照，再观察玩家自己的理由；不把模拟分叉带入正式世界复制资源。

<a id="goal-delegation-experience"></a>
### 1.5 目标委托：少确认，不少约束

委托首读回答“要做成什么、允许怎样做、遇到什么再来问我”。例如采用一份当前可执行的首笔生产方案，Agent 可代办已接入且被授权覆盖的步骤；没有接线的建厂、补料或排产仍明确由玩家完成。任务目标不是无限许可，也不因为模型说理解了就产生执行保证。

普通重复检查、授权内执行和已覆盖的恢复不逐步弹确认。只有新增目标、超出原授权的成本/范围、原授权未覆盖的不可逆承诺，或必须由玩家重新选择时才请示。请示保留当前成果与未决义务，并提供真实可做的继续、缩小范围、重新授权或停止选项。已经明确授权的成本不反复询问；暂停代办不等于取消已提交动作。

玩家可表达“偏向保留资源”等软偏好，但只有系统实际强制执行的额度/范围才能称作保证。界面应让两者可辨识；不支持的硬限制不能伪装成可保存的开关。验证用一个受支持约束的变化观察 Agent 行为，而不是统计对话轮数、确认数量或生成解释长度。

需要玩家纠正的关键决策，先解释与当前目标有关的理由、可见证据、主要代价、预期后果与真实替代，再说明纠正最早能影响哪一步。纠正被接受与世界结果改变分别反馈；已经发生的成本不因改主意消失。这里提供可核对的理由摘要，不要求公开模型私有推理或不可见信息。

**委派的成对文档推演。** 在同一工厂、同一合法配方与同一批量请求下，对照两份真实可表达的委托：A 允许使用当前可用投入，B 要求保留一项系统实际支持强制执行的资源下限。其余条件相同，若下一笔消耗将越过 B 的下限，B 应在该消耗之前拒绝/停止新的执行；A 仍须满足原有准入与权限，并不保证成功。比较的是是否改变了获准动作及剩余资源，而不是 B 的解释是否更长。若当前入口不支持这个硬约束，明确记为未来验证场景；可提示软偏好，但不能把软偏好替代成保证，也不宣称当前已有额度开关。

同一授权内的合法重复步骤不增加逐步确认；新事实使原授权不再覆盖时，只问具体的新选择，例如“继续此目标会动用原先未授权的储备”，并展示可行替代与代价。尚未确认结果的旧请求先保留原身份与风险敞口并核对；新下限不追溯撤销已经接受的消耗。不存在满足约束的更小合法批次或恢复路线时，诚实给出停止新增执行、改目标或能力缺口，而不是由 Agent 编造一条廉价路线。这一推演验证授权与玩法选择，不是暂停世界或取消已提交工作的承诺。若进一步声称降低重复确认负担，后续运行证据须覆盖至少一次授权内重复执行，再遇边界停止；单次成对请求不能证明连续委派体验。

<a id="readable-causality-experience"></a>
### 1.6 可读因果：同一个对象上的方案、阻塞与结果

围绕当前工厂只前景化目标、实际投入、主要阻塞和下一动作。提交前看到的是“这份方案预计使用什么、形成什么、还受什么条件限制”；提交后看到的是“实际接受/执行到哪里、消费或保留了什么、结果是否已确认”。两个阶段不能共用一个模糊的“成功”标签。

工厂、配方、批量或权限变化时，旧预览保留为历史参考或重新获取，不能继续控制当前提交。结果未知时先核对原请求，而不是引导玩家重复生产；确定失败与未观察到结果分别解释。多个阻塞中先说明安全、权限和不可逆后果，再显示可恢复的生产缺口。

在世界舞台关联选中工厂、实际依赖的输入与已结算产物，帮助玩家理解过程；只表现当前配方真正依赖的环节，不为表现效果虚构运输或多输入等待。技术身份和账本可展开查看，不要求新人读原始 JSON 才知道下一步。

<a id="return-visit-experience"></a>
### 1.7 成果与回访：关心变化，而不是阅读流水账

回访首读采用四个问题：“上次我选了什么；此后能确认什么变化；哪些成果、风险与义务还在；现在有什么值得决定”。旧目标未完成时先帮助续接，目标已完成时再展开后继用途；不要重新把玩家送回首局教学。

首笔铁锭的进展片段说明实际归属与所得、支持哪项后继条件，以及仍缺什么。只有精确因果成立时才说“你的决定带来了这个结果”；不足时仍展示真实成果，但不强行归功。它引用现有回执，不增加第二份庆祝或可结算奖励。

无新事件、服务未运行和事件窗口无法确认分别表达。窗口完整时才总结“离开后发生了什么”；窗口不完整时说明当前已确认状态与缺口。按离线时长推算收益、按库存差值猜测生产、由叙事填补丢失结果，都不属于这套体验。

损失、授权失效和未决请求优先于普通环境消息；重复事件可以合并，关键义务不能被摘要吞掉。回访是否有吸引力，以玩家能否恢复目标、解释变化并自主选择下一步观察；摘要长度、自动重连和 Agent 持续活动不证明真实留存。

<a id="demand-cooperation-experience"></a>
### 1.8 真实合作：先有用途，再有已确认需求

当前先把后继用途讲准确：某产物可用于自己的下一项能力，不等于附近经营者正在求购；出现需求描述，也不等于已签约或已交付。无真实需求记录时使用“潜在用途/自己的后继目标”，不能让 action 对应的固定文案或 Agent 想象承担需求证明。

后续合作样例限定为一个可确认需求方、一种材料或服务、一组可理解条件和一项真实交付后果。玩家需看懂是否值得供给、接受后承担什么、需求取消/缩减或交付未确认时能如何处置；不能先发“区域供应商”称号再等待用途出现。合作自愿，独立成长仍然成立。

当前 production_only 首产物继续独立完成；真实需求、合作承诺、交付与需求减少各自取证，不把完整市场或 terminal-admission 正向链提前加入首局。后续方案由原工业需求与市场专题继续细化，而不是建立平行经济系统。

**后续合作的稀缺材料场景。** 仅在当前世界已有可确认需求与合法交付入口时，比较“将自己可处分的铁锭用于下一项自建能力”与“按真实条件供给另一主体”。先核对可用数量、既有承诺和交付成本，并确认自建路线也真实合法、可达，再说明同一份铁锭不能同时投入两处。若当前支持分批、延后或补料，应纳入比较，不人为制造二选一；不可用的自建候选不能冒充照护的机会成本。若已有合同约束这批材料，就不是可以无代价撤回的自由选择；若只是潜在买方或尚未接受的请求，也不能写成玩家已负债。没有实际需求时，本例仍是候选验证，不生成一名虚构买家补齐合作故事。

交付结果说明谁的哪项已记录目标或缺口改变、自己延后了什么，并区分生产完成、送达与对方确认消费；只多一条“合作愉快”台词不构成结果。玩家可为维护一项真实服务而自愿牺牲扩张速度，也可在无既有义务时选择自建。既往互助、当前义务与期待未来互惠分别呈现，历史不自动保证信任、回报或新增权限。检验时让玩家解释“为什么帮助这个对象而不是另一个用途”，也允许回答“不值得”。稀缺性证明机会成本，不证明玩家必然关心该人物；若交付未改变任何可确认目标，先修用途，不增加社交奖励维持表面热闹。

<a id="strategy-reuse-experience"></a>
### 1.9 复用成功办法：先保存方法，不保存权力

后续从玩家已经反复使用、且有真实效果的一种经营办法开始：说明它解决什么问题、适用对象和前置条件、哪些参数可以调整，以及过去在哪些条件下有效。优先复用现有目标配置、参数或受支持模块，不要求普通玩家先理解 WASM，也不把每个办法都变成独立程序。

“保存办法”不产生世界效果；“再次采用”重新检查当前对象、版本、能力、成本和授权，并形成新的合法目标。旧授权、凭据、未决请求和过去余额不能跟随模板复制，过去的成功不保证再次获得相同收益。使用别人的模板也不扩大读取或执行权限。

保存位置、版本兼容、分享范围和具体采用入口尚未确定前，只保留后续设计候选，不呈现已可用的保存/执行功能。反证信号是模板只减少重复输入，却没有值得重复的玩法，或玩家不能解释采用后会做什么；此时不扩展成商城、排行榜或新的编程平台。

**从一台成功 Smelter 到另一个经营场景。** 候选模板可以保存配方选择、目标用途、检查顺序和参数说明，但将方法用于另一主体、地点或版本时，先重查实际扣费账本、当地可达输入、输出容量和所需权限。原处的库存不自动可用，在途货物不当作已到账；若供给距离、成本或副产物处置已变，旧方案只作比较基线。当前不支持跨区或模板执行时，这只是后续压力场景，不增加首局必做任务。

“采用方法”新建合法目标，与“续接原请求”核对原结果是两种操作。模板不能携带原结算标识来领取旧产出，也不能复制未决消耗或预留；对未知原请求先按原身份核对。新目标仍须扣除原操作真实占用，不因未复制请求而释放原有预留或风险敞口。调整后若仍被有效授权覆盖，不为复制动作机械地追加确认；超出授权时才说明变更成本、范围与所需决定。规模化的价值是玩家从重复录入转向决定用途、储备与供应关系，验证同时观察错误复用、必要干预、结果理解和决策负担。点击变少但执行变得不可解释，不能称作规模化成功。

<a id="experience-hypotheses-and-pacing"></a>
### 1.10 体验目标、学习与节奏

设计从希望玩家获得的体验反推机制，再用实际行为检验，而不是从现有字段或按钮数量推导乐趣。当前优先追求“我知道这次投入为什么值得”“这个 Agent 在执行我的打算”“我已经做成一件有后续用途的事”；与他人的关系和策略分享保留后续范围。

| 体验假设 | 机制与玩家经历 | 可观察结果 | 反证与修订方向 |
| --- | --- | --- | --- |
| 可理解的成果建立胜任感 | 提交前预测主要投入/用途，执行后对照实际结算，再在单个约束变化时选择下一步 | 玩家用自己的话解释所得、剩余条件和可行调整 | 只能背按钮顺序：先修反馈和学习顺序，不增加资源种类 |
| 有边界的委托保留自主感 | 玩家采用目标与真实边界，Agent 代办重复步骤；关键变化时展示理由、代价和纠正点 | 玩家知道哪项决定改变了后续执行，并理解仍须自己选择什么 | Agent 替玩家选完目标或逐步索要确认：调整决策分工，而非增加聊天 |
| 看得见的后继用途形成期待 | 首产物明确完成，展示它满足的后继条件、真实缺口和可选方向 | 玩家在没有催促下选择继续、换向或说明计划回来做什么 | 只有资源数值增长或必须完成下一任务才能退出：先修用途和停点 |

学习按事件推进：先让玩家理解一个主要生产问题，再看到结果与用途，最后按真实可达条件展开比较。首次可用、已经生产、已经形成稳定能力是不同的成就；反馈强度匹配实际结果，不让排队成功抢走首产物的完成感。新概念在成为当前决策前置时解释，详细账本和恢复依据始终可查，不要求先读完所有系统说明。

微循环是“决定 → 委托/操作 → 看见后果 → 调整”；会话循环围绕一项有限成果；跨会话循环保留成果、义务与下一目标。三者可以相接但不强制同场完成。等待时说明真实条件、仍可作出的决定和离场后哪些事实可保留；没有新取舍时不靠重复确认或制造事故填充时长。观察工厂与成果也可以有价值，不把所有非操作时间都判为失败；无法判断进展、无意义重试或只为等待倒计时而停留才需要诊断。

方法依据：[以玩家体验为中心的设计与原型迭代](https://www.gamedesignworkshop.com/)、[决策、信息、反馈与节奏](https://www.gdcvault.com/play/1015756/interesting)、[自主、胜任与关系需要](https://selfdeterminationtheory.org/player-experience-of-needs-satisfaction-pens/)。本节将这些方法转化为本项目的设计假设，不声称方法能保证留存；不复制 PENS 量表或把自拟观察当作经过验证的心理测量。

## 2. 代表性片段与阶段信息

<a id="des-first-profile-001"></a>
### DES-FIRST-PROFILE-001：按选定 profile 展开节点

承接 [REQ-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#req-first-industrial-001)、[002](first-session-and-continuation.prd.md#req-first-industrial-002)、[003](first-session-and-continuation.prd.md#req-first-industrial-003)、[004](first-session-and-continuation.prd.md#req-first-industrial-004)、[005](first-session-and-continuation.prd.md#req-first-industrial-005)、[006](first-session-and-continuation.prd.md#req-first-industrial-006) 与 [PRD profile 适用性](first-session-and-continuation.prd.md#starter-profile-applicability)。当前片段为合法 Smelter、配方与输入/电力、排程、真实生产结算、持久里程碑与 Assembler 候选；来源/精炼、handoff、物流和多输入节点仅在所选配方/profile 明确依赖时展开。后续 terminal-admission 专题独立走到 delivery receipt；当前 production_only 保持 `produced/undelivered`，且不生成交付用途或终端奖励。

下表按这套适用范围读取。不适用必须由所选配方/profile 和现行专业 authority 支持；实际必需 authority 缺失仍保持 unknown/blocked，并返回补证、修复、等待或重新定目标的路径，不能以“不适用”绕过。产品与 gameplay owner 拥有范围和完成边界，runtime/M4 提供真实依赖与结算，Viewer/Agent 消费，QA 按[VAL-FIRST-PROFILE-001](#val-first-profile-001)分别取证。

| 阶段 | 玩家知道什么 | 可以选择什么 | 成本与承诺 | 反馈与下一步 |
| --- | --- | --- | --- | --- |
| Gate 与目标 | candidate 是否由同一份 fresh authority snapshot 支持，以及完成边界、主要风险和复查点 | 接受候选、查看阻塞、改候选或回到其他目标 | Gate 不扣资源、不锁容量、不排程；unknown 不被填成可达 | `candidate_available` 或 `no_safe_starter_chain`；后者给出最早 blocker、保留价值与复查点 |
| 工厂与配方 | 工厂能力、配方适用性、合法输入与电力，以及适用的原料来源和运输风险 | 比较可用配方、处理前置或延期 | 比较不占库存；提交后仍须按当前 authority 重验 | 接受、拒绝或重新报价；下一步是准备真实输入 |
| 来源与物流（依赖适用时） | source/refinement receipt 是否已结算，以及是否已完成 source/owner ledger handoff 并形成带 lineage 的 `ready_for_logistics` 批次；在途和到达是否已结算 | 获取、精炼、补充、减量、等待容量或改道 | 消耗、占用、损耗和未满足量保持可见；`source-settled` 不等于 `ready_for_logistics` 或到达 | `ready_for_logistics`、in-transit、arrival、applicable 各自独立；不依赖物流的合法输入不伪造这些阶段 |
| 输入前置与排程 | 实际输入/电力是否满足；需要多输入 join 时，各 parent 是否已 settlement、到达且适用；排程是否只是接受 | 等待缺失输入、补齐、重排、换候选或结束意图 | 适用 join 的先到输入可保持有界等待；排程接受不等于生产开始 | 主 blocker、已保留价值和下一复查点可读 |
| 生产与交付 | `accepted/scheduled`、执行、`produced/undelivered`、delivery/terminal settled 的差别，以及当前 profile 的完成边界 | 等待、处理输出去向、交付或延期 | `production_only` 在 matching production receipt 后完成首产物；`terminal-admission` 才等待匹配 delivery/terminal settlement；production receipt 不能自动产生交付用途或终端奖励 | 独立 receipt 可回指同一因果链；只有适用 profile 的交付完成才打开目的地后果 |
| 阶段承接 | 已形成的能力、仍在的约束和当前主目标 | 选择扩张、稳定/恢复、专业化/服务或主动换向 | 新方向承担新的资源、容量、风险和机会成本 | 下一 session 有明确第一动作；旧目标的义务和结果仍可追踪 |

状态优先级先保护安全，权利和授权，再保护不可逆损失、资源扣减和锁定，再处理可恢复前置与可选信息。状态不一致时只能给出复核、恢复、安全停止或重新定目标；状态变化后旧动作必须重新判断。

### 2.1 用实体关系回答玩家问题

此表是现有规则的阅读模型，不新增实体、字段或统一 `available` 状态；精确账本、配方、物流与结算仍由[相关专业 authority](#10-相邻权威与未决边界)定义。对象名只有在帮助玩家决定下一步时才前景化。

| 玩家问法 | 必须分开的关系 | 一个典型误读 |
| --- | --- | --- |
| 我看到了矿或材料，为什么不能用？ | 已知资源、当前持有、消费账本、授权、预留/托管和实际可用量 | 地图总量或所有权不等于工厂可消费库存 |
| 这个工厂能替我做什么？ | 工厂能力、合法配方、输入到位、有效槽位/时机和实际产出 | 名义产能或候选可见不等于现在能生产 |
| 货已发出，为什么目标没完成？ | 来源批次、在途义务、目的账本、接收资格与匹配结算 | 路径可达不保证按时交付；产出不是交付 |
| Agent说可以，为什么还要我决定？ | 建议、软偏好、真实可执行能力、有效授权与未决动作 | 生成计划不能授予权限；多次重试不能消除阻塞 |
| 成果是我的，为什么不能随便改用途？ | 归属、既有承诺/占用、终端用途、对方权利与允许的恢复 | 归属不自动解除托管或已经接受的交付义务 |

例外、未知与冲突回到相应对象的 authority 解释，再给实际支持的补证、补料、延期或重新定目标路径。不能因为另一个对象标为可用就跨过当前限制，也不把所有内部对象都变成新人必须阅读的清单。

## 3. 机制与体验关系

| 体验目标 | 玩家决策 | 权威约束 | 预期互动结果 | 失败信号 |
| --- | --- | --- | --- | --- |
| 建立信任 | 是否采用当前 starter candidate | Gate、profile、factory、recipe、input、power，以及明确依赖的 path/terminal authority | 一条有稳定 identity 和完成边界的可解释链路 | `no_safe_starter_chain`、unknown 或最早 blocker |
| 理解投入 | 是否获取、精炼、等待或改源 | source/owner ledger、规格、容量、电力和物流合同 | 资源去向、保留、消耗和未满足量可回看 | 证据不足、争用、低电、容量或适用性失败 |
| 归因结果 | 是否继续、恢复、交付或换向 | production receipt 与 delivery/terminal settlement 分层 | 玩家知道生产完成与目的地后果不是同一结果 | produced/undelivered、pending、delivery 未确认 |
| 保持选择 | 首局后走哪一类持续分支 | 当前世界状态和专业 profile | 后续两个 beat 有实质差异，并给出回访 hook | 没有安全路径时停止并返回决策面 |

## 4. 循环与成长

首局循环采用“目标 → 接受/拒绝 → 推进/阻塞 → 权威后果 → 下一决策或恢复”。所选配方/profile 明确要求从来源获取/精炼并经 handoff、物流供应输入时，才展开来源评估、获取/精炼结算、source/owner ledger handoff、`ready_for_logistics`、运输、到达重验以及适用齐套的子循环。在这条子循环中，`source-settled` 不等于 `ready_for_logistics`，不能被一个“获取原料”动作掩盖，也不能把 source receipt 直接当作下游输入；它不为无需这些阶段的 canonical Smelter 增加新前置。

首次持续能力必须表现为可继续运转、可恢复并能打开新选择的能力。扩张增加覆盖或产出并引入吞吐压力；稳定/恢复保住能力并放弃一部分即时扩张；专业化/服务把能力转成对本地需求或协作的用途。每个方向都要说明即时收益、后续两个 beat、风险或锁定和下次会话第一动作；适用时还要说明回退窗口、代价、保留和失去的价值。

当主目标是本地或区域专业化时，分支详情继续解释生产能力服务什么需求。模块、工厂核心与合金可以是合法下一步；它们的收益和风险不能突然宣称玩家已进入治理或冲突阶段。当前不可执行的动作不作为可立即提交的推荐；后继目标仍可展示其用途、已满足条件、真实缺口与受支持的补足路径。查看/比较候选与执行建设必须区别，信息不完整时返回可读的不可用原因，不用未知条件宣称可达。

回退是有期限的选择。窗口关闭时，玩家应立即看到原因、已保留的结果和义务，以及仍可执行的修复、重排或换向；后续恢复成功不会重新打开旧窗口。各条路线按自身权威条件判断，展示样例的阶段只解释样例，不代替真实世界的回退能力。首产物完成继续按既定 profile 判断，不能为延长回退窗口暗中加入交付条件。

## 5. 行为与后果

玩家意图只表达目标和授权范围，不能直接保证 Agent 行动或世界事件成功。入口展示的 accepted、执行、阻塞、生产和交付状态必须来自同一权威事实；推荐、缓存、叙事或计数不能替代 receipt。

目标换向按 [PRD 的三阶段边界](first-session-and-continuation.prd.md#goal-redirection-pending-and-settled-boundary)表达：预览只用于比较；已接受的承诺 / 待决请求按专业合同保留其有限 hold、排队或未决义务，尚未取得适用于原请求及相应结果范围的权威证据时显示“结果尚未确认”，既不证明已成功，也不证明未生效；只有权威拒绝或其他明确无效果的结果才显示该范围未生效；只有权威 receipt 支持且实际生效的后果才显示为已结算结果。已结算范围不能因换向被抹除或再次执行，未结算范围也不能被同一目标的部分 receipt 提前标成完成。新主目标可以取得前景位置，旧请求的待决状态和合法取消、过期、重评或恢复路径仍须可读；关闭界面、停止后续委托、重新定目标与取消已提交动作分别处理。恢复沿原请求身份核对，刷新到新快照不等于核对了该请求；receipt 的具体含义与结果范围由原专业合同决定。

报价或展示后发生事实漂移时，系统只能重新评估、无副作用拒绝或进入专业合同明确的有界 pending。重连、重复、乱序、Agent retry、snapshot restore 和 replay 只能重读原 disposition，不得复制材料、资格、receipt、进度、奖励或目的地后果。Viewer 与 pure API 的布局可以不同，但动作、主 blocker、完成边界、下一步和复查点必须同义。

## 6. 资源与机会成本

该设计只约定资源来源、用途、持有、流转和损失应可归因：原料可能被保留或消耗，电力和容量可能成为阻塞，物流可能带来占用和损耗，产物可能停留在生产后但未交付状态。正式配方、产率、批量、价格、物流、容量、tick 和结算公式由专业 authority 决定；产品层不填默认值，也不把 unknown 当作零成本、免费输入或无限容量。

## 7. 失败、阻塞与恢复

恢复反馈要指出最早且可行动的根因、已保留/已消费/已损失价值、下一复查边界和真实可用的等待、补充、减量、改源、改配方、改道、延期或停止路径。来源耗尽且无补充、规格未知、低电、输入未齐套、路径或容量失效、输出去向缺失、终端资格变化时，不得无限等待、免费补料、静默改道或把 target/unknown 写成完成。没有安全路径时，返回重新定目标或安全停止。

## 8. 入口与可访问性边界

Viewer 与 pure API 共享权威事实和玩家语义；每个入口分别证明 cold start、进行中、重连/续玩以及空/阻塞状态仍有有效动作或真实恢复动作。设计不冻结控件位置、字段 schema、提示文案或窄屏布局。入口需让状态接受、执行、失败、等待和恢复可辨识，并避免用原始枚举、日志或隐藏字段要求玩家猜测。

## 9. 取舍、验证与证据边界

选择“先通过可行性 Gate、再逐节点推进、最后按 profile 独立结算 production 或 delivery”，保留资源守恒、失败恢复和玩家归因；`production_only` 的交付后置与 `terminal-admission` 的交付前置都必须在结果和下一动力中可读，且生产与交付仍由独立 receipt 表达。

设计覆盖的产品要求和场景见：[`工业因果链要求`](first-session-and-continuation.prd.md#req-first-industrial-002)、[`原料与物流边界`](first-session-and-continuation.prd.md#req-first-industrial-003)、[`生产与交付边界`](first-session-and-continuation.prd.md#req-first-industrial-004)、[`漂移与重复`](first-session-and-continuation.prd.md#req-first-industrial-005)、[`阻塞恢复`](first-session-and-continuation.prd.md#req-first-industrial-006)、[`持续承接`](first-session-and-continuation.prd.md#req-first-industrial-008)、[`正向链路验收`](first-session-and-continuation.prd.md#ac-first-industrial-001)、[`安全恢复验收`](first-session-and-continuation.prd.md#ac-first-industrial-002) 和 [`生产未交付验收`](first-session-and-continuation.prd.md#ac-first-industrial-005)。跨入口与回流的证据边界见 [`回流与入口验收`](first-session-and-continuation.prd.md#ac-first-industrial-004)。

<a id="val-first-profile-001"></a>
### VAL-FIRST-PROFILE-001：当前首产物与后续专题分别验收

本验证承接 [DES-FIRST-PROFILE-001](#des-first-profile-001)、[AC-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#ac-first-industrial-001)、[002](first-session-and-continuation.prd.md#ac-first-industrial-002)、[003](first-session-and-continuation.prd.md#ac-first-industrial-003)、[005](first-session-and-continuation.prd.md#ac-first-industrial-005) 与 FS-15；专业 owner 和原始候选记录沿用 PRD 验收追踪，不创建另一套通过状态。

| 验证范围 | `test_tier_required` 判定 | 适用边界 |
| --- | --- | --- |
| 当前 canonical `production_only` | 合法 Smelter/recipe、实际输入与电力、matching production receipt、owner-bound 正量铁锭、持久里程碑与 Assembler 候选一致；主 blocker 与恢复可读，重复/重连/replay 不复制 receipt、产出或里程碑 | 不要求建成 Assembler、稳定窗口、物流/multi-input 或 terminal-admission 的独立能力；当前选定配方的真实依赖仍必须验证 |
| 当前生产/交付负边界 | `produced/undelivered` 可完成首产物，却不产生 delivery-only 用途、需求减少或终端奖励；production receipt 不能代替 delivery receipt | 保留当前 required 负例，无需先实现完整终端正向路径 |
| 来源/handoff/物流/多输入 | 依赖适用时验证 `source-settled`、`ready_for_logistics`、arrival/applicable 与 join 分层、各主 blocker、适用 arrival-order 和无重复效果；缺必需 authority 为 unknown/blocked | 仅按所选配方/profile 明确依赖加入；无依赖不伪造通过，未知或不支持的必需依赖不得标为不适用 |
| 后续 `terminal-admission` | 独立证明 delivery/terminal settlement 正向完成，以及生产成功但交付失败/未确认仍为 `produced/undelivered` | 专题启动且选定该 profile 后执行，不作为当前首产物完成前置 |

`test_tier_full` 按选定范围覆盖跨窗口、争用、适用损耗/终端故障、持久化恢复及多入口 parity。记录固定候选、profile/revision、实际依赖、证据层级和未覆盖项；文档、自动化或合成/Agent 样例只证明其实际范围，不能宣称当前 runtime、Viewer、Agent、真人留存、可玩性或发行就绪。

目标换向的 [FS-13](first-session-and-continuation.prd.md#fs-13) 验收保留四类对照：原请求已生效但响应丢失时不显示未生效或诱导重提；原请求结果仍未知时不冒充成功或确定失败；部分已结算与仍待决范围分别可读；换向后旧结果迟到仍归原请求，不覆盖新目标。权威 receipt 支持的已结算结果不因换向、重连或重试消失或重复。该语义核对不代表本次已执行真实 runtime/Agent 换向或跨入口游玩验收。

分支比较承接 [REQ-FIRST-BRANCH-001](first-session-and-continuation.prd.md#req-first-branch-001) / [AC-FIRST-BRANCH-001](first-session-and-continuation.prd.md#ac-first-branch-001)：比较主指引和每个推荐的完整承诺，并保留有效的工业用途。回退承接 [REQ-FIRST-BRANCH-002](first-session-and-continuation.prd.md#req-first-branch-002) / [AC-FIRST-BRANCH-002](first-session-and-continuation.prd.md#ac-first-branch-002)：分别观察截止前、截止时和恢复后的选择与后果。具体投影、样例边界及回归方法由 [Viewer 专业设计](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#branch-preview-consistency) 承接；结构正反例不能代签玩家实玩。

<a id="val-first-decision-sample"></a>
### 9.1 当前样本的分项验证

本节复用现有 AC、FS 与 testing 层级，不新增评分系统或全仓库门禁。具体候选、环境、提示介入和观察结果写入关联 Task/PR，不进入长期设计正文。

| 独立问题 | 上游条款 | 验证与接收方 | 不可替代的边界 |
| --- | --- | --- | --- |
| 操作是否正确 | [AC-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#ac-first-industrial-001)、[003](first-session-and-continuation.prd.md#ac-first-industrial-003) | Runtime/M4/Viewer：同参数 quote→submit→实际 ledger/receipt；输入变化、旧响应迟到、在途失效、权限变化、重试与部分结算对照 | 静态合同、mock 与固定操作脚本不证明真实入口已经通过 |
| 玩家是否理解 | FS-16 / [单约束迁移](first-session-and-continuation.prd.md#first-decision-transfer) | gameplay/QA：完成基线后只改变一个受支持约束；记录玩家能否解释目标、实际所得、成本与下一条件 | 给目标和必要情境，不给按钮名称或点击路径；提示前后分开，不将代操作计作自主发现 |
| 经营选择是否有价值 | FS-10 / [首次分支](first-session-and-continuation.prd.md#31-首次分支承诺) | gameplay/Runtime：同初始状态分叉，核对资源替代用途、两步后果与机会成本；再取得适用入口观察 | 只有一条合法路径就记录未形成该分支；参数或按钮数量不是经营分支证据 |
| Agent 是否带来控制感 | FS-12 / [间接控制设计](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007) | Agent/Runtime/QA：明确玩家决定、支持的代办步骤，以及约束/纠正与后续结果的精确关联 | 生产完成、Agent 活动量或解释篇幅不证明主体感；未接入授权的动作不冒称可代办 |
| 是否愿意继续或回来 | [AC-FIRST-INDUSTRIAL-004](first-session-and-continuation.prd.md#ac-first-industrial-004)、FS-8 | gameplay/Viewer/QA：无额外催促的后继选择与理由；自然离场、重入后第一动作和提示介入分别记录 | 内容量、理解、继续点击、Agent 自报不代签真人续玩意愿或真实留存 |

系统接点分别由 [Viewer 当前方案交互](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction)、[M4 报价到提交](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)、[Runtime 未知结果与恢复](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation) 承接；这些是实现与验证义务，不是本次已运行游戏的声明。

本设计的分项场景见[系统设计验证映射](../../game/gameplay/gameplay-decision-delegation-return.design.md#111-验证映射表)：真实分支与回退、完整因果与纠正、授权累计边界、报价与原请求一致性、阻塞理解、跨入口同义性、回访事实连续性及用途/需求/交付分层。沿用上述分项判定，合作与模板候选不加入当前首产物验收；场景名称不代表已有同名测试或已取得真人证据。

<a id="playtest-hypothesis-review"></a>
### 9.2 从设计假设到试玩判断

先验证规则与入口，再观察体验；失败需要定位层次，而不是把所有问题记作“不好玩”。样例与提示使用当前入口实际支持的能力，不让准备好的 Agent 任务环境代替新玩家冷启动。具体版本、事实与提示介入记录在实际 PR/测试材料，不在本文维护通过率。

| 独立观察 | 情境与中性问题 | 判定与下一步 |
| --- | --- | --- |
| 成本与用途理解 | 给出当前真实目标，提交前问“这次投入会换来什么”；完成后问“现在能做什么，还缺什么” | 先区分数量/状态错误、信息找不到和概念未理解；给出按钮路径后的成功只记辅助完成 |
| 有效经营决策 | 在规则对照成立的阶段询问“你先做哪件事，为什么”；另看只有一条可用与两者可兼得的对照 | 核对理由与实际机会成本；多数人选择同一路线不是自动失败，若同目标同约束下另一项始终无价值，则记录支配性方案待改 |
| 委托与纠正 | 提供一个受支持目标/约束，观察重复执行与关键请示；适用的关键因果样例核对理由、证据、代价、替代、纠正最早生效点和结果 | 行为不同不自动证明由玩家引起；保留完整关联并记录模型/环境差异，不能用一次随机输出或解释长度代替控制感 |
| 自然停点与回访 | 成果后允许结束，不要求立刻继续；回来后先观察其自行恢复目标，再询问理由 | 区分主动继续、表示愿意回来、预约回访和实际自发回访；测试员邀请的第二场不算自然留存，合理离场不算失败 |

对照规则使用相同合法初始状态与可比目标；玩家比较需记录经验、顺序、指导与版本，避免把第二次操作学会了误判为方案更好。受随机性影响的 Agent 行为需重复观察或保留可复现条件，不能把单次差异宣称为因果效应。小规模观察用于发现问题和修订，不给出未经统计支持的留存提升结论。

**第二、第三次决定的配对反证。** 先在同一合法初态声明玩家目的与偏好，并确认两项实际可达用途；提交前请其预测投入会延后哪项价值，结算后再作下一决定。另一次从同一初态只改变一项可证实约束，例如输出空间或后继首配方的可达条件，再推进到第二、第三决定。记录偏好为何改变或合理不变，不要求为展示“策略”强迫反转；若变化只是禁用一条路，只证明可达性识别，不证明多策略深度。

将囤料、立即建设及当前合法的分配/延期方案放在实际目标下比较。若囤料在所有可达目标中都无成本，或建设始终支配其他选择，先查真实用途、机会成本与后继决定是否成立，交玩法 owner 调整；不要靠追加随机事故、隐藏成本或奖励文案修饰。合作与模板尚未接入时只记候选场景，不能拿未来能力补足当前首局的第三次决定。

失败按证据分流：事实与结算不一致，修实现；正确事实找不到，修呈现；能背原例却不能解释单约束变化，修例子与教学顺序；理解正确却仍没有值得选择的后果，修规则或目标。文档角色推演只能查逻辑和表达，真实意愿、重玩与留存仍需独立人类观察，不因能讲完三步故事就宣布已验证。

运行正确、信息可读、玩家理解、自主决策和继续动机分别记录。信息足够但仍无值得选择的后果时，交由玩法 owner 调整真实规则或目标；不要继续堆解释。具体来源绑定、入口适用性与场景选择见[系统验证方法](../../game/gameplay/gameplay-decision-delegation-return.design.md#playtest-observation-design)。

## 10. 相邻权威与未决边界

详细玩法顺序、profile 完成边界、receipt 与稳定窗口由 [`工业 walkthrough 合同`](../../game/gameplay/gameplay-industrial-representative-execution-walkthrough.prd.md#3-五阶段玩家-walkthrough) 和 [`首产物结算合同`](../../game/gameplay/gameplay-industrial-starter-completion-contract.prd.md#3-五阶段结算表) 拥有；批次、物流、容量、终端和守恒由 [`M4 合同`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md#4-technical-specifications) 拥有；实现和入口证据由 [`world-runtime 专业 PRD`](../../world-runtime/prd.md#consumer-compatibility-industrial-profile-and-stage-execution)、Viewer、Agent、[`testing 专业 PRD`](../../testing/prd.md#6-validation-decision-record) 等专业 authority 拥有。任何专业规则变化都应重新核对本设计的产品边界和证据窗口。
## PRD REQ/AC fragment mapping

This design delegates product requirements and acceptance semantics to its paired PRD. Every row below is a navigable fragment mapping; this design does not create a second requirement authority.

| PRD requirement | PRD acceptance |
| --- | --- |
| [REQ-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#req-first-industrial-001) Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | [AC-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#ac-first-industrial-001) / [AC-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#ac-first-industrial-002) |
| [REQ-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#req-first-industrial-002) Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | [AC-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#ac-first-industrial-001) |
| [REQ-FIRST-INDUSTRIAL-003](first-session-and-continuation.prd.md#req-first-industrial-003) Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | [AC-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#ac-first-industrial-001) / [AC-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#ac-first-industrial-002) |
| [REQ-FIRST-INDUSTRIAL-004](first-session-and-continuation.prd.md#req-first-industrial-004) Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2 | [AC-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#ac-first-industrial-001) / [AC-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#ac-first-industrial-005) |
| [REQ-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [AC-FIRST-INDUSTRIAL-003](first-session-and-continuation.prd.md#ac-first-industrial-003) |
| [REQ-FIRST-INDUSTRIAL-006](first-session-and-continuation.prd.md#req-first-industrial-006) Trace refs: O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3 | [AC-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#ac-first-industrial-002) |
| [REQ-FIRST-INDUSTRIAL-007](first-session-and-continuation.prd.md#req-first-industrial-007) Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812 | [AC-FIRST-INDUSTRIAL-004](first-session-and-continuation.prd.md#ac-first-industrial-004) |
| [REQ-FIRST-INDUSTRIAL-008](first-session-and-continuation.prd.md#req-first-industrial-008) Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | [AC-FIRST-INDUSTRIAL-004](first-session-and-continuation.prd.md#ac-first-industrial-004) |
| [REQ-FIRST-BRANCH-001](first-session-and-continuation.prd.md#req-first-branch-001) Trace refs: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1 | [AC-FIRST-BRANCH-001](first-session-and-continuation.prd.md#ac-first-branch-001) |
| [REQ-FIRST-BRANCH-002](first-session-and-continuation.prd.md#req-first-branch-002) Trace refs: O7-REQ-2e82fef6-bbb5-43b6-a7d0-833905e8e8a3 | [AC-FIRST-BRANCH-002](first-session-and-continuation.prd.md#ac-first-branch-002) |

