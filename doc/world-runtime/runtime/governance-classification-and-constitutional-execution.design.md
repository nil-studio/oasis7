# 普通治理分类与独立宪制执行系统设计

- 设计 ID：`DES-WR-GOV`
- 状态：`active`，长期目标合同；不表示运行时、正式玩家入口或发布验收已经完成。
- Owner role：`runtime_engineer`；`producer_system_designer` / `gameplay_designer` 审查制度边界，`blockchain_ops_engineer` 审查身份、授权与独立性，`qa_engineer` 审查验证。
- 固定审读基线：`nil-studio/oasis7`，`e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`，2026-10-19。相对源码链接指该基线的组件位置；实际实现和验证版本由相应 PR 记录。
- 上游：[普通共同决策与宪制边界](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)。

## 1. 问题、目标与非目标

普通票局能记录提案、投票和通过结果，但自由文本、`passed` 或技术 manifest 的延迟与 finality 都不能说明事项属于哪种权力、授权从何而来、独立宪制条件是否齐备。本设计把“事项声明与分类”“审议结果”“当前可执行授权”“世界生效结果”分开，在任何业务扣减、占用和权限变更前拒绝越界或条件缺失的请求。

目标是普通事项具备可检查的白名单与授权来源，其他合法宪制事项具备独立合取闸门，并可从声明、审查、异议追溯到唯一结果。不可改写的保护在所有路径都先检查，不能把受保护事项送入另一轨道后绕过禁止。

