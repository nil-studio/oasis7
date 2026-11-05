# 区域冲突资产、恢复与软赛季连续性系统设计

- Owner role：`runtime_engineer`；gameplay、区域/tenure、物理资产、资源/运输、授权、QA 共同审读。
- 状态：`target`。本文交付原 CC-001/002/004/005 的完整目标技术合同；CC-003 继续由[恢复项目设计](bounded-recovery-projects-and-contribution-settlement.design.md#recovery-model)拥有。本次未实现、未运行游戏或证明可玩性。
- 固定审读基线：`eng-cc/oasis7@cca8cda97cf121ebc1e25e96d6b0204a421323ea`，2026-11-05。玩法权威为[冲突、恢复与软赛季政策](../../game/gameplay/gameplay-chartered-conflict-recovery-and-soft-season-contract.prd.md#conflict-enrollment-policy)，地点权利沿[原 tenure 政策](../../game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md#tenure-policy)。

## 1. 问题、目标与非目标

现有 WarDeclared/WarConcluded 保存联盟、强度和资源/声誉后果，没有参战资产、离线 envelope、真实毁损、连续占有、条件 tenure 转移、物理提取和软赛季连续性的完整合同。普通工厂维护、材料转移或恢复项目也不能代签这些结果。

目标是从预先合法暴露到真实有限效果、地点权利和货物分别结算，再到恢复/退出与下一窗口，始终保持同一世界、原身份、账本和历史。**损失恢复同时适用于冲突、区域危机、一般经营能力损坏**；后两类不要求创建冲突或取得战争权限。

本设计不启用当前 War=Defer，不改变旧战争数值或 SC-32 pilot，不把战争变成首局主线；[原重启条件](../../game/gameplay/gameplay-top-level-design.prd.md#94-历史战争政治数值基线边界)继续要求独立平衡与玩家证据。不新增货币、全局赢家、无预算赔偿、身份捕获、世界 reset 或无限级联反击。

## 2. 上游约束与相关角色

gameplay 拥有已接受的竞争、损失、时界和恢复政策；区域/tenure 拥有合法区域和地点权利；原物理/库存/运输 writer 拥有实际位置、条件、数量与工作结果；L2 institution 提出有限有权效果，Kernel 保持通用授权、资源与物理不变量；Viewer/API/Agent 只消费当前受权事实。当前原授权、资金和随机名册是否实际存在，是激活前置，不从本文名称推导。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | 合法charter/主体、精确暴露、公示和非参与/新人/第三方保护 | [当前准入](#conflict-authority) | 原区域/主体/资产/义务authority与当前治理 | 联盟身份、报价或上线状态不构成参战同意 |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | 触发单次、累计cap、独立最终撤销额度/费用和无递归反击 | [离线防御](#conflict-defense) | 原Agent/设备/移动、当前grant与真实预算 | 模型建议不得扩大目标；自动响应不递归触发 |
| [CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | 冲突/区域危机/一般经营三原因的真实损失、全部声明资产类及身份历史 | [实际资产与来源](#conflict-assets) | 原body/equipment/factory/inventory/work/区域能力writer | native资源分数不能替物理结果，不能删除身份 |
| [CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | 三类原因下repair/rebuild/pivot/退出、逐步成本及真实恢复 | [恢复与独立退出](#conflict-recovery) | 原目标/资产定义、维修/建造/物流/tenure | 外援可选；不要求先参战或免费复活 |
| [CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | contested occupation、持续H至到期、原条件接受和实际tenure转移 | [连续占有与权利](#conflict-occupation) | 当前空间/控制/能源事实及tenure两instance | 临时控制不代长期权利；不延原期限或没收第三方 |
| [CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | source片段/custody、真实路线/arrival/保护接收、最终收益与在途尾部 | [实体提取](#conflict-extraction) | 原物流/资产/receiver/容量writer | direct transfer/ACK/装载不代签物理交割 |
| [CC-005](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-005)；Trace refs: O7-REQ-ce9fbace-7fc6-4973-8bcf-8b58e11b3339 | 预告刷新/时界/复核，保留world/身份/能力/价值/历史及原义务 | [软赛季连续性](#conflict-season) | 原季节政策/资格/结果source、当前治理和原领域收尾 | 不造新World、回滚资产或自动奖励 |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | 原授权/范围争点、独立随机panel、final guard、最小保全和程序尾部 | [有限授权复核](#conflict-review) | 目标regional-conflict.review与当前名册/随机/预算 | 他域panel/管理员不得代签，不形成一般法院 |
| [CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | 冲突实际因果/责任/损失、真实bond赔付与原结果单次保留 | [有限责任与赔付](#conflict-review) | 原资产/责任/预算writer，非战争原因沿原authority | 不凭缺席判责，不免费复原或无预算追扣 |
| [CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | 未结entitlement/临时用途精确保全、有限处分与实际提取保护 | [未结权利争点](#conflict-review) | 原case、tenure/loot及当前receipt来源 | 不回滚已合法第三方交付，不扩大为全资产冻结 |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | 全流程current guard、原请求/并发/两instance/G1三态不重效 | [原事务与恢复](#conflict-persistence) | 原ExecutionTransaction、G1/history/原domain writers | S0或旧war测试不证明新组合/跨节点通过 |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | 全部source/证明/两invoke/控制tick、普通与撤销/复核/关闭资金计入 | [有界来源和关闭预算](#conflict-budget) | 原metering schedule、真实payer、受权source/index | 不免费控制区间/第三invoke，不全扫后截断 |

## 3. 当前状态、目标状态与差距

| 固定源码/合同 | 当前真实范围 | 新设计的独立差距 |
| --- | --- | --- |
| [WarState](../../../crates/oasis7_world_state/src/gameplay_state.rs)、[宣战入口](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_gameplay_war.rs) | 检查主体/联盟、强度、已存在战争、资源等；状态记录比分/结果 | 没有本契约的逐资产参加/保护、区域边界或持续控制证明 |
| [war publication](../../../crates/oasis7_world_state/src/world/alliance_war_publication.rs) | 稀疏准备/安装WarDeclared与WarConcluded，后者应用Electricity/Data/reputation delta | 不是chassis毁损、tenure转移、cargo/安全提取或赛季writer；不能扩展原字段含义 |
| [旧war baseline](../../game/gameplay/gameplay-war-politics-mvp-baseline.design.md#des-war-politics-001) | 强度1～10，动员E=12+4i、D=8+3i，旧duration=6+2i及原core/M5差别 | 保留兼容/原验证；新可暴露资产与时界不由旧公式推导，不以其quote批准新冲突 |
| [factory生命周期](../../../crates/oasis7_world_state/src/state/industry_transition/factory_lifecycle.rs)、[material transfer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transfer.rs) | 原durability/维护/回收和材料receipt有局部来源 | 目标需精确损失原因、权利/custody、受保护WIP/第三方与所有实际资产adapter |
| [生产恢复](production-recovery-comparison.design.md#production-recovery-candidates)、[恢复项目](bounded-recovery-projects-and-contribution-settlement.design.md#recovery-contribution) | 原有限生产路线和贡献型恢复目标设计 | 不等于全部战损/危机/日常损坏的实际恢复；系统项目仍需原containment/资格 |

以上是静态源码/合同核对；未运行现有测试，也不对未提供的外部部署作全仓未实现推断。

## 4. 边界与结构

冲突/占领/争点归一个稳定 `(world, module_id, instance_id, conflict_id)` 的受治理L2状态；tenure保持独立instance，季节窗口也有独立稳定instance。host按原manifest选择当前artifact/schema/activation，禁止caller提供任意代码、回调或把新判断加入旧native War。每个原子操作至多调用两个已声明instance；原资产/空间/资源writer提供通用有限effects并保持自己的不变量，未证明的第三次invoke不免费隐藏到“验证来源”中。

`ConflictSourceReader` 先读精确key/range descriptor与coverage generation，再读当前同cut的主体/保护/charter/授权、资产/义务、位置/容量、费用和原结果。所有影响原对象的writer需更新可验证索引或提供到同cut的完整水位；缺项/落后/超限返回incomplete。持有一个客户端清单或泛签“我有权”不能代替当前完整来源；不全扫World/journal后截断。

| 目标记录 | 必需语义与唯一owner |
| --- | --- |
| `ConflictPolicy/Declaration` | world/region/charter/current authorization，固定政策digest、参数Q/W/H/X/时界、范围/参加者/资产root与cap、动作能力、费用/责任/关闭预算、review和结果类型；L2 owns |
| `Participation/Exposure` | 原合法主体/代表、已同版接受、原资产/quantity slice/用途、最大可损失/可转范围、第三方安排、原期限/退出、条件转让同意和真实hold；不能以组织成员集合代替 |
| `DefenseEnvelope` | 每原conflict/participant唯一稳定lineage、当前grant/revision、Agent/设备/资产、触发effect类型、动作/位移/次数/逐资源cap、expiry、共享已用量/触发消费root与最终停用标记；不能另建活动包或刷新旧预算 |
| `AssetLossView` | 原因类型conflict/regional_crisis/ordinary_operation、原领域effect/时间/版本、before/after/实际损失/可恢复状态/owner/custody/义务、稳定Agent身份ref；只投影原事实 |
| `OccupationRecord/ResultSlot` | 声明用途、原tenure/条件同意、原完整控制区间/中断/结束cut、qualified/not/unknown、单次权利结果与实际tenure receipt |
| `LootLot/Extraction` | 原source lot/数量/权利、暴露/损失/entitlement、真实custodian/location、载运/剩余/损失、destination/acceptance、原transfer/arrival/final receipt与未结责任 |
| `ConflictCase/SeasonWindow` | 原scope/evidence root、当事人、法定随机/名册/阶段/时界、决定/执行；或原season/前后窗口、真实结果集root、有限资格及关闭/复核；各自授权隔离 |

完整ref固定world/domain或instance/schema/object/revision/commit；数字带原单位，采用checked非负整数。canonical编码、对象/字节/证明上限先核再解码，未知版本拒绝。同名材料、裸receipt ID、hash、季节序号不能跨world/branch归并。

## 5. 关键运行流程

<a id="conflict-authority"></a>
### 5.1 准入与暴露闭包

`Enroll/Declare` 依次验证：当前成熟区域和明确竞争授权；主体登记/代表/新人保护；精确暴露和原owner/必要第三方同版接受；可用资产/用途且无其他重叠未结冲突；可执行动作/资产与物流adapter；完整时界、独立review/随机/程序预算；行动、责任bond与安全收尾的真实资源。全部通过在一个原事务写参加/宣告、有限holds、消费/费用与receipt。preview/offered/ack均无这些效果。

暴露是数量/用途承诺，不把原资产搬走。新的转让、抵押、生产input或委托writer必须读取当前暴露/处置约束，不能一边承诺可能损失一边许给未同意第三方；已有受保护义务没有合法处分就不能暴露对应范围。更换owner/Agent/组织不让旧承诺失踪，新人/非参与者也不因此成为新目标。活动窗口内只能收缩未承诺的新范围，扩大须独立新同意/声明且不得追溯。

到开始时仍须核当前charter/授权/保护与原参加事实；失效停止新增冲突效果并保留原资金/义务结清，不将旧receipt当无限权限。结束时所有新攻击/扩大提取按时界拒绝，后台结算未跑完不延长窗口。退出停止未来新承诺，既有暴露只持续原期限；原请求未知先核清，不通过新request绕开。

<a id="conflict-defense"></a>
### 5.2 离线防御与效果消费

`Install/RevokeDefenseEnvelope` 绑定原主体当前grant、接受的原冲突与精确范围。每个完整 `(world, conflict_id, participant_id)` 由authority分配唯一稳定lineage并CAS唯一索引，至多一个活动版本；caller不得另选lineage，participant沿原已接受主体，不因换Agent、代表或包名变更。普通16次设置/收缩槽只更新这条lineage的版本，全部版本共享累计已用次数/资源和触发消费索引，旧版本仅留历史。触发必须为对列明资产的已确认合法敌对effect，唯一键 `(world, conflict_id, participant_id, hostile_effect)`；不能以换lineage、revision、request或重装再次响应。自动防御effect带`is_automatic_response`且不能再成为自动反击触发，累计操作/资源cap和原expiry每次重验。

最终`Revoke`使用该主体独立预留的一次停用槽与费用，原子写稳定lineage的`finally_revoked`及当前版本receipt；所有实际响应入口同cut核此标记，停止该主体在本冲突中的全部新防御，不能换ID、退出重入或重装清除。再次撤销只返回原结果，不另耗成功迁移或费用。普通修订/动作预算耗尽不能阻断该最终停用；已经提交的合法响应不回滚。不同participant各有一条lineage与一次最终停用份额，16份覆盖本契约全部参加主体。

响应只可执行已列`protect/defend`或预先选定安全路径的有限撤离；不能自主招募、宣战、扩大target或追击区域外。来源、位置、预算、设备状态、保护或grant失效则拒绝新动作，无部分花费；已经提交的动作保留原结果。scheduler只提交当前授权的候选，模型输出仅建议；同一tick多个触发按canonical顺序消费共同剩余预算。

<a id="conflict-assets"></a>
### 5.3 实际损失、资产适配与身份

原对象适配器至少有稳定原identity、owner/custody、当前位置与完整义务、condition/数量/进度、可用能力、允许有限effects和真实费用；每类能力由已治理的原资产定义发布版本化规则，不能由攻击caller填一个损失数量。首次激活必须具备所宣告所有类别的adapter，缺某类就不能声明支持它；完整目标包括chassis、装备、工厂/设施、库存、未完成工作/局部经营能力，不拿一个资源扣分替全类。

| 原资产类 | 有界真实效果与计算 | 必须保持的内容 |
| --- | --- | --- |
| chassis/equipment/设施结构 | 原asset定义给整数condition ppm、工作阈值、允许impact和每次资源cost。对确有攻击能力、当前合法距离/目标/预算的effect，`d=min(impact_ppm, exposure_remaining_damage, current_condition)`，`condition_after=current_condition-d`；所有运算checked，实际费用与条件同根 | 原body/equipment/factory identity和owner保持；不能删/俘获Agent稳定身份。低于工作阈值停止新的依赖动作，原成功结果不消失 |
| 库存/材料lot | q受原真实可支出数量、已暴露片段、能力cap限制；明确dest为毁损或争夺custody，二者不能同时记；source余额减少、destroyed累计或custody余额增加单次守恒 | 非暴露/受保护第三方量不变，旧source provenance与实际数量保留，不能从同名world库存补齐 |
| 工作进度/WIP | 对精确原job读取已sink/未sink、完成与保护边界，原已接受的domain中断规则给出失效的未完工作、仍可回收量和安全关闭；受保护义务没合法方案就拒绝会伤及它的effect | 已完成production/delivery/milestone不撤销；已sink只由真实回收产生有限返还，未sink不伪称已毁损 |
| 区域控制/局部能力 | 只使原有权许可/设施能力进入已声明的受限或失效状态，具体cause与scope链接真实效果 | 不借能力失效删除charter/tenure/长期合同，原恢复/申诉另有主责 |

`impact_ppm`不是旧war intensity换算：它来自参与前可读且受治理的设备/资产规则，范围`1..=1_000_000`，每次非零效果需声明并实际支付正的原资源成本，动作距离/姿态/位置依原物理authority；缺真实能力、费用或合法目标时拒绝。攻击只在当前提交点产生有限效果，不新增未定义的迟发导弹/范围伤害；若以后增加，必须另完善在途责任和预算后才能激活。

损坏来源同时接收：①本契约冲突实际effect；②原区域危机/受权恢复领域已提交的损坏事实；③原一般经营的折旧、故障、生产失败和受支持中断。读取原writer before/after/原因/receipt建立`AssetLossView`，不为第二、三类执行一次假攻击。根源缺证时分开“当前条件已坏”和“原因未知”，不得编造责任或调用战争权限。工厂原`durability_ppm`与维护/回收可复用，chassis等尚缺的物理状态/adapter按本目标补齐；Agent本体、所有权与历史由原身份合同保持。

<a id="conflict-occupation"></a>
### 5.4 连续控制与到期权利结算

每个已批准空间/用途有唯一控制判据：当前一个合格参加方的已列控制设备在精确scope内、condition达到原工作阈值且本区间所需能源真实可用并结算；对手合格控制设备同时满足时为`contested`，双方都不积累连续持有。只有普通Agent到访没有控制能力的情况不积累。所有移动、断电/补电、设备损坏、资格/授权变化与争点保护writer均更新同cut控制区间；区间记录保留原事实链和序列，无间隙才可证明连续。

在每个已提交有效时间区间读取原实际状态产生`ControlInterval`，不能预付未来能源就记录未来控制。每个`RecordControlTick(conflict,t)`在一次有费用迁移内处理全部至多16个已声明用途；原物理/资格writer的完整变更索引保留区间内每次变更，module核该区间的无遗漏证明后才推进`confirmed_until/last_continuous_start/root/count`。原件追加保存，结束读取已逐次验证的完整prefix与当前head，不把全部历史藏进一个免费对象。一个中间丢失/离场/对手进入事件立即结束当前连续段，回到同位置也不能拼接；服务或来源unknown保留缺口，不凭前后位置补满。在`t>=end`的准入先停新冲突效果，再用完整截至end的区间冻结结果。满足`last_start+H<=end`且持续至end、原条件仍适用才qualified；最后一刻触碰、离散样本或未维持到期均不合格。

若原暴露选择条件转让，执行[tenure两instance结算](location-tenure-and-disposition.design.md#tenure-temporary-use)：已生效的原holder条件同意、当前受让资格/接受、剩余期限/用途、第三方与现行settlement authority全部重验，一次消费result slot、改变精确原tenure holder/revision并保留原结束时间。scope不可分时不能拆半；原资产物理交接另逐项执行。只有同根tenure receipt才是长期权利结果，不以qualified或本地战报代签。

临时使用作为另一个已明确接受的结果类型，保持原holder，按原截止关闭新用途。两类结果不能对同一排他scope同时生效；没有任何合格方时保留原权利，不补选分数赢家。到期结算缺证/未知保持`settlement_pending`，按原结果恢复/有限程序核清，不让双方各自先用一个版本。

<a id="conflict-extraction"></a>
### 5.5 实体提取、最终收益与未结尾部

`LootEntitlement`只可由已授权实际冲突effect为精确source片段产生，旧owner title与保管/可支出状态分别记录。原owner相应量进入不可重复支出的争夺custody，受益者尚无可支出收入。每个source片段维护跨request/冲突/季节的唯一消费索引；可分lot按稳定不重叠quantity区间记账，不用换lot名字复制。

`StartExtraction` 重验原权利/期限、当前custody/位置/真实可载量、合法路径、运输能力与资源、目标保护规则/当前receiver接受/容量及原义务。只生成一次已装载/在途状态和真实source扣移；不是最终ownership。原运输writer保存每个move/损失/改道/到达的原receipt和remaining；不能以普通direct TransferMaterial或一个RPC ack跳过路线。

`FinalizeExtraction`同时核：实际到达的同批剩余量、原entitlement未结、当前合法接收方和受保护目的地/容量、原scope/来源/保护无冲突、实际费用和全部原义务。source custody减少q、receiver可用账增加q、entitlement consumed、arrival/final receipt与费用同一个事务发布；这时法律/经济转移才最终成立。若arrival与final分开，arrival保留不可支出的buffer。目的地满/撤权/变化只允许受支持的等待、原范围显式改道或返还，不能静默换收款人。

未提取entitlement在X截止失效并释放对应未领取custody，只有原owner真实重新可用的量才写returned。已装载或在途仍按已签运输/安全收尾预算完成接收或返还；超原上限先阻新动作，保留当前物理位置/风险/原责任及安全方案，不自动传送、销毁或免费退款。原合法实际毁损保留毁损；归属纠正不能制造已经不存在的货物。安全destination不得成为同时可攻击的暴露目标；新暴露若破坏正在履行的保护接收义务则拒绝，不能让受益者在最后一步失去已声明保护。

<a id="conflict-recovery"></a>
### 5.6 三类原因下的修复、重建、转向与退出

从同一当前cut读取conflict/regional_crisis/ordinary_operation损失、本人已接受目标、原账本/占用、位置/用途、受保护责任与合法动作，构造只读比较。没有冲突的普通损坏不创建ConflictRecord、不锁战斗bond，也不需要conflict.review；原领域争议/补偿沿其自身授权。

修复模板固定原对象、目标condition/能力及原维修规则：按原`repair_step_ppm`和真实材料/电力quantum逐步计算，`n=ceil((target-current)/step)`，每步实际有预算和可用位置；不能超过原max condition。重建模板列合法新site/tenure、原construction recipe/投入/时间、新identity和旧残骸/WIP处置；不能复用retired identity或自动恢复旧许可/库存。pivot先核原目标fit，不匹配须显式新目标；退出只取消未承诺的新工作，逐项选择合法取回/回收/保管/安全关闭。

每条候选按同一源独立overlay顺序核成本与资源峰值，已损失/已sink/仍hold/可回收分别列；不同候选不能同时花同一资源，待收回物资只有其前置receipt实际可达后才可供后继。跨asset/batch/location、原资源单位、因果和每步未知都保留，不能只用净成本掩盖第一步资金不足。当前真实原规则实现可复用[生产恢复比较](production-recovery-comparison.design.md#production-recovery-candidates)的有界读取/顺序求值，但该文未覆盖类别需按本资产adapter补齐。

一次比较最多3条明确选择的候选，每条≤8原动作/完成边界、4种原资源与8个原资产/义务；选择者给exact合法候选，不扫描全世界寻找“最优”。有限找不到不证明全局必须外援。至少两条实际可行的受控样例用于独立比较，真实世界没有安全路线时返回具体缺口、保留价值、延期/重新定目标；不递归推荐用缺失能源制造同能源。实际选择仍逐步提交原领域动作、fresh重验并查原结果；比较本身不恢复、不扣费或奖励。

原身份/历史持续可读；chassis重建只绑定新的物理承载，在当前owner/授权下恢复相应行动能力，不创建新Agent洗掉历史。区域危机可依原containment参加CC-003恢复项目，但其贡献/奖励和本人修复分别结算，不把普通维护receipt当系统恢复项目完成。

<a id="conflict-review"></a>
### 5.7 独立复核、真实支付与程序尾部

按[原政策](../../game/gameplay/gameplay-chartered-conflict-recovery-and-soft-season-contract.prd.md#conflict-review-policy)在激活前验证**目标** `regional-conflict.review`授权，绑定world/region/事项/资产cap、政策、完整名册/冲突排除、不可预选随机和费用。当前源码没有因为本设计就具备这一制度；旧war、tenure、服务、RD panel或管理员不可代签。每直接主体每阶段保留8证据引用，case scope/当事人/固定C不因重命名更换；证据是原签署、授权、物理/账本/时间/程序事实，不用声誉或缺席自动判责。

每effect/片段只登记一争点，冻结完整scope/evidence root后消费首个合法随机排列，初审/复核各3名不同合格人，至少2个相同typed disposition。早期保全只锁未结争议片段的不可逆处理，不冻结无关资产/权利。初审、程序申诉、final、执行和实际付款独立存储；请求、一个投票或tail tick均不代表支付。

证据/申诉区间严格半开，按canonical受理时间而非客户端发送时间判断。初审不直接执行赔付/撤销权利；至C+4Q窗口已闭，完整受理索引证明无待决合法程序申诉，或有效复核已经完成，才成为executable_final。执行和该判断同一current cut/CAS；此前已受理但仍待处理的申诉或unknown结果阻断争议项不可逆处分。C+4Q的新申诉超窗不回填，原安全收尾与无争议义务继续。

允许处分：确认/拒绝原条件资格、撤销尚未结的entitlement/临时grant、缩小保护、在原责任bond与实际证明损害内分资源赔付。因果/责任/数量证据不足不得判罚；付款使用原真实payer、受保护预算及recipient，`min(proven_loss_r, accepted_liability_cap_r, remaining_bond_r)`且不能重复同一损害。已合法实际损失不物理回滚，已结第三方交易不追溯扣回；不足部分如实记录有限补偿，不能另造无资金赔款。

到C+6Q先查全原case/决定/执行/支付/未知请求。已有final保留并继续合法待付；可证明无有效改变基线结论时解除程序保全，按原权利/到期规则处理未结量，未使用bond/费用原路释放。unknown保持原恢复fence，不当作无责任；同一resolution key只结一次。之后客观来源更正追加事实和消费者重验，不刷新一次程序窗口或重复有损处分。

<a id="conflict-season"></a>
### 5.8 软赛季关闭、下一窗口与有限申诉

`SeasonPolicy/Window`在开放前固定同一world/branch、season身份/前身、时间、区域/参加上限、实际结果predicate、临时权利种类/消费点、费用/奖励真实来源、关闭和复核。后来的规则只对新窗口/明确新承诺生效，不能调整旧分母或补选赢家。每个result slot以原source effect/quantity/claim role去重；角色由原source authority发布，不由赛季caller创造来重复claim。

临时资格一经接受就绑定原window/expiry；所有实际消费者每次检查当前有效期/撤销/使用量，即使关闭扫尾未运行也拒绝过期新效果。到end停止新计入并固定原结果范围/root；用有界页按稳定顺序计算原指标，缺记录/未知writer保持incomplete。旧普通tenure、长期合同、进行中生产/运输、未结提取/付款和程序仍按原截止/责任继续，不因换season释放同一钱或发第二份权利。新window只有当前有效授权才开放，不能由旧ended状态自动生成World、资源、身份或新能力。

刷新/更正必须有清楚的原资格影响范围：未生效旧资格关闭；既有world effect不可撤回；尚需旧资格来完成安全/原义务的最小收尾按原签条款执行，不接受新scope。资格变化不能没收非参与者资产、基本独立能力、可迁移价值或历史。排名不自动写reputation/治理权/tenure；受权奖励另核原资金/资格和单次effect，缺来源保持无奖励事实。

每window预留独立的**目标** `season-window.review`授权、当前名册/随机和程序预算；它能审查原结果遗漏/错误归类、资格时界或程序，不具有战争处分或历史重写权。C=season.end，沿相同2Q证据/3Q初审/4Q程序申诉/5Q复核/6Q尾部规则，另用独立case/key和权限。更正追加版本化结果/理由，影响未结资格须在原允许范围内，不延过期权利或重复已结奖励。原receipt客观纠错不再发一次收入；需要经济纠正必须有原奖励authority的有资金linked处分。没有有效结论时保留已核基线，缺证条目仍unknown，不能靠尾部改成零；原请求恢复和异议读面保留。

## 6. 接口与数据合同

| 目标接口 | 当前必需输入 / 原结果 | 失败语义 |
| --- | --- | --- |
| `Inspect/Enroll/Declare/LeaveConflict` | 当前scope/保护/参加接受、资产/义务索引、Q/W/H/X、grant、费用/closing预算、稳定operation | `unauthorized / protected_subject / immature_region / exposure_conflict / source_incomplete / invalid_window / funding_missing`；拒绝无部分暴露/扣费 |
| `Set/RevokeDefense/ApplyConflictEffect` | 原scope/current版本、hostile effect、精确能力/资产状态、累计cap、当前位置与费用 | `expired / revoked / lineage_conflict / defense_finally_revoked / trigger_consumed / response_chain_forbidden / out_of_scope / insufficient_resources`；拒绝不消费响应资格 |
| `RecordControl/FreezeOutcome/SettleTenure` | 完整控制区间/否定事实、原条件同意、结束cut、当前tenure/receiver/保护 | `control_gap / contested / last_moment_touch / tenure_expired / settlement_pending`；只有两侧原receipt确认转移 |
| `Start/Redirect/Finalize/ReturnExtraction` | exact source slice/custody/transit/arrival、真实目标接收/容量/保护、原entitlement与费用 | `destination_full / protected_claim / route_unavailable / original_result_unknown`；保留实际位置/剩余 |
| `InspectLoss/CompareRecovery` | 三类原因原来源、当前goal/asset/ledger、有限候选与原规则 | `cause_unknown / unsupported_adapter / needs_goal_change / no_feasible_route / source_budget_exceeded`；不执行修复 |
| `Open/Close/InspectSeason/RegisterReview` | 原window/current授权、完整结果root/资格/消费、case/期限/预算 | `source_conflict / incomplete_results / eligibility_expired / wrong_jurisdiction / window_closed`；不重置世界 |

所有unknown保留原operation、scope、当前可证事实和下一核查；不可见不等于不存在/零，错误不靠默认成功填空。三入口响应含当前世界/subject、来源cut/时效、原request/receipt、已损失/占用/在途/已结、真实下一步与当前费用/截止；不能由UI或Agent推断缺失物理结果。公开环境和私人资产/案件材料按当前audience区分。

## 7. 状态、事务与持久化

<a id="conflict-persistence"></a>
参战、交战、占有、权利结算、运输、争议和季节各有独立状态；`ended`不表示全部资产/款项结清。结束只关闭对应新权限，并保留逐项责任/actual remaining/current source/原结果。原scope/head/hash、participant/asset roots、调用版本、预算、计时/暂停prefix、证据/随机/决定、source消费键和全部最终receipt保存同一完整generation。

所有有世界效果的动作经原ExecutionTransaction准备完整写集：当前授权/保护、instance状态、原资产/资源/容量、root/index、水位、费用/hold、receipt/journal/outbox一次发布。原operation/同payload重读，异payload冲突；不同operation仍不能重复同source片段、响应触发、结果slot、case resolution或payment key。两个攻击/维修/转移抢同asset revision、两方control/tenure结算、start/expiry和revoke/defense依canonical顺序CAS，后者重验当前状态，不能同时消费旧余额。

raw/trusted/native/tick/module/post-event入口均经同一current guard；只在UI按钮或公开action入口校验不足。外部通知/实际支持的异步运输adapter效果在commit后沿原durable outbox与稳定effect键派发；ack/timeout不是世界业务最终结果。若该物理域本来同world同步提交，则使用同root的原domain receipt，不额外造网络ACK authority。

replay应用canonical原效果/费用/索引，不重跑模型、抽签、损伤算法、控制未来区间、搬迁、赔付或自动发下一赛季。物理/资源恢复与制度记录同代；未知/缺失adapter或artifact不得部分加载再允许mutator。G1按[原generation设计](runtime-storage-footprint-governance.design.md#storage-generation-g1-design)区分NotCommitted/Committed/StatusUnknown，后者fence整个不确定旧World所有mutator/save/依赖派发，核清完整原候选并对齐内存/index才解除。仅客户端ACK未知则查原operation，不假定未提交、退款或重复攻击。

## 8. 部署、安全与运行约束

新能力是目标受治理L2合同，当前War=Defer与原保护保持。激活必须独立具备合法分类/授权、准确artifact/schema/instance、完整原资产/tenure/物流/资金reader-writer、三类review名册/随机/预算和当前语义一致的三入口。先声明有限真实scope与容量，实际跨节点/响应/恢复/玩家证据不齐就不能宣布战争重启；本文件不颁发授权。

访问只由原身份/当前grant与case/audience验证；panel按原案件/任期最小取证，不获得对无关World的查询权。只读预算/超时/分页/缓存按subject/cut/scope绑定；结果返回前重验current权限/变更head，漂移返回stale，不能借旧case读取前任私有目标。旧history应可审计但不默认公开；加密/私密性沿原存储/访问合同，不由普通hash承诺。

## 9. 质量、容量与可行性

<a id="conflict-budget"></a>
以下冻结本目标的工程容量及完整计量方法，不用技术cap排除已接受的直接权利人，不宣称生产性能已验证。容量不足在参加/宣告/开窗前拒绝新scope；不能事后删掉证据、把一次完整权利拆成多个ID规避。

| 维度 | 有界合同 | 失败/保留 |
| --- | --- | --- |
| 一个冲突或季节window | ≤16直接参加/权利主体、64暴露资产或结果槽、32并行lot/transit、16地点用途、256实际冲突/响应effects；交战W≤4096有效ticks，current instance≤128 KiB | 新效果超过累计cap停止，原已结/安全关闭/原结果查询不被清除；非参与者仅被保护，不列入参加额度 |
| 一个案件 | 16主体×每阶段8证据×2阶段=256摘要；初审/复核各3人，原直接主体阶段份额独立 | 原文存原canonical journal，当前root/count/摘要有界，超过上限先拒绝准入scope |
| 一次原读/计算 | ≤256真实对象/证明节点、累计raw+decoded≤4 MiB、≤32次读取/1024索引访问、≤2秒；不预扫/clone | 任何嵌套asset/grant/claim/absence/current head/fee都计入，超量incomplete/no effect |
| 一次制度转换 | ≤2声明invoke，每次总input≤512 KiB/output≤256 KiB/state≤128 KiB；真实manifest更小则从小 | 所有authority验证/原费用在闭包内，第三次调用不隐藏或免费 |
| 内存与并发 | 每槽8 MiB、4槽≤32 MiB任务内存；含迟到I/O/两调用/发送；原结果查询独立2个1 MiB槽 | 满槽拒绝新比较/效果，原结果仍精确key恢复；底层固定开销另验 |

代表性current state上限：16主体×256=4 KiB；64资产/结果槽×384=24 KiB；32lot×384=12 KiB；16控制头×256=4 KiB；256证据摘要×96=24 KiB；6投票×512=3 KiB；policy/时界/预算/head与原请求摘要保留32 KiB，共103 KiB，小于128 KiB。已完effects的完整原件追加到journal，current state保留完整消费prefix root/count与当前项；每次新effect验证原未消费证明/追加root，不能只保留最后一个effect而忘记去重。源码实现须以编码实测确认，必要字段超量即不准入。

4 MiB source/proof + 两次512 KiB input + 两次256 KiB output +64 KiB request +1 MiB scratch=6.5625 MiB，剩余覆盖当前state/发送；上限仍计底层真实持有buffer，不以压缩wire小于解码空间为由忽略。读回、负例和原结果恢复也计服务资源，不能通过无限轮询保持免费全表查询。

在参加前真实payer分别保护行动、程序、责任bond和关闭预算。一次冲突最大成功迁移计量上界：16参加+1宣告+256防御设置/收缩槽（每参加者唯一lineage至多16个版本）+16独立最终撤销槽（每参加者1次关闭整条lineage）+256冲突/响应effects+32lot各4迁移+16地点各2结果+256证据+1冻结抽签+6投票+16申诉登记+1final+32各1付款/释放+1关闭=1018；另预留128次原资产/义务安全核查/尾部处置，并计入至多W次`RecordControlTick`，共`1146+W<=5242`个迁移额度。每额度按最多2次invoke及原fee schedule的完整input/output/compute上界报价，实际控制能源、运输/维护成本另逐资源保护，不含在“调用次数”里。最终撤销份额不得被普通设置挤占，已撤销再次同请求只读；超过普通修订或4次lot操作cap停止新范围，仍有原安全关闭预算，旧累计不重置。技术失败/重读不占成功槽也不重复扣业务费。

season复核同样保护16主体两阶段/关闭份额；指标的真实来源读取与奖励writer另按原域预算，不能借仅16主体隐瞒64结果槽及全部proof。current fee上升超已签cap时暂停新效果并报价/重新授权，既有真实责任与原保护的安全结清预算保持；原钱不足时不得用预期战利品、未来收入或他人余额垫付。未用份额只有原operation和全部对应义务已明确结清才原路释放。

## 10. 兼容、迁移与回滚

旧war active/score/reputation不能迁为已同意参战、资产暴露、控制区间或loot entitlement。新合同激活时旧战争继续原兼容轨道或按原有权方式结清；两者不得对同source/范围并行造成新制度效果。迁移必须证明原独立资产、holder、债务/权利与真实记录，不从历史字符串补造同意。

升级保留逻辑instance、原policy、scope、source/付款/持有消费keys、原时界/证据/结果与资产identity；新artifact不能重算旧窗口输赢。回滚/停用只收缩后续新权限，已发生损失、付款、tenure转移、合法提取与历史保持。现版本不能安全处理旧记录时停止新承诺，由当前明确受权的兼容/关闭路径处理；不恢复旧可写World或任意运行撤销代码。

## 11. 验证设计与可追溯性

<a id="conflict-verification"></a>
完整独立方法见[验收手册](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#conflict-scope)。只读文档检查是S0；required/full和真人结果尚未执行。所有期初/实际delta/期末、主体保护、原receipt/fee和身份由独立预期核算，不从被测module输出复制答案。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | [当前准入](#conflict-authority) | 合法charter/主体、精确暴露、公示和非参与/新人/第三方保护 | [CF-01](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#conflict-admission-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 原授权/接受/暴露/hold/拒绝前后状态 | 联盟身份、报价或上线状态不构成参战同意 |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | [离线防御](#conflict-defense) | 触发单次、累计cap、独立最终撤销额度/费用和无递归反击 | [CF-02](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#conflict-defense-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | trigger消费、实际actions/费用、cap耗尽后撤销 | 模型建议不得扩大目标；自动响应不递归触发 |
| [CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | [实际资产与来源](#conflict-assets) | 冲突/区域危机/一般经营三原因的真实损失、全部声明资产类及身份历史 | [CF-03](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#loss-recovery-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | condition/量/原因/保留价值与稳定身份历史 | native资源分数不能替物理结果，不能删除身份 |
| [CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | [恢复与独立退出](#conflict-recovery) | 三类原因下repair/rebuild/pivot/退出、逐步成本及真实恢复 | [CF-06](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#loss-recovery-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 两实际路线/无安全路、完整成本与恢复receipt | 外援可选；不要求先参战或免费复活 |
| [CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | [连续占有与权利](#conflict-occupation) | contested occupation、持续H至到期、原条件接受和实际tenure转移 | [CF-04/TN-07](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#occupation-extraction-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 完整控制prefix、原/新holder及同根权利receipt | 临时控制不代长期权利；不延原期限或没收第三方 |
| [CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | [实体提取](#conflict-extraction) | source片段/custody、真实路线/arrival/保护接收、最终收益与在途尾部 | [CF-05](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#occupation-extraction-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | source/custody/transit/destroyed/destination独立账 | direct transfer/ACK/装载不代签物理交割 |
| [CC-005](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-005)；Trace refs: O7-REQ-ce9fbace-7fc6-4973-8bcf-8b58e11b3339 | [软赛季连续性](#conflict-season) | 预告刷新/时界/复核，保留world/身份/能力/价值/历史及原义务 | [CF-08](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#review-season-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 同World/原history、结果/资格root与未结恢复 | 不造新World、回滚资产或自动奖励 |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | [有限授权复核](#conflict-review) | 原授权/范围争点、独立随机panel、final guard、最小保全和程序尾部 | [CF-07](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#review-season-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | scope/evidence/受理索引、投票/final与真实权限 | 他域panel/管理员不得代签，不形成一般法院 |
| [CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | [有限责任与赔付](#conflict-review) | 冲突实际因果/责任/损失、真实bond赔付与原结果单次保留 | [CF-07](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#review-season-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 独立损害/cap/bond与实际付款/剩余 | 不凭缺席判责，不免费复原或无预算追扣 |
| [CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | [未结权利争点](#conflict-review) | 未结entitlement/临时用途精确保全、有限处分与实际提取保护 | [CF-07](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#review-season-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 原片段/决定/已结与未结权利对照 | 不回滚已合法第三方交付，不扩大为全资产冻结 |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | [原事务与恢复](#conflict-persistence) | 全流程current guard、原请求/并发/两instance/G1三态不重效 | [CF-09](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#recovery-capacity-cases)；full；固定同一实现候选/原世界、当前授权与真实来源 | 同代root/receipt、完整写集/费用和未知提交恢复 | S0或旧war测试不证明新组合/跨节点通过 |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | [有界来源和关闭预算](#conflict-budget) | 全部source/证明/两invoke/控制tick、普通与撤销/复核/关闭资金计入 | [CF-10](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#recovery-capacity-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 编码/对象/字节/时限/并发边界与1146+W复算 | 不免费控制区间/第三invoke，不全扫后截断 |

## 12. 决策、风险与实施结束条件

选定受治理L2的完整有限冲突流程、原domain真实资产effects、条件tenure转移与实体提取分别结算、三类损坏共用真实恢复输入，以及显式season资格边界。没有以临时占领替长期权利转移，也没有把一般经营恢复变成参战前置。

设计义务的结束条件是原CC001/002/004/005与GR007的政策/数据/执行/拒绝/预算/恢复和方法关系完整，主要正向与失败组合不再挂未设计的tenure/资产处置authority。实现结束仍需所有声明资产类型与原writer、完整有界来源/索引、两instance原子性、真实资金/复核/物流、当前三入口和手册同候选证据；有界示例不能替全声明范围。战争重启另需原治理分类、独立平衡和玩家证据；任何production参数、物理adapter或权限来源改变须重核相应组合，不能由文档active推导可发布。
