# 首局与持续游玩

当前阶段唯一代表性链采用产品根选定的 `starter-industrial-smelter-to-assembler-v1` / `production_only`：合法地点建成 `factory.smelter.mk1`，以 `recipe.smelter.iron_ingot` 完成合法 cycle，在 owner-bound output ledger 形成首笔匹配正量 `iron_ingot` 结算，保留对应持久首产物里程碑，再开放 Assembler 候选。排程、资源扣减、入列和普通库存变化不能替代该结算；首产物不要求建成 Assembler，也不代签稳定能力、交付或需求满足。

首轮前景化一个主要生产目标，Agent 只代办有效授权范围内的执行。优先观察生产规模与余量，以及缺料/缺电后等待、补足、减量或改道的真实取舍；只展示当前 authority 支持的动作。再次进入恢复旧成果、当前目标、未完成义务和下一动作，按当前授权、资源与版本重新裁决 pending，不复活失效授权，不重新结算旧效果。玩法恢复、节点灾备和玩家回访分别取证。提交结果未知时保留原请求身份核对，不判确定失败；安全停止或重新定目标不等于取消既有未知提交。

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[`prd.md`](prd.md)
- 配对产品 design：[`first-session-and-continuation.design.md`](first-session-and-continuation.design.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-14
- 专业域权威：[`doc/game/prd.md`](../../game/prd.md)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md)

本文是长期产品分册，承载首局微循环、后引导承接与首次持续能力的玩家承诺。它不冻结 UI 字段、tick、数值阈值、任务状态或实现方案。

## 设计适用性与生命周期闭合

- 设计判定：`paired-design`。
- 配对关系：[first-session-and-continuation.design.md](first-session-and-continuation.design.md) 承接本 PRD 的玩家经历、信息层级、状态反馈与恢复解释；产品真值仍由本 PRD 拥有。
- 设计适用性理由：首局工业因果链、持续循环、失败恢复和跨入口阅读由同名 design 承接。
## 1. 产品目标

### 1.1 代表性情境

玩家第一次来到一个仍有资源和生产约束的世界，想把一个可理解的工业目标推进到当前 `starter_completion_profile` 声明的真实完成边界：先确认工厂和配方，再按该配方的实际前置准备合法输入与电力，完成排程与生产；来源获取/精炼、handoff、物流抵达和多输入齐套仅在所选配方/profile 明确依赖时进入这条链。`production_only` 在匹配 production receipt 后完成首产物并保持 `produced/undelivered`，后续 `production-stable` 仍须满足另行声明的稳定条件；后续 `terminal-admission` 专题才独立确认匹配 delivery/terminal settlement 是否产生目的地后果。玩家需要在每一步知道系统是否接受、当前主要阻塞是什么、已经保留或消耗了什么，以及失败、重连或事实变化后还能安全地继续哪条路；首局结束后，他还应理解下一项可以持续运转的能力和真实分支，而不是只看到一次动作成功。

### 1.2 事实、假设与未决问题

- 已验证产品事实：本分册已有首局闭环、首次持续能力、Viewer/pure API 入口、工业 walkthrough、receipt 分层和恢复边界的首个工业闭环需求/验收追踪；专业规则仍由 `doc/game`、gameplay、runtime、M4 和 Viewer authority 承担。
- 设计假设：在首局早期先让玩家理解一条安全、可归因的因果链，再展开持续能力分支，更有助于形成继续游玩的判断；该假设需要适用体验证据验证，不能由结构或局部绿色检查代替。
- 已冻结义务与未决方案分开：真实取舍、可归因成果、授权内代办和安全恢复是产品义务；当前哪两条经营路线具有实质差异、哪些步骤已支持 Agent 委托，以及这些设计是否能被玩家理解，不能由义务存在推导为已解决。具体方案由 producer_system_designer / gameplay_designer 会同 runtime、Agent、Viewer 核对，QA 分别验证理解、主体感与续玩；没有成立样本时保留方案或证据缺口，不转写成实现完成。当前可用性、候选、未覆盖窗口与 residual risk 从关联 Task/PR 读取，不在长期文档记录测试通过状态。

玩家从第一次发出有效意图开始，就能持续回答五个问题：我正在追求什么、系统是否接受、世界发生了什么、为什么被阻塞、下一步怎样继续。首局结束不是体验终点，而是进入可恢复、有阶段成果且能展开中循环选择的持续游玩链路。阶段成果不是世界的通关条件，而是具有完成边界、可归因后果和下一阶段方向的有限进展。

首局信任与首次持续能力是两个相邻但独立的产品结果。前者证明玩家能够可靠地控制、理解并愿意继续当前体验；后者证明玩家已经获得能持续运转、经受阻塞并展开新选择的世界能力。世界仍在推进、进度数值变化或单次动作成功，均不能单独证明玩家被吸引而愿意继续，也不能替代其中任一结果的组合证据。

<a id="first-session-concrete-sample"></a>
### 1.3 当前首局的具体决策样本

