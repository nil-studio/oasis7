# OC 转让与世界权利双向隔离验收手册

- 生命周期：`active`；主责：QA；规则分别由原链上转账、世界业务及其治理 authority 拥有。
- 配对设计：[OC 转让与世界权利隔离](../../world-runtime/runtime/oc-transfer-and-world-rights-isolation.design.md)。
- 上游：[REQ-WR-GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-011)、[AC-WR-GR-011](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-011)及 [AC-GAME-018-03](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-03)。

Trace refs: O7-REQ-d27a209c-a703-4443-92d9-acadb9820ed2。

本手册验证 OC 转让和原世界权利各自成立的条件、费用、结果及恢复，不定义新资产、兑换、资格或统一提交接口。八组方法适用于已采用的全部消费者；原业务合法的显式付款、同根原子交换、事项锁定及单向服务额度桥分别按其原合同验证。方法存在不表示正式 producer、入口或运行已经通过。

`test_tier_required` 使用实际候选的原解析器、签名校验、evaluator、原子写点及确定性 replay；`test_tier_full` 再验证正式入口、真实持久恢复、适用独立节点和玩家理解，按[测试规范](../prd.md)选环境。直接 setter、注入资格或仅返回成功的 helper 只可作为明确的组件反例，不证明合法来源或正式正向路径。

<a id="oc-isolation-01-source"></a>
## OI-01：固定来源、原操作和独立账本预期

