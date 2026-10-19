# 额外 Agent 工业制造、交付与恢复

- 生命周期：`active`；含义为设计合同，未实现/未运行验收。
- Owner role：`runtime_engineer`；共同审读：`gameplay_designer`、`agent_engineer`、`qa_engineer`。
- 适用范围：同一权威世界中额外 Agent 的报价、订单、生产、交付、身份/claim 取得与失败恢复。
- 静态实现基线：`e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`；核对日期：2026-10-19；未执行 Runtime、模型、交易或玩家验收。
- 产品声明：[REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；专业政策：[制造首版政策](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#agent-manufacturing-v1)。

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928

## 1. 问题、目标与非目标

当前材料配方可以消耗资源、占用工厂并产出物料；现有 ClaimAgent 对一个已经存在的 Agent 建立经济 claim。两条能力之间没有正式制造订单，无法保证“这一单投入只交付这一个 Agent”，更不能以先注册后认领两个独立提交满足原子交付。

目标是让一张有限订单连接报价、真实投入、产能占用、生产记录和唯一交付，且断线、重复提交、权限变化或恢复均不会复制资源、身份、控制权。订单确认只证明资源被预留；制造投入从开工起消耗；只有最终权威交付同时建立身份、经济 claim 和可控制对象。未交付不开始新 Agent 的 upkeep。

本设计采用专业政策首版：自营/现行权限已足够的装配设施、标准型号、单单产一个、工厂原地交付。它不实现首次注册、主绑定迁移、首聊资助、第三方撮合/代扣、跨世界物流、Agent 经济转让或新的组织权限模型；这些能力缺失时保持 blocker。它也不将普通治理 quota 转为供给权限。

## 2. 上游约束与相关角色

产品拥有单主 Agent 路线、扩张选择和可读结果；gameplay 拥有唯一配方、费用、容量、时间和损失政策；runtime 拥有原子写入、事件顺序和恢复；Agent/身份专业拥有主体解析、现行授权与自治边界；Viewer/pure API/Agent 是各自获准范围内的消费者。

### 2.1 需求承接与分配表

每行均消费同一要求的真实子义务，不另分配 Trace ID。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 在确认前冻结真实配方、成本、产能、交付与损失政策，报价无效果 | [DES-AGENT-MFG-001](agent-industrial-manufacturing.design.md#des-agent-mfg-001) | gameplay；[唯一首版政策](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#agent-manufacturing-v1) | 不把新配方写成已在目录上线 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 订单/生产资源与容量在所有竞争入口保持互斥 | [DES-AGENT-MFG-002](agent-industrial-manufacturing.design.md#des-agent-mfg-002) | runtime 工业、账本、设施、取得容量 | 不新增独立资源余额或只适用于制造的容量旁路 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 等待、合法生产进度、取消和失败各有确定结算及截止 | [DES-AGENT-MFG-003](agent-industrial-manufacturing.design.md#des-agent-mfg-003) | runtime 工业、账本、设施、时间 | 不新增通用工业退料或远程交付政策 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 完整交付才创建一次身份、claim 及当前合法控制结果，维护继续 | [DES-AGENT-MFG-004](agent-industrial-manufacturing.design.md#des-agent-mfg-004) | runtime / identity / Agent；既有 claim 经济合同 | 不授予新自治目标、长期委托或首个支持 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 重试、跨请求重放、权限变化不重复交付，不以 quota 绕过 | [DES-AGENT-MFG-005](agent-industrial-manufacturing.design.md#des-agent-mfg-005) | authenticated entry / current grants / world event ordering | 不从客户端身份字符串推导权限 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 已投入、未交付、已交付事实经持久化及旧数据恢复保持唯一连续 | [DES-AGENT-MFG-006](agent-industrial-manufacturing.design.md#des-agent-mfg-006) | runtime / world storage / version capability | 不将 snapshot import 变成新的制造 producer |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 玩家可读阶段、资源/风险与真实下一步，保持单主 Agent 路线及适用入口同义 | [DES-AGENT-MFG-007](agent-industrial-manufacturing.design.md#des-agent-mfg-007) | Viewer / pure API / Agent / QA | 未支持入口 unavailable，不由 UI 存在推导实现 |
| [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 交付后维护、claim 终态和控制来源保持同一身份与责任连续性 | [DES-AGENT-MFG-004](agent-industrial-manufacturing.design.md#des-agent-mfg-004) | runtime / identity / Agent；现有 claim 生命周期 | 不建立 AOS-005 经济转让合同或复活旧 grant |
| [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 后续控制仍依赖当前权限，quota、旧请求与来源历史不直接供给 Agent | [DES-AGENT-MFG-005](agent-industrial-manufacturing.design.md#des-agent-mfg-005) | current grants / world event ordering / QA | 不赋予额外世界权力或迁移旧待决执行 |

## 3. 当前状态、目标状态与差距

下列路径对应上述固定提交的静态源码，不是当前新制造能力的运行证据。

| 对象/能力 | 当前状态（基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| 标准工业目录 | 十一个配方产出材料；Assembler MK1 有两个 recipe slots | 采用独立新配方及一次 Agent terminal 交付 | 不能将 drone/材料产出改名为 Agent | [recipe catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)、[factory catalog](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs) |
| 工业启动与完成 | 开工验证 builder、材料、电力/容量；完成写材料 ledger 和工业 receipt | 在订单下预留后消耗，完成生成不可转让的待交付记录 | 当前 RecipeCompleted 不创建 Agent；普通材料 receipt 不能兑付 | [recipe lifecycle](../../../crates/oasis7_runtime/src/state/industry_transition/recipe_lifecycle.rs) |
| 身份与 claim | AgentRegistered 创建 AgentCell；claim 要求目标已存在、未认领 | 一个 prepared publication 安装制造身份、claim 和完整经济结果 | 不复用两次独立 append 伪装事务 | [core policy transition](../../../crates/oasis7_runtime/src/state/core_policy_transition.rs)、[claim publication](../../../crates/oasis7_runtime/src/world/agent_claim_economic_publication.rs) |
| owner/control 来源 | owner control、grant source 和正式 owner key 解析仍依赖目标 `starter_oc_claims`，本地 host 还检查其玩家绑定 | 增加有制造/有效 claim 依据的非主绑定来源，并适配正式鉴权读写及 host | 只有 AgentCell + 经济 claim 不能让无 starter 记录的新 Agent 接受合法指令 | [owner admission](../../../crates/oasis7_runtime/src/world/owner_control_admission.rs)、[control replay/bind](../../../crates/oasis7_runtime/src/world/agent_delegation_controls.rs)、[grant source](../../../crates/oasis7_runtime/src/world/agent_delegation.rs)、[正式 owner key](../../../crates/oasis7_runtime/src/service_admission/agent_authority.rs)、[host adapter](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/control_plane/agent_memory_correction.rs) |
| 费用/容量 | 现有 claim quote 按已有经济 claim 数计算；主注册可能没有经济 claim | 制造用主绑定/claim 去重规模加在途预留，至少按额外槽收费 | 必须适配报价和所有冲突资源/取得写入口，不能误走 slot-1 资助 | [claim rules](../../../crates/oasis7_runtime/src/agent_claims.rs)、[claim owner quote](../../../crates/oasis7_runtime/src/world/agent_claims.rs) |
| 原子状态发布 | 现有 prepared delta 将适用状态、journal/event 与路由合并准备 | 增加制造准备器及历史/恢复支持 | 稀疏状态安装是复用基础，尚无制造 delta | [publication](../../../crates/oasis7_runtime/src/world/event_processing/publication.rs) |
| 外部入口 | 既有 chain claim submit 提交现存目标，响应有 preview/action id | 显式制造能力、固定订单状态及恢复读面 | 现有响应不能证明制造交付；不得复用裸 submit 当新授权入口 | [claim API](../../../crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/agent_claim_api.rs) |

## 4. 边界与结构

| 责任面 | 产生/拥有的事实 | 允许读写与禁止旁路 |
| --- | --- | --- |
| 玩法政策 resolver | 已启用的配方、body 模板和 claim 条款 revision/hash | 只从适用权威配置解析；请求不得提供任意配方计划、消耗量或初始 Agent 资产 |
| 制造订单 authority | quote、order、资源/容量 reservation、WIP、唯一交付/处置 | 唯一状态生产者；确认、启动、完成、交付分别有事件，全部走正式 publication |
| 工业/经济 authority | 物料、电力、占用、原始资金来源、claim、bond/sink | 在同一事务中验证余额扣除 reservation 后可用；不直接编辑 ledger 补偿已提交结果 |
| 现有身份/授权 authority | authenticated principal 到 buyer owner、builder、beneficiary 的真实绑定 | 订单保存关系快照用于归因，每次效果提交仍查当前资格；不能令 player_id、claim_owner_id、agent_id 相等来省略解析 |
| Service/Node | 受理结果与已提交世界状态的可信读面 | 传入授权上下文及预期版本；网络接受不等于应用或持久化；不改共识/正式世界闸门 |
| Viewer/API/Agent | 决策预览、订单读面、受限历史 | 只读解释与显式请求；不本地生成 Agent、退款、进度或成功 receipt |

<a id="des-agent-mfg-001"></a>
### DES-AGENT-MFG-001：冻结报价与完整承诺

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928；声明：[REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)。

报价绑定 `world/chain/genesis`、authenticated buyer principal、解析后的 claim owner、builder/factory、beneficiary、固定制造配方/body/claim terms、input ledger、工厂位置 authority、取得槽位与相关授权/账本/设施 revisions。它包含专业政策全部成本、来源、时间窗口、最大不可逆损失、交付范围及 scope digest；`quote_id + quote_digest` 定位精确内容。请求只选择受支持身份和策略，不上传自定义材料表或 Agent 初始资源。

quote 不写资产预留或创造库存，可以由可信状态产生可验证的有界报价载荷；确认提交须重新解析其内容并验证同一适用事实。以 `now < expires_at` 判断有效，缺失或冲突事实返回重新报价，不能取新值静默补齐。无关世界事件或 tick 前进本身不改变价格/授权 revision，只有窗口超时或相关依赖失配使报价失效。

政策正常更新仅影响新报价，新单确认固定完整条款。已确认订单可按旧经济条款履行，但安全撤销、设施退役、无效模板、当前取得限制始终能阻止后续效果；政策停用必须声明“停止新单，允许旧单履行”或“禁止旧单继续生产/交付”的准确范围，缺失 disposition 时保持 blocked。不得以冻结价格为由冻结权限，也不得在恢复时补收新价差。

## 5. 关键运行流程

<a id="des-agent-mfg-002"></a>
### DES-AGENT-MFG-002：确认、预留与开工

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928；声明：[REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；验收：[AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)。

1. 当前获准主体选择报价。确认在同一事务内重验身份/权限、现行政策准入、去重经营规模和 cap、材料齐套/已到达、制造电力、liquid claim 金额、空闲 recipe slot 和成品保管位。任一不足只返回具体 blocker，订单及资源均不部分预留。
2. 成功确认分配世界内 `order_id`，持久化 quote 摘要与幂等索引，同时预留材料、电力、claim upfront、取得容量、一个 recipe slot 和一个保管位。预留单仍无 Agent、无 claim、无新 upkeep、无输入 sink；玩家明确可取消。没有空闲槽位时只提供无占用的等待/重新报价，不建立无限候补队列。
3. 系统在正式 world tick 尝试启动：验证原单、当前操作/生产权限、相关 recipe/factory scope、原预留仍存在。开工事务把材料和制造电力的预留一次变成消耗，建立 WIP、`started_at`、`completed_work_ticks=0` 和初始预计 `ready_at`，保留 claim upfront/取得容量及工厂槽位。不再次从可用余额扣相同资源。开工后的每个合法生产 tick 最多提交一次进度，累计达到专业政策的 12 ticks 才能完成。
4. 确认后未能在专业政策期限内合法启动，只能取消一次并原路释放尚未消耗的全部预留。取消与启动并发按同一 canonical event order 决定唯一胜方；启动已提交后普通取消请求返回 `production_already_started` 及已发生的损失边界。

预留必须进入对应物料、电力、token 和取得容量 authority 的“可用量”判定。通用 Transfer、ScheduleRecipe、ClaimAgent、设施回收及受限处置不能忽略它；只在新制造 handler 检查余额会造成双花。取得容量的守卫在订单终结后仍持续：所有新增取得入口都核对 prospective `primary-bound Agent ∪ claimed targets ∪ reserved manufacturing targets` 集合，已在集合内的目标去重，制造交付只将预留目标替换为实际目标，不额外释放名额。存量超过新口径不自动回收，但不能继续扩大集合。调用方拥有预留不能转移、消耗或释放别人的预留；终态解除由订单 authority 按原来源执行。制造消耗不新增第二个通用 hardware 扣费，设施原有维护计费继续独立运行。

<a id="des-agent-mfg-003"></a>
### DES-AGENT-MFG-003：生产、等待、取消与损失

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928；声明：[REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；验收：[AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)。

| canonical order 状态 | 进入条件与事实 | 后续动作和期限 | 不允许的效果 |
| --- | --- | --- | --- |
| `reserved` | 确认事务已提交；资源/容量有唯一预留 | 系统合法开工；owner 可取消；启动期限到一次取消 | 无 Agent/claim、无制造 sink、无 upkeep |
| `producing` | 开工及输入 sink 已提交，存在一个 WIP | 每个合法生产 tick 推进一次；累计 12 ticks 后完成；遇权限/设施/政策阻断转受阻 | 不退已经消费的材料/电力，不以墙钟或预计时间替代工作进度 |
| `production_blocked` | 保留原 WIP、首次阻断原因/时间和资源去向 | 在专业政策有界恢复窗内重评同单；永久不可恢复/期限到报废 | 不刷新启动或阻断期限，不切工厂/配方延用进度 |
| `awaiting_delivery` | 完成事务生成唯一待交付 lot，释放生产 slot，保留成品位和 claim/取得预留 | 系统尝试交付；资格/授权恢复后原单续交；窗口到报废 | 生产完成不是可控 Agent，不发新 upkeep、不公开认领 lot |
| `delivered` | 完整交付事务已提交，lot 已消费，唯一 Agent/claim/来源可查 | 只读原交付结果，后续按既有 claim 生命周期运行 | 不能重新制造、重复领取或通过退款撤销世界历史 |
| `cancelled` | 开工前取消/启动过期结算已提交 | 原路解冻，保留终态和原因 | 不消耗制造投入、不产 Agent |
| `disposed` | 开工后永久失败、到期或有权主体显式确认报废 | 消费 WIP/lot，确认已消耗损失、解冻未结算认领款并释放容量/保管位 | 首版无 salvage 返料/转卖收益，不删除历史 |

quote 是独立只读预览，不作为持久 order 状态。提交超时/连接断开是 request outcome unknown，不是新增业务终态，也不触发退款或重新制造。技术 prepare/serialize/publication 失败若无提交，则保持前态，不转成一次经营损失；如果已提交但 ACK 丢失，回读同单恢复事实。

生产进度来自已提交的 `completed_work_ticks` 和 `last_progress_tick`：仅在该 tick 当前权限、同一设施/配方、维护与原 WIP 条件全部合法时增加一次；受阻或世界暂停不增加。`ready_at` 只是按剩余工作量重算的预测，不是完成证据；合法恢复保留已完成工作，不能把无权限期间计作生产。硬截止仍为专业政策定义的首次阻断期限与 `started_at + 12 + 24` 上界的较早者，恢复不刷新；若剩余合法生产时间不足则明确预计无法按期完成。到期事件与恢复/交付同序处理，窗口端点 `now >= deadline` 先判过期，不能在等于截止时恢复后再终结一次。经济冻结/争议禁止支出的退款部分保留原受益人的冻结返还记录，不转给操作者或治理池。

## 6. 接口与数据合同

以下是待实现的逻辑操作名与最小合同，不声明现有 HTTP 路径已开放；外部入口应走既有鉴权 Service。所有身份包括 world/chain/genesis，载荷使用 `schema_revision=1`，仅允许已启用字段；未知影响行为的字段/版本拒绝，不通过忽略字段降级。

| 接口/条款 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| `GetAgentManufacturingQuote` | world read → Buyer/Viewer/API/Agent | subject/factory/recipe、quote id/digest、依赖 revisions、有效期 | 只读且无续租；引用可信读取版本 | 完整 quote 或缺料/产能/资格/unknown blocker | 未声明制造 capability 时 unavailable |
| `ConfirmAgentManufacturingOrder` | 获准主体 → order authority | quote id/digest、request id、buyer/beneficiary 与当前授权上下文 | 一个 quote 仅消费一次；同键同载荷复用结果 | submitted 只表示受理；正式 reserved 需提交定位 | 老 ClaimAgent submit 不能转换为此操作 |
| `CancelAgentManufacturingOrder` / `DisposeAgentManufacturingOrder` | 当前合法主体 → order authority | order id、expected order revision、意图/损失确认摘要 | 原单 CAS；终态重复返回原结果；冲突请求拒绝 | reserved 可取消；开工后只允许显式报废/法定到期 | 不将旧通用 cancel 解释为退款权限 |
| `ResumeAgentManufacturingOrder` | 同一受益关系的当前合法主体 → order authority | order id、expected revision、当前重新认证的执行授权引用/revision | 原单 CAS；原受益人/工厂/配方/资金来源不变，保留工作量与所有硬截止 | 当前授权及原条件全部合法则可续原单；否则返回可读 blocker，不恢复旧 grant | 只替换已验证的当前执行授权，不重绑 owner、不创建新单 |
| 内部 `Start/Progress/Complete/Deliver/Expire` | canonical world scheduler → prepared publication | order id/revision、预留或 lot id、causal parent、progress tick | 相同顺序规则；每个合法生产 tick 最多一次进度；内部身份不能由外部任意提供 | 一次状态转移或具体 blocker，无部分效果 | 原单 pinned policy；拒绝不支持 producer |
| `ResolveAgentOwnerSource` / `RefreshManufacturedAgentOwnerSource` | 既有身份/控制 consumer；原合法 owner 的当前证明 → runtime | target、claim acquisition ref、source/identity generation、规范化根 owner ref 及当前世界/身份版本 | 解析只读；刷新同原主体和同存续 claim 的来源 CAS，严格增加 generation，旧 grant/待决执行不继承 | 无目标 starter 记录也能解析制造控制；根来源撤销/变更则禁止新控制，合法同主体恢复才可显式刷新 | 不改 PlayerSession primary，不新建 key registry；见 DES-AGENT-MFG-004 |
| `GetOrder` / `ListMyOrders` / `ResolveRequest` | canonical projection → 获准消费者 | subject/world/order/request、order revision、commit/source identity | 不推进时间、结算、续租或授予权限 | phase、资金/物料分类、receipt、下一步/unknown | 裁剪或旧缓存显式不完整，不补成 delivered |

订单保存 `order_id`、buyer principal、经济 owner、builder、受益人、工厂/位置、固定配方/body/政策/claim 条款、quote hash、确认/启动/完成/截止 tick、`completed_work_ticks/last_progress_tick`、原始资产来源、各 reservation、WIP/lot、状态/revision、原因、唯一交付关联和各阶段提交定位。资金来源和金额保留 checked integer 精度，禁止负量/溢出/重复材料条目掩盖欠额。费用不是依赖 UI 重算的最终状态。

请求 id 最多 128 UTF-8 字节，逻辑载荷上限 16 KiB；数量固定一，材料清单只接受本版本标准五项，不由客户端扩展。单主体按 cap 自然最多两个制造未终结单，单设施最多一个未终结制造单；订单列表默认 20、上限 50，读面从索引分页，不把无限历史塞进 Agent context。这些是首版资源预算，不是实测性能指标。

## 7. 状态、事务与持久化

制造、[紧急保供](emergency-supply-authorization-and-settlement.design.md)和[组织资产处置](organization-continuity-and-disposition.design.md)必须消费同一底层资源 revision、已承诺库存和 reservation 守卫；不同业务不能各自把同一物料、电力或资金当作可用。已经被订单合法预留/消费的部分不是无主余额，不因保供授权、组织休眠或接替而可直接征用/返还。组织处置只能承接其有权处置的订单权益及未决义务，不能静默改制造受益人、退款来源或以成员身份动用个人 builder 资产；需要合法限制时冻结受影响后续动作并保留原账，不复制、删除或解除原业务义务。

<a id="des-agent-mfg-004"></a>
### DES-AGENT-MFG-004：交付原子安装身份、经济与控制事实

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928；声明：[REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；验收：[AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)。

交付前必须同时具备：一个匹配订单/工厂/配方的未消费 lot、已确认且未释放的 claim 资金与取得容量、当前有效受益主体/主绑定和取得资格、有效工厂交付位置、未撤销的 body/recipe 交付权限、未过期订单，以及从未使用过的目标 Agent 身份。报价权限不够，系统调度也不免除这些检查。

`PreparedAgentManufacturingDelivery` 在 detached projection 中一次准备以下差量，所有校验和 fallible 计算完成后才进入正式 publication：

1. 消费唯一 lot，记录订单到新 Agent 的不可变制造来源；目标 ID 由世界分配的制造身份空间和 order ID 确定，客户端不能挑已有 ID、原主 Agent ID 或其他世界 ID。
2. 以固定标准 body 模板创建一个 AgentCell，位置来自原设施 location authority，初始资源为零，不执行 onboarding bootstrap、RegisterAgent 外部调用、starter 资助或隐式模块安装。
3. 按冻结的制造 claim 条款把原 liquid 预留结成既有 activation fee sink、liquid provenance bond 和首期 upkeep；不能再次从账户余额扣同笔款。claim owner 使用已验证映射；当前 epoch 是真实交付 epoch，后续 upkeep 从下一适用 epoch 开始。
4. 安装排他 claim、下述 `ManufacturedAgentControlSourceV1` 和新目标的既有 `CapabilityAgentIdentity { owner_binding, generation }` 投影，解除对应取得/资金/保管预留。三个事实与 AgentCell 同根提交；不得在交付后再独立调用身份安装方法补权限。来源适配只把这一合法取得接入既有控制 authority，不新造 PlayerSession primary binding、不复制主 Agent 的目标/grant，不赋予团队额外自治。
5. 在同一 publication 中将 order 标为 delivered，写唯一 delivery receipt、去重/终态索引、journal/event 与可恢复的 state delta。中间未认领 Agent、已扣款未交付或已赋权未有制造来源的状态不得被观察或持久化。

**无 starter 记录的成功控制路径。**新增逻辑记录 `ManufacturedAgentControlSourceV1` 由交付准备器产生，首版字段为 `world identity / target_agent_id / manufacturing_order_id / delivery_receipt_ref / claim_acquisition_ref / claimer_agent_id / owner_subject / root_owner_source_ref / source_generation / identity_generation / status`。`claim_acquisition_ref` 引用这一次原子取得的权威 receipt，不能只用可重复的 target/owner/epoch 组合；`source_generation` 对该 Agent 单调且不可复用，初次为 1。持久状态为 `active / revoked`，当前根证据缺失、过期或版本失配另投影 `suspended` 且不可执行，不因旧 `active` 字段继续授予权限。新目标 capability identity 的 `owner_binding` 是已验证玩家主体，generation 从新身份 1 开始，后续沿既有 identity authority 严格增加。制造来源和历次控制来源分开保存：前者永远指向原订单，后者随合法 claim/根身份状态演进。

`root_owner_source_ref` 必须归一到交付时已验证的原主 Agent 独立 owner 来源和当前身份/密钥 authority；v1 的 claimer 也是该根经营主体的合法映射。它引用来源身份及 revision，不复制密钥、不从物料持有或主体字符串推导所有权。制造 Agent 不能成为另一个制造控制来源的根，也不能递归解析“子 Agent 拥有子 Agent”；自营映射必须一次解析至非制造根，循环、多根或组织无合法映射时拒绝。根 owner 的现有注册密钥与世界/branch/reorg/identity 校验继续生效。

| 必须接入同一 owner-source resolver 的 consumer | 制造来源的可执行合同 |
| --- | --- |
| 正式 Service 的 owner key/签名读面、owner chat/intent、受支持的定向 gameplay action | 当前 claim acquisition、目标 identity generation、根来源与已认证 owner 全部匹配时，原玩家可明确选择这个额外 Agent 查看获准 owner 数据、提交直接意图和执行现有已准许动作；不能要求目标有 starter 记录，也不能让旁人仅凭 target ID 获权。明确 target 的请求不改变默认主 Agent 路由 |
| Runtime owner control admission 及 replay/bind、host `validate_registered_owner` 与 Viewer 控制映射 | host 验证当前账号密钥后消费同一 committed 制造来源；sidecar/“我的 Agent”关系仅为可重建投影，不是新 authority。原 owner 可按现有规则中止/override；历史重试也先重验当前来源，缓存不能保持失效权限 |
| grant 安装/撤销、grant source 校验和真正执行 consumer | 制造交付不创建 grant。owner 后续显式申请的 grant 才可引用 `manufacturing_claim:(target, source_generation, claim_acquisition_ref)`，沿既有动作白名单、对象、期限、预算、撤销和异议规则执行；该 source 不能替代独立 signer/capability 委托。所有制造目标的普通动作及 signer/capability 执行也重验 live claim、source generation 和根来源 revision，不能只比未刷新的目标 identity generation 而绕过根撤权 |
| claim 终态与后续合法新认领 | `AgentClaimReleased/Reclaimed` 的正式终态同次撤销当前控制来源并推进 identity generation；旧 grant、旧 signer 委托和旧待决意图在新效果提交前失效。grace/ReleaseRequested 只消费既有生命周期的限制，不擅自提前删除 claim |

后续对这个既有 Agent 的普通 ClaimAgent 只有在原认领规则独立准入、共享容量守卫及未决义务约束通过后，才在新 claim 的同一事务中创建后继控制来源与更高 identity/source generation，解析新 claimer 的合法非制造根；不能复活原制造 owner/grant、重发 Agent 或更改原订单受益人/退款。该适配不建立 AOS-005 经济转让合同，也不让 release→claim 消灭其原业务责任；不符合既有取得/义务边界的请求继续拒绝。

根 owner 来源撤销、失配或转给其他主体时，所有相关制造来源在每次鉴权/执行重验时立即不可用，不能等异步投影更新。相同合法主体完成既有身份恢复或密钥轮换后，可用当前证明执行 `RefreshManufacturedAgentOwnerSource`；它重验同一存续 claim、原 owner、当前根和期望版本，在同根事务中发布新来源/identity generation，不更新订单、不迁移主绑定、不恢复旧 grant/待决执行。Refresh 使用原 owner 的当前独立根身份证明进行专用恢复准入，仍要求原主体、同一存续 claim 和 expected source generation 匹配，不要求已经 suspended 的旧目标来源先变回 active；这一准入只可刷新来源，不能用于普通动作、派发或越过其他授权。只有历史 title、当前根属于别人或 claim 已终结时不得刷新；经济责任和只读来源保留，另行取得须走合法新 claim。

控制读面可以异步投影，但必须绑定交付提交；投影未追上时显示“交付已确认，控制列表同步中”，不能显示第二个确认认领动作。只有上述 source、capability identity、鉴权/执行 consumer 以及 host 适配全部可用后才能启用制造；未适配时是部署 capability 缺口，不能把永久 `owner_source_unavailable` 当完整成功路径。运行时内存应用、journal 记载与正式持久世界对外发布分别遵守现有宿主保证；只有具备相应已提交持久状态定位才对外称“已持久交付”，单 action id 或 preview 不足以证明。

<a id="des-agent-mfg-005"></a>
### DES-AGENT-MFG-005：跨请求唯一性、权限变化与冲突

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928；声明：[REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；验收：[AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)。

- 请求幂等键为 `(world identity, authenticated buyer principal, operation, request_id)`，绑定规范化 payload digest。同键不同载荷返回 conflict；同键同载荷返回原动作/订单及当前获准读面，不再执行。
- 业务唯一性另外覆盖：`(world, quote_id)` 只能确认一单；`(world, order_id)` 只能有一个生产根、一个 lot、一个交付或报废终态；`(world, order_id, output_index=0)` 只能对应一个 Agent；目标 Agent ID 还受全局未使用/不可复用守卫。换 request_id、换设备、重报价或从不同入口重放同一 order/lot 不能再次交付。新 quote 是新业务承诺，仍受相同主体/资金/材料/容量守卫约束。
- pending/终态索引与各资源预留属于同一提交点。读缓存、仅 HTTP request cache 或内存递增 action id 均不能独立提供上述保证。压缩历史时保留业务终态与已使用身份的不可再用证明，不能按普通请求缓存 TTL 删除唯一性依据。
- 权限 revision 变化后，未产生效果的请求重新检查当前主体、设施支出和受益资格。操作人失权只使其后续请求失效；不会把订单/退款给新操作者，也不撤销旧已完成效果。原合法主体可以使用当前凭据对同一 order 继续/取消/报废；新请求不能恢复已撤销的 grant。身份重绑定或受益人改变不由制造自动接纳。
- 普通治理 quota、运营注入、raw RegisterAgent、材料兑换、自定义模块 receipt 不能写制造交付来源或通过 lot/identity 唯一守卫；系统只能从完整匹配的制造 producer 创建这一类结果。普通首次注册仍是独立机制，不得为已有账号反复开新首次身份以冒充制造。

<a id="des-agent-mfg-006"></a>
### DES-AGENT-MFG-006：恢复切点与不可重复消费

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928；声明：[REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；验收：[AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)。

快照/checkpoint 必须同切点包含 order、quote 消费索引、资源和取得 reservations、WIP/lot、Agent 来源/claim、制造控制来源与根引用/状态、capability identity generation、幂等与终态索引、事件顺序及定时器依据。journal replay 验证各阶段事件与其父记录及 pinned payload 匹配；已提交事件恢复原效果，不用现在的政策重判历史为另一结果。恢复 owner 读写前仍核对当前根身份/密钥与存续 claim，不能凭历史制造 receipt 恢复旧权限。尚未终结工作恢复后，以当时真实 world tick 和当前授权/能力重评后续动作。

启动前崩溃若确认已持久化，恢复 reserved 及原 deadline；未持久化的受理响应只能显示结果未知/未找到，不能合成订单。开工 sink 已提交时恢复原 WIP 和已提交工作 ticks，不能再次扣料、按离线墙钟补生产或重复计同一 tick；完成已提交时恢复同一 lot，不能重产；交付已提交时返回原 Agent/claim/receipt，不能再扣款。状态、余额、来源或唯一性索引互相冲突时隔离该制造写路径并提供诊断，不删除异常记录后“重建成功”。

技术回滚不能撤回已提交的制造消耗、已开始的维护或外部已见交付。可恢复差异必须通过该订单允许的后续事件处理；未规定的资产赔偿需由现有治理/争议 authority 另行判断，不能由 runtime 自行铸币或改快照。

## 8. 部署、安全与运行约束

只在运行时、正式持久化读写、有效主体映射及对应消费者声明制造 capability 后启用新单。首版复用既有单权威世界执行，不新增服务/数据库/后台私钥；多节点只通过现有世界提交过程形成一个顺序，不能由各节点各自产生交付 ID。既有正式世界准入、共识、发布与部署限制继续适用。

签名只证明请求来源。确认必须分别证明 buyer 当前能承诺其资金/取得规模、builder 当前能操作该工厂并支出其材料/电力、beneficiary 当前具有取得资格。首版这些角色只能落在现行权限支持的同一自营关系；如果跨主体授权尚无合法映射，返回 `unsupported_authority_scope`，不把组织成员列表当授权。Agent 自主发起仍需现行委托的具体范围及预算，不借制造新增授权。

权限撤销、设施禁用、政策停止新单不会阻止只读结果；也不能无限延长未结单。退款原持有人因争议处于冻结时，退款记录归该主体的冻结余额，不能转给请求人、新控制者或世界公共池。无身份/权限的读取不得暴露其他人的资金、原料数量或订单详情；公共工业聚合按现行隐私权限消费脱敏信息。

<a id="des-agent-mfg-007"></a>
### DES-AGENT-MFG-007：同义读面和下一步

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928；声明：[REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；验收：[AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)。

读面最少给出 world/subject/order、phase、已知提交点、quote/生产/交付时间、每类已消耗/仍占用/可返还价值、认领 upfront 与未来 upkeep、原交付位置、主要 blocker、允许动作、`next_recheck_tick` 和成功/失败原因。正常流程始终允许玩家理解“保留主 Agent，稍后制造”的选择；制造入口不可把额外成员包装为普通工业的强制前置。

Viewer、pure API 和获准 Agent 对同一 committed projection 保持相同完成语义；未支持或裁剪的消费者明确 unavailable/incomplete，不能把 missing 当零成本、无责任、空订单或成功。语言转换不改费用和损失。当前主体切换、quote 过期、订单更新或断线后清除旧的可执行结论；查询/显示不确认、不启动、不续期、不解冻。

## 9. 质量与容量

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| 竞争与守恒 | 同世界一工厂两个普通槽；100 次并发/重试制造请求及普通 recipe/claim/transfer | 所有已提交顺序只产生合法预留、一次 sink/交付，无负余额或超槽 | 专业政策与唯一性不变量；通过数=所有枚举前后态一致 | MFG-V02/MFG-V04 的 deterministic runtime 测试 | 目标，未运行；不承诺 HTTP 吞吐 |
| 有界占用 | 每设施一未终结单、每主体最多两个；跨所有等待截止端点 | 等待不会无限续租，已消费与未消费资产分类准确 | 专业政策时间窗；`deadline-1/deadline/deadline+1` 全覆盖 | MFG-V03 | 目标，未运行 |
| 故障恢复 | 在确认、开工、完成、交付的 prepare/persist/publish 边界逐点中断 | 重开与 uninterrupted reference 的最终状态/来源/资金/唯一性一致 | 事务与原世界持久化合同 | MFG-V05 | 目标，未运行；内存测试不代签正式持久世界 |
| 读取预算 | 20/50 分页、16 KiB 请求、完整历史保留且无全量 prompt 注入 | 过大载荷拒绝，分页不丢待决订单、不以裁剪宣称完结 | 本文首版资源预算 | MFG-V06 | 目标，未测时延/P95或跨平台性能 |

## 10. 兼容、迁移与回滚

1. 先实现新 policy/quote/order/receipt 和版本识别、资源预留守卫、完整 prepared delta、制造 owner-source/identity 与全部正式鉴权读写/host 消费者、持久恢复及只读投影，再开放确认入口。原来只接受 starter 来源的 consumer 须明确接受合法制造来源；旧的无目标 CollectData/首个认领入口仍只解析原主 Agent，不因 owner 有多个 Agent 而任意挑选或变成歧义。目录存在不表示新 recipe 可被普通 ScheduleRecipe 当材料配方执行；必须通过制造订单 producer 才可进入其 Agent output 分支。
2. 可信旧 schema 确认不含制造能力时，可按明确的版本迁移规则增加空制造集合；现有材料工业、身份和 claim 保留原值/来源，不自动转换成制造订单，也不对首次注册补 claim。含未识别制造数据的快照禁止写模式启动，不能忽略字段后读成空订单。
3. 已有 claim 不改写费率、slot、bond provenance 或维护历史。新增取得的共享 guard 检查真实经营规模；历史超过新口径的主体只禁止继续扩张，不强制回收。所有会动用相同资金、材料、电力、工厂槽和取得容量的现行入口必须识别订单预留及已完成制造的实际占用后才能启用，不能只给新入口加锁或只在订单 pending 时限制普通 ClaimAgent。
4. 正常参数调整给新 quote 使用新 revision；旧单保留冻结内容和固定截止。禁用新单后可继续按受支持旧 revision 履约/处置。存在未终结订单或已交付新 provenance 时，不允许降级到丢弃这些字段/事件的旧二进制写入；必须保留兼容读取与终结器，无法满足时维持制造写入禁用并使用支持新数据的版本修复。
5. 不定义自动退款“全仓迁移”或一次性清空订单。回滚的安全边界是关闭新接单、继续读既有结果并用受支持版本结清；已经交付的 Agent/claim 及已确认的投入不能当作可撤销部署配置。

## 11. 验证设计与可追溯性

下面的 MFG-V01–MFG-V07 是[制造系统验收手册](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md)中的**未来验证场景 ID**，不是已存在测试函数或已通过证据。实现时先落入真实 runtime/API 测试及适用正式持久世界验证，再在 PR/既有 [#4850](https://github.com/nil-studio/oasis7/issues/4850) 记录同候选结果；源码静态核对、文档结构检查和历史 claim 测试各自保持证明范围。

<a id="manufacturing-validation"></a>
### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-AGENT-MFG-001](agent-industrial-manufacturing.design.md#des-agent-mfg-001) | 报价、配方、费用、冻结及非 slot-1 补贴 | [MFG-V01](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v01)：同 source/config/policy 的 required 规则与只读无效果对照；未执行 | PR 测试结果及 quote fixture | 不证明平衡/玩家理解 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-AGENT-MFG-002](agent-industrial-manufacturing.design.md#des-agent-mfg-002) | 资源和产能预留被所有冲突入口计入 | [MFG-V02](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v02)：固定世界/政策的 required runtime 并发及完整账本对照；未执行 | 事件与余额/预留 oracle、同候选 CI | 单 handler 单测不证明集成守卫 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-AGENT-MFG-003](agent-industrial-manufacturing.design.md#des-agent-mfg-003) | 等待、取消、报废、期限与损失确定且有界 | [MFG-V03](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v03)：固定世界时间/授权输入的 required 状态、截止与守恒矩阵；未执行 | 状态转移及资金/物料守恒 oracle | 不推断远程运输或 salvage |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-AGENT-MFG-004](agent-industrial-manufacturing.design.md#des-agent-mfg-004) | 原子交付身份、claim、费用与真实控制来源 | [MFG-V04](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v04)：同候选 required 逐写点注入及 full 正式 Service/host 鉴权正反例；未执行 | Agent/claim/source/identity/receipt 与 rejected effects oracle | API accepted/旧 claim 回归不能证明此项 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-AGENT-MFG-005](agent-industrial-manufacturing.design.md#des-agent-mfg-005) | 跨 request 唯一性、伪造拒绝与交付前权限变化 | [MFG-V04](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v04)：固定同世界/order/lot，required/full 的重试、跨主体/世界与来源唯一性矩阵；未执行 | 唯一 quote/order/lot/Agent 索引、拒绝差分与 receipt | 仅请求缓存去重不证明业务唯一性 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-AGENT-MFG-006](agent-industrial-manufacturing.design.md#des-agent-mfg-006) | 同切点恢复、旧数据兼容与终态不可重放 | [MFG-V05](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v05)：同 source/config/world 的 full 正式持久化切点及版本矩阵；未执行 | 正式持久世界恢复记录、版本与状态 hash/receipt | 内存 replay 不代签持久恢复/跨节点 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-AGENT-MFG-007](agent-industrial-manufacturing.design.md#des-agent-mfg-007) | 各受支持读面同义、未知不误导、仍有主 Agent 路线 | [MFG-V06](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v06)：同候选 full，正式 Viewer/API/Agent 各阶段与原主 Agent 经营对照；未执行 | 逐入口固定 projection、截图/API 及行动回执 | 未支持入口 unavailable；静态文案不证明体验 |
| [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-AGENT-MFG-004](agent-industrial-manufacturing.design.md#des-agent-mfg-004) | 交付后维护、claim 终态与后继控制来源连续 | [MFG-V07](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v07)：同候选 required 生命周期及 full 跨 upkeep/释放/新认领/重启；未执行 | 余额/claim/source generations、旧执行拒绝及世界归因 | 新场景未实现；旧 claim 测试不代签制造适配 |
| [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-AGENT-MFG-005](agent-industrial-manufacturing.design.md#des-agent-mfg-005) | 根来源失效与恢复不复活旧执行，quota 非供给 | [MFG-V07](../../testing/manual/agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v07)：固定 owner/source/world 的 required 撤权/Refresh/伪造对照与 full 受支持实玩；未执行 | CI、当前根鉴权/旧 grant 拒绝、单主路线与扩张记录 | Agent/自动化不代签真人理解或部署 |

验证不仅比较“重试后的最终成功”，也要先观察失败点没有部分状态/扣减，保留失败与恢复的原始关系。文档 PR 只做内容/引用/关系与独立设计审读，不触发或代签游戏实现测试。

## 12. 决策、长期风险与未决问题

| 决策或风险 | 首版选择与理由 | 后果、owner 与复核触发 |
| --- | --- | --- |
| 自营先行 | 当前工业 requester=builder；先复用合法自营输入、设施和资金，再定义第三方交易 | 第三方组织/玩家下单若无授权映射会 blocked；gameplay/identity owner 在真实采购需求明确时再扩政策，不临时跨账号代扣 |
| 生产与取得分开 | 完成只生成唯一 lot，交付原子生成 Agent+claim；避免可被抢占的未认领窗口 | 需新 prepared delta 与保管位，不能仅新增一个 RecipeExecutionPlan；runtime owner 在实现审读检查全部写入口 |
| 制造取得接入现有控制 authority | 新目标没有 starter 记录，交付同次增加制造 claim 控制来源及 capability identity；正式 Service/host 共用 resolver | identity / Agent / runtime owner 须实现所有鉴权读写与终态失效适配后才启用；主绑定、密钥 authority 和自治规则保持原主责，不能只交付一个永远不可控制的 AgentCell |
| 确认时预留资源与 claim 成本 | 让玩家的固定承诺能履约，同时保持未交付不扣认领费 | 必须扩展同资源消费者的可用量判定；未全覆盖前不得开放新单 |
| 固定上限和期限 | 单单/单设施有界，为普通工业留容量，不引入排队市场 | 短等待窗口与 12 tick 新配方是待测平衡选择，gameplay owner 以真实材料获取/等待/取消样本复核；不得写成已验证最优 |
| 失败不返已投入材料 | 首版没有经批准的 salvage 算法；固定单次全投入最易审计，避免返料套利 | 损失可能过重，需在用户决策预览及未来玩家样例验证；新 salvage 能力须新政策，不能改旧 receipt |
| 绑定规模与旧 claim 数不同 | 制造按主绑定与 claims 并集计规模，至少额外槽，禁止 slot-1 洗补贴 | 现有 claim quote/投影需要明确适配，历史费用不迁移；runtime/Agent owner 以“已注册但零 claim”样例检查 |
| 功能完整性与证据 | 本文冻结可实施合同及首版政策，实际制造 producer/恢复/读面尚未实现 | #4850 继续承接实现/验证；有界结构通过不能升级 AOS-004 实现状态或公开能力 |

首版无必须临时猜测的配方、费用、产能、等待或损失参数；长期开厂服务、远程运输、定制型号和可回收半成品明确不在本期。对应专业 owner 只有在补齐政策、权限、结算和同候选验证后才能扩大范围，不因本文 active 自动启用。
