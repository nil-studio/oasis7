# 高影响保护动作的授权、复核与恢复设计

- 文档类型：专业系统设计。
- 生命周期：`active`；Owner role：`runtime_engineer`；协作政策 owner：`producer_system_designer` / `gameplay_designer`，身份/证明与验收分别由 P2P / QA 专业 owner 承接。
- 上游：[普通共同决策与宪制边界](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)。
- 适用阶段：沿上游原阶段；以下新协议与消费者是实现目标，不声明当前正式入口、线上授权或验收已完成。

## 1. 目标与范围

本设计补齐 GCB-006 已采纳的冻结、否决、降权、惩罚及紧急 containment 的正式授权、有限效果和救济链。复用现有身份/签名 authority、制度分类、原业务执行器、[ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0) 与持久化合同；不新增 native `Action`，不让普通票局、节点身份、部署者或 validator admission 获得新的紧急权力。

目录只映射本设计列出的原效果。没有精确来源与消费者的“其他高影响动作”不能作为开放命令。它必须先证明原专业合同已授予该效果，再经原受控 schema/政策激活进入目录；本设计不提供扩权入口。紧急保护不得改变普通白名单、不可修改保护、已确认历史、finality、signer/custody 或独立资产的原权利。

## 2. 上游要求与责任

Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 原义务与本设计承接 | 本设计条款（path#anchor） | 协作 authority / 消费者 | 适用边界 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | 正式提交、initiator/guardian 本次授权与独立复核角色 | [授权 producer](#protective-authority) | 原身份/签名 authority、经授权制度 policy 与 Runtime | 登记 node ID 或技术 admission proof 不代签。 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | 冻结、否决、降权、惩罚逐类有限效果与紧急防绕过 | [效果目录](#protective-action-catalog) | 原 proposal、身份、资格与 policy 执行器 | 不接管 validator/finality，不新增受罚资产。 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | 理由、充分证据、精确 target/scope/金额/期限与单次处置 | [证据与边界](#protective-evidence-bounds) | 原证据 producer、分类器与资产/资格 authority | 证据不足不产生部分惩罚；新请求键不生成新事件权。 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | 可用复核/申诉、有限期限、解除与恢复 | [复核和到期](#protective-review-expiry) | 原受影响主体、独立 reviewer/resolver、当前资格消费者 | 解除本件限制不能解除其他合法限制或重写旧票。 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | 唯一 receipt、原子失败、原请求恢复与提交未知 | [提交与恢复](#protective-commit-recovery) | Runtime 根事务、G1、正式 authority profile | 局部 prepared helper 不证明完整正式提交。 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | 玩家可核结果、当前读权、完整费用预算和正式消费验证 | [消费者与预算](#protective-consumers-budgets) | Viewer、pure API、Agent、QA | 本地成功不自动升阶；不能以拒绝样例代替授权完整正例。 |

## 3. 当前事实与目标差距

本节固定基线 `2731f781536018a857e5daf2cfa7ede504e9cd69` 的静态事实；测试源码不是运行记录。

