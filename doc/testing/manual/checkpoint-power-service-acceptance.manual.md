# 有限检查点供电服务验收手册

- Owner role：`qa_engineer`；协作：`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`blockchain_ops_engineer`。
- 验证对象：`owner_checkpoint_power_service_v1` 的同版形成、有限履约、资金保护、救济、显式继受和原件恢复。
- 唯一业务政策：[有限供电服务](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-scope)；技术合同：[履约系统设计](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-formation)。
- Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。承接 [AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) 与 [AC-WR-CR-002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-002)。

本手册定义未来验收方法，未执行下列业务、故障、跨节点或玩家样例。当前 native Service 的拒绝、旧 AtomicExchange、静态面板和文档 CI 均不能代签目标服务可用。

本手册保留两个个人主体与 Electricity/Data 的原算例及 PS01–08。组织、多方、维护、物料运输和协作按[完整服务验收](continuing-service-fulfillment-and-remedies-acceptance.manual.md)使用其真实 family、条款、费用图和证据；不能把那些输入改标为供电 v1 获得更少接受或更低预算。组合验收还须证明旧供电原件及未结义务仍按原版本恢复，新 family 缺适配器时明确拒绝，两个版本的单位/费用/责任账本不互相代签。

## 1. 固定候选、来源与通用判据

执行者先固定 source/build、world/genesis/branch、当前权威 root/tick、profile/schema、instance/activation manifest、原条款和接受摘要、合法费用政策/计量 schedule、hold 适配器完整覆盖、panel 权限/名册/抽签来源及正式入口。保存实际使用的命令、原 request、结果、receipt、资源/责任差分及未覆盖项至对应 PR/测试记录；不得把下列例子填成真实运行结果。

只有目标 profile 的真实 capability 和必要 authority 均具备时执行其正向业务用例；缺一项时先验证 `profile_unavailable` 与零业务效果，正向项标为 blocked。可控 fake 只验证其接口约束；生产 adapter、真正随机/签名来源、全部资源消费者、持久提交与真实 Viewer/API 另取证，不以 fixture 自报替代。

每个变体使用独立初态，或明确从指定已提交状态继续。通过原世界来源观察以下量：每个 Agent 的 gross、每笔 purpose/source/unit scope 的 hold、`available=gross−holds`、treasury、其他合法 sink/source、单元分区、fee accumulator、请求/业务键、案件/裁决与实际支付。任何持久 unknown 先按原 generation 恢复，不能为了继续测试换请求或新建合同。

只读/非绑定、拒绝和精确重放允许其合同规定的协议记录，不允许新增资源占用、费用、服务责任或世界收益。成功事务必须将净交付、净价款、抵押处置、全部适用费用、单元记录和 receipt 同根提交。用原输入、政策和 checked 整数公式独立计算 oracle，不从被测返回的“成功”“已扣费”或余额字段反推预期。

上述零新增业务费要求只针对本 service 的确定拒绝、非绑定及精确重放。用于竞争 hold 的其他既有资源入口仍按各自合法收费/审计规则验证；不能将本 profile 的拒绝语义推广为全世界动作免费。

## 2. 可手算的共同测试输入

下列是合法测试候选的条件输入，不是生产价格、默认 collateral、真实 tick 时长或平衡结论。若该候选的合法政策不接受这些输入，应选择该政策允许且覆盖同一算术边界的值，保留原失败结论，不旁路准入。

| 输入 | 固定测试值 |
| --- | --- |
| 主体 | 供方 `P`、买方 `B` 是两个不同个人；`AP`、`AB` 为各自当前自有 Agent，同 world/branch。接收地点 `L` 是可核验 canonical GeoPos。 |
| 服务 | 两个检查点；每个 `q=4` 个 Electricity 单元，`p=2 Data/单元`，`c=1 Data/单元`。价款 hold 总量 `16D`，抵押 hold 总量 `8D`。 |
| 时间 | 激活 tick `90`；正常窗口分别为 `[100,110)`、`[140,150)`；仅本例 `Q=8`。其他截止按[政策公式](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-terms)复算。 |
| 费用 | 固定合法 artifact-owner 为 `B`，但交易费付款方仍分别按其政策解析。记 `F_PE/F_PD/F_BE/F_BD` 为各方各资源的 operational+closure 全部费用预算；从实际交易费、计量上界与成功命令次数独立复算，不能假定 `F_P=0` 或只预存一次调用费用。 |
| 初始 gross | `P=(40E+F_PE, 40D+F_PD)`，`B=(20E+F_BE, 80D+F_BD)`。费用支出前的业务本体合计为 `60E/120D`；费用预算真实存在于原账户，不额外铸造。 |
| 争端 | 本地获准 jurisdiction、有效名册中至少 6 个不冲突的独立候选、可验证抽签来源及其真实绑定；固定原协议和检查点身份，不让操作者选择有利 seed。 |

激活后，扣除实际费用及对应 fee hold 的变化，业务本体的 gross 仍为 P `40E/40D`、B `20E/80D`；price/collateral 只是 hold，故业务可用量分别为 P `40E/32D`、B `20E/64D`。不能把 gross 同时减去 hold，再在消费时重复扣一次。

下表仅列净业务本体；每个实际 gross 分量还应加其原 `F`、减该方迄今实际合法费用，并单列该候选真实发生的其他世界变化 `W`。跨 tick 若触发 upkeep 或其他业务，按其原 authority 记录 W 与 sink/source，不修改世界规则把它们压成零。可隔离 fixture 的静态手算不代签完整世界运行。

| 已确认结果 | P 业务本体 | B 业务本体 | 尚未解除的业务 hold |
| --- | --- | --- | --- |
| K1 完成交付 4E | `36E/48D` | `24E/72D` | B price `8D`；P collateral `4D` |
| K1 完成、K2 交付 2E | `34E/52D` | `26E/68D` | B price `4D`；P collateral `2D` |
| 两检查点共交付 8E | `32E/56D` | `28E/64D` | price/collateral 均为 0；fee 余量待合法终结释放 |
| 从第二行终止余量，无有效判罚 | `34E/52D` | `26E/68D` | 残余 price/collateral 解除；释放不增加 gross |
| 从第二行对余 2 单元有效判赔 | `34E/50D` | `26E/70D` | 残余 price 解除，`2D` collateral 真正转入 B；费用另按合法来源计入 |

每次分别核对 E/D 的 gross、treasury 与显式 source/sink 守恒；价款、抵押和 fee holds 不是第二份余额。代入真实费用后，正常与失败样例均不得出现负余额、暗扣第三方或额外奖励。

<a id="ps01-formation"></a>
## PS01：完整条款、同版接受与一次激活

依据：[形成政策](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-terms)、[技术形成](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-formation)。

**前置与刺激。** 从共同初态依次查看 preview、创建 offer、单方接受、双方同版接受、重复接受和显式 activate。分别在提交前改变 q/p/c、Q、截止、费用来源、payer、接收位置、当前 owner/generation、Data 授权或余量；再测试未知正文/政策版本、1 与 17 个检查点、零量/零单价/零抵押、交付窗口重叠、过期接受、跨 world/branch、同一主体控制双方及整数溢出。只改变一个原因以定位拒绝边界。

**预期。** activate 之前不存在任何服务 hold、费用或绑定义务；一个方向接受不能激活。修改已接受条款使旧接受失效；完整真实条件满足才一次形成 active receipt 和四种 purpose 的足额保护。失败不留下部分资金占用、抵押、费用或 active 状态。两方并发 activate、不同 request 的同义激活以及激活后重放只得到一个业务赢家。

**证据。** 完整条款与正文摘要、接受签名/当前授权、前后 root 与逐项 gross/hold/treasury、原 request 结果及唯一激活 receipt。同时保存全部绝对截止，后续 PS05/PS06 直接比较，不从新 case 重算。

<a id="ps02-checkpoints"></a>
## PS02：净交付、部分履行与累计交易费

依据：[检查点政策](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-checkpoints)、[单次结算](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-checkpoint-settlement)。

**正向。** tick101 对 K1 `[0,4)` 交付；tick141 对 K2 `[0,2)` 交付，核对共同算例前两行。每笔 E 净交付、Data 净价款、对应 collateral 释放、全部费用、已完成范围和 receipt 同时出现。分别用 `[0,4)` 一次交付与 `[0,2)`、`[2,4)` 两次交付完成同量，比较累计业务结果。

**拆分费率边界。** 使用合法测试费用来源，例如 E leg `2500bps`、Data leg `1250bps`，独立计算每检查点 `floor(累计净业务量×bps/10000)`：4E 的累计费为 1E，8D 价款的累计费为 1D；第一次 2E/4D 的两项交易费均为 0，第二次必须分别补 1，不能再次用单次量 floor 成 0。确认这些值确由允许的测试政策签发；若本环境不允许该政策，换用同样能跨过最小单位边界的合法输入。累计交易费应与一次交付相同，mandatory metering 则按真实调用次数分别结算，不要求两种拆分的总运行费相同。

**失败与边界。** 提交与已完成区间相交的 `[1,3)`、越界 `[3,5)`、空区间、同 request 不同内容、换 request 重发已完成范围；在 E、净价款、任一费用或原子写入点注入不足/失败。另改变共址、接收位置或当前方向授权。全部拒绝样例没有净交付、付款、释放抵押或新费；共址/授权拒绝不直接判 default。

**到期与修复。** tick150 读取 K2，看到 2 已交付、2 未交付及 checkpoint_due，抵押 2D 未被罚没；tick160 合法补交 `[2,4)` 后核对全交付行。独立变体在 K2 `R=166` 时才提交修复，预期 window_closed，不倒拨截止或重放 K1。同一 tick 的截止、交付与终止按 canonical 顺序分别验证两种可达排列。

**证据。** 单元分区、每腿累计 basis/assessed fee、逐笔费用来源与真实 meter、全量资源差分和 receipt；不同请求/修复/继受后的 accumulator 仍绑定原 lineage/checkpoint/fee leg。

<a id="ps03-holds-fees"></a>
## PS03：所有资源消费者、费用预算与安全结清

依据：[hold 语义](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-resource-holds)、[费用与 drain](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-fee-budgets)。

1. 在激活已持有 price/collateral/operational/closure 的 root 上，让该候选登记的每类 native、module、费用、账户/owner 变化消费者分别尝试消费 `available+1` 或挪用不属于自己的 hold。检查直接调用、正式入口和低层 publication 路径；只在服务入口拒绝而其他消费者可透支即失败。未覆盖消费者必须列出，不能从普通 transfer 一例推全仓。
2. 篡改 hold 的 owner/source、purpose、unit scope、索引或剩余量；将原 Agent 重新绑定给他人，再以新 owner 提取旧价款、抵押或退款。原资金权利不转移，错误 scope 拒绝；合法原受益人尚无安全收款位置时为 return_pending，不能入新 owner 可支出余额。
3. 独立按实际交易费来源、Kernel/module schedule、规范化输入/输出、effect/emit 等上界与每类成功次数复算 F；真实执行按真实 meter 计费。测试恰好足额、少一最小单位、加法/乘法溢出、缺 fee authority、非法免收、第三方 artifact owner、计量 payer 与报价不符。失败均不能借 price/collateral 补费。
4. 用合法成功操作耗尽交付/修复的 operational 份额或成功次数，确认剩余相应操作被明确阻断，而双方程序份额及原 closure 预算保持完整。一方用尽自身合法补证份额后，另一方举证、复核、最终确认和 closure 仍有原足额预算与次数，不能受普通操作侵占。以最大剩余区间/案件/责任状态完成合法结清，记录最末次 terminal fee 与余量释放在同一根事务。不能先解锁所有费用后再另起一次无资金的“最终确认”。本 service 的精确重放、只读、非绑定及确定拒绝不再消费预算或费。
5. 分别测试交易费两腿、抵押赔付适用的独立 closure fee leg，以及明确合法免收的 scope。净赔付仍为约定金额，适用费用由已声明预算支付。金额相同但拆分、修复、换 ID/继受时，累计 floor 差额不得重置；变化的真实计量费必须能解释。
6. 在有未结清合同、payer 已变更和旧 artifact/schedule 被禁用的不同初态提出升级/drain。若不能保留原合法退出，拒绝破坏性变更或保持已定义的安全隔离；不能追收新费率、暗扣新 owner、删掉旧 budget，或用缺 owner 的免费分支运行旧合同。

**通过依据。** 逐账户 `available≥0`，reserve/release 不改变 gross，消费同时减少指定 hold/gross 并记合法接收账；交易费、meter、赔付和费用余款全部守恒。固定候选最大闭合写集确实可在保留预算与技术上限内提交；仅写出预算公式不算此项运行通过。

<a id="ps04-repair-termination"></a>
## PS04：可归责失败、原范围修复与终止

依据：[责任判据](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-checkpoints)、[恢复选择](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-remedies)、[恢复动作](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-recovery-actions)。

**前置。** 使用 K1 已完成、K2 已完成 2 单元的共同状态，剩余 price4D/collateral2D。正常修复沿 PS02；每个以下分支从该状态独立开始。

| 刺激 | 可判定结果 |
| --- | --- |
| 只有 due 或 R 到达，供方没在线/没回复 | 只记录真实未履行与下一步，不由 timer、缺席或一个失败 ack 罚没。 |
| 双方同版明确确认余 2 单元的供方责任及终止/赔付 | 只处置余量；净 collateral2D 给 B，price4D hold 解除，原 K1/K2 已完成范围不变，核对共同算例判赔行及全部费用。 |
| 买方在相关窗口撤去接收授权或离开位置；供方请求被拒 | 给出具体接收障碍，不能把该拒绝自动判为供方违约。双方中性终止或依有效程序判断；已完成量保留。 |
| 仅当前余额不足或历史/执行可用性覆盖未知 | 信息不足不等于缺供责任；不能凭当前快照补写过去事实。保留原件查询、争端/到期的条件路径。 |
| 双方同版中性终止余量 | 解除 price4D/collateral2D，gross 不因释放增长；核对共同算例中性终止行，已合法费用不自动退回。 |
| 单方终止已确认赔付、修改金额或用新合同覆盖旧责任 | 拒绝该越权范围；final 待付款仍保留，不降级为未决/中性。 |
| 提交结果未知后重新修复、另换 request 或主动续期 | 先查原件；同单位至多一个 effect，不能支付后又退款或产生第二次电力。 |

**证据。** 原责任/接受来源、已完成和剩余单元、当前 blocker、选择前后资金和费用、绝对截止、原 request/receipt。任何返回“terminated”的样例还须分别证明赔付/返还是否实际到账，未到账原因必须可读。

<a id="ps05-disputes"></a>
## PS05：最小保全、随机 panel 与固定尾部

依据：[唯一争端政策](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-disputes)、[争端权限](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-dispute-authority)。

**正向责任样例。** 从共同失败状态取 K2 余 2 单元；固定原条款已生效、到 R 未履行、买方接收条件与正式执行的完整来源均成立，且供方合法尝试因其 Electricity 不足失败后未补足，或有权供方对原范围作出明确签署拒绝。保存相反证据查询结果，不能预先把结论喂给裁决者。在原证据截止后一次固定整案 scope/证据 root，合格 panel 才可投票；2/3 对精确范围、同一 root 与 supplier-default 处置签署一致初审。程序窗口结束并依法 final 后，只将 collateral2D 给 B，price4D 解除，其余历史保持。

**相反证据样例。** 在相同未交付数量下加入买方移位/撤权的 canonical 来源、相关可用性未知或已受保护导致的执行停止；分别检查证据集合、理由和结果。未满足预声明责任条件不能认定供方违约；不能以其中一方缺席、拒补非必要材料或模型认为“不可信”补齐责任证据。

| 组别 | 刺激与必须观察的结果 |
| --- | --- |
| 资格与最小 scope | 空白“不同意”、无事实、越出原单元、已交付范围和无权主体不产生 stay；有效受理只保护具体残余。在 K1 争议时正常完成 K2，确认后者没有被全合同冻结。 |
| 权限与抽样 | 拒绝组织 charter/运维身份替代个人服务 jurisdiction；名册重复 controller、当事方/关联受益人和未知冲突者不能凑人数。先固化 case/名册再取获准随机轮次；public block hash、客户端 seed、选择已知有利轮次均不能激活/选人。 |
| 防重抽 | 换 request、撤回重开、补证、同 scope 换 case、继受、关联续签均回到原案与固定抽样；冲突补位按原排列顺延，不能再生成 seed。初审成员不能进入申诉组三人。 |
| 证据与票数 | 每方每阶段 16 引用上限及上限+1；初审用满后仍可在独立程序份额中合法申诉。证据截止前双方合法争点合并一案；提前投票不得耗尽唯一轮次，截止后普通扩范围/补证拒绝。1/3、冲突成员票、不同 disposition/root 或超范围金额均不能形成有效决定；冻结整案后2/3的合法一致决定才能推进。 |
| 有效初审与申诉 | 无合格申诉时按原 Q 窗口 final；有合格申诉时，各方合法理由并入同一轮，原申诉窗结束冻结程序 root 后才复核。先申请者不能占用另一方资格或提前结案。未获复核结果时，截止保留仍有效初审；仅提交申诉、拖延回复或复核人离线不能把赔付改成原额返还。 |
| 有权程序纠错 | 提交可核验授权错误、已交证据遗漏、利益冲突或越界；只有有权复核确认具体错误才能 invalid/纠正初审。一般重复争论不能撤销；合法纠正保留关联原决定与原因。 |
| 绝对上界 | 以 K2 的 Q=8 复算 R166、证据截止182、初审截止198、最迟申诉截止206、复核截止222、处置上界230。分别早开、临近182才开、不开案和继受，以上边界完全相同；临近开案不另获64 tick。 |
| 无有效结论的尾部 | 完整 canonical 记录证明至230仍无有效责任结论，且未交付/资金仍未处置时，按原已签政策中性终止；无 panel、无随机或无 quorum 不默认判任一方胜诉。保留未履行与证据，不显示“没有违约”。 |
| 既有责任与 unknown | 至230已有 final 待付款、有效和解或仍有效初审时，保持相应责任；案件/裁决提交已成功但客户端未知时先恢复原结果，不能按“没看见”中性退回。原 writer 状态未知也不得在旁路退款。 |

所有尾部动作必须实际完成合法有预算的 canonical 提交才算结清；tick 到达、计算出 eligible、一次轮询或返回失败均不能作为资金已退证据。缺受益人合法收款条件时明确 return_pending，不把款转给新 owner。当前没有真实 panel/randomness/正式入口的候选，正向裁决仍为 blocked，mock 签名或伪随机只证明对应 fixture 分支。

<a id="ps06-succession"></a>
## PS06：三方显式继受与旧责任隔离

依据：[继受政策](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-succession)、[技术继受](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-succession)。

**正向。** 从 K1 完成、K2 尚未交付且未有争端的状态，引入与 B 不同的个人 S 及其当前自有 Agent AS。P、B、S 同版接受只交接 K2 `[0,4)`；S 用自己的资金补足 collateral4D 及其适用费用，B 的计量 payer/source 维持合法。一次提交切换未来供方/收款责任和准确资金占用，P 对应未来 collateral 可解除；B 的原 price 来源、K1 receipt、所有绝对截止、原单位及 fee accumulator 不变。S 后续交付4E，核对 S 的净电力、B 的净电力与向 S 的8D价款，不能把这8D再付给 P。

**必须失败或保持未决的变体。** 缺任一方接受、条款不同、AS 非 S 当前所有、S 与 B 同控制、源权限漂移、新 collateral/费用不足、与旧已完成/已判罚/争议范围重叠，以及无法保持合法 artifact/payer/drain 的交接，均不得先释放 P 的资金或切换责任。若 P 原来也是 artifact payer，单纯改成 S 不足以获得计量权；合法新执行绑定未具备时拒绝。只交接 K2 的 `[0,2)` 而让 P 继续 `[2,4)` 的请求必须拒绝；继受必须覆盖该合同全部仍可继续履约的未交付范围，不得形成两个并行供方。

**责任与竞态。** 保留 P 在别的检查点的 final 待付款责任，交接不取消、不改成 S 的债务。交接与 P 的最后一笔交付分别按两种 canonical 顺序执行：实际已完成的范围不可再交接，已交接范围的旧请求不得作为 S 的意愿执行。原 Agent 后来换 owner，也不能把旧资金/责任、个人 credential 或情境声誉复制给新 owner。

对仍可能解冻并在原 R 前继续交付、但尚未明确处置的旧争议范围，继受必须先拒绝；不能借 P 退出强制取消。只剩责任结清的原争议/冻结范围可以保留，但继受不转走其 hold；后来解除保护也不自动恢复 P 的供给入口或给 S 添未签责任。对原签署 manifest 的继受次数与 provider/程序 actor slots 测试 cap−1、cap 和 cap+1；最后一次合法接替仍须保留所有旧责任槽及费用，超限没有新 slot、预算或交接效果。更换主体和三方重签不得重置计数，容量耗尽也不能侵占旧举证/复核/closure 份额。

**证据。** 三方签署、原/新主体来源、完整旧责任、旧/新 hold、payer/fee 绑定、原与新请求结果、唯一交接 receipt。续期另建有当前报价的新合同，旧单元、case、抽样与未付责任仍可查询，不由新合同清零。

<a id="ps07-recovery"></a>
## PS07：原请求、持久未知与恢复闭包

依据：[持久化与恢复](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-persistence)。

固定候选分别在 activation、部分交付、责任判付、继受及最后闭合的 prepare 前后、持久决定前后、响应发送和 snapshot/journal 安装处注入故障。每例记录真实故障点与持久结果，使用原 request/world/branch/instance/lineage 查询和恢复；重连先读，不自动新建合同或重发履约。

- 无提交决定时，不出现电力、付款、抵押释放、费用或一半新主体；原身份在权限仍有效时可重试。
- 已有提交决定而响应丢失时，返回原 receipt；exact replay 不再交付、扣费、返还、改 provider 或重抽 panel。换 request 仍受同一业务单元和 disposition key 约束。
- 客户端 `outcome_unknown` 与持久 generation `StatusUnknown` 分开；后者必须 fence 整个旧 writer，不能只暂停本合同后继续其他写入，也不能换 writer 猜退款。
- 恢复需同时恢复原条款/接受/狭义处分权、单元分区、全部 hold/index、fee basis/assessed/remaining、provider 段、Q/固定截止、case/抽样/证据/初审/复核/final、请求与 receipt。删失任一必需部分、来源冲突或未知版本应停止相关写入，不能以空集合补齐。
- replay 消费当时已确认事实，不重新抽签、不按当前费率补扣、不用当前 owner 改原收款人、不补算现实离线 tick。旧合同 artifact 被禁用时仍必须走合法 drain；旧二进制不能忽略新资金责任字段后继续写。

**证据。** 恢复前后各权威 root、原 durable 决定、writer fence、逐来源资源与责任账、原请求 readback 和不重复效果断言。单机 fixture 只证明对应进程故障；声称跨节点/最终性时另执行该候选正式组合，不能沿用其他 profile 的结果。

<a id="ps08-read-compatibility"></a>
## PS08：玩家理解、同源读面、容量与旧版边界

依据：[玩家预览](../../game/gameplay/gameplay-checkpoint-power-service-contract.prd.md#service-policy-preview)、[统一读面](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-read)、[兼容](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md#service-compatibility)。

对同一固定 root 分别读取 Viewer、正式 API 与 Agent 结果，比较条款/单位、Q 与绝对窗口、当前 authority、四类资金占用、费用、已完成/未完成/被保护范围、初审/申诉/final、待付赔偿/待返还和合法下一步。公开与授权视图按原权限裁剪，同义不表示泄露其他主体的私人证据。stale、未知、unsupported、预算耗尽和 source 不可用各有真实原因；不存在专业写入口时控件不可伪装可执行。

使用未被预告答案的代表性读者，依次给下列具体状态，先问其判断与选择，再对照 authority；原话、错误、提示和结果分开记录。本段是任务脚本，尚未执行，不能据文案存在认定玩家理解通过。

| 状态与原话任务 | 正确理解的可观察边界 |
| --- | --- |
| 激活前：“你想在两个窗口得到8E。请决定是否接受，并说出现在和以后分别会发生什么。” | 能指出未来电力尚未到账、16D价款/8D供方抵押与费用占用、首检查点4E的实际价值、时间条件及不签约的替代；未把接受当激活。 |
| K2 到期仅交付2E：“你现在已经得到多少，还有什么可做？” | 区分总已交付6E与余2E，due不等于违约；说出原窗口修复、符合条件的终止/争端及费用/授权限制，不要求重新做K1。 |
| 有效初审待申诉：“对方刚申请申诉，你现在能把抵押取走吗？” | 知道申请不撤销初审、已有责任和资金保护仍在；只能按固定程序或原件查询继续。 |
| 固定尾部无有效结论：“到了原截止，能说明谁无责吗？哪些钱真到账了？” | 中性处置不等于清白或全赔；区分已确认责任、可结清、unknown与return_pending，实际到账以receipt为准。 |
| S拟接替：“P、B都同意了，S还没签，你能否让S开始？” | 三方同版和新方资金/当前权限均须满足；旧请求、期限、历史责任和个人credential不自动转移。 |

容量检查使用 [runtime 合同](../../world-runtime/runtime/checkpoint-power-service-fulfillment.design.md)及其实际 manifest：覆盖 terms/command 字节、区间分片、引用数量、成功命令数、继受/provider/程序 actor slots、分页及最大闭合状态的边界与边界+1。超限拒绝不截断条款、隐去未读页债务或通过碎片/新主体耗尽破坏结清。每页绑定同一 root，读不到下一页不是“无剩余责任”。

旧 native Open/Accept/Settle/Expired、旧快照与 replay 保持原语义，旧 Open 的到期声誉后果不能被解释为零效果 offer，旧单腿付款不能充当新检查点供电/价款的双向结果。原 [CF01–CF08](agent-mediated-contract-formation-acceptance.manual.md#cf-01-nonbinding) 即时交换范围保持；本手册不改其正常结算。B23/B24 的 `missing_full_write`、未实现 surface 和实际运行状态只能由真实后续交付改变。

## 3. 接收结论

QA 分别记录各 PS 的候选、实际入口、方法、通过/失败/blocked、缺陷、复测与未覆盖范围；不把手算、mock、文档门禁或其他 profile 的通过汇总成完整服务 PASS。按源 authority 修复后复测实际受影响部分。只有当前候选的正常、关键失败、真实资源消费者、费用/退出和恢复组合均有对应证据，才可报告该有限 profile 的能力；通用持续服务、组织/多方、物料物流、完整仲裁和整个 SC-14 继续保持各自范围。
