# 配方替代决定与实际转换系统设计

- 设计 ID：`DES-WR-SUB`
- 状态：`active`，长期目标合同；不表示当前排产动作已实现替代、转换或正式入口验收。
- Owner role：`runtime_engineer`；Recipe/Factory/M4 提供材料、适用性和转换规则，`gameplay_designer` 维护节奏/价值约束，`qa_engineer` 验证守恒、恢复及三端语义。
- 固定审读基线：`nil-studio/oasis7`，`e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`，2026-10-19。
- 上游：[SC-30](../../product/world-rules-core-gameplay/prd.md#sc-30)、[原料适用性与替代玩法合同](../../game/gameplay/gameplay-industrial-raw-material-qualification-substitution-contract.prd.md#4-decisionconversion-与因果隔离)。

## 1. 问题、目标与非目标

同材料更换来源、异材料替代与换配方具有不同的资格和产出因果。普通排产直接消费输入，不能兼任“记录一次替代选择”的动作；仅看到选择成功也不能声称材料已经转换。本设计冻结候选判定、明确确认、独立 decision、实际 conversion、数量/残余结算与恢复的完整关系。

成功结果是合法候选逐项可解释，preview 与 decision-only 不产生 sink、credit、conversion、稳定 `W` 或 reward；只有 profile 明示且实际发生的转换产生独立 linked settlement/provenance。任何切换、拆分、并发、重连和恢复都不能复制原料、转换、产物或进度。

不新增材料 taxonomy、品质等级、比例、产率、价格、损耗数值或自动替代策略；不接管站点选择、额外 Agent 制造或完整市场。其余工业身份、工作图、配方六状态和存量处置复用[工业工作图合同](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-identity)，不建立第二套 root/receipt 系统。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [合法替代与 no_legal_substitute](../../product/world-rules-core-gameplay/prd.md#root-sc30-subchoice)；Trace refs: O7-REQ-8a79f47e-f132-4467-8d09-d0ceacaac868 | 同料换源、显式异料替代、换配方分别判定，披露数量/ratio/rounding/residual、损耗/占用、value/风险/W/复查；未知不伪装安全。 | [候选与规则](#substitution-candidates) | Recipe/Factory/M4 profile、batch/ledger、物流/电力/terminal authority | 不新增材料或经济参数；缺规则不可执行。 |
| [合法替代与 no_legal_substitute](../../product/world-rules-core-gameplay/prd.md#root-sc30-subchoice)；Trace refs: O7-REQ-8a79f47e-f132-4467-8d09-d0ceacaac868 | 同料换源、显式异料替代、换配方分别判定，披露数量/ratio/rounding/residual、损耗/占用、value/风险/W/复查；未知不伪装安全。 | [接口](#substitution-model) | Recipe/Factory/M4 profile、batch/ledger、物流/电力/terminal authority | 不新增材料或经济参数；缺规则不可执行。 |
| [替代决定与实际 conversion 分离](../../product/world-rules-core-gameplay/prd.md#root-sc30-subsettle)；Trace refs: O7-REQ-300c6cbb-49f5-4858-a749-309991d5a5d0 | preview 无效果；fresh 明确确认只写一次 decision；decision-only 零经济/进度效果；实际转换有独立 linked 单次结算。 | [明确确认](#substitution-decision) | 工业工作图/事务、M4 converter、owner/capability 授权 | 不以普通 ScheduleRecipe 或 generic module 代签。 |
| [替代决定与实际 conversion 分离](../../product/world-rules-core-gameplay/prd.md#root-sc30-subsettle)；Trace refs: O7-REQ-300c6cbb-49f5-4858-a749-309991d5a5d0 | preview 无效果；fresh 明确确认只写一次 decision；decision-only 零经济/进度效果；实际转换有独立 linked 单次结算。 | [转换](#substitution-conversion) | 工业工作图/事务、M4 converter、owner/capability 授权 | 不以普通 ScheduleRecipe 或 generic module 代签。 |
| [替代决定与实际 conversion 分离](../../product/world-rules-core-gameplay/prd.md#root-sc30-subsettle)；Trace refs: O7-REQ-300c6cbb-49f5-4858-a749-309991d5a5d0 | preview 无效果；fresh 明确确认只写一次 decision；decision-only 零经济/进度效果；实际转换有独立 linked 单次结算。 | [事务恢复](#substitution-persistence) | 工业工作图/事务、M4 converter、owner/capability 授权 | 不以普通 ScheduleRecipe 或 generic module 代签。 |
| [替代因果与防套利](../../product/world-rules-core-gameplay/prd.md#root-sc30-subcause)；Trace refs: O7-REQ-84ea0d88-f33b-4ac1-9722-ca4892df9152 | 全产出因果不变才保留 pinned recipe/child decision，其余新 candidate W=0；首 sink 后仅受支持单次处置，切换/拆分/重试不套利。 | [因果与存量](#substitution-causality) | M4 量化/残余、recipe causal fingerprint、cutover/DispositionPlanV1 | 不迁移旧 receipt、hold、顺位、W、delivery 或 reward。 |
| [替代因果与防套利](../../product/world-rules-core-gameplay/prd.md#root-sc30-subcause)；Trace refs: O7-REQ-84ea0d88-f33b-4ac1-9722-ca4892df9152 | 全产出因果不变才保留 pinned recipe/child decision，其余新 candidate W=0；首 sink 后仅受支持单次处置，切换/拆分/重试不套利。 | [转换](#substitution-conversion) | M4 量化/残余、recipe causal fingerprint、cutover/DispositionPlanV1 | 不迁移旧 receipt、hold、顺位、W、delivery 或 reward。 |
| [替代因果与防套利](../../product/world-rules-core-gameplay/prd.md#root-sc30-subcause)；Trace refs: O7-REQ-84ea0d88-f33b-4ac1-9722-ca4892df9152 | 全产出因果不变才保留 pinned recipe/child decision，其余新 candidate W=0；首 sink 后仅受支持单次处置，切换/拆分/重试不套利。 | [恢复](#substitution-persistence) | M4 量化/残余、recipe causal fingerprint、cutover/DispositionPlanV1 | 不迁移旧 receipt、hold、顺位、W、delivery 或 reward。 |

## 3. 当前状态、目标状态与差距

| 对象 | 固定审读时基线 | 目标 | 差距与证据边界 |
| --- | --- | --- | --- |
| 配方 profile | [RecipeProfileV1](../../../crates/oasis7_wasm_abi/src/economy.rs) 描述 recipe、瓶颈、阶段与工厂偏好；没有 profile-declared 替代规则。 | `SubstitutionRuleV1` 明确材料、适用性、数量、全产出影响及 conversion 模式。 | 普通输入表不能推导异料兼容。 |
| 执行动作 | [工业 Action](../../../crates/oasis7_agent_api/src/world_action/action.rs) 与 [recipe lifecycle reducer](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs) 提供普通排产及其消费/产出。 | 无效果 decision 和独立转换工作/结算。 | 现有单 job/去重不能证明替代身份或残余防套利。 |
| 报价与恢复 | [M4 报价设计](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design) 已有领域报价边界。 | 候选→确认→转换→下游消费有完整单案记录。 | 本文补专业执行设计；专用 producer、schema 编码、正式三端和运行验收仍须实现。 |

## 4. 边界与结构

Recipe/Factory/M4 拥有有限替代集合、材料/品质/custody 适用性、量化与产出关系；Runtime 拥有 canonical 输入、权限、工作图、账本及单次提交。Viewer、Agent 和模型只提出选择或请求确认，不能从同名、市场可买、`degraded` 或旧 receipt 推断兼容。所有生效输入遵守[共同权威约定](../design.md#runtime-gameplay-authority-designs)，不在确定性执行时用节点实时 HTTP 或模型意见重新判定历史。

`SubstitutionDecisionV1` 是工业工作图中的独立 decision child。`ConversionOperationV1` 是引用该 decision 的独立工作 child；普通生产、交付与奖励仍是其他 effect role。只读预览不创建这些记录，decision 也不自动变成 conversion、input join、production 或 terminal settlement。

## 5. 关键运行流程

<a id="substitution-candidates"></a>
### 5.1 候选与 profile 合法性

三种候选有不同证明条件，不能只用统一的 `compatible=true`：

| candidate kind | 合法性与身份 | 不能继承 |
| --- | --- | --- |
| `same_material_source` | 材料 profile 身份不变，但新 source/ledger/path、owner、provenance、quality/custody、数量与当前 required input 的适用性全部重验。 | 旧 source 的 hold、物流顺位或未来供给保证。 |
| `profile_substitute` | 当前 pinned recipe 的准确 input edge 声明该有限替代规则，规则及所有 mandatory authority 完整有效。 | 未声明的等量、零损耗、相同产出或安全等级。 |
| `new_recipe` | 新 recipe/version/factory、全部输入/输出/副产物、路径、电力、terminal 和当前生命周期准入重新判定。 | 旧 candidate 的进度、receipt、reservation、顺位与已消费价值。 |

`SubstitutionRuleV1` 必须声明：准确 recipe/version/input edge、允许材料 profile 集、接受的规格/品质与 custody、owner/ledger/path 条件、正整数有理数量比例及单位、canonical executable quantum、rounding 模式及适用聚合边界、residual disposition、损耗、额外 power/time/logistics/buffer/terminal 义务、全 output/value/quality/byproduct 影响、是否需要 conversion 及 converter profile、适用输入状态和复查条件。缺字段即规则不可用，不自动补 1:1、零损耗或无期限。

数量由专业 authority 用 checked integer/rational arithmetic 计算，保留分子/分母、量化前需求、admissible/committed 数量、residual/unmet 和结果单位。不能按客户端浮点数或材料字符串求比例；溢出、单位不匹配或非正比例拒绝。partial 只有 recipe 与转换 profile 均明确支持时可用，零 executable quantum 无 sink。ratio 数值仍是 profile 输入，本设计冻结验证/计算边界而非新的产率。

只读候选集合绑定同一 blocked input、root/revision/child、pinned recipe、policy/profile、batch/ledger 状态版本和候选摘要。候选逐项显示投入/损耗/占用、已持有/已消费价值、预期 output/value/terminal、因果影响、风险、`next_action/next_recheck`。等待/补标准料、减量/延期和 pivot 只有当前 profile 真实支持时才展示；推荐不建立授权。已检查完有限合法集合且无候选时返回 `no_legal_substitute`；证据不完整为 `unknown/blocked`，不能将未检查完伪装成确定没有或确定安全。

<a id="substitution-decision"></a>
### 5.2 明确确认与唯一 decision

确认请求绑定准确 preview/candidate digest、actor 与 owner/代理 capability、input slot、batch/数量、所选规则、因果分类以及明确允许的动作范围。动作范围显式区分 `decision_only` 与 `decision_and_exact_conversion`；后者仅允许预览中已披露的转换与上限，不能在失败后自动换料/换配方。涉及因果换向时还须绑定完整 cutover/disposition plan；没有确认其范围则只能记录选择并等待后续明确确认。

确认在同一 canonical snapshot 重新校验 profile/recipe 生命周期、batch provenance/quality/custody/applicability、owner/ledger、数量/ratio/残余、route/capacity、power、所有 output/terminal 与授权。任何选项、金额、范围或代价漂移都要求新预览/新确认，不能自动替用户接受“更好的”替代。原候选无效并不阻断仍合法的标准原料等待/补料路径。

确认事务仅记录 `SubstitutionDecisionV1`、其 parent-linked child 及不可变依据/唯一结果。`decision_only` 下世界资源、预算 debit、hold、join sink、conversion、production、`W` 和 reward 的变化全部为零；消费“记录选择”的幂等身份不消费实际转换的额度。decision receipt 的 effect role 固定为 `substitution_decision`，下游不得把它当 settlement receipt。

decision 不是资源预留或永不过期的执行权；后续转换/排产仍以当前 authority 重验。已确认选择不可改写；改变选择建立新的 linked decision，并先确认旧转换是否已开始/结算或处置，不能靠新 decision ID 重用同一批次或 input slot。

<a id="substitution-conversion"></a>
### 5.3 实际转换、结算与残余

只有 `conversion_mode=required` 且明确授权覆盖该准确转换时，执行器才创建转换操作；`conversion_mode=none` 不能伪造 conversion receipt，只能在后续正常 join/production 提交时消费实际已到达且适用的原批次。`decision_and_exact_conversion` 允许在确认后请求原操作，但网络重连或阻塞解除不能扩展授权；若需改候选、cutover 计划、上限或范围必须重新确认。

转换开始前重验 decision 内容、当前权限/recipe/转换 profile、批次可独占量、目标 input/join、输出去向与容量，以及全部必需能源/物流/时界。源未在可消费 ledger 或仍在途中时不能瞬移；先由物流协议产生实际到达/接收，再重验。转换要么在一个事务内原子结算，要么使用 profile 明确声明的多 tick 模式：

| 模式/边界 | 原子变化 | 证明范围 |
| --- | --- | --- |
| 即时转换 | 源数量/授权额度扣减、所有输出/残余/有界损耗、目标 ledger/容量、已消费 lot 标记、操作终态及 linked settlement/provenance receipt 一起提交。 | 真正转换已结算；不是原目标生产、交付或奖励。 |
| 多 tick start | 输入 sink、该边界电力/容量义务、conversion WIP、remaining obligations、固定 converter/version 与 start receipt 一起提交。 | 已授权不可逆投入；start receipt 不是产物 settlement。 |
| 多 tick finish | 当前必需条件仍有效或存在明确允许的 drain authority 时，实际输出/副产物/残余/损耗、WIP 终结、容量/能源义务处置、唯一 settlement 一起提交。 | 只有 matching settlement 才可作为后续 join 的输入来源。 |
| finish 条件失效 | 按启动时已声明的有界 hold、quarantine、return、rework、salvage 或 terminate 路径，保留已消费量及未结义务。 | 不凭空退款、不先 credit 后补证，不遗失 WIP 或将失效写成已完成。 |

不支持安全 WIP/输出保持及截止处置的多 tick profile 不得启动；不能用“未来再补恢复”接受不可逆 sink。每个边界是单独事务，开始与完成之间的合法在制状态不是半笔失败；任一边界失败不能留下该边界的半扣减/半入账。原料转换本身不增加原生产目标的 `W` 或 reward，实际生产/交付仍分别验证。

转换守恒按 profile 的领域单位和变换关系检查，不比较不同材料的裸数字：输入各 material/unit 的实际 sink、目标数量、声明损耗及 residual 必须满足同一确定转换方程。residual 只能按声明 `retain/carry/return/declared_loss/reject` 处置并有唯一 owner/ledger/lot；不能凭空入第二个 join。rounding 在 profile 声明的 canonical batch/cycle 聚合域执行，拆分请求不能重置余数；可结转残余记录在原 lot/转换计量域，消费或结转均单次对账。

<a id="substitution-causality"></a>
### 5.4 因果分类、切换与首 sink 后处置

因果等价由 pinned profile 的明确声明和完整字段比对共同证明，必须涵盖产出、品质、power、时间、副产物、terminal 与 value class；缺一个证据即不能认定等价。全项不变时保留 pinned recipe 和生产 candidate，追加 child decision segment，记录实际 batch、比例、rounding/residual、损耗及父 receipt；即使无需切换，新的 batch/ledger/权限仍要重验。

同因果换源不改变已冻结的 required-slot 拓扑、数量、spec、执行 quantum 或时界义务。对于尚未 sink 的 slot，`PrepareSubstitutionInputBinding` 只读生成绑定原 decision、expected binding revision 与准确新 parent settlement/lot 的 plan，复用[工业 join 合同](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-join)的 `ParentBindingRevisionV1`，不另建平行 schema；decision-only 不安装该 binding，也不释放旧 hold。后续正常 join/start 提交调用工业权威 `ApplyParentBindingV1`，在同一事务中重验全部 parent、按已确认原身份至多释放一次旧未消费 hold、为新来源重新取得合法 reservation/容量分配、保留旧 binding provenance 并安装受权 binding revision；不继承旧来源顺位。改变来源不能重置 `(join_id,parent_slot,parent_sink)` 消费键。需要 conversion 的来源必须等其真实 settlement 已 committed 且有 matching receipt 后才能绑定为可消费 parent，start/decision receipt 均不适用。已 sink 的 slot 不允许此接口，任何拓扑/数量/spec/产出因果变化走新 candidate/cutover。

任一因果项变化或属于 `new_recipe` 时，decision 指向待建立的 parent-linked 新 candidate；旧候选不能在 decision-only 阶段被隐式激活/改写。只有[受控 cutover](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-cutover)在明确授权范围内原子处置旧工作并创建新 candidate/child root，初始 `W=0`。decision 已确认不代表 cutover 已发生；实际新 identity 从 cutover receipt 取得。旧 receipt、reservation、queue priority、actual、delivery 和 reward 不移到新身份。

首个不可逆 sink 后，禁止把旧 batch/WIP/transit/buffer 重贴为替代输入。只能从 `DispositionPlanV1/DispositionItemV1` 选择 profile 支持的单次路径；hold 的选择、后续受权 resume/finish 或修改未终结计划分别有 linked 操作，不能因“单次”误禁合法继续，也不能把每次继续当新源。取消、返工、转换、回收与完成竞争同一旧 lot/义务，已完成或释放的量不可再次消费。

支持跨旧/新候选的 handoff/conversion 时，来源 receipt 始终属于原 root，目的操作引用它并证明当前适用性/数量，不能改写所属 root。终端目的、recipient、容量和 ownership 只按对应授权转换；同名材料、新 decision 或新 converter version 不能洗掉来源、已损失价值和剩余义务。

## 6. 接口与数据合同

<a id="substitution-model"></a>
逻辑 schema 固定为 `recipe-substitution/v1`。复用[工业模型](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-model)的 `RootOperationV1/RevisionV1/ChildRefV1/WorkNodeV1/IndustrialReceiptV1`；所有外部引用带 world、domain/module instance、schema 与准确 revision/digest，不能仅传裸 batch/decision ID。数量为非负整数加 domain unit，比例为正整数 numerator/denominator；所有运算 checked，集合 canonical 排序且不得重复。字节编码沿既有版本化 ABI 发布，不以 `serde(default)` 使缺少的 authority/比例变合法。

| 记录 / 接口 | 必需内容与封闭值域 | producer → consumer / 约束 |
| --- | --- | --- |
| `SubstitutionRuleV1` | rule ref、recipe/version/input edge、有限材料/规格/custody、数量/单位/ratio/quantum/rounding/聚合域/residual、损耗与资源/输出影响、conversion mode(`none/required`)、converter ref 或显式不适用、适用状态/处置/复查 | Recipe/Factory/M4→候选与执行；缺任何适用必需项拒绝，发布后不可改写旧版本。 |
| `SubstitutionPreviewV1` | preview digest、root/revision/target input slot、baseline/profile/state refs、候选有序集合；每候选 kind/来源/数量/代价/因果/风险/W/next action/recheck | 只读 evaluator→Viewer/API/Agent；无候选区分 `no_legal_substitute` 与 `unknown/blocked`，不分配世界 identity 或 receipt。 |
| `ConfirmSubstitutionV1` | request key、chosen preview/candidate digest、owner/actor authority、动作 scope、明确上限、expected state refs、可空的 exact cutover plan digest | 已认证主体→Runtime；空 plan 只表示未授权 cutover，不能默认同意。 |
| `SubstitutionDecisionV1` | decision `ChildRefV1`、confirmation intent/key、parent refs、target input/join slot、candidate kind/批次/数量/规则、causal verdict、authority/state refs、授权动作范围、decision receipt | Runtime→工作图/转换器；不可变 confirmed 选择，经济 effect role 为零；后继取消/失效引用原记录。 |
| `PrepareSubstitutionInputBinding` | 只读 plan 引用工业 `ParentBindingRevisionV1`：原 join/slot、expected binding revision、decision、完整旧/新 parent settlement/lot、原固定拓扑/quantity/spec/quantum/time 和受权 hold disposition | 只读准备→正常 join/start 的 `ApplyParentBindingV1`；字段、安装和去重合同由工业 join 拥有，仅未 sink slot 可追加 binding。 |
| `ConversionOperationV1` | conversion child、decision ref、source lot refs、target input/join、converter version、quantity/units/rounding domain、owner/payer/authority、mode、phase、WIP/remaining obligations、start/settlement/disposition refs | Converter/M4→Runtime；phase 为 `pending/running/held/settled/terminated/rejected/expired`；不存在无需 settlement 的 completed。 |
| `ConversionSettlementV1` | 工业 receipt identity、decision/conversion/父来源 refs、实际 source sink、output/residual/loss 向量、ledger/owner、authority revisions、前后状态与 commit、是否可消费的用途范围 | Runtime→join/production；effect role 为 `conversion_settlement`，唯一消费链接，不是 production/delivery receipt。 |
| `ReadSubstitutionV1` | decision/conversion 全限定引用、source commit、调用者读权限 | 权威投影→三端；同时返回选择、执行阶段、真实结算、已消费/可追回/损失、blocker、下一动作/复查，重连只读。 |

确认幂等键为 `(world, module_instance, owner, confirmation_intent_key)`，绑定 payload digest；真正效果继续由 `(root, revision, child, effect_role)` 去重。相同确认键同内容返回原 decision，不同内容 `idempotency_conflict`。另由 canonical input/join slot 的当前 decision/消费状态和 source lot 的数量账本约束换键/拆分：新的确认不能使同一已 sink 量再次可用。转换 start、settlement、每次合法处置各有独立 effect role，并引用同一原操作。

错误集合为 `unsupported_schema / unsupported_substitution / missing_authority / not_applicable / stale_candidate / owner_mismatch / quantity_or_unit_invalid / quantization_blocked / resource_unavailable / causal_change_requires_cutover / conversion_not_authorized / unsupported_disposition / idempotency_conflict / commit_unknown`；返回准确 identity、已知结果/未决义务和可执行下一步，不从 timeout 推断未消费。`degraded` 只表示 profile 明确接受的品质等级，不是错误或未知的兼容别名。

### 6.1 原生 profile 与 authority 绑定

`RecipeProfileV1.substitution_rules` 发布有限的 `SubstitutionRuleV1` 集合。旧 profile 缺少该字段时只表示没有异料替代声明，不能默认生成 1:1 规则；空集合保持旧 profile 的编码。非空规则必须有明确版本引用、所有适用性、比例/单位/量子、rounding/residual、损耗、各因果影响与复查/处置政策。规则集合按稳定 rule identity 排序且唯一；缺少必需字段、未知值域、非正比例、溢出、超限或不匹配的 recipe 身份均拒绝。bootstrap、治理与 module release 使用相同校验。

整数数量计划计算整个已接受的 canonical batch/cycle 域，而不是调用者任意拆分的 delta。计划保留 source requirement、显式 loss、target executable 与有理 residual；计算函数本身不授予权限。执行器持久化原 domain 和其单次消费，拆分、换键、换 decision 或 retry 不能重新获得舍入收益。

规则引用不是外部证据本身。原生 execution authority 必须由现有受信 finality authority 对完整世界、root/slot、profile、来源 lot、owner、路径/品质、资源和所有因果影响作准确认证，并在执行时与当前 canonical 依赖重验。该 authority 不增加库存；实际来源只来自共享工业托管。未解析的 profile 引用、缺失 batch authority 或证据过期显示 unknown/blocked，不能仅凭字符串相等声称安全。转换只支持已实现且明示的模式；未支持的模式在 sink 前拒绝。

### 6.2 原生能力发现与明确操作

授权 `world_rule_status.recipe_substitution` 是 API、Agent 与 Viewer 的同一读面；包含 `reader_agent_id`、root、完整 preview、decision 与独立 settlement。只有当前经济 owner source 匹配的记录可见，同一 Agent ID 被重新取得不会转移旧私密记录。Viewer 的 chain 模式只读取已验证的私有 projection，缺失时不以本地重建世界补齐。超出读取或 preview 字节上限明确报告，不能把截断集合当完整合法选择。

`WorldRuleCommandV1::IndustrialWork` 提供认证批次导入、接受 root、正常 `StartRoot`、单次 `TerminateRoot` 和未消费余额释放。`RecipeSubstitution` 提供认证 authority 发布、`Confirm`、`Convert`、`ApplyBinding`、`Cutover`；每个操作仍需要当前 actor、准确对象的 native grant 与 canonical admission。确认直接携带读取的完整 preview 和候选摘要、明确 scope 与上限；新配方 cutover 的完整处置 plan 与 digest 由 preview 提供，客户端不能靠猜测序列化顺序重建授权摘要。

同料换源直接核对已认证 baseline 和源 lot，数量与单位完全一致，不依赖异料规则，也不产生 conversion。异料替代只使用有限显式规则；当前原生转换模式是同 ledger 的即时原子转换，`duration_ticks != 0` 或无需转换的异料直投在首 sink 前明确拒绝。原生跨材料且改变产出因果的转换不冒充等价转换；换配方引用另一个已认证、未 sink 的 root，预览在私有状态上执行正常 StartRoot guards 后，明确 cutover 只建立旧/新身份的 receipt 关系，再由新 root 正常生产。当前不宣称多 tick converter、任意 post-sink rework 或 #4856 其余独立叶子的实现。

原生认证 authority 逐项声明风险、下一复查时刻和带单位的 held/consumed/expected-output 估值；这是该 profile 的机会成本说明，不是可支配货币、credit 或 reward。所有字段均进入签名及候选摘要，不能补默认安全、零估值或免费影响。各 input 和 output 的整数单位、recipe/profile、品质、保管、owner、工厂、ledger、材料余额、电力与有限 terminal 容量在实际边界重验；normal StartRoot 和最终产出共同遵守冻结的 per-material output 上限。终端权限或容量失效时原 pending job 保持原数量与损耗，产生可见 blocker，条件未改变不反复追加同一 blocker；恢复后只结算一次，不能重新扣输入。

首 sink 后当前原生显式支持 baseline 声明的 `Terminate`：关联准确原 job、保留已消费材料和电力的 loss，移除原 pending commitment 并只写一次处置 receipt；不退款、不生成产物。未声明或未实现的 Hold/rework/return/salvage 均拒绝，不能用普通暂停伪造安全 WIP 迁移。已完成的 job 不能再终止；终止的未 sink 绑定可以释放仍未消费的托管量，但任何仍有效的新 root 绑定继续阻止释放。

## 7. 状态、事务与持久化

<a id="substitution-persistence"></a>
decision 的已确认内容不变；其可执行性由当前 recipe/authority、input slot 和后继失效/替代记录计算。转换 `pending` 无 sink/hold，只有明确授权的 start 才可转 `running`；即时模式直接由 pending 到 settled。running 可在支持条件下到 held、settled 或 terminated；held 仅在新鲜合法继续授权下恢复，settled/terminated 为终态。rejected/expired 限于尚未产生 start 效果的尝试；已经 running 的过期必须保留 WIP/义务并作受支持处置，不能伪装为无效果拒绝。

decision-only 事务只写选择、child、幂等与 decision receipt。每次实际转换事务共同提交账本/批次、必要授权额度或消费标记、power/容量、WIP/残余、phase、幂等及对应 receipt；下游不得在其中任一项尚未提交时看见可消费材料。遵守[统一事务 disposition](../design.md#622-disposition-and-idempotency-contract)，无法确定持久化是否发生时围住确切操作，不能新建 conversion 或先退款。

恢复必须同时载入 decision/confirmation、converter/profile 的固定版本、root/revision/input/join、源 lot 消费与残余、已结输出、WIP/remaining obligations、授权/撤销、时间与 receipt/去重索引。缺任何必需依赖时可诊断读取但不执行；不能用新版比例重新计算旧 WIP、以当前余额反推未 sink，或从 decision 补造 settlement。canonical replay 重建已记录效果，不发新转换/物流请求、不增加时间、稳定 `W` 或奖励。

同一个源 lot 在转换、普通 join、return 与 salvage 之间通过同一账本/工作图争用；同 tick 稳定顺序及 expected revision 决定合法赢家，失败方不得观察部分输出。源消费与目的可消费资格共同可追溯；conversion settlement 只授予其声明范围的材料使用，不自动完成 production、terminal、需求扣减或 reward。

## 8. 部署、安全与运行约束

转换规则由经授权的 profile 发布入口提供，Runtime 校验其 schema、内容摘要、作用范围和当前激活/退役状态。纯 API、Viewer、Agent、module command 与恢复路径使用同一 guard，不另设跳过明确确认或 fresh revalidation 的内部快捷入口。Decision 权限与 converter 消费权限分开；签过 decision 不代表可以任意扣所有者材料。

材料、quality/custody、ledger/path 和当前授权证据须已进入可验证的 canonical 输入；外部查询只可准备证据，不决定提交的节点本地结果。资源/证据超限、执行超时、profile 缺失只阻断相关替代，不阻断标准料路径；不能把异常回退为免费替代、混料或自动降级。

## 9. 质量与容量

| 场景 | 环境 / 刺激 | 判定指标与依据 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 候选合法性 | 同一 input fixture，三类候选、未声明异料、同名不适用、degraded/unknown 成对样例 | 未支持候选进入可执行集次数为零；每项数量/成本/因果完整，依据 SC-30 | [SUB-VERIFY-001](#sub-verify-001) | 目标，未运行。 |
| 无效果决定 | preview/确认、重复确认与 stale authority | preview 无世界写；decision-only 所有资源/进度 delta 为零 | [SUB-VERIFY-002](#sub-verify-002) | 目标，未运行。 |
| 转换守恒 | 即时/多 tick、分拆数量、rounding/residual、并发 lot 与输出容量冲突 | 每个 start/settlement/disposition 至多一次；转换方程成立且 residual 不重复 | [SUB-VERIFY-003](#sub-verify-003) | profile 参数按案例固定，未运行。 |
| 因果与恢复 | 全因果等价/逐项改变、sink 后切换、提交故障、重连/replay | 无身份洗白、W/receipt 迁移、双结算或收益复制 | [SUB-VERIFY-004](#sub-verify-004) | 目标，未运行。 |
| 有界计算 | 候选、source lots、output branches 超过版本化容量 | 预检明确拒绝，无截断规则/输出或无限枚举 | [SUB-VERIFY-005](#sub-verify-005) | runtime/M4 配置确定容量；不声明性能数值。 |

## 10. 兼容、迁移与回滚

旧 profile 不含替代规则时仍可走原有普通排产，不自动填充等量替代或转换。新 schema 先加入只读能力发现/解析和拒绝逻辑，再显式开放受支持规则及正式确认入口；旧客户端不能把缺失字段解释成 `conversion_mode=none` 或“无风险”。

旧 RecipeJobState、普通生产 receipt 和已到达材料不升级成 substitution decision、conversion settlement 或完整 batch lineage。可由明确兼容 adapter 读取其原事实，不能补造父 decision；需要新能力的未消费输入经新请求/新规则重新确认。profile 退役/替代消费[配方六态和旧工作处置](industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)，不重新定义退役状态。

回滚只能回到能读新 decision、lot/残余、WIP 与 settlement 且冻结新准入的兼容版本。不可逆 input sink 与已结输出不因禁用替代被撤销；未结操作保留其固定 converter 版本与合法处置能力，不能用旧 binary 忽略其占用后再次排产。旧版本/证据不可取回时保持有界待决并由对应 owner 处理，不自动退款或重算。

## 11. 验证设计与可追溯性

以下场景 ID 是实现验收协议，尚不表示已有专用测试。每个成对样例固定 source/build、recipe/converter/profile、world、source lots、权限与 blocked-input 原因，记录 producer→工作状态/receipt→授权读面；required/full 保留 [SC-30 原层级](../../product/world-rules-core-gameplay/prd.md#51-验收追踪)。

<a id="sub-verify-001"></a>
- **SUB-VERIFY-001 / required**：同料换源、显式异料、换配方、无合法替代各一例；删去 ratio、quality/custody、owner/provenance、route 或 terminal 证据，profile 未声明及过期，degraded 接受/拒绝成对。检查完整候选字段和 no_legal_substitute/unknown 区别，三端不得推荐虚假安全选项。
<a id="sub-verify-002"></a>
- **SUB-VERIFY-002 / required**：preview 前后比较 canonical state/journal/counters；decision_only 明确确认后验证仅 decision 元数据和回执新增，材料/电力/预算/hold/W/reward 无变化；重复/并发确认同结果，换内容同 key 拒绝；任何 fresh 条件漂移不生成第二选择或转换。
<a id="sub-verify-003"></a>
- **SUB-VERIFY-003 / required + full**：即时与多 tick conversion 验证真实 start/settlement、input slot、source lot 与 parent decision 匹配；不需 conversion 的候选无 conversion receipt。对拆分、partial、最小 quantum、rounding/residual、输出容量失效和源批次并发争用做守恒断言；full 加多阶段 join、品质/保管漂移、in-transit/buffer、跨 root 争用和完整持久恢复。
<a id="sub-verify-004"></a>
- **SUB-VERIFY-004 / required + full**：分别改变 output、quality、power、time、byproduct、terminal 和 value class，证明任何一项变化需新 candidate W=0，全项不变才保留 pinned recipe/child decision；首 sink 前拒绝无效果，首 sink 后只能受支持处置。每个提交点故障、开始/结算响应丢失、重连/Agent retry、切换/拆分/乱序/restore/replay 不复制 sink、residual、conversion、output、W 或 reward；full 覆盖跨窗口 cutover。
<a id="sub-verify-005"></a>
- **SUB-VERIFY-005 / required**：Viewer/pure API/Agent 对同一 preview、decision-only、running/held/settled、unknown 结果读取相同候选类型、实际量/损耗/lineage、blocker、下一步与复查；检查读权限及超限集合不截断成合法方案。正式 Agent 功能实现按[自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md)取得真实操作与世界结果，文档检查不代签。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [root-sc30-subchoice](../../product/world-rules-core-gameplay/prd.md#root-sc30-subchoice)；Trace refs: O7-REQ-8a79f47e-f132-4467-8d09-d0ceacaac868 | [候选](#substitution-candidates) | 三种候选分开、成本/数量/因果可比较、默认禁止及未知保守拒绝。 | [系统测试手册](../../../testing-manual.md)；按本文场景 SUB-VERIFY-001、005；同 blocked-input、profile 的 required 及 full 组合；专用合同回归由实现提供，本文仅规定验收。 | profile/候选原始输出、权威状态、三端语义；实际 PR 记录版本与结果 | 不证明未提供材料规则或数值平衡。 |
| [root-sc30-subchoice](../../product/world-rules-core-gameplay/prd.md#root-sc30-subchoice)；Trace refs: O7-REQ-8a79f47e-f132-4467-8d09-d0ceacaac868 | [模型](#substitution-model) | 三种候选分开、成本/数量/因果可比较、默认禁止及未知保守拒绝。 | [系统测试手册](../../../testing-manual.md)；按本文场景 SUB-VERIFY-001、005；同 blocked-input、profile 的 required 及 full 组合；专用合同回归由实现提供，本文仅规定验收。 | profile/候选原始输出、权威状态、三端语义；实际 PR 记录版本与结果 | 不证明未提供材料规则或数值平衡。 |
| [root-sc30-subsettle](../../product/world-rules-core-gameplay/prd.md#root-sc30-subsettle)；Trace refs: O7-REQ-300c6cbb-49f5-4858-a749-309991d5a5d0 | [确认](#substitution-decision) | preview/decision-only 无效果，转换明确受权且真实单次结算，失败无半笔提交。 | [系统测试手册](../../../testing-manual.md)；按本文场景 SUB-VERIFY-002、003、004；即时/多 tick、故障/并发/恢复层；专用合同回归由实现提供，本文仅规定验收。 | 决定与转换独立记录、input/output/residual 对账、linked receipt 与重试结果 | 普通排产成功、类型存在或静态结构检查不证明本链。 |
| [root-sc30-subsettle](../../product/world-rules-core-gameplay/prd.md#root-sc30-subsettle)；Trace refs: O7-REQ-300c6cbb-49f5-4858-a749-309991d5a5d0 | [转换](#substitution-conversion) | preview/decision-only 无效果，转换明确受权且真实单次结算，失败无半笔提交。 | [系统测试手册](../../../testing-manual.md)；按本文场景 SUB-VERIFY-002、003、004；即时/多 tick、故障/并发/恢复层；专用合同回归由实现提供，本文仅规定验收。 | 决定与转换独立记录、input/output/residual 对账、linked receipt 与重试结果 | 普通排产成功、类型存在或静态结构检查不证明本链。 |
| [root-sc30-subsettle](../../product/world-rules-core-gameplay/prd.md#root-sc30-subsettle)；Trace refs: O7-REQ-300c6cbb-49f5-4858-a749-309991d5a5d0 | [恢复](#substitution-persistence) | preview/decision-only 无效果，转换明确受权且真实单次结算，失败无半笔提交。 | [系统测试手册](../../../testing-manual.md)；按本文场景 SUB-VERIFY-002、003、004；即时/多 tick、故障/并发/恢复层；专用合同回归由实现提供，本文仅规定验收。 | 决定与转换独立记录、input/output/residual 对账、linked receipt 与重试结果 | 普通排产成功、类型存在或静态结构检查不证明本链。 |
| [root-sc30-subcause](../../product/world-rules-core-gameplay/prd.md#root-sc30-subcause)；Trace refs: O7-REQ-84ea0d88-f33b-4ac1-9722-ca4892df9152 | [因果](#substitution-causality) | 全因果判定、cutover 新身份/W=0、sink 后有界处置，拆分/切换不套利。 | [系统测试手册](../../../testing-manual.md)；按本文场景 SUB-VERIFY-003、004、005；完整因果维度与原 SC-30 full 范围；专用合同回归由实现提供，本文仅规定验收。 | 旧/新工作图、disposition、receipt/数量/进度及恢复对账 | 不宣称当前可玩、正式网络或 release readiness。 |
| [root-sc30-subcause](../../product/world-rules-core-gameplay/prd.md#root-sc30-subcause)；Trace refs: O7-REQ-84ea0d88-f33b-4ac1-9722-ca4892df9152 | [转换](#substitution-conversion) | 全因果判定、cutover 新身份/W=0、sink 后有界处置，拆分/切换不套利。 | [系统测试手册](../../../testing-manual.md)；按本文场景 SUB-VERIFY-003、004、005；完整因果维度与原 SC-30 full 范围；专用合同回归由实现提供，本文仅规定验收。 | 旧/新工作图、disposition、receipt/数量/进度及恢复对账 | 不宣称当前可玩、正式网络或 release readiness。 |
| [root-sc30-subcause](../../product/world-rules-core-gameplay/prd.md#root-sc30-subcause)；Trace refs: O7-REQ-84ea0d88-f33b-4ac1-9722-ca4892df9152 | [恢复](#substitution-persistence) | 全因果判定、cutover 新身份/W=0、sink 后有界处置，拆分/切换不套利。 | [系统测试手册](../../../testing-manual.md)；按本文场景 SUB-VERIFY-003、004、005；完整因果维度与原 SC-30 full 范围；专用合同回归由实现提供，本文仅规定验收。 | 旧/新工作图、disposition、receipt/数量/进度及恢复对账 | 不宣称当前可玩、正式网络或 release readiness。 |

## 12. 决策、长期风险与未决问题

选择“决定与转换独立记录、每个真实边界独立原子结算”，允许玩家保存选择而不提前扣料，也容纳有时间成本的合法转换。代价是读面必须同时表达选择、执行和结算，不能再以一个成功图标覆盖整条链。

M4/Recipe owner 在支持某一规则前必须提供其比例/单位、量化聚合域、残余、全产出因果、converter 及在制/失效处置政策；缺项只使该规则未支持，不改变本文已冻结的身份、事务和恢复协议。替代集合、ratio/rounding、causal fingerprint、converter 或 recipe lifecycle 变更触发新版本与上述守恒/反套利复核，不能重写已确认的 decision 或旧 WIP。
