# 区域冲突、地点 tenure、恢复与软赛季验收手册

- Owner：QA；由非作者建立预期账本与权利结果，gameplay/Runtime/资产/区域共同解释真实来源。
- 当前状态：**方法已设计，业务场景、正式入口、跨节点、真人与性能均未运行**。本手册中的数量/时间是独立复算用例，不是现行默认、已构建fixture或运行结果。
- 固定设计输入：`eng-cc/oasis7@cca8cda97cf121ebc1e25e96d6b0204a421323ea`，2026-11-05；实际执行另固定同一source/build、manifest/artifact/schema、world/branch、初态、规则参数、权限/资金与环境。不得把历史通过或另一候选结果拼入。
- 专业权威：[冲突技术设计](../../world-runtime/runtime/chartered-conflict-assets-and-soft-season-continuity.design.md#conflict-verification)、[tenure技术设计](../../world-runtime/runtime/location-tenure-and-disposition.design.md#tenure-verification)、[冲突政策](../../game/gameplay/gameplay-chartered-conflict-recovery-and-soft-season-contract.prd.md#conflict-enrollment-policy)、[原tenure政策](../../game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md#tenure-policy)。

<a id="conflict-scope"></a>
## 1. 范围、前置与证据层级

| 原产品范围 | Trace | 本手册场景 |
| --- | --- | --- |
| [CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001) | O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | CF-01/02/07/09/10 |
| [CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002) | O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | CF-03/06/07/09 |
| [CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004) | O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | CF-04/05/07/09，TN-07 |
| [CC-005](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-005) | O7-REQ-ce9fbace-7fc6-4973-8bcf-8b58e11b3339 | CF-08/09/10 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007) | O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | TN-01～07，CF-09/10 |