依据：[来源与状态类型](../../world-runtime/runtime/oc-transfer-and-world-rights-isolation.design.md#oc-isolation-source-types)。在实际实现 PR 中固定 source/tested tree、World/branch、chain/network、原 manifest/policy/schema/artifact 和受检时间窗口；没有采用的未来 fee 模型不填为当前能力。每个场景保留原 OC 操作 O 和原世界操作 W 的身份、完整请求摘要及各自结果来源，不能生成一个新的“隔离事务”替代它们。

执行前从原规则和合法前态独立列出预期，不以被测 UI、审计报表或运行后差分反推允许写集：

| 对照向量 | 必须保留的类型、范围与来源 |
| --- | --- |
| OC | 同 chain/World 的账户、liquid、restricted starter、vesting/已解锁部分、适用 lock/escrow/财库、总供给、原 sender nonce、原操作及结果；不同持有类别不能相加后当作可转账余额。 |
| 世界资源与责任 | Electricity/Data、物料及库存/custody、费用/hold、原工作/合同/履行单位、个人权益和责任；沿原 typed ledger，不按字段名或数值相同换成 OC。 |
| 世界权利 | 原主体/控制人/任期、工业作者与准入、许可、Agent claim/制造、charter/地方资格、治理事项 snapshot、tenure、levy、竞争/保供及其当前来源/期限。 |
| 结果与观察 | 签名/提交接受、链上包含、World 拒绝或实际效果、业务 receipt、当前投影和客户端观察分别记录；公开链信息不授予私有世界原件读权。 |

正常前态须由原合法 producer 或可核验已确认历史形成，并记录其读取完整性；缺源、未实现 producer、读权限不足或不可取得原件分别记 `blocked`、`not_run` 或当前未知，不用空数组/默认零证明无权利。固定源集合的未启用项须有原配置及完整范围依据，不能因为正常脚本没调用就排除。

每例先列 O、W 各自按原合同可改变的准确对象、单位、金额/数量、nonce、权利及结果关联，再比较实际差分。隔离环境固定其他事件顺序；有合法后台 tick/维护时，另取同序列无 O 或无 W 的对照，区分原维护变化与被测操作效果。不要求生产读面全量 clone World；读取仍沿各领域的精确 key/range、同切点与预算。

**反例。** 将另一 chain 的 OC、同名资源、旧 owner 的 receipt、未解锁 vesting、restricted starter、quota 或裸 `ServiceOutcome` 当作 liquid/资格来源；逐一替换 World、policy、主体、类型或结果版本。预期在原相关校验处拒绝或保持未知，不产生新业务效果。缺字段的状态不能伪装完整零状态。

<a id="oc-isolation-02-consumers"></a>
## OI-02：14 类消费者的双向对照

依据：[完整消费者目录](../../world-runtime/runtime/oc-transfer-and-world-rights-isolation.design.md#oc-isolation-consumers)。每行均执行 O 单独成功、W 单独成功及不合法混用来源的对照；明确原合法付款造成的差分，再检查额外自动授予或兑回。共用 O 的受控正常例可复用，但不能以一个世界消费者代表其余各行。

**O 的独立算例。** 在当前 Phase 1 原 native 转账路径，A liquid=40、B liquid=0、A 原 nonce=2；有效签名和链身份、未过期请求转 10、nonce=3。按[原余额/nonce evaluator](../../../crates/oasis7_world_state/src/world/main_token_monetary_publication.rs)事先算得 A=30、B=10、nonce=3，总供给和 restricted/vesting 不因该转账改变；14 行世界权利不因裸转账增加。此例只核原无 fee debit 的 Phase 1 路径，不设新费率，也不声明其他业务免费。采用了另一合法费用合同的候选须按该原合同另算本金、费用及接收方，不能硬套 30/10。

| 目录项与独立世界正例 | OC 侧刺激及必须保持的边界 | 世界侧结果及反向检查 |
| --- | --- | --- |
| GRI-C01 资源/库存：按 [CF-12](agent-mediated-contract-formation-acceptance.manual.md#cf-12-family-handoff)从合法来源完成原 direct 资源或物料动作，逐账本复算数量。 | 裸 O 不生成 Electricity/Data、物料、库存或 custody；余额不足/无权的资源动作不能用 O receipt 补齐。 | 原数量、所有权和 custody 按 W 改变，不因资源单位被标为“有价值”自动增发/兑回 OC；显式支付另按 OI-04。 |
| GRI-C02 服务信用/履行事实：按 [CSF-01](continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-01-domain-results)分别取得真实交付、维护或工作单位。 | O、quota 或付款 ACK 不冒充单位完成、验收或服务信用；有价格不等于已履行。 | 保存原单位、用途、接受和责任。当前没有同名独立 credit ledger/API 时不造该资产；原履行、声誉和服务 quota 不改名互换，缺准确 producer 保持未知。合法服务价款按原结算单列。 |
| GRI-C03 合同：按 [CF-12](agent-mediated-contract-formation-acceptance.manual.md#cf-12-family-handoff)形成原允许的个人/组织/多方合同。 | 一张 OC receipt、单方付款或余额充足不能补必需方同版接受、代表任期、mandate 或实际业务来源。 | 合同/责任存在不自动转成 OC 或可兑付资产；条款明示的 MainToken 付款按 OI-04 的原家族单独验证。 |
| GRI-C04 市场/情境信誉：按 [REP-11](contextual-reputation-acceptance.manual.md#rep11-consumers)取原认可来源和实际消费者。 | 裸 O、余额排序、两次独立转账或咨询读取不产生新信誉/资格；真实交易只有符合原 source/purpose 采用条件才提供相应事实。 | 原咨询、credential、市场事实和历史 score 不铸造 OC，不隐含折扣或付款；旧版本与新用途分别按原政策计算。 |
| GRI-C05 工业作者/R&D：按 [RD-02](rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd02-provenance)、[RD-04](rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd04-transfer)核真实贡献及显式整槽转移。 | OC 付款、artifact owner 或组织名字不替代贡献、使用权、原 charter 或收益转让同意。 | 原贡献、slot/bps/E、paid 和历史个人主体保持原义；份额或作者身份不能自动包装成可转账 OC，原合法收益与程序费另账。 |
| GRI-C06 提案/试点/准入及普通 product/factory：按 [IC-M04](industrial-creation-simulation-and-admission.manual.md#ic-m04)及 [GEL-PF01](governed-extension-lifecycle.manual.md#gel-pf-publication)走真实四阶段和当前类型入口。 | 加 OC、旧票数或技术 Apply 不能补 simulation、pilot、admission、当前 grant、scope 和期限；每个实际效果入口重验。 | 合法发布/准入只产生原声明能力；不增发 OC，不把配方产物或工厂授权变成链上资产。原生产与费用继续对账。 |
| GRI-C07 许可/版税：按 [IL-02](industrial-license-and-royalty-settlement.manual.md#il02-use)、[IL-03](industrial-license-and-royalty-settlement.manual.md#il03-settlement)核真实使用和个人应付。 | 购买/持有 OC 不代许可、admission、成功完整 batch、N/E/slot 或当前收款权；不按 O 数量创造 fee unit。 | 只支付原资源/单位及原个人 payable；公开、到期、收益或许可身份不自动兑换 OC，也不使既有责任消失。 |
| GRI-C08 Agent claim/制造：按 [MFG-V02](agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v02)、[MFG-V04](agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v04)取得合法扩张与一次交付。 | OC 充足仍不能绕过材料/电力、claim cap、产能、prospective 集合或当前控制来源；restricted starter 只按原 slot-1 合同适用。 | 原 quote/order/claim/Agent/控制与实际费用按原规则同时核对；新 Agent、quota 或控制权不重发 starter、不铸造 OC、不授自动自治。 |
| GRI-C09 charter/地方资格：按 [地方双资格](../../world-runtime/runtime/regional-charter-authority-and-recovery.design.md#charter-dual-qualification)取得完整当前来源。 | O 或高余额不补持续本地贡献、scope、受限 lock/委托/控制人条件；仅在原规则声明的地方事项中合取。 | 资格/charter 形成不增发或任意转移 OC，不取消无关账户的独立转账；原明示 bond/lock 变化单列。 |
| GRI-C10 普通治理：按 [GAME-019 AC02](../../game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#ac-game-019-02)、[AC04](../../game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#ac-game-019-04)核主体、事项 lock/委托/控制人与冻结 snapshot。 | 外部转让不复制当前资格、双投或追溯改写有效窗口的原票；只有原事项合同允许的重评可改变当前资格。 | 原合法 lock/解除可改变其明确持有类别；资格、投票或 passed 字样本身不增发 OC、不授未声明转账。原货币治理动作须另有自己的合法 effect。 |
| GRI-C11 tenure：按 [TN-01～06](chartered-conflict-recovery-and-soft-season-acceptance.manual.md#tenure-cases)取得、续期、转移或收回地点权利。 | O、买方自报或旧 tenure receipt 不补地点、期限、当前转让/收回 authority 和独立权利。 | 仅原权利/资产/责任按原规则变化；不可从 tenure 变化补造 OC 价款或全局赎回，明示 bond/补偿仍沿原来源单次结算。 |
| GRI-C12 levy：按 [levy 验证](../../world-runtime/runtime/public-levy-authorization-and-settlement.design.md#levy-verify-001)核预授权、目的/受益 scope、预算、期限及原账本。 | O、余额或普通投票不能补合法 levy；无效/过期/越界请求不得借 OC 来源扣原资源或欠费。 | 原合法 levy 按原资产/单位记一次；缴付或拒付不自动改变 OC 转让规则、通行/无关资格，不将 levy 账目泛化为 OC 资产或兑换权。 |
| GRI-C13 竞争/战争：按 [冲突准入](chartered-conflict-recovery-and-soft-season-acceptance.manual.md#conflict-admission-cases)、[恢复与软赛季](chartered-conflict-recovery-and-soft-season-acceptance.manual.md#review-season-cases)核原版本。 | O 不补宣战/参战/目标保护、占领或提取条件；War Defer/未启用范围仍保持，不为测试新开战争入口。 | 原合法损失、归属和历史按原规则保留；胜负、积分、占领和赛季更新不自动产生 OC 或兑回。旧公式回放不被新隔离说明重写。 |
| GRI-C14 紧急保供：按 [ES-VERIFY-001](emergency-supply-system-acceptance.manual.md#es-verify-001)及 [004](emergency-supply-system-acceptance.manual.md#es-verify-004)取得合法授权与真实履行。 | OC 付款/余额不补危机、真实需求、授权、路线、目的地或资源；不得把地方双资格自动移入原保供 gate。 | 采购/补偿只按原承诺、真实接收和已锁来源付款；受益资源不变自由库存、OC 或额外治理权，不重复付款/派发。 |

各行均保留 O、W 的实际结果和完整相关差分，明确原允许写集内变化及其依据。只看到“不支持”、无异常日志或未出现额外字段，不能算该行合法正例已通过；当前无法构造原合法来源时，已定义的正向方法仍保留并记录实现/环境阻塞。

<a id="oc-isolation-03-gates"></a>
## OI-03：原资格与付款条件分别成立

依据：[地方双资格适用性](../../world-runtime/runtime/oc-transfer-and-world-rights-isolation.design.md#oc-isolation-governance)。对原明确采用双资格的同一地方事项固定当前 policy/scope、贡献窗口、主体和控制绑定，分别运行：全部条件成立；只有 OC/lock；只有贡献；委托已撤销；scope 或有效期错误；控制人上限不满足；证据未知。只有原全部条件成立者取得该事项的当前资格。把未知贡献/lock 当零而放行、换账户扩大资格、转让后重算旧冻结票，均为失败。

再取 C03、C06、C08、C14 的原合法正例，保持其实际付款和其他必要资源足额，只移除与该事项无关的地方 charter/双资格材料。预期仍按各自原合同判定，不因统一套入地方 gate 而多出拒绝。反向分别移除真实必需方接受、工业阶段、claim 容量或危机证据，即使 OC 足额仍拒绝。费用不足是原付款条件失败，不能被写成 OC 自动拥有或剥夺资格。

在预览之后、正式提交之前改变原当前授权或必要来源，按原操作重验；失效不追溯删除此前合法结果。预览、查询和原因解释不预留业务资格或资源，不推进原转账/世界业务的 operation nonce，也不把 World 业务状态改为已满足；原 signed read/preview 的认证 nonce、有界请求 ledger 或审计元数据仍沿其原协议处理。

<a id="oc-isolation-04-payment"></a>
## OI-04：独立单侧失败与原同根交易

依据：[合法显式付款与原子族](../../world-runtime/runtime/oc-transfer-and-world-rights-isolation.design.md#oc-isolation-explicit-payment)。先声明本例是两个独立操作，还是原协议已经定义的同根业务；以原动作、条款、写集和提交边界判断，不能按“页面有两个步骤”分类。

| 可达输入/顺序 | 事先写明的独立预期 |
| --- | --- |
| 独立 O 成功后，W 因明确缺资格而被原 gate 拒绝 | O 的真实余额/nonce/结果保持；W 不产生被拒的资格或业务效果。已成立的原费用按各自原合同保留，不为“两个页面都成功”补权或自动退款。 |
| 原合法 W 成功后，独立 O 因余额不足、错签名或重复 nonce 拒绝 | W 的原权利、责任、结果及合法成本保持；O 无转账效果，不撤销或重做 W。 |
| 独立 O、W 均成功，次序分别为 O→W、W→O | 只出现各原合同允许的一次效果；W 若明示依赖 O 的已确认资金，只有满足该真实前置的顺序可通过，不借入账前资金或等待状态补成功。 |
| 独立操作的一侧提交结果未知，另一侧已有可确认结果 | 保留已确认一侧；未知侧按 OI-06 查原件，不能推为失败、未提交或新操作。是否阻塞后续依赖按原业务，不新增跨侧自动补偿。 |
| 原 `immediate_exchange_v2` 等合法同根家族 | 沿 [CF-12](agent-mediated-contract-formation-acceptance.manual.md#cf-12-family-handoff)提供同世界 liquid MainToken、原 token auth/nonce、完整必需方接受和所有腿/费用。全部成功才同次形成原结果；不得拆成两次独立转账后声称原子交换。 |
| 同根家族最后一项来源、nonce、权限、容量、费用 payer 或 domain write 失败 | 按 [CF-04](agent-mediated-contract-formation-acceptance.manual.md#cf-04-atomic)和 [CF-15](agent-mediated-contract-formation-acceptance.manual.md#cf-15-recovery-audience)，确定未提交时所有原业务写集保持前态；提交未知保候选并按 G1 恢复，不能拿“单侧历史保留”豁免该族的原子性。 |

同根正例与受限来源反例必须成对：自由 liquid 可用于原合法腿；同量 restricted starter、未解锁 vesting、quota、外部桥到账或世界 credential 不能冒充该腿。原有合法主币付款可能使世界业务同时取得资源或履行合同，那是显式条款与原 authority 的结果，不能误判为禁止的自动兑换。

<a id="oc-isolation-05-fees-bridge"></a>
## OI-05：费用、服务信用与单向额度桥

依据：[费用与 bridge 边界](../../world-runtime/runtime/oc-transfer-and-world-rights-isolation.design.md#oc-isolation-fees-bridge)。

1. **Phase 1 quote。** 复用[签名 metadata 与校验](../../p2p/token/mainchain-token-ideal-transaction.design.md#itx-signed-metadata)、[实际展示边界](../../p2p/token/mainchain-token-ideal-transaction.design.md#itx-surface)。有效 `max_fee/fee_asset_id` 随签名/显示保留，但不从字段存在推定真实 debit、payer/sponsor 或 priority。缺/错必需字段按原解析器拒绝；[Phase 2 原 gate](../../p2p/token/mainchain-token-ideal-transaction.design.md#itx-phase2-gate)未采用时不编造收费正例，也不放宽其他业务的真实费用。
2. **原领域费用。** C03/C06/C07/C08/C12/C14 分别从原 policy、真实输入/输出及 payer 算本金、费用、hold、结算/退回；使用旧报价、错资产、受限来源、聚合不足和接收溢出反例。收费 attempt、确定未提交、已提交但回复丢失各按原合同核账，不能统一写“失败零费”，也不能为查询原结果重新调用收费业务。复用 [IL-02](industrial-license-and-royalty-settlement.manual.md#il02-use)、[IL-07](industrial-license-and-royalty-settlement.manual.md#il07-recovery)和 [CSF-05](continuing-service-fulfillment-and-remedies-acceptance.manual.md#csf-05-budgets)的独立费用闭包。
3. **无独立 credit 资产。** 分别输入原服务履行事实、情境声誉投影、运行 quota 和未识别的 `credit` 标签；只采用原 producer 的准确单位/用途。标签、付款额、聊天声明或未识别 schema 不被转换成可转账/可兑回资产；缺 producer 不由测试补造 ledger。
4. **bridge 正常与异常。** 按[原 operator runbook](../../p2p/token/mainchain-token-newapi-quota-bridge.runbook.md)完成获准测试用户/route、正确金额与 deposit token、确认入账、reconcile、远端核对和原订单结果。重复/过期 route、金额/token 不匹配、缺绑定、query mismatch、timeout 分别按原路径保留 `manual_review` 或原真实状态；原链上入账事实不因服务失败消失，未确认服务不能填为已充值。
5. **bridge 重启与反向拒绝。** 保留原 `external_order_id`、deposit/route/ledger 身份和状态文件，在远端已处理但本地回包丢失后核原订单；不得换订单重发 credit、删 state 重建、口头标成功或将 `close` 当退款。服务 quota/token_key 不提供 `OC <- quota` 兑回、世界库存、治理/工业资格或 Runtime receipt。实际充值、provider 额度消耗和世界执行分别判定；一侧成功不补另外两侧结果。

使用已授权隔离材料；证据只保安全关联和必要摘要，不保存私钥、raw token_key 或完整敏感 bridge state。

<a id="oc-isolation-06-recovery"></a>
## OI-06：重复、过期、重连和历史回放各一次效果

依据：[原请求与恢复](../../world-runtime/runtime/oc-transfer-and-world-rights-isolation.design.md#oc-isolation-recovery)。分别对原 O、各类 W 及 OI-04 的原同根族运行下表；时钟、nonce、业务键和期限沿原协议，不创造共用过期字段或跨侧去重 ID。

| 刺激 | 必须成立的结果 |
| --- | --- |
| 同一完整签名 payload/nonce 重发；换连接或 transport ID；同 nonce 改 payload | 原 OC nonce 守卫和各 W 的原业务去重各自生效；至多一个原业务效果。原 OC 接口可以返回 nonce replay 拒绝，不要求新增“缓存成功”能力。换包装不允许复制原业务。 |
| OC expiry 与 World 原期限的前/边界/后；分别改客户端时间、重新连接 | 使用各自原有效性时钟及边界；不能把 Unix ms 直接作 World tick，也不能重签/重连延长旧有效期。已确认历史按原版本保留。 |
| submit 接受、链上包含、World 决定、投影、回包各边界丢失或失败 | 各层如实显示；链包含和余额/last nonce 变化不能单独证明某笔 World 成功或失败。沿原身份、原 action/payload 和真实 World 结果关联核对。 |
| status/history 未查到、tracker 超出保留范围、落后节点、暂缺原件 | 原未知保持未知；按原精确身份/范围查询或等待恢复，不以空结果生成新 nonce/新业务。安全查证不可用时阻断有歧义的后继效果，给具体缺项和原恢复路径。 |
| 确认未提交、确认提交、真正 G1 `StatusUnknown` | 未提交才按原规则处理；提交恢复原结果不重扣/重发；G1 未决保完整 staged candidate/base generation，并 fence 整个旧 World/writer、tick、save 及依赖派发。普通客户端丢 ACK 不自动等于根提交未知。 |
| 当前失权、换主/换 key、旧历史仍存在 | 当前写权和原件读权分别核对；缓存命中不授私密读取或新效果。按原允许恢复取得原结果，不改原主体、责任或已确认成本。 |
| snapshot + 非空 journal、跨适用节点恢复、升级后 replay | 原版本/工件/业务身份/nonce/费用和权利恢复一致；缺依赖拒绝相关写入。canonical 后缀按原 delta 重建本应存在的余额、权利和 nonce；历史重放不重新运行收费业务，不产生第二份资格、余额增量或 bridge 订单。 |

OC 正常样例重发后仍为 A=30、B=10、nonce=3；同 nonce 改 amount 也不能形成第二次转账。原 W 的累计量、claim/hold、已付/未付、slot/E、准入和业务 receipt 按各自样例独立复算。若首次结果未知，不能先假定这个 30/10 已成立；必须先核真实提交三态。

<a id="oc-isolation-07-coverage"></a>
## OI-07：完整消费者、原范围与有界取证

依据：[有界组合](../../world-runtime/runtime/oc-transfer-and-world-rights-isolation.design.md#oc-isolation-bounds)。先由选定候选的原 manifest、真实 handler/writer、module/原生适配和直接 consumer 建立 GRI-C01～14 对应关系，注明各原规则版本与其合法写集；再逐项回查 OI-02 的正向、反向和混用来源反例。入口包括实际采用的正式调用、原 native/WithModule、tick/内部 producer、恢复与 replay，不强制无对应协议的分支新增 CLI 或 HTTP route。

每一已采用 producer/consumer 至少对应可达正常前态、原结果来源及一项会检出 OC/世界权利混用的反例；同一通路可复用原测试，但必须记录其准确覆盖关系。只测 Viewer、只看相同 DTO、只列 14 个名称或用 unsupported 负例，不能消除漏接入口。未支持实现和缺真实来源逐项保留，不能改写原产品义务；原明确未采用的能力同时记录依据及采用变化后的重核条件。

单次对照只关联一个 O 与一个原 W；bridge 是其独立服务视图。W 的原完整当事人、证据、责任、费用和恢复材料不因比较被截成一个代表对象。按各原读写上限取严，测试 limit、limit+1、分页 root/权限变化、超预算和缺原件；超限保持明确失败/未知，不取前 N 项签完整，不新增全 World/journal 扫描或旁路费用。缺一页不等于这一类没有权利。

组合验证保存每个原接口的实际调用次数、字节/证明范围、错误和账本差分；预算是候选配置或独立计算，实测值另列。只读核对不推进原转账或世界业务的 operation nonce，不产生业务 reserve、资格、订单、业务费用扣账或重新调用计费业务；原读认证 nonce、有限请求/审计元数据按原协议处理。原查询本身若有另行声明的合法成本，按其原读取合同单列，不能伪装业务再提交。

<a id="oc-isolation-08-surfaces"></a>
## OI-08：真实入口、结果解释和安全下一步

依据：[同源读面](../../world-runtime/runtime/oc-transfer-and-world-rights-isolation.design.md#oc-isolation-read-model)。按同候选、原 O/W 和同等当前可读范围核对已有 Launcher/链 API、适用 Viewer/API/Agent；不把链公开字段外推为世界私密权限，也不为原公开读接口新增鉴权前置。

| 已有接点 | 核对方法及结果层 |
| --- | --- |
| Launcher [转账构造/提交](../../../crates/oasis7_client_launcher/src/transfer_entry.rs)、[转账面板](../../../crates/oasis7_client_launcher/src/transfer_window.rs) | 校验输入、签名及 live chain/network 来源；缺 identity 按原 guard 拒绝签名。检查提交、状态、history/accounts 的原关联，不把账户余额或“已提交”显示成 World 资格成功。 |
| [链 transfer API](../../../crates/oasis7_chain_runtime/src/host/transfer_submit_api.rs) 的 `/v1/chain/transfer/submit`、`status`、`history`、`accounts` | 实际解析/鉴权/preflight 与原 nonce；submit success 只代表交入共识，当前 tracker `confirmed` 来自 committed batch，仍须查原 World 结果。重连/查询不到不能用新 nonce补发原未知转账。 |
| [Phase 1 原验证定位](../../p2p/token/mainchain-token-ideal-transaction.design.md#itx-validation) | 可复用 expired、非法 asset、签名及 restricted-only balance 等[现有测试源码](../../../crates/oasis7_chain_runtime/src/host/transfer_submit_api_tests.rs)；断言定义、静态阅读和实际运行分别记录。 |
| Viewer [GR-011 目标读面](../../world-simulator/viewer/viewer-governance-targets-uiux.design.md#t-gov-08-gr-011)与实际世界 consumer | 对照 O/W 各自条件、成本、权限、来源、结果及下一步；目标图稿不证明完整写入口。没有真实 route/producer/原件 lookup 时明确缺项，不造资格兑换或恢复按钮。 |
| API/Agent 的原世界结果 | 沿[逐入口适用方法](external-agent-runtime-contract-validation.manual.md#api-primary-entry-validation)和各领域原方法核同一结果、current read authority 与准确继续；普通 SDK/CLI 能发某消息不证明全部命令可恢复。模型解释或新建议不能补签原结果。 |

对照四种画面/返回：O 成功而 W 拒绝、W 成功而 O 拒绝、任一侧未知、两侧各自成功。玩家应能指出哪一侧已确认、另一侧缺何条件、哪些资产/责任仍保留以及真实下一步；不能把全页合并为一个成功灯。原读权限不足时给可公开限制，不泄露另一方原件；已确认结果可查询不意味着当前仍可执行旧授权。

需要真人理解结论时，选已实际接通的受控入口与上述四种状态，先让参与者自行解释，再展示原结果对照；记录原话、提示和误解，不以解释者读过本手册证明产品表达有效。现阶段没有正式 consumer 或参与者时只报告设计/方法及对应未运行项。

## 结果记录

后续实际实现/验收 PR 记录候选、入口、合法前态来源、GRI-C01～14 覆盖、独立预期和实际差分、O/W 原件与结果层、费用版本、故障点、当前权限和未运行/阻塞理由。通过只覆盖实际完成的场景与环境；原消费者未实现、测试定义未执行、正式资料或真实费用缺失均不改写为通过。发现隔离错误时修原 writer/consumer 与必要设计，保留另一侧已确认历史和原失败证据，不用清账、换身份、重签或新建资产掩盖。
