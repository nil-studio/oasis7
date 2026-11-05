# 区域冲突、损失恢复与软赛季玩法合同

- Owner role：`gameplay_designer`；Runtime、区域/资产、授权、QA 和玩家体验 owner 共同审读。
- 生命周期：`active`；交付目标政策与验收，不声明实现、平衡、玩家可用或发行。
- 固定输入：`eng-cc/oasis7@cca8cda97cf121ebc1e25e96d6b0204a421323ea`，2026-11-05。原产品范围是[区域冲突专题](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；地点权利政策只由[区域 tenure 合同](gameplay-regional-charter-tenure-funding-contract.prd.md#tenure-policy)拥有。
- 设计适用性：`simple-topic-exemption`（`PRD-only-sufficient`）。本文件拥有玩家选择、冲突/损失/赛季政策和验收；执行、状态、预算、读面、恢复由[冲突连续性设计](../../world-runtime/runtime/chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-authority)及[tenure 设计](../../world-runtime/runtime/location-tenure-and-disposition.design.md#tenure-identity)承接。

## 1. 问题、目标与范围

玩家加入的是有期限、范围和代价的区域竞争。一次宣战、地图上的占领标记、已装船的货物或赛季排名都不能替代真实权利和世界结果。目标是让参与者可以在明确暴露的资产上竞争，在损失后比较修复、重建、转向或退出，并带着身份、历史和真实剩余能力继续同一个世界。

本文一起承接 CC-001、CC-002、CC-004、CC-005 的完整目标流程，不扩写四套独立玩法。CC-003 的系统性危机恢复项目继续由[原设计](../../world-runtime/runtime/bounded-recovery-projects-and-contribution-settlement.design.md#recovery-authority)拥有；普通战损不自动满足 containment 或项目参与/奖励资格。

当前[总纲 §0.6](gameplay-top-level-design.prd.md#06-历史-closure-归并后的稳定边界)和[历史战争数值边界](gameplay-top-level-design.prd.md#94-历史战争政治数值基线边界)继续适用：战争不是当前 player-facing 主线，任何重启须独立平衡与玩家证据。Runtime 的 [War=Defer](../../world-runtime/prd.md#native-surface-分类与冻结规则)不因本文改变；不选择 SC-32 pilot，不把新制度塞进旧 native War。旧成本、评分和结算仅按[原 baseline](gameplay-war-politics-mvp-baseline.design.md#des-war-politics-001)解释，不作为新资产破坏或征服权限。

<a id="conflict-enrollment-policy"></a>
## 2. 参战、宣告与有界承诺

Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3；[CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)。

### 2.1 先读清楚再参加

只有当前有效、成熟且明确允许该类竞争的区域 charter 可提出冲突窗口。参加者可以是自然主体或有当前精确代表授权的组织；组织成员身份不等于同意暴露个人资产。加入前签署区域、参加主体、可用 Agent/设备、可被影响的资产/数量/权利、最大损失与真实资源预算、期限、退出、离线防御、争议与费用。默认不参加；新人保护或未成熟区域不能靠地方同意解除。

资产逐项证明合法 owner、custody、位置、条件和第三方义务。工厂列入不自动包含厂内他人库存；机体列入不包含 Agent 身份、记忆、一般 credential 或全部钱包；tenure 列入只涉及原已明确接受的临时使用或条件转让范围。抵押、受托、在途或有受保护第三方请求的资产，未取得其原权利允许的精确处分与必要同意时排除。没有完整清单或来源时保持不可参加，不能先勾选“全部资产”后再解释。

一份已接受的参战条款允许按原规则被指定为目标，不要求每次收到攻击前重新在线同意。它不允许后来扩大区域、资产或攻击类型。退出请求立即停止未承诺的新扩张；已进入公示窗口的暴露只持续至原固定结束，不重新计时。撤销自动防御立即停止新的代办，不能撤销已确认损失或让已承诺暴露提前免疫。转让、换 Agent、离开组织或下线均不隐式转移原责任或刷新窗口。

### 2.2 宣告、时间与选择

宣告列同一已授权区域、双方参战条款、目标对象/用途、开始/结束、操作及损失上限、占领条件、提取与结清窗口。预览只有估计和风险；其后所有前置在提交点重验。宣告只有一个权威 receipt 才生效，不能由聊天、治理票数、action ACK 或旧 war quote 代签。

使用原世界有效时间。已授权政策给出正整数响应单位 `Q`；公示至少 `2Q`，交战窗口 `W` 满足 `2Q <= W <= 8Q`，占领连续证明窗口 `H` 满足 `Q <= H <= W`，停止新冲突效果后的提取期 `X` 满足 `2Q <= X <= 4Q`，随后 `6Q` 为固定争议程序上界。具体值在参加/宣告前可读并接受，checked 加法溢出拒绝；这些关系是目标边界，真实 tick 长度、响应可及性与平衡须另验。旧 native 的 `6+2i` 不被改写或冒充这些期限。

同一暴露资产/数量片段和同一排他临时用途至多处于一个未结冲突中；多个无重叠冲突可并行。参与者可继续无关生产、公共通行和原义务。费用、资源 hold、行动预算与责任 bond 分开；花掉的费用不是可退余额，未来战利品不能先给当前行动融资。若费用或安全结清资金不足，拒绝新承诺并保留已有合法收尾，不能收费之后才发现无恢复路径。

<a id="conflict-defense-policy"></a>
## 3. 离线防御与实际冲突后果

Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3；O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c；原 [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001) / [CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)。

### 3.1 防御授权

玩家选择 `defend_declared_assets`、`withdraw_to_declared_safe_destination`、`stop_new_defense` 或保持不授权。防御包固定原冲突、受保护资产、允许的设备/Agent、触发的已确认敌对效果、最多次数、逐资源限额、移动路径/距离上限和结束时界，截止不得超过交战结束。它只能响应对已列资产的实际敌对行为，不能以对方出现、社交关系或预测风险主动开战；不能追击到区域外、增加目标、重新招募或自动续期。

每个参加主体在同一冲突中只有一份持续的防御授权；普通设置或收缩只更新该授权版本，所有版本共享已用次数、费用、剩余上限和已消费触发。换 Agent、包名或请求标识不能另建一份并行授权。最终撤销一次停止该主体在本冲突内全部新防御代办，不能靠重装或换标识重开；其停用费用在参加时独立保护，不受普通修订额度耗尽影响。每个敌对 effect 最多消费一次响应资格；同一防御 effect 不能再次触发新的自动反击链。新反击必须有新的、独立的合法敌对输入且仍在原总预算内。撤权、期满、预算不足、来源未知、目标或路径不合法时停新动作并给出可读原因；已提交原操作继续查原结果。模型建议不改变包的范围，玩家回来可看每次触发、实际花费、保留/损失和下一步。

### 3.2 损失和终局

新冲突以实际有限效果为真值，不以联盟分数自动选出必须存在的赢家。交战结束只冻结该窗口已经确认的损伤、持有、消耗和未结运输/争议；双方可能都未达到目标。旧 WarConcluded 的比分及 Electricity/Data 后果不能改名为新 chassis、equipment、factory 或库存损失。

结构对象保留稳定身份、原 owner 和条件历史；合法损伤降低条件/可用能力，达到停用阈值停止新工作，不删除已完成成果或身份。可被摧毁的库存量分成仍可取用、已移入争夺 custody、真实毁损三部分，总量与原来源一致。一个容器内未暴露的独立资产不得一并受损。已经消费的 WIP/进度按其原生产/资产政策处置；未完成部分没有自动退款。实际生效后的后悔、退出、赛季结束、申诉或断线都不能把损失当未发生。

目标效果的有限伤害、消耗和停用参数来自激活前已冻结的资产能力/冲突政策。玩家能看量纲、范围、成本和上限；来源或适配器缺失时该效果不可提交，不能用强度、动画或日志文本猜数量。无敌对行动也不保证资产不受其原正常折旧、费用或其他已承诺风险影响。

<a id="conflict-occupation-policy"></a>
## 4. 占领、持续持有与安全提取

Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d；[CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)。

### 4.1 占领不是永久产权

占领对象须是已同意暴露的地点用途或可转移 tenure 范围，原 holder 在参加前明确同意临时使用或条件转让、最大暴露、剩余期限、受让资格和第三方义务安排；原 tenure 必须允许这些结果，不冲突于公共通行或安全接收目的地。权威空间/控制事实证明 contested occupation 的开始、每个有效区间和中断。一次到访、占领按钮、图标或几次离散采样不能填补中间未知。中断重置未完成连续窗口；旧段保留历史但不拼接。

临时使用只限原用途，截止为声明的临时期限与原 tenure 期限两者较早者，不改变原 holder。**条件转让**还须在冲突结束时仍控制该范围、此前连续满足 `H`、有完整无中断证明，并具备原 holder 的已生效条件同意、当前受让资格/同版接受、真实剩余期限和第三方保护；到期冻结的证据只是结算输入。冲突与 tenure authority 在同一事务登记一次原权利变更 receipt 后，受让人才获得剩余期限/用途内的长期建设权；不得延长原期限、取消维护、继承未同意债务、处分独立资产或封路。没有满足条件则不转让，不根据总分另选赢家；最终结果尚未知保持待结，不能让双方同时主张同一用途。

临时用途到期首先停新用途，已发生合法结果保持；条件转让后的原资产、在场货物和待办仍按声明清单与原[tenure 处分](gameplay-regional-charter-tenure-funding-contract.prd.md#tenure-recovery-policy)交接，不能从地图移除就算腾空。受让人也须遵守实际占用、取回保护、原结束时间和后续续期规则。未参加冲突的普通 tenure 取得/续期/转让沿原政策，不能凭占领积分绕过登记。

### 4.2 战利品的物理交割

合法冲突结果只能为精确资产/lot 片段建立待提取 entitlement 和 custody；此时原资产不能又由原 owner 支出，也不能由获益者当收入支出。提取者在原期限内选择当前获权、在冲突目标之外、具有接收容量和保护规则的安全目的地；接收方明确接受该批次。保护是原世界访问/处分规则，不意味着免费、无限容量或绝对免疫。

领取、装载、运输、到达、入库及 entitlement 最终结算分别有事实。真实运输可能消耗能源、容量和时间，可能按原运输规则损失或阻塞。只有源端 quantity/custody、真实 arrival、当前接收权/容量和原 entitlement 同根核验，才可一次把实际剩余量变成接收方可用库存并记最终收益。途中、到达但未接收、满仓、撤权或请求未知均不算收益；换目标需要有权且未结部分显式重报价，不能复制 cargo。

`X` 结束不销毁在途资产：停止新提款和扩大提取，已装载且真实在途批次按原合同完成安全收尾或返还，仍受原物理与费用上限；未领取 entitlement 失效，原 custody 解除后原 owner 才重新可支出。尚未回到原 owner 可用账本的返还标记为 `return_pending`。归属争议只保护对应未结片段；实际毁损和已合法最终入库不回滚，后续纠正须独立有权、有资金的 linked 处置。

<a id="conflict-recovery-policy"></a>
## 5. 损失后的独立恢复与自然退出

Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c；[CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)。

恢复覆盖**冲突、区域危机和一般经营能力损坏**三类原适用原因，从同一当前损失记录、本人目标、资产/资源账本及原义务开始，分别显示可恢复条件、已损失、已承诺、仍可取回和未知。普通折旧、停产或危机后损坏不要求先参战、建立新冲突或取得战争权限；它们以原真实原因/结果形成恢复输入。每条路线须列出资源/时间、真实权限、必要步骤、预期恢复的具体能力、保留价值及不能恢复的部分；quote 不修复世界。

| 路线 | 适用与代价 | 成功依据与下一步 |
| --- | --- | --- |
| 修复 | 原对象仍可修、保留原身份与合法场地；投入当前有源的维修材料/电力并按原维修量与上限执行 | 原条件或能力真实提高的 receipt；不返还已消费投入，不重发原产物/奖励 |
| 重建 | 有合法新位置、建设资源/权限、可负担输入和时间；新对象使用新身份，旧残骸/债务另处置 | 新对象真实建成及可用前置；旧历史不换成新履历，不自动恢复旧许可、库存或进度 |
| 转向 | 原目标仍可满足则比较实际替代能力；目标改变须明确接受新目标和风险 | 真实替代产出/用途，而非更换标签；原损失与义务继续，不能用新目标清账 |
| 退出/保留 | 停止未承诺新投入，完成受支持的安全收尾、取回/回收或保留 custody；世界仍继续 | 逐项已结/未结及原结果入口；无额外奖励，不要求入会、被赞助或一直在线 |

没有可负担路线时明确展示资源/权限缺口和重新规划途径，不生成免费材料、不建议递归调用同一个不可负担修复。至少两条不同可行路线的受控样例用于比较取舍，并另有无安全路线样例；真实玩家环境不保证永远同时有两路。普通[生产恢复比较](../../world-runtime/runtime/production-recovery-comparison.design.md#production-recovery-candidates)可复用原规则，但其 native 生产范围不代签 chassis、装备、争夺 custody 或地点处置。系统性危机后符合原 containment 与项目资格时才可另行参加恢复项目，贡献/效果仍按原 CC-003。

<a id="conflict-review-policy"></a>
## 6. 争议、程序上界与费用

Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3；O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c；O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d；分别沿 [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)、[CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)、[CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)。

激活前必须具有独立 `regional-conflict.review` 授权、合法名册、不可预选的可验证随机和程序预算；tenure、服务、RD 或普通投票不能代签。每个冲突一份有界案件目录，原直接权利/责任人均有保留的证据份额；对象或主体超过技术容量则不准入，不能把受影响者排除出复核。每个原 effect/资产片段最多一个争点，重复请求合并并保留原时界。

以原提取结束 `C` 为固定程序起点，`C+2Q` 冻结已登记争点/证据，使用提交 scope 后首个合格随机结果固定排列；初审三名无利害关系者至 `C+3Q` 两个相同合法投票才成决定，`C+4Q` 前可提一次程序复核，由不同三人至 `C+5Q` 决定，`C+6Q` 为尾部。当事人、其 controller、直接利益人、原批准者排除；不能换 ID 重抽或择有利辖区。交战中可提前登记并最小保全未结片段，不能提前用尽证据窗口裁掉后续合法争点。

证据/申诉在其截止前取得 canonical 受理，采用半开区间；截止时刻不能凭客户端发送时间追填。初审只有在 C+4Q 申诉窗口关闭且完整原案证明没有已受理待决复核，或有效复核已经完成后，才能成为可执行 final。赔付、撤销未结 entitlement 或权利的不可逆执行须在同一当前提交点重验此条件；末刻合法申诉或原结果 unknown 不可被遗漏。安全收尾和无关义务保持。

裁决只可确认原授权/实际效果事实、撤销尚未结算的 entitlement 或临时用途、缩小保全、按参战时已保护的责任 bond 对可证实损害作有限补偿。不得再扣无关资产、造无预算债务、反向扣回已合法完成的第三方交付或恢复已毁损物。待定损害金额不能由声誉/缺席决定；补偿逐资源且不超过原暴露/责任 cap，实际支付 receipt 与认定分开。不能确认责任时保留原已验证事实；没有合格决定的尾部释放程序保全，未提取权按原到期规则处理，有效 final/未付责任和原结果 unknown 优先保留。错误来源的客观更正可追加，但不重开无限有损审理。

参加前分别保护行动费、程序费、责任 bond 和安全结清资源，初审/复核/关闭不得互相挤占；未使用份额在精确原效果结清后原路释放。拒绝新动作不没收 bond，程序尾部不自动证明无责任或全额赔偿。读者始终能区分 `decision_final`、`payment_pending`、`paid`、`return_pending` 和 `outcome_unknown`。

<a id="soft-season-policy"></a>
## 7. 软赛季与跨窗口连续性

Trace refs: O7-REQ-ce9fbace-7fc6-4973-8bcf-8b58e11b3339；[CC-005](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-005)。

软赛季是一份预先公布的竞争/公共项目窗口，不是新 world。它固定同一 world/branch、原 season identity、起止、参加/计分条件、结果来源、临时资格清单、容量及下一窗口关系。入账只消费真实独立结果的稳定 identity；同一产物、effect、贡献或投递不能因跨窗口、复制 receipt 或改 claimant 再计分。排名只解释原预定指标，不自动给钱、reputation、治理票权或永久 tenure；任何奖励/资格都须有其已声明的独立授权、来源和预算。

窗口结束停止新资格消费和新结果计入，冻结原结果集与完整来源；计算尚未完成时显示待结而不是把缺证计零。新窗口从其原授权开始，旧临时资格不会被静默续到新窗口。按生效时界核验资格，即使旧状态尚未后台归档也不能继续使用。已经接受的长期合同、资产、生产、运输、未结提取、程序权利和责任继续沿原截止和原 receipt 收尾；不通过赛季边界重复退款或结算。需要新窗口资格的新动作必须重新取得，不能继承旧报价。

同一主体可选择不参加下一窗口而继续独立生产、取回和恢复；旧成果与历史保持可读。世界 identity、因果时间线、Agent 身份、个人 credential、可转移价值和未结义务不重置、不换世界、不由管理员选择性没收。规则变化只对新窗口/新明确接受的承诺适用，不能追溯降低旧成本、改结果来源或抹除旧失败。

## 8. 玩家验收与证据

独立方法见[冲突、tenure 与软赛季验收手册](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#conflict-scope)。

| 原产品验收 | 必须观察的玩家结果 | 专业实施 / 方法 |
| --- | --- | --- |
| [AC-1](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-1)，Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | 参加/不参加、第三方资产与公示窗口均可读；越界无效果 | [准入](../../world-runtime/runtime/chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-authority)；手册 CF-01 |
| [AC-2](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-2)，Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | 离线包有触发/累计上限，撤权与到期停止新增效果 | [防御](../../world-runtime/runtime/chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-defense)；CF-02 |
| [AC-3](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-3)，Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | 连续持有、到期与原条件同意后以真实 tenure receipt 改变持有人，保留剩余期限/用途/第三方；临时用途另列 | [占领/权利](../../world-runtime/runtime/chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-occupation)；CF-04 / TN-07 |
| [AC-3](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-3)，Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | 已装载/在途/已安全入库及到期分别结算，只有真实最终入库形成收益 | [实体提取](../../world-runtime/runtime/chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-extraction)；CF-05 |
| [AC-4](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-4)，Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | 真实损失与身份历史分开，比较实际修复/重建/转向/退出 | [资产与恢复](../../world-runtime/runtime/chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-assets)；CF-03 / CF-06 |
| [AC-5](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-5)，Trace refs: O7-REQ-ce9fbace-7fc6-4973-8bcf-8b58e11b3339 | 刷新竞争资格且保留世界、旧合同、身份、资产与恢复 | [赛季](../../world-runtime/runtime/chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-season)；CF-08 |

`required` 核每条正反例、受权读面与真实资产结果；`full` 加两实例、并发、跨节点、恢复、时界和未知提交组合。玩家证据单列：是否理解参加代价、暂缓理由、不能回滚的损失、仍可恢复的价值及下次选择。按钮可点、自动测试、模型观测、留在页面的时长都不证明玩家理解或愿意继续。

## 9. 取舍、结束条件与未证明范围

保留单一持久世界与有限自愿竞争，代价是不能以自动判胜、瞬移库存或赛季清账简化收尾。真实资产、长期义务与短竞争窗口相交时，关闭新权限可以立即发生，物理处置和付款必须等待原真实结果；读面明确这些剩余，而非制造“全部结束”。

设计完成要求上述政策、两个专业技术设计和独立验证逐项对应原 CC/GR-007，主要正向/拒绝/并发/恢复情境均可实施；它不要求这次运行游戏。后续必须实现受治理模块与准确作用域、资产/运输/tenure 适配、当前证明和独立复核、费用与安全结清、正式三入口及恢复验证，并取得独立平衡/玩家证据后才申请战争重启。没有隐含生产默认、现成 panel 或已验证恢复率；参数的合法发布与实际效用分别证明。
