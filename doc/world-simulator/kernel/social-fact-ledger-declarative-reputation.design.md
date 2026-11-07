# 社会事实账本与声明式关系层设计

- 对应需求：[社会事实账本与声明式关系层 PRD](social-fact-ledger-declarative-reputation.prd.md)。
- 专业主责：Simulator social 维护者；协作：runtime_engineer、agent_engineer、viewer_engineer、qa_engineer。
- 生命周期：`active`；静态审读基线：`8035c556ade7eb989491d37e74f890701509bd40`，2026-11-04。
- 当前任务、实现候选和执行证据由 GitHub Task / PR 与实际验证记录承载。下文新增内容是目标合同，不是运行或发布结论。

## 1. 问题、目标与非目标

事实账本记录谁声明、引用什么证据、当前生命周期及质押；关系边记录声明者、两端主体、方向、权重和 backing。玩家还需要在提交前分清这些确定的账本变化、声明本身的可信程度，以及实际消费者可能采用它们的后果。现有固定 `positive`、schema 名称匹配和通用说明不能提供后一项证据。

本设计保留原社会内核与 [SOCIAL-AC-1–5](social-fact-ledger-declarative-reputation.prd.md#social-lifecycle-contract)，补齐 [五动作后果预览](social-fact-ledger-declarative-reputation.prd.md#social-preview-contract)：一次候选动作在明确来源截面上的只读、有限、可失效预览，以及 Viewer / Agent 消费和验证边界。技术目标是让同一输入得到可对账的条件后果、风险和下一步；发布或裁决成功不自动证明内容真实或合作获益。

不增加世界 Action、原动作 ABI、费用、质押规则、仲裁主体、全局声誉分、经济定价或准入政策；不把通用社会声明接入个人合同、R&D 或组织托管履历评分，也不据此开放尚无真实入口的正式 Runtime 能力。预览不建立持久任务、第二账本或争议程序。

## 2. 上游约束与相关角色

唯一产品要求仍是 [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)，Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。专业 PRD 拥有玩法含义和验收；Simulator 拥有原动作、事件与维护顺序；各真实读服务拥有来源、授权和有界读取；消费者 owner 拥有实际采用规则，不能由预览替他们宣布采用。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [SOCIAL-AC-1](social-fact-ledger-declarative-reputation.prd.md#social-ac-1)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 共享资格与只读 quote、执行时重核；限已有 Simulator 社会动作 | [独立义务 1](#social-obligation-1) | Simulator social 维护者 / 玩法规则维护者 / qa_engineer | 仅 REQ-WR-CR-003 的局部支撑；不增加仲裁权限、经济规则或 runtime-live 可达性；非行为已修证明 |
| [SOCIAL-AC-2](social-fact-ledger-declarative-reputation.prd.md#social-ac-2)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 拒绝无资源副作用与一次提交；限已有 Simulator 社会动作 | [独立义务 2](#social-obligation-2) | Simulator social 维护者 / 玩法规则维护者 / qa_engineer | 仅 REQ-WR-CR-003 的局部支撑；不增加仲裁权限、经济规则或 runtime-live 可达性；非行为已修证明 |
| [SOCIAL-AC-3](social-fact-ledger-declarative-reputation.prd.md#social-ac-3)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 终态、backing 与历史 challenge；限已有 Simulator 社会动作 | [独立义务 3](#social-obligation-3) | Simulator social 维护者 / 玩法规则维护者 / qa_engineer | 仅 REQ-WR-CR-003 的局部支撑；不增加仲裁权限、经济规则或 runtime-live 可达性；非行为已修证明 |
| [SOCIAL-AC-4](social-fact-ledger-declarative-reputation.prd.md#social-ac-4)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 合法首次挑战与原结算保持；限已有 Simulator 社会动作 | [独立义务 4](#social-obligation-4) | Simulator social 维护者 / 玩法规则维护者 / qa_engineer | 仅 REQ-WR-CR-003 的局部支撑；不增加仲裁权限、经济规则或 runtime-live 可达性；非行为已修证明 |
| [SOCIAL-AC-5](social-fact-ledger-declarative-reputation.prd.md#social-ac-5)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 新动作 replay 与旧 journal 分离；限已有 Simulator 社会动作 | [独立义务 5](#social-obligation-5) | Simulator social 维护者 / 玩法规则维护者 / qa_engineer | 仅 REQ-WR-CR-003 的局部支撑；不增加仲裁权限、经济规则或 runtime-live 可达性；非行为已修证明 |
| [SOCIAL-PREVIEW-AC-1](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-1)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 完整候选、来源截面、维护阶段与读取覆盖绑定 | [来源](#social-preview-source) | Simulator / Runtime 的原生来源与有界索引 | seed clone、重编号和测试注入不证明正式来源 |
| [SOCIAL-PREVIEW-AC-2](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-2)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 声明、引用存在、生命周期与实际内容含义分别表达 | [含义](#social-preview-meaning) | social 规则 owner、具体 schema / consumer owner | 不代判真假，不将负权重变为统一正收益 |
| [SOCIAL-PREVIEW-AC-3](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-3)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 五动作直接后果、逐 owner / kind 质押、原维护顺序 | [效果](#social-preview-effects) | 原状态机、资源容量与维护实现 | 不改结算、不把动作回执代签后续维护 |
| [SOCIAL-PREVIEW-AC-4](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-4)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 当前读权、动作资格与历史声明主体分开 | [访问](#social-preview-access) | 当前授权服务、各入口 audience policy | 存在、登录、观察、当前控制均不自动授予其他权限 |
| [SOCIAL-PREVIEW-AC-5](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-5)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 实际登记 consumer、Viewer / Agent 能力与玩家判别 | [消费者](#social-preview-consumers) | consumer owner、Viewer / Agent API 适配 | 不覆盖未登记消费者、未来采用或第三方意愿 |
| [SOCIAL-PREVIEW-AC-6](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-6)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 预览零写入、漂移失效、取消与原提交未知恢复 | [恢复](#social-preview-recovery) | 原动作提交/回读、Runtime G1 边界 | 不以 quote 重算重做世界动作 |
| [SOCIAL-PREVIEW-AC-7](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-7)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 完整输入有界读取、证明及解码成本、满额和超限结果 | [预算](#social-preview-budget) | 有界读 adapter、宿主配额与 QA | 不将全量 clone、无限索引扫描或分页拼接计作一次读 |

## 3. 当前状态、目标状态与差距

下表只描述上述静态基线；没有运行模型、游戏、浏览器或 Cargo 验收。已有测试代码证明测试意图，不能替代当前候选执行结果。

| 对象 | 当前源码事实 | 目标 / 差距 |
| --- | --- | --- |
| 五动作及 quote | [Simulator Action](../../../crates/oasis7_simulator/src/types.rs) 与 [kernel/social.rs](../../../crates/oasis7_simulator/src/kernel/social.rs) 存在五类动作与只读方法。Publish / Declare 固定 `positive`；`social_affected_surfaces` 按 schema 子串推测表面。 | 保留原动作；预览按实际直接变化和 consumer 采用证据分别生成，不能保留固定获益兜底。 |
| 证据与身份 | `has_journal_event` 以 gapless journal 的 `next_event_id` 判断存在；owner 检查只说明 Agent / Location 存在；publisher 是当前唯一可表示的 adjudicator。 | 引用存在、声明内容、当前远程调用身份、历史作者与独立裁决分别验证/表述；不新设 World 或 panel 仲裁者。 |
| 旧 lifecycle 缺口 | `apply_challenge_social_fact` 在事实/lifecycle 检查前扣 stake；Adjudicate quote / apply 尚缺当前 `Challenged` guard。 | 仍由旧 SOCIAL-AC-1–5 的目标与验证负责；本次文档不修复源码，也不把旧错误当预览规则。 |
| 维护与撤销 | `step` / `step_intents_batch_from_envelopes` 在动作后调用维护；先过期事实，再过期 backing 不活跃的关系边。Revoke quote 同时枚举 active / inactive backing 边。 | 截面固定维护阶段，区分动作直接变化、条件性维护变化和已经失效的历史边；Confirm 不恢复旧 Expired 边。 |
| Viewer 预检 | [social_quote.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/social_quote.rs) 有 Publish、Declare、Adjudicate、Revoke 四 route，先核签名、session、绑定与 Agent 访问；没有 Challenge route。 | 四 route 的存在不是五动作产品闭环；新版消费者须单独接入、协商版本并核来源。 |
| Viewer 来源 | [mapping.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/mapping.rs) 从 `seed_model` clone，更新 Agent 等字段，未由当前 Runtime 重建 social ledger；Publish 还枚举整个 Runtime journal，将事件重编号后做存在校验。 | 不把 seed + 当前余额 + 重编号 journal 拼成 exact cut；完整 source adapter、原事件身份和有界读取是启用前置。 |
| 正式 Agent / World | [Agent API Action](../../../crates/oasis7_agent_api/src/world_action/action.rs)、[WorldRuleCommandV1](../../../crates/oasis7_agent_api/src/world_action/world_rule.rs) 的当前变体不含这五个社会动作；[GameClient 命令](../../../crates/oasis7_game_client/src/commands.rs) 的 quote 入口是 build / schedule 等已声明路径。Simulator LLM parser 可解析社会动作，属于另一执行链。 | 不将通用 JSON 参数、Simulator parser 或 Viewer quote 当正式 Game API 已接入；各模式逐项证明真实 producer / submission / result reader，缺口保持 `unsupported`。 |
| 下游社会采用 | 当前可核的直接 reader 是 [social observation](../../../crates/oasis7_simulator/src/kernel/observation.rs) 的事实/边投影及 backing / maintenance。它们不证明交易、治理、claim 或 blacklist 准入采用。 | 明确登记、启用并绑定版本的 consumer 才能给其领域结果；没有登记覆盖证据时保留 unknown。 |

## 4. 边界与结构

预览执行器位于原读服务内，消费受信任的当前来源和只读业务校验。它只建立一次有界工作集：候选输入 → 来源/权限 → 直接后果 → 已登记消费者求值 → 当前授权重核 → 回复。世界写入继续走原动作队列、规则和事件链；预览执行器不调用 apply、扣款、预留或维护函数。

来源 adapter 提供实际记录、精确截面和查询范围的完整性。可使用同一 native Kernel 的短期只读借用，或已有权威快照/版本读取；不能在预览期间重建全世界、遍历全 journal、访问任意 URL 或执行模型。远程来源的签名/验证等级沿原接口，不把服务断言升级为独立 finality。派生反向索引和 consumer 目录不是第二真值：其覆盖版本不匹配即停用，本次请求不触发无限重建。

调用分两类：未提交候选可重新预览；已提交动作只按原 operation 身份回查。原世界写 fence 和来源恢复规则优先于预览可用性，见 [Runtime 提交与恢复](../../world-runtime/design.md#runtime-authority-commit-recovery-target)及 [G1](../../world-runtime/runtime/runtime-storage-footprint-governance.design.md#storage-generation-g1-design)。

<a id="social-lifecycle-design"></a>
## 5. 当前状态校验与资源提交

承接 [SOCIAL-AC-1–5](social-fact-ledger-declarative-reputation.prd.md#social-lifecycle-contract)。
实现范围是 Simulator 的 social 动作、只读 quote、结算与既有 replay 边界，不扩成全 runtime 事务改造。

<a id="social-obligation-1"></a>
### 5.1 共享资格校验

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；主责：Simulator social 维护者 / qa_engineer；承接 [SOCIAL-AC-1](social-fact-ledger-declarative-reputation.prd.md#social-ac-1) 的局部义务，产品链限于 REQ-WR-CR-003 的只读预览与 lifecycle 支撑，不覆盖整项产品验收。

quote 和 apply 复用只读资格判定；apply 不能信任预览时的状态，而应在当前动作执行边界重新检查。
Challenge 保留 Active/Confirmed 且 challenge 为空的既有资格；Adjudicate 只接受 Challenged 且有 challenge，
再核发布者身份与其他业务条件。Confirmed 的新重复裁决和 Retracted/Revoked/Expired 均拒绝。
上层对同请求返回已完成结果属于幂等查询，不重新运行本动作或新增结算。challenge 历史始终保留作审计。

<a id="social-obligation-2"></a>
### 5.2 先验证，再提交一次

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；主责：Simulator social 维护者 / qa_engineer；承接 [SOCIAL-AC-2](social-fact-ledger-declarative-reputation.prd.md#social-ac-2) 的局部义务，产品链限于 REQ-WR-CR-003 的只读预览与 lifecycle 支撑，不覆盖整项产品验收。

Challenge 的顺序是 owner/reason/stake 格式→事实存在及 lifecycle/challenge→库存和必要写入条件→
准备同额 challenge 记录→在一个不可部分提交的动作边界内扣资源并安装记录→生成成功事件。
扣 stake 之后不能再留下普通业务拒绝分支；必要时使用暂存变更或等价回滚，不引入第二套余额真值。
Adjudicate 在取走 stake、移除事实或修改生命周期之前完成状态和结算容量检查；结算与新状态一起提交。
任何失败不得留下半次库存、池或事实变更；既有合法路径和资源上限检查保持。

<a id="social-obligation-3"></a>
### 5.3 终态与 backing

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；主责：Simulator social 维护者 / qa_engineer；承接 [SOCIAL-AC-3](social-fact-ledger-declarative-reputation.prd.md#social-ac-3) 的局部义务，产品链限于 REQ-WR-CR-003 的只读预览与 lifecycle 支撑，不覆盖整项产品验收。

撤销和过期可以保留 challenge 对象，但后续裁决仍按当前 lifecycle 拒绝。支持 backing 的资格只取
当前 Active/Confirmed 状态；终态被拒裁决后不能通过新边重新取得有效性，也不自动恢复旧失效关系边。
本设计不通过清空历史字段、降低检查或新增“上诉/重开”通道消除反例。

<a id="social-replay-compatibility"></a>
<a id="social-obligation-5"></a>
### 5.4 新动作与历史事件重放分开

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；主责：Simulator social 维护者 / qa_engineer；承接 [SOCIAL-AC-5](social-fact-ledger-declarative-reputation.prd.md#social-ac-5) 的局部义务，产品链限于 REQ-WR-CR-003 的只读预览与 lifecycle 支撑，不覆盖整项产品验收。

新动作资格约束施加在 quote/apply 入口；历史 journal 是已记录事件，不应简单重新送入收紧后的动作检查。
兼容读取沿用已支持记录版本的确定性事件语义，不读取墙钟、随机数或外部系统，不改写历史事件和已确认状态。
若实现确须改变 replay 约束，先明确受影响记录版本、旧记录读取/转换方案与正负 fixture，再启用新约束；
不能把一个新 guard 直接加到所有历史重放并宣称兼容。无法由旧 ActionRejected 还原的历史扣款不凭空修补；
应保留缺证和受影响范围，按相应恢复决策另行处置，本修复只证明新动作的拒绝不变性。

<a id="social-obligation-4"></a>
### 5.5 验证场景

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；主责：Simulator social 维护者 / qa_engineer；承接 [SOCIAL-AC-4](social-fact-ledger-declarative-reputation.prd.md#social-ac-4) 的局部义务，产品链限于 REQ-WR-CR-003 的只读预览与 lifecycle 支撑，不覆盖整项产品验收。

| 验收 | fixture / 执行层 | 必须区分的结果 |
| --- | --- | --- |
| SOCIAL-AC-1 | 同一事实状态矩阵分别调用只读 quote 和公开 apply | 同一当前资格判定；quote 无副作用；quote 后撤销/过期仍由 apply 拒绝 |
| SOCIAL-AC-2 | 正 Data stake，缺失/重复/终态事实；重复提交拒绝请求 | 库存、stake pool、social facts/edges 不变；时间和拒绝事件 ID 可正常推进 |
| SOCIAL-AC-3 | Publish→Challenge→Revoke→Confirm→新建 backing；另测 Retract/Expired | 最后裁决拒绝且终态保持，新边拒绝、旧边不变，挑战历史仍存在 |
| SOCIAL-AC-4 | 合法首次挑战与正常 Confirm/Retract、撤销/过期 | 一次扣款/对应记录/一次原结算；重复业务裁决不再结算，既有上层幂等语义不变 |
| SOCIAL-AC-5 | 动作前 snapshot＋后续 journal 与 live 对照；固定旧版本 journal | 新拒绝序列资源/社会状态一致；历史兼容另验，不用只读 quote 测试代替 apply/replay |

回归复用既有 social/social_persist/social_revoke_quote fixture，并通过真实公开动作建立合法前置；
不通过直接篡改模型、伪造事件或删除断言来制造通过。上述是验证设计，不是已执行的测试或所有 runtime-live 入口证明。

## 6. 接口与数据合同

以下 `SocialConsequencePreviewV2` 是拟实现的版本化只读 envelope，不是新的世界动作或 receipt；字段采用语义名，正式 wire 名称须在原 Viewer / Agent 协商合同中登记。旧 `SocialFactImpactQuote` 的 string / i64 字段保留旧协议含义，新客户端不能补默认值后声称 V2 已验证。

<a id="social-preview-source"></a>
### 6.1 候选输入与来源截面

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接 [SOCIAL-PREVIEW-AC-1](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-1)。

| 合同组 | 必须携带和核验的内容 |
| --- | --- |
| 请求绑定 | `preview_request_id` 仅是读请求关联身份；`draft_revision`、完整 typed action、原 actor、schema、subject/object 或 from/to、claim/reason/notes、decision、confidence、signed weight、证据/backing IDs、ttl、逐 kind stake 均进入 canonical `candidate_digest`。关联键是 `(reader, preview_request_id, draft_revision, candidate_digest)`，不能只按 ID 匹配缓存/响应；服务端重算摘要，不一致拒绝，任何编辑使旧预览失效。它不承诺跨重启记住全部旧读 ID。拒绝重复 JSON key、未知动作和溢出数字，不只绑定 action_kind 或当前选中 Agent。 |
| 原生来源 | `source_profile`、世界/Kernel instance、generation、cut 身份、tick、journal 原始 head / version、`maintenance_phase` 与对应 authority assurance。所有事实、关系、余额、池、规则、consumer 配置和范围覆盖均绑定同一不可变 cut；不得用客户端时间或拼接多个 snapshot 得到。 |
| 当前资格 | 返回当前 reader / actor 授权版本、业务校验范围、已知资格与未能覆盖的 admission 条件。`eligible_at_cut` 只说明所列校验在该 cut 成立；未来 slot、前序动作、其他 runtime guard 与提交成功不由 quote 承诺。缺少必需 guard 输入时不能给完整可提交结论。 |
| 直接状态与覆盖 | 指定事实原件、当前生命周期、剩余 stake、挑战、绝对到期 tick；关联边按 fact ID 的反向索引包含 active 与 inactive，记录覆盖范围/版本。所有候选引用按原 namespace 定位；同一个数字来自不同世界/journal/实体类型不能互换。 |
| consumer 覆盖 | 明确本 source profile 中已声明且实际启用的 consumer 集合、规则版本、适用 schema/主体/上下文与读取截面。只对该范围声称 complete，不推断其他人、外部组织或未来消费者。 |
| 输出边界 | `completeness`、缺口类别、可公开条件、`source_refs` 和已应用的 redaction。只读预览给条件后果，不给 World committed 状态；证据/权限缺失不能折叠成“没有影响”。 |

源截面原则上取 `after_maintenance`：其中事实和边的既有到期/失效已经按原维护顺序落位。来源只能给 action 后、维护前数据时，标出该阶段及尚未确认的维护结果，不能把它当当前完整预览。新候选的维护结果仍是条件推演；实际提交后的事件必须另回读。

本地 adapter 在原 Kernel 读边界下捕获有限原件、索引覆盖与规则版本，然后释放读 pin 再计算；当前授权在回复前重新读取。远程 adapter 使用原读接口的固定版本/快照，逐页返回同 cut；最后比较相关 source / policy / authorization 版本，漂移返回 `stale`，不在一次调用内自动重开来源。没有原生 cut 或索引 coverage 时返回 `source_unavailable`，不会退回 `seed_model`。

反向查询需要能证明“这个 fact 的所有直接 backing 边”，包含已失效边。页可作为本次内部有界读取单元；只有完整闭包才完成关键预览。未读页、被过滤行、没有可用索引和隐藏依赖分别保留缺口，不能将空页或 UI 未显示当全量为空。去重仅复用同 namespace、同版本、同 payload 的已验证对象；预览输入仍保留原数组/重复项语义，列表长度在去重前限额，不能靠重复引用绕过预算。

<a id="social-preview-meaning"></a>
### 6.2 声明、证据和内容含义

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接 [SOCIAL-PREVIEW-AC-2](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-2)。

输出分别呈现以下层次：原发布者/声明者说了什么；引用的原事件是否存在且可读；当前 lifecycle 和 backing 是否有效；实际 consumer 是否在该情境采用、产生什么条件结果。已存在事件不证明任意 claim 与其一致，schema 字符串、历史名称和当前 Agent 控制人均不能补足语义。没有适用的内容验证器时，内容真实性保持未评估；有验证器时也只写其版本、可验证命题与结果，不扩大为人格或全部履约证明。

`confidence_ppm` 是声明携带的值，不是系统测得真实性概率。`Confirmed` 在当前 v1 是原 publisher 的裁决结果，不写成独立 panel、多方背书或客观事实认证。不同制度对同一主体的声明保留各自来源，不合成全局声誉分。

`ResourceOwner::Agent` 与 `ResourceOwner::Location` 使用带 namespace 的 typed 身份，即使 ID 字符串相同也不是同一主体；World owner 不在现有 ABI 中。Declare 的 `declarer` 与 `from/to` 分开；允许发布关系声明不代表任一对端同意，正/零/负 `weight_bps` 原值和方向保留，未获 consumer 解释时不得给 `positive`、净合作增益或 blacklist 准入结果。

`recommended_social_action` 与 `why_this_action_matters` 由已核事实、所选候选和缺口生成受限说明：能判断只会新增账本声明时就说明这一点；缺语义或采用证据时建议暂缓或检查来源；生命周期/权限不符时指明不能继续及合法下一步。claim、reason、notes、schema / relation 文本和 consumer 描述均是不受信任的内容，只按原文安全呈现；其中的代码、HTML、提示词和动作要求不得成为执行指令、授权依据或 World 写工具参数。Agent 只能消费校验后的结构字段和受限说明，自动建议仅来自已核字段，不借 LLM 补写未知收益或执行夹带动作。

<a id="social-preview-effects"></a>
### 6.3 五动作后果与资源明细

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接 [SOCIAL-PREVIEW-AC-3](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-3)。

在原资格及资源容量检查全部成立时，按以下条件直接后果生成预览。动作在执行时重核；共享 validator 的实现仍须满足 §5 旧合同。推演的是该候选被原 authority 接受后产生的变化，不给真实 fact/edge 新 ID 预分配或替原权威预留位置。覆盖范围是该候选的事实、质押及直接关联边与它们的 backing 闭包；其他无关世界对象同 tick 的自然变化不归因于候选，也不宣称已覆盖全世界维护。

| 候选 | 条件直接变化 | 资源 / backing / 维护解释 |
| --- | --- | --- |
| Publish | 新增原 actor 的 Active 声明和原事件引用。 | 可选 stake 从 actor 对应 kind 可用库存锁入该 fact；没有 stake 就无本动作锁款。公开声明和引用存在不增加合作资格；`ttl_ticks` 是从实际创建 tick 计算的原长度。 |
| Challenge | Active/Confirmed 且无已有 challenge 时新增挑战，fact 变 Challenged，历史原声明仍保留。 | challenger 可选 stake 锁入 challenge；publisher 已锁 stake 保持。原维护时，直接 backing 该 fact 的 active 边会因不再支持 backing 变 Expired；已 inactive 边保持。不能把“提出质疑”描述为已证明造假。 |
| Adjudicate Confirm | 当前 Challenged 且有 challenge，原 publisher 裁决为 Confirmed，保留挑战审计。 | 剩余 challenger stake 按各 kind 进原池，剩余 publisher stake 返回原 owner。fact 可再次用于合法新 backing；此前已经 Expired 的边不复活。 |
| Adjudicate Retract | 同一合法裁决前置下 fact 转 Retracted，保留历史。 | 剩余 publisher stake 入池，剩余 challenger stake 返回；关联 active 边按原维护失效，inactive 边不重复失效。 |
| Revoke | 当前允许撤销的 fact 由原 publisher 变 Revoked；不会删除历史内容/挑战。 | 双方尚存 stake 分别退回。active backing 边在维护阶段失效，inactive 边仅作为既有历史列示；不写作新损失或本次再次撤销。 |
| Declare | 新增 declarer 的 directed edge，保留 schema、relation_kind、原 from/to、signed weight 和非空有效 backing。 | 无新增 stake 或费用。所有 backing 在实际执行时须支持 backing；随后任一 backing 失效或本 edge 到期，由原维护使 edge 过期。声明不是对端同意或已获得协作入口。 |

`resource_effects[]` 每项为 `{owner_type, owner_id, resource_kind, amount, movement, condition, source_ref}`；movement 区分候选新锁定、已有锁定、条件返还、条件入池。输出 publisher、challenger 与实际承担者，不能以旧单个 `stake_at_risk: i64` 代表“我的总风险”，也不把不同 kind 相加。相同 owner + kind 的多笔返还先 checked-add，再与同 cut 库存容量共同校验；池增量也按 kind 校验。沿用 [stake_returns](../../../crates/oasis7_simulator/src/kernel/social/stake_returns.rs) 的原容量规则，溢出拒绝，不用新费用抵消或修改结算。

已有 fact/edge 同时显示原 `ttl_ticks`、`expires_at_tick`、当前 cut tick 和可核的剩余 ticks；剩余值以绝对到期计算并饱和至零，不重新从原长度起算。声明无 TTL 不表示永远有用，backing、权限和 consumer 版本仍可使其失效。新候选的到期时间依赖实际接受 tick，未知时给条件公式，不能用 quote 时间预先盖章。使用原实现的时间运算及维护顺序；若原绝对到期在紧随动作的维护点已到达，须显示可能同一步过期及原 stake 返还条件，不能只报动作瞬间的 Active。

输出 `action_effects` 与 `maintenance_effects` 分列，后者注明“原动作成功并完成相应维护后”。[step 顺序](../../../crates/oasis7_simulator/src/kernel/step.rs) 的返回动作事件不包含全部维护事件；已提交结果须读取原事件链和维护后状态，不能从 ActionReceipt 存在推出所有边已过期。若原执行入口有 batch 前序动作或额外 guard，条件中保留该不确定性，完整来源只证明当前 cut 的反事实，不保证未来受理。

<a id="social-preview-consumers"></a>
### 6.4 采用者与玩家、Agent 消费

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接 [SOCIAL-PREVIEW-AC-5](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-5)。

consumer 由其已有模块/策略 owner 声明只读适配：`consumer_id`、实际启用的规则版本、来源 profile、所用 schema/关系种类、主体和情境、输入选择器、现行读权、结果单位、条件和评估器。预览仅调用已登记的有限确定性评估器；不动态运行任意用户代码、网络查询或模型。目录记录只描述实际部署采用，不新增游戏准入 authority。目录缺失、版本不匹配或评估器缺输入时，不能从文档名称或可编译 fixture 认定已采用。

每个适用表面分别取以下状态，禁止合并成一个好/坏评分：

| 状态 | 证明条件与玩家解释 |
| --- | --- |
| 已采用且有条件变化 | 同 cut 的实际 consumer、原输入及候选后输入均完整，评估器给 before/after、方向/单位和条件；可说明该明确入口的变化，仍待原提交及入口自身准入。 |
| 已采用且验证无变化 | 相同完整条件下评估结果相等；仅在所列 consumer / scope / cut 内说无变化，不等于全世界无影响。 |
| 本范围未采用 | 当前完整启用目录及适用选择器证明本声明在所列 profile / consumer 范围未接入；显示未采用，不能写验证零变化、普遍无风险或默认可合作。 |
| 未知 / 不完整 | 缺目录覆盖、权限、原输入、读证明、评估器或版本；给缺口和恢复路径，不将字段缺失转换成空数组、0 或 positive。 |

核心 ledger / backing 的直接影响可独立完整，而外部合作、交易、治理、claim 或 blacklist 采用仍为 unknown。顶层须保留各部分 completeness；任一所需关键解释缺失，不输出“全部影响已确认”或推荐确认。`affected_social_surfaces` 只列有依据的直接表面和已核采用表面，其他显示未知类别，不用 `schema_id.contains(...)` 决定政策。其他咨询/归因专题中个人合同、R&D、组织/岗位履历的目标读服务拥有各自来源合同，不能把此处普通声明自动映射为其计分来源。

Viewer 与 Agent 读取同一个版本化语义 envelope，身份及披露范围可不同。Viewer 按“谁声明/指向谁 → 已知账本和资源变化 → 采用范围与未知 → 可继续/暂缓原因”呈现；CJK、键盘和文本 fallback 保留同一含义。原文作不受信任文本转义，裁剪长文只影响摘要，不能裁掉条件、撤权或不完整提示；完整原件链接重新鉴权。Agent 获得 typed 状态与原因，不从自然语言的“正向”“已确认”猜控制资格，也不依据 quote 自动提交。

每条 route 独立声明支持的 action_kind、来源 profile、V2、读取与提交能力。只支持预览的入口明确不能提交；没有 route 时显示 `unsupported`，不能报网络重试或借另一种动作代替。现有 Viewer 四 route 需要补其各自完整来源，Challenge 还需接入；正式 Agent API 在原 authority 承接该能力前保持未支持。本设计不新增动作 ABI，也不允许把 Simulator 本地结果包装为正式 World 已接受。

## 7. 状态、事务与持久化

<a id="social-preview-recovery"></a>
### 7.1 只读执行与原请求恢复

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接 [SOCIAL-PREVIEW-AC-6](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-6)。

读请求状态为 `validating → reading → evaluated → returned`，失败终止为 `denied / unsupported / incomplete / stale / cancelled / timeout / busy`。服务器按以下流程执行：

1. 在任何大型解码/来源读取前校验 envelope、字节/列表上限、版本、当前身份和并发槽。普通业务拒绝保留动作原因；未授权错误不泄露对象是否存在。
2. 取一个 exact cut 和原生有限读 pin；按绑定候选获取直接状态、反向索引覆盖、已登记 consumer 输入及证明，预算累计不重置。无副作用校验与条件推演不执行维护、发事件、扣库存或改池。
3. 全部必需输入齐备后生成直接后果与逐 consumer 结果；返回前再核当前 reader/actor 授权、source generation、相关 policy / source 版本。变化即作废此次结果；不能持有旧私有 payload 继续发回“当前”。
4. 计算完成即释放不再需要的来源读 pin 和临时原件；仍持有的 decoder、I/O 与待发送回复继续计入原槽和工作内存，直至响应完成或连接终止且实际清理结束才释放槽。慢客户端也受总截止约束；取消/超时停止新读取及派生工作。底层 I/O 尚未终止或持有内存时，槽继续占用且受总上限约束，不能提前复用制造隐蔽超额并发。

quote 无世界事务提交点、持久 request ledger、资源预留或新的收费。成功只说明该读请求在所列 cut 得到这些条件结果。进程重启可丢弃 quote 和派生索引；索引恢复由原来源按真实 cut 完成后再启用，不能在本次请求中无限补历史。重复 quote、刷新或取消不增加 journal/event ID、库存、池、事实、边或 World mutation。

| 情况 | 唯一允许的恢复与可观察结果 |
| --- | --- |
| 仅预览响应丢失 | 保留原 draft 和候选摘要；可用新读请求重新获取当前预览。重新读取不重放任何世界动作，也不承诺相同 cut 下仍有效。 |
| 来源/权限/输入变化 | 作废旧确认，重取来源或重新编辑后预览；旧异步响应只按其 request / revision 显示为过时，不能覆盖新草稿或新 cut。权限未知先撤去敏感详情，当前可读性恢复后再取。 |
| 超限、超时、取消或 busy | 返回有界结构化原因及可行的缩小范围/稍后重试/恢复源建议；保留草稿，丢弃未完成结果。重试是新的独立读操作，不能接着隐藏累加旧结果凑“完整”。 |
| 原动作已提交，结果未知 | 保留原 `operation_id` / action identity / 参数绑定，按原 authority 查询；在未知解除前不换新键再次发布、挑战、裁决、撤销或声明，也不通过重算 quote 推断未提交。已明确拒绝后，修订候选需重新预览和显式提交。 |
| 世界存储 G1 未决或来源恢复中 | 遵守原 World 读写边界；预览不能解除 fence、接管旧 writer 或用 seed/local clone 建立新的世界。允许读的历史结果保留历史标签，不能作为当前完整预览。 |

原动作仍由其已有提交、事件、持久化和幂等合同保证一次世界效果；Simulator 本地 action ID 与正式 Runtime operation ID 不互换。预览错误不会撤销已经接受的世界行为；隐藏受限 UI 也不删除权威历史。WorldAction 本身没有可用的原结果回读入口时，该组合不能宣称提交/恢复闭环已支持。

预览 freshness 不是新的 World admission gate：A 在 F 为 Active 时预览 Revoke，随后 C 合法 Challenge，旧预览的质押/backing说明已陈旧；若 A 仍有原资格且当前容量足够，原 Revoke 对 Challenged F 仍可合法执行并返还当前双方剩余 stake。客户端需要最新判断时重取预览，执行端继续只按原规则重验，不因 cut 不同单独拒绝、不强制携带 quote token，也不静默修改原 payload。失权、终态或资源不符的拒绝必须来自各自原规则。

## 8. 部署、安全与运行约束

<a id="social-preview-access"></a>
### 8.1 当前受众与历史主体

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接 [SOCIAL-PREVIEW-AC-4](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-4)。

`ensure_owner_exists` 是业务对象检查，不是远程认证。外部服务先核自己的当前 session/reader authority，再分别核行动 actor 的授权、事实/关系/证据/余额/consumer 输入的读取 audience；可观察一个 Agent 不代表能以其名义行动、查看私有 backing 或执行 publisher 裁决。AgentId、LocationId 和账户当前 controller 不混用。历史 publisher/declarer 由原记录确定，只在原证据明确时关联当时账户；当前持有者不能被追认为旧声明人。

可信服务可以在自身合法内部权限内完成必要资格/覆盖核验，再只输出该 reader 可见的最小条件后果。不能因为预览需要原事件而向客户端开放完整 journal 或所有治理/授权正文；原件链接和每次重取都重核权限。隐藏对象的 ID、计数、标题、存在性及缺项位置均遵循原 audience policy；无法安全披露充分影响时统一返回相应范围不可用，不借 `not_found` / hidden count 泄露。

来源原件属于同 cut，响应资格属于当前读边界，两者分别证明。当前授权撤销、主体切换、断连后资格不明或来源 profile 变化，使相关缓存与确认立即失效；旧历史仍存在于原账本，但客户端只有重新证明当前读权才能显示其受限内容。共享缓存必须包含 reader / authority version / source profile / cut / candidate digest；不支持完整隔离时不共享。首版只保留当前页面草稿和预览，不持久缓存私有 source 原件。

原生只读借用不得跨无界网络等待；remote adapter 的每次读都有同一个总截止与取消传播。预览不持有写锁，不等待世界 action 完成，不占用原世界提交恢复所需的持久预留。宿主已有更严格的读配额和优先级继续适用；原 operation 回查不以获取预览为前置。

## 9. 质量与容量

<a id="social-preview-budget"></a>
### 9.1 有界闭包与工程初值

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接 [SOCIAL-PREVIEW-AC-7](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-7)。

下面是单进程、单次候选的首版工程目标，依据是五动作少量直接关联的完整解释；不是实测延迟或世界动作输入上限。由 Simulator / Runtime owner 在实现前按真实输入和宿主配额核验，调整须连同组合可行性与手册边界样例更新。宿主更小上限优先，不能为预览放宽已有授权或 Game API 上限。

| 计量项 | 首版硬边界 | 超限 / 计量规则 |
| --- | --- | --- |
| 候选输入 | 1 个动作；解压后 wire ≤32 KiB；JSON 深度 ≤8；单文本字段 ≤2048 UTF-8 bytes；各 evidence / backing 引用列表 ≤16 项 | 解析前限 bytes，解析中限深度/分配，去重前限列表；身份/摘要/签名也计入 request。超限保持原动作 ABI，不截断后执行。 |
| 来源对象 | 每次请求总解码对象 ≤64；每对象 ≤64 KiB；累计来源 bytes ≤2 MiB | 每个事实、边、事件、库存/池、授权行、规则行、索引页和独立解码/校验的证明节点分别计量；页内另含独立记录/节点仍逐项计数。嵌套列表/祖先也受限，包装为 bundle 不减少实际对象和 bytes。 |
| 有界读 | ≤8 次跨 adapter 读取；每次返回 ≤16 对象、≤256 KiB；所有索引探查/证明节点访问合计 ≤256 | 探查未命中、过滤、重读当前授权、校验与取消前已发出的读取均计费于工作预算；大小元信息不足时有界解码，不能先全量 clone 再计数。 |
| 关联范围 | 直接关联边 ≤32；已声明启用 consumer ≤8；每个对象内引用/证明数组仍受上述对象/节点/bytes 总限 | 事实 backing 闭包、实际 consumer 依赖和完整范围证明均加入同一总限；不能将最大值简单相乘后隐式越额。 |
| 回复 | wire ≤128 KiB；单 effect 说明 ≤2048 bytes；受限错误 ≤4 KiB | 保留身份、条件、完整性与恢复；不能为压缩成功响应静默删除关键项，超额改为有界 incomplete。 |
| 并发与时间 | 每进程最多4个预览；每当前 reader 最多1个；不排后台队列；从接纳起2秒为求值/响应截止 | busy 不产生持久任务；同 reader 换 action/ID 不绕配额。截止时停止新工作并发起取消/关闭响应；不承诺底层I/O已清理完毕，清理期间仍占槽和内存，超时不继续后台补证。 |
| 工作内存 | 每预览计量工作缓冲 ≤8 MiB，包含原始输入/来源、解码分配、索引/证明临时数据和回复 | 最多4×8=32 MiB计量工作缓冲，非整个进程 RSS 承诺。解码器必须在实际分配前预留/检查；不能按压缩后 bytes 估算无界内存。 |

完整关键预览要求全部必要闭包在这些边界内结束。任何部分超限、coverage 缺失或当前授权无法重核，整份“可以完整判断本次动作”的结论为 incomplete；允许保留已验证且仍可披露的局部内容，但不能把局部显示为完整、适用 consumer 全量或准入成功。首版不发可接续的完整预览 cursor；不得多次分页/刷新后拼成一次完整结果。每次新预览可有自己的预算，却是独立 cut / 授权 / 输入判定，不能复用未验证旧片段来规避限额。

### 9.2 正常路径与组合核算

代表性 Revoke 使用可达的 `after_maintenance` 前态：A 发布 F 并质押，F 与另外两份事实共同支持12条合法边；B 随后合法 Challenge 并质押，维护后 F 为 Challenged，12条边均已 Expired，双方剩余 stake 尚在。查询 A 的 Revoke 将说明双方条件返还与历史边保持，不声称再次使旧边失效。另用 Active F 的旧TTL已过期边和后来新Active边组合验证 active/inactive 区别，该组尚无 challenge，不混入双方在押样例。

预算样例采用同进程原生来源：关联两份原事件，consumer 目录由同一 source profile 管理且完整，不声称外部合作采用。来源已有不可变读取期间可用的 native 索引，不在本请求中构建。以下包含实际原件与授权重核，不把一个闭包包装成一项：

| 输入组 | 实际对象数 | 保守 bytes 预算 |
| --- | ---: | ---: |
| 原生 cut 身份、source profile | 2 | 2×8=16 KiB |
| F 与另外两份 backing 事实，含有界 challenge/stake 字段 | 3 | 3×16=48 KiB |
| 12 条已经 Expired 的直接 backing 边 | 12 | 12×8=96 KiB |
| 两份原事件正文 | 2 | 2×16=32 KiB |
| 两 owner 库存与原 stake pool | 3 | 3×4=12 KiB |
| 实际采用目录与适用范围记录 | 2 | 2×8=16 KiB |
| 当前 reader/actor 两份授权，起始与返回各读取一次 | 4 | 4×8=32 KiB |
| 返回时原生 current head：固定 generation / state / consumer / authority revision 字段 | 1 | 1×8=8 KiB |
| 4份固定大小原生索引/范围覆盖记录，内含有界引用、无独立证明节点正文 | 4 | 4×16=64 KiB |
| 合计 | 33 | 324 KiB |

可按六次读取完成：cut/profile+初始授权（4对象/32KiB）→事实+库存/池（6/60）→边（12/96）→事件+两覆盖记录（4/64）→目录+两覆盖记录（4/48）→返回授权与 current head（3/24）。每次都满足16对象/256KiB，合计33对象/324KiB；剩余31对象、1724KiB与两次读取给该样例的额外必需依赖留余量，不保证任何请求能同时达到各项最大值。样例采用索引深度不超过4、实际 native 索引探查不超过160的有界读 fixture；这些探查是对已安装索引的访问/重复访问，不是另解码160个proof节点。4份覆盖记录最多各16项原件引用，正文已逐项计入上表；如来源增加独立证明节点，每节点新增对象、bytes与访问计数，不能藏在4份记录内。33对象与160次探查必须在同一个真实fixture中同时对账，该条件是需要实现记录的输入参数，不以对象数替代探查计数。返回组在同一原生读边界读取两份当前授权和 current head；head 只含固定版本/root字段，当前 consumer 目录由同一 source profile 管理，不包含其无限依赖正文。跨 authority 或需要额外证明的来源必须逐项补入预算；无法取得一致当前版本时不能复用此样例声称通过。

同一 owner 的 Data 两笔返还必须聚合再检容量；Data 与 Electricity 两种各自检查，不求和。输出只列直接变化、尚存风险和真实采用缺口即可满足此正常案例，无需创造交易 consumer。若为新增已采用 consumer 读取规则/原状态，所有实际输入继续计入原预算；无法满足时是 incomplete，不能声称样例本身证明所有消费者可行。

| 验证环境 / 刺激 | 预期响应与指标 | 入口 / 当前范围 |
| --- | --- | --- |
| 同候选 native Kernel fixture；固定上述33对象和原事件链 | 重复读一致、零写入、六次读与324KiB预算可对账；无跨 kind 合并 | [手册07](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-07)；目标，未运行 |
| 各单限及组合 limit / limit+1、深索引、隐藏依赖、巨大单事件 | 读取/分配前停止超额；无伪完整、无静默截断、无后台补证 | 同手册；目标，未运行 |
| 4并发占满、同reader第二请求、取消后底层读仍挂起 | busy/timeout有界；槽和buffer不提前复用，结束后可再读；原world回查不依赖quote | 同手册；目标，未运行 |

2秒是停止求值/响应并发起取消的保护目标，不是底层I/O清理完成或 P95 服务性能承诺。实现验证记录硬件、来源记录规模、索引深度、并发、输入分布、实际读/字节/节点峰值和超时释放耗时；不得仅以33对象 fixture 推断公开网络性能。

## 10. 兼容、迁移与回滚

升级仅作用于只读 envelope 与消费者解释。Viewer 在原能力协商中新增明确 V2 能力，逐 route 声明可用的来源 profile；正式 Agent 入口按原 Game API 协议登记只读能力，不能利用开放 JSON 绕过当前 Action 枚举。发布新字段的服务与实际消费者同时完成严格解码、未知枚举/能力处理和 fixture；原世界 Action / event / snapshot 版本及 SOCIAL-AC-5 的历史 replay 边界保持。

旧 quote 没有来源 cut、当前授权版本、分 kind 质押和 completeness，不能零值补齐为 V2。新客户端收到旧格式只显示能力受限/待升级，不继续按旧 `positive` 宣称合作收益；旧客户端不会收到无法理解的 V2 状态。缺少完整源或有界 adapter 的 profile 不公布此能力，即使旧测试能用 seed 注入产生字符串。

回滚时停用相应 V2 能力、失效当前预览并保留草稿；原动作结果继续通过原身份回查。可回到已知旧只读协议的受限诊断范围，不能恢复错误收益兜底或伪称支持完整预览。无需迁移/补写历史事实与事件，不撤销已有世界结果，不改仲裁权限与结算。

## 11. 验证设计与可追溯性

新增验证使用 [社会动作后果预览手册](../../testing/manual/social-action-consequence-preview-acceptance.manual.md) 的七项方法；先用真实公开 Simulator 动作建前置，检查事件与维护后的状态，再验证有授权、真实来源的外部适配。单元 fixture 的临时 consumer 只能证明求值分支，不证明实际部署采用；注入 seed 或直接改状态不能代签正常业务入口。

### 11.1 验证映射表

| 上游 requirement / professional acceptance | 本设计条款 | 独立 obligation 与适用条件 | 准确验证方法、source / scenario / layer | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [SOCIAL-AC-1](social-fact-ledger-declarative-reputation.prd.md#social-ac-1)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [独立义务 1](#social-obligation-1) | 共享资格与只读 quote、执行时重核 | [现有 fixture](../../../crates/oasis7_simulator/src/tests/social_revoke_quote.rs)；状态矩阵分别调用 quote/apply；旧 quote 后撤销/过期；§5.5 计划由 Simulator required 回归逐项补证 | planned；实施候选、命令、各场景结果与未覆盖范围，由 Simulator social 维护者 / qa_engineer 记录 | 未执行行为测试；现有 fixture 不证明新增矩阵、全部产品验收或 runtime-live 入口 |
| [SOCIAL-AC-2](social-fact-ledger-declarative-reputation.prd.md#social-ac-2)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [独立义务 2](#social-obligation-2) | 拒绝无资源副作用与一次提交 | [现有 fixture](../../../crates/oasis7_simulator/src/tests/social.rs)；正 Data stake 对缺失/重复/终态事实重复提交；库存/pool/facts/edges 不变，时间与拒绝事件另断言；§5.5 计划由 Simulator required 回归逐项补证 | planned；实施候选、命令、各场景结果与未覆盖范围，由 Simulator social 维护者 / qa_engineer 记录 | 未执行行为测试；现有 fixture 不证明新增矩阵、全部产品验收或 runtime-live 入口 |
| [SOCIAL-AC-3](social-fact-ledger-declarative-reputation.prd.md#social-ac-3)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [独立义务 3](#social-obligation-3) | 终态、backing 与历史 challenge | [现有 fixture](../../../crates/oasis7_simulator/src/tests/social.rs)；公开动作建立 Publish→Challenge→Revoke→Confirm；另测 Retract/Expired 与新旧 backing；§5.5 计划由 Simulator required 回归逐项补证 | planned；实施候选、命令、各场景结果与未覆盖范围，由 Simulator social 维护者 / qa_engineer 记录 | 未执行行为测试；现有 fixture 不证明新增矩阵、全部产品验收或 runtime-live 入口 |
| [SOCIAL-AC-4](social-fact-ledger-declarative-reputation.prd.md#social-ac-4)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [独立义务 4](#social-obligation-4) | 合法首次挑战与原结算保持 | [现有 fixture](../../../crates/oasis7_simulator/src/tests/social.rs)；合法首次 Challenge、Confirm/Retract、撤销/过期；一次扣款与同额记录、一次原结算，上层幂等保持；§5.5 计划由 Simulator required 回归逐项补证 | planned；实施候选、命令、各场景结果与未覆盖范围，由 Simulator social 维护者 / qa_engineer 记录 | 未执行行为测试；现有 fixture 不证明新增矩阵、全部产品验收或 runtime-live 入口 |
| [SOCIAL-AC-5](social-fact-ledger-declarative-reputation.prd.md#social-ac-5)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [独立义务 5](#social-obligation-5) | 新动作 replay 与旧 journal 分离 | [现有 fixture](../../../crates/oasis7_simulator/src/tests/social_persist.rs)；动作前 snapshot+journal 对照 live；旧版本 journal 独立兼容验证；§5.5 计划由 Simulator required 回归逐项补证 | planned；实施候选、命令、各场景结果与未覆盖范围，由 Simulator social 维护者 / qa_engineer 记录 | 未执行行为测试；现有 fixture 不证明新增矩阵、全部产品验收或 runtime-live 入口 |
| [SOCIAL-PREVIEW-AC-1](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-1)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [来源](#social-preview-source) | 候选及 exact cut、阶段与闭包覆盖 | [手册01](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-01)：真实五动作来源，换主体/参数、混合cut、seed/重编号负例 | 同候选原件、覆盖证明、摘要、拒绝原因 | planned；未证明正式入口或全世界覆盖 |
| [SOCIAL-PREVIEW-AC-2](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-2)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [含义](#social-preview-meaning) | 声明/证据/裁决/内容分层，typed主体和signed权重 | [手册02](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-02)：真实事件无关claim、publisher Confirm、正零负权重与误导schema | 输入、独立语义oracle、玩家/Agent输出 | planned；不证明通用内容真实性 |
| [SOCIAL-PREVIEW-AC-3](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-3)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [效果](#social-preview-effects) | 五动作、逐owner/kind资源、维护与TTL | [手册03](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-03)：公开动作建例，quote/apply与维护事件对账；同owner容量、异kind和旧边反例 | 独立资源账、原生命周期与维护序列 | planned；旧5AC缺陷与全部runtime能力另验 |
| [SOCIAL-PREVIEW-AC-4](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-4)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [访问](#social-preview-access) | 当前授权与历史作者，最小披露 | [手册04](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-04)：A/B reader、Location同ID、撤权/转让、私有事件/consumer与返回竞态 | 有权与无权请求的正文/缓存/错误对照 | planned；不新增读取权限 |
| [SOCIAL-PREVIEW-AC-5](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-5)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [消费者](#social-preview-consumers) | adopted / unchanged / not-adopted / unknown，各入口和可理解结果 | [手册05](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-05)：真实consumer目录/求值、四/五route、旧DTO、Viewer与Agent问答 | 同source/consumer版本的独立结果与玩家判别记录 | planned；fixture不证明生产采用或交易/治理开放 |
| [SOCIAL-PREVIEW-AC-6](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-6)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [恢复](#social-preview-recovery) | 只读、超时取消、漂移和原动作未知恢复 | [手册06](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-06)：重复quote零写、旧响应、提交丢响应、G1、重启与回查 | 原operation、世界效果次数、读pin/slot释放及恢复轨迹 | planned；quote成功不证明World committed |
| [SOCIAL-PREVIEW-AC-7](social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-7)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [预算](#social-preview-budget) | 有限输入/依赖/并发与正常路径组合 | [手册07](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-07)：§9正常样例、各limit及组合超限、同reader公平和未终止I/O | 实际对象/bytes/节点/调用/内存、完整性、停止与恢复 | planned；静态计算不证明延迟或公开容量 |

## 12. 决策、长期风险与未决问题

| 选择 / 风险 | 理由与适用边界 | 主责与解除 / 复核触发 |
| --- | --- | --- |
| 保留原五动作与旧生命周期合同，新增只读语义 envelope | 玩家需要可判断的后果；直接改动作、仲裁或经济规则会超出原要求。新预览不得复制已知旧生命周期错误。 | Simulator owner；旧SOCIAL-AC-1–5修复及回归独立完成后才证明apply一致。 |
| 必须取得真实来源与有界索引 | 当前 Viewer seed clone / 全journal重编号无法证明同cut或容量；quote成功字符串不解决源断点。 | Runtime / Viewer owner；原生source、授权和coverage adapter实现并按手册01/04/07验证前，该profile不得宣称V2已支持。 |
| consumer 只按实际声明启用范围计算 | 拒绝schema字符串推导和固定positive，允许直接账本后果完整而其他采用未知；范围小但可以诚实验收。 | 各consumer owner；每次新增/撤销采用或规则版本变更，重核输入闭包、授权、单位和实际consumer验收，不扩张全局评分。 |
| 五动作外部入口尚未齐全 | Simulator合法前置可以验证直接语义；Viewer四quote及正式Agent无对应Action不能代签整个产品闭环。 | Viewer / Agent / Runtime owner；接入真实读源与现行提交/原结果回查后，逐入口取证；不以本设计新增ABI。 |
| 小范围有界预览首版 | 选择一次完整候选闭包，避免无限历史、跨页拼接和后台任务；宽fan-out或大历史可能不可完整预览。 | Runtime owner；真实负载超出§9时先评估可保持完整性的有界索引/预算调整，不能删除旧边或放宽真值/权限。 |
| 声明真实性与真实合作意愿仍有边界 | 内容引用、publisher确认、指定consumer规则均不证明所有事实真实或他人会合作。 | social / 产品owner；仅在原产品与具体consumer authority新增可验证合同后扩大含义，保持本轮未知与未采用区分。 |

本轮验证交付是设计、源代码事实核对与手册方法；实施、真实玩家理解、实际外部 Runtime、性能和生产采用仍须以其候选/环境证据确认。长期合同不以任务合入替代这些证据。
