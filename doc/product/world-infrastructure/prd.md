# 权威世界基础设施 PRD

## 文档身份

- 产品模块：权威世界基础设施
- 产品模块 slug：`world-infrastructure`
- 产品层唯一 PRD：`doc/product/world-infrastructure/prd.md`
- 产品模块总入口：`doc/product/README.md`
- Product PRD-ID：`PRD-PRODUCT-002`
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：`2026-10-09`
- 后继文档：`无`
- 下层专业域：[`doc/game/prd.md`](../../game/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)

本文只承载产品承诺、范围 taxonomy、跨域 authority 与组合验收。专业域 PRD 继续拥有各自规则、实现契约、PRD-ID 和测试证据；任何下层专题都不得再自称“权威世界基础设施”的并列产品总入口。

## 1. 产品承诺

权威世界基础设施是 oasis7 的区块链/分布式系统与确定性世界运行时基础。它提供一个可验证的权威世界历史：合法激活的提交权威 profile 决定何时提交，网络和存储复制并提供 hash-bound 状态材料，确定性执行把已排序输入转成世界状态，恢复重建同一历史而非第二个世界。

它是下层 provider，而不是设施、市场、区域、frontier、组织治理或玩家循环的产品入口。世界规则与玩法系统、智能体、世界模拟与交互、玩家接入与发行在此基础上组合各自产品语义；它们都不能借由本模块的技术能力直接扩张为权威写入权。

### 使用者情境与正常路径

玩家希望知道一次行动是否真的改变了目标世界，Agent 需要据此决定下一动作，节点运营者需要知道副本何时可以提供可信状态。正常路径是：消费者先确认目标世界身份与可用性，读取已验证的状态，再按适用协议提交意图；按合法 profile 验证同一有序输入的提交证明，持久提交条件成立后产生已提交结果，消费者核对其 receipt 才更新世界结论。提交被入口接受只是请求进入处理路径，不是资源、权限、生产完成或交付的保证。

当当前条件不能得到证明时，使用者可以查看最后可信状态、等待、重新验证或重新规划；撤回、替代和补偿只能采用专业合同明确支持的路径。等待可能延迟行动，重新规划可能放弃原计划，但重连和重提不会免费解除已经提交的义务、返还已消费投入或取得新的优先级。实际资源成本、损失和可退出条件由相应玩法及专业合同定义，本模块不增加费用或完成时间承诺。

### 阅读术语

`world_id` 是世界身份；`canonical` 指该世界唯一的权威顺序；`intent` 是请求产生效果的意图，`pending` 包括已知尚无效果的待决与提交状态未知、待核对的请求；后者不能推断无效果；`committed` 指已进入权威历史，`finality` 是使该提交可验证且不能由本地推测替代的最终性。`receipt` 是关联已提交结果的回执，不是再次执行授权；`manifest` 是版本化执行规则及工件的声明，提交端兼容声明不能选择它。`hash-bound` 表示材料与其哈希承诺绑定，`state root` 是用于核对状态一致性的根；`fail closed` 表示无法证明安全条件时拒绝或保持无效果，而不尝试产生部分结果。

工业 `root` 是一次操作的不可变因果身份，`revision/child` 记录因果变更和子效果，`lineage` 是这些请求或操作的可追溯关联；它们不等于状态根。`hold/reservation` 是有界且排他的容量或投入承诺，`WIP` 是在制状态。下文单独说明 `W`、window 与 lease 的专业语义，不能从英文名称推导资源、进度或授权。共识消息、执行版本字段和上述关联的具体格式仍由专业 authority 定义。

## 2. 范围

本模块只覆盖区块链/分布式底层和其上的确定性世界执行。它不拥有上层的游戏规则、Agent 行为或玩家入口，但这些消费者必须遵循其最终性、版本与 committed-state 边界。

### 活跃产品专题

- [`分布式共识与状态可用性`](distributed-consensus-and-state-availability.prd.md)：共识最终性、单一 canonical history、复制/状态同步、存储角色、网络暴露、bootstrap 与灾难恢复。
- [`确定性世界执行`](deterministic-world-execution.prd.md)：共识之上的版本化确定性执行、验证者重执行、消费者协议、升级 lane 与 finality 缺失时的 fail-closed 语义。
- [`完整世界历史与无损存储`](lossless-world-history-storage.prd.md)：完整历史与恢复依赖的保留边界、无损减少重复存储、冷热读取、独立副本、回收与迁移，以及容量和恢复的组合验收；目标合同已承接，实现与运行证据按专题分别列示。

### 非权威迁移索引

- 连续性 CR-1/2 的产品语义由本模块活跃专题 DC-1/DC-2 与 DE-1/DE-3 接收；CR-3/4/5/6 的跨模块产品映射见产品总入口。原 CR-1..6 条款及完整映射保留在 Git 历史提交 `a83c57893de2ec4217fb7c1a04d30575ebd783e6`，仅作为来源 provenance，不构成 active authority 或验收。
- 区域 charter、地点 tenure 与公共融资已由世界规则与玩法系统的区域专题接收，不构成本模块 authority。工业/市场/保供产品语义已由世界规则与玩法系统接收；对应专题和稳定条款由该模块根入口列出，本模块不重复声明 authority。PRD-GAME-018 消费对应的 GR/SC31/ES 产品锚点见[世界规则与玩法系统模块入口](../world-rules-core-gameplay/prd.md)。消费/组合说明：普通治理当前接收与 GG-1..5 历史映射见世界规则与玩法系统[模块入口](../world-rules-core-gameplay/prd.md)中的普通共同决策专题 §6.3；本模块不拥有该产品 authority。原 frontier 与区域能力/扩展迁移债务已由世界规则与玩法系统吸收，不再保留在本模块索引。

此前的区域设施、市场/工业、charter、frontier 与普通治理分册已从本模块退休：它们是上层 gameplay/world-rule 产品语义，不能再作为基础设施的 taxonomy 或验收门槛。对应专业域与其他产品模块继续拥有其规则、实现合同和现状证据；本次退休不宣称这些能力已迁移、实现或公开可用。

## 3. 权威与冲突处理

| 层次 | 本模块产品承诺 | 专业域权威 |
| --- | --- | --- |
| 分布式底层 | 验证者集合、最终性、唯一顺序、复制、可用性、存储与恢复共同守住一个权威 `world_id` 历史 | `doc/p2p/prd.md` 拥有共识、网络、节点、存储与运维技术合同 |
| 确定性执行 | 已最终化输入经版本化规则产生唯一世界结果；执行与共识具有独立语义边界 | `doc/world-runtime/prd.md` 拥有状态机、执行、升级、receipt 与 replay 技术合同 |
| 消费者 | gameplay、Agent 与入口以稳定协议提交 intent、观察 committed state 或验证证明 | `doc/game/prd.md`、`doc/world-simulator/prd.md`、入口/Viewer authority 拥有各自产品行为与体验 |
| 证据 | 目标能力只在同一候选的跨域专业证据成立时才可表达当前状态 | `doc/testing/prd.md` 与根 `README.md` 分别拥有验证和公开 claim envelope |

产品层不定义共识实现、签名算法、消息/存储 schema、runtime ABI、游戏规则、Agent 行为、UI、经济参数或操作 runbook。冲突时以更窄的安全和权威边界为准，并由 P2P 或 runtime 专业 owner 显式裁决。

## 4. 路线图

1. 首次开放先完成受控单权威持久世界的正式提交、独立故障域副本、旧写者隔离与同世界恢复；兼容 threshold prototype 不取得正式权威。
2. BFT 协议并行研发，完成持久可验证证书、round、锁定、验证者转换和分区恢复后，按同世界 H/H+1 交接激活。
3. 建立 hash-bound state availability、按角色的存储/serving、bootstrap/recovery 信任链和 restore drills。
4. 把 deterministic execution 与共识隔离为可版本化语义合同，完成 certificate-gated execution、升级与 replay 证明。
5. 以同一协议支持 game/Agent/入口的并发消费，逐步扩展 proof-serving 与 light companion，同时保持 finality 缺失 fail closed。

<a id="world-infrastructure-current-stage"></a>

### 当前阶段：持久单权威到同世界多节点

首次开放的目标 profile 为 `controlled_single_authority`：受控权威决定提交，证明历史来源、顺序与连续性，正确性信任包含运营权威。后续 `bft` 以合法验证者集合的完整协议证书提供适用故障模型内的共识保证。两者复用一个世界、动作、确定性执行、receipt 和恢复协议；现有单签/threshold prototype 仍是原型，本次文档收敛不宣称任一正式 profile 已通过。

`world_id`、`chain_id`、`genesis_hash` 固定；既有保留承诺存在时必须沿用既有合法身份与历史。软件、manifest 与 authority 可按创世或已提交升级规则演进，不能要求所有未来 manifest hash 不变，也不能重发资产、清空成果或把新创世称作恢复。世界保留承诺、发行阶段、网络环境和资产价值分别表达：受限 preview 可保留合法身份、设施、材料和资格，生产结算资格仍由经济/发行条件独立决定，网络 tier 不自动改变资产价值。

首次开放前必须保证单主机/单存储故障域失效后已确认效果不丢失；独立故障域未持久确认完整记录及正式提交决定时不能回应 committed。最终存储原子检查 epoch、parent/head 和顺序，旧 signer/写权隔离无法证明时停写。超时或回复丢失而无法证明未提交时显示“提交状态未知、待核对”，按同一请求身份核对，不判确定失败或制造第二效果。已知无效果 pending 与未知提交必须分开表达；下文“无效果 pending”条款只适用于前者。