当前样本以“生产第一笔铁锭，为评估 Assembler 建设准备材料”为主目标。初始处境只取玩家实际可控制的对象、合法地点、可用材料与电力；已有合法 Smelter 不重复建造，前置缺失就说明真实缺口，不靠免费输入或 debug 注入补齐。完整片段由[配对设计](first-session-and-continuation.design.md#des-first-concrete-sample)承接，复用 REQ-FIRST-INDUSTRIAL-001..008、FS-10/11/12/13/16，不新增首产物完成条件。

先使用**固定方案、准确预览、明确提交**的操作基线：玩家决定是否采用该生产目标、承担当前方案的实际投入、处理真实阻塞或暂缓；Agent 只代办已接入且受有效授权覆盖的步骤。固定参数来自同一规范动作来源，不在产品文档复制数值。尚未接入提交的可编辑预览只能作为独立查询，不得让玩家误以为它配置了正式动作。

经营选择不能由两个批量按钮证明。只有所保留资源有实际替代用途、另一方案具有不可被等成本续批替代的价值，且后续两次决策产生实质差异时，才把参数变化称作经营分支；条件不成立就保留操作基线并记录玩法方案缺口，不虚构效率、奖励、订单或强制事故。此边界不取消首次持续能力后的分支义务。“等成本”包括实际执行时长、占槽与时机；材料和电力线性不代表拆批等价，未核对这些约束前不宣称大批或小批更优。Agent 主体感独立要求玩家能说明“哪个目标或约束改变了它的行为”，生产成功、解释篇幅或确认次数均不能代替该证据。

首产物反馈先讲实际付出和所得，精确因果成立时再讲本次玩家决定的贡献；只有 owner 或时间相近不足以归因。Assembler 候选展示用途、已满足条件、真实缺口与当前支持的补足动作，不能伪装成已建成或立即可提交的动作。首产物可以是自然离场点，离场与回访保留已得成果、仍待决请求和有效委托；不要求当场完成持续能力，也不承诺未验证的离线收益。

0–30 分钟是观察节奏的窗口，不是按分钟强制事故、纠正或成长的脚本；首产物、首局信任、首次持续能力和单次会话分别判定。安装/登录成本与进入可控制世界后的游玩耗时分别记录，不能把前者隐藏在观察起点之外。

## 2. 首局微循环

每个受支持的玩家意图都必须形成可读闭环：

`选择目标或行动 -> 接受或拒绝 -> 推进或阻塞 -> 可读后果 -> 下一决策或恢复`

- 接受、执行、阻塞、改道、无进展完成和有效进展必须可区分。
- 显式推进请求的排队或接受不等于观察到完成；本次完成窗口内无进展必须保持可读、可关联且可继续决策，不能被包装为成功或永久失败。
- 被接受的首局控制不保证立即产生 Agent 行动或世界事件；等待、自动开始、背景推进、重新连接或界面计数都不能替代一次可归因的权威后果。没有观察到进展时，玩家仍应获得当前可信状态和可执行下一步。
- 世界变化必须回到当前主意图的成本、进度与后果，不能要求玩家从原始日志猜测。
- 当前动作不可执行时，系统必须给出原因以及等待、修复、改道或重新定目标中的可行下一步。
- 当前可执行动作与主要 blocker 必须进入玩家入口；内部 snapshot 存在字段不能替代玩家可用证据。
- Viewer 与 pure API 消费同一权威事实，但需要分别证明该闭环在各自入口可用。
- 玩家在提交控制意图前必须能发现当前支持的动作及必要输入；提交后必须能区分接受或拒绝、系统实际理解的动作、失败原因和建议下一步，不能靠盲试协议或原始日志确认系统是否听懂。

### 2.1 状态感知的“现在做什么”动作集

每个正式玩家入口都必须从同一权威状态导出一个规范的、状态感知的“现在做什么”动作集；这里的入口是 Viewer 与 pure API，二者不能因入口不同而让玩家猜测不同的首步。该动作集是玩家决策承诺，不规定字段、接口形状、UI 布局或命名。

- 每个入口在当前状态至少给出一个可立即执行的有效动作，或明确说明当前没有推进动作并给出能恢复决策权的有效动作；“查看状态”或继续等待本身不算满足这一要求，除非它就是当前唯一有意义的恢复决策。
- 对玩家此刻可能合理尝试、但尚不可执行的动作，入口必须同时给出不可用原因，以及对应的解锁条件或恢复路径；不得把不可用动作静默隐藏后要求玩家自行发现前置条件。
- 原因应关联当前目标和世界约束，并把解锁/恢复表述为可选的下一步：例如完成前置能力、补足可恢复的约束、等待可预期的世界恢复、改道至替代目标，或在无安全路径时重新定目标。它不承诺这些路径必然成功或冻结其成本、时机与实现。
- cold start 必须优先给出能建立第一条有效意图链的动作；进行中的目标必须优先给出推进、确认或有价值的改道动作；重连/续玩必须恢复当前目标、blocker 和相应的有效动作；空快照或被阻塞快照必须把恢复、替代或重新定目标作为有效动作，而不能只呈现空动作列表或技术错误。
- 一个动作被解释为暂不可用，不得伪装成已接受、已完成或永久失败；一旦状态变化，入口必须更新有效动作、不可用原因和解锁/恢复路径，使玩家可以重新作出选择。

### 2.1.1 状态置信度与多重 blocker 的动作优先级

动作集必须先通过状态置信度闸门，再进行 blocker 仲裁。这里的置信度是产品语义，不规定快照字段、时钟或实现方式：

- 只有在入口能够取得一份相互一致、仍适用于当前目标的权威状态时，任何会改变资源、权限、路线承诺、设施状态或他人权利的动作才可标记为有效。状态缺失、明显陈旧、来源冲突或无法判断适用范围时，入口必须收窄为取得当前权威状态、重新进入、等待明确复核，或安全停止/重新定目标中的适用恢复动作；不得根据缓存、推断或 Agent 计划猜测一个可执行的世界动作。
- 状态置信度恢复前，入口仍须保留玩家的决策权：至少给出一个真实的恢复或安全停止动作，并说明当前未知/冲突为何阻止原目标。单纯展示“查看状态”、无限等待或空动作列表不构成恢复，除非它就是当前唯一有意义且有边界的复核选择。
- 在状态一致时，多个 blocker 按以下稳定优先级仲裁当前主要 blocker：**安全/权利/授权保护**高于**不可逆损失、资源扣减或锁定**，高于**当前目标的可恢复前置条件**，高于**可选优化或补充信息**。高优先级 blocker 必须先决定主动作；较低优先级 blocker 只有在会改变当前选择、损失、锁定或恢复路径时才同时呈现，不能用次要信息遮蔽高后果约束。
- 同一优先级的多个安全路径不得被静默合并或随机择一：入口可以给出一个有理由的主推荐，但必须保留其他仍适用的可比较路径；如果没有可解释的安全推荐，则明确返回重新定目标或安全停止。推荐排序不能制造新的资格、优先权或世界效果。
- 动作集生成后到提交前若状态、权限或 blocker 发生变化，提交必须按当前权威状态重新判断；旧动作只能被明确拒绝、重新评估或转为恢复/改道提示，不得沿用旧成本、旧资格、旧风险，也不得静默切换到另一个动作。Viewer 与 pure API 可以有不同表现，但这套状态置信度、主 blocker 和重新判断语义必须一致。

专业 gameplay 合同负责把上述决策语义映射到真实动作能力、状态边界与验证样例；runtime 与 viewer/API 专业域分别负责实现和入口证据。本分册不新增 runtime 字段或规定表现层结构。

### 2.2 目标清晰度与首屏优先级

- 首局主目标必须同时说明玩家要采取的动作、怎样算完成，以及玩家可理解的时间或阶段预期；不能只展示描述性主题、内部状态名或没有完成边界的方向。
- 玩家入口优先呈现一个当前主目标。次要目标可以折叠或延后，但必须能被找回，且不能与主目标争夺首屏注意力或给出冲突指令。
- 当前目标的剩余条件、主要 blocker 与恢复动作必须随权威进度更新；世界仍在运行不能替代“玩家知道自己是否推进”的证据。
- 当系统推荐首个采集、探索或工业目标时，玩家需要在行动前读懂推荐对象的预期价值、可达性或进入成本，以及它与首次持续能力的关系；不能只以“最近”或隐藏排序作为理由。
- 推荐首个资源目标时，上述理由可以包含粗粒度材质倾向，但必须明确它只是决策提示，不承诺精确产出、稀有掉落、固定收益或必然完成首次持续能力。
- 首局完成时，体验应回顾已经形成的能力或世界后果，并把主 CTA 交给后引导阶段；一次性庆祝、静态总结或继续观察不能代替下一阶段承接。
- 系统可在首局至首次持续能力期间提供预设目标作为必要引导脊柱；推荐必须来自当前世界状态，并在玩家形成持续能力后降为可选模板，而不是固定职业、强制阵营或无限任务清单。

### 2.2.1 首个工业闭环 walkthrough

首局至首次持续能力至少要能把一条代表性工业目标讲成同一条可追溯因果链。当前 canonical `production_only` 首产物按以下边界验收：

`合法 Smelter 就绪 -> 确认配方与合法输入/电力 -> 排程 -> 真实生产 receipt 与 owner-bound 正量铁锭结算 -> 持久首产物里程碑与 Assembler 候选`

<a id="starter-profile-applicability"></a>
以下适用范围共同约束本节 walkthrough、feasibility Gate、REQ-FIRST-INDUSTRIAL-001..006、相应 AC 与 FS-15；具体依赖由所选配方/profile 和现行专业 authority 判定，不按通用表格自动增加当前前置。

| 范围 | 纳入的前置与验收边界 |
| --- | --- |
| 当前 `starter-industrial-smelter-to-assembler-v1` / `production_only` | 核验合法工厂、该配方实际需要的输入/电力、真实结算、持久里程碑、Assembler 候选与阻塞恢复；匹配 production receipt 完成首产物，不要求稳定窗口、建成 Assembler 或 delivery。保留不得伪造交付、终端奖励或需求减少的负向验证。 |
| 来源/精炼、handoff、物流与多输入 | 仅在选定配方/profile 明确依赖对应阶段时加入，使用下表与专业合同逐节点验收；无该依赖时不要求生成对应批次、物流、join 或 receipt。真实前置的 authority 缺失、未知或不受支持仍须 fail closed，不能改称“不适用”。 |
| 后续 `terminal-admission` | 专题启动并选定该 profile 后，独立验证匹配 delivery/terminal settlement 的正向完成及生产成功但交付失败/未确认；不阻断当前 `production_only` 首产物完成。 |

这是一条玩家可执行的引导脊柱，不是把内部步骤逐项变成任务清单。下表按上述适用范围展开，每个适用节点都必须让玩家知道当前要做的事、什么会让它停下、哪些投入已被保留或消耗，以及完成后下一步是什么；同一事实可由 Viewer 或 pure API 以各自入口表达，但不能改变动作、阻塞或完成边界。

| 引导节点 | 玩家可做的决定与完成边界 | 必须区分的主阻塞、反馈与恢复 |
| --- | --- | --- |
| 工厂就绪 | 检查一座当前可用的工厂，并选择将其作为本次工业目标；“看到了工厂”不算就绪。就绪只表示该工厂可接受本目标的后续比较，不表示已经占用产能或开始生产。 | 工厂能力、权限或必要条件未确认时，明确说明缺失的类别，提供补足前置、改选可用工厂或重新定目标；不得把已接受的目标写成已开工。 |
| 比较配方 | 比较当前可用配方的投入、产出用途、适用工厂、原料来源与主要运输风险，选择一个配方；预览/比较不产生世界效果，也不锁定库存。 | 配方不适用、资格/规格证据不足或当前来源不可用时，保留比较结果并给出换配方、补证据或先处理原料的路径；不能用材料同名或缓存建议推断适用。 |
| 准备原料（按 profile 获取/精炼或直接获取） | 进入下方“原材料准备子循环”，按当前 profile 使用采集/精炼、已持有库存、购买或回收等受支持路径；真实材料与来源/持有证据成立，满足本次物流准入时才算准备完成。已经在目标账本且适用的输入可以直接进入齐套检查。 | 区分获取不足、未结算、不可达、运输准入未满足与待目的地检验；不能强迫直接获取路径再采集/精炼，也不能把合法送检误报为已适用于生产。恢复只使用专业合同当前支持的路径。 |
| 物流抵达 | 选择并提交把已准备的原料送到工厂的路径；“在途”只表示运输义务存在，只有在目标账本/工厂侧实际到达并结算后才算抵达。 | 路线、边吞吐、在途损耗、目的地容量或时限造成的阻塞必须分开可读；玩家只能比较适用专业合同当前支持的等待、改道、减少承诺或取消尚未生效运输等路径，已发生的在途结果不能伪装为未发生或自动退款。 |
| 多输入齐套 | 在同一配方批次中确认所有独立输入均已到达且适用，再让本次生产进入可执行状态；先到的输入可保持在有界等待/保留状态，但不单独算生产进度。 | 缺失、冲突、过期或不适用的 parent input 是齐套阻塞，不得按到达顺序猜测或先消费一部分来冒充齐套。玩家可补齐缺口或等待未决输入；释放未消费保留、重新排配方等处置只有在适用专业合同支持时才可展示，后到输入只重评尚未满足的部分。 |
| 排程 | 在该配方实际输入与电力前置满足后决定何时进入工厂执行；需要多输入 join 时先完成对应齐套。排程被接受不等于已获得执行容量，也不等于生产开始。 | 工厂/阶段容量、电力、窗口或竞争导致的排程阻塞必须说明是尚未开始、暂缓还是拒绝，并显示保留与机会成本；玩家只能比较适用专业合同当前支持的等待释放、重排优先级、改配方/工厂或结束本次意图等路径。 |
| 生产 receipt | 只有配方周期实际执行并按该配方的产物策略结算后，才算“生产完成”；这证明产物在工厂或规定的中间缓冲中形成。`production_only` profile 在匹配 production receipt 后达到首产物完成，但仍保持 `produced/undelivered`，不因此证明已经交付、可交易或已满足目的地用途。 | 生产阶段停机、产物分支无法接收或执行窗口失效时，明确保留的输入、已发生的产出与未决义务；只提供适用专业合同支持的恢复、暂停、改道、重排或终止，不能只显示排程成功。 |
| 交付 receipt（`terminal-admission` profile） | 只有声明需要终端准入的 profile 才继续等待产物通过运输并进入目标终端/目的地的准入或 buffer；取得独立的交付/终端结算 receipt 后，才算该 profile 的“交付完成”。该 receipt 必须能回指本次生产结果和实际运输后果，且只有交付完成后才可把目的地用途作为本链路后果展示。 | 终端容量、目的地资格/需求失效、路线迟到或结算未确认时，生产完成保持生产完成，交付保持未完成；玩家只能比较适用专业合同当前支持的等待、改道、本地用途或结束交付意图等路径，不得把准入、buffer、在途、缓存或预览当作交付。 |

#### 原材料准备子循环：从受支持来源到可进入物流的批次

本子循环按所选配方/profile 的实际输入路径适用。需要从 source 获取/精炼的路径沿 `source assessment -> acquire/replenish decision -> extraction/refinement settlement -> ready_for_logistics` 推进；已持有、购买、回收等直接获取路径按各自已实现的领域合同证明库存、owner、数量与来源，不强制重新采集/精炼，也不伪造 source/refinement receipt。已到达目标消费账本且适用的库存直接进入齐套检查，不虚构一次运输。

本子循环复用产品根 PRD 的 source/deposit 与 handoff、[`M4 工业资源流转合同`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md) 的材料账本/适用性边界，以及 [`gameplay` 工业引导合同](../../game/gameplay/gameplay-top-level-design.prd.md) 的精炼和恢复选择；只说明“原料已合法取得”“可以运输”“可以消费”的区别，不新增获取动作、资源类型、字段 schema、产率或数值公式。

| 子步骤 | 玩家可比较的决策与完成边界 | 必须反馈的阻塞与恢复 |
| --- | --- | --- |
| 来源/库存评估 | 在同一候选与权威快照检查 source、已持有库存、购买或回收等 profile 支持的来源；说明 owner、数量、lineage、新鲜度、获取与运输条件、仍待检验的质量/规格。比较无世界效果，不创建 claim、hold、批次或补充资格。 | 来源未知、过期、耗尽、无权限、争用或持有证据不足分别可读；不支持补充时不能推荐等待补充，不支持购买/回收时不能发明该动作。 |
| 获取/补充决策（若需要） | 确认受支持的获取、精炼、等待补充、换源或减量路径；预览只说明预计数量、成本、风险与复查点，不能变成已取得批次。 | 提交按当前来源、数量、权限、电力及该获取动作真正要求的质量/规格重新校验；争用只按专业有界分配规则处理，不靠重试或拆批插队。 |
| 取得材料及其领域结果 | 采集/精炼分别取得 source/refinement receipt；购买/回收使用各自结果；已持有路径读取现有权威账本及可追溯来源。只有实际形成且可归属的数量可继续，不因重新查看再造 receipt 或库存。 | 来源结果未确认、余额不足或无法证明 ownership/必要 provenance 时不推进；保留已消费、占用、可追回与未满足价值。已有合法库存不得因没有采集/精炼 receipt 而被排除。 |
| `ready_for_logistics` | 本次材料在合法 source/owner ledger 有可对账数量、来源与运输资格，可以按 profile 交给物流；这不等于运输已提交、已到达或已满足配方规格。 | 本次运输真正要求的 owner、合法目的账本、容量及材料运输限制不足时 blocked；若规格/质量必须到目的地检验，只有 profile 明示支持的送检/hold/隔离路径可继续，并保留 `quality/applicability pending`。 |
| 到达并取得消费适用性 | 物流分别处理 submit、in-transit、arrival；到达后读取目的地检验与 recipe applicability，只有同一批次 `arrived + applicable` 可进入后续消费/join。 | 已到达但 unknown/not-applicable 的批次保持有界 hold/隔离或按合同返还、复验、换候选；不得消耗、推进 WIP/稳定进度或自动转成适用批次。 |

运输准入与下游消费准入必须分开：允许把尚待目的地检验的批次合法送达，不等于允许该批次生产。只有 profile 明确支持且运输所需事实齐备时才开放送检/隔离；若未知属性本身也是运输安全前置，仍须在运输前拒绝或补证。不得以“到达后才能验证”的条件禁止唯一受支持的到达检验路径，也不得用合法转运跳过最终质量/规格、owner 或配方适用性检查。

`preview` 无世界效果，领域 receipt 只证明它声明的取得/转换结果，`ready_for_logistics` 只证明运输准备，`in-transit` 不等于到达，`arrived` 不等于 applicable。获取和物流各在自己的提交边界 fresh revalidate；后续事实漂移保留既有结果及最早 blocker，只用受支持的等待、补料/补电、改源、减量、送检/隔离、复验、返还或延期路径恢复。重连、重试、乱序和 replay 不复制任何库存、来源结果、到达 credit、适用资格或进度。

该子循环按[适用范围](#starter-profile-applicability)进入所选链路后，`test_tier_required` 至少覆盖：采集/精炼形成 ready 批次的正向链；profile 支持的已持有/购买/回收直接获取不被采集门槛阻断；批次需目的地检验且允许送检时可合法到达、检验通过前无消费/join，检验失败保持隔离；运输自身必需证据不足仍拒绝；耗尽无补充、争用、低电力、未结算和无安全恢复各保留明确状态。preview、取得、ready、运输、到达和 applicable 分层，重复/重连/replay 不复制结果。`test_tier_full` 再覆盖适用的多来源/多批次、跨账本损耗、乱序、持久化与 Viewer/pure API/Agent 同义性；这些后续子循环样例不自动成为 canonical Smelter 首产物的前置。

本 walkthrough 的反馈按适用范围区分实际发生的状态与尚未满足的边界：**已接受但未开始**、**正在执行/适用的在途**、**已生产但未交付**、**已交付并产生目的地后果**。其中交付部分，当前 production_only 验证尚未交付的负边界；已交付的正向完成留给 terminal-admission 专题。生产 receipt 与交付 receipt 是两个独立完成窗口；重复提交、重连、重排或回放不能复制任一世界效果，也不能把前一个 receipt 代替后一个 receipt。若链路在任一节点停下，玩家看到的主 blocker 应是当前最早且可行动的未满足条件，后续影响可作为次要后果呈现。

当前首局的 `test_tier_required` 覆盖 canonical `production_only` 的正向 walkthrough、各适用节点的可恢复阻塞、生产完成仍为 `produced/undelivered` 且不产生交付/终端奖励，以及重连/重复提交不复制 receipt、里程碑或产出。输入到达顺序与多输入齐套仅在选定配方/profile 明确依赖时验证；`terminal-admission` 的正向及生产成功但交付失败/未确认样例在专题启动后独立验收。`test_tier_full` 再按适用范围覆盖跨阶段窗口、容量争用、在途损耗、终端失败、恢复/换线与 Viewer/pure API 的事实一致性；这些验证由对应专业域执行，本分册只冻结玩家可理解的因果边界。

### 2.2.2 Starter Industrial Feasibility Gate

在系统把预设工业目标作为首局引导脊柱或当前主推荐交给玩家前，必须先运行产品根 PRD 定义的 `Starter Industrial Feasibility Gate`。本分册把它收敛为一张玩家可读的 feasibility card：闸门顶层结果只有 `candidate_available` 或 `no_safe_starter_chain`，不把内部 job、ledger、target contract 或旧缓存当成可行性证明；闸门本身不创建目标、不锁定库存/容量、不扣资源、不排程，也不保证产量、价格、ETA 或长期稳定。

`candidate_available` 只表示同一份 fresh authority snapshot 已证明一条代表性 starter chain 能到达其 profile 声明的完成边界。卡片至少说明稳定的 starter-chain/candidate identity（或等价的不可变上下文摘要）、绑定的 authority snapshot/version、当前工厂/地点、可用 recipe candidate、合法输入、电力与输出前置、主要风险、completion boundary、首个可验证成果、即时收益、`progression_effect`、`next_action`、`next_recheck` 与完成后打开的下一 beat；来源/精炼结果、handoff、物流路径、多输入和终端前置仅按[适用范围](#starter-profile-applicability)纳入。identity 必须覆盖实际改变本候选产出因果的工厂、配方、输入来源、适用物流与完成边界。如果有多个候选，不得静默选定，推荐必须能说明来自当前事实的理由。只有已被专业 evidence 证明的 current path 才能通过；多输入 join、多阶段 root/window、output bundle、terminal settlement 或 maintenance 等 target-only 能力不能借 walkthrough 文本直接当作当前前置。首个可验证成果只能证明该链真实产生了声明的第一个世界结果，不能提前发放稳定/交付成长；即时收益必须是本次选择保住的能力、用途或下一选择，而不是后台完成或免费资源。

`no_safe_starter_chain` 不是永久失败。卡片必须指出最早且可行动的 blocker、证据分类（`current-evidence-backed`、`target-contract` 或 `unknown/not_tracked`）、已保留/已消费的价值、真实可用的补料、补证、等待、修复、改道、改候选或重新定目标路径，以及下一次复查边界；该结果不产生工业成长奖励、`progression_effect` 或下一 beat，只有恢复后从 fresh authority snapshot 重新通过闸门，才能再次进入目标。没有安全恢复时，系统应安全停止并交回其他当前可达目标；不得无限等待、自动补料、后台改道、发放免费输入，或把 target/unknown 填成“可达”。

闸门只负责接受前的可行性判断。玩家确认后仍须经过既有配方、排程、生产、适用交付与 receipt 边界；提交前本候选实际依赖的工厂、配方、输入、路径、容量、电力或终端事实发生变化，或 starter-chain/candidate identity 绑定的 authority snapshot/version 失效时，必须重新判断或无副作用拒绝，不能保留旧 identity、静默切换候选或沿旧 identity 发放 `progression_effect`。`production_only` 的 starter 目标在匹配 production receipt 后完成首产物，仍标记 `produced/undelivered`；另行声明的稳定条件只决定何时可标记 `production-stable`，不得延迟首产物完成。声明 terminal-admission/delivery 的目标必须等匹配 delivery/terminal settlement receipt，不能把生产、buffer 或准入当成交付。

闸门的 current/target 切线是玩家承诺的证据边界，不是新的 runtime 状态：`current-evidence-backed` 可以进入候选但提交仍须 fresh revalidation；`target-contract` 只能作为未来能力或复查方向；`unknown/not_tracked` 必须进入 `no_safe_starter_chain`，并保留未知原因。相同 authority snapshot/version 应得到相同结果；重连、重复请求、Agent retry、snapshot restore 与 replay 只能重读同一 feasibility/receipt 结果，不复制资源效果、目标完成或奖励。在 fresh composite runtime + QA evidence 证明 Gate 与 starter chain 之前，`test_tier_required` 与 `test_tier_full` 只是验收目标，不是当前 pass；已有 Gate、milestone 与投影实现不代签同候选正式默认可用；surface 可说明实现事实，但正式推荐缺当前适用证据时必须返回 `no_safe_starter_chain`。

本闸门的 `test_tier_required` 至少覆盖当前 canonical `production_only` 正向 starter chain、稳定 identity 与 authority snapshot/version 绑定、首个可验证成果/即时收益/`progression_effect`/下一 beat、实际工厂/配方/输入/电力/输出 blocker、target-only/unknown fail-closed、报价后事实漂移，以及重复/重连/replay 无副作用；生产成功不得伪造 delivery、需求减少或终端奖励。来源/handoff/物流/多输入仅按所选依赖追加，terminal-admission 正向及其交付失败/未确认按后续专题独立验证。`test_tier_full` 再按适用范围覆盖多候选争用、跨窗口/多阶段链、持久化恢复和 Viewer/pure API 对结果、blocker、下一步与复查点的同义表达。该卡片不新增配方、数值、runtime schema、任务树、自动补给/改道或 UI 布局。

### 2.2.3 首个工业闭环的叶子要求

本节把 2.2.1 walkthrough、原材料准备子循环和 2.2.2 feasibility gate 的既有承诺整理为稳定产品条目。它不替代原段落，也不新增配方、数值、runtime schema 或实现完成声明。

<a id="req-first-industrial-001"></a>
#### REQ-FIRST-INDUSTRIAL-001：推荐前的安全可行性

Trace ID: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已交付 canonical Smelter→Assembler 的只读 feasibility，生成 authority_snapshot，返回 candidate_available/no_safe_starter_chain 与 blocker/next_action。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/state/starter_industrial.rs#L106-L216)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/tests/industrial_progression_readiness/starter_chain_prerequisite.rs#L27-L74)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 其余实现交付 PR 与适用运行记录未逐条核定；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 剩余范围：当前核对 canonical Smelter→Assembler 只读 feasibility；推荐前所有目标/profile 的安全可行性与正式入口调用覆盖仍需验收。

- 适用条件：系统准备把预设工业目标作为首局引导脊柱或当前主推荐。
- 要求：系统必须先以同一份 fresh authority snapshot 通过 `Starter Industrial Feasibility Gate`，并只返回 `candidate_available` 或 `no_safe_starter_chain`；闸门不得扣资源、锁定库存/容量、排程或发放成长奖励。
- 上位承诺：2.2.2、FS-7、FS-15。
- 专业权威：[`世界规则与玩法系统 PRD`](prd.md) 的 starter feasibility 合同、[`gameplay` 专业设计](../../game/gameplay/gameplay-top-level-design.prd.md) 与 [`M4 工业资源流转合同`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md)。
- 验收：AC-FIRST-INDUSTRIAL-001、AC-FIRST-INDUSTRIAL-002。

<a id="req-first-industrial-002"></a>
#### REQ-FIRST-INDUSTRIAL-002：完整工业因果链

Trace ID: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95

- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 系统设计依据：[viewer-gameplay-release-experience-overhaul.design / starter-plan-interaction](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction)；[industrial-resource-flow-contract.design / starter-quote-submit-binding](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)。已承接：同一方案的查询/提交、事实漂移/未知结果或成果后继交互。
- 实现与验证依据：已交付 canonical owner-bound 首产物投影与 milestone→Assembler 候选承接；历史独立 prepared-world 样本有构建、供料、生产和原结果恢复，不代表当前 main 重跑。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/starter_industrial_outcome.rs#L11-L58)、[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/state/starter_industrial.rs#L155-L200)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/starter_industrial_outcome.rs#L152-L179)、[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/doc/testing/evidence/native-task-starter-chain-2026-10-13/bounded-retry/README.md#L3-L11)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 [实施 PR #4662 的原始样本](https://github.com/eng-cc/oasis7/pull/4662#issuecomment-6162409087)只支持其 prepared-world 范围；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 剩余范围：已核对 canonical 首产物投影与历史 prepared-world 构建/供料/生产样本；需按适用 profile 另核来源精炼、handoff、物流/多输入与 terminal-admission，不能将 produced 写成交付。

- 适用条件：玩家选择一个可行的首局工业目标。
- 要求：玩家必须能沿当前 `starter_completion_profile` 声明的完成边界理解“工厂就绪 → 配方与合法输入/电力 → 排程 → 生产 receipt”；来源获取/精炼、handoff、物流抵达和多输入齐套仅在所选配方/profile 明确依赖时展开，交付 receipt 仅由后续 terminal-admission 专题要求。每个适用节点都必须保留当前动作、完成边界、主 blocker、投入状态和下一步。当前 canonical `production_only` 在匹配的 owner-bound 正量铁锭结算与持久首产物里程碑后开放 Assembler 候选，并保持 `produced/undelivered`，不要求稳定窗口、建成 Assembler 或 delivery/terminal settlement；`terminal-admission` profile 才必须继续到匹配的 delivery/terminal settlement。适用的中间阶段不得被合并成单一成功。
- 上位承诺：2.2.1、FS-1、FS-15。
- 专业权威：[`gameplay` 专业设计](../../game/gameplay/gameplay-top-level-design.prd.md) 与 [`M4 工业资源流转合同`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md)。
- 验收：AC-FIRST-INDUSTRIAL-001。

- 局部专业设计导航：[玩法总纲的首产物完成边界](../../game/gameplay/gameplay-top-level-design.design.md#first-product-boundary)仅接收当前 production_only 的工厂、合法输入/电力、排程与 owner-bound 正量铁锭结算分开；结算后开放 Assembler 候选。 此为总纲局部职责与待执行验证计划，不是本要求完整系统设计或实现通过依据；原状态、剩余范围及既有证据边界保持不变。

<a id="req-first-industrial-003"></a>
#### REQ-FIRST-INDUSTRIAL-003：原料准备与物流边界

Trace ID: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51

- 系统设计状态：未核实
- 实现状态：未核实
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：canonical production_only的factory/recipe/milestone路径已定位，但其合法输入不自动承担条件适用的来源准备合同。 [canonical生产profile读取factory/recipe/site/power与持久milestone，只开放Assembler candidate；不能作为条件适用的source/handoff ready_for_logistics交付。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/state/starter_industrial.rs#L107-L207) [legacy refine quote把Data消耗与hardware output计入同一ResourceKind；只读报价不创建source receipt、lineage material batch或owner handoff。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_simulator/src/kernel/actions_refine_quote.rs#L2-L91) [生产dispatcher把战争/危机、经济合同、工业及物流分别路由；没有把这些普通动作自动解释为专属frontier/认可/赛季合同。](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing.rs#L379-L478) 本轮只读静态审读，未执行验收；实际实现交付PR映射未逐条核清，固定生产源码为交付子范围依据，不以未取得运行证据抹去已证交付。
- 核对日期与基线：2026-10-17；需求/专业设计/静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；本轮对比 main 仅文档变化，本条源码继续适用。无新产品运行验收。
- 剩余范围：需在实际依赖source/refinement的profile追到source receipt、owner ledger handoff、实际损耗/lineage与ready_for_logistics batch；legacy refine quote及production receipt都不能代签该节点。保留后续transfer submit/in-transit/arrival的独立重验。

- 适用条件：选定配方/profile 明确要求来源获取/精炼、source/owner ledger handoff 及物流供应输入；不适用于已由其专业 authority 证明无需这些阶段的 canonical 输入路径。
- 要求：只有 source/refinement 结算后按适用的 source/owner ledger 规则完成 handoff、实际数量/损耗与来源 lineage 可对账，并形成带 lineage 的 `ready_for_logistics` material batch 后，原料准备才算完成；`source-settled` 是 source/refinement receipt 的结算状态，不是 `ready_for_logistics` 的别名。`preview` 不产生世界效果，`transfer submit`、`in-transit` 和 `destination arrival` 必须由后续物流节点独立表达并重新校验。
- 上位承诺：2.2.1 原材料准备子循环、FS-15。
- 专业权威：[`M4 工业资源流转合同`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md) 和 [`gameplay` 专业设计](../../game/gameplay/gameplay-top-level-design.prd.md)。
- 验收：AC-FIRST-INDUSTRIAL-001、AC-FIRST-INDUSTRIAL-002。

<a id="req-first-industrial-004"></a>
#### REQ-FIRST-INDUSTRIAL-004：生产与交付完成边界

Trace ID: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已交付只消费匹配当前 owner、profile、recipe、site、正量 iron_ingot 的持久首产物投影；不由当前余额反推结算。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/starter_industrial_outcome.rs#L11-L58)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/starter_industrial_outcome.rs#L152-L179)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 其余实现交付 PR 与适用运行记录未逐条核定；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 剩余范围：owner-bound 正量首产物投影已核；accepted/executing/produced/delivered 四态在全部入口及 terminal-admission profile 的区分仍未闭合。

- 适用条件：工业链路处于接受、执行、生产或终端阶段。
- 要求：入口必须区分“已接受但未开始”“正在执行/在途”“已生产但未交付”和“已交付并产生目的地后果”，并按当前 `starter_completion_profile` 表达完成边界。`production_only` profile 可在匹配 production receipt 后完成首产物，但仍保持 `produced/undelivered`；只有声明终端准入的 profile 才要求匹配的 delivery/terminal settlement。生产 receipt 不得代替 delivery/terminal settlement receipt，也不得把 production-only 完成表现为已交付。
- 上位承诺：2.2.1 walkthrough、FS-3、FS-4、FS-15。
- 专业权威：[`gameplay` 工业引导成就闭环专业规则](../../game/gameplay/gameplay-top-level-design.prd.md#25-前期工业引导成就闭环)、[`M4 工业资源流转合同`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md) 与 [`world-runtime` 专业 PRD](../../world-runtime/prd.md)。
- 验收：AC-FIRST-INDUSTRIAL-001、AC-FIRST-INDUSTRIAL-005。

<a id="req-first-industrial-005"></a>
#### REQ-FIRST-INDUSTRIAL-005：事实漂移与重复无副作用

Trace ID: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48

- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 系统设计依据：[viewer-gameplay-release-experience-overhaul.design / starter-plan-interaction](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction)；[industrial-resource-flow-contract.design / starter-quote-submit-binding](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；[industrial-resource-flow-contract.design / request-outcome-observation-separation](../../world-simulator/m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)。已承接：同一方案的查询/提交、事实漂移/未知结果或成果后继交互。
- 实现与验证依据：已交付 canonical 首产物投影的 owner/origin 重验及缺失历史事实不回填；局部 prerequisite 拒绝断言核对资源和待建队列不变。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/starter_industrial_outcome.rs#L11-L58)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/starter_industrial_outcome.rs#L152-L197)、[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/tests/industrial_progression_readiness/starter_chain_prerequisite.rs#L77-L130)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 其余实现交付 PR 与适用运行记录未逐条核定；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 剩余范围：已有 intent 原请求去重、首产物归属和拒绝无副作用的局部断言；工业所有 action/profile 在漂移、乱序、restore/replay 下材料/资格/奖励不复制未核清。

- 适用条件：报价后事实变化，或玩家重连、重复提交、乱序、Agent retry、snapshot restore 或 replay。
- 要求：提交必须按当前 authority 重新校验；过期事实只能重新评估或无副作用拒绝，重复/回放只能重读同一 disposition，不得复制材料、资格、receipt、齐套进度、资源效果或奖励。
- 上位承诺：2.2.1 子循环、2.2.2 Gate、FS-13、FS-15。
- 专业权威：[`world-runtime` 专业 PRD](../../world-runtime/prd.md)、[`world-simulator` 专业 PRD](../../world-simulator/prd.md) 和 [`Viewer 手册`](../../world-simulator/viewer/viewer-manual.manual.md)。
- 验收：AC-FIRST-INDUSTRIAL-003。

<a id="req-first-industrial-006"></a>
#### REQ-FIRST-INDUSTRIAL-006：阻塞与恢复路径

Trace ID: O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已交付工业快照的blocker/复查提示及enabled-action fallback；无安全fallback显示下一决策，failure disposition存在时清除过时恢复选择。全部工业来源/物流/接收类blocker与实际恢复效果仍未闭合。 [源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L124-L248)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot_fallback.rs#L6-L105)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/player_gameplay.rs#L480-L525)；[测试定义](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/tests/industrial_progression.rs#L294-L380)。本轮只读静态源码与断言，未运行Rust或正式玩家入口；同SHA CI的业务job为skipped，不能记为这些测试通过。实现交付PR到本条的完整映射仍未核清。
- 核对日期与基线：2026-10-17；需求、专业设计与静态源码基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；仅GitHub只读源码及CI日志审查，未执行本条业务测试、正式入口、跨节点或真人验收。
- 剩余范围：存在工业阻塞与 fallback 投影；所有来源、规格、电力、物流、齐套、接收/终端 blocker 的最早可行动原因、已保留价值及真实恢复动作未逐项证明。

- 适用条件：工业链路出现来源、规格、权限、电力、物流、容量、输入齐套、产出接收或终端阻塞。
- 要求：入口必须指出当前最早且可行动的 blocker、已保留/已消费价值、下一复查边界和真实可用的等待、补充、减量、改源、改配方、改道、延期、恢复或重新定目标路径；没有安全路径时必须停止并返还决策面。
- 上位承诺：2.2.1、2.2.2、5、FS-2、FS-7、FS-15。
- 专业权威：[`gameplay` 专业设计](../../game/gameplay/gameplay-top-level-design.prd.md)、[`M4 工业资源流转合同`](../../world-simulator/m4/industrial-resource-flow-contract.prd.md)、[`world-runtime` 专业 PRD](../../world-runtime/prd.md) 与 [`Viewer 手册`](../../world-simulator/viewer/viewer-manual.manual.md)。
- 验收：AC-FIRST-INDUSTRIAL-002。

<a id="req-first-industrial-007"></a>
#### REQ-FIRST-INDUSTRIAL-007：两入口玩家语义一致

Trace ID: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已核清两入口真实消费者接线：pure API发RequestSnapshot并序列化最新player_gameplay；Viewer buildGameplaySummary从同一snapshot读取目标、blocker、下一步/恢复及工业profile，并实际渲染首工业结果卡。首工业完成边界按settlement、正量iron_ingot和durable milestone归一化；不是只有共享DTO。 [源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/control_blocking.rs#L373-L413)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_client_cli/src/bin/oasis7_pure_api_client.rs#L82-L103)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_feedback_module.js#L332-L350)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_feedback_module.js#L1115-L1157)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/industrial_starter_outcome.js#L5-L16)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_app.jsx#L2690-L2700)。本轮只读静态源码与断言，未运行Rust或正式玩家入口；同SHA CI的业务job为skipped，不能记为这些测试通过。实现交付PR到本条的完整映射仍未核清。
- 核对日期与基线：2026-10-17；需求、专业设计与静态源码基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；仅GitHub只读源码及CI日志审查，未执行本条业务测试、正式入口、跨节点或真人验收。
- 剩余范围：已核清两入口实际consumer，不再以仅共享DTO作为缺口；Viewer存在缺实体/本地fallback归一化，需逐字段核清中断/重连、旧feedback隔离、主blocker优先级、恢复动作和复查点与pure API原始读面同义，含每种适用工业profile。未取得同候选双入口组合运行证据。

- 适用条件：Viewer 与 pure API 表达同一工业链路或恢复状态。
- 要求：两入口必须消费同一权威事实，并对动作、主 blocker、完成边界、下一步和复查点保持同义；允许布局和操作方式不同，但不得让玩家猜测不同的世界结果。
- 上位承诺：2.2.1、FS-6、FS-9、FS-15。
- 专业权威：[`world-simulator` 专业 PRD](../../world-simulator/prd.md)、[`Viewer 手册`](../../world-simulator/viewer/viewer-manual.manual.md) 与 [`testing` 专业 PRD](../../testing/prd.md)。
- 验收：AC-FIRST-INDUSTRIAL-004。

<a id="req-first-industrial-008"></a>
#### REQ-FIRST-INDUSTRIAL-008：首局成果承接持续能力

Trace ID: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3

- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 系统设计依据：[viewer-gameplay-release-experience-overhaul.design / starter-plan-interaction](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction)。已承接：同一方案的查询/提交、事实漂移/未知结果或成果后继交互。
- 实现与验证依据：已交付匹配持久 milestone 后的 build_factory_assembler_mk1 候选和明确 next_action/progression_effect；只证明候选承接切片。 实现定位：[代码](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/runtime/state/starter_industrial.rs#L155-L200)；断言/历史样本：[证据](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/viewer/runtime_live/tests/industrial_progression.rs#L514-L528)。本轮只读源码和断言，未运行 Rust、正式入口或真人验收；历史样本仍以其原候选/环境为准。 其余实现交付 PR 与适用运行记录未逐条核定；[核对任务 #4698](https://github.com/eng-cc/oasis7/issues/4698)仅记录审查。
- 核对日期与基线：2026-10-15；需求、专业设计与静态源码基线 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；本轮无新运行环境或端到端验收结果。
- 剩余范围：canonical milestone 可开放 Assembler 候选；持续能力、一次动作成功、首局信任和安全停止后的下一决策需分别验收，静态阶段文案不能代签。

- 适用条件：玩家完成代表性首局工业阶段成果或安全结束不可达目标。
- 要求：入口必须呈现已形成的能力或世界后果、当前主目标、主要 blocker 与可执行下一步；首次持续能力必须与一次动作成功、首局信任和后续分支分别判定，不得用静态总结或无限等待代替承接。
- 上位承诺：1、3、4、FS-3、FS-4、FS-5、FS-8、FS-12。
- 专业权威：[`世界规则与玩法系统 PRD`](prd.md)、本分册后引导章节和 [`可玩性证据与 claim 边界`](playability-evidence-and-claim-boundaries.prd.md)。
- 验收：AC-FIRST-INDUSTRIAL-004。

- 局部专业设计导航：[玩法总纲的成果承接与自然离场](../../game/gameplay/gameplay-top-level-design.design.md#continuation-boundary)仅接收一次成果连接真实后继及离场/重入，保留原成果、未决义务和适用授权；一次产出与持续能力分开。 此为总纲局部职责与待执行验证计划，不是本要求完整系统设计或实现通过依据；原状态、剩余范围及既有证据边界保持不变。

### 2.2.4 首个工业闭环验收场景

<a id="ac-first-industrial-001"></a>
#### AC-FIRST-INDUSTRIAL-001：正向 starter chain

Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502; O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95; O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51; O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2；声明：[REQ-FIRST-INDUSTRIAL-001](#req-first-industrial-001)、[REQ-FIRST-INDUSTRIAL-002](#req-first-industrial-002)、[REQ-FIRST-INDUSTRIAL-003](#req-first-industrial-003)、[REQ-FIRST-INDUSTRIAL-004](#req-first-industrial-004)；适用范围沿用本 AC 原覆盖条件。

给定：当前 canonical `starter-industrial-smelter-to-assembler-v1` / `production_only` 的工厂、配方、合法输入、电力与输出前置均有 fresh authority evidence；来源/handoff/物流/多输入只按所选配方/profile 的真实依赖增加前置。
当：玩家从 feasibility card 选择候选并完成合法 Smelter 就绪、配方与输入/电力确认、排程和真实生产；仅执行该候选实际需要的来源准备、handoff、物流抵达或多输入齐套。
则：每个适用节点显示可执行动作、完成边界、投入状态、主 blocker 和下一步；production receipt、owner-bound output ledger 首笔匹配正量 `iron_ingot`、对应持久首产物里程碑及 Assembler 候选相互一致；结果保持 `produced/undelivered`，不要求建成 Assembler、稳定窗口或 delivery，也不得产生交付/终端奖励或需求减少。需要物流时，`ready_for_logistics` 与 arrival 继续独立可追溯。后续 terminal-admission 专题使用本 AC 的适用节点并独立增加匹配 delivery/terminal settlement 正向验收，只有交付完成后才呈现目的地后果。

覆盖：REQ-FIRST-INDUSTRIAL-001、002、004，以及适用时的 003；对应 FS-1、FS-7、FS-15。
非证明项：该场景不证明所有工厂、配方、路线都可在首局完成，也不证明当前实现或发行已通过。

<a id="ac-first-industrial-002"></a>
#### AC-FIRST-INDUSTRIAL-002：不可达链路的安全恢复

Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502; O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51; O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3；声明：[REQ-FIRST-INDUSTRIAL-001](#req-first-industrial-001)、[REQ-FIRST-INDUSTRIAL-003](#req-first-industrial-003)、[REQ-FIRST-INDUSTRIAL-006](#req-first-industrial-006)；适用范围沿用本 AC 原覆盖条件。

给定：Gate 或链路的实际前置遇到缺料、低电、权限/规格未知或争用；来源耗尽、物流容量和终端不可用仅在所选配方/profile 明确依赖时适用。
当：玩家查看当前推荐或提交下一节点。
则：系统返回 `no_safe_starter_chain` 或当前最早 blocker，保留已发生价值与未满足量，提供真实的补充、减量、改源、改配方、改道、延期、恢复或重新定目标路径；没有安全路径时停止，不无限等待、不免费补料、不静默改道。

覆盖：REQ-FIRST-INDUSTRIAL-001、006，以及适用时的 003；对应 FS-2、FS-7、FS-15。

<a id="ac-first-industrial-003"></a>
#### AC-FIRST-INDUSTRIAL-003：漂移与重放不复制结果

Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48；声明：[REQ-FIRST-INDUSTRIAL-005](#req-first-industrial-005)；适用范围沿用本 AC 原覆盖条件。

给定：玩家已有 quote 或 accepted intent，之后库存、权限、质量、路径、容量、终端或 authority snapshot/version 发生变化，或玩家重连/重复/乱序/replay。
当：入口再次提交或恢复该意图。
则：未提交或仍待执行部分按当前 authority 重新评估或无副作用拒绝；已存在的原请求先核对其权威结果，不把重读历史结果当作重新执行。相同 receipt/disposition 可被重读，但不产生第二份材料、资格、receipt、生产/交付效果、齐套进度或奖励。验证同参数预览与实际提交、输入/对象变化、查询在途失效与迟到响应；另分别覆盖已生效但响应丢失、结果仍未知、部分结算及换向后旧结果迟到，均不误报未生效、重复消费或归给新目标。

覆盖：REQ-FIRST-INDUSTRIAL-005；对应 FS-13、FS-15。

<a id="ac-first-industrial-004"></a>
#### AC-FIRST-INDUSTRIAL-004：回流、两入口与持续承接

Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812; O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3；声明：[REQ-FIRST-INDUSTRIAL-007](#req-first-industrial-007)、[REQ-FIRST-INDUSTRIAL-008](#req-first-industrial-008)；适用范围沿用本 AC 原覆盖条件。

给定：玩家在首局工业链路中断、重连，或完成首个阶段成果后从 Viewer 与 pure API 继续。
当：入口恢复目标、blocker、已形成能力和下一步。
则：两入口对同一事实、完成边界、主 blocker、恢复路径和复查点保持同义；玩家能看到可达的持续主目标，或在当前条件允许时比较 2 至 3 个实质不同方向。后继目标可展示用途和真实缺口，只有当前可执行动作使用提交型入口；不能以数量要求伪造分支。自然离场保留成果、待决请求和有效委托，重入依据当前事实恢复第一动作，不复活旧授权或重复庆祝旧成果。首局信任、持续能力、Agent 主体感和继续动机分别可判定。

覆盖：REQ-FIRST-INDUSTRIAL-007、008；对应 FS-3、FS-4、FS-5、FS-6、FS-8、FS-9、FS-10、FS-12。

<a id="ac-first-industrial-005"></a>
#### AC-FIRST-INDUSTRIAL-005：生产完成但交付未完成

Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2；声明：[REQ-FIRST-INDUSTRIAL-004](#req-first-industrial-004)；适用范围沿用本 AC 原覆盖条件。

给定：本次工业候选已有匹配的 production receipt；当前 production_only 先验证不得冒充交付，terminal-admission 分支仅在该后续专题启动且选定相应 profile 后独立验证。
当：玩家重连、重新打开该候选，或尝试使用产物的目的地用途。
则：当 `starter_completion_profile=terminal-admission` 且尚未取得匹配的 delivery/terminal settlement receipt 时，系统必须保持 `produced/undelivered`，继续显示交付 blocker 与真实可用的等待、改道、持有、重报价或延期路径；不得表现为已交付，不得产生目的地后果、交付/终端奖励、需求减少或 terminal-admission 用途解锁。只有匹配的 delivery/terminal settlement receipt 才能推进该 profile 的交付完成。当 `starter_completion_profile=production_only` 时，同一 `produced/undelivered` 状态是首产物的正向完成边界；仍不得把它表现为已交付、发放 terminal reward 或解锁 delivery-only 用途。任一 profile 下重读 production receipt 都不得复制生产效果。

覆盖：REQ-FIRST-INDUSTRIAL-004；对应 FS-3、FS-4、FS-15。

<a id="first-decision-transfer"></a>
### 2.2.5 首局决策理解迁移

完成一条 starter chain 是成功基线，还需在随后一次决策中观察玩家是否能将因果理解迁移到变化后的约束。适用验收先沿声明的 `starter_completion_profile` 取得匹配 receipt，再在该链路的下一次受支持行动中只改变一个已知约束；保持其余条件可比，记录变化前后 authority 与完成边界。若前一链路未完成、变化不受支持或同时存在无法隔离的其他变化，应记录未覆盖，不能把它当成理解迁移通过。

变化后观察玩家能否依据入口事实说明当前主要 blocker、哪些已得成果或仍占用价值被保留（以及已消费/损失部分），并选择一个当前有效的下一安全动作及其适用条件。可以通过解释、实际选择或行动理由取得证据，不要求复述术语。专业 walkthrough 的[单约束配对样例](../../game/gameplay/gameplay-industrial-representative-execution-walkthrough.prd.md#single-constraint-decision-transfer)承接具体情境与记录方式。

正常引导与恢复建议仍须可用；本要求是验收观察，不是游戏内强制测验，也不要求人为制造失败、撤销已得成果或阻断继续游玩。分别记录链路是否完成、决策理解是否迁移、是否愿意继续，以及提供了什么提示/代操作、何时介入、介入前后的表现。辅助后的正确选择只证明相应辅助条件下的理解，不能冒充独立判断；链路完成或继续点击不能代签理解，理解也不能代签继续意愿、首次持续能力或真实留存。确定性 fixture 只证明约束与后果；适用入口观察和 playability 证据仍按 `doc/testing` 的独立层级记录，不相互升级。

### 2.3 早期 quote/preview 的信息仲裁

用于某次操作的推荐、预览与提交必须对应同一对象和实际参数。入口区分查看对象、请求只读预览和提交动作；从 Next Move 进入明细后仍能辨认当前对象、方案、主要成本/阻塞、点击的真实效果与返回路径。独立查询必须明确其与正式动作的区别，不能把“查询成功”当作已配置或已执行。

预览的材料需求、有效消费账本、电力和预期产出由对应专业 authority 提供；缺字段就显示未确认，不由 UI 照配方常量补算。输入、对象、权限或适用状态变化后，旧报价不可继续提交，也不能静默替换成新方案。报价失效与在途查询分别处理，不能丢失请求归属或让迟到响应覆盖新方案；安全重新提交必须遵守原请求核对和幂等边界。具体数据关系与时序由 M4、Runtime 和 Viewer 专业设计承接。


首局与早期持续游玩中的既有 quote、preview 与推荐，应优先帮助玩家完成一个当前主要决策，并突出一个会改变该决策的主导 blocker 或成本。可以延后与当前选择无关、且可恢复的补充细节，但延后必须保留可回看的路径和时机；它不能把复杂性伪装成没有代价。

当任何细节会改变损失、锁定、权威移交、不可逆行动或恢复路径是否可用时，必须立即提升为当前决策的显式信息，不能被仲裁为次要或延后内容。仲裁只可重排和解释同一权威事实，不能省略权威成本、改变动作语义、把风险降格，或把建议写成已发生的后果。本规则不规定 UI 层级、字段、数值或实现方式。

## 3. 首局后的阶段承接

首次行动闭环完成后，玩家必须进入正式的后引导阶段，而不是只看到一次性总结或回到无目标观察态。

- 系统提供一个可达的主目标，并说明当前进度、主要阻塞与建议下一步。
- 正式入口默认把这个主目标作为唯一当前焦点，并优先给出“继续推荐路线”的低负担路径；其他目标保持可找回但不争夺当前决策。
- 默认承接应优先帮助玩家形成持续能力，例如稳定生产、恢复被阻塞的能力或完成首次有效协作；不得直接抛出与当前世界状态脱节的宏大目标。
- 玩家可以自由探索、暂时收起目标或在首产物后自然离场，但必须能重新聚焦；重连或回流后恢复已得成果、当前目标、阻塞、原请求待决状态和下一步。后继目标展示与当前可执行动作分开，不以一个不可兑现的“继续建设”代替真实缺口与补足路径。
- 主目标不可达时，体验切换到保全、恢复或替代路径，不能只要求继续等待。
- 玩家只在阶段成果后从 2 至 3 个实质不同方向中选择，或主动要求换向；系统不得把目标作用域、canonical 转译、资源/权限校验、共同治理或审计的内部步骤逐项暴露为常规决策。它们只有在会改变当前目标的成本、锁定、恢复或共同承诺时，才应以简短原因和可执行替代路径出现。
- 首局信任只要求玩家能够可靠理解并继续当前链路；首次持续能力及其后的路线选择应在后续游玩中得到独立证明，不能把两者混成同一结论。

### 3.1 首次分支承诺

首次持续能力形成后，后引导至少应在当前世界状态允许时提供下列 2 至 3 类规范分支中的可达选项；类别是玩家可理解的承诺，不是固定职业、功能清单或强制顺序：

| 分支类别 | 即时收益 | 后续两个 beat 必须发生的实质变化 | 主要约束、风险或锁定 | 下次会话 hook |
| --- | --- | --- | --- | --- |
| 扩张 | 将已证明的能力转为更大的产出、覆盖或选择空间。 | 先要为扩张投入或排除当前瓶颈；随后要处理新吞吐、供给或维护压力，而非重复原有循环。 | 资源被占用、旧目标延后，或暴露新的稳定性压力。 | 回来可检查扩张是否兑现，并决定继续扩张、先稳住，还是改道。 |
| 稳定/恢复 | 守住已获得的能力，移除当前阻塞或降低已知失败风险。 | 先完成恢复、保全或替代供给；随后验证该能力能持续运转并重新开放被阻塞的选择。 | 放弃一段即时扩张机会，且恢复可能保留部分约束。 | 回来可确认稳定结果，并选择利用被保住的能力还是处理残留风险。 |
| 专业化/服务 | 将当前能力转成对本地目标、需求或协作有明确用途的贡献。 | 先选择能服务当前世界问题的用途；随后交付、验证或调整该用途，而非只累积库存。 | 对特定需求、地点或协作机会形成机会成本；不得默认要求加入 major power。 | 回来可追踪该贡献的后果，并选择深化、改换用途或保持独立。 |

每个实际推荐必须让玩家在选择前读懂本局的即时收益、与其他分支不同的后续两个 beat、主要约束/风险/锁定及下次会话的第一件可做之事。只给“扩张”“修复”或“协作”等标签，或让各路线在两个 beat 内回到同一个循环，不构成分支承诺。

可回退的路线还必须说明回退仍有效的窗口、回退的主要代价，以及回退后保留与失去的价值；不可回退或尚无安全回退时也必须明确这一点。上述承诺不冻结成本、数值、世界状态、runtime 字段或 UI 表现；具体可用路线仍取决于权威世界状态。

<a id="req-first-branch-001"></a>
#### REQ-FIRST-BRANCH-001：分支解释与当前阶段保持同义

Trace ID: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1

- 系统设计状态：部分覆盖
- 实现状态：部分实现
- 系统设计依据：[viewer-gameplay-release-experience-overhaul.design / branch-stage-projection](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#branch-stage-projection)。已承接：分支当前阶段投影或样例回退窗口的一致性。
- 实现与验证依据：已交付按当前工业阶段和可执行动作过滤的分支解释，snapshot实际消费该投影；禁用动作不推荐，regional specialist保留工业路线。local_need与后续两步仍是静态说明，不能当作当前区域真实需求或可玩结果。 [源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/branch_commitment.rs#L8-L167)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L139-L160)；[测试定义](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/branch_commitment.rs#L200-L265)。本轮只读静态源码与断言，未运行Rust或正式玩家入口；同SHA CI的业务job为skipped，不能记为这些测试通过。实现交付PR到本条的完整映射仍未核清。
- 核对日期与基线：2026-10-17；需求、专业设计与静态源码基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；仅GitHub只读源码及CI日志审查，未执行本条业务测试、正式入口、跨节点或真人验收。
- 剩余范围：Viewer 分支条款与工业阶段投影存在；尚未证明模块/工厂核心/合金选择的真实本地需求、风险及后续两步在正式入口可达。

- 适用条件：当前主目标允许玩家从已形成的工业能力转向本地或区域专业化。
- 要求：主指引、分支收益、后续两步、风险和回访动作必须表达同一阶段的可达用途。模块、工厂核心或合金等真实可用的生产选择应说明它们服务的本地需求；不得仅凭内部阶段名称暗示治理、投票、领土或冲突机制已经开放。
- 上位承诺：3.1、FS-5、FS-10；合法工业动作和地理意义上的区域需求仍可作为专业化路径。
- 专业权威：[`Viewer 分支投影设计`](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#branch-preview-consistency)。
- 验收：[AC-FIRST-BRANCH-001](#ac-first-branch-001)。

- 局部专业设计导航：[玩法总纲的分支解释与阶段](../../game/gameplay/gameplay-top-level-design.design.md#branch-explanation)仅接收分支收益、风险/锁定、两个后续 beat 与回访第一动作按既有专业合同解释；当前阶段与真实合法动作保持同义。 此为总纲局部职责与待执行验证计划，不是本要求完整系统设计或实现通过依据；原状态、剩余范围及既有证据边界保持不变。

<a id="ac-first-branch-001"></a>
#### AC-FIRST-BRANCH-001：专业化指引不发布未开放的阶段承诺

Trace refs: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1；声明：[REQ-FIRST-BRANCH-001](#req-first-branch-001)；适用范围沿用本 AC 原覆盖条件。

- 给定：玩家处于本地或区域专业化阶段，部分工业动作可执行，另一些候选被权威状态禁用。
- 当：玩家读取主指引、比较分支并查看后续用途。
- 则：各处均说明当前可达的生产、服务或协作用途；禁用动作不会被推荐，合法工业动作不会因其类别被隐藏，也不会把内部阶段名称解释为已获得治理或冲突能力。
- 覆盖：REQ-FIRST-BRANCH-001、FS-5、FS-10。

<a id="req-first-branch-002"></a>
#### REQ-FIRST-BRANCH-002：回退承诺随窗口关闭而失效

Trace ID: O7-REQ-2e82fef6-bbb5-43b6-a7d0-833905e8e8a3

- 系统设计状态：部分覆盖
- 实现状态：未实现
- 系统设计依据：[viewer-gameplay-release-experience-overhaul.design / fixture-rollback-window](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#fixture-rollback-window)。已承接：分支当前阶段投影或样例回退窗口的一致性。
- 实现与验证依据：适用前提是路线自身提供明确有效回退窗口；当前推荐不存在该authority，故不因保留通用收益或有world rollback就判本卡partial。 [typed-action](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7_agent_api/src/world_action/action.rs#L355-L418)；[dispatcher](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/event_processing.rs#L379-L478)；[branch-read](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/viewer/runtime_live/branch_commitment.rs#L61-L168)；[loss-read](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L227-L290)；[world-rollback](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/viewer/runtime_live/recovery.rs#L20-L125)；[restore](https://github.com/nil-studio/oasis7/blob/e27f4d990bc4d481097c003ff75afc582fd8f65d/crates/oasis7/src/runtime/world/persistence.rs#L735-L785)。仅当前默认/内置路径静态审读，未运行业务测试或正式玩家验收。
- 核对日期与基线：2026-10-17；需求固定 PR #4844 `5d03f1fcda523b071c2d75f634165af344ca7b6b`；实现固定 main `e27f4d990bc4d481097c003ff75afc582fd8f65d`；所引源码逐 Git blob 复验，与前轮同源部分复用；无新运行验收。
- 剩余范围：提供回退的路线在比较/下一阶段/恢复后三时点共享有效窗口：正式branch producer只对当前enabled动作作推荐，无窗口期限或关闭authority；fixture文案不是生产消费者；窗口关闭说明原因并保留真正修复/重排/重新定目标：当前fallback或失败reschedule有用，但没有回退窗口关闭因果；不能以普通恢复代签；回退/换向不抹兑现收益或义务，冲突未知不承诺：模块实例rollback与signed checkpoint rollback属于代码/世界恢复，并不是保留已兑现收益的路线选择合同。结论限当前仓库默认/内置及已审正式路径；未提供的线上自定义artifact不在本轮结论范围。

- 适用条件：当前路线提供有明确有效窗口的回退选择。
- 要求：玩家比较、进入下一阶段或完成恢复后，回退可用性、截止条件和解释必须一致。窗口关闭后说明不可回退的原因，保留真实可用的修复、重排或重新定目标路径；已兑现的收益和既有义务不因回退失效或换向而被抹去。状态不足或冲突时不能继续承诺可回退。
- 上位承诺：3.1、3.2、FS-10、FS-13；路线各自拥有截止条件，本分册不统一所有路线的截止时点。
- 专业权威：[`Viewer 回退样例与状态边界`](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#branch-preview-consistency)。
- 验收：[AC-FIRST-BRANCH-002](#ac-first-branch-002)。

- 局部专业设计导航：[玩法总纲的回退窗口边界](../../game/gameplay/gameplay-top-level-design.design.md#rollback-window)仅接收适用时回退期限、成本与保留/损失收益同义；不可回退或无安全路径必须显式说明。 此为总纲局部职责与待执行验证计划，不是本要求完整系统设计或实现通过依据；原状态、剩余范围及既有证据边界保持不变。

<a id="ac-first-branch-002"></a>
#### AC-FIRST-BRANCH-002：截止前、截止时和恢复后的回退信息一致

Trace refs: O7-REQ-2e82fef6-bbb5-43b6-a7d0-833905e8e8a3；声明：[REQ-FIRST-BRANCH-002](#req-first-branch-002)；适用范围沿用本 AC 原覆盖条件。

- 给定：同一路线分别处于回退窗口内、窗口刚关闭和随后修复或完成的状态。
- 当：玩家查看回退说明或尝试继续使用旧的回退选择。
- 则：只有当前窗口允许时才可回退；截止时及之后均说明不可用原因，保留已形成的结果和义务，并展示仍合法的恢复或换向选择。展示样例只能证明自身的状态合同，不能据此声称真实世界支持回退。
- 覆盖：REQ-FIRST-BRANCH-002、FS-10、FS-13。

<a id="goal-redirection-pending-and-settled-boundary"></a>
### 3.2 目标换向与在途意图边界

本节沿用[模块的阶段合同](prd.md#跨资源承诺、失效与重复提交边界)和[会话失效与待决 intent 边界](../player-entry-distribution/prd.md#player-session-authorization-continuity)。主动换向改变玩家接下来要追求的主目标，但不把旧目标的不同处理阶段混成一个结果：

- **预览 / 推荐**只说明候选目标、预计成本、主要 blocker 与下一步，不产生世界效果；换向可以放弃它，不能把预览当作已提交或已占用资源。
- **已接受的承诺 / 待决请求**仍按其专业合同判断可继续、明确取消、过期或重新评估。尚未取得适用于原请求及对应结果范围的权威证据时保持“结果尚未确认”，既不证明已成功，也不证明未生效。只有权威拒绝或其他明确无效果的结果才能说明该范围未生效；提交、接受、有限 hold 或排队记录不能单独证明目标结果已结算。换向、重连、刷新或普通重试不得把旧请求自动迁移到新目标、复制其资格/优先级或产生第二次世界效果；若需要新目标下的行动，玩家或 Agent 必须形成独立的新请求。
- **receipt 支持的已结算结果**只包括已有权威 receipt 支持且实际生效的世界后果；该范围不因换向被追溯取消、伪装成未发生或再次执行。尚未结算的部分仍按承诺或待决请求处理，不能因同一目标已有部分 receipt 就整体视为完成；后续纠错、救济或持续义务只能沿适用专业合同处理。换向前已存在的义务、风险和可归因后果仍须可读。

关闭等待界面、停止后续委托、改变主目标与取消已提交动作是不同操作。恢复必须沿原请求身份核对；取得新世界快照不等于核对了该请求，不得因超时、断连或缺回执用新请求身份重做可能已生效的动作。已知结果与仍未知范围分别保留，合法查询/安全重试不可用时明确说明限制，不提供假恢复按钮。

换向确认后，正式入口只将新目标作为当前主焦点，同时保留旧目标的已生效结果、未决义务/风险和可执行的取消、等待、恢复或重新规划下一步；不得把两个目标同时表现为已接受的当前主线，也不得以切换成功代替旧请求的权威结果。具体请求身份、去重、取消、过期、重评与 receipt 由 runtime、Agent 和 Viewer 专业权威定义。

## 4. 首次持续能力与中循环展开

首次持续能力不是完成一次重复动作，而是玩家建立了一项能够继续运转、修复并产生新选择的世界能力。

- 玩家能读懂投入、产出、当前用途、维护或恢复方式以及下一步价值。
- 首个阶段成果必须在合理的早期游玩窗口内可达；具体时长和数值由专业域与当前验证计划维护。
- 能力证明必须覆盖一次可理解的阻塞及其可恢复处理，表明该能力能继续运转或在受限后恢复，而非只证明一次无阻碍的动作完成。
- 达成后至少展开一个中循环方向，例如生产扩张、区域服务、治理影响或协作保障。
- 每个推荐方向都要说明即时收益、后续体验变化、主要风险或约束以及下次会话的继续理由；方向标签本身不能代替选择后果。可以回退的选择还必须让玩家理解何时仍可回退、回退的主要代价，以及哪些收益会保留或失去，不能用笼统的“可回退”掩盖取舍。
- 首局控制可信度、继续游玩的动机和首次持续能力是相邻但不同的判断，不得用其中一项的通过代签其他项。
- 阶段成果必须使玩家读懂已经形成的能力或世界后果、它为什么不是单次动作成功，以及下一次可以继续、选择或换向的理由；不得将“继续点击确认”或逐动作审核包装为成长。

## 5. 失败与恢复

失败必须保留玩家的下一次决策权：

- 阻塞原因使用玩家可理解的资源、能源、物流、治理、危机、协作或权限类别。
- 恢复建议说明会保留什么、损失什么、何时复查，以及何时应该放弃当前路线。
- 当资源稀缺、采空或恢复节奏阻塞当前目标时，玩家必须能比较等待恢复、转移到其他目标或区域、改走替代材料路线，并读懂这些选择对当前目标的帮助以及等待或改道的主要成本；后台恢复事件不能替代这次选择。
- 没有安全恢复动作时，明确结束当前意图并返回重新定目标或升级约束的决策面。
- 不以无限等待、重复同一操作、隐藏自动改道或泛化错误文案伪装持续游玩。

## 6. 组合验收

- FS-1：首局样例可端到端证明目标、行动、接受或拒绝、推进或阻塞、世界后果与下一步处于同一因果链。
- FS-2：无进展样例能给出可理解的 blocker 和至少一个可执行恢复或重排路径；没有安全路径时能明确返回新的决策面。
- FS-3：首次行动闭环后存在可达的后引导主目标，并能在离开和重连后恢复目标、阻塞及下一步。
- FS-4：首次持续能力样例能证明投入、产出、用途、维护/恢复和后续价值，而不是只证明一次动作成功。
- FS-5：阶段成果后的方向选择至少说明收益、体验变化、风险或约束和继续游玩的 hook。
- FS-6：Viewer 与 pure API 分别提供自身入口证据，且二者对权威事实、动作能力和主要因果保持一致。
- FS-7：首局入口样例能证明主目标包含动作、完成条件和时间或阶段预期，次要目标不干扰当前决策；推荐首个资源目标时可解释其粗粒度价值、可达性与首次持续能力关联而不承诺产出，资源阻塞样例能比较等待、转移与替代路线的主要成本，结束后能进入后引导主目标。
- FS-8：组合证据分别证明首局信任、继续游玩的动机与首次持续能力；持续能力样例包含可恢复阻塞和后续路线，路线样例说明收益、体验变化、风险或约束、回访理由及适用时的可逆取舍。确定性或模拟性证据可以验证规则和体验结构，但不能单独替代真实留存或受控 provider readiness 的专业验证。
- FS-9：Viewer 与 pure API 分别在 cold start、进行中、重连/续玩以及空快照或被阻塞快照证明同一状态感知的“现在做什么”语义：有效动作，合理但不可用动作的原因，以及解锁或恢复路径；空/阻塞快照必须仍保留可作出的恢复、替代或重新定目标决策。
- FS-10：首次持续能力后的样例提供 2 至 3 个当前可达的分支类别，并为每项证明即时收益、实质不同的后续两个 beat、约束/风险/锁定与下次会话第一动作；可回退项还证明窗口、代价、保留价值与失去价值。
- FS-11：代表性早期 quote/preview 样例围绕一个当前主要决策突出一个主导 blocker 或成本，并保留可恢复细节的回看路径；任何损失、锁定、权威移交、不可逆行动或恢复可用性变化均被提升，不会因信息仲裁而遗漏或改变语义。
- FS-12：首局至首次持续能力的样例以预设引导脊柱建立一个当前主目标和可执行“继续”路径；达成阶段成果后只在 2 至 3 个实质不同方向或玩家主动换向时请求选择，后台作用域/转译/校验/治理/审计只在实质影响当前选择时提供原因和替代路径。
- <a id="fs-13"></a>FS-13：代表性主动换向样例区分预览、已接受的承诺 / 待决请求与 receipt 支持的已结算结果。无适用权威证据的原请求保持结果尚未确认，不从缺回执推断未生效或确定失败，提交、接受、hold 或排队不能代签目标结算；部分结果已结算时，剩余部分仍按其承诺或待决范围判断。换向、重连、并发或重试不会追溯取消已结算结果、自动迁移旧请求或产生第二次 receipt。新目标独立形成，玩家能读到旧目标的已生效结果、未决义务/风险及取消、等待、恢复或重新规划下一步，且正式入口不会把旧、新目标同时表达为当前主线。
- FS-14：空、陈旧或冲突状态，以及同时存在多个 blocker 的代表性样例，证明 Viewer 与 pure API 采用相同的状态置信度闸门和主要 blocker 优先级：状态未确认时不提供会改变世界的猜测动作，至少保留真实的复核、恢复、安全停止或重新定目标路径；状态一致时先呈现安全/权利/授权与不可逆后果，再处理可恢复前置和可选信息；同级安全路径可比较且不会被静默合并。状态在展示后变化时，旧动作必须按当前状态重新判断，不得沿用旧资格/成本、静默改道或产生第二次世界效果。
- FS-15：代表性首局工业 walkthrough 在展示为当前主推荐前，必须先通过 `Starter Industrial Feasibility Gate`，并按[所选 profile 的实际前置](#starter-profile-applicability)，沿工厂就绪、合法输入/电力、排程与实际生产结算逐节点证明玩家动作、完成边界、主 blocker、反馈与恢复；来源获取/精炼、handoff、物流抵达和多输入齐套仅在该配方/profile 明确依赖时验证，不增加 canonical Smelter 的隐藏前置。当前 `production_only` 核验匹配 production receipt、owner-bound 正量铁锭结算、持久首产物里程碑与 Assembler 候选一致，首产物完成仍保持 `produced/undelivered`。闸门只返回 `candidate_available` 或 `no_safe_starter_chain`，后者必须保留 current/target/unknown 证据分类、可行动 blocker、下一动作和复查边界，不得发放免费输入或静默改道。walkthrough 区分 accepted-unstarted、active、produced-but-not-delivered 与适用的 in-transit/delivered/terminal-settled；当前 required 证据保留 production-only 完成不等于交付用途、需求减少或终端奖励，以及重连/重复提交/回放不复制任一 receipt、里程碑或产出的负向验证。当前选定 `production_only` 的正向路径与可恢复阻塞需要 `test_tier_required` 证据；arrival reorder 仅在实际物流前置适用时验证。`terminal-admission` 专题启动后独立核验正向与 production 成功但 delivery 失败/未确认情形，不阻断首产物完成；跨窗口/争用/损耗/适用终端故障与两入口一致性进入 `test_tier_full`。

- FS-16：首局成功基线后的单约束配对样例按 2.2.5 记录匹配完成 receipt、变化前后权威事实与未变化条件，分别给出主要 blocker、保留/占用/消费/损失价值、下一安全动作及条件的观察证据；链路完成、理解迁移、继续意愿与提示介入分别判定。缺少适用观察时保持未验证，不以 fixture、代操作或重复确认宣称独立理解。

<a id="fs7-starter-resource-reason"></a>
#### FS-7 既有条件子义务：推荐首个资源目标的理由

Trace ID: O7-REQ-f6aebb85-7a07-4e0b-a3cf-f23655f293a2

本条仅为上文 FS-7 已采用的“推荐首个资源目标时可解释其粗粒度价值、可达性与首次持续能力关联而不承诺产出”登记唯一身份，不新增产品承诺。首个 frag 的专业字段与提示缺失语义仍由[出生 PRD 原条件合同](../../world-simulator/scenario/agent-frag-initial-spawn-position.prd.md#fsp-starter-reason-contract)及[设计接收条款](../../world-simulator/scenario/agent-frag-initial-spawn-position.design.md#fsp-reason-design)拥有；[验证关系](../../world-simulator/scenario/agent-frag-initial-spawn-position.design.md#fsp-verification)只覆盖该局部条件，不证明整项 FS-7、首局可达/产出或正式入口已经通过。FS-7 原全文、测试层级、现有产品状态与其余接收范围不变。

### 6.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| FS-1 | gameplay_designer / runtime_engineer / viewer_engineer | PRD-GAME-004 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md` | 首局目标、行动、接受/拒绝、推进/阻塞、权威后果和下一步的 S6 组合证据 | test_tier_required |
| FS-2 | gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer | PRD-GAME-004 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md` | blocker、恢复、重排及无安全路径时返回决策面的 S6 证据 | test_tier_required |
| FS-3 | gameplay_designer / viewer_engineer / qa_engineer | PRD-GAME-007 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | PostOnboarding 转换、目标恢复、重连续玩的 S6 与 playability 证据 | test_tier_required |
| FS-4 | gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-007 / PRD-GAME-012 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 首次持续能力的投入、产出、用途、维护/恢复和后续价值组合证据 | test_tier_required |
| FS-5 | gameplay_designer / viewer_engineer / qa_engineer | PRD-GAME-007 / PRD-GAME-012 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 分支收益、体验变化、风险/约束和继续游玩 hook 的 S6 证据 | test_tier_required |
| FS-6 | gameplay_designer / viewer_engineer / qa_engineer | PRD-GAME-008 / PRD-WORLD_SIMULATOR-039 / PRD-WORLD_SIMULATOR-041 / PRD-WORLD_SIMULATOR-046 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | Viewer 与 pure API 各自入口证据及权威事实、动作能力、主要因果 parity 对账 | test_tier_full |
| FS-7 | gameplay_designer / viewer_engineer / qa_engineer | PRD-GAME-004 / PRD-GAME-012 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 首局主目标结构、首屏优先级、推荐理由、阻塞恢复与 PostOnboarding 交接的 S6 入口证据 | test_tier_required |
| FS-8 | producer_system_designer / gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-007 / PRD-GAME-012 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 信任、继续动机与持续能力的分离证据；能力恢复、路线取舍与专业留存/提供方 readiness 验证的组合审计 | test_tier_full |
| FS-9 | gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-004 / PRD-GAME-008 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 两个正式入口在 cold start、进行中、重连及空/阻塞快照的有效动作、不可用原因和解锁/恢复路径 parity 证据 | test_tier_required |
| FS-10 | gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-007 / PRD-GAME-012 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 当前可达分支的即时收益、两个不同后续 beat、约束/锁定、回访第一动作及适用时的回退取舍证据 | test_tier_required |
| FS-11 | gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-012 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 首局 quote/preview 的主要决策、主导成本/阻塞、延后信息回看与高后果信息提升证据 | test_tier_required |
| FS-12 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-004 / PRD-GAME-007 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 单一主目标、预设引导脊柱、继续/分支/换向及仅在实质相关时出现的后台护栏 S6 组合证据 | test_tier_required |
| FS-13 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-GAME-004 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 换向、重连、并发与重试下的预览/已接受承诺或待决/receipt 支持的已结算结果分类，缺适用证据保持结果尚未确认而非未生效、部分结算不冒充整体完成、旧请求不自动迁移、已结算结果不追溯取消、单次 receipt、新目标独立形成，以及旧义务/风险与下一步的玩家可读性组合证据 | test_tier_full |
| FS-14 | producer_system_designer / gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-004 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 空/陈旧/冲突状态与多 blocker 的状态置信度闸门、稳定优先级、同级路径比较、展示后重新判断，以及无猜测动作/无静默改道/无第二次效果的 Viewer + pure API 组合证据 | test_tier_full |
| FS-15 | producer_system_designer / gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-007 / PRD-GAME-012 / PRD-WORLD_RUNTIME-019 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | 按[profile 适用性](#starter-profile-applicability)固定 Gate 两结果、candidate/snapshot identity、current/target/unknown 与 blocker/恢复/复查；当前 required 核验 Smelter 正向/阻塞、matching receipt/正量铁锭/持久里程碑/Assembler 候选一致、不得伪造 delivery/reward，以及重复/重连/回放单次结果。来源/handoff/物流/多输入及 arrival-order 仅在所选依赖成立时追加；terminal-admission 正向及交付失败/未确认在后续专题独立取证；适用 full 证据核验跨窗口、持久化恢复与 Viewer/pure API 同义。fresh composite runtime + QA evidence 前不作 current claim | test_tier_required + test_tier_full（按 profile 适用性） |
| FS-16 | gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer | PRD-GAME-007 / PRD-GAME-012 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `doc/game/gameplay/gameplay-industrial-representative-execution-walkthrough.prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 2.2.5 单约束配对样例的完成基线、权威变化与保留价值/安全动作合同证据；入口观察分别记录理解、续玩意愿与提示介入，沿 testing 既有层级独立给出结论 | test_tier_required（合同） + test_tier_full（适用入口组合）；playability 独立 |

具体字段矩阵、测试命令与历史 verdict 不复制到本分册。当前样本按[配对设计的分项验证](first-session-and-continuation.design.md#val-first-decision-sample)分别记录操作正确性、玩家理解、经营选择、Agent 主体感与继续/回访意愿；给玩家目标和必要情境，不给按钮名称或点击路径，提示/代操作前后分别记录。固定脚本、fixture、内容量和 Agent 自报不代签自主理解或真人留存。

### 6.2 首个工业闭环叶子追踪

| 叶子要求 | 原条款/证据 | 验收场景 | 专业 authority |
| --- | --- | --- | --- |
| REQ-FIRST-INDUSTRIAL-001 | 2.2.2、FS-7、FS-15 | AC-FIRST-INDUSTRIAL-001/002 | 根 PRD starter feasibility、`doc/game`、M4 |
| REQ-FIRST-INDUSTRIAL-002 | 2.2.1、FS-1、FS-15、profile completion boundary | AC-FIRST-INDUSTRIAL-001 | gameplay industrial walkthrough、starter completion profile |
| REQ-FIRST-INDUSTRIAL-003 | 按[profile 依赖](#starter-profile-applicability)适用的原料准备子循环、FS-15 | AC-FIRST-INDUSTRIAL-001/002（适用时） | M4 工业资源流转合同 |
| REQ-FIRST-INDUSTRIAL-004 | walkthrough 四类阶段、profile completion boundary、FS-3/4/15 | AC-FIRST-INDUSTRIAL-001/005 | gameplay/runtime 结算合同 |
| REQ-FIRST-INDUSTRIAL-005 | 子循环、Gate、FS-13/15 | AC-FIRST-INDUSTRIAL-003 | runtime/Agent/Viewer authority |
| REQ-FIRST-INDUSTRIAL-006 | 2.2.1/2.2.2、FS-2/7/15 | AC-FIRST-INDUSTRIAL-002 | gameplay/M4/runtime authority |
| REQ-FIRST-INDUSTRIAL-007 | FS-6/9/15 | AC-FIRST-INDUSTRIAL-004 | Viewer/pure API/testing authority |
| REQ-FIRST-INDUSTRIAL-008 | 1、3、4、FS-3/4/5/8/12 | AC-FIRST-INDUSTRIAL-004 | 产品根 PRD 与 playability evidence |

## 7. Non-Goals

- 不定义固定 UI 布局、提示文案、事件字段或计时阈值。
- 不要求完整动态任务树或由 LLM 自由生成任务。
- 不把玩家锁进线性教程，也不把自由探索解释为无目标漂浮。
- 不要求玩家逐动作确认 canonical 转译或审核后台校验；预览、报价和治理细节只在它们实质改变当前选择时服务于该选择。
- 不以历史任务完成态或旧版本样本声明当前体验已经通过。
- 本 walkthrough 不新增配方、产能、物流损耗、运输时限、队列、库存或价格数值，不冻结 runtime 字段、事件/receipt schema、UI 布局或具体任务文案；这些仍由 `doc/game`、M4/runtime、Viewer 与 QA 专业权威承接。
- 本 walkthrough 不承诺每座工厂、每个配方或每条路线都可在首局完成；它要求当前被选中的代表性链路能解释可达性、阻塞与恢复，并在不可达时返回安全的替代或重新定目标路径。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-FIRST-INDUSTRIAL-001](#req-first-industrial-001) / [AC-FIRST-INDUSTRIAL-001](#ac-first-industrial-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-INDUSTRIAL-001](#req-first-industrial-001) / [AC-FIRST-INDUSTRIAL-002](#ac-first-industrial-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-INDUSTRIAL-002](#req-first-industrial-002) / [AC-FIRST-INDUSTRIAL-001](#ac-first-industrial-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-INDUSTRIAL-003](#req-first-industrial-003) / [AC-FIRST-INDUSTRIAL-001](#ac-first-industrial-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-INDUSTRIAL-003](#req-first-industrial-003) / [AC-FIRST-INDUSTRIAL-002](#ac-first-industrial-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-INDUSTRIAL-004](#req-first-industrial-004) / [AC-FIRST-INDUSTRIAL-001](#ac-first-industrial-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-INDUSTRIAL-004](#req-first-industrial-004) / [AC-FIRST-INDUSTRIAL-005](#ac-first-industrial-005) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-INDUSTRIAL-005](#req-first-industrial-005) / [AC-FIRST-INDUSTRIAL-003](#ac-first-industrial-003) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-INDUSTRIAL-006](#req-first-industrial-006) / [AC-FIRST-INDUSTRIAL-002](#ac-first-industrial-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-INDUSTRIAL-007](#req-first-industrial-007) / [AC-FIRST-INDUSTRIAL-004](#ac-first-industrial-004) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-INDUSTRIAL-008](#req-first-industrial-008) / [AC-FIRST-INDUSTRIAL-004](#ac-first-industrial-004) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-FIRST-BRANCH-001](#req-first-branch-001) / [AC-FIRST-BRANCH-001](#ac-first-branch-001) | `gameplay_designer` / `viewer_engineer` / `qa_engineer` | [`Viewer 分支投影设计`](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#branch-preview-consistency) | 主指引与各分支的阶段同义、真实动作过滤及本地用途回归；各入口体验证据独立 | `test_tier_required`；适用入口体验 `test_tier_full` |
| [REQ-FIRST-BRANCH-002](#req-first-branch-002) / [AC-FIRST-BRANCH-002](#ac-first-branch-002) | `gameplay_designer` / `viewer_engineer` / `qa_engineer` | [`Viewer 回退状态设计`](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#branch-preview-consistency) | 同一路线截止前、边界、修复后与完成态的正反例；样例与真实世界证据分开 | `test_tier_required`；真实入口适用时 `test_tier_full` |
