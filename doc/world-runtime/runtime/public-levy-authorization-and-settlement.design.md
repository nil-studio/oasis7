# 公共 levy 授权、预算账本与单次结算系统设计

- 状态：`active`（目标系统合同；不表示功能已经实现或验收）
- Owner role：`runtime_engineer`；共同审读：`producer_system_designer`、`gameplay_designer`、`blockchain_ops_engineer`、`qa_engineer`
- 适用范围：不可排他公共品 levy 的授权、征收、公开账目、支出边界、失效和恢复；可排他服务费继续使用原合同。
- 审读基线：`e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`；实现跟踪：[公共 levy 实现范围 #4861](https://github.com/nil-studio/oasis7/issues/4861)。

## 1. 问题、目标与非目标

当前物流服务有真实的报价、托管和费用分配，但一次服务收费不能证明公共征收具备 charter 预授权、目的与受益范围、预算上限、期限和公开复核。目标是把这些条件变成同一权威提交点的合取判定，并使每笔来源、支出、剩余资金和失效处置可以对账。

成功意味着合法征收只产生一次可核对结果；缺失、失效、越界或未知授权不会产生扣费、欠费、资格改变或通行限制；网络恢复只能核对原请求。本文不选择费率、预算数值、征收周期、纳费主体算法或新税制，也不把 levy 变成一般世界资格、加入组织或独立成长的前置条件。

## 2. 上游约束与相关角色

产品拥有公共品适用性和筹资边界；gameplay 拥有收益、机会成本、退出替代及申诉语义；charter authority 拥有区域能力与其退化；runtime 拥有身份、时间、账本、原子提交和恢复；P2P 继续拥有权威顺序与 finality。征费主体、用途、费率计算及支出政策必须是这些权限已批准的输入，不能由调用者或模型建议补齐。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-GR-008](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-008)；Trace refs: O7-REQ-17536c7e-c59a-464a-90e8-c36f0330c8f7 | 仅合法公共品在 charter、用途、受益范围、上限、期限、账目与复核全部成立时征收。 | [授权与提交](#levy-authority) | [CharterAuthorityRef](regional-charter-authority-and-recovery.design.md#charter-authority-interfaces)；gameplay 公共融资政策 | 不重新定义 charter、费率或一般税收；服务费保持原路径。 |
| [AC-WR-GR-008](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-008)；Trace refs: O7-REQ-17536c7e-c59a-464a-90e8-c36f0330c8f7 | 过期、无账目、无审计和越界请求原子拒绝；重复与重连不会再次扣款。 | [原子性与单次义务](#levy-transaction) | runtime/QA 同候选资源及 receipt 对账 | 普通投票通过、报价或服务费 receipt 不代签 levy。 |
| [REQ-WR-MW-003](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-003)；Trace refs: O7-REQ-7e2cd84b-9145-4e2f-b3c2-f9c451cec6c5 | 展示费用类型和用途、期限、退出替代；失效筹资不能变成独立成长的隐性租金。 | [费用与恢复读面](#levy-read-model) | [公共融资玩法验收](../../game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md#ac-game-017-05)；Viewer/Agent | 不扩展成熟世界认可、tenure、基本通行或治理资格。 |
| [AC-WR-MW-003](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-003)；Trace refs: O7-REQ-7e2cd84b-9145-4e2f-b3c2-f9c451cec6c5 | 缺项、失效和撤销不自动续费、累计欠费或限制正常恢复。 | [失效与关闭恢复](#levy-expiry-recovery) | 当前 charter 状态、资金权威和合法恢复入口 | 文档不证明玩家界面或跨节点恢复已经完成。 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线与证据 | 目标及差距 |
| --- | --- | --- |
| 可排他物流服务费 | [物流报价](../../../crates/oasis7_runtime/src/world/logistics.rs)与[转运状态转换](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs)已有费用和托管/完成分配；旧 receipt 的 `governance_tax=0` 保留原意。 | 继续独立运行，不把旧服务收费重标为 levy。 |
| 治理与公共征收 | [治理事件准备](../../../crates/oasis7_world_state/src/world/governance_meta_publication.rs)承接普通治理事实；不是本文的 levy 权限、预算及复核合取执行器。 | 新增本专题的领域记录和提交消费者，不能只增加展示字段。 |
| 账目与恢复 | [持久化入口](../../../crates/oasis7_runtime/src/world/persistence.rs)提供现有恢复基础；没有据此取得 levy 专用账本和单次结算的证据。 | 接入同一持久化边界并完成专用恢复与对账验证。 |

上述仅为固定版本静态审读。目标模型和验证计划不改变产品的当前实现结论，不推断未提供的外部 WASM 或线上配置。

## 4. 边界与结构

`CharterAuthorityRef` 只证明指定 charter revision 的能力范围与当前有效性；charter 处于有效状态本身不授权征费。引用必须保留其 `world_id + module_instance_id + charter_id` 全限定身份和相应 revision，不能跨治理实例按同名 charter 解析。独立的 `LevyAuthorityV1` 消费该引用并拥有公共品、用途、受益/适用主体、上限、时界、账目及复核规则。两个 authority 必须同时有效。

评估器只读当前权威状态。结算器只能写筹资账本与匹配资金账户，不拥有身份、通行或治理资格的写权限。支出消费者只消费合法资金和独立的用途/预算授权，不能因账户已有余额就获得任意支出权。Viewer、Agent、WASM 提案和运营工具不能直接改余额、应付义务或结算结果。

## 5. 关键运行流程

<a id="levy-authority"></a>
### 5.1 授权与提交

1. 注册筹资提案时固定 `levy_id`、政策 revision、资金资源/单位、公共品和用途、受益范围、合法征收依据、授权主体以及 charter 引用。提案、报价和审查不扣款、不建立欠费、不预留通行或资格。
2. 只有 charter 明确允许该类公共融资且专项授权满足全部产品条件时，记录 `LevyAuthorityV1`。其授权同时绑定征收和支出上限、费率/计算规则、开始/到期边界、公开账目、复核证据及失效后未用资金处置。没有所需政策、授权或可验证计算规则时拒绝启用，不填默认税率或零风险。
3. 只读评估根据当前主体/受益范围和合法征收依据产生 `LevyAssessmentV1`，列明待扣资源、金额、预算影响、授权版本与过期点。组织成员、历史贡献、到访、持有资产或专业化本身不能生成征收依据。
4. 提交携带原评估、请求身份和内容摘要；runtime 从同一当前世界状态重验 charter capability、专项权限、目的/范围、合法主体、计算规则、费率和累计预算、资金余额、期限、账目可核验性及复核条件。相关 revision 漂移、依据冲突或金额超出已披露边界时原子拒绝并要求重新评估，不静默降额、补签或换 charter。
5. 所有条件成立时，按[事务合同](#levy-transaction)同时记录付款方扣减、专项账户入账、累计征收额度使用和唯一 settlement receipt。请求被接收、授权有效或评估成功均不是结算。

授权使用权威世界时间的 `[effective_from, expires_at)` 区间。到期判定直接发生在提交点，不能依赖后台清理或客户端时钟。公开账目和复核须有已进入 canonical 状态、可验证的版本/状态证明；仅存在 URL、说明文字或一次旧审计并不满足当前条件。

授权适用性同时受[预算和公开账本](#levy-ledger)、[失效恢复](#levy-expiry-recovery)及[费用读面](#levy-read-model)约束：它们是征收成立的组成条件，不能只核对签名与时间后略过。

<a id="levy-ledger"></a>
### 5.2 预算、支出和公开账本

账本按 `world_id + levy_id + resource_kind + budget_line` 隔离，保存已确认来源、支出、退回、资金占用及其 receipt。不同资源分别对账，不以一种资源盈余抵销另一种短缺。金额使用资源域既有精确单位；计算溢出、负数、未知舍入或不支持的费率规则在扣款前拒绝。

每个资源的余额满足“期初资金 + 已结算流入 − 已结算支出 − 已结算退回 = 当前实际余额”；可用额再减去尚未终止的合法支出占用。未完成支出从预留转为支出时同一原子更新只计一次。征收上限、支出预算和账户实际余额分别检查：有余额不等于有支出授权，有预算也不代表已筹到资金。

支出请求必须绑定已批准用途、受益范围、预算行、收款方/交付义务及来源账户。支付、保留、释放和最终结算遵循所消费领域的原合同；不把“计划支出”算作付款，不把物流到达当作最终用途结算。征收授权不自动批准新补贴、额外补偿或范围外转账。没有适用支出消费者时允许公开显示有界余额及阻塞原因，不对外声称项目已产生效果。

公开读面可从相同账本重建总来源、支出、剩余、占用与失效处置，逐笔关联必要的目的、授权和 receipt。公开信息仅包含核对所需内容；完整付款身份、原始资格证据按原读取权限保护，公开摘要保留可验证的明细承诺，不能只提供无法对账的累计数字。

<a id="levy-expiry-recovery"></a>
### 5.3 失效、关闭与恢复

到期、撤销、charter capability 收缩、账目不可核验或复核不通过，立即禁止新的征收和续费；未提交评估只失效，不形成欠费。续期或政策更新必须形成新的明确授权 revision 和新评估，不能给旧请求换上新 charter 或延长旧授权。

已经完成的征收保持历史。尚未使用资金按启用时绑定的关闭政策逐笔处置；允许的释放、退款或履约必须有相应当前权限和唯一处置身份。合法未结承诺按原合同保存，不因授权到期删除资金、货物或责任。关闭政策缺失、当前处置权限不足或状态未知时，专项资金保持可审计冻结/待核对，不自动发给运营者、不凭空退款，也不继续征费补窟窿。此冻结只约束该笔专项资金。

响应丢失先以原请求和结算身份查询；已提交返回原 receipt，确证未提交才返回无效果，无法核验保持 `unknown`。恢复读取原 charter/levy revisions、权威时间、账本、未结承诺和去重记录；不读取当前配置重算旧金额。重启不恢复失效权限，缺失结算原件也不能当作“尚未收费”。

<a id="levy-read-model"></a>
### 5.4 可读结果

所有入口区分 `service_fee` 与 `public_levy`，显示用途、受益范围、依据、成本、预算/费率上限、期限、账目与复核、真实退出/替代和下一步。筹资状态分别为“提案/待审”“已授权未结算”“有 receipt 的已结算”“失效/撤销”；结算观察完整性另列新鲜、陈旧或未知，不能把未知结果显示成未扣款。

普通服务付费、独立行动、基本通行和恢复沿各自规则继续。levy 拒绝不能写入通行拒绝、组织准入失败、治理失权或隐藏债务。下一步只能指向已支持的补证、申诉、常态付费、独立恢复或重新规划入口；没有合法操作时说明缺口，不自动重提。

## 6. 接口与数据合同

下列是必须保留的逻辑数据关系；`V1` 是目标模型版本，不宣称已有 wire 类型。具体编码可复用现有命令/事件封套，不能删去这些身份、谓词或资源边界。

`levy_id`、专项授权和征收依据均在 `world_id + authority_module_instance_id` 的完整命名空间中分配；后续简称指这份全限定身份。治理实例重建或 charter 换版不能让旧义务在另一个同名实例再次结算，跨实例交接必须显式保留原义务与来源索引。

| 对象/接口 | producer → consumer | 身份与必需内容 | 顺序、错误与兼容 |
| --- | --- | --- | --- |
| `LevyAuthorityV1` | 合法治理/筹资 authority → runtime | `world_id, levy_id, revision`；[charter 引用](regional-charter-authority-and-recovery.design.md#charter-authority-interfaces)、授权来源、公共品/目的、受益及征收范围、政策摘要、资源/预算/费率上限、有效期、账目/复核引用、关闭政策 | 不可变 revision；撤销独立事件；缺项/未知政策/越界拒绝；不能从旧服务合同升级。 |
| `LevyAssessmentV1` | 权威只读评估器 → 提交者 | 原请求、授权 revision、征收依据身份、被征收主体、适用期间、金额与资源、预算/账户 revision、快照、内容摘要 | 只读且可过期；相同评估不产生 reservation；漂移返回重评。 |
| `LevySettlementV1` | runtime → journal/receipt/查询 | 请求身份与摘要、唯一征收义务键、原评估/授权、付款/收款 ledger entries、实际金额、权威提交引用 | 同内容重投返回原件；同键不同内容冲突；超时未知。 |
| `LevyBudgetEntryV1` | 资金/支出 authority → 公开及授权读面 | 专项账户/预算行、来源或支出/退回/占用类别、金额、前后余额、配对分录、授权与领域 receipt | 分录只追加；修正通过新的合法对冲记录，不重写历史；守恒或关联失败阻断写入。 |
| `LevyClosureV1` | 到期/撤销及资金处置 authority → runtime | 原授权、关闭原因、未用/未结资金集合、各项处置、政策及 receipt | 每项按原资金身份至多处置一次；部分未决保留；不以关闭标记代替退款。 |

错误至少区分 `authority_unavailable`、`expired_or_revoked`、`scope_mismatch`、`assessment_stale`、`budget_exceeded`、`funds_unavailable`、`audit_unverifiable`、`identity_conflict`、`version_unsupported` 与 `outcome_unknown`。请求/证据/分录大小、批量数量及查询分页由版本合同声明上限；超限在写入前拒绝，不截断收费集合。传输重试必须使用同身份和同内容，读取仍检查当前读取权限。

## 7. 状态、事务与持久化

授权逻辑状态为 `proposed -> authorized -> expired/revoked`，未到开始时间的 authorized 尚不可执行；拒绝提案单独终态，续期不复活旧 revision。请求状态为 `evaluated -> submitted -> settled/rejected/expired`；提交尚无权威结果时保持 pending，网络 `unknown` 属观察状态。支出、关闭与征收有独立结果，不能用一个 `done` 覆盖未结资金。

<a id="levy-transaction"></a>
### 7.1 原子性、单次义务与保留

权威提交序中的一次条件事务共同检查相关 authority/账户/预算 revision，并写付款扣减、专项资金入账、预算使用、结算身份和 receipt。准备失败或日志提交失败时全部不应用。若底层现有封套不能承载这组联合变化，应先扩展其原子准备/提交能力；禁止把多次可独立成功的写入当作完成的征收。

幂等分两层：传输键是原 request identity + digest；业务键是 `world_id + levy_id + authorized_assessment_source + liable_subject + assessment_period`。业务键由权威征收依据颁发，不由客户端随机生成；变更 request ID、政策 revision 或 charter 不能再次结算同一义务。确有不同周期/新公共项目时必须有新的合法依据，不能仅改日期或标签。没有可验证依据身份就拒绝，不能从成员列表自动生成持续欠费。

同业务键同内容返回原 settlement；内容冲突拒绝。多个合法义务竞争同一剩余预算时按权威序列化提交并共享累计额，不能分别预验后透支。退款/纠错引用原 settlement 与独立处置键，既不删除已收事实，也不清空原征收去重键。

事务拒绝和成功必须映射到[费用读面](#levy-read-model)的真实筹资阶段、用途和退出替代；失效后的资金和恢复按[关闭协议](#levy-expiry-recovery)处理，不能用一条错误码掩盖部分资金变化。

snapshot 和 journal 在同一一致性边界保存授权/撤销、评估引用、账本、占用、关闭项、结算和去重索引。replay 应用原提交，不重新征收或重新调用外部付款方。保留时间受[存储治理](runtime-storage-footprint-governance.prd.md)和合法重放/争议窗口约束；到期不能删除去重事实。无法恢复预算或原件时只隔离该 levy，不能清空账本后重新开始。

## 8. 部署、安全与运行约束

通过现有 runtime 权威提交入口执行，不引入管理员扣费旁路。治理签名验证来源，专项权限及当前适用性另验；普通多数同意或 charter 有效均不能绕过目的/预算/时界。客户端、LLM 与滞后节点没有可直接行使的征收权。

遵循[共同权威规则](../design.md#runtime-gameplay-authority-designs)：新审计/复核证据须先由有权来源按正式路径提交内容摘要、版本、适用区间与状态；执行 guard 只读取当前 canonical 事实和请求中绑定的完整证据。同一历史的征费结果不能取决于节点即时 HTTP、模型判断或本地缓存；网络取证失败只影响是否能提交新证据及观察完整性，不能自行改写已提交的审计状态。canonical 账目/复核状态确实缺失、过期或不可验证时阻断相关征收。

一个 levy 的错误不阻断无关市场、服务费、基本行动或资金账户。真实跨节点、恢复操作和外部资金桥接不由本设计的本地事务假设代签；外部资源系统未提供匹配提交合同前不可启用该资源的 levy。

## 9. 质量与容量

| 场景 | 环境 / 输入或刺激 | 预期响应 / 判定指标 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 授权合取 | 固定候选 deterministic runtime fixture，逐个去除授权/范围/预算/时界/账目/复核 | 每个失败样例资金、欠费、资格和通行变化均为零 | [LEVY-VERIFY-001](#levy-verify-001) | 目标验证，未运行。 |
| 预算竞争与重复 | 同一世界并发请求、不同 request ID 复用义务、最后一份预算竞争 | 每个业务义务一个 settlement，累计不超上限，分录守恒 | [LEVY-VERIFY-002](#levy-verify-002) | 目标验证，未运行。 |
| 失效与恢复 | 到期边界、撤销竞态、响应丢失、snapshot+journal 恢复 | 新征收为零；原资金和 receipt 可对账；未知不重扣 | [LEVY-VERIFY-003](#levy-verify-003) | 目标验证，未运行。 |
| 输入规模 | 版本规定的最大分录/证据量及超限输入 | 有界处理或无效果拒绝；不截断义务/账目 | [LEVY-VERIFY-002](#levy-verify-002) | runtime/QA 实现前确定限额，无吞吐或延迟承诺。 |

## 10. 兼容、迁移与回滚

旧快照没有 levy 状态表示该能力未启用，不产生默认征收。旧物流费和 `governance_tax=0` 维持历史；不从普通治理记录、组织关系或服务费账单迁移出授权。新格式必须显式声明能力/版本，未知必需字段或新账本版本的旧消费者拒绝执行相关写入，不可忽略字段后继续收款。

启用前完成 charter/专项授权接口、联合事务、恢复和消费者支持，再开放新的 levy 请求。禁用只停止新征收，不能删去已收资金、未结支出或关闭义务。回退目标必须能无损读取已写的授权、分录和去重记录；没有兼容基线时停止 levy 写入并修复读取，不能退到旧格式丢弃余额。不可逆支出只能由新的合法纠错/补偿处置，不做快照回退退款。

## 11. 验证设计与可追溯性

<a id="levy-verify-001"></a>
**LEVY-VERIFY-001（授权与无效果）**：固定实际 candidate、world binding、资源初值和有效 charter，逐个覆盖正常授权以及缺 charter capability、错误用途/受益主体、缺征收依据、费率/预算越界、开始前/到期点、撤销、无账目或复核证据。以真实提交消费者比较全部资金账本、债务、资格与通行前后值；报价调用前后世界摘要相同。服务费正例独立保留，不能只测试错误返回码。

<a id="levy-verify-002"></a>
**LEVY-VERIFY-002（账本与幂等）**：从 [治理事务回归](../../../crates/oasis7_runtime/src/tests/governance_meta_publication_transaction_regressions.rs)和真实资金域测试扩展目标 fixture，验证准备失败、提交失败、预算竞争、同义务不同 request、同键异内容、分录溢出、重复退款和超限集合。成功时比对 source/sink 配对、receipt 与预算累计；失败无部分写。新增专用测试应由实现 PR 纳入原 required 层，本文不宣称已有测试覆盖 levy。

<a id="levy-verify-003"></a>
**LEVY-VERIFY-003（失效与恢复）**：在原请求提交前、提交后响应前、退款/支出阶段分别故障注入；从 [恢复回归](../../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs)扩展同候选 snapshot+journal 恢复，核对到期/撤销与提交排序、新授权不能复活旧义务、未用资金和未结承诺保持。API、Viewer 与 Agent 各读取同原件，验证四类筹资状态、未知观察、下一步和基本独立路径；沿 GR-008 的 full 与 MW-003 的原适用层级验证，真实 surface/跨节点证据单独提供。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-GR-008](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-008)；Trace refs: O7-REQ-17536c7e-c59a-464a-90e8-c36f0330c8f7 | [授权与提交](#levy-authority) | 授权合取、公开账目及合法失效处置。 | `LEVY-VERIFY-001`、`002`、`003`；固定 candidate 的 required 域回归及原 full 组合。；按[测试手册](../../../testing-manual.md)扩展本节定义的真实提交/恢复场景。 | 实施 PR 的状态差分、分录/receipt、实际 CI 和授权读面。 | 本文无运行证据，不证明具体税率/预算政策。 |
| [AC-WR-GR-008](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-008)；Trace refs: O7-REQ-17536c7e-c59a-464a-90e8-c36f0330c8f7 | [原子性与单次义务](#levy-transaction) | 无效无效果、有效一次、可区分费用与替代。 | `LEVY-VERIFY-001`、`002`、`003`；拒绝/并发/重连及费用对照。；按[测试手册](../../../testing-manual.md)扩展本节定义的真实提交/恢复场景。 | 同候选原件与全量有关账户对账，真实消费者观察。 | 单元回归不代签正式玩家入口或跨节点。 |
| [REQ-WR-MW-003](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-003)；Trace refs: O7-REQ-7e2cd84b-9145-4e2f-b3c2-f9c451cec6c5 | [费用与恢复读面](#levy-read-model) | 筹资可读且不成为独立、通行或恢复门槛。 | `LEVY-VERIFY-001`、`003`；原 MW-003 required 范围与适用 surface。；按[测试手册](../../../testing-manual.md)扩展本节定义的真实提交/恢复场景。 | 实施 PR 的失败无效果、独立路线和状态读面证据。 | 不证明其余成熟世界成长/认可要求。 |
| [AC-WR-MW-003](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-003)；Trace refs: O7-REQ-7e2cd84b-9145-4e2f-b3c2-f9c451cec6c5 | [失效与关闭恢复](#levy-expiry-recovery) | 缺项、失效和撤销不续费、欠费或改变资格。 | `LEVY-VERIFY-001`、`003`；固定授权版本、边界时刻与恢复记录。；按[测试手册](../../../testing-manual.md)扩展本节定义的真实提交/恢复场景。 | 实施 PR 的原请求结果、资金和资格差分。 | 文档检查不证明实现或发布。 |

## 12. 决策、长期风险与未决问题

| 决策或边界 | 理由、代价及 owner | 复核触发 |
| --- | --- | --- |
| charter 与专项 levy 合取，不复用服务收费身份 | 避免区域成立即有税权、旧服务收入被重标；增加明确授权及对账成本。runtime / 治理 owner。 | charter 能力模型或资金域发生变化时复核接口。 |
| 业务义务和传输请求分别去重 | 防改 request/revision 重扣；要求权威征收依据与保留索引。runtime。 | 具体合法征收依据不能稳定识别时，相关政策禁止启用。 |
| 费率、预算、主体评估及关闭政策作为已批准版本输入 | 本设计给出执行/拒绝协议，不替产品制定数值。producer / gameplay。 | 启用具体 levy 前提供完整政策和正常/失效样例；缺项维持拒绝。 |
| 资金保留与不可逆支出不靠回滚修复 | 保全财产和真实历史；可能出现有界专项待核对。runtime / 资金 owner。 | 每种支出或退款域接入前完成原子/恢复与权限评审。 |

上述接口、状态及验证设计只补系统层；实现和正式启用仍须取得各自实际证据。权限、资金及持久化实现变更依[开发流程规范](../../engineering/workflow/source-of-truth.md)接受独立评审。