同世界恢复先验证历史并提供只读，再核对当前追加权、head、版本与持久条件开放写入。正式交接在旧 profile 的 H 提交后不可撤回，H+1 起只允许新 BFT profile，即使新集合尚未产生首个提交也维持停写。协议、提交/准备区分、隔离与演练细节由 [P2P 权威合同](../../p2p/prd.md#p2p-authority-profiles) 和 [runtime 执行合同](../../world-runtime/prd.md#runtime-authority-profile-commit) 拥有，不在产品层复制 schema。

### 基础不变量

- 单一世界：global canonical order 与 `world_id` 不因区域、环境、节点或缓存而分叉；local development 世界使用独立身份且永不并入。
- 消费者在提交动作前必须能确认目标 `world_id` 及其属于 global-authoritative 世界还是隔离的 local/development 世界；intent 与 receipt 只对该身份下的历史有效。local/development 结果不能作为 global 世界结果的证明，身份缺失或不匹配时不得产生或宣称 global effect。具体 surface 如何表达 scope 仍由其专业 authority 定义。
- 最终性先于效果：未获得当时合法 profile 的可验证正式提交证明并满足持久条件的 intent 不产生权威世界结果；基础设施不可用时 progression fail closed。
- 可重建性：恢复仅接受 manifest、certificate、hash-bound snapshot、canonical replay 和 verified state root 组成的信任链。
- 可替换实现：oasis7 保有协议语义；可选择性采用成熟库，但依赖被版本化合同隔离。
- 消费者边界：game、Agent 和入口与基础设施并发运行，却不能直接写 canonical state 或把 pending 伪装成 committed。

### Finality 缺失时的意图连续性

当已签名并送出、尚待 finality 的 intent 等待最终确认，而 finality 变为不可用、陈旧或无法验证时，玩家与 Agent 可以继续观察最后一个已验证的世界状态，并保留该 intent 作为**尚无世界效果的待决请求**；这不授予资源、控制权、资格、声誉、阶段完成或任何依赖该 intent 的后续结果。提交被接收、本地排队或界面仍显示该请求，都不能被表达为执行成功或对完成时间的承诺。

恢复后，待决请求必须先按当时仍有效的权限与前置条件重新进入 canonical 顺序；它可能被执行、拒绝、过期或需要替换，且只有可验证的 committed receipt 才能改变玩家/Agent 的世界结论。产品体验必须让消费者区分“仍待决且未生效”与“已无效、被拒绝或须重新规划”，并提供查看恢复状态、等待下一次可验证结果或在对应专业域允许时明确撤回/替换/重提的路径。不得静默重放失效控制、重复执行同一请求，或在恢复前后把本地推测包装成权威结果。

当玩家或 Agent 将一个仍待决的 intent 明确作为替代对象时，替代动作本身也只是待决请求：在 canonical committed receipt 确认之前，它不得单方面取消、覆盖、隐藏或宣称优先于原请求，也不得让两个互斥选择同时被表述为已获授权、已保留资源或即将生效。产品必须保留原请求与其撤回/替代请求之间可读的关联，并分别表达仍待决、已提交替代、撤回/替代被拒绝、原请求已执行或替代已生效等真实状态；若专业域不支持安全的撤回或替代，入口只能提供等待、查看状态或重新规划，不能把普通重提伪装成取消。该边界既防止 outage 后的重复/抢先效果，也不阻止专业域明确允许的独立并发 intent。

对于被专业域明确标为同一 intent lineage 中互斥成员的请求，首个产生有效世界效果的 committed receipt 是唯一胜者，并原子地将其余待决成员终止为无效果、不可执行且可追溯的状态。拒绝或过期只终止其自身，不获胜也不取消其他成员；未被标为互斥成员的独立 intent 仍可并发。产品层不定义 lineage 标记、排序、原子化或 receipt schema。

本条只约束跨域产品承诺；pending 的持久化、去重、重试、过期、receipt 字段和具体 UI 由 runtime、P2P、Agent 与入口/Viewer 专业域分别拥有。

### 恢复目标身份与世界隔离边界

恢复成功只能恢复原有的 `world_id` 及其可验证历史，不能把“某个节点重新可运行”“可读取部分数据”或“建立了新的本地/测试世界”表达为玩家原世界已经回来。恢复候选必须同时能关联既有 `world_id`、受验证 checkpoint 或 snapshot、canonical replay 与 state root；任一关联缺失、相互冲突或指向其他 `world_id` 时，产品结论只能是原世界**不可用且尚未恢复**。此时不得继续接受新的权威 intent、为新世界发放原世界的资格或资源结论，或让入口把替代 endpoint、缓存状态、旧截图或本地开发世界伪装成连续世界。

玩家在该状态下可以查看最后一个已验证 receipt 与明确的世界不可用/待恢复说明；这不是新行动、资产转移、控制权、Agent 授权、组织资格或已完成进度的恢复承诺。已经 committed 的历史仍以其原 receipt 为准；未 final 的 intent 继续遵守上文的待决规则，不因恢复目标不明而被静默确认、取消、重放或迁移。恢复完成后，只有同一 `world_id` 的验证链重新成立，消费者才可按 committed state 恢复结论。

若原 `world_id` 无法被证明恢复，产品不得自行把玩家、资产、Agent、组织或历史复制到替代世界，也不得把替代世界称作自动升级、灾备恢复或无损迁移。是否建立新世界、是否提供受治理的迁移方案及其同意、结算和申诉规则，是需要独立跨模块决策的产品变更；在该决策及其专业合同与证据完成前，唯一安全路径是维持隔离并如实报告不可用。具体 identity 检验、checkpoint 格式、停止服务/投票机制、endpoint 行为和界面/API 字段仍由 P2P、runtime、Viewer 与测试专业域拥有。

### 恢复后的重新服务闸门

恢复证明与重新接受权威写入是两个不同的产品结果。即使同一 `world_id` 的历史已经可以读回，消费者也不能仅凭节点可运行、状态可读或 replay 已结束，就把入口重新表述为可以提交新 intent。恢复后的对外状态至少分为以下三类产品语义（不冻结字段或枚举名称）：

- **恢复只读**：同一世界的 identity、checkpoint/snapshot、canonical replay 与 state root 已重新关联，玩家/Agent 可以查看最后一个已验证 receipt、历史和仍无世界效果的 pending；但当前追加、最终性或执行服务尚未重新满足其专业启用条件。新 intent 必须原子拒绝或保持明确的无效果待决，不能取得资源、资格、排队优先或隐性承诺。
- **恢复可服务**：除同一世界验证链外，当前版本化执行规则、追加/最终性权威和 head 的单调连续性也已由专业证据重新成立，消费者可以把新的结果绑定到新产生的 committed receipt。只有进入这一语义状态后，入口才可接受新的权威 intent；“进程已启动”“能读取旧状态”或“某个 endpoint 可达”均不足以满足该闸门。
- **恢复受阻/隔离**：identity、版本、head、root、追加权威或最终性证据缺失、冲突、回退或再次失效。消费者回到不可用/待恢复语义，不接受新的权威 intent，也不把只读历史、缓存或替代世界表达为连续服务。

从恢复只读转为恢复可服务时，既有 pending 必须以当前仍有效的权限、资源、版本和前置条件重新裁决，并按 canonical 顺序处理；停机期间的排队、客户端重试或历史 receipt 不得自动延长期限、继承优先级或制造第二次效果。替代/撤回请求继续与原请求保持可读关联，只有各自的 committed receipt 才能确认其世界结果。若重新服务闸门在提交或恢复过程中失效，系统必须在产生世界效果前退回原子拒绝或无效果待决语义；已经确认的 receipt 不因闸门回退而被撤销、重放或改写。

恢复可服务还须证明正常工作能够继续。在提交权威、世界版本、权限、资源和输入条件保持相同且有效的受控样例中，从较早合法检查点及其后完整、非空的权威事件历史恢复后，面对相同后续动作，接受或拒绝的结果、模块再次行动的逻辑时点及业务结算必须与不中断运行的对照一致；至少一条合法新配方须正常完成一次，不能因恢复而被误认为历史已完成任务。已确认的等待、暂停和费用仍按原历史延续，不提前行动、不漏掉应有行动、不重复计费。具体正向验收由 [AC-DCS-002](distributed-consensus-and-state-availability.prd.md#ac-dcs-002) 承接；“至多一次”与恢复时状态相等不能替代该结果，也不由此承诺无条件可用或完成时限。

这项闸门保持 **world-first**：是否重新开放写入由当前可验证世界与执行/最终性条件决定，而不是由本地进程或界面状态决定；保持 **persistent / auditable**：只读恢复、可服务恢复、再次隔离以及 pending 的重新裁决均可与同一世界历史关联；保持 **extensible**：未来可增加服务角色或证明步骤，但不能把只读恢复降格为可写，也不能跳过当前条件。

### 工业流水线的跨域执行边界

工厂、配方、原材料和物流不是四条可以各自结算、最后再拼成“生产完成”的本地流程。它们只有在同一条可验证世界历史中形成一条因果流水线时，才可以产生玩家或 Agent 能据此行动的工业结果。本节只定义基础设施与工业消费者之间的组合边界，不定义配方比例、产能、价格、物流算法、排队或玩家体验；工业生命周期与材料适用性仍由 [`世界规则与玩法系统`](../world-rules-core-gameplay/prd.md) 及其链接的 M4 / runtime 专业合同拥有。

- **单一流水线身份：** 在工业动作跨过“已接受”边界时，权威世界必须为该次操作建立一个不可变的 root identity，并将它绑定到同一 `world_id`、当前版本化执行规则和 parent committed state。工厂能力/位置、配方版本、原料来源与目标账本、物流 path/edge 以及其 revision/segment 只作为该 root 的可追溯上下文；具体字段、事件和 receipt schema 由 [`world-runtime industrial execution status and authority matrix`](../../world-runtime/prd.md#industrial-execution-status-and-authority-matrix) 定义，产品层不复制 schema。
- **跨阶段守恒：** 工厂启动、配方输入 join、原料 handoff、运输/到达、buffer 或产物交付等 child effect 必须保留 root、所属 revision/segment 与直接 parent/child 关系。首个不可逆 input sink、运输/目的账本 credit 或产出进度前，root、版本、来源/目的地或 parent 关系缺失、冲突或无法验证时，必须原子 fail closed；预览、推荐、pending 或本地排队不得产生上述世界效果。
- **容量承诺与有界背压：** “已接受”只建立该次工业意图的 root，不等同于已经取得阶段槽位、边吞吐、目的地 buffer/terminal 容量或原材料预留。任何 hold/reservation 必须数量有界、不可被多个下游同时承诺，并绑定 root、revision/segment、阶段/边、批次、消费主体和生命周期；预览、报价、计划、accepted-pending 或本地排队不得生成 hold。每条中间边与目的地 buffer 必须有确定性的容量上限和满载处置：保留未消费的上游投入、仅在仍有容量时接收已结算产出，或原子拒绝/延期新的上游承诺。不得静默丢弃、瞬移、无限堆积、自动改道或伪造下游完成；同一过期、撤销、重规划、容量释放或到达事件对同一 hold 的释放，以及该事件触发的 fresh authoritative snapshot 重评，各最多产生一次效果。后续不同的权威事件可以再次重评仍有效意图的未满足部分，但不得复制 hold/库存/进度、回滚已消费量或靠 replay 重复同一效果。
- **因果变更与恢复：** 在同一因果计划内恢复只能继续原 root；若改变工厂、配方、输入/输出、物流边或终端等会改变产出因果的条件，必须建立与旧 root 可读关联的新 revision/child root，重新校验输入、容量、权限和世界版本，不能静默迁移旧任务。重试、重连、snapshot restore 与 replay 只重读同一 root 的 disposition；同一阶段/边/批次不得复制 sink、credit、进度、receipt 或奖励。
- **阻塞与可观察性：** 跨域消费者必须能从 committed state 区分预览、已接受、进行中、被阻塞、已完成与已终止，并定位最早可归因的 stage/edge、仍占用与已消费的投入、下一次权威复查边界和真实可用的恢复/退出路径。Viewer、Agent 和纯 API 可以采用各自读面，但不得把工厂可用、配方已接受、物流已发运或本地重连表述为交付、结算或生产完成。

#### Finality / service outage during industrial execution

现有的通用 finality 规则只能说明“待决 intent 尚无世界效果”，而工业流水线还必须区分**尚未形成 root 的待决请求**、**权威执行链已形成 root 但暂时无法推进 child effect**与**非权威读面暂时不可用**。本节的 service outage 只在明确的权威边界下改变工业处置；Viewer、pure API、hosting 或其他便利读面不可用不能被包装成世界暂停，也不能由缓存或本地 accepted ack 代签世界结果。

- **W 与窗口/租约的语义边界：** `W` 仅引用 [`gameplay stable-line contract`](../../game/gameplay/gameplay-top-level-design.prd.md) 所拥有的稳定产线 candidate/window 计数与 progression 语义，不是基础设施资源、材料数量、receipt 数值、扣款、奖励或可由 endpoint/重试写入的计数器。相关 `window identity` 分别识别适用的 canonical service、measurement 或 stable-line candidate window；`lease` 识别该窗口内的容量/义务持有、有效期与 release/expiry lineage。它们可以按专业合同关联，但都不等同于 `W`，也不能从 lease TTL、停机时长或 receipt 数量推导 `W`。基础设施只携带这些 gameplay disposition 与可追溯锚点，不新增或重算稳定窗口进度。
- **提交前权威不可用：** finality/append/execution 尚未取得 committed 证据的工业 intent 不创建 root、hold、reservation、input sink、WIP、物流 credit、产出或稳定产线 candidate 进度；玩家只能看到仍待决、被拒绝/过期或需要重新规划的真实结果。重连、重复提交和本地排队不能把它升级为已接受工业操作，且此时没有可重置的 `W`。
- **提交后权威执行中断：** 已有 committed root 的流水线若在 stage finish、transit、buffer admission 或 terminal settlement 前遇到 finality、append、execution 或其权威 industrial service outage，已提交的 hold、sink、WIP、在途或已到达状态保持原 root/lineage 与实际数量；中断本身不产生新 child effect、额外扣减、物流/交付 credit 或奖励。中断本身也不由基础设施直接改写 `W`；恢复时若 gameplay authority 针对**当前未完成 candidate**产生一次 canonical interruption disposition，该 disposition 可以按稳定产线规则将该候选的 `W` 重置一次，而不删除 root、已消费/仍占用投入、WIP、既有 receipts 或历史里程碑。相同 disposition 的 retry、reconnect、restore 与 replay 不得再次重置；若因果变更建立 linked revision/child root，则新 candidate/window 按 gameplay contract 重新计数。
- **非权威读面/便利服务中断：** Viewer、pure API、hosting 或明确不拥有 world state/finality 的 read surface 不可用时，只能将观察标记为 `stale/unknown` 或等价的未仲裁状态；不得改变 canonical world effect、root、投入/数量、W、window/lease lineage、receipt、历史里程碑或 progression。重连后先从同一 `world_id` 的 authoritative snapshot/journal 重建并 reconcile 已提交 receipt；若权威世界在读面中断期间继续推进，surface 只补读真实 committed 结果，不制造第二次效果；若没有权威效果，则不因读面恢复而重置、补发或续租。注意：若 storage/hosting 本身是该世界的权威 append/execution 依赖，应归入上一个权威中断类别，而不是按便利读面处理。
- **恢复闸门：** 权威服务恢复后，下一 child effect 必须从同一 `world_id` 的 fresh authoritative snapshot 重新校验版本、factory/recipe fit、批次适用性、owner/权限、路径与容量、power/terminal 前置和仍有效的 `window identity`/`lease`；结果只能是沿同一 root 继续、进入 profile 支持的有界 hold/defer/reject/expire disposition，或因产出因果变化建立已有规则要求的 linked revision/child root。恢复不得按 outage 时长隐式续租、重排优先级、补发失效 ETA 或把旧 quote 当作新授权；world-time/order、window/lease expiry 与 `W` reset disposition 以 canonical journal/replay 和 gameplay authority 为准，不以客户端墙钟或重试次数重算。
- **替代与审计：** 只有专业合同支持的撤回、替代或补偿才能终止/转换已提交操作，并须保留原 root、已消费/仍占用投入、在途损耗和前后 disposition 的可读关联；普通重提不能取消旧操作或绕过其终端结算。每个恢复重评、释放、终止、`W` reset 或补偿至多产生一次效果，既有 receipt 不被撤销、重写或重复消费。

该补充保持 **world-first**：权威执行 outage 与读面 outage 由不同的 world-state 边界决定，工业效果由 committed finality 和恢复后的权威重评决定，不由 endpoint 可达或本地 ack 决定；保持 **emergence-first**：玩家仍可在等待、修复、改道、延期或受支持处置之间作有边界的选择，不获得 outage 后免费重启或免费稳定进度；保持 **persistent / auditable**：root、child、hold、window identity、lease lineage、`W` disposition、投入、损耗、receipt、恢复原因和历史里程碑跨停机、读面恢复与 replay 延续；保持 **extensible**：未来可加入不同的 service/finality profile，但必须声明其属于权威执行链还是非权威读面，不能把未 final 请求变成已生效或跳过 fresh revalidation。

玩家可以区分“尚无世界效果的待决请求”“权威执行已提交但服务受阻的工业操作”“非权威读面不可用的 stale/unknown 观察”和“恢复后重新校验的继续/终止结果”，查看状态、root/窗口/租约 lineage、已消费与仍占用的实际数量、receipt、`progression_effect`、真实复查边界和下一动作；权威 interruption 若由 gameplay canonical disposition 触发 `W` reset，玩家能看到一次性 reset 与保留的历史里程碑，读面 outage 则显示 `W` 未改变并在重连后 reconcile。玩家不能把 accepted ack、缓存、旧 quote、重连、普通重提或便利服务恢复当作 root、预留、交付、自动续租、`W` 进度或第二次生产授权。runtime/consensus 事件、window/lease 算法、gameplay `W` 规则、具体 UI 和 Agent retry 策略仍由专业 authority 冻结。

该边界保持 **world-first**：工业结果由同一 `world_id`、版本化执行与 committed receipt 决定，而不是由某个设施进程或客户端缓存决定；保持 **emergence-first**：玩家/Agent 可在约束内选择来源、工厂、路线和恢复，不获得预写的免费转换；保持 **persistent / auditable**：root、child lineage、批次、receipt、损耗与 terminal disposition 跨重连、恢复和 replay 延续；保持 **extensible**：未来增加新的工厂、配方、材料或物流 profile 时复用同一组合边界而不改变既有历史。

<a id="world-infrastructure-done"></a>
## 5. Done：成功标准与验收

<a id="sc-infra-001"></a>
### SC-1：成功标准与状态

- SC-1：唯一世界按合法激活的 profile 验证提交证明。首次开放的 `controlled_single_authority` 必须证明单故障域恢复不丢已确认效果、唯一追加权与幂等；后续 `bft` 必须形成受治理验证者集合的 deterministic BFT certificate 并通过 H/H+1 交接。错误签名、profile、epoch、parent 或适用阈值/集合/round 均不能推进历史。

- Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46。
- 引用定位与组合边界：O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39（[REQ-DCS-001](distributed-consensus-and-state-availability.prd.md#req-dcs-001)）；O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46（[REQ-DCS-004](distributed-consensus-and-state-availability.prd.md#req-dcs-004)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
#### 功能状态卡：SC-1

- Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46。
- 引用定位与组合边界：O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39（[REQ-DCS-001](distributed-consensus-and-state-availability.prd.md#req-dcs-001)）；O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46（[REQ-DCS-004](distributed-consensus-and-state-availability.prd.md#req-dcs-004)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
- 系统设计状态：部分覆盖
- 系统设计依据：合法 profile、持久提交和交接的 [P2P 编排](../../p2p/design.md#des-p2p-authority-profiles)、[目标 BFT 证书验证](../../p2p/design.md#des-p2p-target-bft)及 [runtime 证明绑定](../../world-runtime/design.md#runtime-deterministic-design)已有明确接收；错误 profile/epoch/证书拒绝与 H/H+1 不可撤回分别保留。
- 实现状态：部分实现
- 实现与验证依据：正式读面 canonical cut 绑定、执行桥 committed height 连续性，以及 prototype validator membership/stake 拒绝代码已交付。 [crates/oasis7/src/world_service/formal_read_service.rs L20-L49](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/world_service/formal_read_service.rs#L20-L49)；[crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs L336-L438](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs#L336-L438)；[crates/oasis7_consensus/src/node_pos.rs L99-L169](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_consensus/src/node_pos.rs#L99-L169)。测试定义：[crates/oasis7/src/world_service/formal_read_adapter_tests.rs L11-L74](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/world_service/formal_read_adapter_tests.rs#L11-L74)；[crates/oasis7_proto/src/distributed_finality_tests.rs L211-L340](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_proto/src/distributed_finality_tests.rs#L211-L340)。[相关历史交付/跟踪 #4683](https://github.com/nil-studio/oasis7/pull/4683)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计：BFT lock/unlock、timeout/new-round、anti-double-sign、集合转换和分区恢复的完整算法/接口仍未闭合；受控权威目标设计不代签真实旧写者隔离或独立故障域执行；本轮实现与验证细化：单权威双故障域持久/旧写者隔离、BFT quorum/round、合法 profile 激活与错误 profile/epoch/证书的整条组合未核清；不能汇总局部验证器为完整唯一权威历史。

- 设计复核日期与基线：2026-10-15；规范化候选基于 `dffd797f`；逐项审读 §5.2 及其 P2P/runtime/QA 接收，不改变上述实现源码或运行证据基线。

<a id="sc-infra-002"></a>
### SC-2：成功标准与状态

- SC-2：验证者、full/state-sync、archive、light companion 与公开服务在各自角色内复制、提供和验证状态；任何非权威服务不取得最终性或写入权。

- Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3。
- 引用定位与组合边界：O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39（[REQ-DCS-001](distributed-consensus-and-state-availability.prd.md#req-dcs-001)）；O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3（[REQ-DCS-003](distributed-consensus-and-state-availability.prd.md#req-dcs-003)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
#### 功能状态卡：SC-2

- Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3。
- 引用定位与组合边界：O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39（[REQ-DCS-001](distributed-consensus-and-state-availability.prd.md#req-dcs-001)）；O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3（[REQ-DCS-003](distributed-consensus-and-state-availability.prd.md#req-dcs-003)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
- 系统设计状态：部分覆盖
- 系统设计依据：[角色隔离](../../p2p/design.md#des-p2p-layer-authority)和[复制边界](../../p2p/design.md#des-p2p-replication-scope)明确非权威服务只供材料、不取得投票/写入权；[恢复设计](../../p2p/design.md#des-p2p-target-recovery)明确 pruning 前的可重建、hash/root 与冗余 archive 条件。
- 实现状态：部分实现
- 实现与验证依据：NodeNetworkPolicy 将 runtime role 绑定 ValidatorCore/FullStorage/ObserverLight，拒绝 Observer 冒充 validator/storage；NetworkLane 权限被真实 Sync/BlobState serving handler 注册条件消费。ObserverLight 不允许 ConsensusGossip publish 或 Sync/BlobState serve，full storage 可服务复制材料。 [types.rs L239–322](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_node/src/types.rs#L239-L322)；[distributed_net.rs L78–108](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_proto/src/distributed_net.rs#L78-L108)；[lib.rs L835–850](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_node/src/lib.rs#L835-L850)；[lib.rs L962–985](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_node/src/lib.rs#L962-L985)。本轮仅固定源码审读，测试定义及CI汇总不代签业务通过。
- 核对日期与基线：2026-10-17；需求与源码固定 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，GitHub connector静态审读；未运行本条测试、故障注入、模型、浏览器或部署。旧历史记录保持其原候选/环境，不刷新为本基线实测。
- 剩余范围：设计仍按本卡原系统设计状态与依据保留；实现与验证：这只核到当前 role/lane/handler 隔离，不证明角色认证、防冒充、sentry转发与投票权所有入口闭合。archive 独立角色/冗余 archive 可用性联合 pruning、全角色恢复 serving/voting gate 和真实权限负例仍缺。commit retention 的热窗/checkpoint保留及冷pack界限/长度检查不等于远端冗余归档可重建保证。

- 设计复核日期与基线：2026-10-15；规范化候选基于 `dffd797f`；逐项审读 §5.2 及其 P2P/runtime/QA 接收，不改变上述实现源码或运行证据基线。

<a id="sc-infra-003"></a>
### SC-3：成功标准与状态

- SC-3：bootstrap、snapshot、replay、state sync、pruning 和灾难恢复证明同一历史/状态根可重建；任一证明缺失或不匹配时停止 serving/voting。

- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4（[REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
#### 功能状态卡：SC-3

- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4（[REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
- 系统设计状态：部分覆盖
- 系统设计依据：[P2P 信任链](../../p2p/design.md#des-p2p-target-recovery)与 [runtime 恢复](../../world-runtime/design.md#runtime-recovery-design)已逐环接收身份、激活、checkpoint、snapshot、replay、root 及失败隔离；QA V-RECOVERY 保留不同恢复入口。
- 实现状态：部分实现
- 实现与验证依据：driver_startup_recovery 与V3 record/CAS恢复实现已存在；回归定义覆盖metadata丢失、缓存删除、CAS缺失、冲突同高record、stale pointer及错误world。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/driver_startup_recovery.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/tests/driver_authoritative_recovery.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4695)。本轮为固定源码审读，回归定义不等于实际通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：设计：bootstrap/state-sync/pruning/archive/灾备所有入口的具体材料与恢复编排尚未全核清；现有局部恢复不能替代 BFT/灾备证明及停止 serving/voting 的全组合。 实现与验证仍按本卡独立状态及 §5.1 原测试层级判定；不以设计接收或局部结果代签完整组合。

- 设计复核日期与基线：2026-10-15；规范化候选基于 `dffd797f`；逐项审读 §5.2 及其 P2P/runtime/QA 接收，不改变上述实现源码或运行证据基线。

<a id="sc-infra-004"></a>
### SC-4：成功标准与状态

- SC-4：两种 profile 使用同一版本化确定性执行；BFT 下全部活动验证者在 attestation 前重执行相同输入；执行升级、混合版本和 replay 不会为同一输入产生两个权威结果。

- Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607, O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc。
- 引用定位与组合边界：O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607（[REQ-DWE-001](deterministic-world-execution.prd.md#req-dwe-001)）；O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc（[REQ-DWE-004](deterministic-world-execution.prd.md#req-dwe-004)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
#### 功能状态卡：SC-4

- Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607, O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc。
- 引用定位与组合边界：O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607（[REQ-DWE-001](deterministic-world-execution.prd.md#req-dwe-001)）；O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc（[REQ-DWE-004](deterministic-world-execution.prd.md#req-dwe-004)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
- 系统设计状态：部分覆盖
- 系统设计依据：[确定性与证明绑定](../../world-runtime/design.md#runtime-deterministic-design)、[版本激活与历史重放](../../world-runtime/design.md#runtime-version-design)及 [P2P 重执行](../../p2p/design.md#des-p2p-receipt-finality)分别承接相同输入、全活动验证者和版本拒绝边界。
- 实现状态：部分实现
- 实现与验证依据：execution driver 以 committed context 执行并校验/保存 execution state root；determinism 回归比较不同node provenance的两driver根和执行hash。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/tests/determinism.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4695)。本轮为固定源码审读，回归定义不等于实际通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：设计：完整 activation ledger、工件/readiness/rollback 及两 profile 的跨验证者组合仍未闭合；已有本地 release lane 不能代替治理激活历史。 实现与验证仍按本卡独立状态及 §5.1 原测试层级判定；不以设计接收或局部结果代签完整组合。

- 设计复核日期与基线：2026-10-15；规范化候选基于 `dffd797f`；逐项审读 §5.2 及其 P2P/runtime/QA 接收，不改变上述实现源码或运行证据基线。

<a id="sc-infra-005"></a>
### SC-5：成功标准与状态

- SC-5：游戏、Agent 与玩家入口经同一版本化协议只处理 committed state；finality 不可用时，待决 intent 不产生下游资格或世界效果，消费者能区分仍待决与须重新规划的结果，并只在恢复后取得的 committed receipt 上更新结论。明确互斥的同 lineage 成员中，首个产生有效世界效果的 receipt 唯一获胜并原子终止其余成员；拒绝/过期只终止自身，独立 intent 保持并发；原请求与撤回/替代请求的关联及各自真实状态可读。

- Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405, O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6。
- 引用定位与组合边界：O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405（[REQ-DWE-002](deterministic-world-execution.prd.md#req-dwe-002)）；O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6（[REQ-DWE-003](deterministic-world-execution.prd.md#req-dwe-003)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
#### 功能状态卡：SC-5

- Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405, O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6。
- 引用定位与组合边界：O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405（[REQ-DWE-002](deterministic-world-execution.prd.md#req-dwe-002)）；O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6（[REQ-DWE-003](deterministic-world-execution.prd.md#req-dwe-003)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
- 系统设计状态：部分覆盖
- 系统设计依据：[pending](../../world-runtime/design.md#runtime-pending-design)与 [lineage 原子处置](../../world-runtime/design.md#runtime-lineage-design)已有重审、唯一有效胜者、原子终止其他成员、拒绝/过期只终止自身和独立并发条款；[原决定接收](../../world-runtime/design.md#runtime-controlled-candidate-recovery-receiver)区分未知提交与已知无效果。
- 实现状态：部分实现
- 实现与验证依据：Agent chat 持久状态、显式替代关联、精确 replay 与 provider 回应不自动变成世界效果已有局部实现/回归定义。 [crates/oasis7/src/runtime/world/agent_intent.rs L382-L580](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/agent_intent.rs#L382-L580)。测试定义：[crates/oasis7/src/runtime/tests/agent_intent_v2/lifecycle.rs L110-L605](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/agent_intent_v2/lifecycle.rs#L110-L605)。[相关历史交付/跟踪 #3612](https://github.com/nil-studio/oasis7/pull/3612)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计：通用 pending 持久字段、domain 互斥标记/排序及 consumer 状态 DTO 尚待批准；跨读面原请求/替代关联与所有恢复竞态的实际执行未证明；本轮实现与验证细化：同一版本化协议的游戏/Agent/玩家 committed-only 消费、通用 finality pending 重审、有效 receipt 唯一胜者和独立 intent 并发的组合未核清，不能用 chat 局部代签。

- 设计复核日期与基线：2026-10-15；规范化候选基于 `dffd797f`；逐项审读 §5.2 及其 P2P/runtime/QA 接收，不改变上述实现源码或运行证据基线。

<a id="sc-infra-006"></a>
### SC-6：成功标准与状态

- SC-6：恢复路径只在同一 `world_id` 的 checkpoint/snapshot、canonical replay 与 state root 验证链成立时恢复世界结论；缺失、冲突或其他 `world_id` 的候选保持原世界不可用和隔离，不接受新权威 intent，也不把本地/测试/替代世界、缓存或部分数据表述为连续恢复。已 committed receipt 保留其原历史，未 final intent 不被静默结算、取消、重放或迁移；任何替代世界或受治理迁移必须作为独立产品决策，而非恢复副作用。

- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4（[REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
#### 功能状态卡：SC-6

- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4（[REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
- 系统设计状态：部分覆盖
- 系统设计依据：[runtime 同世界恢复](../../world-runtime/design.md#runtime-recovery-design)和 [P2P 恢复信任链](../../p2p/design.md#des-p2p-target-recovery)明确错误/缺失/冲突世界隔离、保留历史、不以替代 endpoint 或本地世界冒充恢复。
- 实现状态：部分实现
- 实现与验证依据：driver_startup_recovery 与V3 record/CAS恢复实现已存在；回归定义覆盖metadata丢失、缓存删除、CAS缺失、冲突同高record、stale pointer及错误world。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/driver_startup_recovery.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/tests/driver_authoritative_recovery.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4695)。本轮为固定源码审读，回归定义不等于实际通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：设计：全部恢复入口与消费者的 unavailable/isolation 表达、pending/receipt 连续性 DTO 仍未全闭合；Viewer 的恢复表述及完整同世界组合仍需各自运行证据。 实现与验证仍按本卡独立状态及 §5.1 原测试层级判定；不以设计接收或局部结果代签完整组合。

- 设计复核日期与基线：2026-10-15；规范化候选基于 `dffd797f`；逐项审读 §5.2 及其 P2P/runtime/QA 接收，不改变上述实现源码或运行证据基线。

<a id="sc-infra-007"></a>
### SC-7：成功标准与状态

- SC-7：同一候选的恢复演练至少各覆盖一例“恢复只读”“恢复可服务”和“恢复受阻/隔离”，并覆盖一次闸门在提交或恢复中回退；同一 `world_id` 的历史验证链是必要但不足以重新接受写入，追加/最终性/版本化执行或 head 连续性未重新成立时，所有新 intent 尝试均原子拒绝或保持无效果待决（每个受测新 intent 的 committed receipt 数为 `0`）。进入可服务后，既有 pending 按当前条件和 canonical 顺序重新裁决；停机排队与历史 receipt 不延长期限、不继承优先级；每个被接受的新 intent 最多产生一个 committed receipt，且不产生第二次效果。还须通过 [AC-DCS-002](distributed-consensus-and-state-availability.prd.md#ac-dcs-002) 的正向续跑对照，分别证明新请求不与历史任务混淆、模块行动时点与结算延续，并至少完成一条合法新配方；仅“至多一次”不能判恢复可服务通过。闸门再次失效时回到只读/隔离且不撤销、重放或改写已确认 receipt。玩家/Agent 能读到当前服务语义、主要 blocker 与下一步。测试层级：`test_tier_full`。

- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4, O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b。
- 引用定位与组合边界：O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4（[REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)）；O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b（[REQ-DCS-005](distributed-consensus-and-state-availability.prd.md#req-dcs-005)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
#### 功能状态卡：SC-7

- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4, O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b。
- 引用定位与组合边界：O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4（[REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)）；O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b（[REQ-DCS-005](distributed-consensus-and-state-availability.prd.md#req-dcs-005)）。组合范围仍以本 SC 原文为准，不能由一项叶子完成代表全部义务。
- 系统设计状态：部分覆盖
- 系统设计依据：[P2P 服务闸门](../../p2p/design.md#des-p2p-target-service-gate)、[runtime 恢复](../../world-runtime/design.md#runtime-recovery-design)与[日志后缀独立续跑](../../world-runtime/design.md#runtime-journal-suffix-recovery-design)分别承接只读/可服务/隔离、闸门回退、receipt 0/1 和非空历史后正向对照。
- 实现状态：部分实现
- 实现与验证依据：recovery predecessor 从不变 origin 顺序 replay 到精确 checkpoint，越过目标或缺历史拒绝；正式发布另外校验 canonical cut 与 manifest。 [crates/oasis7/src/world_service/formal_recovery_lineage.rs L12-L33](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/world_service/formal_recovery_lineage.rs#L12-L33)；[crates/oasis7/src/world_service/formal_read_service.rs L20-L49](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/world_service/formal_read_service.rs#L20-L49)。测试定义：[crates/oasis7/src/world_service/formal_recovery_lineage_tests.rs](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/world_service/formal_recovery_lineage_tests.rs)。[相关历史交付/跟踪 #4838](https://github.com/nil-studio/oasis7/issues/4838)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。 本轮追加 formal-live 子范围：formal HTTP 准入在 ready=false 时于入队前拒绝，degraded 仅保留最后 qualified CommitRef 的精确只读；writer 经正式 proof 校验后才推进并发布 cut。 [准入与只读](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/formal_live_service.rs#L594-L634)、[准入](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/formal_live_service.rs#L784-L861)、[提交发布](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/formal_writer.rs#L965-L1081)。 [C06 定义](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/formal_live_protected_availability_tests.rs#L38-L226)覆盖副本失联、原 GameTaskCreate 恢复及重试同结果；测试 ignored、本轮未运行，不是 full 或合法新配方通过；本轮未核清这些实现的精确交付 PR/Task，[#4838](https://github.com/nil-studio/oasis7/issues/4838) 仅承接剩余恢复验收。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。 本轮 formal-live 补查基线 `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5`，仅源码与测试定义，未新增运行证据。
- 剩余范围：设计：DC-5 状态转换、manifest/head 负例、逐 intent receipt 与 consumer blocker/下一步的结构化 attachment/schema 仍待 owner 批准；正向新请求和模块行动续跑、合法新配方以及同候选/同窗口组合未执行；本轮实现与验证细化：恢复只读不等于重新可写；四状态及回退、当前条件重审、非空 tail 后 action allocator/module schedule 与不中断对照及合法新配方完成仍缺同候选证明，J1/J2 由现有 #4838 承接。 本轮进一步限定：四级转换与证明冲突隔离、全部非 serving 等级及跨入口逐 intent receipt=0、恢复 pending 当前条件/期限/优先级重审、新请求与模块行动不中断对照和合法新配方、三消费者 blocker/next-step 与 DC-5 同候选结构化附件及 test_tier_full 执行仍未核实。ready/503、旧 cut 可读或 C06 定义均不代签完整合同。

- 设计复核日期与基线：2026-10-15；规范化候选基于 `dffd797f`；逐项审读 §5.2 及其 P2P/runtime/QA 接收，不改变上述实现源码或运行证据基线。

<a id="sc-infra-008"></a>
### SC-8：成功标准与状态

- SC-8：同一条代表性 `factory → recipe/input join → raw-material handoff → logistics/transit → output or terminal` 流水线在同一 `world_id`、版本化执行规则和 immutable root 下完成可追溯的 accepted/blocked/terminal 结果；任何 child effect 在缺失或冲突 root、revision、parent 或目的状态时于首个不可逆 sink 前 fail closed。已接受意图不得被表述为已获得容量或原料预留；每个 hold/reservation 必须有界、独占并绑定 root/阶段/边/批次，且每条中间边和目的地 buffer 满载时只能保留未消费投入、接收仍有容量的已结算产出，或原子拒绝/延期新的承诺，不得丢弃、瞬移、无限堆积、自动改道或伪造完成。同一权威释放/到达事件对同一 hold 的释放与其 fresh snapshot 重评各最多产生一次效果；后续不同事件只能从新的权威快照重评仍有效意图的未满足部分。改变产出因果的工厂、配方、输入/输出、物流边或终端条件必须产生 parent-linked revision/child root；retry、reconnect、restore 与 replay 只能重读同一 disposition，不产生第二次 sink、credit、进度、receipt 或奖励。玩家/Agent 可读最早 blocker、仍占用/已消费投入、未满足/剩余量与下一次复查边界，但不能把局部完成、在途或本地缓存当作交付/生产完成。测试层级：`test_tier_full`。

- Trace ID: O7-REQ-05c84021-ee48-41ef-b94a-8aa9ceb8048c
- 声明范围：仅本 SC 的基础设施独立义务；玩法配方、容量、W/window 与消费者体验继续由原文引用的产品/专业 authority 拥有。
#### 功能状态卡：SC-8

- Trace refs: O7-REQ-05c84021-ee48-41ef-b94a-8aa9ceb8048c；[SC-8 唯一声明](#sc-infra-008)。
- 系统设计状态：部分覆盖
- 系统设计依据：[world-runtime 相关承接](../../world-runtime/design.md#runtime-industrial-design)；工业因果、pre-sink校验、有界hold/背压及一次处置已有runtime条款与同Trace五/六列表；通用root/join/window/bundle字段及全消费者义务未闭合。
- 实现状态：部分实现
- 实现与验证依据：world_service_execution保留已认证配方提交的consensus/action/payload/主体来源；回归定义验证来源关联、重复输入与保存恢复。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/world_service_execution.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/tests/world_service.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4695)。本轮为固定源码审读，回归定义不等于实际通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：设计仍按上述准确承接及其未覆盖边界；实现与验证方面，此为配方来源与重复请求子范围，非整条工业流水线；root/revision/parent、全边容量/hold、fan-in/out、终端、消费者数量和完整同候选full证据仍缺。

<a id="sc-infra-009"></a>
### SC-9：成功标准与状态

- SC-9：同一代表性流水线必须通过一组**成对的 outage injection** 与恢复证据，分别覆盖：

- Trace ID: O7-REQ-4792541c-aaa4-4ac1-9a2d-374df44eba6a
- 声明范围：仅本 SC 的基础设施独立义务；玩法配方、容量、W/window 与消费者体验继续由原文引用的产品/专业 authority 拥有。
#### 功能状态卡：SC-9

- Trace refs: O7-REQ-4792541c-aaa4-4ac1-9a2d-374df44eba6a；[SC-9 唯一声明](#sc-infra-009)。
- 系统设计状态：部分覆盖
- 系统设计依据：[world-runtime 相关承接](../../world-runtime/design.md#runtime-industrial-outage-design)；八格outage的独立失败边界和验收分层已有runtime精确承接；工业通用字段、consumer DTO及同候选证据合同仍待闭合。
- 实现状态：未核实
- 实现与验证依据：已读真实 execution driver 等高原记录核对/连续高度、RecipeCompleted pending承诺与completion receipt、MaterialTransitCompleted 数量/路径/settled防重，以及Viewer失败处置字段。它们分别属于普通提交、任务和物流；尚未映射成SC-9任一完整成对outage cell。 [driver.rs L344–438](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs#L344-L438)；[recipe_lifecycle.rs L369–577](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/state/industry_transition/recipe_lifecycle.rs#L369-L577)；[material_transit.rs L151–350](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/state/industry_transition/material_transit.rs#L151-L350)；[gameplay_snapshot.rs L340–358](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/viewer/runtime_live/gameplay_snapshot.rs#L340-L358)。本轮仅固定源码审读，测试定义及CI汇总不代签业务通过。
- 核对日期与基线：2026-10-17；需求与源码固定 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，GitHub connector静态审读；未运行本条测试、故障注入、模型、浏览器或部署。旧历史记录保持其原候选/环境，不刷新为本基线实测。
- 剩余范围：设计仍按本卡原系统设计状态与依据保留；实现与验证：stage_finish：普通recipe job/receipt尚未绑定该矩阵root/revision/child/window/W disposition；transit：已有物流pending/quantity/receipt但未闭合authority outage与read outage的成对保留/恢复；buffer_admission：未定位该矩阵的物理buffer准入及容量hold producer；terminal_settlement：未定位terminal目的、最终settlement与progression/W producer。八格同baseline、Agent/Viewer/API同义字段、active-provider required和真实provider/headed full各自仍缺。

  - **权威 finality/append/execution/industrial-service outage：** 在 committed finality 前注入时不得产生 root、hold、sink、WIP、物流 credit、产出、资格或 `W` 进度；在 root 已提交但下一 child effect 尚未形成时注入，保留既有 root、window/lease lineage、已消费/仍占用投入、WIP、在途/到达数量与 receipts，不产生第二次 child effect、delivery credit、隐式续租、优先级刷新或奖励。恢复必须从 fresh authoritative snapshot 重新校验，并最多沿同一 root 形成一次 continuation/hold/defer/reject/expire/compensation disposition；若 gameplay authority 为当前未完成 candidate 形成 canonical interruption disposition，只允许该 disposition 将 `W` reset 一次，历史里程碑与既有 receipts 不变，因果变化才建立 linked revision/child root。
  - **非权威 Viewer/API/hosting/read-surface outage：** 在 canonical service 可继续或已提交结果可读取的样例中注入读面不可用；注入本身不得改变 world state、root、投入/数量、window/lease lineage、`W`、receipts、历史里程碑或 `progression_effect`。Viewer、Agent 与 pure API 先表达 `stale/unknown`，重连后从 authoritative snapshot/journal 只 reconcile 真实 committed 结果，不补发、回滚、续租、重置 `W` 或生成第二次 sink/credit/reward；若权威执行在读面不可用期间完成，三类读面必须一致呈现其真实状态而不伪造 outage 处置。
  - **跨读面一致性：** 两类 injection 的每个样例都必须让 Agent、Viewer、pure API 对 `state_class`/primary blocker、root/window/lease lineage、held/consumed/unmet/residual 数量、receipt identity 与单次效果、`progression_effect`（`W` reset 一次或保持不变、历史里程碑保留）以及 `next_action`/`next_recheck` 给出同义结果；不得把 stale/unknown、accepted ack、局部完成或本地恢复表达为生产/交付完成。测试层级：`test_tier_full`。
  - **确定性成对注入矩阵：** 以上两类 outage 必须在同一 baseline 下分别注入 `stage_finish`、`transit`、`buffer_admission`、`terminal_settlement` 四个边界；baseline 至少固定同一 `world_id`、root、revision、child、stage、edge、batch、requested/committed/executed/held/consumed/unmet/residual quantities、canonical state bucket、window identity/lease lineage、receipts、`W`、`progression_effect`、`next_action` 与 `next_recheck`。同一有序输入只允许一次 fresh-snapshot revalidation、一次 canonical disposition 和一次效果，并由 receipt identity/idempotency 约束：authority outage 服从 no-new-child-effect/一次性 `W` disposition/continuation-or-termination，read-surface outage 服从 stale/unknown/reconcile 且不改 world/W；retry、reconnect、restore、replay、重复 arrival/submit 不得复制 sink、credit、receipt、hold release、`W`、progression 或奖励。
  - **验证层级（目标态验收，不宣称测试已存在或已通过）：** `test_tier_required` 覆盖上述四边界的 deterministic protocol matrix，并在 active-LLM/provider-backed lane 验证 pure API 与同一权威结果的 parity；`test_tier_full` 在此基础上增加真实本地栈/真实 provider 上的 external headed browser S6/Playwright desktop+narrow 截图与 console 证据，以及 provider-backed Agent parity，记录 `agent_decision_source`、`agent_provider_backend`、`agent_provider_contract`、`agent_provider_transport`。`provider_local_mock` 只用于 deterministic plumbing/fixture 预检，不能替代 active LLM、真实 provider、headed S6/Playwright 或 provider-backed Agent parity；本条不扩展 UI 布局、算法、数值平衡或当前 readiness claim。

<a id="sc-infra-010"></a>
### SC-10：成功标准与状态

- SC-10：玩家、Agent 与入口在提交动作前能区分目标 `world_id` 属于 global-authoritative 世界还是隔离的 local/development 世界；intent、committed receipt 与其世界结果始终绑定同一身份。来自 local/development 世界的 receipt 不能证明 global 世界发生了效果；身份缺失或跨世界不匹配时，不接受也不呈现为 global effect。surface-specific scope 表达由对应专业 authority 定义。测试层级：`test_tier_required`。

- Trace ID: O7-REQ-2b7cfef9-d917-46ea-a3a9-e3a4fb96d730
- 声明范围：仅本 SC 的基础设施独立义务；玩法配方、容量、W/window 与消费者体验继续由原文引用的产品/专业 authority 拥有。
#### 功能状态卡：SC-10

- Trace refs: O7-REQ-2b7cfef9-d917-46ea-a3a9-e3a4fb96d730；[SC-10 唯一声明](#sc-infra-010)。
- 系统设计状态：部分覆盖
- 系统设计依据：[world-runtime 相关承接](../../world-runtime/design.md#runtime-world-scope-design)；runtime-world-scope-design和同Trace承接行覆盖target identity、跨world拒绝及receipt/replay边界；全部消费者scope表达仍须逐项核对。
- 实现状态：部分实现
- 实现与验证依据：driver按context/record world_id拒绝跨world恢复；other-world restore回归给出具体负例。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/tests/driver_authoritative_recovery.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4695)。本轮为固定源码审读，回归定义不等于实际通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：设计仍按上述准确承接及其未覆盖边界；实现与验证方面，仅执行桥恢复入口；全部Agent/Viewer/API提交前global/local区分、缺身份、receipt reuse、replay及可见表达未完整核清。


### 既有生产模块发布承诺的精确接收

以下子条款接收已经由 [SR2 专业 PRD](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-release-acceptance) 的 SC/AC/NFR 与 DEC-OMR-001..004 采纳的独立结果；这是旧承诺的产品定位与唯一身份登记，不是新增功能、重定参数或宣布可用。专业原文继续拥有全部字段、算法、阈值、条件、两条原 NFR-OMR-6、实施与验收细节；正文中的原数值只定位既有目标，冲突须交原专业 owner，不能由此副本更改参数。本根原 SC-1..10、现有 DWE-004 身份及其版本/激活语义保持不变。生产发布证书不因此取得完整 BFT 或任一正式 profile 的提交资格；任意合规节点触发也不是 permissionless 激活保证。专业声明/验收使用这里同一身份的 refs，不另发同义身份。

<a id="req-infra-release-source"></a>
#### REQ-INFRA-RELEASE-SOURCE：生产模块来源受治理清单约束

Trace ID: O7-REQ-7a3866ed-cd66-48ea-9cc7-bb793575c957

生产模块加载以治理确认的线上发布清单为主真源，不以仓库内置或任意本地构建结果替代。应急 bootstrap 的适用条件、默认关闭及生产 fallback 限制保持原专业合同。

- 来源与独立验收：[SC-3、AC-1、PRD-WORLD_RUNTIME-016、DEC-OMR-001](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-source-and-trust)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-1](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-1)。
- 系统设计状态：部分覆盖
- 系统设计依据：[生产模块来源受治理清单约束接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：部分实现
- 实现与验证依据：builtin loader 优先读取 online manifest hashes，生产 fallback policy 关闭时缺条目拒绝；但进入 host materializer 后仍按 hash 尝试 fetch 与本地 compile。 [crates/oasis7/src/runtime/world/release_manifest.rs L148-L259](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/release_manifest.rs#L148-L259)；[crates/oasis7_wasm_materializer/src/lib.rs L78-L130](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_wasm_materializer/src/lib.rs#L78-L130)。测试定义：[crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs L706-L1065](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs#L706-L1065)。[相关历史交付/跟踪 #4331](https://github.com/nil-studio/oasis7/pull/4331)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计仅为已有目标与有限接收；本轮实现与验证细化：生产来源约束不能只由外层 action gate 证明；materializer 的 policy/identity/receipt 输入与生产 fallback 限制尚须闭合，同候选 [AC-1](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-1) 未取得运行证据。

<a id="req-infra-release-identity"></a>
#### REQ-INFRA-RELEASE-IDENTITY：生产模块身份与签名合法性

Trace ID: O7-REQ-d01cf80e-1172-4963-8bc0-877dc9de8e00

生产节点加载前校验清单、artifact 签名及 signer 信任根；任一失败拒绝执行，禁止生产宽松身份回退，未签名或伪签名模块误接纳率保持原目标 0。

- 来源与独立验收：[SC-1、AC-2/3、NFR-OMR-4、DEC-OMR-002](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-source-and-trust)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-2](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-2)、[AC-3](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-3)。
- 系统设计状态：部分覆盖
- 系统设计依据：[生产模块身份与签名合法性接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：部分实现
- 实现与验证依据：manifest identity 解析在禁止 fallback 时缺身份拒绝；release shadow 对缺失与未签 identity 的回归定义已存在。 [crates/oasis7/src/runtime/world/release_manifest.rs L148-L259](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/release_manifest.rs#L148-L259)；[crates/oasis7_wasm_materializer/src/lib.rs L78-L130](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_wasm_materializer/src/lib.rs#L78-L130)。测试定义：[crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs L4-L95](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs#L4-L95)。[相关历史交付/跟踪 #4331](https://github.com/nil-studio/oasis7/pull/4331)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计仅为已有目标与有限接收；本轮实现与验证细化：加载链从 manifest 到 artifact signature、signer trust root 的全部生产入口仍须完整核对；materializer 不直接接收 identity/policy，误接纳率0不能由负例定义代签。

<a id="req-infra-release-certificate"></a>
#### REQ-INFRA-RELEASE-CERTIFICATE：发布批准必须来自合法外部证书

Trace ID: O7-REQ-1440a1a4-3240-4644-8ada-bf9a34771465

原 SR2 生产安装、升级、回滚及发布应用不得本地自签或派生 finality 后自动 apply；只有满足原 epoch 快照、stake 与独立 signer 阈值条件的外部证书才能进入其授权应用路径。

- 来源与独立验收：[SC-2、AC-4/8/9、PRD-WORLD_RUNTIME-017、DEC-OMR-003](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-source-and-trust)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-4](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-4)、[AC-8](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-8)、[AC-9](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-9)。
- 系统设计状态：部分覆盖
- 系统设计依据：[发布批准必须来自合法外部证书接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：部分实现
- 实现与验证依据：governance certificate 校验 proposal/manifest/height/epoch/set hash/stake root/阈值、成员与 Ed25519 签名；生产 install/upgrade/rollback/apply 的缺 finality 拒绝与合法证书路径有回归定义。 [crates/oasis7/src/runtime/world/governance_internal.rs L420-L590](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/governance_internal.rs#L420-L590)；[crates/oasis7/src/runtime/world/module_actions/release_support.rs L251-L435](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/module_actions/release_support.rs#L251-L435)。测试定义：[crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs L706-L1065](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs#L706-L1065)。[相关历史交付/跟踪 #4331](https://github.com/nil-studio/oasis7/pull/4331)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计仅为已有目标与有限接收；本轮实现与验证细化：完整外部 certificate 供给、所有生产应用路径不可本地派生自签、同候选外部信任材料与 [AC-4](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-4)/[AC-8](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-8)/[AC-9](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-9) 实际运行仍待核清；当前 prototype 不等于正式 BFT。

<a id="req-infra-release-attestation-authorization"></a>
#### REQ-INFRA-RELEASE-ATTESTATION-AUTHORIZATION：发布证明聚合满足当前授权门槛

Trace ID: O7-REQ-b89862c6-6316-4511-afdc-1e2d1e9a5e8e

ModuleReleaseApply 必须按当前 epoch 快照 signer 集聚合 attestation，仅快照内 signer 计入原阈值；聚合不足不得激活 release manifest。此证明与外部 finality certificate 各为独立必要条件，不互相替代。

- 来源与独立验收：[AC-14](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-node-release)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-14](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-14)。
- 系统设计状态：部分覆盖
- 系统设计依据：[发布证明聚合满足当前授权门槛接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-node-evidence-receiver)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：部分实现
- 实现与验证依据：apply 按 current governance epoch snapshot 筛选 signer、集合去重、累计 stake，同时检查 min_unique_signers 与 threshold_bps；不足会 ActionRejected，随后才进入安装路径。 [crates/oasis7/src/runtime/world/module_actions/release_support.rs L251-L435](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/module_actions/release_support.rs#L251-L435)。测试定义：[crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs L706-L1065](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs#L706-L1065)。[相关历史交付/跟踪 #4331](https://github.com/nil-studio/oasis7/pull/4331)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计仅为已有目标与有限接收；本轮实现与验证细化：需要同候选对当前/过期/非成员 signer、阈值不足不激活，以及 attestation 与外部 finality 两个独立必要条件的组合运行；现有权重测试定义不代替整组 [AC-14](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-14)。

<a id="req-infra-release-release-triage"></a>
#### REQ-INFRA-RELEASE-RELEASE-TRIAGE：发布阻断故障可分诊

Trace ID: O7-REQ-a5a2d539-b2c8-42dd-83e0-a6108b5ab288

发布运行手册必须定义且可执行证明冲突、attestation 阈值不足、manifest 不可达/回滚/漂移三类阻断场景的分诊；保留原故障信号与主 CI 仅回归/对账的边界，不以 submit API 错误响应替代运行分诊。

- 来源与独立验收：[AC-15、Technical Specifications 原 Edge Cases / S11](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-node-release)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-15](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-15)。
- 系统设计状态：部分覆盖
- 系统设计依据：[发布阻断故障可分诊接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-node-evidence-receiver)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：部分实现
- 实现与验证依据：S11 有证明冲突、阈值不足、manifest 不可达/回滚/漂移的信号、阻断与处置；对应 runtime fault_signature/threshold 拒绝和 acceptance 脚本存在。 [testing-manual.md L1199-L1277](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/testing-manual.md#L1199-L1277)；[crates/oasis7/src/runtime/world/release_manifest.rs L148-L259](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/release_manifest.rs#L148-L259)；[crates/oasis7/src/runtime/world/module_actions/release_support.rs L251-L435](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/module_actions/release_support.rs#L251-L435)；[scripts/module-release-node-acceptance.sh](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/module-release-node-acceptance.sh)。测试定义：[crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs L706-L1065](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs#L706-L1065)。[相关历史交付/跟踪 #4102](https://github.com/nil-studio/oasis7/issues/4102)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计仅为已有目标与有限接收；本轮实现与验证细化：尚需同候选实际三类故障注入/原始日志及可执行分诊结果；S11 阈值示例仍用 threshold/aggregated_signers 旧形态，当前代码另含 min_unique_signers/threshold_bps，须确认检索和值守判定不依赖整句模板。

<a id="req-infra-release-ci"></a>
#### REQ-INFRA-RELEASE-CI：生产发布不依赖 CI

Trace ID: O7-REQ-c9c94faf-43bb-474c-8cf9-7ed1441e5494

生产发布在 CI 不可用时仍可完成，不降低阈值签名与审计约束；主 CI 仅承担开发回归和兼容性检查，不承担生产发布写入、激活或判定权。

- 来源与独立验收：[SC-4/6、AC-6/11、NFR-OMR-5、DEC-OMR-004](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-node-release)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-6](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-6)、[AC-11](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-11)。
- 系统设计状态：部分覆盖
- 系统设计依据：[生产发布不依赖 CI接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-node-evidence-receiver)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：未核实
- 实现与验证依据：[CI外proof组装与调用](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/module-release-node-attestation-flow.sh#L486-L529)经[实际submit脚本](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/submit-module-release-attestation.sh#L76-L99)向旧chain attestation端点POST；[Node handler](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/module_release_attestation_submit_api.rs#L71-L172)校验字段、入共识队列后返回ok/action_id/时间，非committed release receipt。[普通路由](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/status_server_support.rs#L405-L413)存在此入口，但[formal服务路由](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/formal_live_service.rs#L389-L410)未开放该旧端点，guarded亦排除legacy submit。已定位实质CI外组装和工程提交机制，尚未核清其生产发布授权、阈值签名、权威结果与activation消费者，不能据caller或queue应答升为本条部分实现。
- 核对日期与基线：2026-10-17；需求与静态源码固定 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`；原38源及独审新增4源逐文件Git blob核验。本轮未运行发布、模型、部署或产品验收；脚本与测试均为定义审读。
- 剩余范围：继续核清生产发布适用入口对CI外proof/签名的实际授权、阈值验证、审计归档、committed结果及activation消费，并核生产凭据/写入权限确与CI隔离。现有旧chain队列应答不是正式发布回执；脚本原样输出HTTP/业务错误也不能表述为提交成功。synthetic跨host fixture、contents:read声明与CI成功均不代签CI不可用时的完整生产发布。

<a id="req-infra-release-participation"></a>
#### REQ-INFRA-RELEASE-PARTICIPATION：合规节点可触发并提交发布证据

Trace ID: O7-REQ-c1427344-798f-47f1-9dab-790fd3693cf7

任意合规节点可触发现有提案、复构建证明、阈值签名与归档流水，并通过既有 node-side 入口直接提交 attestation 到共识队列，不依赖 CI 或手工修改状态。合规、受信 node identity、计入阈值资格与激活条件仍由原专业合同限定；触发或提交不授予单节点批准权。

- 来源与独立验收：[SC-4/7、AC-5/16、PRD-WORLD_RUNTIME-018](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-node-release)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-5](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-5)、[AC-16](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-16)。
- 系统设计状态：部分覆盖
- 系统设计依据：[合规节点可触发并提交发布证据接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-node-evidence-receiver)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：部分实现
- 实现与验证依据：node-side attestation HTTP handler 做字段归一、RuntimeAction 编码并提交共识队列；本地 flow 支持证据打包与提交。 [crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/module_release_attestation_submit_api.rs L96-L171](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/module_release_attestation_submit_api.rs#L96-L171)；[scripts/module-release-node-attestation-flow.sh](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/module-release-node-attestation-flow.sh)。测试定义：[crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/module_release_attestation_submit_api_tests.rs L65-L270](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/module_release_attestation_submit_api_tests.rs#L65-L270)。[相关历史交付/跟踪 #4820](https://github.com/nil-studio/oasis7/pull/4820)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计仅为已有目标与有限接收；本轮实现与验证细化：合规节点从提案到复构建/阈值签名/归档的完整自动流水、可信身份和计阈资格，以及无 CI/无手改状态的端到端仍未核清；ACK 仅接收。

<a id="req-infra-release-reproducibility"></a>
#### REQ-INFRA-RELEASE-REPRODUCIBILITY：发布构建按平台可复现

Trace ID: O7-REQ-f25099a9-5f30-466e-939a-48ac57cbead2

发布端要求同平台可复现；跨平台差异按原 keyed token 合同表达，不要求跨平台 hash 相等；上线前不可复现构建必须阻断，最终仍以签名清单合法性判断。

- 来源与独立验收：[SC-5、AC-7](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-node-release)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-7](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-7)。
- 系统设计状态：部分覆盖
- 系统设计依据：[发布构建按平台可复现接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-node-evidence-receiver)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：部分实现
- 实现与验证依据：summary verifier 校验同 module 的 build hash 与 manifest hash、identity/receipt 集合和 canonical platform/build recipe；flow 可要求 cross-host-closed。 [scripts/ci-verify-m1-wasm-summaries.py L94-L159](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/ci-verify-m1-wasm-summaries.py#L94-L159)；[scripts/module-release-node-attestation-flow.sh](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/module-release-node-attestation-flow.sh)。测试定义：仅设计/脚本入口，尚未核到足以覆盖本条的独立测试定义。。[相关历史交付/跟踪 #4102](https://github.com/nil-studio/oasis7/issues/4102)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计仅为已有目标与有限接收；本轮实现与验证细化：节点实际同平台重复构建、跨平台 keyed token 适用性、不可复现结果阻断生产上线与最终签名合法性组合未核清；比较现成 summary 不是完成复构建。

<a id="req-infra-release-evidence"></a>
#### REQ-INFRA-RELEASE-EVIDENCE：发布证据完整可追溯与可回放

Trace ID: O7-REQ-80d9506a-b1e5-4ac2-8d68-eb73ecfccd51

提案、证明、清单、签名、证书及链上高度保持原目标 100% 可追溯；proof payload 可归档、可读回、可回放并绑定同一发布身份，冲突证明拒绝且保留原审计证据，迁移期事件到清单映射保持可追溯。

- 来源与独立验收：[SC-7、AC-5/10/13/17、AC-14/15 的证明绑定与审计子范围、NFR-OMR-2](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-node-release)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-5](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-5)、[AC-10](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-10)、[AC-13](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-13)、[AC-14](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-14)、[AC-15](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-15)、[AC-17](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-17)。
- 系统设计状态：部分覆盖
- 系统设计依据：[发布证据完整可追溯与可回放接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-node-evidence-receiver)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：部分实现
- 实现与验证依据：proof 打包脚本按附件 SHA-256 建 canonical payload/proof_cid 与 submit request；runtime 拒绝冲突 attestation 并保留首条，apply 比较证明身份元组。 [scripts/package-module-release-attestation-proof.sh L168-L260](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/scripts/package-module-release-attestation-proof.sh#L168-L260)；[crates/oasis7/src/runtime/world/module_actions/release_request_support.rs L440-L499](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/module_actions/release_request_support.rs#L440-L499)；[crates/oasis7/src/runtime/world/module_actions/release_support.rs L251-L435](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/module_actions/release_support.rs#L251-L435)。测试定义：[crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs L706-L1065](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/module_action_loop_release_controls_tests.rs#L706-L1065)。[相关历史交付/跟踪 #4331](https://github.com/nil-studio/oasis7/pull/4331)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计仅为已有目标与有限接收；本轮实现与验证细化：完整提案/证明/清单/签名/证书/高度的100%链路、归档原件读回和回放、迁移映射与同候选实际运行尚未核清；ACK 和哈希字段不代签外部归档可用性。

<a id="req-infra-release-convergence"></a>
#### REQ-INFRA-RELEASE-CONVERGENCE：信任根版本在既有窗口收敛

Trace ID: O7-REQ-02b10123-7422-4509-9d04-ea43444890c6

信任根变更具有版本与生效高度，所有节点在原约束的至多 2 个治理 epoch 内收敛；不以局部节点更新代替所有节点收敛。

- 来源与独立验收：[NFR-OMR-3、AC-12](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-release-quality)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-12](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-12)。
- 系统设计状态：部分覆盖
- 系统设计依据：[信任根版本在既有窗口收敛接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-validation)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：未核实
- 实现与验证依据：实际Runtime按epoch选择显式snapshot/registry并核对证书epoch/set/stake。现有GovernanceFinalityEpochSnapshot字段为epoch/hash/stake/threshold/signers，尚未把产品trust_root_version/effective_height变更与全节点收敛观察者绑定。 [governance.rs L154–202](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/governance.rs#L154-L202)；[governance.rs L690–718](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/governance.rs#L690-L718)；[governance_internal.rs L420–580](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/governance_internal.rs#L420-L580)；[oasis7-runtime-finality-baseline.sh L251–260](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/oasis7-runtime-finality-baseline.sh#L251-L260)。本轮仅固定源码审读，测试定义及CI汇总不代签业务通过。
- 核对日期与基线：2026-10-17；需求与源码固定 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，GitHub connector静态审读；未运行本条测试、故障注入、模型、浏览器或部署。旧历史记录保持其原候选/环境，不刷新为本基线实测。
- 剩余范围：设计仍按本卡原系统设计状态与依据保留；实现与验证：需定位版本和生效高度的真实producer、所有目标节点的传播/采纳/拒旧路径以及不超过2治理epoch的拓扑/窗口/逐节点原始记录。固定脚本full case只调用单World双signer轮换测试；旧2026-03-24默认world 2-of-3 signer02恢复不能证明全节点收敛。

<a id="req-infra-release-module-latency"></a>
#### REQ-INFRA-RELEASE-MODULE-LATENCY：缓存命中时模块验证耗时

Trace ID: O7-REQ-c2a3ca44-d9b3-4ae0-8245-d8f496f38673

在原本地缓存命中条件下，单模块 manifest、identity 与 signature 校验的 p95 耗时不超过原 200ms 目标；测量范围和方法仍由专业合同定义。

- 来源与独立验收：[NFR-OMR-1、AC-12](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-release-quality)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-12](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-12)。
- 系统设计状态：部分覆盖
- 系统设计依据：[缓存命中时模块验证耗时接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-validation)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：未核实
- 实现与验证依据：shadow_proposal→shadow_validate_module_changes→validate_module_manifest完整读到manifest字段、artifact存在、identity、trusted-key Ed25519验签；load_module的热cache只返回wasm artifact bytes，不执行这组合验证。 [governance.rs L489–511](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/governance.rs#L489-L511)；[base_layer.rs L175–261](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/base_layer.rs#L175-L261)；[base_layer.rs L280–378](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/base_layer.rs#L280-L378)；[module_runtime.rs L536–553](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/module_runtime.rs#L536-L553)。本轮仅固定源码审读，测试定义及CI汇总不代签业务通过。
- 核对日期与基线：2026-10-17；需求与源码固定 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，GitHub connector静态审读；未运行本条测试、故障注入、模型、浏览器或部署。旧历史记录保持其原候选/环境，不刷新为本基线实测。
- 剩余范围：设计仍按本卡原系统设计状态与依据保留；实现与验证：尚需确认专业热缓存口径究竟覆盖哪个生产调用、manifest/identity/signature三者的单工作单元与cache配置，并补准确可执行基准及同候选环境/预热/样本/p95。现有triad_release_sla是20次submit→approve→apply，阈值60,000ms；不是原单模块200ms。不能把load bytes快或普通验签通过作为本性能条款的全部实现。

<a id="req-infra-release-certificate-latency"></a>
#### REQ-INFRA-RELEASE-CERTIFICATE-LATENCY：既有快照规模下证书验证耗时

Trace ID: O7-REQ-8d0fe4a1-7c99-42d2-8b5d-7c4990591993

在原 100 signer 快照、热缓存命中条件下，stake/epoch finality 单证书校验的 p95 耗时不超过原 50ms 目标；不扩展为其他规模或冷缓存保证。

- 来源与独立验收：[NFR-OMR-6（finality latency）、AC-12](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-release-quality)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-12](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-12)。
- 系统设计状态：部分覆盖
- 系统设计依据：[既有快照规模下证书验证耗时接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-validation)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：未核实
- 实现与验证依据：apply_proposal_with_finality→prepare_proposal_with_finality→validate_governance_finality_certificate核对proposal/manifest/height/epoch/set/stake及每signer Ed25519，按stake与独立signer阈值判定；已定位可成为精确测量对象的生产函数。 [governance.rs L650–662](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/governance.rs#L650-L662)；[governance_publication.rs L116–132](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/governance_publication.rs#L116-L132)；[governance_internal.rs L420–580](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/runtime/world/governance_internal.rs#L420-L580)；[oasis7-runtime-finality-baseline.sh L183–205](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/oasis7-runtime-finality-baseline.sh#L183-L205)。本轮仅固定源码审读，测试定义及CI汇总不代签业务通过。
- 核对日期与基线：2026-10-17；需求与源码固定 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，GitHub connector静态审读；未运行本条测试、故障注入、模型、浏览器或部署。旧历史记录保持其原候选/环境，不刷新为本基线实测。
- 剩余范围：设计仍按本卡原系统设计状态与依据保留；实现与验证：[AC-12](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-12)固定基准未按100 signer热缓存单证书构造与计时：现脚本计整个cargo test进程，required分别是stake_root mismatch和signer outside epoch的拒绝测试；summary聚合的是命令耗时，没有单证书50ms门槛判定。需补准确工作单元基准/100 signer配置及同候选原始样本，不以旧2-of-3 drill或任意timing字段代替。

<a id="req-infra-release-diagnostics"></a>
#### REQ-INFRA-RELEASE-DIAGNOSTICS：节点证明提交结果可分诊

Trace ID: O7-REQ-0ed26c5c-b1a1-4e51-a462-56bd4712e295

node-side attestation submit 返回结构化成功或失败响应；无效请求、payload 编码失败、共识队列提交失败三类路径均保留原可分诊错误码要求。成功接收不等于 finalized、applied 或产生世界效果。

- 来源与独立验收：[NFR-OMR-6（submit 响应）、AC-16](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#sr2-product-release-quality)；原 SC/AC/NFR/DEC 正文、正式条件与 test_tier 不变。精确验收定位：[AC-16](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.prd.md#ac-16)。
- 系统设计状态：部分覆盖
- 系统设计依据：[节点证明提交结果可分诊接收](../../world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md#sr2-release-validation)；仅该独立结果，未闭合专业及跨 owner 组合仍按原条款保留。
- 实现状态：部分实现
- 实现与验证依据：handler 已逐支实现 invalid_request(400)、payload encode/internal_error(502)、consensus submit_failed(502)及结构化成功 action_id/submitted_at(200)，覆盖原文列举的三类响应分支，但尚不能证明全部失败均结构化返回。 [crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/module_release_attestation_submit_api.rs L96-L171](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/module_release_attestation_submit_api.rs#L96-L171)。测试定义：[crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/module_release_attestation_submit_api_tests.rs L65-L270](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/module_release_attestation_submit_api_tests.rs#L65-L270)。[相关历史交付/跟踪 #4820](https://github.com/nil-studio/oasis7/pull/4820)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计仅为已有目标与有限接收；本轮实现与验证细化：runtime.lock()失败由map_err后直接?返回String，未走本handler的结构化错误写出；该失败及上层响应链仍须核清，不据此宣称线上故障。现有测试只直接定义归一化、payload编码、bad payload与NoopExecutionHook成功接收；编码失败及队列提交失败的实际故障注入/响应断言和全部原AC运行记录仍缺；不把200解释为finalized/applied。

版本选择、激活证明及原 manifest 历史 replay 继续引用 [REQ-DWE-004](deterministic-world-execution.prd.md#req-dwe-004) 原身份；本节未重发或扩展它。固定 benchmark 入口、100 signer 测量配置、summary 归档及具体 proof/schema 方法仍为专业验证机制，不另造产品功能或已通过结果。

### 5.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| SC-1 | blockchain_ops_engineer / runtime_engineer / qa_engineer | PRD-P2P-001 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `doc/p2p/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 单权威原子追加/独立故障域/未知提交/旧写者拒绝；BFT certificate、转换、分区与 H/H+1 负例 | test_tier_full |
| SC-2 | blockchain_ops_engineer / qa_engineer | PRD-P2P-001 / PRD-P2P-002 / PRD-TESTING-003 | `doc/p2p/prd.md`; `doc/testing/prd.md` | 复制、存储角色、proof-serving 与非权威权限负例 | test_tier_full |
| SC-3 | blockchain_ops_engineer / runtime_engineer / qa_engineer | PRD-P2P-002 / PRD-WORLD_RUNTIME-003 / PRD-TESTING-003 | `doc/p2p/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | bootstrap、checkpoint、snapshot、replay、root verification 与 restore drill | test_tier_full |
| SC-4 | runtime_engineer / blockchain_ops_engineer / qa_engineer | PRD-WORLD_RUNTIME-001 / PRD-P2P-001 / PRD-TESTING-003 | `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | deterministic re-execution、upgrade activation、replay 与 mixed-version 拒绝 | test_tier_full |
| SC-5 | producer_system_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 本文 `Finality 缺失时的意图连续性` 对应的 committed/pending protocol boundary、待决/无效或重规划结果的可区分性、恢复后重新校验与 receipt-gated 结果、proof verification；包含同 lineage 互斥成员的竞态/同序唯一胜者、拒绝/过期仅终止自身、replay/重复重试无第二效果及独立 intent 并发负例，以及正式 surface 的关联/状态可读性核对 | test_tier_required |
| SC-6 | producer_system_designer / blockchain_ops_engineer / runtime_engineer / viewer_engineer / qa_engineer | PRD-P2P-001 / PRD-P2P-002 / PRD-WORLD_RUNTIME-003 / `viewer-control-plane-split-live-playback.prd.md` / PRD-TESTING-003 | `doc/p2p/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md`; `doc/testing/prd.md` | 同一 `world_id` 的 checkpoint/snapshot、replay 与 root 验证恢复样例；缺失、冲突或其他 `world_id` 候选的 fail-closed 负例，断言不接收新权威 intent、不把替代 endpoint/缓存/本地世界表述为恢复，并核对 committed receipt 与未 final intent 的连续性；若触达可见 Viewer 恢复/重连/回放表述，按该 Viewer authority 和 `testing-manual.md` S6 核对 canonical snapshot/feedback、可见状态与 browser console 不把本地回放或重连表述为恢复 | test_tier_full |
| SC-7 | producer_system_designer / blockchain_ops_engineer / runtime_engineer / viewer_engineer / qa_engineer | PRD-P2P-001 / PRD-P2P-002 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-003 / PRD-TESTING-003 | `doc/p2p/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 同一候选至少覆盖恢复只读、恢复可服务、恢复受阻/隔离各一例及一次闸门回退；未满足闸门时新 intent 的原子拒绝/无效果待决（committed receipt 为 0）；满足闸门后的 pending 当前条件重裁决与 canonical 顺序；停机排队不继承期限/优先级；每个新 intent 至多一个 committed receipt、无第二次效果；[正向续跑](distributed-consensus-and-state-availability.prd.md#ac-dcs-002) 分别证明新请求身份与模块行动时点延续、同条件对照结算一致且合法新配方完成一次；回退后的 fail-closed 与已确认 receipt 连续性；正式 surface 可读服务语义、blocker 与下一步 | test_tier_full |
| SC-8 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-WORLD_RUNTIME-043 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/world-runtime/prd.md`; `doc/product/world-rules-core-gameplay/prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | 代表性工厂→配方/input join→原料 handoff→物流/transit→产物/终端路径的 root/revision/parent lineage、缺失/冲突 identity fail-closed、已接受与容量/预留的区分、有界 hold 与 buffer 满载背压、释放后单次重评、因果 cutover、blocked/terminal 读面、单次 sink/credit 与 retry/restore/replay 证据；包含 Agent/Viewer/pure API 对 blocker、投入、未满足/剩余量、复查边界与 terminal disposition 的读取一致，且不把局部完成、在途或缓存伪装为交付/生产完成 | test_tier_full |
| SC-9 | producer_system_designer / gameplay_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-043 / PRD-WORLD_SIMULATOR-047 / PRD-TESTING-003 | `doc/world-runtime/prd.md`; `doc/product/world-rules-core-gameplay/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md`; `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`; `doc/testing/prd.md` | 同一 baseline（world/root/revision/child/stage/edge/batch、quantities、states、window/lease、receipts、W、progression、next action/recheck）在 `stage_finish`/`transit`/`buffer_admission`/`terminal_settlement` 四边界成对注入权威与非权威 outage；required 覆盖 deterministic protocol matrix + active-LLM/provider-backed pure API parity；full 追加真实本地栈/provider 的 external headed S6/Playwright desktop+narrow screenshots/console 与 provider-backed Agent parity，并记录 decision source/backend/contract/transport；各边界 fresh snapshot、canonical disposition、exactly-once/idempotency、root/quantity/state/receipt/W/progression 保持一致，`provider_local_mock` 非替代证据 | test_tier_full |
| SC-10 | producer_system_designer / runtime_engineer / agent_engineer / viewer_engineer / qa_engineer | PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-001 / PRD-TESTING-003 | `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 在 distinct global/local `world_id` 样例中验证 consumer target scope、intent 与 committed receipt identity continuity；跨世界提交、replay 或 receipt reuse 不产生或声称 global effect，scope 缺失/不匹配 fail closed | test_tier_required |

### 5.2 根成功标准的义务分配

下表保留根 SC 的独立判定边界，分配产品义务与专业责任；后续接收表给出已核对的具体设计关系，覆盖程度仍分别由各 SC 状态卡判定。专题叶子与组合验收共同承接相关义务；SC-8、SC-9、SC-10 仍由根条款及跨模块专业合同承担，不能因为两个专题共有九组 REQ/AC 就从验收范围中消失。以下链接仅证明已列条款可定位，未列明的字段、算法、外部消费者与证据合同仍按状态卡保留真实缺口。

| 根 SC | 必须分别判定的义务与前置条件 | 产品承接与专业分工 |
| --- | --- | --- |
| SC-1 | 唯一世界与合法激活 profile；单权威持久性/旧写者隔离，BFT 的集合/阈值/round；错误签名、profile、epoch 或适用证书条件各自不得推进历史 | [REQ-DCS-001](distributed-consensus-and-state-availability.prd.md#req-dcs-001)、[REQ-DCS-004](distributed-consensus-and-state-availability.prd.md#req-dcs-004)及 DC-1/DC-4；P2P 拥有证书、集合和 round 合同，runtime 拥有执行绑定，运维负责拓扑事实，QA 判定组合证据 |
| SC-2 | 各角色的复制、存储、证明提供与验证；非权威服务不得获得最终性或写入权；pruning 须先证明可重建、hash/root 校验及冗余 archive 可用 | [REQ-DCS-001](distributed-consensus-and-state-availability.prd.md#req-dcs-001)、[REQ-DCS-003](distributed-consensus-and-state-availability.prd.md#req-dcs-003)及专题存储边界；P2P 拥有角色与存储合同，运维负责暴露面和隔离，QA 核对权限负例 |
| SC-3 | bootstrap、snapshot、replay、state sync、pruning、灾备各自重建同一历史与根；任何缺失或不匹配证明都停止 serving/voting | [REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)、DC-2 与 DE-3；P2P/运维负责恢复材料与同窗口事实，runtime 负责 replay/root，QA 核对信任链各环及失败结果 |
| SC-4 | 同一确定性执行；BFT 下全部活动验证者先重执行再 attestation；同版本、顺序、父状态得到同结果；升级、混合版本及历史 replay 不产生第二个权威结果 | [REQ-DWE-001](deterministic-world-execution.prd.md#req-dwe-001)、[REQ-DWE-004](deterministic-world-execution.prd.md#req-dwe-004)及 DE-1/DE-3/DE-4；runtime 拥有确定性与版本合同，P2P 拥有 attestation/finality，QA 判定同候选组合 |
| SC-5 | committed-only 消费；待决无资格或效果；恢复按当前条件重审且 receipt 才改变结论；互斥 lineage 首个有效 receipt 唯一胜出并原子终止其余成员；拒绝/过期仅终止自身，独立 intent 可并发；原请求与撤回/替代真实关联可读 | [REQ-DWE-002](deterministic-world-execution.prd.md#req-dwe-002)、[REQ-DWE-003](deterministic-world-execution.prd.md#req-dwe-003)及 DE-2；runtime/P2P 拥有顺序、去重和回执，Agent/Viewer/入口拥有消费与反馈，QA 核对竞态及独立请求负例 |
| SC-6 | 同一世界 checkpoint/snapshot、replay、root 验证链；缺失、冲突、错误世界隔离且不接受新权威 intent；已提交历史保留，未 final 请求不被静默确认、取消、重放或迁移；替代世界不是恢复副作用 | [REQ-DCS-002](distributed-consensus-and-state-availability.prd.md#req-dcs-002)、DE-3 与根恢复身份边界；P2P/runtime/运维负责连续性与隔离，Viewer 负责恢复表述，跨世界迁移须独立产品决策，QA 核对身份负例 |
| SC-7 | 同候选只读、可服务、受阻/隔离与闸门回退；历史验证不足以开放写入，追加/finality/版本/head 同时成立；不成立时 receipt 为 0；成立后 pending 当前条件重审、无期限/优先级继承且至多一次效果；合法检查点加非空事件历史恢复后的新请求与模块行动分别通过不中断对照，合法新配方完成一次；回退不改已确认历史；blocker/下一步可读 | [AC-DCS-002](distributed-consensus-and-state-availability.prd.md#ac-dcs-002)、[AC-DCS-005](distributed-consensus-and-state-availability.prd.md#ac-dcs-005)及 DC-5；P2P/runtime 拥有服务闸门，运维捕获恢复窗口，消费者投影真实等级，QA 核对状态转换、正向续跑及每个 intent 的 receipt |
| SC-8 | 同世界/版本/immutable root 的代表性工业流水线；child identity 在首个不可逆效果前校验；accepted 不等于预留；有界独占 hold 与满载背压；同一事件释放与 fresh snapshot 重评各一次；因果变化 linked revision/child；retry/restore/replay 无第二效果；数量、最早 blocker、复查边界可读 | 根工业跨域边界及 [runtime 工业权威矩阵](../../world-runtime/prd.md#industrial-execution-status-and-authority-matrix)、[工业资源流合同](../../world-simulator/m4/industrial-resource-flow-contract.prd.md)；runtime 拥有跨阶段因果与单次效果，玩法 authority 拥有资源/容量规则，Agent/Viewer/API 拥有投影，QA 判定整条流水线 |
| SC-9 | 四个边界分别成对注入权威与非权威 outage；提交前无效果，提交后保留实际数量与 lineage；恢复 fresh snapshot 重评与 disposition 各一次；只有 gameplay canonical interruption 可将当前未完成 candidate 的 W reset 一次；读面仅 stale/unknown/reconcile 且不改世界/W；Agent/Viewer/API 同义；固定 baseline、真实 provider 与 required/full 环境限制全部保留 | 根工业 outage 边界、[工业资源流合同](../../world-simulator/m4/industrial-resource-flow-contract.prd.md)及 [gameplay 稳定产线合同](../../game/gameplay/gameplay-top-level-design.prd.md)；runtime 判定权威依赖与恢复效果，gameplay 拥有 W/window 处置，消费者拥有跨入口一致性，QA 拥有四边界成对矩阵与真实环境证据 |
| SC-10 | 提交前识别 global 与隔离 local 的目标身份；intent/receipt/result 同世界；local receipt 不证明 global effect；身份缺失或不匹配不得接受或呈现为 global effect | 根世界身份边界及 [消费者专业 PRD](../../world-simulator/prd.md)；runtime 拥有身份校验，Agent/Viewer/API 各自拥有 scope 表达，QA 核对跨世界提交、replay 与 receipt reuse 负例 |

表中的分配不改变 §5 成功标准的测试层级。专题局部检查不能替代根 SC 的同候选组合证据；SC-9 的 `test_tier_required` 确定性矩阵与真实 provider API parity、以及 `test_tier_full` 增加的真实浏览器和 provider-backed Agent parity 是分别适用的义务，不能互相代签。

以下回链补齐 §5.1/§5.2 的专业设计与验证接收关系，不改写各 SC 的独立义务或测试层级。QA 场景名称指向同一表中的准确行，不是新增测试或执行结果；每行同时使用 [`QA 方法`](../../testing/design.md#qa-infrastructure-evidence-method) 与 [`环境与 verdict`](../../testing/prd.md#qa-infrastructure-tier-boundaries) 固定 source/candidate/world/window、原始 artifact、domain oracle 与判定边界。

| SC | 已有专业设计接收条款 | 实际 QA 接收场景 |
| --- | --- | --- |
| SC-1 | [`profile/持久提交/交接设计`](../../p2p/design.md#des-p2p-authority-profiles)、[`P2P 设计`](../../p2p/design.md#des-p2p-target-bft)、[`runtime 设计`](../../world-runtime/design.md#runtime-deterministic-design) | [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-FINALITY` |
| SC-2 | [`P2P 设计`](../../p2p/design.md#des-p2p-layer-authority)、[`P2P 设计`](../../p2p/design.md#des-p2p-replication-scope) | [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-FINALITY` |
| SC-3 | [`P2P 设计`](../../p2p/design.md#des-p2p-target-recovery)、[`runtime 设计`](../../world-runtime/design.md#runtime-recovery-design) | [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-RECOVERY` |
| SC-4 | [`runtime 设计`](../../world-runtime/design.md#runtime-deterministic-design)、[`runtime 设计`](../../world-runtime/design.md#runtime-version-design) | [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-EXECUTION` |
| SC-5 | [`runtime 设计`](../../world-runtime/design.md#runtime-pending-design)、[`runtime 设计`](../../world-runtime/design.md#runtime-lineage-design) | [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-EXECUTION / V-CONSUMER` |
| SC-6 | [`runtime 设计`](../../world-runtime/design.md#runtime-recovery-design)、[`P2P 设计`](../../p2p/design.md#des-p2p-target-recovery) | [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-RECOVERY` |
| SC-7 | [`P2P 设计`](../../p2p/design.md#des-p2p-target-service-gate)、[`runtime 设计`](../../world-runtime/design.md#runtime-recovery-design)、[`日志恢复与续跑设计`](../../world-runtime/design.md#runtime-journal-suffix-recovery-design)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-semantic-validator) | [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-RECOVERY / V-CONSUMER`；[正向续跑独立判定](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-journal-suffix-recovery) |
| SC-8 | [`runtime 设计`](../../world-runtime/design.md#runtime-industrial-design) | [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-INDUSTRIAL / V-CONSUMER` |
| SC-9 | [`runtime 设计`](../../world-runtime/design.md#runtime-industrial-outage-design)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-sc9-eight-cells) | [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-OUTAGE / V-CONSUMER` |
| SC-10 | [`runtime 设计`](../../world-runtime/design.md#runtime-world-scope-design) | [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-WORLD-SCOPE / V-CONSUMER` |

SC-5/7/8/9/10 的跨读面合同还由 [`Agent 消费`](../../world-simulator/llm/continuous-agent-harness.prd.md#harness-infrastructure-consumer-matrix)、[`Agent join`](../../world-simulator/llm/continuous-agent-harness.design.md#harness-infrastructure-evidence-join)、[`Viewer 读回`](../../world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md#viewer-infrastructure-readback-contract)、[`Viewer 捕获`](../../world-simulator/viewer/viewer-manual.manual.md#viewer-infrastructure-capture-receiver)、[`Viewer join`](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#viewer-infrastructure-evidence-join) 承接；产品不拥有消费者实现或低层协议。SC-9 的八格分别保留 authority/read × 四边界及每个 authority cell 的 root-absent/pre-finality 与 root-present/post-commit/pre-child 两个 specimen；数量、window/W、receipt、progression 与 next-step 义务不被链接缩写。

### 5.3 事实边界与未决承接

已合并的 P2P/runtime/GWSC 专业设计和历史任务编写的 QA/Agent/Viewer 接收文档现可定位；[历史文档接收结果 #4180](https://github.com/eng-cc/oasis7/issues/4180)仅记录文档与追踪检查。其结果不构成 17/10/8 场景执行、active-provider API、真实 provider Agent、external headed desktop+narrow、同候选完整证明或发布通过。以下问题按“设计/接收可定位，当前实现与执行证明仍须各 owner 判定”复核，不以文档采用删除 proof gap。

本文记录目标产品合同。共识原型与执行、恢复、消费者的当前能力边界分别由[分布式专题的现状](distributed-consensus-and-state-availability.prd.md#5-当前与目标的分离)、[执行专题的现状](deterministic-world-execution.prd.md#3-当前与目标的分离)及专业 authority 说明。文档采纳、设计承接、实现、已执行验证与公开发行是不同事实；本节和追踪表中的场景说明是验证要求，不能读作已有测试或已经通过。

| 未决问题 / 影响 | 决策 role 与所需信息 | 解决触发与临时边界 |
| --- | --- | --- |
| SC-1～SC-10 的准确接收已由 §5.2 及各状态卡逐项定位；其中尚缺哪些完整技术合同与跨域输入？ | 对应 P2P/runtime 专业 owner、消费者 owner 与 QA；依据各卡已列的 BFT 算法、pending/lineage/activation、工业字段、消费者 DTO 和 DC-5 attachment 缺口补技术合同；产品 owner 核对原义务不缩减 | 已有接收关系无需重新发号或重建；新增或修订技术合同批准后复核覆盖状态，当前部分覆盖不等于全量设计、实现或验收闭合 |
| SC-1/SC-4 等组合 full 验收与专题 required 局部检查分别覆盖哪些义务？ | 对应专业 owner 与 QA；提供相同义务的局部范围、组合范围和证据身份 | 验证设计明确覆盖边界后确认；此前保留根及正文更完整的验收条件，不以局部绿色结果降低组合要求 |
| SC-7/DC-5 的结构化 attachment/schema 是否覆盖状态转换、manifest/head 负例、receipt 0/1 与消费者 blocker/下一步？ | runtime/P2P、运维、消费者 owner 与 QA；核对同候选、同证据窗口的字段与判定 | 验证设计及附件合同可定位后复核；仅有 state-sync 模板或 health/transport 成功不能判通过 |

上述问题的历史接收来源为[已关闭的文档迁移协调任务 #3935](https://github.com/eng-cc/oasis7/issues/3935)；该任务已关闭，不恢复其当前义务；交付身份、依赖、处置和结果证据由 Git、PR、实际 CI 与评审记录维护，Issue/Project 按需使用，本文件不复制可变任务进度。技术设计或测试尚未安排时也不虚构任务、锚点或执行结果。

## 6. Non-Goals

这是目标架构，不是当前可用性或发行声明。当前代码仍须由 P2P/runtime/QA 同一候选证据评估；根 `README.md` 独占公开 claim envelope。

- 不把当前 stake-threshold prototype 误报为完整 BFT certificate、分区恢复、permissionless 服务可用性或主网。
- 不以本模块定义区域设施、工业/市场的玩法规则、配方/价格/产能/物流数值、charter、frontier、普通治理、资源平衡、Agent 行为或玩家体验；本模块仅定义本节的跨域执行边界。
- 不把运维拓扑、软件部署、SLA、费用/奖励数值或第三方库选择写成产品已交付事实。
- 不批量改写历史文档，不制造兼容 redirect 壳，不改变当前公开状态或发布声明。
