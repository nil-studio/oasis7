# 紧急保供授权、冻结分配与结算系统设计

- 设计 ID：`DES-WR-ES`
- 状态：`active`（首版目标合同；不是已实现、已启用或发布声明）
- Owner role：`runtime_engineer`
- 专业职责：`gameplay_designer` 拥有 [GAME-018 首版政策](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#emergency-supply-policy-v1)；`blockchain_ops_engineer` 提供既有治理授权与分布式验证；`viewer_engineer` 提供结果投影；`qa_engineer` 验证组合边界。
- 上游产品：[常态市场与有界紧急保供](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001)
- 审读日期：2026-10-19；实现审读基线：`nil-studio/oasis7@e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`。
- 本次设计冻结政策消费、逻辑字段、命令、状态、错误与兼容规则，供 #4866 实施；类型名称为目标接口，并不表示仓库已有这些 Rust 类型、wire 端点或业务测试。

Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1, O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7, O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1, O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3

## 1. 问题、目标与非目标

保供必须把有证据的危机、治理授权、需求来源、真实物资、有限资金和最终接收连成一个可恢复合同。通用危机状态、普通市场报价与通用原子发布底座不能代替业务状态。目标是任何时刻都能解释“谁根据哪项授权，尚缺多少，哪些货物和资金正承担义务，完成或退出后能否再申请”。

本设计执行 GAME-018 §6.4 的首版政策，不另定阈值或配给数值；不重做普通市场、制造、物流、OC、治理或共识底座；不启用新服务，不把文档校验当成运行验证。首版无 lottery，不需要 seed；未知政策或不支持的 effect 必须拒绝，不能选择临时替代算法。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001) Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | 仅完整、有效且权威确认的危机授权可进入紧急执行；缺项不产生部分效果。 | [DES-WR-ES-001](#des-wr-es-001) | GAME-018 §6.4 的首版危机政策；blockchain_ops_engineer 的授权合同 | 消费 GAME-018 首版阈值；不改既有治理签名格式。 |
| [AC-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-001) Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | 仅完整、有效且权威确认的危机授权可进入紧急执行；缺项不产生部分效果。 | [DES-WR-ES-001](#des-wr-es-001) | GAME-018 §6.4 的首版危机政策；blockchain_ops_engineer 的授权合同 | 消费 GAME-018 首版阈值；不改既有治理签名格式。 |
| [REQ-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-002) Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7 | 冻结资格、需求、范围、资源、政策；等价重排序与重复投递保持结果；漂移原子拒绝。 | [DES-WR-ES-002](#des-wr-es-002) | GAME-018 §6.4 的固定分配政策；runtime_engineer 的提交与资源合同 | 执行 GAME-018 确定性比例/余数规则；首版不使用 lottery。 |
| [AC-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-002) Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7 | 冻结资格、需求、范围、资源、政策；等价重排序与重复投递保持结果；漂移原子拒绝。 | [DES-WR-ES-002](#des-wr-es-002) | GAME-018 §6.4 的固定分配政策；runtime_engineer 的提交与资源合同 | 执行 GAME-018 确定性比例/余数规则；首版不使用 lottery。 |
| [REQ-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-003) Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1 | 到期、撤销与续期不迁移旧请求；保留历史且同一需求不重复获益。 | [DES-WR-ES-003](#des-wr-es-003) | runtime_engineer 的时间与恢复；GAME-018 §6.4 的在途处置合同 | 不以新授权修改既有交付或所有权。 |
| [AC-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-003) Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1 | 到期、撤销与续期不迁移旧请求；保留历史且同一需求不重复获益。 | [DES-WR-ES-003](#des-wr-es-003) | runtime_engineer 的时间与恢复；GAME-018 §6.4 的在途处置合同 | 不以新授权修改既有交付或所有权。 |
| [REQ-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004) Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3 | 介入限于声明范围，资格不可交易；交付、存储、结算与 receipt 不可旁路，结果有真实下一步。 | [DES-WR-ES-004](#des-wr-es-004) | runtime_engineer 的物流/结算；viewer_engineer 的投影 | 不代签正式 surface、跨节点与发布验证。 |
| [AC-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-004) Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3 | 介入限于声明范围，资格不可交易；交付、存储、结算与 receipt 不可旁路，结果有真实下一步。 | [DES-WR-ES-004](#des-wr-es-004) | runtime_engineer 的物流/结算；viewer_engineer 的投影 | 不代签正式 surface、跨节点与发布验证。 |

<a id="es-implementation-baseline"></a>
## 3. 当前状态、目标状态与差距

以下是上述固定提交的仓库默认/native 与随仓 builtin 路径的静态审读，未运行 Rust、正式 surface 或真实网络，不覆盖未知线上自定义工件。

| 对象 | 当前基线与证据 | 目标状态与剩余实现 |
| --- | --- | --- |
| 普通市场/物流 | [报价](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy_support.rs)、[生产排程](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)、[运输](../../../crates/oasis7_runtime/src/world/logistics.rs)、[到达结算](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs)具有其原来的资源校验和事实链。 | 复用真实资源/物流条件；普通运输 receipt 不代表本业务需求已经履行。 |
| 通用危机/治理 | [CrisisState](../../../crates/oasis7_world_state/src/gameplay_state.rs) 只有普通生命周期；[native tick](../../../crates/oasis7_runtime/src/world/gameplay_loop.rs) 和 [builtin crisis](../../../crates/oasis7_builtin_wasm_modules/m5_gameplay_crisis_cycle/src/lib.rs)生产原危机事件；[治理入口](../../../crates/oasis7_runtime/src/world/governance.rs)有 brake/veto。 | 仍缺保供专用最小授权包、需求来源证明与冻结分配，不把这些现有能力升级解释为保供。 |
| 业务入口/持久状态 | [Action](../../../crates/oasis7_agent_api/src/world_action/action.rs)、[DomainEvent](../../../crates/oasis7_world_state/src/events/domain_event.rs)、[WorldState](../../../crates/oasis7_runtime/src/state.rs)、[bootstrap](../../../crates/oasis7_runtime/src/world/bootstrap_gameplay.rs)及 native/builtin producer 尚不承载本文授权/批次/累计需求/补偿状态。 | 实现 §§5–7 的类型、producer、reducer、receipt、投影及版本兼容。 |
| 原子发布/恢复 | [publication](../../../crates/oasis7_runtime/src/world/event_processing/publication.rs)已有 prepare → 单次安装，现有 economic/claim/industry 等域保留各自 prepared delta；[persistence](../../../crates/oasis7_runtime/src/world/persistence.rs)提供 snapshot/replay。 | 只接入保供业务 compound delta 与恢复依赖，不增加独立账本服务或重做 TransitionBuffer。 |
| 玩家与验收 | 无本设计专用分配/部分/退出读面及完整验收证据。 | 接通同源 Viewer/API/Agent，按手册分别证明各阶段；文档变更不会升级实现状态。 |

## 4. 边界与结构

| 生产者 / owner | 唯一责任与消费者 | 不具备的权力 |
| --- | --- | --- |
| 玩法政策与最低运行义务 authority | 版本化政策、危机前服务登记、主体聚合、无重叠消耗区间和真实需求来源，供证据验证与分配消费。 | 客户端或 LLM 不可自报需求、把普通 Crisis 严重度当证明或临时把设施标为公共关键服务。 |
| 既有治理 authority | 以现行权限、manifest、finality 证明签发作用域明确的授权/撤销/复核事实。 | 通过授权不能直接增库存/余额，brake/veto 不自动签发保供能力。 |
| 保供业务 reducer | 管理授权、批次、需求累计、承诺和结果，准备资源/资金/receipt 的一个 compound delta。 | 不拥有独立于世界提交的旁路余额；不以客户端到达顺序改变冻结分配。 |
| 资源/制造/物流/资金合同 | 共用资源版本和实际占用，提供库存、运力、存储、真实损耗、接收及 escrow 事实。 | 制造已预留/已消耗电力不可重复成为供给；路线上“到达”不替代接收与所有权结算。 |
| Viewer/API/Agent | 同一权威读面和受限命令请求，返回来源版本与真实下一步。 | 缓存、断线重试和叙述不产生资格，组织接替不能复制需求或继承原执行者的授权。 |

业务状态存放在现有权威世界状态/制度实例的保供命名空间，使用既有事件、state root 与持久化边界；不因本设计新增平行 Kernel 制度真值或数据库。具体模块承载沿用已激活 manifest 的制度边界，下面的 schema 与原子义务在任何承载方式下完全相同。

## 5. 关键运行流程

<a id="des-wr-es-001"></a>
### DES-WR-ES-001：证据、授权与有效期

1. 从危机前已成立的最低服务义务构造三份完整观测。`normal_coverage` 排除所有保供来源且扣除制造/普通合同占用，用于危机进入与恢复；`effective_coverage` 加入已履行和全部活跃承诺，仅用于计算还可分配的量。两种视图都绑定真实资源数量区间和来源，不从一个百分比反推事实。
2. 按 GAME-018 §6.4 验证品类、连续窗口、跨地点/控制主体范围、比率、登记成熟期和证据新鲜度；比例用加宽受检整数交叉相乘，不用浮点比较。授权包须同时有政策 hash、证据 roots、作用域、允许 effect、有效区间、公共资金账户/预算、供给规则、复核主体和申诉入口。
3. `AuthorizeEmergencySupplyV1` 仅由已有治理权限中明确覆盖 `emergency_supply.authorize` 的主体提出，并消费对本包完整 digest 生效的最终性证明。授权范围是该治理权限范围的子集；只能引用已有预算和自愿供给。签名通过而政策/需求证据缺失仍拒绝。复核主体须有 `emergency_supply.review` 范围且满足利益分离；没有合法既有授权来源则不启用，不创建默认管理员权限。
4. 作用域绑定危机前成熟登记的 canonical `region_id`、完整地点归属和 `region_budget_lineage`，不能由申请者圈选子区域。首版原子占用 `(world, region_id, resource_kind, distribution_window)` 的唯一新分配授权槽；同一槽不能让不同授权各收一部分地点另行分配。并发授权只有覆盖互不重叠的规范槽才可同时成立，退出中的旧承诺不占新资格。
5. 在同一原子提交中记录不可变授权包、区域槽、预算区间来源及授权未承诺额度的占用。区间上限扣除全部授权尚未承诺的额度与本区间累计已承诺金额，防止并发授权各自拿到完整上限；此时不付款、不预留物资，也不意味着玩家已获分配。分配提交把相同额度从“授权未承诺”原子转成“本区间已承诺”；退款不减少本区间累计已承诺金额，防止反复取消/续期重置开支上限。区域拆分/合并和换资金账户沿同一 `region_budget_lineage` 对账，缺 lineage 时阻断。
6. 每次登记、冻结、分配及出发都在权威提交序上校验当前授权：`starts_at <= now < expires_at`，未撤销、未进入恢复关闭、证据仍新鲜、manifest/政策兼容、权限与范围一致。到期直接由谓词成立，清理任务延迟不延长效力。已提交历史结果查询在当前权限允许时返回原 receipt，而非再次执行。

`RefreshEmergencyEvidenceV1` 只能追加下一完整观测，不能修改过去样本或政策。新样本显示连续恢复或证据过期时进入 `draining`，立即停止新资格/出发，剩余旧义务按 DES-WR-ES-003 处置。补交证据不复活已停止授权，需显式新授权。危机前公共义务取消或控制主体关系改变时，受影响新请求先停止/重验；不能在同一需求身份下扩量。

<a id="des-wr-es-002"></a>
### DES-WR-ES-002：冻结集合与确定性条件提交

1. `RegisterEmergencyNeedV1` 接受权威需求引用，不接受 caller 自带需求总额。绑定 `(world, service_obligation_id, resource_kind, service_window)` 的 `need_key`；它不包含 request、授权、代理执行者或可变组织控制人。义务来源必须能证明不重叠消耗区间、原主体及合法代理范围。`request_key` 只负责传输去重；隐藏旧关联、更换 ID 或新授权仍命中同一需求账本。
2. 收集窗口固定对齐世界 tick。登记/显式替换均需在关闭前进入权威日志；同一需求只留一份有效申请，旧请求保持可读的 `superseded` 处置。`FreezeEmergencyBatchV1` 在关闭点冻结完整去重集合及五类输入 roots、同一政策、供应报价、相关资源/资金/需求/权限 read set。到达边界及之后的登记返回 `window_closed`，不悄悄进入下一批。
3. 按专业政策的消耗区间、优先级、比例和余数执行纯整数分配。先聚合控制主体，再拆到原始义务；跨地点的共同供给先按同一规则形成地点份额，不能按 batch 提交先后来抢同一资源。整个区域窗口的地点分量作为一个 `BatchSnapshotV1` 原子提交；不能拆成彼此独立的地点写入。
4. 资源 adapter 从已冻结且可同时兑现的库存/报价、路线容量和目的地接收计划提供供给上限与每条数量区间。供给来源不是未来产能；无法证明全体拟分配量能同时走完已声明物理合同则本批 `resource_blocked`，不跳过某些请求偷偷改变公平结果。原批次只读不可变；重新报价使用新 batch revision 和当前快照，原成员须显式确认，关闭窗口之外不吸收新的成员。
5. `CommitEmergencyBatchV1` 比较授权状态、政策 hash、完整 read set、每个需求累计版本、底层库存/路线/目的地容量/账户/预算版本。只比较相关对象版本，无关 world 前进不使结果失效。并发制造、市场、其他授权、组织交接或同需求承诺占用任一相关对象时，全批原子拒绝 `snapshot_conflict`；不部分扣减或自动重算旧快照。
6. 成功的 compound delta 一次记录供应库存/运力/接收容量预留、足额 escrow、每需求新增承诺、固定分配向量、receipt 与幂等处置。相同请求同内容返回原结果；同键不同内容 `idempotency_conflict`。更换请求键也不得重用 `(batch_id, revision)` 或 `(need_key, commitment quantity interval)` 第二次提交。

冻结不锁库存、不扣钱；分配提交只是承诺。各批次均使用同一世界资源占用与需求账本，不能各自维护一份“可用资源”缓存。并发授权仅可占用不重叠的规范区域/窗口槽；预算区间上限、跨区域共享资源及同一原始需求仍联合 CAS。

<a id="des-wr-es-003"></a>
### DES-WR-ES-003：失效、部分履行、申诉与恢复

| 对象状态 | 到期 / 撤销 / 恢复关闭后的处理 | 线性化与幂等边界 |
| --- | --- | --- |
| `registered` / `frozen` | 原请求 `expired`，无新资源效果；未来新授权只能由玩家显式新提。 | 失效谓词先于分配检查；登记不能使旧请求转成已承诺。 |
| `committed` 且未 `dispatched` | 释放可核验的库存/路线/容量/escrow 和同一需求占用，保留原分配 receipt 与已释放量。 | 释放与出发争同一 commitment revision；只有一个成功。 |
| `dispatched` / `in_transit` | 按有效时已签订的原合同继续有限接收/结算或退回；不要求旧授权仍 active，但必须引用其已提交承诺与原范围、价格、截止、预算。 | 禁止任何追加量、展期或再获临时资格。续期不可替换旧合同。 |
| `arrived` / `partially_received` | 合法接收部分逐数量区间结算；未接收部分继续保管、有限等待、退回或真实损失终结。 | 最终接收/结算与取消/退回争同一未处置区间，不能一边释放需求一边交付。 |
| `custody_blocked` / `review_blocked` | 保留货物、责任、资金及对应需求占用，只开放原合同允许的补证、退回/修复或复核。 | 超时不生成退款、损失或需求释放；状态不可核验不得分配第二份价值。 |
| `settled` / `returned` / `lost` | 历史不可改写；关联纠错必须有新的裁决义务、合法余额及独立唯一 effect。 | 旧 receipt 可回读不可重放。真实退回/损失终态才能释放未履行占用。 |

`Q_required` 是同一原始服务义务及消耗区间的总需求；`Q_fulfilled` 记录各渠道已经最终履行的总量；`Q_committed` 只计所有未终止承诺的尚未履行量，一个承诺在采购、预留、分配、在途、待结算中只计一次。账本保持 `0 <= Q_fulfilled + Q_committed <= Q_required`。

新保供量还须扣除当前快照中已唯一归属该需求、未计入上述两项的常态可用覆盖，不把同一库存分给多个需求。需要减少最低义务至低于已有履行/承诺时，先处置承诺，不能直接下调总额破坏守恒。

最终结算原子做 `Q_committed -= q; Q_fulfilled += q`，释放只在对应资源/合同已经合法终止后做 `Q_committed -= q`，二者对同一 quantity interval 互斥。部分完成拆分不重叠区间，原 commitment 身份不变。新服务窗口具有新来源区间，仍扣除当时真实常态可用量；授权 renewal、改名、组织接替或新代理身份都不能重新签发旧 `need_key`。

申诉用 `OpenEmergencyReviewV1` 绑定自己的授权/需求/commitment/receipt、证据与被争议的未结算区间；提交申诉只登记，不自动冻结或恢复资格。`DecideEmergencyReviewV1` 需原包声明的独立复核权限、期限和证据；只允许确认原结果、冻结/取消未结算部分或签发有预算来源的纠错义务。依据缺失或复核超时保持可审计阻塞，无争议区间继续；一项申诉不得冻结整个常态市场。

主体或组织控制权交接后，需求/供货/补偿债务仍使用原 lineage 和累计量；新执行者需现行有界权限，不继承旧临时资格。未结物资、资金和赔偿占用不能进入组织剩余资产处置。恢复读取 authorization、need、batch、commitment、quantity disposition 和底层资源的同一一致性切点，未知关系先阻断该业务，不猜测 `0` 余额或重新派生新身份。

<a id="des-wr-es-004"></a>
### DES-WR-ES-004：物理结算、补偿与诚实读面

`procurement`、`allocation`、`rationing` 和 `compensation` 共用同一 `commitment_id`。采购或分配 receipt 只确认占用；`arrival` 只确认到达；`storage_acceptance` 确认接收与保管；`settlement` 才同时确认该数量区间的需求履行、受限使用权和 escrow 支付。物流到达、通用 tariff receipt、普通经济合同成功或危机解除都不能单独冒充最终保供。

供方款项按固定单位价和实际接收量计算，正向/退回运输只付原报价范围内已验证里程碑；相同成本项不能同时作为采购款和另一笔补偿领取。资金来自原承诺已锁定的专项 liquid OC，供应不足、费用超上限或 escrow 余额不足时阻断该步，不从 restricted starter 余额、未来 levy 或 token issuance 补足。资金不足可以使新批次只分配较少供给，不能截断已签旧承诺。

受益资源进入绑定需求、地点及消耗区间的受限使用记录，仅原最低义务可以消费；不能转售、换成自由库存或兑换现金。真正消耗与已有资源消耗使用同一版本/数量区间，避免“保供完成”和普通生产又消费同一份电力。已验收接收的定义必须包含可证明的路线、目的地、保管/存储和相应用途资格；没有相应电力适配器时保持 `unsupported_resource_path`，不通过普通材料 ID 或免费 `ResourceDelta` 代替。

权威查询返回两个维度：分配类别 `allocated | partial | denied | expired` 与履行阶段。`allocated` 表示该次申请分配额已全部承诺，`partial` 表示仅部分，二者均不表示完成；登记或冻结时类别为 `null`。`denied/expired` 只描述未取得的部分，不覆盖该请求已合法履行的数量。读面同时给出请求量、原分配量、当前承诺/在途/已接收/已履行/已释放/已损失量、当前还可申请量、原因、适用政策/优先依据、下一个合法动作及版本/receipt。已分配后释放也保留原分配类别与释放数量，不能改写历史成“从未分配”。

建议与结果消费保持同源：`query_original`、`wait_until_tick`、`submit_next_batch`、`source_on_market`、`provide_evidence`、`request_review`、`return_under_contract`、`no_safe_action` 仅在该主体有当前权限且接口实际支持时出现。缺 receipt 或来源落后显示 `unverified/stale`；网络超时先查询原身份，重连不自动重提。

## 6. 接口与数据合同

<a id="es-v1-data-contract"></a>
### 6.1 首版类型、身份与版本

下列名称和必填语义是目标 `schema_version = 1` 合同；后续 transport binding 可选择既有 typed action/API 入口，但不得改字段含义或放宽边界。摘要沿用世界既有 canonical 序列化/hash，所有必填字段和有序内容都进入摘要；代码落地需验证同源编码 golden vectors。标识与版本不是 Task/Trace ID。

| 对象 | 必填字段与唯一键 | 不变量与 owner |
| --- | --- | --- |
| `EmergencyCommandV1` | `world_ref, schema_version, actor_ref, request_key, payload_digest, expected_revisions, command`；幂等键为 `(world_ref, actor_ref, request_key)`。 | 身份引用消费既有 authority；字符串非空、至多 128 UTF-8 bytes，digest 为现有 32-byte hash，revision 为正 `u64`；同键异内容拒绝，旧键不因换授权重置。 |
| `EmergencyEvidenceV1` | `policy_ref, sample_ticks[3], sample_roots[3], obligation_root, controller_root, location_root, normal_coverage_root, effective_coverage_root, resource_proof_refs`。 | 每个样本相同世界、完整连续窗口，原始证据按内容寻址可取回；正常/救助 provenance 不可合并。最低义务 producer 证明登记成熟期与非重叠数量。 |
| `EmergencyMinimumObligationV1` | `world_ref, service_obligation_id, source_contract_ref, beneficiary_ref, location_ref, controller_ref, service_class, resource_kind, minimum_quantity_per_window, registered_at, valid_service_windows, source_revision, source_receipt`。 | producer 是危机前已成立的正常运行/公共服务合同，最小量来自该合同已接受的非零最低服务条款，按政策固定消耗窗口使用；该适配记录不凭空新增服务、资格或消耗。原 source/revision/receipt 缺失、与 profile 不一致或生效期不匹配均拒绝。玩家不能经保供 API 登记或调大最低量。 |
| `EmergencyAuthorizationV1` | `authorization_id, revision, policy_ref, evidence_ref, scope, effect_kinds, starts_at, expires_at, issuer_proof, budget_account, budget_period, budget_cap, reviewer_scope, review_entry, status`。 | `authorization_id` 绑定 world 与首次治理决定，包不可改写；revision 只记录证据刷新/撤销/关闭等追加事实，不修改范围/期限/价或预算。scope 显式包含 canonical region_id、region revision、region_budget_lineage、地点、资源、主体与义务集合 roots，缺集合不视为 all。 |
| `EmergencyBudgetWindowV1` | `(world_ref, region_budget_lineage, budget_period)`、`opening_balance_proof, period_cap, committed_total, uncommitted_authorization_allowances, revision`。 | 起点余额与 period cap 不可改写；`committed_total` 单调累计，不因退款/撤销/换资金账户减少；已承诺加各授权未承诺额度不超 period cap。跨授权、各底层账户余额与本表一起 CAS。 |
| `EmergencyRegionSlotV1` | `(world_ref, region_id, resource_kind, distribution_window)`、`authorization_id, batch_ref?, revision, allocation_closed`。 | 对应完整规范区域的唯一新分配槽；占槽/释放与授权状态及预算一同 CAS，旧在途只保留原 commitment。不得按授权 ID 切出平行区域批次。 |
| `EmergencyNeedRecordV1` | `need_key, service_obligation_ref, service_window, original_beneficiary, current_controller_ref, resource_kind, required_quantity, fulfilled_quantity, active_commitments, normal_coverage_refs, revision`。 | `need_key` 由 world/原义务/资源/消耗区间派生；账户/授权/代理改变不改 key。所有普通和紧急履行/占用共同核对，不接受 caller 设置累计量。 |
| `EmergencyNeedRequestV1` | `request_id, authorization_id, batch_window, need_refs, requested_maxima, replaces_request_id?, predecessor_request_ids, payload_digest, disposition`。 | 请求量只可收窄权威剩余量；lineage 由 runtime 补全，不能靠 caller 隐藏旧请求。替换保留全部已承诺/已履行记录，只替换尚未冻结的申请。 |
| `EmergencyBatchSnapshotV1` | `(world_ref, region_id, resource_kind, window, revision)`、`authorization_id, region_slot_ref, request_set, eligibility_root, need_root, scope_root, supply_root, policy_ref, quote_refs, fund_refs, read_set, allocation_vector, snapshot_digest, disposition`。 | 冻结内容不可变；新 revision 仅用于显式重评估并记录旧 revision 终止。read set 含所有被消费的权限/需求/库存/路线/容量/资金及共享预算版本，不能只比较全局高度。 |
| `EmergencySupplyOfferV1` | `offer_id, supplier_ref, resource_lot_ref, quantity, destination_ref, route_plan_ref, storage_ref, ready_at, delivery_deadline, unit_price, forward_fee_items, return_fee_cap, ordinary_price_evidence_ref?, valid_until, supplier_consent_proof, revision`。 | 来自真实可预留供给和普通物流/存储/资金合同的 adapter；零价捐入仍须真实资源和 consent，付费报价须参考价证据。完整可行供给计划显式包含共享路线/存储边的容量占用；报价不足以预留，直到 batch CAS 成功。 |
| `EmergencyCommitmentV1` | `commitment_id, batch_ref, need_key, supplier_ref, beneficiary_ref, resource_lot_ref, quantity_intervals, route_ref, storage_ref, quote_ref, escrow_ref, delivery_deadline, return_budget, revision, stage`。 | ID 由已提交 batch 与 need/lot 唯一关系派生；数量区间不重叠。资源与费用项引用真实预留，不创造副本；承诺拥有量、资金、保管责任及有限退出义务。 |
| `EmergencyEffectReceiptV1` | `effect_id, kind, commitment_id?, need_key?, quantity_interval?, prior_receipt_refs, payload_digest, world_commit_ref, policy_ref, disposition, quantity_delta, money_delta`。 | effect 键绑定业务对象/阶段/数量区间/费用项；不同 request 同业务 effect 仍只一次。拒绝为零经济 delta；每笔物资/资金记双方和来源。 |
| `EmergencyReviewV1` | `review_id, authorization_id, subject_scope, disputed_intervals, receipt_refs, opened_at, deadline, evidence_refs, reviewer_proof, decision, corrective_obligation_ref?`。 | 有权决定与申诉登记分离；纠错引用原事实，不覆盖原 receipt；新增补偿另有来源和唯一 effect。 |

数量使用既有资源最小单位的非负受检整数，金额为既有 liquid OC 的最小单位，不引入汇率或新精度；进入 `u64` 前校验既有有符号量非负，乘法用受检 `u128` 并显式窄化。`WorldTime` 与既有世界 tick 同型，禁止客户端 wall clock；时间/数量/累计溢出返回 `numeric_overflow` 且零部分变更，遵循[数值合同](runtime-numeric-safety.prd.md)。

### 6.2 命令、生产者与并发合同

| 接口 | producer → consumer | identity / version | ordering / idempotency | success / error | compatibility |
| --- | --- | --- | --- | --- | --- |
| `AuthorizeEmergencySupplyV1` / `RefreshEmergencyEvidenceV1` | 已有有界治理主体 / 证据 producer → runtime | 治理决定、包 digest、证据根、当前授权/预算 revision | 权威顺序；授权身份、预算区间唯一且联合 CAS | 授权/刷新 receipt 或缺项、证据、预算、权限拒绝 | 普通 brake/Crisis 不可转换为该调用。 |
| `RegisterEmergencyNeedV1` / `ReplaceEmergencyNeedRequestV1` | 受益主体或有界代理 → runtime | need refs、授权、批次窗口、旧请求 revision | 先校验查询权限再查幂等；同 need 多 request 不增加可分配量 | 登记/替换处置；已冻结、窗口关闭或来源无效拒绝 | 旧客户端无需求来源或 lineage 不能执行。 |
| `FreezeEmergencyBatchV1` / `CommitEmergencyBatchV1` | 权威 tick/确定评估器 → 世界 publication | 完整 batch digest 与 read set | 关闭窗口一次冻结；单批区域分量一次 CAS；重复 commit 返回原 receipt | 整批分配或 `snapshot_conflict/resource_blocked` | 不支持政策版本拒绝，不自动换算法。 |
| `DispatchEmergencyCommitmentV1` | 有原合同权限的供给/物流执行者 → runtime | 原 commitment、资源预留、路线和授权 revision | 与撤销/释放互斥；有效授权内出发一次 | 出发 receipt 或失效/资源冲突 | 通用在途记录不足以新建本业务承诺。 |
| `RecordEmergencyDeliveryV1` / `SettleEmergencyCommitmentV1` | 权威物流/存储 producer → runtime | 原承诺、接收证明、未处置数量区间、escrow/revision | 接收/支付/需求同事务；与退回/损失互斥 | 部分/最终结算 receipt 或物理/资金阻塞 | 失效授权仅可结清原已出发义务，不可增量。 |
| `ReleaseEmergencyCommitmentV1` / `ReturnEmergencyCommitmentV1` | 权威 tick 或原合同有界执行者 → runtime | 承诺、原出发/保管/退回证据、未处置区间 | 释放与最终结算同 CAS；重复释放无效果 | 释放/退回/待保管或拒绝 | 无证据不能把在途量视为可释放。 |
| `RevokeEmergencySupplyV1` / `OpenEmergencyReviewV1` / `DecideEmergencyReviewV1` | 原有撤销权限 / 受影响主体 / 独立复核者 → runtime | 原包、被争议范围、证据、decision revision | 与分配/结算在同一权威序；只冻结指定尚未结算部分 | 撤销/申诉/裁决 receipt，缺权无效果 | 不借此修改普通治理或无限冻结市场。 |
| `GetEmergencySupplyV1` / `GetEmergencyRequestV1` / `GetEmergencyNeedV1` / `GetEmergencyReceiptV1` | 权威投影 → Viewer/API/Agent | world/授权/请求/need/receipt + 来源 commit + 有界 cursor | 纯读；分页绑定同一快照，不补发命令 | 当前、archived、stale/unverified 或不可见 | 未支持能力标 `unsupported`，不填默认 success。 |

稳定错误集合为 `unsupported_version`、`unsupported_resource_path`、`missing_authority`、`scope_mismatch`、`region_slot_conflict`、`invalid_evidence`、`evidence_stale`、`authorization_inactive`、`need_source_unverifiable`、`need_exhausted`、`window_closed`、`batch_overflow`、`snapshot_conflict`、`resource_blocked`、`budget_exhausted`、`idempotency_conflict`、`commitment_terminal`、`settlement_blocked`、`review_blocked`、`numeric_overflow`、`recovery_incomplete`。业务拒绝随原请求保存可读处置，不产生经济效果；未进入权威登记的传输超时是未知结果，应查询原 key，不转换成新 request 自动执行。

## 7. 状态、事务与持久化

授权为 `proposed → scheduled → active → draining → closed`，另有 `rejected`；到期/撤销使有效性立即为假，即使物化状态尚未前进。`closed` 只有全部旧义务处置、未用预算解除且申诉终结后成立。批次为 `collecting → frozen → committed`，失败为 `rejected/expired/overflowed`，旧 revision 不可重新打开。请求与承诺的履行阶段独立，部分量按 quantity interval 记录，不能用单一 success 标志覆盖。

所有原子步骤复用 [publication](../../../crates/oasis7_runtime/src/world/event_processing/publication.rs) 的 pure prepare、完整事件绑定和单次 install：先准备授权/需求/承诺、真实库存、路线、存储、escrow、预算、受限使用、费用和 receipt 的完整后继，再算 prospective root 并发布。任一校验、算术、prepare、日志/共识记录安装失败，均保留原状态；不能先改库存后补写业务账本，也不使用事后退款模拟原子性。action preview、实际 apply 与 replay 消费同一业务 reducer。

分配、出发、分段接收和最终付款可以相隔多个 ticks，是多个分别原子的事务；每个阶段均携带原 commitment/quantity interval 和独立 effect id。对账至少满足：原货物 = 来源库存/预留 + 在途/保管 + 已接收/受限使用 + 真实损失；原锁款 = 剩余 escrow + 已支付 + 已退回；服务总需求 ≥ 已履行 + 活跃承诺。到期解除预算时只释放未承担任何旧义务的部分，旧 escrow 不得因新授权到来而重新可花。

checkpoint 包含政策 artifact 引用、授权状态及撤销序、冻结输入、需求来源与累计账本、承诺/区间处置、共享占用、escrow/预算、幂等结果、复核及 receipt；日志按同一 canonical commit 序恢复。已提交响应丢失返回原 receipt；仅 prepare 未提交的数据可丢弃；持久 pending 仅在原授权/合同仍允许时继续，不给它签发新授权或新 key。缺 artifact、原始需求、资源引用或 root 证明，停止相关保供恢复，不影响可独立验证的常态域。

`accepted` 只证明登记；`applied` 需世界 effect receipt；`persisted` 需同切点恢复可回读；跨节点最终性由既有 P2P 证明。历史保留消费[存储治理](runtime-storage-footprint-governance.prd.md)：申诉窗口结束、全部承诺/托管/纠错义务终结且 canonical replay watermark 越过后可归档详细输入，仍保留需求累计、终态 effect 摘要及防重放 tombstone。首版不按授权到期自动删除这些防重放事实；archive 取回失败返回 `archived/unverified`，不能重新分配。

## 8. 部署、安全与运行约束

嵌入现有世界执行，不新增持有生产密钥的旁路管理服务。治理与复核请求使用现行签名/权限/manifest 机制；运行时校验来源、范围、利益分离与资源责任，不复制一套独立签名阈值。普通守护者 brake 可阻断新动作，但不能凭 brake 权力签发物资或挪动 escrow。对他人私有库存、身份材料、报价证据的公开输出仅给解释所需汇总与承诺证明；本人/授权审计者才可读对应明细。

网络/证据不可达、落后节点、恢复不完整或计算资源耗尽只能形成明确阻塞，不按 wall clock 或线程先后决定谁获配给。证据、预算和资格失败不冻结无关常态市场；服务线程崩溃后以原身份恢复，不补发缺失的世界效果。正式 hosted、跨节点与浏览器环境沿用各自 manual/runbook，本次不产生部署证据。

## 9. 质量与容量

首版接口保护上限是设计值：每授权最多 `64` 地点；每批最多 `1024` 聚合受益主体、`4096` 需求记录及 `4096` 供给批次；单命令最多 `32 KiB`，单冻结/证据对象最多 `4 MiB`，大证据通过既有内容寻址引用，仍受既有更小传输上限约束；查询页最多 `100` 条。

达到登记容量后把整个相应窗口标为 `overflowed`，不把先到的前 N 条当成公平分配获胜者；可读拒绝与下一窗口保留，不截断冻结集合。实现的计算预算必须让同一输入在所有支持节点上得到同一成功/阻塞，超预算不得半算半提交。

| 场景 | 环境 / 刺激 | 预期响应 / 判定指标 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 公平与回放 | 同一政策与冻结集合，重排投递/Map 遍历/节点回放，控制主体拆多请求 | 每主体分配向量和 receipt 一致，分配总量不超冻结供给；隐藏顺序影响为零 | [ES-VERIFY-002](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-002) | 未来验收，未执行。 |
| 原子与守恒 | 普通制造/保供争资源、并发授权、预算冲突、提交故障、响应丢失 | 失败经济 delta 为零；每费用/数量区间一次；资源/需求/资金对账差为零 | [ES-VERIFY-002](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-002) | 未验证。 |
| 自动退出 | 边界 tick、撤销/出发/结算竞态，清理延迟，保供覆盖充足但常态未恢复 | 无新增临时效果；旧合同只按已承诺边界结清；不因救助造成自我恢复判定 | [ES-VERIFY-003](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-003) | 未验证。 |
| 容量与隔离 | 上述各上限及 +1，证据不可取/超预算/落后节点 | 整体明确阻塞、不按先来截断；无关市场继续；各支持节点结果一致 | [ES-VERIFY-004](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-004) | 上限为设计值，无已测吞吐或 SLA。 |

## 10. 兼容、迁移与回滚

首次启用须由既有 manifest/政策激活边界声明 `emergency-supply-v1` 与 artifact digest；实现需同步 typed action、producer/reducer、状态序列化、preview/apply、replay、投影及消费者 capability。schema 缺失只在可证明启用前的旧快照上解释为“未启用、空业务状态”；启用后的快照缺必需字段或未知版本要拒绝恢复，不能用 serde default 消除既有义务。

旧 Crisis、brake、普通交易队列和未知自定义工件不自动迁移成保供授权/需求。策略升级用新政策和新授权，旧承诺继续绑定原 artifact/规则；新客户端可以读旧结果但不得把它当新资格。缺新能力的客户端只允许其已支持的常态行为与诚实只读提示。

启用前须完成本合同的四组业务验收及既有真实网络/消费者验证，不把底座原子测试当作全部完成。回退优先禁用新授权/登记，让已出发义务按兼容代码排空；只有能证明无保供状态/义务遗留的启用前基线，或能够完整读取这些状态的兼容版本，才可接管。不可把含新 schema 的快照交给旧二进制忽略读取；不通过删状态、删 escrow 或回滚世界历史撤销已发生效果。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

表中场景是未来实现验收计划；本次文档检查不构成运行证据。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001) Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | [DES-WR-ES-001](#des-wr-es-001) | 按首版阈值、权限、预算及新鲜度合取验证；普通波动、缺项和越界无紧急效果。 | [ES-VERIFY-001](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-001)：固定候选的 runtime 状态与资源对账。 | 未来 PR 的测试输出、前后状态与 receipt。 | 尚无专用运行测试，不证明真实危机判据。 |
| [AC-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-001) Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | [DES-WR-ES-001](#des-wr-es-001) | 按首版阈值、权限、预算及新鲜度合取验证；普通波动、缺项和越界无紧急效果。 | [ES-VERIFY-001](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-001)：固定候选的 runtime 状态与资源对账。 | 未来 PR 的测试输出、前后状态与 receipt。 | 尚无专用运行测试，不证明真实危机判据。 |
| [REQ-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-002) Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7 | [DES-WR-ES-002](#des-wr-es-002) | 比例/余数分配、跨地点资源计划、冻结/CAS、乱序与换身份不改变公平或复制效果。 | [ES-VERIFY-002](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-002)：确定性 fixture、故障注入与 replay。 | 未来 PR/CI 的分配向量、资源差分与回放结果。 | 不证明经济平衡或规模性能；首版无生产随机性。 |
| [AC-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-002) Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7 | [DES-WR-ES-002](#des-wr-es-002) | 比例/余数分配、跨地点资源计划、冻结/CAS、乱序与换身份不改变公平或复制效果。 | [ES-VERIFY-002](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-002)：确定性 fixture、故障注入与 replay。 | 未来 PR/CI 的分配向量、资源差分与回放结果。 | 不证明经济平衡或规模性能；首版无生产随机性。 |
| [REQ-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-003) Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1 | [DES-WR-ES-003](#des-wr-es-003) | 旧请求不迁移、旧已出发义务有限结算，跨授权/接替/竞态保持累计需求。 | [ES-VERIFY-003](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-003)：边界 tick、竞态、部分履行、checkpoint 恢复。 | 未来 PR/CI 的旧新 lineage 与累计履行对账。 | 不证明实际物流退回、跨节点恢复。 |
| [AC-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-003) Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1 | [DES-WR-ES-003](#des-wr-es-003) | 旧请求不迁移、旧已出发义务有限结算，跨授权/接替/竞态保持累计需求。 | [ES-VERIFY-003](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-003)：边界 tick、竞态、部分履行、checkpoint 恢复。 | 未来 PR/CI 的旧新 lineage 与累计履行对账。 | 不证明实际物流退回、跨节点恢复。 |
| [REQ-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004) Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3 | [DES-WR-ES-004](#des-wr-es-004) | 单份 escrow 支撑采购/补偿/配给，物理接收与需求履行无旁路，读面诚实。 | [ES-VERIFY-004](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-004)：同候选 API/Agent/正式 surface 与资源合同组合验收。 | 未来 full-tier 输出、真实浏览器与网络证据。 | 本次未提供实现、玩家或发布证据。 |
| [AC-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-004) Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3 | [DES-WR-ES-004](#des-wr-es-004) | 单份 escrow 支撑采购/补偿/配给，物理接收与需求履行无旁路，读面诚实。 | [ES-VERIFY-004](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-004)：同候选 API/Agent/正式 surface 与资源合同组合验收。 | 未来 full-tier 输出、真实浏览器与网络证据。 | 本次未提供实现、玩家或发布证据。 |

## 12. 决策、长期风险与未决问题

| 决策 | 选择与理由 | 残余风险、owner 与复核触发 |
| --- | --- | --- |
| 单一首版政策 authority | 消费 GAME-018 §6.4 的全部值；无需实现者再挑危机阈值或算法。首版无 lottery，避免新增 seed 权威。 | 阈值/预算/余数公平仍需平衡验证；gameplay_designer 在手册样本显示救助过慢或持续偏差时提出下一版本，不修改旧批次。 |
| 普通供给与有效覆盖分开 | 进入/恢复看常态供给，防重复分配看全部履行/承诺，避免救助刚足额即提前退出。 | provenance 和跨域占用必须真正接通；runtime_engineer 用 ES-VERIFY-001/003 验证缺来源失败。 |
| 区域窗口整批 CAS | 保持跨地点公平，失败整批拒绝；不按执行顺序抢共享资源。 | 大批次争用会增加重评估，初始规模限 §9；runtime_engineer / QA 测量后才能提出不改变公平的扩容。 |
| 跨授权需求键与数量区间 | 以危机前服务义务和非重叠消耗区间为根，换 request/控制人不重置累计。 | 输入 authority、组织交接与普通需求履行需要新增业务 adapter；缺任一不能启用，不把假定值记成已实现。 |
| 有限旧在途合同继续 | 退出停止新资格，已合法出发义务按原 scope/金额/截止结清；不删除货物，也不授予新权利。 | 真实损失/退回/保管 adapter 与独立复核仍需实现；未知去向保持占用和阻塞。 |
| 共用 publication 与归档 | 接入既有 prepared delta / state root / checkpoint，保留最小累计与防重放事实。 | 不新增第二套事务底座；恢复/压缩需实测，不由日志可读推导 exactly-once。 |

本次完成的是政策和目标技术合同，实施与运行验收仍由 [#4866](https://github.com/nil-studio/oasis7/issues/4866) 的实际候选承接。签名、权限、持久化兼容和结算接口落地时仍按[开发流程规范](../../engineering/workflow/source-of-truth.md)进行对应能力的独立评审；没有预先批准、已执行验证或发布承诺。


## 13. 首版 authority 与同地点供配电适配器

本节为 #4866 的本地实现候选补齐专业承载合同；不改变 GAME-018 §6.4 政策，不代表已完成正式网络或发布验收。首版付费报价、公共预算和跨地点电力网络仍须各自真实适配器；缺失时返回 `unsupported_resource_path`，不得借普通材料运输、OC 转账或客户端描述代签。

### 13.1 独立最低服务登记

最低服务承诺通过独立 `MinimumServiceCommandV1::Register` 形成，不由保供申请建立。登记同时消费原身份/claim 所有权、当前有效的 `AgentLocationAuthorityV1` 与 `LocationAnchorV1`，以及现有 finality signer registry 的阈值证书；提交者为该 Agent 或其冻结控制来源的 primary Agent，并通过正式签名意图入口证明同意。控制主体取 `resolved_agent_owner` 解析的 canonical `player_id`，不把经济 claim 本身当成控制授权；同一玩家的多个根和 Agent 聚合为一个主体。formal contract 冻结 `owner_source_id` 与 `operator_agent_id`；starter primary 自身可登记，制造 Agent 只沿已冻结的制造控制来源登记，后续根重绑或所有者变化不自动继承旧服务权限。登记绑定最低量、每个 60-tick 区间、recipe allowlist、公共关键用途、地点、区域和区域预算 lineage，不允许对已登记 ID 改名或回填登记时间。

证书 manifest 对 `oasis7.minimum-service.register.v1`、live world binding、当前权威 tick、owner、完整登记和区域预算 lineage 的 canonical JSON payload 做 SHA-256 摘要。复用既有 finality 验签及权重，不新增固定的临时签名阈值；未知 signer、旧 registry、错世界、错高度或字段变化原子拒绝。live world binding 来自世界执行上下文，不能取客户端参数。保供 `RegisterService` 只消费该已形成、且至少成熟 600 ticks 的不可变承诺；普通 passed proposal 与 `source_ref` 字符串均不能替代该来源。

### 13.2 权威观测与需求来源

每个世界对齐的 60-tick 边界由 system producer 提交 `EmergencySupply::Observe { region_id }`，其 `action_id` 必须为零；普通 Agent 调用不得借此写入观测。pure reducer 重算完整成熟集合、当前 claim 聚合、有效且连续地点、真实普通 Electricity 库存和 WorldState digest。以未来十个完整 60-tick 区间为视野，对同一 Agent 的普通库存只覆盖一次，按消耗区间先后及义务身份字节序映射到具体最低义务；已预扣的普通制造、供给 escrow 和独立 restricted power 不再出现在该可用库存中。未接收、缺可达证明或未知来源的在途量不充作普通覆盖。

观测保存每义务 600-tick 汇总和每需求区间的精确普通覆盖。`PublishEvidence` 只接受与内部已提交观测完全一致的三个连续窗口、原区域 lineage 及观测集合摘要；客户端提交的 root、比例、需求或 coverage 不产生事实。申请引用具体义务和世界对齐区间，最低量与普通覆盖均从这些权威来源取得，跨授权累计沿唯一需求键继续，不因换 request 重置。

### 13.3 同地点有限供配电

首版实现的免费内部供配电以双方当前有效的 canonical 同地点 authority 和 active anchor 为连接边界；文本相同但缺原权威、位置漂移或跨地点均不支持。供应方在授权时从其真实普通 Electricity 库存预扣自愿供给，进入该授权 escrow，不能同时供普通制造或另一授权使用。冻结保存该资源池及全部需求 read set，分配提交对其联合复核；没有可同时兑现的同地点计划则整批阻断。

`Deliver` 只把原 escrow 中已经分配、未结算且未超过原消耗区间开始的电力转入受益方的 `EmergencyRestrictedPowerV1`。该专用用途余额不加入普通 Electricity、不成为可出售或可转让库存；受益方只可通过已登记 recipe、原义务和当前 60-tick 区间的正式 `ScheduleRecipe` adapter 原子消费。消费与排程共享 prepared successor，不能先加普通电力再补扣，也不能重复使用已消费额度。原 escrow = 未交付余额 + restricted 接收量，收到后普通账户不增；退出只退回未承担交付的原 escrow。

### 13.4 容量和独立复核

每授权窗口以 4096 需求记录、1024 聚合控制主体、64 地点为硬上限。经过来源与权限验证的新增登记会越界时，提交整个区域窗口的持久 `overflowed` 处置，阻止此前已登记集合继续冻结/分配；不采用先到先得前 N 条。超界处置无新增经济效果，重启仍保留阻塞。

授权明确独立 reviewer；该 reviewer 的控制主体与供应方、受益方和分配执行方分离。受影响主体在分配结果提交后 120 ticks 内登记复核，登记不恢复资格或退款；超过受理后 120 ticks 尚未裁决的争议部分为 `review_blocked`，无争议部分继续。裁决消费 exact-bound 治理批准，只可确认结果或取消尚未交付分配并解除对应占用；不改写既有接收、使用或 receipt，不以该免费路径创建无来源的补偿资金。

受限电力启动的真实 recipe job 同时保存原世界绑定。新消费和原始启动事件均比较当前世界绑定；已经在制的 job 迁入其他世界时不自动完成，也不能通过原始 `RecipeCompleted` 兑付物料，原 job 与占用保留，等待在原世界继续或由既有恢复 authority 处理。普通历史 job 的空来源保持原序列化与完成语义。
