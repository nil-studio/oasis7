# Gameplay 工业创建与跨区市场结算合同 PRD

- `PRD-ID`：`PRD-GAME-018`
- 上层产品映射：受治理 recipe 的当前授权与有限退出消费 [`REQ-WR-GR-002`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002) / [`AC-WR-GR-002`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-002) 及 [`REQ-WR-GR-003`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003) / [`AC-WR-GR-003`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003) 的本文件 §3.3 有界子范围；工业提案、模拟、试点与准入消费 [`REQ-WR-GR-009`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-009) / [`AC-WR-GR-009`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009)；创作者许可与开放消费 [`REQ-WR-GR-010`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010) / [`AC-WR-GR-010`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-010)；OC 转让与世界资格隔离消费 [`REQ-WR-GR-011`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011) / [`AC-WR-GR-011`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-011)。需求、报价与生产/交付结算消费 [`REQ-SC31-009`](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009) 及 [`AC-SC31-010`](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-010) / [`AC-SC31-011`](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-011)；常态市场与危机边界消费 [`REQ-WR-ES-004`](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004) / [`AC-WR-ES-004`](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-004)。
- 主题 authority：本文件拥有工业能力创建、跨区市场交易与结算的玩家动作、机会成本、失败恢复、progression 和可玩性验收，以及 §3.3 受治理 recipe 的有限授权/退出、§4.1 有限许可、§6.4 紧急保供首版政策的玩法数值与分配规则；产品层拥有世界承诺与保护边界，`world-runtime` / `world-simulator` / `p2p` / QA 各自拥有执行事实、确定性、资产状态和验证证据。
- 设计适用性：`simple-topic-exemption`（`PRD-only-sufficient`）。本文承载 Why / What / Done、玩家循环、明确归属的玩法政策、风险与验收；API、schema、执行状态、事务实现与 UI 布局由相应专业设计承接，不另建重复的配对系统设计；配对例外在 [`doc/game/prd.index.md`](../prd.index.md) 登记。
- 当前执行：可变 task 状态与当前实现证据由 Git、PR、实际 CI 和评审记录承载；本文件不宣称当前已实现、已平衡、已可交易、已发布或已满足 release readiness。

