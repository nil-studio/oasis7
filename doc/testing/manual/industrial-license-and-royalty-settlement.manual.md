# 工业许可、版税与开放验收手册

- Owner role：`qa_engineer`；共同审读：gameplay、runtime、工业/配方、身份/治理、Viewer/Agent。
- 状态：`active` 验证计划；本轮未运行本功能、世界、模型、Rust、浏览器、真实发布或平衡测试。
- 固定设计审读基线：`ad2bf1bb9e29d315ea70b64603b57c206da1be0f`；实际执行时另记候选 source/tree、artifact、policy、manifest、authority、fee、随机来源和环境，不能把本提交当业务测试通过版本。
- 配套：[工业许可系统设计](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md)；唯一玩法政策：[GAME-018 首版政策](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#industrial-license-policy-v1)。下文数字仅为消费政策的边界或手算输入，不另设参数。

Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965

## 执行准备与证据分层

必须先有当前合法工业/配方准入、真实归属/预工作 charter、许可专属管辖/名册/随机、完整 recipe adapter、Kernel 全路径 hold、费用与兼容 drain、同根通知/账本、持久恢复以及正式入口。缺项保留其不可用/未验证结论，不通过修改 world 文件、放开权限、免费 fallback 或旧 receipt 冒充前置。

`test_tier_required` 验证原生 reducer、所有正式 effect gate、数学、拒绝、并发、事务故障与 canonical replay；`test_tier_full` 再覆盖实际节点/持久化/重启、正式 Viewer/API/Agent 和代表性玩家任务。测试源码、纸面手算、文档检查与业务执行分别记录，不代签彼此。

每例保存原输入、请求/业务键、terms/lineage/admission、当前相关 revisions、authority/费用/随机原件、前后 world root、生产/资金/槽位/权益差分、实际 receipt/N、case/final、读面及未覆盖范围。费用逐真实 source 和 resource 对账；不得只比较总世界 root。涉及未知结果时保留原 request，不为获得“成功”改 request 或清库存。

<a id="il01-authority"></a>
## IL01：条款、归属、lineage 与当前 authority

目标：[许可 authority](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md#license-authority)。

1. 构造一项来源完整、已通过四阶段准入、当前有效的精确 recipe/version/Profile，固定用途/区域/主体/对象、维护责任、期限、分配、收费 adapter、撤销/替代结果和许可专属复核依据。确认所有主体看到同一 digest 和可读正文。分别删改一项关键事实，预期付费激活原子拒绝并给出具体恢复方法；不能从 artifact owner、一次投票、OC 或旧 command grant 填补。
2. 比较技术 Apply、Shadow、模拟、有限 pilot、正式 admission 和完整许可：前四者不能产生商业付费使用或收益；只有 admission 也不能自动取得签发/收款权。对 product/material/factory/module 等无付费 adapter 能力，明确零版税与未知来源分别验证；前者仍须工业权限，后者不能默认“免费成功”。
3. 冻结 lineage 的首次准入 A、canonical 内容、parent/derivation/provenance。用同内容新 ID、非因果 metadata、维护版、换许可/收款人、换 Agent/请求重试，硬开放仍是原 A 的界限。再用来源独立且内容不同的能力验证不会因名字/语义相似强行并入旧 lineage；未知派生返回原归属审查，不创造新费权或免费复制他人资产。
4. 检查首版所有时间边界：许可开始前/恰好开始、单次最大期限和 +1、硬开放前/恰好开放、checked 加法溢出；续期仍不得超过原硬截止。对无有效当前准入、restricted/retiring/retired recipe、欠维护/设施/位置权限分别确认许可不能覆盖。
5. 多受益人使用实质工作前同版签署的 charter。覆盖 1、上限、上限+1，重复身份、零/负权重、权重和不等分母、事后补签、无同意/来源或历史贡献丢失。只有合法完整向量可收费；模型输出、后来的 artifact 占有和社交分数不产生归因。
6. 让受益个人与收款 Agent 不相同，随后转移 Agent 控制权；个人权利、原 provenance 和付款目的不转给新 owner。明确合法同意的有限收款/份额继受与自动身份转让分别测试；仅前者可在准确原范围生效，稳定 benefit slot 和 bps 不改变。

记录许可生效、收费权、工业权限和收款路径四个维度；测试当前可读不能代替当前可用。

<a id="il02-use"></a>
## IL02：正式使用、并发、资源与费用闭合

目标：[正式使用](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md#license-use)。

| 场景 / 刺激 | 执行与独立判据 |
| --- | --- |
| 只读与明确承诺 | 连续 inspect/quote/compare/续期请求、不同入口重复预览；生产资源、许可槽、版税 hold、费用和收益不变。当前同版确认才建立原 root 绑定与足额真实占用；不以 request accepted 作为已生效。 |
| 批次数边界 | B=1、政策最大 B、最大+1、零/负/溢出；当前完整 native plan 必须一致，客户端不能上传假数量。上限内还需所有资源/权限；上限外零部分写入。 |
| 四根并发 | 同 world/lineage/主体从不同 Agent、license、request 同时确认至上限，再发第5个；最多四个未闭合根成功。用别名/同控制映射和未知主体映射验证不能通过换键绕过。 |
| 终结后再用 | 连续完成五个根并等待各自生产、hold、notice、usage案与结果收敛且有效移交池；第5个可新确认。池尾差/个人收款待恢复不把总量误限制为终身128批，也不删除旧权利。仅生产结束但仍有pending-earned/usage案时则仍占根槽。 |
| 全入口合取 | 从正式 native Recipe、WorldRule root、WASM 能触发的工业 effect、自动续产及受权直接入口发等价请求；每条共同提交路径均重验当前许可/准入/配方/资源；沿四阶段 `IndustrialExecutionContextV1` 仅 `purpose=formal` 可进入本付费路径，`pilot`、`explicit_legacy` 和未知 `production` 字面量不能被默认为正式使用。只加 quote 或某 handler 的实现不能通过。 |
| 不得创造额度 | fee hold 与原材料、电力、设施、目的地和原工业容量同时竞争；预留不是新增资源。正常维护、转移、模块计量、owner操作不能消费许可保护款。 |
| 主体及来源分离 | 使用者、payer、artifact owner、受益人分别变更或撤权，逐项判定；许可不能代签未授权第三方，不能用将到账收益支付本次费用。 |
| 同源合计 | 按现行计量复算：input/output各256KiB、1effect/1emit/new_state时每invoke上界515Data+4E；每逻辑迁移至多2invoke为1030Data+8E，613次操作/案件加128次closure共763230Data+5928E。已激活条款与已funded池另有独立预算。原生产40E、版税4，同一合法payer需5972E及763230Data，任一缺1拒绝。实际30次迁移×2invoke，各1KiB入/出+1effect/1emit/new_state，则总计300Data+240E；加生产与版税消耗/应付284E，释放5688E及762930Data。 |
| 真实费用与程序槽 | 从候选完整调用闭包逐类复算费用，不能用zero-effect planner代替真实制度state/notice/资源变更；最多两次当前有权invoke、受限Kernel apply、actual charge与业务同根。检验Data Compute和Electricity、各artifact owner当前payer、preflight upper_bound的输出/effects/emits/new_state项，有权hold对preflight可用且actual不在hold外双扣。额外invoke或component访问不在预算、unknown fee、缺owner免费分支、原生制度旁路、初审挪复核均拒绝激活。最大m/N按设计复算601个程序写入上界；激活/确认前完整枚举使用主体、净版税payer、签发者与受益人，同一责任主体的别名去重后10人可继续校验、11人须在承诺前拒绝。不同权利主体不能因关联控制被合并；不能在受理时删第11方或让后续转让先增权利再裁减机会。不同技术费owner沿原fee合同处理，不自动成为收益案当事方。 |
| 闭合及池预算 | 最大已接受来源/slot/case/批次和 proof 负载下，原子完成+notice可提交，全部合法结清有费用和写集上界；原root新增来源不能偷偷摊给最后一人。最后一笔先算自己的费用再释放余款。当前兼容drain不足的升级不激活。 |

另测试条款/主体/资金/能力/设施相关 revision 漂移及无关世界事件前进。相关漂移需明确拒绝或新接受，不能静默换价格/范围/来源；无关事件不应作为随意清空旧结果的理由。

在准备、占用/派生索引写入前、权威提交前和提交后丢 ACK 注入故障：确定未提交时全部无效果，已提交只回读一次原结果，unknown 不换 root 重试。费用实际提交与业务同根，不允许生产/版税/费用某一项单独成功。

按最小技术profile验证8个terms、8个root、16个已预留/活动case、4个pool和每pool64来源的上限/+1。预留case槽覆盖未开案的合法窗口及本源未终结closure；到期条款、已释放用途额度但案件/closure移交池的根仍占原保留槽，不能为新root剥夺原申诉。反复到期/换term/完成生产不能清掉旧预留；只有本源通知、程序、费用与closure全部结清才可释放。按16份条款/root各128次加4个pool各640次复算全部closure为4608次、288批，在两个各192批的可服务tick内验证处理上界，不能漏掉条款自身退出。根8次显式换绑用尽后只拒绝新增迁移，128次原closure仍完整。每pool640次closure预算须在首个贡献前真实足额，不能以以后肯定有人付费融资。

证明一root一case的实际前提：本paid adapter只接一次完整RecipeCompleted，[0,B)全部批次同一完成/notice/N。尝试同root先完成一部分、后来再产生新N，v1明确拒绝该adapter路径；不能把新N并入旧案。未来分批候选必须按每份真实新义务预留case/费用/closure，本手册不以旧16槽结果为其放行。制度命令必须有完整instance/schema与最多两次明确invoke闭包，第三次必要调用不能省略，也不能退回任意WorldRule/专属WorldState制度旁路。

<a id="il03-settlement"></a>
## IL03：实际完成、通知、整数分配与个人应付

目标：[计量与结算](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md#license-settlement)。

### A. 成功事实和通知

- 使用真正的正式 root/current admission/native recipe receipt 完成完整批次。生产输出、原 root 完成、fee-unit、pending-earned、可回读 canonical notice 与 N 必须在同一根；缺任一写点的候选不能启用 paid adapter。
- 分别输入 pilot、Shadow、预计产量、被拒/部分验证输出、单独运输到达、商业 escrow 锁款和旧无license绑定receipt，均不能新增版税。对同 root 的生产/交付/运输/产品验证分别重复提交，最多计一次原 batch。
- 篡改 world、精确版本/hash、root/child/revision、purpose、数量范围、实际完成tick、许可binding或payer来源，拒绝整个 effect；不能用 hash 存在代替来源验证。
- 令客户端在完成时断线，到 N+120 之后才首次读取；N 始终是原权威完成提交 tick，不重新开窗、不重复扣款。模拟输出/notice准备失败须原子不产生完成或收费；响应unknown先查原请求。

### B. 待确认、E 与最小保全

在 N+119 查询/提款时 pending-earned 尚不可分配；无案且到 N+120 后才按当前有效事实进入 E。开启使用量/金额争议，只相关batch lot不进入E；开启单一benefit slot归属案，在实际量已确认且窗口结束时可入E，但只该slot待付受保护，其他slot照常。新案/旧案合并、窗口结束、mature和付款两种权威顺序都必须保持唯一账。

已有final支持确认或纠正时按精确范围执行；原费权到期/公开不会没收旧pending-earned或个人应付。不能把“不在E”错误显示为“未发生合法使用”，也不能把“已入E”显示成“所有人已到账”。

### C. 手算与拆分不变

采用冻结slot权重`5000/3000/2000`：

| 操作 | 累计E | 基础应计A | 尾差H | 本次新增应计/最终预期 |
| --- | --- | --- | --- | --- |
| 7批合法完成且程序条件满足 | 7 | 3 / 2 / 1 | 1 | 3 / 2 / 1；尾差不可提款、不可退payer |
| 再4批满足条件 | 11 | 5 / 3 / 2 | 1 | 2 / 1 / 1；每slot不倒退 |
| 费权与全部旧义务/窗口/案件/unknown闭合 | 11 | 5 / 3 / 2 | 0 | 尾差1分给最大余数slot；最终6 / 3 / 2 |

与一次11批以及合法分成11根各1批的累计终值对照；每根仍遵守并发/通知/资源，金额一致。枚举receipt到达顺序、Map顺序、同余数、换request/续期/收款账号，对原冻结slot的分配不变。把每单先四舍五入/最大余数的错误算法作为反例，验证不会通过。E×w使用足够宽的checked中间值，超合法金额拒绝，不饱和、不负值。

### D. 来源、尾差与收款

1. 多payer向同池贡献时，每fee-unit保留原来源；逐来源检查原占用等于unused、pending、payable、tail、paid、returned的互斥总和，池E等于paid+payable+tail。随机重排付费/退款并与普通生产/费用竞争，差额必须为零。
2. 仅硬开放，但仍有旧root、N窗口、使用案、归属案、unknown或晚到既有义务时不能分终值尾差。全部可核验闭合才写一次allocation_closed；重复close或新license不再分一次。未赚取款可返原payer，已赚取H不能退payer。
3. 受益人失联、Agent换owner、旧grant撤销、接收余额溢出、账户无法解析分别验证：权利留原个人slot，保持payable，不转新owner、不退payer、不丢失。合法原主体恢复或明确有权继受后按原义务支付一次；更换收款人不改变稳定slot tie顺序/权重。
4. 模拟实际付款先于争议窗口关闭、用版税净额承担额外交易费、用将到账余额付本次执行费、旧无绑定receipt后来补费，全部拒绝。最终paid只有实际资源入账和准确付款receipt证明。

<a id="il04-lifecycle"></a>
## IL04：到期、公开、撤销与原子替代

目标：[生命周期](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md#license-lifecycle)。本节只验证工业许可对 GR-003 的适用子范围。

| 场景 | 核验动作与结果 |
| --- | --- |
| 半开期限 | 在开始前/开始时、结束前/恰好结束、硬开放/提前开放两侧提交；清理任务延迟不能让过期效果成功。边界前真实成功产生旧fee unit，边界后尚未完成不预赚。 |
| 单次到期与硬开放 | 单次≤600到期但未达硬开放时，旧license不可用且并不自动免费；需新接受的当前term。硬开放后新合法用量零版税，仍需当前准入/配方/设施/安全权限。 |
| 存量工作 | 分别取未确认、confirmed未sink、已sink未完成、完成待notice/case、E/payable和已付；严格按设计处置，不删除资产/历史、不返已消耗材料、不用license创造宽限。 |
| 同root续期 | 当前new paid term合法时，只换绑原剩余未计费batch，同源旧unused占用退出/有权重绑与新占用原子完成，不双冻，不改sink、旧pending/E或已完成账。新payer须同意且足额，旧款只返旧主体。 |
| 同root转公开 | 公共零费条件、当前admission和原work disposition都允许才换绑；旧未赚取款原路释放，后续合法完成零版税。不得新建root再领一次材料或把retired配方复活。 |
| 当前条件拒绝 | 缺新term接受、维护不合格、准入撤销/收缩、recipe restricted/retired、处置不允许收尾、root已终结；均不得因旧license、当前免费或旧成功自动恢复。 |
| 限制/退出类型 | 决定只停新使用、限制后继、受控退出及组合顺序分别验证；缺既有能力disposition时原子拒绝该变更，不能任意停或无限豁免。 |
| 原子替代 | pending新binding不并行使用；提交同时缩小旧重叠范围并生效新范围。对交接前/中/后故障、旧待决/新请求竞态验证每scope最多一个可执行授权；非重叠合法scope独立继续。 |
| PPE边界 | 触发既有FROM artifact admission-freeze；只按其原语义阻止新发布/激活，不能由许可文档改变成所有现存实例自动停机。 |
| 争议与历史 | 开放、到期、撤销后继续恢复旧合法收益和案件；已付历史不返扣，合法未付款权利保留。实际补偿必须另有有权资金，不能改旧receipt。 |

完成与公开、mature与case、支付与收款控制变化各运行两种合法权威顺序。比较相同输入/顺序的恢复结果，不用客户端点击先后来裁定。

<a id="il05-review"></a>
## IL05：专属管辖、无冲突随机程序与硬尾部

目标：[有限争议](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md#license-review)。

### A. 激活前置和公平选择

构造有明确 `industrial-license.review` scope的真实本地授权、固定jurisdiction、政策、金额/对象上限及合格名册。用服务panel权限、普通charter、运维身份、多数票替换均拒绝；名册/随机producer未实现时整条付费路径unavailable。分别测试名册最大/最大+1、6名/5名候选、同controller重复、当事/收益/关联人、原准入和许可批准者、未知冲突证明；不足独立资格不得启动。

固定原案/名册承诺后接受首个合法不可预选随机证明，记录唯一排列。初审3人、复核另3人、2/3精确一致才有结论；换request/撤回重开/nonce、换随机轮次、前端随机、block hash或LLM推荐不能改选。候选失格后顺延；曾参与初审不进入复核。名册不足、抽样迟到或无quorum按原截止继续，不临时任命管理员。

### B. 窗口与机会

以固定 N、共同程序基点C=N+120和各方独立accepted_at建案件；分别把事件放在每个边界前一tick、恰好边界和之后：N+120、C+60、C+120、初审发布+60与C+180的较早边界、C+240、C+300。一方N+1先受理、另一方N+119才登记时，后者仍可并案并从自己的accepted_at补证到共同C+60；C绝不改为先到tick。初审只能在证据关闭后针对全部已受理范围和统一证据root投票；复核集合冻结后才对其程序事实给出一次决定。普通证据截止后不得扩大范围或让先提交者吃掉他人合法机会。

每当事方初审/复核各16不同receipt引用，上限+1拒绝；case内附带的引用也占同阶段额度。同内容重试不收费不占槽，另一方初审和自己的复核仍有保护额度；投票/资格/随机/冻结/结论/closure各槽独立，不能由大量生产或故意重试耗尽。

将新完成的后续义务放在旧案后，保留其自己的原N和程序机会；同一原fee_unit的合法主张统一并入原案，不要求争点或证据相同；不能用旧案关闭剥夺后续不同fee_unit的窗口。使用量案、单slot归属案、全分配依据案分别核验精确保全范围，不得冻结industrial admission、其他root、无争议slot、公共通行或独立资产。

同一原fee_unit更换争点、证据hash、申请者、request或nonce仍只能并入原案；case身份不能只在“争点和证据也完全相同”时才去重。后续不同原fee_unit才有自己的N和程序，不借新license把原单位再开案。

集中提交反例必须实际验证：将最大16案的合法剩余登记/证据放到最后一个可入窗canonical tick，和冻结、初审/复核签署、普通迁移竞争。64是单批写上限，profile承诺同tick最多192个有界批的处理容量；以16案全程序9616加普通192的保守9808次验证最大proof/两次invoke/实例状态/持久化，而非只算2720≤64×60的均值。已在截止前canonical登记的合法证据全入冻结root；仅客户端发出或host排队不能伪装已入窗，晚到不能回填旧tick，也不得自动延长C。实际候选达不到最大集中处理容量则本profile未通过、不得激活；不能把缺测写成足够。closure每批独立16槽，普通调用不能耗尽它。

同场加入高频公开查询、原请求重复、格式错误、身份失败、越权/超预算候选与新可选条款，核Service的独立有界admission/读取池和actor/phase隔离。它们可在canonical受理前被背压，不能消耗已登记合法证据、对方程序或closure保留；原结果恢复仍可读。只在无干扰空载下达到192批不算通过，排队的客户端请求不能被事后伪装成窗口内已登记。

### C. 结论与尾部

| 输入结果 | 必须观察到的行为 |
| --- | --- |
| 正确初审、无合格申诉 | 原程序窗口结束后final，只按准确原范围执行一次 |
| 合格程序申诉、有效复核 | 复核可在授权错误/遗漏合法证据/冲突/越界/计算错误范围内替代，不能重新分配其他人的bps或改价格 |
| 合格申诉但复核无有效替代 | 原仍有效初审到固定复核截止后final；申诉本身不撤销它 |
| 初审有可验证权限/冲突错误 | 不因时间过去变有效；保留真实程序失败并走有权复核/硬尾部 |
| 硬尾部前已有final/同版合法和解/确认债权、尚未付款 | C+300仍保留并执行原权利，不能中性退款清掉责任 |
| 完整canonical证明没有有效改变原条款的结论 | 结束程序保全，原已验证实际收益按原slot基线处理，未赚取款返原payer；如实说明没有实质裁判或全赔 |
| canonical结果unknown/incomplete | 查原request/案，不造“无结论”、不新案/重抽/重计时、不向两方补钱；恢复后沿原时界收敛 |
| 后来发现已合法付款历史另有损失 | 不改旧receipt/强制追索/铸币；未补偿范围可读，新补偿须另有合法资金 |

最后对比party缺席、panel下线、source不可达与主动失权：它们本身不能改价、返已赚款、形成新罚没、延长费权或授予第三方新控制。

<a id="il06-reading"></a>
## IL06：玩家比较、正式读面与可理解性

目标：[统一读面](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md#license-read)。

先做文档走查，随后在有完整实现的同候选正式 Viewer/API/Agent 验证：让代表性使用者在接受前找出精确用途/区域、单次到期与硬开放区别、最大冻结及生产/执行/闭合成本、维护、分配、离线仍推进的异议窗口和可用替代；让创作者找出pending-earned、E、个人payable、尾差和实付差别。实际受试者和结果须记录；作者自评不写为玩家验证通过。

至少覆盖正常有限许可、明确零费/开放、旧scope失效、无安全收尾、只争一个slot、受益Agent换owner、late ACK、查询旧snapshot、未知adapter和无实际名册/随机。三个入口在相同快照下显示等义的状态/原因/next_action/next_recheck；任何入口不能将accepted写成applied、pending-earned写成到账、公开写成无限工业权限。

只读查看不产世界效果；用户对继续/续期、等公开、既有能力替代、当前范围重提、原root处置、按期争议或恢复原付款的选择均需真实可执行条件。没有合法路径时明确指出缺项。权限裁剪后仍能理解自己的责任，不泄露他人无关余额、私有来源、个人证据或控制密钥。

保存实际画面/API响应/Agent解释的来源版本和权限条件；缺正式某入口，明确未验证该入口，不能用设计表格代签UI交付。

<a id="il07-recovery"></a>
## IL07：事务、持久化、旧数据与恢复

目标：[事务与恢复](../../world-runtime/runtime/industrial-license-and-royalty-settlement.design.md#license-recovery)。

1. 在许可登记、use确认/hold、first sink、完成+pending-earned+N、受理/抽样/证据冻结、mature入E、slot付款、替代、pool移交与终值尾差前后分别做checkpoint+非空日志恢复，与不中断的同一权威事件序列比较。应得到相同资产/个人权益/原批次/费用/窗口/随机/receipt和防重放键。
2. 注入prepare失败、权威提交前失败、持久决定已写但内存/ACK丢失、镜像/GC失败。确定未提交才允许无效果拒绝；已有commit只恢复原件；durable generation StatusUnknown须fence整个旧writer，不能仅暂停license同时继续写其他世界状态。
3. 逐项删失或损坏原terms/signatures、lineage源、admission/配方ref、fee/source/purpose、hold索引、N/notice、原使用batch、case名册/随机/证据/final、E/slot/尾差/paid、当前/历史schema或fee artifact。应incomplete并阻断相关新效果，不能补零/空数组/新N来退款或再扣款。
4. 相同request+相同payload返回原结果；相同request异payload拒绝；新request重放已计费batch、已返款source、已付slot或尾差仍至多一次。旧成功回读只证明历史，不让旧license/已撤销grant重新可执行。
5. 恢复时越过原许可、开放、N和case截止：先核已提交事实，再按原时间判定剩余；不按当前价格/recipient重算、不重抽panel、不把离线时间补成生产或延长费权。
6. 激活前旧合法production/receipt保留原免费或原费用语义，不补license/notice/版税；激活后新use缺license binding不得伪装legacy。未知新schema拒绝写入，旧客户端不理解不得显示完成。
7. 升级、停用、回滚与费用/owner变化必须保留当前合法drain和完整reader/个人权利。无法理解已写状态的旧binary只能隔离/验证只读；不删字段、不回旧snapshot重做。末笔closure必须将实际费和余款释放同根提交。

归档后重放旧请求仍能验证原终态、累计量和权益来源。真正无权/unknown与合法原主体恢复分别记录；receipt存在、状态看似正常或零余额都不是已完成全部义务的充分证据。

## 记录结论

本手册是目标验收方法。执行报告逐项给出实际候选、环境、步骤、权威证据、对账和未验证部分；只执行文档链接/结构检查时，结论只能是相应文档检查结果。完整 R&D、其他工业计量、一般扩展退出、生产平衡及未执行正式入口继续按各自原 owner 和验收范围保留，不因本手册存在判完成。