| 当前依据 | 已存在的有限事实 | 本设计补齐的目标 |
| --- | --- | --- |
| [guardian 检查](../../../crates/oasis7_runtime/src/world/governance_internal.rs) | `validate_guardian_signers` 检查登记公钥的 node ID、去重和数量阈值。 | 验证 guardian 的当前精确权限及对本次完整 payload 的实际签名，不能用 ID 列表替代。 |
| [brake/veto 内部 API](../../../crates/oasis7_runtime/src/world/governance.rs) | brake 有正时长、上限与释放；veto 要求 `Approved` 且 queued。brake 实际阻断技术 proposal apply，不暂停整个 World。 | 正式发起者、理由/证据、范围、期限与独立结果关联；保持该有限作用面。 |
| [身份处罚/申诉内部 API](../../../crates/oasis7_runtime/src/world/governance_identity_penalty.rs)、[prepared reducer](../../../crates/oasis7_runtime/src/world/event_processing/prepared_governance_events.rs) | target+evidence incident 去重、slash 不超过锁仓、`Frozen`、申诉截止与接受后返还/恢复；actor 字段检查不等于正式角色授权。 | 正式 actor/authority、当前证据判定、期限与重叠限制、复核角色；保留既有历史版本。 |
| [Runtime 原子准备边界](../design.md#5-current-implementation-boundary-and-target-gap) | brake/veto/penalty/appeal 的 typed prepared publication 保留各自失败不半写保证；若干 sidecar 不在 `WorldState` root 中。 | 完整案例、授权消费、限制、金额、费用、索引和结果同根发布及恢复。 |
| [节点 slashing intent](../../../crates/oasis7_node/src/node_engine_slashing.rs) | `pending_governance_submission`、`enforced=false`；其证据/目标提示不是处罚结果。 | 经过本案正式证据/授权 producer 后才可提交，仍不能将 intent 自动生效。 |
| [正式治理 proof](../../../crates/oasis7_runtime/src/world/governance_authorization.rs)、[Node 入口白名单](../../../crates/oasis7_node/src/node_runtime_governance_auth.rs) | `GovernanceAuthorizationV1` 当前只消费 validator admission 与 finality key rotation 的精确 Action。 | 本专题使用自身经授权模块的精确 payload 与权限；不扩用该 proof、不添加伪造支持的现有 HTTP/CLI 命令。 |
| [policy 更新](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_gameplay_policy.rs)、[旧授权 helper](../../../crates/oasis7_runtime/src/world/event_processing.rs) | `blocked_agents/forbidden_location_ids` 由现有 policy 路径接收；旧 helper 的某个 `Passed` 不是精确高影响批准。 | 同一实质分类和高影响 guard 也覆盖该写路径及混合效果，不能只守三个紧急 helper。 |

[制度分类设计](governance-classification-and-constitutional-execution.design.md#governance-classification) 明确把本专题交给独立 authority。技术 atomicity、普通投票 `passed`、身份密钥存在、provider 接通与本案生效分别验收。

## 4. 结构与正式授权来源

<a id="protective-authority"></a>
### 4.1 已授权 policy 的 producer 与消费者

`ProtectivePolicyV1` 是本专题目标模块的版本化制度记录，不是部署配置中的自授权限。其正式 producer 使用已有[proof-bearing capability authority 准入](../../../crates/oasis7_runtime/src/world/capability_authorization_admin.rs)与[CapabilityGrantV2](../../../crates/oasis7_wasm_abi/src/capability_v2.rs)的精确命令授权；不得把“已有一位 authority”作为充分条件。接收链固定如下：

1. **信任根及当前 World。** 从固定 world/chain/genesis 和合法 authority profile 的合格历史取得[CapabilityAuthorityRecord/FinalityProof](../../../crates/oasis7_world_state/src/capability_authorization.rs)。原 verifier 核已知且 Approved/Applied 的精确 proposal/manifest、历史 finality epoch snapshot、证书阈值和 signer 集，以及签名覆盖的完整 authority binding：issuer/kind/key/key epoch/governance epoch、finalized/rotation receipt、world/branch/finality epoch/block/status。record hash、proof binding 和全部 signer 集须相同，issuer 的当前 key 也须匹配；[原验证与 transition](../../../crates/oasis7_runtime/src/world/capability_authorization_events.rs)拥有这些判定。裸 certificate、deployment config、旧无 proof 安装接口、active charter 或 validator admission proof 均不能补此链。它证明可信 issuer，不自动批准其任意业务效果。
2. **精确任命/规则授权。** 本目录只接受上述原 `issuer_kind=governance` 的已签且已登记 GrantV2；scope 必须精确为本案获准模块 M 的 ID/version、namespace `governance.protection`、`object_kind=command/operation=execute`，以及原批准 schema 中分别声明的 `policy.publish` 或 `role.bind` 对象名。固定 subject/owner-generation、audience world/branch/target instance、entity/resource selectors、policy_class、payload 上限、grant nonce/body hash、有效期和当前撤销 epoch。policy/角色记录必须绑定这个 grant ID、issuer receipt、原业务批准 receipt 和其准确效果摘要；普通票局或技术 manifest 安装没有该精确保护授权时仍为 `needs_authority`。命令名是目标 module schema，不新增 native Action，也不声称当前部署已暴露它们。
3. **初始化与实际发布。** 先由原 `install_capability_authority_record_with_finality_proof` / `AuthorityInstalledWithProof` 验证并记录 issuer，再由该 issuer 按原 GrantRegistered 路径给真实 policy publisher 发精确 grant。publisher 经原 subject-bound invocation/context/预算进入目标模块；模块校验上游业务批准与 GCB-006 保护底线，首次写入 policy/任命原件及其 source 链，并与原结果同根发布。 `role.bind` 记录任命而不替其他主体签发 grant；I/G/R 等各自所需命令 grant 仍由原 issuer 单独签发，不能从 publisher 的 parent grant 派生为另一个 subject。空模块状态、genesis 文件或首次请求都不自动产生角色。对现有已批准保护规则的任命只是消费其原范围；任何实质新权限仍须原分类与独立制度流程，不能由本模块自批。
4. **使用、替换与撤销。** `case.submit/confirm/execute/appeal/resolve/release` 分别使用声明对应对象名的独立 scope 和角色断言，不能复用 `policy.publish/role.bind` 或另一角色权限。原 GrantV2 的 issuer 签名、parent chain、subject/audience、scope attenuation、有效期、revocation/supersession 和 current key epoch 每次重验；不能以子 grant 更换主体或扩大父 scope。policy/角色替换沿原获准新修订发布，保留前驱和 source；原 grant 的 revoke/supersede/trust-root rotation 走 proof-bearing 同根 transition，遵守 revocation 单调、替代链无环、key epoch 严格递增和 rotation receipt，旧 key/撤销 grant 不能新签确认。已有结果、申诉和原件查询仍保留，换实例/角色不重置处置槽。

上述具体原信任和 grant 承载已存在；本专题的 policy/role schema、业务批准到这些精确 scope 的 producer 及正式 adapter 是本次明确的实现目标。它不把 grant 的技术签发权等同于无界政治权力；缺任一业务授权/角色/实际 producer 时阻断该保护能力，不能从普通 Passed、技术 finality 或 current charter 状态反造授权。

policy 固定 `world/chain/genesis`、原 issuer/authority identity、module/schema/artifact 与 policy revision/digest、有效窗口、原 source receipt/commit/精确 grant、允许效果目录、jurisdiction、角色任命及撤销版本、guardian 阈值、证据判定版本、金额/持续时间上限、`review_due_tick/appeal_deadline_tick` 及独立复核接收者、预算 payer 与恢复规则。该版本缺任一必要字段为 `needs_policy`，不能用普通票局默认值或现有节点名单补齐。规则、source 与撤销遵守[共同权威输入约定](../design.md#runtime-gameplay-authority-designs)：验证后的 canonical 输入可重算，禁止执行中实时网络查询或模型判断。

`ProtectiveRoleAssertionV1` 由该精确 `role.bind` producer 发布，绑定 issuer、holder 的真实主体/当前身份代际及 signing key、role、精确 capability/target scope、原 grant、有效区间、revocation revision、source receipt/commit 和 policy digest。身份 authority 只证明绑定与签名来源；其结果不能替代制度角色。Runtime 按同一当前合格 parent 检查 policy、任命、撤销和目标；入口预检不能冻结未来权限。

| 角色 | 能提交的事实/操作 | 必须独立核验的权限边界 |
| --- | --- | --- |
| initiator | 提交准确案例及拟议完整效果 | 当前有该目录项和 target/scope 的发起权，并签完整本次请求；文本 `initiator` 不足。 |
| guardian | 对同一案例修订、证据判断和完整效果确认 | 每个实际签名均由当前角色/key 验证；去重按 policy 的原主体规则，别名/key 轮换不多计。阈值取当前合法 policy；fixture 的 2 来自现有默认值，不覆盖其他合法版本。 |
| evidence producer / reviewer | 提供原始证据及本案适用性判断 | source 真正有对应事实/审查职责；本案 payload 必须绑定精确 predicate、target 与证据摘要。未经授权的举报只能是待核材料。 |
| appellant | 对自身受影响结果或具有明确代理权的该结果提出申诉 | 当前读权和受影响关系/精确代理权，签该 case/result；不要求仍拥有被临时冻结的普通行动资格。 |
| resolver | 对本案申诉/复核作接受、驳回或有界纠正 | 当前独立复核权限；与本案 initiator、原决定 guardian、target/controller 的 policy 禁止利益重合逐项核验，不能自审或靠另一账户代签。 |
| executor | 释放已经完整获准的效果 | 只消费同案同修订的执行槽及原业务 authority；不能改金额、范围、截止、证据或付费人。 |

目标模块使用既有批准的 module command/权限入口；表内名称是逻辑角色和 schema，不是已存在的新 native Action 或可调用 CLI。输入端、模块、Kernel effect adapter 和恢复端消费同一角色/范围验证结果。原 guardian node key 可作为 policy 已任命主体的签名 key；仅被节点 registry 登记不取得本案权限。

### 4.2 本次签名与请求身份

`ProtectiveRequestV1` 采用原签名库和原协议的确定编码，域为 `oasis7.protective-governance.v1`。签名覆盖：world/chain/genesis、目标 module/schema/artifact/policy digest、合格 parent commit/root、case/incident 与 revision、原 operation/nonce、操作种类、规范化完整效果和 digest、target/controller/scope、金额及原单位/账源、证据 refs/digest、有效/复核/申诉窗口、角色及其 authority/revocation revisions、payer/费用边界。每项列表按 schema 确定顺序且无重复/未知字段；签同名 case、附件 hash 或旧 payload 均不能替代本请求签名。

initiator 与所需 guardian 各自签同一内容；review/appeal/release 使用绑定原结果的后继请求。不同 guardian 签了不同 scope/截止/金额时不能拼合阈值。原请求身份、完整规范 bytes/digest、签名与期待结果查询键在发送前保存；重连仅查询原件，未知结果不生成新的业务请求。

## 5. 保护效果、证据和救济

<a id="protective-action-catalog"></a>
### 5.1 有限效果目录与原执行器

| 原效果类别 | 精确作用对象与释放条件 | 期限/恢复和禁止扩大之处 |
| --- | --- | --- |
| technical governance brake / release | 当前受支持的技术 proposal apply gate；复用原 brake producer/准备语义与合法 guardian policy。 | 时长 `0<duration<=原 emergency_brake_max_ticks`，有效窗半开；不暂停 World tick、普通玩家、审计或申诉，不改变 finality。新 case 不能借重复本案延长。 |
| queued proposal veto | 指定 proposal ID/revision、approved manifest/effect digest，执行时仍为原 `Approved` 且 queued。 | 只终止该尚未生效的排程。已 Applied/改变 payload/不在范围则拒绝；到期或申诉接受不能重新激活旧提案，后续须沿原合法重提/批准，不重写旧拒绝。 |
| identity freeze / 原 identity penalty | 精确治理 identity profile 与原受支持的治理资格；penalty 的 slash 使用原 `stake_locked` 单位和同一原记录。 | 临时资格限制有明确截止；slash 是签定金额的一次记账，不是每 tick 扣取，且不超过当时可罚余额/原授权上限。原内部字段不证明外链 OC 或独立资产罚没；没有该资产域原授权与同根映射就不能扩用。 |
| 当前资格域的 temporary demotion | 已存在的普通治理资格/影响上限 scope；输出下一节的有界 restriction，由[原资格消费者](ordinary-governance-subject-and-oc-rights.design.md#og-lock-delegation-cap)取当前合法 cap 与限制 cap 的最小值。 | 只能收窄本 scope 的新准入/未来有效影响；不更改原 cap 算法、历史快照/已确认票、OC/custody 或 validator 角色。冻结是相应新准入为 0；不创造额外降权业务命令。 |
| 已支持 policy 中的个人/地点限制 | `blocked_agents/forbidden_location_ids` 等实际效果先经原实质分类；需要高影响权限时，原 policy adapter 消费精确本案 target/scope/期限。 | 不因写在普通 policy payload 内而免审，不给一个普通 Passed 复用额度。含越权保护的混合 payload 整体无业务效果；合法普通项仍走原普通分类。 |

所有对象须列明原权利来源、基线版本与实际受影响能力。`ProtectiveRestrictionV1` 冻结字段为 `restriction_id/case_id/original_result_ref/world_id/target_subject/controller_scope/capability_scope/matter_or_region_scope/cap_if_applicable/[effective_from,effective_until)/policy_schema_authority_versions/current_proof/revocation_review_refs`。它不是新权利凭证，只能限制原消费者本来允许的集合。

原资格 snapshot quota 不变。提交新 cast 或原规则允许的显式同 origin 替换时，消费者在完整候选写集上核当前 cap；requested 超限整笔拒绝，不自动裁剪。已确认 ballot7 及代表占位不因新 cap3 自动重算或释放；只有原单一合同允许的同 ballot key 显式替换、独立 correction 或事项关闭才能调整有效贡献/占位。protective restriction 不授予 duplicate-subject 事实裁决权；freeze 也不关闭原有权 query/revoke/exit/appeal。

每项限制与原结果独立保留。有效资格是当前原合法资格与全部仍有效限制的交集；cap 取最小值。解除/到期只去掉本件贡献，不能用旧 `identity_status_before` 覆盖当前其他冻结、合法基础状态变更或其他 cap。brake 同理按仍有效且同 scope 的案例计算最大截止；原单个 `release=None` helper 不能无条件清掉另案。新适配在同根 staged replacement 中合成真实后态，保留旧 helper/旧版本重放；不串行执行两个会重复扣款或清账的发布。 原 brake 实际覆盖技术 proposal apply gate，不能只签一个 proposal 的狭窄 scope 却调用全局 brake；单提案中止走原 veto。policy adapter 对新增、删除或减少保护限制都核同一原 authority，普通 payload 换空列表不能解除本案或另案限制。

<a id="protective-evidence-bounds"></a>
### 5.2 证据充分性、target、金额和期限

原始证据必须来自 policy 声明的原 authority，含 schema/version、事件/对象及发生切点、完整被证事实、target 映射、签名或可验证 canonical commit/proof。`evidence_hash` 非空只是格式检查，hash、threat heatmap、截图、举报、LLM 结论或 pending slashing intent 都不能单独证明可处罚。

有限 evidence adapter 有两类：(1) 原已验证的 consensus misbehavior facts，复用原证据规则检查冲突的已签消息、同一逻辑位置/类型、当时 validator/node 与 stake root、精确 target 映射和真实可罚范围；不重定义共识违规或从 validator ID 字符串猜 Agent。(2) 原制度/业务权限违规，原 authorized reviewer 绑定精确已声明规则、原对象/效果与事实，出具当前 policy 指定 predicate 的可验证判断；不能从“危险”自由文本直接生成 sanction。每类的角色、predicate、对象范围与证据 schema 是 policy 的闭合字段；未支持证据类型为 `unsupported_evidence`，缺原件/目标证据为 `needs_evidence`，零初始保护效果。

在同一 staged view 内同时核：target 当前存在/属于该 jurisdiction；controller/身份映射和原资格版本；证据与该 target、原事件/时间及拟议效果一致；完整效果不触及保护底线；金额非负、单位/账源准确、不超过 policy cap 与原可罚余额；半开有效窗非空、不过原最大期，申诉/复核窗口及可用接收者明确。金额不足不裁剪到余额，期限超界不自动缩短，效果减少也必须由原签名/政策允许的确定性收窄规则授权；否则返回准确缺项、重新明确提交。

初始处置槽由执行器固定为 `(world,authority_namespace,canonical_incident,target,initial_disposition)`；旧 identity penalty 保留其原 `target+evidence` incident 映射与历史。证据的重复附件、排列、请求 ID、Agent 别名、政策迁移或 key 轮换不能生成新槽。初始 freeze+slash 等获准组合整体只有一个结果，不拆成重复处罚。新的事实须证明是不同原事件；同案复核/申诉/返还沿原结果追加有唯一后继槽的救济，不是第二次初始处罚。

<a id="protective-review-expiry"></a>
### 5.3 独立复核、申诉、到期及并发恢复

案例状态区分 `draft/needs_evidence/awaiting_authority/ready/applied/review_pending/appealed/resolved/expired`；`submitted/ready` 不产生限制或扣款。正式效果以唯一 execution receipt 为准。受影响主体可按当前读权查询原理由、公开证据摘要、准确范围/金额/截止、复核/申诉入口和原结果；必要私下证据仅对原有权角色开放。

每个 policy 必须给出非空申诉窗口和独立 resolver 的当前可用接收路径；不得先启用保护再关闭审计/申诉。身份冻结只限制所声明业务能力，不撤销本人对案例的查询与申诉资格。代理必须已有精确授权；失去普通角色、退出组织或执行方更换不删除原案、已提申诉与救济权。无法确认合格 resolver 或指定复核期限时，不把该 policy 标可执行。

申诉按原 deadline 的端点规则接收；现有 identity penalty 是 `tick<=appeal_deadline_tick`。复核绑定原 case/revision/result、原证据与新反证、resolver 权限/独立性及理由。接受时只按原救济规则释放本件限制并返还本件实际 slash 一次；驳回时不返还。不能以接受申诉重放原处罚、改写已确认事件或自动生效被 veto 的提案。超时未决保留可查申诉，不认作驳回；临时限制仍不得越过签定截止。

限制的有效性直接按权威逻辑时间判断 `[from,until)`；每个相关执行/资格消费者都应用该谓词，不靠页面刷新、GC 或后台调度才解除。tick/recovery 的到期投影与审计 receipt 使用固定 `(restriction_id,expiry)` 槽；同 tick 多入口/重启不重复返还或生成第二次恢复。临时到期只恢复该件资格限制，已经一次执行的 slash 不自动退款；其返还须有原接受救济结果。veto 保留已经发生的拒绝事实。

解除与到期竞争、两个独立处罚/限制重叠、基础资格合法变化均在同一当前 view 重算。禁止盲写申请时的 profile 快照。 例如原100先扣本案10、再有另一合法案扣7，当前83；接受本案申诉只返本案实际10，结果93且另案限制仍在，不能整对象恢复为100/Active。旧 penalty 的 accepted 恢复语义仍按历史版本重放；新目录版本只对其完整 restriction/结果记录提供上述组合保证，不反造旧案已缺失的期限或角色证明。

## 6. 接口及可追溯数据

| 逻辑记录 / 接口 | producer → consumer | 精确身份与不变量 | 可见结果 |
| --- | --- | --- | --- |
| `ProtectivePolicyV1/RoleAssertionV1` | 原获授权制度源 → 模块/Kernel | 原 source commit/receipt、scope、版本/有效期/撤销；不得自任命 | current/missing/revoked/expired/unsupported |
| `ProtectiveCaseV1` | 已鉴权 initiator → 有权审查者 | 原 incident、target、effects、reason、完整证据 refs、policy、预算及所有时间边界 | 待补/待确认/可执行；无隐形限制 |
| guardian/review attestation | 各真实角色 → 案例闸门 | 同 payload digest、角色断言、当前身份、精确签名、独立性和时间 | 每一项单独可核，不能仅一个 approved 布尔值 |
| `ProtectiveRestrictionV1` | 原获准案例 → 资格/业务消费者 | 只约束其 exact target/scope；原结果引用与半开有效窗 | 当前限制/到期/已解除与准确原因 |
| execution / remedy receipt | 唯一槽的根事务 → 原请求查询/审计 | 同案同修订同操作返回同原件；冲突拒绝；金额和效果完整 | applied/rejected/outcome_unknown/linked remedy |
| `GetProtectiveOutcome` | 当前获准 reader → Viewer/pure API/Agent | 查询原 world/module/case/operation；先核当前读权，再解析原结果 | receipt 与生效状态、原来源、缺项和唯一安全下一步 |

上述是目标模块 schema 和适配责任，不声明当前 wire 已具这些字段。提交者不得通过客户端提供的 receipt 路径直接更新权威索引。原 canonical 结果和索引是查询依据；projection、页面文案、当前余额或不存在于当前页的记录不能反推原请求未提交。

## 7. 原子提交与恢复

<a id="protective-commit-recovery"></a>
提交顺序固定：当前鉴权/读权 → 查询原请求及初始处置/后继槽 → 验证 payload 冲突 → 在同一合格 parent/staged view 重验全部 policy/role/证据/金额/期限/范围/预算 → 准备完整效果 → 一次发布。已有同内容结果返回原 receipt，不因后来权限过期而重复执行；读权不足不泄露案卷，也不执行新的写入。

完整事务包含 policy/role 消费标记、模块实例状态、case/restriction、proposal/profile/精确账源、实际费用与原返还、incident/operation/结果索引、计时状态、审计/事件/journal/allocator、receipt、outbox 与 commitment。使用原[staged typed delta](../design.md#621-explicit-transaction-and-typed-delta-model)及[disposition/idempotency](../design.md#622-disposition-and-idempotency-contract)；已有 brake/appeal sidecar 的局部原子性不能代签该集合。任何需要的后置来源失败不能先留下一半冻结、罚款、费用或审查状态。

[普通 G1](../prd.md#runtime-generation-persistence-contract) 的完整候选绑定稳定原操作、base generation 与 snapshot/journal/module store/sidecars/索引/receipt/outbox 全部内容：`NotCommitted` 才可丢弃未提交候选并保留旧 World；`Committed` 先核原候选和结果，安装或恢复该 generation；`StatusUnknown` 保留完整候选，隔离旧 World 与 tick 的继续发布，按原身份查清后解除隔离。禁止回滚猜测、混合新旧 generation、换 case/nonce 再处罚。

本地 `Committed` 不代签[正式 authority profile](../prd.md#runtime-authority-profile-commit) 的提交证明、独立故障域或恢复 serving。原件尚不能证实正式生效时仍显示实际待核状态。提交后通知/读面/维护失败不撤销已提交结果；outbox 只在提交后投递，canonical replay 只恢复账本/索引，不重新收签名、请求裁决、扣款或发送外部效果。每次 restore 核完整旧案、非空日志后缀、限制有效期/原政策、去重槽和已付/返还，缺数据阻断相关新写，不能用当前 policy 重造历史。

## 8. 权限、分类与兼容入口

所有实际写入路径，包括模块命令、native 兼容动作、direct/trusted helper、tick directive、policy 更新和迁移，均在真正 effect release 点核相同实质分类和本案 guard。经过普通治理、技术升级、嵌套复合 payload、先执行后追认均不能取得未声明的紧急权限。原[分类/提交合同](governance-classification-and-constitutional-execution.design.md#governance-commit)和[真实执行消费者](governance-classification-and-constitutional-execution.design.md#governance-execution-consumers)继续拥有分类、保护底线与普通写入接点，本专题不复制白名单或普通业务 schema。

部署正式支持必须声明确切 profile/manifest、caller 类型、签名/role producer、原 command transport、查询入口和结果 authority。当前正式入口没有本专题适配时为 unsupported/blocked，并保留原只读能力；不能把内部 helper 的正例、现成 validator proof、M5 普通票局 finalized directive 或 node intent 称作正式生效。trusted fixture 只验内部 reducer 层，不表示远程可绕过。

## 9. 消费者、费用和有限容量

<a id="protective-consumers-budgets"></a>
Viewer、pure API 与 Agent 对同一 case/result 投影相同事实：待补/已确认条件/已生效/申诉中/到期或解除/结果未知；准确 target/scope/金额/期限、原 authority/结果引用、当前可用查询/申诉下一步。只读状态不能补原 receipt。用户没有相应角色时仅显示其可读事实，不提供假的可提交按钮或默认 admin fallback。个人签名、guardian 操作和独立审查各沿其实际授权界面，SDK 的局部能力不代签整个入口。

目标有界 profile `protective_case_v1` 的上限是设计初值，未经性能或平衡实测：完整签名请求原件≤64 KiB；总签名校验≤16（其中 guardian≤12）；证据项≤16；完整来源对象≤64、raw/解码来源各≤256 KiB；单 invocation 原 state≤32 KiB，完整 ABI input≤256 KiB、output≤128 KiB，`max_effects=1/max_emits=1`。一个 effect 是本案已规范化完整原子处置，不能把隐藏的另一次 module invocation 或多案当同一 effect。超过任一界限在业务写前拒绝，不截断 guardian/证据/旧案/fee 图；合法 policy 阈值或旧状态无法容纳时，该 profile 不适用，须沿原版本化配置/迁移合同处理。

ABI 沿用现有[类型](../../../crates/oasis7_wasm_abi/src/lib.rs)及[trusted command 编码路径](../../../crates/oasis7_runtime/src/world/capability_authorization_command.rs)：先编码 `ModuleCommandEnvelope`，再将完整编码作为 `ModuleCallInput.action` 编入 input。`payload` 与 `action` 均按普通 `Vec<u8>` 整数数组编码，须计入两层展开；请求对 input 的贡献可接近原 payload 的4倍，另计 state、ctx 与封套。ctx 的完整编码上限仍为32 KiB，其余外层封套为16 KiB。原请求64 KiB、raw state32 KiB与完整 input256 KiB是必须同时满足的独立限额，不保证原始项可同时取满；完整编码超限时在模块调用前拒绝，不截断或改用另一种序列化绕过。

output 仍按 new_state 字节元素64+完整 effect/emit32+封套16=112 KiB≤128 KiB分配；数组头、字段键及其他外层结构计入封套配额。effect/emit 配额是完整编码后上限，各分额与完整实际 output 均须满足。

host 请求缓冲/投影工作集的设计分额为原件64+解码128+source256+source解码256+input256+output128+staged128+response64+scratch256=1536 KiB<2 MiB；最多4个并发按每槽2 MiB预留合计8 MiB，同案例写串行，无额外无界队列。编码后 envelope、输入副本和 codec 临时结构的同时存活峰值须合计到相应分额及共享 scratch 内；组合装不下即拒绝该 profile 或沿原版本化预算处理，不能把这些分配遗漏到预算之外。该分额算术不是现有 codec 的峰值证明，实际实现仍须核验。它不包含WASM线性内存/引擎缓存：实际manifest与executor另取严；按[现有Guarded上限](../../../crates/oasis7_chain_runtime/src/host/controlled_execution_policy.rs)的64 MiB memory/10,000,000 fuel每调用上限，4个实例的线性内存至多256 MiB，加上述buffer预留为264 MiB。引擎/编译缓存继续受原独立配置预算约束并计入部署总预算，不称整个进程仅8 MiB。父级已验证 authority/finality 能力按原合同消费；未验证证明不能绕到该预算之外查询。较小的 host/world/module/transport 上限取严，不能自动调大。

费用复用[原 module fee](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)，不增加罚款以外的收费。每个真实 invocation 列原 owner/payer、grant/预算、完整 I/O、效果/emit/new_state、原 required hooks、原业务收费和实际/预检量；payer 的授权不从受罚人推导。上界 `Data=ceil(I/1024)+128+2×1+1`、`Electricity=1+1+1+1=4`，最大 I 时为387/4。实际 `Data=ceil(I/1024)+ceil(O/1024)+2×effects+emits`，`Electricity=1+effects+emits+has_new_state`；预检余量不是另一次扣费。 对实际 input8192 bytes 的该单 invocation，预检为139D/4E（8+128+2+1），并非一律要求profile最大387D；138D拒绝、139D可通过该项预检，仍须满足其他实际前置。若真实图有额外执行/索引/receipt 消费，逐项相加并重算，不宣称只一 call；原 native/direct 路径未收此模块费时也不得虚构扣费。原请求查询、重复原件回读和 canonical replay 不重跑初始收费调用。

trusted command 还必须满足独立[CapabilityBudgetAccount](../../../crates/oasis7_runtime/src/world/capability_authorization_state.rs)及[原 reserve/settle](../../../crates/oasis7_runtime/src/world/capability_authorization_command.rs)，不能只看Data/E。它按 `(subject,grant_id)` 记录 `remaining/reserved/spent`，以 `response.envelope.payload` 的原字节长F计算 `reserve=ceil(F/1024)+128+2×1+1`，实际为 `ceil(F/1024)+ceil(O/1024)+2×effects+emits`；完整ABI输入I只用于上述Runtime资源费用。上界F64 KiB时reserve195单位。原caller/grant/账户授权与余额先核，reservation和actual settlement连同nonce/receipt在同一根事务中发布，预检余量返还到同一账户，不成为Data/E、不另向受罚人扣取。

独立双账样例固定实际F2048、I8192、O4096及1effect/1emit/new_state：capability reserve133、actual9，初始remaining200/reserved0/spent0最终191/0/9；Runtime另为Data预检139、实际15与Electricity4，P400D/10E最终385D/6E。capability132不足，即使Data/E充足仍在sandbox前拒绝；Data138不足，即使capability200也不能入。提交前失败不留下hold/费用/nonce，已提交同原请求不重复两种结算。额外真实hook或第二调用的两账分别重新纳入原图，不以同一数字代两套计量。

案例状态与历史结果不能因容量耗尽清掉唯一性索引。所有未结束案例、当前限制、原初始/救济槽和查询所需归档引用纳入完整 state/source 预算；容量不足停止该 profile 的新准入，保留查询/安全恢复。合法迁移须保留旧 world/module/case/incident/operation 到准确原结果的映射、权限/期限与原 policy，不能靠换实例恢复一次处罚额度。

## 10. 兼容与发布

新 policy/schema/消费者通过原受控激活启用；先有可验证 reader/恢复和原结果查询，再开放新写。旧自由 actor 字符串、node ID 列表、旧 no-signature helper 和旧案记录只保留其历史含义，不自动补成新角色证明。旧 brake max/release、旧 penalty evidence-chain 与 refund/status 仍按原版本重放；新组合限制不追溯改变它们。

缺少新 restriction/索引/政策的旧节点不接新写。回滚仅退到仍能读取新记录、维持有效限制和恢复的兼容版本；不能抹掉已生效保护或取消申诉。实施与正式入口验收均另行提供同候选证据，本文件不宣称当前网络、发行或玩家可用性升阶。

## 11. 验证设计

方法主责为[普通与保护治理验收手册](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md)。以下独立预期是目标 oracle，不是运行记录；正式正例须有真实签名/authority producer/reader/适配，缺失应在对应层 blocked，不能用全是拒绝的样例代签。

- **PROTECT-01，角色与证据**：固定 policy P 与准确来源，initiator I、当前两 guardian G1/G2 各签同一 payload，独立 resolver R。合法对象/窗口/金额产生一次完整结果；复制 ID、同一主体换 key 双计、错签 digest、撤销、越 scope、缺证据/target 映射、只有 validator proof/node intent 各零初始效果。普通 policy 合法正例与夹带越权 blocked_agents/封路的混合 payload 配对，后者整体无效果。
- **PROTECT-02，期限/金额/降权**：brake 在 t100 请求10，生效 `[100,110)`，原合法 max720；同请求 t105 重读不延长，t110 原阻断谓词失效。另案至114仍阻断，释放前案不能清空后案。penalty fixture 的原 locked100/slash10→90；policy 明定冻结至140、原32tick申诉窗至132，132可提/133拒绝；独立 accepted 返还10一次，rejected不返还，140只去掉本案临时限制。降权原 cap7→本案3、窗口 `[100,110)`，旧snapshot7不改；另案cap2仍有效时，前案解除后结果仍2。金额101>100、超原时长或期限含糊均拒绝，不裁剪。
- **PROTECT-03，否决/救济**：原 Approved+queued 且 digest 相符的提案可 veto；Applied、换 payload 或不同提案均拒绝。复核接受也不恢复旧 queued/Applied 状态；本人被冻结仍能提交原案申诉，原决策者自审拒绝；另人无代理不能查看私下证据或代诉。当前基础资格变化/多案重叠后，仅恢复本件实际影响。
- **PROTECT-04，原子与恢复**：提交前各写点失败保持原 case/profile/proposal/金额/费用/事件/ID 不变；提交后丢响应按原件回读，换nonce/别名/实例不能再罚。G1 三态分别核完整 staged+base generation、非空tail与旧World/tick隔离，正式 authority proof 独立核验；outbox/投影失败不回滚结果。到期与解除/申诉并发、重启和 replay 均只消费各自唯一槽一次。
- **PROTECT-05，容量/费用/读面**：按两层请求编码核独立原始项及完整I/O限额，包含原始项合规而完整input超限的拒绝；核output112 KiB组成、host1536 KiB各分额及同时存活中间结构/副本峰值，WASM/caches独立核验。超签名、对象、bytes、旧状态或隐藏第二call须拒绝/重算。若实际完整 I8192/O4096、1effect/1emit/new_state，则原 fee 为Data15/E4，真实payer D400/E10→385/6；不得同时扣预检387/4。按该实际 input 的预检139D/4E核138D拒绝、139D可入且实际仍15D/4E；387D只对应最大input。补充合法额度只恢复原可重试未提交操作。另以F2048核capability133预留/9实际及200→191、132拒绝，保持两账/hold/nonce同根；三入口同原结果/当前读权/未知安全下一步一致，合法读面不因缺写适配被关闭。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | [授权](#protective-authority) | 真正角色 producer 与本次签名，不从技术身份扩权 | [GOV08](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-08-protective-authority)，PROTECT-01/03；同候选 required/full 分层，固定 P/I/G1/G2/R 和正反 payload | role/source/签名原件、受理/拒绝、真实 execution receipt | 不由内部 helper 或仅 source 阅读证明正式可达。 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | [目录](#protective-action-catalog) | 各原效果有限作用、旧历史保持、policy 旁路拒绝 | [GOV08](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-08-protective-authority)，PROTECT-01/02/03；各目录实际 adapter 正反例，不用一种通过代替全部 | 前后资格/proposal/账源与旧 snapshot/receipt、关联 case | 不授权目录外效果或改变外链资产。 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | [证据边界](#protective-evidence-bounds) | 充分证据、精确 target/scope/金额/期限、唯一初始处置 | [GOV08](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-08-protective-authority)，PROTECT-01/02；固定原事件、资格和余额的成对样例 | predicate/source/目标映射、拒绝原因、前后余额/唯一槽 | 证据 hash 非充分证据，参数未称平衡实测。 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | [复核/到期](#protective-review-expiry) | 真实复核/申诉角色与到期仅解除本件 | [GOV08](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-08-protective-authority)并用同手册GOV09，PROTECT-02/03/04；含deadline边界、多案和并发恢复 | 原申诉/裁决/返还、当前资格与旧历史、固定expiry槽 | 不把未决视为驳回或后台刷新当期限语义。 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | [提交/恢复](#protective-commit-recovery) | 完整事务、唯一结果、G1未知隔离与原件恢复 | [GOV09](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-09-recovery)，PROTECT-04；full含正式proof/恢复，同候选base与非空tail | 所有staged/root/金额/费用/索引、三态、原结果/outbox | local commit不代正式authority或恢复serving。 |
| [REQ-WR-GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)；Trace refs: O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8 | [消费者/预算](#protective-consumers-budgets) | 原费用、完整有界输入输出、当前读权及跨入口结果 | [GOV10](../../testing/manual/ordinary-and-protective-governance-acceptance.manual.md#gov-10-bounds-surfaces)，PROTECT-05；真实完整I/O和调用图，required/full按原门禁 | payer/fee图、精确字节与拒绝、三入口同原件/安全下一步 | 不以文档算例代性能、已接CLI或发行验证。 |

## 12. 设计决策、实施边界与风险

选择逐 case 的原授权和限制叠加，目的是保留其他合法资格、旧责任与历史；选择有限 profile 并取所有下层限额的最严值，避免把缺证据或超预算误作允许。角色源、作用域、schema/manifest、费用图或历史索引变动时，须重新核对应 adapter 与同候选正反/恢复方法；缺生产者只阻其声明能力，不关闭无关合法读面。

本设计已明确原产品义务的正式 producer/角色、有限目录、证据/金额/期限、消费与恢复合同及有界独立方法。仍需实现该版本制度记录与签名/证据适配、当前资格/效果 release guard、完整 root/G1 索引与原件读面，并按同候选执行 required/full 验收；原内部 prepared seams、原签名工具和相邻治理设计提供可复用基础，不独自证明本专题实现。源代码与旧行为在本设计交付中保持原样。
