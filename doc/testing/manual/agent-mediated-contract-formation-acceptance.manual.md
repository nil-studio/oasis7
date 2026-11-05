# Agent 代表的原子交换合同形成验收手册

> 范围阅读：原 §1、CF01–CF08 保持个人双边 v1 的方法和历史边界。新增 §2、CF09–CF17 验证一般组织、多方、长期受托以及各原履约族的共同形成；不把旧 v1 的 unsupported 当成新版正向覆盖。

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

## 2. 一般形成的独立验证准备

核对日期与基线：2026-11-05；设计输入 ebd9b8431f2dc084c4ef6be3bcea9ca054ab2a81。本节对应[一般形成政策](../../game/gameplay/gameplay-agent-mediated-contract-formation-policy.prd.md#formation-policy-principals)及[一般形成设计](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-terms)，承接 [REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) / [AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001)。Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

先固定实际实施候选和 world/branch、family/policy/activation、条款及 renderer、普通组织来源/mandate、原 usage/source/obligation 索引、各真实费用和当前受众。对每种正向族分别记录 producer、正式入口、exact command、full encoded input/output、authority、费用 graph 与原结果；当前缺功能则该项未执行，不用人工写余额、合成 receipt、旧 native 单腿或静态 UI 补签。以下命令名是目标语义，实施 PR 指明实际入口，手册不虚构可运行 CLI。

独立 oracle 来自签署的完整义务、提交前资源、原授权上限、领域原件及权威顺序；不能用被测实现生成的 summary 再断言它自己正确。每例记录全部受影响主体/资源/hold/usage/费用/权限/原领域状态差分；原始私有 proofs 按原权限保存，公共证据只保留必要关联和结论。实际运行、持久恢复、跨节点和玩家观察分别判定。

<a id="cf-09-organization"></a>
## CF-09：普通组织签约、代表任期与资金来源

设计依据：[主体与 mandate](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-principals-mandates)。

- 正例：固定一个已存在组织、准确 charter、完整当前成员/controller snapshot；首次普通业务附件得到当前 founder、全部去重成员主体和准确治理 payload 的合法确认。附件有明确批准 roster/门槛、source/对象/累计 cap、代表任期。按该来源授予具体 mandate，由当前代表对一份实际完整合同接受，并与所有其他必需方同版激活；核对组织 principal、实际资金权利人、代表与 Agent 分别记录。
- 原资金检验：组织使用成员个人 Agent 或其他人的资源时，列出实际 source owner 的授权及本次所需接受。未有该授权时不应扣钱；组织成员身份、职位、组织受益和代表自己签名都不能替个人 source owner 同意。组织已存在 held-in-trust、restricted support 或其他 hold 时不能计入自由余额。
- 分别移除首次 founder 证明、一名成员同意、正确 charter hash、准确治理 payload；替换成任意 Passed 提案、普通成员身份、OC continuity grant 或历史角色。每个反例都应无新合同、hold、usage 或费用；保留具体 blocker。
- 后续更新：在已经有效的业务附件下更换代表/mandate，按原批准人门槛办理；修改附件/批准 roster 则用上一附件全体批准主体及准确治理更新。只提供 founder 签名应失败，证明 founder 没有永久后门；当前合法来源满足时不因旧 founder 离线而额外要求其永久批准。
- 在接受后、激活前分别改组织风险状态、charter/source revision、代表任期、批准人当前控制或必要资金授权。旧签名不能继续绑定；当前来源恢复也不自动重放旧激活。已先合法激活的责任按 CF13 保留。
- 证据：当前组织/来源/控制事实、明确接受、准确治理绑定、原 source owner、拒绝差分及唯一 activation receipt；组织名称或 UI 有“管理员”标签不算证明。

<a id="cf-10-multiparty"></a>
## CF-10：完整多方、同版正文、零业务效果与不取交集

设计依据：[共同条款](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-terms)、[必需接受](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-required-acceptances)。

- 正例：至少三个独立 principal，包含实际 provider、beneficiary、fee payer 与有必要新许可的权利人。独立从完整 obligations/effects/费用/权利清单计算必需集合；组织多代表只形成其一份 bundle，个人多 Agent 不多计主体。全部同版接受后仍保持 pending，明确 Activate 才有世界结果。
- 逐阶段读取普通聊天、草稿、offer、一方接受、部分方接受、全部接受、重连以及等待到期。对照初态验证本金、fee、hold、D/O/C、业务 slot、顺位、权限、声誉、债务和义务全部不变。协议元数据/CPU防滥用限额可变化，但不得转为玩法扣费/优先级。最后一份接受到达不自动激活，过期不会调用旧 native expiry 声誉效果。
- 缺任一必要付款人、权利人或受影响继受方，或混用 revisions/render digests、添加隐藏费用/用途/义务、删除某责任方，整份拒绝。接受 nonce 的消费只能与实际激活同根，失败或纯接受不消费。
- 完整请求反例：授权动作包含 {维护、交付} 但请求另含运输；授权对象 {F1,F2} 而请求 {F2,F3}；授权资源上限10而请求11。另组合三维均有非空交集、但各含一个未授权成员的请求；不能少执行一部分、换对象、删义务或缩量后继续。合法缩小须新条款/新修订和完整重新接受。
- 摘要独立计算：先原 family body/digest，再 common body/digest、固定双语 render bundle/digest、接受证明、完整 request digest。改变 record_version 只改变请求/CAS，不改变未修订条款；任一源/义务/payer/风险/用途改变须改变 terms/render 并使旧接受失效，禁止 family/common 互嵌派生 ID 造成循环。
- 可读性：当事人可以读完整签署义务、费用和风险；隐藏无关余额/私有证据原件不能删去必要共同条款。缺读权只可返回不可接受和真实下一步。选择中英显示不改相同 bundle 的签署内容；模型润色不能成为正文。

<a id="cf-11-mandate-exposure"></a>
## CF-11：长期授权的实际花费、业务承诺与全部费用

设计依据：[累计暴露](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-cumulative-exposure)。

以原 grant/source/object/period 为 oracle key，独立从领域单位、价格/风险 cap、实际费用和关闭机会重算 D/O/C，不相信一个聚合“剩余额度”字段。对 Electricity、Data、每种材料及适用权利各用自己的原单位；跨独立 grant 不新设总账户。

1. 固定上限10，初态全0。合同甲形成 O=4、C=2；记录同一次 nonce、hold、usage 与领域激活。
2. 合同乙要 O=4、C=1，总暴露11，拒绝且原量不变。令甲乙并发并安排两个权威顺序：至多允许先提交且有额度的完整合同，另一份拒绝；两个入口都读旧余额不能让二者成功。
3. 甲实际支付3、实际费用1后，应为 D=4、O=1、C=1，合计6。只有原剩余义务合法解除、程序/闭合已完成且无未付责任时，O/C才归0；D仍4。
4. 真实收到3退款后仍D=4，不能重获原10额度。新承诺使用剩余6以内；换 request、terms revision、Agent、代表、credential、period显示或合同ID不消除原D及未结负担。
5. 分别留下有效未付债权、unknown结果、返还账户无法解析、仍可能合法判付或未完关闭费用；即使页面显示 terminated/工作结束，也不能释放相应O/C。拆分已完成单位和重复原receipt不重记支出或释放。
6. 所有后续 writer 分别刺激：正常支付、真实材料/电力sink、业务赔付、执行费、每 actor 程序费与final close。要求原资金、对应purpose的O/C、D和同一effect receipt一次更新。一个无适配publisher或不完整source root使本范围新承诺不可执行，不能把未知量归0。

权利/风险单位若是有状态的许可占用而非货币，应以其原消费/解除证明执行同样的原范围判据；不得通过模糊风险分值相抵。授权内正常自动签约不逐次再询问owner，真正新增范围/累计承诺才需要新明确授权。

<a id="cf-12-family-handoff"></a>
## CF-12：全部已采纳族的正向形成与真实领域结果

设计依据：[family handoff](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-family-handoff)、[同根激活](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-activation)。

| 独立正向例 | 形成时验证 | 形成之后不能伪造的事实 |
| --- | --- | --- |
| 三方即时交换 | 全部主体/来源/同版接受、预先资金、所有有向腿和费用同次提交 | 单方到账、两个旧Settle、ACK不证明整体原子完成 |
| 原个人供能v1与一般组织/多方供能 | v1保留原两个人限制；一般供能使用服务族的完整单位/责任/预算/组织来源 | active不证明已经供能或价款已付 |
| 维护/维修 | 明确设施/实际维护动作与输入、接收/验收predicate、时界、承担者和真实payer | 维护意图或施工开始不证明维护完成；只有原producer的真实效果算数 |
| 物料运输 | 原cargo/ledger/数量、路线/目的地、保管/所有权里程碑、损耗/返回责任与费用 | 发出、在途、arrival、入库和商业交付分别判断；不自动返还丢失货物 |
| 一般共同工作 | 有限单位/依赖、每方具体工作与真实原receipt、独立责任及程序机会 | 聊天“合作完成”、模型输出或一方工作成功不能代表全部履行 |
| 组织/多方R&D、成员epoch和显式份额转移 | 原charter/角色/贡献来源/使用权、当期必需方完整接受和费用；同一RD executor消费common输入 | 签署不代表新贡献/成果许可/经济收益；新成员不迁走旧贡献、slot/bps/E或个人credential |

每族至少一个实际正向形成和一个晚到拒绝/来源变化；缺某族实现时单列未执行，不能用unsupported负例宣称原完整范围已验收。服务继续按 [原领域义务](../../world-runtime/runtime/continuing-service-fulfillment-and-remedies.design.md#service-obligations)和 [领域证据](../../world-runtime/runtime/continuing-service-fulfillment-and-remedies.design.md#service-domain-evidence)，R&D继续按 [原激活](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-charter-activation)与 [有限epoch](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-charter-membership-epochs)；只核本次形成与交接，履约/归因各有自己的完整验收。

即时资产单独取正例：Electricity/Data逐方向权限和共址；原direct条件成立的材料ledger；同世界native MainToken自由liquid balance及原auth/nonce。反例分别为错误Data方向、显式path伪direct、不可转让材料、token nonce重放/错链/未签token auth、vested或restricted/指定starter来源、外部OC桥到账、把credential/设施控制/R&D份额塞进即时腿。MainToken的quote fee metadata不能被当成合法新收费。

多方独立算例依设计给定P/Q/R三方及真实合法费用：P→Q 6E、Q→R 4E、R→P 2E，每腿示例交易费1E。根据实际完整输入/输出、3 effects、1 emit、new_state的计量求runtime Data/E，再逐source复算全部余额与treasury；不能照抄局部payload的计数。设计中的数值只在实际fixture确有该合法完整封套时使用，否则先按本fixture真实字节重新列出oracle。P提交前总支出聚合，不能用来自R的本次到账融资。

在所有准备已通过但安装前分别令最后一个authority、source余额、nonce、接收容量、费用payer或domain write set失败。要求原领域记录、全部资源/hold、D/O/C、接受nonce、业务赢家、event/receipt全体无部分安装；确定未提交的形成失败也不能留下module费用。提交结果未知转CF15，不当作失败。复用原family，不增加一个可变formation实例；R&D/许可原两invoke仍明确计为两次。

<a id="cf-13-revocation-succession"></a>
## CF-13：撤回、撤权、代表替换与原责任继受

设计依据：[共同恢复](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-recovery)、[退出政策](../../game/gameplay/gameplay-agent-mediated-contract-formation-policy.prd.md#formation-policy-exit)。

分别安排撤回/撤权先于激活及激活先于撤回的权威顺序。先撤回或来源失效时完整激活无效果；先合法激活时后来Stop Agent/撤销mandate/关闭页面只影响新代理行为，原义务、费用、未付责任和receipt保留。原Data grant撤销不取消已经明确授予的窄范围资金处置权，也不能被该处分权反向扩成新生产/新Data访问权限。

同一组织换代表，组织仍是原principal；新代表不继承旧签名或已耗授权，必须有当前有效角色/mandate。换principal或服务provider则依据原domain范围作显式继受，所有受影响方及新方同版接受，旧方历史D/责任和程序机会不迁走。

使用16个当前服务方加1名新主体的完整必需集合检验17方继受；另把历史未受改动责任方保留在原账，其不需被强迫重签新未来工作但仍能补证/查自己原件。测试缺一新payer、旧单位未知、合法保全、不可分工作、重叠旧新执行范围和旧新费用不同；不能先撤旧后靠后台开新，也不能洗原单位、case时界、fee累计或责任cap。

对退休、组织保护/解散/资不抵债、控制权转让与受益账户不可解析分别验证：冻结对应新风险，按原履约/救济保留历史和顺序；无法安全返还时保持hold/return_pending，不把资金释放给新owner。旧域需要先结清才能处分时，完整拒绝资产转让而非删除义务。

<a id="cf-14-bounded-cost"></a>
## CF-14：完整编码、所有调用费用及不被耗尽的退出

设计依据：[费用与容量](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-budget)。

### 完整封套而非原始payload

对实际codec独立序列化完整输入/输出，包含旧/新state、terms、proof/签名、refs、操作、family参数、返回值和外层wrapper。先用32字节全255与64字节全255作长度检查：若编码为CBOR uint8数组，完整长度分别是66和130，不是32/64；若为byte string，按真正bstr编码另算。内层已经编码为bytes后再次包为数组，还会再扩大；不以raw state上限代替调用上限。

先独立核对[实际 ABI 类型](../../../crates/oasis7_wasm_abi/src/lib.rs)和[命令 route](../../../crates/oasis7_runtime/src/world/module_runtime.rs)：typed payload 编码后进入 `ModuleCommandEnvelope.payload: Vec<u8>`，完整命令再进入 `ModuleCallInput.action: Vec<u8>`，存在**两层数组扩大**；旧state另按原Vec编码。只把typed body直接放到一个自造input字段的单层结果不是合法容量证据。以下正例采用设计明确的内部Hash32/Signature64=bstr和接受/批准tuple，host payload/action/state/new_state仍按原uint8数组，不能改host wire来凑大小。

哈希32字节、签名64字节，内容全255；这些哨兵只验证结构最大元素长度，不是合法签名、真实来源或已实现adapter。完整terms与所有512份批准签名仍在typed payload中。渲染、组织roster/mandate和来源的完整原件由所绑定exact refs解析，不能只看hash就放行；真实候选还须证明它们在同一合法root可验证、可受权取回，实际需要新增proof字节时重新序列化完整调用。正常业务正向仍由CF09/CF12的真实合法来源证明。

| 完整形状 | 必需主体 / 每组织批准签名 / 即时腿 | terms / 接受bundle / typed payload / 命令封套 bytes | 完整ModuleCallInput / ModuleOutput bytes | 判定 |
| --- | --- | --- | --- | --- |
| 小型双层形状 | 3 / 1 / 3 | 2,217 / 631 / 3,843 / 7,746 | 16,567 / 1,823 | 全部host字段与两层数组计入；实际费用按完整大小 |
| 最大接受集合形状 | 32 / 16 / 32 | 13,542 / 39,370 / 58,835 / 116,686 | 233,407 / 11,809 | 32 principal和512签名可完整编码；仍须真实权限、资金、来源及domain条件 |
| 子上限不能同时用满 | 32 / 16 / 64 | 18,998 / 39,370 / 69,747 / 138,352 | 276,576 / 18,561 | input超过262,144，完整拒绝；另有64 effects超过单节点32的拒绝 |
| 内部binary误用Vec的反例 | 32 / 16 / 32 | 19,174 / 76,234 / 104,563 / 208,144 | 416,514 / 16,161 | 保持同样tuple及host wire，只将内部binary误作数组已超界；不能沿用bstr正例 |

下列纯静态脚本完整规定该形状，可从方法正文直接独立复算；只实现这些固定输入需要的CBOR基本类型和确定map排序，不是生产codec、签名验证器或业务测试。`U8Array`明确区分host的Vec与制度内部bstr；输出的数值`output_bytes`取完整结构长度的不动点，不能装入业务payload。真实sandbox必须自己度量实际输出，不能信该字段自报。ModuleOutput的effect名称/params是目标shape示意，不能证明某effect已由Kernel采纳。字段省略规则沿ABI：此命令route无event，tick_lifecycle未设置；新实际可选字段、状态或proof须重新计量。

```python
class U8Array(bytes):
    pass

def head(major, n):
    if n < 24:
        return bytes([(major << 5) | n])
    if n < 256:
        return bytes([(major << 5) | 24, n])
    if n < 65536:
        return bytes([(major << 5) | 25]) + n.to_bytes(2, 'big')
    if n < 2**32:
        return bytes([(major << 5) | 26]) + n.to_bytes(4, 'big')
    return bytes([(major << 5) | 27]) + n.to_bytes(8, 'big')

def enc(value):
    if value is None:
        return b'\xf6'
    if isinstance(value, bool):
        return b'\xf5' if value else b'\xf4'
    if isinstance(value, int):
        return head(0, value)
    if isinstance(value, str):
        raw = value.encode()
        return head(3, len(raw)) + raw
    if isinstance(value, U8Array):
        return head(4, len(value)) + b''.join(enc(x) for x in value)
    if isinstance(value, bytes):
        return head(2, len(value)) + value
    if isinstance(value, list):
        return head(4, len(value)) + b''.join(enc(x) for x in value)
    if isinstance(value, dict):
        pairs = sorted(((enc(k), enc(v)) for k, v in value.items()),
                       key=lambda pair: (len(pair[0]), pair[0]))
        return head(5, len(pairs)) + b''.join(k + v for k, v in pairs)
    raise TypeError(type(value))

def fixture(n, signatures, leg_count, internal_arrays=False):
    h = bytes([255]) * 32
    sig = bytes([255]) * 64
    if internal_arrays:
        h, sig = U8Array(h), U8Array(sig)
    principals = [{
        'principal': ['organization', f'org-{i:02}'],
        'roles': ['provider', 'payer'], 'agent': f'agent-{i:02}',
        'source': h, 'tenure': 1, 'mandate': h,
        'cap_e': 100000, 'cap_d': 100000, 'read_scope': 'full-contract'
    } for i in range(n)]
    legs = [{
        'slot': i, 'kind': 'electricity', 'from': i % n,
        'to': (i + 1) % n, 'source': h, 'target': h,
        'amount': 10, 'permission': h
    } for i in range(leg_count)]
    terms = {
        'schema': 2, 'world': h, 'branch': h, 'genesis': h,
        'family': 'immediate_exchange_v2', 'instance': h, 'agreement': h,
        'revision': 1, 'previous': None, 'policy': h, 'activation': h,
        'manifest': h, 'schedule': h, 'quote_root': h,
        'not_before': 1, 'expires': 101, 'principals': principals,
        'required': list(range(n)), 'obligations': legs, 'dependencies': [],
        'fee_graph': [{
            'node': 0, 'artifact': h, 'instance': h, 'payer': 0,
            'schedule': h, 'input_max': 262144, 'output_max': 262144,
            'effects_max': 32, 'emits_max': 16, 'fuel_max': 1000000,
            'count': 1, 'purpose': 'instant'
        }],
        'fee_policy': h, 'resource_caps': [[i, 100000, 100000] for i in range(n)],
        'renderer': h, 'remedy': 'query-original-final-result',
        'alternative': 'decline-or-requote', 'family_body_digest': h,
        'source_projection_use': None
    }
    accepts = [[i, h, 1, h, h, h, 101,
                [[j, sig] for j in range(signatures)]] for i in range(n)]
    body = {
        'schema': 2, 'operation': 'activate', 'business': h,
        'request_id': 'r' * 128, 'requester': 0,
        'expected_record_version': 1 + n, 'terms': terms,
        'acceptances': accepts, 'source_root': h, 'usage_root': h,
        'family_body': {'legs': legs, 'fee_policy': h},
        'new_effects_expected': leg_count, 'nonce_root': h
    }
    payload = enc(body)
    command = {
        'namespace': 'oasis7.formation', 'name': 'activate',
        'schema_version': 2, 'schema_hash': 'ff' * 32,
        'payload': U8Array(payload)
    }
    old = {
        'version': 2,
        'proposal': {'business': h, 'revision': 1,
                     'terms_ref': h, 'acceptance_root': h},
        'active': [], 'usage_refs': [], 'nonce_refs': []
    }
    limits = {
        'max_mem_bytes': 67108864, 'max_gas': 1000000,
        'max_call_rate': 64, 'max_output_bytes': 262144,
        'max_effects': 32, 'max_emits': 16
    }
    ctx = {
        'v': 'wasm-1', 'module_id': 'formation-family-v2',
        'trace_id': 'r' * 128, 'time': 1,
        'origin': {'kind': 'module_command', 'id': 'r' * 128},
        'caller': {'kind': 'agent', 'agent_id': 'agent-00'},
        'limits': limits, 'stage': 'module_command',
        'world_config_hash': 'ff' * 32, 'manifest_hash': 'ff' * 32,
        'journal_height': 1, 'module_version': '2',
        'module_kind': 'reducer', 'module_role': 'mechanism'
    }
    inp = {'ctx': ctx, 'action': U8Array(enc(command)),
           'state': U8Array(enc(old))}
    state = {
        'version': 2,
        'activated': {'business': h, 'terms': h,
                      'acceptance_root': h, 'receipt_ref': h},
        'units': [[i, 'fulfilled'] for i in range(leg_count)],
        'usage_refs': [h] * n, 'nonce_refs': [h] * n
    }
    out = {
        'new_state': U8Array(enc(state)),
        'effects': [{
            'kind': 'formation.transfer',
            'params': {'slot': i, 'resource': 'electricity', 'amount': 10,
                       'from': f'agent-{i % n:02}',
                       'to': f'agent-{(i + 1) % n:02}'},
            'cap_ref': 'ff' * 32, 'cap_slot': f'leg-{i:02}'
        } for i in range(leg_count)],
        'emits': [{'kind': 'formation', 'payload': {
            'business': 'ff' * 32, 'terms': 'ff' * 32,
            'outcome': 'fulfilled', 'receipt': 'ff' * 32,
            'source_projection_use': None
        }}],
        'output_bytes': 0
    }
    while True:
        size = len(enc(out))
        if out['output_bytes'] == size:
            break
        out['output_bytes'] = size
    return {
        'terms': len(enc(terms)), 'acceptances': len(enc(accepts)),
        'payload': len(payload), 'command': len(enc(command)),
        'input': len(enc(inp)), 'output': len(enc(out)),
        'old_state': len(enc(old)), 'new_state': len(enc(state))
    }

for args in [(3, 1, 3, False), (32, 16, 32, False),
             (32, 16, 64, False), (32, 16, 32, True)]:
    print(args, fixture(*args))
print('primitive', len(enc(U8Array(bytes([255]) * 32))),
      len(enc(U8Array(bytes([255]) * 64))),
      len(enc(bytes([255]) * 32)), len(enc(bytes([255]) * 64)))
```

所有容器头、键名、内层编码和两次再包装均计入。64腿反例还须拆成合法完整调用图才能满足单节点32-effect界限，修正调用图不会自动消除输入超界；任何合法替代仍需一次权威提交、完整新报价与同意，不能分两次生效。512签名正例不意味着其他子上限也能同时取满。原先单层推演的13,506/238,739/272,691等数值不再作为ABI容量或费用依据。

### 费用与机会的独立闭包

- 枚举实际根调用图的每个invoke，包括相同module的不同ordinal、原license两调用、所有收费hook/订阅、后续真实工作、修复/返程及RD程序。common validation嵌原family，不额外算一个虚构module；host不复制institution解释器来声称0调用。每项核真实artifact owner/payer、计量schedule、完整bytes/effects/emits/fuel、实际和最大次数。
- 节点最大256KiB输入/输出、32 effects、16 emits时，上界592 Data/50 Electricity，64节点上界37,888 Data/3,200 Electricity；这是受签保守上界，实际成功按真实计量。小型双层形状input16,567/output1,823、3 effects/1 emit/new_state=true，结构计量为17+2+6+1=26 Data及1+3+1+1=6 Electricity；最大接受集合input233,407/output11,809、32 effects/1 emit/new_state=true时为228+12+64+1=305 Data/35 Electricity。不能把上界直接收费，也不能用局部payload、原单层推演或另一fixture的1KiB计数。
- 对每个payer把整个生命周期最大成功/明确可计费attempt次数、程序actor/阶段和close完整展开，独立与signed cap、D/O/C及真实可用资金比较；源相同的原调用按完整effect/ordinal去重，不把两个invoke误并。实现配置/旧数据超过已签完整envelope时应在发布前失败。
- 逐一取principal、proof数、bytes、obligation/edge/source、invoke/fuel、活跃合同/历史初次序号的边界和超一；再取嵌套子项各自合法但完整envelope超限。要求稳定拒绝、零部分业务状态，旧原件/关闭机会不被删。
- 耗尽一方普通执行或自己的证据份额，另一方及最终系统close仍有其独立预算和机会；同一人改ID不能重装机会。预算不是共享可任意消费池。真实domain close未完成，不得先退其唯一资金再说“以后补费”。
- 让普通请求占满4在途和ordinary rate，原件查询用完独立16次，再令只读long-poll占优先槽；第一次合法Stop/撤回/原授权停止仍有按原proposal/grant和revision的独立机会，可抢占自己的long-poll并经正确权威入口送达，不依赖剩余执行预算或普通查询额度。相同控制换request ID只能恢复原键，不创建无限份额。协议限额仍保护CPU/通信，不能用控制槽执行免费付款。根writer unknown时只报告真实pending/恢复，不越过fence假装已停止。
- 分清非绑定登记、激活暂存计算、active之后已明确接受的attempt费。前两者无已发布失败业务费；active后原合法attempt按operation/node ordinal一次计费，其原责任不由形成层取消。每一类的真实费用/零费用来源及后果必须各有证据。

<a id="cf-15-recovery-audience"></a>
## CF-15：原结果、根未知与当前受众

设计依据：[共同恢复](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-recovery)。

按真实候选分别注入准备中、全部准备完成但持久决定前、决定未知、已提交回复前及投影/GC后失败。确认未提交时nonce、义务、资源、usage、费用及winner全体未生效；确认提交只返回原receipt；持久决定未知则fence整个旧writer，连同所有后续mutator、世界tick推进、save及依赖派发停止，并恢复精确候选，不只冻结当前合同。

重连、换request ID、重发原payload、不同payload复用原ID、改revision、key轮换、artifact升级、停用/回滚分别核同business/nonce/effect去重。not_found_at_root不证明无在途提交；旧身份来源不可读或当前授权失效不触发重新执行。snapshot/journal恢复要包含全部来源、接受、nonce、usage、holds、unit/case/fee counters、必要原工件及当前查询权限；缺一保持不可写/明确unknown。

当前受众分别为原个人、组织有权代表、已离任代表、新owner、新继受方、原资金权利人和旁观者。读取自己原责任所需原件不授新写权，不泄露新方无关余额/私有prompt；仅持有旧合同ID不能读取全部内容。原合法资金处分与历史读权分开检查，source变化不清旧责任。

<a id="cf-16-consumers"></a>
## CF-16：同源协商、真实后果和已声明下游用途

设计依据：[共同读面](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-read)、[共同条款](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-terms)。

用同一candidate/root在实际Agent/API/Viewer核对完整签署正文、principal/代表/任期、接受来源、D/O/C可见部分、主要成本/风险、无合同替代和真实next_action。逐项区分ACK、pending acceptance、activation、work started、partial/fulfilled、responsibility final、paid/return_pending和unknown。API/Viewer可以不同布局，不能不同事实；缺正式consumer就记录未覆盖。

用源过期、缺授权、资金不足、未知原结果、无合法继受和当前读权不足分别刺激，检查呈现的是准确刷新、修改新条款、重新授权、原件查询、等待或停止，不虚构可执行按钮。可见理由沿原causal receipt关联实际世界结果；模型建议不补接受，停止建议不证明旧义务已取消。

即使交易看起来成功，普通聊天、offered、全部接受及激活都不能自行产生声誉/资格。对immediate_exchange_v2分别测试明确source_projection_use、明确null、字段缺失/非法、接受后改变policy/purpose/audience和旧v1原件。仅完全签署的合法声明及原政策认可的实际双向非零receipt可交 [source families](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-source-families)，[purpose admission](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-purpose-admission)与 [实际消费者](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-all-consumers)按其原规则只读计算。形成/支付executor不写score/资格/额外资源奖励；一份receipt不证明原主体/控制独立性，消费者仍核真实source。CR003的数值、期限、申诉和完整消费者方法由其手册另验。

<a id="cf-17-integration-compatibility"></a>
## CF-17：三个领域的组合与旧合同兼容

设计依据：[族交接](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-family-handoff)、[共同恢复](../../world-runtime/runtime/agent-mediated-contract-formation.design.md#contract-formation-common-recovery)。

选择同一组织mandate及其真实资金来源，先形成一般服务，再在该授权剩余额度内形成R&D合作；各自义务/费用仍在原domain，common引用同一原usage。服务未付责任或R&D未闭合工作仍占原暴露时，新承诺不得因切family而获新额度。形成host不复制服务归责或R&D贡献逻辑，原executor完整调用图计费且所有相应common/source关系同根。

服务继受、RD成员epoch/份额转移分别消费完整必需接受，历史工作/责任/原分配不重置；当受合法保全的原scope存在时，只处理允许的无争议范围，不令形成层替领域解冻。未来结果反馈只沿原effect lineage进入其适用声誉/用途，原accepted、控制切换或common metadata不成为新事实来源。

最后对旧native EconomicContract、owner_atomic_resource_exchange_v1及个人power v1运行其原方法和兼容oracle，保留原字段、费用、到期与历史重放；新组织/长期受托仍不能伪装成旧v1输入。未知版本、缺制度activation/合法fee/source、破坏性upgrade或丢恢复字段分别拒绝写入且保留原件/drain。设计静态链接/表格检查只证明结构，不能代签这组真实组合、节点最终性或玩家理解。
