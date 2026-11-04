# 工业能力许可、版税与开放执行设计

- 生命周期：`active`；本文是目标合同，不是现有功能、已激活政策或平衡实测声明。
- Owner role：`runtime_engineer`；`gameplay_designer` 拥有费用、期限、开放和争议政策；`producer_system_designer` 拥有产品边界；身份/治理、WASM、Viewer 与 QA 分别负责权限、执行、消费和验证。
- 审读日期：2026-11-04；固定静态基线：`ad2bf1bb9e29d315ea70b64603b57c206da1be0f`，repository `nil-studio/oasis7`。
- 唯一玩法政策：[GAME-018 industrial-license-policy-v1](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#industrial-license-policy-v1)。本文消费其中的数值和权利规则；手算例不是另一个政策源。
- 验证入口：[工业许可与版税验收手册](../../testing/manual/industrial-license-and-royalty-settlement.manual.md)。本轮仅文档与静态源码审读，未运行世界、模型、Rust、真实发布或玩家验收。

Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965

## 1. 问题、目标与非目标

技术发布、工业准入、使用许可和收益结算分别证明不同事实。当前模块 owner、command grant、配方完成或物流 tariff receipt 都不足以证明某位创作者对某次工业使用享有版税。目标是把有范围的许可、当前使用权、真实完成批次、已占用资金、有限争议与到期开放连成可复算的权威记录，并让所有正式入口保持同一结果。

首版许可生命周期适用于已准入的工业能力；付费 adapter 仅支持可证明成功完成的完整配方生产批次，结算资源为既有 `ResourceKind::Electricity` 原生整数单位。其他能力只有明确的零费或开放条款可用；未知计量不收费，也不凭未知来源为他人能力签发免费许可。费用范围之外的生产、资源、设施、区域和配方权限仍独立成立。

本设计不创造现实著作权、专利、外部兑换或预期收益保证；不完成整个 R&D、组织份额交易、普通合同仲裁或一般扩展撤销系统。工业提案、模拟、试点与准入由[四阶段合同](industrial-creation-simulation-and-admission.design.md#creation-admission)负责，已发布模块的有限回滚仍沿[既有 PPE 处置](../module/player-published-entities.design.md#ppe-rollback-disposition)。本设计只承接工业许可在 GR-003 中的适用子范围。

## 2. 上游约束与相关角色

产品与 GAME-018 决定有限收益、开放、玩家选择和首版政策；工业准入 authority 证明精确能力、用途、区域及当前效力；身份与既有权利 authority 证明许可签发、收款和资源来源；Runtime 消费这些事实并提交账本。Viewer/API/Agent 解释同一投影，不能以本地状态、建议或签名本身代替权威结果。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | 已准入能力的用途、区域、期限、维护、收益与归属必须完整且可验证 | [许可 authority](#license-authority) | GAME-018、工业准入、原权利人及身份 authority | 不从 artifact owner 推导创作归因；不完成通用 R&D |
| [AC-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | 任何正式使用重验当前范围、资金和资源；试点、预览及旧请求不签发使用权 | [正式使用](#license-use) | current admission、配方 lifecycle、工业工作图、资源 hold | 不提供工业准入或生产资源 |
| [AC-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | 仅绑定真实使用事实的 verified receipt 产生一次版税，未完成不预赚 | [计量与结算](#license-settlement) | recipe completion、当前费用 authority、冻结分配条款 | 不以运输到达或任意 escrow 自动计费 |
| [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | 到期、开放、撤销和收缩有确定当前结果，不永久封锁或改写合法历史 | [生命周期](#license-lifecycle) | GAME-018；工业及配方处置 owner | 开放不扩大工业、安全、通行或资产权限 |
| [REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 工业许可限制/退出声明后续效果，保留历史和最小合法处置 | [生命周期](#license-lifecycle) | 当前工业授权；原 Root/Child/Disposition | 只覆盖工业许可；不覆盖全部已生效扩展 |
| [AC-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | 重叠工业许可替代原子交接；旧待决请求不能并行生效 | [生命周期](#license-lifecycle) | authoritative ordering 与同根事务 | 不使 PPE 回滚 marker 变成既有实例停机权 |
| [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | 审计、争议、维护责任和可读救济保持有界，不挪用已确认收益 | [有限争议](#license-review) | 许可专属本地管辖、合法名册和随机源 | 不借个人服务 panel 自动取得工业许可管辖 |
| [AC-GAME-018-02](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-02)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | 玩家可比较许可、续期、开放、替代与争议；显示成本、当前范围、失败和下一步 | [统一读面](#license-read) | Viewer/API/Agent、QA | 未支持的正式入口保持 unavailable |
| [AC-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | 重复、重连、旧缓存及恢复不续期、复制权利或追溯收费 | [事务与恢复](#license-recovery) | world commit、持久化与当前执行 authority | 不以文档或 fixture 代签正式网络验证 |

## 3. 当前状态、目标状态与差距

以下均是固定基线的静态观察；现有基础的存在不证明许可功能已实现。

| 对象/能力 | 当前状态（基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| 原生资源与物流费用 | Electricity/Data 是资源种类；物流开始预扣 Electricity，完成记录 tariff、owner payouts，governance tax 为零 | 使用原生 Electricity 的受保护许可 hold、已赚取权益与独立付款 | tariff 不是版税；受保护占用必须覆盖所有可冲突扣减入口 | [资源种类](../../../crates/oasis7_agent_api/src/world_action/support.rs)、[运输实际消费者](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs) |
| 配方完成 | 对比 pending payload、完成 receipt 与 settled job；工业 root 校验 first sink、slots、owner、设施、目的地和 plan | 在同一完成提交中加入精确许可、非重叠批次、fee unit 与 canonical notice | 现有完成记录无许可/收益份额绑定；普通 accepted_batches 不能追溯收费 | [完成准备器](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs)、[工业完成 gate](../../../crates/oasis7_world_state/src/world_rules/industrial_completion.rs) |
| 原生业务状态 | WorldRuleCommandV1 有 industrial work、substitution、供给、组织和制造等具型命令；WorldRuleReceiptV1 记录 command 结果 | 新许可操作、账本、验证和读面经正式 reducer / publication | 当前枚举和消费者未提供本合同；任意 WorldRule receipt 不等于 verified royalty use | [命令全集](../../../crates/oasis7_agent_api/src/world_action/world_rule.rs)、[当前 reducer](../../../crates/oasis7_world_state/src/world_rules.rs)、[实际事件分派](../../../crates/oasis7_runtime/src/world/event_processing.rs) |
| 工业身份 | IndustrialRef/Root/Child/Receipt、immutable recipe origin 与工作处置已有专业合同和部分代码 | 引用原身份，不复制一套生产账 | 完整配方 lifecycle、creation admission 和 license 的合取仍需适配并验证 | [原生工业身份](../../../crates/oasis7_agent_api/src/world_action/industrial_work.rs)、[工业工作图](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition) |
| 受保护资金、费用及 drain | 供能服务设计已规定 Kernel 全路径 hold、当前合法 payer/metering 与独立 closure 预算 | 许可复用相同资源保护和执行边界，采用本许可的独立 purpose/额度/程序槽 | 这是一项设计依赖，不能宣称已有完整实现 | [资源保护](checkpoint-power-service-fulfillment.design.md#service-resource-holds)、[费用预算](checkpoint-power-service-fulfillment.design.md#service-fee-budgets) |
| 归因及争议 authority | 产品要求预工作 R&D charter、永久 provenance、显式份额转让和 receipt-first 本地程序 | 许可只消费可核验归因、明确同版分配及许可专属 panel artifact | 现有模块 owner、治理投票或社交分数不具这些证明 | [合作与归因产品](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md)、[原生治理状态分派](../../../crates/oasis7_world_state/src/state.rs) |

## 4. 边界与结构

| 责任面 | 产生/拥有的事实 | 允许与禁止 |
| --- | --- | --- |
| 内容、工业及配方 authority | exact artifact/Profile、来源、creation/admission、RecipeVersion 与当前 lifecycle | 技术 Apply、模拟、pilot 或历史 admitted 不直接产生许可或版税；restricted/retiring/retired 不被许可覆盖 |
| 归属及许可签发 authority | 原创作/派生 lineage、有限收益资格、原权利人同意、维护责任、条款和开放条件 | 签发者须有本能力/本范围的当前许可权；不能由 artifact owner、OC 或多数票猜出 |
| 使用者与 payer | 当前身份、原始资源来源、同版条款接受和明确工业使用请求 | 使用许可不提供资源；payer 与操作者/受益人分别鉴权，不强扣未签第三方 |
| Runtime | 使用互斥、真实 hold、批次事实、pending-earned、notice、分配和 receipt | 在一个 canonical world transaction 内提交，不能旁路改余额或导入 receipt 造收益 |
| 许可专属 panel | 在原管辖、范围、窗口内对已冻结证据作有限决定 | 不能撤销工业 admission、控制设施、重写已付历史或临时创造罚没 |
| 身份与治理执行 | 当前主体/控制证明、合法 jurisdiction、名册、随机、执行 artifact 与 finality | Agent 换 owner 不转让原个人收益权；通用服务管辖不自动适用于许可 |
| Viewer/API/Agent | 同一快照下获准可见的条款、成本、归属、状态和下一步 | 未验证推荐、时间估计、客户端点击或离线缓存均不产生权利 |

许可使用条件是当前工业准入、当前配方/工作处置、当前执行/资源/设施权限与当前许可范围的交集。开放只移除适用的创作者费用限制，不把这个交集变成全局使用权。保留独立公共安全、身份、资产和通行保护；本合同不创建全局拒绝名单。

许可条款、fee-unit、分配池和案件属于 L2 institution，遵守[Kernel/Institution 与 native 分类冻结](../prd.md#native-surface-分类与冻结规则)。目标使用带完整 world/instance/artifact/schema/activation 的 module-owned 记录及受保护存储；不为本制度新增 `WorldRuleCommandV1` 分支、核心 Action 或专属 WorldState 顶层字段，不扩大旧 EconomicContract，也不选择 SC-32 migration pilot。Kernel 仅提供通用身份、范围、资源 hold/狭义处分、当前工业不变量、预算及同根提交的强制校验。

每一接受的逻辑迁移最多触达两个已声明、已激活且当前有权的制度/验证 instance：例如 root 完成记账、或 root 向池交接；其他必要 authority 以有界可信输入/现有原生 gate 消费，不能伪装为免费额外 module invoke。精确路由/调用闭包在激活时固定，禁止调用者提供任意模块/回调；必要第三次调用未进入已签预算即不属于本profile，拒绝启用或原子无效果拒绝，不能只贴“两次”标签省掉真实消费者。条款、root、case、pool 是有精确独立身份的有界记录，单次替换的 instance state≤128 KiB；case说明正文保留在可审计内容/原journal，当前state存有界不可变引用，不内嵌每次全部历史说明。320个receipt引用、名册、各主体/阶段及审计根仍须按最大输入验证在状态上界内。实现使用受保护component时也须有当前已批准schema、完整写集和访问计费，不把新component接口当作已存在；本初值不依赖其免费访问。

制度 reducer 返回自身合法new_state、有界狭义资源变更候选及一份必要业务/notice emit；受限 effect 必须通过 exact cap_ref/cap_slot、当前允许的Kernel资源操作、原hold/受益范围及守恒校验，不接受任意“apply_license”原生制度旁路。真实工业完成、制度状态、验证过的资源delta、notice、全部actual metering和receipt在同一当前事务中准备并发布；新state或emit本身不证明产物、资产或通知已提交。只允许有合法有限资源适配器和同根消费链的候选启用；EffectQueued/外部异步ACK不能代替这里的同步世界资金结果。实际调用闭包超过两次、需要额外强制访问/费用或状态/效果上界时必须重新完整定额并取得新接受，不能省略预算继续执行。

## 5. 关键运行流程

<a id="license-authority"></a>
### 5.1 完整条款、归属与有限权利

`LicenseTermsRef` 是逻辑合同，至少绑定：world/branch/genesis、不可变 capability lineage、精确能力种类/版本/hash/Profile digest、[IndustrialAdmissionRef](industrial-creation-simulation-and-admission.design.md#creation-interfaces)、签发者的当前有限许可 authority、被许可使用主体、用途/区域/对象、开始含结束不含的世界 tick 区间、政策、计量 adapter、单价/最大单位、维护责任、撤销/收缩/替代结果、公开条件、审计/通知、专属争议管辖与原条款 digest。未知字段或不完整的关键事实不得被默认值补成合法许可。

许可 `expires_at` 不晚于原 admission/相关授权期限、单次政策上限和 lineage 硬开放 tick。开始、报价和结束加法全部 checked；溢出拒绝。单次许可到期终止该许可的收费使用权；继续有限许可必须新接受且仍受同一 lineage 硬截止约束。提前公开须由当前有权且条款要求的主体明确选择；续期、维护版本、换账户、换 request 或缓存不能推迟已公开条件。

lineage 不是调用者任填的收费键。首次准入原件固定经济能力身份、canonical 内容承诺、审定的 parent/derivation/provenance 与最早准入 tick；规范化内容相等的注册项和已确认派生/维护版本必须反查同一注册关系和最早 tick。仅换 ID、包装 metadata 或维护 revision 不产生新收费期。规范化所含因果字段、版本和排除的非因果 metadata 由该能力专业 schema 冻结，不能由前端删字段制造新摘要。两个内容不同且有独立来源的能力不因名称或语义相似被强制合并；来源/派生关系不明时交原归属/工业审查 authority，阻断有争议的付费发行，不替他人公开独立资产。

收益受益人是有来源依据的原个人/合法权利主体，收款 Agent 只是经当前授权解析的承载账户。合作产生收益份额时，须有在实质工作前各贡献方同版明确接受的 R&D charter：目标、角色、方法、适用 royalty 范围、争议/变更边界、永久 Agent provenance 和分配向量均可核验。受益人数量与正整数 bps 合计沿唯一政策；无重复受益身份，不以自报 owner 代替证明。单一创作者也须有可核验来源与许可权。缺任何必要归属或同意则拒绝相应付费条款；此依赖不声称完整 R&D 已完成。

维护责任固定负责主体、能力/版本、应交付 evidence、复核条件、违规触发和替代路线；不因维护声明另收未签费用。缺维护证据只影响被当前 authority 限制的后续新效果，不由 license handler 自行剥夺公共通行、Agent 身份或既有资产。新维护条件需要未来生效、当前授权和明确接受，不追补旧费用。

`inspect/compare/renew_request/challenge_request` 本身无占用或版税；同版接受及当前验证后登记一次有限许可也不代表生产资源已预留或收益已赚取。单纯提交请求不等于受理或激活；实际业务写入的合法执行费按 §6.1 独立披露，不能以“零版税”绕过强制计量。正式使用另走下一节。所有参与者签署前须看到世界 tick 窗口离线也推进、不换算固定现实分钟，以及收益和损失均不保证。

<a id="license-use"></a>
### 5.2 从许可到一次正式使用

1. 只读报价列出精确能力/admission/条款、正式 recipe use adapter、原工业 root/版本/固定生产计划、完整批次数、当前作用域、原资源与费用来源、受益分配、时间边界、替代及最坏成本。quote 不保留生产槽、许可额度、资源或版税。只选 term 不能制造正式 root。
2. 确认时，从当前 authority 重建完整身份和条件：上述权限交集、`IndustrialExecutionContextV1.purpose=formal`、完整归属/分配、政策当前可用性、条款同版接受、原生 plan/单位、当前可用资源、维护/位置/设施、费用及可闭合条件。`purpose=pilot`、Shadow、模拟输出、模块发布成功、旧 receipt 或未知 adapter 均不能进入付费计量。
3. 本次 B 批的最高版税为 `R=B×p`，B/p 取冻结政策和原生产计划；版税不是生产所需 Electricity 或正常交易/执行费。按同一 payer/resource 汇总所有相关生产、版税、操作和 closure 占用后，才验证可用余额。禁止逐项足额但总额超支，禁止以未来收益融资本次费用。
4. 付费使用确认、原工业 root 绑定、当前使用槽、版税/操作/closure 真实 hold、原请求结果在同一提交安装。hold 是真实已有余额的受保护占用，不是第二份资源；所有 native、WASM、自动生产、普通转移、维护、费用和 owner 控制扣减入口均须扣除它。未交付完整保护适配器时付费 profile 不可激活。
5. 并发按政策的 `(world, capability_lineage, 使用主体)` 聚合，而非按 license/request/Agent 数量计数。使用主体来自当前可核验的主体/控制关系映射；别名和代理入口共用同一键，未知映射阻断新承诺。原始 root 是业务身份；换请求、配方显示 ID 或续许可不增加槽位。第一 sink、每个后续新效果和完成均重验当前条件，确认不提供未来永久许可。
6. root 未开工可按原合同原子解除未消费的 hold 和适用槽；已经 sink 的物料/电力不得由取消许可自动退回。生产槽在原工业义务真正处置完毕时释放；四个付费用途根槽仍按 §7.1 等待本根费用/程序或有效移交闭合。归属、通知、争议、分配及待付款使用独立的有界记录，不因生产终结而删除。

原生 `RecipeCompleted` 以该 root 的完整 `accepted_batches` 为目前可适配证据形态；新 adapter 必须核 exact recipe origin、root/child/revision/effect role、真实输出接收/验证与无重叠批次身份。目标允许通过具型 producer 证明的完整批次集合；不能用任意百分比、客户端计数或在制品折算批次。单个 batch 的多个产物、产品验证、生产完成、商业交付和运输 receipt 只对应一份使用义务，不收多次。

同一 root 的计费键绑定 `(world, capability_lineage, original_root_ref, native_batch_range, charge_role)`；不同 transport request 或 receipt alias 不改变它。只有已有正式 use binding 的成功批次可进入下一节；旧无绑定的生产 receipt 不补造费用。所需新 guard 必须位于所有正式工业 effect 的共同提交路径，不能仅检查报价/网页或某个 WorldRule handler。

<a id="license-settlement"></a>
### 5.3 计量、待确认收益、累计分配与真实付款

#### 5.3.1 完成事实与通知的原子边界

每个 fee unit 保存原 use/root/batch 范围、exact admission/许可/政策/分配版本、生产完成 event/receipt、权威完成 tick、金额、原资金来源、受益 slot 与状态。首版 recipe adapter 只接受原生一次完整成功完成所证明的 `[0,B)` 批次；不能把失败后幸存物料或部分产品验证比例推成成功批次。未来若支持分批完成，须新增能证明非重叠完整批次的 adapter 并验证同一累计账，不能直接解释既有 `accepted_batches` 为多份证据。

在收费权半开区间内且符合当前工业授权的成功完成，以权威完成 tick 判断适用价格。产物/生产事实、原 root 完成范围、版税从 unused hold 转为 `pending-earned`、可由原相关主体权限回读的 canonical notice、原请求结果和 receipt **同根提交**。`N` 就是这次权威提交 tick；没有可证明持久登记的 notice 不允许付费 adapter 启用。它不要求玩家在线或 UI 已读，读面延迟不改 N；世界暂停只沿世界时基处理，不以客户端时钟或现实分钟裁定。

`pending-earned` 证明存在绑定实际使用的待程序确认收益，不表示收款人已到账。普通争议窗口结束或有效 final 前不提前提款。精确重试回读原 N、原 fee unit 与 receipt；新 request、换网络或客户端第一次看见都不重新开窗。响应未知时恢复原提交；旧无许可绑定的 receipt、后来补写的本地时间和导入证据不得制造新收费。

收费期结束后尚未完成的单位不预赚；原已完成但后来才查询到的 fee unit 保留原 N、权利和价格，许可过期不能清掉它。存在同 root 商业 escrow 时只能引用这一 fee unit 和原资金来源处理其状态，不能在生产完成和交付里程碑各扣一遍。

#### 5.3.2 整数计量与累计分配

数量、金额、tick 和权重分别带单位；不得把 Electricity、Data、OC 或材料数量相加。版税单价 p 取唯一政策。每个完成集合的新增数量 `q` 是尚未计量的完整 batch 身份数，新增金额 `e=checked(q×p)`；原费权之外 q 不进入收费账。重复/部分重叠范围拒绝整个新 effect，调用者先查询再显式提交确切剩余，不能静默裁小后收费。

分配池键是 `(world, capability_lineage, 原冻结分配来源)`，原来源绑定唯一 charter/distribution revision。同一权利期间的不同许可证、payer、root、request 和续期共同累计；v1 拒绝通过改 charter、权重、分配 revision 或另建池从 E=0 重开。允许的显式转让/身份纠正只改变原稳定 benefit slot 未付权益的合法主体或收款路径，bps、E、已计费批次和余数连续。真正改变 bps 需另版治理政策及完整旧权益迁移合同，本首版不支持；不能伪装成普通换收款人，也不能改变硬开放或永久 provenance。

将已过通知窗口、无使用事实/金额争议或经有效 final 确认的金额累计为 E。仅受益 slot 归属有争议时，实际使用量可在原窗口结束后进入 E，但只阻止该 slot 付款；涉及数量/金额的争议 lot 在结论前不进入 E。原 bps 与 slot 身份冻结，归属纠正不任意改变其他人的权重；需要否定整个分配依据时，精确列出受影响 lot/slot，不能冻结无关生产。

对每个稳定 benefit slot i，权重 `w_i>0` 且 `Σw_i=D`，D 取政策的 bps 分母。定义：

```text
A_i(E) = floor(checked(E × w_i) / D)
本次新增应计_i = A_i(E_after) − A_i(E_before)
H(E) = E − Σ A_i(E)        0 ≤ H < 受益 slot 数
```

E 非递减，所以每个新增应计非负。A_i 是个人权利 slot 的累计应计，包含已实付与仍受保护的 payable；收款 Agent 下线、失控、变 owner 或余额溢出不使该权利变成新 owner 的资产，也不退给 payer。只有原受益主体当前合法收款路径或有明确证据的有限继受被验证后，才把其 payable 原子转入可支出账户。已有收益不保证立刻可提款。

H 留在已赚取的受保护分配尾差中，不视为 unused hold，不借给生产、费用或其他池。**硬开放/合法提前终结收费、全部旧 root 处置、所有 fee notice 窗口、案件/申诉和未知结果恢复均已证明闭合**之后，才能一次终值分配：按 `(E×w_i) mod D` 从大到小，每次给一个最小 Electricity 单位，余数相同按原冻结 benefit slot 的稳定身份字节序；写入唯一 `allocation_closed`。不能一开放就分光，也不能在新许可证中重新取整。未能收款的最终尾差仍归原 slot 的 payable，不阻止确定归属，但不能标成到账。

手算例：三个 slot 的权重为 `5000/3000/2000`。E=7 时 A=`3/2/1`、H=1；E=11 时 A=`5/3/2`、H=1，新增应计为 `2/1/1`。终值确认 11 后，尾差给第一个 slot，最终 `6/3/2`。把 11 个成功批次拆成多条合法 receipt 不改变结果。不得对每条 receipt 单独做余数分配；也不得以累计最大余数结果直接求差，后者可能使某人的累计份额倒退。

若额外合法交易费适用于版税转移，按独立费权、payer 和费用 hold 计算，不从上述净版税扣除。累计费率按同 fee leg 的累计 basis 差额收费，不能随拆单/续期/换请求重置；真实 metering 则只按实际合法成功执行消费。未知合法 fee schedule 不默认为零。

#### 5.3.3 守恒与并发

逐原资金来源对账，分类互斥：

```text
最初版税占用 = 未赚取占用 + pending-earned
             + 已入E但未付的个人payable + 已入E的尾差
             + 已实付版税 + 已原路释放的未赚取款
E = 已实付版税 + 个人payable + 分配尾差
```

每个权利 slot 另有应计、受争议保全、可提款、已实付的明确分区；不得把同一 payable 同时算成可退费用或通用余额。多个 payer 的池资金仍保留原 fee-unit 来源，按权威 fee-unit 提交序和来源身份确定性消费现有资金，不能凭池总额创建资源。池层 checked 容量、账户上界与所有受影响源/目标同时预检；不足或冲突全笔不写，不使用 saturating 算术隐藏欠额。

并发完成、取消、公开、争议、到期和付款均以同一权威顺序/CAS 判断。完成先提交则产生原 pending-earned；公开/到期先使收费失效则无新版税。已完成历史不因之后撤销改变；当前金额/依据有未知结果时先恢复原请求，不能在 payer 和收益人两边都“先补一次”。

<a id="license-lifecycle"></a>
### 5.4 到期、公开、撤销、替代与存量工作

许可效力、经济收费权、工业/配方权限、工作状态和收款状态分别记录。时间谓词直接在每次 effect 提交判断，清理任务或通知延迟不延长收费或使用期。`rights_open` 对已确认 lineage 单调生效；公开不能恢复失效的 admission，也不对别人独立资产产生使用权。

| 原状态/决定 | 允许的下一结果 | 必须保留/禁止 |
| --- | --- | --- |
| 预览、待接受或待续期 | 关闭、重新报价或当前条款重提 | 没有资金/生产预留；旧待决请求不能延长许可 |
| 有效许可、未确认 use | 当前条件仍有效才可确认；失效则拒绝 | 不允许用旧 quote 抢到期边界 |
| use 已确认、尚未 sink | 当前取消/到期或限制决定下，原子解除真实未消费资源/版税 hold 与槽 | 不返还已经用于其他原义务的资产；不伪造已出产 |
| 已 sink、仍未成功完成 | 按原[六类工作处置](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)及当前工业准入选择 hold、合法收尾、return/terminate 或适用的显式转换 | 许可本身不提供额外宽限、继续生产权、瞬时返料或新收入 |
| 到期或提前公开时未完成 | 未赚取版税保持可核验原路释放；需继续的原 root 以显式当前授权原子换绑到有效新许可/零费开放条款 | 旧 binding 退出与新 binding 安装同根提交；保留 root、sink、累计完成、原投入和所有 disposition；不得另建免费 root 复制生产 |
| 已成功完成、处于 notice/case/待分配 | 沿原 fee unit/N、程序和资金来源继续确权/结清 | 不因到期/公开退 payer 或重造一份收益；不是新工业能力 |
| 已应计、未到账或已付款 | 原个人 payable 恢复合法收款；已付历史保持 | Agent 控制转移不转让个人权利；纠错另有明确资金及关联 receipt，不铸币或强制追缴历史 |

撤销/收缩决定须声明仅停止新许可/使用确认、限制既有后继效果、受控退出的一项或多项及共同生效边界；缺这项 disposition 的决定不完整。安全或范围限制不能由许可续期覆盖。为保安全、价值保存或原不可绕过义务所需的最小动作必须有当前原专业 authority，不能从旧 receipt 推导无限期收尾。

同一使用主体、能力和重叠用途/区域/对象范围最多一个可执行许可 binding。替代 pending 不改变旧状态；实际替代时，当前新条款、当前权限、旧重叠范围收缩/终止、原 root/占用交接和 receipt 同根提交。旧未确认请求拒绝或要求显式重提，不能由中间节点自动改写成新许可；不重置累计批次、余额、案件或硬截止。非重叠的合法范围可独立使用，不以粗粒度全能力锁阻断。

单次许可到期并不把尚未到硬截止、也未提前公开的 lineage 立即变免费。欲续用须明确接受当前新 term；期限仍不超过原硬截止，当前准入、原工作 disposition、来源和所有真实资源 gate 均成立才可换绑。对同一未完 root，只为尚未完成/未计费的原批次计算新 `remaining_batches × current_price`；新条款确认、旧未赚取占用退出/同源有权重绑、新必要占用和 binding 切换同根安装，不能同时冻结旧新两份最高费用，也不能先释放给无关消费再另步求新预算。不同 payer 时原款只向原主体解除，新 payer 必须独立明确接受并足额；旧 pending-earned、E、个人应付和原 sink 完全保留。任何新范围、派生内容或价格变化缺必要授权/同意，拒绝换绑而保留原工作及责任；新许可不把已经受限或已终结的 root 恢复成可生产。

新签发、完整收费权到期与对现有工作收尾的 effect 分开：费用或许可失效并不删除独立生产资产；资产保全也不让失效费用或操作权限复活。重新公开、恢复或迁移必须保留原 root 和历史，无法证明合法当前路径时显示具体 `disposition_blocked`，停止新风险。

<a id="license-review"></a>
### 5.5 许可专属的有限争议与硬尾部

#### 5.5.1 管辖、名册与一次抽样

付费许可激活前必须具备由原世界治理程序认可的 `industrial-license.review` 本地授权 artifact，绑定 world/branch、固定 jurisdiction/region、适用能力/事项、政策版本、合法名册来源、可处分资产和金额上限。此项管辖独立于供能服务 panel、组织 charter、当前多数或 API 管理员；名册、随机和消费者尚未具备时该付费 profile 不可激活。

名册及初审/复核人数、门槛沿唯一政策。controller 去重并排除当事方、受益方、已确认关联控制、原准入/该许可批准者及本案直接经济利益者；证明不完整不当作独立候选。激活时核足可用于初审及不重叠复核的合格候选，并不保证日后仍足额。

受理前先提交原案身份、完整候选集合/承诺、冲突证明、政策及待抽取轮次，再消费第一个已授权且在该承诺前不可预选的可验证随机结果，得到唯一候选排列。初审、补位和复核沿这份排列，不重抽、不由 caller 换轮次或通过换 case/request 挑 panel。资格丧失或发现冲突后排除该候选并顺延；曾参与初审者不进入复核。public block hash、客户端随机、单节点自报和 LLM 选择不代替合格随机 authority。

#### 5.5.2 原案、通知、窗口与最小保全

收益 case 键固定为 `(world, capability_lineage, 原 fee_unit_id)`；fee_unit_id 由 Runtime 从原 root、原完整批次范围和 charge role 唯一生成，不接受 caller 任造“争议义务”。它不包含争点、证据 hash、申请者、nonce 或请求身份；同一 fee unit 先有案后换理由/证据仍并入该案，不增加程序机会。首版一次完整 root 完成只有一份该 fee unit。原条款的非金额争议绑定其 immutable terms notice，不能用它另外重复保全同一 fee unit；后来真实不同义务保留其自己的原 N 与案件，不能按旧案关闭吞掉新窗口。换 license 显示名、续期、撤回重开或转移控制不重置原案。

首个可核验 canonical notice 的 N 来自权威原件。收益 notice 必须与完成原子登记；条款 notice 随有权条款结果持久登记。只有原相关权利主体可在原窗口提出具体争点及可核验引用；缺席、断线、单句不同意或未收到 ACK 不足以证明违约。申请本身不保全资金；受理须核当前管辖、当事身份、范围、窗口、事实和不存在重复 case，才记录 accepted_at 和最小保全。**C 固定为 N+120**，是全部相关主体共同登记窗口的结束，不是第一个申请者到达的时刻；先到者不能缩短后来者的已声明机会。

| 程序节点 | 从唯一政策推导的绝对窗口/结果 |
| --- | --- |
| 受理 | `[N,N+120)`；恰好边界不再新开该程序；C 固定为 N+120，accepted_at 另记 |
| 初审证据 | 各方从自己的合法 accepted_at 起即可追加去重，直到 C+60；到边界冻结全部合并争点与证据 root |
| 初审 | 证据冻结后、`C+120` 前取得同一 typed disposition/范围/证据 root 的有效门槛签署 |
| 一次程序申诉 | 初审发布后 60 ticks 内且严格早于 `C+180`；仅一次复核，后到合格主张合并，不重启窗口 |
| 复核 | `C+240` 前有效；不能以一方迟交、缺席或换候选延长 |
| 硬尾部 | `C+300` 起按完整 canonical 事实执行下面的固定处置，实际到账仍需有权提交 |

通知必须披露离线仍推进、原截止、可核验正文及恢复查询。每方每阶段的证据引用上限来自政策，case 内提供和普通追加共用本人该阶段额度；初审用完不占复核，先提交的一方不能耗尽其他人的机会。普通证据截止后拒绝扩范围/普通补证，程序复核只核其被允许的事实。窗口不会因客户晚读或无回应重新开始。

争议使用量/金额只保全对应未付 fee lot；争议某个 benefit slot 的归属只保全对应未付款权益，其他已确认 slot 和其他 root 继续。保全不暂停工业 admission、生产、独立资产、无关付款或公共通行；不能借程序产生另一种许可拒绝名单。错误保全应按当前有权程序解除，并保留原因与原处置记录。

#### 5.5.3 允许结论、一次复核与终结

初审和复核分别由政策限定人数、门槛和独立候选决定，只针对冻结的受理范围和证据。允许的 typed disposition 是：确认原账；以完整 receipt 纠正尚未入 E 的实际使用量/金额；按原已验证 charter 或显式继受证据纠正受争 slot 的**未付**受益身份；返还未赚取 hold；解除错误程序保全。权重、单价、许可期限、其他 slot 和未争议范围不能任改；新补偿、生产限制和归属主合同的变更继续由原 authority 另行授权。

程序性申诉仅限授权错误、遗漏在期限内合法提交的证据、利益冲突、越界或可验证计算错误。不得因不满意重审全部事实；提交申诉不取消原初审。有效初审在无合格申诉的原窗口结束后 final；存在合格申诉时，以有效复核替代，复核截止仍无有效替代则按仍有效的原初审 final。已被可核验权限/冲突证据否定的初审不能靠超时变有效。

程序申诉/复核证据的共同截止取“初审可验证发布 tick + 政策申诉长度”和原绝对申诉截止的较早者。到该边界一次冻结全部在期内合法主张及证据 root，之后复核只消费该集合，不能抢先用部分主张耗尽唯一复核；原初审发布时间来自同根登记，不从首次客户端查看推导。各方追加机会受本人复核分槽保护，截止之后不能用补位或补证延长。

硬尾部先查询原 request、原案、完整证据/初审/申诉/复核/final 与资金结果，区分“有结论”“已证明无有效改变原条款的结论”和 unknown：

- 已有有效 final、同版合法和解或已确认未付款权利，继续执行其准确范围；到期不消灭责任或把钱退错人。
- 已证明没有有效改变原条款的结论时，终结本案程序保全：已验证收益按原 charter/slot 和使用基线处理，未赚取 hold 原路释放。此中性尾部只维持原已签基线，不证明争议得到实质裁判或损失已获全赔。
- 结果 unknown/incomplete 时进入原请求/权威恢复，保留资金和现存权利，不假定“没有裁决”或伪造付款。原截止不重置、任何一方不获得继续冻结/处置的新权力；恢复后按原时间和已提交结果收敛。

晚读或晚恢复已经发生的合法完成仍按原 fee unit/N 确认一次，不因程序窗口关闭没收收益；但不能据此补造旧无绑定 receipt、开启无界新案件或改写已合法付出的历史。历史错误造成的未补偿损失如实显示，后续补偿需要新的合法资金与原 receipt 关联，不由本 panel 铸币、强制追缴或转扣无关合同。

<a id="license-read"></a>
### 5.6 玩家选择与统一消费

同一获准快照至少分别显示：能力/归属/lineage/准入/当前许可；费用终止与工业权限终止；最大冻结/已消费/待确认收益/争议/个人应付/尾差/已到账/原路释放；原 root 阶段和真实投入；维护义务；当前 source/finality/freshness；固定通知与程序截止；primary blocker、`next_action`、`next_recheck` 及原请求查询身份。不得用单个 `licensed/paid/success` 代替这些独立事实。

接受前比较“继续有限许可、等公开/选择零费能力、续期/重提、停止或既有能力替代”；事后展示与当前状态相符的“补来源/重新报价、当前范围使用、最小处置/显式换绑、查询原件、按期争议、等待原结论、原主体恢复收款”。没有安全动作则直说缺少哪项 authority，保留可读历史；不伪造一定可用的替代。首局独立路线不被创作许可强制替代。

Viewer、pure API、Agent 使用相同源版本产生等义结果；未知或未实现 adapter 显式 unavailable/incomplete，不用 UI 元素存在代签。公开面只披露条款、必要理由和结果；私人贡献/控制证明、其他人的余额与无关争议材料按当前权限裁剪。控制失效的客户端可看到被允许的原件恢复入口，不能凭缓存继续使用或读取其他主体材料。

## 6. 接口与数据合同

以下是目标逻辑操作和关系，不宣称已有 HTTP/WASM ABI、原生 Action 或生产启用。所有写入都消费正式 authenticated/current authority，未知影响行为的 schema/版本拒绝；quote/模型输入不能直接填权威结果。

| 接口/条款 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| `ResolveIndustrialLicense` / quote | 当前准入、归属与许可 reader → 使用者/正式入口 | world、exact capability/admission、lineage、许可/政策/条款与相关状态 revision | 只读，不占槽/资产，不续期 | 完整条款或具体来源/权限/计量 blocker | 旧 grant/artifact owner 不自动解释为 license |
| `Accept/ActivateLicenseTerms` | 有权签发者、使用主体同版接受 → authority | terms digest、当前签发/使用权限、通知、jurisdiction | 精确范围 CAS；待接受无收费权 | 一份范围明确的许可结果；漂移要求重新接受 | 许可登记不产生版税；适用技术费独立按合法 quote |
| `ConfirmLicensedUse` | 当前使用主体和所有适用 payer → runtime | 原 root/plan、B、license/admission refs、fee manifest、request | 共用主体额度/真实 hold/生产 guard 同根提交 | committed use 或原子无效果拒绝 | 没有完整 native/自动/WASM gate 不启用 |
| `RecordLicensedCompletion` | 原具型生产完成 producer → 账本/通知 | 原 root/child/batch、实际完成 tick、原 fee binding | 与成功产物、pending-earned、N/notice 同根一次 | 可核验完成与待确认收益；不代表到账 | 未绑定旧 receipt 不补造收费 |
| `MatureEarnedLot` / `Allocate/PayRoyalty` | 原窗口/final/当前收款权 → 资源/个人权益 | 原 fee unit、pool、slot、fee/source、recipient authority | 原 lot 与累计 E/CAS；应计与付款分别去重 | 应付/实际付款/合法待收款 | 新 Agent owner 不继承原个人收益 |
| `Open/Expire/Restrict/Replace` | 原有权政策/授权变更 → license/work consumer | 原 lineage、terms、当前决定与 disposition | 当前时间谓词；替代重叠范围同根切换 | 当前范围/零费/退出/拒绝 | PPE freeze 不扩大为所有实例停机 |
| `OpenCase/Evidence/Draw/Vote/Appeal/Finalize` | 当前当事主体、获选 panel、调度器 → case authority | 原案、N/C、名册/随机/证据 root、角色/phase | 固定一案、一排列、一次复核与程序分槽 | 最小保全/原结论/硬尾部/unknown | 服务 panel 与普通投票不具工业管辖 |
| `ResolveRequest/GetTerms/GetUse/GetPool` | canonical 状态 → 获准 Viewer/API/Agent | 源 root、原请求/条款/root/pool/revision | 查询无效果，不延长任何窗口 | 真实维度、来源、下一步和未知 | 老消费者不会理解的新状态不可降级成成功 |

记录至少包含以上身份、完整签署/authority 来源、资金 purpose、个人权利与账户的分离、原操作/业务 effect 去重、各阶段时间、计量区间、N、case/final、当前及必要历史版本。正文必须能回到原 immutable terms，不只留一串摘要而丢失可审计条款。证据采用有界完整引用，hash 仅绑定内容，不自证来源、权限或真实性。

### 6.1 真实资金保护、执行费和闭合预算

复用[受保护资源与狭义处分合同](checkpoint-power-service-fulfillment.design.md#service-resource-holds)的通用边界，设置独立 `royalty_unused`、`pending_earned`、`royalty_payable/tail`、`license_operational_fee` 和 `license_closure_fee` purpose。原余额、hold 索引和可用量同根变化；所有能扣该资源的入口均不能消耗他人或其他 purpose 的 hold。许可 actor 只能处分其原具体义务，不能因知道 hold ID 调整任意余额。

实际交易费、强制 Kernel/module metering 分别解析合法 schedule、当前执行 artifact/instance、真实 payer、源账户、接收账、资源及 signed cap。不能用缺 owner 的免费分支、旧 artifact 或普通管理员代签；明确零费也须有适用 scope/version。业务费用、个人待分配权益和生产电力不能互相融资。当前 artifact owner 变化需要重新核验合法费用来源，不转移原受益权。

固定基线的[实际 metering consumer](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)按当前 artifact owner 找 payer，Compute 从 Data 扣，另扣 Electricity；先用[upper_bound](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)预检，再按 actual 计费。其上界包含最大输出/effects/emits，并为可能的 new_state 保留 Electricity 项，即使候选纯 planner 实际不写状态也不能先减掉。新 hold adapter 必须让 preflight 读取“本命令有权消费的准确费用 hold + 可合法使用的未占用量”，而不是锁完后读零导致所有调用失败，也不能把实际 charge 再从 hold 外扣一次。Data 与 Electricity 同源/异源都独立证明，不能只按实际较低费用反推最初预留；缺 artifact owner 的旧免费分支不得成为付费 profile 的可用路径。

每个 payer/resource 分别计算：

```text
需占用 = 生产投入上界 + 最大净版税
       + Σ(各操作分槽成功次数上界 × 该槽真实费用上界)
       + Σ(各闭合分槽成功次数上界 × 该槽真实费用上界)
       + 适用独立交易费的累计最大额
```

先按实际源账户合并全部 purpose 和既有占用再比较可用量；需要 Electricity/Data 两种资源时逐种足额，不估值互换。费用上界由合法 metering 的输入/输出/effects/emits/访问计数与确定写集推导；实际成功只消费实际费用。失败、相同请求、无变化与确定拒绝不再扣业务费；协议限流独立，不能变成额外版税。

操作预算必须保护每个原当事 slot/阶段，不是一方可用尽的公共桶：每方一次受理/并案、初审和复核各至多政策规定的证据追加、一次合格程序主张；每阶段各名册位置至多一个有效投票和一次资格排除/顺延；原随机证明一次、两个证据冻结及两次结论发布。case/ref/新 license 不增加额度，初审不挪复核、普通生产不挪程序权。缺少该案必要 payer 的明确同意及足额预算，不激活付费条款；不能届时扣 panel 私人资源或挪净版税。

closure 独立覆盖原 root 的终结/合法换绑、所有残余单位、未消费 hold 的来源释放、案件 final/硬尾部、个人 payable 登记、池终值/尾差与最终 receipt。每次 closure 批次必须终结至少一个尚未终结对象或完成一个未执行的合法阶段，不能靠 caller 任意拆片用尽预算。最后一笔先计算并保留本次实际费，再在同一候选中释放未用预算。无效果重试不消费 closure，当前无合法收款账户不靠收费轮询耗空其原款。

跨多个 use roots 的池在接受首个资金贡献前必须已有有权主体明确签署并足额保护的池级 closure 预算，覆盖固定 slot 数、终值和原来源索引的有界结清；新 root 只增加自身来源写集所需的预算，不擅自摊派给未签第三方。每一已接受贡献保留其原 fee/source cap，池规模受 §9 当前 manifest 的显式上限约束；不能把无限来源留给最后一个用户承担。一次接受、复核或升级均不得削减已有 root/程序/池的可执行退出路径。

新 intent 使用[当前 governing manifest](../module/module-lifecycle.md#sr2-activation-contract)选版，旧签名不授权执行已撤销代码。当前合法版本必须支持原条款的有限 settlement/drain、保留必要 reader/账本/fee/source 并验证最坏 closure 仍足额；不能激活破坏该条件的升级/停用。当前无可验证执行或费用事实时停止新付费承诺，保存原件和已有狭义权利，不以无限 blocked 代替启用前的闭合设计。

## 7. 状态、事务与持久化

<a id="license-recovery"></a>
### 7.1 独立状态、事务与去重

| 维度 | 目标语义 | 终结/保留边界 |
| --- | --- | --- |
| 条款/许可 | 待接受、已获准、受限、到期、被替代 | 不清除原正文、来源、期限及已发生结果 |
| 收费权 | 有限收费、提前公开/硬开放 | 开放单调；与工业 admission 独立 |
| use root | 待确认、已占用、已 sink、完成/按原工作合同处置 | 槽位、生产和费用移交各有证据，不能因 UI 终态释放未决义务 |
| fee unit | 未赚取、pending-earned、受使用量争议、可入 E、已移交池 | notice/case/current result 不丢失，原业务键永不重新可收费 |
| pool/slot | E、基础应计、尾差、受争 slot、可支付、已付、终值关闭 | 个人权利与物理账户分离，控制变化不清应付 |
| 程序 | 未开案、已受理、证据冻结、初审、一次复核、final/中性尾部、结果恢复 | 不重抽、不重计时，不以 unknown 形成退款或否定责任 |

传输键 `(world/branch, authenticated principal, request_id)` 绑定完整 payload digest、当前 scope、expected revisions 和证据；同键同内容返回原结果，异内容拒绝。业务键再独立约束 root/batch/charge、通知、应计、付款、尾差和处分；换请求、许可版本或 caller 不重复同一义务。旧成功查询只返回历史，不使旧 authority 重新可执行。

完成、资金 hold 转换、所有应计/分配差额、当前许可 binding、范围/额度索引、案件/通知、请求结果及 world/execution receipt 进入同一[ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0)。同一世界 canonical root/提交顺序处理所有竞争，prepare 不能先修改索引或先付钱。当前判定拒绝若本身不能可靠保存，返回 outcome unknown，不能宣称已拒绝后再重开单。

生产槽与付费用途根额度区分：生产/资源容量沿原工业合同释放；政策的四个付费用途根槽，要等生产和未消费 hold、该根 notice、使用量/金额争议、原请求结果已明确，并把已赚取金额及完整来源不可逆移交同 lineage pool 才释放。移交后独立池的尾差、slot 归属程序或受益人收款恢复继续，不占 root 槽也不清权利。它是并发未闭合限制，不是每主体终身最多 128 批；正常终结后的第五个 root 可以新确认，四个未闭合时第五个必须拒绝。

### 7.2 恢复闭包与晚到结果

checkpoint/journal 闭包包含原 terms/签署、capability/derivation/first-admission、相关工业/配方引用、原 root/batch/计费绑定、所有 hold/source/purpose 与预算、notice/N、案件/C/名册/随机/证据/结论、累计 E/slot/尾差/付款、来源与去重索引，以及必要历史 artifact/schema/fee schedule。不能仅恢复余额而丢掉来源、原个人权利或防重放边界。

重放归约原已提交事实，不重跑模型/WASM 创造新提案、不重抽 panel、不按当前单价、recipient 或世界时间重算历史应计。恢复后的新 effect 仍按当前合法权力/执行版本提交。缺初始 root、连续日志覆盖、原 notice、原 fee binding 或结果证明即 incomplete；不能补零、空集合、新时间或自动“免费迁移”。

客户端超时查原请求和业务键；落后节点上的 not_found 不证明未提交。若 world durable generation 为 `StatusUnknown`，遵守[原持久化 G1 fence](runtime-storage-footprint-governance.design.md#storage-generation-g1-design)，阻止整个不确定 writer 的后续 mutator/save，确认精确持久决定并安装一致状态后恢复，不只冻结本许可旁边继续写世界。

到期/开放/程序截止已过时先重建原时间线和已提交决定，再处理当前剩余；停机不延长费权或程序，迟到 ACK 不收费第二次。去重、零剩余 hold、个人 payable、闭合尾差和最终裁决按[存储治理](runtime-storage-footprint-governance.prd.md)保留可验证摘要/原件；业务 TTL、普通客户端清历史不能删除仍可合法重放的义务。

## 8. 部署、安全与运行约束

启用顺序是完整当前 authority/原工业消费 gate、Kernel 全路径资源保护、许可/fee-unit/池/程序 reducer 和 reader、真实 fee/drain、通知/名册/随机、持久恢复、正式消费者与候选验证。任何前置缺失，付费 profile 返回明确 unavailable；可读取目标条款，但不以兼容旧接口悄悄开始收费。零费/公开路径也须有当前准确授权及非付费的真实工业消费者。

签名仅证明来源，不自动证明许可权、controller 独立、随机公平、已完成生产或裁决终局。proof 均绑定 world/branch、精确对象、版本、范围和时间；外部 receipt、模型文本、人工改 world/JSON、管理员余额修改及私钥导出不属于此执行路径。公开日志不存私钥、私有 prompt 或无关个人证据。

授权故障阻断相关新义务；当前 writer 状态未知按全局 fence。身份或接收账户失效只保留原权益和合法恢复路径，不能把它当作永久罚没或把原款暴露给新控制人。当前可服务性与合法执行是实际付款前提，设计不承诺停机期间自动退款或任何现实时间 SLA。

## 9. 质量与容量

玩法上限只从 GAME-018 读取。技术首版上限：terms canonical bytes ≤32 KiB、单命令含 proof refs ≤64 KiB、分页 ≤50、每 use root 批次区间不得超过其实际 B、benefit slot 数不得超过政策上限；有界说明 ≤4 KiB，单 receipt 引用 ≤256 bytes。超限先检查再分配，不截断权益或证据。影响行为的新字段不能被旧消费者忽略。

### 9.1 费用、程序、时间与写集的组合可行性

有界 batch/并发不等于有可用闭合资源。本首版选用以下最小技术 profile 初值，需通过真实候选计量/处理验证才可激活；它不是测得的 SLA。按当前 world 的 manifest 执行：活跃付费条款≤8、未闭合付费 use roots≤8、已预留或活动的 case 槽≤16、收益池≤4、每池原资金贡献来源≤64。每个新付费条款和 use root 都必须在现有全局 case 预算中预留其自己的程序槽。16 槽计入所有尚有本源程序或 closure 义务的条款/root，包含到期条款、已释放用途额度但案件/closure 移交池的根；不只统计当前有效条款或已开案件。到期、换 term、生产完成或单一案件 final 不单独释放该预留，须等该来源的通知、程序、费用和 closure 全部结清；带剩余义务移交池时连同原预算和原槽一并保留，不能用新 root 挤掉既有申诉。生产/四根用途额度仍按 §7.1 独立释放，不因池侧保留槽而重新占用它。**一根一份收益案预算只适用于§5.3.1的完整root单次完成adapter：全部B批同根形成一个fee unit/一个N；分批完成、不同N、多个完成fee unit在v1拒绝。** 将来支持每个新义务/N的adapter，必须在其产生前逐份保护case/费用/closure，不能沿用本处16槽证明。容量不足只拒绝新承诺，不能删除未付历史；另建池不能绕过原lineage的E/来源上限。

每个有界处理批最多64个正常/程序有效写入，并独立保留16个closure写入；同一canonical tick允许多个有界提交，最坏集中输入需有192批的已验证处理容量。64是单批上限，不是整个世界tick的硬64。读请求、无效/重复请求及新的可选条款不能占掉已接受义务的受保护程序份额。初值调整需要新合法manifest、同意与旧义务兼容，不能削减旧程序。receipt/proof refs仍受前述字节上限，单命令从可信引用实际读取验证的证明总量≤1 MiB；无法在命令/处理预算内核验时不激活对应新条款，不接受后再截断证明。

正式Service在进入受保护写队列前校验已认证主体、原actor/phase槽、格式/字节与原请求去重；未认证、超限、失败候选和普通读取使用独立有界admission/读取资源池，可提前背压或拒绝，不向业务分槽扣款/扣次数。已canonical登记的合法输入和原请求结果恢复使用保留的处理/读取路径；不能让攻击性读流量、失败候选或新条款竞争清掉它们。每actor/phase队列分开，耗尽自身候选处理预算不能消耗另一方原定机会。实际验收要在这些干扰并发存在时证明最大已登记义务仍可处理；没有该隔离能力不能把理想空载的192批结果当激活依据。服务背压只陈述尚未canonical受理，不伪造入窗保证。

本技术 profile 的案件当事主体上限为 m≤10，名册 N≤64。条款激活和 use 确认前必须枚举完整的有权当事集合，包括使用主体、净版税 payer、签发者与各受益人；不假设使用主体等于 payer。只对已验证属于同一责任主体的别名/代理去重，不能因关联控制合并不同权利主体。若独立使用主体、payer、签发者与八名受益人合计11人，则该候选超出本 profile，必须在接受任何新承诺前明确拒绝，不能开案时删除第11方或挪用他人分槽。后续变更若增加独立程序主体，也须在其生效前证明完整机会与已签预算仍满足上界，否则保留原权利并拒绝该新变更。生产或技术执行费用的不同 owner 沿原费用合同保有权利，不因付执行费自动获得本收益案管辖内的当事资格。单收益案操作成功写入上界保守为：

```text
K_case = m × (1次受理/并案 + 16次初审证据 + 16次复核证据 + 1次程序主张)
       + 2N次投票 + 2N次资格排除/顺延
       + 1次随机证明 + 2次证据冻结 + 2次结论发布
       = 34m + 4N + 5；m=10、N=64 时为 601
```

这些是用于费用/处理上界的允许写入，不保证每次都发生，不增加 panel 同时在任人数或申诉轮次。批量证据可减少实际调用，但预算不能假定当事人必定合批。条款案与各实际义务案分别预留，不能用一个通用 100 次 cap 削掉某方已签程序。证明正文可分页读取；只在冻结/投票命令提交精确证据 root 和有界引用，不能把所有证明无界塞进一条命令。

普通 root 许可操作最多 12 个有效写入槽：确认、first sink、成功完成+notice、mature/移交各一次，同 root 显式续期/作用域换绑合计最多八次；拒绝/无变化/原请求恢复不耗槽。此上限随签署展示，耗尽时不能增加收费或重开 root，仍保留合法闭合与独立重新规划。原工业执行/生产本身的预算另按真实 plan 计算，不用这 12 次免除其合法费用。一个 use root 的许可操作+完整案件为 `12+601=613` 次。

每 root 预留 128 次 closure，覆盖至多32批次的原处置/未赚取金额终结、一次case final、一次mature/移交、至多8个slot应付登记、至多64个原fee/source/purpose残余释放以及root槽释放与最终receipt（合计最多108次）。root原费用来源/purpose槽≤64，超量在接受前拒绝；不按平均payload合批压低上界。池另有640次closure槽，覆盖64个贡献×8个slot的有界实付/待付终结、64个来源关闭、8个终值尾差归属和最终receipt（最多585次）；贡献带入的原案件/费用恢复继续使用各自已保留预算，不挪池尾差。未找到合法收款路径时一次形成可恢复个人payable，无效果重试不收费；之后有效收款消费该原slot尚未使用的付款槽。关闭须保存原来源与去重，不能通过清记录腾位置。

分别核对确认的全部资金、同根完成/notice、程序与退出最坏写集；不能用均匀到达的窗口均值证明最后一tick突发。为16个既有案件保守预留其**整个剩余程序**至多 `16×601=9616` 次，再加8个条款和8个root各12次正常迁移，合计9808次；虽然合法程序并不可能全阶段在同tick完成，这个更宽上界仍低于 `192×64=12288`。已接受的actor/阶段槽先占这份处理容量，新可选激活不得通过循环创建/关闭挤占；最大proof、两次module调用、状态写集、冻结root和持久化都须在实际候选证明能完成相应192个有界批。闭合另按全部未终结的条款/root 共16份原预留各128次，加4个pool各640次计，总上界 `16×128+4×640=4608` 次需要288个closure批，最多跨两个具备192批容量的可服务tick；条款自己的退出不能漏计，也不能通过循环到期/重建把未结义务移出这个计数。未知资金/收款条件不算可推进批，仍保留明确待恢复状态。

截止按**在固定世界tick/顺序中完成的canonical受理或证据登记**判断。网络送达、客户端发送、host排队及HTTP ACK都不承诺入窗；提前披露此边界。任何已在截止前canonical登记的合法证据都必须纳入对应冻结root，不能因后台批处理更晚而丢失；冻结消费完整已登记索引，未证明覆盖完整不签结论。到期后不能把未登记队列补写成旧tick，也不自动延长C。多个批次使用同一尚未推进的合法canonical tick是已有世界顺序内的有界处理，并非按wall clock延期；当前世界若不能提供上述集中接纳/处理能力，则该profile不可激活。本段给出待验证的技术容量，不保证客户端送达、节点不停机或现实时间SLA。

费用基准采用现行算法可复算的完整reducer上界：每invoke总input≤256 KiB、`max_output_bytes=262144`（包含new_state及候选/emit）、`max_effects=1`、`max_emits=1`，允许new_state。每次预检为 `256+256+2×1+1=515 Data`、`1+1+1+1=4 Electricity`。每个成功逻辑迁移按最多两次完整invoke预留，即1030 Data和8 Electricity；不以零effect的pure planner代表完整制度写入，也不把实际较低费用反推最初预留。两次invoke分别从其当前artifact owner的准确已签源计费；同owner可合并预检，不同owner独立足额，不能偷换成license使用者。原技术交易费若有其他合法适用项继续另列；最小profile须有scope/version证据证明没有额外转移费。若使用新component访问，则按真实非遗漏的访问schedule再计预算，不能假定当前公式自动免掉新访问费。

据此，一个完整root的许可操作/案件预算为 `613×(1030 Data,8 Electricity)=(631390 Data,4904 Electricity)`，closure为 `128×(1030,8)=(131840 Data,1024 Electricity)`；合计 `(763230 Data,5928 Electricity)`。每pool独立closure为 `640×(1030,8)=(659200 Data,5120 Electricity)`，在首个贡献之前由实际有权且明确签署的payer足额保护。条款自己的完整案件另按601次加登记/生命周期12次与128次closure预留同样的费用向量，不转嫁给未签的后续使用者。实际闭包只需一次invoke时仍按原有足额上界激活、只扣一次actual，不能把第二次保留量当作已经赚取的费用。

独立算例取已激活条款/已足额funded pool，四批生产的原plan总成本40 Electricity（包含其自身适用执行费，Data原生产成本为0仅作此fixture），净版税4。若合法生产/版税payer恰好也是两个制度invoke的当前artifact owner，确认新root总需 `40+4+5928=5972 Electricity` 和763230 Data；任一缺1全体拒绝。若实际产生10次正常/程序成功迁移及20次closure，每次两次invoke的实际input/output各1024 bytes、各1 effect/1 emit并有new_state，则每invoke actual为5 Data和4 Electricity；60次invoke共300 Data和240 Electricity。原生产40、版税4加上240，Electricity消耗/应付284、释放未用费用5688，Data释放762930。不同payer逐源拆开验证，不用未来收益填差额。实际候选必须证明每条完整invoke/受限Kernel apply/状态/notice链均在上界内；本复算只证明预算内部一致，保守程序占用远大于净版税，费用与玩家承担意愿尚无平衡证据，不能宣传成已经经济可用。

| 场景 | 环境、输入或刺激 | 预期响应 / 判定指标 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 权限与期限 | 固定当前来源；边界 tick、派生/改 ID、失权、未准入或 pilot | 越界新效果及费用为 0；lineage 硬截止不变 | IL01 / IL04 | 目标，未运行 |
| 计量与守恒 | B 上限与 +1、并发根、不同拆单、费用同源竞争 | 每批至多一次；每来源与每 pool 守恒差为 0 | IL02 / IL03 | 目标，未运行 |
| 程序与最小保全 | 最大名册/证据、全部窗口、冲突/随机缺失/一方耗槽 | 无争议款继续；每方程序机会保持；unknown 不伪装退款 | IL05 | 目标，未运行 |
| 组合容量 | 最大 payload/proof/来源、真实 metering 和 closure、窗口处理上界 | 上限内可有界结清；上限外原子拒绝，不侵占旧预算 | IL02 / IL05 | 性能及实际费用未测，不是 SLA |
| 恢复与读面 | prepare/commit/ACK/checkpoint 故障、正式跨入口 | 原结果/权益/通知/随机不变；同快照等义 | IL06 / IL07 | 目标，未运行 |

## 10. 兼容、迁移与回滚

启用前的合法 legacy 生产、receipt 和资产保留原语义，不追溯生成 license、N、费用或收益。新政策只作用于明确支持 adapter 且已经当前接受/绑定的新 use；是否 legacy 必须来自可验证启用边界和历史身份，不能让新调用通过省略字段变免费。新 fee binding 缺失、未知版本或恢复闭包不完整拒绝写入。

先部署支持读取/恢复新状态的消费者，再经现行治理启用写入；保持原条款与必要历史 artifact。停用阻止新承诺但须保留兼容合法 drain、个人权益、费用和程序；更改计量/费率/期限需新政策及未来接受，不能重算旧合同。回滚到无法理解已写状态的版本只能验证只读/隔离，不删字段、不回旧 snapshot 后重做收费。旧无许可证的合法工作可按原合同结束；它们不是新 profile 的默认豁免入口。

## 11. 验证策略

实施 PR 固定候选 commit/tree、启用政策/manifest、真实 authority/fee/随机来源、数据与执行环境。`test_tier_required` 覆盖同根 reducer/当前 gate、数学、并发、故障、重放和拒绝；`test_tier_full` 再验证正式持久化/节点与 Viewer/API/Agent，以及有界窗口和实际计量。阅读测试定义、链接检查或手算均不代签业务运行。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 验证目标 | 验证方法 / 测试入口 | 证据与版本 | 外部依赖 / 未覆盖范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | [许可 authority](#license-authority) | 全条款、归属、来源/lineage、当前范围和维护责任完整 | [IL01](../../testing/manual/industrial-license-and-royalty-settlement.manual.md#il01-authority)：固定来源逐项缺失/派生/身份漂移对照 | exact source/terms/authority、原后状态；实施候选记录 | 通用 R&D、外部知识产权不在本次 |
| [AC-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | [正式使用](#license-use) | 正式所有入口的 current gate、生产/费用/闭合合取与并发额度 | [IL02](../../testing/manual/industrial-license-and-royalty-settlement.manual.md#il02-use)：native/WASM/自动/直接入口竞争，边界/余额/hold/预算反例 | 同根资源、费用、scope/slot 和拒绝差分 | 未适配入口不可由一个 handler 成功代签 |
| [AC-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | [计量与结算](#license-settlement) | 同根成功/N、分层金额、累计整数、拆单不变和个人受益权 | [IL03](../../testing/manual/industrial-license-and-royalty-settlement.manual.md#il03-settlement)：独立重算及错误 receipt/尾差/受益人反例 | fee-unit、N、E/slot/资金源、原后余额与提交定位 | 实际收款、治理费来源仍须真实候选 |
| [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | [生命周期](#license-lifecycle) | 到期/open/撤销保持历史、后续范围及合法存量处置 | [IL04](../../testing/manual/industrial-license-and-royalty-settlement.manual.md#il04-lifecycle)：时界与 effect 两种顺序、同 root 换绑和收尾拒绝 | 原新 binding、sink/批次、旧应付与 disposition | 原配方/工业处置当前合法性不可由许可制造 |
| [REQ-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | [生命周期](#license-lifecycle) | 工业许可限制/退出有明确结果、无追溯剥夺 | [IL04](../../testing/manual/industrial-license-and-royalty-settlement.manual.md#il04-lifecycle)：缺 disposition、只停新用/限制/退出对照 | 权威决定、当前 scope、已有资产与 receipt 差分 | 一般其他扩展退出仍独立 |
| [AC-WR-GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-003)；Trace refs: O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965 | [生命周期](#license-lifecycle) | 重叠范围最多一个 executable binding、旧待决不继承 | [IL04](../../testing/manual/industrial-license-and-royalty-settlement.manual.md#il04-lifecycle)：pending/commit/crash/旧重试及非重叠许可 | 原子交接、业务键、无部分 scope 的证据 | 不扩大 PPE 回滚含义 |
| [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | [有限争议](#license-review) | 有权随机无冲突 panel、有限窗口/申诉和最小保全/硬尾部 | [IL05](../../testing/manual/industrial-license-and-royalty-settlement.manual.md#il05-review)：固定名册/随机与全部窗口、final/unknown/无结论分支 | notice/case/证据/裁决、分槽预算与原资金来源 | 实际名册/随机/制度激活尚须证明 |
| [AC-GAME-018-02](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-02)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | [统一读面](#license-read) | 三入口同义，玩家可比较成本、风险、范围、替代及真实下一步 | [IL06](../../testing/manual/industrial-license-and-royalty-settlement.manual.md#il06-reading)：同候选审读与正式入口任务观察 | 权限下投影、快照/截图、实际受试记录 | 纸面审读不代表玩家已理解或UI已交付 |
| [AC-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-010)；Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 | [事务与恢复](#license-recovery) | 重试/旧数据/崩溃不复制效果、重置时间或抹去权益 | [IL07](../../testing/manual/industrial-license-and-royalty-settlement.manual.md#il07-recovery)：各提交边界故障、缺依赖、旧新 schema 与重放 | durable 决定、世界根、账本/个人权利/原请求一致性 | 本地 fixture 不代签真实故障域 |

## 12. 决策、权衡与风险

采用既有 Electricity、首版完整生产批次与独立 fee unit，避免把广告/试点/运输和世界货币混为收益。代价是其他工业能力目前只能声明零费/开放，付费扩展要新政策和准确 adapter。采用权威完成 tick 加同根 notice，明确离线窗口和晚到结果；代价是不能把“玩家没看见”当作时钟暂停。

采用累计 floor 与池终值一次尾差，保持拆单不变和每位受益人的应计非递减；代价是少量已赚取尾差等待全部原义务闭合。受益身份与资源账户分离保护历史权益，但需要可验证原权利和安全收款恢复。有限程序与中性尾部能结束无有效改变原合同结论的争议，不能保证缺席的 panel 有实质裁判，也不保证所有损失获得补偿。

| 风险/未完成项 | 主责与当前边界 | 关闭依据 |
| --- | --- | --- |
| 完整当前工业/配方准入与所有正式消费 gate | runtime、工业/配方 owner；本设计不替代相邻权限 | 同候选全部入口成功/拒绝及存量处置证据 |
| 受保护 hold、真实 fee/程序/closure 和池来源上界 | runtime/WASM/费用 authority；缺前置不启用 paid profile | 全路径竞争、实际 metering、最大写集与合法 drain 验证 |
| 预工作 charter、许可权与 lineage 来源 | gameplay、原归属/工业 authority；不能用 artifact owner 或自报替代 | 精确来源、同版同意、派生/独立反例和当前资格 |
| 本地工业许可管辖、无冲突名册及可靠随机 | 治理/身份 owner；服务 panel 不自动迁移 | 实际授权/随机 producer 和程序消费者的独立验收 |
| 参数与窗口的玩法可用性 | gameplay / producer；首版数值未做平衡或真人响应实测 | 固定候选试玩、真实成本/机会/离线窗口理解证据；只改未来政策 |
| 正式持久化、三入口与权利恢复 | runtime/Viewer/Agent/QA；本轮均未运行 | IL01–IL07 的真实候选/环境记录 |

上述前三类技术和制度实现缺口不是把已冻结 GR-010 产品结果改成“以后再决定”。本轮已明确可评审的政策及执行/验证合同；工程、治理激活和运行证据仍须按原 owner 实际交付，完整 R&D 与一般 GR-003 仍按各自原范围继续。