CC-003没有被本手册改为新的战争奖励规则。需要系统性恢复项目时沿[原项目验证](../../world-runtime/runtime/bounded-recovery-projects-and-contribution-settlement.design.md#recovery-transaction)，核其containment、参加/贡献与单次效果，不能用本手册的普通维修替代。

S0仅核正文、来源/链接、关系、算式及方法独立性；required核真实实现候选的正反例与三入口读面；full加两instance、并发、恢复/跨节点、边界时刻与持久化故障。没有当前治理、物理adapter、资金、名册/随机或正式入口时，记录准确阻点和未执行场景，不临时给管理员权限、不改制度分类、不用stub通过代签真实链。

### 1.1 受控材料

使用同一持续World W、成熟区域R、两名明确参加的A/B、一个非参加者N、一个受新人保护者P、邻接未成熟区域U。若用组织参加，至少加一个组织成员但未授权个人资产的N2。准备两种不同artifact相同逻辑schema的合法版本供升级验证，以及同artifact不同instance的两组对象供隔离验证。实际测试中的名称不是生产ID。

原参数示例为Q=8，公示[0,16)，交战[16,48)，H=8、提取[48,64)，程序C=64，证据截止80、初审88、程序申诉96、复核104、尾部112。tenure正常样例为开始0/结束160、Q=8、H=160、M=32，续期窗口[128,160)。这些是已签测试参数；不能假定当前世界tick等于现实秒，也不将样例升级成生产默认。

资产包括原稳定Agent身份I及可损坏chassis、可独立处分equipment、工厂F、带第三方保留量的lot L、一个未完成job/已完成receipt、一个带剩余期限的tenure T和一个受保护接收点D。每项必须由真实原writer产生/读取，或在L1中明确标合成初态；required/full的正式入口另证明来源可达。原Quote/签署/activation、各hold与真实payer、独立review授权/名册/随机和availability记录完整。

### 1.2 先写预期再执行

QA在提交前独立记录：原主体/授权、每本资源账期初/可用/hold、source/custody/transit/destroyed/destination数量、condition/工作/identity、tenure holder/期限/用途、程序时界与费用、source effect消费键、当前world/root。依据原政策手算期末与允许变化，不从被测模块返回summary复制预期。

每次保留原request、规范payload、当前cut、正式接受/拒绝、实际domain receipt、全部受影响root及当前读回；secret/私有原件按原权限封存，不在报告复制凭据。三入口分别核同一受权subject，不要求公开面泄漏私人case。正向和拒绝使用同一基本材料改变一个关键条件，防止以“不同世界恰好不同”解释错误。

<a id="conflict-admission-cases"></a>
## 2. CF-01：宣告、参加与资产保护

1. 以合法R、A/B接受、精确暴露、原asset/第三方清单、当前授权、完整时界与资金声明冲突。预览/聊天/待校验接受均应零hold、零费用、零权限。只有一个成功receipt后才有对应暴露和明确有限预算；逐项核current authority与实际payer。
2. 分别换成U、新人P、N、未同意个人资产的N2、过期/撤销charter、扩大region或含第三方未同意片段。每次应原子拒绝；原资产/钱/权利不变，不因邻接、物流经过、组织名或历史信誉成为合法目标。
3. 使用相同资产/quantity片段声明第二冲突，另用不重叠片段声明第二冲突。前者拒绝，后者须按自身完整条件判断；不能用“全世界已有战争”误阻独立合法范围。
4. 宣告后尝试普通转让/抵押同一暴露给无同意第三方，退出/换Agent/组织后再次支出已承诺量。证明原义务不消失、未参与第三方不被拉入；只对未承诺的新扩张停止。结束后重放旧参与quote不得重开窗口。
5. 公示期最后边界、正式开始、正式结束分别提交；所有入口遵守同一边界。后台未扫尾也不得在t>=end继续新攻击。

通过证据是原暴露/资源/保护/index与receipt一致；旧DeclareWar成功或UI显示“开战”单独不通过。

<a id="conflict-defense-cases"></a>
## 3. CF-02：离线防御与撤销容量

1. A对两个精确资产授权一个允许两次响应、E总额6的包；每次合格响应成本E3。产生两个不同合法敌对effect，应最多响应两次且总E=6；重发第一effect、换request ID、乱序到达均不增加第三次。同participant另造lineage/并行包应拒绝；合法版本更新仍共享原已用量与触发索引，不能用同一effect重复响应。
2. 用非敌对观察、区域外目标、未知effect、已自动响应effect作为触发。全部拒绝；两个自动包不能互相无限反击。目标在scope内但设备/路径已失效同样不能扣半笔费用。
3. 第一个effect后撤销包，同时排入第二触发。按canonical先后分别跑两种顺序；revoke先则第二无新effect，response先则只保留那次原合法结果，不回滚它。过期、当前grant撤销和E不足分别核。
4. 16个主体各自唯一lineage的普通设置/收缩16槽全用完，再分别最终撤销。必须仍能各用独立保留的一次最终停用槽与原费用，关闭该主体的全部活动版本；不能要求新的暴露、追加行动预算或另一个同意。再次撤销只返回原结果、不重复收费；换lineage/包名/Agent/request、退出重入或重装均不能清除finally_revoked并重开原冲突防御。
5. 离线后回到Viewer/API/Agent，核触发→实际响应→总花费/剩余→原拒绝/撤销原因→下一步；没有常在线要求或模型自授范围。

<a id="loss-recovery-cases"></a>
## 4. CF-03 / CF-06：三类损失与真实恢复

### CF-03：原因、损失与身份

三组分别由真实原writer产生输入：冲突合法损伤；区域危机造成的局部设施/服务损坏；一般经营折旧、故障或生产失败。后两组不创建ConflictRecord、不占战斗bond、不要求conflict.review。只有L1材料可直接构造前态，必须标明不能证明正式producer。

结构手算样例：F当前condition=800000 ppm，合法impact=200000、remaining damage cap=300000，实际后值600000。重复同effect保持600000，不二扣费用；一次修复后新合法攻击按新current revision重验，不以旧800000为基数覆盖。改变impact来源/目标owner/授权或保护应拒绝。chassis降到停用阈值后I的身份/owner/原provenance/历史receipt保持，不能成为胜者身份；equipment、库存、工作与区域能力分别核其原adapter，不以F一项代签。

对工厂内12单位lot，其中8暴露、4归N受保护：合法销毁3并使5进入争夺custody后，N的4保持，不能又把8当原owner可用量。已完成production receipt与持久里程碑保持；未完成job只按原中断规则处理真实已sink/未sink/可回收，不能清空历史或全额退款。未知原因样例只显示已核condition和cause_unknown，不编造谁应赔。

### CF-06：有取舍、无安全路与自然退出

1. 从同一个F=600000及原目标，准备两条由测试候选真实原规则支持的路线。例：修复每100000需1个维修部件+E2，目标900000，共3部件+E6；重建原规则需6部件+E12、6ticks、一个合法新地点，生成新identity。QA先逐步核当前可用部件/E、原工厂/运输/tenure、时间和费用，不能借未来回收物支付尚未进行的第一步。
2. 正常材料给两路都实际可行，再只减少当前E至8：修复仍可行、重建当前不行；再只撤销原地点维修权但保留新地点建设权，结论应相应改变。比较不扣费/不修复，实际选一条后以原领域receipt核新增能力。
3. pivot使用输出与原目标不同的合法recipe，应返回needs_goal_change，只有玩家明确接受新目标才比较新方向；不把换标签当原目标成功。三类损坏各重跑至少一组真正适用路线，不用战争fixture代签危机/一般经营。
4. 没有可负担能源、无可用地点或source未知时显示具体缺口、保留价值、延期/重新规划；不能递归推荐先花缺失能源来免费补能，不能强制入会/赞助或通过新身份重建。
5. 选择退出：停未承诺工作，仍在途、待付款、已损失和可回收分别保留；重连查原结果再决定下一笔。退出不自动退款，也不抹除I的基本身份和仍合法独立参与资格。

### 独立判定

验收须同时比较真实condition/数量/能力与玩家解释。只有“恢复计划存在”不通过；只有实际condition提高但玩家不知道成本/仍有义务，也不能证明体验通过。样例数值只在原测试规则真实支持时使用；否则记录该场景不可执行，并由QA用同样覆盖形状的真实参数重新算期初/期末。

<a id="occupation-extraction-cases"></a>
## 5. CF-04 / CF-05 / TN-07：占领、tenure转移与物理提取

### CF-04 / TN-07：长期权利必须真实结算

T原holder=B、结束160、允许的有限用途和第三方保护完整。B在暴露前签署该范围按本冲突持续H=8至end=48条件转让的承诺，A已接受剩余期限/维护与义务；不能用临时许可代替这份条件同意。

正向：A合格控制设备在[40,48)连续满足位置、工作condition、真实控制能源与当前权限，全部区间/变更prefix无洞，end仍合格。冻结结果后尚未settle，T仍是B且A不能据qualified先建造；成功两instance同根settlement才变为A，期限仍160、用途/维护及未列第三方资产保持。再次同/异request结算不得再变权或刷新期限。

独立反例：只有[47,48)最后触碰；[40,43)与[44,48)断开；中间离开再返回同位置；对手控制设备同时合格；缺一段authority/电费/完整索引；没有B原条件同意；受让资格当前失效；原tenure在48前已结束；对该片段有合法未结保护。全部不能转移长期权利。unknown保留待结，不自动判B或A有另一份新权利。

临时用途分支另测：原outcome只允许TemporaryUseGrant，A可在其已签范围/期限使用，B holder不变；到期停新用途但保留实际货物和旧合法结果。不能把这个分支当已通过上述正式转移。

### CF-05：lot守恒与安全目的地

承接CF-03的5单位争夺custody，声明D为原world内真实合法、具有保护/当前receiver接受和足够容量的目的地。通过原运输路线装载5，真实途中损失1，到达4。arrival后未finalize时A仍不能支出4；一次finalize后才有4可用量，原owner对应量不再可用。

QA独立等式：原12 = N保留4 + 原毁损3 + 途中损失1 + 最终D可用4。receipt、custody与历史摘要不是额外库存。重试装载、到达/结算乱序、重复receipt、跨season/换request/改lot名均不能多出量。原source/claim键相同但body不同必须冲突，不能择一。

独立改变D满仓、接收撤销、位置/保护失效、路径中断、费用不足、原结果未知。应保持实际位置和remaining，允许的下一步来自真实wait/原范围改道/返还；不能拿普通direct transfer、client ACK、截图或目标名字替物理交割。改道先明确接受新目标和成本，只处理未结量。

到X结束分别测未装载5、已装载在途5、已到达未接收4、已final4。未装载的权利失效并单次释放真实custody；在途/待接收保留原责任和安全收尾，不瞬移退回或计最终收入；已final保持。再申请将D变为可攻击暴露，若破坏原接收保护义务必须拒绝，不能使“安全到达”成为虚假承诺。

<a id="tenure-cases"></a>
## 6. TN-01～TN-06：地点权利与实际处分

| 场景 | 输入与操作 | 独立预期 |
| --- | --- | --- |
| TN-01 取得/规划/维护 | T开始0/结束160、Q8/H160/M32、明确用途/维护与真实资产；相邻公共路径与无关库存 | quote零效果；取得后horizon/维护成本/普通通行可读，title不含无关库存；登录/刷receipt不算实际使用 |
| TN-02 续期/转让并发 | t127申请续期；t128合法；B同时向A/C转让；普通转让与条件冲突转让相竞争 | 提前拒绝；合法续期重核使用/维护/累计horizon；最多一新holder，后者stale。普通转让不刷新原notice/clock，不自动接替旧债或出售独立资产 |
| TN-03 四种收回触发 | 到期、完整M无合格使用、可证违约、已预定义公共必要性；另加短暂停机/一次失败/竞争比分/缺席 | 仅前四按原政策进入notice；纯停机/短失败或未证明原因不能造成提前收回/没收；缺提醒不新造永久使用权也不跳过资产通知 |
| TN-04 通知/复核 | N=40，证据56、初审64、申诉72、复核80、尾部88；受权inbox、六名无冲突成员 | 未完整通知或当前入口不可用不能跑完处分时钟；2/3同typed决定才成结论；换人/ID不重抽/重置；无有效final尾部不提前收回，原自然期限保持 |
| TN-05 部分真实处分 | 3资产分别实际迁移、按真实condition回收、原地保护custody；其中1运输失败 | 权利停止与vacated分开；只完成真实项、失败保留位置/费用/下一步，公共通行保留；后继不得覆盖仍占用footprint |
| TN-06 公共必要性补偿 | 独立真实payer、原替代报价10单位部件，已接受cap10；实际返回3同种部件，搬迁费E4独立 | 先足额保护最大10部件与E4，实际应付7部件，不能预扣尚未返回3；原payer/holder期末守恒，重复付款0新效果；E不与部件相减 |

TN-04额外核：在合法证据窗口最后提交一项证据，初审不能提前用尽唯一轮次；初审/复核不能同三人；panel名单缺完整排除、随机被caller选择、wrong jurisdiction或程序预算不够均不能以管理员/多数票补齐。保持无关义务与通行，仅保全争点未执行部分。

时间guard独立负例：N+3Q只有preliminary；N+4Q前已canonical受理的程序申诉尚待复核，此时不可执行争议资产拆除或赔付。N+4Q同刻新申诉因半开区间超窗，不能以客户端先发送追填；执行仍先核完整此前受理索引/未知请求，不能因读取少一条而误判无申诉。有效复核完成或窗口已闭且证明无待决复核后才可executable_final；此等待不冻结无争议安全收尾。

TN-05同时保留原厂WIP/在途第三方货物和已结production：停止新tenure用途不删除它们。回收使用原可回收量及receipt，不按当年建设成本全退；无合法destination/运输能力时保留pending，不能声称完成迁移。技术终态/到期标签不是实际腾空证据。

TN-06再分别测试未实返3、实际仅返1、原quotation来源未知、资金不足、超cap的新费用、holder拒绝新包、付款持久状态未知。实返不足时原包最大10的保护金额足以按9核真实差额；不得在通知前只预留预计净7。未知或不足时不可先执行不可逆公共收回、再靠临时债/他人余额补偿。普通到期/闲置/本方违约没有自动经济赔偿，但保留原资产取回/回收权。

净差模式在实际回收尚在途、protected custody或结果unknown时不得先按0回收支付10再返还3。待该item原回收结果封存、应返还量真实到账后只付净差。另测明确残余权益交换：holder同版接受原精确片段的残余回收权与全额10补偿同时移交，payer付款与权益变更同根，后来回收3只给payer合法账本；不能既付10又给holder3。缺holder接受、错scope、权利交接只成功一半、两模式同时执行、换request重复关闭都应拒绝/恢复原结果；不影响清单外资产和身份。

原tenure条件转让与区域退化、自然到期、公共必要性notice交叉见TN-07/CF-04。先后顺序由同一canonical revision决定，不能用两个authority各自产生成功receipt而留下两个holder。

<a id="review-season-cases"></a>
## 7. CF-07 / CF-08：有限复核与软赛季

### CF-07：事实、处分与钱分开

以CF-01/03/05同一冲突建立一个精确未结片段争点，原当事人各保留初审/复核8个证据引用份额。初审前至少注入一份合法晚到证据；不得提前裁决使其失效。原随机必须在scope/evidence承诺后按固定来源取首个合格结果，保存排列；不允许多次抽到满意panel。

手算有限赔付：已证明可归责损失5单位，原责任cap4、remaining bond3，同单位实际支付至多3，缺2不隐含追扣无关余额；认定final但未支付应显示payment_pending。每原损害片段只能赔一次，换case/request/season不再支付。若已final付3，到尾部只返回原结果，不再释放同3或支付第二次。

分别测试名册被当事人控制、初复审同人、普通治理/tenure/service/RD权限代签、声誉/缺席替因果、冻结无关资产、程序费被普通操作耗尽、结果unknown。应拒绝越界处分并保留原合法事实/窗口。没有final的尾部解除程序保全且原未提取权按原到期处置；不宣称任何人清白、不恢复已毁损物、不中断无关履约。

在C+3Q只得到初审、C+4Q前最后合法tick受理程序申诉，随后提交赔付或撤销未结entitlement：应等待有效复核/原程序尾部，不可先付款或转权。C+4Q的新申请超窗但此前已受理的申诉不能丢；受理索引不完整或原request未知同样阻断争议项不可逆执行。将此边界也应用于season-window.review，case与授权仍相互独立。

### CF-08：窗口刷新与完整旧世界

建立旧窗口S-A与已合法授权的新窗口S-B，二者同World/branch。跨边界放置：普通长期tenure、合法条件转移后的剩余tenure、旧生产job、在途提取、未付final、原Agent身份与memory/provenance、普通非参与者库存和core能力。

1. 在S-A截止前成功及截止后新请求分别执行。实际consumer必须按原expiry拒绝新资格效果，即使关闭批处理尚未完成；已接受旧义务按原条款安全收尾。
2. 同一个真实结果改window/claimant/receipt包装再次计分，必须由原source claim预算拒绝；新source真实效果按新窗口自己的资格判断。缺证条目unknown，不默认0或删掉不利结果。
3. 比较前后world identity、时间历史prefix、Agent/能力/资产/合同/tenure期限/付款key：全部保持原连续性，只有预告的临时资格/项目/排行榜窗口变化。非参与者不受隐性没收或基本资格惩罚。
4. 以独立season-window.review授权质疑原结果遗漏/错误归类和资格时间。复核只能改版本化结论或原允许未结资格，不能延长过期权利、重复已付奖励或借冲突panel扩大处分。原奖励资金不足/授权缺失不因rank更正自动补发。
5. 新窗口policy/artifact变化不回算S-A输赢；重连/恢复返回原request与原费用/时间，不能新建World、发新身份或重置旧未结责任。

<a id="recovery-capacity-cases"></a>
## 8. CF-09 / CF-10：组合恢复、容量与权限

### CF-09：先列全写集再注入故障

至少选择：Declare/暴露、一次损伤、一次防御、正式tenure条件转让、提取final、补偿付款、case final/尾部、season关闭。对每项在验证后/准备后/持久提交前/提交后回应前/后续维护阶段注入故障；保存原operation/key/body、全部instance/资产/资源/index/fee/receipt与durable candidate。

| 持久判定 | 独立预期 |
| --- | --- |
| NotCommitted | 原状态、余额、权利、source消费键和event分配均不出现半个成功；合法重试可按当前状态重验 |
| Committed但客户端未知/维护失败 | 恢复/安装原新candidate；重读原receipt，维护失败单列；不回退旧可写World、不重复费用或资产效果 |
| StatusUnknown | fence整个不确定旧World所有mutator/save/依赖派发，连无关写也不能覆盖可能已提交候选；查清精确完整generation后才解除 |

恢复snapshot+canonical后缀，与不中断同输入分支逐项比较原root/receipt/资产/费用/时界/prefix。replay不得调用模型、module抽签、damage evaluator、真实运输adapter或发外部通知；只应用原canonical结果。两同artifact不同instance同时操作，验证所有key/ref/receipt不串案。outbox同key同payload去重，异payload冲突拒绝，不能声称外部物理exactly-once由本地receipt保证。

关键并发对：revoke/defense；attack/repair；transfer/tenure expiry；两方占领到期；finalize extraction/return；final/payment/tail；season expiry/new eligibility use。每对两种顺序都跑，最多一个版本的权利/资源成功，后者fresh重验。并发不同source/范围应保持独立，不以全局业务锁掩盖缺少正确对象键。

### CF-10：上限、闭包与不会耗尽的原结果入口

按设计容量边界减1/等于/加1三组输入，逐一核主体、scope、asset/obligation、lot、source effect、证据阶段槽、current state编码、raw+decoded bytes、proof节点、读取次数/索引访问、deadline、invoke数及输出。16主体/64资产不表示源只需16或64个对象：每个嵌套grant、第三方、quantity kind、negative/range proof、fee/head重验均计入256对象/4MiB。

对1秒/2秒附近读取延迟、超时后迟到buffer、4槽满载分别核内存和取消；不能一超时就释放配额却仍持有迟到I/O。原结果查询的独立2个1MiB槽仍按精确key和当前权限可用，不借此扫描全World或开放历史私有数据。

费用独立复算：tenure成功迁移预留538+64=602；冲突普通/程序1018、安全128以及W个有费用控制tick，共1146+W，W最大4096时5242。每迁移最多2个有费用invoke，原控制能源/运输/维修/补偿另分资源；不能遗漏控制区间、第三次module、否定证据或关闭款。这里核预算公式不是声称已花这些费用。满普通设置cap后仍能revoke（CF-02）；普通工作不得耗掉复核/关闭保留份额。

注入一个真实所需但未适配的writer、第三方义务超上限、索引水位滞后、range proof不完整、wrong-world同ID、schema未知、数值溢出、当前audience撤销。必须incomplete/reject/stale且不造“无义务/零余额”。返回前权限改变，三入口都不得从缓存泄漏原私有材料。

## 9. 玩家理解与平衡证据

在实际候选可用后，非作者主持受控试玩。先让参与者说明“参加会暴露什么、还有什么不受影响、何时能停自动防御、失败会留下什么”；宣告前能选择参加、暂缓或保留独立路线。损坏后让其比较两路真实恢复成本并选择，另观察没有安全路线时是否理解阻点；不要提前告诉唯一正确答案。

同样询问“占领/qualified是否已经取得tenure”“装载/到达是否已经收入”“赛季结束后原运输/债务是否还在”。要求用当前真实receipt、成本、状态和下一步解释；仅复述按钮或主持提示不计独立理解。记录原引导、额外提示、介入时点与前后表现，不隐藏必要帮助换无提示通过。

结果分开报告功能链完成、理解、可恢复性、选择/继续意愿和各类未观察项。样本来自受控新手/已熟悉玩家/专家/模型分别标记；这份手册、单场自动测试或通过文档门禁均不证明平衡、留存、长期世界健康或战争重启。真实响应窗口、费用、损失上限和控制节奏要由同候选独立平衡与玩家证据支持。

## 10. 结果记录与结束条件

实施PR记录每场实际candidate/environment、真实authority/source、动作/结果、独立预期/实际差异、原receipt/root/费用/时界、保护/身份/资产检查、未运行项与具体原因。报告只链接有权保存的原材料，不粘贴私有证明或凭据。方法ID是验证定位，不是新的产品要求或运行协议。

S0通过最多说明稿件、引用和算式一致；设计覆盖依据仍逐原CC/GR007检查。required/full及正式三入口、所有声明资产类别/原因、跨节点/恢复/容量、玩家理解与独立平衡未完成时，保留相应未验证结论。旧war/工厂/物流CI、当前PR合入或任何单个绿色结果不能代签完整冲突与长期世界连续性。
