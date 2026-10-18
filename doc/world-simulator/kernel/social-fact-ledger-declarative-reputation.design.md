# 社会事实账本与声明式关系层设计

- 对应需求文档: `doc/world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

## 1. 设计定位
定义面向生产级社会系统的最小语义内核：以内核事实账本记录可验证事实，以声明式关系边承载信任/合作/黑名单/声誉等多制度关系，并保持可回放、可审计、可治理。

### 目标与范围

本设计目标是上述事实/关系语义内核；§4 拥有约束边界，§5.1–5.5 逐项给出已有 Simulator 动作的目标资格、资源提交、终态与回放合同。下表只建立这些已有义务的准确追踪，不扩展产品承诺或运行模式；当前源码差距和未执行范围见末节。

## 2. 设计结构
- 事实账本层：支持发布、质疑、仲裁、撤销、过期等社会事实生命周期。
- 关系声明层：主体可基于事实声明关系边，不预设单一评分公式。
- 后果预览层：把事实/关系动作转换成玩家可读的信任、合作、黑名单、治理、claim 或交易协作表面影响。
- 治理护栏层：约束证据引用、可选质押、角色权限和状态机合法性。
- 事件溯源层：所有状态变更落入事件，支持跨节点重放与审计。
- 测试验证层：以 required/full 分层覆盖事实状态机、争议闭环与回放确定性。

## 3. 关键接口 / 入口
- `PublishSocialFact`
- `ChallengeSocialFact`
- `social_fact_impact_quote` / `relationship_consequence_preview`: `actor_id`、`action_kind`、`schema_id`、`subject_id`、`object_id`、`claim_summary`、`confidence_ppm`、`stake_at_risk`、`ttl_ticks`、`affected_relationships`、`affected_social_surfaces`、`cooperation_opportunity_delta`、`blacklist_or_dispute_risk`、`governance_or_claim_relevance`、`recommended_social_action`、`why_this_action_matters`
- 社会事实 ledger 与关系边状态机
- 证据引用 `WorldEventId`
- simulator 回放与持久化链路

## 4. 约束与边界
- 内核只维护可验证事实与状态，不解释为全局唯一声誉分。
- 事实必须可追溯到证据引用；质疑与裁决受当前生命周期、身份和资源条件限制，历史 challenge 不授予当前资格。
- 多 schema/关系维度并行共存，互不覆盖。
- `social_fact_impact_quote_missing` 只表示玩家无法判断本次社会动作的影响对象、可见社交表面、合作机会变化、争议/stake 风险或推荐理由；不得被解释为要新增全局声誉分、经济定价联动、复杂外交 UI 或 NPC 谈判系统。
- 本阶段不做性能压缩和经济定价联动，优先保证语义完整性。

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

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [SOCIAL-AC-1](social-fact-ledger-declarative-reputation.prd.md#social-ac-1)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 共享资格与只读 quote、执行时重核；限已有 Simulator 社会动作 | [独立义务 1](#social-obligation-1) | Simulator social 维护者 / 玩法规则维护者 / qa_engineer | 仅 REQ-WR-CR-003 的局部支撑；不增加仲裁权限、经济规则或 runtime-live 可达性；非行为已修证明 |
| [SOCIAL-AC-2](social-fact-ledger-declarative-reputation.prd.md#social-ac-2)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 拒绝无资源副作用与一次提交；限已有 Simulator 社会动作 | [独立义务 2](#social-obligation-2) | Simulator social 维护者 / 玩法规则维护者 / qa_engineer | 仅 REQ-WR-CR-003 的局部支撑；不增加仲裁权限、经济规则或 runtime-live 可达性；非行为已修证明 |
| [SOCIAL-AC-3](social-fact-ledger-declarative-reputation.prd.md#social-ac-3)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 终态、backing 与历史 challenge；限已有 Simulator 社会动作 | [独立义务 3](#social-obligation-3) | Simulator social 维护者 / 玩法规则维护者 / qa_engineer | 仅 REQ-WR-CR-003 的局部支撑；不增加仲裁权限、经济规则或 runtime-live 可达性；非行为已修证明 |
| [SOCIAL-AC-4](social-fact-ledger-declarative-reputation.prd.md#social-ac-4)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 合法首次挑战与原结算保持；限已有 Simulator 社会动作 | [独立义务 4](#social-obligation-4) | Simulator social 维护者 / 玩法规则维护者 / qa_engineer | 仅 REQ-WR-CR-003 的局部支撑；不增加仲裁权限、经济规则或 runtime-live 可达性；非行为已修证明 |
| [SOCIAL-AC-5](social-fact-ledger-declarative-reputation.prd.md#social-ac-5)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 新动作 replay 与旧 journal 分离；限已有 Simulator 社会动作 | [独立义务 5](#social-obligation-5) | Simulator social 维护者 / 玩法规则维护者 / qa_engineer | 仅 REQ-WR-CR-003 的局部支撑；不增加仲裁权限、经济规则或 runtime-live 可达性；非行为已修证明 |

### 11.1 验证映射表

| 上游 requirement / professional acceptance | 本设计条款 | 独立 obligation 与适用条件 | 准确验证方法、source / scenario / layer | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [SOCIAL-AC-1](social-fact-ledger-declarative-reputation.prd.md#social-ac-1)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [独立义务 1](#social-obligation-1) | 共享资格与只读 quote、执行时重核 | [现有 fixture](../../../crates/oasis7_simulator/src/tests/social_revoke_quote.rs)；状态矩阵分别调用 quote/apply；旧 quote 后撤销/过期；§5.5 计划由 Simulator required 回归逐项补证 | planned；实施候选、命令、各场景结果与未覆盖范围，由 Simulator social 维护者 / qa_engineer 记录 | 未执行行为测试；现有 fixture 不证明新增矩阵、全部产品验收或 runtime-live 入口 |
| [SOCIAL-AC-2](social-fact-ledger-declarative-reputation.prd.md#social-ac-2)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [独立义务 2](#social-obligation-2) | 拒绝无资源副作用与一次提交 | [现有 fixture](../../../crates/oasis7_simulator/src/tests/social.rs)；正 Data stake 对缺失/重复/终态事实重复提交；库存/pool/facts/edges 不变，时间与拒绝事件另断言；§5.5 计划由 Simulator required 回归逐项补证 | planned；实施候选、命令、各场景结果与未覆盖范围，由 Simulator social 维护者 / qa_engineer 记录 | 未执行行为测试；现有 fixture 不证明新增矩阵、全部产品验收或 runtime-live 入口 |
| [SOCIAL-AC-3](social-fact-ledger-declarative-reputation.prd.md#social-ac-3)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [独立义务 3](#social-obligation-3) | 终态、backing 与历史 challenge | [现有 fixture](../../../crates/oasis7_simulator/src/tests/social.rs)；公开动作建立 Publish→Challenge→Revoke→Confirm；另测 Retract/Expired 与新旧 backing；§5.5 计划由 Simulator required 回归逐项补证 | planned；实施候选、命令、各场景结果与未覆盖范围，由 Simulator social 维护者 / qa_engineer 记录 | 未执行行为测试；现有 fixture 不证明新增矩阵、全部产品验收或 runtime-live 入口 |
| [SOCIAL-AC-4](social-fact-ledger-declarative-reputation.prd.md#social-ac-4)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [独立义务 4](#social-obligation-4) | 合法首次挑战与原结算保持 | [现有 fixture](../../../crates/oasis7_simulator/src/tests/social.rs)；合法首次 Challenge、Confirm/Retract、撤销/过期；一次扣款与同额记录、一次原结算，上层幂等保持；§5.5 计划由 Simulator required 回归逐项补证 | planned；实施候选、命令、各场景结果与未覆盖范围，由 Simulator social 维护者 / qa_engineer 记录 | 未执行行为测试；现有 fixture 不证明新增矩阵、全部产品验收或 runtime-live 入口 |
| [SOCIAL-AC-5](social-fact-ledger-declarative-reputation.prd.md#social-ac-5)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [独立义务 5](#social-obligation-5) | 新动作 replay 与旧 journal 分离 | [现有 fixture](../../../crates/oasis7_simulator/src/tests/social_persist.rs)；动作前 snapshot+journal 对照 live；旧版本 journal 独立兼容验证；§5.5 计划由 Simulator required 回归逐项补证 | planned；实施候选、命令、各场景结果与未覆盖范围，由 Simulator social 维护者 / qa_engineer 记录 | 未执行行为测试；现有 fixture 不证明新增矩阵、全部产品验收或 runtime-live 入口 |

## 6. 设计演进计划
- 先固定社会事实与关系边的状态机语义。
- 再补治理动作、证据校验与回放持久化闭环。
- 最后通过测试矩阵验证跨节点重放、仲裁和过期逻辑稳定。

### 当前源码与残余风险

静态基线 `c1ae79da79c2cb7962a5e87bb3cee1b60eaf623e` 的 `simulator/kernel/social.rs` 中，`apply_challenge_social_fact` 仍在事实存在、lifecycle/challenge 检查前 `lock_social_stake`；`apply_adjudicate_social_fact` 检查 challenge 和发布者，却未先验证当前 lifecycle 为 Challenged。现有 quote/apply/replay 测试分工不等于正 stake 拒绝不变性、终态复活与旧 journal 兼容矩阵已通过。上述五项仍是目标设计和 planned 验证；不得由文档合入推出运行时已修复，不修改原仲裁主体、结算规则、ABI 或测试 tier。