本设计承接 GCB-001/002；不接管 GCB-004/005 的主体聚合、OC 权利或额度桥实现，也不接管 GCB-006 的紧急保护权限。GCB-003 的普通事项后续执行只消费其现有保护优先、当前重验和单次结果约束，不将本设计视为该要求所有消费者的完整实现。阈值、期限、超多数与独立确认集合由经授权的版本化制度政策提供，不在此新增经济或政治参数。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GCB-001](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)；Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b | 普通政策、财库、既有 charter 日常事项须声明类别、目标、范围与来源；紧急、升级、复合或触及保护的事项不得隐式获得普通权限。 | [分类与来源](#governance-classification) | 产品制度白名单；P2P 授权与主体断言；财库/charter 专业 authority | 不重新定义主体聚合、OC 锁定与紧急授权。 |
| [REQ-WR-GCB-001](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)；Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b | 普通政策、财库、既有 charter 日常事项须声明类别、目标、范围与来源；紧急、升级、复合或触及保护的事项不得隐式获得普通权限。 | [提交边界](#governance-commit) | 产品制度白名单；P2P 授权与主体断言；财库/charter 专业 authority | 不重新定义主体聚合、OC 锁定与紧急授权。 |
| [REQ-WR-GCB-002](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)；Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06 | 可修改的宪制事项须同时具有审议、延迟/复核、适用超多数、独立确认、审计、申诉及连续 receipt；执行前授权失效或任一条件缺失均无部分效果。 | [宪制合取](#constitutional-conjunction) | 现行宪制政策、独立审查角色/受影响主体、权威世界时间 | 不改变保护底线，不把技术 manifest 治理或 BFT finality 当宪制批准。 |
| [REQ-WR-GCB-002](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)；Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06 | 可修改的宪制事项须同时具有审议、延迟/复核、适用超多数、独立确认、审计、申诉及连续 receipt；执行前授权失效或任一条件缺失均无部分效果。 | [提交边界](#governance-commit) | 现行宪制政策、独立审查角色/受影响主体、权威世界时间 | 不改变保护底线，不把技术 manifest 治理或 BFT finality 当宪制批准。 |
| [REQ-WR-GCB-002](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)；Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06 | 可修改的宪制事项须同时具有审议、延迟/复核、适用超多数、独立确认、审计、申诉及连续 receipt；执行前授权失效或任一条件缺失均无部分效果。 | [恢复](#governance-persistence) | 现行宪制政策、独立审查角色/受影响主体、权威世界时间 | 不改变保护底线，不把技术 manifest 治理或 BFT finality 当宪制批准。 |

身份绑定、签名与实际控制关系由 P2P 专业 authority 提供最小化可验证断言；本组件消费其结果，不以 Agent ID、余额或自报身份补齐资格。普通财库拨款和区域操作仍须由相应账本、[charter authority](regional-charter-authority-and-recovery.design.md#charter-authority-interfaces) 在执行点确认其专业条件。

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线事实 | 目标 | 差距与证据边界 |
| --- | --- | --- | --- |
| 普通事项输入 | [OpenGovernanceProposal](../../../crates/oasis7_agent_api/src/world_action/action.rs) 接收自由标题、描述、选项及窗口/阈值；[票局状态](../../../crates/oasis7_world_state/src/gameplay_state.rs) 是 `Open/Passed/Rejected`。 | 声明、效果集合、分类与来源可分别审查。 | 原字段检查和只读票局 quote 不等于分类执行器。 |
| 技术发布治理 | [governance_publication](../../../crates/oasis7_runtime/src/world/governance_publication.rs) 处理技术 manifest 发布。 | 宪制确认属于独立制度记录与授权域。 | 不从发布成功推导受影响主体确认、宪制申诉或规则生效。 |
| 提交与恢复 | [统一事务设计](../design.md#621-explicit-transaction-and-typed-delta-model) 已有 staged delta、journal 与恢复边界，局部实现范围由其正文说明。 | 分类、授权消费、规则变化和唯一结果在同一权威事务可恢复。 | 本专题所需记录、执行器、兼容与组合测试仍须实现，通用机制不代签。 |

## 4. 边界与结构

普通事项和宪制案例使用不同的逻辑记录与验证器。声明者只提供 intent；分类器读取完整效果集合、白名单政策、对象类型及保护索引；专业审查者提供各自范围内的证据；Kernel 保留不可绕过的保护、能力范围和事务校验；制度模块只能提出 typed delta，不能直接修改权威状态、journal 或资产。

所有生效判断遵守[共同权威输入约定](../design.md#runtime-gameplay-authority-designs)：政策、证据、撤销和时间均来自已验证且绑定摘要的 canonical 输入；外部查询只准备证据，不由执行节点的实时网络结果或模型判定影响最终效果。

实现遵守 [SC-32 制度分类](../../product/world-rules-core-gameplay/prd.md#sc-32) 与 [制度迁移边界](../design.md#61-institution-migration-test)：Governance 的 Kernel guardrail 不等于把所有制度流程新增为 native action/state。新制度记录落在经批准的模块实例状态与相应 schema，调用同一权限、预算和事务入口；本设计不改变 Alliance/EconomicContract 的兼容定位，不隐式启用制度迁移试点。

`ordinary`、`treasury`、`existing_charter` 的结果只授予所声明事项的执行机会；`constitutional`、`emergency`、`technical_upgrade` 各有独立 authority。路由只提供目标合同与下一步，不创建新授权、不转移旧票数，也不使缺少执行器的轨道变得可用。

## 5. 关键运行流程

<a id="governance-classification"></a>
### 5.1 事项声明、分类与授权来源

1. 只读预览绑定 `world_id`、事项身份/修订、规范化 payload digest、类别、明确对象与空间/资产/权限范围、请求效果列表、授权来源引用及相关状态版本。标题/描述供人阅读，不能参与权限推断。预览不建立 hold、财库预算占用、角色、资格或规则效果。
2. 声明必须列出所有效果及依赖，包括财库用途、charter 范围、模块/规则变化和间接授予的能力。仅接受当前 schema 能理解的封闭效果类型；不接受不透明脚本或“其他”类别绕过分类。声明中的范围由权威对象解析，不能由客户端自由文本圈定。
3. 分类器首先检查不可削弱的保护：独立资产/既有合同最低保护、身份和确认历史、审计/申诉、finality/signer/custody，以及普通治理不得自行扩权的边界。任一效果会直接或间接削弱这些保护，整项 `rejected_protected`，不能降为普通事项或以宪制待决继续申请该禁止效果。普通事项自行修改白名单、分类规则或扩大自身权限同样拒绝。
4. 不触及禁止项的效果，再与当前白名单逐项匹配：政策运行绑定已声明的政策授权，财库用途绑定有界公共项目与预算 authority，charter 日常事项绑定当前 charter 的动作及作用范围。来源至少可追溯到授权者、授权记录/版本、允许动作、对象/受益范围、有效期、撤销状态及来源 receipt；一份有效签名不是完整来源证明。
5. 改变其他可修改宪制规则的事项进入 `constitutional`。上游明确允许的未来普通事项类别扩展也只能走该独立轨道：当前宪制政策必须具有该精确变更能力，效果保持既有保护，且完整满足下一节全部条件，才可更新类别政策；不能由普通多数、标签改名或本设计自行授予扩展权。紧急介入、技术升级进入各自独立轨道。未支持轨道返回 `unsupported_route` 与准确下一步。复合提案规范化所有子效果并保留依赖；包含非普通子效果时，整个复合方案不得先执行普通部分。只有玩家显式重提且各子项本身独立合法、无隐藏前置效果时才可拆案。
6. 分类结果持久化为声明/审查事实，包含匹配或拒绝条款、缺项、所需 authority 和 `next_action/next_recheck`；该记录不产生经济或权力效果。后续修改任何规范化效果、目标、范围或政策版本都会形成新修订，旧分类、投票、确认不能自动迁移。执行时再次分类，防止白名单、对象或授权变化后仍沿用旧结论。

<a id="constitutional-conjunction"></a>
### 5.2 独立宪制合取闸门

`ConstitutionalCase` 必须引用现行宪制政策版本，锁定本次变更全文/效果摘要、影响说明、受影响主体/区域集合、范围和恢复边界。所需确认集合由政策和权威影响证据确定，不能由提案人删减。政策未规定适用阈值、时间或独立性规则时保持 `needs_policy`，不得挪用普通票局默认值。

以下条件分别存储为带 producer、权限范围、payload digest、世界版本和可追溯 receipt 的事实，不能合并成一个 `approved` 布尔值：

| 条件 | producer 与证明内容 | 缺失、漂移或冲突的结果 |
| --- | --- | --- |
| 公开审议与影响说明 | 制度记录绑定已公开的准确变更、受影响权利/资源和恢复方案，并记录审议窗口。 | 未公开、内容不一致或影响集缺证保持待补；不启动有效延迟窗口。 |
| 延迟与复核 | 权威世界时间记录有效公开/复核起点、最早生效点和截止点；时间区间按政策明确端点。 | 未到最早点为待决；已过截止为过期；修改实质内容后按新修订重算，不能沿用旧等待。 |
| 适用超多数 | 宪制投票 authority 按本案政策、独立资格快照与确定计票提供证明。 | 普通 `passed`、技术 quorum 或仅签名数量不能代替。 |
| 独立确认 | 适用的跨区域或受影响主体分别确认同一 payload；断言证明其资格与同普通决策的独立性。 | 缺一项必需确认、撤回、超范围、重复控制或无法证明独立时阻断；提案者/普通票局不能自行代签。 |
| 独立审计 | 经授权审计主体审查准确变更、权限差异、历史保持和恢复边界；政策规定应与提案/执行方分离的角色不得兼任。 | 审计拒绝或尚未完成不能进入可执行态。 |
| 程序性申诉 | 可使用的受理/查询入口、已通知主体、异议/申诉记录及政策要求的处理结果。 | 阻断生效的未决异议保持 `appeal_hold`；仅有入口链接不能替代实际处理。 |
| 当前授权与连续记录 | 每项来源仍有效，声明、审查、确认、异议、裁决与预期生效身份构成完整链。 | 授权撤销/过期/收缩、链缺口或相互矛盾时待补、过期或原子拒绝。 |

闸门是以上适用条件的合取。界面主状态可以显示 `reviewing/delay_pending/confirmation_pending/audit_pending/appeal_hold/ready`，但执行许可取决于对完整条件集合的当前计算；不能用状态顺序跳过某一项。`ready` 仍不修改规则或保留资产。未阻断生效的异议也须保留理由与处理状态，不能删掉以制造“无异议”。

<a id="governance-commit"></a>
### 5.3 当前重验、唯一效果与申诉恢复

执行请求绑定事项/案例身份、修订、payload digest、预期授权与规则版本、语义操作身份及调用者权限。先验证当前读权限，再查询同一操作的已提交结果：同身份同内容返回原 receipt，即使其授权随后过期也不重做；同身份不同内容拒绝 `identity_conflict`。

没有已提交结果时，在同一 staged view 中重验保护、分类、所有当前授权、上述合取和受影响专业状态。任何缺项阻止整个业务效果集合；财库、charter 或规则侧不能先行提交。相关世界事实改变时返回精确 `stale_scope/stale_authority/stale_effects`，无关对象变化不应被解释成授权变化。

有效提交原子写入规则/限定业务 delta、按授权合同确需消费的额度或使用标记、案例终态、语义去重结果、事件和执行 receipt。可复用授权不得被误删；每个已通过事项/案例修订具有唯一 `ExecutionSlotV1=(world_id,module_instance_id,matter_or_case_id,approved_revision,execution)`，`execution` 由执行器固定，调用者不能更换 operation ID/kind 再执行一次。禁止创建一般“已通过额度”，再由另一路径不带本案限制地消费。

确定未提交的失败丢弃 staged 业务变化；不能持久化拒绝记录时返回基础设施错误，不伪造稳定 receipt。持久化状态不明时围住本次操作并查询其确切提交点，不能换身份再试。提交后响应丢失只返回原结果。申诉在生效前可按政策阻断本案；生效后以独立裁决和明确授权的纠正/补偿动作追加 linked receipt，不改写已确认历史，不取得修改 finality 或独立资产的额外权力。

## 6. 接口与数据合同

下表冻结逻辑 schema `governed-matter/v1`；是实现目标，不声明当前已有 wire/API。`world_id/domain/module_instance_id/local_id` 共同限定所有事项、案例、授权及 receipt 引用；revision/tick 为单调无符号整数，digest 使用相应 canonical codec 的域分离承诺。效果集合和 required confirmation set 按全限定 ID 排序、拒绝重复；字节编码及运行容量沿既有版本化 ABI/配置发布，不允许可选字段默认成授权通过。

| 接口 | producer → consumer | 身份 / 版本与必要内容 | 顺序 / 幂等 | 成功 / 错误及兼容 |
| --- | --- | --- | --- | --- |
| `MatterDeclaration` | 玩家/受权 Agent → 制度模块 | world、matter/revision、规范化效果摘要、类别、对象/范围、来源引用；free text 仅说明 | 同修订内容不可改写；新内容新修订 | 声明已记录或字段/类型/范围拒绝；旧文本提案无自动执行资格。 |
| `AuthorityRef` | 授权 authority → 分类/执行器 | 全限定授权 identity、`source_kind, issuer, subject, grant_id, grant_revision, capability_scope, target_scope, effective_interval, revocation_revision, source_receipt_ref, source_commit_ref` | 以当前权威状态核验；快照不冻结未来有效性 | 有效/缺失/撤销/过期/不明；不将 OC receipt 或技术 manifest 互换成制度权限。 |
| `ClassificationRecord` | 分类器 → 审议/读面 | matter/revision、policy/保护索引版本、完整 effect set、route、理由与缺项 | 同输入确定；记录结果不产生业务效果 | ordinary 子类、独立路由或拒绝；未知 effect 不宽松放行。 |
| `ConstitutionalConditionRecord` | 各独立 producer → 宪制闸门 | case/revision、condition kind、准确 payload、actor/authority、适用范围、时间/撤回及 receipt | 每个确认去重；撤回/裁决为后继记录，原事实保留 | satisfied/pending/expired/rejected/unknown；不同 authority 不共用 approval。 |
| `ExecuteGovernedMatter` | 有当前执行权限的主体 → Kernel/事务 | case 或 matter、revision、operation identity、payload、相关版本与授权 | 单个根事务；同操作同内容复读，冲突拒绝 | 唯一 execution receipt、原子拒绝或结果待核验。 |
| `GovernanceOutcome` | 权威记录 → Viewer/pure API/Agent | classification、条件向量、已确认结果、来源世界版本、缺项、下一步、申诉引用 | 按世界版本读取；重连不补发执行 | 不把 submitted/passed/ready/pending 表达为 effective；公开面最小披露。 |

`MatterDeclaration.category` 的普通值域固定为 `policy_runtime/treasury_use/existing_charter_operations`；其他请求类型为显式 `constitutional/emergency/technical_upgrade/composite`，不能使用 `other`。`ClassificationRecord.route` 为相应普通子类、独立轨道、`needs_evidence` 或 `rejected_protected`；只有 policy 中有独立 authority 的轨道可被标记为可继续。`ConstitutionalConditionRecord.condition_kind` 固定为 `deliberation/delay_review/supermajority/independent_confirmation/audit/appeal/current_authority`；每项带适用 subject/scope、状态与来源，不支持未知条件时阻断该政策版本。

确认记录的语义幂等键为 `(world_id, module_instance_id, matter_or_case_id, revision, condition_kind, confirming_subject)`，撤回/申诉裁决通过各自后继操作记录。实际生效只消费上述唯一 `ExecutionSlotV1`，与客户端请求 ID 或任意 operation 标签无关；同事项实质变更必须新修订并重新审查/批准，不能仅换键续用旧批准。错误种类冻结为 `unsupported_schema/unsupported_route/missing_declaration/missing_authority/rejected_protected/conditions_missing/independence_unverified/appeal_pending/stale_scope/stale_authority/stale_effects/expired/identity_conflict/outcome_unknown`；均返回相关条件/对象、是否已有结果及实际下一步，不把基础设施疑义写成未生效结论。

## 7. 状态、事务与持久化

<a id="governance-persistence"></a>
声明、普通审议、宪制条件、执行结果是四种记录，生命周期不可互推。普通票局 `passed` 最多对应 `execution_eligible`；宪制 `ready` 是动态合取结果。执行终态为 `effective/rejected/expired/cancelled`，缺证据的 `pending` 不是成功。终态的同一操作不因重试变更；新的合法尝试须引用旧结果、使用新修订并重新取得必要确认。

完整效果集合在 [ExecutionTransaction](../design.md#622-disposition-and-idempotency-contract) 的单根提交内完成。规范化 payload、独立确认、规则版本、相关授权、已消费操作标记、receipt、审计索引与后继状态进入同一可恢复 generation；外部通知仅经提交后的 durable outbox，canonical replay 不重新通知、投票、确认或执行外部副作用。

恢复共同校验案例/声明、条件链、授权版本、历史规则版本、时间进度、效果索引与 receipt。缺少条件、版本或去重索引时允许历史诊断读取并阻断相关新执行，不能从 `passed` 或当前规则反造历史确认。未提交准备态可丢弃；已提交效果不可因投影/通知失败回滚。到期基于权威时间谓词即时失效，不依赖后台清理任务。

`recorded` 只证明元数据进入日志；`effective` 需本案业务 execution receipt；`persisted` 需相应 generation/日志恢复合同；跨节点 finality 仍归 P2P。去重、撤销和审计链不能因提案关闭被提前删除；归档后必须仍能回答旧操作是否已生效及其准确摘要。

## 8. 部署、安全与运行约束

新功能沿现有世界提交入口部署，不增加可直接改规则或财库的管理员服务。所有正式写入口、模块命令、tick directive 与恢复路径都要经过同一 guardrail，不能仅在 Viewer 提交时分类。普通治理凭据不得拥有宪制确认、审计或系统安全 capability；部署 operator 也不能用技术角色自动取得制度权限。

分类和独立性输入必须可由验证者重算或验证；LLM 推荐、网页内容与附件只能作为待核实材料。公开审计显示范围、理由、确认状态和结果引用，控制关系证据保持最小私下断言，不公开密钥、账户关联细节或非必要个人数据。证据不可达、未知 schema、落后节点和超时只阻断相关事项，并保留其他合法世界活动。

## 9. 质量与容量

| 场景 | 环境 / 刺激 | 判定指标与阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 分类一致 | 同一 world/policy/effect set，重排输入或包装复合事项 | route/理由/完整效果摘要一致；保护绕过与业务副作用均为零，依据 GCB-001 | [GOV-VERIFY-001](#gov-verify-001) | 目标，未运行。 |
| 宪制合取 | 每次缺一个条件、替换确认主体、撤销或截止并发 | 未满足任一条件时规则/财库/权限变化为零，依据 GCB-002 | [GOV-VERIFY-002](#gov-verify-002) | 目标，未运行。 |
| 单次与故障 | 同语义操作并发、提交前故障、提交后丢响应和重启 | 业务效果计数至多一，结果与恢复 root 一致 | [GOV-VERIFY-003](#gov-verify-003) | 目标，未运行。 |
| 资源有界 | 效果数、声明深度、证据/确认数量超过版本化服务上限 | 在业务写前拒绝；不截断保护检查或确认集合 | [GOV-VERIFY-004](#gov-verify-004) | 上限由 runtime 配置冻结；本文不虚构吞吐/SLA。 |

## 10. 兼容、迁移与回滚

新 schema/分类政策通过受控激活边界启用。旧自由文本提案及旧 `passed` 结果保留原文、投票和 receipt，可供历史阅读；没有完整效果声明、来源和独立条件时不能转成新执行授权。需要继续的新事项由玩家明确重提，绑定旧记录作 provenance，不能自动搬票、确认或资格。旧普通票局的历史重放仍使用其原版本。

消费者先获得只读解析与错误处理，再开放新确认/执行能力；不支持 schema 的节点或客户端明确拒绝对应新路径。历史政策及规范化算法必须随快照/日志可取回，不用当前政策重算旧裁决。回滚只能退回仍可读新记录且能冻结新准入的兼容版本；新规则已生效后不能直接退到忽略这些效果的旧 binary，纠正需新的合法决定与 receipt。

## 11. 验证设计与可追溯性

以下验收场景 ID 供实现时的合同测试与正式入口验收绑定，不表示专用测试文件已存在。所有成对样例使用同一 source/build、world/profile/policy 和授权事实，只改变待验证条件；required/full 层级沿上游原要求与 [测试分层](../../testing/prd.md#testing-sc-1)，正式 Agent 功能交付遵守 [实际自测方法](../../testing/manual/agent-runtime-self-playtest.manual.md)。

<a id="gov-verify-001"></a>
- **GOV-VERIFY-001 / required，分类 producer→记录→读面**：分别提交合法政策/财库/既有 charter、缺来源、受保护资产、finality、普通白名单自扩展、紧急/升级伪装和嵌套复合样例；比较 route、来源和缺项；预览、拒绝、路由后核验资金、hold、资格、角色、规则均未改变。另以有合法宪制扩展能力且保持保护的新增类别作独立路由正例，与普通自扩权及削弱保护的宪制伪装配对；更名、拆分及效果顺序变化不能避开保护。
<a id="gov-verify-002"></a>
- **GOV-VERIFY-002 / required，独立条件逐项负例**：同一合法宪制变更分别缺审议、延迟、复核、超多数、一个必需独立确认、审计或申诉处理；普通 passed/技术 finality/同控制人重复签名均不能补齐。完整正例在当前授权下得到一份生效 receipt；执行前撤销、范围漂移、过期仍零部分效果。
<a id="gov-verify-003"></a>
- **GOV-VERIFY-003 / required + full，事务及恢复**：多个请求 ID 指向同一操作、相同 ID 不同 payload、授权到期/撤销与执行竞争、提交前各写点失败、提交后响应丢失、恢复后重试及 canonical replay；逐项核对原/后规则、账本、权限、条件链、去重状态与 receipt。full 加跨节点、快照与日志后缀恢复，未知提交状态不能生成第二次执行。
<a id="gov-verify-004"></a>
- **GOV-VERIFY-004 / required，读权限、容量与下一步**：在 Viewer、pure API、Agent 对同一结果核对 pending/ready/effective、缺项、恢复/申诉与来源版本；私下身份断言不泄露。输入、条件集合与证据超限时不能截断成合法项；落后读面和网络超时不自动重提。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GCB-001](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)；Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b | [分类](#governance-classification) | 合法事项可进入正确流程；越界/缺来源及复合旁路零业务效果。 | [系统测试手册](../../../testing-manual.md)；按本文场景 GOV-VERIFY-001、003、004；同候选 required，full 覆盖恢复/跨节点；专用合同回归由实现提供，本文仅规定验收。 | 声明/分类记录、前后资源与权限、唯一 receipt、正式读面；实际 PR 保存版本和结果 | 文档检查不证明分类实现、政治平衡或发布。 |
| [REQ-WR-GCB-001](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)；Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b | [提交](#governance-commit) | 合法事项可进入正确流程；越界/缺来源及复合旁路零业务效果。 | [系统测试手册](../../../testing-manual.md)；按本文场景 GOV-VERIFY-001、003、004；同候选 required，full 覆盖恢复/跨节点；专用合同回归由实现提供，本文仅规定验收。 | 声明/分类记录、前后资源与权限、唯一 receipt、正式读面；实际 PR 保存版本和结果 | 文档检查不证明分类实现、政治平衡或发布。 |
| [REQ-WR-GCB-002](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)；Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06 | [合取](#constitutional-conjunction) | 任一条件缺失/过期/失效不产生规则、资产、权限或历史改写；独立确认不可代签。 | [系统测试手册](../../../testing-manual.md)；按本文场景 GOV-VERIFY-002、003、004；固定独立角色、政策、时间与世界版本的正负样例；专用合同回归由实现提供，本文仅规定验收。 | 各条件原始证明、申诉链、原子提交/拒绝与恢复结果；实际 PR 证据 | 不证明未提供的政策、身份机制或线上自定义模块。 |
| [REQ-WR-GCB-002](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)；Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06 | [提交](#governance-commit) | 任一条件缺失/过期/失效不产生规则、资产、权限或历史改写；独立确认不可代签。 | [系统测试手册](../../../testing-manual.md)；按本文场景 GOV-VERIFY-002、003、004；固定独立角色、政策、时间与世界版本的正负样例；专用合同回归由实现提供，本文仅规定验收。 | 各条件原始证明、申诉链、原子提交/拒绝与恢复结果；实际 PR 证据 | 不证明未提供的政策、身份机制或线上自定义模块。 |
| [REQ-WR-GCB-002](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-002)；Trace refs: O7-REQ-79b6687e-3b8c-42a3-a266-df4d191fec06 | [恢复](#governance-persistence) | 任一条件缺失/过期/失效不产生规则、资产、权限或历史改写；独立确认不可代签。 | [系统测试手册](../../../testing-manual.md)；按本文场景 GOV-VERIFY-002、003、004；固定独立角色、政策、时间与世界版本的正负样例；专用合同回归由实现提供，本文仅规定验收。 | 各条件原始证明、申诉链、原子提交/拒绝与恢复结果；实际 PR 证据 | 不证明未提供的政策、身份机制或线上自定义模块。 |

## 12. 决策、长期风险与未决问题

采用显式声明和独立条件记录，避免给自由文本票局增加可被误用的万能 `approved`。代价是执行需要跨 authority 读取精确版本；以相关对象 read-set 和单根事务限制并发范围，不能通过缓存授权绕过重验。

现行宪制政策的阈值、延迟、受影响集合判定及角色独立性规则由 `producer_system_designer` / `blockchain_ops_engineer` 在受控启用前冻结；当前没有合法政策输入时该轨道保持不可执行。若今后修改保护分类、schema、独立性或规则生效点，必须重审对应负例、旧案例兼容及已确认历史，不能以本设计存在作为解除依据。
