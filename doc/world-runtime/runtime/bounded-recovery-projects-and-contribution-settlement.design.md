# 有界恢复项目与贡献单次结算系统设计

- 状态：`target` 技术合同；冻结项目、贡献、里程碑、授权消费与恢复的逻辑 schema，不声明代码实现、实际恢复效果或部署就绪。
- Owner：`gameplay_designer` 负责恢复目标、参与资格与政策；`runtime_engineer` 负责校验、事务、持久化与恢复；治理专业 owner 负责 containment 和项目授权；M4/资源与受损对象 owner 负责真实投入及效果；Agent、Viewer/API、QA 负责消费者与组合验证。
- 审读基线：`nil-studio/oasis7@e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`；范围来源 [#4863](https://github.com/nil-studio/oasis7/issues/4863)，只承接贡献型恢复项目。

## 1. 问题、目标与非目标

普通 `Crisis` 的 active/resolved/timedout 和资源 impact 已存在，但它没有系统性 containment、项目范围、贡献资格、里程碑或单次项目效果身份。目标是在同一世界中以已授权的有界项目恢复具体服务、物流、能力或公共条件，让每次恢复效果都能追溯项目、参与者、实际贡献、资源义务和权威里程碑收据。

containment 限制扩散并保护基本连续性；恢复项目产生独立的、按规则确认的修复效果。两者分别记账。项目不能把普通危机 `success=true`、Agent 自报完成、治理票数或存储恢复成功当作贡献，更不能 reset 世界、改写损失、免除责任、选择性 bailout 或免费发资源。本文不制定奖励金额、危机触发阈值、预算、资格数值；不接管普通危机、战争、软赛季、工业图、charter 退化恢复或存储恢复。

## 2. 上游约束与相关角色

产品权威为 [REQ-WR-CC-003](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)及 [恢复项目生命周期](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#61-恢复项目生命周期与结果边界)。项目必须在有效 containment 之后开放，原 AC-6/7 以及 test_tier_full 组合边界保持，不由普通 Crisis 或工业维护测试替代。

### 2.1 需求承接与分配表

| 上游 requirement / product AC（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-CC-003](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | 系统性危机后公开项目范围、资格、资源义务、期限、里程碑、退出/申诉；完整记录作为开放前置 | [持久模型](#recovery-model) | gameplay/治理政策、Runtime、资源与效果 owner | 不新定政策值；不证明普通 Crisis 满足本条 |
| [AC-6](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-6)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | 多区域/公共条件危机的 containment 与规则化恢复独立，保留历史、审计与申诉且目标真实变化 | [authority](#recovery-authority) | 宪制化 containment 授权，受损对象权利保护 | 不替代共识灾备、重开世界或外部赔偿 |
| [AC-7](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | 两个不同 scope 项目中，贡献、source claim、参与和资格各自有效且不跨项目复用 | [贡献](#recovery-contribution) | Runtime、source 资源 owner、Agent/API | 不以普通 job 去重代签 |
| [AC-7](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | evidence-only decision 与真实资源/恢复效果分开，每个贡献和 milestone slot 仅一次结算 | [里程碑](#recovery-milestone) | Runtime、target/资源 writer、政策预算 | 不制定奖励数值和不支持的域外效果 |
| [AC-7](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | scope/revoke/cancel/expire 与完成竞争，退出/申诉及失败后的常态下一步可读 | [退出申诉](#recovery-exit-appeal) | 项目 owner、申诉 authority、资源处置 owner | 不自动退款、改投、延长期限或复活取消项 |
| [AC-7](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | 响应未知、跨入口重试与 snapshot/journal 恢复保持原项目身份、已生效结果和未决义务 | [事务恢复](#recovery-transaction) | Runtime 持久化、API/Viewer/Agent、QA | 本地 fixture 不代签全部真实入口和部署 |

## 3. 当前状态、目标状态与差距

| 对象/能力 | 当前状态（基线） | 目标状态 | 差距/证据 |
| --- | --- | --- | --- |
| 普通危机 | `CrisisStatus` 与 `CrisisState` 记录 active/resolved/timedout、resolver、success/impact | 保留原能力；新增独立的 containment/project/contribution authority | [当前状态](../../../crates/oasis7_runtime/src/gameplay_state.rs)并无贡献型项目 |
| 危机写点 | `ResolveCrisis` 校验普通 crisis 后形成 impact；prepared reducer 原子发布局部结果 | 项目效果必须匹配资源事实、资格和里程碑 receipt | [当前 command](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_gameplay_meta.rs)、[当前 reducer](../../../crates/oasis7_runtime/src/world/governance_meta_publication.rs)不提供项目效果证明 |
| 模块/恢复 | 已有 gameplay module、snapshot/journal 与一般恢复 | 版本化项目/贡献/消费索引持久化，restore 后恢复相同义务 | [M5 crisis](../../../crates/oasis7_builtin_wasm_modules/m5_gameplay_crisis_cycle/src/lib.rs)、[持久化](../../../crates/oasis7_runtime/src/world/persistence.rs)是已有基础，不代签新合同 |
| 验证 | 当前专题未执行业务验证 | AC-6/7 full，同候选跨项目/范围变化/恢复/读面 | 本文只给实施与验证合同 |

## 4. 边界与结构

<a id="recovery-authority"></a>
### 4.1 相互独立的 authority

| 事实 | 唯一 producer / writer | 可以证明什么 | 不能替代什么 |
| --- | --- | --- | --- |
| 系统性危机与 containment | 具当前范围和期限的治理/宪制授权，经 Runtime 提交 | 危机触发事实、允许限制的对象/行为、有效边界、审计/申诉路径 | 不是项目贡献、奖励或目标修复 |
| 恢复项目政策与开放 | 受权项目 owner，消费 gameplay policy 和 containment ref，由 Runtime 准入 | 项目目标、scope、参与/退出规则、资格、资源和效果上限 | 不是对任意资产的写权，也不授予范围外主体权利 |
| 贡献资源/工作事实 | 资源、工业、服务等原领域的权威 writer | 真实拥有、可用量、已到达/已完成的特定工作及 receipt | 自报 success、quote、accepted intent 或未到达运输不成立 |
| 贡献资格与里程碑判定 | 项目规则 evaluator，经 Runtime 记录 immutable decision | 指定主体、项目、贡献和证据是否满足当时里程碑 | 判定通过本身不写资源或受损对象 |
| 目标恢复效果 | 拥有目标状态的 domain writer，经同一 ExecutionTransaction 提交 | 匹配里程碑的实际有限 delta 与最终 receipt | 不得直接覆盖全部世界、撤回历史损失或重复发个人权益 |
| 授权读面 | Runtime committed projection → Viewer/API/Agent | 项目状态、本人义务、有效可选动作、真实下一步 | 读失败/缓存/模型解释不推进状态 |

项目状态可承载在版本化 gameplay module instance state；领域 owner 提交 typed effect proposal，Runtime 校验 capability、预算、scope 和事务。模块不能直接改 Kernel 状态或 journal，节点/Viewer 不能提供第二套修复入口。containment 的独立专业授权来源继续由治理/宪制 authority 拥有；[治理分类和授权来源](governance-classification-and-constitutional-execution.design.md#governance-classification)负责拒绝普通票局结果、技术 brake、charter 地位或 emergency 标签代签，分类结果自身不签发 containment grant。

`ContainmentRefV1` 必须解析到已提交的 authority decision、触发证据、world/scope、有效世界时界、允许限制/开放恢复的能力、当前 revision 和复核/申诉状态。missing、expired、revoked、scope mismatch 或保护条件未满足均为 blocked。项目不能自签 containment；暂停、到期及解除分别按原授权处理，不因项目 active 自动延长限制。实际恢复按 [里程碑到效果流程](#recovery-milestone)取得目标 writer 的 receipt；审计和申诉继续按 [退出申诉协议](#recovery-exit-appeal)保留。

## 5. 关键运行流程

<a id="recovery-open"></a>
### 5.1 containment 到项目开放

1. 受权主体提交 `ProposeRecoveryProject`，固定世界、containment、目标对象、policy revision、effect slots、资源来源/义务、资格及参与方式、里程碑与期限、退出/申诉和失败后的常态动作。提议/预览无 target effect，也不自动锁资源。
2. Runtime 从同一快照核 containment、项目 owner 权限、目标 owner/区域 scope、非参与者保护、预算/资源 authority 和 policy 完整性。候选只有在完整记录 `RecoveryProjectV1` 后才可通过 `OpenRecoveryProject` 进入 recovery_open；unknown 不得当作零成本或默认允许。
3. 项目范围、允许效果和参与规则固定为可读 revision。scope 扩大、资格改变或新增资源义务须新建 linked proposal/revision 并重新授权；受影响贡献不能自动迁入新政策。scope 收缩/撤权与待决贡献在一个原子边界记录处置，不能先改 scope 后让旧操作按缓存继续。
4. 期限使用已提交 world-time 边界。运行时每次推进都重验 containment/项目写权和自身范围；containment 结束后能否继续，必须有明确仍有效的独立恢复授权，不能从旧 containment receipt 推导无限恢复权。

<a id="recovery-contribution"></a>
### 5.2 贡献申请、资格与资源义务

`OfferRecoveryContribution` 由 contributor 本人或当前委托的 Agent 提交。输入绑定 exact project/revision、milestone、贡献 kind/quantity、source receipt/lot/ledger、授权、幂等键和最大允许资源义务。Runtime 校验实际参与记录、当下资格、source ownership/custody、scope、未认领数量和截止边界，再给出 accepted、rejected 或 pending_no_effect。

贡献加入与个人受益资格分别记录，不从提交、组织名称、邻近区域或历史声望推断。`EligibilityDecisionV1` 固定 subject、participation ref、project/policy、规则证据、scope、有效期及 eligible/ineligible/unknown；接受和执行前均核当前有效性。未参与或不合格主体不得获得个人贡献效果/奖励。合法公共设施恢复可能改变共同条件，但这不发给未参与者个人 credit、资格或治理权；不能把公共效果误解释为个人受益资格。

Offer/pending 不产生 hold 或 sink。accepted commitment 只有在明确资源授权、准确 payer/ledger、数量和释放规则已经提交后，才能产生 `ResourceObligationV1` 的有限 hold/custody；接受不表示目标已修复。真实工作可由已有领域协议执行，投入消耗归其原 receipt；对已消耗资源不能再当作余额扣一次。运输中、未结算或证据未知时保留 waiting_evidence/blocked 与下一步，不计为已完成贡献。

跨入口幂等键为 `(world, authenticated_contributor, contribution_intent_key)`，绑定原 project/milestone/payload digest；同键重读，同键不同目标拒绝。另有全域 `ContributionClaimIndexV1` 对真实 source effect/quantity slice 记录 claimed/consumed/released，防止更换 contribution id 或项目重复认领同一材料或已完成工作。claim role 必须来自 source authority 已发布的受限枚举，并绑定 source receipt 的固定 claim budget，不由 caller、项目或 policy revision 选择/新增。不可分 source effect 在所有互斥用途角色上共享一个唯一 claim budget；不可用更换 role 再认领一次。可分量只有 source authority/profile 显式允许时才可按稳定 lot slice 认领，各 project/role 共用该 source 的 checked 总量和重叠检查；独立角色只有在原权威明确证明对应不同、不重叠的 source effect 时才可使用不同 budget，不能按项目各复制一份额度。资源在其它领域的消费也须在同一资源 authority 下反映，项目索引不能凭空锁定已不属于自己的材料。

<a id="recovery-milestone"></a>
### 5.3 里程碑判定到实际效果

1. `EvaluateRecoveryMilestone` 读取项目、已确认 participation/eligibility、准确贡献与 source receipt 集、resource obligations、目标当前状态及 evidence predicate。输出 immutable `MilestoneDecisionV1`，固定 ordered contribution set、predicate/policy version、evidence anchors、预期 target revision 和 bounded effect descriptor。preview、客户端 success、达到申请人数或普通 crisis resolved 都不是合格证据。
2. decision 可为 qualified、not_qualified 或 unknown；只有 qualified 且其全部 required evidence 可重验时才可被消费。decision 本身无 target/credit/reward effect，不能先改项目 completed 再等待执行。证据变化产生 linked 新 decision，旧记录不改写，不能靠新 decision 重置已消费标记。
3. `ApplyRecoveryMilestone` 从 fresh canonical state 重验项目未终止、授权/资格/scope、期限、source claim、资源义务、目标写权/版本及 effect budget。该操作用 `(project_id, milestone_id, effect_slot_id)` 作稳定效果键；policy/project revision 不在唯一键中，不能换 revision 再发同一效果。一个 milestone 可包含预先定义且数量有限的 effect slots，默认整体原子，不能将失败子项静默丢弃。slot 的目标和效果绑定在项目开放时固定，caller 不能自行命名；后继 revision 不得通过删除/更名旧 slot 重置消费。真正新增效果须重新授权新 slot，仍计入原项目累计效果/资源预算，旧 slot 与实际消耗保留。
4. 同一事务原子提交 source claim 消费、必要资源 sink 或已有 sink 引用、目标有限 delta、个人权益（仅当 policy 有明确资格且有资金来源）、milestone/贡献状态、`RecoveryMilestoneReceiptV1`、预算实际消耗与 journal/outbox。所有适用投入和效果守恒；只使用实际被消费的贡献，未用 surplus 保留/释放，不能按请求量计效果。
5. receipt 记录每个 effect 的 before/after、单位/量、目标 authority、参与主体、source/decision refs 和 commit identity。目标 domain writer 必须能验证“确实恢复了哪个状态或能力”；只改变项目进度计数不算修复服务、物流、能力或公共条件。仅所有 required milestones 和未决资源义务都结清后，项目可 recovery_completed。

聚合项目可能先取得多项 contribution evidence 后才产生一个 milestone 效果，每项贡献的 credit allocation 固定在该 receipt；不能让每个贡献者各重放整个公共修复。后续 correction/compensation 必须新建受权记录并链接旧 receipt，明确可修正部分及资金/资源来源；不能原地撤销既有历史或再次消费相同 source。

<a id="recovery-exit-appeal"></a>
### 5.4 退出、到期、阻断与申诉

未生效 contribution 能否撤回、改投由原 policy 冻结。`WithdrawRecoveryContribution` 只对未消费 source claim 和未 sink hold 做一次释放；已发生工作成本、在途或其他未决 custody 必须按原领域处置，不自动退款。改投是显式新 intent，只有旧 claim 释放并重新证明资格/资源后才能认领到另一个项目，保留旧 receipt 和机会成本，不继承旧顺位。

项目取消作为受权 close command，记录 `close_reason=cancelled` 及逐贡献处置；对外映射 recovery_blocked 的终止结果，不能宣称 recovery_completed。到期为 recovery_expired。已经有部分合法效果时保留原 receipt 和实际恢复量；未执行部分不能因已到期继续发生。关闭前必须记录 remaining obligations、具体处置、期限/下一复查点；有在途或待退资源时，业务终止与 settlement pending 分开，不把结束状态当作账务清零。

申诉创建 `RecoveryAppealV1`，固定 applicant、项目/贡献/decision/receipt、理由/证据、复核 authority、时界和 resolution。申请本身不自动暂停、恢复或发放效果；是否暂停由原政策及受权决定明确记录。复核只可维持决定、重新核未决贡献、解除合法限制或授权独立纠正；每次有可审计 receipt。未知/失败必须显示适用的 wait、repair、rebuild、pivot、补证、退出或申诉，不能返回实际不存在的恢复动作。

## 6. 接口与数据合同

<a id="recovery-model"></a>
### 6.1 最小持久模型

所有记录为目标 `v1` 逻辑 schema；ref 绑定世界、domain/instance 与 schema version，政策版本含内容摘要，数量采用各 domain 的非负整数单位和 checked arithmetic。具体编码字节服从现有 ABI/manifest 版本，不改变下列字段语义。

| Record | 必需字段 | 所有者和不变量 |
| --- | --- | --- |
| `RecoveryPolicyV1` | policy id/revision/digest、scope/target kinds、参与/贡献/受益资格规则、资源/效果上限及单位、milestones/predicates、有效期、退出/取消/到期/申诉、剩余资源与失败路径 | gameplay/治理发布；缺任何被采用能力的政策项即拒绝该模式，不补默认数值 |
| `ContainmentRefV1` | authority decision/receipt、trigger facts、world/scope、allowed capabilities、effective_from/until、revision、review/appeal state | 治理权威来源；不能由项目 owner 自报 |
| `RecoveryProjectV1` | project id、proposal intent/digest、parent revision、owner/authority、containment ref、scope/target refs、policy、预算与资源义务、milestone ids、state/previous state、open/deadline/recheck、close reason、remaining obligations、revision | Runtime 唯一写入；scope 和目标效果不因参与而扩大 |
| `ParticipationV1` / `EligibilityDecisionV1` | subject/代表授权、project、加入/退出意图与 receipt；rule/evidence/scope/validity、contributor/beneficiary eligibility、reason | 参与和资格分离；执行重验，旧资格不能永久继承 |
| `RecoveryContributionV1` | contribution id、actor/intent/payload、project/revision/milestone、source receipt/lot/quantity/unit、claim/obligation refs、eligibility、status、decision/settlement/withdrawal refs | 原项目身份不自动改写；每份真实贡献仅在允许数量内消费 |
| `ResourceObligationV1` / `ContributionClaimIndexV1` | payer/owner/ledger、resource/unit、authorized/held/sunk/returned/lost/remaining、source authority/claim budget、固定 role/互斥角色集、source claim slices/累计量、current custody、release/consume receipts | 不能把已有 work sink 再扣一次；认领索引与真实资源权威同步 |
| `MilestoneDecisionV1` | project/milestone/decision id、revision/policy/predicate、ordered contributions/evidence、eligible subjects、target revisions、effect descriptors/budget、qualified/not_qualified/unknown、supersedes | immutable evidence-only；decision 不产生效果 |
| `RecoveryMilestoneReceiptV1` | stable settlement/effect keys、decision/source/eligibility/authority refs、actual resources/claims、target before/after/delta、beneficiaries及分配、remaining obligations、journal/commit ref | 与目标变更同事务生成；revision/重放不重发同 slot |
| `RecoveryAppealV1` / `RecoveryDispositionV1` | appeal/close/withdraw intent、目标 project/contribution/receipt、reason/evidence、review authority、expiry、resolution；逐资源动作/执行状态/结果 receipt | 申请与执行分开；记录受权纠正及未决义务，不改历史 |

这些持久模型经 [项目开放与准入](#recovery-open)、[生命周期](#recovery-lifecycle)和 [事务恢复](#recovery-transaction)执行。项目完成只看已确认效果和 obligation closure，不能用 `contribution_count`、百分比、系统消息或普通 crisis impact 代替。项目与贡献的字段有独立身份；同一 contributor 对不同项目可以有独立合法贡献，但不能重复使用同一 source effect。

### 6.2 跨组件接口

| 接口 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| `ProposeRecoveryProject` / `OpenRecoveryProject` | 有权主体 → gameplay authority/Runtime | project intent、exact containment/policy/target revision | preview 无效果；开放 CAS 校验全部前置 | opened / invalid_scope / stale_authority / missing_policy | 不把普通 Crisis 自动迁成项目 |
| `JoinRecoveryProject` / `OfferRecoveryContribution` | 主体/有权 Agent → Runtime | subject、project/revision、intent key、source claims | 参与与资格分开；相同提交重读 | eligible accepted / ineligible / unknown / claim_conflict | 原有通用 action 不能绕过项目接口 |
| `EvaluateRecoveryMilestone` | 受权 evaluator → Runtime | project/milestone + evidence/policy/target revision | 单次 immutable evidence decision；linked revalidation | qualified / not_qualified / unknown | 不调用任意外部 success 接口形成权威 |
| `ApplyRecoveryMilestone` | 项目 authority → target/resource writers，经 Runtime | stable effect keys、decision、expected revisions | source claims +资源+target+receipt 原子提交 | applied / stale / expired / denied / commit_unknown | 新记录必须有全部消费者与 replay 能力 |
| `ReviseOrCloseRecoveryProject` / `WithdrawRecoveryContribution` | 有权 owner/contributor → Runtime | 原 id/revision、处置计划 digest、intent key | 与完成、撤权和到期竞争同一版本检查 | committed disposition / stale / already_settled | 已提交效果不从返还路径再取得资源 |
| `AppealRecoveryDecision` / `ResolveRecoveryAppeal` | 有资格申诉者 / 指定复核 authority → Runtime | 原项目/贡献/receipt、appeal intent/revision | 新证据独立链接，执行纠正独立去重 | recorded / maintained / revalidated / corrected / denied | 不复活已失效授权 |
| `ReadRecoveryProject` / `ReadRecoveryContribution` | Runtime committed projection → API/Viewer/Agent | project/contribution、audience、read anchor | 只读分页和原幂等键查询 | authorized facts / pending / unknown / not_authorized | 不从本地计数或其他项目补齐结果 |

错误返回原 identity、已知 submitted/accepted/evidence/committed 阶段、真实资源义务、拒绝原因与 next action/recheck。网络 timeout 只能是响应未知；调用方先 query 原 intent/receipt，不换项目、重签授权或另发贡献。读面公开项目目的、合法范围、参与要求和结果，个人 source、预算和申诉证据按当前 audience 限制。

## 7. 状态、事务与持久化

<a id="recovery-lifecycle"></a>
### 7.1 项目和贡献分别转移

| 对象/转换 | 前置与触发 | 结果与拒绝边界 |
| --- | --- | --- |
| containment → recovery_open | 有效 containment 已限制扩散，项目完整 policy、scope/target/资源授权通过 | 开放不产生目标修复；无 containment 或保护条件拒绝 |
| recovery_open → recovery_in_progress | 至少一份合法 accepted 贡献及明确资源/执行义务 | 进度不等于 milestone applied；未知资格保持待决 |
| recovery_open / recovery_in_progress → recovery_blocked | authority、scope、期限前的前置或证据失效，或受权取消 | 保留 previous_state、reason、remaining 和 disposition；不清空资源或抹去部分效果 |
| recovery_blocked → recovery_open / recovery_in_progress | 非终止 blocked，原项目重新具备当前授权/资格/资源；逐项重验未决贡献 | 新 continuation revision/receipt，保留已结算效果；不自动迁移 scope、延长期限或复活取消项 |
| recovery_open / recovery_in_progress / recovery_blocked → recovery_expired | canonical deadline 到达且未完成；自动执行原到期处置 | 停止新贡献与未发生效果；保留/结算旧义务，不自动退款 |
| recovery_in_progress → recovery_completed | 所有 required milestone slots 已 applied，实际 target 结果及资源义务均闭合 | 无 unresolved/unknown 才完成；证据计数不能代签 |
| completed / expired / cancelled → 重新开放 | 不支持隐式复活 | 新项目须独立授权并链接原项目；历史 source 消费与效果键仍有效 |
| contribution offered → accepted / rejected / pending_no_effect | 参与/资格、source claim、作用域、授权及 deadline 检查 | offered/pending/rejected 无隐含资源义务；accepted commitment 明确记录有限 hold |
| accepted → waiting_evidence / blocked / ready_for_settlement | 实际 source 工作/到达与资格核验 | ready 仍不是目标效果，证据-only 不发奖励 |
| ready_for_settlement → settled | matching milestone 同事务实际生效 | 绑定 receipt 和效果分配；不允许另一项目再认领 |
| 未 settled → withdrawn / rejected / expired | 合法退出/关闭处置且未决资源状态已明确 | 只释放未消费部分，已 sink 成本保留；待退资源作为 remaining obligation |

`recovery_blocked` 同时保存 `resumable` 与 close reason：取消项 `resumable=false`，暂时阻塞 `resumable=true`；这是本逻辑 schema 的技术区分，不新增产品生命周期。过期/取消可以仍有财务或 custody 收尾，`settlement_pending` 是义务状态，不能让已终止项目继续生成恢复效果。

<a id="recovery-transaction"></a>
### 7.2 事务与并发

每次 mutation 在 [ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0)不可变快照上准备，提交前检查项目 revision、containment/恢复授权 revision、eligibility、source claim/资源余额、目标对象 revision 与世界时界。项目/贡献状态、资源账、claim 索引、目标 delta、milestone receipt、allocator、journal 与 outbox 同一提交；任一 failure 丢弃整批，不出现“贡献已扣，目标没修”或“目标已修，receipt 未记”的可见半状态。

Apply 与 revoke/scope shrink/withdraw/cancel/expire 的并发按 canonical 输入顺序和同一 CAS 决定一个胜者：效果先提交，后者必须承认已发生 receipt 并只处置 remaining；关闭先提交，效果必须 fresh reject。不能事后按另一顺序解释历史，也不能因 Agent 重试绕过失败。项目 revision 变化只触发真实未决工作的重新判定，不改稳定 effect key 与 source claim ownership。

跨多个目标/账本的 milestone 缺省是完整原子，全部 touched objects 必须纳入同一 authority snapshot 和事务范围。不能在客户端分次写目标状态再“补收据”。无法提供原子事务或超出一次操作预算时，拒绝该项目/milestone；可以由 policy 在项目开放前定义独立、有界 milestones，但各自的可见效果、失败责任和完成条件必须明确，不能执行中拆分隐瞒部分失败。

### 7.3 持久化、恢复与重放

Snapshot 保存完整 project/policy/participation/eligibility、contribution、source claim、resource obligation、milestone decision/receipt、appeal/disposition、allocator 与效果消费索引；journal 保存可重建这些记录和 target deltas 的版本化事件，并承诺同一 generation。未决记录和幂等 tombstone 不能随 UI 历史分页裁剪；retention 保留关闭项目所需完整审计/申诉与防重复材料。

恢复验证完整 world/manifest/历史 policy/schema、checkpoint/journal/state-root 及所有 parent/source 引用，重建实际 target/资源/claim/receipt 一致性。缺贡献 receipt、半个 milestone、重复 claim、混合 generation 或 unknown required schema 都不得安装为可写世界。历史 replay 只应用已确认 delta 和决定，不再执行工作、不重新验证外部证据、不重计贡献或重发奖励。

恢复后先区分：已提交效果返回原 receipt；已知无效果的 pending 在当前 scope/授权/资格下逐项重验；提交未知先对账原 intent 与 durable generation，保持 unknown 且禁止第二次效果。恢复不是新项目创建、旧授权续期或 contribution migration。外部通知只走 [durable outbox](../design.md#61-institution-migration-test)；replay 不执行外部业务效果，发布失败不能回滚已确认世界结果。

## 8. 部署、安全与运行约束

本合同使用既有正式世界写入/恢复边界，与单节点或后续多节点 authority profile 无关；缺正式 authority 的 preview 环境不得发布真实项目效果。Gameplay module 只能在声明的 instance/capability 内给出 proposal，目标 writer 仍检查 current authority 与资源预算。不得使用管理员万能入口、普通 crisis command 或客户端事件补发资源。

每个 profile 明确项目最大目标数、参与数、活跃贡献数、source refs、单 milestone effects/bytes、未决义务与读分页；缺界限不得开放。达到上限拒绝新的 offer 或项目修改，保留已有义务和关闭路径。所有 deadline/recheck 使用 canonical world time，网络或 wall-clock 等待不能续期。用户和 Agent 只可代表自己或当前授权主体行动，项目 owner 不能没收非参与者资产或扩大有权 target set。

## 9. 质量与容量

| 场景 | 环境、输入与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| 单次效果与跨项目隔离 | 同候选至少两个不同 scope 项目、相同 contributor/source、并发 API/Agent 请求 | 合法不同来源可各生效；同 source/slot 不重复；不跨 scope | AC-7、source/target profile 的单位与界限 | §11 `RP-ISOLATION` / `RP-SETTLE` | 目标，未运行 |
| 真实恢复与守恒 | 非零投入，受损服务/物流/能力/公共条件的目标 writer | receipt 对应实际 target delta，投入/剩余/损耗守恒，无免费补偿 | gameplay/M4/目标域 policy，不新增数值 | §11 `RP-EFFECT` | 不以普通 impact 数值代签 |
| 故障与恢复 | commit 前后、receipt 发布前、snapshot+非空 journal 后缀 | 已确认不丢失，未决保留，未知不重试生效 | AC-6/7、既有持久恢复合同 | §11 `RP-RECOVERY` | 不证明跨故障域部署 |
| 大小界限 | 达到及超过声明项目/贡献/effect/byte 上限 | 越界无新义务，已有项目仍可读取和收尾 | 版本化 profile 与 Runtime 执行预算 | `RP-BOUNDS` 属性/边界测试 | 未承诺未测量 P95 或吞吐 |

## 10. 兼容、迁移与回滚

旧 `CrisisState`、普通 ResolveCrisis 与其历史 impact 保持原 schema/语义；缺新项目记录意味着没有本合同项目，不补造 containment、eligibility、milestone 或 contribution receipt。新能力由显式 module/schema/profile 版本启用；producer、所有 target writer、reader 和 replay reducer 缺一则拒绝新项目，而非降级调用普通 crisis。

部署顺序为新记录 reader/replay 与隔离兼容验证、目标/资源 writer 原子接口、项目 mutation、授权投影，最后开放具体 policy。此处只是依赖顺序，不创建新的发布授权。禁用新 admission 后仍须支持已接受贡献的读/处置/恢复；旧 binary 不理解新记录时不得直接回退写入。已生效修复、投入与个人权益无法由代码回滚取消；合法修正使用受权 linked compensation 并保留原结果，不能删除 receipt/source claim 后再执行。

## 11. 验证设计与可追溯性

本节给出的 scenario ID 是可实施测试定位，实际候选、命令、输入/输出与未执行项放在实施 PR。按 [testing-manual](../../../testing-manual.md)执行单元/合同/故障层，然后依 AC-6/7 的 `test_tier_full` 取得正式生产入口、授权 API/Viewer/Agent 读取和完整恢复证据。普通 Crisis、工业维护和存储恢复只能证明原范围。

### 11.1 验证映射表

| 上游 requirement / product AC（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-CC-003](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | [持久模型](#recovery-model) | 项目 scope/资格/义务/里程碑/期限/退出申诉可读 | `RP-OPEN`：有效/缺失/失效 containment，完整/缺政策，越界 target；open 无 target effect；新增独立项目生产路径并验证 native/module 同义；按 [testing-manual](../../../testing-manual.md)组织同候选 full 分层验证 | 项目/授权/策略 receipt、read projection、拒绝无效果 | 不证明普通 Crisis 已支持项目 |
| [AC-6](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-6)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | [authority](#recovery-authority) | 多区域/公共条件 containment 与恢复分离，真实效果且无 bailout | `RP-EFFECT`：系统性危机 fixture，选至少一类受损 target writer 的非零贡献恢复；对照 containment-only、self-reported success、普通 resolved；验证原身份/历史损失不变和实际目标变化；按 [testing-manual](../../../testing-manual.md)组织同候选 full 分层验证 | 触发事实、两类 receipt、source/target before/after、保护范围 | 不以进度计数、截图或文档存在代签 |
| [AC-7](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | [贡献](#recovery-contribution) | 双项目 source claim、参与/资格和 scope 隔离 | `RP-ISOLATION`：两个不同 scope 项目、同主体/同 source 重复及可分量边界；更换 project/contribution id/claim role 重复认领、未经 source authority 声明的角色与跨角色 slice 超额均拒绝；跨入口同/不同 intent key；未参与/不合格/失效 delegation；合法不同 source 正例；按 [testing-manual](../../../testing-manual.md)组织同候选 full 分层验证 | claim ownership、source balance、eligibility/拒绝、效果唯一键 | 普通 job 去重不代签贡献认领 |
| [AC-7](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | [里程碑](#recovery-milestone) | decision 与实际效果独立，聚合贡献和资源只结算一次 | `RP-SETTLE`：未完成/未知/错误 predicate/target revision；qualified 后 apply；policy revision 不重发 slot；多贡献聚合、surplus、已有 sink、原子失败；逐个 checkpoint 注入；按 [testing-manual](../../../testing-manual.md)组织同候选 full 分层验证 | decision/effect receipt、actual delta、一次资源结算与 remaining | 不声明奖励数值或域外效果支持 |
| [AC-7](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | [退出申诉](#recovery-exit-appeal) | scope/revoke/cancel/expire 与完成竞态不越界，失败有下一步 | `RP-DISPOSITION`：所有竞争顺序、已部分 settled 项目、剩余在途/hold、合法 withdraw/改投、禁止取消项复活；申诉本身无效果及独立 correction；按 [testing-manual](../../../testing-manual.md)组织同候选 full 分层验证 | 每个原贡献/资源处置、时间/授权、next action/recheck | 不把 expired 当全部资源已归还 |
| [AC-7](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#ac-7)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | [事务恢复](#recovery-transaction) | 响应丢失、重试、snapshot/journal、授权读面保持原身份与单次结果 | `RP-RECOVERY`：commit 前/后/回复前 crash，未知提交对账、乱序重复事件、非空 journal restore 与不中断对照；[现有 persistence recovery 入口](../../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs)新增独立项目 fixture；full 核 API/Viewer/Agent 对状态与下一步一致 | 固定候选/环境、完整 graph/source/receipt/root、授权消费者产物 | 本地 fixture 不证明全部真实入口或部署；未运行不能标通过 |

## 12. 决策、长期风险与未决问题

选择独立项目、贡献、里程碑判定和实际效果记录，避免把一个 crisis status 或 success 布尔值承载资格与结算。选择 source claim 索引和稳定 effect slot 双重去重，解决“换贡献 ID/换项目重复认领”和“换政策 revision 重发效果”两个不同问题。选择目标 writer 与资源/receipt 同事务，使项目完成对应实际能力恢复；代价是每类可恢复对象都需提供明确的有界 delta 和事务接口，缺接口的类型保持不支持。

具体 policy 的贡献种类、资格参数、预算、奖励、效果上限、期限和撤回/申诉规则由 gameplay/治理 owner 在开放前发布并绑定版本；本文不自行选择。资源与 target owner 必须为其支持类型提供可验证 receipt、单位/守恒、权限和恢复 reducer；missing/unknown 一律禁止相关 effect。Runtime/WASM owner 负责编码和 schema 兼容；QA 在同候选 AC-6/7 full 证据后才能支持实现完成结论。新政策或新 target kind 触发相应资格、scope、资源和恢复合同复核，不能借扩展记录绕过同一世界历史与非参与者保护。
