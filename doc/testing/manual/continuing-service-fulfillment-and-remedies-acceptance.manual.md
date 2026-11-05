# 持续服务履约、责任与有限救济验收手册

- 生命周期：active（目标验收方法；未执行正式服务、WASM、Rust、模型、故障域或玩家验收）。
- Owner role：qa_engineer；由非设计作者按固定候选独立生成预期，原玩法与Runtime owner提供真实权威和源。
- 固定设计审读输入：eng-cc/oasis7，ebd9b8431f2dc084c4ef6be3bcea9ca054ab2a81，2026-11-05；运行时必须另固定实际source/build/manifest/authority/environment。
- 上游：[CR002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)，Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d。
- 被验目标：[唯一政策](../../game/gameplay/gameplay-continuing-service-fulfillment-and-remedies-contract.prd.md#continuing-service-policy)及[执行设计](../../world-runtime/runtime/continuing-service-fulfillment-and-remedies.design.md#service-obligations)。本手册不产生服务政策、费用权限或真实panel。

<a id="service-acceptance-scope"></a>
## 1. 范围、候选与证据等级

CSF-01–06共同覆盖有限供能、维修/维护、物料运输、已有工作协作以及个人/组织/多方，保持原供能PS01–08和旧native行为。每项运行均选择确已实现、合法激活且有完整source/adapter的同一候选；任一原已采纳服务缺真实路径就记录该项blocked，不能用其他类型成功填补。

预先冻结：family/common完整条款与render、全部接受/当前代表/资金权属、world/branch/instance/artifact/policy、正常/安全/程序完整调用图、真实fee authority及payer、物理输入/容量、合法服务事项管辖/名册/随机、checkpoint时界和所有原件留存位置。验收者另外导出原物理/资源账、可核snapshot/tail和authority根，独立算预期，不调用服务自己的“expected result”作为oracle。

每个场景记录真实输入、原operation/candidate、before/after根及完整receipt、各单位/物理source/hold/paid/unpaid/fee/source-use、结构化拒绝、所用用户界面与source版本。隔离四类结论：静态来源/算术；规范编码shape；真实执行/故障；真人理解。本文中下列数值形状已可用标准Python独立复算，但它们不带真实签名/治理/世界，不是运行成功证据。

<a id="csf-01-domain-results"></a>
## 2. CSF-01：四类真实结果与原单位守恒

### 2.1 维修和有限维护

1. 独立读取合法工厂状态：condition=950,000ppm，max=1,000,000，原hardware_part恢复量25,000ppm。资产方按exact对象授予维修权限；供方真实持有请求3份parts，条款只买50,000ppm净恢复，价款200 Data。
2. 原物理执行应实际消费2份parts、condition增加50,000至max，余1份仍归原权利人；服务只确认这一原受权source的50,000，净价200按原单位一次成立。独立核材料ledger前后及原工厂receipt，不以服务状态文字代替。
3. 同receipt换request、同effect换role别名、第二agreement重复引用都不得新修复/付款；新damage将condition再降25,000时，原完成不重开。新的维修需要新真实输入/对应原残余或明确新合同。
4. 请求3份但实际只持有2份，在当前原MaintainFactory先验请求量的路径上应拒绝，不能由服务偷偷改为2；目标新adapter若另允许精确2份候选，必须明确重提同意，不能伪造builder。无资产grant、旧controller、已retired、少completion/safe资源或改asset版本逐项拒绝。
5. 对maintenance_interval，准备同一总终态但不同历史：A在整段窗口均满足可用条件；B中间一段失效后修复；C缺该段历史。只A满足完整窗口谓词，B按原明示容许停机/风险条款判断，C为pending_evidence。不得由末尾condition正常推全段完成。
6. 以原world物理承载/chassis维修的真实规则补一正向；其量纲/材料/授权取原source，不能把factory的25,000套用。无该adapter则该义务blocked，不能以工厂例代签。

### 2.2 运输、托管、接收与货权

预先由物流source给出10单位真实货物及获准route/capacity，当前fixture允许其中1单位物理损耗，目的地实际accept9；原服务允许每单位30 Data的部分交付。独立预期为sent10 = received9 + confirmed_loss1 + remaining_custody0，服务价款270，剩余30不得被arrival或运费receipt赚取。风险责任对这1损耗另取原accepted risk/因果，不能自动认定carrier有错。

- 分别停在dispatch、arrival、storage_acceptance、ownership/terminal之前读取服务；各前序不得冒充原完整交付谓词。native完成只证明收到量/原tariff，不伪造商业接收。
- 再做whole10的对照：实际9与损耗1仍记录，但不因其收到9就赚whole价款；退出按预签已发生成本/残余规则，不把9库存抹去。
- 仓库第三方custody、货物title、付款escrow用不同原主体。解除价款hold不移动货权；拒收不能把货物改为承运人自有；相同lot不能源库和在途同时可花。
- 分别撤route、占满storage/terminal、收货人撤准入、换controller。未开始的本次候选整项拒绝；已出发者保留真实cargo和责任，执行有权保管/返程。真实return必须有新的route/资源/容量和原目的地accept receipt，确认returned不能早于真实结果。
- 补运原损失1必须另取真实1单位与费用，parent指向原失败；无输入不得免费补货。原loss不回滚，旧fee basis与损害已赔量延续。
- 竞争请求消费重叠lot或storage capacity，按原canonical顺序只一个赢家。最后资金/cargo/demand账在原合法结算边界一次提交；失败无半边价款/货权。

### 2.3 工作协作

选择原工业工作图的两个必需角色A/B与一个terminal；再选择一个已有有效RD charter内“完成该次实验”的work-contribution。完整读取原work/role/input/许可/贡献证据。

A完成自己的有偿input，B未完成：own_completion的A应赚其事前价款；joint_completion的整体不赚whole价款。必须保存A的真实投入和终止时预签可付项，不能把B失败写成A从未履行。若全体所需child及terminal齐备，再验证整体正向一次完成；生产而未terminal不冒充交付。

同一实际贡献不能以两个服务角色或两个agreement各领完整价款；允许多受益人共同单元时，实际分配在签署正文已给定且总量守恒。RD失败实验若准确完成原过程义务可以满足“实验完成”，但不能显示新发现、改RD归因/share或撤销独立许可。原RD panel不能作本服务default裁决。缺原许可消费、物理source或join资格，不能靠聊天/模型总结补齐。

### 2.4 一般组织/多方的有限Electricity供能

选择一个真实有权组织供方和一个独立个人供方，向同一买方实际Agent供给整数Electricity；组织代表、真实资源owner、artifact fee owner、受益与付款角色全部分别核权。三个Agent在canonical同址，检查点净需求8E，组织slot3E、个人slot5E，价格2Data/E；价款16Data先按原source保护，未来供方Electricity不伪造预存，当前必须各自足额支付交付及同源实际费用。

先由组织真实供3E：独立账应原source−3、买方+3，原unit[0,3)一次完成、own_completion价款6Data成立并按原同根计划结算，price/fee/collateral各准确变化。再由个人真实供5E，最终净8E和16Data；将成功与真实fee分账，若artifact owner也是个人供方，5E与其执行费必须合并前余额核对。受款的价款不得融资这次供电/费用。多方joint_completion变体只能形成预先同意的条件价款；已发生3E仍是真实转移，不因另一方尚未交5E就免费回滚。

对照未到电但合同active、没电、暂停、失去current mandate、移开共址、自账户转移、循环转回、重复原receipt/换unit role；均不得制造新净交付。原raw ResourceTransferred不带完整service grant/hold semantics，目标adapter缺任一consumer就blocked，不能用raw写绕过。在同一root分别注入E扣减、receiver credit、Data价款/conditional claim、collateral、fee和receipt准备失败，必须没有半边提交；StatusUnknown按CSF-06恢复原根，不重发电。

撤普通Data grant不能抹掉激活时已成立的准确狭义价款处置权；新电力执行仍按当前真实authority，服务不得自授任意Data或远程电网权。repair只供原剩余真实E，终止/争议/继受保留已交和旧责任。旧两人power v1的参数、同址规则及PS方法另完整回归，不以本例替换。

<a id="csf-02-scoped-recovery"></a>
## 3. CSF-02：部分完成后的四条路径

准备三个检查点：一项已完成且已付、一项有真实在途/未终结结果、一项没有共享不可分节点的正常到期义务。独立画出只由实际共享asset/lot/hold/work构成的冲突集合；不要使用“同组织”作为边。

1. **repair/reperform**：在[due,R)提交恰好原未满足量，使用新真实材料/费用并继承原unit与fee basis。夹入任一已完成范围的请求全项拒绝，不能裁掉后继续；等于R拒绝该窗口的新修复。原费用/损失继续存在。
2. **terminate/settle**：分别在未开始、材料sink后、在途、buffer/terminal-pending取消。只未消费且无原权利负担的量可以释放；WIP/在途遵原真实处置。已赚价款及有权取消成本保留，取消成本必须有真实原source且不超过预签cap，不能索取未生产的利润。
3. **dispute/preserve**：在原Register/Merge/Evidence/Appeal的已预算调用内携精确preservation_request，独立核直接权利/具体争点/真实冲突不可逆effect；有权谓词成立才对单lot/单损害建立stay，受理本身不保证保全。无权或无实际冲突时应仅保留合法登记（若成立）及准确保全拒绝，不能生成stay。记录实际invoke数，缩解随原已计和解/final/Terminate/work/close，不额外计费；source纠错让后续效果重验，而非无限refresh调用。同步完成第三个无冲突检查点，应正常成功。故意把scope扩到整组织应拒绝/重提原精确范围。不可分共用node可冻结最小整组，但必须给出真实依赖证明；安全保管/防止额外损失不因一般stay停掉。
4. **explicit successor**：使用CSF-04的完整接受交接真正剩余，确认旧已付/未付、损失、case deadline、source-use不清零。unknown原work不得同时交给新方。
5. 两个恢复请求竞争同一残余：一个已提交后另一个不能重做/新付。原物理效果已发生但服务核验断线的分支，应为performed_unsettled，原outbox只补核定，不能再调用维修/运输。
6. 独立给出有效award30、当前可付10：实际pay10、confirmed_unpaid20。中性tail、合同close、改owner或续期均不得清除20。其后原债务人用当前合法资金新增清偿7，应只变paid17/unpaid13并按新付款实际费；原key重复不新费/新付，两个并发7若会超最终残余时后者整项拒绝。旧债权人不必重签新服务或免债。
7. 原收款身份不可解析时return_pending保留原权利/hold；把refund给当前新owner为失败。恢复原合法收款路径只核原件，不要求先签新约才能领取。
8. 续期使用新明确范围/报价/必要接受；对原source或曾已完成范围再收新价为失败。新scope不能恢复原先停新的相同lineage或重置累计grant D。

<a id="csf-03-review"></a>
## 4. CSF-03：独立归责、随机程序与时界

用一组明确数值切点：due=100，Q=10，R=120；初审证据截止140，初审截止160；初审D=155时申诉窗为[155,165)，复核截止190，tail=200。所有time是canonical ticks，非现实秒。

### 4.1 先准备两套相反事实

- A：同版有效义务、完整窗口、接收方全程提供约定准入/容量、有权供方在可控范围挪走已承诺输入、原未满足范围和准确损害均可回读。只在合法和解或最终程序后可认其对应责任；检查实际赔付不超过尚未补偿损害/该方cap/有效决定三者最小值。
- B：数量相同未完成，但接收方撤了必需准入，或原不可归责损耗，或已有合法stay；不得给出与A同样的供方default。C只缺窗口coverage必须未证；不能以缺席、无回复、current balance、低声誉或聊天补因果。
- 多方同一damage先真实修复价值6，再实际赔付4，已证总损害20：剩余可补偿10；不同责任人不能再各领/赔20。任一分担不超过原向量/个人cap，组织成员不因身份被扣款。

### 4.2 抽样与完整原案

固定合法服务事项authority、6名最小合格候选的原名册与case承诺；先case再取首个获准不可预选后续随机proof。独立复算排列和3+3、2/3；相同controller多个别名不增票，初审人不得进复核。把当事人、组织代表/控制人、资产/费用直接受益人、继受人加入候选应排除；资格未知不得充数。

140前两个不同主体的合法争点/证据必须入同一frozen scope/root，后到不能因先方已经投票被排除；不得提前冻结半案。换request、撤回重开、继受不重抽或刷新截止。名册一人合法失格只沿原排列补位，其旧票保留但不计；所有候选不足/quorum不足不产生默认胜者。跨领域panel签名、普通governance passed或管理员提交都不代替local-service.review。

### 4.3 半开端点、final和tail

在139/140、159/160、164/165、189/190、199/200各提交前后对照；同tick记录canonical顺序。D=155初审发布后付款必须仍阻断；164已canonical接纳的申诉即使客户端165才收到ACK，也进入原复核。165新申诉不接纳。不能先支付再补录164已提交申诉。

完整申诉窗关闭且无有效待决复核、有效复核已结束，或原复核截止后仍有效初审依法final，才可执行对应处分。复核申请不自动撤销初审；有效纠错或invalidate必须有准确原source/权限。初审被有效撤销与“复核无人回应”分别测试，不能把两者都叫no_decision。

200时分别放置：(a)有效final未付；(b)仍有效初审且复核到界；(c)合法和解已付；(d)完整证明不存在有效改变基线结论；(e)原commit/authority未知。a/b保留并执行合法原权利，c不再付款，d才中性停止与原路释放，e保持恢复fence/原资金。不得从缓存没有看到case得出d；程序结束不瞬移货物或免除confirmed_unpaid。

<a id="csf-04-parties-succession"></a>
## 5. CSF-04：共同形成、组织、多方与清偿继受

固定四类服务的同一完整TermsBody、共同形成摘要/渲染和全部required bundles；至少一名组织principal由其真实typed signing clause授权的多代表按门槛接受。由独立owner表列出签约主体、实际执行者、代表、资产人、价款人、artifact fee owner、custodian、受益人和责任人，不能按Agent ID混为一列。

1. 正向形成必须在原L2 family调用/完整graph内完成，common验证不产生额外影子履约ledger。接受pending时world余额/hold/容量/责任为零变化；activation成功才一次产生全部原义务、真实hold和receipt。sandbox可能已执行的全项拒绝仍零已发布业务费，不允许把业务解释搬进native以伪造零invoke。
2. 每次删一必需接受、更改render、漏source_projection_use、复用旧controller/mandate、改未被包含的scope/amount/interval、缺资产人或真实fee owner同意：全部拒绝原候选，不能默改为证据交集。较小方案须另显示/另接受。组织普通passed/成员身份/连续性grant各作一负例。
3. 原资产属于成员个人或第三方，组织签名不得替其授权。事前单独接受共同损害分担向量才可按向量追责；只写组织名/同组则不得产生个人连带。独立核各真实source gross/hold与mandate D/O/C，不从组织标签创造余额。
4. 正向继受用16个当前方加1个新方、最多17个required bundles；当前参与仍≤16、lifetime slots增加而旧有责方保留读权/程序。原方、新方及所有付款/财产/受益/风险被改变者必须同版接受；无新增用途的旧真实source授权可以精确引用。
5. 单一新方有真实输入/容量/collateral/fee后，同根缩旧执行范围、增新范围/holds和provider段；交接与旧最后一次履约竞争只一个赢家。历史source、已经付出的D、fixed deadlines/随机/已用次数不变。
6. 不可分工序、未知在途、原可能恢复的同一争议范围分别拒绝切交；超S/H、新方导致必要panel/conflict或程序能力不成立也拒绝。不能用新别名、组织改名或新agent绕已停范围和历史上限。
7. 普通mandate撤销即刻阻止新动作，即使normal/query额度已满、服务投影尚旧；原狭义合法结清继续。每party整范围stop只占自己的保留槽且一次覆盖全部既有包，另造lineage不能绕过。
8. 对组织解散/资不抵债/业务转让，读取完整ServiceObligationInventory和原estate清单，以独立原顺序核：既有合同→第三方custody→claims/costs→business→residual。删一未结分页、伪造零负债、跳过unknown、把成员财产入estate都应阻断。物理搬运/在途完成仍须真实执行。
9. 清偿一笔既有unpaid采用§3步骤6的原claim adapter，不另建债务、延长case或强迫债权人重签。当前来源与费用足额才一次付款；组织case finished不代表所有货物/钱已经到账。

<a id="csf-05-budgets"></a>
## 6. CSF-05：完整费用、可编码形状和满槽关闭

### 6.1 独立计算所选预算，不以理论最大收费

从实际选定graph枚举每个真实invoke及ordinal，使用当前合法原schedule独立算完整I/O费用；每个node读取实际artifact owner，native/交易/route/存储/许可/模型等其他必需项逐source核对。计算“模块调用的上界”与“所有真实成本向量”两张账，再按原source-purpose identity去重合并；不能漏node或把两个license阶段合成一次。mandatory source producer若不在图中不得静默执行。

复算小合同H4/K2/N6/S1/F4/J4：case170、M355、371节点；紧配置 activation32/32KiB为67D，普通64/32KiB为99D，证据与继受128/32KiB为163D，均1effect/1emit/4E。normal=1×67+1×163+8×99=1,022D/40E，procedure=256×163+84×99=50,044D/1,360E，closure=21×99=2,079D/84E，总53,145D/1,484E。完整证据/继受输入使用宽类，不用小成功path抹掉其最大source。另核价款600、collateral30、累计100bps价款费6、取消/赔付费上界4、真实物理及保管40E，以及原材料/货权/容量。各项都要求真实fee authority；fixture明确免收的其余项目仍记录其scope/version。

供方是实际artifact owner且明确承担该例全部module fee；其最小可用量53,179D/1,524E，买方606D，组织资产人当前同意。少1单位任一来源应activation全拒绝；用即将到款补足、借另一party预算、借collateral、缺owner免费分支均失败。第三方实际artifact owner未接受时不能改把费用扣给请求人。

正向跑activation、三项work各begin/complete、close，共8个实际节点。依真实全编码I_j/O_j分别算D_j=ceil(I_j/1024)+ceil(O_j/1024)+2effects_j+emits_j、E_j=1+effects_j+emits_j+has_new_state_j；以原meter和treasury独立对账。371节点中未执行的363节点不收费，不能扣53,145上界。price累计fee应为6，原physical实际消耗/route payout依真实receipt，40E只是上界。最后close先计自己的真实费，再原路释放所有已无负担的unused holds，gross不因release增加。

随后逐一做：换真实payer、额外validator/callback、相同模块两次调用、原license/退出程序预算共享、分拆价款/重演receipt、actual输出少于上界、graph64/65、累计65,536/65,537和整数溢出。拒绝不能留下半边fee或隐藏node；共享钱只hold一次，但多项授权cap都须覆盖完整原请求。D已花、O剩余业务、C未来费用的减/增按同原效果对账；refund不把D清零。

### 6.2 active计费attempt与非绑定/激活拒绝分开

用同一真实候选的五个分支对照：

- preview/接受pending、静态权限或预算拒绝、全项activation失败、精确重放：零新业务费用/成功槽。即使staged sandbox已计算，也不能发布费用；协议CPU限制不产生游戏权益。
- active且原条款明确允许的attempt，真实计算后得到合法accepted_attempt_result但无生产：仅实际已执行节点费用一次，消耗一次本人normal attempt槽，不赚价款、不自动default。
- 同active结果未签收费语义、普通trap/timeout/out-of-fuel、没有合法可发布结果：不能补签收费。原先已经合法成功的begin等独立节点保留其原一次费用，本次失败节点不收费。
- 原物理成功、服务核验断线：outbox保留已付节点/原结果，后续只做未执行的已列节点；不重复物理动作或完整图费用。
- 任一节点/根StatusUnknown：不执行新ID重试、不猜零费、不先释放fee hold；先查询原operation并恢复全部实际结果。

旧power v1按原确定拒绝零费方法回归，不能被新attempt分支改变。

### 6.3 最后一次关闭、每方阶段和16案同时未结

将normal J/S额度消耗至边界，留真实在途/未结价款，再执行每历史party的独立stop、必要安全plan、最终责任/资金处分、来源返还和最后close。普通CPU/query队列满时，保留通路仍可停新；不得要求追加normal预算才能取回无负担原权利。无效/重复/return_pending无效果重试不消费最后成功槽或闭合费。

对每party分别耗尽初审16引用后提交其合法复核16引用；第二party仍有完整两阶段。再把一party两阶段都用满，其他party/候选/系统final槽仍可使用。不能第一方提交一次appeal便耗完其他方回应份额。名册每位置最多一票/一次排除的上界逐项核对，故意重抽/改ID不得增槽。

构造16个同时未结case、32个历史/后继主体、每主体每阶段16refs，总16,384refs。独立复算C_case=1,382、所有case22,112迁移、M22,450；one-invoke metadata加512个64节点work为55,218节点及30,960,406D/1,728,200E。若实际metadata另需调用，必须重算且总预留≤65,536，不能删任何必需主体/phase。每案final/资金均未结时保存完整原件，按CSF-06全量恢复；只保存热state摘要为失败。

### 6.4 完整canonical封套的独立编码

下列标准库脚本仅生成**规范尺寸shape**：使用原ABI的ctx/action/state与完整ModuleOutput字段，outer Vec按CBOR整数数组编码；内层payload/envelope也先完整编码再被外层包裹。source_projection_use完整含schema/purpose/context/audience/consumer policy id/version/hash/source-effect predicate，family为continuing_service_v2；签名、authority/source refs是固定占位，不是真实有效签名；此方法不运行服务、不证明business schema/证明真实性。运行验收仍需用正式codec对真实候选复算并执行前述业务正向。

可复算期望如下：

| 规范shape | 完整state原字节 | 内层全部编码后action | 完整input | 完整output | 当前公式实际费 |
| --- | --- | --- | --- | --- | --- |
| 小合同activation、3principal/4代表签名 | 1 | 15,204 | 30,794 | 12,850 | 47 Data /4 Electricity |
| 小合同active work | 6,332 | 1,343 | 15,706 | 12,850 | 32 Data /4 Electricity |
| 小合同全部历史/phase/claim索引的普通final | 9,610 | 1,354 | 22,245 | 19,368 | 用真实结果另计，完整落在64/32KiB |
| 同一peak热状态与16引用证据 | 9,610 | 11,554 | 42,629 | 19,368 | 完整落在128/32KiB |
| 最大热索引形状：32历史方/64slot/256fragment/16case/512work roots | 37,503 | 1,343 | 73,398 | 70,542 | 使用通用256KiB配置，不强塞小合同32KiB输出 |

8个规范wire节点为activation 30,794/12,850B，其后begin各15,694/12,850B、complete各15,706/12,850B及close15,694/12,850B；实际尺寸代入当前公式得到47+7×32=271Data/32Electricity。它们是完整编码的计量形状，不是运行过的8个World效果；真实执行须核其真实输入输出/状态/receipt后再计费，不强令真实fee等于该数。小合同peak热状态还包含所有4历史主体、两阶段计数/票与决定根、残余claim/hold和8工作roots；追加16条source view可正向编码，未删任何phase机会。

最大shape的16,384个冷引用正文为1,082,439B，完整proof/原件另按实际保留，不能因为仅该引用列表约1MiB便省略源。内部Hash32明确编码成canonical byte-string；若同shape把哈希改成64字节十六进制字符串，state变为69,343B，超过64KiB，必须拒绝该encoding。不能把这个失败解释为可以删除后继方或未结案。

另用128KiB的raw u8数据做外层数组反例，其CBOR开销已超过256KiB，不能因raw≤128KiB放行；给真实候选的完整封套做256KiB与256KiB+1边界（连wrapper/state/proofs都算），后者必须在publication前拒绝。相同子上限分别通过不证明合起来通过。decoder对过深嵌套、节点/allocator超界也应先拒绝，不能只检wire长度。

以下代码固定参数、canonical排序与所有字段，可另存为临时脚本复算；不调用仓库业务程序。

```python
from hashlib import sha256
from math import ceil

def head(m, n):
    if n < 24: return bytes([(m << 5) | n])
    for tag, size in ((24,1),(25,2),(26,4),(27,8)):
        if n < 1 << (8*size): return bytes([(m << 5) | tag]) + n.to_bytes(size,'big')
    raise ValueError('integer overflow')

def cb(x):
    if x is None: return b'\xf6'
    if isinstance(x,bool): return b'\xf5' if x else b'\xf4'
    if isinstance(x,int):
        return head(0,x) if x >= 0 else head(1,-1-x)
    if isinstance(x,str):
        b=x.encode(); return head(3,len(b))+b
    if isinstance(x,bytes): return head(2,len(x))+x
    if isinstance(x,list): return head(4,len(x))+b''.join(cb(v) for v in x)
    if isinstance(x,dict):
        rows=sorted(((cb(k),cb(v)) for k,v in x.items()),key=lambda p:(len(p[0]),p[0]))
        return head(5,len(rows))+b''.join(k+v for k,v in rows)
    raise TypeError(type(x))

h=lambda v:sha256(cb(v)).hexdigest()
limits=dict(max_mem_bytes=67108864,max_gas=1000000,max_call_rate=4,
            max_output_bytes=32768,max_effects=1,max_emits=1)
graph=[['service',h('artifact'),'P1',0],['service',h('artifact'),'P1',1]]
terms=dict(schema=1,world='w',branch='b',instance='s',policy=h('policy'),
 parties=[['P0','individual','buyer','owner0'],['P1','individual','provider_fee','owner1'],
          ['P2','organization','asset_owner','signing_clause']],
 checkpoints=[[0,100,200],[1,201,300]],Q=10,
 units=[[0,0,'maintenance_delta','asset',0,1,'whole','P1','P0',200],
        [1,0,'material_delivery','lot',0,10,'integer','P1','P0',300],
        [2,1,'work_contribution','work',0,1,'whole','P1','P0',100]],
 source_scopes=[['asset',h('asset_grant')],['lot',h('title_custody')],
                ['work',h('work_role')]],price=['P0',600],
 risk=[['P1','P0','proved_loss',30,30,h('risk_predicate')]],
 normal_plans=[[i,graph,h(['physical',i]),h(['safe',i])] for i in range(4)],
 safety_plans=[[i,graph,h(['return',i])] for i in range(4)],
 cancellation=[['unstarted','release'],['started','actual_safe_disposition'],
               ['earned','pay'],['unpaid','keep_original_claim']],
 receipt_predicates=[h(['predicate',i]) for i in range(3)],
 capacity=[h(['capacity',i]) for i in range(4)],limits=[4,2,6,1,4,4,371],
 fees=[['normal','P1',1022,40],['procedure','P1',50044,1360],
       ['closure','P1',2079,84],['trade','P0',6,0],['award','P1',4,0],
       ['physical','P1',0,40]],
 fee_authorities=[h(['fee',i]) for i in range(6)],
 panel=[h(['juror',i]) for i in range(6)],random=h('random_authority'),
 source_projection_use=dict(schema='source_projection_use.v1',purpose='service_performance',
  context=dict(kind='service_obligation',scope='original_agreement'),
  audience=dict(kind='parties',scope='own_obligations'),
  consumer_policy=dict(id='contextual_reputation',version=1,hash=h('consumer_policy')),
  source_effect_predicate=dict(schema=1,kind='actual_final_service_outcome',
     unit_scope='original',requires=['actual_effect','complete_coverage','current_validity'])),
 deadlines=[[0,220,240,260,270,290,300],[1,320,340,360,370,390,400]])
family_digest=h(terms)
common=dict(schema=2,family='continuing_service_v2',family_digest=family_digest,
 family_body=terms,required_principals=['P0','P1','P2'],
 authority_sources=[h(['mandate',i]) for i in range(3)],
 source_projection_use=terms['source_projection_use'])
digest=h(common)
accepts=[dict(principal=p,terms=digest,render=h(['render',digest]),
             signers=[dict(controller=c,signature='f'*128) for c in signers],
             mandate=h(['mandate',i]),proof=h(['current',i]))
         for i,(p,signers) in enumerate([('P0',['C0']),('P1',['C1']),
                                         ('P2',['C2','C3'])])]
state=dict(schema=1,terms=terms,terms_digest=family_digest,common_digest=digest,
 partitions=[[0,0,1,'unstarted'],[1,0,10,'unstarted'],[2,0,1,'unstarted']],
 party_phase_counts=[[[0,0] for _ in range(4)] for _ in range(2)],
 case_roots=[h(['case',i]) for i in range(2)],request_root=h('requests'),
 holds=[h(['hold',i]) for i in range(12)],work_root=h('work'),fees_used=[0,0])
def env(command):
    return cb(dict(namespace='service',name='apply',schema_version=1,
                   schema_hash=h('command_schema'),payload=list(cb(command))))
def inp(raw_state, command):
    context=dict(v='1',module_id='service',trace_id='shape',time=100,
       origin=dict(kind='agent',id='A0'),caller=dict(kind='agent',agent_id='A0'),
       limits=limits,world_config_hash=h('world'),manifest_hash=h('manifest'),
       journal_height=100,module_version='1',module_kind='reducer',module_role='institution')
    action=env(command)
    return action,cb(dict(ctx=context,state=list(raw_state),action=list(action)))
def out(raw_state):
    result=dict(new_state=list(raw_state),
       effects=[dict(kind='protected_resource_plan',params=dict(plan_ref=h('plan')),
                     cap_ref='original_cap')],
       emits=[dict(kind='service_receipt',payload=dict(result_ref=h('receipt')))],output_bytes=0)
    for _ in range(4): result['output_bytes']=len(cb(result))
    encoded=cb(result)
    assert result['output_bytes']==len(encoded)
    return encoded
activate=dict(op='activate',request='q0',expected_version=0,terms=common,
              acceptances=accepts,authority_root=h('authority'),fee_graph=graph[:1])
work=dict(op='complete',request='q1',expected_version=1,agreement=family_digest,
 unit=[1,0,10],result_ref=h('transport_receipt'),coverage=h('coverage'),
 authority_root=h('authority'),work_graph=graph,source_use=h('source_use'))
raw=cb(state)
for label,old,command in [('activation',cb({}),activate),('active_work',raw,work)]:
    action,encoded=inp(old,command); output=out(raw)
    print(label,dict(state=len(old),action=len(action),full_input=len(encoded),
       full_output=len(output),actual_D=ceil(len(encoded)/1024)+ceil(len(output)/1024)+3,
       actual_E=4))
steps=[activate]
for i in range(3):
    for op in ('begin','complete'):
        c=dict(work); c.update(op=op,request='q'+str(len(steps)),unit=[i,0,1 if i!=1 else 10])
        steps.append(c)
close=dict(work); close.update(op='close',request='q7',unit=[0,0,3]); steps.append(close)
sizes=[]
for i,c in enumerate(steps):
    _,encoded=inp(cb({}) if i==0 else raw,c)
    output=out(raw)
    sizes.append((len(encoded),len(output),ceil(len(encoded)/1024)+ceil(len(output)/1024)+3,4))
print('eight_wire_shapes',sizes,'sum_fees',(sum(x[2] for x in sizes),sum(x[3] for x in sizes)))
peak=dict(state)
peak.update(
 history=[[i,h(['party',i]),h(['grant',i]),h(['segment',i])] for i in range(4)],
 case_phase=[[i,h(['case_phase',i]),[[16,16] for _ in range(4)],
              [[h(['vote',i,j]),j%3] for j in range(6)],h(['final',i])] for i in range(2)],
 residual=[[i,h(['fragment',i]),h(['claim',i]),h(['hold',i])] for i in range(4)],
 plans=[h(['plan',i]) for i in range(8)])
peak_raw=cb(peak)
base=dict(work); base['op']='finalize'; base['case']=1
proof=dict(base); proof['op']='append_evidence'
proof['source_view']=[dict(ref=h(['ref',i]),before=h(['before',i]),after=h(['after',i]),
   quantity=i,role='accepted_role',source=h(['source',i])) for i in range(16)]
for label,c in [('peak_base',base),('peak_evidence',proof)]:
    action,encoded=inp(peak_raw,c); output=out(peak_raw)
    print(label,dict(raw_state=len(peak_raw),action=len(action),full_input=len(encoded),full_output=len(output)))
    assert len(peak_raw)<=12288 and len(encoded)<=(65536 if label=='peak_base' else 131072)
    assert len(output)<=32768
cold={str(k):[[h([k,p,phase,e]) for e in range(16)]
             for p in range(32) for phase in range(2)] for k in range(16)}
bh=lambda v:sha256(cb(v)).digest()
hot=dict(schema=1,terms_root=bh('max_terms'),
 parties=[[p,bh(['party',p]),bh(['mandate',p])] for p in range(32)],
 slots=[[i,i%16,bh(['unit',i]),bh(['predicate',i])] for i in range(64)],
 fragments=[[i,i//4,i%4,(i%4)+1,bh(['source',i])] for i in range(256)],
 cases=[[i,bh(cold[str(i)]),[[16,16] for _ in range(32)],bh(['decision',i])]
        for i in range(16)],work_roots=[bh(['work',i]) for i in range(512)],
 budget_root=bh('budgets'),request_root=bh('requests'))
hot_raw=cb(hot)
limits['max_output_bytes']=262144
action,encoded=inp(hot_raw,work)
print('max_hot',dict(raw_state=len(hot_raw),full_input=len(encoded),
                     refs=sum(len(v) for rows in cold.values() for v in rows),
                     cold_refs_bytes=len(cb(cold)),full_output=len(out(hot_raw))))
def hex_hashes(x):
    if isinstance(x,bytes): return x.hex()
    if isinstance(x,list): return [hex_hashes(v) for v in x]
    if isinstance(x,dict): return {k:hex_hashes(v) for k,v in x.items()}
    return x
print('bad_hex_hot_state',len(cb(hex_hashes(hot))))
assert len(cb(hex_hashes(hot)))==69343
assert len(hot_raw)<=65536 and len(encoded)<=262144
assert len(cb({'x':list(bytes([255])*131072)}))>262144
def counts(H,K,N,S,F,J):
    stage_counts=[H,H*16,H*16,H,N,N,N,N,1,2,2,1,H]
    case=sum(stage_counts)
    M=sum([1,S,H,K*case,F,H,1])
    return case,M
case_small,M_small=counts(4,2,6,1,4,4)
case_max,M_max=counts(32,16,64,16,256,256)
tight_fees=[(32+32+3)+1*(128+32+3)+8*(64+32+3),
            256*(128+32+3)+84*(64+32+3),21*(64+32+3)]
assert (case_small,M_small)==(170,355)
assert (case_max,M_max)==(1382,22450)
assert tight_fees==[1022,50044,2079]
assert sum(tight_fees)==53145
print('arithmetic',dict(case_small=case_small,M_small=M_small,nodes_small=M_small+8*2,
    purpose_D=tight_fees,module_small_D=sum(tight_fees),module_small_E=(M_small+8*2)*4,
    case_max=case_max,M_max=M_max,nodes_one_metadata=M_max+512*64,
    D_one_metadata=M_max*515+512*64*592,E_one_metadata=M_max*4+512*64*50))

```

<a id="csf-06-recovery-read"></a>
## 7. CSF-06：持久三态、完整原件和三表面理解

### 7.1 故障与同一原操作

对activation、真实维修/供能发布、transit出发/完成、work outbox、最终裁决/资金、继受、最后close逐一固定原operation/candidate，分别注入：prepare前、准备完publication前、durable决定处、commit已成但回复丢失、snapshot边界及tail恢复。不能只测同一个失败点。

| 独立authority事实 | 恢复必须发生 | 失败判据 |
| --- | --- | --- |
| 原候选NotCommitted有真实证明 | 丢弃该候选全部staged变化/计量，保留原合法独立拒绝审计；需要新操作仍按当前scope | 把网络失败当未提交，或留下半边hold/资源/fee |
| 原候选Committed | 安装/读取同一原完整delta/receipt，不再执行物理动作、module、抽样或扣费 | 按当前owner/价格/旧余额重建不同结果 |
| StatusUnknown | 隔离整个旧World写与tick，保留原candidate，按原ID查询权威；核为Committed或NotCommitted才解除 | 只冻结服务行、继续旧World tick；新ID重试；把没有ACK当退款条件 |
| 物理已Committed但服务尚未核定 | 原source-use reservation/outbox与真实结果仍在；只完成原核定/未执行节点 | 维修、供电、运输、实验再次执行，或删掉真实结果 |
| final/付款/尾部未知 | 保留原有效责任、钱/物原权利和请求；按原候选恢复 | 缓存没有final便中性释放，或有效award到界被清零 |

恢复后必须核same-key/same-payload、same-key/different-payload、新key/same业务unit、相同物理source跨agreement及历史生效版本。分别检查普通native、direct/trusted、raw publication、无module due、fee、资产转让/删除/claim回收、module升级/停用路径都不能绕hold/source-use。某路径未适配就不能给服务族全消费者通过。

### 7.2 16案与完整留存

选择CSF-05的16案/32历史方/16,384 refs的全部未结状态，包含不同初审/申诉/final、paid/unpaid、旧后继主体、原物理在途和已付fee。独立导出每个原terms/acceptance/mandate、proof、source输入输出、case名册/随机/票/排除/截止、work/outbox、held/fees/claims/request、所有purpose与phase槽；导出结果的完整根作为外部oracle，不能仅导出模块热state。

在双快照各自有效水位、older snapshot+tail、缺最后ACK、source/index版本不齐及重启场景恢复。全页读取回同一根/水位，逐原件比对16,384引用及其完整可达内容；每主体每阶段机会/未决性一致。删一个冷proof、一个旧owner接受、一个原fee node、一页org inventory或一个terminal source，应该明确恢复/读受阻，不能补成空或“已无责任”。把原件压成不可回读摘要是失败，即使热statehash没变。

重放不得运行WASM/模型、重抽、按新price重价、转给新owner或发新replay credit。已关闭后的tombstone/lineage/source-use和原责任仍可核；TTL/GC不得重新开放单位或抹unpaid。源客观纠错只追加有效replacement/invalidity/current cut，读者看到新有效性但原物理事实和实际费用不能被删除。

### 7.3 当前受众、source_projection_use与玩家理解

在同一root查询API、Viewer、Agent。用当前各party、已退出旧责任方、后继方、无关旁观者及同一人合法credential轮换作矩阵；确认source_projection_use覆盖具体情境/用途/受众。旧方可读自己的原付款/责任，不能读新方私有余额；公开摘要不泄露私证，也不能把缺可见cause描述为已确认错误。current grant撤销/轮换时缓存页必须重验，不能持旧cursor越权。

页长64KiB/64记录、单请求4页；拿到第一页时明确partial/next_cursor，不能显示“全已完成/无负债”。切root续读须固定cut或重启，不能把不同代页拼成全量。不读全case/receipt页的CR003消费者不能自判default；pending appeal、final、paid、unpaid分别呈现，继受不转个人credential。

按实际发布表面选取具有相关角色的真人参与者，事前记录样本/任务与环境，避免只由设计作者演示。让参与者分别解释：(1)哪些已真实交电/维修/送达/完成工作；(2)哪些只是active/arrival/被保全；(3)谁有未付义务及证据；(4)能否只修残余、何时可终止/申诉/换方；(5)资金/货物是否已到账；(6)缺权限/资料/预算时的合法下一步。独立观察其在三表面定位原件/执行安全操作，记误判而不提示答案；API结构检查不能代替理解度。

## 8. 记录与完成判定

每个方法记录实际候选、authority/源、正向和独立反例、原件/差分、真实费用、运行环境及未测边界。出现原件不可读、当前权限未知、未适配consumer或未执行故障域时保留blocked/unproven，不由静态表格或成功示例补签。

本手册内的算术/规范CBOR脚本只证明所列输入形状的计算结果。政策、执行设计和方法经过独立非作者审读可支持设计覆盖结论；实际业务、正式来源/随机/模块、恢复、跨节点和玩家体验必须由对应运行证据分别支持。原产品实现状态与供能历史证据保持原义。
