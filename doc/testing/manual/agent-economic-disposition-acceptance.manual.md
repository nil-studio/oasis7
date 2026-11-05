# Agent 经济转让、处置与责任历史验收手册

本手册核对已存在 Agent 的经济转让、当前控制消费、退休/报废与历史连续性。它是实现后的验证方法，不是运行记录；不存在正式入口、producer、持久恢复或 reader 时，该项为 blocked/not_run。组件构造、源码中的测试和文档检查不能替正式世界结果、实际客户端或玩家验收。

- 固定设计输入：2026-11-05，main `b3658f94a7755fc766fbefe95e83a24566e6068c` 及本次专业补充；实际执行另记录 source/build、world/branch、policy/artifact、入口 profile 与声明支持范围。
- 产品承接：[AUTH001](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-001)、[AUTH002](../../product/agents-world-simulation/agent-authority-ownership-and-accountability.prd.md#req-agent-auth-002)、[AOS005](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-005)。Trace refs: O7-REQ-bd548350-7fce-4026-aeec-5b26a10b86ad、O7-REQ-be254f99-9b66-47b1-ad7f-4a356d296b2a、O7-REQ-42168149-c979-4211-9777-2d72f7050147。
- 专业权威：[Agent 经济转让与处置](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md)、[原授权与转让消费](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011)、[已交付 Agent 首版处置政策](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#agent-disposition-policy)。本手册不另定收费、退款、资产去向、资格或主体规则。
- 主责：runtime_engineer / agent_engineer / gameplay_designer / viewer_engineer / qa_engineer。测试只使用本案受权临时资源，不销毁、报废或接管他人的 Agent、存储或会话。

## 共同准备与独立判定

固定一个真实存在的 Agent A、转让人 O1、接收人 O2、原 claim 的 claimer Agent、当前经济 owner/source 与各自 generation；player/session、claimer Agent ID、组织代表和 executor 不能互相代替。另准备其他主体 O3、另一 Agent B、不同 world/branch、已到期/撤销的授权与旧签名。按原正式路径取得候选，不用 registry 首次绑定、release 后重新 claim、人工改账或 WIP Scrap 冒充已经交付 Agent 的转让/处置。

从提交前的已确认原件独立列出：双方签署的完整条款/quote、五门槛来源、实际 liquid/受限来源与 holds、原押金与受益权人、应付维护、库存/模块/在途工作、原 D/O/C 与未结义务、配置 provenance、已提交结果和当前私有受众。预期差分按签署条款和原领域算法计算，不从被测 summary、两端相同文字或当前余额差倒推正确。记录准确调用图、费用 payer/grant、完整编码与预算；原身份和原 receipt 是关联，不能新增另一份“已完成”账来裁决世界效果。

每例分开报告：正式请求可达、已接受但未生效、经济/控制权的原子生效、原领域履行/结算、历史 reader 可消费及真实平台结果。涉及已合入的一般形成，复用 [CF09–CF17](agent-mediated-contract-formation-acceptance.manual.md#cf-09-organization) 的适用方法；本手册只核本处置实际消费，不能把某一合同族、native lane 或组件样例的通过推广到所有入口。

<a id="agent-disposition-authority"></a>
## AD01：真实 authority、双方接受与已采纳 profile

设计依据：[转让 authority 与形成](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-authority-formation)、[已采纳授权 profile](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#agent-authority-adopted-profiles)。

1. 为 O1/O2 固定各自当前 owner/source、真实签名主体、完整条款及原接受身份；组织参与时使用真实 charter/代表任期、当前 mandate 和实际资金权利人的许可。分别移除一方接受、改条款后保留旧签名、使用另一 world/Agent/source/generation 或仅有组织名称/职位，均不得产生经济或控制转让。
2. 原受保护 owner 控制与普通 executor 路径分开：executor、模型输出、WASM capability、旧 receipt、registry 行或一次已通过的签名不授予出售/报废权；原组织 mandate 的来源与当前范围继续逐项核对。引用 CF09 的组织来源负例，但另核本次真实处置 consumer，不能仅因 CF09 通过就认定它已接通。正式调用另核实际 CapabilityAuthorityRecord/FinalityProof 和精确 GrantV2；分别换 issuer/历史 proof、module/version、subject/owner generation、audience 或 selectors，应在经济效果前拒绝，不借 parent attenuation 把卖方 grant 改给买方。
3. native owner-delegation 的 Local Viewer、Signed Runtime、底层 grant storage 与 chain-linked local mutation 按原 profile 分别成案，不取 action 并集。沿既有 signed vectors 核 install/revoke 的 Applied/ExactReplay、同 ID 冲突、旧 nonce 只读匹配、当前 owner/source 和拒绝顺序；被撤销 install 不复活，读取不增加 usage 或业务效果。当前支持的 action 不等于已有 Agent 转让/退休入口。
4. 一般形成的 mandate 沿原 grant/source/object/period 和 D/O/C 执行。复用 CF11 的独立算例：上限 10，甲 O=4/C=2，乙再需 O=4/C=1 时总额 11 拒绝；甲真实支付 3、费用 1 后 D=4/O=1/C=1，总额 6，真实退款 3 也不抹 D。转让、换代表/Agent/nonce、重连或新 UI 不能清空原记录，native Electricity 与形成 Data/原权利单位不得互换。
5. 在接受后、效果前改变 owner/source、mandate revision、期限或原资金授权，要求在本次提交点重验。原已合法生效的合同与窄资金处置权按 CF13 保留，只限制未来新风险；不能用出售 A 自动撤销他人的既有权利，也不能用旧义务反向增加 O2 新业务授权。

本次 `agent_disposition_v1` 是独立新增候选 family；正常组织样本须有按原完整修订程序批准的签约附件及 mandate，明确包含本 family、动作/对象/产权 source、期间和累计范围，再取得本次完整接受。独立负例分别保留旧即时交换/服务签名、只改 family 标签、使用 wildcard 或缺一项新增范围批准；均不能授权处置。未正式采纳/接线时如实不可用，不借旧已上线 family 执行；原 CF 方法仍拥有其一般形成规则。

证据保留原签署条款、实际 signer/issuer/代表/资金 source、profile、当前拒绝或唯一结果，以及原累计账前后值。私有 proof 留受保护记录；验签成功、accepted 或普通聊天不是经济/控制生效证据。

<a id="agent-disposition-transfer"></a>
## AD02：经济转让的五门槛与同根生效

设计依据：[转让门槛](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-transfer-gates)。

固定一个全部条件可满足的合法转让，再一次只改变资格、容量、资源、治理、反滥用中的一类。每类覆盖当前失败、过期与尚待确认来源；结果应保留真实门槛、blocker 和可行下一步，不能统一显示成功、未知归零或让另四类通过补足。前态应包括旧 owner/source、双方资金与库存、旧控制及原未结义务；业务效果尚未提交时这些量不能部分转到 O2。

分别在 quote 后、双方接受后、候选准备后及效果提交前改变一项来源。旧 quote/接受不固定未来资格；当前重验失败须完整拒绝或保留原合同允许的待决结果，不自动换价、换买方、改签名或忽略新增义务。并发 A→O2 与 A→O3、同目标容量的另一取得、原资金的竞争支出，按两种权威顺序分别核对：只能有完整合法的胜者，失败者不能消费部分权利、容量或新 owner generation。

报价、预览与历史读取无转让/处置业务效果，允许其原鉴权及有限协议元数据。正式同根失败按原事务保留完整前态；前序独立操作中已经合法产生的费用/义务仍按其原结果单列，不能把它们删除后伪装从未发生。

在同一未变 economic generation、空 plan occupancy 下并发两个不同 lineage 的 Begin，独立期望是原子比较 occupancy/guard revision 后仅一个 `empty→active`，不能因 Begin 不推进经济 generation 而生成两个 guard。同一 Begin 的原件重读/精确重复不二次消费完整接受 nonce 或收费；Final 引用原 plan 和已消费的接受并重验当前源，不能因 nonce 已在 Begin 消费而误拒合法终结，也不能把它消费第二次。旧 guard revision、另一 plan、同键异 digest 和并发 Final/Abort 分别验证唯一终结 ordinal，不复用已关闭 plan。

**正常资金截面，按原单位独立复算。** 本例消费本次 `agent-disposition-v1` 新政策，不声称现有native Release已经支持bond抵欠。固定原真实 claim、双方准确资金账户和已签完整清单；接收方已有一个计入同一取得容量的 Agent，当前合法 slot-2 quote 为 activation150/bond300/upkeep38。旧 bond 的 provenance 为 restricted150/liquid50，当前 c=12 有真实未结旧 upkeep25，无其他处罚且 restricted refund sink 当前合法；原受益账户 restricted10/liquid80。接收方仅以自有已获授权 liquid800 支付 price60 与新取得费用；本次不消费restricted，不触发自动首次补足或新增/重领首聊支持。历史已合法进入普通liquid的资金仍按原可转让语义可用，不能另造来源禁用标记。

| 项目 | 独立计算与唯一期望 |
| --- | --- |
| 旧 bond 与欠费 | 先按 restricted-first 消耗25，剩余退款 restricted125/liquid50；原 bond200清算一次，欠费25入原准确 sink，不附加主动处置罚款。 |
| 接收方 liquid 与新 bond | `800−60−150−300−38=252`；新 bond300属于新方原资金来源，不把旧bond或预计退款拿来补其付款缺口。 |
| 原持有人原受益账户 | liquid=`80+60+50=190`，restricted=`10+125=135`；价款与原路退款分别有来源，不能混为可再次领取的补贴。 |
| activation 与维护 | activation150按原5000bps分为 burn75/treasury75；旧upkeep25与新upkeep38分别入原sink，Data/Electricity/capability另账。 |
| 全集守恒 | 初态 `800+80+10+150+50=1090`；终态 `252+190+135+300+25+38+75+75=1090`。burn是明确sink，不能遗漏或另铸余额；原责任之外的其他账户不受此截面擅改。 |

原受限来源应回 treasury 的变体，125进入 `refund_sink` 指定的准确原池，原受益账户 restricted仍10，其余价格/取得费用不变。另测零价合法赠与：新方liquid312、原方liquid130，取得费用及资格不减免；新方liquid547即使另有大量restricted也不足548而拒绝。以曾合法取得首聊赠款、现为普通liquid的账户做正常样例，付款权限不因这段历史失效；转让、换主、退休、报废和重复请求均不得刷新原once标记或增发赠款。原bond不足旧欠费时，必须有原责任人真实合法补足或原债权合同允许的完整处置，不让未同意的O2继承缺口；未解析退款目的地不能当作已退款。

**epoch、等待与 quote 交错。** 原精确处置的权威接受epoch r=9，最早在12进入最终校验；11仍不交割，12也只有全部当前门槛及签定有效窗仍满足才可生效。改对象/买方/scope、变造请求或重复Begin不能借另一lineage的等待或缩短冷却。上述c12欠费结清后e12，新方仅预付第13期，下一应付14；另构造真实已付至e14的原记录，在c12转让应仅新付15、下次16，不退回或重复收费12–14。若quote后原upkeep writer已结清25，最终不能再扣bond25；以同一当前parent的真实c/paidThrough为已签typed公式输入，在相同policy/source/rate及逐项金额上界内重算欠费、退款和未覆盖期，不重开冷却、不二次消费接受。另以改价、换rate/source/policy、超公式或金额上界为独立负例，必须取得准确新修订/接受或拒绝，不能只因总额低于较宽cap继续。缺原支付证明、e+1/e+2溢出、到期或原forced reclaim先合法终结时，不推定可继续交割。

原授权窗为canonical `[not_before, expires_at)`；用候选真实epoch长度求第12期起始tick `t12`。若最早最终校验时点恰为`expires_at=t12`，该请求没有可用交割时点，Begin即应拒绝该不可达窗口，不能因冷却刚满而忽略到期；有效窗包含t12的独立正例才继续五门槛校验。不从客户端时钟或整数epoch显示值猜tick单位。

<a id="agent-disposition-obligations"></a>
## AD03：库存、债务、来源与旧义务连续性

设计依据：[经济与库存连续性](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-economic-inventory-continuity)、[所有 writer](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-all-writers)。

- 分别准备 liquid、restricted support、vesting/held-in-trust、原押金、已预留原材料、cargo/ModuleItem、已交付与在途工作；按其真实 owner、purpose、单位和原 obligation 列账。合法转让只改变完整条款及原政策允许的归属，不能把受限余额洗成可转 liquid、免费扩容、清空 cargo、复制模块或重新领取首次支持。
- 未结债务、赔付、费用和历史 D 不由 A 的新 owner 标签强加 O2，也不因 O1 退出消失。原领域允许显式继受时复用 CF13 的完整必需接受、旧新范围与责任保留；缺一资金权利人、旧单位未知、合法保全或不可分旧工作时拒绝受影响处分，不能先转后让后台补同意。
- 原已生效工作与新代理行为分开。停止旧 executor 不自动取消旧生产/履约；受原窄授权允许的 settlement、赔付、费用、合法退款与 return_pending 仍由真实领域 producer 在原身份下处理。无法解析原受益账户时保留 hold/return_pending，不转给当前新 owner 或伪退款。
- 从各适用原领域 writer 逐条刺激支付、材料/电力 sink、旧作业结算、费用和退回。比较 source、hold、D/O/C、receipt 和资产归属；同原 effect 重放无第二次扣款/释放/分配。未适配 writer 或缺完整来源闭包时本范围保持不可写/未知，不以抽样一个绿色 writer 宣称全覆盖。

<a id="agent-disposition-owner"></a>
## AD04：生效 owner、控制代际与待决行动

设计依据：[有效 owner 与 generation](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-effective-owner-generation)、[控制及历史 consumer](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#agent-disposition-control-history-consumer)。

预先固定原控制切点 C1、O1/source/generation、Task/goal/executor fence，以及一个已提交世界结果 P1 和一个只有 accepted/submitted、尚无领域效果的 P2。中间 Begin 切点 B1 只提交 canonical durable disposition-plan admission guard；owner/identity generation、task_revision、executor_epoch、context 与 Prompt V 均不变，不提前标成 O2或伪造 task Suspended。分别驱动所有真实 writer，在 effect-time 验证本件 guard 阻止本 scope 新风险，原获权窄 drain 与原请求查询继续。合法 FinalizeTransfer 在 C2 生效后，才按原字段核新经济 owner、身份/控制来源、task/executor fence 及 Prompt 中性版本；不要求不同领域 generation 数字相等，也不能只改可见 owner 字符串便认为接收完整。Abort 只去本件 guard，不自动重发 pending、dispatch 或调用 Resume；仍有效 context 可显式依原规则继续，另已失效者重新取得，不复活其他撤权/到期或改写历史。

| 输入与交错 | 独立预期 |
| --- | --- |
| P1 先提交，再转让 | P1 的原授权、责任、receipt 与已生效 delta 保留；新 owner、Stop 或转让不将它显示为未发生，不把它改归 O2 的新策略。 |
| P2 先 accepted，C2 后才尝试效果 | 旧接受不固定执行资格；在后续效果前用 C2 当前 owner/grant/source 与完整前置重评。只能按原合同继续、拒绝/过期/取消，或经当前有权主体明确新确认产生新 request/cause；不能复制 O1 签名给 O2。 |
| 旧 token、旧 executor、旧 context 或缓存晚到 | 旧范围的新写入拒绝；原 P1/P2 仍按当前读权查原件。禁止重签新 nonce、换 Agent、重启 worker 或换客户端绕过当前 fence。 |
| O2 新配置或新委托 | 沿原受保护 owner 路径明确提交，从真实生效点使用新目标/策略；经济取得不自动给自治、组织角色、module capability、provider 访问或旧 executor 本机权限。 |
| 先撤权/到期/出现硬边界，再尝试转让或旧请求 | 当前重验不得被 quote/accepted/旧历史掩盖；拒绝及原未结关联保留。源恢复不自动执行旧请求。 |

核对每个仍待执行意图的原 identity、最近完成重评 revision 与实际效果提交次序；不通过扫描 UI 队列或给全部 pending 标“已取消”替代原 producer 重验。旧已发生效果与当前新增效果分别计数。

<a id="agent-disposition-history"></a>
## AD05：不可变配置、责任历史与三端当前读取

设计依据：[当前读模型](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-read-model)、[控制及历史 consumer](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#agent-disposition-control-history-consumer)。

用 AD04 的 C1/P1 与 C2，保留旧配置的准确 version/provenance、原创建来源和适用建议、owner 确认/override、组织指令/受益、执行载体及结果关联。转让 C2 按原消费合同追加中性 Prompt 版本 V+1，三个私有 override 均为 None，cause 归原处置，不冒充 O2 已选择新策略；旧 V、decision 和责任不变。O2 再合法提交 C3 新目标/角色策略/Prompt/work context，后续版本从 C3 起生效，不重置为 fresh0 或覆盖旧 snapshot。分别使原 V 无法核验、版本溢出，或 O2 rollback 指向无读/重用权的旧私有 V：拒绝相应写入，不能靠当前 owner 身份取得旧正文。

分别以 O1、O2、原资金权利人/有权组织代表、离任代表和旁观者读取。O1 只保留当前仍合法的自身历史范围且没有新写权；O2 取得的经济/控制权限不自动开放旧私有 Prompt、完整记忆、密钥、模型账号、他人余额或不相关历史。失去读权时清旧私有投影与晚响应；只持旧 Agent/receipt ID 不授读权，公开可审计责任信息也不等于开放私有正文。

在实际 Launcher、Viewer、API 消费者按同一 candidate、原结果身份、可核 root/as-of 和各自当前 audience 对比：稳定 Agent、原来源、处置/控制生效点、旧新配置、已提交结果、未结义务、准确 blocker 和可行下一步一致。不同身份可见范围不同、布局不同不算冲突；不能把无权缺项填零，也不能因一个 API 有记录就签另外两端已可用。Launcher 只展示入口或进程诊断而未接历史 reader 时如实记录该能力未执行，不新造按钮或后门。

对 Prompt sidecar 另做掉进程/恢复检查：RuntimeInstance 的 ack、`persistence/sync=None` 或可解码旧本地文件不等于 durable 历史。只有原专业来源的持久记录、版本与根关联闭合后才发布相应当前读面；reader/sidecar 落后不回滚经济结果，不以重新应用 P1 或重新调用模型补历史。责任的有权摘要与私有正文按原合同分别验证。

<a id="agent-disposition-retirement"></a>
## AD06：退休、报废与真实资源结果

设计依据：[退休与报废](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-retire-scrap)。

仅对已存在的受权 Agent 执行原政策允许的处置；未交付制造 WIP、release/reclaim、离线或失去控制不自动等于退休/报废。分别核当前 owner 的明确确认、原资格/义务/库存、合法实际 producer、状态生效点和所需完整写集，不能以界面隐藏 Agent 代替领域终态。

准备有未结工作、债务、受限来源/hold、不可安全返还账户以及仍活动的 executor 的候选。原义务和已确认成果不消失；未满足原退出/清理条件时给准确 blocker、保留对应记录，不先删除身份或把资源转给任意新 owner。退休/报废的投入、押金、维护、资产去向及允许恢复分支按原政策分别计算；不把未列出的回收价值、免费修复或再次首领支持当默认收益。

1. **Begin与等待。** 同AD02精确lineage的r=9→最早12边界，确认Begin后原取得容量、bond及真实维护仍保留，只停止本件禁止的新风险；原已承诺的有限履约/保全/结算及有权查询继续。Begin不可标成已退休/报废，不立即退押金；没有新增运行许可也不冻结欠费、闲置或forced reclaim的原时间。guard仅阻本案Agent/受影响scope的新风险，不能撤销O1对其他未售Agent/资产的权利。
2. **合法退休正例。** 准备当前原件齐备、原领域drain已合法完成、费用/未结责任已支付或有合法保留/承接的A。先核未终态时容量仍占；FinalizeRetire成功才退出active acquisition、停止新运行。保留estate、未售库存、holds、合同责任、稳定Agent ID及配置/结果历史。维护按真实paidThrough与最终c结清，bond退款沿AD02同源算法，不能按Begin时冻结金额或把保管/旧责任写成零。
3. **安全报废正例与负例。** 先从完整活资产清单证明自有机体之外的cash/cargo/ModuleItem、借用物和holds已合法保留/返还/移交，且每项有真实获权账本目的地与同根结果；空清单也须原来源证明。随后对被明确授权的自有机体完成Scrap，运行能力不可用、ID留不可复用tombstone。没有真实已采纳回收recipe，材料补偿为0；这不允许把其他资产清零。缺目的地、漏一件借用物、原件未知或在途工作不可分时阻断相应处置。
4. **不复活、不洗责任。** 对退休/报废结果尝试普通Claim、制造交付alias、旧snapshot恢复、重复Begin/Finalize及改owner标签；均不能重新active、重发primary/首聊支持或清原责任。当前版没有对应恢复入口就准确显示无此动作，不造可执行按钮。原强制回收条件若先成立，按其原规则和唯一终态处理；主动处置本身不额外套2000bps罚款，也不能免除真实强制事件。
5. **关闭也须当前调用权。** Begin前固定原合法closing principal/caller及独立、精确的current close-only Grant。正常执行grant已撤/到期而close grant仍有效时，可用精确plan、原sink和已保护预算完成一次合法Abort/Expire，不能借关闭权Finalize或新增处分。再分别于准备前、guest前及最终提交前使close grant本身撤销/到期或identity漂移，真实admission应拒绝并保留guard/hold；业务窄关闭权、费用hold和plan到期都不代当前技术调用权。沿原issuer权限恢复准确close源、先核原决定后才可关闭，不伪造自动有权Expire、免费退出或另一计划。

分别比较Begin、有限drain、Abort/Finalize的实际原结果和资源差分；它们不是一次跨时段的假原子提交。正常终结所需费用与旧领域闭合机会须有完整合法来源；用户明确取消或失败恢复不能抹已提交Begin/旧履行费用。Abort只解除本件guard且不自动继续；当前context是否可显式使用沿AD04，其他失权不因退出消失。

<a id="agent-disposition-recovery"></a>
## AD07：原件、G1 三态、重连与非空后缀恢复

设计依据：[原事务恢复](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-transaction-recovery)；复用 CF15 的原结果与根未知方法，不创建第二套提交状态。

保存真实 original request/payload、双方接受、quote、原 generation/root 与完整 staged 候选，包括经济/控制、plan guard、资产/义务、预算/费用、nonce、原结果及历史引用。在 Begin 与 Finalize/Abort 两个真实阶段分别，于准备前后、全部准备完成但持久决定前、持久决定未知、已提交回复前、reader/投影更新前注入本案故障；每阶段 base 都是它自己的 parent。最终阶段 NotCommitted 保留已经提交的 B1、其停止新风险 guard 及先前真实费用/责任，不能退回整个处置开始前的 C1。只丢客户端 ACK 与真实 G1 状态未知分开构造。

关闭重试同时接收AD06的两grant反例：普通执行grant失效但当前close-only源有效，与close源本身失效分别留证；任何新grant/恢复决定都不证明原终结未发生，须先以原identity查询。恢复只能重验同plan的合法关闭来源及唯一terminal ordinal，不因预算仍在、旧snapshot可读或重放曾经有效的grant而准入。

| 原权威判定 | 必须满足的恢复判据 |
| --- | --- |
| NotCommitted | 完整原世界/控制/资产/义务/费用/usage/nonce 前态保留；候选未生效，不能发布局部新 owner、退款、注销或历史终态。仍须保留原操作关联，再按原合同和当前资格决定下一步。 |
| Committed | 本阶段规定的guard或经济/控制写集及原receipt恰好一次，不给Begin伪造最终交割；回复丢失只查询原结果。reader/sidecar落后时暂停依赖其不完整状态的新动作，追平到原提交，不回滚领域结果、重复付款或用旧本地generation继续。 |
| StatusUnknown | 保留完整 base/staged/original 身份；fence 整个旧 World/writer，包括 tick、save 和依赖派发，恢复原候选决定。不能只冻结 A、另开事务、重建空状态或换请求避开。 |

普通客户端断连/丢 ACK 不自动把整个 World 判成 G1 unknown；有权查询原请求并读取明确结果，pending/不可查询/超时不等于未执行。失去读权时不借 O2 或 owner signer 自动接管原查询，保留受限未知和合法下一步；查询可以消费原认证/有限协议 metadata，但无新的处置业务效果。

构造 checkpoint C0 后有真实非空 canonical 后缀：先有原工作/费用/配置结果，再有转让或处置生效，最后有当前允许的后续记录。恢复从 C0 加原 suffix 得到独立预期的相同 root、控制与历史关联；后态 snapshot 单独恢复不代签 suffix。原 delta apply 可重建余额/权利/nonce，但不重新执行实时外部调用、模型、收费或资产发放。

同 request 同内容重试、同 ID 异内容、换 ID 的同一业务处分、key/generation 轮换、重连和版本变化逐项核原去重与当前权限。旧原件/代码版本不兼容、缺持久配置来源或必需历史只读证明时 fail closed/明确未知；不把 migration/replay 变成新授权或删除原结果。成功只证明实际运行的候选与恢复边界，不能外推跨节点/跨版本已支持。

<a id="agent-disposition-bounds"></a>
## AD08：完整来源、编码、费用与所有消费者预算

设计依据：[有界闭包](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-bounds)、[所有 writer](../../world-runtime/runtime/agent-economic-transfer-and-disposition.design.md#ad-all-writers)。

列齐同次处分必需主体/签名、资产/义务/控制条目和实际 producer/adapters，而非只测成功主调用。正常边界、超一项、缺一个 source、重复/冲突原件及超完整字节上限分别核对；来源和费用图不全时拒绝本范围新写，不能截断接受集合、跳过旧义务、用未知作零或分多次发布来冒充一次原子结果。

下面逐行对应原 W01–W12，使用每条实际正式writer及必要内部timer；组件构造只能证明其覆盖的分项。原正常结果和每行单变量反例分别留证，不能只测试UI禁用按钮。

| 原catalog | 本处置的独立检查 |
| --- | --- |
| W01 注册/primary/身份 | A作为原root actor且另有未售B、钱包或组织来源时，完整列出依赖；Final只交付已签A，O1未售权利仍有真实来源，O2不能凭新`current_owner(A)`接管B。缺保留来源适配则阻交割，不生成补发primary或重开once。 |
| W02 claim/upkeep/release/reclaim | 将旧upkeep、release、forced reclaim与本处置按两种提交顺序交错；相同claim/source只能有准确赢家，旧请求不能删新claim、向新方重复计旧费或重复退bond。AD02的paidThrough先更新变体必须由真实writer产生。 |
| W03 制造/取得容量 | 新方同时制造/claim/受让时只消费同一去重容量；先合法占用者保留，另一完整请求按当前条件拒绝。旧Deliver不能对A再注册，WIP Dispose不能安装本Agent的退休终态。 |
| W04 delegation/task/执行许可 | Begin guard在每个实际effect入口阻其scope内新风险，不能靠旧permit越过；Final后旧owner/source/task/executor按各原版本拒绝新写。原窄drain、未知原请求回查和其它不受影响Agent不被扩大阻断。 |
| W05 Prompt/memory/provider/outbox | 旧Prompt/记忆版本和原输出归属不变；late provider输出和旧outbox不能改签给新owner。AD05中性版本/私有历史与当前读权分别核，不从sidecar ACK推durable。 |
| W06 原action/tick/Body | 同一A在转让后不重建frame/模块、不填满battery、不重置heat/cooldown/补能水位；退休终态后无未授权新运行。内部tick与真实Move/BodyAction也核当前lifecycle和guard，不只外层HTTP受控。 |
| W07 token/资源/restricted/Data | 保留hold和受托量对新owner一般消费不可用；合法旧债入账/返还只走准确原source/purpose。分别测restricted退款原受益人与原池，不把普通合法liquid历史另变受限。 |
| W08 cargo/ModuleItem/artifact/install | 机体、模块实体、artifact产权/许可和私有状态逐项分开。只转已签合法部分，不生成默认附件、免费slot或新许可；旧安装/升级原费用payer与新owner不可按Agent标签混同。 |
| W09 合同/服务/R&D/组织estate | 原D、已确认贡献、未付责任和程序时界保留；显式可分继受按CF13/17完整接受。旧真实结果仍归原单位/epoch，不因处置触发第二份声誉或成果。 |
| W10 工业/物流/供能/levy | 在途slot/work/custody保留真实承诺与受益人；变carrier/control需要原完整交接，旧货物不能改送新owner。许可/地域资格不因随卖设施或Agent自动授予。 |
| W11 dispatcher/计费/capability | 所有嵌套invoke、artifact payer、原cap reserve/settle和最后终结ordinal纳入真实图；缺owner资金或grant时不能走ownerless免费分支，旧budget不因新generation恢复满额。 |
| W12 journal/restore/reader | AD07的完整base/staged/非空tail及当前读权逐项验证；重放不重新运行guest/provider或付钱。ordinary Committed、可解码snapshot与正式authority/serving分别判定。 |

任一必需writer未提供当前guard、源索引或同根适配时，本受影响处置不能整体通过；不存在该类关系也须原域完整索引/absence证明，不能把本地空列表作N/A。

计量分别保留实际完整 ABI input/output、模块 effects/emits、原 payer 的 Data/Electricity，以及适用 trusted grant 的独立 capability reserve/spent；raw payload、内部命令、外层 Vec 编码不能互当完整 I/O。只有真实适用该 capability 账的调用才计入，原 native/Pure Domain 不虚造此账。预检上限/预留、实际费用与释放余量分别比较，不把预留当实际消费，不把退款或重启当预算重置；下层 host/world/command/transport 任一更严上限都必须生效。

按本次有限 profile 单独构造一个双个人、完整嵌入证明的合法小样本，再验证计数与字节上限的交集：一次 1 Agent、最多 32 必需 principal、组织各最多 16 签名且合计最多 512、资产/义务/控制证明条目总数最多 256，原件读取128KiB以内、至多16页/每页16条，实际验证与原索引读取分别至多512/256。512份签名字节本身即可占满32KiB，不声称所有计数上限能同时装入合法请求。引用已验证原件时仍须同root、完整current proof/source且计入实际校验、索引和调用图；缺原件、不可解引用摘要、未验证hash或借用另一root均不能放行。覆盖最后一页、缺页、128KiB+1、257条及新增未知writer类别，不截取前256项冒完整。

本次依赖的私有准备另按真实受控存储核最多8个不可变对象、每个完整编码≤256KiB且总量≤2MiB；当前投影完整响应≤16KiB，历史页同root最多16条且完整响应≤32KiB。分别构造第9对象、单对象256KiB+1、总量超限、不可耐久/不可解引用旧版本、缺当前存储权限/配额或漏实际收费 source；均阻塞相应处置/读取，不丢历史或生成新模型正文补证。有实际 billable prep invoke 时占完整63个adapter预算；无invoke的读取、加密/耐久准备也须核原有限存储/服务预算。已存历史复用其受保护完整索引，不因此扫描全部正文；分页边界与权限漂移仍沿AD05。

**ABI 与费用的独立算例。** 对 `event=None` 的新 producer，原 payload/state 各 32 KiB，内部命令完整上界 65 KiB+5；经实际 `Vec<u8>` 外层编码后连同完整 state、ctx 及所有 framing 的 16 KiB 预算，输入上界为 210 KiB+20。输出 new_state 的原 `Vec<u8>` 数组封装、完整 effects 64 KiB、emits 16 KiB 与其余 framing 16 KiB 合为 160 KiB+5，均小于完整 256 KiB 上限。用真实 codec 复算引号、整数/数组头、option/tag 与空 event；存在 event 或实际封套不同则重算整个 I/O，不能套用本上界或裁剪后通过。

原 manifest 上界为完整 I/O 各 256 KiB、32 effects、16 emits。单次世界保守预检为 Data `256+256+2×32+16=592`、Electricity `2+32+16=50`；新 producer 的 trusted capability reserve 为 `32+256+2×32+16=368`，其他实际适用 trusted 调用的 payload≤64 KiB 时上界为 400。按完整实际调用图逐节点计费；超 effects/emits、缺合法 payer/grant、任一下层上限不足或金额计算溢出，都不能产生部分处置。

最终交割/退休/报废/Abort的单root最多64次真实调用：1个新producer与最多63个适用原adapter；最满合法图保守为 `64×592=37888 Data`、`64×50=3200 Electricity`，capability上界 `368+63×400=25568`。三类处置均须把真实Begin的1次producer计入，Begin与一个合法终结root的新增总上界为65次、38480 Data/3250 Electricity/capability25936。旧领域drain、先前独立接受/准备或额外读取若实际发生，仍按其原调用/预算另外完整计入，不能把65当成包括所有既有业务的免费总图。缺准备/退出费用、超64节点或把两个root拼成一次atomic成功均失败。

Begin的单调用正例使用已存在的canonical产权/组织附件/mandate/proof、准确closing principal/caller与独立close-only Grant及原生通用验签/权限检查；若必须在Begin内再调用source或制度producer补授权，该有限profile应明确拒绝。须先独立完成的原业务授权保留其真实费用/责任，再形成准确处置；不能漏算、改称host免费核验或延后归入Final。完整准备图还须证明一次终结的费用与机会被独立保留，普通请求/查询不能花掉；费用足额与AD06的当前技术调用权分别核验，撤权/过期不能靠已预留预算绕过pre-guest或commit前检查。

使用实际合法编码可确认的轻形 `F=2 KiB、I=8 KiB、O=2 KiB、1 effect、1 emit、new_state`，独立期望世界预检 Data344/Electricity50、trusted reserve338，实际 Data13/Electricity4、trusted spent7。准备各自真实账户：世界 Data400/Electricity100，capability remaining400/reserved0/spent0；成功后应分别为 Data387/Electricity96 与 capability remaining393/reserved0/spent7。令 Data343 或 Electricity49，或 cap remaining337（另两账足额），分别验证入场拒绝；不能因为预计 actual 较小而绕过各账保守准入，也不能将三种单位互相抵扣。数值是独立计划算例，执行时必须提供真实 codec、grant、调用记录及计量，不能以手算充运行通过。

对查询/预览、等待原结果、Stop/退出、原领域 drain 与正式 terminal 分别记录真实调用和成本来源；有限退出机会沿原合同保留，不能因正常请求排满而无限阻塞，也不能将无限扫描/无限重试伪装成一个便宜 API。证据只保存有界脱敏引用、计量、差分和结论，不公布可重放签名、私有正文、密钥或完整模型上下文。

原协议资源上限另验：每稳定认证主体普通在途4加独立控制/原件1槽、每60秒单调服务窗普通128/原件16、未关闭proposal16、每Agent至多1生效未终结计划；分别在边界与超1时检查准确拒绝/合并、有限重试及退出槽可达。轮换key或换客户端不复制主体额度；原服务更严时取严。重复已有决定不重新跑guest或收业务费，已确认未提交的候选费用不落账；这不授权无限免费计算。真实manifest的有限memory/fuel、并发和新读缓冲按实际来源验证，不用静态算例宣称性能通过。
