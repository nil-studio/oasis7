# 代表性工业执行档案验证手册

- 状态：设计与验证方法源；本次只做文档/源码审读、数例复算与静态门禁，没有执行 World、Rust/WASM、模型、正式客户端、故障注入或真人测试。
- Owner：qa_engineer；独立领域审阅由 M4、runtime、gameplay 与 Viewer/Agent owner 配合。固定审读基线：nil-studio/oasis7@37f7719f6a68b23c6f8f6632571d82ce33a1b4e6。
- 唯一产品范围：[root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117。专业源：[RI-01～04](../../world-runtime/runtime/representative-industrial-execution-profile.design.md#rie-profile)；玩家源：[walkthrough政策](../../game/gameplay/gameplay-industrial-representative-execution-walkthrough.prd.md#representative-profile-policy)。

## 1. 执行前分层与证据包

| 层级 | 可证明什么 | 不能代签什么 |
| --- | --- | --- |
| 静态设计审阅 | 原REQ/SC覆盖、具体profile完整、数量/费用/所有权/恢复逻辑自洽、来源及链接有效 | 实际authority签发、正式入口、运行性能、真实理解与意愿 |
| 后继组件/确定性fixture | 已接通边界的材料/容量/receipt once、拒绝与恢复不变量 | 直接构造前态不能证明身份/原料/工厂来自正式producer |
| 同候选组合 | 正式接受→实际writer→同generation读面→Viewer/API/Agent，正常/错误/恢复同义 | 没有真实用户观察不能称体验可用或继续意愿已证明 |
| 真人/服务 | 使用已具备capability的候选，观察玩家解释、选择、行动与恢复；记录实际环境和成本 | 模型或Agent代操作、完成数量、停留时长不等于独立理解或长期留存 |

实施验证前保存：source/tree/build和真实部署环境、world/branch/C0 state-root/完整journal界、RIE profile版本/采纳记录、角色权限、factory与Site/route/profile revisions、正向来源、编码后的完整proof/manifest、实际request/root/child refs、入口capability、实际费用payer和预算、预期表及结果记录位置。敏感签名/个人/私有原件只放受权证据包，公开报告给脱敏ref与可授权查验方式。

新profile若未被owner采纳或正式producer/初始binding/terminal writer/全writer容量守卫未接通，相应运行项记录 blocked/not_run 与准确缺口；设计审阅仍可评价完整度。不得用“无法运行”删除原required/full方法，也不得将该限制写成当前功能已可用。

<a id="rie-m01"></a>
## 2. RIE-M01：档案完整、来源、守恒与预算

### 2.1 可编辑源与原authority交接

从 [RI-01](../../world-runtime/runtime/representative-industrial-execution-profile.design.md#rie-profile)逐行读入profile，不从图稿、旧缓存或别的PR拼同名参数。版本化数据清单必须覆盖下表；未选择的T用途在P中只是可读后继，不产生hold或终端接受。

| 核查对象 | 正常资料与独立判定 |
| --- | --- |
| 同一候选身份 | P/T分开接受、world/branch/candidate/root/request不混；一个版本对应一份完整内容与canonical摘要，当前配置与历史配置分别保存 |
| 能力/权限 | F实际建成、builder A、精确native smelter spec、当前recipe lifecycle/scope/工厂适配；S/I/T访问和L的route合法性、V的真实artifact ownership均有独立来源 |
| 账本 | I是F的实际input/output同一Site；S ore5、I fuel1、I ore/iron/slag0、T iron0。原料来自合法旧库存；C0前成本保留，不把本样本当免费bootstrap |
| 配方/维护 | exact iron recipe一批，原有效consume是ore4/fuel1、8E、1tick、iron3/slag1；确认有效product maintenance_sink没有额外材料，不能忽略实际字段 |
| 路线 | e-in显式S→I、100km/cap5/tariff1、High20bps；e-out显式I→T、1km/cap3/tariff1、Medium10bps；global inflight起始0、两腿顺序不超过1，原global cap2仍生效 |
| 资格 | ore/fuel分别有spec/quality/custody/profile、真实provenance/quantity与finality certificate；source_ledger=I、owner binding、expiry/height准确。arrival只证明到达，必须另有正向applicability |
| 有限容量 | escrow ore4/fuel1；I输出iron3/slag1；T iron3。包含所有既有余额/holds/pending输出/其他writer，不用整数上界或本root局部余额代替共享cap |
| terminal | 确切owner A、recipient A、T、3iron、owner_inventory_acceptance/rie-v1、parent production/arrival、capacity和独占义务；无需求或奖励消费者 |
| read/执行限额 | 完整proof/数据封套、有限records/branches/parents与所有页的覆盖标记；不能取前N或删字段后假称完整 |

原生参考入口为 [catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)、[material defaults](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs)、[loss/ticks](../../../crates/oasis7_world_state/src/world/pure_helpers.rs)、[qualified import/root](../../../crates/oasis7_world_state/src/world_rules/industrial_work_admission.rs)、[StartRoot](../../../crates/oasis7_world_state/src/world_rules/industrial_work_start.rs)。静态检查只能读这些现有源；后继自动化应扩展原工业/物流/恢复测试职责，不把尚不存在的测试名或文件作为通过证据。

### 2.2 独立数量与能量复算

| 切点 | 必须成立的等式/结果 |
| --- | --- |
| ore输入腿 | start后S原5=源剩余0+在途5；此时received4/loss1只是原公式预测，不是actual。completion后loss=floor(5×100×20/10000)=1、received4，S原5=I实际4+已结算loss1、在途0。pending期间不能又把5算源可用库存，也不能先扣实际loss |
| qualified导入 | 原普通I ore4/fuel1 → 0/0；共享工业escrow→4/1。每种材料数量不增不减；尚无production/input sink |
| join与生产 | escrow4/1只消费一次；原生一批产iron3/slag1。不能把不同材料的“输入5、输出4”直接解释为1单位物流损耗，recipe变换与运输损耗分别对账 |
| iron输出腿 | loss=floor(3×1×10/10000)=0，received3；I iron3→0、transit3→T3；slag1仍在I，两个变体都不删除它 |
| terminal接收 | admission前后T物理余额都为3；terminal状态由pending→settled一次，容量占用仍为3；不能再credit3或因accepted清掉真实占用 |
| A业务费用 | P:5+8=13E；T:5+8+3=16E。运费由A escrow转L，生产8E单独sink；两者不重复计费 |
| V调用 | 真正pinned iron artifact，输入I_bytes≤4096；preflight≤10Data/4E，actual=ceil(I_bytes/1024)+2 Data和2E。完整编码字节而非字段raw相加；没有owner的兼容免收费路径不得作为正例 |
| 背景/耐久 | A64E、V64E/128Data；原折旧每tick最多2000ppm，32tick至少剩936000/1000000。完整writer清单证明额外材料/E/Data delta0时，P A51E/T A48E/V62E、V Data扣实际值、L收5/8E；原耐久变化另外记录 |

独立审阅者从原公式重新算，不只检查作者表中两个数字相等。明确核所有product validation resolver命中：iron1次、slag无适用validator且本profile正面声明not_required；完整module registry/订阅集合不能留下隐藏调用。记录逐个原tick/background writer及其输入、是否触发、资源delta；任何漏读或额外delta使字面终值不可用，需修正前态和预算或记录not_run，而非补零。

费用来源使用 [原公式](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)、[metering](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)、[产品模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_product_module_template.rs)与原authority验证。外部签发/托管/人工或模型服务费按真实环境另记；本次未发生的费用写未发生/未测，不把游戏E单位换成现实价格。

<a id="rie-m02"></a>
## 3. RIE-M02：两个独立正常例与消费者

### 3.1 后继组合执行步骤

在两个隔离但内容相同的C0副本分别运行P和T，不在已发生效果的root上切completion profile。每步先读前态和原请求状态，再执行真实可用的正式动作并按authority结果推进；无结果只保留pending/unknown，不以超时认定失败。

| 次序 | 操作/观察 | 正常判据 |
| --- | --- | --- |
| 1 | 三入口只读同一档案/quote，并比较P/T投入和收益 | 可授权字段/primary blocker/next_action一致；balance、root、hold、queue/W/validator调用数均不变化 |
| 2 | A显式确认e-in，读原start | S扣5，原transit5/route5/tariff escrow5成立；I ore0；同请求重复不多扣 |
| 3 | 推进到原ready_at，读arrival及L账 | I ore4、loss1、route释放5、L收5E；到货没有quality标为applicable的越权推断 |
| 4 | 原authority确认各qualified事实，分别导入、接受root、初始绑定两slot | 普通I余额0/0、escrow4/1；准确父refs和baseline；未通过正式初始binding时保持blocked，不手工补parents伪造成功 |
| 5 | fresh StartRoot | 一次join sink4/1、8E、1job；pending输出义务iron3/slag1；不产生半批或第二次普通账扣减 |
| 6 | 原ready_at后验证并完成 | 一次实际iron产品验证/费用；两输出同事务credit，production receipt回指原root/parents/job；P只在此完成produced/undelivered，T仍未完成 |
| 7，仅T | 显式选择本已接受T义务的发货，核全writer容量guard后launch | 一次I扣iron3、A3E、transit3、route3、T reservation3；slag1留I |
| 8，仅T | 原arrival、再由当前A明确接收 | 到货时T3但terminal_pending；matching admission后T仍3/occupied3、terminal_settled一次；L再收3E，无重复credit |
| 9 | 重连、同身份重读与选择下一步 | P完成并保留原I库存，T完成并保留T库存；首批不stable；实际后继只在原capability/余额/权限支持时可选，不自动再生产或授予收益 |

receipt检查要求原source/production/arrival/terminal四类身份逐项回溯。Root/operation的目标接受不是旧传输ACK；production receipt不是输入proof，不是terminal receipt；terminal receipt不是下游实际使用。P结束前没有T reservation/arrival/admission；T中的production、arrival和terminal都是不同事实，完成计数不累加成3次。

### 3.2 三入口与稳定反例

固定同一read-anchor，逐项对照Viewer/pure API/Agent：工厂与recipe版本、输入数量/资格/父来源、effective path与损耗/容量、full-only周期、power/validation成本、两个输出、终端用途、已消费/仍占用/损失、primary blocker、opportunity cost、next_action/recheck与progression。无权读取第三方详情时三入口同样给授权摘要，不按隐私缺字段推断安全。

正常例W初始0、仅一笔生产；它不满足 [canonical stable-line](../../../crates/oasis7_world_state/src/state/industry_transition/mod.rs)所需的至少3个相同canonical完成，也不能省略 [current candidate匹配](../../../crates/oasis7_world_state/src/gameplay_capability.rs)。重复读receipt、到货或terminal接收不得增加生产W。后继若验证重复周期，必须各自有新鲜输入、已清出的真实output room、真实费用和匹配生产receipt；未知RIE/canonical lineage不能靠同名recipe标stable。

记录静态文本审阅和实际客户端观察的区别。[T-ECO-04文本稿](../../world-simulator/viewer/viewer-economic-targets-uiux.design.md#t-eco-04-root-sc28-profile)存在不等于DTO/按钮已接通；缺capability时禁用写动作且保留原因，不能凭目标图点击模拟真实结果。

<a id="rie-m03"></a>
## 4. RIE-M03：单变量失败、恢复与未知提交

每个反例从保存的合法C0/边界副本开始，只改一项原authority事实；保存改动来源与fresh快照，重复/乱序使用同一effect identity。先证明首个非法效果没有发生，再检查真实已发生效果仍在，最后恢复原条件验证同义务一次继续。不得让测试恢复用免费材料或强行改账消除失败。

| 编号/切点 | 变化与预期 | 恢复/守恒检查 |
| --- | --- | --- |
| R03-01 input unknown | 分别删失ore与fuel的受信证明，或让quality不适用/证书过期/owner不匹配；未通过的lot不导入、不sink；另一合法lot保持原余额或escrow | 合法同source证明恢复后重验两slot；不重复e-in损耗/费用，不用“最新同名lot”填slot |
| R03-02 join未齐套/到达乱序 | 置换两个parent到达/绑定的观察顺序、重复arrival；先到一类不能开始 | 缺项补齐时一个join/一个job；每个parent只sink一次、sink key不随binding revision重置 |
| R03-03 route/global满 | 分别用真实既有hold占满e-in route5，或占满global inflight2；新请求原子无扣减、无隐含hold或queue | 原占用真实释放后fresh重报，仅未决e-in重评；retry不刷新接受时点/优先级，已完成腿不重启 |
| R03-04 power/fit漂移 | quote后A可用E降到7，另例F pause/tag/scope变更；start前拒绝，无新sink或WIP | 原有权资源/能力恢复后同因果续接；若修复需无权或不可负担资源，明确无安全动作 |
| R03-05 slag去向start前失败 | I slag空间已占1或owner/access失效；不能只credit主产物或删副产物 | 恢复同destination后完整bundle一次开始/完成；未改变required/output因果才能保留原candidate |
| R03-06 slag去向start后失败 | 让mandatory branch在finish前失效；4/1和8E已sink不退款，iron/slag都无新credit | 一个有限WIP保留原两branch；有已声明hold/finish能力才允许此profile启动。修复后一次bundle，不把普通Terminate当salvage |
| R03-07 T发货前失效 | T满3、owner/recipient/资格被撤销，分别验证；I iron3与slag1保留，原production不撤销，e-out无扣减 | 同T资格恢复才launch；修改T/purpose是新candidate，不自动改收货人 |
| R03-08 T到货待接收 | arrival后撤销admission资格；T物理3且pending独占，占cap3，不accepted、不reward | 恢复后原terminal key接收一次，不再credit3、不释放库存真实占用；另一writer不能花pending3或绕cap |
| R03-09 验证失败/余额不足 | V不足preflight预算、artifact/hash漂移、iron验证否定，分别保留原因；不得通过缺payer分支免收费 | 按原validation attempt/receipt恢复，已提交调用结果不再invoke/charge；未发生的合法调用才fresh执行，全bundle边界保持 |
| R03-10 retry/reconnect | 同请求同payload重复、同键改payload、两个不同intent同payload、consumer重读 | 分别重读/冲突/独立归因；任何profile/root不消费另一份receipt；没有新材料、费用、容量、W或terminal效果 |
| R03-11 checkpoint/replay | 在导入、input sink、bundle、transit start/arrival、terminal admission前后保存合法snapshot/journal，制造重启/重放 | 与不中断对照逐receipt/delta/root/capacity/bucket一致；不重算历史损耗、费用、validator或签发证据，缺parent/索引/anchor拒绝安装 |
| R03-12 publication未知 | 在原G1发布前后切断确认，分别取得NotCommitted/Committed/StatusUnknown | NotCommitted才按原键规则重试；Committed读原结果；StatusUnknown隔离整个旧World所有writer/tick，先恢复generation，禁止换ID重交或局部goal解锁 |
| R03-13 因果cutover | 未sink、WIP、transit、buffer、terminal_pending分别改变recipe/output/terminal用途 | Prepare完整枚举旧义务，只有已有合法hold/finish/未消费释放才Commit；新candidate W=0，旧actual/receipt/顺位不迁移；缺处置整次拒绝 |

R03-11/12必须共同覆盖业务状态与控制状态，不能只核材料总数。一个世界所有writer/tick的隔离要有原执行层证据；前端“按钮禁用”不能证明G1。未知请求恢复引用[原request recovery方法](../../../testing-manual.md#request-recovery-regression)与 [Runtime事务设计](../../world-runtime/design.md#62-unified-executiontransaction-architecture-p0)，新profile要在自己的实际候选上取得证据。

复核器独立检查不变量：同材料source=仍可用+独占hold/在途/实际destination+已消费/损失，bucket互斥；配方按原变换关系逐material核，不直接跨材料相加；容量held+occupied不超各业务cap；all-output无部分credit；每个终端用途只有matching settlement；每个已收费invoke/运输/生产不因观察失败再发生一次。补偿若原profile不支持应拒绝，不能为了凑测试覆盖新增退款。

<a id="rie-m04"></a>
## 5. RIE-M04：full三阶段、跨窗口与体验交接

### 5.1 full 正常配置与独立算例

先独立接收基础P/T，再使用 [RI-04](../../world-runtime/runtime/representative-industrial-execution-profile.design.md#rie-full)的 RIE-IRON-GEAR-MOTOR/v1 完整独立C0。不要对已接受RIE-IRON-1原地改cap、材料、recipe或用途；三个stage保留共同pipeline lineage和各自child/parents，当前native单job记录不是完整pipeline已接通的证明。

| 对象 | full固定预期 |
| --- | --- |
| 初始材料与来源 | S ore10，I fuel2，J copper_wire3/hardware_part1；I/J/T相应输出0；F和J上的Assembler均已合法建成，实际world已有所需scale_out/recipe准入，不由本样本白送 |
| 输入腿 | ore10经100km/High20bps/cap10/tariff1：loss2、arrive8、1tick、10E；route释放发送量10 |
| stage1 | native iron2批有效sink ore8/fuel2、16E，产iron6/slag2，duration1tick；iron4去J、iron2保留I，slag2独立保留I |
| 中间边 | iron4走I→J 1km/Medium10bps/cap4/tariff1：loss0、arrive4、1tick、4E；拆分child+parents准确，J到货后独立quality/custody证明 |
| stage2 | native gear2批，iron4/8E→gear2，duration1tick；J gear cap2，产品验证按已采纳not_required政策且无fallback |
| stage3 | native motor1批，gear2+wire3，加原motor maintenance_sink的hardware1，共3个required slots；7E→motor1，duration1tick；J motor cap1，全部parents适用才sink |
| 终端腿 | motor1走J→T 1km/Low5bps/cap1/tariff1：loss0、arrive1、1tick、1E；T motor cap1，另有明确matching owner库存接收，arrival不能代签 |
| 终态材料 | S ore0、输入escrow均0，I iron2/slag2，J gear/wire/hardware/motor均0，T motor1；oreloss2单列。所有不同材料按recipe关系核，不把中间iron6和gear2当同时保有的终态库存 |
| 完整费用/耐久 | A业务46E，C0 A64E→18E；L收15E；V iron6和motor1各一次实际validator，共4E、Data≤12；preflight合计≤8E/20Data，V64E/128Data足够。128tick每厂折旧最多256000ppm。上述终值仍要求完整背景writer额外材料/E/Data delta0 |

独立检查原 [motor module](../../../crates/oasis7_builtin_wasm_modules/m4_product_component_motor_mk1/src/lib.rs)的stack_limit128、精确artifact/manifest/payer及实际完整输入≤4096；原gear/slag无适用validator必须有完整registry证据。原effective consume不可漏hardware1；stage3若没有其合法来源/qualification就是明确blocked，不用自定义maintenance=0通过。

### 5.2 full 失败、共享与跨窗口

沿R03反例扩展到每个stage及edge：stage1 WIP、I→J transit、J gear buffer、stage3三个parents、T terminal_pending，分别检查故障后有限数量、原费用与恢复。至少两个accepted roots在同一有限output/route/terminal domain竞争，正向取得完整contender集；原样本策略按accepted canonical tick再root ID，full-only结果为一方full、另一方deferred，release后仍有效项获得分配，重连/retry不能插队。不同root所需原料独立有来源，不靠重复同lot构造两个合法承诺。

至少3个parent-linked diagnostic窗口分别含stage1/中间边、stage2/3、terminal及残余；每次checkpoint只在原合法原子边界关闭。未完成义务作为下一窗口opening bucket一次carry，不能重建材料、迁移receipt、刷新W或释放容量。Checkpoint邻接cutover时，对照 uninterrupted run，确认旧输入/iron余量/slag/费用/terminal obligations留旧revision，新candidate不继承旧实际成果。

为每种声明支持的状态保留一项正例和一项fail-closed反例；不支持output split fan-out、return、salvage、conversion或自动reroute时，验证“拒绝且保留旧义务”就是正确结果。full所需iron6→4+2的受权child数量划分与output split fan-out分别核查，不能因后者禁用而跳过前者。full三阶段不顺带接收所有SC21/24/27/29/32或市场功能。

### 5.3 玩家与服务验证安排

后继真人验证使用已接通、可追溯的候选及正常引导，由qa_engineer执行、gameplay owner独立解释结果。先让受试者查看P/T实际档案并自选明确用途，再完成一次正常链；随后用隔离副本展示一个输出或terminal阻塞，观察其能否指出已消费、仍保留、尚未完成和下一安全动作。不要在真实拥有库存的世界里人为破坏数据来制造事故。

分别记录玩家解释、选择和实际动作，及正常引导、额外提示/代操作的内容、时点和前后表现；不得为了“无提示通过”隐藏必要安全信息。询问/观察其为何保留库存、选择终端、等待或退出；不预设必须继续生产/使用T才是正确答案。无真实后继能力时明确当前只有保留/退出，不编造买家、奖励或免费恢复。可用性、独立理解、继续意愿分别结论，不能由完成计数或Agent成功推导真人理解。

证据包固定版本、平台/客户端与环境、参与主体/样本性质、实际场景覆盖、原request/receipt链、失败/恢复结果、提示与代办、资源和服务费用、耗时及偏差。费用未知写unknown，方法/估算与实际分栏。任何真实环境、模型或人工执行另按当前任务授权实施；本设计PR的静态核查不填这些运行结果。

## 6. 接收输出与变更维护

接收者按M01～04报告：已读范围、精确源版本、设计问题及修复、静态数例/链接/反向trace结果、独立review结论；实现/组件/组合/真人各自列pass/fail/blocked/not_run及证据来源。不要把全部未运行项目称为设计缺口，也不要为关闭父issue把not_run写pass。

本profile的编辑源是专业设计中的具体表和本方法；玩家政策与Viewer只保留用途/表达。更改任何配方/批次/质量或custody baseline、route/loss/capacity、费用/验证器、terminal用途、full后继或稳定条件，须更新profile版本、重新生成预期表、由另一审阅者复算并重新核三入口。历史receipt/证据不改写；旧source/tree、实际原件、权限与费用记录可回查。当前范围没有正式美术/音频新素材或外部授权依赖；若后续制作展示资源，沿原素材交付规范处理，不能用它们替代本档案。
