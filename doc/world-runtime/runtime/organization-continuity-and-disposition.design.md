# Organization Continuity and Disposition System Design

生命周期：`active`；Owner：`runtime_engineer`；协作 owner：`gameplay_designer`、`blockchain_ops_engineer`、`viewer_engineer`、`qa_engineer`；Last reviewed：2026-10-19。

## 1. 问题、目标与非目标

承接[组织连续性产品 PRD](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md)：组织解散、不活跃保护、受限维护和恢复不能造成越权处分、双重授权、重复资产效果或历史删除。本文冻结首版逻辑 schema、命令、作用域、事务与恢复合同，具体语言类型与 transport 编码由实现适配；玩法参数消费 [organization-continuity-policy-v1](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-time)，不在 runtime 复制一套数值或创造治理权限。

本次设计基线：`nil-studio/oasis7` 提交 `e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`。新政策、schema 与接口为本 PR 提出的实现输入，不是已上线合同或先前批准记录。产品保留的 2026-10-17 静态实现证据仍只覆盖其固定版本与窄范围；本次未运行组织业务、Rust 或跨节点测试。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / AC | 具体 obligation | 本设计条款 | 外部 owner / dependency | 排除范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001) Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | 冻结、合同、托管、债权责任、业务处置、剩余分配顺序及独立权利保护 | [DES-WR-OC-001](#des-wr-oc-001) | game 拥有经济/合同政策；各资产 authority 拥有实际转移 | 不定义外部清算价格、自动拍卖或额外资产权属 |
| [AC-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-001) Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | 冻结、合同、托管、债权责任、业务处置、剩余分配顺序及独立权利保护 | [DES-WR-OC-001](#des-wr-oc-001) | game 拥有经济/合同政策；各资产 authority 拥有实际转移 | 不定义外部清算价格、自动拍卖或额外资产权属 |
| [REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001) Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | 跨请求/case 的来源守恒、单次处分及恢复后权利/责任读面 | [DES-WR-OC-004](#des-wr-oc-004) | runtime journal/资产 owner/Viewer；消费本节清单与义务来源 | 不以记录存在证明实际外部结算 |
| [REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | 通知、保护、受限持续与请求层分离；恢复不越级 | [DES-WR-OC-002](#des-wr-oc-002) | game 提供本 PR 的 v1 政策；P2P 提供权威授权与可用性事实 | 不把离线或消息未读认作放弃 |
| [REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | 通知、保护、受限持续与请求层分离；恢复不越级 | [DES-WR-OC-004](#des-wr-oc-004) | game 提供本 PR 的 v1 政策；P2P 提供权威授权与可用性事实 | 不把离线或消息未读认作放弃 |
| [AC-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | 通知、保护、受限持续与请求层分离；恢复不越级 | [DES-WR-OC-002](#des-wr-oc-002) | game 提供本 PR 的 v1 政策；P2P 提供权威授权与可用性事实 | 不把离线或消息未读认作放弃 |
| [AC-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | 通知、保护、受限持续与请求层分离；恢复不越级 | [DES-WR-OC-004](#des-wr-oc-004) | game 提供本 PR 的 v1 政策；P2P 提供权威授权与可用性事实 | 不把离线或消息未读认作放弃 |
| [REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | 重叠范围唯一授权、原子交接、不迁移旧请求 | [DES-WR-OC-003](#des-wr-oc-003) | runtime 拥有执行和 journal；P2P 拥有授权证明；Viewer 只读投影 | 不授予永久所有权或运营旁路 |
| [REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | 重叠范围唯一授权、原子交接、不迁移旧请求 | [DES-WR-OC-004](#des-wr-oc-004) | runtime 拥有执行和 journal；P2P 拥有授权证明；Viewer 只读投影 | 不授予永久所有权或运营旁路 |
| [AC-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | 重叠范围唯一授权、原子交接、不迁移旧请求 | [DES-WR-OC-003](#des-wr-oc-003) | runtime 拥有执行和 journal；P2P 拥有授权证明；Viewer 只读投影 | 不授予永久所有权或运营旁路 |
| [AC-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | 重叠范围唯一授权、原子交接、不迁移旧请求 | [DES-WR-OC-004](#des-wr-oc-004) | runtime 拥有执行和 journal；P2P 拥有授权证明；Viewer 只读投影 | 不授予永久所有权或运营旁路 |

消费[确定性执行](../design.md#runtime-deterministic-design)、[待决恢复](../design.md#runtime-pending-design)、[恢复边界](../design.md#runtime-recovery-design)和[存储治理](runtime-storage-footprint-governance.prd.md)。它们分别约束执行、查询/非重放、恢复及 retention，不替代本专题资格与处置规则。

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线 | 目标 | 差距 / owner |
| --- | --- | --- | --- |
| 产品边界 | 基线 PRD 已声明三组 REQ/AC 和 OC-1..7 | 每项有明确技术条款及验证计划 | 本文补齐映射，不是运行证据 |
| 组织执行、存储与 API | 本次基线 `AllianceState` 仅有成员、charter 和成立信息；`DissolveAlliance` 仍走既有联盟删除及成员保留路径 | 处置、授权、义务、资产与 receipt 可联合恢复 | 本文冻结业务合同；组织模型与 consumer 仍需实现，不以 Agent claim 状态代签 |
| 跨节点及玩家表面 | 未验证 | 同候选权威结果与正式表面对账 | P2P / Viewer / QA 负责真实环境验收 |

## 4. 边界与结构

game 产生版本化 charter、有效活动、通知/保护期限、资格和处置政策；治理/P2P 提供有权主体批准、撤销与裁决的权威事实。runtime 在当前世界顺序中重验这些事实，拥有阶段、授权范围、未决义务、请求与 effect journal。资产、合同、托管子系统拥有其资源与最终结算事实，不能被组织状态直接覆盖。

执行复用现有 [staged transition 合同](../prd.md#runtime-deterministic-acceptance)、[publication / 恢复设计](../design.md#runtime-deterministic-design)与[普通 generation 持久结果](../design.md#runtime-generation-persistence-design)，在其 typed prepared delta 中增加组织业务投影；不重做 `TransitionBuffer`、第二套提交日志或联盟 WASM 迁移。当前基线定位：[AllianceState](../../../crates/oasis7_world_state/src/gameplay_state.rs)、[联盟动作](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_gameplay.rs)、[联盟 prepared publication](../../../crates/oasis7_world_state/src/world/alliance_war_publication.rs)、[canonical publication](../../../crates/oasis7_runtime/src/world/event_processing/publication.rs)。这些源码只证明可复用基础，不证明本专题业务已接通。

本文的类型和命令是组织权利保护的逻辑合同，不指示直接扩展被冻结的 native Alliance 经营语义、核心 action enum 或 `WorldState` 顶层制度字段。实现按现有[Kernel/制度边界](../prd.md#kernelgoverned-physics-与-institution-module-边界)分类并选择合法的授权/权利记录承载，保持同一 canonical publication 与恢复验证；本次不选择或启动 SC32 制度 pilot，也不把 Alliance 迁入 WASM 作为本保护合同的前提。

组织成员、Agent 和客户端只提交 intent/证据；签名只能证明来源，不能单独证明处分权。Viewer/API 读取来源版本和 receipt，不通过缓存、管理员编辑或重连制造处置资格。不得把组织资产集合扩大到成员独立资产，或把 Agent 控制变更转换成身份/来源删除。

## 5. 关键运行流程

<a id="des-wr-oc-001"></a>
### DES-WR-OC-001：处置顺序与分步原子提交

Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a；声明：[REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001)；精确政策：[estate 白名单与清偿顺序](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-estate)。

1. 按组织身份、处置 case 身份、政策版本、触发事实与范围登记解散/重组 intent；验证治理授权、相关资产 owner、未决义务与保护底线。缺失、冲突或未知时阻断，不能视为无义务。
2. 确认风险冻结，只限制声明范围内的新风险与越权处分；合法合同履行、保护和查询路径仍按各自合同运行。未提交冻结不能宣称已保护。
3. 依次处理合同履行/终止/结清、可识别托管返还、债权/成本/责任、Agent/设施/业务处置，最后处理剩余。每步保存义务身份、处理证明、资产版本、效果量和 receipt。债权阶段可以先确认合法偿付计划与足额预留，再准许业务资产变现；所得必须回流清偿，不能因推进到业务处置就直接分剩余。未知数量且不能形成可靠上限的责任继续阻塞相关资产；游标前进不证明前序已结清。
4. 长期物流、拍卖、合约结算不是一个跨时间事务。每一步条件提交独立 effect，与前序 receipt 关联；未完成外部义务保持可读待决，不能提前分配其占用资产。外部合同无法支持原子资源变更与 journal 对账时禁用该类处分。
5. 重试同 effect 返回原结果，不能重复返还、转移或剩余分配。处置终结保留稳定组织/Agent 身份、来源、权利与责任记录；未来纠错必须创建关联的新裁决/effect，不改写历史。

**清单与冻结点。** `EstateManifestV1` 对每一项记录 `asset_ref / title_receipt / rights / quantity / encumbrances / disposition_class / version`，分类为组织可处分、第三方托管返还、原规则受限、权属争议或未知。成员关系、设施所在位置和持有 Agent 不能推导组织所有权；没有资产 owner 的完整权属与负担证明时，整项保持隔离。冻结提交同时固定 manifest root、相关对象版本及禁止新风险的 scope；预览和 case 登记均不是冻结成功。冻结之前已合法提交的效果保留，之后到达的操作按新范围重验，签名早于冻结不取得优先权。

**跨业务占用。** 组织库存、客户托管、制造认领预留、制造在制品/交付、保供承诺和补偿预算保留原 `business_origin_ref`、资源 owner 及原业务 receipt。只有原合同的结算/取消/交接 consumer 可以消费或释放，或在已有根事务内提供同等 typed delta；组织 case 不能直接把它们标成余额、退款或剩余。替换执行者不新生成需求，保供的新授权仍沿原需求 lineage 重验，不继承旧授权的不可转让资格。一个在途效果未明确 settled/cancelled 时，其量仍占用；共同制造工序、托管包和共享义务不得被部分交接撕开。

**守恒与责任。** 来源剩余额度为 `origin_total − 已互斥确认的 fulfilled/returned/disposed 总量`；接纳新预留时再减同来源其他活跃预留。实际可执行量取该来源额度与原 owner 提供的 `spendable_for_this_effect` 的较小者，后者是当前未占用可用余额加本 effect 自有预留，不能包括其他效果占用。底层余额已经反映的历史支出和预留不再重复相减。使用资产最小单位和 checked 整数运算，按政策逐资产/币种分配，拒绝隐含汇率、负余额和复制不可分资产。退款、返还、出售所得、清偿、剩余分配保留原来源/衍生所得关联；同级不足和最小单位分配保存参与人、分母、余数顺序与结果，replay 不重新取现价或当前成员名单。未清偿余额、责任主体与后续救济保留，结案不等于债务归零。

<a id="des-wr-oc-002"></a>
### DES-WR-OC-002：不活跃基础阶段与恢复请求

Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3；声明：[REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002)；政策：[活动/时间](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-time)、[reclaim/申诉](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-remedies)。

基础阶段是正常经营、可读通知、保护、受限持续、已解决；estate / 可撤销 delegation 是受限持续的容器类型，reclaim/申诉是独立请求层。

| 转移 | 提交前置与结果 | 失败 / 恢复 |
| --- | --- | --- |
| 正常 → 通知 | 当前版本政策确认不活跃事实；记录通知原因、对象、范围和权威时间 | 缺事实或政策拒绝；不改控制/资产 |
| 通知 → 保护 | 满足政策规定的通知与保护前提，仍存在触发事实且有可用主张入口 | 通知未读/不可达不能替代前提；事实纠正、活动恢复或通知失效则停止推进，记录确认后的恢复结果 |
| 保护 → 受限持续 | 保护条件已满足且仍有保全需要；确认有限范围、期限、可撤销授权及未决义务 | 无有效授权或存在阻断性待决裁决保持保护，不以超时自动通过 |
| 受限持续 → 保护 | 授权撤销、到期或前提失败；禁止新受限效果，保留已提交安全动作 | 已确认在途义务按其合同处理，不删除资产 |
| 保护 / 受限持续 → 已解决或适用阶段 | reclaim、申诉、恢复或处置裁决已确认，明确范围和保留义务 | 请求 accepted 不改基础阶段；拒绝/过期/撤回只终止请求 |

**唯一时间与截止。** 所有时间使用专业政策定义的 `C(t)`，由已提交世界 tick 与同一日志中的服务暂停事实确定。暂停事实只收窄执行、不能赋予处分权限；恢复事实由已有基础设施 authority 证明正式读写/通知/救济入口恢复，不接受客户端自报。各窗口为 `[starts_at_C, expires_at_C)`，`C == expires_at_C` 时旧授权已失效，阶段推进仍要重新验证所有前置；不会因为 timer 到期就自动获权。相同 tick 内使用既有权威输入顺序，执行和截止判断都在提交点发生；重放不再累计时间，恢复不补扣世界离线或已记录暂停区间。政策变更对新 case 生效，在途 case 保留原版本与剩余窗口；未知版本拒绝推进。

计时器持久化 `last_accounted_world_tick / C / availability_ref`。推进候选只消费尚未计入的连续区间，按已有基础设施 authority 的有签名/来源证明、完整覆盖该区间且未过期的 availability receipt 计算非负增量，与 phase/grant 变更同一根提交；证据缺失/冲突时把该区间以 `paused(reason=availability_unknown)` 固化，推进 accounted 游标但 C 增量为零并显示 `clock_paused`。服务中断时无法提交的事实，可在下一次可提交时随该未计入区间一次确认；不得事后修改已计入区间、补扣被固定暂停的时长、倒扣 C 或复活已终止 grant。迟发现错误可按新裁决补给未来恢复窗口或发新授权，保持原截止与 receipt。runtime 不以单个节点本地健康探测取代 authority，也不复用过期的“可用”证明累积期限。

**活动与通知证据。** `activity_receipt` 必须通过组织归属、执行时授权、事件种类和已确认实际后果校验；组织之外的 `Agent.last_active`、Agent claim idle/grace 和保护维护事件都不消费为组织活动。通知发布记录完整收件范围、内容摘要、可读版本和 `published_at_C`；缺一个必需收件投影或可用救济入口则不启动窗口。未读保留未读事实，不当成同意。新增资产/权利人单独获得完整窗口，旧已通知范围不因重复发布被随意缩短或重置。

组织活动适配器还必须提供按世界 tick 单调推进的完整覆盖水位；只有在可信事件源已完整扫描、并把所有可验证的组织业务后果写入同一提交日志后，才能推进该水位。缺少归属证明或任一来源覆盖未知时，read model 显示 `activity_evidence_unavailable`，自动沉寂立案及沉寂阶段推进保持阻断；不得以空 receipt 集合、成员 `last_active`、生产/库存变动或保供记录推定组织不活跃。首版尚无该完整活动适配器，因此启用后沉寂自动判定不可用，voluntary case 与只读状态仍可用。

**首版 AtomicExchange 活动源合同。**v1 charter 必须显式选择 `registered_atomic_exchange_v1` 活动源策略；该策略的完整 source catalog 是 runtime 持久化、按组织 ID 排序并承诺哈希的已登记 AtomicExchange 合同集合。只有 founder 同时是组织成员和合约 creator、合约另一方不是组织成员且解析出的 player 与 founder 不同，才允许 founder 在现有通过的治理 proposal 下登记该来源。登记时合同必须为已接受且未结算的 `AtomicExchange`；记录合同全部固定条款哈希、世界绑定、登记 tick、creator/counterparty 的 controller root、player、source、generation 及治理 receipt。未知、同控制、已结算、服务类和未登记合同均不属于本版本活动源，也不能被解释成活动或“无业务”的证明。缺失显式 source catalog 时不启用自动沉寂。

`EconomicContractSettled` 的 prepared delta 只在合同仍与登记条款完全一致、双方当前 controller bindings 与来源记录匹配、结算成功且正向转移时，原子追加一条以合同 settlement receipt 为 `source_ref` 的活动事实。失败、过期、同控制者、条款漂移或未经登记的 settlement 不记活动。第一次成功 source registration 的 world tick `C` 是观测起点，cursor 从 `C-1` 起算；Enable 之前的未知时间不累计，也不从 `last_active` 补算。

每个已登记组织在 native 与 builtin 路径处理完 `process_gameplay_cycles`（含该轮全部经济合同 settlement reducer 和 module event routing）后，才提交 runtime-only `CloseActivityTick`。close 绑定当前 `world_binding`、tick、source catalog root、前一 close cursor、最后规范事件 ID/hash 与前一 close hash，只允许 `action_id == 0`、当前 tick、严格连续游标。正常 close 后推进完整覆盖水位；缺 tick、游标倒退/跳跃、hash 或 source catalog 不匹配时，持久化 `activity_evidence_gap`，只推进关闭检查游标而冻结完整覆盖水位，后续 close 可继续记录但不能回填缺口或恢复自动沉寂。没有来源登记、没有有效 close 或 coverage 落后当前 tick 一律显示 `activity_evidence_unavailable` 并阻断自动沉寂。

**触发类型。** `Dormancy` 严格走观察、通知、保护与受限持续。经现有 charter 权限确认的 `VoluntaryDissolution / Reorganization` 可以直接提交有界风险冻结并进入保护，但仍须完整资产公示与债权/争议处理，不能以主动解散绕过第三方权利；战争或其他专业阻塞继续消费原 authority。`Resolved` 保存恢复、重组、解散或事实错误等具体原因；恢复正常经营只关闭本 case，后续独立事实使用新 case sequence。

**请求层与救济。** reclaim 与 appeal 分为独立请求类型，绑定申请人的权利来源、争议范围、原决定/通知 receipt 与证据摘要。注册后先校验资格、证据存在及范围；缺证进入补证状态且不产生处分权。有效受理建立该请求自己的 `stay_scope`，禁止争议范围的不可逆处分和扩权交接，不暂停无关组织、其他范围或安全停止。政策允许的有权无利益冲突裁决者才能支持/部分支持/拒绝；客观事实更正也须有预先有效规则授权。裁决到复查点未完成只显示待决，不能用过期自动通过/拒绝已受理案件。

支持 reclaim 时，恢复范围、新旧授权变更、争议 stay 处置和未决义务一起条件提交。部分支持保留未裁决范围；驳回/撤回/补证过期只终止该请求，不取消 case 保护、其他 stay 或已成立债务。对已合法转移的第三方资产只能通过新补偿/纠错效果处理，不逆写旧 receipt。活动恢复是证据，不能自动撤销已提交合同或绕过裁决；已解决案件的新事实走关联的新程序。

**救济 admission 保护。** 每个 case 仅有一个有界、版本化 `RemedyAdmissionGateV1`，由既有受影响权利人集合、当前 case/主体请求计数和正式入口 availability 共同决定。集合/可用性未知，或 case 配额、其中任一受影响权利人的主体配额已满，均为 `closed`；拒收的请求不另建无界 stay。接纳最后可用名额、终止请求、权利人集合或服务事实变化的同一根事务更新 gate，不能等下一位合法请求被拒后才临时记保护。任一不可逆处分、首次 dispatch 和扩权交接须在根提交时重验当前 gate/version 及其来源，即使手动发起也不能越过；有实际容量的救济请求及原请求查询/补证/裁决仍可按各自条件处理。首版 gate 保守阻断本 case 的上述新效果，不冻结其他组织。

按 game 政策，关门时对仍有效且因入口受阻的程序性救济窗口保存剩余量与 `closed_at_C`；重新开放只按已记录关门区间调整这些未来截止，同一窗口不重复累计重叠区间。`C` 继续保持唯一、单调，操作 grant 与关门前已到期窗口的截止均不变。容量释放不自动支持申诉或重发旧动作；已合法取得派发许可的固定在途履约继续依 §7.1 对账。

<a id="des-wr-oc-003"></a>
### DES-WR-OC-003：范围唯一与原子授权交接

Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268；声明：[REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003)；资格与顺位：[接替政策](../../game/gameplay/gameplay-organization-continuity-policy.prd.md#oc-policy-succession)。

授权范围由可比较的权威对象/权利/动作集合表示，不能用客户端自由文本判断重叠；范围解析或集合完整性未知时阻断。每个组织、对象及受限动作的重叠范围最多一个可执行授权；不重叠范围可继续原授权。

`ScopeAtomV1 = (world_id, organization_id, asset_ref, right_kind, action_kind, unit_slice, valid_from_C, valid_until_C)`；`case_id` 记录来源但不作为隔离重复处分的屏障。`asset_ref` 使用原资产 authority 的稳定类型/ID/lineage；`right_kind` 首版为 `custody / maintenance / operation / disposition`，实际动作只能来自该权利经原 owner 确认的白名单。`unit_slice` 为不可分对象的 `whole`，或原 owner 已登记可分 lot 的半开区间 `[start,end)`；不得用小数百分比、通配符、未解析组或客户端文本展开资产。

**重叠算法。** 先按原 owner 的 lot/对象关系归一化 scope，再比较同一世界与资产 lineage、相交量/不可分对象和相交有效时间。不可逆 disposition 与该单元的一切写权利冲突；首版单 Agent 控制和单设施写入/维护均视为互斥，不能以 action 名不同规避唯一性。查询不占独占权；共享调度或不同动作真正兼容的扩展须由新版本冲突表明确，不在 v1 猜测。唯一索引和价值占用按资产 lineage 跨 case 检查，防止同一资产被关联组织或另一个 case 再授权。

交接请求绑定旧授权、旧范围版本、新授权来源、新范围、理由、有效期和预期 case 版本。确认前新授权仅待决。提交时重验新旧授权、时间、资格、裁决、冻结范围及全部相关对象版本，扩权交接还须满足当前 remedy admission gate；在同一事务中终止/收缩旧范围、激活新范围并记录唯一交接 receipt。冲突或任一写失败时全部不变。部分交接保留未交接范围及同一历史关联，不能创建第二套可执行影子授权。

旧、新授权执行、交接、撤销、reclaim 与申诉均按权威提交序重验：先提交的合法效果保留，后到操作基于更新后的范围与义务判断。交接不授予越过 pending 裁决或资产占用的优先权。旧请求保留原授权绑定，失效范围内拒绝或保持不可执行待决；新主体必须显式重提新请求并关联原请求，不继承权限、排位或效果。

**候选、部分交接与退出。** 候选表冻结本轮来源、资格证据、政策排序与接受意愿；推荐/申请不创建授权，任命者仍需独立的既有 charter 权限。提交 `CommitHandoff` 时重验任命证明、当前候选资格、剩余预算、scope root 和 revocation generation。可分 lot 在原 owner 同事务拆分后才能转交子范围；不可分 Agent、设施、合同包或共享在制工序只能整体交接，或保持旧授权至阻塞结束。在途外部效果已经占用的范围先完成/取消并对账，不与新执行权并行。非重叠范围保留原历史与授权期限；撤销/到期不自动续期、不继承待决请求。

<a id="des-wr-oc-004"></a>
### DES-WR-OC-004：幂等、恢复与结果投影

Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a、O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3、O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268；声明：[REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001)、[REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002)、[REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003)。

请求身份在组织/case 与请求者上下文内唯一，内容摘要固定；同身份同内容查询/返回原结果，内容不同拒绝。跨请求 ID 的重复处分还由权威义务/资产来源身份、effect kind、已履行量及活跃占用约束；不能通过换 ID 或授权再次处分同一份价值。真正的新义务必须具有新权威来源，不能仅靠客户端声称。

相同权利人、scope、事实来源和请求类型最多一个活跃 reclaim/appeal；补证增加原请求 revision，不重设最初窗口或申请排位。`effect_key` 至少绑定原业务/义务来源、效果种类、资产 lineage、量切片和既有阶段原因，不能只绑定新请求 ID。每个 origin 保留 `reserved / fulfilled / returned / disposed` 的合法互斥关系及累计量；形成、拆分或转移 lot 时将消费来源一起传递。新 case 或新授权仍查询同一全局来源索引。

响应超时意味着结果未知。先查询原请求的权威 journal；未提交可以在原身份且授权仍有效时重试，已提交只返回原 receipt。恢复同时读取 case、基础阶段、政策版本、授权范围/撤销、义务占用、资产状态、请求/effect 与 receipt；缺依赖、日志损坏或来源陈旧时保持 blocked，禁止猜测重放。重连仅查询，不自动重提或将旧请求移到新授权。

投影保留基础阶段与请求状态两个维度，并提供原因、范围、授权来源、保留价值/义务、来源世界版本、receipt 及 authority 支持的下一步。accepted 仅表示请求登记；applied 需要 effect receipt；persisted 需要恢复可回读；finality 与跨节点可用性引用 P2P 合同。未知/陈旧不能显示为恢复完成、资产返还或新 owner 已接管。

`ReadCase` 必须能回读：基础阶段/触发事实、`C` 与暂停原因/各窗口剩余量、完整清单版本与被隔离的部分、前序义务阻塞、候选顺位及跳过理由、新旧授权的有效范围与期限、reclaim/appeal 的独立状态和 stay、remedy admission gate 的状态/版本/原因与程序窗口顺延、逐笔处置量/receipt、当前能继续/补证/等待/退出的动作。公开面只给必要组织/权利摘要，申请证据和个人信息按权利范围访问；Viewer 与 Agent/API 消费相同结果，不各自重算时间、资格或清偿。分页始终绑定同一 source root，不能把未读下一页当成不存在债务。

## 6. 接口与数据合同

以下为 `OrganizationContinuityV1` 目标逻辑合同，版本固定为 `1`，不宣称现有 wire 已有这些字段。通过既有鉴权 world action / service adapter 接入；native、builtin 与后续模块消费同一验证器，不给客户端或运营工具开放 JSON patch 入口。

### 6.1 身份、字段和状态

共用 `world_id`、原资产/主体/receipt 的稳定身份；新增 `case_id = (organization_id, case_sequence)`，sequence 由权威事务分配且永不复用。外部请求 ID 为非空、最多 128 字节的 UTF-8 不透明值，在 `(world_id, case_id, requester_subject)` 内唯一。版本和 `C` 使用无符号 64 位整数；金额/物料量使用原资源最小单位的 checked 无符号整数，逻辑上限为 128 位；摘要使用当前 runtime 的 canonical commitment 类型，不引入客户端自选 hash 算法。

当前首版 adapter 只启用下列可证明的子集。组织连续性须由 `AllianceState.charter` 中严格解析的 `OrganizationContinuityCharterV1` 启用；schema、继任者/维护者名单、任命 authority、救济裁决者、活动源策略/清单和剩余份额都参与内容 hash。启用还要求 founder 确认及已有通过的治理 proposal；没有有效治理记录、字段不认识或旧 charter 不是该 typed schema 时保持未启用。启用时冻结 founder、全部成员及 charter 指定继任者、维护者、任命 authority、救济裁决者、剩余分配对象的 controller root/player/source/generation。每个特权写入按同一身份解析器重验执行者的冻结绑定；来源缺失或主体已重绑时拒绝，必须经新治理重新启用/授权，Agent ID 相同不转移旧权限。不得从旧 `last_active`、旧联盟标题或固定治理权重推定授权。

可用性证据由既有 `governance_finality_signer_registry` 与 node identity bindings 验证。签名 payload 绑定 world/genesis manifest、组织 registration、charter、覆盖 tick 区间和三项服务可用性；registry 的不同 node 必须有不同解码公钥、正 stake 与 `1..=10000` 的 stake threshold。每个已提交 tick 先处理本 tick 的有效签名证据，再由 runtime-only action `FinalizeAvailabilityUnknown` 封存未覆盖尾段：cursor 前进、暂停原因可读、`C` 不增加。后来的证书只能覆盖 cursor 之后的区间，不能把已封存未知时段改成可用，也不能追溯补算授权期限。成员历史从显式启用 tick 初始化，之后只由权威 Alliance join/leave 事件推进；资格只查询该连续性版本的历史，不导入旧历史。

组织活动覆盖水位与 availability cursor 相互独立。首版只采用上文定义的已登记 AtomicExchange catalog 和 settlement receipt prepared delta；不得以 `RecipeCompleted`、成员库存变动或普通 activity 自报替代。Runtime 在完整 reducer 阶段后提交的 system-only tick close 是推进 coverage 的唯一 producer；close 游标和 hash 持久化并随普通确定性 replay 恢复。catalog 空、source 未登记或发生 coverage gap 都保持自动沉寂阻断；客户端不能提交 coverage 标记。

首版 material estate adapter 只接受现有 `DepositOrganizationEstate` 的 canonical receipt，并以同一次 prepared publication 更新 manifest root、来源材料账与 asset record；held-in-trust 资产始终标记原 contributor。其他业务合同、订单预留、外部服务额度、个人资产、租赁物和未知权属均不能由组织清单推定为空或纳入处分。当前可实际执行的 material effect 仅为把完整 held-in-trust 项返还给记录的 contributor：原 appointment authority 必须处于 `Disposition/CustodyReturn` 且通过当前治理；有效 successor handoff 只能对其明确 scope 中的该类资产，在 `Protected` 或 `Disposition/CustodyReturn`、`C < expires_at_C`、资格仍有效且没有重叠 stay/quarantine 时执行。候选接受时冻结控制 root、player、source 和 generation；换控制来源后旧 grant 立即不可用。已生效重叠 handoff 阻止旧任命者执行，部分救济只撤重叠 scope；未知的合同/清偿/一般维护或处分动作保持拒绝，不从有限 handoff 推导出来。

`OpenContinuityCase` 在尚未分配 case ID 时，以 `(world_id, organization_id, requester_subject, request_id)` 去重，并在同一根提交中保存到新 case ID 的映射；响应丢失只查此映射，不能再次分配 case sequence。

| 记录 | 必备字段与 producer | consumer / 不变量 |
| --- | --- | --- |
| `ContinuityPolicyRef` | game 发布的 `policy_id/version/hash`，现有治理的 activation receipt，适用世界与生效点，时间参数/活动白名单/资格顺序/分配规则 | runtime 固定消费同一版本；未激活、未知或 hash 不匹配不能进入新业务，旧 case 不自动换版本 |
| `OrganizationCaseV1` | runtime：组织稳定 ID、sequence、trigger/source receipt、policy ref、charter authority ref、基础阶段、处置阶段、case version、manifest root、各窗口的 C、关闭原因/前序 case | 文本 charter 不转成权限；`base_phase` 与处置阶段、请求状态分开；actor 不能用新 case 绕过同资产占用 |
| `EstateManifestV1` | 原资产/合同 authority 提供完整性证明；runtime 固定对象、权属/负担、最小单位、分类、scope root 和版本 | 不把未知集合当空；变更追加新版本与通知，资产删除不消灭未结责任 |
| `ContinuityGrantV1` | 任命 authority：grant ID、授予者/接替人、来源决定、canonical scope、有效窗口、预算 ref、revocation generation、新旧关联与理由 | `proposed/active/revoked/expired/exhausted` 独立持久化；有限权限不隐含 dispose、再委托或业务准入 |
| `ContinuityRequestV1` | 请求者：ID、kind、content hash、提交时 grant/version、expected case/manifest/scope versions、权利与证据 refs、deadline C、supersedes ref（可选） | 旧请求保留原 grant；同 ID 异内容拒绝；`supersedes` 只表因果，不继承授权、排位或效果 |
| `RemedyRecordV1` | runtime：`reclaim/appeal`、申请人/权利来源、目标 receipt、争议 scope、证据 revision、补证/复查 C、独立 stay、裁决者及冲突检查、结果/支持范围 | 每项 stay 有原请求来源；部分支持保留未决部分；注册/复查超时均不等于支持 |
| `RemedyAdmissionGateV1` | runtime：case ID、version、受影响权利人 root、admission 计数 root、availability ref、`open/closed`、原因、关门/重开 C、原程序窗口调整记录 root | 每 case 一个 gate，由既有有界记录派生；所有新不可逆效果共同消费，不因拒收另建 stay；关门/窗口顺延与来源同根持久化 |
| `DispositionLedgerV1` | 原业务 authority + runtime：义务/asset lineage、总量、占用/已履行/返还/处分量、优先级/受益人、前序 receipt、计划 root、effect key | 同一来源跨请求/授权/case 共享；各合法余额与互斥终态守恒，不以换请求 ID 产生新来源 |
| `ContinuityEffectV1` | runtime：effect key、原请求/授权、origin refs、scope/量/目标、前置 receipts、资源版本、outbox 状态、结算 receipt | 本地效果与 journal 同事务；外部 dispatch/ack 只针对已提交记录，重放不执行外部调用 |

基础阶段使用 `Operating / Notified / Protected / Restricted / Resolved`；未开案正常组织可只投影 `Operating`。处置阶段使用 `Inventory / Obligations / CustodyReturn / Claims / BusinessDisposition / Residual / Complete`。请求状态使用 `Registered / AwaitingEvidence / UnderReview / Ready / EffectPending / Applied / PartiallyApplied / Rejected / Expired / Withdrawn`：前三者都没有资产效果；`Applied/PartiallyApplied` 必须绑定实际决定及其效果 receipt，不能仅凭 request acceptance 升阶。`result_unknown` 是响应/读取不确定性，不是可假定无效果的请求终态。

### 6.2 命令、响应和错误

| 接口 | producer → consumer | identity / version | ordering / idempotency | success / error / compatibility |
| --- | --- | --- | --- | --- |
| `InspectContinuity / ReadCase / ReadRequest` | 玩家/Agent → world service 只读 | 组织/case/request、expected source root（可选）、页游标 | 无预留、无续权、无自动提交；分页同 root | 带 source root/C/receipt 的快照；访问证据按权利限制；陈旧/未知显式返回 |
| `OpenContinuityCase` | 既有 charter 有权代表，或依据政策的权威不活跃判定 → runtime | trigger、来源事实、policy/charter refs、expected organization version、请求 ID | 当前 tick/活动/治理事实重验后登记一次 | case/registration receipt；登记不等于风险冻结；仅成员身份不足以清算 |
| `PublishContinuityNotice / AdvanceContinuityPhase` | 正式通知投影/政策执行者 → runtime | case/version、通知完整性/可达范围与 source receipt、expected phase、C | 通知完整后启动窗口；推进在提交点重验全部前提 | notice 或 phase receipt；不可用/未到期/待决阻塞保持原阶段 |
| `SubmitSuccessionCandidate` | 自愿候选 → runtime | candidate/request、能力/成员资格 refs、接受范围、budget ref、round/version | 同轮冻结比较，同 ID 同内容返回原记录 | 只登记候选；排序不发授权，不收取未批准资金 |
| `CommitHandoff / RevokeContinuityGrant` | 既有任命/撤权 authority → runtime | 新旧 grant、canonical scope/root、资格/决定 proof、expected versions、期限 | 唯一索引、占用、stay 和 C 重验；扩权须重验 admission gate；范围收缩/激活/receipt 同一提交 | handoff/revocation receipt；冲突/不可分范围/过期原子拒绝 |
| `PrepareDisposition` | 有权主体 → runtime 只读 | case/manifest root、拟处置范围、contract/asset refs | 只读 quote/计划，不预留资格、对价或额度 | 可执行步骤/成本/阻塞；无可核验权属时不可执行 |
| `CommitDisposition` | 有具体处分 authority 的主体 → runtime 与原 asset/contract owner | plan hash、origin/effect key、量、对象版本、前序 receipts、scope/grant、admission gate version | 当前 gate 及来源重验；typed delta 原子消费/保留；跨 ID 查询来源账 | 唯一 effect receipt 或待外部结算；无原 owner 适配能力则拒绝该类效果 |
| `SubmitRemedy / AmendRemedyEvidence` | 组织合法代表或有具体受影响权利来源者 → runtime | kind、目标 receipt/scope、right/evidence refs、request/revision、C | 先验权利与范围；补证不重设窗口；相同事实去重 | 注册/补证/受理/拒绝与专属 stay receipt；不发控制权 |
| `ResolveRemedy` | 既有无利益冲突裁决 authority → runtime | remedy/revision、裁决来源/理由、支持scope、义务处理、expected versions | 按当前权利状态裁决；支持后的撤权/恢复与责任同事务 | 支持/部分支持/拒绝 receipt；未知资格/利益冲突/旧版本保持待决 |
| `WithdrawContinuityRequest` | 原请求者 → runtime | request/content hash、expected revision | 与执行竞争同一序；未提交效果才能撤回 | 撤回 receipt；已确认结果返回原结果和后续救济，不能抹除 |
| `ReconcileContinuityEffect` | 原外部效果 adapter → runtime | effect key、原目标、dispatch/ack 身份、结算 proof、expected ledger version | 对原 effect 查询；重复 ack 无新增效果 | settled/cancelled/仍未知；未知不释放占用、不改 ID 重投 |

所有写命令共享 `schema_version / world_id / organization_id / requester / request_id / content_hash / expected_versions / authority_refs`；命令子类型必须拒绝未知字段所引入的执行能力，不能把未识别 scope 默认为全部。正式服务 adapter 负责原有身份校验，runtime 在提交时独立验证授权来源、范围、撤销与期满。证据采用有界权威引用，不把用户提交的任意 URL 内容直接当作授权。

错误至少区分 `policy_unsupported / upgrade_required / stale_version / notice_unavailable / clock_paused / qualification_failed / authority_expired / scope_unknown / scope_conflict / indivisible_scope / obligation_blocked / asset_already_disposed / idempotency_conflict / evidence_unavailable / limit_exceeded / remedy_admission_blocked / result_unknown`。响应包含命令身份、case/request version、当前 source root、`registration_receipt` 和适用的 `effect_receipt`、稳定错误原因及真实 `next_action / next_recheck_C`。超限在接纳前拒绝，不截断集合继续执行；容量关门保护来自已持久化 gate 和当前来源校验，不要求为被拒请求创建记录。基础设施/结果未知只能先查原请求，客户端不能据 HTTP 超时假定未提交。

## 7. 状态、事务与持久化

基础阶段、请求状态、授权有效性和处置进度分别持久化，不能用一个枚举合并。请求为待验证/已登记/待决/已提交或拒绝/过期/撤回；撤回必须与执行竞争同一权威序，已提交不能被撤回抹除。处置进度以各义务的确认结果判断，不靠游标前进假定结清。

每步 effect 将相关资源与义务变化、case 版本、授权范围变化（适用时）、幂等结果及 receipt 纳入同一提交边界。故障前无提交则全部不变；提交后响应丢失返回原结果。checkpoint 与 journal 一致，重放使用原政策和事实身份，不因新政策重算既有结果。

### 7.1 根事务与外部 effect

根事务读取 case/manifest/grant/revocation、政策/C、remedy admission gate 及其计数/可用性来源、资产 owner/合同版本、来源账与 world base head，构造 `PreparedOrganizationContinuityDelta`。所有权限、占用、容量、checked 计算、序号、receipt 和持久化准备先完成，随后与既有 typed publication 一次安装。任一基线变化或 precommit 失败丢弃全部候选；稳定拒绝/故障 disposition 仍遵守现有根事务合同，不在失败后另追加一次有副作用的审计。当前基础只提供复用位置，组织数据投影和 durable outbox 接线仍是实现义务。

长期物流、外部付款和跨服务结算采用已提交 outbox，不能包装为跨时间原子事务：

1. 预留实际资产/义务量、写入确定性 effect key 与 `dispatch_ready` outbox、更新 case/journal/receipt，在同一根提交中完成；此 receipt 只证明排入效果，不能表示已交付或结清。
2. adapter 首次派发前通过内部 `BeginContinuityDispatch` 根事务，把 `dispatch_ready` 条件转换成 `inflight` 并取得唯一 dispatch permit。该提交重验当前 grant、scope、C、revocation generation、stay、remedy admission gate/version 及来源、资源占用和 effect revision，与撤销/交接/容量关门竞争同一权威序；`C == expires_at_C` 拒绝，outbox 已登记不豁免重验。许可绑定原 effect key、目标、内容摘要与固定原业务合同；并发 worker 只能取得/查询同一许可，不能创建第二次效果。
3. 只有已合法取得首次 dispatch permit 的记录才可外发，之后未获回执为 `inflight/uncertain`，保留占用。外部接收方须支持持久幂等键、结果查询和明确取消/完成状态，缺项时 v1 不启用该不可逆 adapter。若撤权/期满先发生，尚为 `dispatch_ready` 的项停止首次发送，转为可核验取消或原业务安全终结；不偷偷迁移到新 grant。若许可先合法提交，后续重连或重试只沿原 key/固定合同查询或重投，授权失效不抹除这笔既有在途责任。未派发可取消项先取得取消证明，已派发项等待完成/取消对账，期间不开放重叠新执行。
4. 接收匹配原 origin、目标、量与权限边界的 ack 后，用另一根事务同时登记 ack、推进履行量、释放对应占用并写结算 receipt；重复 ack 返回原结果。阶段推进只消费该确认结果，不消费发送成功或网络超时。
5. snapshot/replay 只重建 outbox 和结果，不执行网络调用。恢复对已派发未确认项查询原目标；结果不可核验时停在未知，不能退款、分剩余或发第二个 effect。

| 故障/竞争点 | 必须保留的结果 | 后续动作 |
| --- | --- | --- |
| 冻结/交接/扣款准备失败，确认未提交 | scope、授权、资产、义务、序号与 receipt 均无部分业务变化 | 修正明确原因后用原请求重试或按当前版本显式重提 |
| generation 替换结果未知 | 禁止恢复旧世界写能力，不宣称撤销成功 | 按原 generation/请求身份核对，确认后安装/返回原结果 |
| 已提交但响应丢失 | 原授权变更或效果唯一有效 | 查询原 journal/receipt，不重新分配来源 |
| outbox 发出后崩溃或 ack 丢失 | 原占用与 effect key 保留 | 查询/重投同 key；未知不释放 |
| outbox 登记后、首次 dispatch 许可前撤销或期满 | 未获得许可的项不得发送，原占用保留待安全终结 | 根事务重验拒绝，取消/结清后释放一次；不换 grant 重发 |
| 交接与旧请求、撤销、reclaim、到期同 tick | 按相同权威提交序重验；期限等值时旧 grant 无效 | 无旧请求自动迁移；已确认在途范围保持占用 |
| 结案后出现新证据或责任 | 原 receipt/身份/责任摘要保留 | 建关联的新请求/裁决，不能回滚既有第三方效果 |

### 7.2 快照、重放与保留

快照纳入 case、policy hash、C 及暂停状态、manifest、grant/scope 唯一索引、全部未决 remedy/stay、admission gate/计数/权利人 root 与程序窗口调整、来源占用/累计履行、请求去重、outbox/ack 与 receipt 关联，并由同一 canonical generation/root 覆盖。恢复先验证所有引用与互斥/守恒关系，再发布可读/可写状态；缺 policy、资产 lineage、原 effect 或授权证据时整体候选不发布，相关业务保持 blocked。恢复须按同一记录核对 gate 派生结果，缺失或来源陈旧不能当作 `open`；重连、重启和自动/手动入口消费同一 gate。

已解决或授权到期不触发记录删除。活跃世界保留最小不可复用的 case/资产来源/已处分量/effect key/receipt 摘要及必要关联；去重 tombstone 不设按时间删除的 TTL，不能在原资产仍可被再次请求时失去唯一性。完整证据 payload 与已不再参与恢复的历史细节可按既有[存储治理](runtime-storage-footprint-governance.prd.md)分层，但必须保留可验证来源及申诉所需证据，不把容量压力转换成隐式免债或重新处分许可。

## 8. 部署、安全与运行约束

嵌入既有 runtime 与权威日志，不新增管理员直接处分入口。治理确认与执行资格分开校验，组织多数、运营工具或有效签名均不能免除资产 owner/合同校验。最低公开投影与敏感申诉证据分离；内部证据按适用权限读取，审计可读不等于无界公开。

依赖故障只阻断相关范围的新效果，查询保留未知语义；无关组织和已确认义务按既有合同继续。预算限制见 §9；不得为通过配额而截断义务、默许超范围执行或丢弃 stay。正式救济的容量/可用性阻塞按 DES-WR-OC-002 的有界 case gate 持久化并由所有不可逆处分、首次 dispatch、扩权交接共同校验，禁止只停自动任务却保留手动旁路。资源失败不是权利人的补证失败；客户端可读阻塞原因、保留窗口与后续重试条件。操作和恢复步骤留在既有专业 runbook，不由本文授权部署。

## 9. 质量与容量

| 场景 / 环境与刺激 | 响应及指标 | 验证入口 | 当前范围 |
| --- | --- | --- | --- |
| 固定候选 runtime，乱序/重复/换 ID/重连 | 相同权威输入效果一致；重复资产效果为零 | [OC-VERIFY-001](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-001) / [003](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-003) | 计划，未执行 |
| 授权交接、撤销与恢复并发，提交故障 | 重叠可执行授权最多一个；失败零部分写 | [OC-VERIFY-003](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-003) | 计划，未执行 |
| checkpoint 恢复、响应丢失、依赖缺失 | committed 可查；未知无新处分；状态/资产/receipt 一致 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004) | 计划，未执行 |
| 最大允许集合及超限、证据超时 | 不截断后执行、不污染无关范围 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004) | 使用下述 v1 资源边界，尚未验证吞吐/延迟 |

v1 工程 admission 上限：单命令编码后 64 KiB、32 个证据引用、单次 mutation 最多 256 个规范化 scope atoms；单 case 的完整资产清单最多 65,536 项，以固定 root 分页；单主体同 case 最多 8 个活跃新请求、单 case 最多 256 个活跃新请求；读页最多 128 项。同 ID 查询/重试先查去重再查新请求配额。大 estate 可按互不重叠且无共享义务的批次推进，单次不能超限；不可分事务超过上限则明确不支持该操作，保持权利/资产原状，不切小事务制造假原子性。这些是首版安全容量选择，不是性能数据；QA 在临界值与 `limit+1` 验证零部分写，扩容须版本化并测量资源后调整。

请求配额的接纳/释放在同一根事务更新计数与 gate；到达上限时即记录关门，恢复合法可用容量和正式入口后才条件重开。未通过身份/范围/格式校验或超限而未接纳的输入不增加计数，也不凭自己创建新 stay。未知容量/可用性在高影响提交点按关门处理；不得根据旧缓存或忽略某个受影响权利人的主体配额而继续处分。

## 10. 兼容、迁移与回滚

本次不迁移运行态。启用时按 `OrganizationContinuityV1` 的显式 feature/schema 能力检查接入：先升级读取/恢复消费者，再登记政策与可用的资产事务 adapter，建立组织权属/charter 映射，最后在权威提交点启用新写入。能力声明、policy hash 与原 asset owner 不匹配即拒绝激活；不从空 `serde` 默认值推导组织资产或授权。

| 数据/消费者组合 | 兼容行为 |
| --- | --- |
| 旧联盟快照，尚未启用 v1 | 保留原成员/charter/历史及窄解散事实，投影 `continuity_not_enabled`；缺清单不视为空 estate |
| 旧联盟显式接入 v1 | 保留 `alliance_id` 与历史作为 organization stable identity，登记权属/授权后从启用 C 起观察；不得导入 Agent.last_active 推定先前不活跃 |
| 新 schema / 旧客户端 | 有能力协商时只读受限摘要；不认识 pending/保护语义的写请求返回 `upgrade_required`，不退回立即删除联盟 |
| 新 schema / 旧 writer 或恢复二进制 | 拒绝取得 writer lease/发布状态；不能忽略未知组织字段后计算旧 root |
| 历史 `AllianceDissolved` replay | 仍按历史事件语义保留原结果，不合成新 case、不补发退款或转移；v1 启用后的新解散 intent 走组织 case 合同 |
| 已写 v1 后禁用新入口 | 保留状态、查询、在途结算与救济 consumer，禁止新增高影响命令；必须继续识别旧 effect key 与占用 |

仅在兼容消费者就绪后启用新请求。新 schema 写入后，旧二进制不能忽略字段继续执行；禁用入口仍保留历史查询、在途义务及适用救济。回退至能读取已写状态的验证基线；不能回滚已提交转移，只能走新的有权纠错程序。无可兼容基线时停止相关执行并保存状态。

## 11. 验证设计与可追溯性

以下是未来实现的可判定验收计划，文档检查不证明运行通过。每次运行固定 source/tested tree、政策/快照、环境、输入顺序及前后资产/义务/授权/receipt；输出进入实现 PR 和实际测试产物。

### 11.1 验证映射表

| 上游要求与验收 | 本设计条款 | 独立 obligation | 验证方法 / candidate / layer | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001) Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | [DES-WR-OC-001](#des-wr-oc-001) | 处置顺序、独立权利与历史 | [OC-VERIFY-001](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-001)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | fixture 前后账本、失败差分、receipt | 外部合同实现、经济平衡 |
| [AC-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-001) Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | [DES-WR-OC-001](#des-wr-oc-001) | 处置顺序、独立权利与历史 | [OC-VERIFY-001](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-001)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | fixture 前后账本、失败差分、receipt | 外部合同实现、经济平衡 |
| [REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001) Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | [DES-WR-OC-004](#des-wr-oc-004) | 跨请求/恢复的来源守恒、去重及处置读面 | [组织连续性验收手册](../../testing/manual/organization-continuity-system-acceptance.manual.md)：OC-VERIFY-001 分批守恒与 OC-VERIFY-004 恢复矩阵；固定候选/政策，required + full；未执行 | 原来源/余额/占用/处置差分、重复效果计数和同一 receipt | 外部实际结算和长期样本未验证 |
| [REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | [DES-WR-OC-002](#des-wr-oc-002) | 阶段/请求层与恢复不越级 | [OC-VERIFY-002](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-002)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 场景断言、恢复对账、真实表面证据 | 通知渠道有效性及上线范围 |
| [REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | [004](#des-wr-oc-004) | 阶段/请求层与恢复不越级 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 场景断言、恢复对账、真实表面证据 | 通知渠道有效性及上线范围 |
| [AC-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | [DES-WR-OC-002](#des-wr-oc-002) | 阶段/请求层与恢复不越级 | [OC-VERIFY-002](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-002)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 场景断言、恢复对账、真实表面证据 | 通知渠道有效性及上线范围 |
| [AC-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | [004](#des-wr-oc-004) | 阶段/请求层与恢复不越级 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 场景断言、恢复对账、真实表面证据 | 通知渠道有效性及上线范围 |
| [REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | [DES-WR-OC-003](#des-wr-oc-003) | 唯一授权、原子交接和非重放 | [OC-VERIFY-003](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-003)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 顺序矩阵、故障注入、账本/receipt | 尚未实现专用执行与测试 |
| [REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | [004](#des-wr-oc-004) | 唯一授权、原子交接和非重放 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 顺序矩阵、故障注入、账本/receipt | 尚未实现专用执行与测试 |
| [AC-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | [DES-WR-OC-003](#des-wr-oc-003) | 唯一授权、原子交接和非重放 | [OC-VERIFY-003](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-003)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 顺序矩阵、故障注入、账本/receipt | 尚未实现专用执行与测试 |
| [AC-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | [004](#des-wr-oc-004) | 唯一授权、原子交接和非重放 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 顺序矩阵、故障注入、账本/receipt | 尚未实现专用执行与测试 |

## 12. 决策、长期风险与未决问题

| 决策 / 未决项 | owner 与理由 | 解除 / 复核触发 |
| --- | --- | --- |
| 基础阶段与请求层分开 | runtime / Viewer；避免申诉被误读为控制恢复 | schema/API 实现时验证二维投影 |
| 分步处置并保留占用，拒绝跨时间假原子事务 | runtime / game；保障合同与价值连续，可能长期待决 | 每类外部 effect 获联合提交/终止/结算合同后才启用 |
| 对象/权利/动作/量切片定义重叠，首版不可分写权互斥 | runtime / P2P；避免同一资产被不同动作名或 case 绕过保护；会限制未证明可并行的场景 | 用已冻结 ScopeAtomV1 与部分交接/截止矩阵验收，未来扩展兼容表须独立证明安全 |
| v1 活动、时间、资格、债权与剩余政策由 game 单一 authority 提供 | game / 产品 owner；本 PR 提出首版选择，不冒充历史批准；保全优先可能造成较长等待 | 有真实组织活跃/恢复/清偿样本后复核周期和候选覆盖，不追溯重算在途 case |
| 新逻辑 schema、工程 admission 与旧数据边界明确 | runtime / QA；限量和 fail closed 可保护完整性，但大不可分 estate 或缺 adapter 的业务会被阻塞 | 实现 OrganizationContinuityV1，按 OC-VERIFY-004 验证容量、旧消费者和持久化；扩容不截断义务 |
| 外部 effect 必须有首次 dispatch 许可及持久幂等/查询/终态 | 原业务 owner / runtime；outbox 登记不保证仍有执行权，未知可能长期占用 | 对照派发前失权与派发后 ack 丢失，缺原业务 adapter 保持该类效果未启用 |
| 完整敏感证据分层，最小责任和去重摘要持续保留 | runtime / QA；节省 payload 不得重新开放已处分来源或使申诉无证可查 | 沿现有存储治理验证压缩/恢复，未证明可删除的依赖继续保留 |

本文不改变当前权限、共识或持久化实现。未来改变这些边界时，按[开发流程规范](../../engineering/workflow/source-of-truth.md)安排对应能力的独立评审；逻辑设计和计划不能代替该评审、实际验证或发行判断。
