# 预工作 R&D charter、贡献归因与份额转让验收手册

- Owner role：`qa_engineer`；协作：gameplay、Runtime、Agent、治理/P2P、工业许可与 Viewer owner；日期：2026-11-04。
- 适用政策：[personal-rd-charter-v1](../../game/gameplay/gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-charter-policy-v1)；专业合同：[R&D 执行设计](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md)。本手册消费原政策，不另定人数、窗口、经济向量或管辖。
- Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。许可消费另接 Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47；[GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)。仅证明本 R&D 切片，不代签整个产品要求。
- 状态：目标验证计划，未执行。计量源码审读基线为 `c3862d479ba835bddde3539e8e8e97c5cb563166`；静态复算、接口设计、测试实现和实际运行分别记录，不把本手册中的逻辑操作名当作已有 HTTP、Action、WASM ABI 或 fixture。

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
| 一份 charter 合计：`1016×(1030,8)` | 1046480 | 8128 |

两次 invoke 分别核当前合法 payer、全部 input/state/proof、实际 output/effect/emit/new_state、native apply、commit、notice 和 fee schedule；不以零 effect planner 漏掉制度写入成本。额外合法费用/component 访问须进入 signed cap，最小 profile 未证明无遗漏收费则不能激活。转让/客观更正/迟到首次交接自身最多 `(1030,8)`，许可新增历史当事机会的费用须另按其原公式足额预留。

- 测同 payer 合计、两个独立 payer、任一 Data/Electricity 缺 1、错误 payer/缺签、owner 或费率漂移。逐 source/resource/purpose 对账；不得用未来版税、贡献款、panel 私产或未签主体补差。换 owner 不继受旧 payer 同意。
- 分别保护正常/程序与 closure；初审不能吃复核，一方无效/重复流量不能耗尽他方。把生产、维护、转移、模块计量等所有实际扣款入口与 hold 并发，保护款不可被消费；preflight 又必须能读取正确 hold，actual 不在 hold 外双扣。确定拒绝/无变化/同原件恢复不重复业务费用。
- 用 12 次实际成功逻辑迁移、每次两 invoke、每 invoke 实际入/出各 1024 bytes、1 effect/1 emit 且有 new_state 的受控用例，期望 24 次合计 `120 Data/96 Electricity`，未用 RD 预算返还 `1046360/8032`；原研究 producer 的生产/模型/停止成本另外核算。该示例不是已运行结果，不同 payer 必须逐源拆分。
- 最后一次释放须先保留并支付自身实际费用；无合法收款路径保留原个人权益/恢复，不以有费轮询吃空旧款。旧合法条款、费率/执行版本、程序和 closure 必须能 drain；升级/停用只停止新承诺，不能削减旧预算、强迫换签或让旧已付回滚。
- 对 RD03 的 `handoff_blocked` 终态，先核原工作/案/费用确已结清、逐源余款已退及工作槽已释放，再补齐同一成果前置。无新 cap、错 payer/缺签或资金不足时不交接、不重新占用旧预算；合法新 cap 覆盖当前完整闭包，最多两 invoke，actual/余量/receipt 同根且逐源对账。迟到首次交接与转让/更正共用最多 8 个待决权利写上界；用提交前失败、提交后丢 ACK、重复及换键重放核一次业务效果。不新增研究或重开 T/N/C，不变原分配/E。原关闭或费用释放 unknown 不能先退 hold、腾工作槽；迟到首次交接 unknown 保留该次新预算及权利写 fence，先恢复原请求，不用另一新 cap 重做。

### 容量与集中输入

在实际候选组合最大 terms 32 KiB、签署命令 64 KiB、完整 proof ref 256 bytes、说明 4 KiB、分页 50 及转让/更正 128 项写集，同时测各上限 +1。每份 charter 及唯一归因案独占一个 RD instance；四份 charter 是四个 instance，不将四份 current state 或无限历史塞进一次 reducer 输入。

满额 state 的 oracle 是两阶段 16 方各 16 项、共 512 项证据的紧凑记录，每项含编码开销 ≤96 bytes，合计 48 KiB；另加名册/排列/票 12 KiB、当事/接受/phase 8 KiB、工作 16 KiB、槽/head 2 KiB、header/原件根/费用与请求 32 KiB，共 118 KiB，余量 10 KiB，总 state ≤128 KiB。实际按 canonical codec 序列化核字节与字段覆盖。到第 511 项后，最后一方最后一个合法槽仍能受理；不能缩减证据、截名册/转让史或丢旧原件来达标。原完整条款/接受、长说明、256-byte 引用、正文和追加历史仍可沿原 journal/受权原件取回，当前 roots/counts 覆盖完整已验证集合；缺 prefix 不凭 current head 猜补。

同次完整 invoke 的输入实际合计 `128 KiB state +64 KiB command +48 KiB 本次验证 proof +16 KiB envelope =256 KiB`，输出合计 `128 KiB new_state +64 KiB effect +32 KiB emit +32 KiB 封装 =256 KiB`。全部分额同时接近上限且包括自身编码开销，不能分别用满后另加元数据。较大原件须有原 source 合法的有界证明 adapter；缺这种证明时拒绝新承诺，不以摘要冒充验证。单证据来源在 48 KiB 分额内可核，16 个合法槽可分别提交；冻结/投票使用完整索引，panel 受权分页读原件，不重复把全部正文塞入 invoke，也不新增免费第三次调用。

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
