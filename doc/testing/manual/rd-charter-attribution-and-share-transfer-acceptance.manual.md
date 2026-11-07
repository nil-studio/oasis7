# 预工作 R&D charter、贡献归因与份额转让验收手册

- Owner role：`qa_engineer`；协作：gameplay、Runtime、Agent、治理/P2P、工业许可与 Viewer owner；日期：2026-11-05。
- 适用政策：[personal-rd-charter-v1](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-charter-policy-v1)；专业合同：[R&D 执行设计](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md)。本手册消费原政策，不另定人数、窗口、经济向量或管辖。
- Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。许可消费另接 Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47；[GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)。RD01–RD07保留原R&D义务与v1兼容，RD08–RD10覆盖后继组织/多方、开放epoch与完整编码；完整CR003还须联合[情境声誉整项方法](contextual-reputation-acceptance.manual.md#rep13-composition)及其直接来源/消费者，不以一项代签整卡。
- 状态：目标验证计划，未执行。计量与完整wire静态审读基线为 `ebd9b8431f2dc084c4ef6be3bcea9ca054ab2a81`；原首版 `c3862d479ba835bddde3539e8e8e97c5cb563166` 为历史；静态复算、接口设计、测试实现和实际运行分别记录，不把本手册中的逻辑操作名当作已有 HTTP、Action、WASM ABI 或 fixture。

## 执行边界与同候选准备

沿[总测试手册](../../../testing-manual.md)选择实际层级。普通 CI 先验证 codec、签名/版本、确定性规则、原子性、费用及持久故障；实际来源、治理/随机、R&D→工业许可、恢复及正式三入口使用同候选组合。真实模型、游戏、部署与真人任务只在已有许可和预算的环境执行。缺实现、来源、合法授权或安全故障注入时记 `blocked/not_run`，不得临时伪造 receipt 或免费旁路取得绿色结论。

每案固定 source/build、module artifact/manifest、政策/schema、world/genesis/branch、RD/许可 instance、当前身份/授权、fee schedule 和测试开始/结束状态。实施方登记真实入口与 fixture，QA 实际发现并核对其断言；示例字段、零测试或仅 HTTP 200 不算通过。只使用本案隔离世界、临时主体和获准材料，不修改他人会话或线上记录。

共同 fixture 使用真实有权主体与独立执行身份；原收益槽可取 `5000/3000/2000`，另有明确零收益贡献方。需要新同意的来源权利人和两个 instance 的实际费用主体均纳入必需方，不能默认贡献者、caller、artifact owner 与 payer 相同。每项源工作使用候选已支持的真实 producer，产生可回读的开始/结果/退出及使用权原件；合成字符串只用于解码负例。工业消费者另具其合法 lineage/admission、正式生产、许可和资金条件。

统一 oracle 是原权威提交、完整原件及前后状态：条款/接受/nonce，工作与来源，notice/案件/随机，rights/pool head，费用 hold/actual，累计收入 `E`、各 slot 已付/未付及实际到账。每项核对原对象、版本和来源，不只看哈希相等。确定未提交才断言无效果；提交未知保留原件和正确 writer fence，不能强行把余额差记成零。原件及签署材料按原权限保护；公开记录仅保留可关联的脱敏身份、次数、范围和结论。

<a id="rd01-activation"></a>
## RD01：同版接受、激活与开工顺序

承接 [AC-RD-01](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-01)；目标：[激活与首次工作](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-charter-activation)。Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

**输入与刺激。** 用共同 fixture 的 scope、角色/工作单元、精确来源、费用和 RD 管辖取得只读 quote `Q`；各必需方接受相同条款。实际提交激活得到 `T`，再通过源工作消费者开工。分别保留同 tick 不同 canonical 顺序，以及全部时间边界的前一 tick、边界 tick；checked 运算溢出作独立负例。

| 检查 | 可判定预期 |
| --- | --- |
| 非绑定阶段 | 重复预览、比较、拒绝、改草稿和未齐接受，合作 slot、资源、预算、工业容量与资格均不改变；不声称已绑定。 |
| 完整接受 | 16 个真实必需方与第 17 方、别名/多个 Agent、缺零收益同意、漏实际费用方、需要新同意的来源方、错版本/签名/nonce 分别验证。只按真实主体去重，不能合并独立权利人；不合格激活整体拒绝。 |
| 条款与时间 | 正权重、至多 8 槽、总和 10000、互异初始 holder、32 单元及一个 lineage 均按原政策校验；零/负 bps、和不符、+1 或未知 schema 拒绝。`[Q,Q+60)` 外接受不能激活，漂移需新 quote 和完整新接受，不续用旧签名。 |
| 先约定后工作 | 在激活之前或同 tick 更早顺序生成新分析/模拟/实验/代码/训练结果，源 gate 必须拒绝；有效激活之后仍须当前工作授权、资源及有限退出合同。只读比较/非绑定提案可进行，但不能藉“草稿”生成新研究贡献。 |
| 冻结与期限 | 激活后改 scope、名册、bps 或用新 ID 洗同一原 scope/lineage 均不通过；撤回自己的后续工作资格不扣固定份额。`T+600` 不开新工作，未具有限停止/迟到 fence/原预算的 source adapter 不能开始。 |
| 旧输入 | 事后上传或不可证明时序的资料只作明确先存来源，不能补成 charter 后贡献；已有合法许可且无需新义务的来源不强造第三方接受。 |

**证据与边界。** 保存完整同版接受关联、原报价/激活/Begin receipt、canonical 顺序、拒绝差分及实际费用保护。该结果只证明事前约定与开工 gate；不证明研究成功、来源科研结论正确、工业准入或参与者合作意愿。

<a id="rd02-provenance"></a>
## RD02：贡献、使用权、Agent 任期与永久因果

承接 [AC-RD-02](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-02)；目标：[贡献来源](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-contribution-source)、[Agent provenance](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-agent-provenance)。Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

**输入与刺激。** 对候选实际声明支持的每类工作 producer，取得同一 work unit 的合法 Begin、成功/失败/安全终止及 source/grant 原件；再提交重复、错 world/branch/任期、过期授权、伪造 event ID、只有 hash 或模型自述的版本。只有解析器 fixture 的类别保持未完成消费者验证。

- 真实结果按事前角色与结果判据登记；失败、未发现新成果和未执行分别可读。重复结果不另占单元；没有原依据的工时/模型评分不得改 bps、没收份额或产生信用评价。
- 每项结果能回到当时主体、稳定 Agent、identity generation/控制任期、执行授权、module/version、工作请求及关键 receipt。实际训练/微调核原输入、过程及许可；仅调用 provider 时只证明其可验证版本/授权，未披露训练来源显示 `not_disclosed/unknown`，不能补成已许可或全局失信。
- 贡献事实、成果使用 grant、固定经济槽与个人 credential 分开读写。先存资料保留原版本、权利人及适用条件；缺本次必要许可阻断对应使用/交接。R&D panel、artifact publisher、当前 owner 或普通 passed 不能替第三方授权。
- 注入 canonical 源纠错、字段错配、重复、遗漏和缓存滞后：更正追加原因及替代关系，旧原件可追溯；不倒写接受、不改实际发生历史、不重开 `N/C`。源已变更而投影未更新时标陈旧/来源变化，不能继续报旧证明有效。
- 转让、重配置、离线、退休或报废 Agent 后重读旧贡献。原任期、训练/模块来源及结果保持；新 owner 不继承作者身份、旧收益或个人资格。归档/恢复后因果索引仍能定位原件，不能用删除历史释放当前容量。
- 分别以公开受众、原当事人、获选 panel、失效 panel、新 owner 和无权主体读取。秘密原件只在原 scope/任期授权下提供；拒绝不泄露私有正文、计数/指纹。源不可取回时保留承诺和不可核验状态，不以缓存摘要放行必要缺项。

**证据与边界。** 保存 source→work→contribution→更正/任期的受保护关系和逐 audience 输出差分。永久 provenance 是可验证因果连续性，不证明所有原件无限公开、节点永久在线、外部知识产权已裁判或模型训练资料完整。

<a id="rd03-handoff"></a>
## RD03：关闭交接与真实工业许可消费者

承接 [AC-RD-03](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-03)；目标：[冻结交接](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-distribution-handoff)、[直接消费者](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-consumers)。Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905, O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47。

**输入与刺激。** 在同一候选中完成真实源工作，保留未开始、成功、失败及结果未知单元。由实际关闭/交接入口产生原目录与 notice，再经实际工业提案、许可激活、首次建池、合法正式用量与付款消费者读取；不得手填一个 `handoff_ready` 或导入伪 pool JSON 代替生产链。许可自身的条款、准入、收费、开放与恢复同时按[许可验收手册](industrial-license-and-royalty-settlement.manual.md)验证。

| 阶段 | 可判定预期 |
| --- | --- |
| 提案与研究工作 | 有效 charter 和当时已有证据足以进行合法非绑定提案，不等待尚未产生的最终交接；若模拟/试点生成本 scope 新研究结果，则必须先过 RD Begin，不能倒签。 |
| 关闭与 notice | 未开始单元记未执行；已开始的未知结果先恢复原件，不以超时签无效果。提前关闭须全体同意；窗口结束停止新开工且不因一方离线无限续期。完整目录、原向量、不完整项和 notice 同根，`N` 为首次 canonical 关闭提交，不以 ACK/首读/修订重算。 |
| 交接判定 | 核齐全部来源、原槽主体、原案/更正及当前 rights head；通知窗口结束且必要条件成立才可对精确成果交接。仅署名争点不阻断无关合法成果；必要身份/权利未明则阻断本次交接，不能删争议槽后重分比例。 |
| 明确受阻后迟到首次交接 | 构造同一成果必要来源/lineage 确定不足，记 `handoff_blocked`；工作、原案和费用均明确结清后退未用预算并释放四份 charter 中的原工作槽。后来合法前置补齐，无当前真实 payer 的新明示 cap 则拒绝首次交接；具备新同意与完整 cap 后，按 RD06 的有界原子迁移交接一次。不复活已退预算、不重开工作/T/N/C/原案、不改原 scope/名册/分配/E。原提交、工作或费用 unknown 不能冒充已关闭；首次交接已发布或未知时只恢复原业务键，不新发一份。 |
| 许可激活 | 真实许可入口核验 exact charter/revision、合法 lineage、原分配及当前交接；以 artifact owner、Cargo license、普通治理 passed、自由 claim、错来源或撤销 grant 替换均拒绝。RD 原件不代签工业准入、设施、正式生产和收费 adapter。 |
| 首次建池 | 真实许可 instance 只建该原分配/lineage 的唯一池，读取当前 RD rights head 并做相应版本校验。未收费时不显示已有收入；建池费用与独立 closure 须按许可原合同已足额保护。先转后建及竞争另见 RD04。 |
| 已有池收入与付款 | 通过真实合格 fee unit 增加原池 `E`，依法完成窗口/程序后读取并支付原槽当前权益。新 charter/revision、另一 license/root 或 holder 不能重开池、从零重算 `E`、刷新硬开放或对旧无绑定 receipt 补费。 |
| 全入口与完整闭包 | 沿实际调用图检查 API、Agent、自动消费者、模块与付款共同 current gate。RD 与这一许可 instance 各至多一次完整 invoke；原子 native apply、费用和 receipt 均纳入。必要第三个可变 instance、第三次 invoke、缺 current reader 或未列收费使当前 profile 拒绝，不能异步补半份或称“查询”免计。 |

重复原请求、换 request 重放相同业务 effect、准备失败、提交后丢 ACK 分别取证；只有原 committed 结果能确认激活/建池/收入/付款，unknown 先恢复原关联。PPE 的技术发布另外核其当前权限，不能由发布事实倒推贡献和经济归属。

**证据与边界。** 形成实际 `source/work → close/notice → allocation/rights → license activation → pool → fee unit/E → payment receipt` 关系，记录各阶段 current revision、commit、实际调用/费用及状态差分。source validator 或单 handler 通过不证明该链；缺任一消费者明确保留缺段，不宣称可收费、已到账或完整产业可用。

<a id="rd04-transfer"></a>
## RD04：整槽转让、同根付款与历史当事人

承接 [AC-RD-04](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-04)；目标：[整槽转让](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-share-transfer)。Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905, O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47。

**输入与刺激。** 原合法 holder 与受让人接受同一当前报价，包含原 slot/bps、完整转让链、已付/未付、程序风险、收款授权和真实费用。分别使用无池、已有池及付款已 dispatch/结果未知的状态；已有池通过真实许可收入达到所测余额，不能仅修改 R&D head 冒充双消费者运行。

| 用例 | 可判定预期 |
| --- | --- |
| 基本转让与不支持项 | 原合法 holder/新自然主体同版接受后整槽生效；Agent owner、错 holder、缺同意/来源、拆 bps、受让人已持同分配另一槽、附价款/条件或过期报价拒绝。受让人不成为历史贡献者，不继承个人 credential、维护/赔偿等原义务。 |
| 双 instance 同根 | RD rights head 与已有许可池的同槽 holder/version、程序增量、费用及 receipt 在一个 canonical commit 同时变化。任一验证/资金/写入失败时没有半份转让；一侧 ACK 丢失只回查同一提交，不靠第二次更新“修复”。 |
| 先转后建池 | 无池时只推进 RD rights head；随后真实建池使用该当前 head，初始 holder 不复活。交错建池先提交/转让先提交两种顺序，后者重验版本；不能得到两个池或两名当前 holder。 |
| 付款与转让竞争 | 两候选均从旧版本准备。付款先提交则旧人合法已付保持，转让只移动余款/未来；转让先提交则旧 holder 的新付款候选失效。付款读取同一权利版本，不能把旧快照收款人沿用到新 head。 |
| 未知付款与重放 | 原付款已发送但回包丢失时，转让不得改 recipient/nonce 再发；原提交 pending/unknown、查询失败或本地 cache miss 不推断未付。查明 committed 后只记一次原到账，再处理剩余；同键重复/异载荷及换键重放均不复制效果。 |
| 历史程序主体 | 对每个仍有效许可 notice/case 计算原有权方、仍持对应权益的历次 holder 与新 holder 的并集。构造 9→10 与 10→11 人：前者仍须完整槽/费用/原期限成立，后者在生效前拒绝；不以 current holder 替换历史人、不因关联控制合并独立权利。 |
| 程序及独立性 | 新 actor/phase 的真实费用增量另足额保护，不挪原方额度。新 holder 使已选 panel 冲突且原程序不能保持机会时拒绝该转让，不重抽、不改管辖/截止。R&D 冻结名册和旧已登记主张不变，继受不重开其窗口。 |
| 收款授权与权益账 | 同一主体更换收款 Agent 仅改授权；无合法收款路径形成该 holder 的个人应付，不返 payer、不转给新 Agent owner、不清零或复制 paid。读回完整旧 paid recipient、当前 payable 和未知 intent。 |
| 客观主体更正与已有转让 | 分别在转让前后更正有权原来源：只改历史署名不动 rights；影响收款主体/授权映射时，核完整转让 prefix，按同一 rights-version、付款 fence 和现存 pool 同根更新。纠正初始别名不能用 `initial_holder` 覆盖合法 current holder；缺当前主体证明时先标来源变化并阻断该槽新付款，不任择旧人/新人。未知付款先恢复，旧 paid、bps、E、尾差不改；无池时后续建池仍读更正后的 current head。 |

**整数连续性 oracle。** 用原 `5000/3000/2000` 向量及真实合法收入达到 `E=7`，基础应得为 `3/2/1`，第一槽已合法付旧人 2。整槽转让后再以真实合格 fee unit 达到 `E=11`；第一槽累计基础为 5，新 holder 的基础应得为 3。原许可的全部闭合条件成立后，该原槽再取得 1 尾差；最终旧人已付 2、新人 4、其他两槽 3/2，总额 11。重复结清或新许可不再分尾差，转让前后原 slot/bps、`E`、批次与已付历史不变。此算例为数值 oracle，不是现有实测。

**证据与边界。** 保存每次两端 head/版本、原签署与付款关联、完整历史当事集合和逐源费用，按两种 canonical 顺序核实际到账。没有真实许可建池/付款消费者就保持 RD04 未通过；不外推拆槽、合槽、附价交易、任意第三 instance 或一般身份转移。

<a id="rd05-review"></a>
## RD05：受权归因程序、一次申诉与有限尾部

承接 [AC-RD-05](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-05)；目标：[归因复核](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-review)。Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

**输入与刺激。** 使用原世界真实授予的 `rd-charter.review`、固定 region/事项/资产处分范围、合法名册及随机 producer。以普通 passed、管理员、charter 自授权、服务 panel 或 `industrial-license.review` 替代均不得激活。验证名册 64/+1、可证明无冲突候选 6/5、重复 controller、相关批准者、当事/关联/经济关系及未知冲突证明；没有真实 producer 时只能称验证规则 fixture。

先固定原案承诺，再消费事前不可挑选随机源的首个合格结果。初审 3 人、复核另 3 人，各至少 2 人针对同一冻结输入形成相同合法类型结论；修改 nonce、重开案、补位或转让均不重抽，失格只沿原排列顺延。两阶段误用同人、错 phase/root、越权或重复票不得成为结论。

| 时间/程序刺激 | 可判定预期 |
| --- | --- |
| 登记边界 | 原通知 `N`、`C=N+120`；在 `[N,C)` 的 canonical 登记才入窗。迟读、排队、客户端发送及 ACK 不改 N；C 上首次登记不得补旧 tick。一个 charter/notice 一案，后到合格争点合并原案而非另开一轮。C 后证据/申诉不能扩成未登记的新经济争点，署名问题不能临时升级为份额重分。 |
| 普通证据与初审 | 每方至多 16 个不同权威引用，重复恢复原结果，+1 拒绝且不挪他方/复核槽。`C+60` 前的完整合格集合冻结；初审在冻结后且 `C+120` 前发布，实际提交 tick 为 P。决定性 receipt 错配/重复/授权错误先重验完整原件，不交多数票改真值。 |
| 一次申诉 | 只接 `[P,min(P+60,C+180))` 内的合格程序主张，每方一次及至多 16 个引用；权限、遗漏证据、冲突、程序越界/可验证错误分别测试。补证不重置 N/C/P，先到者不能提前冻结他方尚有的窗口。 |
| 复核与初审效力 | `C+180` 冻结全部合格复核输入，在 `[C+180,C+240)` 形成复核；合法初审无合格申诉，在原申诉窗结束后可 final。有申诉但没有有效替代复核时，初审按原规则 final；申诉本身不撤销它。 |
| 无初审、无复核 | 完整原件能证明没有有效初审时，不编造 P/结论或开启新申诉窗；候选不足/未出结论保留缺口。到硬尾部按完整结果处理中性退出，不用管理员补裁；有初审而无有效复核按上行处理。 |
| 硬尾部与未知 | `C+300` 查齐原案、初审/复核、和解、final 与账本。有权 final/合法同版和解继续执行；已证明无有效改变原约定的结论才解除本案程序保全、按已验证原条款处理并释放未用费用。原件/提交结果未知走恢复，不能把未知当无裁决、自动退款或重复开窗。final 与硬尾部只有一个原 case resolution effect；交错/重复提交、丢 ACK 重启后仍只有一次处分/结清/释放，已 final 且结清时硬尾部只读返回原结果。 |
| 允许处分 | 只接受专业合同的确认基线、事实追加更正、按原接受纠正主体映射、来源未成立及解除错误保全。改 bps、追扣合法已付、补偿/罚款、创许可、转身份或判一般失信均拒绝。 |
| 最小保全 | 只署名纠正不阻断独立合法成果；必要来源缺失精确阻断该交接，未付槽争议只保护对应范围。不冻结全部产业、无关许可/root/资产或另一 charter；来源 blocker 不是可无限续期的申诉 hold。 |
| 两程序分工 | 将科研贡献争点投许可案、将 fee unit/数量/付款/开放争点投 RD 案，返回精确原管辖/对象/截止。转介不算登记、不增加期限/保全；两程序不能各裁一次同一争点或重开已关闭窗口。 |

**证据与边界。** 对各边界前/恰好边界实际推进 canonical tick，保留原输入全集/count、名册资格、随机证明/排列、投票 root、结论及精确保全差分。无结论的完整证明与无法取得结论分别留证。该计划不证明真实治理已激活、随机源已合格、外部权利争议已裁判或现实响应时限。

<a id="rd06-recovery"></a>
## RD06：真实费用、容量、旧预算与持久恢复

承接 [AC-RD-06](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-06)；目标：[资源保护](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-resources)、[组合上界](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-capacity)、[原请求恢复](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-recovery)。Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905, O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47。

### 费用与完整闭包

在候选实际路径核[计量 payer](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)与[费用算法](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)：当前 payer 取存在的 artifact owner；这不是 caller 或新 holder 的默认扣款资格。基线缺 owner 时有不计费返回路径，本 profile 不得借此免费激活。下面只复算算法与目标限额，尚不证明 hold、双 instance 或真实扣费已经实现。

名册大小在此记 `R`，避免与通知 `N` 混淆。`m≤16、R≤64` 时，完整程序 `K=34m+4R+5=805`，包含各方受理/两阶段证据/程序主张及名册投票、资格处理、随机、冻结和结论。正常迁移 83，独立 closure 128，总计 1016 次；closure 原对象清单按专业合同复核，不把同一已完成 effect 再做一次凑次数。

| 静态预算 oracle | Data | Electricity |
| --- | ---: | ---: |
| 一次完整 invoke：入/出上界各 262144 bytes，1 effect、1 emit，含 new_state | 515 | 4 |
| 一个逻辑迁移至多两次完整 invoke | 1030 | 8 |
| 正常与完整程序：`(83+805)×(1030,8)` | 914640 | 7104 |
| 独立 closure：`128×(1030,8)` | 131840 | 1024 |
| 一份 charter/epoch 合计：`1016×(1030,8)` | 1046480 | 8128 |

两次 invoke 分别核当前合法 payer、全部 input/state/proof、实际 output/effect/emit/new_state、native apply、commit、notice 和 fee schedule；不以零 effect planner 漏掉制度写入成本。额外合法费用/component 访问须进入 signed cap，最小 profile 未证明无遗漏收费则不能激活。转让/客观更正/迟到首次交接自身最多 `(1030,8)`，许可新增历史当事机会的费用须另按其原公式足额预留。

- 测同 payer 合计、两个独立 payer、任一 Data/Electricity 缺 1、错误 payer/缺签、owner 或费率漂移。逐 source/resource/purpose 对账；不得用未来版税、贡献款、panel 私产或未签主体补差。换 owner 不继受旧 payer 同意。
- 分别保护正常/程序与 closure；初审不能吃复核，一方无效/重复流量不能耗尽他方。把生产、维护、转移、模块计量等所有实际扣款入口与 hold 并发，保护款不可被消费；preflight 又必须能读取正确 hold，actual 不在 hold 外双扣。确定拒绝/无变化/同原件恢复不重复业务费用。
- 费用算法的条件算例取12次成功逻辑迁移、每次两invoke、每invoke实际入/出各1024bytes、1effect/1emit且有new_state；它不证明16方/512证据完整shape能压成1024bytes，真实有界编码另按RD10。若真实受控用例满足这些条件，期望 24 次合计 `120 Data/96 Electricity`，未用 RD 预算返还 `1046360/8032`；原研究 producer 的生产/模型/停止成本另外核算。该示例不是已运行结果，不同 payer 必须逐源拆分。
- 最后一次释放须先保留并支付自身实际费用；无合法收款路径保留原个人权益/恢复，不以有费轮询吃空旧款。旧合法条款、费率/执行版本、程序和 closure 必须能 drain；升级/停用只停止新承诺，不能削减旧预算、强迫换签或让旧已付回滚。
- 对 RD03 的 `handoff_blocked` 终态，先核原工作/案/费用确已结清、逐源余款已退及工作槽已释放，再补齐同一成果前置。无新 cap、错 payer/缺签或资金不足时不交接、不重新占用旧预算；合法新 cap 覆盖当前完整闭包，最多两 invoke，actual/余量/receipt 同根且逐源对账。迟到首次交接与转让/更正共用最多 8 个待决权利写上界；用提交前失败、提交后丢 ACK、重复及换键重放核一次业务效果。不新增研究或重开 T/N/C，不变原分配/E。原关闭或费用释放 unknown 不能先退 hold、腾工作槽；迟到首次交接 unknown 保留该次新预算及权利写 fence，先恢复原请求，不用另一新 cap 重做。

### 容量与集中输入

原128+64+48+16KiB的raw相加不足以证明256KiB完整输入，现按[修正wire分配](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-capacity)及RD10检验：current body96KiB、普通已编码action24KiB、完整ctx/outer15KiB；下一epoch先有≤8KiB compact head，已编码activation action≤112KiB。ModuleCommandEnvelope.payload与ModuleCallInput.action/state的嵌套Vec整数数组均实际encode，不能漏一层。输出完整new_state/effect/emit/outer合计亦≤256KiB；签署/证明子上限不保证能同时用满。

每案仍完整16方×2阶段×16证据=512项、名册64、工作32、收益槽8；raw96KiB内分额为48/12/4/8/2/16KiB加6KiB余量。第511项后，最后一方最后一个合法槽仍有完整空间和费用。原terms/render、接受/mandate、原256byte引用/正文和旧epoch仍按原audience可取回；不能截断源证明、少计epoch或只留不可解析hash来达标。当前body仅紧凑索引，不把四份current case或无限历史塞进一份invoke。

closure128次的实际对象清单为原108项加一次CompactClosedEpoch共109，仍有19次保守余量；此调用包含原件可回读核定、compact head、最后实际费用/释放及receipt，不是免费第三invoke。预算或source超限只能在新承诺前拒绝，已受理完整程序与原有权退出必须保留。

同 world 4 个未闭合 charter、8 个待决权利写及各自 +1；未知提交/未完程序/费用不腾槽。将整个剩余正常/程序集中输入，核 `4×888+8=3560≤64×64=4096`；独立 closure 核 `4×128=512≤64×16=1024`。在上述最大输入/输出、两 invoke、真实写集/持久化及无效流量干扰下，验证专业合同每可服务 tick 的 64 个有界批和分离的 closure 能力；窗口平均速率或静态乘法不证明此能力。背压只表示未受理，不替未 canonical 登记者保留旧 tick；新可选工作不能挤占已接受程序/退出容量。

### 原件、故障与 G1

1. 在激活、Begin/结果、关闭/notice、随机/排列、证据冻结/final、费用释放、转让/建池/付款的 prepare、commit、publish/ACK 和 checkpoint 边界分别故障。用持久 checkpoint 加非空日志重开，与同一权威顺序的不间断运行比较条款、结果、随机、窗口、双 head、权益、费用和 receipt；只内存重试不算持久恢复。
2. 按 [G1 writer fence](../../world-runtime/runtime/runtime-storage-footprint-governance.design.md#storage-generation-g1-design) 分开注入三类故障。只有客户端 ACK、原业务请求或某受权投影未知时，维持原请求/受影响 slot 的付款和转让 fence，查询精确结果，不误称整个 World 的 durable 提交未知。确定未提交才断言无效果；已确认提交后索引/镜像/维护失败不能回退旧 World。实际 durable generation=`StatusUnknown` 时，整个不确定旧 World writer 的 mutator、save 及依赖派发全部停止；尝试另一 slot、另一 charter 和无关业务写也必须拒绝，不得靠换请求/实例继续。仅在核清 exact durable candidate 的持久决定、内存与索引对齐并满足原当前执行资格后恢复；不能按另一健康候选推断原提交失败。两 instance 的不同 ACK 顺序以同一 canonical commit 对账，不新增恢复协议。
3. 丢失/损坏原签署 payload、nonce、source/grant、Begin/结果、notice、名册/随机、final、rights/pool 索引、费用 hold/manifest 或历史 schema，必须保留精确不可核验/恢复边界；不能补零/空数组、新 N、新 holder 或当前代码输出来“修复”。重建索引须核完整原件闭包。
4. 同请求同内容回原结果、异内容拒绝；换 request 重放同激活、work unit、notice/case、转让/付款 effect 仍至多一次。未知先查询原 correlation，只有原协议允许才重送相同原件，不重签新 nonce；本地 cache miss/HTTP 404 不替代权威结果。
5. 恢复不重调模型、重做实验、重抽随机或延长窗口。当前执行版本/权限仍需有效，不能借 replay 复活撤销代码或 grant；不支持的旧 schema/旧组织 charter 保留原含义，不自动迁成 R&D 或追溯收费。原请求/权益读取不要求另交一笔世界费用。

**证据与边界。** 保存每 payer 的初始/hold/actual/释放/余量、完整调用图与两 instance 同 commit 差分、最大输入实际字节/处理结果、故障点及重启读回。分别给出“算法复算”“实现合同测试”“持久组合”“实际性能”结论；缺任何层不以其他层代签。这里不证明经济可承受、吞吐已达标、真实掉电或跨故障域可用性。

<a id="rd07-reading"></a>
## RD07：Viewer、API 与 Agent 的同义只读体验

承接 [AC-RD-07](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-07)；消费政策 §7 和 [直接消费者](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-consumers)。Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905, O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47。

**输入与刺激。** 使用前述实际案例的同一权威快照/来源版本与同一 audience，分别读取正式 Viewer、API 和 Agent 观察。覆盖未激活、已开工、无发现、部分来源未明、交接受阻、未有收入、已有 E/未付、实际到账、转让后、程序中、硬尾部及恢复未知。入口不存在就标未覆盖，不用 mock UI 或另一入口截图代签。

| 玩家任务/负例 | 可观察 oracle |
| --- | --- |
| 决定是否参加 | 能找到原 scope/角色、零收益选择、固定 bps、来源、真实费用保留及各阶段期限；可比较继续合作、开工前缩 scope、有权先存来源、独立研究、零费公开或停止的约束。零费不等于第三方材料/设施可任用，预留不等于已花费或收益。 |
| 解释现有结果 | 三入口分别区分贡献获证、使用权成立、具备许可前置、预计收益、已赚取、可分配和已到账。相同 Agent/当前 holder 不等于作者或个人 credential；研究失败不自动扣份额/变坏信用。 |
| 处理转让及期限 | 能辨认原 slot/bps、当前 holder、旧合法已付、当前个人应付和原程序风险；离线后原 N/C/P 不重置。拆槽、合槽、附价款等不支持项有准确原因，不能用可点按钮暗示能完成。 |
| 面对待决和错误 | 来源缺失、权限失效、原提交未知、无合格 panel、没有合法收款路径分别给出适用的补证/原件查询/等待/授权入口。既有合法成果和无关许可可读可继续；不能将局部 blocker 显示为全产业冻结或成功。 |
| 同义与只读 | 重复检索、比较、预览、翻页、重连、切换三入口和读取拒绝前后，tick、工作/收益/许可、hold/费用、nonce/案窗口和业务事件不因读取改变。不同 snapshot 不伪装同一当前结果；权限不足/未加载不显示零收益或无争议。 |
| 受众与隐私 | 公开、当事人、当期获选 panel、前任/新 owner 和撤权者分别核对。只读也重验当前权限；公开界面、API、日志/截图不泄露前任私有目标、训练原件、密钥或不必要个人材料。永久因果可定位不等于无限公开正文。 |

在获准的玩家任务走查中，分别让贡献方（含零收益方）、受让方和争议当事人完成“是否参加、能否用此成果、为何这笔尚未到账、何时/去哪里提出原范围问题、现在能否安全继续”的任务；不给答案式逐步提示。记录实际人数、经验、提示、误判、放弃点与修复后复测条件；便利样本和纸面走查不外推普遍可玩性或经济接受度。

**证据与边界。** 保存同版本三入口的受权输出/交互、原世界结果关联、只读前后差分及玩家任务原始观察。界面出现绿色、阅读了条款或模型称已理解都不证明贡献权、真实到账或治理已启用；静态设计、计算样例与本手册本身均不证明正式发行 readiness。

<a id="rd08-organizations"></a>
## RD08：组织、多方与当前代表的完整形成

承接 [AC-RD-08](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-08)及[激活共同合同](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-charter-activation)。固定同候选真实ordinary组织登记/签署附件、原完整批准/当前mandate、自然与组织principal、个人Agent/资产source owner、全部实际费用payer和RD管辖/随机。

**正向。** 组织O与自然P/Q合作；O实际使用个人S的Agent或资源时，S是另一个必需接受主体。加入真实当前holder及两个费用payer，按集合并集形成≤16的完整必需方；可以一主体兼多角色，但组织O与自然S不能合并。每个零收益方明确接受0，原8槽和sum10000不变。先完整接受、实际Activate，再经真实source Begin/结果；所有本金/费用与当前授权由原原件独立核对，不能仅用common validator的accepted标志。

**完整签署与权限。** 对16必需principal正例及第17个来源owner反例，分别检验组织多批准人门槛、mandate的scope/来源/额度/任期、原当前controller及source owner单独同意。撤一份必需签署、仅持组织名/成员/普通passed/OC grant、代表过期/被撤、签后换scope/源/费用、用新代表复用旧个人接受均不得激活。common≤32不能放宽本案≤16；别名/Agent多开不扩容，也不能为凑16删掉独立权利人。

**无环摘要。** 独立从完整family terms body按规定排除自身派生ID/common摘要/receipts后计算family digest；再计算含完整body的common terms/render摘要和acceptances/request。逐层改一个字段必须破签或拒绝；不能只比被测输出两个digest相等。真实producer的activation receipt绑定原两摘要而不是逆向嵌入terms。旧v1按原字节/版本可读，不后填新接受。

**责任与历史。** 在Begin之前和真实结果之后分别让O代表A换B、个人S的Agent换owner、某角色授权收缩。新动作须当前mandate，旧结果仍引用当时O/P/S、Agent/控制和工作任期；B不变旧作者，O不获得S个人credential，S的资产同意不等于有权代表O全部事务。原承担与后续授权分别显示，旧合法义务不因撤代理消失。

**预算/调用。** 以原IA usage/domain obligations/fees独立核D已花、O残余业务/风险、C未来执行/程序/closure；退款不重置D，换代表不新建额度账。完整RD+许可真实调用图最多两invoke，common prepare在原RD调用内，源nonce/费用仍按原Kernel同根；隐藏第三mutable formation/资金模块或未计费读取使该组合失败。至少一个实际组织+个人真实source组合必须正向成立，不能全拒后声称组织已覆盖。

<a id="rd09-membership-epochs"></a>
## RD09：加入、退出、未来变化与旧收益连续性

承接 [AC-RD-09](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-09)及[epoch执行合同](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-charter-membership-epochs)。每个后继epoch分别完成原RD01–RD07适用方法；不是仅检查epoch_count字段。

1. 正向epoch1使用O/P、7000/3000、真实32以内工作及完整原来源。工作结束、原notice/程序与费用按规则闭合；从正式受权读取路径实际取回完整terms/render/全部接受、mandate、工作、证据、随机/原结论、费用/rights/receipt，后续compact head仍能定位这些原件。
2. epoch2让Q以明确零收益角色加入新工作，P退出未来工作而保留3000和旧贡献。P仍作为当前holder明确接受这次新的未来工作安排；O由当前受权代表B签署。Q的新接受及其源/费用完整，任何人旧接受、无响应或原签约都不能代替。作用范围、工作角色/来源可改未来项，旧epoch的真实贡献/原经济向量不变。
3. 独立分支先由P依原完整转让把3000交Q，实际RD/许可rights同根、旧已付保留，再由Q作为current holder接受epoch2。比较前后原slot/bps/E/已付/未付、同一pool和当前holder；Q不成旧作者，P旧个人责任不移交。转让/新激活并发分别交换canonical顺序，后者须重验当前head，不能两份旧签名都生效。
4. 活跃epoch中P撤未来工作授权，自己的新Begin立即不再可用且本epoch不能借换代表/nonce恢复，无需全体同意；已接受work沿原source安全结束，收益槽不自动没收。试图插入Q、改当前scope或强用退出者资产要拒绝；新阶段合法加入不得因此被永远误报为“不支持”。新epoch缺任一必需holder/source owner/payer，或union成为17方，拒绝新承诺且旧权不动。
5. 对前epoch已过窗口但source结果未知、合法case尚待决、关闭费用/compact提交未知、完整原件无法按原受众取回的情况，不得腾槽开启下一阶段。确定handoff_blocked且原work/case/费用都已合法闭合可以结束本阶段；未来新工作不因此获得旧缺失的许可。先前许可未结收益案只影响其真实受争范围，不无故阻断不依赖它的研究。
6. 顺序完成epoch3和4，每次有新完整当前接受、有限预算和原程序。第5次拒绝并指向历史/权益/更正/原请求，原epoch4所有证据和退出仍可完成。换charter名、Agent、module instance、组织代表、nonce或重新登记同scope不得重置计数。原v1未签四阶段不能直接升级。
7. 向量改为6000/4000、拆3000槽、合槽、有价出售或新charter把同lineage的E归零均拒绝。另一个真正独立成果仅在原工业/来源authority可证明独立lineage和新scope时另案；单独hash不同、代码重打包、引入一个新成员或旧成果后续使用不证明独立。旧池/许可/责任必须继续。
8. 归档后按public、退出的原个人、现组织代表、前代表、现Agent owner及有权panel读取原epoch；分别核原audience和当前读权。旧原件可读与新私有资料不可读同时成立；不可解析digest、失去索引后编造空历史、换代表改署名均失败。
9. 对每个新Activate与CompactClosedEpoch注入prepare/commit/publish/ACK/checkpoint故障；原请求恢复唯一。未知不跳到下一epoch，不重花旧关闭款、不再执行原模型/工作；World generation unknown时连无关写/tick也fence。对原许可交接/付款、情境声誉的真实消费者确认新epoch是原来源追加，不能新建收益池或取得新个人credential/acquisition单位。

最后核对四阶段总正常/程序/closure最多4064逻辑迁移、4185920 Data/32512 Electricity的保守费用界，逐阶段真实payer分别接受和保护；这不是一次向第1期预扣所有未来款，也不代表实际支出。每world同时4个active charter/epoch与8个权利写的并发界不因四阶段变16个active；旧历史/权益不以活动容量已满而删除。

<a id="rd10-encoded-capacity"></a>
## RD10：完整canonical封套、512证据与真实闭合预算

承接 [AC-RD-10](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#ac-rd-10)、[完整容量](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-capacity)。这项同时要求**结构可编码**和真实候选可消费，分层报告，不能把静态字节算术变业务通过。

### 独立编码与负例

按[原ABI](../../../crates/oasis7_wasm_abi/src/lib.rs)和[canonical map排序](../../../crates/oasis7_wasm_abi/src/canonical_cbor.rs)，ModuleCommandEnvelope.payload、ModuleCallInput.action/state、ModuleOutput.new_state均按其真实Vec整数数组编码；新的RD compact body内部32byte承诺按明确byte string编码。禁止把host数组当成byte string省一半，禁止遗漏命令payload嵌套、签名、render、current proof、ctx/limits/version或输出new_state。

| 检查输入 | 独立判据 |
| --- | --- |
| 普通完整形状 | raw current body≤96KiB，已经编码的action≤24KiB，完整ctx/outer≤15KiB；最坏261130bytes<262144 |
| 下一epoch完整激活 | 前closed head≤8KiB，已编码action≤112KiB，ctx/outer≤15KiB；最坏同为261130bytes |
| 全输出 | new_state raw≤96KiB、已编码effect≤32KiB、emit≤16KiB、其余outer≤15KiB；最坏261125bytes |
| payload第二层 | 普通payload≤11KiB、激活≤55KiB、命令外层≤1KiB，经A(payload)后仍分别小于24/112KiB；以实际完整encode为最终判据 |
| 旧错误上限 | 128KiB的0xFF raw state按整数数组单项已262149bytes，尚无ctx/action也必须超限；不得采用旧raw相加 |
| 最后合法证据 | 16方两phase各16、完整512项/64名册/32work/8slots及所有必要元数据≤96KiB；第512个合法槽不能因前511项用尽空间而拒绝 |
| 超限 | raw/state/component/完整action/ctx/完整input/output及write-set128任一+1，都在新的承诺/效果前拒绝；已有程序/原件与关闭预算不删除 |

下面是仅供独立复算的编码形状向量，使用确定性合成承诺/证明和签名字节，**不验证真实性、业务合法性或正式ABI已实现**。它覆盖16方各16批准签名及完整512证据数量，输出body55271、普通action8608/input121923/output104366、closed head1211、activation payload45792/action90902/input183759bytes。普通完整input的SHA256为baddda1d083825ba60a63c2401dd26e4b67c9173d8da1bb0625308acfcbb21e2；下一epoch为92b61b9c3689f2363ae41e94e46b666245882626ee30161c0c1792f64d343387。实际候选codec须另与此嵌套结构及其自己的真实schema逐字段对照，不能向业务validator提交这些合成原件后期待成功。

```python
import hashlib,json

def head(m,n):
    if n<24:return bytes([(m<<5)|n])
    for a,b in ((24,1),(25,2),(26,4),(27,8)):
        if n < (1<<(8*b)):return bytes([(m<<5)|a])+n.to_bytes(b,"big")
    raise ValueError(n)
def cb(x):
    if x is None:return b"\xf6"
    if isinstance(x,bool):return b"\xf5" if x else b"\xf4"
    if isinstance(x,int):return head(0,x) if x>=0 else head(1,-1-x)
    if isinstance(x,bytes):return head(2,len(x))+x
    if isinstance(x,str):
        z=x.encode();return head(3,len(z))+z
    if isinstance(x,list):return head(4,len(x))+b"".join(map(cb,x))
    if isinstance(x,dict):
        kv=[(cb(k),cb(v)) for k,v in x.items()]
        kv.sort(key=lambda p:(len(p[0]),p[0]))
        return head(5,len(kv))+b"".join(a+b for a,b in kv)
    raise TypeError(type(x))
def h(k):return hashlib.sha256(str(k).encode()).digest()
state={
 0:2,1:h("charter"),2:3,
 3:[[p,phase,i,h(("e",p,phase,i)),h(("registration",p,phase,i)),65535] for p in range(16) for phase in range(2) for i in range(16)],
 4:[[i,h(("controller",i)),h(("eligible",i)),h(("vote1",i)),h(("vote2",i)),1] for i in range(64)],
 5:[[i,h(("principal",i)),h(("role",i)),h(("accept",i)),h(("authority",i)),16,16,1] for i in range(16)],
 6:[[i,h(("begin",i)),h(("result",i)),h(("source",i)),h(("actor",i)),2] for i in range(32)],
 7:[[i,1250,h(("holder",i)),h(("rights",i)),11,0] for i in range(8)],
 8:{"terms":h("terms"),"notice":h("notice"),"case":h("case"),"request":h("request"),"fee":h("fee"),"current_cut":h("cut"),
    "history":[[i,h(("terms",i)),h(("directory",i)),h(("close",i))] for i in range(3)]}
}
body=cb(state)
limits={"max_mem_bytes":16777216,"max_gas":1000000,"max_call_rate":64,"max_output_bytes":262144,"max_effects":1,"max_emits":1}
ctx={"v":"wasm-1","module_id":"rd1","trace_id":"r1","time":100,"origin":{"kind":"agent","id":"a1"},"caller":{"kind":"agent","agent_id":"a1"},"limits":limits,"stage":"module_command","world_config_hash":"f"*64,"manifest_hash":"e"*64,"journal_height":101,"module_version":"2","module_kind":"reducer","module_role":"institution"}
payload=cb({"kind":"record_evidence","charter":h("charter"),"epoch":3,"party":15,"phase":1,"ordinal":15,"source":h("source"),"proof":bytes([255])*4096})
cmd=cb({"namespace":"rd","name":"record_evidence","schema_version":2,"schema_hash":"d"*64,"payload":list(payload)})
inp=cb({"ctx":ctx,"action":list(cmd),"state":list(body)})
out_record={"new_state":list(body),"effects":[{"kind":"rd_fee","params":{"fee_ref":"f1"},"cap_ref":"cap1"}],"emits":[{"kind":"rd_result","payload":{"receipt_ref":"r1"}}],"output_bytes":0}
for _ in range(4):
 out_record["output_bytes"]=len(cb(out_record))
out=cb(out_record)
assert out_record["output_bytes"]==len(out)
compact=cb({0:2,1:h("charter"),2:3,7:state[7],8:state[8]})
activation=cb({"kind":"activate_epoch","charter":h("charter"),"epoch":4,
 "family_terms":bytes([255])*4096,"render":("条款"*400),
 "acceptances":[{"principal":h(("p",i)),"approvals":[[h(("approver",i,j)),bytes([255])*64] for j in range(16)],"nonce":i} for i in range(16)],
 "authority_proof":bytes([255])*8192,"source_proof":bytes([255])*4096})
actcmd=cb({"namespace":"rd","name":"activate_epoch","schema_version":2,"schema_hash":"d"*64,"payload":list(activation)})
actinp=cb({"ctx":ctx,"action":list(actcmd),"state":list(compact)})
assert len(body)<=96*1024 and len(cmd)<=24*1024 and len(inp)<=262144 and len(out)<=262144
assert len(compact)<=8*1024 and len(actcmd)<=112*1024 and len(actinp)<=262144
print(json.dumps({"encoding_shape_only":True,"state_body":len(body),"normal_payload":len(payload),"normal_envelope_action":len(cmd),"normal_full_input":len(inp),"full_output":len(out),"closed_head":len(compact),"activation_payload":len(activation),"activation_envelope_action":len(actcmd),"next_epoch_full_input":len(actinp),"normal_input_sha256":hashlib.sha256(inp).hexdigest(),"next_input_sha256":hashlib.sha256(actinp).hexdigest(),"upper_normal":2*96*1024+5+2*24*1024+5+15*1024,"counterexample128_ff_array":len(cb([255]*(128*1024)))},sort_keys=True))
```

### 真实候选与持续收尾

将编码形状替换为同候选真实原条款/签署/mandate/refs/工作/两阶段证据/费用/rights和各完整ctx，实际serialize并记录payload→命令action→完整input与完整output的字节/哈希/字段；使用与producer不同的计算脚本/fixture从原件重建期望，不能仅调用被测size helper。原件正文与compact引用必须完全对应且可授权读回，四阶段历史不允许只留root。

至少执行一个16必需方、512合法证据均能入窗并结束的真实源/程序案例，以及16方新接受的下一epoch激活正例。若某接受/源proof格式过大，应在新承诺前准确报出不支持的具体组合并修正合法有限格式；不能签约后删最后一方、缩减16证据或以全类型拒绝代签设计目标。source工件真实性仍按RD01–RD09核，不用这份编码向量代替。

独立逐项清点closure的32work终态、16角色、32source目录、16费用/source/purpose、8slots和4个固定最终事项，再加1次CompactClosedEpoch=109≤128；剩余19仅供原合法关闭，不能再无限造子阶段。填满普通操作/证据槽及无效读取队列后，原所有phase机会、关闭、原请求恢复仍可推进；第512证据、最后退款、compact自身实际费用不得饥饿。重复compact只恢复原receipt，未核定其提交不得释放预算或生成新epoch。

actual费用按真实完整input/output与effect/emit/new_state计算；output_bytes需与真正serialized输出一致，不能信模块自报小值逃费。保持每invoke515Data/4Electricity、每逻辑迁移最多两invoke的预留界，原108→109关闭项仍在128内。四活跃epoch加8权利请求集中输入的3560≤4096、关闭512≤1024按真实最大proof/两个invoke/写集/持久化/敌对无效流量验证；静态乘法不证明现实tick处理能力。

compact、历史保留或当前编码失败只有已证NotCommitted才断言原世界不变；Committed读回完整原件/费用和精确head；StatusUnknown fence整个旧World的写/tick/save/依赖派发并恢复原候选。回滚/停用仍须读原v1/后继历史和drain旧责任，不能以新codec无法解释为由吞原证据、款项或接受。

独立结论分别报告：静态算术/完整嵌套编码形状、真实schema与业务合同、同根两消费者/持久恢复、处理容量和真人理解。当前设计只提供规则与方法；这些范围均需实际候选证据才能标已验证。
