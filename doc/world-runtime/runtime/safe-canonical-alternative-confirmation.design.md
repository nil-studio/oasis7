# 安全 canonical 替代、明确确认与唯一结果系统设计

- 状态：`active`（目标协议；不表示候选、确认和执行链已实现）
- Owner role：`runtime_engineer`；共同 owner：`agent_engineer`、`viewer_engineer`、`gameplay_designer`、`qa_engineer`
- 适用范围：不支持的细粒度原请求，转换为当前受支持、可比较并经明确确认的安全 canonical 替代；涵盖正式 canonical/legacy chat 与第一方 native SDK 的适用生产入口。
- 审读基线：`e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`；实现跟踪：[安全替代实现范围 #4867](https://github.com/nil-studio/oasis7/issues/4867)。

## 1. 问题、目标与非目标

现有 chat 的 owner 签名和 native SDK 的工具授权能保护原请求，但不能表达“比较一个新承诺后，玩家明确同意这个具体替代”。目标是将原请求、安全候选、独立确认、范围授权消费和最终结果连成可审计协议；没有安全替代就明确停止，未知原结果先恢复原请求。

本文冻结逻辑身份、确认语义、状态机、失败、幂等及持久化合同，不新增任意动作或替玩家选择不可逆后果。已在既有授权范围内的普通工具、只读 quote、高自治行为和原 operation 恢复保持原合同；材料/配方替代与转换由其工业专题承接，不在这里实现 conversion、Provider Programme 或全部历史间接控制功能。

## 2. 上游约束与相关角色

[SC-5](../../product/world-rules-core-gameplay/prd.md#sc-5)拥有完整产品范围与 required/full 条件。runtime 拥有原请求、权威检查与结果；Agent/provider 只能提出建议；Viewer/API 取得有权主体的独立确认并显示真实阶段；canonical action 与领域服务继续拥有执行语义和 settlement。

本协议复用 [DES-WR-IA-001](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-001) 的请求/效果身份、[DES-WR-IA-010](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010) 的当前授权与累计额度、[DES-WR-IA-011](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) 的权限变化；它新增本适用场景必需的候选集、明确确认和一次性授权消费，不能反向声称邻域合同已完成这三项。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [canonical 替代比较](../../product/world-rules-core-gameplay/prd.md#root-sc5-altcompare)；Trace refs: O7-REQ-d76b49a6-01a7-46db-968b-23bfd35fc295 | 不支持原细粒度请求时比较安全单/多候选的目标、范围、成本、风险、后果、可撤回性和理由；无安全候选停止。 | [原请求与候选](#alternative-candidates) | Agent 候选建议、runtime 受支持能力和领域只读校验、Viewer/API | 不新增动作、不静默改写原目标、不用 Wait 假装替代能力。 |
| [替代的明确授权与无效果预览](../../product/world-rules-core-gameplay/prd.md#root-sc5-altauth)；Trace refs: O7-REQ-7326a105-57ef-4cf4-823c-2755c539671b | 确认前无效果；新增承诺或不可逆后果取得精确范围授权。 | [明确确认协议](#alternative-confirmation) | 当前玩家/控制权、原 domain authority、各入口独立确认 producer | 普通 chat owner 签名、旧 grant、模型建议和 test-only proposed 不代签。 |
| [替代失效与单次结果](../../product/world-rules-core-gameplay/prd.md#root-sc5-altrepeat)；Trace refs: O7-REQ-ea9640b0-fb39-4fde-9ec1-69e25c448ba5 | 拒绝、过期、权限收缩/替换、并发确认、重连、重试不静默执行或重复，原请求到唯一结果可审计。 | [唯一执行与结果协议](#alternative-consumption) | canonical request/intent/domain receipt；当前权限与持久化；跨入口 QA | 不重做普通原 operation 恢复、不宣称未提供的外部 adapter 已兼容。 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线与证据 | 目标及缺口 |
| --- | --- | --- |
| 正式 chat | [canonical owner admission](../../../crates/oasis7_runtime/src/service_admission/agent_chat.rs)校验 owner、world/branch、nonce 和 replacement；[intent 写入](../../../crates/oasis7_runtime/src/world/agent_intent.rs)记录生命周期。 | 必须增加“原请求不支持 → 候选 → 另一次明确确认”的真实 producer/consumer，连续记录 proposed/submitted/accepted 不能替代确认。 |
| Native SDK | [工具选择](../../../crates/oasis7_agent_harness/src/native_task_tools.rs)按既有授权使用工具并保留只读 quote；[SDK recover](../../../crates/oasis7_agent_harness/src/native_task_sdk.rs)先核对原 operation。 | 受本合同约束的新替代不能直接当作旧授权工具执行；需绑定候选确认及唯一执行。 |
| 建议与结果 | [provider advisory](../../../crates/oasis7_runtime/src/world/agent_intent_terminal.rs)的 proposed producer 仍为测试路径；普通 receipt/恢复机制已有自身范围。 | 目标需要生产可用的候选与独立确认，不把 fixture 或普通幂等当作本专题完成。 |

以上为固定版本静态事实，不是业务测试或正式玩家验收。本文不改写现有正向能力，也不推断未知外部部署。

## 4. 边界与结构

原请求登记和候选评估形成 control metadata，不修改资源、执行队列资格、经济额度或世界玩法状态。候选建议可以由模型产生，但授权和安全结论来自当前受支持的 canonical 行为及权威校验；候选生成失败不触发备用世界动作。

候选 generator、确认 producer、权威消费者和结果 reader 必须可分别定位。拥有生成候选的能力不意味着能确认；拥有签名 chat 的能力不意味着已经确认替代；受支持工具也不能跳过该工具所需的当前 domain authorization。所有写入口消费同一原请求 lineage 和服务器端状态，不能由不同 surface 各维护一次性状态。

<a id="alternative-model"></a>
### 4.1 逻辑持久模型

| 目标模型 | 必需字段与关系 |
| --- | --- |
| `AlternativeRequestV1` | `alternative_request_id`、原 canonical request/intent/operation identity 与 payload digest、authenticated actor、目标 Agent、world/branch/reorg/identity generation、原请求适用能力判定与无效果证明、替代 lineage、可选 predecessor lineage 与其无效果终态引用、当前 generation/state。原文按原权限保存。 |
| `AlternativeSetV1` | `set_id, generation, content_digest`、原请求引用、候选列表、同一评估快照/相关 config/capability/authority revisions、有效期、候选生成来源、被替换 set 引用；一经发布不改内容。 |
| `AlternativeCandidateV1` | `candidate_id, candidate_digest`、规范 action/goal 及精确参数、目标/对象/作用范围、资源来源/成本及上限、风险和不可逆后果、预计结果、可撤回性/最早有效点、推荐理由、必要权限、评估依据与未决 blocker。 |
| `AlternativeConfirmationV1` | `confirmation_id`、有权 confirmer 与当前 authority 来源/revision、独立认证 nonce、原请求/set/generation/candidate/digest、明确允许的 action/target/object/resource/budget/期限/风险范围、确认或拒绝选择、权威提交引用。 |
| `AlternativeAuthorizationV1` | `authorization_id`、精确 confirmation 引用、原 lineage、授权范围摘要、有效期、状态 `available/bound/consumed/expired/revoked`、唯一绑定 `execution_id`、消费结果引用；不能独立转让或用于另一候选。 |
| `AlternativeOutcomeV1` | 原请求/set/candidate/confirmation/authorization/execution 的完整链、实际 canonical intent/effect/domain receipt 引用、拒绝/待决/应用/终结范围、最终结果及观察新鲜度。唯一终态只追加，不按“最新聊天”推断归属。 |

显示文案与执行参数共同绑定候选内容摘要，特别是成本、风险、后果和可撤回性；不能确认一版说明后执行另一版参数。客户端显示名、内存 index、模型 tool-call ID 或 Trace ID 不作业务主键。具体编码复用现有版本封套，以上字段关系和状态语义不得因 wire 映射而丢失。

## 5. 关键运行流程

<a id="alternative-candidates"></a>
### 5.1 原请求判定与候选发布

1. 关联原请求的世界、主体、目标 Agent 和不可变内容。只有确认原细粒度请求不受支持且没有未核明的执行承诺/效果，才进入候选生成；原 operation 已提交或结果未知时先恢复原件，不与替代并行发第二个请求。
2. 候选 generator 从同一授权可读快照提出零个、一个或多个当前已支持的 canonical 候选。权威只读校验核对动作支持、资源/对象范围、基本合法性和安全边界；不支持或安全性未知的项不可选择，可作为明确 blocked 解释保留。
3. 发布不可变候选集，逐项比较目标/范围、成本、风险、预计后果、可撤回性与推荐理由。必要字段或安全依据未知时标明缺项，不能把 unknown 当作零成本、无风险或可撤回。推荐仅是建议，不自动选中或执行。
4. 没有安全候选返回 `no_safe_alternative` 并说明真实限制；生成器/依赖失败返回 `evaluation_unavailable`。两者都停止执行，但不把系统故障包装成“已证明没有任何合法动作”。用户之后明确要求重新评估时生成新的 set generation 并保留旧集，不因 reconnect 自动生成新候选。

首次候选没有新的 canonical effect/root；预览不得创建资源 hold、扣预算、排产、执行顺位或其他世界效果。可记录审计元数据，其保存本身不能被解释为玩家已接受新目标。

<a id="alternative-confirmation"></a>
### 5.2 明确确认与范围授权

确认必须是候选发布之后的独立、有身份的用户命令；它明确指向原请求、当前 set generation、一个 candidate 及完整内容摘要，并覆盖被展示的新承诺/不可逆风险。普通原 chat 签名、聊天自动回显、同一次函数调用生成的 proposed/submitted、LLM 输出“已确认”或默认推荐值不成立。

确认者必须在提交时具有对该目标与范围的当前控制/授权。范围只能来自其现有合法权限，不能因确认额外获得资源、组织或治理权。runtime 重验 confirmer、候选有效性、成本/风险范围和必要 authority；拒绝、过期、旧 generation、已替换目标、权限收缩或不支持的风险类型不形成执行授权。

接受确认的同一事务锁定该 lineage 的选定候选，并创建唯一 `AlternativeAuthorizationV1`；此时仍不产生玩法效果。两个并发确认选择不同候选时，只有第一个有效提交可锁定选择，另一项明确返回冲突和原选择。对同候选的不同确认 ID 也不生成第二份授权；相同 confirmation identity 且同内容返回原结果，异内容冲突。若需要改选，只有尚未绑定 execution 的 lineage 才可通过显式取消旧授权、新 generation 和新确认处理；已绑定者只能恢复原执行，确知无效果后的新尝试遵循[新请求边界](#alternative-consumption)，不能覆盖原绑定。

拒绝整组替代记录原请求下的拒绝，不自动选其他候选。候选有效期和确认授权期限使用权威世界时间 `[from, until)`，并且授权不能比被确认候选或基础 authority 的期限更长。尚未开始或已失效的权限都不能被确认补签。

确认只产生本次范围授权，实际执行必须继续经过[唯一执行与授权消费](#alternative-consumption)；候选生成与[模型身份](#alternative-model)也不能被同一入口合并成隐含确认。

<a id="alternative-consumption"></a>
### 5.3 一次性授权消费与唯一执行

1. 执行请求携带原 lineage、set/candidate digest、confirmation、authorization、canonical 参数和稳定 request digest。runtime 先核对已绑定/已完成结果，再在当前世界状态重验所有相关权限、候选范围和执行前置条件。无关快照变化不强制重复确认；目标、对象、成本上限、风险、后果、撤回边界或必要 profile/authority 的相关变化需要显式重评/新确认，不静默换候选。
2. 在 canonical admission 的同一事务中，将 authorization 从 `available` 条件更新为 `bound(execution_id)`，锁定整个替代 lineage 的唯一执行，并写入原 canonical intent/operation 及因果映射。binding 只把授权独占给原 execution，不算资源扣减或最终结果；不能先发送动作，稍后才标记授权被用。
3. 在第一次受授权的 world-effect 提交点，联合检查当前 authority、授权与 execution 绑定、相关领域条件及累计预算，原子提交世界效果/领域 receipt、实际预算使用和 authorization `consumed`。失败不能留下单边扣账、已生效但无身份的效果或被另一个 execution 复用的授权。
4. 若现有 canonical 行为本来有后续步骤，它们仍属于同一个 execution，每个后续效果都同时受原 confirmed candidate 的动作、目标/对象、作用范围、累计费用上限、风险/后果边界及适用时界约束，并重验当前 domain grant 和领域预算，逐步保留 receipt。确认范围是持续有效的执行上界，较宽的 domain grant 不能在 `consumed` 后重新扩权；累计预算不能按子步骤或重试归零。`consumed` 表示不能创建第二条替代执行链，不代表余下合法步骤可以绕过权限，也不要求把真实多阶段业务伪装成一条物理 receipt。任何边界失效都停止新的不合规效果；必要收尾若超出原确认范围，必须由独立且当前合法的领域处置权限明确覆盖动作、范围、成本和后果，并链接原 execution/未结义务，不能从旧确认推断例外。
5. 最终 `AlternativeOutcomeV1` 引用该 execution 的唯一权威终态与实际 domain receipts。accepted/queued、部分应用、领域完成、最终目标结果分别表达；不能用“授权已消费”宣布整个目标成功。原细粒度请求的“不支持/无效果”结果继续保留，显示实际完成的是哪一项被确认的替代。

若既有 admission 自身会取得游戏资源 hold、有限排队资格或其他承诺，这一 admission 就是首个 world-effect 边界，必须在同一事务完成 execution 绑定、授权消费和相应预算占用，不能先产生承诺再等后续物理生产才消费。预算的 reservation 与实际消耗仍按原领域账本区分、转换和释放，二者不能重复计量。

执行在首个效果前被权威拒绝、取消或到期时，写明无效果终态，原授权终结且不能自动回到 available。凡已经绑定 execution，原 lineage 的绑定和终态终身保留，不因无效果而重新分配。只有原 execution 已被权威证明没有效果且没有未结承诺，用户才能明确提交新的原请求，形成引用旧无效果终态的 predecessor-linked 新 lineage，再重新评估和确认；这不是恢复旧请求。原请求及其所有精确重试始终返回原 execution/终态。已发生效果、部分结果或未知结果只能恢复同 execution，不能通过新 confirmation/request ID 绕过唯一执行限制。

所有并发、失效和响应丢失使用[失败恢复协议](#alternative-recovery)与[状态条件提交](#alternative-state)，结果经[同源读面](#alternative-read)关联原请求。只实现一次 binding 而缺失恢复/读面，不构成本协议的单次结果保证。

<a id="alternative-recovery"></a>
### 5.4 失效、并发、停止与恢复

| 事件 | 权威处理 | 用户可见下一步 |
| --- | --- | --- |
| 旧候选迟到、被替换或过期 | 保留旧集和关联，拒绝旧确认/新执行；不得覆盖当前集。 | 查看当前集或明确重新评估。 |
| 权限撤销/收缩、owner/Agent generation 或世界 binding 变化 | 在后续效果前按当前 authority 重验；不能移用旧确认，新 owner 不继承旧授权；已发生结果保留。 | 核对原结果，在有权范围内取消/补授权/重新规划。 |
| 确认后关闭窗口、断连或重复点击 | 不自动撤回也不自动执行；以服务器持久状态判定已确认、已绑定或已完成。 | 重读原请求；停止/取消仍是独立权威命令。 |
| 取消与执行并发 | 在同一权威序列条件提交；取消先有效则不能新产生效果，效果先发生则保留结果并按原可撤回边界处理。 | 明确显示取消范围，不能把已发生效果显示成未执行。 |
| admission/执行响应丢失 | 原 lineage、authorization 与 execution 不释放或另分配；查询/精确重试原身份。 | 保留 pending/unknown；不能自动换 ID、换候选、重新调用模型执行。 |
| 已知拒绝且无效果 | 记录原终态及原因，旧授权不可重复用；已绑定 execution 的原 lineage 不再分配。 | 未绑定者可明确更新 generation；已绑定者需用户显式新原请求与 predecessor-linked 新 lineage。 |
| snapshot、日志或原 receipt 不完整 | 不重造确认或成功；隔离受影响执行，恢复可验证原件。 | `outcome_unknown` / 恢复受阻及真实限制。 |

授权撤销不删除确认历史，历史确认也不维持当前执行资格。服务器端拒绝/过期的终态与观察端“看不到结果”严格区分。拒绝一个确认不改变其他无关原请求的执行并发。

## 6. 接口与数据合同

| 接口 | producer → consumer | 身份、输出及顺序 | 错误、超时和兼容 |
| --- | --- | --- | --- |
| `propose_safe_alternatives` | 合法原请求入口/Agent 建议 → runtime 只读评估及控制记录 | 原 request/digest/world binding；输出不可变 set/candidate 与评估依据；不执行 | 原结果未知先 recover；不支持/无安全候选/评估不可用分别表达。 |
| `confirm_safe_alternative` | 当前有权主体 → 独立确认 consumer | 原请求、set generation、candidate digest、明确范围、独立认证 nonce；确认及 authorization 同时持久化 | 旧集、摘要不同、权限不足、期限无效或并发选项冲突拒绝。 |
| `execute_confirmed_alternative` | 已确认选择的执行适配器 → 原 canonical admission | authorization + 稳定 execution/request digest；事务内 claim 唯一执行，首效果原子消费 | 重试同身份同内容重读；不支持一次性绑定的入口返回 unsupported，不降为直接 chat/tool。 |
| `cancel_safe_alternative` | 当前有权主体 → 权威控制消费者 | 原 lineage、当前 authorization/execution revision、取消范围 | 与效果序列化竞争；已发生/未知效果不假取消；不撤销已消耗资源。 |
| `read_safe_alternative` | canonical state/receipt → 当前有权读取者 | 原请求到候选/确认/授权/结果链及新鲜度 | 只读；断连重读不创建新集或执行；失去权限按原策略脱敏。 |

错误集合至少为 `original_outcome_unknown`、`original_not_eligible`、`no_safe_alternative`、`evaluation_unavailable`、`candidate_stale`、`confirmation_required`、`confirmation_rejected`、`authority_changed`、`authorization_expired_or_revoked`、`execution_already_bound`、`identity_conflict`、`version_unsupported`、`outcome_unknown`。错误响应关联原请求与可合法读取的原因，不能附带未授权的新操作。

候选数、参数大小、证据/解释长度及评估预算由版本化协议声明上限。超限或超时不能截去成本/风险/权限条件后交给用户确认，须返回有界失败或重新生成明确的新集。不同入口的 DTO 可以不同，但必须包含同一可确认摘要和足够的安全信息；旧客户端不支持此协议时保持 unavailable，不伪造确认。

<a id="alternative-read"></a>
结果读面分别显示原请求限制、候选比较、选定项、确认范围、授权状态、canonical 执行和实际结果，并列出 blocker/next/recovery。一个入口确认后，其他入口重读同一选项/状态；迟到的旧结果只回填自己的原 lineage，不能更新“最新目标”或重复完成提示。原始目标、授权材料和其他玩家信息按当前 owner/读取权限保护；同一结果公开可读不代表其私密原因公开。

## 7. 状态、事务与持久化

<a id="alternative-state"></a>
### 7.1 状态与唯一性

替代请求的逻辑状态为 `evaluating -> awaiting_confirmation -> confirmed -> execution_bound -> executing -> terminal`，另有 `no_safe_alternative`、`evaluation_unavailable`、`declined`、`expired`、`superseded` 和无效果 `rejected`。每种终态绑定自己的原因与范围；有实际已发生效果的执行不能转成无效果 rejected。观察 `unknown` 独立于这些权威状态。

一个原 lineage 同时只能有一个当前 set generation、一个已选候选和一个有效 confirmation/authorization；在其整个生命周期最多绑定一个 execution，绑定后即使无效果终结也不复用槽位。set generation 更新仅限尚未绑定 execution 的 lineage，旧集与旧确认只追加保留。身份不同但重复选择/执行不因此获得新槽位。候选替换、确认、绑定、取消和效果提交均使用该 lineage 与当前 record revision 的条件更新，由权威序列决定竞争结果。

三处事务不能拆成无保护的跨写：确认 + 创建 authorization；authorization bound + canonical admission + 完整映射；实际 world effect + domain receipt + 授权消费/预算使用 + 对应结果。真实多阶段执行使用原 execution 的子效果身份而非分配新的替代 authorization。无法在原边界联合提交时先补领域封套，不能凭客户端布尔值或异步补日志宣称实现了唯一消费。

### 7.2 持久化和重放

snapshot/journal 共同保留原 request/digest、eligibility/no-effect 证明、候选内容与 generation、确认签名/范围、authority revisions、授权状态、唯一执行索引、原 intent/effect 及 receipt。候选 control metadata 与 gameplay state 分开解释，但必须同样可恢复，不能仅存浏览器 local storage。

恢复按原身份先查询 canonical admission/结果，再判断尚未产生效果的授权是否仍可用；不能从新 snapshot 自动创建原候选/确认。replay 使用已记录的候选与确认，不重跑模型生成新选择，不重签消息，不消费第二次授权。retention 至少覆盖原 intent 的重放、恢复和争议窗口，参照[存储治理](runtime-storage-footprint-governance.prd.md)保留必要原件和去重事实；原始私密内容可按既有策略脱敏，但不能丢去重后继续接受旧请求。

## 8. 部署、安全与运行约束

所有执行入口通过相同 runtime 规则；没有该 consumer 的旧路径不得直接执行来自安全替代流程的新承诺。第一方 native SDK 仍遵守原 context 最多一个 physical side effect 的限制，适配器不能用这个协议扩大工具集或预算。普通已授权行动继续按原合同，不要求整套系统每个动作都走本专题确认。

遵循[共同权威规则](../design.md#runtime-gameplay-authority-designs)：模型候选是非权威建议，必须先规范化并固定内容/证据；决定合法性、确认、授权消费和执行的 guard 只读取同一 canonical 输入与当前已提交的权限/政策事实。节点即时模型响应、HTTP 可达性、客户端时间或本地缓存不能让同一历史请求得到不同经济结果。模型输出不确定不影响已固定候选的确定性重放。

候选文本按不可信数据显示，不能执行内嵌指令或把隐藏字段当作用户确认。签名域要区分原 chat、确认、取消和执行，覆盖完整语义摘要；相同 nonce 在不同消息类型中不能被错误复用为同一批准。world/branch/reorg/identity generation 与 owner fence 在授权和执行阶段分别核对，不跨世界迁移旧确认。

## 9. 质量与容量

| 场景 | 环境 / 输入或刺激 | 预期响应 / 判定指标 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 比较与只读 | 固定 canonical 能力、单/多/零安全候选、缺成本/风险/权限 | 全部比较字段可读；确认前 sink/hold/世界顺位变化为零 | [ALT-VERIFY-001](#alt-verify-001) | 目标 required，未运行。 |
| 明确确认 | 不同签名语义、旧集/摘要篡改、权限漂移、并发确认 | 只有独立有效确认产生一个授权；未确认执行为零 | [ALT-VERIFY-002](#alt-verify-002) | 目标 required 与原 full 跨入口组合。 |
| 恢复与唯一结果 | admission/效果提交前后丢响应、重连、并发、取消、restore/replay | 原 lineage 最多一执行链、每个原子效果一次；未知不重建新请求 | [ALT-VERIFY-003](#alt-verify-003) | 目标 required/full，未运行。 |
| 有界评估 | 协议上限内/外候选与解释，模型/证据超时 | 明确失败或有界结果，不截去授权信息；失败无世界动作 | [ALT-VERIFY-001](#alt-verify-001) | 数值上限由实际协议版本声明，无延迟/吞吐承诺。 |

## 10. 兼容、迁移与回滚

不从旧 proposed/submitted/accepted 或 chat owner proof 推导明确替代确认；旧记录保持原语义。新协议必须有显式能力/版本，在 canonical chat、legacy chat 和 native SDK 分别完成生产接入后才能为该入口宣称支持。未接入入口显示不可用或返回安全停止，不采用直接 goal/tool 的隐式回退。

升级保留原 request/world identity、已提交效果和权限 fencing。旧 snapshot 缺少替代状态表示尚未启用该能力；新 snapshot 已含 authorization/绑定执行时，旧二进制不能忽略它后执行。禁用只禁止新候选/确认/绑定，读取与原执行合法收尾仍需保留。回退基线必须识别已写状态，不能复用旧 grant 或清空唯一执行索引；已产生结果通过原领域合法恢复/纠错，不靠退版本撤销。

## 11. 验证设计与可追溯性

<a id="alt-verify-001"></a>
**ALT-VERIFY-001（候选与无效果）**：固定实际 candidate、world binding、能力/授权和原不支持请求，分别提供单个、多个、零安全候选以及成本/风险/authority unknown 样例。覆盖比较全部维度、无安全候选停止、评估失败、超限和旧响应迟到。预览前后对照真实资源、hold、队列资格与玩法状态摘要均不变，原请求/候选审计元数据的预期新增单独核对，不要求保存候选后整个 canonical state root 不变；每个入口分别定位真实候选 producer/reader，不使用 test-only provider advisory 代替。

<a id="alt-verify-002"></a>
**ALT-VERIFY-002（确认及消费）**：在 [canonical admission 测试](../../../crates/oasis7_runtime/src/service_admission/agent_chat_tests.rs)、[intent 生命周期测试](../../../crates/oasis7_runtime/src/tests/agent_intent_v2/lifecycle.rs)与 native SDK 的实际消费边界扩展专用测试。分别验证普通 chat 签名、同次 proposed/accepted、模型“同意”、错误摘要、旧 generation 均不能确认；当前有效独立确认才创建授权。并发同/不同候选、拒绝、到期、权限收缩/替换与 owner 变化后消费，断言每 lineage 一授权绑定、一 canonical 执行且当前预算守恒。多阶段任务在首效果前后分别发生权限收缩或确认时界到期；后者保留 consumed 与原 receipt，较宽 domain grant 不能允许后继 sink 超出已确认范围、累计费用、风险/后果或时界，原范围内合法步骤继续逐步提交，另有当前受权收尾时仍链接原 execution/义务。

<a id="alt-verify-003"></a>
**ALT-VERIFY-003（原件及跨入口恢复）**：沿 [receipt reconciliation](../../../crates/oasis7_runtime/src/tests/agent_intent_v2/receipt_reconciliation.rs)、[持久化恢复](../../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs)和 [SDK 原 operation 恢复](../../../crates/oasis7_agent_harness/src/native_task_sdk.rs)的生产边界注入确认提交、admission 绑定、首效果和最终结果各阶段响应丢失。恢复、重试、并发取消和 replay 后核对完整原请求→候选→确认→授权→execution→唯一结果链及真实资源变化；原请求未知不得另起替代。已绑定后无效果终结再提交旧请求或新 generation，必须仍返回原结果或冲突；用户明确新请求才产生 predecessor-linked 新 lineage，旧 receipt 不改。required 覆盖比较、确认前无效果、拒绝/过期/权限/重试；full 按 SC-5 覆盖跨入口并发确认、权限收缩/替换与唯一 receipt 关联，固定各入口构建和授权环境，正式 Viewer/API/Agent 分别留证。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [canonical 替代比较](../../product/world-rules-core-gameplay/prd.md#root-sc5-altcompare)；Trace refs: O7-REQ-d76b49a6-01a7-46db-968b-23bfd35fc295 | [原请求与候选](#alternative-candidates) | 单/多候选全部比较维度、安全未知和无安全候选停止。 | `ALT-VERIFY-001`；原 required 固定候选与各入口真实 producer/reader。；按[测试手册](../../../testing-manual.md)扩展本节定义的真实提交/恢复场景。 | 实施 PR 的只读前后状态、比较数据和真实 surface captures。 | 文档和模型 fixture 不证明生产接入或真人理解。 |
| [替代的明确授权与无效果预览](../../product/world-rules-core-gameplay/prd.md#root-sc5-altauth)；Trace refs: O7-REQ-7326a105-57ef-4cf4-823c-2755c539671b | [明确确认协议](#alternative-confirmation) | 精确范围独立确认，首效果前当前授权及单次消费。 | `ALT-VERIFY-001`、`002`；required 正负例及原 full 权限收缩/替换。；按[测试手册](../../../testing-manual.md)扩展本节定义的真实提交/恢复场景。 | 实施 PR 的真实确认消息、授权/执行绑定、原子前后差分。 | 普通 owner 签名、quote、test-only proposed 不代签。 |
| [替代失效与单次结果](../../product/world-rules-core-gameplay/prd.md#root-sc5-altrepeat)；Trace refs: O7-REQ-ea9640b0-fb39-4fde-9ec1-69e25c448ba5 | [唯一执行与结果协议](#alternative-consumption) | 重连/并发/拒绝/失效无重复或隐式执行；完整原请求因果可读。 | `ALT-VERIFY-002`、`003`；required 恢复与 full 跨入口关联，固定实际 candidate/runtime/authority。；按[测试手册](../../../testing-manual.md)扩展本节定义的真实提交/恢复场景。 | 实施 PR 的唯一执行/receipt、资源及持久化对账和各入口观察。 | 本文没有运行证据；不证明未知外部 adapter、全部间接控制或发布。 |

## 12. 决策、长期风险与未决问题

| 决策或边界 | 理由、代价及 owner | 复核触发 |
| --- | --- | --- |
| 原请求、候选、确认和执行身份分别保存 | 避免签原目标等于批准另一目标；增加控制记录和跨入口映射。runtime / Agent / Viewer。 | 任一入口更换 DTO、身份或权限模型时核对整链。 |
| 确认、独占绑定、首效果消费分阶段且各自原子 | 支持等待及恢复并防先执行后记授权；不能只依靠客户端幂等。runtime。 | canonical admission 或 domain 事务改造时复验故障边界。 |
| 原结果未知先恢复，不生成新替代执行 | 避免双重承诺；恢复不可用时会明确阻塞。runtime / harness。 | 原 operation 查询/retention 改变时复核；缺原件前不允许放宽。 |
| 候选构造可迭代，合法性与已发布摘要必须确定 | 保留模型建议价值且不让模型成为 authority；无可验证候选时停止。gameplay / Agent / runtime。 | 扩展可支持 canonical 行为或风险分类时先提供真实能力及验收样例。 |

本设计已经确定逻辑协议，具体 wire 编码、资源限额和接入 API 版本需在实现中保持这些合同并验证兼容；未完成入口不得宣称已支持。权限和持久化实现变更按[开发流程规范](../../engineering/workflow/source-of-truth.md)接受独立评审，文档补齐不替代实际验证。