- 配套专业系统设计：[工业四阶段执行](../../world-runtime/runtime/industrial-creation-simulation-and-admission.design.md)承接提案、确定性模拟、有限试点与治理准入；[受治理扩展生命周期](../../world-runtime/runtime/governed-extension-lifecycle.design.md)及其[核验手册](../../testing/manual/governed-extension-lifecycle.manual.md)承接 §3.3 与 AC-GAME-018-07..14 的当前授权、有限退出和替代；[有限许可与版税结算](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md)承接 §4 的使用权限、托管、有限收益、开放与争议；[跨区物流与 escrow 结算](../../world-runtime/runtime/cross-region-delivery-and-escrow-settlement.design.md#des-wr-cr-001)承接 AC-GAME-018-04..06 的物流、结算与读面；[紧急保供授权、冻结分配与结算](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md#des-wr-es-001)承接 §6.4 的执行合同。专业设计覆盖与当前实现、运行验证分别判定，产品设计豁免保持原意。

工业创建的具体编辑来源、完整发布包、离线配方样例及状态正文见[玩家发布内容交付](../../world-runtime/module/player-published-entities.design.md#ppe-creator-inputs)与[核验手册](../../world-runtime/module/player-published-entities.manual.md)。该专题保留 append-only 发布的创作交接；四阶段强制执行与许可由上述独立专题承接。实际 Shadow 与工业模拟、技术发布与产品准入仍须分别判定。

## 1. 目标与范围

工业创建和跨区交易都应给玩家留下可归因的选择，而不是一个“提交后世界自动变好”的黑箱。玩家要能理解新工业能力如何从提案进入受限试点和治理准入，以及货物如何从可见报价经过真实路线、目的地接收和分阶段结算。

本合同覆盖五个相连但不可混淆的玩家问题：

1. 如何提交可审计的工业提案，比较确定性模拟与原型/试点的风险和收益，并等待有范围的治理准入。
2. 如何在既有准入的范围、期限或 authority 改变时，区分停止新工作、原有限处置、等待替代和重新提交，保留已确认的投入、成果与责任。
3. 如何在期限、用途、区域和开放条件明确的创作者许可/版税之间做选择，不把一次创新变成永久垄断。
4. 如何把 OC 的链上转让与世界内资源、信用、合同、资格和高影响权利分开理解，避免用外部资产持有绕过世界条件。
5. 如何从有新鲜度边界的全球情报走到报价/合同、物理路线、目的地存储、escrow 里程碑和争议恢复。

工业创建、许可、OC 隔离、跨区结算与紧急保供的产品消费均已闭合到上列 active `GR-009..011`、`SC31-009/010/011` 与 `ES-004` `REQ/AC` 锚点。普通治理的事项快照与额度桥消费由 [`PRD-GAME-019`](gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md) 的 [`AC-GAME-019-04`](gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#ac-game-019-04) / [`AC-GAME-019-05`](gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#ac-game-019-05) 承接，不在本工业/市场合同中重复定义。产品层的“工业规则、市场边界和不作实现/合规承诺”仍优先于本合同，本合同不把紧急保供扩展为新玩法系统。

`GR-002/003` 的当前授权、收缩、替代和退出另由 [§3.3](#governed-recipe-lifecycle-policy-v1) 与 [AC-GAME-018-07..14](#ac-game-018-07) 承接 `governed_recipe_factory_v1` 子范围。一般 product/factory 能力、其他 profile 与所有入口的完整落实仍是原产品义务；本文的有限首版不下调这些义务，也不将其整体标为完成。

OC 与世界资格分离现在有明确的 active 产品接收边界：工业/世界资格消费 [`REQ-WR-GR-011`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011) / [`AC-WR-GR-011`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-011)。治理快照与额度桥消费 [`REQ-WR-GCB-005`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005) / [`AC-WR-GCB-005`](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#ac-wr-gcb-005) 由 [`AC-GAME-019-04`](gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#ac-game-019-04) / [`AC-GAME-019-05`](gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#ac-game-019-05) 承接；GAME-018 在此保留 GR-011 的工业/世界资格隔离，不把 GCB-005 的快照/额度桥验收重新定义为工业或市场规则。本合同只定义目标玩法隔离和失败可读性，不把产品锚点写成当前实现、当前可用或 release 证据；执行与实现仍由对应专业 authority 收口。

## 2. 玩家循环：从创造机会到可复盘结算

所有预览、报价和治理请求都是只读的：它们不预留资源、能力、资格、库存、路线、结算顺位或版税。玩家确认后，权威执行必须以同一份新鲜事实重验；事实漂移时只能要求重报价、补证、缩小范围、改走替代路线或原子拒绝。

| 阶段 | 玩家动作与比较 | 玩家获得的即时价值 | 机会成本、失败与下一步 |
| --- | --- | --- | --- |
| 1. `inspect_opportunity` | 查看待解决的需求/瓶颈、现有能力、资源与资格边界；判断提出新能力还是使用既有生产/交易路线 | 能区分“值得创新的问题”与可由现有路线解决的问题，并看到可能影响范围 | 花费注意力、证据准备和试点窗口；证据不足时补证、缩小目标或放弃，不进入不可逆提案 |
| 2. `submit_auditable_proposal` | 提交来源、假设、适用范围、预期影响、风险和退出条件；比较继续、延期、拆分或撤回 | 提案取得可读的审查位置和缺口清单，不取得世界写入权 | 提案准备会占用时间/材料/押注机会；重复、越界或无法复核时只保留可读拒绝与补证路径 |
| 3. `compare_simulation_and_prototype` | 先看同一确定性输入下的模拟结果，再选择受限原型/试点、继续观测、改参数假设或停止 | 看见预期收益、资源压力、失败后果、适用范围和下一审查条件 | 模拟失败不产生世界效果；试点会占用限定资源/窗口且可能暴露损失，但成功也只产生证据，不自动获得全局能力 |
| 4. `request_governed_admission` | 提交模拟/试点证据，比较有范围准入、补证、拒绝或重提；读取准入范围与复核点 | 获得可追溯的局部能力使用条件、许可候选或开放倒计时/条件 | 治理等待、复核和合规成本可能延误收益；准入只作用于声明范围，缺授权、过期或范围漂移时停止并返回补证/重提 |
| 5. `inspect_market_information` | 查看跨区供给、需求、报价、信誉、路线与信息时间戳；比较本地生产、远端购买、延期和替代用途 | 形成带新鲜度和不确定性的交易判断，不把发现当成库存或到货 | 情报会过期、私有或不完整；错误判断会消耗机会窗口、信息成本或预备资金，玩家可改走本地/替代路线 |
| 6. `commit_route_and_escrow` | 比较报价/合同、路线、时间、损耗、目的地容量、escrow 锁定与里程碑；确认一条可追溯物理交付承诺 | 取得明确的进行中承诺、下一里程碑和取消/争议边界 | 资金、库存、运力和时间被占用；提交前条件漂移只能重报价/拒绝，不得静默换货、换路或提前结算 |
| 7. `observe_delivery_and_settlement` | 逐阶段查看出发、在途、到达、目的地存储与里程碑；选择等待、补救、争议、退回或替代用途 | 每个里程碑产生一次可归因结果；最终接收后才形成相应用途与 progression 方向 | 延误、损耗、目的地拒收或对手违约保留已占用/已损失价值与证据；下一步是恢复、争议、重提或回到本地路线，而不是重复领取奖励 |

循环完成的标准不是“看到一个报价”或“完成一次模拟”，而是玩家能回答：我改变了哪项能力或合同、投入和锁定了什么、哪一事实让它失败、还保留什么、下一次回来应做什么。任何模型建议、投票支持、旧缓存、市场热度或链上 receipt 都不能单独被表达为世界能力、到货或结算完成。

## 3. 工业能力创建：提案、模拟、试点与准入

### 3.1 四个阶段必须可区分

| 阶段 | 玩家可读承诺 | 不能推断的结果 | 可恢复动作 |
| --- | --- | --- | --- |
| 可审计提案 | 来源、假设、边界、预期影响、风险、证据缺口和撤回条件 | 没有世界写入权、全局可用性、默认配方、收益或治理资格 | 补证、缩小范围、拆分目标、撤回或改走既有能力 |
| 确定性模拟 | 在同一权威输入和声明范围内比较资源消耗、产出预期、失败路径与敏感假设 | 不扣库存、不占容量、不创造 recipe/设施、不发奖励或能力 | 修改假设后重新模拟、选择观测、停止或进入受限原型 |
| 原型/受限试点 | 在明确 scope、期限、资源上限和退出条件下验证“能否运行”和风险 | 不取得永久许可、全球市场资格、世界规则写入或外部价值 | 降低范围、暂停、回收未消费投入、记录失败证据并重提 |
| 治理准入 | 证据、审查结果、允许用途/区域/期限、复核点和撤销条件 | 不取得范围外权力、永久垄断或绕过公共安全/既有权利 | 补证、申诉、重提、等待复核或回退到既有能力 |

玩家至少能比较 `continue_proposal`、`refine_scope`、`run_deterministic_simulation`、`start_limited_pilot`、`request_admission`、`defer` 与 `abandon` 的时间、资源、锁定、风险和保留价值。准入或试点成功只让声明范围内的下一动作可用；任何 surface、Agent 或离线文本不得将其中某一阶段直接表述为最后阶段。

### 3.2 失败恢复和滥用边界

- 输入缺失、模拟不可复现、原型超过资源/范围、审查证据失效或治理资格不足时，失败必须保留提案来源、已消费/未消费投入、失败原因和下一次复查条件；不得自动重试、自动扩大范围或把失败试点当成正面 evidence。
- 模拟与试点的重复提交、重连、Agent retry、快照恢复和 replay 至多产生一份同义结果；不重复扣材、不重复发证、不复制准入、不继承旧奖励。
- 任何“模型生成配方”“高票支持”“历史先例”“一次试点成功”都不是单独的准入凭证。玩家需看到仍缺少什么、补齐后的下一步和不补齐时的安全停止。
- 受限准入的失败恢复是 `补证 -> 缩小 scope -> 再审查` 或回到已有生产/交易路线；没有安全的替代路径时，应显示 `no_safe_creation_recovery`，保留已有事实而不是自造默认能力。

<a id="governed-recipe-lifecycle-policy-v1"></a>
### 3.3 受治理 recipe 授权与有限退出首版政策

本政策消费[受治理扩展产品 §2.2](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#2-玩家边界与组合关系)及 `GR-002/003`，拥有首版的经营选择、期限/范围、有限处置与玩家结果。它以 §3.1 的四阶段准入为前提；[PPE 发布](../../world-runtime/module/player-published-entities.design.md#ppe-publication-authority)、[recipe 六态和六类工作处置](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)、§4 许可及工厂原控制权继续合取适用。授权状态不替代 recipe 状态，PPE exact-artifact admission freeze 也不自动停止所有既有实例。

#### 3.3.1 有限能力范围与授予条件

`governed_recipe_factory_v1` 只接受单个 exact `Pure` recipe/artifact 与不可变 recipe/Profile 内容，在最多 **2 个明确列举的既有工厂** 上消费；每厂最多 **1 个未闭合 job**，每 job 最多 **2 批**。本首版没有物流、转换、迁移或付费许可；许可 authority 必须明确给出合法零费或 `not_required`，缺许可事实不视为免费。`Pure` 只约束模块执行形式，不授予生产、资产访问或治理权。本次替代仅更换同一内容的授权；新行为、artifact、因果版本或扩张范围重新经过原适用四阶段与发布准入，不能继承旧证据或原工作的输入/产物。

首个执行接收器仅支持[配套 Runtime §9.2](../../world-runtime/runtime/governed-extension-lifecycle.design.md)的铁锭候选及其明确产品验证闭包；其他 Pure recipe 不能仅因满足工厂/批次数上限就推定适用，仍须补足对应的来源、原准入、真实消费者与有限退出设计。这个接收器范围不代表一般 recipe 或其他扩展类型已经支持。

授予时必须同时说明原 authority、不可变能力内容、由合法 authority 认可的稳定能力范围身份、具体工厂、允许操作、世界 tick 半开有效区间 `[start_tick,end_tick)`、复查条件和到期处置。稳定范围不因新 module/Profile ID、安装实例、别名、请求 ID 或访问入口改变；调用者不能自报一个新范围身份绕过重叠。其他权利或许可仍受其原边界约束，当前工厂 owner 的同意不能代替治理准入。

期限处分必须在原授予时绑定，列出所有可产生后续效果的消费者、原工作最大量、支持的安全动作、合法接收者/保管责任、每项必要费用与资源上界以及有限处置截止点。预览与申请不锁定资源或权限；正式授予及实际工作提交时，必须能够证明所需退出资源、容量、authority 和完整处置适配器均可用。缺失或相互冲突则不授予；不能先投入生产再补造退出权。首版数值约束单次影响集合，不宣称容量或玩法平衡已经验证。

#### 3.3.2 授权变化必须明确处分

| 明确选择 | 玩家能取得的结果 | 必须保留的边界 |
| --- | --- | --- |
| 仅停止新增发布/激活 | 指定范围不能再取得新的技术发布、安装或激活结果；既有能力按其仍有效的原范围运行 | 不从这项处分推导所有既有实例停机，也不把历史 receipt 当成再发布权 |
| 限制既有能力未来新效果 | 受影响范围停止被限制的新排程或后续效果，且不得新增发布、安装、激活或扩权；其他明确未受影响范围继续受原条件约束 | 必须逐项说明旧工作可执行的原承诺或安全处置，不以错误返回删除材料或责任 |
| 受控退出 | 关闭受影响新工作及新增发布、安装、激活，仅执行已批准且当前仍合法的有限最小处置，直至原义务结清或出现具体 blocker | 不扩展 scope、重开批次、新建控制权或用临时处置恢复普通生产；未结清不表示安全停止 |

组合选择必须声明次序及共同生效边界。对既有能力缺少完整处分的管理性收缩、撤销或替代请求没有生效结果，不能先改新发布资格、授权 scope 或现有工作再补计划。有效区间的终点按已绑定期限处分执行：`tick >= end_tick` 时普通发布/激活和生产权归零，不依赖清理任务是否运行；它不需要临时决定“是否继续旧批准”。续期或复查请求、申诉、重连和客户端时钟均不延长原区间。

#### 3.3.3 原工作与最小动作

每次新工作、首 sink 及后续生产/完成效果，均须在实际生效边界读取当前授权、scope、原控制/安全/许可与 recipe 条件。只检查模块 planner 不足以允许原生入口、tick 自动完成或恢复动作继续；旧 receipt 只证明已经发生的事实。一个 job 被接收，不承诺它可以越过授权终点继续普通生产。

停止与退出逐项沿原六类工作处置处理已接受未投入、WIP、在途、缓冲和终端义务；本首版不含的物流、转换、迁移类型若真实存在，必须明确拒绝本 profile 的转换而保留事实，不能当成空集合。新排程也属于关闭判断。已耗投入不返还，独立资产、已确认产物、工厂/Agent 身份、来源、已结算合同和原 receipt 不删除或追溯改写。`PauseFactoryProduction` 本身不作为暂停在制 job 的证据；只有原消费者真实支持并获当前授权的处置才可执行。

期限或撤销之后，最小动作只为安全、可辨认价值保全、不可避免的原义务或可读退出结果服务，且须绑定原有限 job/数量、原合法处置、当前原 authority 和剩余资源。首版处置窗口为 `[effective_tick,effective_tick+4)`；原 job 已有更早合法截止时取更早者，计算溢出不能启用。它不赋予新的生产批次、额外收益资格或任意 `finish` 权；普通生产和正常替代例的 finish 在原普通权仍有效且当前安全条件允许时完成。超出普通期限的完成只可来自明确覆盖该原不可避免义务的有限处置授权，不能把 4 tick 处置窗口当生产宽限期。重试、暂停、重启和换键不能重置累计量、费用或期限。处置预算/截止点耗尽、接收者不可用或当前权利不再允许动作时显示 `exit_blocked`，保留原有限材料、保管责任和未结义务，停止进一步效果；不得宣称强制闭合、删除责任或自动授予兜底能力。重新取得处置条件仍需原合法程序，不静默续权。完整调用、逐 payer 费用、保留资源与闭包预算按 [Runtime 的有限预算](../../world-runtime/runtime/governed-extension-lifecycle.design.md#extension-budget)实现，不能只计生产耗材而漏掉必要验证/退出调用。

#### 3.3.4 先结清再替代与可读选择

在任一权威状态，同一重叠能力范围最多一份可执行授权。首版替代前，受影响工厂必须关闭旧授权的新工作并结清该范围全部原义务；未结 WIP 或终端义务阻止新授权激活，不存在旧授权 drain 与新授权生产并行的例外。新授权保持 pending，直至一次权威提交同时使其生效并缩去旧授权重叠部分。非重叠工厂保留原条件，不因另一工厂的替代被扩大或冻结。旧未确认请求明确拒绝或由原请求人按新条件显式重提，不自动改绑或迁移旧 job。

正常设计算例为：`G0=[10,30)` 覆盖 `F1/F2`，`G1=[20,40)` 申请同一 exact recipe/artifact 的 `F1` 范围；tick 18 先合法开始 `F1/J1`，再通过完整处分关闭 `F1` 新排程，只保留原 `J1` 的有限合法 finish，tick 19 `J1` 完成并结清全部义务；tick 20 一次提交使 `G1/F1` 生效、从 `G0` 移除 `F1`，`G0/F2` 继续原合法运行。三个演示 job 各 1 批：`J1` 属于旧 `F1`；tick 20 替代后开始的 `J2` 属于保留的旧 `F2`，`J3` 属于新 `F1`，均在 tick 21 结清，从不复制同一批投入或历史。`G1` 在提交前始终 pending；若 `J1` 未结清、scope/authority 漂移或越过原期限，替代拒绝并返回具体 blocker。此例是待实现合同，不是当前 world/artifact 或运行结果。

玩家应能比较等待原有限处置、停止新增投入、补证/复查、显式重提、申请替代与使用已有独立路线。读面同时说明原 authority、变化原因、当前允许/移除范围、期限、原工作责任、替代 pending/生效事实、`next_action` 和 `next_recheck`。关闭请求不等于已停止，技术发布不等于当前可用，替代待决不等于新能力就绪；缺事实时展示 `incomplete/unknown/blocked`。完整消费者、有限闭包、提交和恢复的具体合同由[Runtime 设计](../../world-runtime/runtime/governed-extension-lifecycle.design.md)与[核验手册](../../testing/manual/governed-extension-lifecycle.manual.md)承接。

## 4. 创作者许可、版税与开放

获准能力的收益权是可读、可审计、有限期的玩法奖励，不是永久产业封锁。玩家在确认许可或开放路径前，必须能比较：适用能力/用途/区域、开始和结束条件、版税/收益分配、维护责任、撤销/争议、开放条件以及不能保证的未来价值。

- `inspect_license_terms`：读取 scope、期限、用途、收益/版税、审计与复核点；这一步不锁定能力或资金。
- `accept_scoped_license`：接受一次明确范围的使用权，承担许可成本、合规和维护机会成本；超出范围必须重新准入。
- `choose_open_condition`：在已声明的到期或开放条件达到时，比较继续有限许可、转入公共开放或重新提出维护版本；开放不追溯取消已合法产生的历史收益或 receipt。
- `challenge_or_renew`：对错误归属、范围漂移、到期判断或收益结算提出可读争议/续期请求；待决请求不延长旧许可、不产生第二次权利。

许可到期、既定开放条件满足、撤销或 scope 缩小时，玩家看到通知、理由、当前可用动作、已有工作如何处置以及不失真的迁移/替代路线。获得许可权本身不等于版税已经结算：没有绑定能力使用/交付事实的 verified receipt 或有效 escrow 里程碑，不得发放、扣除或宣称版税收益。开放条件不能被单方隐藏地延长，创作者收益不能覆盖公共安全、玩家独立资产、身份或其他宪制保护。首版玩法数值与收益归属只采用下节；执行算法、事务与实际可用性由配套 Runtime 设计及证据承接，不引入汇率或外部价值承诺。

<a id="industrial-license-policy-v1"></a>
### 4.1 有限许可首版政策：`industrial-license-policy-v1`

Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47。承接 [GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010) 的有限许可与开放；受限退出同时消费原 GR-003 的历史保留和单一授权边界。

本政策是 2026-11-04 提出的可实施 v1 目标，不是已治理激活的制度、已实现功能或经过平衡实测的经济参数。使用当前世界的 `Electricity` 最小整数单位；不使用 OC、外汇、现实货币估值、借款或对未来收入的预支。只有准确声明的当前合法准入、资源权限、许可和执行条件同时满足才可使用能力；许可和开放均不替代其中任何一项。

| 首版规则 | 固定政策与边界 |
| --- | --- |
| 可收费单位 | 仅支持适配器可验证的 recipe 成功完整生产批次；每批次 1 Electricity。必须绑定 exact capability/admission、原工业 use root、互不重叠批次、执行时适用许可与实际完成 receipt。试点、预计产量、失败/被拒输出、普通发布或独立运输/交付不收费；生产和交付不能对同一批次收两次。 |
| 其他能力 | 生命周期/范围规则通用；没有已批准原生计量适配器的能力只能明确为 0 版税或开放条件，不能把未知计量当免费成功，也不能让任意 receipt 或物流 tariff 发版税。扩展付费计量须另版政策明确单位、source 与重复边界。 |
| 硬开放 | 同一 capability lineage 首次合法工业准入 tick 为 `A`，费权最迟在 `A + 3600` 开放；所有区间左闭右开且 checked 运算。提前公开只能缩短费权，不能延长。维护、改 ID、换许可、换受益人或续期不重置 `A`。 |
| 单次许可 | 使用范围、用途、区域、主体及维护义务明确；期限至多 600 ticks，且结束不晚于硬开放。待决续期不延长旧权，成功续期也只授予当前剩余有限期间。 |
| 单次承诺及并发 | 一次确认至多 32 个成功完整批次；同一 world/lineage/使用主体聚合至多 4 个未终结付费用途根。换 Agent、许可、请求 ID 或切单不重置该主体限额；本根生产处置、未消费 hold、通知窗口、使用/数额争议或原请求恢复未闭合时仍占槽。已赚取款可证明移交同一 lineage 收益池并保留完整来源/个人应付后，池的尾差、受益槽归属及收款恢复独立持续，不把已终结用途根一直占到硬开放，也不清除原权利；4 是同时上限，不是终身只许 128 批。 |
| 收益分配 | 至多 8 个有明确身份的受益人；权重为正整数 basis points，合计 10000。分配必须来自实质工作前同版同意、可验证来源和贡献 provenance 的 charter；没有合法分配依据则不能激活该付费路径。份额/收款路径转移只能显式同意并合法校验，不转让个人身份、一般信誉或过去事实。 |
| 预留与费用 | 查看或取得许可本身不扣版税。付费用途开工前足额冻结该根全部最大版税；生产资源、执行费用、争议/退出闭合预算另列合法来源、上界和受保护 hold，不互相借用。不是原资金主体的同意不得由许可或治理代签。 |

合作成果的事前同意、贡献与使用权证明、固定分配和退出由 [R&D charter 政策](gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-charter-policy-v1)拥有；许可消费其已验证交接，不替它裁定贡献。首版[份额转让](gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-share-transfer)只移动同一原槽全部未付及未来合法权益，拒绝向已持本分配其他槽的主体转让，保持原槽、bps、累计 E 和已付历史连续。其原归因程序与本节使用/收益程序分别有权，不能借新 charter、转让或换程序重新起算旧窗口。

lineage 必须有可核验的初次准入和来源链；同一规范内容和已知维护/派生版本不能靠新键重开收费期限。独立创造的不同能力不能仅因主观“相似”被强行归入他人 lineage；来源或派生关系无法证明时转原有权归属审查，不默认取得新费权或复制他人独立资产。专业设计负责规范身份、等价来源与当前消费者的机械验证。

单次使用许可到期只终止该许可的后续使用权，不提前改变整条 lineage 已声明的硬开放时间。硬开放前继续付费使用，须重新同意当前条款、取得有效有限许可，并对未完 root 在原工作处置允许时原子换绑；保留原 sink、完成量与已赚取账，只调整尚未消费的费用承诺，不双重冻结或重复生产。

到硬开放或声明的提前开放后，同 lineage 后续合法使用的创作者版税为零，不追溯取消已经合法赚取的收益。开放的是本政策内的工业许可费权，不代表他人材料、资产、私有数据、设施、身份、安全门槛或外部素材使用权一并开放。没有当前准入、安全或设施权限时，仍不能使用。

<a id="industrial-license-policy-earned"></a>
### 4.2 赚取、通知、可分配与支付

只有在费权和本次使用条件有效的半开区间内，实际权威完成的合法批次才赚取版税。新 profile 的完成提交须同根记录该批次、从版税 hold 转出的 `pending-earned` 以及按原主体权限可回读的 canonical 通知；通知 tick `N` 为该提交的世界 tick。它不要求客户端已读或玩家在线，不以第一次读取重新起算；缺这种原子登记能力不得启用付费适配器。迟到读取只能恢复原绑定、原通知和原义务，旧的无许可 receipt 不补造收费。

每笔收益先经过 `[N, N + 120)` 的异议窗口。无受理案件且窗口已结束，或对应精确范围已有有效 final 时，才能进入可分配累计收入 `E`。争议使用事实/数量时仅相关批次不进入 `E`；若实际使用量已确认，只争某个冻结份额槽的受益身份，则窗口结束后该量可进入 `E`，仅保全该槽的应付，其他槽正常处理。涉及整个原分配依据的争议也只保全确受影响的槽；不得借一个归属问题冻结全 lineage。确认前须披露 canonical tick 在离线期间仍可能推进，通知与申诉窗口不保证现实分钟或客户端送达时限。

同一 world/lineage/原分配版本按累计 `E` 记账，受益人 `i` 的基础归属为 `floor(E × w_i / 10000)`。新增归属只取相邻累计值的非负差；不对每张小单单独舍入，不在每次增长时使用可能使旧份额倒退的最大余数差额。未分尾差 `H = E − Σ floor(E × w_i / 10000)` 留在已经赚取的收益池，`0 ≤ H < 受益人数`，不退付款人。

只有费权结束、全部既有 root/批次与晚到义务、通知窗口、案件及未知提交均可证明处置完成，才按最终余数从大到小、同余数按原分配版本的稳定份额槽标识排序，一次分配 `H` 个最小单位。槽标识及 basis points 不随身份纠错、收款路径或显式转让改变；有权处理只能核定该槽未付权益的合法归属，不能重分其他人的比例。换单、换许可或显式转让不重置累计基数；更换收款授权不重新取整或抹掉旧应付。分配与实际支付分开：原受益人无可验证接收账户时保留其个人应付权利，不能转给新的 Agent owner、退给付款人或让他人领取。

静态算例：权重 50%/30%/20%，`E=7` 时基础归属为 `3/2/1`、尾差 1；`E=11` 时为 `5/3/2`、尾差 1；完整闭合后最终为 `6/3/2`。分批到账和一笔到账相同。128 个待完成批次的最大版税 hold 为 128 Electricity，但这不包含生产、执行或闭合费用，也不是玩家获利或系统吞吐的实测值。

<a id="industrial-license-policy-exit"></a>
### 4.3 到期、公开、维护和存量工作

完成 tick 在旧费权范围内的合法收益，即使晚回读或晚支付仍沿旧已验证义务结算。到期/公开时尚未完成的批次不预先赚取，也不凭旧许可继续生产或自动收尾；必须沿[工业工作处置](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)及当前准入、执行权限重新判定。

允许在公开零费条件下完成的存量工作，必须以显式原子换绑保留原 root、已 sink 投入、已完成批次与旧收益账；后续合法完成量为零版税，旧未赚取费用返原来源。不得重建 root 再领材料、清掉旧债权或借换绑跳过安全处置。不能安全继续时明确停止、保留 custody/责任和原请求恢复路径；已消费材料和真实损失不自动退回。

维护义务须列明验证事项、适用范围、检查时点和失败处置；失效只缩小或停止对应后续使用，不自动没收无关资产。维护版本需重新走实际变更要求的模拟/试点/准入，不能借维护重新收费期限或延续旧批准。替代能力也必须独立具备当前准入、许可和资源条件，不保证一定存在安全替代。

<a id="industrial-license-policy-review"></a>
### 4.4 许可专属的有限争议程序

付费条款激活前须绑定现有世界治理明确授予的 `industrial-license.review` 本地事项权限、固定 region、政策版本、可处理对象/金额上限、名册和随机来源。服务合同 panel、charter、运维权限或多数支持不自动授予此管辖。该专业权限和程序仍是目标前置，没有真实合法来源及预算时付费路径不可激活；零费路径仍需自身有效工业权限。

名册至多 64 个去重 controller，排除付款方、受益人、关联控制者、原准入/许可批准者及本案直接经济关系人。付费激活时须能证明至少 6 名无冲突候选。案件承诺先固定，再消费已授权、事前不可预选的可验证随机源的首个合格结果，形成唯一候选排列；初审 3 人、复核另 3 人，补位和复核沿原排列顺延，不通过重开、nonce 或换随机结果重抽。每阶段至少 2 人同意同一范围、证据 root 及类型化处分，才形成该阶段结论。

数额、重复或范围的确定性错误优先按完整 canonical 证据重验；不需要责任判断的重验不能被包装成任意多数裁决。争议仅针对确切原条款或实际使用义务，各合法当事方都保有对应 `[N,N+120)` 登记窗口。相同受影响单位、争点和证据只能进入原 case；重试/换 ID 不能重复开案。不同后来义务保留其原通知与程序机会，不能为合并旧案剥夺新单位的申诉窗口。实际受理另记 `accepted_at`；统一程序基点固定为原通知的 `C = N + 120`，不由第一位申请人提前起算，以免抢先开案耗尽其他人的登记/证据机会。按以下边界推进：

| 阶段 | 固定窗口或截止 |
| --- | --- |
| 普通证据 | 从各方实际受理起可提交，统一至 `C + 60`；每个有责当事主体每阶段最多 16 个不同权威 receipt 引用。普通证据关闭后冻结全部已受理 scope/证据，不能抢先用局部结论耗尽他人机会。 |
| 初审 | 证据关闭后至 `C + 120`；按冻结输入判断。 |
| 一次程序申诉 | 初审可验证发布后 60 ticks 内且不晚于 `C + 180`；各方合格主张并入同一轮，非先到者独占，不扩展为一般重审。 |
| 复核 | 申诉集合冻结后至 `C + 240`；针对权限、遗漏证据、利益冲突、程序越界或可验证错误；不重选一套可被当事方操纵的随机结果。 |
| 硬尾部 | `C + 300`；查验完整 canonical 案件/初审/复核/final/和解与账本，执行下述有限结清。重连、补位、补证和继受不重置 `C`。 |

保全只涉及直接争议的未支付收益、对应未消费资金或必要的不可逆效果；无争议份额、工业 admission、独立资产和其他工作继续按原规则处理。panel 只能确认原账、依完整证据纠正未付归属/合法使用量、返未赚取资金或解除错误保全，不能改价格、续期、扩大 fee root、追索已合法支付历史、增罚或铸造补偿。

复核不自动推翻初审。有效初审没有合格申诉或到复核截止仍未被有效替代时按原规则 final；有权结论、同版合法和解和已确认未付款债权继续保留执行。硬尾部能够证明没有有效改变原合同的结论时，结束程序性保全，按原已验证 charter/实际使用基线结清已赚取款、返还未赚取款。不能取得完整结果时保留明确的证据/authority 恢复状态，查询原操作；不把未知当“无裁决”、不重开期限，也不给当事方单方无限冻结权。

归因更正只能处理有权可证明的合同内权益，不代签完整一般 R&D 制度或现实法律结论。已合法支付款不被本程序追溯改写；如后续证据揭示另有未补偿损失，必须如实显示，不能凭空扣他人余额或虚构补偿。

### 4.5 玩家比较与专业验收

接受前必须同时比较当前可用 scope/期限、有限费权和硬开放、最多冻结金额、已披露生产/执行/闭合成本、维护责任、收益分配、可用争议及其有限窗口、未完成工作处置，以及公开/零费或已有能力替代。收益预期不是保证；创造路线不能成为首局唯一解，换主体、抢提交、囤许可或暂停客户端均不延长权益。

具体消费者、整数守恒、并发与恢复由[许可设计](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md)承接；[验收手册](../../testing/manual/industrial-license-and-royalty-settlement.manual.md)逐项验证窗口边界、开工/完成/支付分层、累计分配、单次托管、部分保全、转让和当前准入组合。四阶段方法另见[工业创建手册](../../testing/manual/industrial-creation-simulation-and-admission.manual.md)。参数复算不证明经济平衡、运行能力或玩家理解；改动计量、费率、期限或分配必须建立新政策版本并保留旧义务，不通过后台配置重写已接受合同。

## 5. OC 转让与世界资格分离

玩家可以在链上转让 OC，但该动作只改变 OC 的链上所有权/结算事实。受限 OC 锁定、可撤回委托或控制人快照仅在已声明的区域 charter/地方治理规则要求时，才与持续运营、维护或交付形成的本地贡献共同评估；工业提案/准入、普通市场合同和紧急保供不自动继承该双资格门槛。它们不单独自动取得：

- 世界内区域资源、服务信用、库存、合约、市场信誉或交付资格；
- 工业提案的作者身份、治理准入、试点资格、区域 charter、tenure 或公共 levy 权利；
- 高影响竞争、生产、治理、紧急保供或任何外部兑换/收益承诺。

反向地，世界内的资源、信用、合同和资格也不自动变成 OC、可链上转让资产或可赎回外部价值。玩家比较 `transfer_oc`、`qualify_world_capability`、`earn_world_credit` 和 `enter_contract` 时，必须看到它们各自的条件、收益、不可替代之处和机会成本。OC 交易成功但世界资格不足时，系统应保留 OC receipt、明确资格 blocker 和补资格路径；资格成功但 OC 余额/来源不满足时，保留世界记录并返回资金/合同替代路线，不能把一侧成功伪造成另一侧成功。

## 6. 跨区发现、物理交付与 escrow 里程碑

### 6.1 发现不是到货

市场发现应同时显示供给/需求来源、报价或合约状态、信息时间戳/新鲜度、可见范围、不确定性、路线候选、预计损耗、目的地容量和下一复核时间。陈旧、私有或不完整情报必须显式标记，不能伪装成全局实时库存。

玩家必须能比较本地替代、远端报价、等待新情报和放弃交易。`discover_global_offer` 只产生可读情报；`accept_quote` 或 `commit_contract` 才产生范围明确的交易承诺；这两者都不表示货物已出发、已到达或需求已满足。

### 6.2 物理路线和目的地状态

确认合同后，玩家看到真实可审计的出发、在途、边/路线、时间、损耗/风险、目的地准入与存储占用。途中失败可以选择等待修复、有限改道、退回、交接、取消或争议；每个选择必须显示剩余货物/资金、已损失价值、重新占用的时间与容量，以及不允许该动作的原因。发现结果、报价、出发 receipt、到达、目的地存储和最终用途是不同的状态，前序不能覆盖后序。

到达、存储接收和所有权/合同结算必须分别可读：`arrival` 只证明货物抵达目的地边界；`storage_acceptance` 还要求目的地准入、容量与保管责任成功；`ownership_settlement` 只有在合同声明的接收事实与 escrow 里程碑均验证后才可成立。货物已到达但未被目的地接收，或已入库但所有权里程碑仍待决时，不得发放最终交付收益、版税或把它们合并成一个完成状态。

### 6.3 Escrow、里程碑与争议

资金或等价结算可以按声明的里程碑释放，但 escrow 不是物理交付的替代物。玩家能看到当前里程碑、释放/保留的理由、双方责任、证据窗口、争议入口和下一复查时间。

- 里程碑只有在与其绑定的物理/接收事实满足时结算一次；报价、链上转账、离线截图或单方 receipt 不足以完成里程碑。
- 争议待决时保留已确认的货物、资金、责任、证据和占用边界；不自动给任一方最终收益，不重复扣款/释放，不追溯改写此前已结算的合法里程碑。
- 交付失败或目的地拒收时，玩家比较补救、退回、改道、重谈、索赔/争议与放弃的时间和损失；没有合法路线时安全停止，并保留可恢复的 escrow/物理状态。

<a id="emergency-supply-policy-v1"></a>
### 6.4 紧急保供首版政策：`emergency-supply-policy-v1`

Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1, O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7, O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1, O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3

分别承接产品 [REQ-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001)、[REQ-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-002)、[REQ-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-003)、[REQ-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004)。

本节是本次设计提出的完整首版政策，供实现与评审使用；不是历史批准、已启用参数或已经完成平衡测试的声明。政策的精确数值只在本节声明，产品专题解释玩家含义，runtime 设计定义可验证执行方式。后续改值形成新的政策版本与未来生效边界，不能修改已有授权、冻结批次或已承诺的资金。

#### 6.4.1 品类、证据与触发

首版必需品仅为既有资源种类 `Electricity`，用途仅限已登记的最低连续运行义务。`Data`、一般工业扩张、投机库存、OC、restricted starter claim funding 与尚未登记的模块材料均不属于首版保供。电力必须由真实供给、路线/供配电和接收/存储合同支撑；不存在可用适配器时返回“不支持该保供通路”，不得把电力记账当作瞬时跨地点运输。后续新增必需品要另发政策版本、匹配实际资源身份和对应验收，不用模糊的“其他重要物资”扩大首版。

区域使用危机前已连续有效达到登记成熟期的 canonical `region_id` 及地点归属；证据分母包含该区域全部合格主体/地点，提案者不能圈选较小范围制造比例。每个世界、区域、品类和分配窗口只允许一个生效或待生效的新分配授权及一个区域批次槽；另一授权不能拆走部分地点单独抢资源。已进入退出中的旧授权仅保留旧合同结算，不占新资格。区域拆分/合并在同一预算区间沿原 `region_budget_lineage` 合并计算累计开支，不因新 ID 或换资金账户重置上限；缺可核验归属/lineage 时不启动付费采购。

| 政策项 | 首版值与口径 |
| --- | --- |
| 观测与批次时间 | 全部采用权威世界 tick。每 `60` ticks 一个固定观测/收集窗口，窗口对齐世界 tick `0`；不换算为现实分钟。 |
| 危机观测长度 | 连续 `3` 个完整观测窗口均达到短缺条件；授权提交时最新证据不得早于 `60` ticks。 |
| 连续运行视野 | 未来 `600` ticks 的已登记最低消耗义务；仅使用有权威来源、无重叠的消耗区间，不接受玩家自行预测的经营增长。 |
| 进入阈值 | 常态供给未覆盖需求占最低需求至少 `30%`，且受影响至少 `3` 个独立控制主体、至少 `2` 个权威地点，并占所声明区域合格主体至少 `30%`。上述条件必须同时满足；分母为零或不完整则证据无效。 |
| 登记成熟期 | 最低义务、地点归属、公共关键服务标签与控制主体聚合必须在首个证据窗口之前已连续有效 `600` ticks。危机中临时改名、拆号或新建空设施不增加本次额度。 |
| 单次授权 | 生效区间为开始含、到期不含，最长 `600` ticks；无自动续期。开始 tick 必须对齐收集窗口。 |
| 提前恢复阈值 | 连续 `3` 个观测窗口的常态供给未覆盖比例均低于 `10%` 时，关闭新分配；用低于进入阈值的退出线避免来回震荡。证据停止更新超过 `60` ticks 时也停止新分配。 |

每个观测点的需求是合格最低义务之和，必须分别记录两种覆盖。`normal_coverage` 仅含受益方合法可用的常态库存与非保供已确认承诺，并扣除制造等其他义务占用；危机进入与恢复退出只用它计算常态缺口。`effective_coverage` 再计入已履行和所有保供活跃承诺，仅用于计算本次还可分配多少，不能把本计划刚发出的救助视为市场恢复。保供来源库存/承诺保持 provenance，到期或转库不能洗成常态供给。

计入任一种覆盖的在途量均须有真实货物、容量、目的地接收依据并能在对应消耗区间前到达。一个资源批次或承诺只能覆盖一个数量区间；未来产能、待售广告、客户端估算、重复物流记录及无法证明可达的在途货物不算供给。证据须保留相应 world/state root、义务/主体/地点集合和资源事实；只提交汇总百分比不合格。

价格上涨不参与触发，单处故障或单个经营者经营失败本身也不触发。局部故障只有已形成上述跨地点、跨主体、持续的最低运行缺口时才属于系统性危机。小规模世界达不到范围门槛时继续走普通市场与恢复路径，不能伪造额外主体。普通 `CrisisSpawned` / `CrisisTimedOut` 只保留其原玩法含义。

#### 6.4.2 资格、固定批次与配给

受益主体必须有当前有效的世界身份、合格控制主体聚合、范围内地点及最低义务。首版优先顺序为：`P0` 已登记的公共供电与其必要调运/存储服务的最低运行；`P1` 其他合格 Agent 的最低运行。公共标签必须来自危机前的有界服务登记；持币、治理票数、管理员偏好、付费会员和一般产能扩张不加分。处于相同控制关系的账户合并计算，不按账户或 request 数量增加份额；没有聚合证明时阻断而非假定独立。

每笔最低义务按世界对齐的 `60`-tick 消耗区间生成唯一需求。申请可覆盖未来视野内、且结束不晚于原授权到期的完整区间；同一区间不能被相邻授权重新定义。供货预计到达不晚于该区间开始，否则该需求本批次拒绝。最先到来的消耗区间先分配，同一区间内先 `P0` 后 `P1`；更远期的 P0 不能挤占当前区间的 P1。

收集窗口为 `[60k, 60(k+1))`，到边界即关闭。每个主体可以补全自己的申请，但最后一次内容必须在关闭前取得权威登记；替换会保留原请求 lineage，不能同时生效两份。迟到、网络重试或重连不加入已关闭批次，玩家需显式选择下一窗口。资格、需求、地点范围、实际资源池、政策、报价与资金同时冻结；冻结后不得加入新请求、换政策或改供给池。

在同一资源种类、交付地点、消耗区间和优先级内，对可核验剩余需求按量同比例分配：先取每主体精确份额的整数下界，再按小数余数从大到小分配剩余的最小资源单位；余数相同时按危机前已登记的控制主体身份字节序决胜。多条最低义务先聚合主体额度，再按义务身份字节序拆分，不能通过拆申请扩大份额。此规则公开且确定，不使用 lottery、随机 seed、请求到达顺序或管理员顺序。供给足够才进入下一优先级；余量不足以完成全部需求时返回真实部分分配。

资源池以已存在且供应方自愿绑定到该地点的可交付供给为限。相互冲突的库存、路线、目的地容量和资金必须在冻结前形成可同时兑现的供给计划；无法证明该条件时整批阻断，不由评估器猜测运输能力。跨地点共用的资源先按相同“消耗区间、优先级、比例、稳定身份余数”规则确定各地点可用量，再做地点内分配；不得让某地点因提交更快占走其他地点的份额。常态已承诺资源优先履行其原合同，不得被保供追回。

#### 6.4.3 采购、补偿与资金来源

首版只支持自愿供给和已授权公共储备，不强制征用、不更改普通市场价格，也不增加公共 levy。授权声明唯一公共资金账户及真实已入账的专项 liquid OC 预算；restricted starter claim grant、starter OC 发放通路、未兑现贡献和未来税收不得作为来源。一个区域在同一 `600`-tick 预算区间内的新保供承诺总额，最多为该区间起点可自由支配公共余额的 `10%`；跨授权、分账户划转或续期不重置该上限。其他合同已锁资金先扣除，负数或来源不明不允许启动付费采购。

付费报价必须由供应方签署，声明资源单位价、数量、正向运费、退回费用上限、交付截止、保管与损失责任。可采购单位价不超过普通市场参考价的 `125%`，向下取到最小 OC 单位；这只限制本计划买价。参考价为首个危机证据窗口前 `600` ticks 内独立控制主体之间已最终结算的同地点/同种类普通成交单位价的成交量加权中位数，至少 `20` 笔且至少 `3` 组独立交易双方；排除保供交易、关联方交易、撤销与未结算订单。证据不足时付费采购不可用，但真实无偿捐入的库存仍可配给，不凭空补参考价。

同样可用的供给按含可验证正向运费的单位成本从低到高选择，成本相同按危机前供应主体身份和货物批次身份排序。供方与该笔需求受益控制主体不得相同。每个承诺在建立时足额锁定采购款、已承诺运输费及退回费用上限；总锁款同时受账户余额、授权预算与上述区间上限约束。

采购、补偿、分配与 rationing 是同一供给义务的不同效果：采购锁定真实货物/资金；分配或 rationing 确认受限数量；补偿只向供方/承运方结算该义务中已经验证的物资和服务价值，不另发一份补贴。最终付款为实际被合法接收的数量乘固定单位价，加已完成运输里程碑费用，全部来自该承诺 escrow；未交付采购款不支付，未用费用原路释放。受益人获得绑定最低消耗义务的电力使用量，不取得可转售库存、liquid OC、可提现补贴或额外治理资格。不得同时发放采购款、等值现金补偿和一份重复物资。

#### 6.4.4 退出、部分履行与申诉

| 退出时所处阶段 | 首版处置 |
| --- | --- |
| 仅登记或冻结评估，未承诺 | 到期/撤销/恢复关闭后终止为 `expired`，无库存、资金或需求占用；不得迁移到新授权。 |
| 已承诺但尚未出发 | 不再出发；原子解除未消费的货物、路线、目的地容量、资金和需求占用，供应方收回原货物，未完成运输不收费。 |
| 已在原授权有效时合法出发 | 保留原供货合同，按原地点、原数量、原截止和原 escrow 完成接收及结算；这是已有义务，不是新授权或新增资格。不得增量、加价、改受益人或延长截止。 |
| 延误、目的地拒收或到达已超过消耗区间 | 不计需求完成；保持货物保管与需求占用，最多等待 `120` ticks 修复；仍不满足则按原已锁定退回预算发起物理退回。无合法路线则进入 `custody_blocked` 并保留保管责任，不删除库存或造退款。 |
| 部分接收 / 部分损失 | 仅接收部分增加已履行量并结算；剩余部分独立保留占用、退回或按真实损失 receipt 终结。损失由供应方承担未交付采购本金，承运方仅获已完成里程碑费用；首版无额外损失保险或惩罚性赔偿。 |
| 已确认的交付与补偿 | 保留旧授权 receipt 和历史；后来的撤销不回收已合法消耗的电力或改写付款。纠错通过新的、有预算来源的裁决义务处理。 |

同一需求的可再申请量是权威最低需求减去“已履行量 + 未终止的采购/预留/分配/在途承诺”。阶段切换只移动同一个承诺的数量，不重复扣除；最终结算必须把占用原子转为已履行，退回/终止只有在原货物及责任已证明处置后才释放占用。续期、换 request、换账户、重新命名需求或跨授权都不能清零这本账。进入下一消耗区间是新的真实需求，但仍扣除当时实际可用库存和可用承诺，不能通过虚构消耗获补两次。

任何受影响主体可在结果可验证发布后 `120` ticks 内申请复核。授权包必须给出有 `emergency_supply.review` 范围的既有治理复核主体；复核主体与该项供需交易及原分配执行方的控制主体须分离。登记申诉不恢复旧资格、不自动退款；只有有权裁决可冻结受争议未结算部分或产生有界纠错。首版裁决只允许确认原结果、在不增加原承诺的范围纠正证据、取消未结算部分，或以新预算形成单独补偿；不得修改历史 receipt。受理后 `120` ticks 未形成可核验裁决，受争议部分保持 `review_blocked`，无争议部分继续，读面显示所缺裁决和下一复核点。

本政策仍需检验“180-tick 证据等待能否及时避免中断”“10% 预算是否足以保供”“稳定身份余数是否造成可见长期偏差”等平衡假设。验证这些假设可以改变下一版政策，不能让首版实现自行挑阈值或放宽授权。验收沿用产品 ES-001..004 与 [ES-VERIFY-001..004](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-001)，并增加阈值边界、跨地点/主体份额、无参考价、资金耗尽、部分退回与跨授权重复需求组合。

## 7. 进度、收益与平衡风险

本主题的长期收益不是无条件增加库存或货币，而是新增可复用选择：一项可审计的工业能力、一段有范围的生产/服务权、一个更可靠的跨区供应路线、一次争议后仍可恢复的合同信誉，或一条从创新回到独立经营的替代路径。奖励必须与实际完成边界对应：模拟/提案只给信息与 evidence 进度，受限试点只给试点结果，准入/许可只给其声明权利，物理接收和相应里程碑才给交易用途或交付 progression。

平衡与滥用风险至少包括：

- `grind_only`：玩家反复提交模拟/投票只换取进度，缺少新的决策、能力或恢复弹性。
- `pay_to_qualify`：OC 持有量直接成为世界资格，绕过贡献、证据、准入或目的地条件。
- `permanent_lock-in`：许可、版税或市场优势没有到期/开放/替代路线，创新者变成永久垄断者。
- `information_fiction`：陈旧报价被展示为实时库存，或者全球发现掩盖路线、损耗、容量和时延。
- `escrow_bypass`：报价、链上转账或客户端 receipt 直接触发交付奖励/最终结算。
- `route_multiplication`：重连、重试、并发争议或旧 receipt 重复释放资金、库存、奖励或需求减少。

验证样例必须证明创造路线不会成为首局唯一解，跨区物流不会取代本地专业化，失败后仍有独立可选路线，且玩家能判断继续投入是否值得。受治理 recipe 的首版范围、普通期限与有限处置采用 [§3.3](#governed-recipe-lifecycle-policy-v1)；紧急保供的首版阈值、份额和资金限制只采用 [§6.4](#emergency-supply-policy-v1)；有限版税与开放只采用 [§4.1](#industrial-license-policy-v1)及其收益/处置规则；其他成本、常态价格和奖励数值仍由相应专业 authority 定义。

## 8. 专业验收

<a id="ac-game-018-01"></a>
### AC-GAME-018-01：工业创建阶段形成可恢复的玩家循环

Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313；承接 [AC-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009) 的四阶段玩家选择与可恢复结果。

给定一项新工业能力机会，玩家能够依次读取可审计提案、确定性模拟、受限原型/试点和治理准入的差异，比较继续、补证、缩小范围、延期、停止和既有路线；任一阶段均不自动取得世界能力。输入缺失、模拟不一致、试点失败、准入拒绝或条件漂移时，结果保留原因、投入/损失、scope、下一复查和安全替代，不产生第二次世界效果。

<a id="ac-game-018-02"></a>
### AC-GAME-018-02：许可和开放奖励受范围、期限与公共边界约束

Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47；承接 [AC-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-010) 的有限许可、真实收益、到期开放与玩家选择。

给定一个获准工业能力，玩家能看到用途/区域、期限、版税/收益、维护、撤销/争议与到期/开放条件，并在继续许可、续期、开放、替代路线之间作出有机会成本的选择。到期或开放条件达到后按规则开放；许可不能永久封锁、覆盖玩家独立资产/身份或扩大到未准入范围。重复、旧缓存和待决请求不延长、复制或追溯改写许可。

<a id="ac-game-018-03"></a>
### AC-GAME-018-03：OC 转让与世界资格保持双向语义隔离

给定 OC 链上转让、世界资源/信用、工业准入或区域资格的任一组合，玩家能分辨各自的成功、失败、条件和下一步；OC 持有/转让不自动取得世界资格，世界资格也不自动铸造、转让或兑换 OC。任一侧失败都不抹掉另一侧已确认的 receipt，不产生绕过资格、治理、竞争或外部价值承诺的旁路。

<a id="ac-game-018-04"></a>
### AC-GAME-018-04：市场发现、报价和物理交付有新鲜度与状态切线

Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2；产品适用承接：[REQ-SC31-009](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009)。 本条与该产品要求的关联仅覆盖跨区情报、物流/接收、escrow 及对应读面；工业创建、许可准入、OC 资格与其他 progression 义务仍按原专业条款独立适用，不由该关联代签。

给定跨区供给/需求，玩家能看到情报来源、时间戳/新鲜度、不确定性、报价/合同、路线、时间、损耗、目的地准入与容量，并区分发现、承诺、出发、在途、到达、存储和用途。陈旧或私有情报不会伪装成实时事实；前序状态不能表示货物已到达、需求已满足或最终结算。

<a id="ac-game-018-05"></a>
### AC-GAME-018-05：Escrow 里程碑和争议可恢复且 exactly-once

Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2；产品适用承接：[REQ-SC31-009](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009)。 本条与该产品要求的关联仅覆盖跨区情报、物流/接收、escrow 及对应读面；工业创建、许可准入、OC 资格与其他 progression 义务仍按原专业条款独立适用，不由该关联代签。

给定一项带 escrow 的跨区合同，玩家能看到每个里程碑的物理/接收条件、可释放或保留的资金、证据窗口、争议状态和下一步。只有绑定事实满足时才产生一次里程碑结算；延误、损耗、拒收、争议、重连、重试、并发到达和 replay 不重复释放、扣款、发奖、减少需求或改写已结算历史，并保留合法的补救、退回、改道、重谈或安全停止路径。

<a id="ac-game-018-06"></a>
### AC-GAME-018-06：跨 surface 玩家语义和 progression 边界一致

Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2；产品适用承接：[REQ-SC31-009](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009)。 本条与该产品要求的关联仅覆盖跨区情报、物流/接收、escrow 及对应读面；工业创建、许可准入、OC 资格与其他 progression 义务仍按原专业条款独立适用，不由该关联代签。

同一权威快照下，Viewer、pure API 和 Agent 对提案阶段、准入范围、许可状态、OC/世界资格、情报新鲜度、物流/escrow 状态、primary blocker、机会成本、`next_action`、`next_recheck` 和 `progression_effect` 给出等义解释。推荐不能替玩家创建新能力、改道、争议或完成结算；缺 authority 时统一返回 `incomplete / unknown / blocked`，不能用默认值补齐。

<a id="ac-game-018-07"></a>
### AC-GAME-018-07：授权、处分与稳定范围同时完整

Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965；承接 [AC-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-002) 的能力范围及 [AC-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003) 的既有能力处分。

给定 §3.3 的 recipe/工厂请求，当前 authority、exact 内容、稳定范围身份、工厂集合、期限、允许操作及既有能力完整处分必须共同成立。缺处分或改变顺序/共同边界的管理性请求整项不生效，不能先停止新发布再补处置；改 module/Profile ID、实例或入口不能逃离同一治理范围。授权与四阶段、recipe、PPE、工厂控制和适用许可均可独立拒绝，不互相代签。

<a id="ac-game-018-08"></a>
### AC-GAME-018-08：待决发布与激活消费当前授权

Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911；承接 [AC-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-002) 的批准后授权漂移及跨入口重试。

给定已批准但未完成发布/安装/激活的同一 recipe 请求，实际提交时必须重验当前授权、范围与世界前置。到期、撤销、收缩、替代或内容不符只能得到明确拒绝、过期、取消或显式重提，不能继承旧批准、从缓存恢复权利或留下部分可用状态；直接入口、旧请求、重连与合法发布路径给出等义结果。

<a id="ac-game-018-09"></a>
### AC-GAME-018-09：全部真实效果入口遵守当前范围与期限

Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965；承接 [AC-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-002) 的实际准入和 [AC-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003) 的既有能力未来效果。

同一授权条件对模块/native 排程、首 sink、tick 后继完成及恢复待决动作同时有效。`tick >= end_tick` 的普通效果被拒绝，不以清理顺序、先前 acceptance 或客户端时间继续生产；管理性变更按权威提交前后区分历史与新效果。受限/退出的同一受影响 scope 也不得新增发布、安装、激活或扩权，未受影响的 `F2` 仍按原合法条件运行。只挡 planner 却允许新技术准入、原生别名、自动 completion 或旧 pending 继续生效，应判失败；技术发布 receipt 和旧能力缓存均不授予后续权利。

<a id="ac-game-018-10"></a>
### AC-GAME-018-10：有限退出保留历史、资产与未结责任

Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965；承接 [AC-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003) 的最小处置与历史保留。

受限制或退出时，原五类存量工作及新排程均有明确判断；最小动作须有原批准、当前合法 authority、具体原工作/数量、资源及不晚于 §3.3 截止的有限处置。不能新增批次、返还已耗投入、删除独立资产/身份、改写历史合同或以临时处置恢复生产。无合法出口、预算不足或到达截止时 `exit_blocked` 保留原有限量与责任；重复、重启或申诉不续期，未结清不得显示安全停止。

<a id="ac-game-018-11"></a>
### AC-GAME-018-11：替代先结清后原子切换且保留非重叠范围

Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965；承接 [AC-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003) 的重叠单一可执行授权、旧请求处置与非重叠连续性。

§3.3 正常例须证明 `G1` 在 pending 时不能执行，`F1/J1` 全义务结清后才可一次提交使 `G1/F1` 生效并移除 `G0/F1`，`G0/F2` 保持原条件。未结 WIP/终端义务、scope/authority 漂移、先新后旧的半提交或同时 drain/生产均拒绝。旧未确认请求明确拒绝或由原请求人重提，旧 job 不迁移、重编号或复制投入与收益。

<a id="ac-game-018-12"></a>
### AC-GAME-018-12：有限预算覆盖完整影响闭包

Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965；承接 [AC-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-002) 的入口一致性和 [AC-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003) 的完整存量处置。

首版在 2 工厂、每厂 1 未闭合 job、每 job 2 批上界内，完整列出受影响实例/消费者、权威待决请求、原工作及 custody/终端义务；声明为空也须有权威依据。完整调用、生产、验证/退出和逐 payer 资源保留均落在有限预算内。漏入口、未知工作类型、存在本 profile 不支持的物流/迁移、截取前 N 项、闭包不完整或费用不足时不启用，不部分发布 cutover，也不临时上调限额或挪用退出保留。

<a id="ac-game-018-13"></a>
### AC-GAME-018-13：重复与恢复不复制或复活授权效果

Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965；承接 [AC-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-002) 的重复/跨入口新准入和 [AC-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003) 的替代及存量效果恢复。

并发、重试、重连、快照恢复和 replay 只能读取或恢复原权威结果，不重复扣费/发料、创建第二授权、延长处置或重开旧工作。提交未知先查询原 intent/receipt 与完整持久 generation；不拼接半个替代，不重新运行历史模块或当前规则重判已确认事实。已知未生效 pending 在当前权限下才可继续，历史授权不能覆盖新限制。

<a id="ac-game-018-14"></a>
### AC-GAME-018-14：玩家读面区分历史、当前权利与待决结果

Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965；承接 [AC-WR-GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-002) 的拒绝/过期/重提结果和 [AC-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003) 的既有能力与替代/退出可读性。

同一权威状态下，Viewer/API/Agent 展示等义的原 authority、理由、当前允许/移除范围、期限、存量责任、替代 pending/有效事实、blocker、`next_action` 与 `next_recheck`。发布成功、许可成立、当前可执行、旧工作完成和替代生效分别对应其原证据，不能互相推断。处置待决不写成安全停止，新授权 pending 不写成 ready；缺事实统一显示 `incomplete/unknown/blocked`，保留合法等待、重提、申诉或独立路线。

以上验收是目标玩法合同，不是当前实现或 release 证据。`test_tier_required` 应覆盖每一阶段的可读预览、fresh revalidation、原子拒绝、替代路径、OC/资格隔离、情报过期、路线失败、escrow 里程碑与重复请求；受治理 recipe 还须覆盖完整处分、普通期限终点、原有限处置、先结清后替代、真实消费者闭包及其独立正常/反例。`test_tier_full` 再覆盖跨节点授权、持久化、并发、replay、争议恢复和跨 surface parity；AC-GAME-018-07..14 的具体记录按[生命周期核验手册](../../testing/manual/governed-extension-lifecycle.manual.md)逐项区分设计、当前实现和未运行/blocked。

## 9. 权威边界与证据切线

| 语义 | 本合同拥有 | 其他 authority |
| --- | --- | --- |
| 玩家循环与动词 | 提案/模拟/试点/准入选择、许可取舍、市场发现/路线/交付/争议动作、收益和失败恢复 | 产品专题拥有世界规则和玩家承诺边界 |
| 工业与市场事实 | 只定义玩家需要理解的阶段与完成边界，不定义配方、价格、订单簿、物流、库存或 escrow 算法 | `world-runtime` / `world-simulator` 拥有确定性执行、资源、物流、状态、receipt 和 replay |
| OC 与身份 | 明确 OC 链上转让与世界资格不可互推 | `p2p` / blockchain authority 拥有链上资产、身份、签名和分布式状态 |
| 许可与治理 | §4 唯一定义首版收费单位/有限期限、收益分配、通知与有界争议、开放及玩家选择 | 产品层拥有世界治理、公共安全和长期经济总原则；Runtime 拥有当前权限、资金、计量、程序与恢复，P2P/身份 authority 证明原合法来源 |
| 受治理 recipe 生命周期 | §3.3 定义有限工厂/工作范围、普通期限与最小处置窗口、先结清后替代及玩家结果；不重新定义 recipe 六态或许可收费 | 产品 GR-002/003 拥有一般扩展完整义务；Runtime 拥有稳定范围/消费者映射、当前授权、闭包、预算、原子提交和恢复；原治理/身份 authority 证明合法来源 |
| 紧急保供政策 | §6.4 唯一定义初始品类、危机阈值、优先级、配给、窗口、预算/补偿与退出规则 | 产品 ES-001..004 拥有保护边界；runtime 拥有版本化授权、需求账本、原子结算与恢复接口；P2P 证明既有治理权限和权威顺序 |
| 表达和验证 | 规定玩家应看到的原因、机会成本、下一步和 parity 目标 | Viewer/API/Agent 拥有表达实现，QA 与 `doc/testing/prd.md` 拥有验证与 release 判断 |

当前代码、局部 fixture、单项历史 green、产品迁移完成或本 PRD 本身都不能单独把本合同标为 current playable、balanced、available 或 released。只有对应专业任务在同一候选上提供 fresh implementation / QA / playtest 证据，并通过统一 claim gate，才能更新公开状态。

## 10. Non-Goals

- 除 §3.3 的有限 recipe 授权/退出、§4 的有限许可和 §6.4 的保供首版政策外，不规定配方、材料、产率、常态价格、税费、版税数值、汇率、订单簿、库存、物流寻路、损耗公式、路线费率或通用 escrow 释放公式；这些有界政策不反向改写常态合同。
- §3.3 不把单 recipe/同 artifact 授权替代扩写成一般扩展迁移、付费许可 rebinding、全类型既有实例停机或全部 GR-002/003 已落实；不赋予原消费者尚未支持的 WIP 暂停、物流、转换、迁移或任意安全处置。
- 不实现工业提案、确定性模拟、试点、治理准入、许可、市场撮合、物流、目的地存储、结算、争议或任何 runtime/API/schema/action。
- 不把模型生成、投票、链上持有、客户端 receipt、报价或历史先例变成世界写入、玩家资格、最终交付或 reward 旁路。
- 不新增第五产品模块、不改变现有目录 slug 或产品 PRD-ID，不扩展 `PRD-GAME-015`/`016` 的 authority，也不替代常态市场与紧急保供产品专题。
- 不宣称外部兑换、支付服务、OC 价值、投资收益、合规资格、当前可用性、发行或 release readiness。
