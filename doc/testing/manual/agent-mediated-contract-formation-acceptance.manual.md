# Agent 代表的原子交换合同形成验收手册

- 状态：`active`（目标合同的验收方法，不是实现或业务验收通过记录）。
- Owner role：`qa_engineer`；协作：`runtime_engineer`、`gameplay_designer`、`agent_engineer`。
- 技术合同：[合同形成设计](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-scope)。
- 源码与历史合同审读基线：`5e62da69ddb511f9f10a524accdf0071ecd6b91c`；目标设计、本手册及未来实现的实际候选分别由对应 PR 固定。
- 本轮只执行设计审读；以下业务 fixture、故障注入、持久恢复、跨节点和正式入口验证均未执行。实际执行结果进入 PR 与原证据入口，不由本手册生成通过结论。

## 1. 范围、前置与证据

本手册承接 [REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) / [AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) 中的非绑定沟通、同条款明确接受、当前授权、权威生效与原结果恢复，Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。正向范围仅为两个不同个人当前 owner，各自拥有一个不同 Agent；两个 Agent 同 world 且 canonical 共址，每方提供一条正数 Electricity/Data 腿作即时交换。形成与履行在同一次 canonical staged commit 完成，不包含跨地点运输或用对方本次到账为自己的支出融资。

对 Agent 的理由、替代和纠正，只验证这一交换场景实际适用的 [REQ-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-003) / [AC-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#ac-wr-ia-003)，Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376；不代签全部社交、治理、冲突或记忆驱动决策。

组织、长期委托、跨时间履约、物料运输、检查点、抵押、争端、继受、自动续期和公共 levy 均不属于此正向 profile。[CR-002 的持续服务义务](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002)保留原范围；本手册只用未支持的服务请求验证拒绝边界，不验证其履约或救济。旧 `OpenEconomicContract / AcceptEconomicContract / SettleEconomicContract` 不作为新双腿 profile 的运行方法。

### 1.1 执行准备

实施候选具备目标能力后，先固定合同/profile 版本、条款表示与内容身份、授权和资源 authority、同 world/共址事实、Data grant、世界时钟、交易费用政策、kernel runtime 计费 authority/输入/实际 payer、各人各资源总扣减上限、操作身份与业务 lineage 的映射、权威提交和最终性判据、原件查询入口及适用容量边界。实现 PR 登记真实 fixture、故障注入点和调用方式；本手册不虚构尚不存在的命令或端点。缺任一必要能力时记未支持、未执行或未覆盖，不能用旧单腿付款、合成回包或截图补签。

| 验证层 | 方法与判定范围 |
| --- | --- |
| 设计审读 | 按以下八个场景逐项推演条款、权限、状态、账本及失败出口，核对准确设计链接和前后不变量。只证明设计已说明相应义务。 |
| 确定性业务验证 | 在获准隔离世界执行同候选的正常、拒绝、并发、重复和守恒 fixture，读取真实权威结果；不能只断言函数返回成功或请求被接受。 |
| 持久恢复与跨节点 | 在实施已支持的日志/快照、发布和最终性边界注入故障，固定节点、配置与实际故障点；单进程算例不证明这一层通过。 |
| 正式读面与玩家理解 | 在同候选实际提供的入口核对正文、受限读面和因果解释；分别记录 API、Agent 和 Viewer 的实际覆盖。文档推演不证明 UI 接入、模型自主执行或真人理解。 |

每次保留 source/tested-tree、世界与政策/profile 版本、授权可见的条款/修订和接受关联、原操作身份、合同及义务 lineage、实际权威顺序、故障点、前后资源/权限/义务差分和 receipt。记录应足以复算结果，但不公开私钥、可重放凭据、完整签名、私有 prompt、其他主体数据或受限证据正文；敏感原件按原权限保存，公开记录只给脱敏关联和结论。通用身份、未知结果及受限查询方法可复用 [api-auth](external-agent-runtime-contract-validation.manual.md#api-auth) 与 [api-idempotency](external-agent-runtime-contract-validation.manual.md#api-idempotency)，不能替代本合同自身的判定。

### 1.2 共同正向样本与算例

个人甲与乙分别是 Agent A、Agent B 的当前 owner。甲希望用可用电力换取 Data，乙希望获得电力且具有可转移 Data；两人比较交换、继续独立取得资源或暂缓。Agent 只呈现提议及理由，owner 的明确接受另行记录。固定同一世界与 canonical 共址事实、两个来源账本、实际接收方、同一条款修订，以及 Data 从 B 到 A 所需的真实有效 grant；客户端写距离为零不算共址证明。

条款中 `e > 0` 表示 B 实收的 Electricity，`d > 0` 表示 A 实收的 Data。费用必须分开列出交易费用与 mandatory kernel runtime 计量费，并同时进入总扣减闭包：每个 Agent、每种资源的总扣减等于该资源净转出本金加交易费，再加实际由其承担的 runtime 费。runtime compute 使用 Data、运行用电使用 Electricity，可能与 payer 交易腿的资产不同；不能只检查本腿资产。

费用只来自固定、合法且适用于本 profile 的政策/计费 authority，按原资产向明示合法 treasury 接收账入账。runtime payer 由实际 artifact owner authority 解析，不能默认等于提交方或由客户端选择；它必须是两名已明确授权的当事 Agent 之一，不能让第三方支付，也不能因 owner 缺失、查找失败或关闭计费开关而免费绕过。各人各资源总扣减受已接受上限约束；实际计费输入、成本、payer 或政策漂移按原合同拒绝/重报价。生产适用授权不成立或费用闭包不完整时保持 blocked/unsupported，合法测试输入不证明生产适用。本 profile 不引入 burn sink、奖励、声誉变化、公共 levy 或新税率；有未承接的强制费用/效果时不能忽略后继续提交。

以下数字仅用于实施提供合法测试费用快照时的复算示例，不是生产参数或推荐交换比例。测试交易费为 A 支付 1 Electricity、B 支付 1 Data；合法 artifact owner/payer 为 A，固定计费输入与输出各 1024 bytes、2 effects、1 emit、`new_state=true`。按基线 [runtime 费用函数](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)，这一输入的 compute 费为 7 Data、用电为 5 Electricity；[payer 解析与费用准备](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)不能用提交方身份替代。该例只在实际 profile 的输入、输出、界限与费用 authority 均匹配时使用，不能伪填 effect/byte 数压低费用；其他合法输入须重算全部账户。

| 资产 | A 初态 | B 初态 | treasury 初态 | 净交付 / 交易费 / runtime 费 | 完成后 A / B / treasury |
| --- | --- | --- | --- | --- | --- |
| Electricity | 20 | 10 | 0 | A 向 B 净交付 6；A 交易费 1；A runtime 费 5 | 8 / 16 / 6 |
| Data | 20 | 20 | 0 | B 向 A 净交付 12；B 交易费 1；A runtime 费 7 | 25 / 7 / 8 |

逐资产 oracle：本金分别按两腿净额入账，全部交易费和 runtime 费按已验证 payer/资源分别扣减、进入明示 treasury；相关账户该资产总量守恒，不跨资产折算。本例 A 的提交前支出为 12 Electricity、7 Data，B 为 13 Data；两方必须用各自提交前可用余额足额承担，不能借本次到账融资、只提交净差额或只检查每个分项单独足额。接收和 treasury 容量/整数上限也按聚合后值校验，不允许饱和截断造成资产消失。

交易 Data 腿的定向 grant 是转移前置，不是被交付的资产，也不因交换自动创建或扩大；Data 费用进入 canonical treasury 时按系统费用权限校验，不伪造 Agent grant 或把此边界扩大到其他收款方。若只读比较另取 `d = 4` 且其余费用相同，甲的 Data 由 20 变为 17，必须诚实显示，玩家可以暂缓/重报价；这属于收益取舍，不新增“不划算即拒绝”的规则。

<a id="cf-01-nonbinding"></a>
## CF-01：非绑定沟通与待校验接受无业务效果

设计依据：[范围](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-scope)、[生效提交](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-commit)。

- 给定：共同样本的前态与未成立的提议；记录全部相关余额、占用、权限、声誉、业务义务和结果账本。
- 执行：依次保存普通聊天、Agent 草稿、预览与 offer；仅甲接受；甲乙接受但尚未由任一当前当事 owner 显式提交同一 proposal/revision/digest。每个阶段分别重复读取、重连、重发同一非绑定操作并检查当前结果；另用“同意”“已完成”等普通聊天内容作负例。
- 判定：所有这些阶段仍为非绑定，不预留或扣款，不创建债务/collateral、奖励、优先级、权限、声誉效果或已成立义务。提议、接受事实及操作审计可以被记录，但不能被当成业务效果。最后一方接受不自动触发提交；双边接受待校验仍不能展示已交换、已履行或可用新增余额。
- 计费前置：验证非绑定登记/查询具有合法零游戏资源费路径；若当前入口会在这些阶段先扣 module/runtime 费且没有满足本合同的接入方式，该 profile 仍不可启用。不得删除 artifact owner、关闭现有计费或让第三方暗付，以制造零效果通过。
- 反例：报价读取增加占用、单方接受预扣一腿、重连把 pending 推为成功、依据聊天或模型自报产生接受，任一出现即失败。绑定与履行只按 [CF-04](#cf-04-atomic) 的同一权威结果判定。
- 证据：各阶段与前态的业务差分、接受来源及对应修订、原操作关联和实际状态；不能仅保留无按钮或灰色按钮的图片。

<a id="cf-02-terms"></a>
## CF-02：完整明确条款与同版双边接受

设计依据：[条款](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-terms)、[原子交换 profile](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-atomic-profile)。

- 给定：包含世界、合同/业务 lineage、修订与内容身份、双方 owner/Agent、每条腿资产/净额/来源/接收方、交易费政策、runtime 计费 authority/payer/输入、费用接收、各人各资源总扣减上限、适用期限和同 world/共址等前置条件的条款；双方可读取各自必须明确接受的完整范围。
- 执行：先让双方明确接受相同修订作为正例；再分别令甲接受 r、乙接受 r+1，或在一方接受后修改主体、Agent、任一腿金额/方向、费用/接收方、政策、artifact/payer 或计费输入、总扣减上限、期限或作用范围。另测删除一腿、未知资产、非法单位/数量、金额溢出、错世界、仅以自由文本替代 typed 条款，以及用旧内容身份搭配新正文。
- 判定：接受必须绑定实际明确呈现的同一内容，不能按相同标题、显示名、合同 ID 或相似文案拼接。原接受保留其出处；影响条款的变更必须重新获得所需接受，不能复用旧签名/接受或静默缩量、换资产、换收款方。内容身份与完整条款不一致、缺少执行前置或超出 profile 时无业务效果地拒绝。
- 摘要向量：按设计依次计算不可变条款、签署正文、接受证明和请求摘要。第一方接受与第二方接受各自增加 `record_version` 后，同一 `terms_revision` 的 `terms_digest/render_digest` 必须保持不变；修改任一可执行条件则改变条款摘要并使旧接受失效。验证派生摘要不进入自身输入，不把可变 CAS 版本混入不可变条款，避免第一份接受反过来使第二方无法接受同版正文。
- 容量边界：按设计 §9 的 canonical terms/命令 bytes、UTF-8 注释、分页项数及合法 manifest 的存续期/未关闭元数据限额，逐项构造边界内、恰好边界和超限输入；多字节字符按实际 bytes 计，不能只数显示字符。超限须有界拒绝、不截断成另一份可签条款、不产生费用或部分接受；既有原件与去重事实不因新请求超限被删掉。
- 补测：对同一修订重复接受不会创建第二份业务；签名/请求来源有效不能替代条款完整性。核对受签正文由该 typed 两腿条款按 canonical 模板唯一渲染：删改正文、添加模板外承诺或用不同正文套同一内容身份，均不能在不重新核验的情况下接受/提交。预览、接受和提交使用的费用/政策关联必须可核对，不能在最后一方接受后偷偷附加费用。聊天、理由和未来用途只能明确放在非绑定注释中，提到未来目标本身不应误拒绝合法交换。
- 证据：脱敏的条款差异、每方明确接受所指内容、拒绝原因和零业务差分。不能以实现恰好使用某个字段名称代替上述语义。

<a id="cf-03-authority"></a>
## CF-03：双方当前 owner 权限与 Data grant 重验

设计依据：[授权](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-authorization)、[提交点](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-commit)。

- 给定：两个不同个人当前 owner 对各自不同 Agent 的直接签名/明确接受、同 world/共址事实，以及每条实际交易 Data 腿发送方到接收方的有效 grant。只读凭据、公开主体 ID、历史 owner 证明、Agent 建议和一般登录状态均不能替代这些前置。
- 执行：在预览后、第一方接受后、双方接受后但提交前，分别撤销或改变一方当前 owner/写权限、使授权到期、移交 Agent、移动 Agent 破坏共址，或撤销/过期 Data grant。交换实际 Data sender/recipient、只提供相反方向 grant、使用另一世界/Agent 的接受、只读或长期受托身份也须分别测试；两方条件分别变化，不能只测发起方。
- 双向与 payer 负例：两腿都选 Data 时分别只保留 A→B 或 B→A 的 grant，并在接受后撤销其中一向；不能用一向 grant 覆盖另一向。另令实际 artifact owner 为第三 Agent、缺失或无法核实，或客户端把 payer 改成提交方，均不得向未经此合同明确授权的主体扣费或免费继续。两个 Agent 由同一个人控制也不满足本 profile 的双个人主体前置。
- 判定：权威提交边界核对两位当前个人 owner、同条款接受、同 world/canonical 共址及每条腿真实权限。签名曾经有效不保证现在可执行；拒绝或重报价不留下部分绑定、资产占用或扣款。新 owner 不自动继承旧接受/待决请求；组织或长期委托不被转换成本 profile 的个人 owner 授权。缺共址事实不能用客户端距离、同名地点或一次旧查询补齐。
- 竞态：允许已先行合法提交的结果保留历史，但其后的撤权不能被旧接受绕过以创建新效果；先行撤权/过期时未提交交换必须失败。具体顺序按 [CF-05](#cf-05-races) 记录，不用客户端时间判断。
- 证据：受权可见的 owner/grant 版本及范围、接受和提交所读版本、权威顺序、拒绝后两腿及费用账户的前后值。历史结果的读取权限另按 [CF-07](#cf-07-read) 验证。

<a id="cf-04-atomic"></a>
## CF-04：两腿即时形成、结算与逐资产守恒

设计依据：[原子交换 profile](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-atomic-profile)、[canonical 提交](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-commit)。

- 给定：共同正向样本、同版明确接受、当前权限、交易费用政策与 mandatory runtime 费用闭包；每个 Agent/资源的本金及全部费用聚合后可用余额足额，真实资源上限检查和目标能力已在实施候选中提供。
- 执行：由当前当事 owner 对已获双方同版接受的 proposal/revision/digest 显式提交一次，读取对应世界权威结果和全部相关账本；甲、乙各自可提交的正例使用独立 fixture，均按 §1.2 逐项复算。随后分别在一腿计算已完成后使另一腿/计费 payer 余额、grant、费用接收、总扣减上限、同 world/共址或前态校验失败；接收者和 treasury 聚合入账容量/整数溢出分别测试，覆盖全部准备完成但提交前失败及 late failure。故障点必须由真实实现注入，不以一个模拟错误返回代替。
- 聚合不足：在本例中令 A Electricity 余额为 8，虽分别足够本金 6、交易费 1 和 runtime 费 5，仍不足合计 12，必须整笔拒绝；令 A 只有 3 Data 时也不能借本次收到的 12 支付 runtime 费 7。两条同资产腿再分别测试提交前不足但净额看似足够，以及各自足额的正常例；都不能净额融资、漏掉跨资产 runtime 费或使用未披露 payer。
- 正向判定：合同形成、A/B 两腿本金、全部交易费与 mandatory runtime 费、已履行业务身份和 receipt 在同一次 canonical staged commit 成立。不得留下“已绑定但只交换了一腿”或“计费已发生、交易尚未提交”的本 profile 状态；admission/请求接受本身不是这个结果。
- 失败判定：任一必要条件失败或 late failure 已确认未提交时，全部相关业务账本保持前态，包括双方资源、费用、绑定义务及业务去重账本；不得出现先扣再靠后台补偿伪装原子性。持久提交未知不能判为无效果或回滚成功，须按 [CF-06](#cf-06-recovery) fence/查询原候选；已确认提交后的发布/维护或回复失败不撤销原交换。操作失败记录本身不证明交易成立或未成立。
- 守恒判定：按每个 Agent/资源分别复算净转出本金、交易费及 runtime 费的聚合扣减，再与双方净入账和 treasury 全部费用入账对账；逐资产总量、非负余额、明示接收范围及一次履行业务身份均成立。未定义 sink、来源、payer 或政策效果使该 fee profile 不支持，不能凭空扣失、增发、漏费或改用其他资产。额外奖励/声誉/collateral 不能从旧合同参数导入。
- 证据：同一提交关联的两腿及费用差分、条款/政策身份、authority receipt、故障前后完整相关状态。仅断言一方余额、单条事件或返回 success 不足以通过。

<a id="cf-05-races"></a>
## CF-05：接受、撤回、到期与资源竞争的权威顺序

设计依据：[提交](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-commit)、[授权](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-authorization)。

对下列每组竞态分别安排两种可达的权威顺序，记录原操作与最终 receipt；无需用客户端到达时刻猜测胜者。

| 刺激 | 必须成立的结果 |
| --- | --- |
| 双方重复/并发接受或同一合同同时尝试提交 | 接受事实不叠加成第二份义务；同一业务最多一个完整交换结果。 |
| 生效前撤回与提交竞争 | 撤回先合法生效则不形成交换；交换先合法提交则保留结果，迟到撤回不能退款、反向转账或抹除 receipt。不能同时显示撤回成功和交换成功却不说明各自范围。 |
| 期限边界前、到达边界及之后提交；客户端改时间或重连 | 按设计的权威世界时钟和边界判定；旧接受不延长期限，重试不刷新有效期。到期不新增罚款、声誉效果或新义务。 |
| owner/grant/位置/费用政策的有效条件改变，与最后提交竞争 | 校验与提交消费一致的有效前态；变更先成立则拒绝/重报价，不允许准备阶段旧通过绕过新的必要条件。已合法提交结果不被后来的变化回写。 |
| 两份真实不同交换同时使用不足以覆盖两者的同一余额 | 只允许当前资源能完整承担的交换；拒绝者两腿、费用和绑定均不产生部分效果，不透支、双花或因早看预览获得优先权。 |

退出本场景只指未生效提议的撤回或到期。停止 Agent 的未来代办、关闭会话、断线或取消本地等待，均不能被用作已提交交换的取消/退款。结果未知时须先查询原操作，不把本地取消作为权威未发生证明。

<a id="cf-06-recovery"></a>
## CF-06：原请求恢复、业务去重与未知结果

设计依据：[恢复](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-recovery)、[提交](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-commit)。

- 给定：持久操作身份、合同/业务 lineage、条款内容及可查询的权威结果；分别准备尚未提交、已提交但未收到答复，以及当前无法确认提交结果的候选状态。
- 执行：在接受事实登记、admission、权威提交/持久确认、发布与回复的实际边界前后注入响应丢失或进程故障。依实施支持范围分别从日志、快照及另一节点恢复，保留原身份，不重新构造一份“相同意思”的交易。
- 请求去重：同键同内容只关联原操作及其真实状态/结果；同键异内容明确冲突。撤权或读权限改变后不得因为命中缓存就交付原私密正文，查询仍需当前授权。
- 业务去重：换 request ID、换 transport nonce、重连、重放、换合同外部 ID 或条款修订包装同一已履行业务 lineage，不得再扣任一腿、交易费/runtime 费或重新成立义务；返回原关联或明确重复拒绝，不为查原结果重新运行计费入口。真正的新交换须有新的明确业务意图、双方新接受和当前校验，不能把“参数相同”一概判为重复，也不能用新身份掩盖原未知操作。
- 客户端观察未知：pending/unknown、查询超时、落后节点或暂缺 receipt 均不等于失败、未提交或可重新交易。保留原操作和关联，显示查原结果/等待可验证恢复的下一步；无法排除已提交时阻断该业务的新副作用，不自动换身份重签或补发。只有已确认 runtime 提交/服务状态安全时，客户端对某操作的观察未知才不应无故扩大为其他业务冻结；该原则不允许未恢复的旧 writer 继续提交。
- 持久提交未知：另在 generation 替换/同步/回读边界制造 `StatusUnknown`，按 [G1 普通保存与内存安装](../../world-runtime/runtime/runtime-storage-footprint-governance.design.md#storage-generation-g1-design)保留原 key、精确候选和必要材料，并 fence 整个旧 World/writer。随后尝试另一无关账户的 mutation/save 及依赖该未决结果的外部派发，均不得越过 fence；核定精确 generation 的提交结果、完成必要同步并对齐内存后才可恢复服务。读状态只能沿原合同使用最后已验证状态并说明限制；不通过重写旧 index、换世界或回退 snapshot 消除歧义。
- 恢复判定：确认已提交则安装/恢复原同一业务结果，全部资源/费用与 receipt 一致；确认未提交才依原合同允许路径处理，不能从一次查询空结果或读到旧 generation 推导未提交。材料不足或 writer 归属不明继续停止相关服务，不补造结果。恢复不能丢失去重、重置 lineage、延长条款期限或因重复读取创建第二次义务。
- 证据：每个真实故障点、原请求和业务关联、候选/generation 与提交三态、writer fence/内存安装/恢复服务结果、权威顺序、所有相关账户差分、持久 receipt 及当前查询权限。普通函数重试不代签崩溃、跨节点或持久恢复。

<a id="cf-07-read"></a>
## CF-07：正文、受限读面与 Agent 因果解释

设计依据：[读面](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-read)、[条款](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-terms)。

- 给定：同一候选、同一权威结果，分别以甲、乙、只有只读资格的受权主体、无关主体和已失去读权的历史 owner 查询；具体允许读者由设计及当前 authority 决定，不能由本手册增发权限。
- 执行：逐项比较完整条款和当前授权允许的投影；覆盖非绑定、单方接受、双方接受待校验、已完整交换、拒绝、撤回/过期及结果未知。注入缓存过期、权限改变和无关世界余额变化，再从实际提供的正文/API/Agent/Viewer 入口读回。
- 结果判定：受权读者能分清双方、实际代表权、同一修订及接受范围、两腿净交付、交易费、runtime payer/跨资产计量费、各人总支出与最终净变化、权威结果/未知状态和可执行下一步。不同读者允许看到的字段可不同，但同一事实不能被讲成不同结果；不得根据余额变化、聊天、Agent 自报、读到旧 receipt 或前端计数推导本合同已结算。
- 隔离判定：只返回当前授权允许的合同与证据范围；不能通过正文、原件/digest、错误或计数泄露其他主体私有内容或可枚举状态。历史签名和过去的参与资格不是现在的读授权。费用/交换所需公开给当事人的信息不扩张为查看对方完整余额、私有 prompt 或所有 Data 内容的资格。
- 因果判定：在本交换场景，说明被接受意图、Agent 理由和可读证据、stakes、预期后果、独立取得/暂缓等真实替代、纠正/打断最早生效点和纠正后结果。提交前改变金额或方向产生需重新明确接受的条款；交换已提交后纠正只能影响合法的后续决定，不能写成撤销已结算结果。接受纠正与应用结果分别说明，不索取模型私有推理链。
- 读者检查：仅凭允许的内容应能回答“我给出与收到什么、谁接受了哪一版、是否已产生世界效果、当前能撤回什么、拒绝/未知时下一步是什么”。设计推演可以检查文案是否提供这些信息；正式玩家理解仍须实际样本，不能由字段齐全代签。
- 证据：同一候选/快照的脱敏正文与各受限投影、授权变化前后响应、因果及纠正关联。此计划不承诺当前 UI 已接通，不要求修改受保护 Viewer 设计或 SVG 来制造通过证据。

<a id="cf-08-compatibility"></a>
## CF-08：旧协议兼容与不支持 profile 的拒绝

设计依据：[兼容边界](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-compatibility)、[范围](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-scope)、[原子交换 profile](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-atomic-profile)。

- 旧协议对照：读取基线中 native EconomicContract 的动作形状、Opened/Accepted/Settled/Expired 历史样本及原测试定义。旧协议实际是 creator 向 counterparty 单边付款，未接受的 Open 到期仍有既有声誉后果；它不能映射成新合同零效果的 `offered`，也不证明双边对同一修订的明确接受。
- 兼容执行：实现候选按原协议重放旧事件、恢复旧快照，并比较原结果/资源/声誉和历史身份。不得补造第二条交换腿、双方接受、target profile 身份或新的 receipt；保留原语义和出处。拒绝目标协议、降级或暂不支持新 profile 时，也不能改写旧历史或把已提交新结果删掉。
- 架构检查：实现所用新语义必须遵守 [Runtime 的 compatibility keep 边界](../../world-runtime/prd.md#native-surface-分类与冻结规则)及合法部署/版本选择；本手册不授权扩展冻结的 native 协议、改核心 Action enum、选择 SC32 制度试点或建立旁路提交。新逻辑合同不是已有 wire/API 能力声明。
- 服务负例：以 typed service/profile 或未知义务字段请求把 `iron_ingot` 分两批送到另一工厂，附未来到达、维护、检查点、collateral 或延期义务；另在 typed 原子交换不变时，篡改应展示给签署者的 canonical 正文，加入模板外服务承诺。前者因 profile/字段不支持拒绝，后者因受签正文与 canonical 条款渲染不一致拒绝；不能忽略正文中的额外承诺继续收款。运行时不承担识别任意自然语言隐含承诺的验收要求，非绑定聊天/理由中提及未来用途也不应误拒绝合法交换。
- 服务拒绝判定：上述不支持或正文不一致的请求不能转换成 Electricity/Data 转账、把付款当交货或接受后等待未来完成。拒绝不生成绑定、资产/货物/容量占用、任何腿或费用、collateral、债务、奖励、声誉效果或优先级。
- 其他负例：组织/长期受托主体、未支持资产、交付 Data grant 本身、未披露费用接收方、未定义 sink/奖励/声誉政策或超出部署支持范围的 profile 均不能静默降级。明确不可用或拒绝，并保留安全的重新选择/暂缓路径；不虚构当前服务修复、争端、继受或退款按钮。
- 证据：旧原件与重放差分、两种协议身份/版本的可辨识读面、目标负例的原请求和零业务差分。旧协议测试定义、目标拒绝正确或设计审读均不证明 CR-002、完整 CR-001、GD-13、正式 UI 或发行 readiness 已完成。
