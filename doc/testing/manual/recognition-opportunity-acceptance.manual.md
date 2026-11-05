# 成熟世界认可与有限机会验收手册

- 生命周期：`active`；Owner role：`qa_engineer`；协作：`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`、`blockchain_ops_engineer`；Last reviewed：2026-11-04。
- 上游：[REQ-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)、[AC-WR-MW-004](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004)；专业依据：[成熟世界认可与有限机会](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-recognition-opportunity)及 AC-RO-01–08。
- 技术依据：[认可与机会结算设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md)。本手册拥有成熟前态、单次机会、原物流消费者、恢复与跨入口的可执行验收方法，不重新定义认可政策、队列次序、经济参数、原权限或物流结算。

Trace refs: O7-REQ-33e8fa74-7e9d-45ae-82e9-93274f84f026。下列方法区分静态审读、受控 fixture、正式来源、三入口运行和真人理解；本文件存在不表示任一运行通过。

## 1. 固定前态与证据层次

审读基线为 `25ae2095e67e00179cc1e46bb8d2f6a428b9055a`。每次执行另记 source HEAD、tested tree/build、world/branch、逻辑时间与事件读点、profile/规则版本、当前 issuer/beneficiary/reader/executor、owner 与授权 generation，以及实际使用入口。保存原输入、原事实引用、操作身份、逐账本前后量、当前结果和独立 oracle；未知的操作结果先按[原请求恢复](../../../testing-manual.md#request-recovery-regression)回读，不以重新生成请求替代。

| 证据层 | 能证明什么 | 不能替代什么 |
| --- | --- | --- |
| 源码与合同静态审读 | 原物流动作、权限检查、容量、扣料、运费、时长、损耗和去重分支存在；目标组合有明确定义 | 认可 producer、统一分配器、接口或完整玩法已经实现 |
| 受控 fixture | 在标明直接安装 authority/profile/库存的隔离前态中检查状态转换和反例 | 正式鉴权、成熟历史、供应方真实审核、合法来源或真实玩家体验 |
| 正式来源及 authority | 用原合法动作形成历史与资源，原主体产生审核/授权，实际协议形成可回读结果 | 只靠最终 DTO、截图、fixture 内部写事件或普通 receipt 自动补齐来源 |
| 同候选运行及真人理解 | 同一个 tested build 的 Pure API、Agent Client、Viewer 各自运行证据，以及真人决策理解 | 文档审查、共享 DTO、一个入口成功或模型自述 |

缺正式 producer、共享 capacity authority、generic 免费 inbox/读面、制度执行费用保护及同根结算能力、接口或可达前态，相关运行层记 `blocked`；能力具备但本次未运行记 `not_run`。证据冲突或来源覆盖不全保持 `pending/unresolved`。不把已有测试定义、历史 round 或静态算例报告为本次 PASS。

| 可复用的真实接缝 | 本手册的检查边界 |
| --- | --- |
| [`Action::TransferMaterial`](../../../crates/oasis7_agent_api/src/world_action/action.rs)、[原 action evaluator](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_core.rs) | 原请求者、source/destination、amount、显式 route_ids 与 auto_reroute；原材料、Electricity、路径及 global inflight 准入仍须成立 |
| [原路径与 quote](../../../crates/oasis7_runtime/src/world/logistics.rs)、[时间与损耗](../../../crates/oasis7_world_state/src/world/pure_helpers.rs) | quote 不预留容量；显式多跳路径的距离、运费和损耗按原规则计算。空 route 的 legacy direct 动作不能被冒称使用了 R |
| [原 transit reducer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs) | start 扣 source 材料与原运费、占各 hop；complete 验证 job 原承诺、入 destination、支付 route owner 并释放占用。`prepare_started_with_reservation(true)` 只跳过源材料再次扣除，仍增加 route reserved，不能当作目标 `L→T` 容量转换 |
| [原 Agent 委托检查](../../../crates/oasis7_runtime/src/world/agent_delegation.rs)与[原 owner 委托控制](../../../crates/oasis7_runtime/src/world/owner_delegation_control.rs) | 认可不代签 Transfer grant；A/B 必须各自有当前合法执行资格，从本人 Agent ledger 出料。共享 hub 的可见库存不等于二者拥有该库存 |
| [原 canonical stable-line helper](../../../crates/oasis7_world_state/src/state/industry_transition/mod.rs)、[原 recipe completion](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs) | 本 profile 的成熟 witness 取本人原工厂稳定线历史：相同 canonical snapshot 的 repeat count 至少3及其连续完成窗口；当前字段或直接设置 count 不能代替当时实际接受/完成及个人归属证明 |
| [物流网络测试定义](../../../crates/oasis7_runtime/src/tests/economy_priority_logistics_network_tests.rs)、[网络完整性](../../../crates/oasis7_runtime/src/tests/economy_priority_logistics_network_integrity_tests.rs)、[工业路径 authority](../../../crates/oasis7_runtime/src/tests/economy_priority_logistics_recipe_path_authority_tests.rs) | 可定位原多跳、完成和工业接缝，直接设置的输入须标为 fixture；不能据此宣布目标认可 producer 或全部容量 writer 已接入 |
| [工业 start](../../../crates/oasis7_world_state/src/world_rules/industrial_work_start.rs)、[保供分支](../../../crates/oasis7_world_state/src/world_rules/supply.rs) | 前者将 route/path 放入 RecipeStarted 引用，不能据此当作新增 R 占用的 writer。后者虽有 DispatchSupply→reserved transit 分支，但当前 `valid(...)` 固定 false，AuthorizeSupply 也拒绝该物料路径；预装 auth/batch 仍不构成当前合法可达容量反例 |
| [原 module metering](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)、[原 fee arithmetic](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)与[目标结算设计](../../world-runtime/runtime/recognition-opportunity-settlement.design.md) | generic 只读/inbox 不运行 sandbox；实际制度审核、分配和 Use disposition 仍由受治理 WASM 执行并保留原计费。原 artifact-owner payer、费用预算保护、同根原子提交及新 producer/adapter 分别取证；不能把 L2 规则迁入 native 来免计费，也不能把原付费 WASM 当作免费 pending |

## 2. 正常样例与独立结果账

### 2.1 成熟历史与原供应方审核

在已有组织、治理和历史的隔离成熟世界中，先保存 A、B 各自完成首次持续能力的原 canonical 证据：本人合法 FactoryBuilt、当时 builder/control 来源、同一 canonical `FactoryProductionSnapshot` 的至少三个连续完成 job 及完整窗口，按原 stable-line helper 判断；后续当前状态不能改写这段已成立的历史。供应方 O 在本次贡献开始前声明技术设计中的 `local delivery program`、本地用途/范围、认可审核和机会规则。A/B 读取已声明规则，随后通过原合法动作完成适用交付；历史须能从本人授权、source 库存来源、原提交追到 `MaterialTransitStarted`、`MaterialTransitCompleted` 和实际 destination 增量。单个 receipt、世界整体阶段或启动时直接设置的里程碑不能证明本人已完成首次持续能力；不能在交付完成后补写 program 或提高已接受条件。

再由 O 的当前合法控制者在原可复核审核 authority 下显式核验上述 A/B 历史，分别形成认可来源。记录 program、规则版本、被审核原事实、决定者、当前授权、开始/到期时间和复核入口。普通物流完成 receipt 不会自己授予认可；没有正式 O 审核 producer 时，只能构造标明来源的目标 fixture，正式层为 `blocked`。拒绝用 meta points/tier、社会声誉或已有运输成功替代该 producer。

在审核及当前权限均有效、既有 transit 已完成后冻结 C0。A/B 对应两个 distinct 且当前合格的 canonical beneficiary，分别绑定本人的贡献 Agent；两者都满足同一 occurrence 的认可、用途、地域、期限、权限和反滥用条件。两者拥有各自的 Transfer 执行资格。选取实际 canonical beneficiary ID 排序为 A 在 B 之前的样例，使同龄闭合轮次的正常胜者可独立确定；别名和请求 ID 不参与排序。若真实 ID 顺序相反，交换样例标签并保留原 ID，不篡改 authority。

### 2.2 固定路线与账本

O 是三条真实 route 的原 owner，且与 A/B 不同；本例各次原 `RegisterLogisticsRoute` 的 requester 均为 O，不能由 A/B 登记 feeder 却把其 owner 收益算给 O。A→H、B→H 的 feeder 与共同 R=`site:H→agent:O` 组成两条显式合法的两跳路径。H 是路径中转节点；本次 Transfer 的 source 仍分别为 `agent:A`、`agent:B`，destination 均为 `agent:O`，不得改从 H 出料。R 的物理容量为 2；O 为同一个 opportunity occurrence 仅声明 1 个有限机会单位。发布和 pending 均不占容量，结算才增加未使用 allocation `L`；使用时由同一事务转换为原 transit 占用 `T`。物理容量与一次性 occurrence 分别对账。

| C0 的固定项 | 正常值与来源要求 |
| --- | --- |
| A/B 的本次可用材料与电力 | 各 `steel_plate:1`、Electricity 至少 3；保存原余额和合法来源。其他既有材料/产物保留基线，不计入本次交付 |
| O 的制度执行资金 | O 同时为原 route issuer 与已声明的 module artifact owner，显式授权从自身真实 Data/Electricity 保护 operational 与 closure 费用预算；分别保存 gross/held/available/已花费。按技术预算和原 metering 取足额前态，不依赖尚未收到的未来 tariff；保护 adapter 未实现时为 `blocked` |
| feederA / feederB | 各 100 km、capacity 至少 1、tariff 每单位 1 Electricity、available；由 O 按原合法来源登记 |
| 共同 R | 100 km、capacity 2、tariff 每单位 2 Electricity、available；无既有 T/L，占用来源完整 |
| 两个实际转移动作 | 各 amount 1、kind=`steel_plate`、显式 `[本人的 feeder, R]`、总 distance 200 km、`auto_reroute=false`；路径以原 quote/evaluator 重验 |
| 原 profile 与时间 | 无额外 loss profile，baseline 为每 km 5 bps；100 km/tick，global inflight 上限 2；所有既有 job 在 C0 已结清。数值来自原源码及本例输入，不是新认可经济参数 |
| opportunity | 同一个 occurrence，仅 1 个单位，绑定 R、O 的 program、本地用途、期限和当前 profile；A/B 均可合法竞争 |

从 C0 建立互不污染的正常与负例分支。需要直接 `set_*` 或注入状态时只报告 fixture；正式层用原合法来源达到等价前态，正式 route 登记/授权入口缺失也须单独 `blocked`。取证保存每条 route 的实际 owner 和当前材料 profile；正式世界有 Low/Medium/High loss class 时按原 factor 1/2/4 重算，不能删除 profile 来迁就受控例。若 feeder 由 A/B 登记，各自原 owner 回款1、O回款2，须另记为不同输入的 oracle，不能套用下表 O 独收3的账。冻结后不得补料、补电、删除 job、复原旧权限或改 capacity 来帮助目标结果。测试预算、轮次上限、分页/批量/重试和历史保留边界均引用[技术设计的唯一预算表](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-budget)，本手册不另定数值。

### 2.3 正常执行顺序与独立 oracle

先让 A/B 只读认可与机会，再显式提交。使用技术设计定义的有限闭合轮次、`first_eligible_round` 升序及稳定 canonical beneficiary tie；必须包含同 capacity 域的全部合格原候选和既有占用，不能先运行一个仅含 A/B 的私有优先池。该正常分支的完整清单确无其他候选，A/B 同龄，故 A 取得唯一 receipt。B 得到冲突原因和独立下一步。

随后由 B 显式通过原普通 Transfer 入口使用 R 剩余 1 个物理单位；A 再显式使用 receipt。B 的原合法动作不要求认可。分别保留 pending request、settlement operation、receipt、use operation、原 action/job、start/complete 事件与账本差分的完整关联，不能只数最终 receipt 数量。

| 观察点 | 独立预期；所有增减均相对 C0 |
| --- | --- |
| generic 查看与两个 pending inbox | R `T=0,L=0`，不扣 A/B/O 的材料、Electricity、Data、OC，不新增执行费或改费用 hold；没有 transit、分配或排他效果。允许的 inbox 元数据不等于预先占槽；正式 program/review 是 C0 前另有资金与计费记录的制度决定 |
| A 原子结算成功 | 同 occurrence 只有一份有效 receipt；R `T=0,L=1`。尚未扣运输材料/运费，未开始运输；B 无 allocation、申领费用或第二 receipt。O 对该已接受制度决定的原执行费用从保护预算与效果同根结算，只计一次 |
| B 显式普通 Transfer 开始 | R `T=1,L=1`；feederB `T=1`，B 材料−1、Electricity−3，O 尚无本次新增材料/运费收入。B 的原 job 尚未到站 |
| A 显式 Use 成功 | 同事务 R `T=2,L=0`，feederA `T=1`；A 材料−1、Electricity−3；各自一份原 job。O 的原制度执行费同根单次结算；不能先释放 L 让他人插入，也不能保留 L 后再增加同一 T |
| 原 ready_at 前 | 每个 job 的 ready_at 为各自 start 逻辑时点加 2；未到期 job 不可提前完成，O 不得凭 receipt/ACK 先得材料或运费 |
| 两个原 job 各完成一次 | 各 `floor(1×200×5/10000)=0` 损耗，O 各收材料1；各原运费3按 feeder1+R2归 O，governance tax 为0。O 运输收入合计 Electricity6，扣本次真实制度执行费后的净量见 §2.4；A/B 各净材料−1、Electricity−3，H 无本次材料增减 |
| 已完成后的正式最后关闭 | 按技术正常闭包执行可合并的制度终态/余费关闭，先同根计入准确 terminal fee，再释放未用 operational/closure 余量；不能把所有保护预算直接扣成费用，也不能先释放余额再找钱完成最后关闭 |
| 完成、关闭后的 replay | R 与两个 feeder 的本次 T 均释放，L 为0；原 job/receipt、制度费及关闭去重，材料、运费、执行费和完成数不再增加。同 occurrence 的一次性结算记录仍保留，R 再空闲也不重发机会 |

oracle 从原动作参数、路径/owner/profile、原 job 身份及逐账本事件独立计算，不从被测机会服务返回的 cost、winner、剩余容量或状态摘要复制预期。正常顺序须在 B 的 ready_at 前完成 A Use，以实证同时 `T=2`；若环境无法在该边界观察，记录缺失证据，不假装已验证该中间态。传输时长是逻辑 tick，网络、轮次和人工操作时间另记。

### 2.4 原制度执行费、保护资金与净额

记录每次实际已接受的制度决定对应的 WASM/manifest hash、artifact owner、原输入字节数、经验证输出的字节数、effects/emits 数量、`new_state` 是否存在、业务键、同根提交及 `ModuleRuntimeCharged`。独立按原 fee arithmetic 计算：`units(0)=0`，其他字节数为 `ceil(bytes/1024)`；Data 为 `units(input)+units(output)+2×effects+emits`，Electricity 为 `1+effects+emits+has_new_state`。保留源码的饱和计算语义；输入/输出和调用次数来自原件，不从被测 cost 摘要反推，不猜固定单价或假定一次查询恰好一次模块执行。

在 C0 之后，把所有实际接受的本 profile 执行费用分别记为 `ΣexecD`、`ΣexecE`。最终 O 的材料净量为+2、Electricity 净量为 `+6−ΣexecE`、Data 净量为 `−ΣexecD`；原 world 对应 resource treasury 分别增加 `ΣexecE/ΣexecD`，不得把费用 burn、藏入 transport tariff 或转嫁给 A/B。C0 之前的发布、审核及历史交付费用单列原件，不通过重置基线抹去。正常的 transport tax0 与制度执行费是两条独立账。

费用 hold 是可用余额限制，不是已花费或另铸资产。逐阶段核 O 的 gross、operational hold、closure hold、available、真实 spent 和 terminal release；来源必须为当时自身真实资金。保护预算按[目标资源 hold](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-resource-holds)和[目标费用预算](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-fee-budgets)的通用接缝及本 profile 预算承接，不能把目标 adapter 当作已实现入口。仅进入 generic inbox 的新请求，以及 stale、无效果拒绝/等待、重复 retry 均不扣费；已接受且实际改变制度状态的决定依原计费同根提交一次，不通过把无效果拒绝改名为“决定”收费。若闭合轮次真实接受并持久化首次有效等待身份或整轮结果，即使没有分配 receipt，也须按 accepted 制度变化独立计 O 的原费；不能只凭 `deferred` 标签推定免费，更不能把该 L2 写入挪给 generic inbox。unknown 先查原业务键和原 fee，不重新执行计费。

| O 的资金/费用反例 | 独立预期 |
| --- | --- |
| 发布/激活前真实资金小于原 manifest 与本 profile 所需 operational+closure 上界 | 不激活、不先承诺 L 或借未来 tariff 凑款；既有资金/预算无部分安装。调用数量、上界和 closure 余量从技术唯一预算及原 manifest 独立计算 |
| 激活后 O 用其他合法支出耗尽非保护余额，再尝试侵占 operational/closure | 所有真实余额 writer 均保护 hold；新支出不能花掉保留款，不能通过另一个 module/普通转账绕过。禁止用当前 gross 代替 available 判可付，也不能从 A/B 临时扣款补 O |
| 原可用余额耗尽、operational 达声明结束边界，仍有未使用 L 要 expiry/abort | 原 closure 预算足够执行已承诺的终态处置，真实原 WASM 费与单次 L 释放同根提交；不因 O 无自由资金泄漏容量。用合法支出/原预算边界建立前态，不删除 hold 或直接改余额伪造可达性 |
| accepted 制度提交丢响应，或在预算消费/费用入 treasury 时注入提交失败 | unknown 先查原 op/fee/hold，成功最多一份 debit/treasury credit；已证 precommit failure 无半份 state/fee。重连、拒绝和重复调用不再次花 operational/closure，也不把 closure 已用款重新视为可释放余额 |
| artifact owner、manifest/原费率或 O 的当前控制变化 | 新 intake 按当前权限处理；旧 program 必须仍可按原明确授权、真实 payer/source 与足额上界 bounded drain。不能换 owner 后继续无权扣 O、默认新 owner 代付或遗失 closure；破坏原收尾能力的切换须受阻 |

正常场景采用[技术唯一预算表及正常闭包](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-budget)中明确的 beneficiary/round/review 程序与 manifest，独立复算 operational/closure 成功槽和最坏费用向量，另记录实际发生的调用；保留未使用复核/终止槽只恢复可用量，不成为收入。若环境需要额外强制模块调用或真实编码超界，先按原规则重新确定完整预算，否则该候选 `blocked`，不能只沿用受控正常调用数。所有预算保护、余额 writer 和 closure 路径都是目标能力的执行门槛；若当前只有原收费模块而无这些接缝，相关运行 `blocked`，不得只验证成功扣费就宣称费用生命周期闭合。

<a id="recognition-opportunity-01"></a>
## RO-01：成熟阶段与可达来源

承接 [AC-RO-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-01)；依据 [recognition-stage](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-stage)。

1. 正常分支分别核 A/B 本人的首次持续能力证据、后续原交付历史和 O 的显式审核；世界成熟不替代本人阶段，O 的认可决定不替代历史。逐项保存来源读点及完整覆盖，不以窗口内刚好一条记录声称完整历史。
2. 从等价受控前态独立移除首次持续能力证据，或仅保留首局成功、starter grant、一个普通 receipt、meta tier、环境活动。预期不得形成本叶认可/机会；原独立动作若合法仍可继续。
3. 历史源未启用、记录分页未完、读权不足、读取失败或只有旧缓存时保留实际原因与 `unknown/unresolved`，不得解释为“零贡献”或默许满足门槛。直接注入历史或审核只记录 fixture，不升级正式前态结论。
4. 独立取仅两次同 snapshot 完成、第三次更换 snapshot、只设置 repeat count 而无原 job 的反例，不能据此形成该首版成熟 witness。反向取已经完成原稳定线、后来合法换配方使当前 snapshot/重复窗口改变的主体；可核验的原历史不应被当前 `current_stable_line` 的变化或后续受阻抹去。缺历史 adapter 时为 `maturity_source_unavailable`，不能依当前 summary 猜历史。

<a id="recognition-opportunity-02"></a>
## RO-02：认可事实、当前权限与窄作用域

承接 [AC-RO-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-02)；依据 [recognition-authority](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-authority)。

对正常认可逐项回读 program/规则版本、决定者与审核授权、原交付事实、beneficiary、本地范围、用途、开始/到期及复核入口，并单独读取当前请求/执行权限。A/B 获认可不意味着拥有 H 库存、其他人的 grant、OC、资产、治理或地区控制权。两个 contender 必须都通过同一机会的实际条件；用一个无权主体作为“败方”不能通过竞争验收。

分别改变 issuer、beneficiary、world/branch、region、用途、program/recognition revision、当前 grant 或时间边界；要求来源绑定不符、已失效或待复核有明确结果，不继承历史响应的可结算结论。单独取贡献开始后才发布 program、事后换用途或修订条件的反例，原交付不得被新版本追认成当时已经接受该 program 的贡献；保留原版本与未结义务。缺原审核、权限证明或范围字段时不得由模型、Viewer 当前点击或普通成功 receipt 补签。读取不属于当前 reader 的审核事实时，不经数量、失败详情或存在性泄露受限记录；公开机会说明与私有证据按原读权分别取证。

另从原个人已接受并完成交付的历史分支开始，通过原 authority 把执行 Agent 的当前控制转给另一个个人。后者即便本人已成熟且当前合法控制 Agent，也不得用 current owner 把旧 source 归到自己名下。审核须核原运输接受时的 beneficiary/owner generation 及其权威历史绑定；缺该来源为 unavailable/blocked，不能仅由相同 Agent ID 和现 owner 推导旧贡献归属。该反例不否定新个人以后按新 program、当前权限完成自己的合法贡献。

<a id="recognition-opportunity-03"></a>
## RO-03：预览、pending 与免费协议边界

承接 [AC-RO-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-03)；依据 [opportunity-preview-pending](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-preview-pending)。

在同一读点重复查看认可、预览、邀请、推荐、资格说明，先对比完整业务账本，再显式提交到 generic inbox 并进入有界 pending。只读阶段不能写资格、排队 age、顺位、优先级或 allocation；提交后的 inbox 记录按设计有界，pending/hold 仍不占槽、不排他、不形成 receipt，也不后台启动收费 sandbox。检查材料、Electricity、Data、OC、费用 hold/spent、transit/job、route T/L、资产与治理状态，而非只检查前端显示余额。随后真实的 institution 审核/闭合轮次/分配按原 WASM 规则和 O 的受保护费用预算取证，不将其伪装成 pending 免费内部调用。

| 独立反例 | 必须观察的结果 |
| --- | --- |
| 反复刷新、多个客户端查看、断线后重新打开 | 不取得更早 `first_eligible_round`，不插队，不重新发布机会或重复结算 |
| 仅进入 pending/hold 后普通合法需求到达 | 当前物理容量仍可按原共享规则竞争；不可预先给 A/B 排他单位或隐藏优先级 |
| 用当前 governed WASM 写状态来接受 generic pending，导致 artifact owner 被扣 Data/Electricity | 不能因 A/B 未付费就判零效果。免费 read/inbox 必须不跑 sandbox；也不能把制度审核/排序/分配移入 native 以绕过原 metering。缺正确 adapter 则 profile 不启用、运行 `blocked` |
| pending 名义下增加 route reserved、扣料、escrow、收费或预发可用 receipt | 判失败，不能用“临时占用”解释；原运输 fee 只在真正的原动作 start 发生 |
| 完整队列、请求预算或来源扫描超界 | 按技术唯一预算表返回有界待决/限制/未知及下一步；不能截断候选后偷偷选 winner，也不能无限 retry/扫描 |

预算逐维取合法边界内、边界值和超界一项的独立输入，核编码字节、全部证明/字符串、source job、完整候选/route/resource/权利集合、读条数/解码字节、claimant 及有限轮次边界。混合类别按同一域的合计计量，不拆成多份局部请求绕过上限；typed request、旧 instance state、host 原件及证明全部计入制度输入，不能只核最外层 request 字节。超界整轮无新分配且不让先到的前 N 人获利；按技术规则保存 bounded overflow，不能不登记额外合格请求来伪造完整集合。停止 intake 时仍可按原 key 回读、到期释放和完成既有 L/T/义务，不以预算耗尽删除历史。

单命令制度调用次数、确定性 gas/memory/output/effect/emit、source 获取时限及 closure 调度保留槽按同一技术预算逐项核。取消、超时或失去读权停止前置 source 读取并保留 unavailable；墙钟保护不成为 canonical 分配规则、不延长机会时间。已可能提交时仍查询原 op，不把取消响应当成回滚；已接受原 transit 和有保护预算的 closure 继续按原规则完成。可选 review/round 不能用完原收尾调度能力。

本 profile 不对 A/B 收取认可、申领或分配的额外业务费；generic read/inbox 零执行费，正式制度决定沿原 WASM metering 由已声明的 O 支付，真实运输另按原 tariff 收费。任何一项隐匿 O 的真实成本、默认 payer 缺失所以免费、改变旧费率或把 L2 规则 native 化的做法均不通过。

<a id="recognition-opportunity-04"></a>
## RO-04：两合格主体竞争、真实使用与容量守恒

承接 [AC-RO-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-04)；依据 [opportunity-once-settlement](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-once-settlement)。

执行 §2 正常分支。用相同冻结候选集分别改变到达顺序、并发提交、重连和相同请求 retry，独立按原有限闭合轮次与 canonical beneficiary 排序核对结果。同一有效 pending 的 retry 保留原 `first_eligible_round`；请求 ID、刷新次数、隐藏收款或认可分数不得提升名次。原有已接受占用与普通/工业候选都在同一 capacity authority 的视图中；本手册不增设另一套公平参数。

| 必测接缝或负例 | 独立判据 |
| --- | --- |
| 同 occurrence、不同请求 ID、两客户端、并发结算或丢响应后的 retry | 唯一 receipt/一次性 occurrence 记录与 L 增量一致，最多一次；相同 ID 不同 payload 按身份冲突处理。失败者没有申领 charge/材料/容量效果，O 对已接受的整体决定只发生一次原执行费，无效果重复不收费 |
| A 已 `L=1`、B 普通转移已 `T=1`、A 尚未 Use | 关键绕行反例在此时执行，不能等到 `T=2` 才测。独立负例从 B 初始材料2/Electricity6、feederB capacity2 的合法 C0 开始；B 第一笔后仍有第二笔 amount1 的原料、电力、feeder 与 global inflight 余量。显式第二笔普通 Transfer 必须因 R 的 `T+L=2` 被阻挡，无扣料/费/新 job。只检查 native reserved 或只靠原 global inflight=2 拒绝均不足以通过 |
| 同域其他容量 writer 与启用清单 | 对当前可达和未来激活的 writer 分别建清单；每个启用项须用原本合法、确实经过所测 route 的候选证明读取 L，不能让原料、source ownership、路径、权限或 global inflight 先行挡住。当前 industrial start 的 route 引用不算占用；当前 DispatchSupply 的 `valid=false` 及 route 必须 Agent→Agent 的限制使其不适用于本例 site:H→agent:O，不能用改 hub 或预装 auth 假装可达。未来启用这些分支前须补原政策、合法前态与同域容量证据，缺项为 `blocked`，不得为本 profile 绕过原禁用条件 |
| 原独立通路证明缺失，或机会试图吞并普通保留份额 | 发布/结算受阻；普通份额可被普通工作合法占用，但不能被本机会 L 或其后续 T 侵占。不能用全额独占必经路段后要求普通主体申请认可的场景通过本 profile |
| A Use 的原事务边界失败或重试 | 一次成功同时消费 L、建立对应 T、原 source 扣料和原 tariff escrow；提交失败不得留下半份转换或第二 job。`reserved=true` 不能免去本例 A 的材料−1，也不能让 R 同时保留 L 又重复加 T |
| A/B source 改为 H，或 Use 偷改路径、destination、amount、材料、grant | 原权限及 receipt 绑定仍拦截；不得借认可从 H/他人账本免费取货，不能用空 route direct 绕过绑定后仍声称消费该机会 |
| L 存在时原无关 transit 把 global inflight 填满；feeder/余额/原 grant 变化 | Use 按原准入重验，receipt 不承诺运输必然开始；保留准确阻塞与 L/期限状态，不绕过原限制、不复制 fee/material |

正常例没有独立工业 reservation，`I=0`。在含原工业权利的分支，按技术 authority 的原 reservation/job 身份去重，核 `T+I+L≤capacity`；`T_rec` 已含于 T，已转成 T 的工业权利不能再计 I。另核 `L+T_rec≤capacity−ordinary_floor`，防止 Use 后把认可运输改标普通运输腾出新认可份额。完整 writer 清单、原 pending transit 和逐 hop 占用共同对账，不能只比较机会模块的局部计数。B 的普通路线必须实际 start、complete 并付原价；按钮可见或 quote 可行不能证明独立基线。

<a id="recognition-opportunity-05"></a>
## RO-05：当前条件重验与权限连续性

承接 [AC-RO-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-05)；依据 [opportunity-current-revalidation](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-current-revalidation)。

各反例从有合法 pending、尚未结算的独立分支开始，只通过原 authority 改变一个条件，保留变更事件、当时版本与次序。更换 source/recognition revision、范围、用途、资格、route availability/capacity、期限或已审核反滥用结果，结算须重读当前事实，不沿用 preview 的可行性。无当前证据与已证无资格分别报告，不能互相折叠。

发布前独立核新增窗口：`claim_open_tick < round_close_ticks[0]`，round closes 严格递增，且 `last(round_close_ticks) < settle_before_tick < use_before_tick`，Use 截止不晚于认可到期。单独测试 last close 等于 settle 截止、settle 等于 Use 截止、时间溢出或缺时基，均须拒绝配置，不能发布一轮已经来不及结算/使用的机会。新增窗口按半开区间检查截止点；原 native grant 的 inclusive/exclusive 规则保持原值。同 tick 的 close 前后按原 canonical 阶段/序号划分，网络到达和重启不能延长轮次或截止。

| 时间顺序 | 必须观察的状态与效果 |
| --- | --- |
| 提交 → 权限缺失、撤销或到期 → 尝试结算 | 原 pending 的 generic 协议终态免费，保留历史、不继续可结算；receipt、L/T、运输材料/费、claimant 业务费和该协议处理的执行费增量均为0。必要制度归档合并原已预算 CloseRound；不能每次撤权/新提额外收 closure 费，最终每 beneficiary 槽只处理当前尚未闭合状态 |
| 提交 → 撤权 → 权限恢复 → 首次结算尝试 | 即使结算时“当前权限有效”，原 pending 仍已失效。核连续性 barrier/撤权历史，不能仅读当前布尔值复活原请求 |
| 撤权后通过原 authority 恢复 → 玩家显式新提交 | 新请求重新核所有当前条件；不能沿用旧 age、receipt、自动续期或历史认可补签。同 occurrence 已消耗时，新权限也不让它再次发行 |
| 权限持续有效，只有容量暂满或其他允许等待的条件改变 | 按声明的 bounded pending 规则继续待决或终止，并说明 blocker；不得把全部容量等待误判为权限撤销，也不能暗中预留 |
| 期限边界、scope/region 改变或旧 revision 与新权限拼接 | 按原时间及版本规则失效/重新核验，不跨区域携带、不自动延长、不混读形成虚假可结算状态 |

认可权限恢复、Transfer grant 恢复和原动作未知结果是三个独立事实。恢复其中一项不自动重提另外两项；新显式 submission 也不能掩盖尚未查明的旧结算结果。

<a id="recognition-opportunity-06"></a>
## RO-06：未用/已用到期、拒绝与原请求恢复

承接 [AC-RO-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-06)；依据 [opportunity-recovery](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#opportunity-recovery)。

| 独立分支 | 历史、容量与独立继续的 oracle |
| --- | --- |
| A 获唯一 receipt 后未 Use 即 expiry/revoke | L 只释放一次，原 receipt/occurrence 历史保留，未起原 transit、不扣运输材料/费。物理 R 可再被合法普通动作使用，但同 occurrence 不重发给 A/B，不把空闲 capacity 当作“尚未结算” |
| A Use 已起原 transit 后 receipt/recognition 到期或撤销 | 停止未来的新使用；已接受原 job 的材料、escrow、路线占用与完成义务按原领域继续。不得凭认可失效撤销运输、免费回料/退费、提前释放仍在途的 T 或复制第二份货 |
| rejected/released/expired/paused 或申诉待决 | 原因、当前效果及复核边界可回读；独立 Transfer、补证、替代路线或原 repair/rebuild/pivot 按各自规则评估。申诉待决不重新激活旧 pending/receipt |
| 结算已受理但回包丢失，或 Use 结果未知 | 先用原 request/operation/receipt/job 身份恢复，并回读对应 O 的执行费与 hold 消费。查到成功只展示同一结果，查到明确失败才按允许规则显式后续；缺失、超时、历史窗口断档或 endpoint 错误均不等于“未发生”，不得另造请求规避资源或费用去重 |
| 在 settlement、L→T、原 complete 之后分别断线/重放 | 各独立观察点最终账本与无故障分支一致，没有第二 L、T、source debit、fee、payout 或 destination credit；历史查询不能重执行动作 |
| 有效 Use 与 expiry/release 在同一 receipt 上竞争 | 同一原 authority 顺序只提交一个分支：Use 先成立则 T 及原义务保留；终态释放先成立则 Use 不起 job。不得出现已放回普通容量且仍启动运输、重复释放或事后复活 receipt |
| 重启输入缺 once tombstone、缺授权失效史、半份 L→T、丢费用 hold/spent，或混用 generation/schema | 拒绝安装不完整/不兼容恢复态并 fence 新写；不得把缺失默认为 unallocated、已授权、无占用、免费或可重做。完整 generation 回读恢复后再按当前权限处理未决 |

至少一次在认可失效后由 B 通过当前有效原 grant 完成独立普通 Transfer，并保存原费用与交付原件；若本分支仍有合法在途占用，等待其原完成或选原合法替代路线，不删除 job 腾位置。独立基线不要求供应方再次认可或强制投靠组织。若原环境本身无独立可行路线，说明真实 blocker 和待决来源，不靠新按钮宣布继续路径已验证。

<a id="recognition-opportunity-07"></a>
## RO-07：不可转让、审核处置与专题边界

承接 [AC-RO-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-07)；依据 [recognition-abuse-review](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-abuse-review)。

对同一 generic recognition/receipt，分别尝试出售、出租、转让 beneficiary、拆分单位、叠加多个来源、代理或组织代持，以及更换账户/请求 ID 重复申领。同一 canonical beneficiary 更换贡献 Agent/别名不能变成两个 contender；另一 beneficiary 也不能接管其认可。结果不得移动 ownership、扩大 occurrence 预算、隐式收费或提升优先级；合法的原普通行动仍按当前权限处理。

分别提供伪造来源、循环背书、刷取/批量自动化信号与已完成复核的实际处置。仅有嫌疑时不生成惩罚；正式处置须追到事先声明且当时适用的规则、审核 authority、事实类别、范围/期限、当前效果、复核路径和再获资格条件。申诉修正保留原审计链，按新事实重新评估后续资格，不抹去旧 receipt 或重开已消耗 occurrence。免费协议的请求上限及公平等待按技术预算/排序检查，不以“可疑”绕过审查。

交叉取 CR-003 情境声誉、FI-002 pioneer priority 和 GR-001 区域设施容量的合法原件，确认 generic 层不把它们互相兑换为认可、全球权力或 route 容量。尤其 FI-002 在自己 authority 下允许的专门 transfer 不被本节 generic 禁转让覆盖；也不能反向拿 FI-002 的可转让性移动本例 receipt。以上专题各自结果只按原语义呈现，缺 adapter/来源时报告不支持或未核实。

<a id="recognition-opportunity-08"></a>
## RO-08：三入口、真人理解与组合证据闸门

承接 [AC-RO-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ro-08)；依据 [recognition-consumers](../../world-runtime/runtime/recognition-opportunity-settlement.design.md#recognition-consumers)。

同一候选版本分别通过 Pure API、Agent Client 和 Viewer，从原正式身份获取认可来源/当前权限，完成 preview、显式提交、pending、结算回读、Use、普通独立动作、原请求恢复及历史复核。每个入口保留真实请求与输出、身份/授权、当前 source cut 和原 operation/job/receipt 链；某入口缺正式 producer/adapter 时单独 `blocked`，不由另一个入口或共享 DTO 代签。

| 证据责任 | 最少保存的原件与判断 |
| --- | --- |
| gameplay | §2 成熟前态、两名合格竞争者、独立继续、用途/期限和玩家取舍；不能把更高分数或一个成功事件当成机会循环 |
| runtime | 完整同域候选/占用/全部 writer、原轮次排序、免费 generic read/inbox、O 原 metering/受保护 operational+closure 及单次费用、单次 occurrence/receipt、L→T 原子性、逐账本守恒及撤权连续性 |
| Agent | 实际读取和选择理由、当前 grant、显式提交/使用与等待/恢复行为；没有自动续期、静默重提、绕权限或用历史认可补签。模型说明不能替代原动作原件 |
| Viewer / Pure API / Agent Client | 各自来源、状态、未知/终态、原请求恢复和独立下一步可读且与同一 canonical 结果一致；旧快照/ACK 不显示已分配、在途不显示已交付 |
| blockchain / world authority | 原授权、审核与实际结算的权威记录、顺序、回读/重放及不重复效果证据；未提供真实链路时保持该层未核实，不把本地 fixture 宣称链上最终性 |
| QA | 输入与隔离分支、独立数值 oracle、关键反例、覆盖/缺口、raw 输出与判定；逐 AC 和逐入口记录 PASS/FAIL/blocked/not_run，组合结论不高于最弱所需层 |

另招募真人在不告知答案的情况下解释并操作：自己为何获认可；现在只是可申请、pending、已分配、在途还是已完成；认可与当前权限有什么区别；机会的范围/期限和真实成本；为何另一个合格者没有获机会；权限撤回后为何不能复活旧请求；机会失败或到期后如何继续。让其实际选择并完成一次合法独立动作，保存任务脚本、关键交互、原话、误解和修正；Agent 自述、评审者阅读文档或一张截图不能代替该层。

结果记录须列出候选 build/源版本、world/profile、8 个 AC 对应原件、三入口及真人样本、未执行与阻塞项，并区分设计可评审、实现存在、完整运行通过与当前发行 readiness。缺同一候选 gameplay/runtime/Agent/Viewer/blockchain/QA 组合证据时保持 `pending/unresolved`；本手册及静态数值推导不提供当前成熟世界认可/机会闭环已通过的结论。
